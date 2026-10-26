//! Import from the construction model, the construction plugin leaving detailed parts alone,
//! U-values, checks, takeoff and the plan drawing.

use plannr_core::model::{Document, Entity};
use plannr_core::ops::OpResult;
use plannr_core::plugin::Registry;
use plannr_core::{compile, CompileOptions};
use plannr_plugin_construction::ConstructionPlugin;
use plannr_plugin_construction::masonry::{data::*, import, uvalue};
use serde_json::{json, Value};

fn registry() -> Registry { let mut r = Registry::new(); r.register(Box::new(ConstructionPlugin)); r }
fn ap(doc: &mut Document, t: &str, data: Value) -> OpResult { registry().apply(t, data, doc).unwrap_or_else(|e| panic!("{t}: {e:?}")) }
fn data_of(r: &OpResult) -> Value { match &r.entities[0] { Entity::Custom(c) => c.data.clone(), _ => Value::Null } }

/// Two ground-floor rooms side by side (6 × 4 m together) and one room upstairs over both, cavity walls.
fn house() -> Document {
    let mut d = Document::default(); registry().on_load(&mut d);
    ap(&mut d, "construction.setup", json!({ "storeys": [{ "name": "Gelijkvloers", "height": 2.6 }, { "name": "Verdieping", "height": 2.5 }], "cavity": { "enabled": true, "innerLeaf": 0.09, "cavity": 0.06, "outerLeaf": 0.09 } }));
    ap(&mut d, "construction.add_room", json!({ "name": "Living", "level": 0, "origin": [0, 0], "size": [4, 4] }));
    ap(&mut d, "construction.add_room", json!({ "name": "Kitchen", "level": 0, "origin": [4, 0], "size": [2, 4] }));
    ap(&mut d, "construction.add_room", json!({ "name": "Bedroom", "level": 1, "origin": [0, 0], "size": [6, 4] }));
    d
}

#[test]
fn uvalue_of_an_empty_cavity_wall_and_after_a_fill() {
    let d = Defaults::default();
    let w: Wall = serde_json::from_value(json!({ "a": [0, 0], "b": [4, 0], "type": "cavity" })).unwrap();
    let (_, r, u) = uvalue::layers(&w, &d);
    // Rsi 0.13 + plaster 0.026 + clay block 0.333 + cavity 0.17 + facing brick 0.1 + Rse 0.04
    assert!((r - 0.799).abs() < 0.01, "R {r}");
    assert!(u > 1.2 && u < 1.3, "U {u}");
    let rep = uvalue::report(&w, &d);
    assert_eq!(rep["meets"], json!(false));
    let eps = rep["withFill"].as_array().unwrap().iter().find(|x| x["material"] == "eps").unwrap()["U"].as_f64().unwrap();
    assert!(eps > 0.4 && eps < 0.46, "a 6 cm EPS fill: U {eps}");
    // boards with a ventilated gap: the outer leaf no longer counts
    let wb: Wall = serde_json::from_value(json!({ "a": [0, 0], "b": [4, 0], "cavity": { "width": 0.15, "insulation": { "kind": "board", "material": "pir", "thickness": 0.12 } } })).unwrap();
    let (_, _, ub) = uvalue::layers(&wb, &d);
    assert!(ub < 0.2, "12 cm PIR: U {ub}");
}

#[test]
fn import_details_the_construction_model_and_construction_steps_aside() {
    let mut d = house();
    let before = compile(&d, &registry(), &CompileOptions::default());
    let living = d.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.type_ == "construction.room" && c.data["name"] == "Living" => Some(c.id.clone()), _ => None }).unwrap();
    assert!(before.groups.iter().any(|g| g.ids.iter().any(|i| i.starts_with(&format!("{living}#wall.")))), "construction draws the living walls");
    let r = ap(&mut d, "masonry.import_construction", json!({}));
    let h = House::read(&d);
    let cavity = h.walls.iter().filter(|(_, w)| w.type_ == "cavity").count();
    let single = h.walls.iter().filter(|(_, w)| w.type_ == "single").count();
    assert!(cavity >= 4 && single >= 1, "{cavity} cavity walls, {single} partitions");
    assert!(h.walls.iter().all(|(_, w)| w.source.is_some() && w.reference == "center"));
    // the exterior side of an imported cavity wall points away from its room
    for (_, w) in h.walls.iter().filter(|(_, w)| w.type_ == "cavity" && w.level == 0) {
        let l = w.length(); let u = [(w.b[0] - w.a[0]) / l, (w.b[1] - w.a[1]) / l];
        let n = if w.flip { [-u[1], u[0]] } else { [u[1], -u[0]] };
        let mid = [(w.a[0] + w.b[0]) / 2.0 + n[0] * 0.5, (w.a[1] + w.b[1]) / 2.0 + n[1] * 0.5];
        assert!(!(mid[0] > 0.0 && mid[0] < 6.0 && mid[1] > 0.0 && mid[1] < 4.0), "exterior normal of {:?}→{:?} points inside", w.a, w.b);
    }
    // one beam-and-block floor for the upper storey, inset 3 cm to the inner leaves' cavity faces
    assert_eq!(h.floors.len(), 1);
    let f = &h.floors[0].1;
    assert_eq!(f.level, 1); assert_eq!(f.rooms.len(), 1);
    let (minx, maxx) = f.points.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
    assert!((minx - 0.03).abs() < 1e-6 && (maxx - 5.97).abs() < 1e-6, "{:?}", f.points);
    assert!(r.created.len() == h.walls.len() + 1);
    // the construction plugin no longer draws the detailed walls nor the upstairs floor slab
    let after = compile(&d, &registry(), &CompileOptions::default());
    assert!(!after.groups.iter().any(|g| g.ids.iter().any(|i| i.starts_with(&format!("{living}#wall.")))), "detailed walls are left to masonry");
    let bedroom = d.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.type_ == "construction.room" && c.data["name"] == "Bedroom" => Some(c.id.clone()), _ => None }).unwrap();
    assert!(!after.groups.iter().any(|g| g.ids.iter().any(|i| i == &format!("{bedroom}#floor"))));
    // importing again replaces
    let n = d.entities.len();
    let r2 = ap(&mut d, "masonry.import_construction", json!({}));
    assert_eq!(r2.removed.len(), r.created.len()); assert_eq!(d.entities.len(), n);
}

#[test]
fn checks_flag_the_u_value_and_floor_span() {
    let mut d = house();
    ap(&mut d, "masonry.import_construction", json!({}));
    let c = data_of(&ap(&mut d, "masonry.check", json!({})));
    let f = c["findings"].as_array().unwrap();
    let titles: Vec<&str> = f.iter().map(|x| x["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"Wall U-value above the EPB requirement"), "{titles:?}");
    assert!(f.iter().any(|x| x["title"] == "Floor span" && x["severity"] == "ok"), "a 3.9 m span on 16+4 is fine: {titles:?}");
    // a 7 m span on 12 cm blocks is not
    ap(&mut d, "masonry.add_floor", json!({ "level": 1, "origin": [20, 0], "size": [10, 7], "blockHeight": 0.12 }));
    let c = data_of(&ap(&mut d, "masonry.check", json!({})));
    assert!(c["findings"].as_array().unwrap().iter().any(|x| x["title"] == "Floor span too long" && x["severity"] == "error"));
    // cavity fill brings the wall to ~0.43 but still above 0.24: the warning stays, with the new value
    ap(&mut d, "masonry.set_defaults", json!({}));
    let u = data_of(&ap(&mut d, "masonry.uvalue", json!({})));
    assert!(u["buildups"].as_array().unwrap().len() >= 2, "cavity and partition build-ups: {u}");
}

#[test]
fn union_outline_merges_rectangles() {
    let a = vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
    let b = vec![[4.0, 0.0], [6.0, 0.0], [6.0, 2.0], [4.0, 2.0]];
    let u = import::union_outline(&[a, b]);
    assert_eq!(u.len(), 1);
    assert_eq!(u[0].len(), 6, "an L: {:?}", u[0]);
    assert!((plan_area(&u[0]).abs() - 20.0).abs() < 1e-9);
    let o = import::offset_polygon(&u[0], -0.03);
    assert!((plan_area(&o).abs() - (20.0 - 0.03 * 2.0 * (6.0 + 4.0) + 0.0036 * 4.0 - 0.0)).abs() < 0.05, "{}", plan_area(&o));
}

#[test]
fn plan_svg_is_well_formed() {
    let mut d = house();
    ap(&mut d, "masonry.import_construction", json!({}));
    let s = data_of(&ap(&mut d, "masonry.plan_svg", json!({ "level": 0 })))["svg"].as_str().unwrap().to_string();
    assert!(s.starts_with("<svg") && s.trim_end().ends_with("</svg>"));
    assert!(s.matches("class=\"outer\"").count() >= 4 && s.contains("class=\"joist\""));
}

#[test]
fn clear_heights_follow_the_structure_above() {
    let mut d = house();
    ap(&mut d, "masonry.import_construction", json!({}));
    // 2.6 floor-to-floor with a 0.20 floor above leaves 2.40: ask for 2.50 clear instead
    let s = data_of(&ap(&mut d, "masonry.storeys", json!({})));
    assert!((s[0]["clear"].as_f64().unwrap() - 2.4).abs() < 1e-9, "{s}");
    let s = data_of(&ap(&mut d, "masonry.set_clear_height", json!({ "level": 0, "clear": 2.5 })));
    assert!((s[0]["height"].as_f64().unwrap() - 2.7).abs() < 1e-9 && (s[0]["clear"].as_f64().unwrap() - 2.5).abs() < 1e-9, "{s}");
    // the construction plugin hangs ceilings and ceiling lights under the floor
    let m = plannr_plugin_construction::expand::model(&d);
    assert!((m.settings.clear_height(0, [2.0, 2.0]) - 2.5).abs() < 1e-9);
    // attic joists in the top of the upper storey count as structure too
    ap(&mut d, "construction.update_storey", json!({ "index": 1, "height": 2.68 }));
    let mut reg = registry(); reg.register(Box::new(plannr_plugin_framing::FramingPlugin));
    reg.apply("framing.add_floor", json!({ "level": 1, "origin": [0.03, 0.03], "size": [5.94, 3.94], "joistSize": "32x178", "elevation": 2.5, "rimJoist": false, "subfloor": { "enabled": false } }), &mut d).unwrap();
    let s = data_of(&ap(&mut d, "masonry.storeys", json!({})));
    assert!((s[1]["clear"].as_f64().unwrap() - 2.5).abs() < 1e-6, "{s}");
    let h = House::read(&d);
    assert!((h.floor_depth_above(1, [3.0, 2.0]).unwrap() - 0.18).abs() < 1e-6);
}

#[test]
fn a_new_model_is_masonry_without_any_import() {
    let mut d = house();
    // nothing imported: the walls are masonry, the upper floor a derived masonry floor
    let s = compile(&d, &registry(), &CompileOptions::default());
    let ids: Vec<&String> = s.groups.iter().flat_map(|g| g.ids.iter()).collect();
    assert!(ids.iter().any(|i| i.ends_with("#outer")) && ids.iter().any(|i| i.ends_with("#inner")) && ids.iter().any(|i| i.ends_with("#single")), "cavity walls and partitions");
    assert!(ids.iter().any(|i| i.contains("#mfloor0#")), "the upper storey's floor is masonry");
    assert!(!d.entities.iter().any(|e| matches!(e, Entity::Custom(c) if c.type_.starts_with("masonry."))), "no masonry entities needed");
    // the takeoff counts the derived walls
    let t = data_of(&ap(&mut d, "masonry.estimate", json!({})))["totals"].clone();
    assert!(t["bricks"].as_u64().unwrap() > 1000 && t["blocks"].as_u64().unwrap() > 100, "{t}");
    // one source of truth: a thicker inner leaf in the build-up moves the construction geometry
    let mut inner = Defaults::default().inner; inner.width = 0.14;
    ap(&mut d, "masonry.set_defaults", json!({ "inner": inner }));
    assert!((plannr_plugin_construction::ops::settings(&d).cavity.inner_leaf - 0.14).abs() < 1e-9);
    // and the construction setup writes the build-up
    ap(&mut d, "construction.setup", json!({ "wallThickness": 0.1 }));
    assert!((settings(&d).defaults.partition.width - 0.1).abs() < 1e-9);
    // paint on a masonry face lands on the room's finish
    let living = d.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.data["name"] == "Living" => Some(c.id.clone()), _ => None }).unwrap();
    plannr_core::ops::apply(&mut d, &plannr_core::ops::Op::Paint { ids: vec![format!("{living}#wall.0#plaster.int")], material: "construction.paintBlue".into(), side: None }, &registry()).unwrap();
    let h = House::read(&d);
    let w = h.walls.iter().find(|(id, _)| id == &format!("{living}#wall.0")).unwrap();
    assert_eq!(w.1.finish_interior.as_deref(), Some("construction.paintBlue"));
    // customising one wall writes just that one out
    let r = ap(&mut d, "masonry.import_construction", json!({ "only": [{ "owner": living, "key": "wall.0" }] }));
    assert_eq!(r.created.len(), 1);
    let h = House::read(&d);
    assert!(!h.walls.iter().any(|(id, _)| id == &format!("{living}#wall.0")), "the derived wall gives way to the customised one");
}
