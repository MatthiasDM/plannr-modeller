//! Intersect Faces, Push/Pull with a new starting face, radius and diameter dimensions, screen text.

use plannr_core::geom::*;
use plannr_core::model::{Document, Entity, Face};
use plannr_core::{apply, Op, Registry};

fn reg() -> Registry { Registry::new() }
fn rect(doc: &mut Document, a: P3, b: P3, n: P3) -> String {
    apply(doc, &Op::DrawRectangle { corner: a, opposite: b, normal: n, u: None, layer: None, material: None, group: None }, &reg()).unwrap().created[0].clone()
}
fn faces(doc: &Document) -> Vec<&Face> { doc.entities.iter().filter_map(|e| if let Entity::Face(f) = e { Some(f) } else { None }).collect() }
fn edges(doc: &Document) -> usize { doc.entities.iter().filter(|e| matches!(e, Entity::Edge(_))).count() }
fn try_intersect(doc: &mut Document, ids: &[&String]) -> Option<plannr_core::OpResult> {
    apply(doc, &Op::IntersectFaces { ids: ids.iter().map(|s| s.to_string()).collect() }, &reg()).ok()
}
fn intersect(doc: &mut Document, ids: &[&String]) -> plannr_core::OpResult { try_intersect(doc, ids).unwrap() }
const UP: P3 = [0.0, 1.0, 0.0];
const X: P3 = [1.0, 0.0, 0.0];

#[test]
fn two_faces_that_cross_cut_each_other() {
    let mut doc = Document::default();
    let h = rect(&mut doc, [-2.0, 0.0, -2.0], [2.0, 0.0, 2.0], UP);
    let v = rect(&mut doc, [0.0, -1.0, -2.0], [0.0, 1.0, 2.0], X);
    let r = intersect(&mut doc, &[&v]);
    assert_eq!(edges(&doc), 1, "one edge along the crossing");
    assert_eq!(faces(&doc).len(), 4, "both faces are divided");
    assert!(r.modified.contains(&h) && r.modified.contains(&v));
    assert!(faces(&doc).iter().all(|f| (area(&f.outer) - if f.id == h || normal(&f.outer)[1].abs() > 0.9 { 8.0 } else { 4.0 }).abs() < 1e-9));
    // again: the edge is there already, there is nothing to do
    assert!(try_intersect(&mut doc, &[&v]).is_none() && edges(&doc) == 1 && faces(&doc).len() == 4);
}

#[test]
fn a_crossing_that_stops_inside_a_face_leaves_an_edge_on_it() {
    let mut doc = Document::default();
    let h = rect(&mut doc, [-2.0, 0.0, -2.0], [2.0, 0.0, 2.0], UP);
    let v = rect(&mut doc, [0.0, -1.0, -3.0], [0.0, 1.0, 3.0], X);
    let r = intersect(&mut doc, &[&h]);
    assert_eq!(edges(&doc), 1);
    assert_eq!(faces(&doc).len(), 3, "the ground face is cut in two, the upright one only gets the edge");
    assert!(r.modified.contains(&h) && !r.modified.contains(&v));
    match doc.entities.iter().find(|e| matches!(e, Entity::Edge(_))).unwrap() { Entity::Edge(e) => assert!((dist(e.a, e.b) - 4.0).abs() < 1e-9), _ => unreachable!() }
}

#[test]
fn a_face_standing_on_another_cuts_it_and_the_edges_of_a_box_are_left_alone() {
    let mut doc = Document::default();
    let h = rect(&mut doc, [-2.0, 0.0, -2.0], [2.0, 0.0, 2.0], UP);
    let v = rect(&mut doc, [0.0, 0.0, -2.0], [0.0, 1.0, 2.0], X);
    intersect(&mut doc, &[&v]);
    assert_eq!((edges(&doc), faces(&doc).len()), (1, 3), "the foot of the upright face divides the ground face");
    assert!((area(&faces(&doc).iter().find(|f| f.id == h).unwrap().outer) - 8.0).abs() < 1e-9);

    let mut doc = Document::default();
    let top = rect(&mut doc, [0.0, 0.0, 0.0], [4.0, 0.0, 3.0], UP);
    apply(&mut doc, &Op::PushPull { face: top, distance: 2.0, new_face: false }, &reg()).unwrap();
    let all: Vec<String> = faces(&doc).iter().map(|f| f.id.clone()).collect();
    assert!(try_intersect(&mut doc, &all.iter().collect::<Vec<_>>()).is_none() && edges(&doc) == 0 && faces(&doc).len() == 6, "faces that only share edges do not cross");
}

#[test]
fn push_pull_with_a_new_face_builds_on_the_solid() {
    let mut doc = Document::default();
    let top = rect(&mut doc, [0.0, 0.0, 0.0], [4.0, 0.0, 3.0], UP);
    apply(&mut doc, &Op::PushPull { face: top.clone(), distance: 2.0, new_face: false }, &reg()).unwrap();
    assert_eq!(faces(&doc).len(), 6);
    let op: Op = serde_json::from_value(serde_json::json!({ "op": "push_pull", "face": top, "distance": 1.0, "newFace": true })).unwrap();
    let r = apply(&mut doc, &op, &reg()).unwrap();
    assert_eq!(r.modified, vec![top.clone()]);
    assert_eq!(r.created.len(), 5, "a face where the top was, and four new sides");
    assert_eq!(faces(&doc).len(), 11);
    let level = |f: &Face, y: f64| f.outer.iter().all(|p| (p[1] - y).abs() < 1e-9);
    assert!(level(faces(&doc).iter().find(|f| f.id == top).unwrap(), 3.0));
    assert_eq!(faces(&doc).iter().filter(|f| level(f, 2.0)).count(), 1, "the face it started from stays");
    assert!(faces(&doc).iter().all(|f| f.outer.iter().map(|p| p[1]).fold(f64::MIN, f64::max) - f.outer.iter().map(|p| p[1]).fold(f64::MAX, f64::min) < 2.0 + 1e-9), "the sides below are not stretched");
}

#[test]
fn radius_dimensions_and_screen_text() {
    let mut doc = Document::default();
    let dim = |style: Option<&str>| Op::AddDimension { a: [0.0; 3], b: [1.0, 0.0, 0.0], offset: [0.5, 0.0, 0.0], style: style.map(String::from), layer: None };
    let r = apply(&mut doc, &dim(Some("radius")), &reg()).unwrap();
    match doc.find(&r.created[0]).unwrap() { Entity::Dimension(d) => assert_eq!(d.style.as_deref(), Some("radius")), _ => panic!() }
    assert!(apply(&mut doc, &dim(Some("angle")), &reg()).is_err());
    assert!(apply(&mut doc, &dim(None), &reg()).is_ok());

    let op: Op = serde_json::from_value(serde_json::json!({ "op": "add_text", "text": "North", "screen": [0.5, 0.1], "anchor": [1, 2, 3] })).unwrap();
    let r = apply(&mut doc, &op, &reg()).unwrap();
    match doc.find(&r.created[0]).unwrap() { Entity::Text(t) => { assert_eq!(t.screen, Some([0.5, 0.1])); assert!(t.anchor.is_none(), "screen text has no leader"); } _ => panic!() }
    let off: Op = serde_json::from_value(serde_json::json!({ "op": "add_text", "text": "x", "screen": [1.5, 0.1] })).unwrap();
    assert!(apply(&mut doc, &off, &reg()).is_err());
    let scene = plannr_core::compile(&doc, &reg(), &Default::default());
    assert_eq!(scene.texts.iter().filter(|t| t.screen.is_some()).count(), 1);
    assert_eq!(scene.dimensions.iter().filter(|d| d.style.as_deref() == Some("radius")).count(), 1);
}
