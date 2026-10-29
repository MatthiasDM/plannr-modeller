//! A 2D slice of the model at a section plane (SketchUp's "Create Group from Slice"): every face —
//! plugin geometry included — is cut by the plane, and the cut lines become plain edges in a new group
//! lying in the plane. Pieces of one line are merged; where two solids of the same material touch, the
//! joint between them is left out (a wall built of several pieces reads as one outline), while the line
//! between different materials (brick against insulation) stays.

use std::collections::HashMap;

use crate::geom::*;
use crate::model::*;
use crate::ops::{OpError, OpResult};
use crate::plugin::Registry;

const EPS: f64 = 1e-9;

/// One cut through a face: an interval on a line in the plane, the face's material, and on which side
/// of the line (within the plane) the face looks.
struct Cut { key: LineKey, d: P3, base: P3, t: [f64; 2], material: String, side: bool }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LineKey([i64; 6]);

fn q(x: f64) -> i64 { (x * 1e5).round() as i64 }

/// The world-space faces of the (expanded) document, skipping hidden layers.
fn world_faces(doc: &Document) -> Vec<(Vec<Vec<P3>>, String)> {
    let hidden: Vec<&str> = doc.layers.iter().filter(|l| !l.visible).map(|l| l.id.as_str()).collect();
    let mut out = vec![];
    fn walk(doc: &Document, list: &[Entity], m: Mat4, hidden: &[&str], out: &mut Vec<(Vec<Vec<P3>>, String)>, depth: usize) {
        if depth > 16 { return; }
        for e in list {
            if hidden.contains(&e.layer()) { continue; }
            match e {
                Entity::Face(f) => {
                    let mut loops = vec![f.outer.iter().map(|p| mat_apply(&m, *p)).collect::<Vec<_>>()];
                    loops.extend(f.holes.iter().map(|h| h.iter().map(|p| mat_apply(&m, *p)).collect::<Vec<_>>()));
                    out.push((loops, f.material.clone().unwrap_or_default()));
                }
                Entity::Group(g) => walk(doc, &g.entities, mat_mul(&m, &g.transform), hidden, out, depth + 1),
                Entity::Instance(i) => {
                    if let Some(def) = doc.components.iter().find(|c| c.id == i.definition) {
                        walk(doc, &def.entities, mat_mul(&m, &i.transform), hidden, out, depth + 1);
                    }
                }
                _ => {}
            }
        }
    }
    walk(doc, &doc.entities, mat_identity(), &hidden, &mut out, 0);
    out
}

/// Cut one face (outer loop + holes, world coordinates) by the plane (o, n).
fn cut_face(loops: &[Vec<P3>], material: &str, o: P3, n: P3) -> Option<Vec<Cut>> {
    let outer = loops.first()?;
    if outer.len() < 3 { return None; }
    let fnrm = normalize(newell(outer));
    let dir = cross(fnrm, n);
    if len(dir) < 1e-6 { return None; } // parallel to the plane: nothing to cut
    // a canonical direction, so both faces of a joint land on the same line with the same parameter
    let mut d = normalize(dir);
    let first = d.iter().position(|c| c.abs() > 1e-9).unwrap_or(0);
    if d[first] < 0.0 { d = scale(d, -1.0); }
    // the crossings of every loop's edges with the plane (half-open: a vertex on the plane counts as below)
    let s = |p: P3| { let v = dot(sub(p, o), n); if v.abs() < EPS { 0.0 } else { v } };
    let mut hits: Vec<(f64, P3)> = vec![];
    for lp in loops {
        let k = lp.len();
        for i in 0..k {
            let (a, b) = (lp[i], lp[(i + 1) % k]);
            let (sa, sb) = (s(a), s(b));
            if (sa > 0.0) == (sb > 0.0) { continue; }
            let p = add(a, scale(sub(b, a), sa / (sa - sb)));
            hits.push((dot(p, d), p));
        }
    }
    if hits.len() < 2 { return None; }
    hits.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    // the line: its foot from the world origin, and which side of it (within the plane) the face looks
    let base = sub(hits[0].1, scale(d, hits[0].0));
    let key = LineKey([q(d[0]), q(d[1]), q(d[2]), q(base[0]), q(base[1]), q(base[2])]);
    let side = dot(fnrm, cross(n, d)) > 0.0;
    let mut cuts = vec![];
    for pair in hits.chunks(2) {
        if pair.len() < 2 { break; }
        let (t0, t1) = (pair[0].0, pair[1].0);
        if t1 - t0 > 1e-6 { cuts.push(Cut { key, d, base, t: [t0, t1], material: material.to_string(), side }); }
    }
    Some(cuts)
}

/// Union of intervals.
fn union(mut v: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    v.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
    let mut out: Vec<[f64; 2]> = vec![];
    for iv in v {
        match out.last_mut() {
            Some(last) if iv[0] <= last[1] + 1e-6 => last[1] = last[1].max(iv[1]),
            _ => out.push(iv),
        }
    }
    out
}

/// Where exactly one of the two interval sets covers the line.
fn xor(a: &[[f64; 2]], b: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut ts: Vec<f64> = a.iter().chain(b).flat_map(|x| [x[0], x[1]]).collect();
    ts.sort_by(|x, y| x.partial_cmp(y).unwrap());
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-7);
    let inside = |set: &[[f64; 2]], t: f64| set.iter().any(|x| t > x[0] && t < x[1]);
    let mut out = vec![];
    for w in ts.windows(2) {
        let m = (w[0] + w[1]) / 2.0;
        if inside(a, m) != inside(b, m) { out.push([w[0], w[1]]); }
    }
    union(out)
}

/// The cut lines of the document at the plane (o, n), as segments in world coordinates.
pub fn slice_segments(doc: &Document, reg: &Registry, o: P3, n: P3) -> Vec<[P3; 2]> {
    let expanded = reg.expand_all(doc);
    let n = normalize(n);
    let mut lines: HashMap<LineKey, (P3, P3, HashMap<String, (Vec<[f64; 2]>, Vec<[f64; 2]>)>)> = HashMap::new();
    for (loops, material) in world_faces(&expanded) {
        for c in cut_face(&loops, &material, o, n).unwrap_or_default() {
            let entry = lines.entry(c.key).or_insert_with(|| (c.d, c.base, HashMap::new()));
            let sides = entry.2.entry(c.material).or_default();
            if c.side { sides.0.push(c.t) } else { sides.1.push(c.t) }
        }
    }
    let mut out = vec![];
    for (_, (d, base, mats)) in lines {
        // per material, the joints between touching solids cancel; then all materials together
        let mut keep: Vec<[f64; 2]> = vec![];
        for (_, (plus, minus)) in mats { keep.extend(xor(&union(plus), &union(minus))); }
        for iv in union(keep) {
            if iv[1] - iv[0] < 1e-5 { continue; }
            out.push([add(base, scale(d, iv[0])), add(base, scale(d, iv[1]))]);
        }
    }
    // a stable order (top to bottom, then along x/z) keeps ids and diffs predictable
    out.sort_by(|a, b| (q(a[0][1]), q(a[0][0]), q(a[0][2]), q(a[1][0])).cmp(&(q(b[0][1]), q(b[0][0]), q(b[0][2]), q(b[1][0]))));
    out
}

/// `slice_section`: the slice at section plane `id` as a new group of edges.
pub fn slice_section(doc: &mut Document, reg: &Registry, id: &str, layer: Option<&str>, name: Option<&str>) -> Result<OpResult, OpError> {
    // the plane in world coordinates (a section may sit inside a group)
    let mut plane = None;
    doc.visit(|v| { if let Entity::Section(s) = v.entity { if s.id == id { plane = Some((mat_apply(&v.transform, s.origin), normalize(mat_apply_dir(&v.transform, s.normal)))); } } });
    let Some((o, n)) = plane else { return Err(OpError(format!("no section plane {id}"))) };
    let segs = slice_segments(doc, reg, o, n);
    if segs.is_empty() { return Err(OpError("the section plane cuts nothing".into())); }
    // on the given tag, else on a "Slices" tag (made when missing)
    let layer = match layer.filter(|l| !l.is_empty()) {
        Some(r) => doc.layer_by_ref(r).map(|l| l.id.clone()).ok_or_else(|| OpError(format!("no layer {r}")))?,
        None => match doc.layers.iter().find(|l| l.name == "Slices") {
            Some(l) => l.id.clone(),
            None => { let id = new_id(); doc.layers.push(Layer { id: id.clone(), name: "Slices".into(), visible: true, color: "#c8169a".into() }); id }
        },
    };
    let count = doc.entities.iter().filter(|e| matches!(e, Entity::Group(g) if g.name.starts_with("Slice"))).count();
    let name = name.map(|s| s.to_string()).unwrap_or_else(|| format!("Slice {}", count + 1));
    let edges: Vec<Entity> = segs.into_iter().map(|[a, b]| Entity::Edge(Edge { id: new_id(), layer: layer.clone(), a, b, extra: Default::default() })).collect();
    let g = Entity::Group(Group { id: new_id(), layer, name, transform: mat_identity(), entities: edges, extra: Default::default() });
    let gid = g.id().to_string();
    doc.entities.push(g.clone());
    Ok(OpResult { created: vec![gid], entities: vec![g], ..Default::default() })
}
