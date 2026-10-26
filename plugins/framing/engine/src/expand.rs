//! Geometry of the framing plugin: walls, floors, roofs and foundations become lists of members
//! (lumber boxes), panels (sheathing / gypsum / subfloor) and concrete, which are emitted as faces.

use plannr_core::geom::{add, area as poly_area, cross, dot, newell, normalize, scale, P3};
use plannr_core::model::{Entity, Face};

use crate::data::*;

// ---- face builder ---------------------------------------------------------------------------------

pub struct Faces { pub out: Vec<Entity>, layer: String }
impl Faces {
    pub fn new(layer: &str) -> Self { Faces { out: vec![], layer: layer.to_string() } }
    pub fn poly_holes(&mut self, id: &str, mut pts: Vec<P3>, holes: Vec<Vec<P3>>, n: P3, material: &str) {
        if pts.len() < 3 { return; }
        if dot(newell(&pts), n) < 0.0 { pts.reverse(); }
        if poly_area(&pts) < 1e-9 { return; }
        self.out.push(Entity::Face(Face { id: id.to_string(), layer: self.layer.clone(), outer: pts, holes, material: Some(material.to_string()), back_material: Some(material.to_string()), extra: Default::default() }));
    }
    pub fn poly(&mut self, id: &str, pts: Vec<P3>, n: P3, material: &str) { self.poly_holes(id, pts, vec![], n, material); }
    /// An oriented box: `origin + ex·x + ey·y + ez·z` over the three ranges; `ex, ey, ez` orthonormal.
    pub fn boxf(&mut self, id: &str, origin: P3, ex: P3, ey: P3, ez: P3, r: [[f64; 2]; 3], material: &str) {
        let at = |x: f64, y: f64, z: f64| add(add(add(origin, scale(ex, x)), scale(ey, y)), scale(ez, z));
        let [[x0, x1], [y0, y1], [z0, z1]] = r;
        if x1 - x0 < 1e-6 || y1 - y0 < 1e-6 || z1 - z0 < 1e-6 { return; }
        self.poly(id, vec![at(x0, y0, z0), at(x0, y1, z0), at(x1, y1, z0), at(x1, y0, z0)], scale(ez, -1.0), material);
        self.poly(id, vec![at(x0, y0, z1), at(x1, y0, z1), at(x1, y1, z1), at(x0, y1, z1)], ez, material);
        self.poly(id, vec![at(x0, y0, z0), at(x1, y0, z0), at(x1, y0, z1), at(x0, y0, z1)], scale(ey, -1.0), material);
        self.poly(id, vec![at(x0, y1, z0), at(x0, y1, z1), at(x1, y1, z1), at(x1, y1, z0)], ey, material);
        self.poly(id, vec![at(x0, y0, z0), at(x0, y0, z1), at(x0, y1, z1), at(x0, y1, z0)], scale(ex, -1.0), material);
        self.poly(id, vec![at(x1, y0, z0), at(x1, y1, z0), at(x1, y1, z1), at(x1, y0, z1)], ex, material);
    }
    /// A horizontal slab from a plan polygon (y0..y1).
    pub fn slab(&mut self, id: &str, plan: &[P2], y0: f64, y1: f64, material: &str) {
        if plan.len() < 3 || y1 - y0 < 1e-6 { return; }
        let bottom: Vec<P3> = plan.iter().map(|p| to_p3(*p, y0)).collect();
        let top: Vec<P3> = plan.iter().map(|p| to_p3(*p, y1)).collect();
        self.poly(id, bottom, [0.0, -1.0, 0.0], material);
        self.poly(id, top, [0.0, 1.0, 0.0], material);
        let n = plan.len();
        for i in 0..n {
            let (a, b) = (plan[i], plan[(i + 1) % n]);
            let d = [b[0] - a[0], 0.0, b[1] - a[1]]; let nrm = normalize(cross(d, [0.0, 1.0, 0.0]));
            // outward for a counter-clockwise plan polygon; `poly` flips when the winding says otherwise
            let out_n = if plan_area(plan) >= 0.0 { nrm } else { scale(nrm, -1.0) };
            self.poly(id, vec![to_p3(a, y0), to_p3(b, y0), to_p3(b, y1), to_p3(a, y1)], out_n, material);
        }
    }
}
pub fn plan_area(p: &[P2]) -> f64 { let n = p.len(); let mut s = 0.0; for i in 0..n { let (a, b) = (p[i], p[(i + 1) % n]); s += a[0] * b[1] - b[0] * a[1]; } s / 2.0 }

// ---- the parts of an element ------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Member { pub key: String, pub category: &'static str, pub size: [f64; 2], pub size_label: String, pub length: f64, pub material: &'static str, pub origin: P3, pub ex: P3, pub ey: P3, pub ez: P3, pub r: [[f64; 2]; 3] }
#[derive(Debug, Clone)]
pub struct Panel { pub key: String, pub category: &'static str, pub area: f64, pub thickness: f64 }
#[derive(Debug, Clone)]
pub struct Concrete { pub key: String, pub volume: f64 }

/// Items counted by length that are not lumber (gutters).
#[derive(Debug, Clone)]
pub struct Linear { pub key: String, pub category: &'static str, pub length: f64 }

#[derive(Debug, Clone, Default)]
pub struct Expansion { pub members: Vec<Member>, pub panels: Vec<Panel>, pub concrete: Vec<Concrete>, pub linear: Vec<Linear>, pub faces: Vec<Entity>, pub hardware: Vec<crate::hardware::Piece> }

impl Expansion {
    pub fn lumber_length(&self) -> f64 { self.members.iter().map(|m| m.length).sum() }
}

pub(crate) fn label(size: [f64; 2]) -> String {
    let mm = |v: f64| (v * 1000.0).round() as i64;
    match (mm(size[0]), mm(size[1])) { (38, 89) => "2x4".into(), (38, 140) => "2x6".into(), (38, 184) => "2x8".into(), (38, 235) => "2x10".into(), (38, 286) => "2x12".into(), (89, 89) => "4x4".into(), (a, b) => format!("{a}x{b}") }
}

/// Frame-local member: box ranges in a frame (origin, ex, ey, ez); `along` says which local axis is the length.
#[allow(clippy::too_many_arguments)]
pub(crate) fn member(b: &mut Vec<Member>, key: impl Into<String>, category: &'static str, size: [f64; 2], material: &'static str, origin: P3, ex: P3, ey: P3, ez: P3, r: [[f64; 2]; 3], along: usize) {
    let [[x0, x1], [y0, y1], [z0, z1]] = r;
    if x1 - x0 < 1e-6 || y1 - y0 < 1e-6 || z1 - z0 < 1e-6 { return; }
    let length = [x1 - x0, y1 - y0, z1 - z0][along];
    b.push(Member { key: key.into(), category, size, size_label: label(size), length, material, origin, ex, ey, ez, r });
}

pub(crate) fn emit(ex: &Expansion, id: &str, f: &mut Faces) {
    for m in &ex.members { f.boxf(&format!("{id}#{}", m.key), m.origin, m.ex, m.ey, m.ez, m.r, &material_id(m.material)); }
}

const UP: P3 = [0.0, 1.0, 0.0];

// ---- walls --------------------------------------------------------------------------------------------

pub fn expand_wall(id: &str, w: &Wall, house: &House, layer: &str) -> Expansion {
    let mut ex = Expansion::default();
    let l = w.length(); if l < 0.05 { return ex; }
    let base = house.base(w.level);
    let h = w.height.unwrap_or_else(|| { let s = house.height(w.level) - house.floor_thickness; if s > 0.5 { s } else { 2.44 } });
    let [t, d] = w.stud(); let [pt, pd] = w.plate();
    let band = crate::junction::Band::of(w);
    let joins = crate::junction::joins(id, w, &house.walls);
    let u = normalize([w.b[0] - w.a[0], 0.0, w.b[1] - w.a[1]]);
    let n = [band.n[0], 0.0, band.n[1]];                    // exterior normal (left of a→b unless flipped)
    // local frame: ex = along the wall from `a`, ey = up, ez = exterior; z = 0 is the middle of the studs
    let origin = to_p3(band.at(0.0, band.centre()), base);
    let (ex_, ey_, ez_) = (u, UP, n);
    // the framing runs from t0 to t1 along the drawn line: junctions trim or extend it
    let t0 = joins.start.as_ref().map(|j| j.frame).unwrap_or(0.0);
    let t1 = joins.end.as_ref().map(|j| j.frame).unwrap_or(l);
    if t1 - t0 < 2.0 * t { return ex; }
    let m = &mut ex.members;
    let bp = w.bottom_plates as usize; let tp = w.top_plates as usize;
    for i in 0..bp { member(m, format!("plate.bottom.{i}"), "plates", [pt, pd], "plate", origin, ex_, ey_, ez_, [[t0, t1], [i as f64 * pt, (i + 1) as f64 * pt], [-pd / 2.0, pd / 2.0]], 0); }
    // top plates: the lower ones span the framing; the uppermost laps over / is cut back at corners and is cut where partitions lap in
    for i in 0..tp {
        let yr = [h - (i + 1) as f64 * pt, h - i as f64 * pt];
        if i == 0 && tp >= 2 {
            let u0 = joins.start.as_ref().map(|j| j.upper_plate).unwrap_or(t0);
            let u1 = joins.end.as_ref().map(|j| j.upper_plate).unwrap_or(t1);
            let mut pieces = vec![[u0, u1]];
            for bk in &joins.backers {
                let (g0, g1) = (bk.centre - bk.width / 2.0, bk.centre + bk.width / 2.0);
                pieces = pieces.into_iter().flat_map(|[a, b]| if g1 <= a || g0 >= b { vec![[a, b]] } else { vec![[a, g0], [g1, b]] }).filter(|[a, b]| b - a > 0.02).collect();
            }
            for (k, [a, b]) in pieces.into_iter().enumerate() {
                let key = if k == 0 { format!("plate.top.{i}") } else { format!("plate.top.{i}.{k}") };
                member(m, key, "plates", [pt, pd], "plate", origin, ex_, ey_, ez_, [[a, b], yr, [-pd / 2.0, pd / 2.0]], 0);
            }
        } else {
            member(m, format!("plate.top.{i}"), "plates", [pt, pd], "plate", origin, ex_, ey_, ez_, [[t0, t1], yr, [-pd / 2.0, pd / 2.0]], 0);
        }
    }
    let y0 = bp as f64 * pt; let y1 = h - tp as f64 * pt;
    let stud_box = |m: &mut Vec<Member>, key: String, cat: &'static str, ua: f64, ub: f64, ya: f64, yb: f64, mat: &'static str| {
        member(m, key, cat, [t, d], mat, origin, ex_, ey_, ez_, [[ua.max(t0), ub.min(t1)], [ya, yb], [-d / 2.0, d / 2.0]], 1);
    };
    // a stud turned flat against one face (side −1 = interior, +1 = exterior)
    let flat_box = |m: &mut Vec<Member>, key: String, ua: f64, ub: f64, side: f64| {
        let z = if side > 0.0 { [d / 2.0 - t, d / 2.0] } else { [-d / 2.0, -d / 2.0 + t] };
        member(m, key, "studs", [t, d], "lumber", origin, ex_, ey_, ez_, [[ua.max(t0), ub.min(t1)], [y0, y1], z], 1);
    };
    // openings: resolved zones (sorted, clamped)
    struct Zone { o: Opening, u0: f64, u1: f64, ro_bottom: f64, ro_top: f64, header: [f64; 2], header_top: f64 }
    let mut zones: Vec<Zone> = w.openings.iter().filter(|o| o.width > 0.05 && o.height > 0.05).map(|o| {
        let u0 = o.offset.max(t0); let u1 = (o.offset + o.width).min(t1);
        let is_door = o.kind == "door";
        let ro_bottom = if is_door { y0 } else { o.sill.max(y0 + t) };
        let ro_top = (ro_bottom + o.height).min(y1 - 0.02);
        let hs = o.header_size.as_deref().map(|s| lumber_or(s, "2x6")).unwrap_or_else(|| lumber_or(header_size_for_span(o.width), "2x6"));
        let header_top = (ro_top + hs[1]).min(y1);
        Zone { o: o.clone(), u0, u1, ro_bottom, ro_top, header: hs, header_top }
    }).collect();
    zones.sort_by(|a, b| a.u0.partial_cmp(&b.u0).unwrap());
    let kings = |z: &Zone| z.o.king_studs.max(1) as f64; let jacks = |z: &Zone| z.o.jack_studs.max(1) as f64;
    let zone_lo = |z: &Zone| z.u0 - t * (jacks(z) + kings(z)); let zone_hi = |z: &Zone| z.u1 + t * (jacks(z) + kings(z));
    let in_zone = |u: f64| zones.iter().any(|z| u + t > zone_lo(z) + 1e-6 && u < zone_hi(z) - 1e-6);
    // corner and partition-backer studs (intervals along the wall that regular studs must not overlap)
    let mut extra: Vec<[f64; 2]> = vec![];
    for (end_key, j) in [("start", joins.start.as_ref()), ("end", joins.end.as_ref())] {
        let Some(j) = j else { continue };
        if !(j.kind == crate::junction::Kind::L && j.through) { continue; }
        let at_end = end_key == "end";
        match w.corner_detail.as_str() {
            "california" => { let (a, b) = (j.inner - d / 2.0, j.inner + d / 2.0); if in_zone(a) { continue; } flat_box(m, format!("corner.{end_key}.0"), a, b, j.side); extra.push([a, b]); }
            "threeStud" => { let (a, b) = if at_end { (j.inner - t, j.inner) } else { (j.inner, j.inner + t) }; if in_zone(a) { continue; } stud_box(m, format!("corner.{end_key}.0"), "studs", a, b, y0, y1, "lumber"); extra.push([a, b]); }
            _ => {}
        }
    }
    // regular studs: at t0, every spacing, and at t1
    let mut positions: Vec<f64> = vec![];
    let sp = w.stud_spacing.max(0.1);
    let mut p = t0; while p <= t1 - t - 1e-6 { positions.push(p); p += sp; }
    if w.end_stud && (positions.last().map(|q| t1 - t - q > 1e-6).unwrap_or(true)) { positions.push(t1 - t); }
    if !w.start_stud && positions.first().map(|q| (*q - t0).abs() < 1e-9).unwrap_or(false) { positions.remove(0); }
    for (bi, bk) in joins.backers.iter().enumerate() {
        let (g0, g1) = (bk.centre - bk.width / 2.0, bk.centre + bk.width / 2.0);
        if in_zone(g0) || in_zone(g1 - t) { continue; }
        match w.t_detail.as_str() {
            "twoStud" => { for (k, a) in [g0 - t, g1].into_iter().enumerate() { stud_box(m, format!("tbacker.{bi}.{k}"), "studs", a, a + t, y0, y1, "lumber"); extra.push([a, a + t]); } }
            "california" => { let (a, b) = (bk.centre - d / 2.0, bk.centre + d / 2.0); flat_box(m, format!("tbacker.{bi}.0"), a, b, bk.side); extra.push([a, b]); }
            "ladder" => {
                // flat blocks every 0.6 m between the regular studs either side of the partition
                let lo = positions.iter().copied().filter(|q| q + t <= g0 + 1e-6).fold(t0, f64::max) + t;
                let hi = positions.iter().copied().filter(|q| *q >= g1 - 1e-6).fold(t1, f64::min);
                let z = if bk.side > 0.0 { [d / 2.0 - t, d / 2.0] } else { [-d / 2.0, -d / 2.0 + t] };
                let mut y = y0 + 0.3; let mut k = 0;
                while y + d < y1 - 0.05 { member(m, format!("tbacker.{bi}.block.{k}"), "blocking", [t, d], "lumber", origin, ex_, ey_, ez_, [[lo, hi], [y, y + d], z], 0); y += 0.6; k += 1; }
            }
            _ => {}
        }
    }
    let overlaps_extra = |p: f64| extra.iter().any(|[a, b]| p + t > *a + 1e-4 && p < *b - 1e-4);
    let mut si = 0;
    for (k, &p) in positions.iter().enumerate() {
        let is_end = k == 0 || k == positions.len() - 1;
        if in_zone(p) || (!is_end && overlaps_extra(p)) { continue; }
        stud_box(m, format!("stud.{si}"), "studs", p, p + t, y0, y1, "lumber"); si += 1;
    }
    // openings
    for (oi, z) in zones.iter().enumerate() {
        let nj = jacks(z) as usize; let nk = kings(z) as usize;
        for j in 0..nj { stud_box(m, format!("jack.{oi}.l.{j}"), "studs", z.u0 - t * (j + 1) as f64, z.u0 - t * j as f64, y0, z.ro_top, "lumber"); stud_box(m, format!("jack.{oi}.r.{j}"), "studs", z.u1 + t * j as f64, z.u1 + t * (j + 1) as f64, y0, z.ro_top, "lumber"); }
        for k in 0..nk { let a = z.u0 - t * (nj + k + 1) as f64; stud_box(m, format!("king.{oi}.l.{k}"), "studs", a, a + t, y0, y1, "lumber"); let b = z.u1 + t * (nj + k) as f64; stud_box(m, format!("king.{oi}.r.{k}"), "studs", b, b + t, y0, y1, "lumber"); }
        // header: two plies spanning between the kings, sitting on the jacks
        let hu0 = z.u0 - t * nj as f64; let hu1 = z.u1 + t * nj as f64; let [ht, hd] = z.header;
        if z.header_top - z.ro_top > 0.01 {
            member(m, format!("header.{oi}"), "headers", [ht, hd], "header", origin, ex_, ey_, ez_, [[hu0, hu1], [z.ro_top, z.header_top], [-d / 2.0, -d / 2.0 + ht]], 0);
            member(m, format!("header.{oi}"), "headers", [ht, hd], "header", origin, ex_, ey_, ez_, [[hu0, hu1], [z.ro_top, z.header_top], [d / 2.0 - ht, d / 2.0]], 0);
        }
        // cripples above the header
        let mut ci = 0;
        if y1 - z.header_top > t + 0.01 {
            for &p in &positions { if p + t > hu0 + 1e-6 && p < hu1 - 1e-6 { stud_box(m, format!("cripple.{oi}.{ci}"), "cripples", p, p + t, z.header_top, y1, "lumber"); ci += 1; } }
        }
        // windows: a flat sill and cripples below it
        if z.o.kind != "door" {
            let sill_top = z.ro_bottom;
            if sill_top - t > y0 + 1e-6 {
                member(m, format!("sill.{oi}"), "plates", [t, d], "plate", origin, ex_, ey_, ez_, [[z.u0, z.u1], [sill_top - t, sill_top], [-d / 2.0, d / 2.0]], 0);
                for &p in &positions { if p + t > z.u0 + 1e-6 && p < z.u1 - 1e-6 { stud_box(m, format!("cripple.{oi}.{ci}"), "cripples", p, p + t, y0, sill_top - t, "lumber"); ci += 1; } }
            }
        }
    }
    // faces for members
    let mut f = Faces::new(layer);
    emit(&ex, id, &mut f);
    // sheathing panels: exterior and interior, each with its own extent along the wall (laps at corners)
    let holes: Vec<[f64; 4]> = zones.iter().map(|z| [z.u0, z.u1, z.ro_bottom, z.ro_top]).collect();
    let opening_area: f64 = holes.iter().map(|h| (h[1] - h[0]) * (h[3] - h[2])).sum();
    let panel = |f: &mut Faces, key: &str, cat: &'static str, mat: &str, s0: f64, s1: f64, z0: f64, z1: f64| -> Option<Panel> {
        if s1 - s0 < 0.01 { return None; }
        let at = |x: f64, y: f64, z: f64| add(add(add(origin, scale(ex_, x)), scale(ey_, y)), scale(ez_, z));
        let outer = |z: f64| vec![at(s0, 0.0, z), at(s1, 0.0, z), at(s1, h, z), at(s0, h, z)];
        let hs = |z: f64| holes.iter().filter(|hh| hh[0] >= s0 - 1e-6 && hh[1] <= s1 + 1e-6).map(|hh| vec![at(hh[0], hh[2], z), at(hh[1], hh[2], z), at(hh[1], hh[3], z), at(hh[0], hh[3], z)]).collect::<Vec<_>>();
        let fid = format!("{id}#{key}");
        f.poly_holes(&fid, outer(z1), hs(z1), if z1 > z0 { ez_ } else { scale(ez_, -1.0) }, mat);
        f.poly_holes(&fid, outer(z0), hs(z0), if z1 > z0 { scale(ez_, -1.0) } else { ez_ }, mat);
        let (za, zb) = (z0.min(z1), z0.max(z1));
        f.poly(&fid, vec![at(s0, 0.0, za), at(s0, 0.0, zb), at(s1, 0.0, zb), at(s1, 0.0, za)], scale(ey_, -1.0), mat);
        f.poly(&fid, vec![at(s0, h, za), at(s1, h, za), at(s1, h, zb), at(s0, h, zb)], ey_, mat);
        f.poly(&fid, vec![at(s0, 0.0, za), at(s0, h, za), at(s0, h, zb), at(s0, 0.0, zb)], scale(ex_, -1.0), mat);
        f.poly(&fid, vec![at(s1, 0.0, za), at(s1, 0.0, zb), at(s1, h, zb), at(s1, h, za)], ex_, mat);
        Some(Panel { key: key.into(), category: cat, area: ((s1 - s0) * h - opening_area).max(0.0), thickness: (zb - za).abs() })
    };
    if w.sheathing.exterior.enabled {
        let ts = w.sheathing.exterior.thickness;
        let (s0, s1) = (joins.start.as_ref().map(|j| j.ext).unwrap_or(t0), joins.end.as_ref().map(|j| j.ext).unwrap_or(t1));
        if let Some(p) = panel(&mut f, "sheathing.ext", "sheathing", &material_id("sheathing"), s0, s1, d / 2.0, d / 2.0 + ts) { ex.panels.push(p); }
    }
    if w.sheathing.interior.enabled {
        let ts = w.sheathing.interior.thickness;
        let (s0, s1) = (joins.start.as_ref().map(|j| j.int).unwrap_or(t0), joins.end.as_ref().map(|j| j.int).unwrap_or(t1));
        if let Some(p) = panel(&mut f, "sheathing.int", "gypsum", &material_id("gypsum"), s0, s1, -d / 2.0, -d / 2.0 - ts) { ex.panels.push(p); }
    }
    ex.faces = f.out;
    ex
}

// ---- floors ---------------------------------------------------------------------------------------------

/// Intervals where the line `axis = at` crosses the polygon (sorted pairs along the other axis).
fn scan(points: &[P2], axis: usize, at: f64) -> Vec<[f64; 2]> {
    let other = 1 - axis; let n = points.len(); let mut xs = vec![];
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let (ya, yb) = (a[axis], b[axis]);
        if (ya <= at && yb > at) || (yb <= at && ya > at) { let t = (at - ya) / (yb - ya); xs.push(a[other] + t * (b[other] - a[other])); }
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs.chunks(2).filter(|c| c.len() == 2 && c[1] - c[0] > 1e-6).map(|c| [c[0], c[1]]).collect()
}

pub fn expand_floor(id: &str, fl: &Floor, house: &House, layer: &str) -> Expansion {
    let mut ex = Expansion::default();
    if fl.points.len() < 3 { return ex; }
    let [jt, jd] = lumber_or(&fl.joist_size, "2x10");
    let y0 = house.base(fl.level) + fl.elevation; let y1 = y0 + jd;
    let (minx, maxx) = fl.points.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
    let (minz, maxz) = fl.points.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
    // joists span along `span_axis` (0 = x, 1 = z) and repeat along the other
    let span_axis = match fl.direction.as_str() { "x" => 0, "z" => 1, _ => if maxx - minx <= maxz - minz { 0 } else { 1 } };
    let rep_axis = 1 - span_axis;
    let (r0, r1) = if rep_axis == 0 { (minx, maxx) } else { (minz, maxz) };
    let rim = if fl.rim_joist { jt } else { 0.0 };
    let m = &mut ex.members;
    let sp = fl.joist_spacing.max(0.1);
    let mut positions: Vec<f64> = vec![]; let mut p = r0; while p <= r1 + 1e-6 { positions.push(p); p += sp; }
    if positions.last().map(|q| r1 - q > 1e-6).unwrap_or(true) { positions.push(r1); }
    let mut ji = 0;
    for (k, &p) in positions.iter().enumerate() {
        // first and last joists sit just inside the rim
        let (pa, pb) = if k == 0 { (r0 + rim, r0 + rim + jt) } else if k == positions.len() - 1 { (r1 - rim - jt, r1 - rim) } else { (p - jt / 2.0, p + jt / 2.0) };
        let mid = (pa + pb) / 2.0;
        for iv in scan(&fl.points, rep_axis, mid) {
            let (sa, sb) = (iv[0] + rim, iv[1] - rim); if sb - sa < 0.05 { continue; }
            let (xr, zr) = if rep_axis == 0 { ([pa, pb], [sa, sb]) } else { ([sa, sb], [pa, pb]) };
            member(m, format!("joist.{ji}"), "joists", [jt, jd], "lumber", [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], UP, [0.0, 0.0, 1.0], [xr, [y0, y1], zr], if span_axis == 0 { 0 } else { 2 });
            ji += 1;
        }
    }
    if fl.rim_joist {
        let n = fl.points.len(); let ccw = plan_area(&fl.points) >= 0.0;
        for i in 0..n {
            let (a, b) = (fl.points[i], fl.points[(i + 1) % n]);
            let d = [b[0] - a[0], 0.0, b[1] - a[1]]; let len = (d[0] * d[0] + d[2] * d[2]).sqrt(); if len < 0.05 { continue; }
            let u = scale(d, 1.0 / len); let inward = { let o = normalize(cross(UP, u)); if ccw { scale(o, -1.0) } else { o } };
            // rim along the edge, just inside it
            member(m, format!("rim.{i}"), "rim", [jt, jd], "lumber", to_p3(a, 0.0), u, UP, inward, [[0.0, len], [y0, y1], [0.0, jt]], 0);
        }
    }
    if fl.blocking && positions.len() >= 2 {
        let (s0, s1) = if span_axis == 0 { (minx, maxx) } else { (minz, maxz) };
        let mid = (s0 + s1) / 2.0;
        for (bi, w) in positions.windows(2).enumerate() {
            let (pa, pb) = (w[0] + jt / 2.0, w[1] - jt / 2.0); if pb - pa < 0.05 { continue; }
            let (xr, zr) = if rep_axis == 0 { ([pa, pb], [mid - jt / 2.0, mid + jt / 2.0]) } else { ([mid - jt / 2.0, mid + jt / 2.0], [pa, pb]) };
            member(m, format!("blocking.{bi}"), "blocking", [jt, jd], "lumber", [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], UP, [0.0, 0.0, 1.0], [xr, [y0, y1], zr], if rep_axis == 0 { 0 } else { 2 });
        }
    }
    let mut f = Faces::new(layer);
    emit(&ex, id, &mut f);
    if fl.subfloor.enabled {
        f.slab(&format!("{id}#subfloor"), &fl.points, y1, y1 + fl.subfloor.thickness, &material_id("sheathing"));
        ex.panels.push(Panel { key: "subfloor".into(), category: "subfloor", area: plan_area(&fl.points).abs(), thickness: fl.subfloor.thickness });
    }
    ex.faces = f.out;
    ex
}

// ---- roofs: see roof.rs ------------------------------------------------------------------------------------

pub use crate::roof::expand_roof;

// ---- foundations ------------------------------------------------------------------------------------------

pub fn expand_foundation(id: &str, fd: &Foundation, house: &House, layer: &str) -> Expansion {
    let mut ex = Expansion::default();
    if fd.points.len() < 3 { return ex; }
    let top = fd.top.unwrap_or_else(|| house.base(0));
    let mut f = Faces::new(layer);
    let conc = material_id("concrete");
    let area = plan_area(&fd.points).abs();
    if fd.type_ == "stemwall" {
        let n = fd.points.len(); let ccw = plan_area(&fd.points) >= 0.0;
        for i in 0..n {
            let (a, b) = (fd.points[i], fd.points[(i + 1) % n]);
            let d = [b[0] - a[0], 0.0, b[1] - a[1]]; let len = (d[0] * d[0] + d[2] * d[2]).sqrt(); if len < 0.05 { continue; }
            let u = scale(d, 1.0 / len); let out = { let o = normalize(cross(UP, u)); if ccw { o } else { scale(o, -1.0) } };
            let st = fd.stem_thickness; let fw = fd.footing_width.max(st);
            f.boxf(&format!("{id}#stem.{i}"), to_p3(a, 0.0), u, UP, out, [[0.0, len], [top - fd.stem_height, top], [-st / 2.0, st / 2.0]], &conc);
            f.boxf(&format!("{id}#footing.{i}"), to_p3(a, 0.0), u, UP, out, [[0.0, len], [top - fd.stem_height - fd.footing_depth, top - fd.stem_height], [-fw / 2.0, fw / 2.0]], &conc);
            ex.concrete.push(Concrete { key: format!("stem.{i}"), volume: len * st * fd.stem_height });
            ex.concrete.push(Concrete { key: format!("footing.{i}"), volume: len * fw * fd.footing_depth });
        }
        if fd.slab {
            f.slab(&format!("{id}#slab"), &fd.points, top - fd.slab_thickness, top, &conc);
            ex.concrete.push(Concrete { key: "slab".into(), volume: area * fd.slab_thickness });
        }
    } else {
        f.slab(&format!("{id}#slab"), &fd.points, top - fd.slab_thickness, top, &conc);
        ex.concrete.push(Concrete { key: "slab".into(), volume: area * fd.slab_thickness });
    }
    ex.faces = f.out;
    ex
}

/// Expansion of any framing entity.
pub fn expand_any(c: &plannr_core::model::Custom, house: &House, layer: &str) -> Expansion {
    let mut ex = expand_element(c, house, layer);
    crate::hardware::attach(&mut ex, c, house, layer);
    ex
}

/// Expansion of one framing entity without the connectors `hardware::attach` adds.
pub fn expand_element(c: &plannr_core::model::Custom, house: &House, layer: &str) -> Expansion {
    match c.type_.as_str() {
        T_BEAM => serde_json::from_value::<Beam>(c.data.clone()).map(|b| crate::beam::expand_beam(&c.id, &b, house, layer)).unwrap_or_default(),
        T_STAIR => serde_json::from_value::<Stair>(c.data.clone()).map(|s| crate::stair::expand_stair(&c.id, &s, house, layer)).unwrap_or_default(),
        T_WALL => serde_json::from_value::<Wall>(c.data.clone()).map(|w| expand_wall(&c.id, &w, house, layer)).unwrap_or_default(),
        T_FLOOR => serde_json::from_value::<Floor>(c.data.clone()).map(|w| expand_floor(&c.id, &w, house, layer)).unwrap_or_default(),
        T_ROOF => serde_json::from_value::<Roof>(c.data.clone()).map(|w| expand_roof(&c.id, &w, house, layer)).unwrap_or_default(),
        T_FOUNDATION => serde_json::from_value::<Foundation>(c.data.clone()).map(|w| expand_foundation(&c.id, &w, house, layer)).unwrap_or_default(),
        _ => Expansion::default(),
    }
}
