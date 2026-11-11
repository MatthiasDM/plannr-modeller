//! Paste and import (`insert`), locking, and splitting two solids.

use plannr_core::geom::*;
use plannr_core::model::{Document, Entity};
use plannr_core::{apply, apply_many, Op, Registry};

fn reg() -> Registry { Registry::new() }
fn op(v: serde_json::Value) -> Op { serde_json::from_value(v).unwrap() }
fn run(doc: &mut Document, v: serde_json::Value) -> plannr_core::OpResult { apply(doc, &op(v), &reg()).unwrap() }
fn boxed(doc: &mut Document, min: P3, max: P3) -> String { run(doc, serde_json::json!({ "op": "solid_box", "min": min, "max": max })).created[0].clone() }
fn volume(doc: &Document, id: &str) -> f64 {
    let Some(Entity::Group(g)) = doc.find(id) else { panic!("no group {id}") };
    g.entities.iter().filter_map(|e| if let Entity::Face(f) = e { Some(f) } else { None }).map(|f| {
        let idx = tessellate(&f.outer, &f.holes);
        let pts: Vec<P3> = f.outer.iter().chain(f.holes.iter().flatten()).copied().collect();
        idx.chunks(3).map(|t| dot(pts[t[0] as usize], cross(pts[t[1] as usize], pts[t[2] as usize])) / 6.0).sum::<f64>()
    }).sum::<f64>().abs()
}

#[test]
fn inserted_entities_get_new_ids_and_fall_back_to_what_the_document_has() {
    let mut src = Document::default();
    let a = boxed(&mut src, [0.0; 3], [1.0, 1.0, 1.0]);
    run(&mut src, serde_json::json!({ "op": "paint", "ids": [a], "material": "#aa3322", "side": "both" }));
    let copied = serde_json::to_value(src.find(&a).unwrap()).unwrap();

    let mut doc = Document::default();
    let r = run(&mut doc, serde_json::json!({ "op": "insert", "entities": [copied, { "kind": "edge", "id": "e1", "layer": "nope", "a": [0, 0, 0], "b": [1, 0, 0] }], "delta": [5, 0, 0] }));
    assert_eq!(r.created.len(), 2);
    assert!(r.created.iter().all(|id| id != &a && id != "e1"), "new ids");
    let Some(Entity::Group(g)) = doc.find(&r.created[0]) else { panic!() };
    assert_eq!(g.entities.len(), 6);
    assert!(g.entities.iter().all(|e| matches!(e, Entity::Face(f) if f.material.is_none())), "a material this document lacks is dropped");
    assert!((g.transform[12] - 5.0).abs() < 1e-9, "moved by delta");
    let Some(Entity::Edge(e)) = doc.find(&r.created[1]) else { panic!() };
    assert_eq!(e.layer, doc.layers[0].id, "an unknown tag falls back to the default");
    assert_eq!(e.a, [5.0, 0.0, 0.0]);
    assert!(apply(&mut doc, &op(serde_json::json!({ "op": "insert", "entities": [] })), &reg()).is_err());

    // an imported mesh: the two triangles of a square become one face
    let tri = |id: &str, pts: [[f64; 3]; 3]| serde_json::json!({ "kind": "face", "id": id, "layer": "", "outer": pts });
    let mesh = serde_json::json!({ "kind": "group", "id": "g", "layer": "", "name": "mesh", "entities": [tri("a", [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]]), tri("b", [[0.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]])] });
    let r = run(&mut doc, serde_json::json!({ "op": "insert", "entities": [mesh], "merge": true }));
    let Some(Entity::Group(g)) = doc.find(&r.created[0]) else { panic!() };
    assert_eq!(g.entities.len(), 1);
    assert!(matches!(&g.entities[0], Entity::Face(f) if f.outer.len() == 4 && (area(&f.outer) - 1.0).abs() < 1e-9));
}

#[test]
fn an_import_keeps_its_layers_by_name_and_merges_tag_by_tag() {
    let mut doc = Document::default();
    // the tags come first in the same batch; one of them switched off, as in the drawing
    let walls = run(&mut doc, serde_json::json!({ "op": "add_layer", "name": "WALLS", "color": "#ff0000" })).created[0].clone();
    let notes = run(&mut doc, serde_json::json!({ "op": "add_layer", "name": "NOTES", "visible": false })).created[0].clone();
    assert!(doc.layers.iter().any(|l| l.id == walls && l.visible) && doc.layers.iter().any(|l| l.id == notes && !l.visible));

    let tri = |layer: &str, pts: [[f64; 3]; 3]| serde_json::json!({ "kind": "face", "id": "f", "layer": layer, "outer": pts });
    let (a, b) = ([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]], [[0.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]]);
    let (c, d) = ([[1.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 0.0, 1.0]], [[1.0, 0.0, 0.0], [2.0, 0.0, 1.0], [1.0, 0.0, 1.0]]);
    let block = serde_json::json!({ "kind": "group", "id": "b", "layer": "NOTES", "name": "DOOR", "entities": [tri("0", a), tri("0", b), { "kind": "edge", "id": "e", "layer": "WALLS", "a": [0, 0, 0], "b": [0, 1, 0] }] });
    let drawing = serde_json::json!({ "kind": "group", "id": "g", "layer": "0", "name": "plan", "entities": [
        { "kind": "edge", "id": "e", "layer": "WALLS", "a": [0, 0, 0], "b": [4, 0, 0] }, tri("WALLS", a), tri("WALLS", b), tri("NOTES", c), tri("NOTES", d), block ] });
    let r = run(&mut doc, serde_json::json!({ "op": "insert", "entities": [drawing], "merge": true }));
    let Some(Entity::Group(g)) = doc.find(&r.created[0]) else { panic!() };
    assert_eq!(g.layer, doc.layers[0].id, "a name the document lacks is the default tag");
    // the edge, one square per tag (the two squares touch in one plane but stay apart), the block
    let layers: Vec<(&str, &str)> = g.entities.iter().map(|e| (match e { Entity::Edge(_) => "edge", Entity::Face(_) => "face", Entity::Group(_) => "group", _ => "?" }, e.layer())).collect();
    assert_eq!(layers, vec![("edge", walls.as_str()), ("group", notes.as_str()), ("face", walls.as_str()), ("face", notes.as_str())]);
    assert!(g.entities.iter().all(|e| !matches!(e, Entity::Face(f) if f.outer.len() != 4)));
    // the group inside is merged on its own, and its faces are not copied into the group around it
    let Some(Entity::Group(inner)) = g.entities.iter().find(|e| matches!(e, Entity::Group(_))) else { panic!() };
    assert_eq!(inner.entities.len(), 2);
    assert!(inner.entities.iter().any(|e| matches!(e, Entity::Face(f) if f.outer.len() == 4 && f.layer == doc.layers[0].id)));
    assert!(inner.entities.iter().any(|e| matches!(e, Entity::Edge(e) if e.layer == walls)));
}

#[test]
fn a_locked_entity_refuses_changes_until_it_is_unlocked() {
    let mut doc = Document::default();
    let a = boxed(&mut doc, [0.0; 3], [1.0, 1.0, 1.0]);
    let Some(Entity::Group(g)) = doc.find(&a) else { panic!() };
    let inner = g.entities[0].id().to_string();
    run(&mut doc, serde_json::json!({ "op": "set_locked", "ids": [a], "locked": true }));
    for v in [
        serde_json::json!({ "op": "move", "ids": [a], "delta": [1, 0, 0] }),
        serde_json::json!({ "op": "erase", "ids": [a] }),
        serde_json::json!({ "op": "explode", "id": a }),
        serde_json::json!({ "op": "push_pull", "face": inner, "distance": 1 }),
        serde_json::json!({ "op": "paint", "ids": [inner], "material": "#ff0000" }),
    ] {
        let e = apply(&mut doc, &op(v), &reg()).unwrap_err();
        assert!(e.0.contains("locked"), "{}", e.0);
    }
    // copying and hiding are allowed
    let r = run(&mut doc, serde_json::json!({ "op": "copy", "ids": [a], "delta": [3, 0, 0] }));
    assert_eq!(r.created.len(), 1);
    run(&mut doc, serde_json::json!({ "op": "set_hidden", "ids": [a], "hidden": true }));
    run(&mut doc, serde_json::json!({ "op": "set_locked", "ids": [a], "locked": false }));
    run(&mut doc, serde_json::json!({ "op": "move", "ids": [a], "delta": [1, 0, 0] }));
}

#[test]
fn split_cuts_two_solids_along_each_other() {
    let mut doc = Document::default();
    let a = boxed(&mut doc, [0.0; 3], [2.0, 2.0, 2.0]);
    let b = boxed(&mut doc, [1.0, 0.5, 0.5], [3.0, 1.5, 1.5]);
    let r = run(&mut doc, serde_json::json!({ "op": "boolean", "mode": "split", "target": a, "tools": [b] }));
    assert_eq!(r.created.len(), 1);
    assert!((volume(&doc, &a) - 7.0).abs() < 1e-6, "the target without the shared part");
    assert!((volume(&doc, &b) - 1.0).abs() < 1e-6, "the tool without the shared part");
    assert!((volume(&doc, &r.created[0]) - 1.0).abs() < 1e-6, "the shared part");
    assert_eq!(doc.entities.len(), 3);

    // solids that don't overlap: an error, and nothing is left half done
    let mut doc = Document::default();
    let a = boxed(&mut doc, [0.0; 3], [1.0, 1.0, 1.0]);
    let b = boxed(&mut doc, [5.0, 0.0, 0.0], [6.0, 1.0, 1.0]);
    let before = doc.clone();
    assert!(apply_many(&mut doc, &[op(serde_json::json!({ "op": "boolean", "mode": "split", "target": a, "tools": [b] }))], &reg()).is_err());
    assert_eq!(doc.entities, before.entities);
}
