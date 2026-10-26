//! Masonry walls: cavity walls (outer leaf, cavity with insulation and ties, inner leaf, plaster) and
//! single-leaf walls, with mitred corners / trimmed T-junctions, openings with lintels and sills,
//! a damp-proof course and the bond pattern as edges. See docs/MASONRY.md.
//!
//! Every layer of the wall is a *strip* between two offsets along the exterior normal. A strip's
//! ends are lines in plan (square, mitred against a neighbour, or trimmed to a neighbour's face),
//! so in the wall's own (t along a→b, o across) coordinates a strip is a convex quad. The strip is
//! cut into columns at every opening / lintel / sill boundary along t, and each column into
//! vertical pieces outside the voids of the features crossing it; every piece is a closed prism.

use plannr_core::geom::{cross, P3};

use crate::masonry::data::*;
use crate::masonry::expand::*;

const TOL: f64 = 0.03;
const STEEL_T: f64 = 0.008;
const ANGLE_LEG: f64 = 0.1;
const CONCRETE_LINTEL: f64 = 0.14;
const SILL_OVERRUN: f64 = 0.03;

// ---- plan frame ------------------------------------------------------------------------------------

/// The wall's line and its body offsets along the exterior normal.
#[derive(Debug, Clone, Copy)]
pub struct Band { pub a: P2, pub u: P2, pub n: P2, pub len: f64, pub lo: f64, pub hi: f64, pub lo_full: f64, pub hi_full: f64 }
impl Band {
    pub fn of(w: &Wall, d: &Defaults) -> Band {
        let len = w.length().max(1e-9);
        let u = [(w.b[0] - w.a[0]) / len, (w.b[1] - w.a[1]) / len];
        let n0 = [u[1], -u[0]];
        let n = if w.flip { [-n0[0], -n0[1]] } else { n0 };
        let t = w.thickness(d);
        let (lo, hi) = match w.reference.as_str() { "center" => (-t / 2.0, t / 2.0), "interior" => (0.0, t), _ => (-t, 0.0) };
        let p = w.plaster.thickness.max(0.0);
        let lo_full = if w.plaster.interior { lo - p } else { lo };
        let hi_full = if w.plaster.exterior { hi + p } else { hi };
        Band { a: w.a, u, n, len, lo, hi, lo_full, hi_full }
    }
    pub fn at(&self, t: f64, o: f64) -> P2 { [self.a[0] + self.u[0] * t + self.n[0] * o, self.a[1] + self.u[1] * t + self.n[1] * o] }
    pub fn t_of(&self, p: P2) -> f64 { (p[0] - self.a[0]) * self.u[0] + (p[1] - self.a[1]) * self.u[1] }
    pub fn o_of(&self, p: P2) -> f64 { (p[0] - self.a[0]) * self.n[0] + (p[1] - self.a[1]) * self.n[1] }
    pub fn centre(&self) -> f64 { (self.lo + self.hi) / 2.0 }
    pub fn depth(&self) -> f64 { self.hi - self.lo }
    fn contains(&self, p: P2, slack: f64) -> bool {
        let (t, o) = (self.t_of(p), self.o_of(p));
        o >= self.lo_full - TOL && o <= self.hi_full + TOL && t >= -slack - TOL && t <= self.len + slack + TOL
    }
}

fn cross2(a: P2, b: P2) -> f64 { a[0] * b[1] - a[1] * b[0] }
fn sub2(a: P2, b: P2) -> P2 { [a[0] - b[0], a[1] - b[1]] }
fn dist2(a: P2, b: P2) -> f64 { ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt() }

/// Intersection of two plan lines (point + direction).
fn meet(p: P2, d: P2, q: P2, e: P2) -> Option<P2> {
    let den = cross2(d, e);
    if den.abs() < 1e-9 { return None; }
    let s = cross2(sub2(q, p), e) / den;
    Some([p[0] + d[0] * s, p[1] + d[1] * s])
}

/// How one end of a wall is cut.
#[derive(Debug, Clone, Copy)]
pub enum Cut { Square(f64), Line { p: P2, d: P2 } }
impl Cut {
    /// Distance along the wall where its line at offset `o` meets this cut.
    pub fn param(&self, b: &Band, o: f64, fallback: f64) -> f64 {
        match *self {
            Cut::Square(t) => t,
            Cut::Line { p, d } => {
                let q = b.at(0.0, o);
                let den = cross2(b.u, d);
                if den.abs() < 0.12 { return fallback; }
                cross2(sub2(p, q), d) / den
            }
        }
    }
}

fn parallel(a: &Band, b: &Band) -> bool { cross2(a.u, b.u).abs() < 0.17 }

/// The cuts of both ends of wall `id` from the other walls of its storey.
/// Does another wall carry this wall's line straight on past its end `at_b` (a collinear neighbour
/// starting at that end and going the other way)? Then that end is no corner.
pub fn continues(id: &str, w: &Wall, at_b: bool, house: &House) -> bool {
    let e = if at_b { w.b } else { w.a };
    let l = w.length().max(1e-9); let u = [(w.b[0] - w.a[0]) / l, (w.b[1] - w.a[1]) / l];
    let out = if at_b { 1.0 } else { -1.0 }; // the direction the line carries on in
    house.walls.iter().any(|(vid, v)| {
        if vid == id || v.level != w.level { return false; }
        let lv = v.length().max(1e-9); let uv = [(v.b[0] - v.a[0]) / lv, (v.b[1] - v.a[1]) / lv];
        if (u[0] * uv[1] - u[1] * uv[0]).abs() > 0.02 { return false; }
        let (near, far) = if dist2(v.a, e) < dist2(v.b, e) { (v.a, v.b) } else { (v.b, v.a) };
        if dist2(near, e) > 0.02 { return false; }
        let along = ((far[0] - e[0]) * u[0] + (far[1] - e[1]) * u[1]) * out;
        let off = ((far[0] - e[0]) * u[1] - (far[1] - e[1]) * u[0]).abs();
        along > 0.05 && off < 0.02
    })
}

pub fn cuts(id: &str, w: &Wall, house: &House) -> (Cut, Cut) {
    let d = &house.defaults;
    let wb = Band::of(w, d);
    let mut out = [Cut::Square(0.0), Cut::Square(wb.len)];
    for (k, at_b) in [false, true].into_iter().enumerate() {
        let own = if at_b { &w.end_join } else { &w.start_join };
        if own == "none" { continue; }
        // the line carries on: no corner here (a wall meeting this point Ts into it)
        if continues(id, w, at_b, house) { continue; }
        let e = if at_b { w.b } else { w.a };
        let other_end = if at_b { w.a } else { w.b };
        let mut best: Option<(f64, Cut)> = None;
        for (vid, v) in &house.walls {
            if vid == id || v.level != w.level { continue; }
            let vb = Band::of(v, d);
            if vb.len < 0.05 || parallel(&wb, &vb) || !vb.contains(e, wb.depth()) { continue; }
            // is it an L: one of v's ends lies in this wall's body near this end (and is not switched off)?
            let near_end = |p: P2| wb.contains(p, vb.depth()) && (if at_b { wb.t_of(p) > wb.len - vb.depth() - 2.0 * TOL } else { wb.t_of(p) < vb.depth() + 2.0 * TOL });
            let v_end = if near_end(v.b) && (dist2(v.b, e) <= dist2(v.a, e) || !near_end(v.a)) { Some(true) } else if near_end(v.a) { Some(false) } else { None };
            // v's line carries on past that end: this is a T against it, not a corner
            let v_end = v_end.filter(|vb_end| !continues(vid, v, *vb_end, house));
            let cut = match v_end {
                Some(vb_end) => {
                    if (if vb_end { &v.end_join } else { &v.start_join }) == "none" { continue; }
                    // mitre: through the meeting of the faces away from each other's body and of the faces towards it
                    let v_far = if vb_end { v.a } else { v.b };
                    let w_side = if wb.o_of(v_far) >= wb.centre() { 1.0 } else { -1.0 }; // v's body lies on this side of w
                    let v_side = if vb.o_of(other_end) >= vb.centre() { 1.0 } else { -1.0 };
                    let (w_near, w_far) = if w_side > 0.0 { (wb.hi, wb.lo) } else { (wb.lo, wb.hi) };
                    let (v_near, v_far_o) = if v_side > 0.0 { (vb.hi, vb.lo) } else { (vb.lo, vb.hi) };
                    let (Some(p_out), Some(p_in)) = (meet(wb.at(0.0, w_far), wb.u, vb.at(0.0, v_far_o), vb.u), meet(wb.at(0.0, w_near), wb.u, vb.at(0.0, v_near), vb.u)) else { continue };
                    let dd = sub2(p_in, p_out);
                    if dd[0].hypot(dd[1]) < 1e-6 { continue; }
                    Cut::Line { p: p_out, d: dd }
                }
                None => {
                    // T: trim to the face of v on this wall's side (its plaster included)
                    let side = if vb.o_of(other_end) >= vb.centre() { 1.0 } else { -1.0 };
                    let face = if side > 0.0 { vb.hi_full } else { vb.lo_full };
                    Cut::Line { p: vb.at(0.0, face), d: vb.u }
                }
            };
            let t = cut.param(&wb, wb.centre(), if at_b { wb.len } else { 0.0 });
            let score = (t - if at_b { wb.len } else { 0.0 }).abs();
            if best.as_ref().map(|b| score < b.0).unwrap_or(true) { best = Some((score, cut)); }
        }
        if let Some((_, c)) = best { out[k] = c; }
    }
    (out[0], out[1])
}

// ---- layers ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role { Outer, Cavity, Inner, Single, PlasterInt, PlasterExt }

#[derive(Debug, Clone)]
pub struct Layer {
    pub role: Role, pub key: &'static str, pub o0: f64, pub o1: f64,
    /// Flat top (above the storey base); with a `top` profile, its lowest point.
    pub h: f64,
    pub material: String, pub unit: Option<Unit>,
    /// The top along the wall as (t, height) points when it is not flat (a gable, a roof edge).
    pub top: Vec<[f64; 2]>,
}
impl Layer {
    /// Height of the layer's top at `t`.
    pub fn top_at(&self, t: f64) -> f64 {
        let p = &self.top;
        if p.is_empty() { return self.h; }
        if t <= p[0][0] { return p[0][1]; }
        for w in p.windows(2) {
            if t <= w[1][0] { let f = if w[1][0] - w[0][0] > 1e-12 { (t - w[0][0]) / (w[1][0] - w[0][0]) } else { 0.0 }; return w[0][1] + (w[1][1] - w[0][1]) * f; }
        }
        p[p.len() - 1][1]
    }
}

/// Heights of the inner / single leaf and of the outer leaf.
pub fn heights(w: &Wall, house: &House, b: &Band) -> (f64, f64) {
    let d = &house.defaults;
    let full = house.height(w.level);
    let probe_o = if w.is_cavity() { b.lo + w.inner_unit(d).width / 2.0 } else { b.centre() };
    let mid = b.at(b.len / 2.0, probe_o);
    let inner = w.height.unwrap_or_else(|| {
        // under a structure (the floor above, attic joists in the top of the storey) the leaf stops beneath it
        match house.floor_depth_above(w.level, mid) {
            Some(depth) => full - depth,
            None => if house.has_storey_above(w.level) { full - house.floor_thickness } else { full },
        }
    });
    let outer = w.outer_height.unwrap_or(full);
    (inner.max(0.1), outer.max(0.1))
}

pub fn layers(w: &Wall, house: &House, b: &Band) -> Vec<Layer> {
    let d = &house.defaults;
    let (h_in, h_out) = heights(w, house, b);
    let p = w.plaster.thickness.max(0.0);
    let mut v = vec![];
    if w.is_cavity() {
        let (uo, ui, cav) = (w.outer_unit(d), w.inner_unit(d), w.cavity_of(d));
        v.push(Layer { role: Role::Outer, key: "outer", o0: b.hi - uo.width, o1: b.hi, h: h_out, material: w.outer_material.clone().unwrap_or_else(|| uo.material()), unit: Some(uo.clone()), top: vec![] });
        let (c0, c1) = (b.lo + ui.width, b.hi - uo.width);
        let ins = match cav.insulation.kind.as_str() { "fill" => c1 - c0, "board" => cav.insulation.thickness.clamp(0.0, c1 - c0), _ => 0.0 };
        if ins > 1e-4 { v.push(Layer { role: Role::Cavity, key: "cavity.insulation", o0: c0, o1: c0 + ins, h: h_out, material: material_id("insulation"), unit: None, top: vec![] }); }
        v.push(Layer { role: Role::Inner, key: "inner", o0: b.lo, o1: b.lo + ui.width, h: h_in, material: ui.material(), unit: Some(ui), top: vec![] });
        if w.plaster.exterior && p > 0.0 { v.push(Layer { role: Role::PlasterExt, key: "plaster.ext", o0: b.hi, o1: b.hi + p, h: h_out, material: w.finish_exterior.clone().unwrap_or_else(|| material_id("plaster")), unit: None, top: vec![] }); }
    } else {
        let ui = w.inner_unit(d);
        v.push(Layer { role: Role::Single, key: "single", o0: b.lo, o1: b.hi, h: h_in, material: ui.material(), unit: Some(ui), top: vec![] });
        if w.plaster.exterior && p > 0.0 { v.push(Layer { role: Role::PlasterExt, key: "plaster.ext", o0: b.hi, o1: b.hi + p, h: h_in, material: w.finish_exterior.clone().unwrap_or_else(|| material_id("plaster")), unit: None, top: vec![] }); }
    }
    if w.plaster.interior && p > 0.0 { v.push(Layer { role: Role::PlasterInt, key: "plaster.int", o0: b.lo - p, o1: b.lo, h: h_in, material: w.finish_interior.clone().unwrap_or_else(|| material_id("plaster")), unit: None, top: vec![] }); }
    v
}

// ---- roofs ------------------------------------------------------------------------------------------

/// Sample a piecewise-linear function on [a, b] into its corner points.
fn sample(f: &dyn Fn(f64) -> f64, a: f64, b: f64) -> Vec<[f64; 2]> {
    fn rec(f: &dyn Fn(f64) -> f64, a: f64, fa: f64, b: f64, fb: f64, out: &mut Vec<[f64; 2]>, depth: u32) {
        let lin = |t: f64| fa + (fb - fa) * (t - a) / (b - a);
        let qs = [a + (b - a) * 0.25, (a + b) / 2.0, a + (b - a) * 0.75];
        let straight = qs.iter().all(|&t| (f(t) - lin(t)).abs() < 2e-4);
        if straight || depth > 14 || b - a < 0.004 { out.push([b, fb]); return; }
        let m = (a + b) / 2.0; let fm = f(m);
        rec(f, a, fa, m, fm, out, depth + 1);
        rec(f, m, fm, b, fb, out, depth + 1);
    }
    let mut out = vec![[a, f(a)]];
    if b - a > 1e-6 { rec(f, a, f(a), b, f(b), &mut out, 0); }
    // drop collinear points
    let mut simple: Vec<[f64; 2]> = vec![];
    for p in out {
        while simple.len() >= 2 {
            let (q, r) = (simple[simple.len() - 2], simple[simple.len() - 1]);
            let y = q[1] + (p[1] - q[1]) * (r[0] - q[0]) / (p[0] - q[0]).max(1e-12);
            if (y - r[1]).abs() < 1e-4 { simple.pop(); } else { break; }
        }
        simple.push(p);
    }
    simple
}

/// The roof this wall stands under: the roof and its edge whose line is the wall's outer face.
pub fn roof_edge<'a>(w: &Wall, house: &'a House, b: &Band) -> Option<(&'a crate::roof::RoofGeom, &'a crate::roof::RoofEdge)> {
    let mid = b.at(b.len / 2.0, b.hi);
    let mut best: Option<(f64, &crate::roof::RoofGeom, &crate::roof::RoofEdge)> = None;
    for g in house.roofs.iter().filter(|g| g.level == w.level) {
        for e in &g.edges {
            let el = dist2(e.a, e.b); if el < 1e-6 { continue; }
            let ed = [(e.b[0] - e.a[0]) / el, (e.b[1] - e.a[1]) / el];
            if cross2(b.u, ed).abs() > 0.05 || b.n[0] * e.inward[0] + b.n[1] * e.inward[1] > -0.9 { continue; }
            let off = (mid[0] - e.a[0]) * e.inward[0] + (mid[1] - e.a[1]) * e.inward[1];
            if off.abs() > 0.35 { continue; }
            let along = |p: P2| (p[0] - e.a[0]) * ed[0] + (p[1] - e.a[1]) * ed[1];
            let (ta, tb) = (along(b.at(0.0, b.hi)), along(b.at(b.len, b.hi)));
            if ta.max(tb) < 0.05 || ta.min(tb) > el - 0.05 { continue; }
            if best.map_or(true, |x| off.abs() < x.0) { best = Some((off.abs(), g, e)); }
        }
    }
    best.map(|(_, g, e)| (g, e))
}

/// Tops of the layers of a wall under a roof: the leaves under a wall plate stop under it (the inner
/// leaf rises to a plate above the storey, a shed's high side); the outer leaf, the cavity and the
/// render follow the rafters' underside (the eaves) and rise into the gable; the inner leaf and
/// the plaster rise into the gable too. Flat roofs: the outer leaf and cavity rise to the upstand.
fn roof_tops(w: &Wall, house: &House, b: &Band, layers: &mut [Layer], t0: f64, t1: f64) {
    let Some((g, e)) = roof_edge(w, house, b) else { return };
    let base = house.base(w.level);
    let full = w.height.unwrap_or_else(|| house.height(w.level));
    let depth = |o: f64| { let p = b.at(b.len / 2.0, o); (p[0] - e.a[0]) * e.inward[0] + (p[1] - e.a[1]) * e.inward[1] };
    for l in layers.iter_mut() {
        let inner = matches!(l.role, Role::Inner | Role::Single | Role::PlasterInt);
        let (da, db) = { let (x, y) = (depth(l.o0), depth(l.o1)); (x.min(y), x.max(y)) };
        let plate = e.plate.filter(|p| db.min(p.d[1]) - da.max(p.d[0]) > 0.01);
        let (o0, o1, h) = (l.o0, l.o1, l.h);
        let under = move |t: f64| g.under(b.at(t, o0)).min(g.under(b.at(t, o1))) - base;
        let f: Box<dyn Fn(f64) -> f64> = match (e.kind, plate) {
            (_, Some(p)) => {
                let pb = p.bottom - base;
                if inner { if pb > full + 0.01 { Box::new(move |_| pb) } else { continue } } else { Box::new(move |_| pb) }
            }
            (crate::roof::EdgeKind::Flat, None) => { if inner { continue } Box::new(under) }
            (crate::roof::EdgeKind::Gable, None) if inner => Box::new(move |t| under(t).max(h)),
            _ => Box::new(under),
        };
        let pts = sample(&|t| f(t).max(0.1), t0, t1);
        let (lo, hi) = pts.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p[1]), a.1.max(p[1])));
        if hi - lo < 1e-4 { l.h = lo; l.top.clear(); } else { l.h = lo; l.top = pts; }
    }
}

// ---- openings --------------------------------------------------------------------------------------

/// An opening resolved against the wall: zones along t and heights.
#[derive(Debug, Clone)]
pub struct Hole {
    pub id: String, pub kind: String, pub t0: f64, pub t1: f64, pub bottom: f64, pub head: f64, pub sill: f64,
    /// Inner (or single) lintel: kind, extent, height.
    pub li_kind: String, pub li: [f64; 2], pub li_h: f64,
    /// Outer leaf lintel (cavity walls).
    pub lo_kind: String, pub lo: [f64; 2], pub lo_h: f64,
    pub stone: bool, pub st: [f64; 2], pub st_h: f64,
}

pub fn holes(w: &Wall, house: &House, h_in: f64) -> Vec<Hole> {
    let d = &house.defaults;
    let mut v: Vec<Hole> = w.openings.iter().filter(|o| o.width > 0.05 && o.height > 0.05).map(|o| {
        let door = o.kind == "door";
        let bottom = if door { 0.0 } else { o.sill.max(0.0) };
        let head = (bottom + o.height).min(h_in - 0.05).max(bottom + 0.05);
        let bearing = o.lintel.bearing.unwrap_or(d.lintel.bearing).max(0.0);
        let li_kind = o.lintel.inner.clone().unwrap_or_else(|| d.lintel.inner.clone());
        let inner_unit = w.inner_unit(d);
        let li_h = match li_kind.as_str() { "none" => 0.0, "clay" => inner_unit.course(), _ => CONCRETE_LINTEL };
        let lo_kind = if w.is_cavity() { o.lintel.outer.clone().unwrap_or_else(|| d.lintel.outer.clone()) } else { "none".into() };
        let lo_h = match lo_kind.as_str() { "none" => 0.0, "concrete" => CONCRETE_LINTEL, _ => STEEL_T };
        let stone = w.is_cavity() && !door && o.sill_stone.unwrap_or(true) && d.sill.outer != "none" && bottom > d.sill.thickness + 0.01;
        let (t0, t1) = (o.offset, o.offset + o.width);
        Hole { id: o.id.clone(), kind: o.kind.clone(), t0, t1, bottom, head, sill: bottom,
               li: [t0 - bearing, t1 + bearing], li_h: li_h.min((h_in - head).max(0.0)), li_kind,
               lo: [t0 - bearing, t1 + bearing], lo_h, lo_kind,
               stone, st: [t0 - SILL_OVERRUN, t1 + SILL_OVERRUN], st_h: d.sill.thickness }
    }).collect();
    v.sort_by(|a, b| a.t0.partial_cmp(&b.t0).unwrap());
    // neighbouring openings share the pier between them: their lintels and sills stop halfway
    for i in 1..v.len() {
        let mid = (v[i - 1].t1 + v[i].t0) / 2.0;
        let (a, b) = v.split_at_mut(i);
        let (p, n) = (&mut a[i - 1], &mut b[0]);
        if p.li[1] > mid { p.li[1] = mid; } if n.li[0] < mid { n.li[0] = mid; }
        if p.lo[1] > mid { p.lo[1] = mid; } if n.lo[0] < mid { n.lo[0] = mid; }
        if p.st[1] > mid { p.st[1] = mid; } if n.st[0] < mid { n.st[0] = mid; }
    }
    v
}

/// Height ranges a layer loses in the column whose middle is `tm`.
fn voids(role: Role, holes: &[Hole], tm: f64) -> Vec<[f64; 2]> {
    let inside = |r: [f64; 2]| tm > r[0] + 1e-9 && tm < r[1] - 1e-9;
    let mut v = vec![];
    for h in holes {
        if inside([h.t0, h.t1]) {
            let bottom = if role == Role::Outer && h.stone { h.sill - h.st_h } else { h.bottom };
            v.push([bottom, h.head]);
        }
        match role {
            Role::Inner | Role::Single if h.li_h > 0.0 && inside(h.li) => v.push([h.head, h.head + h.li_h]),
            Role::Outer if h.lo_h > 0.0 && inside(h.lo) => v.push([h.head, h.head + h.lo_h]),
            _ => {}
        }
        if role == Role::Outer && h.stone && inside(h.st) { v.push([h.sill - h.st_h, h.sill]); }
    }
    v
}
/// [0, top] minus the voids.
fn pieces(top: f64, mut voids: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    voids.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
    let mut out = vec![]; let mut y = 0.0;
    for v in voids {
        let (a, b) = (v[0].max(0.0), v[1].min(top));
        if b <= a { continue; }
        if a - y > 1e-4 { out.push([y, a]); }
        y = y.max(b);
    }
    if top - y > 1e-4 { out.push([y, top]); }
    out
}

/// Clip a convex polygon in (t, o) to t0 ≤ t ≤ t1.
fn clip_t(poly: &[[f64; 2]], t0: f64, t1: f64) -> Vec<[f64; 2]> {
    let half = |pts: Vec<[f64; 2]>, keep: &dyn Fn(f64) -> bool, edge: f64| -> Vec<[f64; 2]> {
        let n = pts.len(); let mut out = vec![];
        for i in 0..n {
            let (p, q) = (pts[i], pts[(i + 1) % n]);
            let (ip, iq) = (keep(p[0]), keep(q[0]));
            if ip { out.push(p); }
            if ip != iq { let s = (edge - p[0]) / (q[0] - p[0]); out.push([edge, p[1] + (q[1] - p[1]) * s]); }
        }
        out
    };
    let a = half(poly.to_vec(), &|t| t >= t0 - 1e-9, t0);
    if a.len() < 3 { return vec![]; }
    half(a, &|t| t <= t1 + 1e-9, t1)
}

/// The full geometry of a wall, shared by the expansion, the elevation drawing and the tests.
pub struct Built {
    pub band: Band,
    pub cuts: (Cut, Cut),
    pub layers: Vec<Layer>,
    pub holes: Vec<Hole>,
    /// Column boundaries along t.
    pub columns: Vec<[f64; 2]>,
    pub h_in: f64,
    pub h_out: f64,
}
impl Built {
    pub fn new(id: &str, w: &Wall, house: &House) -> Built {
        let band = Band::of(w, &house.defaults);
        let cuts = cuts(id, w, house);
        let mut layers = layers(w, house, &band);
        let (h_in, h_out) = heights(w, house, &band);
        let holes = holes(w, house, h_in);
        let mut ts: Vec<f64> = vec![];
        for l in &layers { for o in [l.o0, l.o1] { ts.push(cuts.0.param(&band, o, 0.0)); ts.push(cuts.1.param(&band, o, band.len)); } }
        let (gmin, gmax) = (ts.iter().cloned().fold(f64::MAX, f64::min), ts.iter().cloned().fold(f64::MIN, f64::max));
        roof_tops(w, house, &band, &mut layers, gmin, gmax);
        let mut bounds = vec![gmin, gmax];
        for l in &layers { bounds.extend(l.top.iter().map(|p| p[0])); }
        for h in &holes { bounds.extend([h.t0, h.t1, h.li[0], h.li[1], h.lo[0], h.lo[1], h.st[0], h.st[1]]); }
        bounds.retain(|t| *t >= gmin - 1e-9 && *t <= gmax + 1e-9);
        bounds.sort_by(|a, b| a.partial_cmp(b).unwrap());
        bounds.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
        let columns = bounds.windows(2).map(|w| [w[0], w[1]]).filter(|c| c[1] - c[0] > 1e-5).collect();
        Built { band, cuts, layers, holes, columns, h_in, h_out }
    }
    pub fn start(&self, o: f64) -> f64 { self.cuts.0.param(&self.band, o, 0.0) }
    pub fn end(&self, o: f64) -> f64 { self.cuts.1.param(&self.band, o, self.band.len) }
    /// The layer's plan quad in (t, o).
    pub fn quad(&self, l: &Layer) -> Vec<[f64; 2]> { vec![[self.start(l.o0), l.o0], [self.end(l.o0), l.o0], [self.end(l.o1), l.o1], [self.start(l.o1), l.o1]] }
    /// Pieces of a layer: (column, y range).
    pub fn layer_pieces(&self, l: &Layer) -> Vec<([f64; 2], [f64; 2])> {
        let mut out = vec![];
        for c in &self.columns {
            let tm = (c[0] + c[1]) / 2.0;
            let top = l.top_at(c[0]).min(l.top_at(c[1]));
            for p in pieces(top, voids(l.role, &self.holes, tm)) { out.push((*c, p)); }
        }
        out
    }
    /// The sloped tops of a layer: (column, bottom, top at the column's start, top at its end).
    pub fn layer_wedges(&self, l: &Layer) -> Vec<([f64; 2], f64, f64, f64)> {
        if l.top.is_empty() { return vec![]; }
        self.columns.iter().filter_map(|c| {
            let (a, b) = (l.top_at(c[0]), l.top_at(c[1]));
            if (a - b).abs() < 1e-4 { None } else { Some((*c, a.min(b), a, b)) }
        }).collect()
    }
    /// Pattern segments ((t, y), (t, y)) on the face of `l` at offset `o`.
    pub fn pattern(&self, l: &Layer, o: f64) -> Vec<([f64; 2], [f64; 2])> {
        let Some(u) = &l.unit else { return vec![] };
        let (s, e) = (self.start(o), self.end(o));
        let c = u.course().max(0.01);
        let (len, wid) = (u.length + u.joint, u.width + u.joint);
        let mut out = vec![];
        for (col, yr) in self.layer_pieces(l) {
            let (ta, tb) = (col[0].max(s), col[1].min(e));
            if tb - ta < 1e-3 { continue; }
            let (y0, y1) = (yr[0], yr[1]);
            let k0 = (y0 / c).floor() as i64; let k1 = (y1 / c).ceil() as i64;
            for k in k0..=k1 {
                let y = k as f64 * c;
                if y > y0 + 1e-4 && y < y1 - 1e-4 { out.push(([ta, y], [tb, y])); }
                // head joints of course k
                let (ya, yb) = ((k as f64 * c).max(y0), ((k + 1) as f64 * c).min(y1));
                if yb - ya < 1e-3 || k < 0 { continue; }
                let kk = k as usize;
                let (phase, period, second): (f64, f64, Option<f64>) = match u.bond.as_str() {
                    "stack" => (0.0, len, None),
                    "third" => ((kk % 3) as f64 * len / 3.0, len, None),
                    "english" => if kk % 2 == 0 { (0.0, len, None) } else { (wid / 2.0, wid, None) },
                    "flemish" => { let p = len + wid; ((kk % 2) as f64 * p / 2.0, p, Some(len)) }
                    _ => ((kk % 2) as f64 * len / 2.0, len, None),
                };
                let first = ((ta - s - phase) / period).floor() as i64;
                let mut m = first;
                loop {
                    let base = s + phase + m as f64 * period;
                    if base > tb { break; }
                    for x in std::iter::once(base).chain(second.map(|q| base + q)) {
                        if x > ta + 1e-4 && x < tb - 1e-4 { out.push(([x, ya], [x, yb])); }
                    }
                    m += 1;
                }
            }
        }
        // the sloped tops: courses up to the slope, head joints cut by it
        for (col, y0, ta0, tb0) in self.layer_wedges(l) {
            let (ta, tb) = (col[0].max(s), col[1].min(e));
            if tb - ta < 1e-3 { continue; }
            let top = |t: f64| ta0 + (tb0 - ta0) * (t - col[0]) / (col[1] - col[0]);
            let (ya, yb) = (top(ta), top(tb));
            let ymax = ya.max(yb);
            // where the slope crosses height y (the side above it)
            let span_at = |y: f64| -> Option<[f64; 2]> {
                if y >= ymax - 1e-4 { return None; }
                let tc = ta + (tb - ta) * (y - ya) / (yb - ya);
                Some(if yb > ya { [tc.max(ta), tb] } else { [ta, tc.min(tb)] })
            };
            let k0 = (y0 / c).floor() as i64; let k1 = (ymax / c).ceil() as i64;
            for k in k0..=k1 {
                let y = k as f64 * c;
                if y > y0 + 1e-4 { if let Some(r) = span_at(y) { if r[1] - r[0] > 1e-3 { out.push(([r[0], y], [r[1], y])); } } }
                let (ya2, yb2) = ((k as f64 * c).max(y0), ((k + 1) as f64 * c).min(ymax));
                if yb2 - ya2 < 1e-3 || k < 0 { continue; }
                let kk = k as usize;
                let (phase, period, second): (f64, f64, Option<f64>) = match u.bond.as_str() {
                    "stack" => (0.0, len, None),
                    "third" => ((kk % 3) as f64 * len / 3.0, len, None),
                    "english" => if kk % 2 == 0 { (0.0, len, None) } else { (wid / 2.0, wid, None) },
                    "flemish" => { let p = len + wid; ((kk % 2) as f64 * p / 2.0, p, Some(len)) }
                    _ => ((kk % 2) as f64 * len / 2.0, len, None),
                };
                let first = ((ta - s - phase) / period).floor() as i64;
                let mut m = first;
                loop {
                    let base = s + phase + m as f64 * period;
                    if base > tb { break; }
                    for x in std::iter::once(base).chain(second.map(|q| base + q)) {
                        let ytop = top(x).min(yb2);
                        if x > ta + 1e-4 && x < tb - 1e-4 && ytop - ya2 > 1e-3 { out.push(([x, ya2], [x, ytop])); }
                    }
                    m += 1;
                }
            }
        }
        out
    }
}

fn p3(b: &Band, t: f64, o: f64, y: f64) -> P3 { let p = b.at(t, o); [p[0], y, p[1]] }

fn unit_category(u: &Unit) -> &'static str { if u.kind == "facingBrick" || u.kind == "brick" { "bricks" } else { "blocks" } }
fn unit_label(u: &Unit) -> String { if u.name.is_empty() { format!("{} {:.0}×{:.0}×{:.0}", u.kind, u.length * 1000.0, u.width * 1000.0, u.height * 1000.0) } else { u.name.clone() } }
fn insulation_label(c: &Cavity) -> String {
    let m = match c.insulation.material.as_str() { "pur" => "PUR", "mineralWool" => "minerale wol", "pir" => "PIR", "xps" => "XPS", _ => "EPS" };
    if c.insulation.kind == "fill" { format!("Spouwisolatie {m} (vulling)") } else { format!("Spouwisolatie {m}-platen {:.0} mm", c.insulation.thickness * 1000.0) }
}

pub fn expand_wall(id: &str, w: &Wall, house: &House, layer: &str) -> Expansion {
    let mut ex = Expansion::default();
    if w.length() < 0.05 { return ex; }
    let d = &house.defaults;
    let bt = Built::new(id, w, house);
    let b = bt.band;
    let base = house.base(w.level);
    let mut f = Faces::new(layer);
    let u3: P3 = [b.u[0], 0.0, b.u[1]];
    let n3: P3 = [b.n[0], 0.0, b.n[1]];
    for l in &bt.layers {
        let quad = bt.quad(l);
        let fid = format!("{id}#{}", l.key);
        let om = (l.o0 + l.o1) / 2.0;
        let (sm, em) = (bt.start(om), bt.end(om));
        let mut area = 0.0;
        for (col, yr) in bt.layer_pieces(l) {
            let poly = clip_t(&quad, col[0], col[1]);
            if poly.len() < 3 { continue; }
            let plan: Vec<P2> = poly.iter().map(|q| b.at(q[0], q[1])).collect();
            f.prism_plan(&fid, &plan, &[], base + yr[0], base + yr[1], &l.material);
            area += ((col[1].min(em) - col[0].max(sm)).max(0.0)) * (yr[1] - yr[0]);
        }
        for (col, y0, ya, yb) in bt.layer_wedges(l) {
            let poly = clip_t(&quad, col[0], col[1]);
            if poly.len() < 3 { continue; }
            let top = |t: f64| ya + (yb - ya) * (t - col[0]) / (col[1] - col[0]);
            let plan: Vec<P2> = poly.iter().map(|q| b.at(q[0], q[1])).collect();
            let tops: Vec<f64> = poly.iter().map(|q| base + top(q[0])).collect();
            f.prism_sloped(&fid, &plan, base + y0, &tops, &l.material);
            let (ta, tb) = (col[0].max(sm), col[1].min(em));
            if tb > ta { area += (tb - ta) * ((top(ta) + top(tb)) / 2.0 - y0); }
        }
        let width = l.o1 - l.o0;
        match l.role {
            Role::Outer | Role::Inner | Role::Single => {
                let u = l.unit.clone().unwrap();
                let count = area * u.per_m2();
                ex.items.push(Item::new(l.key, unit_category(&u), unit_label(&u)).count(count).area(area));
                ex.items.push(Item::new(format!("{}.mortar", l.key), "mortar", "Mortel").volume((area * width - count * u.volume()).max(0.0)));
            }
            Role::Cavity => { ex.items.push(Item::new(l.key, "insulation", insulation_label(&w.cavity_of(d))).area(area).volume(area * width)); }
            Role::PlasterInt => ex.items.push(Item::new(l.key, "plaster", format!("Binnenpleister {:.0} mm", width * 1000.0)).area(area)),
            Role::PlasterExt => ex.items.push(Item::new(l.key, "plaster", format!("Buitenpleister {:.0} mm", width * 1000.0)).area(area)),
        }
        // DPC under each leaf on the ground floor
        if w.dpc && w.level == 0 && matches!(l.role, Role::Outer | Role::Inner | Role::Single) {
            let plan: Vec<P2> = quad.iter().map(|q| b.at(q[0], q[1])).collect();
            f.prism_plan(&format!("{id}#dpc"), &plan, &[], base - 0.003, base, &material_id("dpc"));
            ex.items.push(Item::new(format!("dpc.{}", l.key), "dpc", "Vochtkering (DPC)").length((em - sm).max(0.0)).area((em - sm).max(0.0) * width));
        }
        // bond pattern
        let faces: Vec<f64> = match l.role {
            Role::Outer => if w.plaster.exterior { vec![] } else { vec![l.o1] },
            Role::Inner => if w.plaster.interior { vec![] } else { vec![l.o0] },
            Role::Single => { let mut v = vec![]; if !w.plaster.interior { v.push(l.o0); } if !w.plaster.exterior { v.push(l.o1); } v }
            _ => vec![],
        };
        let pid = format!("{id}#pattern");
        for o in faces {
            let lift = if o >= om { 0.0005 } else { -0.0005 }; // just proud of the face
            for (p, q) in bt.pattern(l, o) { f.edge(&pid, p3(&b, p[0], o + lift, base + p[1]), p3(&b, q[0], o + lift, base + q[1])); }
        }
    }
    // lintels and sills
    let (inner_leaf, outer_leaf) = (bt.layers.iter().find(|l| matches!(l.role, Role::Inner | Role::Single)).cloned(), bt.layers.iter().find(|l| l.role == Role::Outer).cloned());
    for (k, h) in bt.holes.iter().enumerate() {
        if let Some(l) = &inner_leaf {
            if h.li_h > 0.0 {
                let r = [h.li[0].max(bt.start(l.o0).max(bt.start(l.o1))), h.li[1].min(bt.end(l.o0).min(bt.end(l.o1)))];
                let mat = if h.li_kind == "clay" { l.material.clone() } else { material_id("concrete") };
                let origin = p3(&b, 0.0, 0.0, base);
                f.boxf(&format!("{id}#lintel.{k}.inner"), origin, u3, UP, n3, [r, [h.head, h.head + h.li_h], [l.o0, l.o1]], &mat);
                let desc = if h.li_kind == "clay" { "Latei baksteen".to_string() } else { format!("Latei beton {:.0} mm", h.li_h * 1000.0) };
                ex.items.push(Item::new(format!("lintel.{k}.inner"), "lintels", desc).count(1.0).length(r[1] - r[0]));
            }
        }
        if let Some(l) = &outer_leaf {
            let r = [h.lo[0].max(bt.start(l.o0).max(bt.start(l.o1))), h.lo[1].min(bt.end(l.o0).min(bt.end(l.o1)))];
            let origin = p3(&b, 0.0, 0.0, base);
            match h.lo_kind.as_str() {
                "none" => {}
                "concrete" => {
                    f.boxf(&format!("{id}#lintel.{k}.outer"), origin, u3, UP, n3, [r, [h.head, h.head + h.lo_h], [l.o0, l.o1]], &material_id("concrete"));
                    ex.items.push(Item::new(format!("lintel.{k}.outer"), "lintels", format!("Latei beton {:.0} mm", h.lo_h * 1000.0)).count(1.0).length(r[1] - r[0]));
                }
                _ => {
                    // L 100×100×8: horizontal leg under the bricks, vertical leg in the cavity against the outer leaf
                    let back = l.o0 - STEEL_T;
                    let front = (back + ANGLE_LEG).min(l.o1 - 0.01);
                    f.boxf(&format!("{id}#lintel.{k}.outer"), origin, u3, UP, n3, [r, [h.head, h.head + STEEL_T], [back, front]], &material_id("steel"));
                    f.boxf(&format!("{id}#lintel.{k}.outer"), origin, u3, UP, n3, [r, [h.head + STEEL_T, h.head + ANGLE_LEG], [back, l.o0]], &material_id("steel"));
                    ex.items.push(Item::new(format!("lintel.{k}.outer"), "lintels", "Hoeklijn staal L100×100×8").count(1.0).length(r[1] - r[0]));
                }
            }
        }
        if h.stone {
            let (o_back, o_front) = match &outer_leaf { Some(l) => (l.o0, l.o1 + d.sill.projection), None => (b.hi - 0.02, b.hi + d.sill.projection) };
            let o_front = if w.plaster.exterior { o_front + w.plaster.thickness } else { o_front };
            f.boxf(&format!("{id}#sill.{k}"), p3(&b, 0.0, 0.0, base), u3, UP, n3, [h.st, [h.sill - h.st_h, h.sill], [o_back, o_front]], &material_id("bluestone"));
            ex.items.push(Item::new(format!("sill.{k}"), "sills", "Arduin dorpel").count(1.0).length(h.st[1] - h.st[0]));
        }
    }
    // cavity ties on a staggered grid over the net outer-leaf area
    if let (Some(lo), Some(li)) = (&outer_leaf, &inner_leaf) {
        if w.is_cavity() {
            let per = w.cavity_of(d).ties.per_m2.max(0.0);
            if per > 0.0 && lo.o0 - li.o1 > 0.005 {
                let (t0, t1) = (bt.start(lo.o0).max(bt.start(li.o1)) + 0.15, bt.end(lo.o0).min(bt.end(li.o1)) - 0.15);
                let ytop = bt.h_in.min(bt.h_out) - 0.15;
                // the grid inside the 0.15 m margins carries the ties of the whole net outer-leaf area
                let region = ((t1 - t0) * (ytop - 0.15)).max(1e-6);
                let gross = (bt.end(lo.o1) - bt.start(lo.o1)).max(0.0) * bt.h_out;
                let density = per * (gross / region).max(1.0);
                let sy = 0.45; let sx = 1.0 / (density * sy);
                let mut y = 0.15 + sy / 2.0; let mut row = 0; let mut n = 0usize;
                let origin = p3(&b, 0.0, 0.0, base);
                while y < ytop {
                    let mut x = t0 + if row % 2 == 1 { sx / 2.0 } else { 0.0 } + sx / 4.0;
                    while x < t1 {
                        let blocked = bt.holes.iter().any(|h| x > h.t0 - 0.15 && x < h.t1 + 0.15 && y > h.bottom.min(h.sill - h.st_h) - 0.15 && y < h.head + h.lo_h.max(h.li_h) + 0.15);
                        if !blocked {
                            // a tie is a thin rod: drawn as one line across the cavity (a box per tie would cost 12 triangles and 12 edges each)
                            let at = |o: f64| plannr_core::geom::add(plannr_core::geom::add(origin, plannr_core::geom::scale(u3, x)), plannr_core::geom::add(plannr_core::geom::scale(UP, y), plannr_core::geom::scale(n3, o)));
                            f.edge(&format!("{id}#tie"), at(li.o1), at(lo.o0));
                            n += 1;
                        }
                        x += sx;
                    }
                    y += sy; row += 1;
                }
                if n > 0 { ex.items.push(Item::new("tie", "ties", "Spouwhaak RVS").count(n as f64)); }
            }
        }
    }
    ex.faces = f.out;
    ex
}

// ---- elevation ---------------------------------------------------------------------------------------

fn esc(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;") }

/// Elevation of one wall seen from the exterior (SVG).
pub fn wall_svg(id: &str, w: &Wall, house: &House) -> String {
    let d = &house.defaults;
    let bt = Built::new(id, w, house);
    let b = bt.band;
    let face_layer = bt.layers.iter().find(|l| l.role == Role::Outer).or_else(|| bt.layers.iter().find(|l| l.role == Role::Single)).cloned();
    let Some(fl) = face_layer else { return String::from("<svg xmlns=\"http://www.w3.org/2000/svg\"/>") };
    let o = fl.o1;
    let (s, e) = (bt.start(o), bt.end(o));
    let len = (e - s).max(0.1);
    let height = bt.layers.iter().flat_map(|l| l.top.iter().map(|p| p[1]).chain(std::iter::once(l.h))).fold(fl.h.max(bt.h_in), f64::max);
    let scale = (1000.0 / len).min(320.0 / height.max(0.5) * 1.6);
    let (ml, mt) = (70.0, 60.0);
    let wpx = len * scale; let hpx = height * scale;
    let (vw, vh) = (wpx + ml + 60.0, hpx + mt + 190.0);
    // seen from outside: the viewer's right is cross(−n, up); map t onto the drawing so it is not mirrored
    let right = { let f: P3 = [-b.n[0], 0.0, -b.n[1]]; cross(f, UP) };
    let forward = right[0] * b.u[0] + right[2] * b.u[1] >= 0.0;
    let x = |t: f64| ml + if forward { (t - s) * scale } else { (e - t) * scale };
    let y = |h: f64| mt + hpx - h * scale;
    let mut svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{vw:.0}\" height=\"{vh:.0}\" viewBox=\"0 0 {vw:.1} {vh:.1}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"11\">");
    svg += &format!("<rect x=\"0\" y=\"0\" width=\"{vw:.1}\" height=\"{vh:.1}\" fill=\"#ffffff\"/>");
    let fill = fl.unit.as_ref().and_then(|u| u.color.clone()).unwrap_or_else(|| "#c9785a".into());
    // leaf pieces
    for (col, yr) in bt.layer_pieces(&fl) {
        let (ta, tb) = (col[0].max(s), col[1].min(e));
        if tb - ta < 1e-4 { continue; }
        let (xa, xb) = (x(ta).min(x(tb)), x(ta).max(x(tb)));
        svg += &format!("<rect class=\"leaf\" x=\"{xa:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\" fill-opacity=\"0.35\" stroke=\"#333\" stroke-width=\"0.8\"/>", y(yr[1]), xb - xa, (yr[1] - yr[0]) * scale, esc(&fill));
    }
    for (col, y0, ya, yb) in bt.layer_wedges(&fl) {
        let (ta, tb) = (col[0].max(s), col[1].min(e));
        if tb - ta < 1e-4 { continue; }
        let top = |t: f64| ya + (yb - ya) * (t - col[0]) / (col[1] - col[0]);
        svg += &format!("<polygon class=\"leaf\" points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" fill=\"{}\" fill-opacity=\"0.35\" stroke=\"#333\" stroke-width=\"0.8\"/>",
            x(ta), y(y0), x(tb), y(y0), x(tb), y(top(tb)), x(ta), y(top(ta)), esc(&fill));
    }
    // bond pattern
    svg += "<g class=\"pattern\" stroke=\"#6b3a2a\" stroke-width=\"0.4\">";
    for (p, q) in bt.pattern(&fl, o) { svg += &format!("<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\"/>", x(p[0]), y(p[1]), x(q[0]), y(q[1])); }
    svg += "</g>";
    // openings, lintels, sills
    for (k, h) in bt.holes.iter().enumerate() {
        let (xa, xb) = (x(h.t0).min(x(h.t1)), x(h.t0).max(x(h.t1)));
        svg += &format!("<rect class=\"opening\" x=\"{xa:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#dbe9f6\" stroke=\"#1a5fb4\" stroke-width=\"1.2\"/>", y(h.head), xb - xa, (h.head - h.bottom) * scale);
        let label = if h.kind == "door" { format!("D{}: {:.2} × {:.2}", k + 1, h.t1 - h.t0, h.head - h.bottom) } else { format!("W{}: {:.2} × {:.2}, sill {:.2}", k + 1, h.t1 - h.t0, h.head - h.bottom, h.sill) };
        svg += &format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"#1a5fb4\">{}</text>", (xa + xb) / 2.0, y((h.head + h.bottom) / 2.0) + 4.0, esc(&label));
        let lint = if fl.role == Role::Outer { (h.lo, h.lo_h.max(if h.lo_kind == "steelAngle" { ANGLE_LEG } else { 0.0 })) } else { (h.li, h.li_h) };
        if lint.1 > 0.0 {
            let (la, lb) = (x(lint.0[0]).min(x(lint.0[1])), x(lint.0[0]).max(x(lint.0[1])));
            svg += &format!("<rect class=\"lintel\" x=\"{la:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"none\" stroke=\"#555\" stroke-width=\"1\" stroke-dasharray=\"5 3\"/>", y(h.head + lint.1), lb - la, lint.1 * scale);
        }
        if h.stone {
            let (sa, sb) = (x(h.st[0]).min(x(h.st[1])), x(h.st[0]).max(x(h.st[1])));
            svg += &format!("<rect class=\"sill\" x=\"{sa:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#4f5660\" stroke=\"#222\" stroke-width=\"0.8\"/>", y(h.sill), sb - sa, h.st_h * scale);
        }
    }
    // dimensions
    let yd = y(0.0) + 24.0;
    svg += &format!("<g class=\"dims\" stroke=\"#222\" stroke-width=\"0.8\"><line x1=\"{:.1}\" y1=\"{yd:.1}\" x2=\"{:.1}\" y2=\"{yd:.1}\"/><line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\"/><line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\"/></g>", ml, ml + wpx, ml, yd - 5.0, ml, yd + 5.0, ml + wpx, yd - 5.0, ml + wpx, yd + 5.0);
    svg += &format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{:.0} mm</text>", ml + wpx / 2.0, yd + 16.0, len * 1000.0);
    let xd = ml - 26.0;
    svg += &format!("<g class=\"dims\" stroke=\"#222\" stroke-width=\"0.8\"><line x1=\"{xd:.1}\" y1=\"{:.1}\" x2=\"{xd:.1}\" y2=\"{:.1}\"/></g>", y(0.0), y(fl.h));
    svg += &format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" transform=\"rotate(-90 {:.1} {:.1})\">{:.0} mm</text>", xd - 6.0, y(fl.h / 2.0), xd - 6.0, y(fl.h / 2.0), fl.h * 1000.0);
    // title block
    let ty = y(0.0) + 52.0;
    let mut lines = vec![format!("Masonry wall {}", &id[..id.len().min(8)])];
    if w.is_cavity() {
        let cav = w.cavity_of(d);
        lines.push(format!("Cavity wall · outer: {} · inner: {}", unit_label(&w.outer_unit(d)), unit_label(&w.inner_unit(d))));
        let ins = if cav.insulation.kind == "none" { "empty cavity".to_string() } else { insulation_label(&cav) };
        lines.push(format!("Cavity {:.0} mm · {} · ties {}/m²", cav.width * 1000.0, ins, cav.ties.per_m2));
    } else {
        lines.push(format!("Single leaf · {}", unit_label(&w.inner_unit(d))));
    }
    lines.push(format!("Length {:.2} m · height {:.2} m (inner {:.2} m) · storey {}", len, fl.h, bt.h_in, w.level));
    lines.push("Elevation seen from the exterior · dimensions in mm".into());
    let bw = 470.0; let bh = 16.0 * lines.len() as f64 + 12.0;
    let bx = (ml + wpx - bw).max(ml);
    svg += &format!("<rect x=\"{bx:.1}\" y=\"{ty:.1}\" width=\"{bw:.1}\" height=\"{bh:.1}\" fill=\"none\" stroke=\"#222\" stroke-width=\"0.8\"/>");
    for (i, l) in lines.iter().enumerate() {
        let weight = if i == 0 { " font-weight=\"bold\"" } else { "" };
        svg += &format!("<text x=\"{:.1}\" y=\"{:.1}\"{weight}>{}</text>", bx + 8.0, ty + 18.0 + 16.0 * i as f64, esc(l));
    }
    svg += "</svg>";
    svg
}
