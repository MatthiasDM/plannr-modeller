//! Sticky geometry: drawing on faces divides them, Push/Pull moves a face of a shape with its neighbours,
//! erasing an edge joins or removes the faces beside it.

use plannr_core::geom::*;
use plannr_core::model::{Document, Entity, Face};
use plannr_core::{apply, Op, Registry};

fn reg() -> Registry { Registry::new() }
fn rect(doc: &mut Document, a: P3, b: P3, n: P3, group: Option<&str>) -> plannr_core::OpResult {
    apply(doc, &Op::DrawRectangle { corner: a, opposite: b, normal: n, u: None, layer: None, material: None, group: group.map(String::from) }, &reg()).unwrap()
}
fn line(doc: &mut Document, pts: &[P3]) -> plannr_core::OpResult {
    apply(doc, &Op::DrawLine { points: pts.to_vec(), close: false, layer: None, group: None }, &reg()).unwrap()
}
fn face<'a>(doc: &'a Document, id: &str) -> &'a Face { match doc.find(id).unwrap_or_else(|| panic!("no {id}")) { Entity::Face(f) => f, _ => panic!("not a face") } }
fn faces(list: &[Entity]) -> Vec<&Face> { list.iter().filter_map(|e| if let Entity::Face(f) = e { Some(f) } else { None }).collect() }
fn net(f: &Face) -> f64 { area(&f.outer) - f.holes.iter().map(|h| area(h)).sum::<f64>() }
const UP: P3 = [0.0, 1.0, 0.0];
/// A 4 × 2 × 3 box standing on the ground (x 0..4, y 0..2, z 0..3); returns its top face.
fn slab(doc: &mut Document) -> String {
    let top = rect(doc, [0.0, 0.0, 0.0], [4.0, 0.0, 3.0], UP, None).created[0].clone();
    apply(doc, &Op::PushPull { face: top.clone(), distance: 2.0, new_face: false }, &reg()).unwrap();
    top
}
/// Total of the signed volumes under every face: the volume of a closed, outward-facing shell.
fn volume(list: &[Entity]) -> f64 {
    faces(list).iter().map(|f| {
        let idx = tessellate(&f.outer, &f.holes);
        let pts: Vec<P3> = f.outer.iter().chain(f.holes.iter().flatten()).copied().collect();
        idx.chunks(3).map(|t| dot(pts[t[0] as usize], cross(pts[t[1] as usize], pts[t[2] as usize])) / 6.0).sum::<f64>()
    }).sum()
}

#[test]
fn a_line_across_a_face_divides_it() {
    let mut doc = Document::default();
    let f = rect(&mut doc, [0.0, 0.0, 0.0], [4.0, 0.0, 2.0], UP, None).created[0].clone();
    // half way: nothing yet
    let r = line(&mut doc, &[[1.0, 0.0, 0.0], [1.0, 0.0, 1.0]]);
    assert_eq!(r.created.len(), 1, "just the edge");
    assert_eq!(faces(&doc.entities).len(), 1);
    // the rest of the way: the face is two faces, the larger keeps the id
    let r = line(&mut doc, &[[1.0, 0.0, 1.0], [1.0, 0.0, 2.0]]);
    assert_eq!(r.modified, vec![f.clone()]);
    assert_eq!(r.created.len(), 2, "the edge and the piece cut off");
    assert_eq!(faces(&doc.entities).len(), 2);
    assert!((net(face(&doc, &f)) - 6.0).abs() < 1e-9);
    assert!((net(face(&doc, &r.created[1])) - 2.0).abs() < 1e-9);
    assert!(normal(&face(&doc, &r.created[1]).outer)[1] > 0.9, "the piece faces the way the face did");
}

#[test]
fn a_shape_drawn_on_a_face_is_cut_into_it() {
    let mut doc = Document::default();
    let f = rect(&mut doc, [0.0, 0.0, 0.0], [4.0, 0.0, 4.0], UP, None).created[0].clone();
    let r = rect(&mut doc, [1.0, 0.0, 1.0], [2.0, 0.0, 2.0], UP, None);
    assert_eq!(r.created.len(), 1, "one new face: the part inside the outline");
    assert_eq!(r.modified, vec![f.clone()]);
    let (outer, inner) = (face(&doc, &f), face(&doc, &r.created[0]));
    assert_eq!(outer.holes.len(), 1, "the face has a hole where the new one sits");
    assert!((net(outer) - 15.0).abs() < 1e-9 && (net(inner) - 1.0).abs() < 1e-9);
    // from a corner: the face becomes an L
    let r = rect(&mut doc, [3.0, 0.0, 3.0], [4.0, 0.0, 4.0], UP, None);
    assert_eq!(r.created.len(), 1);
    assert_eq!(face(&doc, &f).outer.len(), 6);
    assert!((net(face(&doc, &f)) - 14.0).abs() < 1e-9);
    // half over the edge: the part on the face is cut out of it, the part beside it is a new face
    let r = rect(&mut doc, [-1.0, 0.0, 0.0], [1.0, 0.0, 0.5], UP, None);
    assert_eq!(r.created.len(), 2);
    assert!((net(face(&doc, &f)) - 13.5).abs() < 1e-9);
    let total: f64 = faces(&doc.entities).iter().map(|f| net(f)).sum();
    assert!((total - 16.5).abs() < 1e-9, "no area is covered twice: {total}");
    // the same shape again adds nothing
    let n = doc.entities.len();
    let r = rect(&mut doc, [1.0, 0.0, 1.0], [2.0, 0.0, 2.0], UP, None);
    assert!(r.created.is_empty());
    assert_eq!(doc.entities.len(), n);
}

#[test]
fn offset_divides_the_face() {
    let mut doc = Document::default();
    let f = rect(&mut doc, [0.0, 0.0, 0.0], [4.0, 0.0, 4.0], UP, None).created[0].clone();
    let r = apply(&mut doc, &Op::Offset { face: f.clone(), distance: -1.0 }, &reg()).unwrap();
    assert_eq!(r.created.len(), 1);
    assert!((net(face(&doc, &r.created[0])) - 4.0).abs() < 1e-9, "the inner face");
    assert!((net(face(&doc, &f)) - 12.0).abs() < 1e-9, "the border around it");
    assert_eq!(face(&doc, &f).holes.len(), 1);
}

#[test]
fn pulling_the_top_of_a_box_stretches_its_sides() {
    let mut doc = Document::default();
    let top = slab(&mut doc);
    assert_eq!(faces(&doc.entities).len(), 6);
    assert!((volume(&doc.entities) - 24.0).abs() < 1e-9);
    let r = apply(&mut doc, &Op::PushPull { face: top.clone(), distance: 1.0, new_face: false }, &reg()).unwrap();
    assert!(r.created.is_empty(), "no new faces");
    assert_eq!(r.modified.len(), 5, "the top and the four sides");
    assert_eq!(faces(&doc.entities).len(), 6);
    assert!(faces(&doc.entities).iter().all(|f| f.outer.len() == 4), "still six rectangles");
    assert!((volume(&doc.entities) - 36.0).abs() < 1e-9);
    // and back down, past where it started
    apply(&mut doc, &Op::PushPull { face: top.clone(), distance: -2.5, new_face: false }, &reg()).unwrap();
    assert!((volume(&doc.entities) - 6.0).abs() < 1e-9);
    // all the way: nothing is left
    let r = apply(&mut doc, &Op::PushPull { face: top, distance: -0.5, new_face: false }, &reg()).unwrap();
    assert_eq!(r.removed.len(), 6);
    assert!(faces(&doc.entities).is_empty());
}

#[test]
fn a_shape_on_a_box_pulls_up_and_pushes_in() {
    let mut doc = Document::default();
    slab(&mut doc);
    let inner = rect(&mut doc, [1.0, 2.0, 1.0], [2.0, 2.0, 2.0], UP, None).created[0].clone();
    assert_eq!(faces(&doc.entities).len(), 7);
    // up: a 1 × 1 × 0.5 block on top, no face under it
    let r = apply(&mut doc, &Op::PushPull { face: inner.clone(), distance: 0.5, new_face: false }, &reg()).unwrap();
    assert_eq!(r.created.len(), 4, "four sides");
    assert!((volume(&doc.entities) - 24.5).abs() < 1e-9);
    // down again, into the box: the sides follow, then turn into the walls of a pocket
    apply(&mut doc, &Op::PushPull { face: inner.clone(), distance: -0.5, new_face: false }, &reg()).unwrap();
    assert_eq!(faces(&doc.entities).len(), 7, "level again: the sides are gone");
    apply(&mut doc, &Op::PushPull { face: inner.clone(), distance: -1.0, new_face: false }, &reg()).unwrap();
    assert!((volume(&doc.entities) - 23.0).abs() < 1e-9, "a pocket of 1 m³");
    assert!(normal(&face(&doc, &inner).outer)[1] > 0.9, "the bottom of the pocket looks up");
    // through the bottom: a hole right through
    let r = apply(&mut doc, &Op::PushPull { face: inner.clone(), distance: -1.0, new_face: false }, &reg()).unwrap();
    assert!(r.removed.contains(&inner), "the pushed face is used up");
    assert!((volume(&doc.entities) - 22.0).abs() < 1e-9);
    let bottom = faces(&doc.entities).into_iter().find(|f| f.outer.iter().all(|p| p[1].abs() < 1e-9)).unwrap();
    assert_eq!(bottom.holes.len(), 1, "the bottom is open there");
}

#[test]
fn a_shape_at_the_edge_of_a_box_steps_the_side_it_stands_on() {
    let mut doc = Document::default();
    slab(&mut doc);
    // a 1 × 1 corner of the top, raised by 1
    let corner = rect(&mut doc, [0.0, 2.0, 0.0], [1.0, 2.0, 1.0], UP, None).created[0].clone();
    let r = apply(&mut doc, &Op::PushPull { face: corner, distance: 1.0, new_face: false }, &reg()).unwrap();
    assert_eq!(r.created.len(), 2, "sides only where the top goes on beside it");
    assert_eq!(faces(&doc.entities).len(), 9);
    assert!((volume(&doc.entities) - 25.0).abs() < 1e-9);
    let stepped = faces(&doc.entities).iter().filter(|f| f.outer.len() == 6 && normal(&f.outer)[1].abs() < 1e-9).count();
    assert_eq!(stepped, 2, "the two sides under the corner are stepped, in one piece");
}

#[test]
fn erasing_the_edge_between_two_faces_joins_them_and_any_other_edge_takes_its_face() {
    let mut doc = Document::default();
    let f = rect(&mut doc, [0.0, 0.0, 0.0], [4.0, 0.0, 2.0], UP, None).created[0].clone();
    let r = line(&mut doc, &[[1.0, 0.0, 0.0], [1.0, 0.0, 2.0]]);
    let (edge, piece) = (r.created[0].clone(), r.created[1].clone());
    let r = apply(&mut doc, &Op::Erase { ids: vec![edge] }, &reg()).unwrap();
    assert!(r.removed.contains(&piece) && r.modified.contains(&f));
    assert_eq!(faces(&doc.entities).len(), 1);
    assert!((net(face(&doc, &f)) - 8.0).abs() < 1e-9);
    // an edge along the outline: erasing it takes the face
    let e = line(&mut doc, &[[0.0, 0.0, 0.0], [4.0, 0.0, 0.0]]).created[0].clone();
    let r = apply(&mut doc, &Op::Erase { ids: vec![e, f.clone()] }, &reg()).unwrap();
    assert!(r.removed.contains(&f), "and naming the face as well is no error");
    assert!(doc.entities.is_empty());
}

#[test]
fn drawing_in_a_group_lands_in_the_group_and_cuts_its_faces() {
    let mut doc = Document::default();
    slab(&mut doc);
    let ids: Vec<String> = doc.entities.iter().map(|e| e.id().to_string()).collect();
    let g = apply(&mut doc, &Op::MakeGroup { ids, name: None }, &reg()).unwrap().created[0].clone();
    apply(&mut doc, &Op::Move { ids: vec![g.clone()], delta: [10.0, 0.0, 0.0] }, &reg()).unwrap();
    // a window in the front wall (z = 3), given in world coordinates, pushed right through
    let r = rect(&mut doc, [11.0, 0.5, 3.0], [12.0, 1.5, 3.0], [0.0, 0.0, 1.0], Some(&g));
    assert_eq!(doc.entities.len(), 1, "nothing was added beside the group");
    let Entity::Group(grp) = doc.find(&g).unwrap() else { panic!() };
    assert_eq!(grp.entities.len(), 7);
    let pane = r.created[0].clone();
    assert!(face(&doc, &pane).outer.iter().all(|p| p[0] < 2.5), "stored in the group's own coordinates");
    apply(&mut doc, &Op::PushPull { face: pane, distance: -3.0, new_face: false }, &reg()).unwrap();
    let Entity::Group(grp) = doc.find(&g).unwrap() else { panic!() };
    assert_eq!(grp.entities.len(), 10, "six walls and the four sides of the opening");
    assert_eq!(faces(&grp.entities).iter().filter(|f| f.holes.len() == 1).count(), 2, "front and back are open");
    assert!((volume(&grp.entities) - 21.0).abs() < 1e-9);
}

fn stretch(doc: &mut Document, ids: &[&str], points: &[P3], delta: P3) -> plannr_core::OpResult {
    apply(doc, &Op::Stretch { ids: ids.iter().map(|s| s.to_string()).collect(), points: points.to_vec(), delta: Some(delta), rotate: None, scale: None, group: None }, &reg()).unwrap()
}

#[test]
fn moving_a_face_of_a_box_takes_its_neighbours_along() {
    let mut doc = Document::default();
    let top = slab(&mut doc);
    // straight up: taller
    stretch(&mut doc, &[&top], &[], [0.0, 1.0, 0.0]);
    assert_eq!(faces(&doc.entities).len(), 6);
    assert!((volume(&doc.entities) - 36.0).abs() < 1e-9);
    // sideways: the box leans, every face is still one flat piece and the volume is the same
    let r = stretch(&mut doc, &[&top], &[], [1.0, 0.0, 0.5]);
    assert!(r.created.is_empty());
    assert_eq!(r.modified.len(), 5);
    assert_eq!(faces(&doc.entities).len(), 6);
    assert!(faces(&doc.entities).iter().all(|f| is_planar(&f.outer) && f.outer.len() == 4));
    assert!((volume(&doc.entities) - 36.0).abs() < 1e-9);
}

#[test]
fn moving_an_edge_makes_a_slope_and_moving_a_corner_folds_the_faces_around_it() {
    let mut doc = Document::default();
    let top = slab(&mut doc);
    // the top edge at x = 0 goes up by 1: a lean-to roof (a wedge of 4 × 3 × 1 / 2 = 6 on top)
    let r = stretch(&mut doc, &[], &[[0.0, 2.0, 0.0], [0.0, 2.0, 3.0]], [0.0, 1.0, 0.0]);
    assert!(r.created.is_empty(), "nothing needs folding");
    assert_eq!(faces(&doc.entities).len(), 6);
    assert!(normal(&face(&doc, &top).outer)[0] > 0.1, "the top slopes down towards +x");
    assert!((volume(&doc.entities) - 30.0).abs() < 1e-9);
    // one corner up by 1: the top can't stay flat, it folds along a diagonal
    let mut doc = Document::default();
    let top = slab(&mut doc);
    let r = stretch(&mut doc, &[], &[[0.0, 2.0, 0.0]], [0.0, 1.0, 0.0]);
    assert_eq!(r.created.len(), 1, "the second half of the folded top");
    assert_eq!(faces(&doc.entities).len(), 7);
    assert_eq!(face(&doc, &top).outer.len(), 3);
    assert!(faces(&doc.entities).iter().all(|f| is_planar(&f.outer)));
    let v = volume(&doc.entities);
    assert!((v - 26.0).abs() < 1e-9 || (v - 28.0).abs() < 1e-9, "closed: 24 plus a pyramid under the raised corner ({v})");
}

#[test]
fn a_moved_corner_in_the_middle_of_a_neighbours_edge_takes_that_edge_along() {
    let mut doc = Document::default();
    let top = slab(&mut doc);
    // divide the top at x = 1 and lift the small part: the large part becomes a ramp, the long sides get a corner at x = 1
    let piece = line(&mut doc, &[[1.0, 2.0, 0.0], [1.0, 2.0, 3.0]]).created[1].clone();
    let r = stretch(&mut doc, &[&piece], &[], [0.0, 1.0, 0.0]);
    assert!(r.created.is_empty());
    assert!((face(&doc, &piece).outer.iter().map(|p| p[1]).fold(f64::MAX, f64::min) - 3.0).abs() < 1e-9, "the piece is level at 3");
    assert!(normal(&face(&doc, &top).outer)[0] > 0.1, "the rest slopes up to it");
    assert_eq!(faces(&doc.entities).iter().filter(|f| f.outer.len() == 5).count(), 2, "the two long sides have a fifth corner");
    // 24 + the raised block 1 × 3 × 1 + the wedge 3 × 3 × 1 / 2
    assert!((volume(&doc.entities) - 31.5).abs() < 1e-9, "{}", volume(&doc.entities));
    // groups move as they are: stretch refuses them
    let ids: Vec<String> = doc.entities.iter().map(|e| e.id().to_string()).collect();
    let g = apply(&mut doc, &Op::MakeGroup { ids, name: None }, &reg()).unwrap().created[0].clone();
    assert!(apply(&mut doc, &Op::Stretch { ids: vec![g], points: vec![], delta: Some([1.0, 0.0, 0.0]), rotate: None, scale: None, group: None }, &reg()).is_err());
}

fn turn(doc: &mut Document, ids: &[&str], points: &[P3], origin: P3, axis: P3, angle: f64) -> plannr_core::OpResult {
    apply(doc, &Op::Stretch { ids: ids.iter().map(|s| s.to_string()).collect(), points: points.to_vec(), delta: None, rotate: Some(plannr_core::ops::Turn { origin, axis, angle }), scale: None, group: None }, &reg()).unwrap()
}
fn resize(doc: &mut Document, ids: &[&str], origin: P3, factors: P3) -> plannr_core::OpResult {
    apply(doc, &Op::Stretch { ids: ids.iter().map(|s| s.to_string()).collect(), points: vec![], delta: None, rotate: None, scale: Some(plannr_core::ops::Resize { origin, factors }), group: None }, &reg()).unwrap()
}
/// Every edge of every face is run the other way by exactly one other face: a closed shell without gaps.
fn closed(list: &[Entity]) -> bool {
    let key = |p: P3| [(p[0] * 1e4).round() as i64, (p[1] * 1e4).round() as i64, (p[2] * 1e4).round() as i64];
    let mut seen: std::collections::HashMap<([i64; 3], [i64; 3]), i32> = Default::default();
    for f in faces(list) { for l in std::iter::once(&f.outer).chain(f.holes.iter()) { for i in 0..l.len() { *seen.entry((key(l[i]), key(l[(i + 1) % l.len()]))).or_default() += 1; } } }
    seen.iter().all(|((a, b), n)| *n == 1 && seen.get(&(*b, *a)) == Some(&1))
}

#[test]
fn scaling_a_face_of_a_box_tapers_it() {
    let mut doc = Document::default();
    let top = slab(&mut doc);
    // the top at half size about its centre: a frustum, the sides lean in and stay flat
    let r = resize(&mut doc, &[&top], [2.0, 2.0, 1.5], [0.5, 1.0, 0.5]);
    assert!(r.created.is_empty(), "nothing needs folding");
    assert_eq!(r.modified.len(), 5);
    assert_eq!(faces(&doc.entities).len(), 6);
    assert!(faces(&doc.entities).iter().all(|f| is_planar(&f.outer) && f.outer.len() == 4));
    assert!((net(face(&doc, &top)) - 3.0).abs() < 1e-9);
    assert!(closed(&doc.entities));
    // h / 3 × (A1 + A2 + √(A1 A2)) = 2 / 3 × (12 + 3 + 6)
    assert!((volume(&doc.entities) - 14.0).abs() < 1e-9, "{}", volume(&doc.entities));
    // along one axis only: a ridge-like wedge shape, still six flat faces
    let mut doc = Document::default();
    let top = slab(&mut doc);
    resize(&mut doc, &[&top], [0.0, 2.0, 0.0], [0.5, 1.0, 1.0]);
    assert_eq!(faces(&doc.entities).len(), 6);
    assert!(closed(&doc.entities));
    assert!((volume(&doc.entities) - 18.0).abs() < 1e-9, "24 less the wedge 2 × 2 × 3 / 2");
}

#[test]
fn rotating_a_face_of_a_box_twists_it() {
    let mut doc = Document::default();
    let top = slab(&mut doc);
    // the top turned 30° about the vertical through its centre: it keeps its shape, each side folds in two
    let r = turn(&mut doc, &[&top], &[], [2.0, 2.0, 1.5], UP, 30.0);
    assert_eq!(r.created.len(), 4, "the second half of each side");
    assert_eq!(faces(&doc.entities).len(), 10);
    let t = face(&doc, &top);
    assert_eq!(t.outer.len(), 4);
    assert!((net(t) - 12.0).abs() < 1e-9 && t.outer.iter().all(|p| (p[1] - 2.0).abs() < 1e-9));
    assert!(faces(&doc.entities).iter().all(|f| is_planar(&f.outer)));
    assert!(closed(&doc.entities));
    let v = volume(&doc.entities); // (a smoothly twisted box would hold 22.9; each side folded inward takes a little off)
    assert!(v > 18.0 && v < 24.0, "{v}");
    // tilted about one of its own edges: that edge stays, the opposite one comes up, nothing folds
    let mut doc = Document::default();
    let top = slab(&mut doc);
    let r = turn(&mut doc, &[&top], &[], [0.0, 2.0, 0.0], [0.0, 0.0, 1.0], 20.0);
    assert!(r.created.is_empty());
    assert_eq!(faces(&doc.entities).len(), 6);
    assert!(faces(&doc.entities).iter().all(|f| is_planar(&f.outer)));
    assert!(closed(&doc.entities));
    assert!(face(&doc, &top).outer.iter().any(|p| p[1] > 3.0), "the far edge rose");
}

#[test]
fn whole_shapes_and_grouped_faces_scale_and_turn_too() {
    let mut doc = Document::default();
    slab(&mut doc);
    let all: Vec<String> = doc.entities.iter().map(|e| e.id().to_string()).collect();
    let ids: Vec<&str> = all.iter().map(|s| s.as_str()).collect();
    // every face named: the box as a whole, twice the size, then mirrored (still facing outward)
    let r = resize(&mut doc, &ids, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    assert!(r.created.is_empty() && r.modified.len() == 6);
    assert!((volume(&doc.entities) - 192.0).abs() < 1e-9);
    resize(&mut doc, &ids, [0.0, 0.0, 0.0], [-0.5, 0.5, 0.5]);
    assert!((volume(&doc.entities) - 24.0).abs() < 1e-9, "{}", volume(&doc.entities));
    assert!(closed(&doc.entities));
    // in a group that was turned a quarter: a scale along world x is one along the group's own z
    let mut doc = Document::default();
    let top = slab(&mut doc);
    let ids: Vec<String> = doc.entities.iter().map(|e| e.id().to_string()).collect();
    let g = apply(&mut doc, &Op::MakeGroup { ids, name: None }, &reg()).unwrap().created[0].clone();
    apply(&mut doc, &Op::Rotate { ids: vec![g.clone()], origin: [0.0; 3], axis: UP, angle: 90.0 }, &reg()).unwrap();
    // (the box now covers world x 0..3, z −4..0; its top's centre is at 1.5, 2, −2)
    resize(&mut doc, &[&top], [1.5, 2.0, -2.0], [0.5, 1.0, 1.0]);
    let t = face(&doc, &top);
    let range = |i: usize| (t.outer.iter().map(|p| p[i]).fold(f64::MAX, f64::min), t.outer.iter().map(|p| p[i]).fold(f64::MIN, f64::max));
    assert!((range(0).0 - 0.0).abs() < 1e-9 && (range(0).1 - 4.0).abs() < 1e-9, "{:?}", range(0));
    assert!((range(2).0 - 0.75).abs() < 1e-9 && (range(2).1 - 2.25).abs() < 1e-9, "{:?}", range(2));
    // one change at a time
    let both = Op::Stretch { ids: vec![top.clone()], points: vec![], delta: Some([1.0, 0.0, 0.0]), rotate: Some(plannr_core::ops::Turn { origin: [0.0; 3], axis: UP, angle: 10.0 }), scale: None, group: None };
    assert!(apply(&mut doc, &both, &reg()).is_err());
    let none = Op::Stretch { ids: vec![top], points: vec![], delta: None, rotate: None, scale: None, group: None };
    assert!(apply(&mut doc, &none, &reg()).is_err());
}
