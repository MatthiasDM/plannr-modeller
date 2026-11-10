//! Copies in a row and around an axis.

use plannr_core::geom::*;
use plannr_core::model::{Document, Entity};
use plannr_core::ops::Turn;
use plannr_core::{apply, Op, Registry};

fn centre(doc: &Document, id: &str) -> P3 {
    match doc.find(id).unwrap() { Entity::Face(f) => scale(f.outer.iter().fold([0.0; 3], |a, p| add(a, *p)), 1.0 / f.outer.len() as f64), _ => panic!("not a face") }
}

#[test]
fn copies_go_in_a_row_or_around_an_axis() {
    let reg = Registry::new();
    let mut doc = Document::default();
    let f = apply(&mut doc, &Op::DrawRectangle { corner: [2.0, 0.0, -0.5], opposite: [3.0, 0.0, 0.5], normal: [0.0, 1.0, 0.0], u: None, layer: None, material: None, group: None }, &reg).unwrap().created[0].clone();
    // three more, a quarter turn apart about the vertical through the origin: a ring of four
    let r = apply(&mut doc, &Op::Copy { ids: vec![f.clone()], delta: [0.0; 3], rotate: Some(Turn { origin: [0.0; 3], axis: [0.0, 1.0, 0.0], angle: 90.0 }), count: Some(3) }, &reg).unwrap();
    assert_eq!(r.created.len(), 3);
    let want = [[0.0, 0.0, -2.5], [-2.5, 0.0, 0.0], [0.0, 0.0, 2.5]];
    for (id, w) in r.created.iter().zip(want) { assert!(dist(centre(&doc, id), w) < 1e-9, "{:?} vs {w:?}", centre(&doc, id)); }
    assert!(dist(centre(&doc, &f), [2.5, 0.0, 0.0]) < 1e-9, "the original stays");
    // rising as it turns: a spiral stair's steps
    let r = apply(&mut doc, &Op::Copy { ids: vec![f.clone()], delta: [0.0, 0.2, 0.0], rotate: Some(Turn { origin: [0.0; 3], axis: [0.0, 1.0, 0.0], angle: 30.0 }), count: Some(2) }, &reg).unwrap();
    assert!((centre(&doc, &r.created[1])[1] - 0.4).abs() < 1e-9);
    assert!(dist(centre(&doc, &r.created[1]), [2.5 * 60f64.to_radians().cos(), 0.4, -2.5 * 60f64.to_radians().sin()]) < 1e-9);
    // a plain row needs no rotation; from JSON neither field is required
    let op: Op = serde_json::from_value(serde_json::json!({ "op": "copy", "ids": [f], "delta": [0, 0, 3], "count": 2 })).unwrap();
    assert_eq!(apply(&mut doc, &op, &reg).unwrap().created.len(), 2);
}
