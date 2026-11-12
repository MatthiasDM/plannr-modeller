//! Intersect Faces, Push/Pull with a new starting face, radius and diameter dimensions, screen text.

use plannr_core::geom::*;
use plannr_core::model::{Document, Entity, Face};
use plannr_core::{apply, Op, Registry};

fn reg() -> Registry { Registry::new() }
fn op(v: serde_json::Value) -> Op { serde_json::from_value(v).unwrap() }
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

/// A box from `a` to `b` on the level of `a`, `h` high, as a group of its own.
fn boxed(doc: &mut Document, a: P3, b: P3, h: f64) -> String {
    let base = rect(doc, a, b, UP);
    apply(doc, &Op::PushPull { face: base, distance: h, new_face: false }, &reg()).unwrap();
    let ids: Vec<String> = doc.entities.iter().filter(|e| matches!(e, Entity::Face(_))).map(|e| e.id().to_string()).collect();
    apply(doc, &Op::MakeGroup { ids, name: None }, &reg()).unwrap().created[0].clone()
}
fn group<'a>(doc: &'a Document, id: &str) -> &'a plannr_core::model::Group { match doc.find(id).unwrap() { Entity::Group(g) => g, _ => panic!("not a group") } }
fn loose(list: &[Entity]) -> Vec<(P3, P3)> { list.iter().filter_map(|e| if let Entity::Edge(e) = e { Some((e.a, e.b)) } else { None }).collect() }
fn has_edge(list: &[(P3, P3)], a: P3, b: P3) -> bool { list.iter().any(|(p, q)| (dist(*p, a) < 1e-6 && dist(*q, b) < 1e-6) || (dist(*p, b) < 1e-6 && dist(*q, a) < 1e-6)) }

#[test]
fn two_groups_that_cross_leave_their_crossing_beside_them() {
    let mut doc = Document::default();
    let a = boxed(&mut doc, [0.0, 0.0, 0.0], [2.0, 0.0, 2.0], 2.0);
    let b = boxed(&mut doc, [1.0, 0.5, 1.0], [3.0, 0.5, 3.0], 2.0);
    let r = intersect(&mut doc, &[&a, &b]);
    // the outline of what the two boxes share: six edges round, drawn beside the groups
    let e = loose(&doc.entities);
    assert_eq!(e.len(), 6, "{e:?}");
    assert_eq!(r.created.len(), 6);
    assert!(has_edge(&e, [2.0, 0.5, 1.0], [2.0, 2.0, 1.0]) && has_edge(&e, [1.0, 0.5, 2.0], [1.0, 2.0, 2.0]), "the two upright ones");
    assert!(has_edge(&e, [1.0, 2.0, 1.0], [2.0, 2.0, 1.0]) && has_edge(&e, [1.0, 2.0, 1.0], [1.0, 2.0, 2.0]), "on the top of the first");
    assert!(has_edge(&e, [2.0, 0.5, 1.0], [2.0, 0.5, 2.0]) && has_edge(&e, [1.0, 0.5, 2.0], [2.0, 0.5, 2.0]), "on the bottom of the second");
    // the groups themselves are as they were
    assert!(group(&doc, &a).entities.len() == 6 && group(&doc, &b).entities.len() == 6 && r.modified.is_empty());
    // again: everything is drawn already
    assert!(try_intersect(&mut doc, &[&a, &b]).is_none() && loose(&doc.entities).len() == 6);
    // one group alone crosses the rest of the model just the same
    let mut doc = Document::default();
    let a = boxed(&mut doc, [0.0, 0.0, 0.0], [2.0, 0.0, 2.0], 2.0);
    let _b = boxed(&mut doc, [1.0, 0.5, 1.0], [3.0, 0.5, 3.0], 2.0);
    intersect(&mut doc, &[&a]);
    assert_eq!(loose(&doc.entities).len(), 6);
}

#[test]
fn a_face_inside_a_group_is_cut_by_what_crosses_it_from_outside() {
    let mut doc = Document::default();
    let a = boxed(&mut doc, [0.0, 0.0, 0.0], [2.0, 0.0, 2.0], 2.0);
    apply(&mut doc, &Op::Move { ids: vec![a.clone()], delta: [10.0, 0.0, 0.0] }, &reg()).unwrap();
    let b = boxed(&mut doc, [11.0, 0.5, 1.0], [13.0, 0.5, 3.0], 2.0);
    let inv = mat_inverse(&group(&doc, &a).transform);
    let top = group(&doc, &a).entities.iter().find_map(|e| match e { Entity::Face(f) if f.outer.iter().all(|p| (mat_apply(&group(&doc, &a).transform, *p)[1] - 2.0).abs() < 1e-9) => Some(f.id.clone()), _ => None }).unwrap();
    let r = intersect(&mut doc, &[&top]);
    // two edges inside the group, in its own coordinates, cut the corner out of its top
    let g = group(&doc, &a);
    let e = loose(&g.entities);
    assert_eq!(e.len(), 2, "{e:?}");
    assert!(has_edge(&e, mat_apply(&inv, [11.0, 2.0, 1.0]), mat_apply(&inv, [12.0, 2.0, 1.0])) && has_edge(&e, mat_apply(&inv, [11.0, 2.0, 1.0]), mat_apply(&inv, [11.0, 2.0, 2.0])));
    assert_eq!(g.entities.iter().filter(|e| matches!(e, Entity::Face(_))).count(), 7, "the top is two faces now");
    assert!(r.modified.contains(&top));
    assert!(loose(&doc.entities).is_empty() && group(&doc, &b).entities.len() == 6, "nothing lands outside the group, the other box is untouched");
}

#[test]
fn hidden_things_and_the_faces_of_one_group_do_not_cut() {
    // two faces that cross inside one group: selecting the group does not cut them (open it for that)
    let mut doc = Document::default();
    let h = rect(&mut doc, [-2.0, 0.0, -2.0], [2.0, 0.0, 2.0], UP);
    let v = rect(&mut doc, [0.0, -1.0, -2.0], [0.0, 1.0, 2.0], X);
    let g = apply(&mut doc, &Op::MakeGroup { ids: vec![h, v], name: None }, &reg()).unwrap().created[0].clone();
    assert!(try_intersect(&mut doc, &[&g]).is_none());
    // a hidden box is not crossed
    let mut doc = Document::default();
    let a = boxed(&mut doc, [0.0, 0.0, 0.0], [2.0, 0.0, 2.0], 2.0);
    let b = boxed(&mut doc, [1.0, 0.5, 1.0], [3.0, 0.5, 3.0], 2.0);
    apply(&mut doc, &Op::SetHidden { ids: vec![b.clone()], hidden: true }, &reg()).unwrap();
    assert!(try_intersect(&mut doc, &[&a]).is_none() && loose(&doc.entities).is_empty());
    apply(&mut doc, &Op::SetHidden { ids: vec![b], hidden: false }, &reg()).unwrap();
    assert!(try_intersect(&mut doc, &[&a]).is_some());
}

#[test]
fn a_component_is_crossed_and_crosses_like_a_group() {
    let mut doc = Document::default();
    let a = boxed(&mut doc, [0.0, 0.0, 0.0], [2.0, 0.0, 2.0], 2.0);
    let b = boxed(&mut doc, [1.0, 0.5, 1.0], [3.0, 0.5, 3.0], 2.0);
    let inst = apply(&mut doc, &op(serde_json::json!({ "op": "make_component", "ids": [b], "name": "Block" })), &reg()).unwrap().created[0].clone();
    assert!(matches!(doc.find(&inst), Some(Entity::Instance(_))));
    intersect(&mut doc, &[&a]);
    assert_eq!(loose(&doc.entities).len(), 6, "the group crosses the component");
    let mut doc2 = Document::default();
    let _a = boxed(&mut doc2, [0.0, 0.0, 0.0], [2.0, 0.0, 2.0], 2.0);
    let b = boxed(&mut doc2, [1.0, 0.5, 1.0], [3.0, 0.5, 3.0], 2.0);
    let inst = apply(&mut doc2, &op(serde_json::json!({ "op": "make_component", "ids": [b], "name": "Block" })), &reg()).unwrap().created[0].clone();
    intersect(&mut doc2, &[&inst]);
    assert_eq!(loose(&doc2.entities).len(), 6, "the component crosses the group");
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

#[test]
fn an_angular_dimension_reads_the_angle_between_its_legs() {
    let mut doc = Document::default();
    let r = apply(&mut doc, &op(serde_json::json!({ "op": "add_dimension", "a": [1, 0, 1], "b": [3, 0, 1], "offset": [2, 0, 2], "style": "angle" })), &reg()).unwrap();
    let s = plannr_core::compile::compile(&doc, &reg(), &Default::default());
    assert_eq!(s.dimensions.len(), 1);
    assert!((s.dimensions[0].value - 45.0).abs() < 1e-9, "{}", s.dimensions[0].value);
    assert_eq!(s.dimensions[0].style.as_deref(), Some("angle"));
    // turning it keeps the angle; the corner and both legs move together
    apply(&mut doc, &op(serde_json::json!({ "op": "rotate", "ids": [r.created[0]], "origin": [0, 0, 0], "axis": [0, 1, 0], "angle": 30 })), &reg()).unwrap();
    apply(&mut doc, &op(serde_json::json!({ "op": "scale", "ids": [r.created[0]], "origin": [0, 0, 0], "factors": [2, 2, 2] })), &reg()).unwrap();
    let s = plannr_core::compile::compile(&doc, &reg(), &Default::default());
    assert!((s.dimensions[0].value - 45.0).abs() < 1e-9);
    assert!((dist(s.dimensions[0].a, s.dimensions[0].b) - 4.0).abs() < 1e-9);
    // legs along one line make no angle
    assert!(apply(&mut doc, &op(serde_json::json!({ "op": "add_dimension", "a": [0, 0, 0], "b": [1, 0, 0], "offset": [2, 0, 0], "style": "angle" })), &reg()).is_err());
    assert!(apply(&mut doc, &op(serde_json::json!({ "op": "add_dimension", "a": [0, 0, 0], "b": [1, 0, 0], "offset": [0, 0, 1], "style": "slope" })), &reg()).is_err());
}
