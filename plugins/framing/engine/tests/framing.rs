use plannr_core::compile::{compile, CompileOptions};
use plannr_core::model::{Document, Entity};
use plannr_core::ops::{apply as core_apply, Op, OpResult};
use plannr_core::plugin::Registry;
use plannr_plugin_construction::ConstructionPlugin;
use plannr_plugin_framing::{data::*, expand::*, FramingPlugin};
use serde_json::{json, Value};

fn registry() -> Registry { let mut r = Registry::new(); r.register(Box::new(ConstructionPlugin)); r.register(Box::new(FramingPlugin)); r }
fn doc() -> Document { let mut d = Document::default(); registry().on_load(&mut d); d }
fn fr(doc: &mut Document, op: &str, data: Value) -> Result<OpResult, plannr_core::ops::OpError> { registry().apply(&format!("framing.{op}"), data, doc) }
fn data_of(r: &OpResult) -> Value { match &r.entities[0] { Entity::Custom(c) => c.data.clone(), _ => Value::Null } }
fn custom(doc: &Document, id: &str) -> plannr_core::model::Custom { doc.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.id == id => Some(c.clone()), _ => None }).unwrap() }
fn expansion(doc: &Document, id: &str) -> Expansion { expand_any(&custom(doc, id), &House::read(doc), "l") }
fn count(ex: &Expansion, prefix: &str) -> usize { ex.members.iter().filter(|m| m.key.starts_with(prefix)).count() }
fn wall3(doc: &mut Document) -> String { fr(doc, "add_wall", json!({ "a": [0, 0], "b": [3, 0], "height": 2.44, "studSize": "2x4", "studSpacing": 0.4 })).unwrap().created[0].clone() }

#[test]
fn plain_wall_has_nine_studs_and_three_plates() {
    let mut d = doc(); let id = wall3(&mut d);
    let ex = expansion(&d, &id);
    assert_eq!(count(&ex, "stud."), 9, "{:?}", ex.members.iter().map(|m| &m.key).collect::<Vec<_>>());
    assert_eq!(count(&ex, "plate."), 3);
    let studs: Vec<f64> = ex.members.iter().filter(|m| m.key.starts_with("stud.")).map(|m| m.r[0][0]).collect();
    assert!((studs[0]).abs() < 1e-9 && (studs[1] - 0.4).abs() < 1e-9 && (studs[8] - (3.0 - T)).abs() < 1e-9, "{studs:?}");
    // a stud is full height between the plates
    let s = ex.members.iter().find(|m| m.key == "stud.1").unwrap();
    assert!((s.r[1][0] - T).abs() < 1e-9 && (s.r[1][1] - (2.44 - 2.0 * T)).abs() < 1e-9);
    assert!(ex.panels.iter().any(|p| p.key == "sheathing.ext"), "exterior sheathing on by default");
}

#[test]
fn door_gets_kings_jacks_header_and_cripples_above_only() {
    let mut d = doc(); let id = wall3(&mut d);
    fr(&mut d, "add_opening", json!({ "wall": id, "kind": "door", "offset": 1.0, "width": 0.9, "height": 2.05 })).unwrap();
    let ex = expansion(&d, &id);
    assert_eq!(count(&ex, "king.0.l"), 1); assert_eq!(count(&ex, "king.0.r"), 1);
    assert_eq!(count(&ex, "jack.0.l"), 1); assert_eq!(count(&ex, "jack.0.r"), 1);
    let headers: Vec<&Member> = ex.members.iter().filter(|m| m.key == "header.0").collect();
    assert_eq!(headers.len(), 2, "two plies");
    assert_eq!(headers[0].size_label, "2x6", "0.9 m span → 2x6");
    assert!((headers[0].r[0][0] - (1.0 - T)).abs() < 1e-9 && (headers[0].r[0][1] - (1.9 + T)).abs() < 1e-9, "header spans between the kings");
    assert!(count(&ex, "cripple.0.") >= 1, "cripples above the header");
    assert!(ex.members.iter().filter(|m| m.key.starts_with("cripple.0.")).all(|m| m.r[1][0] > 2.0), "all cripples sit above the header");
    assert_eq!(count(&ex, "sill."), 0, "doors have no sill");
    // regular studs that fell inside the opening zone are gone: positions 1.2 and 1.6 were inside
    assert!(!ex.members.iter().any(|m| m.key.starts_with("stud.") && m.r[0][0] > 0.95 && m.r[0][0] < 1.95));
}

#[test]
fn window_gets_a_sill_and_cripples_below() {
    let mut d = doc(); let id = wall3(&mut d);
    fr(&mut d, "add_opening", json!({ "wall": id, "kind": "window", "offset": 1.0, "width": 1.2, "height": 1.2, "sill": 0.9 })).unwrap();
    let ex = expansion(&d, &id);
    let sill = ex.members.iter().find(|m| m.key == "sill.0").expect("sill");
    assert!((sill.r[1][1] - 0.9).abs() < 1e-9 && (sill.r[0][0] - 1.0).abs() < 1e-9 && (sill.r[0][1] - 2.2).abs() < 1e-9);
    let below: Vec<&Member> = ex.members.iter().filter(|m| m.key.starts_with("cripple.0.") && m.r[1][1] < 0.9).collect();
    assert!(!below.is_empty(), "cripples under the sill");
    assert!(ex.members.iter().any(|m| m.key.starts_with("cripple.0.") && m.r[1][0] > 2.1), "and above the header");
    assert_eq!(ex.members.iter().filter(|m| m.key == "header.0").next().unwrap().size_label, "2x6");
}

#[test]
fn sheathing_has_a_hole_per_opening_and_the_right_area() {
    let mut d = doc(); let id = wall3(&mut d);
    fr(&mut d, "add_opening", json!({ "wall": id, "kind": "window", "offset": 0.5, "width": 1.0, "height": 1.0, "sill": 0.9 })).unwrap();
    fr(&mut d, "add_opening", json!({ "wall": id, "kind": "door", "offset": 1.9, "width": 0.8, "height": 2.05 })).unwrap();
    let ex = expansion(&d, &id);
    let faces: Vec<&plannr_core::model::Face> = ex.faces.iter().filter_map(|e| match e { Entity::Face(f) if f.id.ends_with("#sheathing.ext") => Some(f), _ => None }).collect();
    assert!(faces.iter().any(|f| f.holes.len() == 2), "a sheathing face with two holes");
    let p = ex.panels.iter().find(|p| p.key == "sheathing.ext").unwrap();
    let expected = 3.0 * 2.44 - 1.0 * 1.0 - 0.8 * 2.05;
    assert!((p.area - expected).abs() < 1e-6, "{} vs {expected}", p.area);
}

#[test]
fn flip_moves_the_sheathing_to_the_other_side() {
    let mut d = doc(); let id = wall3(&mut d);
    let side = |d: &Document| { let ex = expansion(d, &id); let f = ex.faces.iter().find_map(|e| match e { Entity::Face(f) if f.id.ends_with("#sheathing.ext") => Some(f), _ => None }).unwrap(); f.outer.iter().map(|p| p[2]).sum::<f64>() / f.outer.len() as f64 };
    let z0 = side(&d);
    fr(&mut d, "update", json!({ "id": id, "patch": { "flip": true } })).unwrap();
    let z1 = side(&d);
    assert!(z0 * z1 < 0.0 && (z0.abs() - z1.abs()).abs() < 1e-6, "sheathing z {z0} → {z1}");
}

#[test]
fn floor_auto_direction_spans_the_short_way() {
    let mut d = doc();
    let id = fr(&mut d, "add_floor", json!({ "origin": [0, 0], "size": [4, 3], "joistSize": "2x10", "joistSpacing": 0.4 })).unwrap().created[0].clone();
    let ex = expansion(&d, &id);
    assert_eq!(count(&ex, "joist."), 11, "{:?}", ex.members.iter().map(|m| (&m.key, m.r)).collect::<Vec<_>>());
    assert_eq!(count(&ex, "rim."), 4);
    let j = ex.members.iter().find(|m| m.key == "joist.1").unwrap();
    assert!((j.length - (3.0 - 2.0 * T)).abs() < 1e-9, "joists span the 3 m way between the rims: {}", j.length);
    assert!((j.size[1] - 0.235).abs() < 1e-9);
    assert!(ex.panels.iter().any(|p| p.key == "subfloor" && (p.area - 12.0).abs() < 1e-9));
    assert!(ex.faces.iter().any(|e| e.id().ends_with("#subfloor")));
}

#[test]
fn fink_truss_geometry() {
    let mut d = doc();
    let id = fr(&mut d, "add_roof", json!({ "min": [0, 0], "max": [6, 8], "type": "fink", "pitch": 6, "spacing": 0.6, "heel": 0.3, "overhang": 0.45 })).unwrap().created[0].clone();
    let ex = expansion(&d, &id);
    assert_eq!(count(&ex, "truss.0.web."), 4, "a W has four webs");
    assert_eq!(ex.members.iter().filter(|m| m.key.ends_with(".bottom")).count(), 11, "6 m at 0.6 → 11 trusses");
    // the top chord underside runs from the heel to the peak: rise = 6/12 × half span (4 m) = 2 m
    let top = ex.members.iter().find(|m| m.key == "truss.0.top.l").unwrap();
    let base_y = 2.6; // storey top with no construction data (base 0 + 2.6)
    let start = top.origin; // underside at s = −overhang
    let at_end = plannr_core::geom::add(top.origin, plannr_core::geom::scale(top.ex, top.length));
    assert!((start[2] + 0.45).abs() < 1e-6, "tail starts at −overhang: {start:?}");
    assert!((at_end[1] - (base_y + 0.3 + 2.0)).abs() < 1e-6, "peak underside at heel + rise: {at_end:?}");
    assert!((start[1] - (base_y + 0.3 - 0.5 * 0.45)).abs() < 1e-6, "heel honoured, tail drops with the slope");
}

#[test]
fn rafters_make_a_ridge_board_ceiling_joists_and_rafters() {
    let mut d = doc();
    let id = fr(&mut d, "add_roof", json!({ "min": [0, 0], "max": [6, 8], "type": "rafters", "spacing": 0.6 })).unwrap().created[0].clone();
    let ex = expansion(&d, &id);
    assert_eq!(count(&ex, "ridge"), 1);
    assert_eq!(count(&ex, "rafter."), 22); assert_eq!(count(&ex, "ceilingJoist."), 11);
    let r = ex.members.iter().find(|m| m.key == "ridge").unwrap(); assert!((r.length - 6.0).abs() < 1e-9 && r.size_label == "2x10");
}

#[test]
fn howe_king_queen_web_counts_and_turned_roof() {
    let mut d = doc();
    for (t, webs) in [("howe", 5), ("kingPost", 1), ("queenPost", 2)] {
        let id = fr(&mut d, "add_roof", json!({ "min": [0, 0], "max": [6, 8], "type": t, "ridgeAlongX": false })).unwrap().created[0].clone();
        let ex = expansion(&d, &id);
        assert_eq!(count(&ex, "truss.1.web."), webs, "{t}");
        // ridge along z: trusses repeat along z (8 m at 0.6 → 15), span 6 m
        assert_eq!(ex.members.iter().filter(|m| m.key.ends_with(".bottom")).count(), 15, "{t}");
    }
}

#[test]
fn foundations_produce_concrete_with_the_right_volume() {
    let mut d = doc();
    let slab = fr(&mut d, "add_foundation", json!({ "origin": [0, 0], "size": [4, 3], "type": "slab", "slabThickness": 0.15 })).unwrap().created[0].clone();
    let stem = fr(&mut d, "add_foundation", json!({ "origin": [10, 0], "size": [4, 3], "type": "stemwall", "stemHeight": 0.6, "stemThickness": 0.2, "footingWidth": 0.5, "footingDepth": 0.2, "slab": false })).unwrap().created[0].clone();
    let es = expansion(&d, &slab); assert!((es.concrete.iter().map(|k| k.volume).sum::<f64>() - 12.0 * 0.15).abs() < 1e-9);
    assert!(es.faces.iter().any(|e| e.id().ends_with("#slab")));
    let ew = expansion(&d, &stem);
    let per = 14.0; let expected = per * 0.2 * 0.6 + per * 0.5 * 0.2;
    assert!((ew.concrete.iter().map(|k| k.volume).sum::<f64>() - expected).abs() < 1e-9);
    assert_eq!(count_faces(&ew, "#stem."), 4 * 6); assert_eq!(count_faces(&ew, "#footing."), 4 * 6);
    let est = data_of(&fr(&mut d, "estimate", json!({})).unwrap());
    assert!((est["totals"]["concrete_m3"].as_f64().unwrap() - (1.8 + expected)).abs() < 2e-3, "{}", est["totals"]);
}
fn count_faces(ex: &Expansion, key: &str) -> usize { ex.faces.iter().filter(|e| e.id().contains(key)).count() }

#[test]
fn frame_room_frames_exterior_edges_with_the_window_in_the_right_wall() {
    let mut d = doc();
    let reg = registry();
    reg.apply("construction.setup", json!({ "storeys": [{ "name": "GF", "height": 2.7 }] }), &mut d).unwrap();
    let room = reg.apply("construction.add_room", json!({ "name": "Room", "level": 0, "origin": [0, 0], "size": [5, 4] }), &mut d).unwrap().created[0].clone();
    reg.apply("construction.add_opening", json!({ "kind": "window", "host": { "room": room, "edge": 0 }, "along": 2.0, "width": 1.2, "height": 1.4, "sill": 0.9 }), &mut d).unwrap();
    let r = fr(&mut d, "frame_room", json!({ "room": room })).unwrap();
    assert_eq!(r.created.len(), 4, "four exterior walls");
    let walls: Vec<Wall> = r.created.iter().map(|id| serde_json::from_value(custom(&d, id).data.clone()).unwrap()).collect();
    let w0 = walls.iter().find(|w| w.edge == Some(0)).unwrap();
    assert_eq!(w0.openings.len(), 1); assert!((w0.openings[0].offset - 1.4).abs() < 1e-9); assert_eq!(w0.openings[0].kind, "window"); assert!((w0.openings[0].sill - 0.9).abs() < 1e-9);
    assert!((w0.height.unwrap() - 2.7).abs() < 1e-9);
    assert!(walls.iter().all(|w| w.openings.len() <= 1) && walls.iter().map(|w| w.openings.len()).sum::<usize>() == 1);
    // exterior side points away from the room centre: sheathing lies outside the 5×4 rectangle
    for w in &walls { let ex = expansion(&d, &r.created[walls.iter().position(|x| x.edge == w.edge).unwrap()]); let f = ex.faces.iter().find_map(|e| match e { Entity::Face(f) if f.id.ends_with("#sheathing.ext") => Some(f), _ => None }).unwrap(); let c = f.outer.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0] / 4.0, a[1] + p[2] / 4.0]); assert!(c[0] < -0.01 || c[0] > 5.01 || c[1] < -0.01 || c[1] > 4.01, "edge {:?} sheathing centre {c:?}", w.edge); }
    // a second room sharing the east edge: framing it exterior-only skips that edge
    let room2 = reg.apply("construction.add_room", json!({ "name": "R2", "level": 0, "origin": [5, 0], "size": [3, 4] }), &mut d).unwrap().created[0].clone();
    let r2 = fr(&mut d, "frame_room", json!({ "room": room2 })).unwrap();
    assert_eq!(r2.created.len(), 3);
}

#[test]
fn estimate_totals_add_up_and_stock_rounds_up() {
    let mut d = doc(); let id = wall3(&mut d);
    let est = data_of(&fr(&mut d, "estimate", json!({})).unwrap());
    let ex = expansion(&d, &id);
    let total: f64 = est["items"].as_array().unwrap().iter().filter_map(|i| i["length_m"].as_f64()).sum();
    assert!((total - ex.lumber_length()).abs() < 0.05, "{total} vs {}", ex.lumber_length());
    assert_eq!(est["totals"]["lumber_pieces"].as_u64().unwrap() as usize, ex.members.len());
    assert!(est["items"].as_array().unwrap().iter().any(|i| i["description"].as_str().unwrap().contains("2x4 × 2.44 m")), "{}", est["items"]);
    assert!(est["items"].as_array().unwrap().iter().any(|i| i["description"].as_str().unwrap().contains("2x4 × 3.05 m")), "3.0 m plates round up to 3.05");
    assert_eq!(est["totals"]["sheathing_panels"].as_u64().unwrap(), ((3.0 * 2.44) / PANEL_AREA).ceil() as u64);
    assert_eq!(est["byElement"].as_array().unwrap().len(), 1);
    assert_eq!(stock_length(2.44), 2.44); assert_eq!(stock_length(2.45), 3.05); assert!((stock_length(6.0) - 7.32).abs() < 1e-9);
}

#[test]
fn transform_moves_a_wall_and_turns_a_roof() {
    let mut d = doc(); let id = wall3(&mut d);
    core_apply(&mut d, &Op::Move { ids: vec![id.clone()], delta: [1.0, 0.0, 2.0] }, &registry()).unwrap();
    let w: Wall = serde_json::from_value(custom(&d, &id).data.clone()).unwrap();
    assert_eq!(w.a, [1.0, 2.0]); assert_eq!(w.b, [4.0, 2.0]);
    let rid = fr(&mut d, "add_roof", json!({ "min": [0, 0], "max": [6, 8] })).unwrap().created[0].clone();
    core_apply(&mut d, &Op::Rotate { ids: vec![rid.clone()], origin: [0.0, 0.0, 0.0], axis: [0.0, 1.0, 0.0], angle: 90.0 }, &registry()).unwrap();
    let r: Roof = serde_json::from_value(custom(&d, &rid).data.clone()).unwrap();
    assert!(!r.ridge_along_x, "a 90° turn swaps the ridge direction");
    assert!(core_apply(&mut d, &Op::Rotate { ids: vec![rid.clone()], origin: [0.0, 0.0, 0.0], axis: [0.0, 1.0, 0.0], angle: 30.0 }, &registry()).is_err(), "30° is refused");
}

#[test]
fn compile_and_metrics_include_framing() {
    let mut d = doc(); let id = wall3(&mut d);
    fr(&mut d, "add_opening", json!({ "wall": id, "kind": "window", "offset": 1.0, "width": 1.0, "height": 1.0, "sill": 0.9 })).unwrap();
    let reg = registry();
    let c = compile(&d, &reg, &CompileOptions::default());
    assert!(c.metrics.faces > 60, "{}", c.metrics.faces);
    assert!(c.groups.iter().any(|g| g.ids.iter().any(|i| i.contains("#stud."))));
    let m = reg.metrics(&d); let f = &m["framing"];
    assert_eq!(f["walls"], 1); assert!(f["studs"].as_u64().unwrap() >= 7); assert!(f["lumber_m"].as_f64().unwrap() > 20.0);
    assert!(d.layers.iter().any(|l| l.name == "Framing"));
    assert!(d.materials.iter().any(|m| m.id == "framing.lumber"));
}

#[test]
fn lumber_table_and_header_rule() {
    assert_eq!(lumber("2x4"), Some([0.038, 0.089])); assert_eq!(lumber("2X10"), Some([0.038, 0.235])); assert_eq!(lumber("38x140"), Some([0.038, 0.140])); assert_eq!(lumber("4x4"), Some([0.089, 0.089])); assert_eq!(lumber("bogus"), None);
    assert_eq!(header_size_for_span(0.9), "2x6"); assert_eq!(header_size_for_span(1.5), "2x8"); assert_eq!(header_size_for_span(2.4), "2x10"); assert_eq!(header_size_for_span(3.0), "2x12");
    let mut d = doc(); let id = wall3(&mut d);
    assert!(fr(&mut d, "add_opening", json!({ "wall": id, "kind": "door", "offset": 2.5, "width": 0.9, "height": 2.05 })).is_err(), "opening beyond the wall end is refused");
    assert!(fr(&mut d, "add_wall", json!({ "a": [0, 0], "b": [3, 0], "studSize": "2x99" })).is_err());
    let oid = fr(&mut d, "add_opening", json!({ "wall": id, "kind": "door", "offset": 0.5, "width": 2.0, "height": 2.05, "headerSize": "2x12" })).unwrap().created[0].clone();
    assert_eq!(expansion(&d, &id).members.iter().find(|m| m.key == "header.0").unwrap().size_label, "2x12");
    fr(&mut d, "remove_opening", json!({ "wall": id, "opening": oid })).unwrap();
    assert_eq!(count(&expansion(&d, &id), "header."), 0);
}

#[test]
fn set_defaults_feed_new_walls_and_summary_lists_elements() {
    let mut d = doc();
    fr(&mut d, "set_defaults", json!({ "studSize": "2x6", "studSpacing": 0.6, "sheathing": { "interior": { "enabled": true } } })).unwrap();
    let id = fr(&mut d, "add_wall", json!({ "a": [0, 0], "b": [3, 0] })).unwrap().created[0].clone();
    let w: Wall = serde_json::from_value(custom(&d, &id).data.clone()).unwrap();
    assert_eq!(w.stud_size, "2x6"); assert!((w.stud_spacing - 0.6).abs() < 1e-9); assert!(w.sheathing.interior.enabled);
    let ex = expansion(&d, &id); assert_eq!(count(&ex, "stud."), 6); assert!(ex.panels.iter().any(|p| p.key == "sheathing.int"));
    let s = data_of(&fr(&mut d, "summary", json!({})).unwrap());
    assert_eq!(s["walls"].as_array().unwrap().len(), 1); assert_eq!(s["walls"][0]["studs"], 6); assert!(s["totals"]["lumber_m"].as_f64().unwrap() > 0.0);
}
