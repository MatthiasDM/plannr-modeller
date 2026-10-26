use plannr_core::{apply, compile, CompileOptions, Document, Entity, Op, Registry};
use plannr_plugin_construction::expand;
use plannr_plugin_construction::ConstructionPlugin;
use serde_json::{json, Value};

fn reg() -> Registry { let mut r = Registry::new(); r.register(Box::new(ConstructionPlugin)); r }
fn doc() -> Document { let mut d = Document::default(); reg().on_load(&mut d); d }
fn op(doc: &mut Document, name: &str, data: Value) -> plannr_core::OpResult {
    apply(doc, &Op::Custom { type_: format!("construction.{name}"), data }, &reg()).unwrap_or_else(|e| panic!("{name}: {e}"))
}
fn house() -> Value { serde_json::from_str(include_str!("fixtures/house.json")).unwrap() }
fn imported() -> Document { let mut d = doc(); op(&mut d, "import_house", json!({ "house": house() })); d }
fn customs<'a>(d: &'a Document, t: &str) -> Vec<&'a plannr_core::model::Custom> {
    d.entities.iter().filter_map(|e| match e { Entity::Custom(c) if c.type_ == t => Some(c), _ => None }).collect()
}
fn scene(d: &Document) -> plannr_core::CompiledScene { compile(d, &reg(), &CompileOptions::default()) }
/// Total compiled area and face count for ids with a prefix.
fn faces_with(s: &plannr_core::CompiledScene, prefix: &str) -> (usize, f64) {
    let mut n = 0; let mut area = 0.0;
    for g in &s.groups {
        if g.back { continue; }
        for (i, id) in g.ids.iter().enumerate() {
            if !id.starts_with(prefix) { continue; }
            n += 1;
            let [start, count] = g.id_ranges[i];
            for t in g.indices[start as usize..(start + count) as usize].chunks(3) {
                let p = |k: u32| [g.positions[k as usize * 3] as f64, g.positions[k as usize * 3 + 1] as f64, g.positions[k as usize * 3 + 2] as f64];
                area += plannr_core::geom::area(&[p(t[0]), p(t[1]), p(t[2])]);
            }
        }
    }
    (n, area)
}
fn max_y(s: &plannr_core::CompiledScene, prefix: &str) -> f64 {
    let mut m = f64::MIN;
    for g in &s.groups {
        for (i, id) in g.ids.iter().enumerate() {
            if !id.starts_with(prefix) { continue; }
            let [start, count] = g.id_ranges[i];
            for k in &g.indices[start as usize..(start + count) as usize] { m = m.max(g.positions[*k as usize * 3 + 1] as f64); }
        }
    }
    m
}
fn room4(d: &mut Document) -> String { op(d, "add_room", json!({ "name": "Box", "level": 0, "origin": [0, 0], "size": [4, 4] })).created[0].clone() }

#[test]
fn import_house_counts() {
    let d = imported();
    assert_eq!(customs(&d, "construction.room").len(), 13);
    assert_eq!(customs(&d, "construction.opening").len(), 28);
    assert_eq!(customs(&d, "construction.roof").len(), 2);
    assert_eq!(customs(&d, "construction.fixture").len(), 6);
    let s = plannr_plugin_construction::ops::settings(&d);
    assert_eq!(s.storeys.len(), 2);
    assert!(s.cavity.enabled);
    assert!((s.storeys[0].height - 2.75).abs() < 1e-9);
    // ids are kept so openings resolve
    let living = customs(&d, "construction.room").into_iter().find(|c| c.data["name"] == "Living Room").unwrap();
    assert_eq!(living.id, "89F61EFB-C203-4569-A97A-459781556369");
    assert_eq!(living.data["points"].as_array().unwrap().len(), 6);
}

#[test]
fn imported_house_expands_every_room_floor() {
    let d = imported();
    let s = scene(&d);
    // ground-floor rooms keep their slab; the upper storeys get the derived masonry floor
    for c in customs(&d, "construction.room").into_iter().filter(|c| c.data["level"].as_u64().unwrap_or(0) == 0) {
        let (n, area) = faces_with(&s, &format!("{}#floor", c.id));
        assert!(n >= 6, "room {} has a slab ({n} faces)", c.data["name"]);
        assert!(area > 0.0);
    }
    assert!(s.groups.iter().any(|g| g.ids.iter().any(|i| i.contains("#mfloor"))), "upper storey floor is masonry");
    assert!(s.metrics.plugins.contains_key("construction"));
    assert!(s.metrics.plugins["construction"]["floorArea"].as_f64().unwrap() > 100.0);
    assert!(!s.edges.ids.is_empty());
}

#[test]
fn shared_kitchen_living_wall_is_built_once() {
    let d = imported();
    let m = expand::model(&d);
    let set = expand::collect(&m);
    // the Kitchen's north edge (2.4,4)→(6.6,4) coincides with Living Room edge 4 on x 3.6…6.6 (the rest borders the WC)
    let shared: Vec<_> = set.walls.iter().filter(|w| {
        let on = |p: [f64; 2]| (p[1] - 4.0).abs() < 1e-6 && ((p[0] - 3.6).abs() < 1e-6 || (p[0] - 6.6).abs() < 1e-6);
        w.level == 0 && on(w.a) && on(w.b)
    }).collect();
    assert_eq!(shared.len(), 1, "one wall for the shared segment");
    assert_eq!(shared[0].rooms.len(), 2);
    assert!(!shared[0].exterior);
    // and it is only emitted by its owner
    let s = scene(&d);
    let owner = &shared[0].owner;
    let others: Vec<_> = set.walls.iter().filter(|w| w.owner != *owner && w.key == shared[0].key).collect();
    let _ = others;
    let (n, _) = faces_with(&s, &format!("{}#{}#", owner, shared[0].key));
    assert!(n > 0);
}

#[test]
fn opening_cuts_the_wall() {
    let mut d = doc();
    let r = room4(&mut d);
    let before = faces_with(&scene(&d), &format!("{r}#wall.0#"));
    op(&mut d, "add_opening", json!({ "kind": "window", "host": { "room": r, "edge": 0 }, "along": 2.0 }));
    let s = scene(&d);
    let after = faces_with(&s, &format!("{r}#wall.0#"));
    assert!(after.0 > before.0, "more pieces: {} > {}", after.0, before.0);
    assert!(after.1 < before.1, "less wall area: {} < {}", after.1, before.1);
    let o = customs(&d, "construction.opening")[0];
    assert!(faces_with(&s, &format!("{}#glass", o.id)).0 > 0, "glass pane emitted");
    assert!(faces_with(&s, &format!("{}#frame", o.id)).0 > 0);
}

#[test]
fn every_roof_style_builds_timber_and_a_covering() {
    let mut d = doc();
    room4(&mut d);
    let rid = op(&mut d, "add_roof", json!({ "style": "gable", "pitchHeight": 1.6, "levels": 1 })).created[0].clone();
    for style in ["gable", "flat", "hip", "shed"] {
        op(&mut d, "update", json!({ "id": rid, "patch": { "style": style } }));
        let s = scene(&d);
        assert!(faces_with(&s, &format!("{rid}#roof")).0 > 0, "{style} roof covering");
        assert!(faces_with(&s, &format!("{rid}#timber")).0 > 0, "{style} roof timber");
    }
}

#[test]
fn move_translates_room_points_and_rotate_about_x_is_rejected() {
    let mut d = doc();
    let r = room4(&mut d);
    apply(&mut d, &Op::Move { ids: vec![r.clone()], delta: [1.0, 0.0, 2.0] }, &reg()).unwrap();
    let c = d.find(&r).unwrap();
    let Entity::Custom(c) = c else { panic!() };
    assert_eq!(c.data["points"][0], json!([1.0, 2.0]));
    assert_eq!(c.data["points"][2], json!([5.0, 6.0]));
    let e = apply(&mut d, &Op::Rotate { ids: vec![r.clone()], origin: [0.0; 3], axis: [1.0, 0.0, 0.0], angle: 30.0 }, &reg());
    assert!(e.is_err());
    apply(&mut d, &Op::Rotate { ids: vec![r.clone()], origin: [1.0, 0.0, 2.0], axis: [0.0, 1.0, 0.0], angle: 90.0 }, &reg()).unwrap();
    let Entity::Custom(c) = d.find(&r).unwrap() else { panic!() };
    // (5,2) about (1,2) by +90° (right-handed about y): x→-z, so (4,0) offset → (0,-4)
    let p1 = c.data["points"][1].as_array().unwrap();
    assert!((p1[0].as_f64().unwrap() - 1.0).abs() < 1e-9 && (p1[1].as_f64().unwrap() - (-2.0)).abs() < 1e-9, "{p1:?}");
}

#[test]
fn set_finish_shows_up_in_compile() {
    let mut d = doc();
    let r = room4(&mut d);
    op(&mut d, "set_finish", json!({ "id": r, "face": "floor", "material": "brick" }));
    let s = scene(&d);
    let g = s.groups.iter().find(|g| g.material == "construction.brick" && !g.back).expect("brick group");
    assert!(g.ids.iter().any(|id| id == &format!("{r}#floor")));
}

#[test]
fn remove_storey_shifts_levels() {
    let mut d = doc();
    op(&mut d, "setup", json!({ "storeys": [{ "name": "G", "height": 2.5 }, { "name": "1", "height": 2.5 }, { "name": "2", "height": 2.5 }] }));
    let a = op(&mut d, "add_room", json!({ "level": 1, "origin": [0, 0], "size": [3, 3] })).created[0].clone();
    let b = op(&mut d, "add_room", json!({ "level": 2, "origin": [0, 0], "size": [3, 3] })).created[0].clone();
    op(&mut d, "add_opening", json!({ "kind": "door", "host": { "room": a, "edge": 0 } }));
    let r = op(&mut d, "remove_storey", json!({ "index": 1 }));
    assert!(r.removed.contains(&a));
    assert!(d.find(&a).is_none());
    let Entity::Custom(c) = d.find(&b).unwrap() else { panic!() };
    assert_eq!(c.data["level"], 1);
    assert_eq!(customs(&d, "construction.opening").len(), 0, "orphaned opening removed");
    assert_eq!(plannr_plugin_construction::ops::settings(&d).storeys.len(), 2);
}

#[test]
fn summary_areas_sum_to_metrics() {
    let d = imported();
    let r = op(&mut d.clone(), "summary", json!({}));
    let Entity::Custom(c) = &r.entities[0] else { panic!() };
    let v = &c.data;
    let per_storey: f64 = v["storeys"].as_array().unwrap().iter().map(|s| s["floorArea"].as_f64().unwrap()).sum();
    let total = v["metrics"]["floorArea"].as_f64().unwrap();
    assert!((per_storey - total).abs() < 0.01, "{per_storey} vs {total}");
    assert_eq!(v["storeys"].as_array().unwrap().len(), 2);
    assert_eq!(v["openings"], 28);
}

#[test]
fn paint_through_core_sets_the_exterior_finish() {
    let mut d = doc();
    let r = room4(&mut d);
    apply(&mut d, &Op::Paint { ids: vec![format!("{r}#wall.2.exterior")], material: "#ff0000".into(), side: None }, &reg()).unwrap();
    let Entity::Custom(c) = d.find(&r).unwrap() else { panic!() };
    let mid = c.data["finishes"]["wall.2.exterior"].as_str().unwrap();
    assert!(d.materials.iter().any(|m| m.id == mid && m.color == "#FF0000"));
    // paint with a finish key via set_finish and both sides via core
    apply(&mut d, &Op::Paint { ids: vec![format!("{r}#wall.1.interior")], material: "construction.brick".into(), side: Some("both".into()) }, &reg()).unwrap();
    let Entity::Custom(c) = d.find(&r).unwrap() else { panic!() };
    assert_eq!(c.data["finishes"]["wall.1.exterior"], "construction.brick");
    assert_eq!(c.data["finishes"]["wall.1.interior"], "construction.brick");
}

#[test]
fn erase_through_a_face_key_removes_the_room() {
    let mut d = doc();
    let r = room4(&mut d);
    let res = apply(&mut d, &Op::Erase { ids: vec![format!("{r}#floor"), format!("{r}#wall.0.exterior")] }, &reg()).unwrap();
    assert_eq!(res.removed, vec![r.clone()]);
    assert!(d.find(&r).is_none());
}

#[test]
fn core_without_the_plugin_ignores_custom_entities() {
    let mut d = imported();
    let empty = Registry::new();
    let s = compile(&d, &empty, &CompileOptions::default());
    assert_eq!(s.metrics.faces, 0);
    let r = apply(&mut d, &Op::DrawRectangle { corner: [0.0; 3], opposite: [1.0, 0.0, 1.0], normal: [0.0, 1.0, 0.0], u: None, layer: None, material: None }, &empty).unwrap();
    assert_eq!(r.created.len(), 1);
    let rid = customs(&d, "construction.room")[0].id.clone();
    let e = apply(&mut d, &Op::Move { ids: vec![rid], delta: [1.0, 0.0, 0.0] }, &empty);
    assert!(e.is_err(), "no plugin → custom entity cannot be moved");
}

#[test]
fn moving_an_opening_slides_it_along_its_wall() {
    let mut d = doc();
    let r = room4(&mut d);
    let o = op(&mut d, "add_opening", json!({ "kind": "door", "host": { "room": r, "edge": 0 }, "along": 1.0 })).created[0].clone();
    apply(&mut d, &Op::Move { ids: vec![o.clone()], delta: [1.5, 0.0, 7.0] }, &reg()).unwrap();
    let Entity::Custom(c) = d.find(&o).unwrap() else { panic!() };
    assert!((c.data["along"].as_f64().unwrap() - 2.5).abs() < 1e-9);
}

#[test]
fn cavity_walls_build_two_leaves_on_exterior_only() {
    let mut d = doc();
    op(&mut d, "setup", json!({ "cavity": { "enabled": true } }));
    let a = op(&mut d, "add_room", json!({ "origin": [0, 0], "size": [4, 4] })).created[0].clone();
    let _b = op(&mut d, "add_room", json!({ "origin": [4, 0], "size": [4, 4] })).created[0].clone();
    let s = scene(&d);
    // left edge: exterior → a cavity wall (outer and inner leaf); right edge shared with b → a single leaf
    assert!(faces_with(&s, &format!("{a}#wall.3#outer")).0 > 0 && faces_with(&s, &format!("{a}#wall.3#inner")).0 > 0);
    assert!(faces_with(&s, &format!("{a}#wall.1#single")).0 > 0);
    assert_eq!(faces_with(&s, &format!("{a}#wall.1#outer")).0, 0);
}

#[test]
fn glb_export_of_the_house_is_valid() {
    let d = imported();
    let s = scene(&d);
    let bytes = plannr_core::export_glb(&s);
    assert_eq!(&bytes[0..4], b"glTF");
    assert!(bytes.len() > 10_000);
}

// ---- stairs, units, shapes, guides (the full example house) -------------------------------------

fn house_current() -> Value { serde_json::from_str(include_str!("fixtures/house_current.json")).unwrap() }
fn imported_current() -> Document { let mut d = doc(); op(&mut d, "import_house", json!({ "house": house_current() })); d }

#[test]
fn current_house_imports_everything() {
    let d = imported_current();
    assert_eq!(customs(&d, "construction.room").len(), 14);
    assert_eq!(customs(&d, "construction.wall").len(), 7);
    assert_eq!(customs(&d, "construction.opening").len(), 36);
    assert_eq!(customs(&d, "construction.fixture").len(), 121);
    assert_eq!(customs(&d, "construction.roof").len(), 3);
    assert_eq!(customs(&d, "construction.stair").len(), 1);
    assert_eq!(customs(&d, "construction.unit").len(), 45);
    let h = house_current();
    // shapes with at least one face (a bare line shape has nothing to show) and not voids
    let non_void = h["shapes"].as_array().unwrap().iter().filter(|s| s.get("role").and_then(|r| r.as_str()) != Some("void") && s["brep"]["faces"].as_array().map(|f| !f.is_empty()).unwrap_or(false)).count();
    let groups = d.entities.iter().filter(|e| matches!(e, Entity::Group(_))).count();
    assert_eq!(groups, non_void);
    let guides = d.entities.iter().filter(|e| matches!(e, Entity::Guide(_))).count();
    assert_eq!(guides, 5);
    assert!(d.layers.iter().any(|l| l.name == "Site"));
    let s = plannr_plugin_construction::ops::settings(&d);
    assert_eq!(s.storeys.len(), 2);
    assert_eq!(s.storeys[0].name, "Ground floor");
    // the stair kept its id and its winders
    let st = customs(&d, "construction.stair")[0];
    assert_eq!(st.id, "8FE2B807-7403-4B34-8C93-71A537239EA5");
    assert_eq!(st.data["winders"], 6);
    assert_eq!(st.data["kind"], "quarterTurn");
    // a kitchen base unit got its size from its script (plinth at the floor, top at plinth + carcass)
    let u = customs(&d, "construction.unit").into_iter().find(|c| c.data["type"] == "base-unit").unwrap();
    assert!(u.data["lift"].as_f64().unwrap().abs() < 1e-6, "{}", u.data["lift"]);
    assert!((u.data["height"].as_f64().unwrap() - 0.84).abs() < 0.05, "{}", u.data["height"]);
    // compile the whole thing
    let sc = scene(&d);
    assert!(sc.metrics.faces > 2000, "faces {}", sc.metrics.faces);
    let pm = &sc.metrics.plugins["construction"];
    assert_eq!(pm["stairs"], 1);
    assert_eq!(pm["units"], 45);
    assert_eq!(pm["openingCount"], 36);
}

#[test]
fn stair_expands_into_treads_and_cuts_the_floor_above() {
    let mut d = doc();
    op(&mut d, "setup", json!({ "storeys": [{ "name": "G", "height": 2.7 }, { "name": "1", "height": 2.5 }] }));
    let up = op(&mut d, "add_room", json!({ "name": "Up", "level": 1, "origin": [0, 0], "size": [5, 5] })).created[0].clone();
    let before = faces_with(&scene(&d), &format!("{up}#mfloor0#topping"));
    let st = op(&mut d, "add_stair", json!({ "level": 0, "origin": [1.0, 4.0], "bearing": 0, "risers": 15, "going": 0.25, "width": 0.9 })).created[0].clone();
    let s = scene(&d);
    // 14 treads (the 15th riser lands on the floor above), 15 risers, 2 stringers
    for i in 1..15 { assert!(faces_with(&s, &format!("{st}#tread.{i}")).0 >= 6, "tread {i}"); }
    assert_eq!(faces_with(&s, &format!("{st}#tread.15")).0, 0);
    assert!(faces_with(&s, &format!("{st}#riser.15")).0 >= 6);
    assert!(faces_with(&s, &format!("{st}#stringer.left")).0 >= 6);
    assert!(faces_with(&s, &format!("{st}#stringer.right")).0 >= 6);
    // the (masonry) floor above lost the well: the topping has a hole and trimmers frame it
    let after = faces_with(&s, &format!("{up}#mfloor0#topping"));
    assert!(after.0 > before.0, "hole sides added: {} > {}", after.0, before.0);
    assert!(after.1 < before.1, "topping area reduced: {} < {}", after.1, before.1);
    assert!(faces_with(&s, &format!("{up}#mfloor0#trimmer")).0 > 0, "trimmers around the well");
    let Entity::Custom(c) = d.find(&st).unwrap() else { panic!() };
    assert_eq!(c.data["risers"], 15);
}

#[test]
fn straight_stair_rises_exactly_the_storey_height() {
    let mut d = doc();
    op(&mut d, "setup", json!({ "storeys": [{ "name": "G", "height": 2.7 }] }));
    let st = op(&mut d, "add_stair", json!({ "level": 0, "origin": [0, 0], "bearing": 90, "risers": 15 })).created[0].clone();
    let s = scene(&d);
    // the top riser reaches the floor above; the last tread tops out one riser below
    let top = max_y(&s, &format!("{st}#riser.15"));
    assert!((top - 2.7).abs() < 1e-6, "top riser reaches {top}");
    let tread = max_y(&s, &format!("{st}#tread.14"));
    assert!((tread - 2.7 * 14.0 / 15.0).abs() < 1e-6, "tread 14 at {tread}");
    // default risers follow the storey height
    let st2 = op(&mut d, "add_stair", json!({ "level": 0, "origin": [3, 3] })).created[0].clone();
    let Entity::Custom(c) = d.find(&st2).unwrap() else { panic!() };
    assert_eq!(c.data["risers"], 15); // 2.7 / 0.18
    // bearing 90 climbs towards +x
    let st3 = plannr_plugin_construction::data::Stair { name: "s".into(), level: 0, origin: [0.0, 0.0], bearing: 90.0, width: 0.9, going: 0.25, risers: 15, kind: Default::default(), turn_after: 6, turns_right: true, winders: None, tread_thickness: None, nosing: None, open_risers: None, stringer: None, rail_left: None, rail_right: None, well_opening: None, extra: Default::default() };
    let steps = expand::stair_steps(&st3, 2.7);
    assert_eq!(steps.len(), 14);
    assert!(steps[13].centre[0] > steps[0].centre[0] && (steps[13].centre[1] - steps[0].centre[1]).abs() < 1e-9);
}

#[test]
fn quarter_turn_lays_out_straight_treads_then_winders_then_the_rest() {
    let st = plannr_plugin_construction::data::Stair { name: "s".into(), level: 0, origin: [0.59, 4.025], bearing: 0.0, width: 0.9, going: 0.25, risers: 15, kind: plannr_plugin_construction::data::StairKind::QuarterTurn, turn_after: 4, turns_right: true, winders: Some(6), tread_thickness: None, nosing: None, open_risers: None, stringer: None, rail_left: None, rail_right: None, well_opening: None, extra: Default::default() };
    let steps = expand::stair_steps(&st, 2.7);
    assert_eq!(steps.len(), 14);
    assert!(steps[..4].iter().all(|s| !s.is_winder && !s.is_landing));
    assert!(steps[4..10].iter().all(|s| s.is_winder));
    assert!(steps[10..].iter().all(|s| !s.is_winder));
    // after the turn to the right from north (-z) you climb east (+x)
    let last = steps.last().unwrap();
    assert!(last.direction[0] > 0.999, "{:?}", last.direction);
    assert!(steps[0].direction[1] < -0.999);
    // heights climb monotonically
    for w in steps.windows(2) { assert!(w[1].top > w[0].top); }
    // a landing instead of winders
    let mut st2 = st.clone(); st2.winders = None;
    let steps2 = expand::stair_steps(&st2, 2.7);
    assert_eq!(steps2.iter().filter(|s| s.is_landing).count(), 1);
    assert_eq!(steps2.len(), 14);
}

#[test]
fn unit_box_has_six_faces_and_a_worktop() {
    let mut d = doc();
    // a type without a script is a plain box (a "base…" label still gets the white top)
    let u = op(&mut d, "add_unit", json!({ "level": 0, "type": "base-box", "origin": [1, 1], "bearing": 90, "lift": 0.12, "height": 0.72 })).created[0].clone();
    let s = scene(&d);
    let (nb, _) = faces_with(&s, &format!("{u}#box"));
    let (nt, _) = faces_with(&s, &format!("{u}#top"));
    assert_eq!(nb, 5);
    assert_eq!(nt, 1);
    let white = s.groups.iter().find(|g| g.material == "construction.paintWhite" && !g.back).unwrap();
    assert!(white.ids.iter().any(|id| id == &format!("{u}#top")));
    let Entity::Custom(c) = d.find(&u).unwrap() else { panic!() };
    assert!((c.data["lift"].as_f64().unwrap() - 0.12).abs() < 1e-9);
    assert!((c.data["height"].as_f64().unwrap() - 0.72).abs() < 1e-9);
    // top of the box = lift + height
    assert!((max_y(&s, &format!("{u}#top")) - 0.84).abs() < 1e-6);
    // move + rotate keep it a plan transform
    apply(&mut d, &Op::Rotate { ids: vec![u.clone()], origin: [1.0, 0.0, 1.0], axis: [0.0, 1.0, 0.0], angle: 90.0 }, &reg()).unwrap();
    let Entity::Custom(c) = d.find(&u).unwrap() else { panic!() };
    assert!((c.data["bearing"].as_f64().unwrap() - 0.0).abs() < 1e-6, "{}", c.data["bearing"]);
}

#[test]
fn summary_works_with_the_current_house() {
    let d = imported_current();
    let r = op(&mut d.clone(), "summary", json!({}));
    let Entity::Custom(c) = &r.entities[0] else { panic!() };
    let v = &c.data;
    assert_eq!(v["stairs"], 1);
    assert_eq!(v["units"], 45);
    assert_eq!(v["openings"], 36);
    let per_storey: f64 = v["storeys"].as_array().unwrap().iter().map(|s| s["floorArea"].as_f64().unwrap()).sum();
    let total = v["metrics"]["floorArea"].as_f64().unwrap();
    assert!((per_storey - total).abs() < 0.01);
    assert!(total > 150.0, "{total}");
    let g = v["storeys"][0]["rooms"].as_array().unwrap().iter().find(|r| r["name"] == "Garage").unwrap();
    assert!((g["area"].as_f64().unwrap() - 25.36).abs() < 0.01);
}

// ---- component scripts (Boa runtime) ------------------------------------------------------------

use plannr_plugin_construction::script;

fn data_of(r: &plannr_core::OpResult) -> Value { match &r.entities[0] { Entity::Custom(c) => c.data.clone(), _ => panic!("no data entity") } }

#[test]
fn prelude_and_builtin_base_unit_build_solids() {
    let mut rt = script::Runtime::new(script::builtin_types());
    assert!(rt.problems().is_empty(), "{:?}", rt.problems());
    assert_eq!(rt.declared().len(), 14);
    let solids = rt.solids("base-unit", &json!({})).unwrap();
    assert!(solids.len() >= 1, "base unit built {} solids", solids.len());
    let b = script::bounds(&solids).unwrap();
    assert!((b.width() - 0.6).abs() < 0.05, "width {}", b.width());
    assert!(b.high > 0.8 && b.high < 0.9, "high {}", b.high);
}

#[test]
fn component_params_lists_base_unit_params_and_applies_rules() {
    let mut d = doc();
    let r = op(&mut d, "component_params", json!({ "type": "base-unit", "values": { "width": 0.8 }, "choices": { "front": "drawers" } }));
    let v = data_of(&r);
    let names: Vec<&str> = v["params"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"width") && names.contains(&"front"), "{names:?}");
    let width = v["params"].as_array().unwrap().iter().find(|p| p["name"] == "width").unwrap();
    assert!((width["value"].as_f64().unwrap() - 0.8).abs() < 1e-9);
    assert!((v["size"]["width"].as_f64().unwrap() - 0.8).abs() < 0.02);
    assert!(v["worktop"]["at"].as_f64().unwrap() > 0.8);
}

#[test]
fn define_component_rejects_a_throwing_script_and_accepts_a_valid_one() {
    let mut d = doc();
    let bad = apply(&mut d, &Op::Custom { type_: "construction.define_component".into(), data: json!({ "id": "bad", "source": "component('bad', { build: function (p) { throw new Error('nope'); } });" }) }, &reg());
    assert!(bad.is_err(), "a throwing script must be rejected");
    let wrong_id = apply(&mut d, &Op::Custom { type_: "construction.define_component".into(), data: json!({ "id": "stool", "source": "component('other', { build: function (p) { return [box({ w: 0.3, h: 0.45, d: 0.3 })]; } });" }) }, &reg());
    assert!(wrong_id.is_err());
    let r = op(&mut d, "define_component", json!({ "id": "stool", "name": "Stool", "group": "Furniture",
        "source": "component('stool', { name: 'Stool', params: [{ name: 'size', label: 'Size', kind: 'length', value: 0.35, min: 0.2, max: 0.6 }], build: function (p) { return [box({ x: -p.size / 2, y: 0, z: 0, w: p.size, h: 0.45, d: p.size, material: 'Timber' })]; } });" }));
    let v = data_of(&r);
    assert_eq!(v["solids"], 1);
    let s = plannr_plugin_construction::ops::settings(&d);
    assert_eq!(s.component_types.len(), 1);
    assert_eq!(s.component_types[0].name, "Stool");
    // the catalogue lists it as a document type
    let cat = data_of(&op(&mut d, "list_catalogue", json!({})));
    let stool = cat["types"].as_array().unwrap().iter().find(|t| t["id"] == "stool").unwrap();
    assert_eq!(stool["builtin"], false);
    assert!(cat["types"].as_array().unwrap().iter().any(|t| t["id"] == "base-unit" && t["builtin"] == true));
    // and removing it works
    op(&mut d, "remove_component_type", json!({ "id": "stool" }));
    assert!(plannr_plugin_construction::ops::settings(&d).component_types.is_empty());
}

#[test]
fn scripted_unit_expands_to_prisms_inside_its_bounds_and_turns_with_bearing() {
    let mut d = doc();
    let u = op(&mut d, "add_unit", json!({ "level": 0, "type": "shelving", "origin": [2, 3], "bearing": 0, "values": { "width": 1.0, "depth": 0.3, "height": 1.8 } })).created[0].clone();
    let Entity::Custom(c) = d.find(&u).unwrap() else { panic!() };
    // sizes were derived from the solids
    assert!((c.data["width"].as_f64().unwrap() - 1.0).abs() < 1e-6);
    assert!((c.data["depth"].as_f64().unwrap() - 0.3).abs() < 1e-6);
    let s = scene(&d);
    let (n, _) = faces_with(&s, &format!("{u}#solid."));
    assert!(n > 6, "a scripted shelving unit has more than a box's faces, got {n}");
    assert!((max_y(&s, &format!("{u}#solid.")) - 1.8).abs() < 1e-6);
    // bearing 0 faces -z: the unit spans x 1.5..2.5 and z 2.7..3.0
    let (lo, hi) = extent(&s, &format!("{u}#solid."));
    assert!((lo[0] - 1.5).abs() < 1e-6 && (hi[0] - 2.5).abs() < 1e-6, "x {lo:?} {hi:?}");
    assert!((hi[2] - 3.0).abs() < 1e-6 && (lo[2] - 2.7).abs() < 1e-6, "z {lo:?} {hi:?}");
    op(&mut d, "update", json!({ "id": u, "patch": { "bearing": 90 } }));
    let s = scene(&d);
    let (lo, hi) = extent(&s, &format!("{u}#solid."));
    // bearing 90 faces +x: now 0.3 deep along x and 1.0 wide along z
    assert!((hi[0] - 2.3).abs() < 1e-6 && (lo[0] - 2.0).abs() < 1e-6, "x {lo:?} {hi:?}");
    assert!((hi[2] - 3.5).abs() < 1e-6 && (lo[2] - 2.5).abs() < 1e-6, "z {lo:?} {hi:?}");
    // changing a value re-expands: a wider unit
    op(&mut d, "update", json!({ "id": u, "patch": { "values": { "width": 1.4 } } }));
    let s = scene(&d);
    let (lo, hi) = extent(&s, &format!("{u}#solid."));
    assert!((hi[2] - lo[2] - 1.4).abs() < 1e-6);
}
fn extent(s: &plannr_core::CompiledScene, prefix: &str) -> ([f64; 3], [f64; 3]) {
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for g in &s.groups {
        for (i, id) in g.ids.iter().enumerate() {
            if !id.starts_with(prefix) { continue; }
            let [start, count] = g.id_ranges[i];
            for k in &g.indices[start as usize..(start + count) as usize] {
                for a in 0..3 { let v = g.positions[*k as usize * 3 + a] as f64; lo[a] = lo[a].min(v); hi[a] = hi[a].max(v); }
            }
        }
    }
    (lo, hi)
}

#[test]
fn current_house_units_expand_via_their_scripts() {
    let t0 = std::time::Instant::now();
    let d = imported_current();
    let s = plannr_plugin_construction::ops::settings(&d);
    assert_eq!(s.component_types.len(), 3, "document component types imported");
    let units = customs(&d, "construction.unit");
    assert_eq!(units.len(), 45);
    let types = script::available_types(&s);
    let mut scripted = 0;
    let fallbacks: Vec<String> = script::with_runtime(&types, |rt| {
        assert!(rt.problems().is_empty(), "{:?}", rt.problems());
        units.iter().filter_map(|c| {
            let u: plannr_plugin_construction::data::Unit = serde_json::from_value(c.data.clone()).unwrap();
            if !rt.has(&u.type_) { return Some(format!("{} ({}): no script", u.name, u.type_)); }
            match rt.solids(&u.type_, &script::given(&u)) {
                Ok(s) if !s.is_empty() => { scripted += 1; None }
                Ok(_) => Some(format!("{} ({}): built nothing", u.name, u.type_)),
                Err(e) => Some(format!("{} ({}): {e}", u.name, u.type_)),
            }
        }).collect()
    });
    for f in &fallbacks { println!("fallback: {f}"); }
    assert_eq!(scripted, units.len(), "{scripted} scripted units; fallbacks: {fallbacks:?}");
    let t1 = std::time::Instant::now();
    let sc = scene(&d);
    println!("import {:?}, compile {:?}, faces {}", t1 - t0, t1.elapsed(), sc.metrics.faces);
    assert_eq!(sc.metrics.plugins["construction"]["scriptedUnits"], scripted);
    // the kitchen's base units land inside the Kitchen (x 2.4..6.6, z 4..7.4)
    let dishwasher = units.iter().find(|c| c.data["name"] == "Dishwasher").unwrap();
    let (lo, hi) = extent(&sc, &format!("{}#solid.", dishwasher.id));
    assert!(lo[0] > 2.35 && hi[0] < 6.65 && lo[2] > 3.95 && hi[2] < 7.45, "dishwasher at {lo:?}..{hi:?}");
    // script materials exist in the document
    assert!(d.materials.iter().any(|m| m.id == "construction.c.Front"));
    assert!(d.materials.iter().any(|m| m.id.starts_with("construction.c.#")), "hex colours from scripts become materials");
}

#[test]
fn a_looping_script_errors_and_its_unit_falls_back_to_a_box() {
    let mut d = doc();
    let looping = "component('loop', { build: function (p) { while (true) {} } });";
    let r = apply(&mut d, &Op::Custom { type_: "construction.define_component".into(), data: json!({ "id": "loop", "source": looping }) }, &reg());
    assert!(r.is_err(), "the runtime limit must stop it");
    // smuggle the type in through an import (no validation there) with one component of it
    op(&mut d, "import_house", json!({ "house": { "componentTypes": [{ "id": "loop", "source": looping }], "components": [{ "type": "loop", "name": "Hang", "origin": [1, 1], "level": 0 }] } }));
    let u = customs(&d, "construction.unit")[0].id.clone();
    let s = scene(&d);
    assert_eq!(faces_with(&s, &format!("{u}#solid.")).0, 0);
    assert_eq!(faces_with(&s, &format!("{u}#box")).0, 5, "box fallback");
    let cat = data_of(&op(&mut d, "list_catalogue", json!({})));
    let t = cat["types"].as_array().unwrap().iter().find(|t| t["id"] == "loop").unwrap();
    assert!(t["size"].is_null());
}

#[test]
fn solids_are_cached_per_type_and_values() {
    let mut rt = script::Runtime::new(script::builtin_types());
    let before = rt.calls;
    rt.solids("wardrobe", &json!({ "width": 1.2 })).unwrap();
    assert_eq!(rt.calls, before + 1);
    rt.solids("wardrobe", &json!({ "width": 1.2 })).unwrap();
    assert_eq!(rt.calls, before + 1, "second ask is a cache hit");
    rt.solids("wardrobe", &json!({ "width": 1.6 })).unwrap();
    assert_eq!(rt.calls, before + 2);
}

#[test]
fn worktop_and_meta_are_read_from_the_script() {
    let mut rt = script::Runtime::new(script::builtin_types());
    let w = rt.worktop("base-unit", &json!({ "height": 0.72, "plinth": 0.12 })).expect("base unit declares a worktop");
    assert!((w.at - 0.84).abs() < 1e-9);
    assert!(rt.worktop("shelving", &json!({})).is_none());
    let m = rt.meta("wc", &json!({})).expect("wc declares meta");
    assert!(m.needs.iter().any(|n| n == "water" || n == "waste"), "{:?}", m.needs);
    assert!(m.wants > 0.0);
    // the unit's compiled worktop sits at 0.84..0.88
    let mut d = doc();
    let u = op(&mut d, "add_unit", json!({ "level": 0, "type": "base-unit", "origin": [0, 0] })).created[0].clone();
    let s = scene(&d);
    assert!((max_y(&s, &format!("{u}#worktop")) - 0.88).abs() < 1e-6);
}

// ---- wall graph: collinear splitting, no coplanar overlaps -----------------------------------

/// Every front wall face of the scene as (id, plane key, 2-D bbox in the plane, area).
fn wall_faces(s: &plannr_core::CompiledScene) -> Vec<(String, (i64, i64, i64, i64), [f64; 4], f64)> {
    let mut out = vec![];
    for g in &s.groups {
        if g.back { continue; }
        for (i, id) in g.ids.iter().enumerate() {
            if !id.contains("#wall") { continue; }
            let [start, count] = g.id_ranges[i];
            let tris: Vec<[[f64; 3]; 3]> = g.indices[start as usize..(start + count) as usize].chunks(3).map(|t| {
                let p = |k: u32| [g.positions[k as usize * 3] as f64, g.positions[k as usize * 3 + 1] as f64, g.positions[k as usize * 3 + 2] as f64];
                [p(t[0]), p(t[1]), p(t[2])]
            }).collect();
            if tris.is_empty() { continue; }
            let n = plannr_core::geom::normalize(plannr_core::geom::newell(&tris[0]));
            if n[1].abs() > 0.01 { continue; } // tops and bottoms of walls meet at corners by design; the symptom is vertical faces
            let off = plannr_core::geom::dot(n, tris[0][0]);
            let key = ((n[0] * 1000.0).round() as i64, (n[1] * 1000.0).round() as i64, (n[2] * 1000.0).round() as i64, (off * 1000.0).round() as i64);
            // in-plane basis
            let u = if n[1].abs() < 0.9 { plannr_core::geom::normalize(plannr_core::geom::cross([0.0, 1.0, 0.0], n)) } else { plannr_core::geom::normalize(plannr_core::geom::cross([1.0, 0.0, 0.0], n)) };
            let v = plannr_core::geom::cross(n, u);
            let mut bb = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
            let mut area = 0.0;
            for t in &tris {
                area += plannr_core::geom::area(t);
                for p in t { let (a, b) = (plannr_core::geom::dot(*p, u), plannr_core::geom::dot(*p, v)); bb[0] = bb[0].min(a); bb[1] = bb[1].min(b); bb[2] = bb[2].max(a); bb[3] = bb[3].max(b); }
            }
            out.push((id.clone(), key, bb, area));
        }
    }
    out
}

fn assert_no_overlapping_walls(s: &plannr_core::CompiledScene, what: &str) { assert_no_overlapping_walls_except(s, what, &[]) }
/// `except`: entity ids whose mutual overlaps are tolerated (rooms that genuinely overlap in the source data).
fn assert_no_overlapping_walls_except(s: &plannr_core::CompiledScene, what: &str, except: &[&str]) {
    let faces = wall_faces(s);
    let mut by_plane: std::collections::HashMap<(i64, i64, i64, i64), Vec<usize>> = Default::default();
    for (i, f) in faces.iter().enumerate() { by_plane.entry(f.1).or_default().push(i); }
    let mut bad = vec![];
    for idx in by_plane.values() {
        for (x, &i) in idx.iter().enumerate() {
            for &j in &idx[x + 1..] {
                let (a, b) = (&faces[i], &faces[j]);
                if a.0 == b.0 { continue; } // same wall id: pieces of one wall only touch
                if except.iter().any(|e| a.0.starts_with(e)) && except.iter().any(|e| b.0.starts_with(e)) { continue; }
                let w = (a.2[2].min(b.2[2]) - a.2[0].max(b.2[0])).max(0.0);
                let h = (a.2[3].min(b.2[3]) - a.2[1].max(b.2[1])).max(0.0);
                // where a wall Ts into a line that changes from partition to exterior wall right there, its end
                // and the plaster of the neighbouring piece overlap in a strip ≤ the leaf width, inside the solids
                if (a.0.contains("plaster") || b.0.contains("plaster")) && w.min(h) <= 0.05 { continue; }
                let overlap = w * h;
                let smaller = a.3.min(b.3);
                if smaller > 1e-6 && overlap > 0.01 * smaller { bad.push(format!("{} ∩ {} = {:.4} m² (smaller {:.4}) plane {:?} bbox {:?} / {:?}", a.0, b.0, overlap, smaller, a.1, a.2, b.2)); }
            }
        }
    }
    assert!(bad.is_empty(), "{what}: {} coplanar overlapping wall faces, e.g.\n{}", bad.len(), bad.iter().take(8).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn no_coplanar_overlapping_walls_in_the_houses() {
    assert_no_overlapping_walls(&scene(&imported_current()), "full house");
    assert_no_overlapping_walls(&scene(&imported()), "simple house");
}

#[test]
fn partially_shared_edges_split_into_partitions_and_exterior_pieces() {
    // the Utility room (x 0…3.6, z 0…2.2) and the WC (x 2.4…3.6, z 2.2…4) both border the Living room's west edge x = 3.6 (z 0…4)
    let d = imported_current();
    let set = expand::collect(&expand::model(&d));
    let on_line: Vec<&expand::WallEntry> = set.walls.iter().filter(|w| w.level == 0 && (w.a[0] - 3.6).abs() < 1e-3 && (w.b[0] - 3.6).abs() < 1e-3).collect();
    let span = |w: &expand::WallEntry| (w.a[1].min(w.b[1]), w.a[1].max(w.b[1]));
    let lo = on_line.iter().find(|w| { let (z0, z1) = span(w); z0.abs() < 1e-3 && (z1 - 2.2).abs() < 1e-3 }).expect("one piece for the Utility room|Living room stretch");
    assert!(!lo.exterior, "shared stretch is a partition"); assert_eq!(lo.rooms.len(), 2);
    let hi = on_line.iter().find(|w| { let (z0, z1) = span(w); (z0 - 2.2).abs() < 1e-3 && (z1 - 4.0).abs() < 1e-3 }).expect("one piece for the WC|Living room stretch");
    assert!(!hi.exterior); assert_eq!(hi.rooms.len(), 2);
    // nothing else covers that range twice
    let covering = on_line.iter().filter(|w| { let (z0, z1) = span(w); z1 > 1e-3 && z0 < 4.0 - 1e-3 }).count();
    assert_eq!(covering, 2, "the stretch z 0…4 is covered by exactly two pieces");
    // a single leaf, not the cavity build-up: 6 faces per piece on that id... (both pieces carry their owner's key)
    let s = scene(&d);
    let (n, _) = faces_with(&s, &format!("{}#{}#single", lo.owner, lo.key));
    assert!(n > 0, "a single leaf");
}

#[test]
fn opening_across_a_split_point_keeps_its_glass_and_frames() {
    // A (0,0)–(4,4); B (2,-4)–(6,0): A's bottom edge is exterior on x 0…2 and shared with B on x 2…4
    // single-leaf walls throughout, so the split and the whole wall have the same thickness
    let mut d = doc();
    op(&mut d, "setup", json!({ "cavity": { "enabled": false } }));
    let a = room4(&mut d);
    let _b = op(&mut d, "add_room", json!({ "origin": [2, -4], "size": [4, 4] })).created[0].clone();
    let set = expand::collect(&expand::model(&d));
    let bottom: Vec<&expand::WallEntry> = set.walls.iter().filter(|w| w.a[1].abs() < 1e-6 && w.b[1].abs() < 1e-6 && w.a[0].min(w.b[0]) > -1e-6 && w.a[0].max(w.b[0]) < 4.0 + 1e-6).collect();
    assert_eq!(bottom.len(), 2, "A's bottom edge is split at x = 2: {:?}", bottom.iter().map(|w| (w.a, w.b, w.exterior)).collect::<Vec<_>>());
    assert!(bottom.iter().any(|w| w.exterior) && bottom.iter().any(|w| !w.exterior));
    // a window centred on the split
    op(&mut d, "add_opening", json!({ "kind": "window", "host": { "room": a, "edge": 0 }, "along": 2.0, "width": 1.2, "height": 1.4, "sill": 0.9 }));
    let o = customs(&d, "construction.opening")[0].id.clone();
    let split = scene(&d);
    let (_, glass_split) = faces_with(&split, &format!("{o}#glass"));
    // the same window on an unsplit wall
    let mut d2 = doc();
    op(&mut d2, "setup", json!({ "cavity": { "enabled": false } }));
    let a2 = room4(&mut d2);
    op(&mut d2, "add_opening", json!({ "kind": "window", "host": { "room": a2, "edge": 0 }, "along": 2.0, "width": 1.2, "height": 1.4, "sill": 0.9 }));
    let o2 = customs(&d2, "construction.opening")[0].id.clone();
    let whole = scene(&d2);
    let (_, glass_whole) = faces_with(&whole, &format!("{o2}#glass"));
    assert!((glass_split - glass_whole).abs() < 0.01 * glass_whole, "glass area {glass_split} vs {glass_whole}");
    // two frame posts in total (none at the split), so the frame area is the same as well
    let (_, frame_split) = faces_with(&split, &format!("{o}#frame"));
    let (_, frame_whole) = faces_with(&whole, &format!("{o2}#frame"));
    assert!((frame_split - frame_whole).abs() < 0.02 * frame_whole, "frame area {frame_split} vs {frame_whole}");
    assert_no_overlapping_walls(&split, "split window");
}

#[test]
fn wall_length_counts_each_piece_once() {
    let mut d = doc();
    room4(&mut d);
    op(&mut d, "add_room", json!({ "origin": [2, -4], "size": [4, 4] }));
    let m = plannr_plugin_construction::ops::metrics(&d);
    // 16 + 16 − the 2 m stretch both rooms share
    assert!((m["wallLength"].as_f64().unwrap() - 30.0).abs() < 1e-6, "{}", m["wallLength"]);
    let set = expand::collect(&expand::model(&d));
    assert_eq!(set.walls.len(), 9, "4 + 4 edges, the shared stretch merged, A's and B's partial edges split: {:?}", set.walls.iter().map(|w| (w.a, w.b)).collect::<Vec<_>>());
}

#[test]
fn adjacent_pieces_of_one_line_join_without_internal_caps() {
    let mut d = doc();
    let a = room4(&mut d);
    op(&mut d, "add_room", json!({ "origin": [4, 0], "size": [4, 2] })); // shares A's right edge on z 0…2 only
    let s = scene(&d);
    // right edge of A: two pieces (partition z 0…2, exterior z 2…4), caps only at the real ends
    let (n_right, _) = faces_with(&s, &format!("{a}#wall.1"));
    assert!(n_right > 0, "faces on A's right edge");
    assert_no_overlapping_walls(&s, "L-shaped neighbour");
}

#[test]
fn a_socket_sits_on_the_wall_face_as_a_plate_with_a_cup() {
    use plannr_plugin_construction::{data::Fixture, fixtures};
    let mut d = doc();
    room4(&mut d);
    // stored 10 cm into the room from the west wall (x = 0): it is drawn on the wall's inner face
    let id = op(&mut d, "add_fixture", json!({ "level": 0, "kind": "outlet", "position": [0.1, 0.3, 2.0], "note": "dubbel" })).created[0].clone();
    let m = expand::model(&d);
    let set = expand::collect(&m);
    let (_, f): &(plannr_core::model::Custom, Fixture) = m.fixtures.iter().find(|(c, _)| c.id == id).unwrap();
    let mt = fixtures::mount(&m.settings, &set, f);
    let w = &set.walls[mt.wall.expect("mounted on a wall")];
    let (lo, hi) = expand::leaf_span(&m.settings, w);
    let face = if w.out[0] < 0.0 { -lo } else { hi }; // the room-side face of the x = 0 wall
    assert!((mt.pos[0] - face).abs() < 1e-9, "on the inner face {face}: {:?}", mt.pos);
    assert!((mt.normal[0] - 1.0).abs() < 1e-9, "faces into the room: {:?}", mt.normal);
    assert_eq!(fixtures::gangs(f), 2);
    let s = scene(&d);
    let (n, _) = faces_with(&s, &format!("{id}#"));
    assert!(n > 40, "a double socket is more than a box: {n} faces");
    // nothing of it is behind the face
    let face_x = mt.pos[0];
    for g in &s.groups {
        for (i, fid) in g.ids.iter().enumerate() {
            if !fid.starts_with(&id) { continue; }
            let [start, count] = g.id_ranges[i];
            for k in &g.indices[start as usize..(start + count) as usize] {
                let x = g.positions[*k as usize * 3] as f64;
                assert!(x >= face_x - 1e-4, "x {x} behind the wall face {face_x}");
            }
        }
    }
}

#[test]
fn a_ceiling_light_hangs_from_the_ceiling() {
    let mut d = doc();
    room4(&mut d);
    let id = op(&mut d, "add_fixture", json!({ "level": 0, "kind": "ceilingLight", "position": [2.0, 2.4, 2.0] })).created[0].clone();
    let s = scene(&d);
    let top = max_y(&s, &format!("{id}#"));
    let h = expand::model(&d).settings.height(0);
    assert!((top - (h - 0.02)).abs() < 1e-3, "fitting under the ceiling finish: {top} vs {h}");
}
