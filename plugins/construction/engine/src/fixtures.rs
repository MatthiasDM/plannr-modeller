//! Electrical points as the things they are: a socket is a white plate with a round cup, two pin
//! holes and an earth pin; a switch a plate with rockers; a light a fitting; the board a cabinet.
//! Wall-mounted points sit on the face of the nearest wall (the stored plan position only says
//! which wall and where along it), so they never float in the room or sink into the plaster.

use plannr_core::geom::{add, scale, P3};

use crate::data::*;
use crate::expand::{leaf_span, Faces, WallSet};

/// Where a fixture really sits.
#[derive(Debug, Clone)]
pub struct Mount {
    /// Plan x, height above the storey floor, plan z — on the wall face for wall-mounted points,
    /// at the ceiling for ceiling lights.
    pub pos: P3,
    /// Unit plan normal out of the wall into the space the point faces.
    pub normal: P2,
    /// Unit plan direction along the wall.
    pub along: P2,
    /// Index of the wall piece in the `WallSet` it is mounted on.
    pub wall: Option<usize>,
    /// Distance along that piece (from its `a`).
    pub t: f64,
}

impl FixtureKind {
    pub fn wall_mounted(self) -> bool {
        use FixtureKind::*;
        matches!(self, Outlet | SwitchPlate | WallLight | DataPoint | Panel | Thermostat)
    }
    pub fn is_electrical(self) -> bool { self.system() == System::Electrical }
}

/// The nearest wall face to a plan point on a storey, within `max` metres:
/// (wall index, distance along it, face offset along `out`, side: −1 owner side / +1 far side).
pub fn nearest_wall(s: &Settings, set: &WallSet, level: usize, p: P2, max: f64) -> Option<(usize, f64, f64, f64)> {
    let mut best: Option<(f64, (usize, f64, f64, f64))> = None;
    for (i, w) in set.walls.iter().enumerate() {
        if w.level != level { continue; }
        let l = w.length();
        if l < 1e-6 { continue; }
        let d = [(w.b[0] - w.a[0]) / l, (w.b[1] - w.a[1]) / l];
        let t = (p[0] - w.a[0]) * d[0] + (p[1] - w.a[1]) * d[1];
        if t < -0.1 || t > l + 0.1 { continue; }
        let off = (p[0] - w.a[0]) * w.out[0] + (p[1] - w.a[1]) * w.out[1];
        let (lo, hi) = leaf_span(s, w);
        let (face, side) = if off < (lo + hi) / 2.0 { (lo, -1.0) } else { (hi, 1.0) };
        let overrun = if t < 0.0 { -t } else if t > l { t - l } else { 0.0 };
        let dist = (off - face).abs() + overrun;
        if dist <= max && best.as_ref().map(|b| dist < b.0).unwrap_or(true) { best = Some((dist, (i, t, face, side))); }
    }
    best.map(|b| b.1)
}

/// Half the width of the point's plate along the wall (keeps it on the wall near a corner).
fn half_width(f: &Fixture) -> f64 {
    match f.kind {
        FixtureKind::Panel => 0.2,
        FixtureKind::WallLight => 0.06,
        FixtureKind::Outlet => 0.04 + 0.0355 * (gangs(f) as f64 - 1.0),
        _ => 0.04,
    }
}

pub fn mount(s: &Settings, set: &WallSet, f: &Fixture) -> Mount {
    let p = [f.position[0], f.position[2]];
    if f.kind == FixtureKind::CeilingLight {
        return Mount { pos: [p[0], s.clear_height(f.level, p), p[1]], normal: [0.0, 0.0], along: [1.0, 0.0], wall: None, t: 0.0 };
    }
    if f.kind.wall_mounted() {
        if let Some((i, t, face, side)) = nearest_wall(s, set, f.level, p, 0.45) {
            let w = &set.walls[i];
            let l = w.length();
            let d = [(w.b[0] - w.a[0]) / l, (w.b[1] - w.a[1]) / l];
            let hw = half_width(f).min(l / 2.0);
            let tc = t.clamp(hw, l - hw);
            let q = [w.a[0] + d[0] * tc + w.out[0] * face, w.a[1] + d[1] * tc + w.out[1] * face];
            let normal = [w.out[0] * side, w.out[1] * side];
            return Mount { pos: [q[0], f.position[1], q[1]], normal, along: d, wall: Some(i), t: tc };
        }
    }
    let a = f.rotation.unwrap_or(0.0);
    let normal = [a.sin(), -a.cos()];
    Mount { pos: f.position, normal, along: [-normal[1], normal[0]], wall: None, t: 0.0 }
}

/// How many sockets / switches share the plate: `gangs` in the data, else read from the note
/// ("dubbel", "2-voudig", "3-voudig").
pub fn gangs(f: &Fixture) -> usize {
    if let Some(n) = f.extra.get("gangs").and_then(|v| v.as_u64()) { return (n as usize).clamp(1, 4); }
    let n = f.note.to_lowercase();
    if n.contains("4-voudig") || n.contains("viervoudig") { 4 }
    else if n.contains("3-voudig") || n.contains("drievoudig") || n.contains("driedubbel") { 3 }
    else if n.contains("2-voudig") || n.contains("tweevoudig") || n.contains("dubbel") || n.contains("double") { 2 }
    else { 1 }
}

// ---- local frame -----------------------------------------------------------------------------

/// `o + u·a + v·b + w·c`: u along the wall, v up, w out of the wall.
struct Frame { o: P3, u: P3, v: P3, w: P3 }
impl Frame {
    fn at(&self, a: f64, b: f64, c: f64) -> P3 { add(self.o, add(scale(self.u, a), add(scale(self.v, b), scale(self.w, c)))) }
    fn rect(&self, u0: f64, u1: f64, v0: f64, v1: f64, w: f64) -> Vec<P3> { vec![self.at(u0, v0, w), self.at(u1, v0, w), self.at(u1, v1, w), self.at(u0, v1, w)] }
    fn circle(&self, cu: f64, cv: f64, r: f64, n: usize, w: f64) -> Vec<P3> {
        (0..n).map(|i| { let a = std::f64::consts::TAU * i as f64 / n as f64; self.at(cu + r * a.cos(), cv + r * a.sin(), w) }).collect()
    }
    /// A box from w0 to w1.
    fn block(&self, b: &mut Faces, id: &str, u0: f64, u1: f64, v0: f64, v1: f64, w0: f64, w1: f64, mat: &str) {
        let base = self.rect(u0, u1, v0, v1, w0);
        b.prism(id, &base, scale(self.w, w1 - w0), mat, mat);
    }
    /// A cylinder (n-gon) along w from w0 to w1.
    fn rod(&self, b: &mut Faces, id: &str, cu: f64, cv: f64, r: f64, n: usize, w0: f64, w1: f64, mat: &str) {
        let base = self.circle(cu, cv, r, n, w0);
        b.prism(id, &base, scale(self.w, w1 - w0), mat, mat);
    }
    /// A flat plate (w0..w1) whose front face has round holes, each with a cup `depth` deep.
    #[allow(clippy::too_many_arguments)]
    fn plate_with_cups(&self, b: &mut Faces, id: &str, u0: f64, u1: f64, v0: f64, v1: f64, w0: f64, w1: f64, cups: &[(f64, f64, f64)], depth: f64, mat: &str, cup_mat: &str) {
        const N: usize = 14;
        let front = self.rect(u0, u1, v0, v1, w1);
        let holes: Vec<Vec<P3>> = cups.iter().map(|&(cu, cv, r)| self.circle(cu, cv, r, N, w1)).collect();
        b.poly_holes(id, front, holes, self.w, mat);
        b.poly(id, self.rect(u0, u1, v0, v1, w0), scale(self.w, -1.0), mat);
        let corners = [(u0, v0), (u1, v0), (u1, v1), (u0, v1)];
        let c = self.at((u0 + u1) / 2.0, (v0 + v1) / 2.0, 0.0);
        for i in 0..4 {
            let (a, bb) = (corners[i], corners[(i + 1) % 4]);
            let (p0, p1) = (self.at(a.0, a.1, w0), self.at(bb.0, bb.1, w0));
            let mid = scale(add(p0, p1), 0.5);
            let n = plannr_core::geom::normalize(plannr_core::geom::sub(mid, c));
            b.quad(id, p0, p1, self.at(bb.0, bb.1, w1), self.at(a.0, a.1, w1), n, mat);
        }
        for &(cu, cv, r) in cups {
            let top = self.circle(cu, cv, r, N, w1);
            let bot = self.circle(cu, cv, r, N, w1 - depth);
            let axis = self.at(cu, cv, w1);
            for i in 0..N {
                let j = (i + 1) % N;
                let mid = scale(add(top[i], top[j]), 0.5);
                let inward = plannr_core::geom::normalize(plannr_core::geom::sub(axis, mid));
                b.quad(id, bot[i], bot[j], top[j], top[i], inward, cup_mat);
            }
            b.poly(id, bot, self.w, cup_mat);
        }
    }
}

fn frame(s: &Settings, f: &Fixture, m: &Mount) -> Frame {
    let y0 = s.base(f.level);
    Frame { o: [m.pos[0], y0 + m.pos[1], m.pos[2]], u: [m.along[0], 0.0, m.along[1]], v: [0.0, 1.0, 0.0], w: [m.normal[0], 0.0, m.normal[1]] }
}

const PLATE: &str = "fixtureWhite";
const INSERT: &str = "fixtureInsert";
const DARK: &str = "black";
const METAL: &str = "steel";
const LAMP: &str = "lampGlass";

/// The fixture as a real object. Returns false for kinds still drawn as a marker box.
pub fn build_detailed(b: &mut Faces, s: &Settings, set: &WallSet, id: &str, f: &Fixture) -> bool {
    if !f.kind.is_electrical() && f.kind != FixtureKind::Thermostat { return false; }
    let key = format!("{id}#marker");
    let m = mount(s, set, f);
    let mat = |k: &str| material_id(k);
    let (plate, insert, dark, metal, lamp) = (mat(PLATE), mat(INSERT), mat(DARK), mat(METAL), mat(LAMP));
    const GAP: f64 = 0.0006; // just proud of the wall face
    match f.kind {
        FixtureKind::Outlet => {
            // Belgian/French type E socket: Ø 39 mm cup, two pin holes, a male earth pin on top
            let fr = frame(s, f, &m);
            let g = gangs(f);
            let pitch = 0.071;
            let half = 0.04 + pitch * (g as f64 - 1.0) / 2.0;
            let centres: Vec<f64> = (0..g).map(|i| -pitch * (g as f64 - 1.0) / 2.0 + pitch * i as f64).collect();
            let cups: Vec<(f64, f64, f64)> = centres.iter().map(|&c| (c, 0.0, 0.0195)).collect();
            fr.plate_with_cups(b, &key, -half, half, -0.04, 0.04, GAP, 0.012, &cups, 0.0115, &plate, &insert);
            for &c in &centres {
                for dx in [-0.0095, 0.0095] {
                    let disc = fr.circle(c + dx, 0.0, 0.0025, 10, 0.0012);
                    b.poly(&key, disc, fr.w, &dark);
                }
                fr.rod(b, &key, c, 0.013, 0.0024, 8, 0.0005, 0.0105, &metal);
            }
        }
        FixtureKind::SwitchPlate => {
            let fr = frame(s, f, &m);
            let g = gangs(f);
            fr.block(b, &key, -0.04, 0.04, -0.04, 0.04, GAP, 0.009, &plate);
            // rockers side by side in a 55 mm square
            let wide = 0.055; let gap = 0.002;
            let each = (wide - gap * (g as f64 - 1.0)) / g as f64;
            for i in 0..g {
                let u0 = -wide / 2.0 + i as f64 * (each + gap);
                fr.block(b, &key, u0, u0 + each, -0.0275, 0.0275, 0.009, 0.0135, &insert);
            }
        }
        FixtureKind::DataPoint => {
            let fr = frame(s, f, &m);
            fr.block(b, &key, -0.04, 0.04, -0.04, 0.04, GAP, 0.009, &plate);
            fr.block(b, &key, -0.0225, 0.0225, -0.0225, 0.0225, 0.009, 0.012, &insert);
            for dx in [-0.011, 0.011] { b.poly(&key, fr.rect(dx - 0.0065, dx + 0.0065, -0.006, 0.006, 0.0122), fr.w, &dark); }
        }
        FixtureKind::Thermostat => {
            let fr = frame(s, f, &m);
            fr.block(b, &key, -0.045, 0.045, -0.045, 0.045, GAP, 0.022, &plate);
            b.poly(&key, fr.rect(-0.03, 0.03, -0.005, 0.025, 0.0222), fr.w, &dark);
        }
        FixtureKind::WallLight => {
            let fr = frame(s, f, &m);
            fr.block(b, &key, -0.05, 0.05, -0.05, 0.05, GAP, 0.015, &plate);
            fr.rod(b, &key, 0.0, 0.0, 0.065, 16, 0.015, 0.09, &lamp);
        }
        FixtureKind::Panel => {
            // surface-mounted cabinet 40 × 60 × 12 cm with two rows of breakers behind a window
            let fr = frame(s, f, &m);
            fr.block(b, &key, -0.2, 0.2, -0.3, 0.3, GAP, 0.12, &plate);
            for v in [0.08, -0.12] {
                b.poly(&key, fr.rect(-0.16, 0.16, v - 0.045, v + 0.045, 0.1206), fr.w, &dark);
                b.poly(&key, fr.rect(-0.15, 0.15, v - 0.025, v + 0.025, 0.1212), fr.w, &insert);
            }
        }
        FixtureKind::CeilingLight => {
            // w points down out of the ceiling; u, v span the ceiling plane
            let y0 = s.base(f.level);
            let ceil = m.pos[1];
            let fr = Frame { o: [m.pos[0], y0 + ceil, m.pos[2]], u: [1.0, 0.0, 0.0], v: [0.0, 0.0, 1.0], w: [0.0, -1.0, 0.0] };
            let drop = ceil - f.position[1];
            if drop > 0.25 {
                // pendant: rose, cord, shade
                fr.rod(b, &key, 0.0, 0.0, 0.05, 16, 0.02, 0.045, &plate);
                fr.block(b, &key, -0.002, 0.002, -0.002, 0.002, 0.045, drop - 0.15, &dark);
                shade(b, &key, &fr, drop - 0.15, drop, 0.06, 0.2, &lamp);
            } else {
                // flush ceiling fitting Ø 30 cm, under the 2 cm ceiling finish
                fr.rod(b, &key, 0.0, 0.0, 0.15, 24, 0.02 - GAP, 0.07, &lamp);
                fr.rod(b, &key, 0.0, 0.0, 0.155, 24, 0.02 - GAP, 0.03, &plate);
            }
        }
        _ => return false,
    }
    true
}

/// An open lamp shade: a frustum from radius r0 at w0 to r1 at w1.
#[allow(clippy::too_many_arguments)]
fn shade(b: &mut Faces, id: &str, fr: &Frame, w0: f64, w1: f64, r0: f64, r1: f64, mat: &str) {
    const N: usize = 20;
    let top = fr.circle(0.0, 0.0, r0, N, w0);
    let bot = fr.circle(0.0, 0.0, r1, N, w1);
    let axis = fr.at(0.0, 0.0, (w0 + w1) / 2.0);
    for i in 0..N {
        let j = (i + 1) % N;
        let mid = scale(add(add(top[i], top[j]), add(bot[i], bot[j])), 0.25);
        let out = plannr_core::geom::normalize(plannr_core::geom::sub(mid, axis));
        b.quad(id, top[i], top[j], bot[j], bot[i], out, mat);
    }
    b.poly(id, top, scale(fr.w, -1.0), mat);
}
