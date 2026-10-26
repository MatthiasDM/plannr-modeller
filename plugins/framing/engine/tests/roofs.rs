//! Roof shapes, truss types, finishing and dormers of the framing plugin.

use plannr_core::compile::{compile, CompileOptions};
use plannr_core::model::{Document, Entity};
use plannr_core::ops::OpResult;
use plannr_core::plugin::Registry;
use plannr_plugin_construction::ConstructionPlugin;
use plannr_plugin_framing::{data::*, expand::*, roof::truss_count, FramingPlugin};
use serde_json::{json, Value};

fn registry() -> Registry { let mut r = Registry::new(); r.register(Box::new(ConstructionPlugin)); r.register(Box::new(FramingPlugin)); r }
fn doc() -> Document { let mut d = Document::default(); registry().on_load(&mut d); d }
fn fr(doc: &mut Document, op: &str, data: Value) -> Result<OpResult, plannr_core::ops::OpError> { registry().apply(&format!("framing.{op}"), data, doc) }
fn data_of(r: &OpResult) -> Value { match &r.entities[0] { Entity::Custom(c) => c.data.clone(), _ => Value::Null } }
fn custom(doc: &Document, id: &str) -> plannr_core::model::Custom { doc.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.id == id => Some(c.clone()), _ => None }).unwrap() }
fn expansion(doc: &Document, id: &str) -> Expansion { expand_any(&custom(doc, id), &House::read(doc), "l") }
fn count(ex: &Expansion, prefix: &str) -> usize { ex.members.iter().filter(|m| m.key.starts_with(prefix)).count() }
fn cat(ex: &Expansion, c: &str) -> usize { ex.members.iter().filter(|m| m.category == c).count() }
fn roof(d: &mut Document, v: Value) -> String { fr(d, "add_roof", v).unwrap().created[0].clone() }
fn get<'a>(ex: &'a Expansion, key: &str) -> &'a Member { ex.members.iter().find(|m| m.key == key).unwrap_or_else(|| panic!("no member {key}")) }
/// Rise over horizontal run of a member's length axis.
fn grade(m: &Member) -> f64 { m.ex[1] / (m.ex[0] * m.ex[0] + m.ex[2] * m.ex[2]).sqrt() }

#[test]
fn hip_with_rafters_has_four_hips_jacks_and_a_shortened_ridge() {
    let mut d = doc();
    // ridge along x: run 10, span 6 → ridge 4 m
    let id = roof(&mut d, json!({ "min": [0, 0], "max": [10, 6], "shape": "hip", "type": "rafters", "spacing": 0.6 }));
    let ex = expansion(&d, &id);
    assert_eq!(cat(&ex, "hips"), 4);
    assert!(cat(&ex, "jacks") > 10, "{}", cat(&ex, "jacks"));
    let ridge = get(&ex, "ridge");
    assert!((ridge.length - 4.0).abs() < 1e-9, "ridge {}", ridge.length);
    assert_eq!(count(&ex, "kingCommon."), 2);
    // hips are one size deeper than the 2x8 rafters
    assert_eq!(get(&ex, "hip.0").size_label, "2x10");
    // four sloped sheathing planes when sheathing is on
    let id2 = roof(&mut d, json!({ "min": [0, 0], "max": [10, 6], "shape": "hip", "type": "rafters", "sheathing": { "enabled": true } }));
    let ex2 = expansion(&d, &id2);
    assert_eq!(ex2.panels.iter().filter(|p| p.category == "sheathing").count(), 4);
}

#[test]
fn square_hip_is_a_pyramid_without_a_ridge() {
    let mut d = doc();
    let id = roof(&mut d, json!({ "min": [0, 0], "max": [6, 6], "shape": "hip", "type": "rafters" }));
    let ex = expansion(&d, &id);
    assert_eq!(count(&ex, "ridge"), 0);
    assert_eq!(cat(&ex, "hips"), 4);
    // the hips meet at the apex
    let ends: Vec<[f64; 3]> = (0..4).map(|n| { let h = get(&ex, &format!("hip.{n}")); plannr_core::geom::add(h.origin, plannr_core::geom::scale(h.ex, h.length)) }).collect();
    for e in &ends { assert!((e[0] - 3.0).abs() < 1e-6 && (e[2] - 3.0).abs() < 1e-6, "{e:?}"); }
}

#[test]
fn hip_with_trusses_has_doubled_girders_and_jack_trusses() {
    let mut d = doc();
    let id = roof(&mut d, json!({ "min": [0, 0], "max": [12, 6], "shape": "hip", "type": "fink", "spacing": 0.6 }));
    let ex = expansion(&d, &id);
    let girders: std::collections::BTreeSet<String> = ex.members.iter().filter(|m| m.key.starts_with("girder.")).map(|m| m.key.split('.').take(2).collect::<Vec<_>>().join(".")).collect();
    assert_eq!(girders.len(), 4, "two doubled girders: {girders:?}");
    assert!(count(&ex, "jackTruss.") > 0);
    assert!(count(&ex, "truss.") > 0, "commons between the girders");
    assert_eq!(cat(&ex, "hips"), 4);
    assert!(truss_count(&ex) > 10);
}

#[test]
fn shed_mono_trusses_rise_to_the_chosen_side() {
    let mut d = doc();
    // ridge along x: span across z = 4 m
    let hi = roof(&mut d, json!({ "min": [0, 0], "max": [8, 4], "shape": "shed", "type": "mono", "pitch": 3 }));
    let ex = expansion(&d, &hi);
    let end = get(&ex, "truss.0.end");
    assert!(end.origin[2] > 3.9, "vertical at the high (max) side: {:?}", end.origin);
    let top = get(&ex, "truss.0.top");
    assert!(top.ex[2] > 0.0 && top.ex[1] > 0.0, "top chord rises toward +z");
    let lo = roof(&mut d, json!({ "min": [0, 0], "max": [8, 4], "shape": "shed", "type": "fink", "highSide": "min" }));
    let ex = expansion(&d, &lo);
    assert!(get(&ex, "truss.0.end").origin[2] < 0.1, "vertical at the min side");
    // rafters: a pony wall on the high wall by default, a ledger for a lean-to
    let r = roof(&mut d, json!({ "min": [0, 0], "max": [8, 4], "shape": "shed", "type": "rafters" }));
    let ex = expansion(&d, &r);
    assert_eq!(count(&ex, "ledger"), 0);
    assert!(count(&ex, "pony.stud.") > 5 && count(&ex, "pony.plate.") == 2);
    let l = roof(&mut d, json!({ "min": [0, 0], "max": [8, 4], "shape": "shed", "type": "rafters", "highSupport": "ledger" }));
    assert_eq!(count(&expansion(&d, &l), "ledger"), 1);
    // mono only on sheds
    assert!(fr(&mut d, "add_roof", json!({ "min": [0, 0], "max": [8, 4], "type": "mono" })).is_err());
}

#[test]
fn gambrel_has_two_pitches_purlins_and_collars() {
    let mut d = doc();
    let id = roof(&mut d, json!({ "min": [0, 0], "max": [8, 6], "shape": "gambrel", "type": "rafters", "pitch": 6, "lowerPitch": 18 }));
    let ex = expansion(&d, &id);
    assert!((grade(get(&ex, "rafter.0.l.lower")) - 1.5).abs() < 1e-6);
    assert!((grade(get(&ex, "rafter.0.l.upper")) - 0.5).abs() < 1e-6);
    assert_eq!(cat(&ex, "purlins"), 2);
    assert!(count(&ex, "collar.") > 0);
}

#[test]
fn scissor_and_attic_trusses() {
    let mut d = doc();
    let sc = roof(&mut d, json!({ "min": [0, 0], "max": [8, 8], "type": "scissor", "pitch": 8 }));
    let ex = expansion(&d, &sc);
    let b = get(&ex, "truss.0.bottom.l");
    assert!((grade(b) - 4.0 / 12.0).abs() < 1e-6, "bottom chord at half the pitch: {}", grade(b));
    assert_eq!(truss_count(&ex), 15, "8 m at 0.6 → 15");
    let at = roof(&mut d, json!({ "min": [0, 0], "max": [8, 9], "type": "attic", "pitch": 12 }));
    let ex = expansion(&d, &at);
    assert!(count(&ex, "truss.0.knee.") == 2 && count(&ex, "truss.0.collar") == 1);
    assert_eq!(get(&ex, "truss.0.bottom").size_label, "2x10", "floor-joist bottom chord");
}

#[test]
fn fascia_soffit_cladding_and_gutters_reach_the_estimate() {
    let mut d = doc();
    let id = roof(&mut d, json!({ "min": [0, 0], "max": [8, 6], "type": "fink", "sheathing": { "enabled": true }, "soffit": { "enabled": true }, "cladding": { "kind": "shingles" }, "gutters": true }));
    let ex = expansion(&d, &id);
    assert_eq!(cat(&ex, "fascia"), 6, "2 eaves + 4 rakes");
    assert!(ex.faces.iter().any(|f| f.id().contains("#cladding.l")));
    let est = data_of(&fr(&mut d, "estimate", json!({})).unwrap());
    let cats: Vec<&str> = est["items"].as_array().unwrap().iter().map(|i| i["category"].as_str().unwrap()).collect();
    for c in ["fascia", "soffit", "cladding", "gutters", "sheathing", "trusses"] { assert!(cats.contains(&c), "{c} missing in {cats:?}"); }
    let t = &est["totals"];
    assert!((t["gutters_m"].as_f64().unwrap() - 16.0).abs() < 1e-6, "{t}");
    assert!((t["cladding_m2"].as_f64().unwrap() - t["sheathing_m2"].as_f64().unwrap()).abs() < 1e-6);
    assert!(t["soffit_m2"].as_f64().unwrap() > 7.0);
    assert_eq!(t["lumber_pieces"].as_u64().unwrap() as usize, ex.members.len(), "gutters and cladding are not lumber");
    // the faces compile
    let c = compile(&d, &registry(), &CompileOptions::default());
    assert!(c.groups.iter().any(|g| g.ids.iter().any(|i| i.contains("#gutter."))));
}

#[test]
fn gable_dormer_cuts_the_rafters_and_adds_trimmers_walls_and_its_roof() {
    let mut d = doc();
    let plain = roof(&mut d, json!({ "min": [0, 0], "max": [8, 6], "type": "rafters", "spacing": 0.4, "pitch": 10 }));
    let id = roof(&mut d, json!({ "min": [0, 0], "max": [8, 6], "type": "rafters", "spacing": 0.4, "pitch": 10, "sheathing": { "enabled": true },
                                 "dormers": [{ "side": "l", "offset": 3.0, "width": 1.6, "setback": 0.6, "wallHeight": 1.1, "kind": "gable" }] }));
    let (p, ex) = (expansion(&d, &plain), expansion(&d, &id));
    // rafters at 3.2 … 4.4 on the left slope are cut into a tail and an upper piece
    let full_len = get(&p, "rafter.8.l").length;
    assert!(get(&ex, "rafter.8.l").length < full_len - 0.5);
    assert!(count(&ex, "rafter.8.l.upper") == 1 && count(&ex, "rafter.8.r.upper") == 0);
    assert_eq!(cat(&ex, "trimmers"), 4);
    assert_eq!(count(&ex, "dormer.0.header.top."), 2);
    assert_eq!(count(&ex, "dormer.0.header.bottom."), 2);
    assert!(count(&ex, "dormer.0.rafter.") >= 4, "{}", count(&ex, "dormer.0.rafter."));
    assert!(count(&ex, "dormer.0.cheek.l.stud.") > 0 && count(&ex, "dormer.0.front.stud.") > 0);
    assert!(count(&ex, "dormer.0.front.header") == 1, "a framed rough opening in the front wall");
    assert_eq!(count(&ex, "dormer.0.ridge"), 1);
    // the main sheathing on that slope has the opening cut out
    let main_l = ex.panels.iter().find(|p| p.key == "sheathing.l").unwrap().area;
    let main_r = ex.panels.iter().find(|p| p.key == "sheathing.r").unwrap().area;
    assert!(main_l < main_r - 0.5, "{main_l} vs {main_r}");
    // shed dormers and validation
    let sh = roof(&mut d, json!({ "min": [0, 0], "max": [8, 6], "type": "fink", "dormers": [{ "side": "r", "offset": 1.0, "width": 2.4, "kind": "shed", "pitch": 2, "wallHeight": 0.7 }] }));
    let ex = expansion(&d, &sh);
    assert!(count(&ex, "truss.") > 0 && count(&ex, "dormer.0.rafter.") >= 6 && cat(&ex, "trimmers") == 4);
    assert!(ex.members.iter().any(|m| m.key.ends_with(".top.r.upper")), "trusses in the opening are cut too");
    assert!(fr(&mut d, "add_roof", json!({ "min": [0, 0], "max": [8, 6], "shape": "hip", "dormers": [{ "offset": 1.0, "width": 1.0 }] })).is_err());
    assert!(fr(&mut d, "add_roof", json!({ "min": [0, 0], "max": [8, 6], "dormers": [{ "offset": 7.5, "width": 1.0 }] })).is_err());
}

#[test]
fn update_validates_roof_fields_and_summary_reports_shape() {
    let mut d = doc();
    let id = roof(&mut d, json!({ "min": [0, 0], "max": [8, 6] }));
    assert!(fr(&mut d, "update", json!({ "id": id, "patch": { "shape": "dome" } })).is_err());
    fr(&mut d, "update", json!({ "id": id, "patch": { "shape": "hip" } })).unwrap();
    let s = data_of(&fr(&mut d, "summary", json!({})).unwrap());
    assert_eq!(s["roofs"][0]["shape"], "hip");
    assert!(s["roofs"][0]["hips"].as_u64().unwrap() == 4);
}
