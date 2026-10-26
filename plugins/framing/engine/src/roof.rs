//! Roof framing (Medeek Truss / Rafter style) over a plan rectangle: gable, hip, shed and gambrel
//! shapes; fink / howe / king-post / queen-post / scissor / attic / mono trusses or rafters; hip
//! girders, jack trusses, hip and jack rafters; dormers on gable slopes; fascia, soffits,
//! sheathing, cladding and gutters.
//!
//! Local frame: `s` across the span, `r` along the run (the ridge), `y` above the bearing (the top of
//! storey `level`'s walls). `sg` (σ) is the distance from an eave inward (sg = s on the left slope,
//! S − s on the right one).

use std::collections::BTreeSet;

use plannr_core::geom::{add, area as poly_area, cross, dot, len as vlen, newell, normalize, scale, sub, P3};
use plannr_core::ops::OpError;

use crate::data::*;
use crate::expand::{emit, member, Expansion, Faces, Linear, Member, Panel};

const UP: P3 = [0.0, 1.0, 0.0];
const SHAPES: [&str; 4] = ["gable", "hip", "shed", "gambrel"];
pub const TRUSS_TYPES: [&str; 8] = ["fink", "howe", "kingPost", "queenPost", "scissor", "attic", "mono", "rafters"];

#[derive(Clone, Copy)]
struct Fr { origin: P3, es: P3, er: P3 }
impl Fr {
    fn at(&self, s: f64, y: f64, r: f64) -> P3 { add(add(add(self.origin, scale(self.es, s)), scale(UP, y)), scale(self.er, r)) }
    /// A member between two points of the truss plane at `r`: `[s, y]`; thickness along the run centred at `r`.
    /// `under`: the line is the member's underside (depth grows upward), else it is centred on the line.
    #[allow(clippy::too_many_arguments)]
    fn bar(&self, m: &mut Vec<Member>, key: String, cat: &'static str, size: [f64; 2], mat: &'static str, a: [f64; 2], b: [f64; 2], r: f64, under: bool) {
        let [t, d] = size;
        let dir = [b[0] - a[0], b[1] - a[1]]; let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt(); if len < 1e-6 { return; }
        let u = [dir[0] / len, dir[1] / len]; let perp = [-u[1], u[0]];
        let perp = if perp[1] < 0.0 { [-perp[0], -perp[1]] } else { perp };
        let ex = add(scale(self.es, u[0]), scale(UP, u[1]));
        let ey = add(scale(self.es, perp[0]), scale(UP, perp[1]));
        let origin = add(add(self.origin, scale(self.es, a[0])), scale(UP, a[1]));
        let (d0, d1) = if under { (0.0, d) } else { (-d / 2.0, d / 2.0) };
        member(m, key, cat, size, mat, origin, ex, ey, self.er, [[0.0, len], [d0, d1], [r - t / 2.0, r + t / 2.0]], 0);
    }
}

/// A member from `a` to `b` (world). Its depth lies in the plane of the line and `toward`, growing
/// toward it from the line when `under`, else centred; its thickness is centred on the line.
#[allow(clippy::too_many_arguments)]
fn bar3(m: &mut Vec<Member>, key: String, cat: &'static str, size: [f64; 2], mat: &'static str, a: P3, b: P3, toward: P3, under: bool) {
    let [t, d] = size;
    let v = sub(b, a); let l = vlen(v); if l < 1e-6 { return; }
    let ex = scale(v, 1.0 / l);
    let ey = sub(toward, scale(ex, dot(toward, ex))); if vlen(ey) < 1e-6 { return; }
    let ey = normalize(ey);
    let ez = cross(ex, ey);
    let (d0, d1) = if under { (0.0, d) } else { (-d / 2.0, d / 2.0) };
    member(m, key, cat, size, mat, a, ex, ey, ez, [[0.0, l], [d0, d1], [-t / 2.0, t / 2.0]], 0);
}

fn grid(len: f64, sp: f64) -> Vec<f64> {
    let mut v = vec![]; let mut p = 0.0; let sp = sp.max(0.2);
    while p <= len + 1e-6 { v.push(p); p += sp; }
    if v.last().map(|q| len - q > 1e-6).unwrap_or(true) { v.push(len); }
    v
}
fn one_up(size: &str) -> &'static str {
    match size.trim() { "2x4" => "2x6", "2x6" => "2x8", "2x8" => "2x10", _ => "2x12" }
}

/// A roof surface (top of the framing) for sheathing and cladding.
struct Plane { key: String, pts: Vec<P3>, holes: Vec<Vec<P3>> }
/// An eave: the outer top edge of the tails, its outward direction, the soffit under it.
struct Eave { a: P3, b: P3, out: P3, soffit: Option<(Vec<P2>, f64)>, gutter: bool }
/// A rake (sloped verge) edge on top of the framing.
struct Rake { a: P3, b: P3, out: P3 }
#[derive(Default)]
struct Shape { planes: Vec<Plane>, eaves: Vec<Eave>, rakes: Vec<Rake> }

/// Everything the builders share.
struct Ctx<'a> {
    rf: &'a Roof, fr: Fr,
    s: f64, r: f64, half: f64, k: f64, heel: f64, oh: f64,
    /// Width of the wall plate the roof bears on (rafter seat / birdsmouth), and a raised truss heel
    /// (height of the heel post from the bottom-chord top to the top-chord underside; 0 = standard heel).
    seat: f64, raised: f64,
    ct: f64, cd: f64, web: [f64; 2], rt: f64, rd: f64, bt: f64, bd: f64,
}
impl Ctx<'_> {
    fn trusses(&self) -> bool { self.rf.type_ != "rafters" }
    /// Depth of the members carrying the roof surface.
    fn depth(&self) -> f64 { if self.trusses() { self.cd } else { self.rd } }
    fn dv(&self, k: f64) -> f64 { self.depth() * (1.0 + k * k).sqrt() }
    fn plan(&self, s: f64, r: f64) -> P2 { let p = self.fr.at(s, 0.0, r); [p[0], p[2]] }
}

pub fn validate(rf: &Roof) -> Result<(), OpError> {
    let e = |m: String| Err(OpError(m));
    if !TRUSS_TYPES.contains(&rf.type_.as_str()) { return e(format!("type must be one of {}", TRUSS_TYPES.join(", "))); }
    if !SHAPES.contains(&rf.shape.as_str()) { return e(format!("shape must be one of {}", SHAPES.join(", "))); }
    if rf.type_ == "mono" && rf.shape != "shed" { return e("mono trusses need shape \"shed\"".into()); }
    if !["max", "min"].contains(&rf.high_side.as_str()) { return e("highSide must be max or min".into()); }
    if !["none", "shingles", "metal", "tiles"].contains(&rf.cladding.kind.as_str()) { return e("cladding.kind must be none, shingles, metal or tiles".into()); }
    if rf.slope() <= 0.0 { return e("the pitch must be above 0".into()); }
    if rf.shape == "gambrel" && rf.lower_pitch <= 0.0 { return e("lowerPitch must be above 0".into()); }
    if lumber(&rf.fascia.size).is_none() { return e(format!("unknown fascia size `{}`", rf.fascia.size)); }
    if !rf.dormers.is_empty() && rf.shape != "gable" { return e("dormers are only supported on gable roofs".into()); }
    let run = if rf.ridge_along_x { (rf.max[0] - rf.min[0]).abs() } else { (rf.max[1] - rf.min[1]).abs() };
    for d in &rf.dormers {
        if !["l", "r", "front", "back"].contains(&d.side.as_str()) { return e("dormer side must be l, r, front or back".into()); }
        if !["gable", "shed"].contains(&d.kind.as_str()) { return e("dormer kind must be gable or shed".into()); }
        if d.width < 0.5 { return e("a dormer needs a width of at least 0.5 m".into()); }
        if d.offset < 0.0 || d.offset + d.width > run + 1e-6 { return e(format!("dormer does not fit along the ridge (run {run:.2} m)")); }
        if d.wall_height <= 0.2 { return e("dormer wallHeight must be above 0.2 m".into()); }
    }
    Ok(())
}

/// Number of trusses: distinct `truss.<i>` / `girder.<i>` / `jackTruss.<i>` among the members.
pub fn truss_count(ex: &Expansion) -> usize {
    let mut set = BTreeSet::new();
    for m in ex.members.iter().filter(|m| m.category == "trusses") {
        let mut p = m.key.splitn(3, '.');
        if let (Some(a), Some(b)) = (p.next(), p.next()) { if ["truss", "girder", "jackTruss"].contains(&a) { set.insert(format!("{a}.{b}")); } }
    }
    set.len()
}

pub fn expand_roof(id: &str, rf: &Roof, house: &House, layer: &str) -> Expansion {
    let mut ex = Expansion::default();
    let (minx, minz, maxx, maxz) = (rf.min[0].min(rf.max[0]), rf.min[1].min(rf.max[1]), rf.min[0].max(rf.max[0]), rf.min[1].max(rf.max[1]));
    let (mut es, mut er, mut span, mut run) = if rf.ridge_along_x { ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], maxz - minz, maxx - minx) } else { ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], maxx - minx, maxz - minz) };
    // a hip roof always runs its ridge along the longer side
    if rf.shape == "hip" && span > run { std::mem::swap(&mut es, &mut er); std::mem::swap(&mut span, &mut run); }
    if span < 0.5 || run < 0.1 { return ex; }
    // the roof bears on the top plates of the walls under its eaves (else on top of the storey)
    let (wall_top, seat) = bearing(rf, house, minx, minz, maxx, maxz);
    let yb = house.base(rf.level) + wall_top;
    let [ct, cd] = lumber_or(&rf.chord_size, "2x4");
    let [rt, rd] = lumber_or(&rf.rafter_size, "2x8");
    let [bt, bd] = lumber_or(&rf.ridge_board, "2x10");
    let fd = lumber_or(&rf.floor_joist_size, "2x10")[1];
    // `heel` = height of the underside line of the sloped members above the wall top, at the wall's
    // outer face. Trusses: a standard heel puts the top chord on the bottom chord (bottom-chord depth;
    // attic trusses: the floor-joist chord); a larger `heel` is a raised (energy) heel with heel posts.
    // Rafters: a birdsmouth seat on the plate — the underside line meets the plate top at its inner
    // edge, so at the outer face it is `slope × seat` below the plate top (the heel cut). A steep
    // rafter gets a shorter seat so the notch never takes more than a third of its depth.
    let k = rf.slope();
    let chord_heel = if rf.type_ == "attic" { fd } else { cd };
    let (heel, raised) = if rf.type_ == "rafters" { (-birdsmouth(seat, rd, k), 0.0) } else if rf.heel > chord_heel + 0.02 { (rf.heel, rf.heel - chord_heel) } else { (chord_heel, 0.0) };
    let c = Ctx { rf, fr: Fr { origin: [minx, yb, minz], es, er }, s: span, r: run, half: span / 2.0, k, heel, oh: rf.overhang.max(0.0), seat, raised,
                  ct, cd, web: lumber_or(&rf.web_size, "2x4"), rt, rd, bt, bd };
    let m = &mut ex.members;
    let shape = match rf.shape.as_str() {
        "hip" => hip(m, &c),
        "shed" => shed(m, &c),
        "gambrel" => gambrel(m, &c),
        _ => gable(m, &c),
    };
    // fascia boards are lumber: add them before the faces are emitted
    let [ft, fdp] = lumber_or(&rf.fascia.size, "2x6");
    if rf.fascia.enabled {
        for (i, e) in shape.eaves.iter().enumerate() { board(m, format!("fascia.eave.{i}"), [ft, fdp], e.a, e.b, e.out); }
        for (i, e) in shape.rakes.iter().enumerate() { board(m, format!("fascia.rake.{i}"), [ft, fdp], e.a, e.b, e.out); }
    }
    let mut f = Faces::new(layer);
    emit(&ex, id, &mut f);
    finish(&mut ex, &mut f, id, rf, &shape, if rf.fascia.enabled { ft } else { 0.0 });
    ex.faces = f.out;
    ex
}

/// Where a ceiling joist goes next to the rafter pair at `rr`: beside it (nailed to its side), inside the run.
fn beside(c: &Ctx, rr: f64) -> f64 {
    let off = (c.rt + c.ct) / 2.0;
    if rr + off <= c.r - c.ct / 2.0 { rr + off } else { rr - off }
}
/// Top of a ridge board: flush with the top edge of the rafters meeting it (slope `k`).
fn ridge_top(c: &Ctx, under_at_centre: f64, k: f64) -> f64 { under_at_centre + c.rd * (1.0 + k * k).sqrt() }

/// Height of the top plates the roof bears on (above the storey base) and their width: the framed
/// walls of the storey that run along the footprint's edges; without any, the storey height.
fn bearing(rf: &Roof, house: &House, minx: f64, minz: f64, maxx: f64, maxz: f64) -> (f64, f64) {
    let near_edge = |p: P2| {
        let inside = p[0] > minx - 0.35 && p[0] < maxx + 0.35 && p[1] > minz - 0.35 && p[1] < maxz + 0.35;
        inside && ((p[0] - minx).abs() < 0.35 || (p[0] - maxx).abs() < 0.35 || (p[1] - minz).abs() < 0.35 || (p[1] - maxz).abs() < 0.35)
    };
    let mut top: Option<f64> = None; let mut seat: Option<f64> = None;
    for (_, w) in house.walls.iter().filter(|(_, w)| w.level == rf.level) {
        let mid = [(w.a[0] + w.b[0]) / 2.0, (w.a[1] + w.b[1]) / 2.0];
        if !(near_edge(w.a) && near_edge(w.b) && near_edge(mid)) { continue; }
        let h = w.height.unwrap_or_else(|| { let s = house.height(w.level) - house.floor_thickness; if s > 0.5 { s } else { 2.44 } });
        top = Some(top.map_or(h, |t: f64| t.max(h)));
        let d = w.plate()[1];
        seat = Some(seat.map_or(d, |x: f64| x.max(d)));
    }
    (top.unwrap_or_else(|| house.height(rf.level)), seat.unwrap_or(0.089))
}

/// A fascia board hanging from an edge: thickness outward, depth downward.
fn board(m: &mut Vec<Member>, key: String, size: [f64; 2], a: P3, b: P3, out: P3) {
    let v = sub(b, a); let l = vlen(v); if l < 1e-6 { return; }
    let ex = scale(v, 1.0 / l);
    let ey = normalize(sub(UP, scale(ex, dot(UP, ex))));
    member(m, key, "fascia", size, "lumber", a, ex, ey, out, [[0.0, l], [-size[1], 0.0], [0.0, size[0]]], 0);
}

fn up_normal(pts: &[P3]) -> P3 { let n = normalize(newell(pts)); if n[1] < 0.0 { scale(n, -1.0) } else { n } }

/// Sheathing, cladding, soffits and gutters.
fn finish(ex: &mut Expansion, f: &mut Faces, id: &str, rf: &Roof, sh: &Shape, fascia_t: f64) {
    let mut lift = 0.0;
    let layer = |pts: &[P3], n: P3, d: f64| -> Vec<P3> { pts.iter().map(|p| add(*p, scale(n, d))).collect() };
    let net_area = |p: &Plane| poly_area(&p.pts) - p.holes.iter().map(|h| poly_area(h)).sum::<f64>();
    if rf.sheathing.enabled {
        let ts = rf.sheathing.thickness;
        for p in &sh.planes {
            let n = up_normal(&p.pts);
            let key = format!("sheathing.{}", p.key);
            f.poly_holes(&format!("{id}#{key}"), layer(&p.pts, n, ts), p.holes.iter().map(|h| layer(h, n, ts)).collect(), n, &material_id("sheathing"));
            ex.panels.push(Panel { key, category: "sheathing", area: net_area(p), thickness: ts });
        }
        lift = ts;
    }
    if rf.cladding.kind != "none" {
        let tc = rf.cladding.depth();
        for p in &sh.planes {
            let n = up_normal(&p.pts);
            let key = format!("cladding.{}", p.key);
            f.poly_holes(&format!("{id}#{key}"), layer(&p.pts, n, lift + tc), p.holes.iter().map(|h| layer(h, n, lift + tc)).collect(), n, &material_id(&rf.cladding.kind));
            ex.panels.push(Panel { key, category: "cladding", area: net_area(p), thickness: tc });
        }
    }
    for (i, e) in sh.eaves.iter().enumerate() {
        if rf.soffit.enabled {
            if let Some((plan, y)) = &e.soffit {
                let key = format!("soffit.{i}");
                f.slab(&format!("{id}#{key}"), plan, y - rf.soffit.thickness, *y, &material_id("soffit"));
                ex.panels.push(Panel { key, category: "soffit", area: crate::expand::plan_area(plan).abs(), thickness: rf.soffit.thickness });
            }
        }
        if rf.gutters && e.gutter {
            let v = sub(e.b, e.a); let l = vlen(v); if l < 1e-6 { continue; }
            let key = format!("gutter.{i}");
            f.boxf(&format!("{id}#{key}"), e.a, scale(v, 1.0 / l), UP, e.out, [[0.0, l], [-0.13, -0.03], [fascia_t, fascia_t + 0.12]], &material_id("steel"));
            ex.linear.push(Linear { key, category: "gutters", length: l });
        }
    }
}

// ---- gable (and dormers) --------------------------------------------------------------------------

/// A dormer resolved against the main roof (heights above the bearing, sg along its slope).
struct Dm { i: usize, side: usize, sb: f64, o: f64, w: f64, y_fb: f64, y_ft: f64, gable: bool, dk: f64, ridge_y: f64, s_wall_end: f64, s_end: f64 }

fn resolve_dormers(c: &Ctx) -> Vec<Dm> {
    let (k, heel, dv, half) = (c.k, c.heel, c.dv(c.k), c.half);
    let top_peak = heel + k * half + dv;
    let sig = |y: f64, lo: f64| ((y - heel - dv) / k).clamp(lo, half);
    c.rf.dormers.iter().enumerate().map(|(i, d)| {
        let side = if d.side == "r" || d.side == "back" { 1 } else { 0 };
        let sb = d.setback.clamp(0.0, half * 0.8);
        let o = d.offset.max(0.0); let w = d.width.min(c.r - o).max(0.1);
        let y_fb = heel + k * sb + dv;
        let y_ft = (y_fb + d.wall_height).min(top_peak - 0.15).max(y_fb + 0.2);
        let gable = d.kind != "shed";
        if gable {
            let dk = d.pitch.max(0.5) / 12.0;
            // keep the dormer ridge under the main ridge: lower its walls when needed
            let y_ft = y_ft.min(top_peak - 0.05 - dk * w / 2.0).max(y_fb + 0.2);
            let ridge_y = y_ft + dk * w / 2.0;
            Dm { i, side, sb, o, w, y_fb, y_ft, gable, dk, ridge_y, s_wall_end: sig(y_ft, sb), s_end: sig(ridge_y, sb) }
        } else {
            let dk = if d.pitch / 12.0 < k && d.pitch > 0.0 { d.pitch / 12.0 } else { k / 3.0 };
            let s_end = ((y_ft - dk * sb - heel - dv) / (k - dk)).clamp(sb, half);
            Dm { i, side, sb, o, w, y_fb, y_ft, gable, dk, ridge_y: y_ft, s_wall_end: s_end, s_end }
        }
    }).collect()
}
fn cut_for(dms: &[Dm], side: usize, rr: f64, t: f64) -> Option<(f64, f64)> {
    dms.iter().find(|d| d.side == side && rr + t / 2.0 > d.o && rr - t / 2.0 < d.o + d.w).map(|d| (d.sb, d.s_end))
}

/// One common truss of a gable profile at `rr` (also hip commons and girders).
#[allow(clippy::too_many_arguments)]
fn gable_truss(m: &mut Vec<Member>, c: &Ctx, prefix: &str, rr: f64, end: bool, cut: [Option<(f64, f64)>; 2]) {
    let (rf, fr, s, half, k, heel, oh) = (c.rf, &c.fr, c.s, c.half, c.k, c.heel, c.oh);
    let chord = [c.ct, c.cd];
    let sx = |side: usize, sg: f64| if side == 0 { sg } else { s - sg };
    // top chords (split around a dormer opening)
    for (side, lr) in ["l", "r"].iter().enumerate() {
        let pt = |sg: f64| [sx(side, sg), heel + k * sg];
        match cut[side] {
            None => fr.bar(m, format!("{prefix}.top.{lr}"), "trusses", chord, "lumber", pt(-oh), pt(half), rr, true),
            Some((lo, hi)) => {
                fr.bar(m, format!("{prefix}.top.{lr}"), "trusses", chord, "lumber", pt(-oh), pt(lo), rr, true);
                if half - hi > 0.02 { fr.bar(m, format!("{prefix}.top.{lr}.upper"), "trusses", chord, "lumber", pt(hi), pt(half), rr, true); }
            }
        }
    }
    let top_at = |x: f64| heel + k * (if x <= half { x } else { s - x });
    // bottom chord(s) and the height of its top at x
    let bk = rf.bottom_pitch.unwrap_or(rf.pitch / 2.0).max(0.0) / 12.0;
    let bk = if rf.pitch_deg.is_some() && rf.bottom_pitch.is_none() { k / 2.0 } else { bk.min(k * 0.8) };
    let fdj = lumber_or(&rf.floor_joist_size, "2x10");
    let bot: Box<dyn Fn(f64) -> f64> = match rf.type_.as_str() {
        "scissor" => { let d = c.cd * (1.0 + bk * bk).sqrt(); Box::new(move |x: f64| bk * x.min(s - x) + d) }
        "attic" => { let d = fdj[1]; Box::new(move |_x: f64| d) }
        _ => { let d = c.cd; Box::new(move |_x: f64| d) }
    };
    match rf.type_.as_str() {
        "scissor" => {
            fr.bar(m, format!("{prefix}.bottom.l"), "trusses", chord, "lumber", [0.0, 0.0], [half, bk * half], rr, true);
            fr.bar(m, format!("{prefix}.bottom.r"), "trusses", chord, "lumber", [s, 0.0], [half, bk * half], rr, true);
        }
        "attic" => fr.bar(m, format!("{prefix}.bottom"), "trusses", fdj, "lumber", [0.0, 0.0], [s, 0.0], rr, true),
        _ => fr.bar(m, format!("{prefix}.bottom"), "trusses", chord, "lumber", [0.0, 0.0], [s, 0.0], rr, true),
    }
    // webs: left out on a side whose top chord is cut by a dormer
    let mut kk = 0;
    let web = c.web;
    let mut webm = |a: [f64; 2], b: [f64; 2], m: &mut Vec<Member>| {
        let left = a[0] < half - 1e-6 && b[0] < half - 1e-6; let right = a[0] > half + 1e-6 && b[0] > half + 1e-6;
        if (left && cut[0].is_some()) || (right && cut[1].is_some()) { return; }
        if (b[1] - a[1]).abs() < 0.02 && (b[0] - a[0]).abs() < 0.02 { return; }
        fr.bar(m, format!("{prefix}.web.{kk}"), "trusses", web, "lumber", a, b, rr, false); kk += 1;
    };
    match rf.type_.as_str() {
        "kingPost" => webm([half, bot(half)], [half, top_at(half)], m),
        "queenPost" => { webm([s / 3.0, bot(s / 3.0)], [s / 3.0, top_at(s / 3.0)], m); webm([2.0 * s / 3.0, bot(2.0 * s / 3.0)], [2.0 * s / 3.0, top_at(2.0 * s / 3.0)], m); }
        "howe" => {
            webm([half, bot(half)], [half, top_at(half)], m);
            webm([s / 3.0, bot(s / 3.0)], [s / 3.0, top_at(s / 3.0)], m); webm([2.0 * s / 3.0, bot(2.0 * s / 3.0)], [2.0 * s / 3.0, top_at(2.0 * s / 3.0)], m);
            webm([s / 3.0, bot(s / 3.0)], [half, top_at(half)], m); webm([2.0 * s / 3.0, bot(2.0 * s / 3.0)], [half, top_at(half)], m);
        }
        "scissor" => {
            if top_at(half) - bot(half) > 0.05 { webm([half, bot(half)], [half, top_at(half)], m); }
            for x in [s / 4.0, 3.0 * s / 4.0] { if top_at(x) - bot(x) > 0.05 { webm([x, bot(x)], [x, top_at(x)], m); } }
        }
        "attic" => {
            let fdv = fdj[1];
            let aw = rf.attic_width.unwrap_or(s / 2.0).clamp(0.3, s * 0.8);
            let (k0, k1) = (half - aw / 2.0, half + aw / 2.0);
            // knee walls of the room
            for (lr, x) in [("l", k0), ("r", k1)] {
                if top_at(x) - fdv > 0.1 { fr.bar(m, format!("{prefix}.knee.{lr}"), "trusses", web, "lumber", [x, fdv], [x, top_at(x)], rr, false); }
            }
            // ceiling collar between the top chords
            let yc = (fdv + rf.attic_height).min(top_at(half) - 0.25);
            if yc > fdv + 0.5 {
                let x0 = ((yc + c.cd - heel) / k).clamp(0.0, half); // top of the collar under the chord
                fr.bar(m, format!("{prefix}.collar"), "trusses", chord, "lumber", [x0, yc], [s - x0, yc], rr, true);
            }
            // a strut from each knee foot out to the top chord
            webm([k0, fdv], [k0 / 2.0, top_at(k0 / 2.0)], m); webm([k1, fdv], [s - k0 / 2.0, top_at(s - k0 / 2.0)], m);
        }
        _ => { // fink (W)
            webm([s / 3.0, bot(s / 3.0)], [s / 4.0, top_at(s / 4.0)], m); webm([s / 3.0, bot(s / 3.0)], [half, top_at(half)], m);
            webm([2.0 * s / 3.0, bot(2.0 * s / 3.0)], [half, top_at(half)], m); webm([2.0 * s / 3.0, bot(2.0 * s / 3.0)], [3.0 * s / 4.0, top_at(3.0 * s / 4.0)], m);
        }
    }
    // a raised (energy) heel: a post at each heel from the bottom chord up to the top chord
    if c.raised > 0.0 {
        // in the truss plane, flush with the wall's outer face; it stops where the top chord's
        // underside is lowest over it (the outer face), so the two never overlap
        let w = c.web[1];
        for (lr, x, edge) in [("l", w / 2.0, 0.0), ("r", s - w / 2.0, s)] { fr.bar(m, format!("{prefix}.heel.{lr}"), "trusses", c.web, "lumber", [x, bot(x)], [x, top_at(edge)], rr, false); }
    }
    if end && rf.gable_ends {
        let mut g = 0; let mut x = 0.6;
        while x < s - 0.3 { if (x - half).abs() > 0.3 && top_at(x) - bot(x) > 0.05 { fr.bar(m, format!("{prefix}.gable.{g}"), "trusses", web, "lumber", [x, bot(x)], [x, top_at(x)], rr, false); g += 1; } x += 0.6; }
    }
}

fn gable(m: &mut Vec<Member>, c: &Ctx) -> Shape {
    let (rf, fr, s, r, half, k, heel, oh) = (c.rf, c.fr, c.s, c.r, c.half, c.k, c.heel, c.oh);
    let positions = grid(r, rf.spacing);
    let dms = resolve_dormers(c);
    let t = if c.trusses() { c.ct } else { c.rt };
    if !c.trusses() {
        // the ridge board's top is flush with the rafters' top edges; its depth covers their plumb cut
        let yt = ridge_top(c, heel + k * half, k);
        let depth = c.bd.max(c.rd * (1.0 + k * k).sqrt());
        member(m, "ridge", "ridge", [c.bt, depth], "lumber", fr.at(half, 0.0, 0.0), fr.er, UP, fr.es, [[0.0, r], [yt - depth, yt], [-c.bt / 2.0, c.bt / 2.0]], 0);
        for (i, &p) in positions.iter().enumerate() {
            let rr = p.clamp(c.rt / 2.0, r - c.rt / 2.0);
            for (side, lr) in ["l", "r"].iter().enumerate() {
                let pt = |sg: f64| [if side == 0 { sg } else { s - sg }, heel + k * sg];
                let top = half - c.bt / 2.0;
                match cut_for(&dms, side, rr, t) {
                    None => fr.bar(m, format!("rafter.{i}.{lr}"), "rafters", [c.rt, c.rd], "lumber", pt(-oh), pt(top), rr, true),
                    Some((lo, hi)) => {
                        fr.bar(m, format!("rafter.{i}.{lr}"), "rafters", [c.rt, c.rd], "lumber", pt(-oh), pt(lo), rr, true);
                        if top - hi > 0.02 { fr.bar(m, format!("rafter.{i}.{lr}.upper"), "rafters", [c.rt, c.rd], "lumber", pt(hi), pt(top), rr, true); }
                    }
                }
            }
            fr.bar(m, format!("ceilingJoist.{i}"), "joists", [c.ct, c.cd], "lumber", [0.0, 0.0], [s, 0.0], beside(c, rr), true);
        }
    } else {
        for (i, &p) in positions.iter().enumerate() {
            let rr = p.clamp(c.ct / 2.0, r - c.ct / 2.0);
            let cut = [cut_for(&dms, 0, rr, t), cut_for(&dms, 1, rr, t)];
            gable_truss(m, c, &format!("truss.{i}"), rr, i == 0 || i == positions.len() - 1, cut);
        }
    }
    // the surface
    let dv = c.dv(k);
    let (ye, yp) = (heel - k * oh + dv, heel + k * half + dv);
    let mut sh = Shape::default();
    for (side, lr) in ["l", "r"].iter().enumerate() {
        let sx = |sg: f64| if side == 0 { sg } else { s - sg };
        let holes: Vec<Vec<P3>> = dms.iter().filter(|d| d.side == side).map(|d| {
            let y = |sg: f64| heel + k * sg + dv;
            vec![fr.at(sx(d.sb), y(d.sb), d.o), fr.at(sx(d.sb), y(d.sb), d.o + d.w), fr.at(sx(d.s_end), y(d.s_end), d.o + d.w), fr.at(sx(d.s_end), y(d.s_end), d.o)]
        }).collect();
        sh.planes.push(Plane { key: lr.to_string(), pts: vec![fr.at(sx(-oh), ye, 0.0), fr.at(half, yp, 0.0), fr.at(half, yp, r), fr.at(sx(-oh), ye, r)], holes });
        let out = if side == 0 { scale(fr.es, -1.0) } else { fr.es };
        sh.eaves.push(Eave { a: fr.at(sx(-oh), ye, 0.0), b: fr.at(sx(-oh), ye, r), out, soffit: Some((vec![c.plan(sx(-oh), 0.0), c.plan(sx(-oh), r), c.plan(sx(0.0), r), c.plan(sx(0.0), 0.0)], fr.origin[1] + heel - k * oh)), gutter: true });
        for (rr, o) in [(0.0, -1.0), (r, 1.0)] { sh.rakes.push(Rake { a: fr.at(sx(-oh), ye, rr), b: fr.at(half, yp, rr), out: scale(fr.er, o) }); }
    }
    for d in &dms { dormer(m, c, d, &mut sh); }
    sh
}

/// Trimmers, headers, front wall, cheek walls and roof of one dormer.
fn dormer(m: &mut Vec<Member>, c: &Ctx, d: &Dm, sh: &mut Shape) {
    let (fr, s, half, k, heel, oh) = (c.fr, c.s, c.half, c.k, c.heel, c.oh);
    let dv = c.dv(k);
    let i = d.i;
    let sx = |sg: f64| if d.side == 0 { sg } else { s - sg };
    let p = |sg: f64, y: f64, r: f64| fr.at(sx(sg), y, r);
    let inward = if d.side == 0 { fr.es } else { scale(fr.es, -1.0) };
    let main_top = |sg: f64| heel + k * sg + dv;
    let (t, depth) = if c.trusses() { (c.ct, c.cd) } else { (c.rt, c.rd) };
    // doubled trimmers beside the opening, full length
    let top = if c.trusses() { half } else { half - c.bt / 2.0 };
    for (lr, rs) in [("l", [d.o - t / 2.0, d.o - 1.5 * t]), ("r", [d.o + d.w + t / 2.0, d.o + d.w + 1.5 * t])] {
        for (j, rr) in rs.iter().enumerate() {
            bar3(m, format!("dormer.{i}.trimmer.{lr}.{j}"), "trimmers", [t, depth], "lumber", p(-oh, heel - k * oh, *rr), p(top, heel + k * top, *rr), UP, true);
        }
    }
    // doubled headers across the opening at its lower and upper ends
    for (name, sg) in [("bottom", d.sb), ("top", d.s_end)] {
        if name == "bottom" && d.sb < 0.05 { continue; }
        for (j, z) in [[-t, 0.0], [0.0, t]].iter().enumerate() {
            member(m, format!("dormer.{i}.header.{name}.{j}"), "headers", [t, depth], "header", p(sg, 0.0, d.o), fr.er, UP, inward, [[0.0, d.w], [main_top(sg) - depth, main_top(sg)], *z], 0);
        }
    }
    // front wall (2x4) with a framed rough opening
    let [st, sdep] = [0.038, 0.089]; let pt = 0.038;
    let wo = p(d.sb, 0.0, d.o);
    let wall = |m: &mut Vec<Member>, key: String, cat: &'static str, mat: &'static str, x: [f64; 2], y: [f64; 2], along: usize| member(m, key, cat, [st, sdep], mat, wo, fr.er, UP, inward, [x, y, [0.0, sdep]], along);
    wall(m, format!("dormer.{i}.front.plate.bottom"), "dormer plates", "plate", [0.0, d.w], [d.y_fb, d.y_fb + pt], 0);
    wall(m, format!("dormer.{i}.front.plate.top.0"), "dormer plates", "plate", [0.0, d.w], [d.y_ft - pt, d.y_ft], 0);
    wall(m, format!("dormer.{i}.front.plate.top.1"), "dormer plates", "plate", [0.0, d.w], [d.y_ft - 2.0 * pt, d.y_ft - pt], 0);
    let ro_w = (d.w - 0.3).max(0.0); let ro_l = (d.w - ro_w) / 2.0; let ro_r = ro_l + ro_w;
    let hd = 0.140;
    let ro_top = d.y_ft - 2.0 * pt - hd;
    let wall_h = d.y_ft - d.y_fb;
    let ro_bot = (ro_top - (wall_h - 0.3)).max(d.y_fb + 2.0 * pt + 0.05);
    let has_ro = ro_w > 0.2 && ro_top - ro_bot > 0.15;
    let mut j = 0;
    let mut x = 0.0;
    let mut xs = vec![];
    while x <= d.w - st + 1e-6 { xs.push(x); x += 0.4; }
    if xs.last().map(|q| d.w - st - q > 1e-6).unwrap_or(true) { xs.push(d.w - st); }
    for &x in &xs {
        if has_ro && x + st > ro_l - st - 1e-6 && x < ro_r + st + 1e-6 { continue; }
        wall(m, format!("dormer.{i}.front.stud.{j}"), "dormer studs", "lumber", [x, x + st], [d.y_fb + pt, d.y_ft - 2.0 * pt], 1); j += 1;
    }
    if has_ro {
        wall(m, format!("dormer.{i}.front.king.l"), "dormer studs", "lumber", [ro_l - st, ro_l], [d.y_fb + pt, d.y_ft - 2.0 * pt], 1);
        wall(m, format!("dormer.{i}.front.king.r"), "dormer studs", "lumber", [ro_r, ro_r + st], [d.y_fb + pt, d.y_ft - 2.0 * pt], 1);
        member(m, format!("dormer.{i}.front.header"), "headers", [st, hd], "header", wo, fr.er, UP, inward, [[ro_l, ro_r], [ro_top, ro_top + hd], [0.0, sdep]], 0);
        wall(m, format!("dormer.{i}.front.sill"), "dormer plates", "plate", [ro_l, ro_r], [ro_bot - pt, ro_bot], 0);
        let mut ci = 0;
        if ro_bot - pt - (d.y_fb + pt) > 0.05 {
            for &x in &xs { if x + st > ro_l + 1e-6 && x < ro_r - 1e-6 { wall(m, format!("dormer.{i}.front.cripple.{ci}"), "dormer studs", "lumber", [x, x + st], [d.y_fb + pt, ro_bot - pt], 1); ci += 1; } }
        }
    }
    // gable triangle over the front wall
    if d.gable {
        let mut g = 0; let mut x = 0.2;
        while x < d.w - 0.2 - st {
            let h = d.dk * (x + st / 2.0).min(d.w - x - st / 2.0) - 0.02;
            if h > 0.08 { wall(m, format!("dormer.{i}.gable.{g}"), "dormer studs", "lumber", [x, x + st], [d.y_ft, d.y_ft + h], 1); g += 1; }
            x += 0.4;
        }
    }
    // cheek walls: studs between the main roof and the dormer's wall top / shed roof
    let wall_top = |sg: f64| if d.gable { d.y_ft } else { d.y_ft + d.dk * (sg - d.sb) };
    for (lr, r0) in [("l", d.o - sdep), ("r", d.o + d.w)] {
        let rc = r0 + sdep / 2.0;
        let s0 = d.sb + sdep;
        let mut sg = s0 + st / 2.0; let mut j = 0;
        while sg < d.s_wall_end - 0.02 {
            let (lo, hi) = (main_top(sg), wall_top(sg) - pt);
            if hi - lo > 0.05 { member(m, format!("dormer.{i}.cheek.{lr}.stud.{j}"), "dormer studs", [st, sdep], "lumber", p(sg, 0.0, r0), inward, UP, fr.er, [[-st / 2.0, st / 2.0], [lo, hi], [0.0, sdep]], 1); j += 1; }
            sg += 0.4;
        }
        if d.s_wall_end - s0 > 0.1 {
            bar3(m, format!("dormer.{i}.cheek.{lr}.plate.top"), "dormer plates", [sdep, pt], "plate", p(s0, wall_top(s0) - pt, rc), p(d.s_wall_end, wall_top(d.s_wall_end) - pt, rc), UP, true);
            bar3(m, format!("dormer.{i}.cheek.{lr}.plate.bottom"), "dormer plates", [sdep, pt], "plate", p(s0, main_top(s0), rc), p(d.s_wall_end, main_top(d.s_wall_end), rc), UP, true);
        }
    }
    // the dormer's own roof
    let (drt, drd) = (0.038, 0.140);
    let doh = 0.15;
    let dvd = drd * (1.0 + d.dk * d.dk).sqrt();
    let toward_eave = scale(inward, -1.0);
    if d.gable {
        let mid = d.o + d.w / 2.0;
        member(m, format!("dormer.{i}.ridge"), "ridge", [0.038, 0.184], "lumber", p(d.sb - doh, 0.0, mid), inward, UP, fr.er, [[0.0, d.s_end - d.sb + doh], [d.ridge_y - 0.184, d.ridge_y], [-0.019, 0.019]], 0);
        let mut sg = d.sb - doh + drt / 2.0; let mut j = 0;
        while sg <= d.s_end + 1e-6 {
            let mt = main_top(sg);
            let x0 = if mt > d.y_ft { (mt - d.y_ft) / d.dk } else { 0.0 };
            if x0 < d.w / 2.0 - 0.05 {
                let xs = if x0 <= 1e-9 { -doh } else { x0 };
                let y0 = d.y_ft + d.dk * xs; let ytop = d.ridge_y - d.dk * 0.019;
                bar3(m, format!("dormer.{i}.rafter.{j}.l"), "dormer rafters", [drt, drd], "lumber", p(sg, y0, d.o + xs), p(sg, ytop, mid - 0.019), UP, true);
                bar3(m, format!("dormer.{i}.rafter.{j}.r"), "dormer rafters", [drt, drd], "lumber", p(sg, y0, d.o + d.w - xs), p(sg, ytop, mid + 0.019), UP, true);
                j += 1;
            }
            sg += 0.4;
        }
        let (ye, yr) = (d.y_ft - d.dk * doh + dvd, d.ridge_y + dvd);
        sh.planes.push(Plane { key: format!("dormer.{i}.l"), pts: vec![p(d.sb - doh, ye, d.o - doh), p(d.sb - doh, yr, mid), p(d.s_end, yr, mid), p(d.s_wall_end, d.y_ft + dvd, d.o)], holes: vec![] });
        sh.planes.push(Plane { key: format!("dormer.{i}.r"), pts: vec![p(d.sb - doh, ye, d.o + d.w + doh), p(d.sb - doh, yr, mid), p(d.s_end, yr, mid), p(d.s_wall_end, d.y_ft + dvd, d.o + d.w)], holes: vec![] });
        sh.rakes.push(Rake { a: p(d.sb - doh, ye, d.o - doh), b: p(d.sb - doh, yr, mid), out: toward_eave });
        sh.rakes.push(Rake { a: p(d.sb - doh, ye, d.o + d.w + doh), b: p(d.sb - doh, yr, mid), out: toward_eave });
        sh.eaves.push(Eave { a: p(d.sb - doh, ye, d.o - doh), b: p(d.s_wall_end, ye, d.o - doh), out: scale(fr.er, -1.0), soffit: None, gutter: false });
        sh.eaves.push(Eave { a: p(d.sb - doh, ye, d.o + d.w + doh), b: p(d.s_wall_end, ye, d.o + d.w + doh), out: fr.er, soffit: None, gutter: false });
    } else {
        let y_at = |sg: f64| d.y_ft + d.dk * (sg - d.sb);
        let mut j = 0; let mut r = d.o - doh + drt / 2.0;
        while r <= d.o + d.w + doh - drt / 2.0 + 1e-6 {
            bar3(m, format!("dormer.{i}.rafter.{j}"), "dormer rafters", [drt, drd], "lumber", p(d.sb - doh, y_at(d.sb - doh), r), p(d.s_end, y_at(d.s_end), r), UP, true);
            j += 1; r += 0.4;
        }
        let (ya, yb) = (y_at(d.sb - doh) + dvd, y_at(d.s_end) + dvd);
        let (r0, r1) = (d.o - doh, d.o + d.w + doh);
        sh.planes.push(Plane { key: format!("dormer.{i}.roof"), pts: vec![p(d.sb - doh, ya, r0), p(d.sb - doh, ya, r1), p(d.s_end, yb, r1), p(d.s_end, yb, r0)], holes: vec![] });
        sh.eaves.push(Eave { a: p(d.sb - doh, ya, r0), b: p(d.sb - doh, ya, r1), out: toward_eave, soffit: None, gutter: false });
        sh.rakes.push(Rake { a: p(d.sb - doh, ya, r0), b: p(d.s_end, yb, r0), out: scale(fr.er, -1.0) });
        sh.rakes.push(Rake { a: p(d.sb - doh, ya, r1), b: p(d.s_end, yb, r1), out: fr.er });
    }
}

// ---- hip --------------------------------------------------------------------------------------------

fn hip(m: &mut Vec<Member>, c: &Ctx) -> Shape {
    let (rf, fr, s, r, half, k, heel, oh) = (c.rf, c.fr, c.s, c.r, c.half, c.k, c.heel, c.oh);
    let ridge_len = (r - s).max(0.0);
    let (r0, r1) = (half, r - half); // ridge from r0 to r1 (equal for a pyramid)
    let peak_y = heel + k * half;
    let rafter = [c.rt, c.rd];
    let hip_size = lumber_or(rf.hip_rafter_size.as_deref().unwrap_or(one_up(&rf.rafter_size)), "2x10");
    let positions = grid(r, rf.spacing);
    let mut jl = 0; let mut jr = 0;
    // side jacks (both shapes): rafters from the eave up to a hip line
    let side_jacks = |m: &mut Vec<Member>, jl: &mut usize, jr: &mut usize| {
        for &p in &positions {
            let reach = if p < r0 - 1e-6 { p } else if p > r1 + 1e-6 { r - p } else { continue };
            if reach < 0.05 { continue; }
            let rr = p.clamp(c.rt / 2.0, r - c.rt / 2.0);
            fr.bar(m, format!("jack.l.{jl}"), "jacks", rafter, "lumber", [-oh, heel - k * oh], [reach, heel + k * reach], rr, true); *jl += 1;
            fr.bar(m, format!("jack.r.{jr}"), "jacks", rafter, "lumber", [s + oh, heel - k * oh], [s - reach, heel + k * reach], rr, true); *jr += 1;
        }
    };
    // four hip rafters from the overhang corners to the ridge ends
    let hips = |m: &mut Vec<Member>| {
        for (n, (sc, rc, so, ro, re)) in [(0.0, 0.0, -oh, -oh, r0), (s, 0.0, oh, -oh, r0), (0.0, r, -oh, oh, r1), (s, r, oh, oh, r1)].iter().enumerate() {
            bar3(m, format!("hip.{n}"), "hips", hip_size, "lumber", fr.at(sc + so, heel - k * oh, rc + ro), fr.at(half, peak_y, *re), UP, true);
        }
    };
    if !c.trusses() {
        if ridge_len > 1e-6 { let yt = ridge_top(c, peak_y, k); let depth = c.bd.max(c.rd * (1.0 + k * k).sqrt()); member(m, "ridge", "ridge", [c.bt, depth], "lumber", fr.at(half, 0.0, r0), fr.er, UP, fr.es, [[0.0, ridge_len], [yt - depth, yt], [-c.bt / 2.0, c.bt / 2.0]], 0); }
        for (i, &p) in positions.iter().enumerate() {
            let rr = p.clamp(c.rt / 2.0, r - c.rt / 2.0);
            if p >= r0 - 1e-6 && p <= r1 + 1e-6 {
                let top = if ridge_len > 1e-6 { half - c.bt / 2.0 } else { half };
                fr.bar(m, format!("rafter.{i}.l"), "rafters", rafter, "lumber", [-oh, heel - k * oh], [top, heel + k * top], rr, true);
                fr.bar(m, format!("rafter.{i}.r"), "rafters", rafter, "lumber", [s + oh, heel - k * oh], [s - top, heel + k * top], rr, true);
            }
            fr.bar(m, format!("ceilingJoist.{i}"), "joists", [c.ct, c.cd], "lumber", [0.0, 0.0], [s, 0.0], beside(c, rr), true);
        }
        side_jacks(m, &mut jl, &mut jr);
        hips(m);
        // end slopes: a king common rafter at mid-span and jacks either side up to the hips
        let mut ends = grid(s, rf.spacing).into_iter().filter(|x| (x - half).abs() > c.rt).collect::<Vec<_>>();
        ends.push(half);
        for (j, &x) in ends.iter().enumerate() {
            let reach = if (x - half).abs() < 1e-9 { half } else { x.min(s - x) };
            if reach < 0.05 { continue; }
            let xx = x.clamp(c.rt / 2.0, s - c.rt / 2.0);
            let (cat, kf, kb) = if (x - half).abs() < 1e-9 { ("rafters", "kingCommon.front".to_string(), "kingCommon.back".to_string()) } else { ("jacks", format!("jack.f.{j}"), format!("jack.b.{j}")) };
            bar3(m, kf, cat, rafter, "lumber", fr.at(xx, heel - k * oh, -oh), fr.at(xx, heel + k * reach, reach), UP, true);
            bar3(m, kb, cat, rafter, "lumber", fr.at(xx, heel - k * oh, r + oh), fr.at(xx, heel + k * reach, r - reach), UP, true);
        }
    } else {
        // doubled hip girders at the ends of the ridge section, commons between, jack trusses beyond
        let plies: Vec<f64> = if ridge_len > 2.0 * c.ct { vec![r0 + c.ct / 2.0, r0 + 1.5 * c.ct, r1 - c.ct / 2.0, r1 - 1.5 * c.ct] } else { vec![half - c.ct / 2.0, half + c.ct / 2.0] };
        for (g, &rr) in plies.iter().enumerate() { gable_truss(m, c, &format!("girder.{g}"), rr, false, [None, None]); }
        let mut i = 0;
        for &p in &positions {
            if p > r0 + 2.0 * c.ct + 1e-6 && p < r1 - 2.0 * c.ct - 1e-6 { gable_truss(m, c, &format!("truss.{i}"), p, false, [None, None]); i += 1; }
        }
        // mono jack trusses on the end slopes, from the end wall to the girder
        let mut j = 0;
        for &x in &grid(s, rf.spacing) {
            let reach = x.min(s - x).min(r0);
            if x.min(s - x) < 0.05 { continue; }
            let xx = x.clamp(c.ct / 2.0, s - c.ct / 2.0);
            for (end, rmap) in [(0, 1.0), (1, -1.0)] {
                let rr = |q: f64| if end == 0 { q } else { r - q };
                let g = r0;
                let key = format!("jackTruss.{j}");
                bar3(m, format!("{key}.bottom"), "trusses", [c.ct, c.cd], "lumber", fr.at(xx, 0.0, rr(0.0)), fr.at(xx, 0.0, rr(g)), UP, true);
                bar3(m, format!("{key}.top"), "trusses", [c.ct, c.cd], "lumber", fr.at(xx, heel - k * oh, rr(-oh)), fr.at(xx, heel + k * reach, rr(reach)), UP, true);
                if g - reach > 0.05 { bar3(m, format!("{key}.top2"), "trusses", [c.ct, c.cd], "lumber", fr.at(xx, heel + k * reach, rr(reach)), fr.at(xx, heel + k * reach, rr(g)), UP, true); }
                let ye = heel + k * reach;
                if ye - c.cd > 0.05 { bar3(m, format!("{key}.end"), "trusses", c.web, "lumber", fr.at(xx, c.cd, rr(g - c.web[0] / 2.0)), fr.at(xx, ye, rr(g - c.web[0] / 2.0)), scale(fr.er, rmap), false); }
                j += 1;
            }
        }
        side_jacks(m, &mut jl, &mut jr);
        hips(m);
    }
    let dv = c.dv(k);
    let (ye, yp) = (heel - k * oh + dv, peak_y + dv);
    let mut sh = Shape::default();
    let dedupe = |v: Vec<P3>| -> Vec<P3> { let mut o: Vec<P3> = vec![]; for p in v { if o.last().map(|q| plannr_core::geom::dist(*q, p) < 1e-6).unwrap_or(false) { continue; } o.push(p); } o };
    sh.planes.push(Plane { key: "l".into(), pts: dedupe(vec![fr.at(-oh, ye, -oh), fr.at(-oh, ye, r + oh), fr.at(half, yp, r1), fr.at(half, yp, r0)]), holes: vec![] });
    sh.planes.push(Plane { key: "r".into(), pts: dedupe(vec![fr.at(s + oh, ye, -oh), fr.at(s + oh, ye, r + oh), fr.at(half, yp, r1), fr.at(half, yp, r0)]), holes: vec![] });
    sh.planes.push(Plane { key: "front".into(), pts: vec![fr.at(-oh, ye, -oh), fr.at(s + oh, ye, -oh), fr.at(half, yp, r0)], holes: vec![] });
    sh.planes.push(Plane { key: "back".into(), pts: vec![fr.at(-oh, ye, r + oh), fr.at(s + oh, ye, r + oh), fr.at(half, yp, r1)], holes: vec![] });
    let ys = fr.origin[1] + heel - k * oh;
    sh.eaves.push(Eave { a: fr.at(-oh, ye, -oh), b: fr.at(-oh, ye, r + oh), out: scale(fr.es, -1.0), soffit: Some((vec![c.plan(-oh, -oh), c.plan(-oh, r + oh), c.plan(0.0, r), c.plan(0.0, 0.0)], ys)), gutter: true });
    sh.eaves.push(Eave { a: fr.at(s + oh, ye, -oh), b: fr.at(s + oh, ye, r + oh), out: fr.es, soffit: Some((vec![c.plan(s + oh, -oh), c.plan(s + oh, r + oh), c.plan(s, r), c.plan(s, 0.0)], ys)), gutter: true });
    sh.eaves.push(Eave { a: fr.at(-oh, ye, -oh), b: fr.at(s + oh, ye, -oh), out: scale(fr.er, -1.0), soffit: Some((vec![c.plan(-oh, -oh), c.plan(s + oh, -oh), c.plan(s, 0.0), c.plan(0.0, 0.0)], ys)), gutter: true });
    sh.eaves.push(Eave { a: fr.at(-oh, ye, r + oh), b: fr.at(s + oh, ye, r + oh), out: fr.er, soffit: Some((vec![c.plan(-oh, r + oh), c.plan(s + oh, r + oh), c.plan(s, r), c.plan(0.0, r)], ys)), gutter: true });
    sh
}

// ---- shed -------------------------------------------------------------------------------------------

fn shed(m: &mut Vec<Member>, c: &Ctx) -> Shape {
    let (rf, fr, s, r, k, heel, oh) = (c.rf, c.fr, c.s, c.r, c.k, c.heel, c.oh);
    let high_max = rf.high_side != "min";
    let sx = |sg: f64| if high_max { sg } else { s - sg };
    let pt = |sg: f64| [sx(sg), heel + k * sg];
    let positions = grid(r, rf.spacing);
    if c.trusses() {
        let chord = [c.ct, c.cd];
        for (i, &p) in positions.iter().enumerate() {
            let rr = p.clamp(c.ct / 2.0, r - c.ct / 2.0);
            let pre = format!("truss.{i}");
            fr.bar(m, format!("{pre}.top"), "trusses", chord, "lumber", pt(-oh), pt(s + oh), rr, true);
            fr.bar(m, format!("{pre}.bottom"), "trusses", chord, "lumber", [sx(0.0), 0.0], [sx(s), 0.0], rr, true);
            let top_at = |sg: f64| heel + k * sg;
            // end and heel posts in the truss plane, flush with the wall faces, cut under the top chord
            let w = c.web[1];
            fr.bar(m, format!("{pre}.end"), "trusses", c.web, "lumber", [sx(s - w / 2.0), c.cd], [sx(s - w / 2.0), top_at(s - w)], rr, false);
            if c.raised > 0.0 { fr.bar(m, format!("{pre}.heel"), "trusses", c.web, "lumber", [sx(w / 2.0), c.cd], [sx(w / 2.0), top_at(0.0)], rr, false); }
            let (a, b) = (s / 3.0, 2.0 * s / 3.0);
            fr.bar(m, format!("{pre}.web.0"), "trusses", c.web, "lumber", [sx(a), c.cd], [sx(a), top_at(a)], rr, false);
            fr.bar(m, format!("{pre}.web.1"), "trusses", c.web, "lumber", [sx(b), c.cd], [sx(b), top_at(b)], rr, false);
            fr.bar(m, format!("{pre}.web.2"), "trusses", c.web, "lumber", [sx(a), c.cd], [sx(b), top_at(b)], rr, false);
        }
    } else {
        for (i, &p) in positions.iter().enumerate() {
            let rr = p.clamp(c.rt / 2.0, r - c.rt / 2.0);
            fr.bar(m, format!("rafter.{i}"), "rafters", [c.rt, c.rd], "lumber", pt(-oh), pt(s + oh), rr, true);
        }
        // the high end: rafters sit (birdsmouthed) on a pony wall built on the high wall's top plate —
        // its plate top meets the rafter underside at the wall's outer face — or hang on a ledger
        let y_hi = heel + k * s;
        if rf.high_support == "ledger" {
            member(m, "ledger", "ledger", [c.bt, c.bd], "lumber", fr.at(sx(s), 0.0, 0.0), fr.er, UP, fr.es, [[0.0, r], [y_hi - c.bd, y_hi], [0.0, c.bt]], 0);
        } else if y_hi > 0.02 {
            let [pt_, pd] = [c.ct, c.seat.max(c.cd.min(c.seat))];
            let w = c.seat;
            let x0 = sx(s - w); let x1 = sx(s);
            let (xa, xb) = (x0.min(x1), x0.max(x1));
            // bottom (sole) plate on the wall's top plate, top plate under the rafters, studs at the spacing
            member(m, "pony.plate.bottom", "plates", [pt_, pd], "lumber", fr.at(xa, 0.0, 0.0), fr.er, UP, fr.es, [[0.0, r], [0.0, pt_.min(y_hi / 2.0)], [0.0, xb - xa]], 0);
            if y_hi > 2.0 * pt_ + 0.02 {
                member(m, "pony.plate.top", "plates", [pt_, pd], "lumber", fr.at(xa, 0.0, 0.0), fr.er, UP, fr.es, [[0.0, r], [y_hi - pt_, y_hi], [0.0, xb - xa]], 0);
                for (j, q) in grid(r, rf.spacing).into_iter().enumerate() {
                    let qq = q.clamp(c.ct / 2.0, r - c.ct / 2.0);
                    member(m, format!("pony.stud.{j}"), "studs", [c.ct, pd], "lumber", fr.at(xa, 0.0, 0.0), fr.er, UP, fr.es, [[qq - c.ct / 2.0, qq + c.ct / 2.0], [pt_, y_hi - pt_], [0.0, xb - xa]], 1);
                }
            }
        }
    }
    let dv = c.dv(k);
    let (ylo, yhi) = (heel - k * oh + dv, heel + k * (s + oh) + dv);
    let up_dir = if high_max { fr.es } else { scale(fr.es, -1.0) };
    let mut sh = Shape::default();
    sh.planes.push(Plane { key: "main".into(), pts: vec![fr.at(sx(-oh), ylo, 0.0), fr.at(sx(s + oh), yhi, 0.0), fr.at(sx(s + oh), yhi, r), fr.at(sx(-oh), ylo, r)], holes: vec![] });
    let y0 = fr.origin[1];
    sh.eaves.push(Eave { a: fr.at(sx(-oh), ylo, 0.0), b: fr.at(sx(-oh), ylo, r), out: scale(up_dir, -1.0), soffit: Some((vec![c.plan(sx(-oh), 0.0), c.plan(sx(-oh), r), c.plan(sx(0.0), r), c.plan(sx(0.0), 0.0)], y0 + heel - k * oh)), gutter: true });
    sh.eaves.push(Eave { a: fr.at(sx(s + oh), yhi, 0.0), b: fr.at(sx(s + oh), yhi, r), out: up_dir, soffit: Some((vec![c.plan(sx(s), 0.0), c.plan(sx(s), r), c.plan(sx(s + oh), r), c.plan(sx(s + oh), 0.0)], y0 + heel + k * s)), gutter: false });
    for (rr, o) in [(0.0, -1.0), (r, 1.0)] { sh.rakes.push(Rake { a: fr.at(sx(-oh), ylo, rr), b: fr.at(sx(s + oh), yhi, rr), out: scale(fr.er, o) }); }
    sh
}

/// Plumb depth of a birdsmouth heel cut below the plate top at the wall's outer face: the full plate
/// width as the seat, shortened so the notch (measured square to the rafter) stays within a third of
/// the rafter depth.
fn birdsmouth(seat: f64, rd: f64, k: f64) -> f64 {
    if k <= 1e-9 { return 0.0; }
    let max_seat = rd / 3.0 * (1.0 + k * k).sqrt() / k;
    k * seat.min(max_seat)
}

// ---- gambrel ----------------------------------------------------------------------------------------

fn gambrel(m: &mut Vec<Member>, c: &Ctx) -> Shape {
    let (rf, fr, s, r, half, ku, heel, oh) = (c.rf, c.fr, c.s, c.r, c.half, c.k, c.heel, c.oh);
    let kl = rf.lower_pitch.max(0.1) / 12.0;
    // the lower rafters seat on the plate at their own (steeper) pitch
    let heel = if rf.type_ == "rafters" { -birdsmouth(c.seat, c.rd, kl) } else { heel };
    let sk = rf.break_ratio.clamp(0.05, 0.45) * s;
    let yk = heel + kl * sk;
    let yp = yk + ku * (half - sk);
    let under = |sg: f64| if sg <= sk { heel + kl * sg } else { yk + ku * (sg - sk) };
    let rafter = [c.rt, c.rd];
    { let yt = ridge_top(c, yp, ku); let depth = c.bd.max(c.rd * (1.0 + ku * ku).sqrt()); member(m, "ridge", "ridge", [c.bt, depth], "lumber", fr.at(half, 0.0, 0.0), fr.er, UP, fr.es, [[0.0, r], [yt - depth, yt], [-c.bt / 2.0, c.bt / 2.0]], 0); }
    // purlins at the knees carried by knee walls standing on the ceiling joists
    for (lr, x) in [("l", sk), ("r", s - sk)] {
        member(m, format!("purlin.{lr}"), "purlins", rafter, "lumber", fr.at(x, 0.0, 0.0), fr.er, UP, fr.es, [[0.0, r], [yk - c.rd, yk], [-c.rt / 2.0, c.rt / 2.0]], 0);
        let (y0, y1) = (c.cd, yk - c.rd);
        if y1 - y0 > 0.1 {
            member(m, format!("kneeWall.{lr}.plate"), "plates", [c.ct, c.web[1]], "lumber", fr.at(x, 0.0, 0.0), fr.er, UP, fr.es, [[0.0, r], [y0, y0 + c.ct], [-c.web[1] / 2.0, c.web[1] / 2.0]], 0);
            for (j, q) in grid(r, rf.spacing).into_iter().enumerate() {
                let qq = q.clamp(c.ct / 2.0, r - c.ct / 2.0);
                member(m, format!("kneeWall.{lr}.stud.{j}"), "studs", [c.ct, c.web[1]], "lumber", fr.at(x, 0.0, 0.0), fr.er, UP, fr.es, [[qq - c.ct / 2.0, qq + c.ct / 2.0], [y0 + c.ct, y1], [-c.web[1] / 2.0, c.web[1] / 2.0]], 1);
            }
        }
    }
    for (i, &p) in grid(r, rf.spacing).iter().enumerate() {
        let rr = p.clamp(c.rt / 2.0, r - c.rt / 2.0);
        for (side, lr) in ["l", "r"].iter().enumerate() {
            let pt = |sg: f64| [if side == 0 { sg } else { s - sg }, under(sg)];
            fr.bar(m, format!("rafter.{i}.{lr}.lower"), "rafters", rafter, "lumber", pt(-oh), pt(sk), rr, true);
            fr.bar(m, format!("rafter.{i}.{lr}.upper"), "rafters", rafter, "lumber", pt(sk), pt(half - c.bt / 2.0), rr, true);
        }
        fr.bar(m, format!("ceilingJoist.{i}"), "joists", [c.ct, c.cd], "lumber", [0.0, 0.0], [s, 0.0], beside(c, rr), true);
        // collar tie between the upper rafters, a third of the way down from the ridge, beside them
        let yc = yp - (yp - yk) / 3.0;
        let sgc = sk + (yc - yk) / ku;
        if half - sgc > 0.2 { fr.bar(m, format!("collar.{i}"), "collars", [c.ct, c.cd], "lumber", [sgc, yc - c.cd], [s - sgc, yc - c.cd], beside(c, rr), true); }
    }
    // top-of-rafter lines of both slopes meet at sg*
    let (dl, du) = (c.rd * (1.0 + kl * kl).sqrt(), c.rd * (1.0 + ku * ku).sqrt());
    let top_l = |sg: f64| heel + kl * sg + dl;
    let top_u = |sg: f64| yk + ku * (sg - sk) + du;
    let knee = if (kl - ku).abs() > 1e-9 { ((yk - ku * sk + du) - (heel + dl)) / (kl - ku) } else { sk };
    let knee = knee.clamp(0.0, half);
    let mut sh = Shape::default();
    for (side, lr) in ["l", "r"].iter().enumerate() {
        let sx = |sg: f64| if side == 0 { sg } else { s - sg };
        sh.planes.push(Plane { key: format!("{lr}.lower"), pts: vec![fr.at(sx(-oh), top_l(-oh), 0.0), fr.at(sx(knee), top_l(knee), 0.0), fr.at(sx(knee), top_l(knee), r), fr.at(sx(-oh), top_l(-oh), r)], holes: vec![] });
        sh.planes.push(Plane { key: format!("{lr}.upper"), pts: vec![fr.at(sx(knee), top_u(knee), 0.0), fr.at(half, top_u(half), 0.0), fr.at(half, top_u(half), r), fr.at(sx(knee), top_u(knee), r)], holes: vec![] });
        let out = if side == 0 { scale(fr.es, -1.0) } else { fr.es };
        sh.eaves.push(Eave { a: fr.at(sx(-oh), top_l(-oh), 0.0), b: fr.at(sx(-oh), top_l(-oh), r), out, soffit: Some((vec![c.plan(sx(-oh), 0.0), c.plan(sx(-oh), r), c.plan(sx(0.0), r), c.plan(sx(0.0), 0.0)], fr.origin[1] + heel - kl * oh)), gutter: true });
        for (rr, o) in [(0.0, -1.0), (r, 1.0)] {
            sh.rakes.push(Rake { a: fr.at(sx(-oh), top_l(-oh), rr), b: fr.at(sx(knee), top_l(knee), rr), out: scale(fr.er, o) });
            sh.rakes.push(Rake { a: fr.at(sx(knee), top_u(knee), rr), b: fr.at(half, top_u(half), rr), out: scale(fr.er, o) });
        }
    }
    sh
}
