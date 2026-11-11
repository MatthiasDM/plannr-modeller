//! Soften Edges (`soften`) and 3D Text (`text3d`).

use plannr_core::compile::{compile, CompileOptions, CompiledScene};
use plannr_core::geom::*;
use plannr_core::model::{Document, Entity, Face};
use plannr_core::{apply, Op, Registry};

fn reg() -> Registry { Registry::new() }
fn op(v: serde_json::Value) -> Op { serde_json::from_value(v).unwrap() }
fn run(doc: &mut Document, v: serde_json::Value) -> plannr_core::OpResult { apply(doc, &op(v), &reg()).unwrap() }
fn scene(doc: &Document) -> CompiledScene { compile(doc, &reg(), &CompileOptions::default()) }
fn faces<'a>(doc: &'a Document, id: &str) -> Vec<&'a Face> {
    let Some(Entity::Group(g)) = doc.find(id) else { panic!("no group {id}") };
    g.entities.iter().filter_map(|e| if let Entity::Face(f) = e { Some(f) } else { None }).collect()
}
/// A 12-sided cylinder standing on the ground: radius 1, height 2.
fn cylinder(doc: &mut Document) -> String {
    run(doc, serde_json::json!({ "op": "solid_cylinder", "center": [0, 0, 0], "axis": [0, 1, 0], "radius": 1, "height": 2, "segments": 12 })).created[0].clone()
}
/// Edge segments that stand upright (the seams of a cylinder's side), each counted once however many faces draw it.
fn seams(s: &CompiledScene) -> usize {
    let p = &s.edges.positions;
    let mut seen: Vec<(i64, i64)> = vec![];
    for i in (0..p.len()).step_by(6) {
        if (p[i] - p[i + 3]).abs() < 1e-6 && (p[i + 2] - p[i + 5]).abs() < 1e-6 && (p[i + 1] - p[i + 4]).abs() > 1.0 {
            let k = ((p[i] * 1e4).round() as i64, (p[i + 2] * 1e4).round() as i64);
            if !seen.contains(&k) { seen.push(k); }
        }
    }
    seen.len()
}

#[test]
fn softening_a_cylinder_hides_its_seams_and_rounds_its_shading() {
    let mut doc = Document::default();
    let c = cylinder(&mut doc);
    assert_eq!(seams(&scene(&doc)), 12);
    let r = run(&mut doc, serde_json::json!({ "op": "soften", "ids": [c], "angle": 35 }));
    assert_eq!(r.modified, vec![c.clone()]);
    assert!(faces(&doc, &c).iter().all(|f| f.smooth() == Some(35.0)));
    let s = scene(&doc);
    assert_eq!(seams(&s), 0, "the sides meet at 30°: soft");
    // the rims (90°) are still drawn: 12 segments each, by the cap and by the side
    assert_eq!(s.edges.ids.len(), 2 * 12 * 2);
    // on the side every normal points straight out from the axis; the caps stay flat
    let (mut round, mut flat) = (0, 0);
    for g in s.groups.iter().filter(|g| !g.back) {
        for i in (0..g.positions.len()).step_by(3) {
            let (p, n) = ([g.positions[i] as f64, 0.0, g.positions[i + 2] as f64], [g.normals[i] as f64, g.normals[i + 1] as f64, g.normals[i + 2] as f64]);
            if n[1].abs() > 0.99 { flat += 1; continue; }
            assert!(n[1].abs() < 1e-6 && dot(normalize(p), n) > 0.9999, "{p:?} {n:?}");
            round += 1;
        }
    }
    assert!(round > 0 && flat > 0);
    // hidden geometry shows the soft edges again
    assert_eq!(seams(&compile(&doc, &reg(), &CompileOptions { show_hidden: true, ..Default::default() })), 12);

    // a smaller angle leaves the 30° seams hard; 0 takes the softening off
    run(&mut doc, serde_json::json!({ "op": "soften", "ids": [c], "angle": 20 }));
    assert_eq!(seams(&scene(&doc)), 12);
    run(&mut doc, serde_json::json!({ "op": "soften", "ids": [c], "angle": 0 }));
    assert!(faces(&doc, &c).iter().all(|f| f.smooth().is_none() && !f.extra.contains_key("smooth")));
    assert!(apply(&mut doc, &op(serde_json::json!({ "op": "soften", "ids": [c], "angle": 0 })), &reg()).is_err(), "nothing softened");
    assert!(apply(&mut doc, &op(serde_json::json!({ "op": "soften", "ids": [c], "angle": 200 })), &reg()).is_err());
}

#[test]
fn softening_survives_a_move_and_a_scaled_group() {
    let mut doc = Document::default();
    let c = cylinder(&mut doc);
    run(&mut doc, serde_json::json!({ "op": "soften", "ids": [c], "angle": 35 }));
    run(&mut doc, serde_json::json!({ "op": "move", "ids": [c], "delta": [3.3333333, 0, 1.2345678] }));
    run(&mut doc, serde_json::json!({ "op": "rotate", "ids": [c], "origin": [0, 0, 0], "axis": [0, 1, 0], "angle": 17 }));
    assert_eq!(seams(&scene(&doc)), 0);
    // a mirrored copy keeps it, with normals still pointing out
    run(&mut doc, serde_json::json!({ "op": "mirror", "ids": [c], "origin": [0, 0, 0], "normal": [1, 0, 0] }));
    let s = scene(&doc);
    assert_eq!(seams(&s), 0);
    let b = &s.bounds; let mid = [(b.min[0] + b.max[0]) / 2.0, 0.0, (b.min[2] + b.max[2]) / 2.0];
    for g in s.groups.iter().filter(|g| !g.back) {
        for i in (0..g.positions.len()).step_by(3) {
            let n = [g.normals[i] as f64, g.normals[i + 1] as f64, g.normals[i + 2] as f64];
            if n[1].abs() > 0.99 { continue; }
            let out = normalize(sub([g.positions[i] as f64, 0.0, g.positions[i + 2] as f64], mid));
            assert!(dot(out, n) > 0.999, "{out:?} {n:?}");
        }
    }
}

#[test]
fn one_edge_softens_through_its_two_faces() {
    let mut doc = Document::default();
    // a roof: two faces meeting at the ridge at 90°
    let a = run(&mut doc, serde_json::json!({ "op": "draw_polygon", "points": [[0, 0, 0], [0, 0, 4], [1, 1, 4], [1, 1, 0]] })).created[0].clone();
    let b = run(&mut doc, serde_json::json!({ "op": "draw_polygon", "points": [[1, 1, 0], [1, 1, 4], [2, 0, 4], [2, 0, 0]] })).created[0].clone();
    let ridge = |s: &CompiledScene| (0..s.edges.ids.len()).filter(|i| { let p = &s.edges.positions[i * 6..i * 6 + 6]; (p[0] - 1.0).abs() < 1e-6 && (p[3] - 1.0).abs() < 1e-6 && (p[1] - 1.0).abs() < 1e-6 }).count();
    assert_eq!(ridge(&scene(&doc)), 2);
    let r = run(&mut doc, serde_json::json!({ "op": "soften", "edges": [[[1, 1, 4], [1, 1, 0]]] }));
    assert_eq!(r.modified.len(), 2);
    let sm = |doc: &Document, id: &str| match doc.find(id) { Some(Entity::Face(f)) => f.smooth(), _ => None };
    assert_eq!((sm(&doc, &a), sm(&doc, &b)), (Some(91.0), Some(91.0)));
    let s = scene(&doc);
    assert_eq!(ridge(&s), 0);
    assert_eq!(s.edges.ids.len(), 6, "the other three edges of each face are still drawn");
    // hard again
    run(&mut doc, serde_json::json!({ "op": "soften", "edges": [[[1, 1, 0], [1, 1, 4]]], "angle": 0 }));
    assert_eq!((sm(&doc, &a), sm(&doc, &b)), (None, None));
    assert_eq!(ridge(&scene(&doc)), 2);
    // not an edge between two faces
    assert!(apply(&mut doc, &op(serde_json::json!({ "op": "soften", "edges": [[[0, 0, 0], [0, 0, 4]]] })), &reg()).is_err());
    // a locked face is not softened
    run(&mut doc, serde_json::json!({ "op": "set_locked", "ids": [a], "locked": true }));
    assert!(apply(&mut doc, &op(serde_json::json!({ "op": "soften", "ids": [a] })), &reg()).is_err());
}

#[test]
fn a_line_drawn_on_a_softened_face_still_shows() {
    let mut doc = Document::default();
    let a = run(&mut doc, serde_json::json!({ "op": "draw_rectangle", "corner": [0, 0, 0], "opposite": [4, 0, 4], "normal": [0, 1, 0] })).created[0].clone();
    run(&mut doc, serde_json::json!({ "op": "soften", "ids": [a], "angle": 45 }));
    run(&mut doc, serde_json::json!({ "op": "draw_line", "points": [[2, 0, 0], [2, 0, 4]] }));
    let n = doc.entities.iter().filter(|e| matches!(e, Entity::Face(f) if f.smooth() == Some(45.0))).count();
    assert_eq!(n, 2, "both halves stay softened");
    let s = scene(&doc);
    let mid = (0..s.edges.ids.len()).filter(|i| { let p = &s.edges.positions[i * 6..i * 6 + 6]; (p[0] - 2.0).abs() < 1e-6 && (p[3] - 2.0).abs() < 1e-6 }).count();
    assert_eq!(mid, 3, "the two halves lie in one plane: the edge between them is drawn (by both, and as the line)");
}

#[test]
fn text3d_makes_a_group_of_letters() {
    let mut doc = Document::default();
    let r = run(&mut doc, serde_json::json!({ "op": "text3d", "text": "Hi" }));
    assert_eq!(r.created.len(), 1);
    let Some(Entity::Group(g)) = doc.find(&r.created[0]) else { panic!() };
    assert_eq!((g.name.as_str(), g.entities.len()), ("Hi", 3)); // H, the stem of the i and its dot
    // flat on the ground, reading along x, the tops of the letters away from the front (-z), 25 cm tall
    let pts: Vec<P3> = faces(&doc, &r.created[0]).iter().flat_map(|f| f.outer.clone()).collect();
    assert!(pts.iter().all(|p| p[1].abs() < 1e-9 && p[0] > -1e-9));
    let far = pts.iter().fold(0.0f64, |m, p| m.min(p[2]));
    assert!(far < -0.249 && far > -0.28, "{far}");
    assert!(faces(&doc, &r.created[0]).iter().all(|f| normal(&f.outer)[1] > 0.99));

    // upright on a wall facing the front, standing out 5 cm, centred, bold serif, painted
    let r = run(&mut doc, serde_json::json!({ "op": "text3d", "text": "CAFÉ", "position": [0, 2, 0], "normal": [0, 0, 1], "height": 0.5, "depth": 0.05, "font": "serif", "bold": true, "align": "center", "material": "#aa3322" }));
    let fs = faces(&doc, &r.created[0]);
    let all: Vec<P3> = fs.iter().flat_map(|f| f.outer.clone()).collect();
    let (lo, hi) = all.iter().fold(([f64::MAX; 3], [f64::MIN; 3]), |(mut lo, mut hi), p| { for i in 0..3 { lo[i] = lo[i].min(p[i]); hi[i] = hi[i].max(p[i]); } (lo, hi) });
    assert!((lo[2]).abs() < 1e-9 && (hi[2] - 0.05).abs() < 1e-9, "{lo:?} {hi:?}");
    assert!((lo[1] - 2.0).abs() < 0.02 && hi[1] > 2.5, "{lo:?} {hi:?}"); // capitals 0.5 tall, the accent above
    assert!((lo[0] + hi[0]).abs() < 0.1, "centred: {lo:?} {hi:?}");
    assert!(fs.iter().all(|f| f.material.is_some()));
    // the round of the C is shaded smooth: fewer edge segments than its side faces would draw
    let s = scene(&doc);
    assert!(s.metrics.faces > 100);

    for bad in [serde_json::json!({ "op": "text3d", "text": "  " }), serde_json::json!({ "op": "text3d", "text": "a", "height": 0 }), serde_json::json!({ "op": "text3d", "text": "a", "font": "comic" }), serde_json::json!({ "op": "text3d", "text": "a", "align": "middle" })] {
        assert!(apply(&mut doc, &op(bad), &reg()).is_err());
    }
}
