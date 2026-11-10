//! Planar arrangements: the regions a set of coplanar segments bounds. This is what makes geometry
//! "sticky": a line drawn across a face splits it, a shape drawn on a face is cut into it, and two
//! faces that lose the edge between them become one.

use std::collections::{BTreeMap, HashSet};

use crate::geom::*;

pub type P2 = [f64; 2];
pub type Seg = (P2, P2);

const TOL: f64 = VERTEX_TOL;

/// A plane with in-plane axes: 3D points to (u, v) and back. Counter-clockwise in (u, v) is
/// counter-clockwise about `n`.
#[derive(Clone, Copy, Debug)]
pub struct Frame { pub o: P3, pub n: P3, u: P3, v: P3 }
impl Frame {
    pub fn new(o: P3, n: P3) -> Frame { let n = normalize(n); let (u, v) = plane_basis(n); Frame { o, n, u, v } }
    /// The plane of a loop, facing the way the loop winds.
    pub fn of(lp: &[P3]) -> Frame { Frame::new(lp[0], normal(lp)) }
    pub fn to2(&self, p: P3) -> P2 { let d = sub(p, self.o); [dot(d, self.u), dot(d, self.v)] }
    pub fn to3(&self, q: P2) -> P3 { add(self.o, add(scale(self.u, q[0]), scale(self.v, q[1]))) }
    pub fn height(&self, p: P3) -> f64 { dot(sub(p, self.o), self.n) }
    pub fn holds(&self, p: P3) -> bool { self.height(p).abs() <= PLANE_TOL }
}

fn sub2(a: P2, b: P2) -> P2 { [a[0] - b[0], a[1] - b[1]] }
fn dot2(a: P2, b: P2) -> f64 { a[0] * b[0] + a[1] * b[1] }
fn cross2(a: P2, b: P2) -> f64 { a[0] * b[1] - a[1] * b[0] }
fn dist2(a: P2, b: P2) -> f64 { let d = sub2(a, b); dot2(d, d).sqrt() }

/// Signed area: positive for a counter-clockwise loop.
pub fn area2(l: &[P2]) -> f64 {
    (0..l.len()).map(|i| cross2(l[i], l[(i + 1) % l.len()])).sum::<f64>() / 2.0
}
/// Even-odd point in polygon.
pub fn inside(p: P2, poly: &[P2]) -> bool {
    let mut c = false;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] { c = !c; }
    }
    c
}
/// Where `p` sits along segment `a`–`b` (0 at a, 1 at b) when it lies on it, ends included.
pub fn on_segment(p: P2, a: P2, b: P2) -> Option<f64> {
    let ab = sub2(b, a); let l2 = dot2(ab, ab); if l2 < TOL * TOL { return None; }
    let t = dot2(sub2(p, a), ab) / l2; let e = TOL / l2.sqrt();
    if t < -e || t > 1.0 + e { return None; }
    if dist2([a[0] + ab[0] * t, a[1] + ab[1] * t], p) > TOL { return None; }
    Some(t.clamp(0.0, 1.0))
}

/// `q` lies on an edge of the region.
pub fn on_boundary(reg: &Region, q: P2) -> bool { reg.segs().iter().any(|(a, b)| on_segment(q, *a, *b).is_some()) }
/// The stretches of the line `p + t·d` (`d` a unit vector) that lie on the region, its edges included: `(t0, t1, on an edge)`,
/// in order, cut at every corner and crossing so that each stretch is either along an edge or inside.
pub fn line_spans(reg: &Region, p: P2, d: P2) -> Vec<(f64, f64, bool)> {
    let mut ts: Vec<f64> = vec![];
    for (a, b) in reg.segs() {
        let e = sub2(b, a); let l = dot2(e, e).sqrt(); if l < TOL { continue; }
        let den = cross2(d, e);
        if den.abs() < 1e-9 * l {
            // parallel: an edge along the line counts with both its ends
            if (cross2(sub2(a, p), d)).abs() < TOL { ts.push(dot2(sub2(a, p), d)); ts.push(dot2(sub2(b, p), d)); }
            continue;
        }
        let s = cross2(sub2(a, p), d) / den;
        if s >= -TOL / l && s <= 1.0 + TOL / l { ts.push(cross2(sub2(a, p), e) / den); }
    }
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ts.dedup_by(|a, b| (*a - *b).abs() < TOL);
    ts.windows(2).filter_map(|w| {
        let m = [p[0] + d[0] * (w[0] + w[1]) / 2.0, p[1] + d[1] * (w[0] + w[1]) / 2.0];
        let edge = on_boundary(reg, m);
        if edge || reg.contains(m) { Some((w[0], w[1], edge)) } else { None }
    }).collect()
}

/// An area of the plane: an outer loop (counter-clockwise) and the holes in it (clockwise).
#[derive(Clone, Debug)]
pub struct Region { pub outer: Vec<P2>, pub holes: Vec<Vec<P2>> }
impl Region {
    pub fn contains(&self, p: P2) -> bool { inside(p, &self.outer) && !self.holes.iter().any(|h| inside(p, h)) }
    pub fn area(&self) -> f64 { area2(&self.outer).abs() - self.holes.iter().map(|h| area2(h).abs()).sum::<f64>() }
    pub fn segs(&self) -> Vec<Seg> {
        std::iter::once(&self.outer).chain(self.holes.iter()).flat_map(|l| (0..l.len()).map(move |i| (l[i], l[(i + 1) % l.len()]))).collect()
    }
    /// A point well inside: the centre of the largest triangle of its triangulation.
    pub fn sample(&self) -> Option<P2> {
        let mut flat: Vec<f64> = self.outer.iter().flat_map(|p| [p[0], p[1]]).collect();
        let mut hole_idx = vec![];
        for h in &self.holes { hole_idx.push(flat.len() / 2); flat.extend(h.iter().flat_map(|p| [p[0], p[1]])); }
        let idx = earcutr::earcut(&flat, &hole_idx, 2).ok()?;
        let pt = |i: usize| [flat[i * 2], flat[i * 2 + 1]];
        idx.chunks(3).filter(|t| t.len() == 3).map(|t| { let (a, b, c) = (pt(t[0]), pt(t[1]), pt(t[2])); (cross2(sub2(b, a), sub2(c, a)).abs(), [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0]) })
            .filter(|(a, _)| *a > 1e-12).max_by(|x, y| x.0.partial_cmp(&y.0).unwrap()).map(|x| x.1)
    }
}

/// The bounded regions of the arrangement of `segs`. Segments are cut where they cross or touch;
/// loose ends and slits (edges with the same region on both sides) bound nothing and are left out.
pub fn regions(segs: &[Seg]) -> Vec<Region> { regions_without(segs, &[]) }

/// As `regions`, with every stretch of a segment that runs along one of `drop` removed first
/// (the edge that was erased between two faces).
pub fn regions_without(segs: &[Seg], drop: &[Seg]) -> Vec<Region> {
    // weld the vertices
    let mut verts: Vec<P2> = vec![];
    fn vid(verts: &mut Vec<P2>, p: P2) -> usize {
        if let Some(i) = verts.iter().position(|q| dist2(*q, p) < TOL) { return i; }
        verts.push(p); verts.len() - 1
    }
    let ends: Vec<(usize, usize)> = segs.iter().map(|(a, b)| (vid(&mut verts, *a), vid(&mut verts, *b))).collect();
    for (a, b) in drop { vid(&mut verts, *a); vid(&mut verts, *b); }
    // crossings become vertices
    for i in 0..segs.len() {
        for j in i + 1..segs.len() {
            let (a, b) = (verts[ends[i].0], verts[ends[i].1]); let (c, d) = (verts[ends[j].0], verts[ends[j].1]);
            let (r, s) = (sub2(b, a), sub2(d, c)); let den = cross2(r, s);
            if den.abs() < 1e-12 { continue; }
            let t = cross2(sub2(c, a), s) / den; let w = cross2(sub2(c, a), r) / den;
            let (et, ew) = (TOL / dot2(r, r).sqrt().max(1e-12), TOL / dot2(s, s).sqrt().max(1e-12));
            if t < -et || t > 1.0 + et || w < -ew || w > 1.0 + ew { continue; }
            vid(&mut verts, [a[0] + r[0] * t, a[1] + r[1] * t]);
        }
    }
    // every segment as a chain through the vertices on it; an edge remembers the segments it came from
    let mut edges: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (si, &(ia, ib)) in ends.iter().enumerate() {
        if ia == ib { continue; }
        let (a, b) = (verts[ia], verts[ib]);
        let mut on: Vec<(f64, usize)> = (0..verts.len()).filter(|k| *k != ia && *k != ib).filter_map(|k| on_segment(verts[k], a, b).map(|t| (t, k))).collect();
        on.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
        let mut chain = vec![ia]; chain.extend(on.into_iter().map(|x| x.1)); chain.push(ib);
        for w in chain.windows(2) {
            if w[0] == w[1] { continue; }
            edges.entry((w[0].min(w[1]), w[0].max(w[1]))).or_default().push(si);
        }
    }
    edges.retain(|&(x, y), _| !drop.iter().any(|(a, b)| on_segment(verts[x], *a, *b).is_some() && on_segment(verts[y], *a, *b).is_some()));

    let cycles = loop {
        // loose ends bound nothing
        loop {
            let mut deg = vec![0usize; verts.len()];
            for &(x, y) in edges.keys() { deg[x] += 1; deg[y] += 1; }
            let n = edges.len();
            edges.retain(|&(x, y), _| deg[x] > 1 && deg[y] > 1);
            if edges.len() == n { break; }
        }
        // neighbours of every vertex, counter-clockwise
        let mut out: Vec<Vec<usize>> = vec![vec![]; verts.len()];
        for &(x, y) in edges.keys() { out[x].push(y); out[y].push(x); }
        for (v, l) in out.iter_mut().enumerate() {
            let ang = |w: usize| { let d = sub2(verts[w], verts[v]); d[1].atan2(d[0]) };
            l.sort_by(|a, b| ang(*a).partial_cmp(&ang(*b)).unwrap());
        }
        // walk every half-edge keeping its region on the left: at each vertex take the first edge clockwise from the way back
        let mut seen: HashSet<(usize, usize)> = HashSet::new();
        let mut cycles: Vec<Vec<usize>> = vec![];
        let mut slits: Vec<(usize, usize)> = vec![];
        for &(x, y) in edges.keys() {
            for (a, b) in [(x, y), (y, x)] {
                if seen.contains(&(a, b)) { continue; }
                let mut cyc = vec![]; let mut own: HashSet<(usize, usize)> = HashSet::new();
                let (mut p, mut c) = (a, b);
                loop {
                    seen.insert((p, c)); own.insert((p, c)); cyc.push(p);
                    let o = &out[c]; let i = o.iter().position(|w| *w == p).unwrap();
                    let nx = o[(i + o.len() - 1) % o.len()];
                    p = c; c = nx;
                    if (p, c) == (a, b) || cyc.len() > 1_000_000 { break; }
                }
                for &(p, c) in &own { if p < c && own.contains(&(c, p)) { slits.push((p, c)); } }
                cycles.push(cyc);
            }
        }
        if slits.is_empty() { break cycles; }
        for s in slits { edges.remove(&s); }
    };

    // corners that came from the segments themselves stay; a vertex that only marks where something
    // pruned touched a straight run goes
    let mut hard = vec![false; verts.len()];
    for (&(x, y), src) in &edges {
        for &s in src { for v in [x, y] { if v == ends[s].0 || v == ends[s].1 { hard[v] = true; } } }
    }
    let tidy = |cyc: &Vec<usize>| -> Vec<P2> {
        let m = cyc.len();
        (0..m).filter(|&i| {
            if hard[cyc[i]] { return true; }
            let (a, b, c) = (verts[cyc[(i + m - 1) % m]], verts[cyc[i]], verts[cyc[(i + 1) % m]]);
            let (d1, d2) = (sub2(b, a), sub2(c, b));
            !(cross2(d1, d2).abs() < 1e-9 * (dot2(d1, d1) * dot2(d2, d2)).sqrt().max(1e-12) && dot2(d1, d2) > 0.0)
        }).map(|i| verts[cyc[i]]).collect()
    };

    // connected components: the clockwise cycle of one is its outside, a hole in the region around it
    let mut comp: Vec<usize> = (0..verts.len()).collect();
    fn find(c: &mut Vec<usize>, i: usize) -> usize { let mut r = i; while c[r] != r { r = c[r]; } let mut i = i; while c[i] != r { let n = c[i]; c[i] = r; i = n; } r }
    for &(x, y) in edges.keys() { let (a, b) = (find(&mut comp, x), find(&mut comp, y)); if a != b { comp[a] = b; } }
    let mut regs: Vec<(Region, usize)> = vec![]; let mut outsides: Vec<(Vec<P2>, usize, P2)> = vec![];
    for cyc in &cycles {
        let raw: Vec<P2> = cyc.iter().map(|i| verts[*i]).collect();
        let a = area2(&raw); let c = find(&mut comp, cyc[0]);
        if a > 1e-10 { regs.push((Region { outer: tidy(cyc), holes: vec![] }, c)); }
        else if a < -1e-10 { outsides.push((tidy(cyc), c, raw[0])); }
    }
    for (lp, c, probe) in outsides {
        let best = regs.iter().enumerate().filter(|(_, (r, rc))| *rc != c && inside(probe, &r.outer))
            .min_by(|x, y| area2(&x.1 .0.outer).partial_cmp(&area2(&y.1 .0.outer)).unwrap()).map(|(i, _)| i);
        if let Some(i) = best { regs[i].0.holes.push(lp); }
    }
    regs.into_iter().map(|(r, _)| r).filter(|r| r.outer.len() >= 3).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Seg> {
        let p = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        (0..4).map(|i| (p[i], p[(i + 1) % 4])).collect()
    }
    fn areas(mut r: Vec<Region>) -> Vec<f64> { r.sort_by(|a, b| a.area().partial_cmp(&b.area()).unwrap()); r.iter().map(|r| (r.area() * 1e6).round() / 1e6).collect() }

    #[test]
    fn a_line_across_a_square_makes_two_regions() {
        let mut s = rect(0.0, 0.0, 4.0, 2.0); s.push(([1.0, 0.0], [1.0, 2.0]));
        assert_eq!(areas(regions(&s)), vec![2.0, 6.0]);
    }
    #[test]
    fn a_line_that_stops_short_splits_nothing() {
        let mut s = rect(0.0, 0.0, 4.0, 2.0); s.push(([1.0, 0.0], [1.0, 1.5]));
        let r = regions(&s);
        assert_eq!(areas(r.clone()), vec![8.0]);
        assert_eq!(r[0].outer.len(), 4, "the touch point is no corner");
    }
    #[test]
    fn an_inner_loop_is_a_region_and_a_hole() {
        let mut s = rect(0.0, 0.0, 4.0, 4.0); s.extend(rect(1.0, 1.0, 2.0, 2.0));
        let r = regions(&s);
        assert_eq!(areas(r.clone()), vec![1.0, 15.0]);
        assert_eq!(r.iter().map(|r| r.holes.len()).sum::<usize>(), 1);
    }
    #[test]
    fn a_bridge_to_an_inner_loop_is_no_boundary() {
        let mut s = rect(0.0, 0.0, 4.0, 4.0); s.extend(rect(1.0, 1.0, 2.0, 2.0)); s.push(([0.0, 1.0], [1.0, 1.0]));
        assert_eq!(areas(regions(&s)), vec![1.0, 15.0]);
    }
    #[test]
    fn overlapping_rectangles_make_three_regions() {
        let mut s = rect(0.0, 0.0, 2.0, 2.0); s.extend(rect(1.0, 1.0, 3.0, 3.0));
        assert_eq!(areas(regions(&s)), vec![1.0, 3.0, 3.0]);
    }
    #[test]
    fn dropping_the_shared_edge_merges_two_squares() {
        let mut s = rect(0.0, 0.0, 1.0, 1.0); s.extend(rect(1.0, 0.0, 2.0, 1.0));
        assert_eq!(areas(regions(&s)), vec![1.0, 1.0]);
        let r = regions_without(&s, &[([1.0, 0.0], [1.0, 1.0])]);
        assert_eq!(areas(r.clone()), vec![2.0]);
        assert_eq!(r[0].outer.len(), 6, "the corners of both squares stay as vertices");
    }
}
