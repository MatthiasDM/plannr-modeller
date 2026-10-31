//! Hiding entities: gone from the compiled scene and slices, back on request, ghosts on show.

use plannr_core::model::{Document, Entity};
use plannr_core::ops::{apply, Op};
use plannr_core::plugin::Registry;
use plannr_core::{compile, CompileOptions};

fn reg() -> Registry { Registry::new() }
/// A 2 × 2 × 2 box in a group; returns the group id.
fn boxed() -> (Document, String) {
    let mut d = Document::default(); d.normalize();
    let f = apply(&mut d, &Op::DrawRectangle { corner: [0.0, 0.0, 0.0], opposite: [2.0, 0.0, 2.0], normal: [0.0, 1.0, 0.0], u: None, layer: None, material: None }, &reg()).unwrap().created[0].clone();
    apply(&mut d, &Op::PushPull { face: f, distance: 2.0 }, &reg()).unwrap();
    let ids: Vec<String> = d.entities.iter().map(|e| e.id().to_string()).collect();
    let g = apply(&mut d, &Op::MakeGroup { ids, name: Some("Box".into()) }, &reg()).unwrap().created[0].clone();
    (d, g)
}
fn hide(d: &mut Document, ids: &[&str], hidden: bool) { apply(d, &Op::SetHidden { ids: ids.iter().map(|s| s.to_string()).collect(), hidden }, &reg()).unwrap(); }

#[test]
fn a_hidden_group_leaves_the_scene_and_comes_back_without_a_trace() {
    let (mut d, g) = boxed();
    let before = serde_json::to_value(&d.entities).unwrap();
    let full = compile(&d, &reg(), &CompileOptions::default());
    assert!((full.metrics.area - 24.0).abs() < 1e-6);
    hide(&mut d, &[&g], true);
    let c = compile(&d, &reg(), &CompileOptions::default());
    assert!(c.groups.is_empty() && c.edges.ids.is_empty(), "nothing drawn");
    assert_eq!(c.metrics.area, 0.0);
    hide(&mut d, &[&g], false);
    assert_eq!(serde_json::to_value(&d.entities).unwrap(), before, "showing it again removes the flag");
    assert!((compile(&d, &reg(), &CompileOptions::default()).metrics.area - 24.0).abs() < 1e-6);
}

#[test]
fn one_face_inside_a_group_can_be_hidden() {
    let (mut d, g) = boxed();
    let face = match &d.entities[0] { Entity::Group(x) if x.id == g => x.entities[0].id().to_string(), _ => panic!("group") };
    hide(&mut d, &[&face], true);
    let c = compile(&d, &reg(), &CompileOptions::default());
    assert!((c.metrics.area - 20.0).abs() < 1e-6, "five of the six sides, got {}", c.metrics.area);
}

#[test]
fn ghosts_are_drawn_on_request_and_do_not_count() {
    let (mut d, g) = boxed();
    hide(&mut d, &[&g], true);
    let c = compile(&d, &reg(), &CompileOptions { show_hidden: true, ..Default::default() });
    assert!(!c.groups.is_empty() && c.groups.iter().all(|x| x.material == "__hidden" && x.opacity < 1.0), "only ghosts");
    assert!(c.groups.iter().flat_map(|x| x.ids.iter()).count() > 0 && !c.edges.ids.is_empty(), "pickable");
    assert_eq!(c.metrics.area, 0.0, "not in the metrics");
}

#[test]
fn hidden_geometry_is_not_sliced_and_an_unknown_id_is_an_error() {
    let (mut d, g) = boxed();
    let s = apply(&mut d, &Op::AddSection { origin: [0.0, 1.0, 0.0], normal: [0.0, 1.0, 0.0], active: Some(false), layer: None }, &reg()).unwrap().created[0].clone();
    hide(&mut d, &[&g], true);
    assert!(apply(&mut d, &Op::SliceSection { id: s, layer: None, name: None }, &reg()).is_err(), "the plane cuts nothing now");
    assert!(apply(&mut d, &Op::SetHidden { ids: vec!["nope".into()], hidden: true }, &reg()).is_err());
}
