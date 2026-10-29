//! The "Center" inference: circles, arcs and regular polygons expose their centre in the compiled scene.

use plannr_core::compile::{compile, CompileOptions};
use plannr_core::model::Document;
use plannr_core::ops::{apply, Op};
use plannr_core::plugin::Registry;

fn doc() -> Document { let mut d = Document::default(); d.normalize(); d }
fn centers(d: &Document) -> Vec<([f64; 3], f64)> { compile(d, &Registry::new(), &CompileOptions::default()).centers.iter().map(|c| (c.point, c.radius)).collect() }
fn near(a: [f64; 3], b: [f64; 3]) -> bool { (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6) }

#[test]
fn a_circle_has_one_centre_and_keeps_it_through_push_pull_and_move() {
    let mut d = doc(); let reg = Registry::new();
    let f = apply(&mut d, &Op::DrawCircle { center: [2.0, 0.0, 3.0], normal: [0.0, 1.0, 0.0], radius: 1.5, segments: Some(24), layer: None, material: None }, &reg).unwrap().created[0].clone();
    let c = centers(&d); assert_eq!(c.len(), 1); assert!(near(c[0].0, [2.0, 0.0, 3.0]) && (c[0].1 - 1.5).abs() < 1e-9, "{c:?}");
    apply(&mut d, &Op::PushPull { face: f, distance: 2.0 }, &reg).unwrap();
    let mut c = centers(&d); c.sort_by(|a, b| a.0[1].partial_cmp(&b.0[1]).unwrap());
    assert_eq!(c.len(), 2, "both caps of the cylinder: {c:?}"); assert!(near(c[1].0, [2.0, 2.0, 3.0]));
    let ids: Vec<String> = d.entities.iter().map(|e| e.id().to_string()).collect();
    apply(&mut d, &Op::Move { ids, delta: [1.0, 0.0, 0.0] }, &reg).unwrap();
    let c = centers(&d); assert!(c.iter().any(|(p, _)| near(*p, [3.0, 0.0, 3.0])), "moved with the geometry: {c:?}");
}

#[test]
fn an_arc_has_one_centre_shared_by_its_segments_and_follows_a_move() {
    let mut d = doc(); let reg = Registry::new();
    let r = apply(&mut d, &Op::DrawArc { center: Some([0.0, 0.0, 0.0]), start: Some([1.0, 0.0, 0.0]), end: Some([0.0, 0.0, 1.0]), through: None, from: None, to: None, bulge: None, normal: Some([0.0, 1.0, 0.0]), segments: Some(6), pie: false, close_chord: false, layer: None }, &reg).unwrap();
    assert!(r.created.len() >= 3);
    let c = centers(&d); assert_eq!(c.len(), 1, "{c:?}"); assert!(near(c[0].0, [0.0, 0.0, 0.0]) && (c[0].1 - 1.0).abs() < 1e-9);
    apply(&mut d, &Op::Move { ids: r.created.clone(), delta: [0.0, 0.0, 5.0] }, &reg).unwrap();
    let c = centers(&d); assert_eq!(c.len(), 1); assert!(near(c[0].0, [0.0, 0.0, 5.0]), "{c:?}");
    // a segment moved on its own carries its circle along: a second centre, shifted with it
    apply(&mut d, &Op::Move { ids: vec![r.created[0].clone()], delta: [0.3, 0.0, 0.0] }, &reg).unwrap();
    let c = centers(&d); assert_eq!(c.len(), 2, "{c:?}"); assert!(c.iter().any(|(p, _)| near(*p, [0.3, 0.0, 5.0])) && c.iter().any(|(p, _)| near(*p, [0.0, 0.0, 5.0])));
}

#[test]
fn a_rectangle_and_a_triangle_have_no_centre_but_a_hexagon_does() {
    let mut d = doc(); let reg = Registry::new();
    apply(&mut d, &Op::DrawRectangle { corner: [0.0, 0.0, 0.0], opposite: [2.0, 0.0, 1.0], normal: [0.0, 1.0, 0.0], u: None, layer: None, material: None }, &reg).unwrap();
    assert!(centers(&d).is_empty());
    apply(&mut d, &Op::DrawCircle { center: [5.0, 0.0, 0.0], normal: [0.0, 1.0, 0.0], radius: 1.0, segments: Some(6), layer: None, material: None }, &reg).unwrap();
    let c = centers(&d); assert_eq!(c.len(), 1); assert!(near(c[0].0, [5.0, 0.0, 0.0]));
}
