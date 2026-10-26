//! Roofs of the construction model, built the way a Belgian brick house is roofed: a wall plate
//! (muurplaat) on the inner leaf and the cavity of each eaves wall, purlins (gordingen) and a ridge
//! purlin (nokgording) bearing in the masonry gables, rafters (kepers) birdsmouthed on the plates,
//! counter battens (tengels), battens (panlatten) and the covering, with fascia, soffit and verge
//! boards. Hip roofs: plates all round, hip rafters, jack rafters and a ridge beam. Flat roofs:
//! joists on the inner leaves, deck, insulation and membrane behind a masonry upstand with coping.
//!
//! The geometry (`RoofGeom`) is shared with the masonry: the walls under a roof take their tops
//! from it (`RoofGeom::under`, the plates), so the gables rise in the wall's own build-up and the
//! walls and the roof always meet.

use plannr_core::geom::{add, cross, normalize, scale, sub, P3};

use crate::data::*;
use crate::expand::{footprint, Faces, Model, WallEntry, WallSet};
use crate::masonry::expand::Item;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind { Eave, Gable, Flat }

/// A wall plate along an eaves edge: its inward distances from the wall's outer face, its bottom
/// (absolute) and height.
#[derive(Debug, Clone, Copy)]
pub struct Plate { pub d: [f64; 2], pub bottom: f64, pub height: f64 }

/// One side of the roof's footprint: the outer face line of the wall under it (`a → b`), the plan
/// direction into the footprint, and what the roof does there.
#[derive(Debug, Clone)]
pub struct RoofEdge { pub a: P2, pub b: P2, pub inward: P2, pub kind: EdgeKind, pub plate: Option<Plate> }

/// Flat roof build-up (depths, m).
#[derive(Debug, Clone, Copy, Default)]
pub struct FlatBuild { pub joist: f64, pub deck: f64, pub insulation: f64, pub membrane: f64, pub upstand: f64 }
impl FlatBuild { pub fn total(&self) -> f64 { self.joist + self.deck + self.insulation + self.membrane } }

/// A roof resolved against the model. `u` runs along the ridge, `v` across it; both are the outer
/// faces of the walls under the roof (overhangs come on top).
#[derive(Debug, Clone)]
pub struct RoofGeom {
    pub id: String,
    pub style: RoofStyle,
    /// The storey the roof sits on.
    pub level: usize,
    pub along_x: bool,
    pub u: [f64; 2],
    pub v: [f64; 2],
    pub ov: f64,
    /// Overhang past each end of the ridge (0 at a party wall).
    pub ov_verge: [f64; 2],
    /// Top of the walls (the storey's floor-to-floor height).
    pub wall_top: f64,
    /// Slope (rise / run) of the pitched planes.
    pub k: f64,
    /// Rafter underside at the outer face of the eaves walls (absolute).
    pub y0: f64,
    /// Outer leaf width (the plate starts behind it), plate width and height, birdsmouth seat.
    pub d0: f64,
    pub pw: f64,
    pub ph: f64,
    pub seat: f64,
    pub cavity: f64,
    pub flat: FlatBuild,
    /// Plan rectangles (u range, v range) taken by taller parts of the building: the roof stops there.
    pub zones: Vec<[[f64; 2]; 2]>,
    /// v0 side, v1 side, u0 end, u1 end.
    pub edges: Vec<RoofEdge>,
}

impl RoofGeom {
    pub fn plan(&self, u: f64, v: f64) -> P2 { if self.along_x { [u, v] } else { [v, u] } }
    pub fn at(&self, u: f64, v: f64, y: f64) -> P3 { let p = self.plan(u, v); [p[0], y, p[1]] }
    pub fn uv(&self, p: P2) -> (f64, f64) { if self.along_x { (p[0], p[1]) } else { (p[1], p[0]) } }
    pub fn span(&self) -> f64 { self.v[1] - self.v[0] }
    /// Height (absolute) of the rafter underside over plan point `p`; flat roofs: the top of the
    /// upstand. Walls under the roof rise to it, never through it.
    pub fn under(&self, p: P2) -> f64 {
        let (u, v) = self.uv(p);
        let (a, b, c, d) = (v - self.v[0], self.v[1] - v, u - self.u[0], self.u[1] - u);
        match self.style {
            RoofStyle::Gable => self.y0 + self.k * a.min(b),
            RoofStyle::Shed => self.y0 + self.k * a,
            RoofStyle::Hip => self.y0 + self.k * a.min(b).min(c).min(d),
            RoofStyle::Flat => self.wall_top + self.flat.total() + self.flat.upstand,
        }
    }
    /// The edges the roof bears on with a plate.
    pub fn plates(&self) -> impl Iterator<Item = &RoofEdge> { self.edges.iter().filter(|e| e.plate.is_some()) }
}

/// Plumb depth of the birdsmouth seat: the full plate width, shortened so the notch (square to the
/// rafter) never takes more than a third of its depth. Returns the seat length.
pub fn birdsmouth(plate: f64, rd: f64, k: f64) -> f64 {
    if k <= 1e-9 { return plate; }
    plate.min(rd / 3.0 * (1.0 + k * k).sqrt() / k)
}

fn ext_thickness(s: &Settings) -> f64 { if s.cavity.enabled { s.cavity.total() } else { s.wall_thickness } }
fn wall_t(s: &Settings, w: &WallEntry) -> f64 {
    if w.exterior && w.thickness.is_none() && s.cavity.enabled { s.cavity.total() } else { w.thickness.unwrap_or(s.wall_thickness) }
}

/// Every enabled roof of the model.
pub fn geoms(m: &Model, set: &WallSet) -> Vec<RoofGeom> {
    m.roofs.iter().filter(|(_, r)| r.enabled).filter_map(|(c, r)| geom(m, set, &c.id, r)).collect()
}

pub fn geom(m: &Model, set: &WallSet, id: &str, r: &Roof) -> Option<RoofGeom> {
    if !r.enabled { return None; }
    let s = &m.settings;
    let count = s.count();
    let top = r.levels.unwrap_or(count).clamp(1, count) - 1;
    let te = ext_thickness(s);
    let (mut lo, mut hi) = match (r.min, r.max) {
        (Some(a), Some(c)) => ([a[0].min(c[0]), a[1].min(c[1])], [a[0].max(c[0]), a[1].max(c[1])]),
        // the rooms' outline runs on the wall centre lines: out to the outer faces
        _ => { let (a, c) = footprint(m, top)?; ([a[0] - te / 2.0, a[1] - te / 2.0], [c[0] + te / 2.0, c[1] + te / 2.0]) }
    };
    // each side lands on the outer face of the exterior wall under it
    for axis in 0..2 {
        let other = 1 - axis;
        for side in 0..2 {
            let cur = if side == 0 { lo[axis] } else { hi[axis] };
            let outward = if side == 0 { -1.0 } else { 1.0 };
            let (olo, ohi) = (lo[other], hi[other]);
            let mut best: Option<(f64, f64)> = None;
            for w in set.walls.iter().filter(|w| w.level == top && w.exterior) {
                let l = w.length(); if l < 0.3 { continue; }
                if ((w.b[axis] - w.a[axis]) / l).abs() > 0.05 || w.out[axis] * outward < 0.9 { continue; }
                let face = w.a[axis] + w.out[axis] * wall_t(s, w) / 2.0;
                let (wa, wb) = (w.a[other].min(w.b[other]), w.a[other].max(w.b[other]));
                if wb < olo + 0.1 || wa > ohi - 0.1 { continue; }
                let dd = (face - cur).abs();
                if dd < 0.35 && best.map_or(true, |b| dd < b.0) { best = Some((dd, face)); }
            }
            if let Some((_, f)) = best { if side == 0 { lo[axis] = f } else { hi[axis] = f } }
        }
    }
    // a hip roof runs its ridge along the longer side
    let along_x = if r.style == RoofStyle::Hip { hi[0] - lo[0] >= hi[1] - lo[1] } else { r.ridge_along_x };
    let (u, v) = if along_x { ([lo[0], hi[0]], [lo[1], hi[1]]) } else { ([lo[1], hi[1]], [lo[0], hi[0]]) };
    if u[1] - u[0] < 0.3 || v[1] - v[0] < 0.3 { return None; }
    // the walls' top: the storey's height, or the rooms' own wall height when every room under the
    // roof has one (a garage lower than the house)
    let wall_top = {
        let inside: Vec<&Room> = m.rooms.iter().map(|(_, rm)| rm).filter(|rm| rm.level == top && { let c = rm.centroid(); c[0] > lo[0] && c[0] < hi[0] && c[1] > lo[1] && c[1] < hi[1] }).collect();
        let own: Vec<f64> = inside.iter().filter_map(|rm| rm.wall_height).collect();
        s.base(top) + if !inside.is_empty() && own.len() == inside.len() { own.iter().cloned().fold(0.0, f64::max) } else { s.height(top) }
    };
    let (d0, pw, cavity) = if s.cavity.enabled { (s.cavity.outer_leaf, s.cavity.inner_leaf + s.cavity.cavity, s.cavity.cavity) } else { (0.0, s.wall_thickness, 0.0) };
    let ph = r.plate_height.max(0.03);
    let ov = r.overhang.max(0.0);
    let vo = r.verge_overhang.unwrap_or(ov).max(0.0);
    let ov_verge = [if r.party.iter().any(|p| p == "min") { 0.0 } else { vo }, if r.party.iter().any(|p| p == "max") { 0.0 } else { vo }];
    let span = v[1] - v[0];
    let run = match r.style { RoofStyle::Shed => span, _ => span / 2.0 };
    let k = if r.style == RoofStyle::Flat { 0.0 } else { (r.pitch_height.max(0.0) / run).max(0.02) };
    let rd = r.rafter_size[1].max(0.05);
    let seat = birdsmouth(pw, rd, k);
    let y0 = wall_top + ph - k * (d0 + seat);
    let jd = (span / 20.0).clamp(rd, 0.3);
    let flat = FlatBuild { joist: jd, deck: 0.018, insulation: 0.12, membrane: 0.005, upstand: 0.08 };
    let mut g = RoofGeom { id: id.to_string(), style: r.style, level: top, along_x, u, v, ov, ov_verge, wall_top, k, y0, d0, pw, ph, seat, cavity, flat, zones: vec![], edges: vec![] };
    // edges: v0, v1, u0, u1
    let plate = |bottom: f64| Some(Plate { d: [d0, d0 + pw], bottom, height: ph });
    let dirp = |du: f64, dv: f64| -> P2 { if along_x { [du, dv] } else { [dv, du] } };
    let high_bottom = y0 + k * (span - d0 - pw + seat) - ph;
    let kinds: [(EdgeKind, Option<Plate>); 4] = match r.style {
        RoofStyle::Gable => [(EdgeKind::Eave, plate(wall_top)), (EdgeKind::Eave, plate(wall_top)), (EdgeKind::Gable, None), (EdgeKind::Gable, None)],
        RoofStyle::Shed => [(EdgeKind::Eave, plate(wall_top)), (EdgeKind::Eave, plate(high_bottom)), (EdgeKind::Gable, None), (EdgeKind::Gable, None)],
        RoofStyle::Hip => [(EdgeKind::Eave, plate(wall_top)), (EdgeKind::Eave, plate(wall_top)), (EdgeKind::Eave, plate(wall_top)), (EdgeKind::Eave, plate(wall_top))],
        RoofStyle::Flat => [(EdgeKind::Flat, None), (EdgeKind::Flat, None), (EdgeKind::Flat, None), (EdgeKind::Flat, None)],
    };
    let lines = [((u[0], v[0]), (u[1], v[0]), dirp(0.0, 1.0)), ((u[0], v[1]), (u[1], v[1]), dirp(0.0, -1.0)),
                 ((u[0], v[0]), (u[0], v[1]), dirp(1.0, 0.0)), ((u[1], v[0]), (u[1], v[1]), dirp(-1.0, 0.0))];
    for (i, (a, b, inward)) in lines.into_iter().enumerate() {
        g.edges.push(RoofEdge { a: g.plan(a.0, a.1), b: g.plan(b.0, b.1), inward, kind: kinds[i].0, plate: kinds[i].1 });
    }
    // taller parts of the building (rooms of the storeys above) stop the roof
    let (cu, cv) = ((u[0] + u[1]) / 2.0, (v[0] + v[1]) / 2.0);
    for (_, room) in m.rooms.iter().filter(|(_, rm)| rm.level > top) {
        let Some((a, c)) = bbox(&room.points) else { continue };
        let (a, c) = ([a[0] - te / 2.0, a[1] - te / 2.0], [c[0] + te / 2.0, c[1] + te / 2.0]);
        let (zu, zv) = if along_x { ([a[0], c[0]], [a[1], c[1]]) } else { ([a[1], c[1]], [a[0], c[0]]) };
        if zu[1] <= u[0] - ov_verge[0] || zu[0] >= u[1] + ov_verge[1] || zv[1] <= v[0] - ov || zv[0] >= v[1] + ov { continue; }
        if cu > zu[0] && cu < zu[1] && cv > zv[0] && cv < zv[1] { continue; }
        g.zones.push([zu, zv]);
    }
    Some(g)
}

// ---- building -------------------------------------------------------------------------------------

/// A slope's frame in (u, v): origin at the corner, `x` along the eaves, `r` inward.
#[derive(Debug, Clone, Copy)]
struct Frame { o: [f64; 2], x: [f64; 2], r: [f64; 2], len: f64 }
impl Frame {
    fn uv(&self, x: f64, r: f64) -> [f64; 2] { [self.o[0] + x * self.x[0] + r * self.r[0], self.o[1] + x * self.x[1] + r * self.r[1]] }
    fn xr(&self, uv: [f64; 2]) -> (f64, f64) {
        let d = [uv[0] - self.o[0], uv[1] - self.o[1]];
        (d[0] * self.x[0] + d[1] * self.x[1], d[0] * self.r[0] + d[1] * self.r[1])
    }
}

/// Cross-section heights of a member over the run `r`: following the slope (offsets above the rafter
/// underside line) or level (absolute).
#[derive(Debug, Clone, Copy)]
enum Ys { Slope(f64, f64), Level(f64, f64) }

type Rect = [[f64; 2]; 2];

/// `rect` minus the zones, as rectangles.
fn free_rects(rect: Rect, zones: &[Rect]) -> Vec<Rect> {
    let mut out = vec![rect];
    for z in zones {
        let mut next = vec![];
        for q in out {
            let iu = [q[0][0].max(z[0][0]), q[0][1].min(z[0][1])];
            let iv = [q[1][0].max(z[1][0]), q[1][1].min(z[1][1])];
            if iu[1] - iu[0] < 1e-6 || iv[1] - iv[0] < 1e-6 { next.push(q); continue; }
            if iu[0] - q[0][0] > 1e-6 { next.push([[q[0][0], iu[0]], q[1]]); }
            if q[0][1] - iu[1] > 1e-6 { next.push([[iu[1], q[0][1]], q[1]]); }
            if iv[0] - q[1][0] > 1e-6 { next.push([iu, [q[1][0], iv[0]]]); }
            if q[1][1] - iv[1] > 1e-6 { next.push([iu, [iv[1], q[1][1]]]); }
        }
        out = next;
    }
    out
}

fn dedupe(pts: Vec<P3>) -> Vec<P3> {
    let mut out: Vec<P3> = vec![];
    for p in pts { if out.last().map_or(true, |q| plannr_core::geom::dist(*q, p) > 1e-7) { out.push(p); } }
    while out.len() > 1 && plannr_core::geom::dist(out[0], *out.last().unwrap()) < 1e-7 { out.pop(); }
    out
}

fn mm(x: f64) -> f64 { (x * 1000.0).round() }

struct Builder<'a> { f: &'a mut Faces, g: &'a RoofGeom, items: Vec<Item> }

impl<'a> Builder<'a> {
    fn p3(&self, fr: &Frame, x: f64, r: f64, y: f64) -> P3 { let uv = fr.uv(x, r); self.g.at(uv[0], uv[1], y) }
    fn ys(&self, ys: Ys, r: f64) -> (f64, f64) {
        match ys { Ys::Slope(a, b) => (self.g.y0 + self.g.k * r + a, self.g.y0 + self.g.k * r + b), Ys::Level(a, b) => (a, b) }
    }
    /// The free pieces (x range, r range) of a rectangle of a slope.
    fn free(&self, fr: &Frame, x: [f64; 2], r: [f64; 2], clip: bool) -> Vec<([f64; 2], [f64; 2])> {
        if !clip || self.g.zones.is_empty() { return vec![(x, r)]; }
        let (c0, c1) = (fr.uv(x[0], r[0]), fr.uv(x[1], r[1]));
        let rect = [[c0[0].min(c1[0]), c0[0].max(c1[0])], [c0[1].min(c1[1]), c0[1].max(c1[1])]];
        free_rects(rect, &self.g.zones).into_iter().map(|q| {
            let (xa, ra) = fr.xr([q[0][0], q[1][0]]); let (xb, rb) = fr.xr([q[0][1], q[1][1]]);
            ([xa.min(xb), xa.max(xb)], [ra.min(rb), ra.max(rb)])
        }).collect()
    }
    /// A member over x × r of a slope, its section (in the r–y plane) extruded along x. Returns the
    /// pieces built (after the zones).
    fn member(&mut self, id: &str, fr: &Frame, x: [f64; 2], r: [f64; 2], ys: Ys, mat: &str) -> Vec<([f64; 2], [f64; 2])> {
        let mut built = vec![];
        if x[1] - x[0] < 1e-4 || r[1] - r[0] < 1e-4 { return built; }
        for (xx, rr) in self.free(fr, x, r, true) {
            if xx[1] - xx[0] < 1e-3 || rr[1] - rr[0] < 1e-3 { continue; }
            let (a0, a1) = self.ys(ys, rr[0]); let (b0, b1) = self.ys(ys, rr[1]);
            let base = dedupe(vec![self.p3(fr, xx[0], rr[0], a0), self.p3(fr, xx[0], rr[1], b0), self.p3(fr, xx[0], rr[1], b1), self.p3(fr, xx[0], rr[0], a1)]);
            if base.len() < 3 { continue; }
            let dir = sub(self.p3(fr, xx[1], rr[0], a0), self.p3(fr, xx[0], rr[0], a0));
            self.f.prism(id, &base, dir, mat, mat);
            built.push((xx, rr));
        }
        built
    }
    /// A plan polygon (x, r of a slope) from y0 to y1.
    fn slab(&mut self, id: &str, fr: &Frame, pts: &[[f64; 2]], y0: f64, y1: f64, mat: &str) {
        let plan: Vec<P2> = pts.iter().map(|q| { let uv = fr.uv(q[0], q[1]); self.g.plan(uv[0], uv[1]) }).collect();
        let mut plan2: Vec<P2> = vec![];
        for p in plan { if plan2.last().map_or(true, |q: &P2| dist2(*q, p) > 1e-7) { plan2.push(p); } }
        while plan2.len() > 1 && dist2(plan2[0], *plan2.last().unwrap()) < 1e-7 { plan2.pop(); }
        if plan2.len() >= 3 && y1 - y0 > 1e-5 { self.f.slab(id, &plan2, y0, y1, mat); }
    }
    /// A bar along a hip line (inward distance r from both eaves), its section in the vertical plane
    /// through the line, `t` thick.
    fn hip_bar(&mut self, id: &str, corner: [f64; 2], diag: [f64; 2], r: [f64; 2], lo: f64, hi: f64, t: f64, mat: &str) {
        let g = self.g;
        let pt = |rr: f64, off: f64| { let uv = [corner[0] + diag[0] * rr, corner[1] + diag[1] * rr]; g.at(uv[0], uv[1], g.y0 + g.k * rr + off) };
        let a = g.at(corner[0], corner[1], 0.0); let b = g.at(corner[0] + diag[0], corner[1] + diag[1], 0.0);
        let along = normalize(sub(b, a));
        let perp = normalize(cross(along, [0.0, 1.0, 0.0]));
        let shift = scale(perp, -t / 2.0);
        let base = vec![add(pt(r[0], lo), shift), add(pt(r[1], lo), shift), add(pt(r[1], hi), shift), add(pt(r[0], hi), shift)];
        self.f.prism(id, &base, scale(perp, t), mat, mat);
    }
    fn timber(&mut self, key: &str, name: &str, size: [f64; 2], count: usize, length: f64) {
        if length <= 1e-6 { return; }
        self.items.push(Item::new(key, "timber", format!("{name} {:.0}×{:.0}", mm(size[0]), mm(size[1]))).count(count as f64).length(length).volume(length * size[0] * size[1]));
    }
}

fn covering_names(cover: &str) -> (&'static str, &'static str, f64, f64) {
    // material key, takeoff name, thickness, module width (for the joints drawn on it)
    match cover { "slates" => ("slate", "Leien", 0.015, 0.25), "metal" => ("steel", "Metalen dakbedekking (felsnaad)", 0.008, 0.5), _ => ("tile", "Dakpannen", 0.04, 0.3) }
}

/// Build roof `id` (faces into `b`) and return its takeoff.
pub fn build(b: &mut Faces, g: &RoofGeom, r: &Roof, id: &str) -> Vec<Item> {
    let mut bl = Builder { f: b, g, items: vec![] };
    match g.style {
        RoofStyle::Gable | RoofStyle::Shed => pitched(&mut bl, r, id),
        RoofStyle::Hip => hip(&mut bl, r, id),
        RoofStyle::Flat => flat(&mut bl, r, id),
    }
    bl.items
}

fn frames(g: &RoofGeom) -> [Frame; 4] {
    let (lu, lv) = (g.u[1] - g.u[0], g.v[1] - g.v[0]);
    [Frame { o: [g.u[0], g.v[0]], x: [1.0, 0.0], r: [0.0, 1.0], len: lu }, Frame { o: [g.u[0], g.v[1]], x: [1.0, 0.0], r: [0.0, -1.0], len: lu },
     Frame { o: [g.u[0], g.v[0]], x: [0.0, 1.0], r: [1.0, 0.0], len: lv }, Frame { o: [g.u[1], g.v[0]], x: [0.0, 1.0], r: [-1.0, 0.0], len: lv }]
}

struct Mats { timber: String, cover: String, fascia: String, cover_name: &'static str, cover_t: f64, module: f64 }
fn mats(r: &Roof, flat: bool) -> Mats {
    let (key, name, t, module) = if flat { ("roofing", "Dakdichting (EPDM / bitumen)", 0.005, 0.0) } else { covering_names(&r.covering) };
    Mats {
        timber: material_id("timber"),
        cover: finish_of(&r.finishes, "roof").map(|f| material_id(&f)).unwrap_or_else(|| material_id(key)),
        fascia: finish_of(&r.finishes, "fascia").map(|f| material_id(&f)).unwrap_or_else(|| material_id("fascia")),
        cover_name: name, cover_t: t, module,
    }
}

/// Batten positions (inward distance r of each batten's lower edge) from the eave end `r0` up to `r1`.
fn battens(r0: f64, r1: f64, ru: f64, gauge: f64, bw: f64) -> Vec<f64> {
    let len = (r1 - r0) * ru;
    let gauge = gauge.max(0.1);
    let mut out = vec![]; let mut s = 0.03;
    while s + bw < len - 0.02 { out.push(r0 + s / ru); s += gauge; }
    let last = len - bw - 0.02;
    if out.last().map_or(true, |q| (last - (q - r0) * ru) > 0.08) && last > 0.0 { out.push(r0 + last / ru); }
    out
}

fn pitched(bl: &mut Builder, r: &Roof, id: &str) {
    let g = bl.g;
    let ms = mats(r, false);
    let (k, y0) = (g.k, g.y0);
    let ru = (1.0 + k * k).sqrt();
    let shed = g.style == RoofStyle::Shed;
    let span = g.span();
    let ridge = if shed { span } else { span / 2.0 };
    let fr = frames(g);
    let slopes: Vec<Frame> = if shed { vec![fr[0]] } else { vec![fr[0], fr[1]] };
    let len = fr[0].len;
    let (ov, ovv) = (g.ov, g.ov_verge);
    let [rt, rd] = [r.rafter_size[0].max(0.02), r.rafter_size[1].max(0.05)];
    let dv_r = rd * ru;
    let (cbt, cbw, bt, bw) = (0.018, 0.038, 0.025, 0.038);
    let (dv_cb, dv_b, dv_t) = (cbt * ru, bt * ru, ms.cover_t * ru);
    let off_b = dv_r + dv_cb;
    let off_c = off_b + dv_b;
    let xe = [-ovv[0], len + ovv[1]];
    // the covering laps 3 cm past a verge; at a party wall it stops on the wall line
    let lap = [if ovv[0] > 0.03 { 0.03 } else { 0.0 }, if ovv[1] > 0.03 { 0.03 } else { 0.0 }];
    let r_top = if shed { span + ov } else { ridge };
    let (tid, cid, fid) = (format!("{id}#timber"), format!("{id}#roof"), format!("{id}#fascia"));
    let (d0, pw, ph) = (g.d0, g.pw, g.ph);
    // ---- wall plates
    let mut plate_len = 0.0; let mut plates = 0;
    for (e, edge) in g.edges.iter().take(2).enumerate() {
        let Some(p) = edge.plate else { continue };
        let (frm, rr) = if shed { (fr[0], if e == 0 { [d0, d0 + pw] } else { [span - d0 - pw, span - d0] }) } else { (fr[e], [d0, d0 + pw]) };
        for (xx, _) in bl.member(&tid, &frm, [d0, len - d0], rr, Ys::Level(p.bottom, p.bottom + p.height), &ms.timber) { plate_len += xx[1] - xx[0]; }
        plates += 1;
    }
    bl.timber("plate", "Muurplaat", [pw, ph], plates, plate_len);
    // ---- purlins bearing in the gables (through them when the verge overhangs) and the ridge purlin
    let [pwid, pd] = [r.purlin_size[0].max(0.03), r.purlin_size[1].max(0.08)];
    let px = [if ovv[0] > 0.02 { xe[0] } else { d0 }, if ovv[1] > 0.02 { xe[1] } else { len - d0 }];
    let (r_lo, r_hi) = (d0 + pw / 2.0, if shed { span - d0 - pw / 2.0 } else { ridge });
    let n = (((r_hi - r_lo) * ru / r.purlin_span.max(0.8)).ceil() as usize).saturating_sub(1);
    let mut purlin_len = 0.0; let mut purlins = 0;
    for frm in &slopes {
        for i in 1..=n {
            let rc = r_lo + (r_hi - r_lo) * i as f64 / (n + 1) as f64;
            let topy = y0 + k * rc;
            for (xx, _) in bl.member(&tid, frm, px, [rc - pwid / 2.0, rc + pwid / 2.0], Ys::Level(topy - pd, topy), &ms.timber) { purlin_len += xx[1] - xx[0]; }
            purlins += 1;
        }
    }
    bl.timber("purlin", "Gording", [pwid, pd], purlins, purlin_len);
    if !shed {
        let topy = y0 + k * ridge;
        let mut l = 0.0;
        for (xx, _) in bl.member(&tid, &fr[0], px, [ridge - pwid / 2.0, ridge + pwid / 2.0], Ys::Level(topy - pd, topy), &ms.timber) { l += xx[1] - xx[0]; }
        bl.timber("ridge", "Nokgording", [pwid, pd], 1, l);
    }
    // ---- rafters with their counter battens
    let (xa, xb) = (xe[0] + rt / 2.0, xe[1] - rt / 2.0);
    let nr = (((xb - xa) / r.rafter_spacing.max(0.2)).ceil() as usize).max(1);
    let (mut raf_len, mut rafters, mut cb_len) = (0.0, 0, 0.0);
    for frm in &slopes {
        for i in 0..=nr {
            let x = xa + (xb - xa) * i as f64 / nr as f64;
            let built = bl.member(&tid, frm, [x - rt / 2.0, x + rt / 2.0], [-ov, r_top], Ys::Slope(0.0, dv_r), &ms.timber);
            if !built.is_empty() { rafters += 1; }
            for (_, rr) in built { raf_len += (rr[1] - rr[0]) * ru; }
            for (_, rr) in bl.member(&tid, frm, [x - cbw / 2.0, x + cbw / 2.0], [-ov, r_top], Ys::Slope(dv_r, dv_r + dv_cb), &ms.timber) { cb_len += (rr[1] - rr[0]) * ru; }
        }
    }
    bl.timber("rafter", "Keper", [rt, rd], rafters, raf_len);
    bl.timber("counterBatten", "Tengel", [cbt, cbw], 0, cb_len);
    // ---- battens and the covering
    let rows = battens(-ov, r_top, ru, r.batten_gauge, bw);
    let mut bat_len = 0.0; let mut cover_area = 0.0;
    for frm in &slopes {
        for &rb in &rows {
            for (xx, _) in bl.member(&tid, frm, xe, [rb, rb + bw / ru], Ys::Slope(off_b, off_b + dv_b), &ms.timber) { bat_len += xx[1] - xx[0]; }
        }
        let cx = [xe[0] - lap[0], xe[1] + lap[1]];
        let c_lo = -ov - 0.05;
        for (xx, rr) in bl.member(&cid, frm, cx, [c_lo, r_top], Ys::Slope(off_c, off_c + dv_t), &ms.cover) {
            cover_area += (xx[1] - xx[0]) * (rr[1] - rr[0]) * ru;
            // courses along the battens and the joints between the tiles, just proud of the covering
            let lift = off_c + dv_t + 0.002;
            for &rb in rows.iter().filter(|q| **q > rr[0] + 0.01 && **q < rr[1] - 0.01) {
                let (a, b) = (bl.p3(frm, xx[0], rb, y0 + k * rb + lift), bl.p3(frm, xx[1], rb, y0 + k * rb + lift));
                edge(bl.f, &cid, a, b);
            }
            if ms.module > 0.0 {
                let mut x = cx[0] + ms.module;
                while x < cx[1] - 0.01 {
                    if x > xx[0] + 0.01 && x < xx[1] - 0.01 {
                        let (a, b) = (bl.p3(frm, x, rr[0], y0 + k * rr[0] + lift), bl.p3(frm, x, rr[1], y0 + k * rr[1] + lift));
                        edge(bl.f, &cid, a, b);
                    }
                    x += ms.module;
                }
            }
        }
    }
    bl.timber("batten", "Panlat", [bt, bw], 0, bat_len);
    if cover_area > 0.0 {
        bl.items.push(Item::new("covering", "roofing", ms.cover_name).area(cover_area));
        bl.items.push(Item::new("underlay", "roofing", "Onderdakfolie (dampopen)").area(cover_area));
    }
    // ridge cap
    if !shed {
        let apex = y0 + k * ridge + off_c + dv_t;
        bl.member(&cid, &fr[0], [xe[0] - lap[0], xe[1] + lap[1]], [ridge - 0.12, ridge + 0.12], Ys::Level(apex - 0.03, apex + 0.07), &ms.cover);
        bl.items.push(Item::new("ridgeCap", "roofing", "Nokpannen").length(xe[1] - xe[0] + lap[0] + lap[1]));
    }
    // ---- eaves: fascia and soffit; verges: boards along the slope
    let mut fascia_len = 0.0; let mut soffit_area = 0.0;
    if ov > 0.03 {
        for frm in &slopes {
            let ye = y0 - k * ov;
            for (xx, _) in bl.member(&fid, frm, xe, [-ov - 0.022, -ov], Ys::Level(ye, ye + dv_r + dv_cb + dv_b), &ms.fascia) { fascia_len += xx[1] - xx[0]; }
            for (xx, rr) in bl.member(&fid, frm, xe, [-ov, 0.0], Ys::Level(ye - 0.012, ye), &ms.fascia) { soffit_area += (xx[1] - xx[0]) * (rr[1] - rr[0]); }
        }
        if shed {
            let yh = y0 + k * (span + ov);
            let yw = y0 + k * (span - d0);
            for (xx, _) in bl.member(&fid, &fr[0], xe, [span + ov, span + ov + 0.022], Ys::Level(yh, yh + dv_r + dv_cb + dv_b), &ms.fascia) { fascia_len += xx[1] - xx[0]; }
            for (xx, rr) in bl.member(&fid, &fr[0], xe, [span, span + ov], Ys::Level(yw - 0.012, yw), &ms.fascia) { soffit_area += (xx[1] - xx[0]) * (rr[1] - rr[0]); }
        }
    }
    for (end, xr) in [[xe[0] - 0.022, xe[0]], [xe[1], xe[1] + 0.022]].into_iter().enumerate() {
        if ovv[end] <= 0.03 { continue; }
        for frm in &slopes {
            {
                for (_, rr) in bl.member(&fid, frm, xr, [-ov, r_top], Ys::Slope(-0.03, off_c + dv_t * 0.6), &ms.fascia) { fascia_len += (rr[1] - rr[0]) * ru; }
            }
        }
    }
    if fascia_len > 0.0 { bl.items.push(Item::new("fascia", "fascia", "Boeiboord / windveer").length(fascia_len)); }
    if soffit_area > 0.0 { bl.items.push(Item::new("soffit", "fascia", "Onderplaat (overstek)").area(soffit_area)); }
}

fn hip(bl: &mut Builder, r: &Roof, id: &str) {
    let g = bl.g;
    let ms = mats(r, false);
    let (k, y0) = (g.k, g.y0);
    let ru = (1.0 + k * k).sqrt();
    let half = g.span() / 2.0;
    let fr = frames(g);
    let ov = g.ov;
    let [rt, rd] = [r.rafter_size[0].max(0.02), r.rafter_size[1].max(0.05)];
    let dv_r = rd * ru;
    let (cbt, cbw, bt, bw) = (0.018, 0.038, 0.025, 0.038);
    let (dv_cb, dv_b, dv_t) = (cbt * ru, bt * ru, ms.cover_t * ru);
    let (off_b, off_c) = (dv_r + dv_cb, dv_r + dv_cb + dv_b);
    let (tid, cid, fid) = (format!("{id}#timber"), format!("{id}#roof"), format!("{id}#fascia"));
    let (d0, pw, ph) = (g.d0, g.pw, g.ph);
    let ht = rt + 0.012;
    let hoff = ht / 2.0 * std::f64::consts::SQRT_2;
    // plates all round, mitred at the corners
    let mut plate_len = 0.0;
    for f in &fr {
        let (a, c) = (d0, d0 + pw);
        bl.slab(&tid, f, &[[a, a], [f.len - a, a], [f.len - c, c], [c, c]], g.wall_top, g.wall_top + ph, &ms.timber);
        plate_len += f.len - 2.0 * d0;
    }
    bl.timber("plate", "Muurplaat", [pw, ph], 4, plate_len);
    // ridge beam
    let [pwid, pd] = [r.purlin_size[0].max(0.03), r.purlin_size[1].max(0.08)];
    let ridge_len = fr[0].len - 2.0 * half;
    if ridge_len > 0.02 {
        let topy = y0 + k * half;
        bl.member(&tid, &fr[0], [half, fr[0].len - half], [half - pwid / 2.0, half + pwid / 2.0], Ys::Level(topy - pd, topy), &ms.timber);
        bl.timber("ridge", "Nokbalk", [pwid, pd], 1, ridge_len);
    }
    // hip rafters from the overhang corners to the ridge ends
    let corners = [([g.u[0], g.v[0]], [1.0, 1.0]), ([g.u[1], g.v[0]], [-1.0, 1.0]), ([g.u[1], g.v[1]], [-1.0, -1.0]), ([g.u[0], g.v[1]], [1.0, -1.0])];
    for (c, dgl) in corners {
        bl.hip_bar(&tid, c, dgl, [-ov, half], dv_r - (dv_r + 0.05), dv_r, ht, &ms.timber);
    }
    bl.timber("hip", "Hoekkeper", [ht, rd + 0.05], 4, 4.0 * (half + ov) * (2.0 + k * k).sqrt());
    // jack and common rafters, counter battens, battens, covering, fascia and soffit per slope
    let (mut raf_len, mut rafters, mut cb_len, mut bat_len, mut cover_area, mut fascia_len, mut soffit_area) = (0.0, 0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let rows = battens(-ov, half, ru, r.batten_gauge, bw);
    for f in &fr {
        let len = f.len;
        let (xa, xb) = (-ov + rt / 2.0, len + ov - rt / 2.0);
        let nr = (((xb - xa) / r.rafter_spacing.max(0.2)).ceil() as usize).max(1);
        for i in 0..=nr {
            let x = xa + (xb - xa) * i as f64 / nr as f64;
            let rmax = half.min(x - hoff - rt / 2.0).min(len - x - hoff - rt / 2.0);
            if rmax < -ov + 0.05 { continue; }
            bl.member(&tid, f, [x - rt / 2.0, x + rt / 2.0], [-ov, rmax], Ys::Slope(0.0, dv_r), &ms.timber);
            bl.member(&tid, f, [x - cbw / 2.0, x + cbw / 2.0], [-ov, rmax], Ys::Slope(dv_r, dv_r + dv_cb), &ms.timber);
            raf_len += (rmax + ov) * ru; cb_len += (rmax + ov) * ru; rafters += 1;
        }
        for &rb in &rows {
            let (x0, x1) = (rb, len - rb);
            if x1 - x0 < 0.05 { continue; }
            bl.member(&tid, f, [x0, x1], [rb, rb + bw / ru], Ys::Slope(off_b, off_b + dv_b), &ms.timber);
            bat_len += x1 - x0;
        }
        // covering: the slope's trapezoid (a triangle on the hip ends) on the battens
        let c_lo = -ov - 0.05;
        let pts = [[c_lo, c_lo], [len - c_lo, c_lo], [len - half, half], [half, half]];
        let base: Vec<P3> = dedupe(pts.iter().map(|q| bl.p3(f, q[0], q[1], y0 + k * q[1] + off_c)).collect());
        if base.len() >= 3 { bl.f.prism(&cid, &base, [0.0, dv_t, 0.0], &ms.cover, &ms.cover); }
        cover_area += ((len - 2.0 * c_lo) + (len - 2.0 * half).max(0.0)) / 2.0 * (half - c_lo) * ru;
        let lift = off_c + dv_t + 0.002;
        for &rb in &rows {
            if len - 2.0 * rb < 0.05 { continue; }
            let (a, b) = (bl.p3(f, rb, rb, y0 + k * rb + lift), bl.p3(f, len - rb, rb, y0 + k * rb + lift));
            edge(bl.f, &cid, a, b);
        }
        // fascia and soffit, mitred at the corners
        if ov > 0.03 {
            let ye = y0 - k * ov;
            let (o1, o2) = (-ov - 0.022, -ov);
            bl.slab(&fid, f, &[[o1, o1], [len - o1, o1], [len - o2, o2], [o2, o2]], ye, ye + dv_r + dv_cb + dv_b, &ms.fascia);
            bl.slab(&fid, f, &[[-ov, -ov], [len + ov, -ov], [len, 0.0], [0.0, 0.0]], ye - 0.012, ye, &ms.fascia);
            fascia_len += len + 2.0 * ov; soffit_area += (len + ov) * ov;
        }
    }
    // ridge and hip caps
    let apex_off = off_c + dv_t;
    if ridge_len > 0.02 {
        let apex = y0 + k * half + apex_off;
        bl.member(&cid, &fr[0], [half, fr[0].len - half], [half - 0.12, half + 0.12], Ys::Level(apex - 0.03, apex + 0.07), &ms.cover);
    }
    for (c, dgl) in corners { bl.hip_bar(&cid, c, dgl, [-ov - 0.05, half], apex_off - 0.03, apex_off + 0.07, 0.22, &ms.cover); }
    bl.timber("rafter", "Keper", [rt, rd], rafters, raf_len);
    bl.timber("counterBatten", "Tengel", [cbt, cbw], 0, cb_len);
    bl.timber("batten", "Panlat", [bt, bw], 0, bat_len);
    if cover_area > 0.0 {
        bl.items.push(Item::new("covering", "roofing", ms.cover_name).area(cover_area));
        bl.items.push(Item::new("underlay", "roofing", "Onderdakfolie (dampopen)").area(cover_area));
        bl.items.push(Item::new("ridgeCap", "roofing", "Nok- en hoekpannen").length(ridge_len.max(0.0) + 4.0 * (half + ov) * (2.0 + k * k).sqrt()));
    }
    if fascia_len > 0.0 { bl.items.push(Item::new("fascia", "fascia", "Boeiboord").length(fascia_len)); }
    if soffit_area > 0.0 { bl.items.push(Item::new("soffit", "fascia", "Onderplaat (overstek)").area(soffit_area)); }
}

fn flat(bl: &mut Builder, r: &Roof, id: &str) {
    let g = bl.g;
    let ms = mats(r, true);
    let fr = frames(g);
    let (d0, wt) = (g.d0, g.wall_top);
    let fb = g.flat;
    let (tid, cid, fid) = (format!("{id}#timber"), format!("{id}#roof"), format!("{id}#fascia"));
    let span = g.span();
    let rt = r.rafter_size[0].max(0.03);
    // joists across the short way, bearing on the inner leaves
    let len = fr[0].len;
    let (xa, xb) = (d0 + rt / 2.0, len - d0 - rt / 2.0);
    let n = (((xb - xa) / r.rafter_spacing.max(0.2)).ceil() as usize).max(1);
    let mut jl = 0.0;
    for i in 0..=n {
        let x = xa + (xb - xa) * i as f64 / n as f64;
        for (_, rr) in bl.member(&tid, &fr[0], [x - rt / 2.0, x + rt / 2.0], [d0, span - d0], Ys::Level(wt, wt + fb.joist), &ms.timber) { jl += rr[1] - rr[0]; }
    }
    bl.timber("joist", "Dakbalk", [rt, fb.joist], n + 1, jl);
    // deck, insulation and membrane inside the outer leaves
    let inner = [[d0, d0], [len - d0, d0], [len - d0, span - d0], [d0, span - d0]];
    let mut y = wt + fb.joist;
    bl.slab(&tid, &fr[0], &inner, y, y + fb.deck, &ms.timber); y += fb.deck;
    bl.slab(&cid, &fr[0], &inner, y, y + fb.insulation, &crate::masonry::data::material_id("insulation")); y += fb.insulation;
    bl.slab(&cid, &fr[0], &inner, y, y + fb.membrane, &ms.cover);
    let area = (len - 2.0 * d0) * (span - 2.0 * d0);
    bl.items.push(Item::new("deck", "roofing", "Dakbeschot OSB 18 mm").area(area));
    bl.items.push(Item::new("insulation", "insulation", "Dakisolatie PIR 120 mm").area(area).volume(area * fb.insulation));
    bl.items.push(Item::new("covering", "roofing", ms.cover_name).area(area));
    // coping on the upstand (outer leaf and cavity), mitred at the corners
    let top = g.under(g.plan(g.u[0], g.v[0]));
    let rc = d0 + g.cavity;
    let steel = material_id("steel");
    let mut coping = 0.0;
    for f in &fr {
        let l = f.len;
        bl.slab(&fid, f, &[[-0.03, -0.03], [l + 0.03, -0.03], [l - rc, rc], [rc, rc]], top, top + 0.02, &steel);
        coping += l;
    }
    bl.items.push(Item::new("coping", "fascia", "Muurkap aluminium").length(coping));
}

fn edge(f: &mut Faces, id: &str, a: P3, b: P3) { f.edge(id, a, b); }
