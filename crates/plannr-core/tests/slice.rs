//! 2D slices at a section plane.

use plannr_core::model::{Document, Entity, Material};
use plannr_core::ops::{apply, Op};
use plannr_core::plugin::Registry;

fn reg() -> Registry { Registry::new() }
/// A box [x0, x0+w] × [0, h] × [0, d] on the ground, its faces painted `material`.
fn boxed(doc: &mut Document, x0: f64, w: f64, d: f64, h: f64, material: &str) {
    let before: Vec<String> = doc.entities.iter().map(|e| e.id().to_string()).collect();
    let r = apply(doc, &Op::DrawRectangle { corner: [x0, 0.0, 0.0], opposite: [x0 + w, 0.0, d], normal: [0.0, 1.0, 0.0], u: None, layer: None, material: None }, &reg()).unwrap();
    let face = r.created[0].clone();
    apply(doc, &Op::PushPull { face, distance: h }, &reg()).unwrap();
    let ids: Vec<String> = doc.entities.iter().filter_map(|e| match e { Entity::Face(f) if !before.contains(&f.id) => Some(f.id.clone()), _ => None }).collect();
    apply(doc, &Op::Paint { ids, material: material.into(), side: None }, &reg()).unwrap();
}
fn doc() -> Document {
    let mut d = Document::default(); d.normalize();
    for m in ["brick", "insulation"] { d.materials.push(Material { id: m.into(), name: m.into(), color: "#888888".into(), opacity: 1.0 }); }
    d
}
fn slice(d: &mut Document) -> Vec<[[f64; 3]; 2]> {
    let s = apply(d, &Op::AddSection { origin: [0.0, 1.0, 0.0], normal: [0.0, 1.0, 0.0], active: Some(false), layer: None }, &reg()).unwrap().created[0].clone();
    let r = apply(d, &Op::SliceSection { id: s, layer: None, name: None }, &reg()).unwrap();
    let g = d.entities.iter().find_map(|e| match e { Entity::Group(g) if g.id == r.created[0] => Some(g.clone()), _ => None }).unwrap();
    assert!(d.layers.iter().any(|l| l.name == "Slices" && l.id == g.layer));
    g.entities.iter().map(|e| match e { Entity::Edge(x) => [x.a, x.b], _ => panic!("only edges") }).collect()
}

#[test]
fn a_cube_slices_into_its_outline_at_the_plane() {
    let mut d = doc();
    boxed(&mut d, 0.0, 2.0, 3.0, 2.0, "brick");
    let segs = slice(&mut d);
    assert_eq!(segs.len(), 4, "{segs:?}");
    assert!(segs.iter().all(|s| (s[0][1] - 1.0).abs() < 1e-9 && (s[1][1] - 1.0).abs() < 1e-9), "in the plane");
    let total: f64 = segs.iter().map(|s| ((s[1][0] - s[0][0]).powi(2) + (s[1][2] - s[0][2]).powi(2)).sqrt()).sum();
    assert!((total - 10.0).abs() < 1e-6, "perimeter 2·(2+3) = 10, got {total}");
}

#[test]
fn touching_pieces_of_one_material_read_as_one_outline_and_different_materials_keep_their_joint() {
    let mut d = doc();
    boxed(&mut d, 0.0, 2.0, 3.0, 2.0, "brick");
    boxed(&mut d, 2.0, 1.0, 3.0, 2.0, "brick");
    let segs = slice(&mut d);
    assert_eq!(segs.len(), 4, "the joint at x = 2 is gone and the long sides are one line each: {segs:?}");
    let mut d = doc();
    boxed(&mut d, 0.0, 2.0, 3.0, 2.0, "brick");
    boxed(&mut d, 2.0, 1.0, 3.0, 2.0, "insulation");
    let segs = slice(&mut d);
    assert_eq!(segs.len(), 5, "brick | insulation: the joint stays as one line, the long sides stay whole: {segs:?}");
    assert!(segs.iter().any(|s| (s[0][0] - 2.0).abs() < 1e-9 && (s[1][0] - 2.0).abs() < 1e-9));
}

#[test]
fn a_plane_that_misses_everything_is_an_error() {
    let mut d = doc();
    boxed(&mut d, 0.0, 2.0, 3.0, 2.0, "brick");
    let s = apply(&mut d, &Op::AddSection { origin: [0.0, 5.0, 0.0], normal: [0.0, 1.0, 0.0], active: Some(false), layer: None }, &reg()).unwrap().created[0].clone();
    assert!(apply(&mut d, &Op::SliceSection { id: s, layer: None, name: None }, &reg()).is_err());
}
