//! A small demo document built entirely through ops.

use serde_json::json;

use crate::model::Document;
use crate::ops::{apply, Op};
use crate::plugin::Registry;

pub fn sample_document() -> Document {
    let reg = Registry::new();
    let mut doc = Document::default();
    let run = |doc: &mut Document, op: Op| apply(doc, &op, &reg).expect("sample op");
    let up = [0.0, 1.0, 0.0];

    let brick = run(&mut doc, Op::AddMaterial { name: "Brick".into(), color: "#B0623F".into(), opacity: None }).created[0].clone();
    let glass = run(&mut doc, Op::AddMaterial { name: "Glass".into(), color: "#9FC9E8".into(), opacity: Some(0.45) }).created[0].clone();

    // two boxes
    let f = run(&mut doc, Op::DrawRectangle { corner: [0.0, 0.0, 0.0], opposite: [4.0, 0.0, 3.0], normal: up, u: None, layer: None, material: Some(brick.clone()) }).created[0].clone();
    run(&mut doc, Op::PushPull { face: f, distance: 2.5 });
    let f = run(&mut doc, Op::DrawRectangle { corner: [5.0, 0.0, 0.0], opposite: [7.0, 0.0, 2.0], normal: up, u: None, layer: None, material: Some(glass) }).created[0].clone();
    run(&mut doc, Op::PushPull { face: f, distance: 1.2 });

    // an L-shaped face pushed up
    let l = run(&mut doc, Op::DrawPolygon { points: vec![[0.0, 0.0, -6.0], [5.0, 0.0, -6.0], [5.0, 0.0, -4.0], [2.0, 0.0, -4.0], [2.0, 0.0, -1.5], [0.0, 0.0, -1.5]], layer: None, material: None }).created[0].clone();
    run(&mut doc, Op::PushPull { face: l, distance: 1.0 });

    // a circle and an offset face
    run(&mut doc, Op::DrawCircle { center: [9.0, 0.0, -3.0], normal: up, radius: 1.2, segments: Some(32), layer: None, material: Some(brick) });
    let sq = run(&mut doc, Op::DrawRectangle { corner: [8.0, 0.0, 1.0], opposite: [10.0, 0.0, 3.0], normal: up, u: None, layer: None, material: None }).created[0].clone();
    run(&mut doc, Op::Offset { face: sq, distance: 0.4 });

    // loose lines that close into a face
    run(&mut doc, Op::DrawLine { points: vec![[-3.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [-1.0, 0.0, 2.0], [-3.0, 0.0, 2.0]], close: true, layer: None });

    // annotations
    run(&mut doc, Op::AddDimension { a: [0.0, 0.0, 3.3], b: [4.0, 0.0, 3.3], offset: [0.0, 0.0, 0.5], layer: None });
    run(&mut doc, Op::AddText { position: [2.0, 3.2, 1.5], text: "Box A".into(), anchor: Some([2.0, 2.5, 1.5]), layer: None });
    run(&mut doc, Op::AddGuide { point: [0.0, 0.0, -7.0], direction: Some([1.0, 0.0, 0.0]), layer: None });

    // a group with a rotated copy
    let g = run(&mut doc, Op::DrawRectangle { corner: [-6.0, 0.0, -5.0], opposite: [-5.0, 0.0, -3.0], normal: up, u: None, layer: None, material: None }).created[0].clone();
    let r = run(&mut doc, Op::PushPull { face: g.clone(), distance: 0.6 });
    let mut ids = vec![g];
    ids.extend(r.created);
    let grp = run(&mut doc, Op::MakeGroup { ids, name: Some("Bench".into()) }).created[0].clone();
    let c = run(&mut doc, Op::Copy { ids: vec![grp], delta: [0.0, 0.0, 4.0], count: Some(1) }).created[0].clone();
    run(&mut doc, Op::Rotate { ids: vec![c], origin: [-5.5, 0.0, 0.0], axis: up, angle: 30.0 });

    // an inactive section plane
    run(&mut doc, Op::AddSection { origin: [0.0, 1.5, 0.0], normal: up, active: Some(false), layer: None });
    doc.extensions.insert("sample".into(), json!({ "generated": true }));
    doc.version = 1;
    doc
}
