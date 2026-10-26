//! Wall junctions, found automatically from the walls of a storey (Medeek-style).
//!
//! A wall's *band* is its framing body in plan: the drawn line a→b plus the offsets (along the
//! exterior normal) of its two faces, which depend on its `reference` (exterior face, centre or
//! interior face of the studs). Where the end of a wall meets another wall:
//!
//! * **L corner** — the end lies in the other wall's band near one of its ends, and that end lies in
//!   this wall's band. One wall runs *through* to the far face of the other, the other *butts*
//!   against its near face. In a chain of walls drawn one after the other the earlier wall (its `b`
//!   end at the corner) runs through; the later one (its `a` end) butts. `startJoin` / `endJoin`
//!   (`through` / `butt` / `none`) override it. The through wall gets the corner detail
//!   (`cornerDetail`: `california` flat stud, `threeStud`, `twoStud` / `none`); the double top plates
//!   interlock (the butting wall's upper top plate laps over the through wall).
//! * **T** — the end lies in the band of another wall away from its ends: this wall butts against the
//!   face on its side, the other wall gets a partition backer (`tDetail`: `twoStud` channel, `ladder`
//!   blocking, `california` flat stud, `none`) and a gap in its upper top plate for the lap.
//!
//! Sheathing laps: at an L the butting wall's exterior sheathing covers the edge of the through
//! wall's sheathing; interior boards stop on the other wall's board.

use crate::data::*;

pub const TOL: f64 = 0.03;

#[derive(Debug, Clone, Copy)]
pub struct Band { pub a: P2, pub u: P2, pub n: P2, pub len: f64, pub lo: f64, pub hi: f64, pub ext: f64, pub int: f64 }
impl Band {
    pub fn of(w: &Wall) -> Band {
        let len = w.length().max(1e-9);
        let u = [(w.b[0] - w.a[0]) / len, (w.b[1] - w.a[1]) / len];
        let n0 = [u[1], -u[0]]; // left of a→b seen from above
        let n = if w.flip { [-n0[0], -n0[1]] } else { n0 };
        let d = w.stud()[1];
        let (lo, hi) = match w.reference.as_str() { "exterior" => (-d, 0.0), "interior" => (0.0, d), _ => (-d / 2.0, d / 2.0) };
        let ext = if w.sheathing.exterior.enabled { w.sheathing.exterior.thickness } else { 0.0 };
        let int = if w.sheathing.interior.enabled { w.sheathing.interior.thickness } else { 0.0 };
        Band { a: w.a, u, n, len, lo, hi, ext, int }
    }
    pub fn centre(&self) -> f64 { (self.lo + self.hi) / 2.0 }
    pub fn depth(&self) -> f64 { self.hi - self.lo }
    pub fn s_of(&self, p: P2) -> f64 { (p[0] - self.a[0]) * self.u[0] + (p[1] - self.a[1]) * self.u[1] }
    pub fn off_of(&self, p: P2) -> f64 { (p[0] - self.a[0]) * self.n[0] + (p[1] - self.a[1]) * self.n[1] }
    pub fn at(&self, s: f64, off: f64) -> P2 { [self.a[0] + self.u[0] * s + self.n[0] * off, self.a[1] + self.u[1] * s + self.n[1] * off] }
    /// Parameter along this band where its line at offset `off` crosses the line of `v` at offset `voff`.
    pub fn cross_param(&self, off: f64, v: &Band, voff: f64) -> Option<f64> {
        let den = self.u[0] * v.n[0] + self.u[1] * v.n[1];
        if den.abs() < 0.15 { return None; }
        let p = self.at(0.0, off);
        Some((voff - v.off_of(p)) / den)
    }
    fn contains(&self, p: P2, slack_s: f64) -> bool {
        let (s, o) = (self.s_of(p), self.off_of(p));
        o >= self.lo - TOL && o <= self.hi + TOL && s >= -slack_s - TOL && s <= self.len + slack_s + TOL
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind { L, T }

/// What happens at one end of a wall.
#[derive(Debug, Clone)]
pub struct EndJoin {
    pub other: String,
    pub kind: Kind,
    pub through: bool,
    /// New end of the framing along the wall (distance from `a`).
    pub frame: f64,
    /// End of the exterior and interior boards.
    pub ext: f64,
    pub int: f64,
    /// End of the upper top plate (laps / is cut back).
    pub upper_plate: f64,
    /// L, through: where the inside face of the butting wall crosses this wall; and which face of
    /// this wall the butting wall sits on (−1 = the `lo` face, +1 = `hi`).
    pub inner: f64,
    pub side: f64,
}

/// A wall butting into the middle of this one.
#[derive(Debug, Clone)]
pub struct Backer { pub other: String, pub centre: f64, pub width: f64, pub side: f64 }

#[derive(Debug, Clone, Default)]
pub struct Joins { pub start: Option<EndJoin>, pub end: Option<EndJoin>, pub backers: Vec<Backer> }

fn parallel(a: &Band, b: &Band) -> bool { (a.u[0] * b.u[1] - a.u[1] * b.u[0]).abs() < 0.17 }

/// How the end (`at_b`) of wall `w` (index `wi`) meets wall `v` (index `vi`): kind and whether `w` runs through.
fn classify(wi: usize, w: &Wall, wb: &Band, at_b: bool, vi: usize, v: &Wall, vb: &Band) -> Option<(Kind, bool, bool)> {
    if w.level != v.level || parallel(wb, vb) { return None; }
    let own = if at_b { w.end_join.as_str() } else { w.start_join.as_str() };
    if own == "none" { return None; }
    let e = if at_b { w.b } else { w.a };
    if !vb.contains(e, wb.depth()) { return None; }
    // L when one of v's ends is also in w's band near this end
    let near_end = |p: P2| wb.contains(p, vb.depth()) && (if at_b { wb.s_of(p) > wb.len - vb.depth() - 2.0 * TOL } else { wb.s_of(p) < vb.depth() + 2.0 * TOL });
    let v_end_b = if near_end(v.b) && (dist(v.b, e) <= dist(v.a, e) || !near_end(v.a)) { Some(true) } else if near_end(v.a) { Some(false) } else { None };
    match v_end_b {
        Some(vb_end) => {
            let theirs = if vb_end { v.end_join.as_str() } else { v.start_join.as_str() };
            if theirs == "none" { return None; }
            let through = match (own, theirs) {
                ("through", _) => true, ("butt", _) => false,
                (_, "through") => false, (_, "butt") => true,
                _ => if at_b != vb_end { at_b } else { wi < vi },
            };
            Some((Kind::L, through, vb_end))
        }
        None => Some((Kind::T, false, false)),
    }
}

fn dist(a: P2, b: P2) -> f64 { ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt() }

/// Every junction of wall `id` with the other walls of the document.
pub fn joins(id: &str, w: &Wall, walls: &[(String, Wall)]) -> Joins {
    let mut out = Joins::default();
    let wb = Band::of(w);
    if wb.len < 0.05 { return out; }
    let wi = walls.iter().position(|(i, _)| i == id).unwrap_or(usize::MAX);
    for at_b in [false, true] {
        let e = if at_b { w.b } else { w.a };
        let other_end = if at_b { w.a } else { w.b };
        let mut best: Option<(f64, EndJoin)> = None;
        for (vi, (vid, v)) in walls.iter().enumerate() {
            if vid == id { continue; }
            let vb = Band::of(v);
            let Some((kind, through, _)) = classify(wi, w, &wb, at_b, vi, v, &vb) else { continue };
            // which side of v this wall's body lies on
            let side = if vb.off_of(other_end) >= vb.centre() { 1.0 } else { -1.0 };
            let (near, far) = if side > 0.0 { (vb.hi, vb.lo) } else { (vb.lo, vb.hi) };
            let sheet = |s: f64| if s > 0.0 { vb.ext } else { vb.int };
            let c = wb.centre();
            let Some(frame) = wb.cross_param(c, &vb, if through { far } else { near }) else { continue };
            let (ext_face, int_face) = (wb.hi, wb.lo);
            let pick = |off: f64, voff: f64| wb.cross_param(off, &vb, voff).unwrap_or(frame);
            let (ext, int, upper_plate, inner) = match (kind, through) {
                (Kind::T, _) => (pick(ext_face, near + side * sheet(side)), pick(int_face, near + side * sheet(side)), pick(c, far), frame),
                (Kind::L, false) => (pick(ext_face, far - side * sheet(-side)), pick(int_face, near + side * sheet(side)), pick(c, far), frame),
                (Kind::L, true) => {
                    let inner = pick(c, near);
                    (pick(ext_face, far), pick(int_face, near), inner, inner)
                }
            };
            // the face of this wall the other one sits on
            let vside = if wb.off_of(if dist(v.a, e) < dist(v.b, e) { v.b } else { v.a }) >= wb.centre() { 1.0 } else { -1.0 };
            let score = (frame - if at_b { wb.len } else { 0.0 }).abs();
            let j = EndJoin { other: vid.clone(), kind, through, frame, ext, int, upper_plate, inner, side: vside };
            if best.as_ref().map(|b| score < b.0).unwrap_or(true) { best = Some((score, j)); }
        }
        let j = best.map(|b| b.1);
        if at_b { out.end = j; } else { out.start = j; }
    }
    // walls butting into this one (T)
    for (vi, (vid, v)) in walls.iter().enumerate() {
        if vid == id { continue; }
        let vb = Band::of(v);
        for at_b in [false, true] {
            let Some((Kind::T, _, _)) = classify(vi, v, &vb, at_b, wi, w, &wb) else { continue };
            let e = if at_b { v.b } else { v.a };
            let other_end = if at_b { v.a } else { v.b };
            let centre_pt = vb.at(vb.s_of(e), vb.centre());
            let den = (vb.u[0] * wb.n[0] + vb.u[1] * wb.n[1]).abs().max(0.15);
            let side = if wb.off_of(other_end) >= wb.centre() { 1.0 } else { -1.0 };
            out.backers.push(Backer { other: vid.clone(), centre: wb.s_of(centre_pt), width: vb.depth() / den, side });
        }
    }
    out.backers.sort_by(|a, b| a.centre.partial_cmp(&b.centre).unwrap());
    out
}
