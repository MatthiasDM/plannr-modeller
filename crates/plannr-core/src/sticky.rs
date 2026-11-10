//! Sticky geometry: the faces and loose edges of one container (the top level, or one group) behave as
//! one connected shape where they touch, the way they do in SketchUp.
//!
//! - A line drawn across a face divides it; a shape drawn on a face is cut into it (`imprint_edges`, `insert_face`).
//! - Erasing the edge between two faces of one plane makes them one; erasing any other edge of a face takes
//!   the face with it (`erase_edge`).
//! - Push/Pull on a face of a solid moves that face: the faces standing along the pull stretch
//!   with it, the others get a new side face, and a face it is pushed onto is cut open (`push_pull`).
//!
//! - Moving a face, an edge or a corner takes along everything attached to it; a face that stops being flat
//!   is folded (`stretch`).
//!
//! Faces keep their own outlines (no shared vertices are stored): what touches is found by position.

use crate::geom::*;
use crate::model::*;
use crate::ops::OpResult;
use crate::planar::*;

fn loops(f: &Face) -> impl Iterator<Item = &Vec<P3>> { std::iter::once(&f.outer).chain(f.holes.iter()) }
fn edges_of(f: &Face) -> Vec<(P3, P3)> { loops(f).flat_map(|l| (0..l.len()).map(move |i| (l[i], l[(i + 1) % l.len()]))).collect() }
fn region_in(fr: &Frame, f: &Face) -> Region {
    Region { outer: f.outer.iter().map(|p| fr.to2(*p)).collect(), holes: f.holes.iter().map(|h| h.iter().map(|p| fr.to2(*p)).collect()).collect() }
}
/// The face lies in the frame's plane (facing either way).
fn in_plane(fr: &Frame, f: &Face) -> bool {
    f.outer.len() >= 3 && dot(normal(&f.outer), fr.n).abs() > 1.0 - 1e-6 && loops(f).all(|l| l.iter().all(|p| fr.holds(*p)))
}
type Box2 = (P2, P2);
fn bbox(pts: impl Iterator<Item = P2>) -> Box2 {
    let mut b = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in pts { for i in 0..2 { b.0[i] = b.0[i].min(p[i]); b.1[i] = b.1[i].max(p[i]); } }
    b
}
fn boxes_touch(a: &Box2, b: &Box2) -> bool { (0..2).all(|i| a.0[i] <= b.1[i] + VERTEX_TOL && b.0[i] <= a.1[i] + VERTEX_TOL) }
fn face_of(e: &Entity) -> Option<&Face> { if let Entity::Face(f) = e { Some(f) } else { None } }

/// Back from the plane to 3D: a point that was given in 3D returns exactly as it was, so pieces keep
/// meeting their neighbours to the last digit.
struct Lift { fr: Frame, known: Vec<(P2, P3)> }
impl Lift {
    fn new(fr: Frame, f: &Face) -> Lift { let mut l = Lift { fr, known: vec![] }; for p in loops(f).flatten() { l.know(*p); } l }
    fn know(&mut self, p: P3) { self.known.push((self.fr.to2(p), p)); }
    fn to3(&self, q: P2) -> P3 {
        self.known.iter().find(|(k, _)| (k[0] - q[0]).abs() < VERTEX_TOL && (k[1] - q[1]).abs() < VERTEX_TOL).map(|(_, p)| *p).unwrap_or_else(|| self.fr.to3(q))
    }
    fn face(&self, r: &Region, like: &Face, id: String) -> Face {
        Face { id, outer: r.outer.iter().map(|q| self.to3(*q)).collect(), holes: r.holes.iter().map(|h| h.iter().map(|q| self.to3(*q)).collect()).collect(), ..like.clone() }
    }
}

fn by_area(mut v: Vec<Region>) -> Vec<Region> { v.sort_by(|a, b| b.area().partial_cmp(&a.area()).unwrap()); v }
fn same_area(a: f64, b: f64) -> bool { (a - b).abs() <= 1e-6 * b.abs().max(1.0) }

/// The pieces `cutters` divide a face into, in the face's own frame; none when they don't divide it.
fn split(fr: &Frame, f: &Face, cutters: &[Seg]) -> Vec<Region> {
    let own = region_in(fr, f);
    let mut segs = own.segs(); segs.extend_from_slice(cutters);
    let parts: Vec<Region> = regions(&segs).into_iter().filter(|r| r.sample().map_or(false, |s| own.contains(s))).collect();
    if parts.len() < 2 || !same_area(parts.iter().map(|p| p.area()).sum(), own.area()) { return vec![]; }
    parts
}
/// What of `reg` no region of `covers` lies on (all in one frame); `None` when nothing of it is covered.
fn uncovered(reg: &Region, covers: &[Region]) -> Option<Vec<Region>> {
    if covers.is_empty() { return None; }
    let mut segs = reg.segs(); for c in covers { segs.extend(c.segs()); }
    let free: Vec<Region> = regions(&segs).into_iter().filter(|g| g.sample().map_or(false, |s| reg.contains(s) && !covers.iter().any(|c| c.contains(s)))).collect();
    if same_area(free.iter().map(|p| p.area()).sum(), reg.area()) { None } else { Some(by_area(free)) }
}
fn report(list: &[Entity], r: &mut OpResult) {
    r.entities = r.created.iter().chain(r.modified.iter()).filter_map(|id| list.iter().find(|e| e.id() == id).cloned()).collect();
}

/// New loose edges (already in `list`) divide the faces they cross: with the other loose edges lying on a
/// face, wherever they close off part of it. The largest piece keeps the face's id.
pub fn imprint_edges(list: &mut Vec<Entity>, new: &[(P3, P3)], r: &mut OpResult) {
    let mut added: Vec<Entity> = vec![];
    for i in 0..list.len() {
        let Some(f) = face_of(&list[i]) else { continue };
        if f.outer.len() < 3 { continue; }
        let fr = Frame::of(&f.outer);
        let fb = bbox(f.outer.iter().map(|p| fr.to2(*p)));
        let on_face = |a: P3, b: P3| fr.holds(a) && fr.holds(b) && boxes_touch(&bbox([fr.to2(a), fr.to2(b)].into_iter()), &fb);
        if !new.iter().any(|(a, b)| on_face(*a, *b)) { continue; }
        let mut lift = Lift::new(fr, f);
        let mut cutters: Vec<Seg> = vec![];
        for e in list.iter() { if let Entity::Edge(e) = e { if on_face(e.a, e.b) { lift.know(e.a); lift.know(e.b); cutters.push((fr.to2(e.a), fr.to2(e.b))); } } }
        let parts = by_area(split(&fr, f, &cutters));
        if parts.is_empty() { continue; }
        let f = f.clone();
        for (k, p) in parts.iter().enumerate() {
            if k == 0 { list[i] = Entity::Face(lift.face(p, &f, f.id.clone())); r.modified.push(f.id.clone()); }
            else { let c = lift.face(p, &f, new_id()); r.created.push(c.id.clone()); added.push(Entity::Face(c)); }
        }
    }
    list.extend(added);
    report(list, r);
}

/// Whether the area inside `cycle` already lies on a face of its plane.
pub fn covered(list: &[Entity], cycle: &[P3]) -> bool {
    let fr = Frame::of(cycle);
    let reg = Region { outer: cycle.iter().map(|p| fr.to2(*p)).collect(), holes: vec![] };
    let Some(s) = reg.sample() else { return false };
    list.iter().filter_map(face_of).any(|f| in_plane(&fr, f) && region_in(&fr, f).contains(s))
}

/// Add a face. Where it lies on faces of its plane it is cut into them: each keeps its id for the part
/// outside the new outline, the parts inside become faces of their own (reported first in `created`, with
/// the old face's look unless the new one names a material). What lies on no face is added as drawn.
pub fn insert_face(list: &mut Vec<Entity>, nf: Face, r: &mut OpResult) {
    let fr = Frame::of(&nf.outer);
    let new_reg = region_in(&fr, &nf);
    let nb = bbox(new_reg.outer.iter().copied());
    let cands: Vec<usize> = (0..list.len()).filter(|&i| face_of(&list[i]).map_or(false, |f| in_plane(&fr, f) && boxes_touch(&bbox(f.outer.iter().map(|p| fr.to2(*p))), &nb))).collect();
    let olds: Vec<Face> = cands.iter().map(|&i| face_of(&list[i]).unwrap().clone()).collect();
    let (mut inside_new, mut rest_new): (Vec<Entity>, Vec<Entity>) = (vec![], vec![]);
    for (&i, f) in cands.iter().zip(&olds) {
        let ffr = Frame::of(&f.outer);
        let mut lift = Lift::new(ffr, f);
        for p in loops(&nf).flatten() { lift.know(*p); }
        let cutters: Vec<Seg> = edges_of(&nf).into_iter().map(|(a, b)| (ffr.to2(a), ffr.to2(b))).collect();
        let parts = split(&ffr, f, &cutters);
        if parts.is_empty() { continue; }
        let (ins, out): (Vec<Region>, Vec<Region>) = parts.into_iter().partition(|p| p.sample().map_or(false, |s| new_reg.contains(fr.to2(ffr.to3(s)))));
        let (mut ins, mut out) = (by_area(ins), by_area(out));
        let keep = if out.is_empty() { ins.remove(0) } else { out.remove(0) };
        list[i] = Entity::Face(lift.face(&keep, f, f.id.clone()));
        r.modified.push(f.id.clone());
        for p in out { rest_new.push(Entity::Face(lift.face(&p, f, new_id()))); }
        for p in ins {
            let mut c = lift.face(&p, f, new_id());
            if nf.material.is_some() { c.material = nf.material.clone(); }
            inside_new.push(Entity::Face(c));
        }
    }
    // what of the new shape lies on no face yet
    let covers: Vec<Region> = olds.iter().map(|f| region_in(&fr, f)).collect();
    let mut lift = Lift::new(fr, &nf);
    for f in &olds { for p in loops(f).flatten() { lift.know(*p); } }
    let free: Vec<Entity> = match uncovered(&new_reg, &covers) {
        None => vec![Entity::Face(nf)],
        Some(parts) => parts.iter().enumerate().map(|(k, p)| Entity::Face(lift.face(p, &nf, if k == 0 { nf.id.clone() } else { new_id() }))).collect(),
    };
    for e in inside_new.into_iter().chain(free).chain(rest_new) { r.created.push(e.id().to_string()); list.push(e); }
    report(list, r);
}

/// Intersect Faces: where a face of `ids` crosses another face of the container (through it, or ending on it), a
/// loose edge is drawn along the crossing, and it divides the faces it runs across, as a line drawn there would.
/// An edge two faces already share, or one that is already drawn, is left alone. False when there was nothing to draw.
pub fn intersect(list: &mut Vec<Entity>, ids: &[String], r: &mut OpResult) -> bool {
    let faces: Vec<(&Face, Frame, Region, (P3, P3))> = list.iter().filter_map(face_of).filter(|f| f.outer.len() >= 3)
        .map(|f| { let fr = Frame::of(&f.outer); let reg = region_in(&fr, f); (f, fr, reg, span(f.outer.iter().copied())) }).collect();
    let drawn = |a: P3, b: P3| list.iter().any(|e| matches!(e, Entity::Edge(e) if within(e.a, e.b, a, b)));
    let mut cuts: Vec<(P3, P3, String)> = vec![];
    for x in 0..faces.len() {
        for y in x + 1..faces.len() {
            let ((f, fa, ra, sa), (g, fb, rb, sb)) = (&faces[x], &faces[y]);
            if !(ids.contains(&f.id) || ids.contains(&g.id)) || !spans_touch(sa, sb) { continue; }
            let d = cross(fa.n, fb.n); if len(d) < 1e-6 { continue; } // parallel planes
            let d = normalize(d);
            // a point of the line both planes share (the one nearest the origin)
            let p = scale(add(scale(cross(fb.n, d), dot(fa.n, fa.o)), scale(cross(d, fa.n), dot(fb.n, fb.o))), 1.0 / dot(fa.n, cross(fb.n, d)));
            let dir = |fr: &Frame| fr.to2(add(fr.o, d));
            let (la, lb) = (line_spans(ra, fa.to2(p), dir(fa)), line_spans(rb, fb.to2(p), dir(fb)));
            let mut on: Vec<(f64, f64)> = vec![];
            for (a0, a1, ea) in &la { for (b0, b1, eb) in &lb {
                let (t0, t1) = (a0.max(*b0), a1.min(*b1));
                if t1 - t0 < VERTEX_TOL || (*ea && *eb) { continue; } // (along an edge of both: the edge they share)
                match on.last_mut() { Some(l) if (l.1 - t0).abs() < VERTEX_TOL => l.1 = t1, _ => on.push((t0, t1)) }
            } }
            for (t0, t1) in on {
                let (a, b) = (add(p, scale(d, t0)), add(p, scale(d, t1)));
                if drawn(a, b) || cuts.iter().any(|(p, q, _)| within(*p, *q, a, b)) { continue; }
                cuts.push((a, b, f.layer.clone()));
            }
        }
    }
    drop(faces);
    for (a, b, layer) in &cuts {
        let e = Edge { id: new_id(), layer: layer.clone(), a: *a, b: *b, extra: Default::default() };
        r.created.push(e.id.clone()); list.push(Entity::Edge(e));
    }
    let segs: Vec<(P3, P3)> = cuts.iter().map(|(a, b, _)| (*a, *b)).collect();
    if segs.is_empty() { return false; }
    imprint_edges(list, &segs, r);
    true
}

/// `a`–`b` and `p`–`q` run along the same line and share more than a point.
fn overlap(p: P3, q: P3, a: P3, b: P3) -> bool {
    let l = dist(p, q); if l < VERTEX_TOL { return false; }
    let d = scale(sub(q, p), 1.0 / l);
    let off = |x: P3| { let w = sub(x, p); len(sub(w, scale(d, dot(w, d)))) };
    if off(a) > VERTEX_TOL || off(b) > VERTEX_TOL { return false; }
    let (ta, tb) = (dot(sub(a, p), d), dot(sub(b, p), d));
    ta.max(tb).min(l) - ta.min(tb).max(0.0) > VERTEX_TOL
}
/// `a`–`b` lies on the segment `p`–`q`.
fn within(p: P3, q: P3, a: P3, b: P3) -> bool {
    let l = dist(p, q); if l < VERTEX_TOL || dist(a, b) < VERTEX_TOL { return false; }
    let d = scale(sub(q, p), 1.0 / l);
    let on = |x: P3| { let w = sub(x, p); let t = dot(w, d); t > -VERTEX_TOL && t < l + VERTEX_TOL && len(sub(w, scale(d, t))) < VERTEX_TOL };
    on(a) && on(b)
}

/// A loose edge `a`–`b` was erased. The two faces of one plane it ran between become one (the larger keeps
/// its id); any other face it was an edge of goes with it.
pub fn erase_edge(list: &mut Vec<Entity>, a: P3, b: P3, r: &mut OpResult) {
    let on: Vec<usize> = (0..list.len()).filter(|&i| face_of(&list[i]).map_or(false, |f| edges_of(f).iter().any(|(p, q)| overlap(*p, *q, a, b)))).collect();
    if on.is_empty() { return; }
    if on.len() == 2 {
        let (mut k, mut o) = (on[0], on[1]);
        let area_of = |i: usize| { let f = face_of(&list[i]).unwrap(); area(&f.outer) - f.holes.iter().map(|h| area(h)).sum::<f64>() };
        if area_of(o) > area_of(k) { std::mem::swap(&mut k, &mut o); }
        let (fk, fo) = (face_of(&list[k]).unwrap().clone(), face_of(&list[o]).unwrap().clone());
        let fr = Frame::of(&fk.outer);
        if in_plane(&fr, &fo) {
            let (rk, ro) = (region_in(&fr, &fk), region_in(&fr, &fo));
            let mut segs = rk.segs(); segs.extend(ro.segs());
            let parts: Vec<Region> = regions_without(&segs, &[(fr.to2(a), fr.to2(b))]).into_iter().filter(|g| g.sample().map_or(false, |s| rk.contains(s) || ro.contains(s))).collect();
            if parts.len() == 1 && same_area(parts[0].area(), rk.area() + ro.area()) {
                let mut lift = Lift::new(fr, &fk);
                for p in loops(&fo).flatten() { lift.know(*p); }
                list[k] = Entity::Face(lift.face(&parts[0], &fk, fk.id.clone()));
                r.modified.push(fk.id.clone()); r.removed.push(fo.id.clone());
                list.remove(o);
                report(list, r);
                return;
            }
        }
    }
    for &i in on.iter().rev() { r.removed.push(list[i].id().to_string()); list.remove(i); }
}

/// A loop with the stretch `a`–`b` of its edge `s` moved by `d`: the edge steps out (or in) there.
fn step_edge(lp: &[P3], s: usize, a: P3, b: P3, d: P3) -> Vec<P3> {
    let m = lp.len(); let (p, q) = (lp[s], lp[(s + 1) % m]);
    let (x, y) = if dist(p, a) <= dist(p, b) { (a, b) } else { (b, a) };
    let mut out: Vec<P3> = vec![];
    for i in 0..m {
        out.push(lp[i]);
        if i == s {
            if !same_point(p, x) { out.push(x); }
            out.push(add(x, d)); out.push(add(y, d));
            if !same_point(q, y) { out.push(y); }
        }
    }
    // the old ends of the stretch are corners no more where the outline now runs straight through them or doubles back
    loop {
        let m = out.len(); if m < 3 { break; }
        let drop = (0..m).find(|&i| {
            let (u, v, w) = (out[(i + m - 1) % m], out[i], out[(i + 1) % m]);
            if same_point(v, w) { return true; }
            if !(same_point(v, a) || same_point(v, b)) { return false; }
            let (d1, d2) = (normalize(sub(v, u)), normalize(sub(w, v)));
            len(cross(d1, d2)) < 1e-9
        });
        match drop { Some(i) => { out.remove(i); } None => break }
    }
    out
}
/// No two edges of the loop cross.
fn simple(lp: &[P3]) -> bool {
    let fr = Frame::of(lp); let q: Vec<P2> = lp.iter().map(|p| fr.to2(*p)).collect(); let m = q.len();
    let cr = |a: P2, b: P2, c: P2| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    for i in 0..m {
        for j in i + 1..m {
            if j == i + 1 || (i == 0 && j == m - 1) { continue; }
            let (a, b, c, d) = (q[i], q[(i + 1) % m], q[j], q[(j + 1) % m]);
            if cr(a, b, c) * cr(a, b, d) < -1e-12 && cr(c, d, a) * cr(c, d, b) < -1e-12 { return false; }
        }
    }
    true
}

/// Push/Pull for a face on the surface of a solid: it shares an edge with another face of its container and
/// has the solid behind it. The face moves by `distance` along its normal and keeps its id and its side. Along each of its edges a
/// neighbour that stands in the pull direction is stretched or shortened; elsewhere a side face is added.
/// Where the moved face comes to lie on another face, that face is cut open and the moved face is used up:
/// pushing through a wall leaves a hole. `None` for a face on its own or in a flat drawing (the caller builds a prism).
pub fn push_pull(list: &mut Vec<Entity>, idx: usize, distance: f64) -> Option<OpResult> {
    let mut orig = face_of(&list[idx])?.clone();
    let n = normal(&orig.outer); let d = scale(n, distance);
    let (lo, hi) = orig.outer.iter().fold(([f64::MAX; 3], [f64::MIN; 3]), |(mut lo, mut hi), p| { for i in 0..3 { lo[i] = lo[i].min(p[i]); hi[i] = hi[i].max(p[i]); } (lo, hi) });
    let near = |f: &Face| { let mut t = true; for i in 0..3 { let (a, b) = f.outer.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[i]), b.max(p[i]))); t &= a <= hi[i] + VERTEX_TOL && lo[i] <= b + VERTEX_TOL; } t };
    let others: Vec<usize> = (0..list.len()).filter(|&j| j != idx && face_of(&list[j]).map_or(false, |f| f.outer.len() >= 3 && near(f))).collect();
    // corners of the neighbours along this face's edges become corners of it too, so every stretch of edge has one neighbour
    let corners: Vec<P3> = others.iter().flat_map(|&j| loops(face_of(&list[j]).unwrap()).flatten().copied().collect::<Vec<_>>()).collect();
    let refine = |lp: &Vec<P3>| -> Vec<P3> {
        let m = lp.len(); let mut out = vec![];
        for i in 0..m {
            let (a, b) = (lp[i], lp[(i + 1) % m]); out.push(a);
            let l = dist(a, b); if l < VERTEX_TOL { continue; }
            let dir = scale(sub(b, a), 1.0 / l);
            let mut on: Vec<(f64, P3)> = corners.iter().filter_map(|c| { let w = sub(*c, a); let t = dot(w, dir); (t > VERTEX_TOL && t < l - VERTEX_TOL && len(sub(w, scale(dir, t))) < VERTEX_TOL).then_some((t, *c)) }).collect();
            on.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
            on.dedup_by(|x, y| (x.0 - y.0).abs() < VERTEX_TOL);
            out.extend(on.into_iter().map(|x| x.1));
        }
        out
    };
    orig.outer = refine(&orig.outer); orig.holes = orig.holes.iter().map(refine).collect();
    let edges = edges_of(&orig);
    let across = |a: P3, b: P3, list: &Vec<Entity>| -> Vec<usize> { others.iter().copied().filter(|&j| edges_of(face_of(&list[j]).unwrap()).iter().any(|(p, q)| within(*p, *q, a, b))).collect() };
    if edges.iter().all(|(a, b)| across(*a, *b, list).is_empty()) { return None; }
    // only a face on the surface of a solid moves with it: one that has material behind it. A face of a flat
    // drawing (a room of a floor plan, a rectangle beside a box) is pulled into a prism of its own instead.
    let fr0 = Frame::of(&orig.outer);
    let from = fr0.to3(region_in(&fr0, &orig).sample()?);
    let behind = list.iter().enumerate().filter(|(j, e)| *j != idx && face_of(e).map_or(false, |g| {
        if g.outer.len() < 3 { return false; }
        let gfr = Frame::of(&g.outer); let along = dot(gfr.n, n);
        if along.abs() < 1e-9 { return false; }
        let t = gfr.height(from) / along; // the ray from `from` along -n meets g's plane at from - t·n
        t > 1e-6 && region_in(&gfr, g).contains(gfr.to2(sub(from, scale(n, t))))
    })).count();
    if behind % 2 == 0 { return None; }

    let mut r = OpResult::default();
    let mut sides: Vec<Entity> = vec![];
    let mut gone: Vec<String> = vec![];
    for (a, b) in &edges {
        let mut stretched = false;
        for j in across(*a, *b, list) {
            let g = face_of(&list[j]).unwrap();
            if gone.contains(&g.id) || dot(normal(&g.outer), n).abs() > 1e-6 { continue; } // only a face standing along the pull follows it
            let mut g = g.clone();
            let Some((li, s)) = loops(&g).enumerate().find_map(|(li, l)| (0..l.len()).find(|&s| within(l[s], l[(s + 1) % l.len()], *a, *b)).map(|s| (li, s))) else { continue };
            let old = if li == 0 { g.outer.clone() } else { g.holes[li - 1].clone() };
            let new = step_edge(&old, s, *a, *b, d);
            if new.len() < 3 || area(&new) < 1e-8 {
                // squeezed to nothing
                if li == 0 { gone.push(g.id.clone()); } else { g.holes.remove(li - 1); list[j] = Entity::Face(g.clone()); if !r.modified.contains(&g.id) { r.modified.push(g.id.clone()); } }
                stretched = true; break;
            }
            if dot(newell(&new), newell(&old)) <= 0.0 || !simple(&new) { continue; } // pushed past its far side: leave it alone
            if li == 0 { g.outer = new; } else { g.holes[li - 1] = new; }
            if !r.modified.contains(&g.id) { r.modified.push(g.id.clone()); }
            list[j] = Entity::Face(g);
            stretched = true; break;
        }
        if !stretched {
            let f = Face { id: new_id(), layer: orig.layer.clone(), outer: vec![*a, *b, add(*b, d), add(*a, d)], holes: vec![], material: orig.material.clone(), back_material: orig.back_material.clone(), extra: Default::default() };
            r.created.push(f.id.clone());
            sides.push(Entity::Face(f));
        }
    }
    let shift = |l: &Vec<P3>| l.iter().map(|p| add(*p, d)).collect::<Vec<P3>>();
    let cap = Face { outer: shift(&orig.outer), holes: orig.holes.iter().map(shift).collect(), ..orig.clone() };

    // pushed onto another face: that face is cut open there, and the part of the moved face lying on it is used up
    let fr = Frame::of(&cap.outer);
    let cap_reg = region_in(&fr, &cap);
    let mut covers: Vec<Region> = vec![]; let mut extra: Vec<Entity> = vec![]; let mut known: Vec<P3> = vec![];
    for j in 0..list.len() {
        if j == idx { continue; }
        let Some(g) = face_of(&list[j]) else { continue };
        if gone.contains(&g.id) || !in_plane(&fr, g) { continue; }
        let g = g.clone(); let gfr = Frame::of(&g.outer);
        let mut lift = Lift::new(gfr, &g);
        for p in loops(&cap).flatten() { lift.know(*p); }
        let cutters: Vec<Seg> = edges_of(&cap).into_iter().map(|(a, b)| (gfr.to2(a), gfr.to2(b))).collect();
        let under = |p: &Region| p.sample().map_or(false, |s| cap_reg.contains(fr.to2(gfr.to3(s))));
        let parts = split(&gfr, &g, &cutters);
        if parts.is_empty() {
            if under(&region_in(&gfr, &g)) { covers.push(region_in(&fr, &g)); known.extend(loops(&g).flatten()); gone.push(g.id.clone()); }
            continue;
        }
        let (ins, out): (Vec<Region>, Vec<Region>) = parts.into_iter().partition(under);
        if ins.is_empty() { continue; }
        covers.push(region_in(&fr, &g)); known.extend(loops(&g).flatten());
        let mut out = by_area(out);
        if out.is_empty() { gone.push(g.id.clone()); continue; }
        list[j] = Entity::Face(lift.face(&out.remove(0), &g, g.id.clone()));
        if !r.modified.contains(&g.id) { r.modified.push(g.id.clone()); }
        for p in out { let c = lift.face(&p, &g, new_id()); r.created.push(c.id.clone()); extra.push(Entity::Face(c)); }
    }
    let cap_id = cap.id.clone();
    match uncovered(&cap_reg, &covers) {
        None => { list[idx] = Entity::Face(cap); r.modified.insert(0, cap_id); }
        Some(parts) if parts.is_empty() => gone.push(cap_id),
        Some(parts) => {
            let mut lift = Lift::new(fr, &cap); for p in known { lift.know(p); }
            for (k, p) in parts.iter().enumerate() {
                if k == 0 { list[idx] = Entity::Face(lift.face(p, &cap, cap_id.clone())); r.modified.insert(0, cap_id.clone()); }
                else { let c = lift.face(p, &cap, new_id()); r.created.push(c.id.clone()); extra.push(Entity::Face(c)); }
            }
        }
    }
    list.extend(sides); list.extend(extra);
    list.retain(|e| !gone.iter().any(|g| g == e.id()));
    r.modified.retain(|m| !gone.contains(m));
    r.removed = gone;
    report(list, &mut r);
    Some(r)
}

/// Points to look up by position.
struct Near { cells: std::collections::HashMap<(i64, i64, i64), Vec<P3>> }
impl Near {
    const CELL: f64 = 1e-2;
    fn cell(p: P3) -> (i64, i64, i64) { ((p[0] / Self::CELL).floor() as i64, (p[1] / Self::CELL).floor() as i64, (p[2] / Self::CELL).floor() as i64) }
    fn new() -> Near { Near { cells: Default::default() } }
    fn has(&self, p: P3) -> bool {
        let c = Self::cell(p);
        for dx in -1..=1 { for dy in -1..=1 { for dz in -1..=1 {
            if self.cells.get(&(c.0 + dx, c.1 + dy, c.2 + dz)).map_or(false, |v| v.iter().any(|q| same_point(*q, p))) { return true; }
        } } }
        false
    }
    fn add(&mut self, p: P3) { if !self.has(p) { self.cells.entry(Self::cell(p)).or_default().push(p); } }
    fn all(&self) -> Vec<P3> { self.cells.values().flatten().copied().collect() }
}
/// `p` lies on the stretch between `a` and `b`, away from both ends.
fn strictly_on(p: P3, a: P3, b: P3) -> Option<f64> {
    let l = dist(a, b); if l < VERTEX_TOL { return None; }
    let dir = scale(sub(b, a), 1.0 / l); let w = sub(p, a); let t = dot(w, dir);
    (t > VERTEX_TOL && t < l - VERTEX_TOL && len(sub(w, scale(dir, t))) < VERTEX_TOL).then_some(t)
}
fn span(pts: impl Iterator<Item = P3>) -> (P3, P3) {
    pts.fold(([f64::MAX; 3], [f64::MIN; 3]), |(mut lo, mut hi), p| { for i in 0..3 { lo[i] = lo[i].min(p[i]); hi[i] = hi[i].max(p[i]); } (lo, hi) })
}
fn spans_touch(a: &(P3, P3), b: &(P3, P3)) -> bool { (0..3).all(|i| a.0[i] <= b.1[i] + VERTEX_TOL && b.0[i] <= a.1[i] + VERTEX_TOL) }

/// Take the corners at `points`, and every corner of the faces and loose edges `ids`, through `m` (a move, a
/// rotation or a scale, in the container's coordinates), with all
/// that is attached to them in the container: every face and loose edge with a corner there follows with
/// that corner. Faces that move as a whole keep their shape; a face that only partly moves is reshaped, and
/// where that leaves it no longer flat it is folded into flat pieces (the largest keeps its id). A corner
/// that sits in the middle of another face's edge becomes a corner of that face first.
pub fn stretch(list: &mut Vec<Entity>, ids: &[String], points: &[P3], m: &Mat4, r: &mut OpResult) {
    let flip = mat_det3(m) < 0.0; // a mirroring scale: faces that go as a whole keep facing outward
    let mut moving = Near::new();
    for p in points { moving.add(*p); }
    let whole: Vec<usize> = (0..list.len()).filter(|&i| ids.iter().any(|id| id == list[i].id())).collect();
    for &i in &whole {
        match &list[i] {
            Entity::Face(f) => {
                for p in loops(f).flatten() { moving.add(*p); }
                // what stands on the face's outline goes with it
                let fs = span(f.outer.iter().copied());
                let edges = edges_of(f);
                for (j, e) in list.iter().enumerate() {
                    if j == i { continue; }
                    let pts: Vec<P3> = match e { Entity::Face(g) => loops(g).flatten().copied().collect(), Entity::Edge(g) => vec![g.a, g.b], _ => continue };
                    if !spans_touch(&span(pts.iter().copied()), &fs) { continue; }
                    for p in pts { if edges.iter().any(|(a, b)| strictly_on(p, *a, *b).is_some()) { moving.add(p); } }
                }
            }
            Entity::Edge(e) => { moving.add(e.a); moving.add(e.b); }
            _ => {}
        }
    }
    let all = moving.all();
    if all.is_empty() { return; }
    let ms = span(all.iter().copied());
    let mut added: Vec<Entity> = vec![]; let mut gone: Vec<String> = vec![];
    for i in 0..list.len() {
        match &list[i] {
            Entity::Face(f) => {
                if f.outer.len() < 3 || !spans_touch(&span(f.outer.iter().copied()), &ms) { continue; }
                // a moving corner in the middle of one of this face's edges is a corner of it from now on
                let refine = |lp: &Vec<P3>| -> Vec<P3> {
                    let m = lp.len(); let mut out = vec![];
                    for k in 0..m {
                        let (a, b) = (lp[k], lp[(k + 1) % m]); out.push(a);
                        let mut on: Vec<(f64, P3)> = all.iter().filter_map(|p| strictly_on(*p, a, b).map(|t| (t, *p))).collect();
                        on.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
                        out.extend(on.into_iter().map(|x| x.1));
                    }
                    out
                };
                let (outer, holes): (Vec<P3>, Vec<Vec<P3>>) = if all.len() <= 256 { (refine(&f.outer), f.holes.iter().map(refine).collect()) } else { (f.outer.clone(), f.holes.clone()) };
                let total = outer.len() + holes.iter().map(|h| h.len()).sum::<usize>();
                let moved = outer.iter().chain(holes.iter().flatten()).filter(|p| moving.has(**p)).count();
                if moved == 0 { continue; }
                let shift = |l: &Vec<P3>| l.iter().map(|p| if moving.has(*p) { mat_apply(m, *p) } else { *p }).collect::<Vec<P3>>();
                let (new_outer, new_holes) = (shift(&outer), holes.iter().map(shift).collect::<Vec<_>>());
                let f = f.clone();
                if moved == total {
                    let mut whole = Entity::Face(Face { outer: new_outer, holes: new_holes, ..f.clone() });
                    if flip { whole.reverse_winding(); }
                    list[i] = whole;
                    r.modified.push(f.id.clone());
                    continue;
                }
                let tidy = |l: &Vec<P3>| { let mut o: Vec<P3> = vec![]; for p in l { if o.last().map_or(true, |q| !same_point(*q, *p)) { o.push(*p); } } while o.len() > 1 && same_point(o[0], *o.last().unwrap()) { o.pop(); } o };
                let flat_outer = tidy(&new_outer);
                if flat_outer.len() < 3 || area(&flat_outer) < 1e-8 { gone.push(f.id.clone()); continue; }
                let n = normal(&flat_outer);
                let flat_holes: Vec<Vec<P3>> = new_holes.iter().map(tidy).filter(|h| h.len() >= 3 && area(h) > 1e-8).collect();
                if is_planar(&flat_outer) && flat_holes.iter().all(|h| plane_deviation(h, flat_outer[0], n) <= PLANE_TOL) {
                    list[i] = Entity::Face(Face { outer: flat_outer, holes: flat_holes, ..f.clone() });
                    r.modified.push(f.id.clone());
                    continue;
                }
                // no longer flat: fold it. Triangles of the face as it was, taken to where the corners are now; those
                // that still lie in one plane join up again
                let tri = tessellate(&outer, &holes);
                let now: Vec<P3> = new_outer.iter().chain(new_holes.iter().flatten()).copied().collect();
                let polys: Vec<crate::solid::Poly> = tri.chunks(3).filter(|t| t.len() == 3).filter_map(|t| {
                    let p = vec![now[t[0] as usize], now[t[1] as usize], now[t[2] as usize]];
                    if area(&p) < 1e-10 { None } else { crate::solid::Poly::new(p, f.material.clone()) }
                }).collect();
                let mut parts = crate::solid::merge_faces(polys, &f.layer);
                if parts.is_empty() { gone.push(f.id.clone()); continue; }
                parts.sort_by(|a, b| (area(&b.outer)).partial_cmp(&area(&a.outer)).unwrap());
                for (k, p) in parts.into_iter().enumerate() {
                    let c = Face { id: if k == 0 { f.id.clone() } else { new_id() }, outer: p.outer, holes: p.holes, ..f.clone() };
                    if k == 0 { r.modified.push(c.id.clone()); list[i] = Entity::Face(c); } else { r.created.push(c.id.clone()); added.push(Entity::Face(c)); }
                }
            }
            Entity::Edge(e) => {
                let (ma, mb) = (moving.has(e.a), moving.has(e.b));
                if !ma && !mb { continue; }
                let mut e = e.clone();
                if ma && mb {
                    let mut x = Entity::Edge(e.clone()); x.transform(m); // (an arc segment takes its centre along)
                    r.modified.push(e.id.clone()); list[i] = x; continue;
                }
                if ma { e.a = mat_apply(m, e.a); } else { e.b = mat_apply(m, e.b); }
                e.extra.remove("arc"); // one end moved: no longer on its circle
                if same_point(e.a, e.b) { gone.push(e.id.clone()); continue; }
                r.modified.push(e.id.clone()); list[i] = Entity::Edge(e);
            }
            _ => {}
        }
    }
    list.extend(added);
    list.retain(|e| !gone.iter().any(|g| g == e.id()));
    r.removed.extend(gone);
    report(list, r);
}
