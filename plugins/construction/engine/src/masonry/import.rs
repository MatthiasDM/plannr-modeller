//! Detail the construction model in masonry: its wall pieces become masonry walls (exterior → cavity
//! walls with the construction cavity build-up, interior → single leaves), the floors of the upper
//! storeys become beam-and-block floors over the outline of their rooms, and optionally a strip
//! foundation goes under the ground floor.

use plannr_core::model::{Document, Entity};
use plannr_core::ops::{OpError, OpResult};
use crate::expand as cexp;
use serde_json::{json, Value};

use crate::masonry::data::*;
use crate::masonry::ops::{push_entity, validate_wall};

fn flag(data: &Value, k: &str, d: bool) -> bool { data.get(k).and_then(|v| v.as_bool()).unwrap_or(d) }

pub fn import_construction(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let d = settings(doc).defaults;
    let levels: Option<Vec<usize>> = data.get("levels").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64().map(|x| x as usize)).collect());
    let want = |l: usize| levels.as_ref().map(|v| v.contains(&l)).unwrap_or(true);
    // `only: [{ owner, key }]`: customise just these wall pieces (no floors, nothing removed)
    let only: Option<Vec<(String, String)>> = data.get("only").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| Some((x.get("owner")?.as_str()?.to_string(), x.get("key")?.as_str()?.to_string()))).collect());
    let mut removed = vec![];
    if only.is_none() && flag(data, "replace", true) {
        doc.entities.retain(|e| match e {
            Entity::Custom(c) if TYPES.contains(&c.type_.as_str()) && c.data.get("imported").and_then(|v| v.as_bool()).unwrap_or(false) && want(c.data.get("level").and_then(|v| v.as_u64()).unwrap_or(0) as usize) => { removed.push(c.id.clone()); false }
            _ => true,
        });
    }
    let m = cexp::model(doc);
    if m.rooms.is_empty() && m.walls.is_empty() { return Err(OpError("there is no construction model to detail (no rooms or walls)".into())); }
    let set = cexp::collect(&m);
    let mut created = vec![];
    // ---- walls: one per piece of the construction wall graph (the same masonry the model derives)
    if flag(data, "walls", true) {
        for w in set.walls.iter().filter(|w| want(w.level) && !cexp::is_detailed(w, &m.detailed_walls)) {
            if let Some(o) = &only { if !o.iter().any(|(ow, k)| ow == &w.owner && k == &w.key) { continue; } }
            let Some(wall) = crate::masonry::derive::piece_wall(w, &m.settings, &d) else { continue };
            if validate_wall(&wall).is_err() { continue; }
            let mut v = serde_json::to_value(&wall).unwrap(); v["imported"] = json!(true);
            created.extend(push_entity(doc, T_WALL, v)?.created);
        }
    }
    // ---- floors: the structural floor of every upper storey, over the outline of its rooms
    if only.is_none() && flag(data, "floors", true) {
        for f in crate::masonry::derive::derived_floors(&m, &d, &[]).into_iter().filter(|f| want(f.floor.level)) {
            if m.detailed_floor_outlines.iter().any(|(l, _)| *l == f.floor.level) { continue; }
            let mut v = serde_json::to_value(&f.floor).unwrap(); v["imported"] = json!(true);
            created.extend(push_entity(doc, T_FLOOR, v)?.created);
        }
    }
    // ---- foundation under the ground-floor outline
    if only.is_none() && flag(data, "foundation", false) && want(0) {
        let polys: Vec<Vec<P2>> = m.rooms.iter().filter(|(_, r)| r.level == 0).map(|(_, r)| r.points.clone()).collect();
        for outline in union_outline(&polys) {
            let v = json!({ "points": outline, "level": 0, "imported": true });
            created.extend(push_entity(doc, T_FOUNDATION, v)?.created);
        }
    }
    if created.is_empty() { return Err(OpError("nothing to import on those storeys".into())); }
    Ok(OpResult { created, removed, ..Default::default() })
}

/// Outline(s) of the union of plan polygons. Rectilinear polygons are merged exactly on a grid of
/// their coordinates (edge cancellation); others are returned as they are.
pub fn union_outline(polys: &[Vec<P2>]) -> Vec<Vec<P2>> {
    const E: f64 = 1e-6;
    let rectilinear = |p: &Vec<P2>| p.iter().enumerate().all(|(i, a)| { let b = p[(i + 1) % p.len()]; (a[0] - b[0]).abs() < 1e-4 || (a[1] - b[1]).abs() < 1e-4 });
    let (rect, other): (Vec<&Vec<P2>>, Vec<&Vec<P2>>) = polys.iter().filter(|p| p.len() >= 3).partition(|p| rectilinear(p));
    let mut out: Vec<Vec<P2>> = other.into_iter().cloned().collect();
    if rect.is_empty() { return out; }
    let mut xs: Vec<f64> = rect.iter().flat_map(|p| p.iter().map(|q| q[0])).collect();
    let mut zs: Vec<f64> = rect.iter().flat_map(|p| p.iter().map(|q| q[1])).collect();
    let snap = |v: &mut Vec<f64>| { v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v.dedup_by(|a, b| (*a - *b).abs() < 1e-3); };
    snap(&mut xs); snap(&mut zs);
    let (nx, nz) = (xs.len().saturating_sub(1), zs.len().saturating_sub(1));
    let filled = |i: usize, j: usize| { let c = [(xs[i] + xs[i + 1]) / 2.0, (zs[j] + zs[j + 1]) / 2.0]; rect.iter().any(|p| point_in_poly(c, p)) };
    // directed boundary edges (counter-clockwise per cell in x/z), opposite pairs cancel
    let mut edges: Vec<((usize, usize), (usize, usize))> = vec![];
    for i in 0..nx { for j in 0..nz {
        if !filled(i, j) { continue; }
        let cell = [((i, j), (i + 1, j)), ((i + 1, j), (i + 1, j + 1)), ((i + 1, j + 1), (i, j + 1)), ((i, j + 1), (i, j))];
        for e in cell { if let Some(k) = edges.iter().position(|x| x.0 == e.1 && x.1 == e.0) { edges.swap_remove(k); } else { edges.push(e); } }
    } }
    // chain into loops
    while let Some(first) = edges.pop() {
        let mut lp = vec![first.0]; let mut cur = first.1;
        while cur != first.0 {
            lp.push(cur);
            let Some(k) = edges.iter().position(|e| e.0 == cur) else { break };
            cur = edges.swap_remove(k).1;
        }
        // drop collinear vertices
        let pts: Vec<P2> = lp.iter().map(|(i, j)| [xs[*i], zs[*j]]).collect();
        let n = pts.len();
        let simple: Vec<P2> = (0..n).filter(|&k| {
            let (a, b, c) = (pts[(k + n - 1) % n], pts[k], pts[(k + 1) % n]);
            ((b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])).abs() > E
        }).map(|k| pts[k]).collect();
        if simple.len() >= 3 && plan_area(&simple).abs() > 1e-6 { out.push(simple); }
    }
    out
}

/// Offset a simple polygon outward by `d` (negative = inward), mitring the corners.
pub fn offset_polygon(p: &[P2], d: f64) -> Vec<P2> {
    let n = p.len(); if n < 3 || d.abs() < 1e-9 { return p.to_vec(); }
    let ccw = plan_area(p) > 0.0;
    // outward normal of edge a→b: for a counter-clockwise polygon (x/z) it is the right-hand normal [dz, −dx]
    let normal = |a: P2, b: P2| { let l = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt().max(1e-12); let r = [(b[1] - a[1]) / l, -(b[0] - a[0]) / l]; if ccw { r } else { [-r[0], -r[1]] } };
    (0..n).map(|i| {
        let (pv, c, nx) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
        let (n1, n2) = (normal(pv, c), normal(c, nx));
        let bis = [n1[0] + n2[0], n1[1] + n2[1]];
        let dotp = bis[0] * n1[0] + bis[1] * n1[1];
        if dotp.abs() < 1e-9 { return [c[0] + n1[0] * d, c[1] + n1[1] * d]; }
        [c[0] + bis[0] * d / dotp, c[1] + bis[1] * d / dotp]
    }).collect()
}
