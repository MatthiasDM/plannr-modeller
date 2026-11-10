//! Solid modelling: closed solids from a box, a cylinder or an extruded profile (with holes), and
//! booleans between solids (union, subtract, intersect).
//!
//! A solid is a group whose faces close up into a watertight shell. Booleans run on triangles in a BSP
//! tree (the csg.js algorithm), then the coplanar pieces of the result are merged back into faces with
//! holes, so a box with a slot cut in it has a top face with a hole, not a mosaic of slivers.

use std::collections::HashMap;

use crate::geom::*;
use crate::model::*;

/// Plane classification tolerance (metres): 1 µm.
const EPS: f64 = 1e-6;

#[derive(Debug, Clone, Copy)]
struct Plane { n: P3, w: f64 }
impl Plane {
    fn flip(&mut self) { self.n = scale(self.n, -1.0); self.w = -self.w; }
}

#[derive(Debug, Clone)]
pub struct Poly { pts: Vec<P3>, plane: Plane, mat: Option<String> }
impl Poly {
    pub(crate) fn new(pts: Vec<P3>, mat: Option<String>) -> Option<Poly> {
        // a robust plane: Newell normal through the centroid
        let n = normalize(newell(&pts));
        if len(n) < 0.5 { return None; }
        let c = scale(pts.iter().fold([0.0; 3], |a, p| add(a, *p)), 1.0 / pts.len() as f64);
        Some(Poly { plane: Plane { n, w: dot(n, c) }, pts, mat })
    }
    pub fn flip(&mut self) { self.pts.reverse(); self.plane.flip(); }
    pub fn points(&self) -> &[P3] { &self.pts }
    pub fn material(&self) -> &Option<String> { &self.mat }
    pub fn set_material(&mut self, m: Option<String>) { self.mat = m; }
}

const COPLANAR: u8 = 0; const FRONT: u8 = 1; const BACK: u8 = 2; const SPANNING: u8 = 3;

/// Split a convex polygon by a plane into the four lists (csg.js `splitPolygon`).
fn split(pl: &Plane, p: Poly, cf: &mut Vec<Poly>, cb: &mut Vec<Poly>, f: &mut Vec<Poly>, b: &mut Vec<Poly>) {
    let mut kind = 0u8; let mut types = Vec::with_capacity(p.pts.len());
    for v in &p.pts {
        let t = dot(pl.n, *v) - pl.w;
        let ty = if t < -EPS { BACK } else if t > EPS { FRONT } else { COPLANAR };
        kind |= ty; types.push(ty);
    }
    match kind {
        COPLANAR => if dot(pl.n, p.plane.n) > 0.0 { cf.push(p) } else { cb.push(p) },
        FRONT => f.push(p),
        BACK => b.push(p),
        _ => {
            let (mut fv, mut bv) = (vec![], vec![]);
            let n = p.pts.len();
            for i in 0..n {
                let j = (i + 1) % n; let (ti, tj) = (types[i], types[j]); let (vi, vj) = (p.pts[i], p.pts[j]);
                if ti != BACK { fv.push(vi); }
                if ti != FRONT { bv.push(vi); }
                if (ti | tj) == SPANNING {
                    let t = (pl.w - dot(pl.n, vi)) / dot(pl.n, sub(vj, vi));
                    let x = lerp(vi, vj, t);
                    fv.push(x); bv.push(x);
                }
            }
            if fv.len() >= 3 { f.push(Poly { pts: fv, plane: p.plane, mat: p.mat.clone() }); }
            if bv.len() >= 3 { b.push(Poly { pts: bv, plane: p.plane, mat: p.mat }); }
        }
    }
}

/// A BSP tree in an arena (no recursion, so deep trees can't overflow the stack).
#[derive(Default)]
struct Bsp { nodes: Vec<Node> }
#[derive(Default)]
struct Node { plane: Option<Plane>, front: Option<usize>, back: Option<usize>, polys: Vec<Poly> }

impl Bsp {
    fn new(polys: Vec<Poly>) -> Bsp { let mut t = Bsp { nodes: vec![Node::default()] }; t.build(0, polys); t }
    fn build(&mut self, root: usize, polys: Vec<Poly>) {
        let mut work = vec![(root, polys)];
        while let Some((i, polys)) = work.pop() {
            if polys.is_empty() { continue; }
            let pl = *self.nodes[i].plane.get_or_insert(polys[0].plane);
            let (mut cf, mut cb, mut f, mut b) = (vec![], vec![], vec![], vec![]);
            for p in polys { split(&pl, p, &mut cf, &mut cb, &mut f, &mut b); }
            self.nodes[i].polys.extend(cf); self.nodes[i].polys.extend(cb);
            if !f.is_empty() {
                let c = match self.nodes[i].front { Some(c) => c, None => { self.nodes.push(Node::default()); let c = self.nodes.len() - 1; self.nodes[i].front = Some(c); c } };
                work.push((c, f));
            }
            if !b.is_empty() {
                let c = match self.nodes[i].back { Some(c) => c, None => { self.nodes.push(Node::default()); let c = self.nodes.len() - 1; self.nodes[i].back = Some(c); c } };
                work.push((c, b));
            }
        }
    }
    fn invert(&mut self) {
        for n in self.nodes.iter_mut() {
            for p in n.polys.iter_mut() { p.flip(); }
            if let Some(pl) = n.plane.as_mut() { pl.flip(); }
            std::mem::swap(&mut n.front, &mut n.back);
        }
    }
    /// The parts of `polys` outside this solid.
    fn clip_polys(&self, polys: Vec<Poly>) -> Vec<Poly> {
        if self.nodes[0].plane.is_none() { return polys; }
        let mut out = vec![]; let mut work = vec![(0usize, polys)];
        while let Some((i, polys)) = work.pop() {
            let n = &self.nodes[i];
            let Some(pl) = n.plane else { out.extend(polys); continue };
            let (mut f, mut b) = (vec![], vec![]);
            for p in polys { let (mut cf, mut cb) = (vec![], vec![]); split(&pl, p, &mut cf, &mut cb, &mut f, &mut b); f.extend(cf); b.extend(cb); }
            match n.front { Some(c) => work.push((c, f)), None => out.extend(f) }
            if let Some(c) = n.back { work.push((c, b)); } // no back child: inside, dropped
        }
        out
    }
    fn clip_to(&mut self, other: &Bsp) {
        for i in 0..self.nodes.len() { let ps = std::mem::take(&mut self.nodes[i].polys); self.nodes[i].polys = other.clip_polys(ps); }
    }
    fn all(&self) -> Vec<Poly> { self.nodes.iter().flat_map(|n| n.polys.iter().cloned()).collect() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolMode { Union, Subtract, Intersect }

pub fn csg(a: Vec<Poly>, b: Vec<Poly>, mode: BoolMode) -> Vec<Poly> {
    let (mut a, mut b) = (Bsp::new(a), Bsp::new(b));
    match mode {
        BoolMode::Union => { a.clip_to(&b); b.clip_to(&a); b.invert(); b.clip_to(&a); b.invert(); let bp = b.all(); a.build(0, bp); a.all() }
        BoolMode::Subtract => { a.invert(); a.clip_to(&b); b.clip_to(&a); b.invert(); b.clip_to(&a); b.invert(); let bp = b.all(); a.build(0, bp); a.invert(); a.all() }
        BoolMode::Intersect => { a.invert(); b.clip_to(&a); b.invert(); a.clip_to(&b); b.clip_to(&a); let bp = b.all(); a.build(0, bp); a.invert(); a.all() }
    }
}

// ---- faces <-> polygons ------------------------------------------------------------------

/// Every face under an entity, in world coordinates, as triangles.
pub fn polys_of(e: &Entity, world: &Mat4) -> Vec<Poly> {
    let mut out = vec![];
    fn walk(e: &Entity, m: &Mat4, out: &mut Vec<Poly>) {
        match e {
            Entity::Face(f) => {
                let outer: Vec<P3> = f.outer.iter().map(|p| mat_apply(m, *p)).collect();
                let holes: Vec<Vec<P3>> = f.holes.iter().map(|h| h.iter().map(|p| mat_apply(m, *p)).collect()).collect();
                let pts: Vec<P3> = outer.iter().chain(holes.iter().flatten()).copied().collect();
                let tri = tessellate(&outer, &holes);
                for t in tri.chunks(3) {
                    if t.len() < 3 { continue; }
                    if let Some(p) = Poly::new(vec![pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]], f.material.clone()) { out.push(p); }
                }
            }
            Entity::Group(g) => { let gm = mat_mul(m, &g.transform); for c in &g.entities { walk(c, &gm, out); } }
            _ => {}
        }
    }
    match e { Entity::Group(g) => { let gm = mat_mul(world, &g.transform); for c in &g.entities { walk(c, &gm, &mut out); } } _ => walk(e, world, &mut out) }
    out
}

/// Signed volume enclosed by triangles/polygons (positive for an outward-facing closed shell).
pub fn volume(polys: &[Poly]) -> f64 {
    polys.iter().map(|p| (1..p.pts.len() - 1).map(|i| dot(p.pts[0], cross(p.pts[i], p.pts[i + 1])) / 6.0).sum::<f64>()).sum()
}

type Key = (i64, i64, i64);
const Q: f64 = 1e7; // 0.1 µm grid for welding
fn qkey(p: P3) -> Key { ((p[0] * Q).round() as i64, (p[1] * Q).round() as i64, (p[2] * Q).round() as i64) }

/// Merge coplanar polygons (same plane, same side, same material) into faces with holes. Polygons whose
/// boundary can't be traced cleanly are kept as they are.
pub fn merge_faces(polys: Vec<Poly>, layer: &str) -> Vec<Face> {
    // weld every vertex to its first representative on a fine grid
    let mut rep: HashMap<Key, P3> = HashMap::new();
    let polys: Vec<Poly> = polys.into_iter().map(|mut p| { for v in p.pts.iter_mut() { *v = *rep.entry(qkey(*v)).or_insert(*v); } p }).collect();
    // group by plane (normal and offset, quantised) and material
    let mut groups: HashMap<(i64, i64, i64, i64, Option<String>), Vec<Poly>> = HashMap::new();
    for p in polys {
        let n = p.plane.n;
        let k = ((n[0] * 1e5).round() as i64, (n[1] * 1e5).round() as i64, (n[2] * 1e5).round() as i64, (p.plane.w * 1e6).round() as i64, p.mat.clone());
        groups.entry(k).or_default().push(p);
    }
    let mut keys: Vec<_> = groups.keys().cloned().collect(); keys.sort();
    let mut faces = vec![];
    for k in keys {
        let ps = groups.remove(&k).unwrap();
        match merge_plane(&ps) {
            Some(loops) => for (outer, holes) in loops { faces.push(face(outer, holes, ps[0].mat.clone(), layer)); },
            None => for p in ps { faces.push(face(p.pts, vec![], p.mat, layer)); },
        }
    }
    faces
}
fn face(outer: Vec<P3>, holes: Vec<Vec<P3>>, mat: Option<String>, layer: &str) -> Face {
    Face { id: new_id(), layer: layer.into(), outer, holes, material: mat, back_material: None, extra: Default::default() }
}

/// The boundary of a set of coplanar polygons as outer loops with their holes.
fn merge_plane(ps: &[Poly]) -> Option<Vec<(Vec<P3>, Vec<Vec<P3>>)>> {
    let n = ps[0].plane.n;
    let (u, v) = plane_basis(n);
    let to2 = |p: P3| (dot(p, u), dot(p, v));
    // all vertices of the plane: a vertex lying on another polygon's edge splits that edge (T-junctions)
    let mut verts: Vec<P3> = vec![]; let mut seen = std::collections::HashSet::new();
    for p in ps { for q in &p.pts { if seen.insert(qkey(*q)) { verts.push(*q); } } }
    let mut edges: HashMap<(Key, Key), usize> = HashMap::new();
    let mut pt: HashMap<Key, P3> = HashMap::new();
    for p in ps {
        let m = p.pts.len();
        for i in 0..m {
            let (a, b) = (p.pts[i], p.pts[(i + 1) % m]);
            let ab = sub(b, a); let l2 = dot(ab, ab); if l2 < 1e-18 { continue; }
            let mut on: Vec<(f64, P3)> = verts.iter().filter_map(|&x| {
                let t = dot(sub(x, a), ab) / l2;
                if t <= 1e-9 || t >= 1.0 - 1e-9 { return None; }
                if dist(lerp(a, b, t), x) > EPS * 0.5 { return None; }
                Some((t, x))
            }).collect();
            on.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
            let mut chain = vec![a]; chain.extend(on.into_iter().map(|x| x.1)); chain.push(b);
            for w in chain.windows(2) {
                let (ka, kb) = (qkey(w[0]), qkey(w[1])); if ka == kb { continue; }
                pt.insert(ka, w[0]); pt.insert(kb, w[1]);
                // an edge and its reverse cancel: inside the merged region
                if let Some(c) = edges.get_mut(&(kb, ka)) { *c -= 1; if *c == 0 { edges.remove(&(kb, ka)); } }
                else { *edges.entry((ka, kb)).or_insert(0) += 1; }
            }
        }
    }
    if edges.values().any(|&c| c != 1) { return None; }
    // trace loops; where several edges leave a vertex, take the sharpest right turn (keeps loops simple)
    let mut out_of: HashMap<Key, Vec<Key>> = HashMap::new();
    for (a, b) in edges.keys() { out_of.entry(*a).or_default().push(*b); }
    for l in out_of.values_mut() { l.sort(); }
    let mut used: std::collections::HashSet<(Key, Key)> = std::collections::HashSet::new();
    let mut loops: Vec<Vec<P3>> = vec![];
    let mut starts: Vec<(Key, Key)> = edges.keys().cloned().collect(); starts.sort();
    for (s0, s1) in starts {
        if used.contains(&(s0, s1)) { continue; }
        let mut lp = vec![s0]; let (mut prev, mut cur) = (s0, s1); used.insert((s0, s1));
        let mut guard = 0;
        while cur != s0 {
            guard += 1; if guard > 100_000 { return None; }
            lp.push(cur);
            let cands: Vec<Key> = out_of.get(&cur)?.iter().filter(|nx| !used.contains(&(cur, **nx))).cloned().collect();
            if cands.is_empty() { return None; }
            let next = if cands.len() == 1 { cands[0] } else {
                let (px, py) = to2(pt[&prev]); let (cx, cy) = to2(pt[&cur]); let din = (cx - px, cy - py);
                *cands.iter().min_by(|a, b| {
                    let ang = |k: &Key| { let (x, y) = to2(pt[k]); let d = (x - cx, y - cy); (din.0 * d.1 - din.1 * d.0).atan2(din.0 * d.0 + din.1 * d.1) };
                    ang(a).partial_cmp(&ang(b)).unwrap()
                }).unwrap()
            };
            used.insert((cur, next)); prev = cur; cur = next;
        }
        // drop vertices in the middle of straight runs
        let pts: Vec<P3> = lp.iter().map(|k| pt[k]).collect();
        let m = pts.len(); let mut clean = vec![];
        for i in 0..m {
            let (a, b, c) = (pts[(i + m - 1) % m], pts[i], pts[(i + 1) % m]);
            let (d1, d2) = (normalize(sub(b, a)), normalize(sub(c, b)));
            if len(cross(d1, d2)) < 1e-9 && dot(d1, d2) > 0.0 { continue; }
            clean.push(b);
        }
        if clean.len() >= 3 { loops.push(clean); }
    }
    // outer loops wind with the normal, holes against it; each hole goes in the smallest outer around it
    let signed = |l: &Vec<P3>| dot(newell(l), n) / 2.0;
    let (outers, holes): (Vec<Vec<P3>>, Vec<Vec<P3>>) = loops.into_iter().partition(|l| signed(l) > 0.0);
    if outers.is_empty() { return None; }
    let inside = |p: (f64, f64), poly: &Vec<P3>| {
        let q: Vec<(f64, f64)> = poly.iter().map(|x| to2(*x)).collect(); let mut c = false;
        for i in 0..q.len() { let (a, b) = (q[i], q[(i + 1) % q.len()]); if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 { c = !c; } }
        c
    };
    let mut res: Vec<(Vec<P3>, Vec<Vec<P3>>)> = outers.into_iter().map(|o| (o, vec![])).collect();
    for h in holes {
        // a point just inside the hole (off its boundary): the midpoint of its first edge, nudged inward
        let (a, b) = (to2(h[0]), to2(h[1])); let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        let d = (b.0 - a.0, b.1 - a.1); let l = (d.0 * d.0 + d.1 * d.1).sqrt().max(1e-12);
        // the hole winds clockwise: its inside is on the right of its edges
        let probe = (mid.0 + d.1 / l * 1e-6, mid.1 - d.0 / l * 1e-6);
        let best = res.iter().enumerate().filter(|(_, (o, _))| inside(probe, o)).min_by(|x, y| signed(&x.1 .0).partial_cmp(&signed(&y.1 .0)).unwrap()).map(|(i, _)| i);
        match best { Some(i) => res[i].1.push(h), None => return None }
    }
    Some(res)
}

// ---- primitives --------------------------------------------------------------------------

/// The faces of a closed solid: a planar profile (with holes) swept by `d`. Faces point outward.
pub fn extrude_faces(outer: &[P3], holes: &[Vec<P3>], d: P3, mat: Option<String>, layer: &str) -> Vec<Face> {
    let n = normal(outer);
    let orient = |mut l: Vec<P3>, want: P3| { if dot(newell(&l), want) < 0.0 { l.reverse(); } l };
    let down = scale(normalize(d), -1.0);
    // the base faces against the sweep, the cap along it
    let base_outer = orient(outer.to_vec(), down);
    let base_holes: Vec<Vec<P3>> = holes.iter().map(|h| orient(h.clone(), scale(down, -1.0))).collect();
    let mv = |l: &Vec<P3>| -> Vec<P3> { l.iter().map(|p| add(*p, d)).collect() };
    let mut faces = vec![face(base_outer.clone(), base_holes.clone(), mat.clone(), layer)];
    let cap_outer: Vec<P3> = mv(&base_outer).into_iter().rev().collect();
    let cap_holes: Vec<Vec<P3>> = base_holes.iter().map(|h| mv(h).into_iter().rev().collect()).collect();
    faces.push(face(cap_outer, cap_holes, mat.clone(), layer));
    // sides: for each loop edge, a quad facing away from the material
    let _ = n;
    for lp in std::iter::once(&base_outer).chain(base_holes.iter()) {
        let m = lp.len();
        for i in 0..m {
            let (a, b) = (lp[i], lp[(i + 1) % m]);
            // the base winds clockwise seen along d (it faces -d), so (a, a+d, b+d, b) faces out
            faces.push(face(vec![a, add(a, d), add(b, d), b], vec![], mat.clone(), layer));
        }
    }
    faces
}

pub fn box_faces(min: P3, max: P3, mat: Option<String>, layer: &str) -> Vec<Face> {
    let (a, b) = ([min[0].min(max[0]), min[1].min(max[1]), min[2].min(max[2])], [min[0].max(max[0]), min[1].max(max[1]), min[2].max(max[2])]);
    let base = vec![[a[0], a[1], a[2]], [b[0], a[1], a[2]], [b[0], a[1], b[2]], [a[0], a[1], b[2]]];
    extrude_faces(&base, &[], [0.0, b[1] - a[1], 0.0], mat, layer)
}

pub fn cylinder_faces(center: P3, axis: P3, radius: f64, height: f64, segments: usize, mat: Option<String>, layer: &str) -> Vec<Face> {
    let n = normalize(axis); let (u, v) = plane_basis(n);
    let pts: Vec<P3> = (0..segments).map(|i| { let a = i as f64 / segments as f64 * std::f64::consts::TAU; add(center, add(scale(u, radius * a.cos()), scale(v, radius * a.sin()))) }).collect();
    extrude_faces(&pts, &[], scale(n, height), mat, layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn group_of(faces: Vec<Face>) -> Entity { Entity::Group(Group { id: new_id(), layer: "l".into(), name: "s".into(), transform: mat_identity(), entities: faces.into_iter().map(Entity::Face).collect(), extra: Default::default() }) }
    fn vol(faces: &[Face]) -> f64 { volume(&polys_of(&group_of(faces.to_vec()), &mat_identity())) }

    #[test]
    fn primitives_are_closed_and_outward() {
        assert!((vol(&box_faces([0.0; 3], [1.0, 2.0, 3.0], None, "l")) - 6.0).abs() < 1e-9);
        let c = vol(&cylinder_faces([0.0; 3], [0.0, 1.0, 0.0], 1.0, 2.0, 64, None, "l"));
        assert!((c - 2.0 * std::f64::consts::PI).abs() < 0.02, "{c}");
        // a square ring: 4x4 minus 2x2, 1 thick
        let outer = vec![[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.0, 0.0, 4.0], [0.0, 0.0, 4.0]];
        let hole = vec![vec![[1.0, 0.0, 1.0], [3.0, 0.0, 1.0], [3.0, 0.0, 3.0], [1.0, 0.0, 3.0]]];
        assert!((vol(&extrude_faces(&outer, &hole, [0.0, 1.0, 0.0], None, "l")) - 12.0).abs() < 1e-9);
    }

    #[test]
    fn subtract_cuts_a_through_slot_into_one_face_with_a_hole() {
        let a = polys_of(&group_of(box_faces([0.0; 3], [0.1, 0.02, 0.05], None, "l")), &mat_identity());
        let b = polys_of(&group_of(box_faces([0.04, -0.01, 0.02], [0.06, 0.03, 0.03], None, "l")), &mat_identity());
        let r = csg(a, b, BoolMode::Subtract);
        let want = 0.1 * 0.02 * 0.05 - 0.02 * 0.02 * 0.01;
        assert!((volume(&r) - want).abs() < 1e-12, "{} vs {want}", volume(&r));
        let faces = merge_faces(r, "l");
        // 6 box faces + 4 slot walls; top and bottom each carry the slot as a hole
        assert_eq!(faces.len(), 10, "{:?}", faces.iter().map(|f| (f.outer.len(), f.holes.len())).collect::<Vec<_>>());
        assert_eq!(faces.iter().filter(|f| f.holes.len() == 1).count(), 2);
        assert!((vol(&faces) - want).abs() < 1e-12);
    }

    #[test]
    fn union_of_touching_boxes_is_one_box() {
        let a = polys_of(&group_of(box_faces([0.0; 3], [1.0, 1.0, 1.0], None, "l")), &mat_identity());
        let b = polys_of(&group_of(box_faces([1.0, 0.0, 0.0], [2.0, 1.0, 1.0], None, "l")), &mat_identity());
        let faces = merge_faces(csg(a, b, BoolMode::Union), "l");
        assert!((vol(&faces) - 2.0).abs() < 1e-9);
        assert_eq!(faces.len(), 6, "the shared wall goes and the sides merge");
    }

    #[test]
    fn intersect_and_overlap() {
        let a = polys_of(&group_of(box_faces([0.0; 3], [2.0, 2.0, 2.0], None, "l")), &mat_identity());
        let b = polys_of(&group_of(box_faces([1.0, 1.0, 1.0], [3.0, 3.0, 3.0], None, "l")), &mat_identity());
        assert!((volume(&csg(a.clone(), b.clone(), BoolMode::Intersect)) - 1.0).abs() < 1e-9);
        assert!((volume(&csg(a, b, BoolMode::Union)) - 15.0).abs() < 1e-9);
    }
}
