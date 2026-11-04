//! Small mesh builder: polygonal tubes swept along polylines (mitred corners, bent corners), rings,
//! frustums, discs and boxes, all emitted as core faces with `<id>#<key>` ids.

use plannr_core::geom::{add, cross, dot, len, normalize, scale, sub, P3};
use plannr_core::model::{Entity, Face};
use serde_json::Map;

pub struct Out { pub id: String, pub layer: String, pub faces: Vec<Entity>, /// Prepended to every face key (`elbow.2.` for the elbow drawn at a pipe's second corner).
    pub prefix: String }

impl Out {
    pub fn new(id: &str, layer: &str) -> Out { Out { id: id.into(), layer: layer.into(), faces: vec![], prefix: String::new() } }
    pub fn poly(&mut self, key: &str, pts: Vec<P3>, mat: &str) {
        if pts.len() < 3 { return; }
        self.faces.push(Entity::Face(Face { id: format!("{}#{}{key}", self.id, self.prefix), layer: self.layer.clone(), outer: pts, holes: vec![], material: Some(mat.into()), back_material: Some(mat.into()), extra: Map::new() }));
    }
    /// Tube of `radius` with `sides` sides along `path` (consecutive duplicate points dropped), mitred at corners.
    pub fn sweep(&mut self, key: &str, path: &[P3], radius: f64, sides: usize, mat: &str, caps: bool) {
        let rings = rings(path, radius, sides);
        self.skin(key, &rings, mat, caps);
    }
    /// Tube whose radius changes linearly along the path from `r0` to `r1`.
    pub fn frustum(&mut self, key: &str, a: P3, b: P3, r0: f64, r1: f64, sides: usize, mat: &str) {
        let d = sub(b, a); if len(d) < 1e-9 { return; }
        let (u, v) = basis(normalize(d));
        let ra = circle(a, u, v, r0, sides); let rb = circle(b, u, v, r1, sides);
        self.skin(key, &[ra, rb], mat, true);
    }
    pub fn cylinder(&mut self, key: &str, a: P3, b: P3, r: f64, sides: usize, mat: &str) { self.frustum(key, a, b, r, r, sides, mat); }
    /// A flat ring (annulus) in the plane normal to `n`, from `r_in` to `r_out`, extruded by `thickness` along `n`.
    pub fn ring(&mut self, key: &str, c: P3, n: P3, r_in: f64, r_out: f64, thickness: f64, sides: usize, mat: &str) {
        let n = normalize(n); let (u, v) = basis(n);
        let c0 = add(c, scale(n, -thickness / 2.0)); let c1 = add(c, scale(n, thickness / 2.0));
        let (i0, o0) = (circle(c0, u, v, r_in, sides), circle(c0, u, v, r_out, sides));
        let (i1, o1) = (circle(c1, u, v, r_in, sides), circle(c1, u, v, r_out, sides));
        for k in 0..sides {
            let j = (k + 1) % sides;
            self.poly(key, vec![o0[k], o0[j], o1[j], o1[k]], mat); // outside
            self.poly(key, vec![i0[j], i0[k], i1[k], i1[j]], mat); // inside
            self.poly(key, vec![i0[k], o0[k], o0[j], i0[j]], mat); // bottom annulus
            self.poly(key, vec![i1[j], o1[j], o1[k], i1[k]], mat); // top annulus
        }
    }
    pub fn disc(&mut self, key: &str, c: P3, n: P3, r: f64, sides: usize, mat: &str) {
        let (u, v) = basis(normalize(n)); self.poly(key, circle(c, u, v, r, sides), mat);
    }
    /// Box centred at `c` with axes (unit) `u`, `v`, `w` and full sizes `s`.
    pub fn block(&mut self, key: &str, c: P3, u: P3, v: P3, w: P3, s: P3, mat: &str) {
        let (hu, hv, hw) = (scale(u, s[0] / 2.0), scale(v, s[1] / 2.0), scale(w, s[2] / 2.0));
        let p = |a: f64, b: f64, cc: f64| add(add(add(c, scale(hu, a)), scale(hv, b)), scale(hw, cc));
        let q = [p(-1., -1., -1.), p(1., -1., -1.), p(1., 1., -1.), p(-1., 1., -1.), p(-1., -1., 1.), p(1., -1., 1.), p(1., 1., 1.), p(-1., 1., 1.)];
        for f in [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]] { self.poly(key, f.iter().map(|i| q[*i]).collect(), mat); }
    }
    fn skin(&mut self, key: &str, rings: &[Vec<P3>], mat: &str, caps: bool) {
        if rings.len() < 2 { return; }
        let n = rings[0].len();
        for w in rings.windows(2) {
            for k in 0..n { let j = (k + 1) % n; self.poly(key, vec![w[0][k], w[0][j], w[1][j], w[1][k]], mat); }
        }
        if caps {
            let mut first = rings[0].clone(); first.reverse(); self.poly(key, first, mat);
            self.poly(key, rings[rings.len() - 1].clone(), mat);
        }
    }
}

/// An orthonormal basis (u, v) perpendicular to the unit vector `d`.
pub fn basis(d: P3) -> (P3, P3) {
    let helper = if d[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let u = normalize(cross(helper, d)); let v = cross(d, u);
    (u, v)
}
pub fn circle(c: P3, u: P3, v: P3, r: f64, sides: usize) -> Vec<P3> {
    (0..sides).map(|i| { let a = i as f64 / sides as f64 * std::f64::consts::TAU; add(c, add(scale(u, r * a.cos()), scale(v, r * a.sin()))) }).collect()
}
/// Rotate `v` by the rotation that takes unit `a` to unit `b` (parallel transport of a frame).
fn transport(v: P3, a: P3, b: P3) -> P3 {
    let axis = cross(a, b); let s = len(axis); let c = dot(a, b).clamp(-1.0, 1.0);
    if s < 1e-9 { return if c > 0.0 { v } else { scale(v, -1.0) }; }
    let k = scale(axis, 1.0 / s);
    // Rodrigues
    add(add(scale(v, c), scale(cross(k, v), s)), scale(k, dot(k, v) * (1.0 - c)))
}

/// Cross-section rings along a polyline: perpendicular at the ends, mitred at the corners.
pub fn rings(path: &[P3], radius: f64, sides: usize) -> Vec<Vec<P3>> {
    let mut pts: Vec<P3> = vec![];
    for p in path { if pts.last().map(|q| len(sub(*p, *q)) > 1e-7).unwrap_or(true) { pts.push(*p); } }
    if pts.len() < 2 { return vec![]; }
    let dirs: Vec<P3> = pts.windows(2).map(|w| normalize(sub(w[1], w[0]))).collect();
    let (mut u, mut v) = basis(dirs[0]);
    let mut out = vec![circle(pts[0], u, v, radius, sides)];
    for i in 1..pts.len() {
        let d_in = dirs[i - 1];
        if i == pts.len() - 1 { out.push(circle(pts[i], u, v, radius, sides)); break; }
        let d_out = dirs[i];
        let ring_in = circle(pts[i], u, v, radius, sides);
        // the mitre plane is perpendicular to the bisector of the two directions (a straight run
        // keeps a perpendicular ring; a reversal has no mitre)
        let n = add(d_in, d_out);
        if len(n) < 1e-6 || len(sub(d_in, d_out)) < 1e-6 { out.push(ring_in); } else {
            let n = normalize(n); let dn = dot(d_in, n);
            // project the incoming cross-section along d_in onto the mitre plane (normal n through the corner)
            let ring: Vec<P3> = ring_in.iter().map(|p| { let off = sub(*p, pts[i]); let s = if dn.abs() < 1e-9 { 0.0 } else { -dot(off, n) / dn }; add(*p, scale(d_in, s)) }).collect();
            out.push(ring);
        }
        u = transport(u, d_in, d_out);
        // re-orthogonalise against drift
        u = normalize(sub(u, scale(d_out, dot(u, d_out)))); v = cross(d_out, u);
    }
    out
}

/// Replace every interior corner of a polyline by a circular arc of radius `r` (clamped so the
/// tangent lengths fit in the adjacent segments). Returns the new path and, per original corner,
/// the radius actually used.
pub fn bend_path(points: &[P3], r: f64) -> (Vec<P3>, Vec<f64>) {
    let mut pts: Vec<P3> = vec![];
    for p in points { if pts.last().map(|q| len(sub(*p, *q)) > 1e-7).unwrap_or(true) { pts.push(*p); } }
    if pts.len() < 3 { return (pts, vec![]); }
    let mut out = vec![pts[0]]; let mut radii = vec![];
    // the tangent length already used on each segment by the previous corner
    let mut used_prev = 0.0;
    for i in 1..pts.len() - 1 {
        let c = pts[i];
        let d1 = normalize(sub(c, pts[i - 1])); let d2 = normalize(sub(pts[i + 1], c));
        let cosang = dot(d1, d2).clamp(-1.0, 1.0); let theta = cosang.acos();
        if theta < 0.01 || theta > std::f64::consts::PI - 0.01 { out.push(c); radii.push(0.0); used_prev = 0.0; continue; }
        let len_in = len(sub(c, pts[i - 1])) - used_prev; let len_out = len(sub(pts[i + 1], c));
        let next_is_corner = i + 1 < pts.len() - 1;
        let avail = len_in.min(if next_is_corner { len_out / 2.0 } else { len_out }).max(0.0);
        let t_want = r * (theta / 2.0).tan();
        let t = t_want.min(avail * 0.999);
        let rr = if (theta / 2.0).tan() > 1e-9 { t / (theta / 2.0).tan() } else { 0.0 };
        radii.push(rr);
        if t < 1e-5 { out.push(c); used_prev = 0.0; continue; }
        let a = sub(c, scale(d1, t));
        let bis = normalize(sub(d2, d1));
        let o = add(c, scale(bis, rr / (theta / 2.0).cos()));
        let ra = sub(a, o);
        let n = ((theta / (std::f64::consts::PI / 2.0)) * 8.0).ceil().max(2.0) as usize;
        for k in 0..=n {
            let phi = theta * k as f64 / n as f64;
            out.push(add(o, add(scale(ra, phi.cos()), scale(d1, rr * phi.sin()))));
        }
        used_prev = t;
    }
    out.push(pts[pts.len() - 1]);
    (out, radii)
}

/// Polygon side count that looks round enough for a radius (m).
pub fn sides_for(r: f64) -> usize { if r < 0.012 { 10 } else if r < 0.03 { 14 } else if r < 0.06 { 18 } else { 24 } }
