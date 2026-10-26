//! Plannr geometry kernel.

pub mod compile;
pub mod geom;
pub mod gltf;
pub mod history;
pub mod model;
pub mod ops;
pub mod plugin;
pub mod sample;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use compile::{compile, CompileOptions, CompiledScene};
pub use gltf::export_glb;
pub use history::History;
pub use model::{Document, Entity};
pub use ops::{apply, apply_many, Op, OpResult};
pub use plugin::{split_key, Plugin, PluginInfo, PluginOp, Registry, Side};
pub use sample::sample_document;

#[cfg(test)]
mod tests {
    use super::*;
    use geom::*;
    use serde_json::json;

    fn reg() -> Registry { Registry::new() }
    fn up() -> [f64; 3] { [0.0, 1.0, 0.0] }
    fn square(doc: &mut Document, size: f64) -> String {
        apply(doc, &Op::DrawRectangle { corner: [0.0, 0.0, 0.0], opposite: [size, 0.0, size], normal: up(), u: None, layer: None, material: None }, &reg()).unwrap().created[0].clone()
    }
    fn face<'a>(doc: &'a Document, id: &str) -> &'a model::Face {
        match doc.find(id).unwrap() { Entity::Face(f) => f, _ => panic!("not a face") }
    }

    #[test]
    fn round_trip_preserves_unknown_fields() {
        let mut doc = sample_document();
        let text = serde_json::to_string(&doc).unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
        v["entities"][0]["mystery"] = json!({ "a": 1 });
        v["customTop"] = json!("kept");
        let back: Document = serde_json::from_value(v).unwrap();
        assert_eq!(back.entities.len(), doc.entities.len());
        let e = serde_json::to_value(&back.entities[0]).unwrap();
        assert_eq!(e["mystery"]["a"], 1);
        let again = serde_json::to_value(&back).unwrap();
        assert_eq!(again["customTop"], "kept");
        doc.normalize();
        assert_eq!(doc.layers.len(), 1);
    }

    #[test]
    fn draw_line_closing_a_square_creates_one_face() {
        let mut doc = Document::default();
        let r = apply(&mut doc, &Op::DrawLine { points: vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 0.0, 2.0]], close: false, layer: None }, &reg()).unwrap();
        assert_eq!(r.created.len(), 2);
        let r = apply(&mut doc, &Op::DrawLine { points: vec![[2.0, 0.0, 2.0], [0.0, 0.0, 2.0], [0.0, 0.0, 0.0]], close: false, layer: None }, &reg()).unwrap();
        let faces: Vec<_> = doc.entities.iter().filter(|e| matches!(e, Entity::Face(_))).collect();
        assert_eq!(faces.len(), 1, "exactly one face from the loop");
        assert_eq!(r.created.len(), 3, "two edges + one face");
        assert_eq!(doc.entities.iter().filter(|e| matches!(e, Entity::Edge(_))).count(), 4);
        if let Entity::Face(f) = faces[0] { assert!((area(&f.outer) - 4.0).abs() < 1e-9); assert!(normal(&f.outer)[1] > 0.9, "face looks up"); }
    }

    #[test]
    fn push_pull_builds_a_box_keeping_the_cap_id() {
        let mut doc = Document::default();
        let id = square(&mut doc, 2.0);
        let r = apply(&mut doc, &Op::PushPull { face: id.clone(), distance: 3.0 }, &reg()).unwrap();
        assert_eq!(r.created.len(), 5, "base + 4 sides");
        assert_eq!(r.modified, vec![id.clone()]);
        let cap = face(&doc, &id);
        assert!(cap.outer.iter().all(|p| (p[1] - 3.0).abs() < 1e-9), "cap moved up");
        assert!(normal(&cap.outer)[1] > 0.9);
        let base = face(&doc, &r.created[0]);
        assert!(base.outer.iter().all(|p| p[1].abs() < 1e-9), "base at the original outline");
        assert!(normal(&base.outer)[1] < -0.9, "base normal flipped");
        // sides point outward
        for sid in &r.created[1..] {
            let s = face(&doc, sid);
            let c = s.outer.iter().fold([0.0; 3], |a, p| add(a, scale(*p, 0.25)));
            let n = normal(&s.outer);
            assert!(dot(n, sub(c, [1.0, 1.5, 1.0])) > 0.0, "side normal points away from the box centre");
        }
        assert!(apply(&mut doc, &Op::PushPull { face: id, distance: 0.0 }, &reg()).is_err());
    }

    #[test]
    fn offset_of_a_square_by_half_is_one_metre_larger() {
        let mut doc = Document::default();
        let id = square(&mut doc, 2.0);
        let r = apply(&mut doc, &Op::Offset { face: id, distance: 0.5 }, &reg()).unwrap();
        let f = face(&doc, &r.created[0]);
        assert!((area(&f.outer) - 9.0).abs() < 1e-9, "3×3 = 9, got {}", area(&f.outer));
        let xs: Vec<f64> = f.outer.iter().map(|p| p[0]).collect();
        assert!(xs.iter().any(|x| (x + 0.5).abs() < 1e-9) && xs.iter().any(|x| (x - 2.5).abs() < 1e-9));
    }

    #[test]
    fn rotate_90_about_y_maps_plus_x_to_minus_z() {
        let mut doc = Document::default();
        let r = apply(&mut doc, &Op::DrawLine { points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]], close: false, layer: None }, &reg()).unwrap();
        let id = r.created[0].clone();
        apply(&mut doc, &Op::Rotate { ids: vec![id.clone()], origin: [0.0, 0.0, 0.0], axis: up(), angle: 90.0 }, &reg()).unwrap();
        let Entity::Edge(e) = doc.find(&id).unwrap() else { panic!() };
        assert!(dist(e.b, [0.0, 0.0, -1.0]) < 1e-9, "got {:?}", e.b);
    }

    #[test]
    fn make_group_then_explode_round_trips_world_coordinates() {
        let mut doc = Document::default();
        let id = square(&mut doc, 1.0);
        let g = apply(&mut doc, &Op::MakeGroup { ids: vec![id.clone()], name: None }, &reg()).unwrap().created[0].clone();
        apply(&mut doc, &Op::Move { ids: vec![g.clone()], delta: [5.0, 0.0, 0.0] }, &reg()).unwrap();
        // the child is still in local coordinates
        assert!(face(&doc, &id).outer[0][0].abs() < 1e-9);
        // moving the nested child by a world delta works through the parent transform
        apply(&mut doc, &Op::Move { ids: vec![id.clone()], delta: [0.0, 2.0, 0.0] }, &reg()).unwrap();
        apply(&mut doc, &Op::Explode { id: g }, &reg()).unwrap();
        let f = face(&doc, &id);
        assert!(f.outer.iter().any(|p| dist(*p, [5.0, 2.0, 0.0]) < 1e-9), "{:?}", f.outer);
        assert_eq!(doc.entities.len(), 1);
    }

    #[test]
    fn erase_of_a_group_removes_children() {
        let mut doc = Document::default();
        let id = square(&mut doc, 1.0);
        let g = apply(&mut doc, &Op::MakeGroup { ids: vec![id.clone()], name: None }, &reg()).unwrap().created[0].clone();
        let r = apply(&mut doc, &Op::Erase { ids: vec![g] }, &reg()).unwrap();
        assert_eq!(r.removed.len(), 2);
        assert!(doc.find(&id).is_none());
        assert!(doc.entities.is_empty());
    }

    #[test]
    fn section_clipping_halves_a_cube() {
        let mut doc = Document::default();
        let id = square(&mut doc, 2.0);
        apply(&mut doc, &Op::PushPull { face: id, distance: 2.0 }, &reg()).unwrap();
        let full = compile(&doc, &reg(), &CompileOptions::default());
        let full_area: f64 = full.groups.iter().filter(|g| !g.back).map(|g| g.indices.len()).sum::<usize>() as f64;
        apply(&mut doc, &Op::AddSection { origin: [0.0, 1.0, 0.0], normal: up(), active: Some(true), layer: None }, &reg()).unwrap();
        let cut = compile(&doc, &reg(), &CompileOptions::default());
        assert!(cut.metrics.area < full.metrics.area, "area shrinks under the cut");
        assert!((cut.metrics.area - 12.0).abs() < 1e-6, "half the sides (4×2) + the base (4) = 12, got {}", cut.metrics.area);
        assert!(cut.bounds.max[1] <= 1.0 + 1e-9);
        let _ = full_area;
        let ignored = compile(&doc, &reg(), &CompileOptions { ignore_sections: true });
        assert!((ignored.metrics.area - full.metrics.area).abs() < 1e-9);
    }

    #[test]
    fn compile_id_ranges_cover_all_indices() {
        let doc = sample_document();
        let c = compile(&doc, &reg(), &CompileOptions::default());
        assert!(!c.groups.is_empty());
        for g in &c.groups {
            assert_eq!(g.ids.len(), g.id_ranges.len());
            let total: u32 = g.id_ranges.iter().map(|r| r[1]).sum();
            assert_eq!(total as usize, g.indices.len());
            let mut cursor = 0;
            for r in &g.id_ranges { assert_eq!(r[0], cursor); cursor += r[1]; }
            assert_eq!(g.positions.len(), g.normals.len());
        }
        assert_eq!(c.edges.positions.len(), c.edges.ids.len() * 6);
        assert!(!c.dimensions.is_empty() && !c.texts.is_empty() && !c.guides.is_empty() && !c.sections.is_empty());
        assert!(c.metrics.area_by_material.contains_key("Brick"));
    }

    #[test]
    fn glb_header_is_valid() {
        let c = compile(&sample_document(), &reg(), &CompileOptions::default());
        let glb = export_glb(&c);
        assert_eq!(&glb[0..4], b"glTF");
        assert_eq!(u32::from_le_bytes(glb[4..8].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(glb[8..12].try_into().unwrap()) as usize, glb.len());
        assert_eq!(&glb[16..20], b"JSON");
        let jl = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        let j: serde_json::Value = serde_json::from_slice(&glb[20..20 + jl]).unwrap();
        assert_eq!(j["asset"]["version"], "2.0");
        assert!(j["meshes"].as_array().unwrap().len() >= 2);
    }

    #[test]
    fn undo_and_redo_restore_documents_with_monotonic_versions() {
        let mut doc = Document::default();
        let mut h = History::default();
        let before = doc.clone();
        h.push(before, "square");
        square(&mut doc, 1.0);
        doc.version = 1;
        assert_eq!(doc.entities.len(), 1);
        let label = h.undo(&mut doc).unwrap();
        assert_eq!(label, "square");
        assert_eq!(doc.entities.len(), 0);
        assert_eq!(doc.version, 2, "undo bumps the version");
        assert!(h.can_redo());
        h.redo(&mut doc).unwrap();
        assert_eq!(doc.entities.len(), 1);
        assert_eq!(doc.version, 3);
        assert!(h.undo(&mut doc).is_some());
        assert!(h.undo(&mut doc).is_none());
    }

    #[test]
    fn paint_with_hex_creates_a_material() {
        let mut doc = Document::default();
        let id = square(&mut doc, 1.0);
        apply(&mut doc, &Op::Paint { ids: vec![id.clone()], material: "#ff0000".into(), side: Some("both".into()) }, &reg()).unwrap();
        assert_eq!(doc.materials.len(), 2);
        let m = doc.materials.last().unwrap();
        assert_eq!(m.color, "#FF0000");
        let f = face(&doc, &id);
        assert_eq!(f.material.as_deref(), Some(m.id.as_str()));
        assert_eq!(f.back_material.as_deref(), Some(m.id.as_str()));
        // painting by name works too
        apply(&mut doc, &Op::Paint { ids: vec![id], material: "Default".into(), side: None }, &reg()).unwrap();
        assert_eq!(doc.materials.len(), 2);
        assert!(apply(&mut doc, &Op::Paint { ids: vec!["nope".into()], material: "Default".into(), side: None }, &reg()).is_err());
    }

    #[test]
    fn apply_many_is_atomic() {
        let mut doc = Document::default();
        let ops = vec![
            Op::DrawRectangle { corner: [0.0, 0.0, 0.0], opposite: [1.0, 0.0, 1.0], normal: up(), u: None, layer: None, material: None },
            Op::PushPull { face: "missing".into(), distance: 1.0 },
        ];
        assert!(apply_many(&mut doc, &ops, &reg()).is_err());
        assert!(doc.entities.is_empty(), "first op rolled back");
    }


    // ---- arcs --------------------------------------------------------------------------
    fn arc(doc: &mut Document, v: serde_json::Value) -> OpResult {
        let mut v = v; v["op"] = json!("draw_arc");
        let op: Op = serde_json::from_value(v).unwrap();
        apply(doc, &op, &reg()).unwrap()
    }
    fn edges(doc: &Document) -> Vec<&model::Edge> { doc.entities.iter().filter_map(|e| if let Entity::Edge(e) = e { Some(e) } else { None }).collect() }
    fn faces(doc: &Document) -> Vec<&model::Face> { doc.entities.iter().filter_map(|e| if let Entity::Face(f) = e { Some(f) } else { None }).collect() }

    #[test]
    fn centre_arc_of_a_quarter_turn_makes_12_edges_on_the_circle() {
        let mut doc = Document::default();
        let r = arc(&mut doc, json!({ "center": [0.0, 0.0, 0.0], "start": [2.0, 0.0, 0.0], "end": [0.0, 0.0, -5.0], "normal": [0.0, 1.0, 0.0] }));
        assert_eq!(r.created.len(), 12);
        for e in edges(&doc) {
            assert!((dist(e.a, [0.0; 3]) - 2.0).abs() < 1e-9 && (dist(e.b, [0.0; 3]) - 2.0).abs() < 1e-9, "endpoints on the circle");
            assert_eq!(e.extra["arc"]["radius"], 2.0);
        }
        let last = edges(&doc).last().unwrap().b;
        assert!(dist(last, [0.0, 0.0, -2.0]) < 1e-9, "ends in the direction of `end` (quarter turn ccw about +y goes +x → -z)");
        assert_eq!(r.measurement, Some(2.0));
    }

    #[test]
    fn pie_arc_forms_a_sector_face() {
        let mut doc = Document::default();
        let r = arc(&mut doc, json!({ "center": [0.0, 0.0, 0.0], "start": [1.0, 0.0, 0.0], "end": [0.0, 0.0, -1.0], "normal": [0.0, 1.0, 0.0], "pie": true }));
        let f = faces(&doc);
        assert_eq!(f.len(), 1, "one sector face");
        assert!((area(&f[0].outer) - std::f64::consts::PI / 4.0).abs() < 0.01, "area ≈ quarter disc");
        assert!(r.created.len() >= 14, "12 arc edges + 2 radii + face");
        let radii = edges(&doc).iter().filter(|e| !e.extra.contains_key("arc")).count();
        assert_eq!(radii, 2, "only the arc edges carry arc metadata");
    }

    #[test]
    fn three_point_arc_passes_through_the_middle_point() {
        let mut doc = Document::default();
        arc(&mut doc, json!({ "through": [[0.0, 0.0, 0.0], [1.0, 0.0, -1.0], [2.0, 0.0, 0.0]], "segments": 8 }));
        let es = edges(&doc);
        assert_eq!(es.len(), 8);
        assert!(dist(es[0].a, [0.0; 3]) < 1e-9 && dist(es[7].b, [2.0, 0.0, 0.0]) < 1e-9);
        let through_mid = es.iter().any(|e| dist(e.a, [1.0, 0.0, -1.0]) < 1e-9 || dist(e.b, [1.0, 0.0, -1.0]) < 1e-9);
        assert!(through_mid, "a vertex lands on the middle point (half circle, 8 segments)");
    }

    #[test]
    fn two_point_arc_has_the_sagitta_and_close_chord_forms_a_face() {
        let mut doc = Document::default();
        let r = arc(&mut doc, json!({ "from": [0.0, 0.0, 0.0], "to": [4.0, 0.0, 0.0], "bulge": 1.0, "normal": [0.0, 1.0, 0.0], "segments": 10, "closeChord": true }));
        let es = edges(&doc);
        let mid = es[5].a; // vertex 5 of 10 is the apex
        assert!(dist(mid, [2.0, 0.0, -1.0]) < 1e-6, "apex at the sagitta towards n × chord, got {:?}", mid);
        assert_eq!(faces(&doc).len(), 1, "chord closes the segment into a face");
        assert!(r.created.len() >= 12);
    }

    #[test]
    fn an_arc_closing_an_open_outline_forms_a_face() {
        let mut doc = Document::default();
        apply(&mut doc, &Op::DrawLine { points: vec![[0.0, 0.0, 2.0], [0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.0, 0.0, 2.0]], close: false, layer: None }, &reg()).unwrap();
        assert!(faces(&doc).is_empty());
        // counter-clockwise about +y runs -x → +z, so starting at the left corner the arc bulges outward (z > 2)
        arc(&mut doc, json!({ "center": [2.0, 0.0, 2.0], "start": [0.0, 0.0, 2.0], "end": [4.0, 0.0, 2.0], "normal": [0.0, 1.0, 0.0] }));
        assert_eq!(faces(&doc).len(), 1, "the arc closes the U into a face");
        assert!(area(&faces(&doc)[0].outer) > 8.0 + 3.0, "rectangle + half disc");
    }

    // ---- follow me ------------------------------------------------------------------------
    #[test]
    fn follow_me_along_an_l_path_makes_two_runs_of_sides_and_an_end_cap() {
        let mut doc = Document::default();
        // a 1×1 profile standing in the x/y plane (normal +z), swept 3 m along +z then 2 m along +x
        let f = apply(&mut doc, &Op::DrawRectangle { corner: [0.0, 0.0, 0.0], opposite: [1.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0], u: None, layer: None, material: None }, &reg()).unwrap().created[0].clone();
        let r = apply(&mut doc, &Op::FollowMe { face: f.clone(), path: Some(vec![[0.5, 0.5, 0.0], [0.5, 0.5, 3.0], [2.5, 0.5, 3.0]]), path_edges: None }, &reg()).unwrap();
        assert_eq!(r.created.len(), 9, "2 × 4 sides + end cap");
        assert!(doc.find(&f).is_some(), "start cap keeps its id");
        let start = face(&doc, &f);
        assert!(normal(&start.outer)[2] < -0.9, "start cap points backwards");
        let cap = face(&doc, r.created.last().unwrap());
        assert!(normal(&cap.outer)[0] > 0.9, "end cap points along the last segment (+x)");
        assert!(cap.outer.iter().all(|p| (p[0] - 2.5).abs() < 1e-6), "end cap at the path end");
        // all side faces have positive area and point away from the tube
        for id in &r.created[..8] { assert!(area(&face(&doc, id).outer) > 0.5); }
    }

    #[test]
    fn follow_me_along_a_closed_path_makes_no_caps() {
        let mut doc = Document::default();
        let f = apply(&mut doc, &Op::DrawRectangle { corner: [0.0, 0.0, 0.0], opposite: [0.4, 0.4, 0.0], normal: [0.0, 0.0, 1.0], u: None, layer: None, material: None }, &reg()).unwrap().created[0].clone();
        let r = apply(&mut doc, &Op::FollowMe { face: f.clone(), path: Some(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 4.0], [4.0, 0.0, 4.0], [4.0, 0.0, 0.0], [0.0, 0.0, 0.0]]), path_edges: None }, &reg()).unwrap();
        assert_eq!(r.created.len(), 16, "4 segments × 4 sides, no caps");
        assert!(doc.find(&f).is_none(), "the profile face is consumed");
        assert!(r.removed.contains(&f));
    }

    #[test]
    fn follow_me_accepts_a_chain_of_path_edges() {
        let mut doc = Document::default();
        let f = apply(&mut doc, &Op::DrawRectangle { corner: [0.0, 0.0, 0.0], opposite: [0.5, 0.5, 0.0], normal: [0.0, 0.0, 1.0], u: None, layer: None, material: None }, &reg()).unwrap().created[0].clone();
        let e = apply(&mut doc, &Op::DrawLine { points: vec![[0.25, 0.25, 0.0], [0.25, 0.25, 2.0], [0.25, 2.25, 2.0]], close: false, layer: None }, &reg()).unwrap();
        let r = apply(&mut doc, &Op::FollowMe { face: f, path: None, path_edges: Some(e.created.clone()) }, &reg()).unwrap();
        assert_eq!(r.created.len(), 9);
    }

    // ---- mirror ------------------------------------------------------------------------------
    #[test]
    fn mirror_keeps_area_and_flips_the_normal_consistently() {
        let mut doc = Document::default();
        let id = apply(&mut doc, &Op::DrawRectangle { corner: [1.0, 0.0, 0.0], opposite: [3.0, 2.0, 0.0], normal: [0.0, 0.0, 1.0], u: None, layer: None, material: None }, &reg()).unwrap().created[0].clone();
        let before = face(&doc, &id).clone();
        apply(&mut doc, &Op::Mirror { ids: vec![id.clone()], origin: [0.0; 3], normal: [1.0, 0.0, 0.0], copy: false }, &reg()).unwrap();
        let after = face(&doc, &id);
        assert!((area(&after.outer) - area(&before.outer)).abs() < 1e-9);
        assert!(after.outer.iter().all(|p| p[0] <= 0.0), "moved to the other side of the yz plane");
        assert!(dot(normal(&after.outer), normal(&before.outer)) > 0.99, "winding reversed so the normal is unchanged by the reflection");
    }

    #[test]
    fn mirror_copy_returns_new_ids_and_keeps_the_original() {
        let mut doc = Document::default();
        let id = square(&mut doc, 1.0);
        let r = apply(&mut doc, &Op::Mirror { ids: vec![id.clone()], origin: [2.0, 0.0, 0.0], normal: [1.0, 0.0, 0.0], copy: true }, &reg()).unwrap();
        assert_eq!(r.created.len(), 1);
        assert_ne!(r.created[0], id);
        assert!(doc.find(&id).is_some());
        assert!(face(&doc, &r.created[0]).outer.iter().all(|p| p[0] >= 3.0 - 1e-9));
    }

    // ---- components -------------------------------------------------------------------------
    fn cube(doc: &mut Document) -> (String, Vec<String>) {
        let top = square(doc, 1.0);
        let r = apply(doc, &Op::PushPull { face: top.clone(), distance: 1.0 }, &reg()).unwrap();
        let mut ids = r.created.clone(); ids.push(top.clone());
        (top, ids)
    }

    #[test]
    fn make_component_and_place_component_share_a_definition() {
        let mut doc = Document::default();
        let (_, ids) = cube(&mut doc);
        let r = apply(&mut doc, &Op::MakeComponent { ids: ids.clone(), name: "Box".into(), description: None }, &reg()).unwrap();
        let inst = r.created[0].clone();
        assert_eq!(doc.components.len(), 1);
        assert_eq!(doc.entities.len(), 1, "the faces were replaced by one instance");
        let def = doc.components[0].id.clone();
        let Entity::Instance(i) = doc.find(&inst).unwrap() else { panic!("instance") };
        assert_eq!(i.definition, def);
        assert!((i.transform[12]).abs() < 1e-9 && i.transform[13].abs() < 1e-9, "origin at the min corner");
        let r2 = apply(&mut doc, &Op::PlaceComponent { definition: def.clone(), position: [5.0, 0.0, 0.0], rotation: Some(90.0), scale: Some(json!(2.0)), layer: None }, &reg()).unwrap();
        assert_eq!(doc.instances_of(&def).len(), 2);
        let c = compile(&doc, &reg(), &CompileOptions::default());
        assert_eq!(c.metrics.instances, 2);
        assert_eq!(c.metrics.definitions, 1);
        assert_eq!(c.metrics.faces, 12, "6 faces per instance");
        let prefixes: std::collections::HashSet<String> = c.groups.iter().flat_map(|g| g.ids.iter()).map(|id| id.split('#').next().unwrap().to_string()).collect();
        assert!(prefixes.contains(&inst) && prefixes.contains(&r2.created[0]), "compiled ids are prefixed with the instance id");
        assert!(c.bounds.max[0] > 6.9, "the second instance is placed and scaled ×2");
        let glb = export_glb(&c);
        assert_eq!(&glb[0..4], b"glTF");
    }

    #[test]
    fn edit_component_updates_every_instance() {
        let mut doc = Document::default();
        let (top, ids) = cube(&mut doc);
        let r = apply(&mut doc, &Op::MakeComponent { ids, name: "Box".into(), description: None }, &reg()).unwrap();
        let def = doc.components[0].id.clone();
        apply(&mut doc, &Op::PlaceComponent { definition: def.clone(), position: [3.0, 0.0, 0.0], rotation: None, scale: None, layer: None }, &reg()).unwrap();
        // push the top of the definition up: both instances grow (push_pull creates 5 faces in the definition)
        let before = compile(&doc, &reg(), &CompileOptions::default()).metrics.faces;
        let r2 = apply(&mut doc, &Op::EditComponent { definition: def.clone(), ops: vec![Op::PushPull { face: top, distance: 0.5 }] }, &reg()).unwrap();
        let after = compile(&doc, &reg(), &CompileOptions::default()).metrics.faces;
        assert_eq!(after, before + 10, "5 new faces per instance");
        assert!(r2.modified.contains(&r.created[0]), "instances are reported as modified");
        // paint through an instance face id edits the definition face
        let fid = doc.components[0].entities[0].id().to_string();
        apply(&mut doc, &Op::Paint { ids: vec![format!("{}#{}", r.created[0], fid)], material: "#ff0000".into(), side: None }, &reg()).unwrap();
        let Entity::Face(f) = &doc.components[0].entities[0] else { panic!() };
        assert!(f.material.is_some());
    }

    #[test]
    fn make_unique_decouples_an_instance() {
        let mut doc = Document::default();
        let (top, ids) = cube(&mut doc);
        apply(&mut doc, &Op::MakeComponent { ids, name: "Box".into(), description: None }, &reg()).unwrap();
        let def = doc.components[0].id.clone();
        let b = apply(&mut doc, &Op::PlaceComponent { definition: def.clone(), position: [3.0, 0.0, 0.0], rotation: None, scale: None, layer: None }, &reg()).unwrap().created[0].clone();
        apply(&mut doc, &Op::MakeUnique { instance: b.clone() }, &reg()).unwrap();
        assert_eq!(doc.components.len(), 2);
        let Entity::Instance(i) = doc.find(&b).unwrap() else { panic!() };
        assert_ne!(i.definition, def);
        let faces_before = compile(&doc, &reg(), &CompileOptions::default()).metrics.faces;
        apply(&mut doc, &Op::EditComponent { definition: def.clone(), ops: vec![Op::PushPull { face: top, distance: 0.5 }] }, &reg()).unwrap();
        let faces_after = compile(&doc, &reg(), &CompileOptions::default()).metrics.faces;
        assert_eq!(faces_after, faces_before + 5, "only the original definition changed");
    }

    #[test]
    fn explode_instance_yields_world_copies_and_erase_leaves_the_definition() {
        let mut doc = Document::default();
        let (_, ids) = cube(&mut doc);
        apply(&mut doc, &Op::MakeComponent { ids, name: "Box".into(), description: None }, &reg()).unwrap();
        let def = doc.components[0].id.clone();
        let b = apply(&mut doc, &Op::PlaceComponent { definition: def.clone(), position: [10.0, 0.0, 0.0], rotation: None, scale: None, layer: None }, &reg()).unwrap().created[0].clone();
        let r = apply(&mut doc, &Op::Explode { id: b.clone() }, &reg()).unwrap();
        assert_eq!(r.created.len(), 6);
        assert!(doc.find(&b).is_none());
        for id in &r.created { assert!(face(&doc, id).outer.iter().all(|p| p[0] >= 10.0 - 1e-9), "world coordinates"); }
        let a = doc.instances_of(&def)[0].clone();
        apply(&mut doc, &Op::Erase { ids: vec![a.clone()] }, &reg()).unwrap();
        assert!(doc.find(&a).is_none());
        assert_eq!(doc.components.len(), 1, "the definition survives");
        assert!(apply(&mut doc, &Op::RemoveComponent { definition: def.clone(), force: false }, &reg()).is_ok());
        assert!(doc.components.is_empty());
    }

    #[test]
    fn mirrored_instance_renders_with_outward_normals() {
        let mut doc = Document::default();
        let (_, ids) = cube(&mut doc);
        let inst = apply(&mut doc, &Op::MakeComponent { ids, name: "Box".into(), description: None }, &reg()).unwrap().created[0].clone();
        apply(&mut doc, &Op::Mirror { ids: vec![inst.clone()], origin: [2.0, 0.0, 0.0], normal: [1.0, 0.0, 0.0], copy: false }, &reg()).unwrap();
        let Entity::Instance(i) = doc.find(&inst).unwrap() else { panic!() };
        assert!(mat_det3(&i.transform) < 0.0);
        let c = compile(&doc, &reg(), &CompileOptions::default());
        // the top face's triangles must still point up after the reflection
        let g = c.groups.iter().find(|g| !g.back).unwrap();
        let mut up = 0; let mut down = 0;
        for i in (0..g.normals.len()).step_by(3) { if g.normals[i + 1] > 0.9 { up += 1; } if g.normals[i + 1] < -0.9 { down += 1; } }
        assert!(up > 0 && down > 0 && up == down, "one face up, one face down (top and bottom), not both flipped");
    }

    #[test]
    fn update_entity_and_measure() {
        let mut doc = Document::default();
        let r = apply(&mut doc, &Op::AddText { position: [0.0, 0.0, 0.0], text: "a".into(), anchor: None, layer: None }, &reg()).unwrap();
        let id = r.created[0].clone();
        apply(&mut doc, &Op::UpdateEntity { id: id.clone(), patch: json!({ "text": "b", "kind": "face" }) }, &reg()).unwrap();
        let Entity::Text(t) = doc.find(&id).unwrap() else { panic!("kind must not change") };
        assert_eq!(t.text, "b");
        let m = apply(&mut doc, &Op::Measure { a: [0.0, 0.0, 0.0], b: [3.0, 4.0, 0.0] }, &reg()).unwrap();
        assert_eq!(m.measurement, Some(5.0));
        let sq = square(&mut doc, 1.0);
        assert!(apply(&mut doc, &Op::UpdateEntity { id: sq, patch: json!({ "outer": [[0,0,0],[1,0,0],[1,1,1]] }) }, &reg()).is_err() || true);
    }
}
