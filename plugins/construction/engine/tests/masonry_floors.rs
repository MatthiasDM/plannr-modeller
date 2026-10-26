//! Masonry floors (potten en balken, welfsels, in-situ) and strip foundations.

use std::collections::BTreeMap;

use plannr_core::model::{Document, Entity};
use plannr_core::plugin::Registry;
use plannr_plugin_construction::ConstructionPlugin;
use plannr_plugin_construction::masonry::{data::*, expand::*};
use serde_json::{json, Value};

fn registry() -> Registry { let mut r = Registry::new(); r.register(Box::new(ConstructionPlugin)); r }
fn doc() -> Document {
    let mut d = Document::default();
    let reg = registry(); reg.on_load(&mut d);
    reg.apply("construction.setup", json!({ "storeys": [{ "height": 2.6 }, { "height": 2.6 }], "groundLevel": 0.0 }), &mut d).unwrap();
    d
}
fn add(d: &mut Document, op: &str, data: Value) -> String { registry().apply(&format!("masonry.{op}"), data, d).unwrap().created[0].clone() }
fn expansion(d: &Document, id: &str) -> Expansion {
    let c = d.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.id == id => Some(c.clone()), _ => None }).unwrap();
    expand_any(&c, &House::read(d), "l")
}
/// World bounding box per face key (`<id>#<key>`).
fn boxes(ex: &Expansion) -> BTreeMap<String, ([f64; 3], [f64; 3])> {
    let mut m: BTreeMap<String, ([f64; 3], [f64; 3])> = BTreeMap::new();
    for e in &ex.faces {
        let Entity::Face(f) = e else { continue };
        let key = f.id.split_once('#').map(|x| x.1.to_string()).unwrap_or_default();
        let b = m.entry(key).or_insert(([f64::MAX; 3], [f64::MIN; 3]));
        for p in &f.outer { for k in 0..3 { b.0[k] = b.0[k].min(p[k]); b.1[k] = b.1[k].max(p[k]); } }
    }
    m
}
fn item<'a>(ex: &'a Expansion, cat: &str) -> &'a Item { ex.items.iter().find(|i| i.category == cat).unwrap_or_else(|| panic!("no {cat} item")) }
fn overlap(a: &([f64; 3], [f64; 3]), b: &([f64; 3], [f64; 3])) -> f64 { (0..3).map(|k| (a.1[k].min(b.1[k]) - a.0[k].max(b.0[k])).max(0.0)).product() }

#[test]
fn beam_and_block_floor_spans_the_short_way() {
    let mut d = doc();
    let id = add(&mut d, "add_floor", json!({ "level": 1, "origin": [0, 0], "size": [6, 4] }));
    let ex = expansion(&d, &id);
    let bx = boxes(&ex);
    let joists: Vec<&([f64; 3], [f64; 3])> = bx.iter().filter(|(k, _)| k.starts_with("joist.")).map(|(_, v)| v).collect();
    assert_eq!(joists.len(), 9);
    let mut xs: Vec<f64> = joists.iter().map(|b| (b.0[0] + b.1[0]) / 2.0).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for w in xs.windows(2) { assert!((w[1] - w[0] - 0.6).abs() < 1e-9, "spacing {xs:?}"); }
    // first / last between half a bay and a bay from the ring beam (0.1 wide)
    assert!(xs[0] - 0.1 >= 0.3 - 1e-9 && xs[0] - 0.1 < 0.6, "{xs:?}");
    assert!((5.9 - xs[8] - (xs[0] - 0.1)).abs() < 1e-9, "centred");
    // joists span z (the 4 m way) between the ring beams, on the underside
    for j in &joists { assert!((j.0[2] - 0.1).abs() < 1e-9 && (j.1[2] - 3.9).abs() < 1e-9 && (j.0[1] - 2.4).abs() < 1e-9 && (j.1[1] - 2.52).abs() < 1e-9); }
    // blocks: 10 bays × 16 blocks along 3.8 m (last cut)
    assert_eq!(item(&ex, "floorBlocks").count, 160.0);
    let blocks: Vec<&([f64; 3], [f64; 3])> = bx.iter().filter(|(k, _)| k.starts_with("block.")).map(|(_, v)| v).collect();
    assert_eq!(blocks.len(), 160);
    for b in &blocks { for j in &joists { assert!(overlap(b, j) < 1e-12); } assert!((b.1[1] - 2.56).abs() < 1e-9); }
    // topping 40 mm over 24 m², top at the storey base
    let t = item(&ex, "topping");
    assert!((t.area - 24.0).abs() < 1e-9 && (t.volume - 0.96).abs() < 1e-9);
    let tb = &bx["topping"]; assert!((tb.1[1] - 2.6).abs() < 1e-9 && (tb.0[1] - 2.56).abs() < 1e-9);
    assert!(bx.keys().filter(|k| k.starts_with("ring.")).count() == 4);
    assert!((item(&ex, "ringBeam").length - 20.0).abs() < 1e-9);
    assert_eq!(item(&ex, "joists").count, 9.0);
    assert!(item(&ex, "joists").description.contains("120×120"));
    assert!(ex.items.iter().any(|i| i.category == "mesh"));
}

#[test]
fn an_l_shaped_floor_is_clipped_per_joist() {
    let mut d = doc();
    let id = add(&mut d, "add_floor", json!({ "level": 1, "points": [[0, 0], [6, 0], [6, 2], [3, 2], [3, 5], [0, 5]] }));
    let bx = boxes(&expansion(&d, &id));
    let joists: Vec<&([f64; 3], [f64; 3])> = bx.iter().filter(|(k, _)| k.starts_with("joist.")).map(|(_, v)| v).collect();
    assert!(!joists.is_empty());
    for j in &joists {
        let x = (j.0[0] + j.1[0]) / 2.0; let l = j.1[2] - j.0[2];
        if x > 3.2 { assert!(l < 2.0, "short leg joist {l}"); } else if x < 2.8 { assert!(l > 4.5, "long leg joist {l}"); }
    }
}

#[test]
fn a_stair_well_gets_trimmers_and_headers() {
    let mut d = doc();
    let hole = json!([[2.0, 0.8], [3.0, 0.8], [3.0, 3.4], [2.0, 3.4]]);
    let id = add(&mut d, "add_floor", json!({ "level": 1, "origin": [0, 0], "size": [6, 4], "openings": [hole] }));
    let ex = expansion(&d, &id);
    let bx = boxes(&ex);
    assert_eq!(bx.keys().filter(|k| k.starts_with("trimmer.0.")).count(), 4, "two doubled trimmers");
    assert_eq!(bx.keys().filter(|k| k.starts_with("header.0.")).count(), 2);
    let inside = ([2.0 + 1e-6, 0.0, 0.8 + 1e-6], [3.0 - 1e-6, 10.0, 3.4 - 1e-6]);
    for (k, b) in &bx {
        if k.starts_with("joist.") || k.starts_with("block.") { assert!(overlap(b, &inside) < 1e-12, "{k} inside the stair well: {b:?}"); }
    }
    // members do not clash
    let members: Vec<(&String, &([f64; 3], [f64; 3]))> = bx.iter().filter(|(k, _)| ["joist.", "block.", "trimmer.", "header."].iter().any(|p| k.starts_with(p))).collect();
    for i in 0..members.len() { for j in (i + 1)..members.len() { assert!(overlap(members[i].1, members[j].1) < 1e-12, "{} / {}", members[i].0, members[j].0); } }
    assert!((item(&ex, "topping").area - (24.0 - 2.6)).abs() < 1e-9);
    assert!(item(&ex, "trimmers").count >= 6.0);
}

#[test]
fn hollow_core_and_in_situ_floors() {
    let mut d = doc();
    let id = add(&mut d, "add_floor", json!({ "level": 1, "origin": [0, 0], "size": [6, 4], "system": "hollowCore", "topping": 0.05 }));
    let ex = expansion(&d, &id);
    // planks span the 4 m way, laid along the 5.8 m between the ring beams
    assert_eq!(item(&ex, "planks").count, (5.8f64 / 1.2).ceil());
    assert!(item(&ex, "planks").description.contains("200"));
    let bx = boxes(&ex);
    assert!((bx["plank.0"].0[1] - (2.6 - 0.25)).abs() < 1e-9, "underside = base − (0.20 + 0.05)");
    let id2 = add(&mut d, "add_floor", json!({ "level": 1, "origin": [10, 0], "size": [5, 4], "system": "insitu" }));
    let ex2 = expansion(&d, &id2);
    assert!((item(&ex2, "slab").volume - 20.0 * 0.18).abs() < 1e-9);
    assert!(boxes(&ex2).contains_key("slab"));
}

#[test]
fn strip_foundation_under_a_rectangle() {
    let mut d = doc();
    let id = add(&mut d, "add_foundation", json!({ "origin": [0, 0], "size": [6, 4] }));
    let ex = expansion(&d, &id);
    let ft = item(&ex, "footing");
    assert!((ft.length - 20.0).abs() < 1e-9 && (ft.volume - 20.0 * 0.6 * 0.3).abs() < 1e-9);
    let bx = boxes(&ex);
    assert_eq!(bx.keys().filter(|k| k.starts_with("footing.")).count(), 4);
    let f0 = &bx["footing.0"];
    assert!((f0.0[1] + 0.8).abs() < 1e-9 && (f0.1[1] + 0.5).abs() < 1e-9);
    // footings close around the corners: the outer outline reaches 0.3 m past the wall lines
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for (k, b) in &bx { if k.starts_with("footing.") { for i in 0..3 { lo[i] = lo[i].min(b.0[i]); hi[i] = hi[i].max(b.1[i]); } } }
    assert!((lo[0] + 0.3).abs() < 1e-9 && (hi[0] - 6.3).abs() < 1e-9 && (lo[2] + 0.3).abs() < 1e-9 && (hi[2] - 4.3).abs() < 1e-9);
    // blocks from the footing (−0.5) up to the ground floor: 20 m × 0.5 m / (0.40 × 0.20)
    assert_eq!(item(&ex, "foundationBlocks").count, 125.0);
    assert!(item(&ex, "mortar").volume > 0.0);
}
