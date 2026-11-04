use plannr_core::model::{Document, Entity};
use plannr_core::ops::OpResult;
use plannr_core::plugin::Registry;
use plannr_plugin_construction::ConstructionPlugin;
use plannr_plugin_plumbing::{data::*, net::{Attach, Net}, PlumbingPlugin};
use serde_json::{json, Value};

fn registry() -> Registry { let mut r = Registry::new(); r.register(Box::new(ConstructionPlugin)); r.register(Box::new(PlumbingPlugin)); r }
fn doc() -> Document { let mut d = Document::default(); registry().on_load(&mut d); d }
fn pl(doc: &mut Document, op: &str, data: Value) -> Result<OpResult, plannr_core::ops::OpError> { registry().apply(&format!("plumbing.{op}"), data, doc) }
fn data_of(r: &OpResult) -> Value { match &r.entities[0] { Entity::Custom(c) => c.data.clone(), _ => Value::Null } }
fn findings(doc: &mut Document) -> Vec<Value> { data_of(&pl(doc, "check", json!({})).unwrap())["findings"].as_array().unwrap().clone() }
fn titles(f: &[Value]) -> Vec<String> { f.iter().map(|x| x["title"].as_str().unwrap().to_string()).collect() }
fn faces(doc: &Document) -> usize { let reg = registry(); let x = reg.expand_all(doc); x.entities.iter().filter(|e| matches!(e, Entity::Face(_))).count() }

#[test]
fn pipe_defaults_follow_the_catalogue_and_corners_are_bent() {
    let mut d = doc();
    let r = pl(&mut d, "add_pipe", json!({ "system": "coldWater", "points": [[0, 0.5, 0], [2, 0.5, 0], [2, 0.5, 1.5]], "autoFittings": false })).unwrap();
    let p = pipe(&d, &r.created[0]).unwrap();
    assert_eq!(p.material, "alupex"); assert_eq!(p.dn, 16.0); assert_eq!(p.joint, "press"); assert_eq!(p.corners, "bend");
    assert!((r.measurement.unwrap() - 3.5).abs() < 1e-9);
    assert!((p.bend_r() - 0.08).abs() < 1e-9, "5 × 16 mm");
    assert!(faces(&d) > 20, "the pipe is a real tube");
    // a drain defaults to PVC 40 with glued joints and elbows
    let r = pl(&mut d, "add_pipe", json!({ "system": "drain", "points": [[0, 0.1, 0], [1, 0.08, 0], [1, 0.08, 1]], "autoFittings": false })).unwrap();
    let p = pipe(&d, &r.created[0]).unwrap();
    assert_eq!((p.material.as_str(), p.dn, p.joint.as_str(), p.corners.as_str()), ("pvc", 40.0, "glue", "elbow"));
    assert!(pl(&mut d, "add_pipe", json!({ "system": "drain", "material": "alupex", "points": [[0, 0, 0], [1, 0, 0]] })).is_ok(), "allowed but flagged by check");
    assert!(pl(&mut d, "add_pipe", json!({ "material": "copper", "joint": "glue", "points": [[0, 0, 0], [1, 0, 0]] })).is_err(), "copper is not glued");
}

#[test]
fn two_pipes_meeting_end_to_end_get_a_coupling_and_are_cut_back() {
    let mut d = doc();
    let a = pl(&mut d, "add_pipe", json!({ "points": [[0, 0.5, 0], [2, 0.5, 0]] })).unwrap().created[0].clone();
    let r = pl(&mut d, "add_pipe", json!({ "points": [[2, 0.5, 0], [4, 0.5, 0]] })).unwrap();
    assert_eq!(r.created.len(), 2, "pipe + coupling");
    let f = fitting_of(&d, &r.created[1]).unwrap();
    assert_eq!(f.kind, "coupling"); assert_eq!(f.joint, "press"); assert!((f.position[0] - 2.0).abs() < 1e-6);
    let pa = pipe(&d, &a).unwrap(); let pb = pipe(&d, &r.created[0]).unwrap();
    assert!(pa.end()[0] < 2.0 && pb.start()[0] > 2.0, "both pipes stop at the coupling's ports");
    assert_eq!(pa.to.as_deref(), Some(r.created[1].as_str())); assert_eq!(pb.from.as_deref(), Some(r.created[1].as_str()));
    let f = findings(&mut d); let t = titles(&f);
    assert!(!t.iter().any(|x| x == "Pipes meet without a coupling"), "{t:?}");
    assert!(!t.iter().any(|x| x == "Diameter mismatch at a fitting"), "{t:?}");
    // a different diameter gets a reducer
    let r = pl(&mut d, "add_pipe", json!({ "dn": 20, "points": [[4, 0.5, 0], [5, 0.5, 0]] })).unwrap();
    assert_eq!(fitting_of(&d, &r.created[1]).unwrap().kind, "reducer");
    // a 90° meeting gets an elbow
    let r = pl(&mut d, "add_pipe", json!({ "dn": 20, "points": [[5, 0.5, 0], [5, 0.5, 2]] })).unwrap();
    let e = fitting_of(&d, &r.created[1]).unwrap();
    assert_eq!(e.kind, "elbow90"); assert!((e.angle.unwrap() - 90.0).abs() < 1e-6);
}

#[test]
fn a_pipe_started_on_another_pipes_body_inserts_a_tee_and_splits_it() {
    let mut d = doc();
    let main = pl(&mut d, "add_pipe", json!({ "dn": 20, "points": [[0, 0.5, 0], [4, 0.5, 0]] })).unwrap().created[0].clone();
    let r = pl(&mut d, "add_pipe", json!({ "dn": 16, "points": [[2, 0.5, 0], [2, 0.5, 1.5]] })).unwrap();
    let tee_id = r.created.iter().find(|id| fitting_of(&d, id).is_some()).expect("a tee was created").clone();
    let tee = fitting_of(&d, &tee_id).unwrap();
    assert_eq!(tee.kind, "tee"); assert_eq!(tee.dn, 20.0); assert_eq!(tee.dn2, Some(16.0));
    assert!((tee.dir2.unwrap()[2] - 1.0).abs() < 1e-6, "branch towards the new pipe: {:?}", tee.dir2);
    let halves: Vec<Pipe> = pipes(&d).into_iter().map(|p| p.1).filter(|p| p.dn == 20.0).collect();
    assert_eq!(halves.len(), 2, "the main pipe was split");
    assert!(pipe(&d, &main).unwrap().end()[0] < 2.0);
    let branch = pipe(&d, &r.created[0]).unwrap();
    assert!(branch.start()[2] > 0.0, "the branch starts at the tee's branch port");
    let net = Net::read(&d);
    assert!(net.ends.iter().all(|e| !matches!(net.end_attachment(e), Attach::Open) || e.pos[0] < 0.01 || e.pos[0] > 3.99 || e.pos[2] > 1.4), "only the far ends are open");
    assert_eq!(titles(&findings(&mut d)).iter().filter(|t| *t == "Open pipe end").count(), 3);
}

#[test]
fn ending_on_a_tap_fixture_puts_a_wall_plate_there() {
    let mut d = doc();
    registry().apply("construction.add_room", json!({ "level": 0, "name": "Bad", "origin": [0, 0], "size": [3, 2.5] }), &mut d).ok();
    let fx = registry().apply("construction.add_fixture", json!({ "level": 0, "kind": "tapCold", "position": [1.0, 0.9, 0.1] }), &mut d).expect("fixture").created[0].clone();
    let r = pl(&mut d, "add_pipe", json!({ "points": [[1.0, 0.3, 0.1], [1.0, 0.9, 0.1]], "to": fx })).unwrap();
    let wp = r.created.iter().filter_map(|id| fitting_of(&d, id)).find(|f| f.kind == "wallPlate").expect("wall plate");
    assert_eq!(wp.fixture.as_deref(), Some(fx.as_str())); assert_eq!(wp.thread.as_deref(), Some("1/2\""));
    assert!(wp.dir2.unwrap()[1].abs() < 0.01, "outlet is horizontal (out of the wall): {:?}", wp.dir2);
    let p = pipe(&d, &r.created[0]).unwrap();
    assert!(p.end()[1] < 0.9, "pipe cut back to the wall plate's port");
    let t = titles(&findings(&mut d));
    assert_eq!(t.iter().filter(|x| *x == "Open pipe end").count(), 1, "{t:?}");
}

#[test]
fn inline_fitting_splits_the_pipe_and_a_tee_branches_towards_the_click() {
    let mut d = doc();
    let id = pl(&mut d, "add_pipe", json!({ "material": "copper", "dn": 15, "points": [[0, 1.0, 0], [3, 1.0, 0]] })).unwrap().created[0].clone();
    let r = pl(&mut d, "add_fitting", json!({ "kind": "ballValve", "onPipe": id, "position": [1.0, 1.0, 0] })).unwrap();
    let v = fitting_of(&d, &r.created[0]).unwrap();
    assert_eq!((v.kind.as_str(), v.joint.as_str(), v.material.as_str(), v.dn), ("ballValve", "solder", "copper", 15.0));
    assert_eq!(pipes(&d).len(), 2);
    let r = pl(&mut d, "add_fitting", json!({ "kind": "tee", "onPipe": r.created[1], "position": [2.0, 1.0, 0.1] })).unwrap();
    let t = fitting_of(&d, &r.created[0]).unwrap();
    assert_eq!(t.kind, "tee"); assert!((t.dir2.unwrap()[2] - 1.0).abs() < 1e-6);
    assert_eq!(pipes(&d).len(), 3);
    // elbows are not inserted mid-pipe
    assert!(pl(&mut d, "add_fitting", json!({ "kind": "elbow90", "onPipe": id, "position": [0.5, 1.0, 0] })).is_err());
    // a cap at the end
    let r = pl(&mut d, "add_fitting", json!({ "kind": "cap", "onPipe": id, "position": [0.0, 1.0, 0] })).unwrap();
    let c = fitting_of(&d, &r.created[0]).unwrap();
    assert!(c.position[0] < 0.0, "beyond the start: {:?}", c.position); assert!((c.dir[0] + 1.0).abs() < 1e-6);
    let net = Net::read(&d);
    let start = net.ends.iter().find(|e| e.pipe == id && e.end == End::Start).unwrap();
    assert!(matches!(net.end_attachment(start), Attach::Port(_)));
}

#[test]
fn mixer_and_manifold_have_the_expected_ports() {
    let mut d = doc();
    let r = pl(&mut d, "add_fitting", json!({ "kind": "mixerWall", "position": [1.0, 1.1, 0.0], "dir": [1, 0, 0], "dir2": [0, 0, 1] })).unwrap();
    let f = fitting_of(&d, &r.created[0]).unwrap();
    let ps = plannr_plugin_plumbing::fittings::ports(&f);
    assert_eq!(ps.len(), 2); assert_eq!(ps[0].name, "hot"); assert!((ps[0].pos[0] - 0.925).abs() < 1e-6 && (ps[1].pos[0] - 1.075).abs() < 1e-6, "150 mm centres");
    assert!((ps[0].dir[2] + 1.0).abs() < 1e-6, "inlets go into the wall");
    let r = pl(&mut d, "add_fitting", json!({ "kind": "manifold", "position": [0, 1.0, 0], "dir": [1, 0, 0], "dir2": [0, -1, 0], "outlets": 6, "dn": 26 })).unwrap();
    let f = fitting_of(&d, &r.created[0]).unwrap();
    let ps = plannr_plugin_plumbing::fittings::ports(&f);
    assert_eq!(ps.len(), 7); assert_eq!(ps[1].dn, 16.0); assert!(ps[1].pos[1] < 1.0);
    // pipes connect to the ports by geometry
    let o = ps[1].pos;
    let r = pl(&mut d, "add_pipe", json!({ "points": [[o[0], o[1], o[2]], [o[0], o[1] - 1.0, o[2]]] })).unwrap();
    assert_eq!(pipe(&d, &r.created[0]).unwrap().from.as_deref(), Some(f_id(&d, "manifold").as_str()));
    assert!(faces(&d) > 100);
}
fn f_id(d: &Document, kind: &str) -> String { fittings(d).into_iter().find(|f| f.1.kind == kind).unwrap().0 }

#[test]
fn segment_length_and_connect() {
    let mut d = doc();
    let id = pl(&mut d, "add_pipe", json!({ "points": [[0, 0.5, 0], [1, 0.5, 0], [1, 0.5, 1]] })).unwrap().created[0].clone();
    let r = pl(&mut d, "set_segment_length", json!({ "id": id, "segment": 0, "length": 2.5 })).unwrap();
    let p = pipe(&d, &id).unwrap();
    assert_eq!(p.points[1], [2.5, 0.5, 0.0]); assert_eq!(p.points[2], [2.5, 0.5, 1.0]); assert!((r.measurement.unwrap() - 3.5).abs() < 1e-9);
    let other = pl(&mut d, "add_pipe", json!({ "points": [[2.5, 0.5, 1.3], [2.5, 0.5, 3.0]] })).unwrap().created[0].clone();
    let r = pl(&mut d, "connect", json!({ "pipe": id, "end": "end", "to": other })).unwrap();
    assert_eq!(r.created.len(), 1, "a coupling");
    assert!(titles(&findings(&mut d)).iter().filter(|t| *t == "Open pipe end").count() == 2);
}

#[test]
fn brackets_follow_the_spacing_rule_and_find_the_wall() {
    let mut d = doc();
    registry().apply("construction.add_room", json!({ "level": 0, "name": "Keuken", "origin": [0, 0], "size": [4, 3] }), &mut d).ok();
    // along the wall z = 0, 10 cm from it
    let id = pl(&mut d, "add_pipe", json!({ "points": [[0.2, 0.4, 0.1], [3.8, 0.4, 0.1]], "autoBrackets": true })).unwrap().created[0].clone();
    let bs = brackets(&d);
    assert_eq!(bs.len(), 8, "3.6 m / 0.5 m → 8 clips: {}", bs.len());
    assert!(bs.iter().all(|(_, b)| b.auto && b.pipe.as_deref() == Some(id.as_str())));
    let b = &bs[0].1;
    assert!(b.normal[2] < -0.9, "towards the wall at z = 0: {:?}", b.normal);
    assert!((b.standoff - 0.1).abs() < 0.02, "plate on the wall: {}", b.standoff);
    assert!(!titles(&findings(&mut d)).iter().any(|t| t == "Bracket spacing too large" || t == "No brackets"));
    // re-running replaces them
    let r = pl(&mut d, "auto_brackets", json!({ "pipes": [id], "spacing": 1.0 })).unwrap();
    assert_eq!(r.removed.len(), 8); assert_eq!(r.created.len(), 4);
    // a vertical pipe near the ceiling / wall
    let r = pl(&mut d, "add_bracket", json!({ "pipe": id, "position": [1.0, 0.4, 0.1] })).unwrap();
    let b = brackets(&d).into_iter().find(|b| b.0 == r.created[0]).unwrap().1;
    assert!(!b.auto); assert!((b.position[0] - 1.0).abs() < 1e-6);
    let t = data_of(&pl(&mut d, "takeoff", json!({})).unwrap());
    assert_eq!(t["items"].as_array().unwrap().iter().filter(|i| i["group"] == "bracket").map(|i| i["count"].as_u64().unwrap()).sum::<u64>(), 5); assert_eq!(t["totals"]["brackets"], 5);
}

#[test]
fn checks_flag_tight_bends_flat_drains_and_open_ends() {
    let mut d = doc();
    // a 16 mm alupex bend needs R 80 mm: legs of 5 cm cannot give it
    pl(&mut d, "add_pipe", json!({ "points": [[0, 0.5, 0], [0.05, 0.5, 0], [0.05, 0.5, 0.05]], "autoFittings": false })).unwrap();
    pl(&mut d, "add_pipe", json!({ "system": "drain", "points": [[0, 0.1, 2], [2, 0.1, 2]], "autoFittings": false })).unwrap();
    pl(&mut d, "add_pipe", json!({ "system": "hotWater", "points": [[0, 2.0, 2], [3, 2.0, 2]], "autoFittings": false })).unwrap();
    let f = findings(&mut d); let t = titles(&f);
    assert!(t.contains(&"Bend radius too small".to_string()), "{t:?}");
    assert!(t.contains(&"Drain without fall".to_string()), "{t:?}");
    assert!(t.contains(&"Hot water pipe without insulation".to_string()), "{t:?}");
    assert!(t.contains(&"Open pipe end".to_string()));
    assert!(f.iter().all(|x| x["ids"].as_array().map(|a| !a.is_empty()).unwrap_or(false)));
}

#[test]
fn takeoff_lists_pipe_lengths_elbows_fittings_and_insulation() {
    let mut d = doc();
    pl(&mut d, "add_pipe", json!({ "material": "copper", "dn": 22, "points": [[0, 0.5, 0], [2, 0.5, 0], [2, 0.5, 2], [2, 1.5, 2]], "insulation": 13, "autoFittings": false })).unwrap();
    pl(&mut d, "add_pipe", json!({ "system": "hotWater", "points": [[0, 0.6, 0], [2, 0.6, 0], [2, 0.6, 2]], "autoFittings": false })).unwrap();
    pl(&mut d, "add_fitting", json!({ "kind": "ballValve", "material": "copper", "joint": "solder", "dn": 22, "position": [3, 0.5, 0] })).unwrap();
    let t = data_of(&pl(&mut d, "takeoff", json!({})).unwrap());
    let items = t["items"].as_array().unwrap();
    let copper = items.iter().find(|i| i["group"] == "pipe" && i["material"] == "copper").unwrap();
    assert!((copper["length_m"].as_f64().unwrap() - 5.0).abs() < 1e-6); assert_eq!(copper["bends"], 0);
    let elbows = items.iter().find(|i| i["kind"] == "elbow90" && i["material"] == "copper").unwrap();
    assert_eq!(elbows["count"], 2); assert_eq!(elbows["joint"], "solder");
    let alupex = items.iter().find(|i| i["group"] == "pipe" && i["material"] == "alupex").unwrap();
    assert_eq!(alupex["bends"], 1);
    assert!(items.iter().any(|i| i["group"] == "insulation" && (i["length_m"].as_f64().unwrap() - 5.0).abs() < 1e-6));
    assert!(items.iter().any(|i| i["kind"] == "ballValve" && i["count"] == 1));
    assert_eq!(t["totals"]["fittings"], 3);
    let c = data_of(&pl(&mut d, "catalog", json!({})).unwrap());
    assert!(c["materials"].as_array().unwrap().len() == 6 && c["fittings"].as_array().unwrap().len() >= 20);
}

#[test]
fn move_keeps_the_pipe_connected_check_and_paint_overrides_the_colour() {
    let mut d = doc();
    let r = pl(&mut d, "add_pipe", json!({ "points": [[0, 0.5, 0], [1, 0.5, 0]] })).unwrap();
    let id = r.created[0].clone();
    registry().apply("plumbing.add_fitting", json!({ "kind": "cap", "onPipe": id, "position": [1, 0.5, 0] }), &mut d).unwrap();
    let reg = registry();
    plannr_core::ops::apply(&mut d, &serde_json::from_value(json!({ "op": "move", "ids": [id], "delta": [0, 0, 1] })).unwrap(), &reg).unwrap();
    assert_eq!(pipe(&d, &id).unwrap().points[0], [0.0, 0.5, 1.0]);
    assert!(titles(&findings(&mut d)).iter().filter(|t| *t == "Open pipe end").count() == 2, "the cap stayed behind");
    plannr_core::ops::apply(&mut d, &serde_json::from_value(json!({ "op": "paint", "ids": [format!("{id}#pipe")], "material": "plumbing.gas" })).unwrap(), &reg).unwrap();
    assert_eq!(pipe(&d, &id).unwrap().paint.as_deref(), Some("plumbing.gas"));
}

#[test]
fn every_fitting_is_compact_around_its_position() {
    let reg = registry();
    for kind in ["coupling", "elbow90", "elbow45", "tee", "reducer", "adapter", "cap", "wallPlate", "stopValve", "ballValve", "checkValve", "pressureReducer", "filter", "meter", "angleValve", "radiatorValve", "mixerWall", "mixerThermo", "mixerBasin", "manifold", "trap", "cleanout", "branch45"] {
        let mut d = doc();
        let sys = if ["trap", "cleanout", "branch45"].contains(&kind) { "drain" } else { "coldWater" };
        let r = pl(&mut d, "add_fitting", json!({ "kind": kind, "system": sys, "position": [1.0, 1.0, 1.0], "dir": [0, 1, 0], "dir2": [0, 0, 1] })).unwrap();
        let id = r.created[0].clone();
        let x = reg.expand_all(&d);
        let mut worst: (f64, String) = (0.0, String::new()); let mut n = 0;
        for e in &x.entities { if let Entity::Face(f) = e { if f.id.starts_with(&id) { n += 1; for p in &f.outer { let dd = ((p[0] - 1.0).powi(2) + (p[1] - 1.0).powi(2) + (p[2] - 1.0).powi(2)).sqrt(); if dd > worst.0 { worst = (dd, f.id.clone()); } } } } }
        assert!(n > 10, "{kind}: only {n} faces");
        let limit = if ["mixerWall", "mixerThermo", "mixerBasin", "manifold"].contains(&kind) { 0.34 } else { 0.2 };
        assert!(worst.0 < limit, "{kind}: a vertex of {} lies {:.2} m from the fitting", worst.1, worst.0);
    }
}


#[test]
fn bent_and_mitred_pipes_stay_within_their_radius() {
    let reg = registry();
    for corners in ["bend", "elbow"] {
        let mut d = doc();
        let id = pl(&mut d, "add_pipe", json!({ "material": "alupex", "dn": 20, "corners": corners, "points": [[0, 0.5, 0], [1, 0.5, 0], [1, 0.5, 1], [1, 1.5, 1], [1.3, 1.5, 1.2]], "autoFittings": false })).unwrap().created[0].clone();
        let x = reg.expand_all(&d);
        let path = [[0.0, 0.5, 0.0], [1.0, 0.5, 0.0], [1.0, 0.5, 1.0], [1.0, 1.5, 1.0], [1.3, 1.5, 1.2]];
        let dist_to_path = |p: [f64; 3]| path.windows(2).map(|w| { let a = w[0]; let b = w[1]; let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]]; let l2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2]; let t = (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1] + (p[2] - a[2]) * ab[2]) / l2).clamp(0.0, 1.0); let q = [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t]; ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt() }).fold(f64::MAX, f64::min);
        let mut worst = 0.0f64; let mut n = 0;
        for e in &x.entities { if let Entity::Face(f) = e { if f.id.starts_with(&id) { n += 1; for p in &f.outer { worst = worst.max(dist_to_path(*p)); } } } }
        assert!(n > 40, "{corners}: {n} faces");
        // a bent pipe cuts the corner by at most the bend radius; an elbow body is 1.5 × dn wide
        assert!(worst < 0.07, "{corners}: a vertex lies {:.3} m from the pipe's centreline", worst);
    }
}

#[test]
fn import_runs_replaces_earlier_imports_and_keeps_hand_drawn_pipes() {
    let mut d = doc();
    let reg = registry();
    let house = json!({ "floors": [{ "id": "F0", "name": "Gelijkvloers", "height": 2.6 }], "rooms": [{ "id": "R1", "name": "Bad", "level": 0, "origin": [0, 0], "size": [3, 2.5] }], "runs": [{ "id": "RUN1", "level": 0, "system": "waterCold", "points": [[0.2, 0.3, 0.1], [2.0, 0.3, 0.1]] }, { "id": "RUN2", "level": 0, "system": "drain", "points": [[0.5, 0.1, 0.5], [2.5, 0.08, 0.5]], "diameter": 0.05 }] });
    reg.apply("construction.import_house", json!({ "house": house }), &mut d).expect("import house");
    let hand = pl(&mut d, "add_pipe", json!({ "points": [[0, 1, 0], [1, 1, 0]], "autoFittings": false })).unwrap().created[0].clone();
    let r = pl(&mut d, "import_runs", json!({})).unwrap();
    assert_eq!(r.created.len(), 2); assert_eq!(r.removed.len(), 2, "the runs are removed");
    let ps = pipes(&d);
    assert_eq!(ps.len(), 3);
    let drain = ps.iter().find(|p| p.1.system == "drain").unwrap();
    assert_eq!((drain.1.material.as_str(), drain.1.dn), ("pvc", 50.0));
    assert_eq!(drain.1.extra.get("source").and_then(|v| v.as_str()), Some("RUN2"));
    // the house comes again (an app re-imports after every change): the imported pipes are replaced, not doubled
    reg.apply("construction.import_house", json!({ "house": house }), &mut d).expect("import house again");
    let r = pl(&mut d, "import_runs", json!({})).unwrap();
    assert_eq!(r.removed.len(), 4, "2 old pipes + 2 runs");
    let ps = pipes(&d);
    assert_eq!(ps.len(), 3);
    assert!(ps.iter().any(|p| p.0 == hand), "the hand-drawn pipe stays");
}

fn room_doc() -> Document {
    let mut d = doc();
    registry().apply("construction.add_room", json!({ "level": 0, "name": "A", "origin": [0, 0], "size": [3, 3] }), &mut d).unwrap();
    registry().apply("construction.add_room", json!({ "level": 0, "name": "B", "origin": [3, 0], "size": [3, 3] }), &mut d).unwrap();
    d
}
fn orthogonal(p: &Pipe) -> bool { p.points.windows(2).all(|w| { let n = (0..3).filter(|i| (w[0][*i] - w[1][*i]).abs() > 1e-3).count(); n == 1 }) }

#[test]
fn route_works_out_orthogonal_corners_and_reports_the_holes() {
    let mut d = room_doc();
    let r = pl(&mut d, "route", json!({ "from": [0.5, 0.5, 0.1], "to": [5.0, 1.0, 2.0], "via": "ceiling", "autoFittings": false })).unwrap();
    let p = pipe(&d, &r.created[0]).unwrap();
    assert!(orthogonal(&p), "{:?}", p.points);
    assert_eq!(p.points[0], [0.5, 0.5, 0.1]); assert_eq!(*p.points.last().unwrap(), [5.0, 1.0, 2.0]);
    assert!((p.points[1][1] - 2.5).abs() < 1e-6, "10 cm under a 2.6 m ceiling: {:?}", p.points);
    let info = match r.entities.last().unwrap() { Entity::Custom(c) => c.data.clone(), _ => Value::Null };
    assert_eq!(info["wallCrossings"].as_array().unwrap().len(), 1, "{info}");
    assert!(info["variants"].as_u64().unwrap() >= 2);
    // the other variant takes the other corner
    let r2 = pl(&mut d, "route", json!({ "from": [0.5, 0.5, 0.1], "to": [5.0, 1.0, 2.0], "via": "ceiling", "variant": 1, "autoFittings": false })).unwrap();
    assert_ne!(pipe(&d, &r2.created[0]).unwrap().points, p.points);
    assert!(pl(&mut d, "route", json!({ "from": [0, 0, 0], "to": [0, 0, 0] })).is_err());
}

#[test]
fn route_prefers_the_wall_and_connects_by_id() {
    let mut d = room_doc();
    // z = 0.1 runs along the wall z = 0 and the way along it is taken first
    let r = pl(&mut d, "route", json!({ "from": [0.5, 0.5, 0.1], "to": [2.5, 0.5, 2.8], "autoFittings": false })).unwrap();
    let p = pipe(&d, &r.created[0]).unwrap();
    assert_eq!(p.points[1], [2.5, 0.5, 0.1], "{:?}", p.points);
    // from a manifold port: it leaves along the port, and the pipe is linked
    let m = pl(&mut d, "add_fitting", json!({ "kind": "manifold", "position": [1, 1, 1], "dir": [1, 0, 0], "dir2": [0, -1, 0], "outlets": 2, "dn": 26 })).unwrap().created[0].clone();
    let r = pl(&mut d, "route", json!({ "from": m, "to": [2.5, 0.2, 2.0] })).unwrap();
    let p = pipe(&d, &r.created[0]).unwrap();
    assert_eq!(p.from.as_deref(), Some(m.as_str())); assert!(orthogonal(&p), "{:?}", p.points);
    assert!(p.points[1][1] < p.points[0][1], "first down along the port: {:?}", p.points);
    // onto a pipe's body: a tee goes in
    let main = pl(&mut d, "add_pipe", json!({ "dn": 20, "points": [[0.5, 1.5, 2.5], [2.5, 1.5, 2.5]] })).unwrap().created[0].clone();
    let r = pl(&mut d, "route", json!({ "from": [1.5, 1.5, 2.5], "fromId": main, "to": [1.5, 1.5, 0.5] })).unwrap();
    assert!(r.created.iter().any(|id| fitting_of(&d, id).map(|f| f.kind == "tee").unwrap_or(false)), "a tee was inserted");
}

#[test]
fn parallel_keeps_the_spacing_round_corners_and_up_risers() {
    let mut d = room_doc();
    let id = pl(&mut d, "add_pipe", json!({ "points": [[0.5, 0.5, 0.5], [2, 0.5, 0.5], [2, 1.5, 0.5], [2, 1.5, 2]], "autoFittings": false })).unwrap().created[0].clone();
    let r = pl(&mut d, "parallel", json!({ "id": id, "system": "hotWater", "distance": 0.06, "autoFittings": false })).unwrap();
    let q = pipe(&d, &r.created[0]).unwrap();
    assert_eq!(q.system, "hotWater");
    assert_eq!(q.points.len(), 4); assert!(orthogonal(&q), "{:?}", q.points);
    assert!((q.points[0][2] - 0.44).abs() < 1e-6 || (q.points[0][2] - 0.56).abs() < 1e-6, "{:?}", q.points);
    let (dx, dz) = (q.points[3][0] - 2.0, q.points[3][2] - 2.0);
    assert!((dx.abs() - 0.06).abs() < 1e-6 && dz.abs() < 1e-6, "same spacing on the last leg: {:?}", q.points);
    // together with a route
    let r = pl(&mut d, "route", json!({ "from": [0.5, 0.3, 0.1], "to": [2.5, 0.3, 2.0], "pair": { "system": "hotWater", "distance": 0.06 }, "autoFittings": false })).unwrap();
    let info = match r.entities.last().unwrap() { Entity::Custom(c) => c.data.clone(), _ => Value::Null };
    assert_eq!(pipe(&d, info["pair"].as_str().unwrap()).unwrap().system, "hotWater");
}

#[test]
fn parallel_stays_away_from_the_wall_unless_told_otherwise() {
    let mut d = room_doc();
    let id = pl(&mut d, "add_pipe", json!({ "points": [[0.5, 0.4, 0.17], [2.0, 0.4, 0.17]], "autoFittings": false })).unwrap().created[0].clone();
    let auto = pl(&mut d, "parallel", json!({ "id": id, "distance": 0.06, "autoFittings": false })).unwrap().created[0].clone();
    assert!((pipe(&d, &auto).unwrap().points[0][2] - 0.23).abs() < 1e-6, "into the room, not into the wall at z = 0");
    let forced = pl(&mut d, "parallel", json!({ "id": id, "distance": 0.06, "side": "right", "autoFittings": false })).unwrap().created[0].clone();
    let left = pl(&mut d, "parallel", json!({ "id": id, "distance": 0.06, "side": "left", "autoFittings": false })).unwrap().created[0].clone();
    let (r, l) = (pipe(&d, &forced).unwrap().points[0][2], pipe(&d, &left).unwrap().points[0][2]);
    assert!((r + l - 0.34).abs() < 1e-6 && (r - l).abs() > 0.1, "opposite sides: {r} {l}");
    assert!(pl(&mut d, "parallel", json!({ "id": id, "side": "up" })).is_err());
}
