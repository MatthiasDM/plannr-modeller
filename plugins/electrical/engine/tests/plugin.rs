use plannr_core::model::{Document, Entity};
use plannr_core::ops::{apply, Op, OpResult};
use plannr_core::plugin::{Plugin, Registry};
use plannr_plugin_construction::ConstructionPlugin;
use plannr_plugin_electrical::{arei, data::*, ElectricalPlugin};
use serde_json::{json, Value};

fn registry() -> Registry { let mut r = Registry::new(); r.register(Box::new(ConstructionPlugin)); r.register(Box::new(ElectricalPlugin)); r }
fn house_doc() -> Document {
    let house: Value = serde_json::from_str(include_str!("fixtures/house.json")).unwrap();
    let mut doc = Document::default();
    let reg = registry(); reg.on_load(&mut doc);
    reg.apply("construction.import_house", json!({ "house": house }), &mut doc).expect("import house");
    doc
}
fn electrical() -> Value { serde_json::from_str(include_str!("fixtures/electrical.json")).unwrap() }
fn el(doc: &mut Document, op: &str, data: Value) -> Result<OpResult, plannr_core::ops::OpError> { registry().apply(&format!("electrical.{op}"), data, doc) }
fn data_of(r: &OpResult) -> Value { match &r.entities[0] { Entity::Custom(c) => c.data.clone(), _ => Value::Null } }
fn findings(doc: &mut Document) -> Vec<Value> { data_of(&el(doc, "check", json!({})).unwrap())["findings"].as_array().unwrap().clone() }
fn titles(f: &[Value]) -> Vec<String> { f.iter().map(|x| x["title"].as_str().unwrap().to_string()).collect() }
fn any_point(doc: &Document, kind: &str) -> String { House::read(doc).fixtures.iter().find(|f| f.kind == kind).map(|f| f.id.clone()).unwrap() }

#[test]
fn imports_the_house_electrical_with_all_points_resolved() {
    let mut doc = house_doc();
    let r = el(&mut doc, "import_electrical", json!({ "electrical": electrical() })).unwrap();
    let d = data_of(&r);
    let s = settings(&doc);
    assert_eq!(s.circuits.len(), 18);
    assert_eq!(s.board.phases, 3); assert_eq!(s.board.main_breaker, 40); assert_eq!(s.board.main_differential, 300); assert_eq!(s.board.default_cable_type, "XVB");
    assert!(s.board.panel.is_some(), "the board should point at the panel fixture");
    let dropped = d["dropped"].as_array().unwrap();
    let total: usize = electrical()["circuits"].as_array().unwrap().iter().map(|c| c["fixtureIDs"].as_array().unwrap().len()).sum();
    println!("dropped {} of {} fixture ids: {:?}", dropped.len(), total, dropped);
    assert!(dropped.len() * 10 <= total, "≥ 90 % of the fixture ids must resolve");
    // every circuit of the example installation points at a fixture that exists
    assert!(dropped.is_empty(), "no fixture id may drop: {dropped:?}");
    assert_eq!(s.circuits[0].number, 1); assert_eq!(s.circuits[17].number, 18);
}

#[test]
fn import_matches_fresh_fixture_ids_by_position() {
    // a house rebuilt through add_fixture has new ids: Studio's fixtures list resolves them
    let house: Value = serde_json::from_str(include_str!("fixtures/house.json")).unwrap();
    let mut doc = house_doc();
    for e in doc.entities.iter_mut() { if let Entity::Custom(c) = e { if c.type_ == "construction.fixture" { c.id = format!("fresh-{}", c.id); } } }
    let d = data_of(&el(&mut doc, "import_electrical", json!({ "electrical": electrical(), "fixtures": house["fixtures"] })).unwrap());
    assert!(d["matchedByPosition"].as_u64().unwrap() >= 70, "{d}");
    assert!(d["dropped"].as_array().unwrap().is_empty());
    assert!(settings(&doc).circuits.iter().all(|c| c.points.iter().all(|p| p.starts_with("fresh-"))));
}

#[test]
fn imported_installation_has_no_cable_sizing_errors_and_demand_fits() {
    let mut doc = house_doc();
    el(&mut doc, "import_electrical", json!({ "electrical": electrical() })).unwrap();
    let f = findings(&mut doc);
    let t = titles(&f);
    println!("findings: {:#?}", f.iter().map(|x| format!("{} {} — {}", x["severity"], x["title"], x["detail"])).collect::<Vec<_>>());
    assert!(!t.contains(&"Breaker too large for the cable".to_string()));
    assert!(!t.contains(&"No main differential".to_string()));
    assert!(!t.contains(&"Estimated load over capacity".to_string()));
    let s = settings(&doc); let h = House::read(&doc);
    assert!(arei::total_demand(&s, &h) < s.board.capacity_kw());
    let sum = data_of(&el(&mut doc, "summary", json!({})).unwrap());
    assert_eq!(sum["circuits"].as_array().unwrap().len(), 18);
    assert!(sum["capacity_kW"].as_f64().unwrap() > 27.0 && sum["capacity_kW"].as_f64().unwrap() < 28.0, "40 A three-phase ≈ 27.7 kW");
}

#[test]
fn breaker_larger_than_the_cable_is_an_error() {
    let mut doc = house_doc();
    el(&mut doc, "add_circuit", json!({ "name": "Test", "type": "sockets", "breaker": 25, "cableSection": 2.5 })).unwrap();
    let f = findings(&mut doc);
    let e = f.iter().find(|x| x["title"] == "Breaker too large for the cable").expect("error expected");
    assert_eq!(e["severity"], "error"); assert!(e["detail"].as_str().unwrap().contains("20 A limit"));
}

#[test]
fn sockets_without_30ma_is_an_error_and_lighting_without_is_not() {
    let mut doc = house_doc();
    el(&mut doc, "add_circuit", json!({ "name": "SC", "type": "sockets", "rcd": "none" })).unwrap();
    el(&mut doc, "add_circuit", json!({ "name": "Licht", "type": "lighting", "rcd": "none" })).unwrap();
    let f = findings(&mut doc);
    let missing: Vec<&Value> = f.iter().filter(|x| x["title"] == "Missing 30 mA protection").collect();
    assert_eq!(missing.len(), 1); assert!(missing[0]["detail"].as_str().unwrap().starts_with("SC"));
}

#[test]
fn nine_sockets_on_one_circuit_is_a_warning() {
    let mut doc = house_doc();
    let h = House::read(&doc);
    let outlets: Vec<String> = h.fixtures.iter().filter(|f| f.kind == "outlet").take(9).map(|f| f.id.clone()).collect();
    assert_eq!(outlets.len(), 9);
    el(&mut doc, "add_circuit", json!({ "name": "Veel", "type": "sockets", "points": outlets })).unwrap();
    let f = findings(&mut doc);
    let w = f.iter().find(|x| x["title"] == "Too many points on a circuit").expect("warning expected");
    assert_eq!(w["severity"], "warning"); assert!(w["detail"].as_str().unwrap().contains("9 points"));
}

#[test]
fn dedicated_washing_machine_circuit_with_two_points_is_a_warning() {
    let mut doc = house_doc();
    let h = House::read(&doc);
    let outlets: Vec<String> = h.fixtures.iter().filter(|f| f.kind == "outlet").take(2).map(|f| f.id.clone()).collect();
    el(&mut doc, "add_circuit", json!({ "name": "Wasmachine", "type": "washingMachine", "points": outlets })).unwrap();
    let f = findings(&mut doc);
    assert!(f.iter().any(|x| x["title"] == "Dedicated circuit shares points" && x["severity"] == "warning"));
}

#[test]
fn assign_moves_a_point_between_circuits() {
    let mut doc = house_doc();
    let p = any_point(&doc, "outlet");
    let a = el(&mut doc, "add_circuit", json!({ "name": "A", "type": "sockets", "points": [p] })).unwrap().created[0].clone();
    let b = el(&mut doc, "add_circuit", json!({ "name": "B", "type": "sockets" })).unwrap().created[0].clone();
    el(&mut doc, "assign", json!({ "points": [p], "circuit": b })).unwrap();
    let s = settings(&doc);
    assert!(s.circuits.iter().find(|c| c.id == a).unwrap().points.is_empty());
    assert_eq!(s.circuits.iter().find(|c| c.id == b).unwrap().points, vec![p.clone()]);
    el(&mut doc, "assign", json!({ "points": [p] })).unwrap();
    assert!(settings(&doc).circuits.iter().all(|c| c.points.is_empty()));
    assert!(el(&mut doc, "assign", json!({ "points": ["nope"], "circuit": b })).is_err());
}

/// True when a point (y above its storey floor) lies inside a wall, or in the floor or ceiling build-up.
fn in_fabric(doc: &Document, level: usize, p: [f64; 3]) -> bool {
    use plannr_plugin_construction::expand;
    let m = expand::model(doc);
    let set = expand::collect(&m);
    let h = m.settings.height(level);
    if p[1] <= 0.0 || p[1] >= h - 0.02 { return true; }
    set.walls.iter().filter(|w| w.level == level).any(|w| {
        let l = w.length(); let d = [(w.b[0] - w.a[0]) / l, (w.b[1] - w.a[1]) / l];
        let t = (p[0] - w.a[0]) * d[0] + (p[2] - w.a[1]) * d[1];
        let off = (p[0] - w.a[0]) * w.out[0] + (p[2] - w.a[1]) * w.out[1];
        let (lo, hi) = expand::leaf_span(&m.settings, w);
        t >= -0.13 && t <= l + 0.13 && off >= lo - 1e-6 && off <= hi + 1e-6
    })
}

#[test]
fn auto_route_ceiling_runs_inside_the_walls() {
    let mut doc = house_doc();
    let h = House::read(&doc);
    let panel = h.panels()[0].clone();
    let outlet = h.fixtures.iter().find(|f| f.kind == "outlet" && f.level == panel.level).unwrap().clone();
    let c = el(&mut doc, "add_circuit", json!({ "name": "SC", "type": "sockets", "points": [outlet.id] })).unwrap().created[0].clone();
    let r = el(&mut doc, "auto_route", json!({ "mode": "ceiling", "circuit": c })).unwrap();
    assert_eq!(r.created.len(), 1);
    let ks = plannr_plugin_electrical::ops::conduits(&doc);
    assert_eq!(ks.len(), 1);
    let k = &ks[0].1;
    assert!(k.points.len() >= 4, "{:?}", k.points);
    // starts at the top of the board, ends at the socket's height, runs 15 cm under the ceiling
    assert!((k.points[0][1] - (panel.position[1] + 0.3)).abs() < 1e-9);
    assert!((k.points.last().unwrap()[1] - outlet.position[1]).abs() < 1e-9);
    // 15 cm under the ceiling, i.e. under the structure in the top of the storey (the masonry floor above)
    let cs = plannr_plugin_construction::expand::model(&doc).settings;
    let structure = cs.structure.iter().filter(|t| t.level == panel.level).map(|t| t.depth).fold(0.0, f64::max);
    let top = h.height(panel.level) - structure - 0.15;
    assert!(k.points.iter().any(|p| (p[1] - top).abs() < 1e-9));
    for p in &k.points { assert!(in_fabric(&doc, k.level, *p), "{p:?} is not inside a wall"); }
    assert!(k.auto && k.circuit.as_deref() == Some(c.as_str()));
    // re-routing replaces, straight mode has 2 points
    let r2 = el(&mut doc, "auto_route", json!({ "mode": "straight", "circuit": c })).unwrap();
    assert_eq!(r2.removed.len(), 1);
    let ks = plannr_plugin_electrical::ops::conduits(&doc);
    assert_eq!(ks.len(), 1); assert_eq!(ks[0].1.points.len(), 2);
}

#[test]
fn auto_route_to_another_storey_adds_a_riser() {
    let mut doc = house_doc();
    let h = House::read(&doc);
    let panel = h.panels()[0].clone();
    let up = h.fixtures.iter().find(|f| f.kind == "outlet" && f.level != panel.level).unwrap().clone();
    let c = el(&mut doc, "add_circuit", json!({ "name": "Boven", "type": "sockets", "points": [up.id] })).unwrap().created[0].clone();
    let r = el(&mut doc, "auto_route", json!({ "mode": "ceiling", "circuit": c })).unwrap();
    assert_eq!(r.created.len(), 2, "riser + run");
    let ks = plannr_plugin_electrical::ops::conduits(&doc);
    let riser = ks.iter().find(|(_, k)| k.extra.get("riser").is_some()).expect("riser");
    assert_eq!(riser.1.level, panel.level.min(up.level));
    // the riser climbs into the storey above inside a wall
    let last = *riser.1.points.last().unwrap();
    let prev = riser.1.points[riser.1.points.len() - 2];
    assert!(last[1] > h.height(riser.1.level) && (last[0] - prev[0]).abs() < 1e-9 && (last[2] - prev[2]).abs() < 1e-9);
    let run = ks.iter().find(|(_, k)| k.extra.get("riser").is_none()).unwrap();
    assert_eq!(run.1.level, up.level);
    assert!((run.1.points[0][0] - last[0]).abs() < 1e-9 && (run.1.points[0][2] - last[2]).abs() < 1e-9, "the run continues from the riser");
}

#[test]
fn the_whole_installation_is_routed_inside_walls_floors_and_ceilings() {
    let mut doc = house_doc();
    el(&mut doc, "import_electrical", json!({ "electrical": electrical() })).unwrap();
    el(&mut doc, "auto_route", json!({ "mode": "ceiling" })).unwrap();
    let ks = plannr_plugin_electrical::ops::conduits(&doc);
    assert!(ks.len() > 40);
    let (mut inside, mut total) = (0usize, 0usize);
    let mut outside = vec![];
    for (_, k) in &ks {
        for w in k.points.windows(2) {
            let n = ((plannr_core::geom::dist(w[0], w[1]) / 0.05).ceil() as usize).max(1);
            for i in 0..=n {
                let p = plannr_core::geom::lerp(w[0], w[1], i as f64 / n as f64);
                total += 1;
                if in_fabric(&doc, k.level, p) { inside += 1; } else if outside.len() < 10 { outside.push(p); }
            }
        }
    }
    println!("{inside} of {total} samples inside the fabric; outside e.g. {outside:?}");
    assert!(inside as f64 >= 0.97 * total as f64, "{inside}/{total} inside; e.g. {outside:?}");
}

#[test]
fn situation_svg_has_one_symbol_per_point_and_is_valid_xml() {
    let mut doc = house_doc();
    el(&mut doc, "import_electrical", json!({ "electrical": electrical() })).unwrap();
    let d = data_of(&el(&mut doc, "situation_svg", json!({ "level": 0 })).unwrap());
    let svg = d["svg"].as_str().unwrap();
    let h = House::read(&doc);
    let n = h.fixtures.iter().filter(|f| f.level == 0 && ELECTRICAL_KINDS.contains(&f.kind.as_str())).count();
    let legend = plannr_plugin_electrical::svg::LEGEND.len();
    let symbols = svg.matches("<g transform=\"translate(").count();
    assert_eq!(symbols, n + legend, "one symbol per electrical point plus the legend");
    assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
    // balanced tags (crude XML validity)
    for tag in ["g", "text", "svg"] { assert_eq!(svg.matches(&format!("<{tag}")).count(), svg.matches(&format!("</{tag}>")).count(), "unbalanced <{tag}>"); }
    assert!(svg.contains("Ground floor") || svg.contains("GROUND FLOOR"));
    assert!(svg.contains("Example Street 1"));
}

#[test]
fn oneline_svg_lists_every_circuit() {
    let mut doc = house_doc();
    el(&mut doc, "import_electrical", json!({ "electrical": electrical() })).unwrap();
    let svg = data_of(&el(&mut doc, "oneline_svg", json!({})).unwrap())["svg"].as_str().unwrap().to_string();
    for c in settings(&doc).circuits { assert!(svg.contains(&format!(">{}</text>", c.number)), "circuit {} missing", c.number); }
    assert!(svg.contains("kWh") && svg.contains("40 A 4P") && svg.contains("300 mA"));
    assert!(svg.contains("XVB 3G1.5") || svg.contains("XVB 3G2.5"));
}

#[test]
fn conduit_expands_to_faces_and_metrics_count_its_length() {
    let mut doc = house_doc();
    let r = el(&mut doc, "add_conduit", json!({ "level": 0, "points": [[1.0, 0.3, 1.0], [1.0, 2.5, 1.0], [4.0, 2.5, 1.0]], "diameter": 0.02 })).unwrap();
    assert!((r.measurement.unwrap() - 5.2).abs() < 1e-9);
    let id = r.created[0].clone();
    let e = match doc.find(&id).unwrap() { Entity::Custom(c) => c.clone(), _ => panic!() };
    let faces = ElectricalPlugin.expand(&e, &doc).unwrap();
    assert_eq!(faces.len(), 12, "6 faces per segment");
    assert!(faces.iter().all(|f| matches!(f, Entity::Face(x) if x.id == format!("{id}#run"))));
    let m = ElectricalPlugin.metrics(&doc).unwrap();
    assert!((m["conduitLength"].as_f64().unwrap() - 5.2).abs() < 1e-6);
    // compile through the registry keeps the id with its face key
    let scene = plannr_core::compile::compile(&doc, &registry(), &Default::default());
    assert!(scene.groups.iter().any(|g| g.ids.iter().any(|x| x == &format!("{id}#run"))));
}

#[test]
fn core_move_shifts_a_conduit_and_update_conduit_validates() {
    let mut doc = house_doc();
    let id = el(&mut doc, "add_conduit", json!({ "level": 0, "points": [[1.0, 0.3, 1.0], [2.0, 0.3, 1.0]] })).unwrap().created[0].clone();
    apply(&mut doc, &Op::Move { ids: vec![format!("{id}#run")], delta: [0.5, 0.1, 0.0] }, &registry()).unwrap();
    let k = plannr_plugin_electrical::ops::conduits(&doc).into_iter().find(|(i, _)| i == &id).unwrap().1;
    assert_eq!(k.points[0], [1.5, 0.4, 1.0]);
    assert!(el(&mut doc, "update_conduit", json!({ "id": id, "patch": { "diameter": 0.03 } })).is_err());
    assert!(el(&mut doc, "update_conduit", json!({ "id": id, "patch": { "kind": "pipe" } })).is_err());
    el(&mut doc, "update_conduit", json!({ "id": id, "patch": { "kind": "cable", "diameter": 0.016 } })).unwrap();
    let k = plannr_plugin_electrical::ops::conduits(&doc).into_iter().find(|(i, _)| i == &id).unwrap().1;
    assert_eq!(k.kind, "cable"); assert_eq!(k.diameter, 0.016);
}

#[test]
fn appliance_without_dedicated_circuit_is_flagged_and_pv_without_rcd_is_info() {
    let mut doc = house_doc();  // the house has a dishwasher point (an outlet noted "Dishwasher")
    el(&mut doc, "add_circuit", json!({ "name": "Licht", "type": "lighting" })).unwrap();
    el(&mut doc, "add_circuit", json!({ "name": "SC", "type": "sockets" })).unwrap();
    el(&mut doc, "add_circuit", json!({ "name": "PV", "type": "pv" })).unwrap();
    let f = findings(&mut doc); let t = titles(&f);
    assert!(t.contains(&"No dedicated dishwasher circuit".to_string()), "{t:?}");
    assert!(f.iter().any(|x| x["title"] == "PV circuit without differential" && x["severity"] == "info"));
    assert!(t.contains(&"Unassigned points".to_string()));
}

/// `PLANNR_DOC=<exported project.json> cargo test -p plannr-plugin-electrical -- --ignored fabric_report --nocapture`
#[test]
#[ignore]
fn fabric_report() {
    let path = std::env::var("PLANNR_DOC").expect("PLANNR_DOC");
    let mut doc: Document = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    doc.normalize(); registry().on_load(&mut doc);
    let ks = plannr_plugin_electrical::ops::conduits(&doc);
    let (mut inside, mut total) = (0usize, 0usize);
    let mut worst: Vec<(String, usize, usize)> = vec![];
    for (id, k) in &ks {
        if !k.auto { continue; }
        let (mut a, mut t) = (0, 0);
        for w in k.points.windows(2) {
            let n = ((plannr_core::geom::dist(w[0], w[1]) / 0.05).ceil() as usize).max(1);
            for i in 0..=n { let p = plannr_core::geom::lerp(w[0], w[1], i as f64 / n as f64); t += 1; if in_fabric(&doc, k.level, p) { a += 1; } }
        }
        inside += a; total += t;
        if a < t { worst.push((id.clone(), t - a, t)); }
    }
    worst.sort_by(|a, b| b.1.cmp(&a.1));
    println!("{inside} of {total} samples inside ({:.1} %)", 100.0 * inside as f64 / total as f64);
    for w in worst.iter().take(8) { let k = &ks.iter().find(|x| x.0 == w.0).unwrap().1; println!("  {} outside {}/{} to {:?}: {:?}", w.0, w.1, w.2, k.to, k.points); }
}
