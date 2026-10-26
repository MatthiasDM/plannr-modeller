use plannr_core::model::{Document, Entity};
use plannr_core::ops::OpResult;
use plannr_core::plugin::Registry;
use plannr_plugin_construction::ConstructionPlugin;
use plannr_plugin_framing::{data::*, expand::*, hardware, FramingPlugin};
use serde_json::{json, Value};

fn registry() -> Registry { let mut r = Registry::new(); r.register(Box::new(ConstructionPlugin)); r.register(Box::new(FramingPlugin)); r }
fn doc() -> Document { let mut d = Document::default(); registry().on_load(&mut d); d }
fn fr(doc: &mut Document, op: &str, data: Value) -> Result<OpResult, plannr_core::ops::OpError> { registry().apply(&format!("framing.{op}"), data, doc) }
fn data_of(r: &OpResult) -> Value { match &r.entities[0] { Entity::Custom(c) => c.data.clone(), _ => Value::Null } }
fn custom(doc: &Document, id: &str) -> plannr_core::model::Custom { doc.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.id == id => Some(c.clone()), _ => None }).unwrap() }
fn expansion(doc: &Document, id: &str) -> Expansion { expand_any(&custom(doc, id), &House::read(doc), "l") }
fn findings_for(doc: &mut Document, id: &str) -> Vec<Value> {
    let d = data_of(&fr(doc, "check", json!({})).unwrap());
    d["findings"].as_array().unwrap().iter().filter(|f| f["element"] == json!(id)).cloned().collect()
}
fn has(f: &[Value], severity: &str, title: &str) -> bool { f.iter().any(|x| x["severity"] == json!(severity) && x["title"].as_str().unwrap_or("").contains(title)) }

/// Well-formed XML: every tag closed in order, quotes balanced inside tags, `&` only as an entity.
fn assert_well_formed(svg: &str) {
    let mut stack: Vec<String> = vec![];
    let mut rest = svg;
    while let Some(i) = rest.find('<') {
        let text = &rest[..i];
        for (k, _) in text.match_indices('&') { let t = &text[k..]; assert!(t.starts_with("&amp;") || t.starts_with("&lt;") || t.starts_with("&gt;") || t.starts_with("&quot;"), "raw & in text: {}", &t[..t.len().min(20)]); }
        let j = rest[i..].find('>').expect("unterminated tag") + i;
        let tag = &rest[i + 1..j];
        assert_eq!(tag.matches('"').count() % 2, 0, "unbalanced quotes in <{tag}>");
        if let Some(name) = tag.strip_prefix('/') { assert_eq!(stack.pop().as_deref(), Some(name.trim()), "mismatched </{name}>"); }
        else if !tag.ends_with('/') { stack.push(tag.split_whitespace().next().unwrap().to_string()); }
        rest = &rest[j + 1..];
    }
    assert!(stack.is_empty(), "unclosed: {stack:?}");
}

#[test]
fn undersized_floor_joists_are_errors_and_adequate_ones_ok() {
    let mut d = doc();
    // 5 × 8 m: joists span the 5 m way
    let weak = fr(&mut d, "add_floor", json!({ "origin": [0, 0], "size": [5, 8], "joistSize": "2x8", "joistSpacing": 0.4 })).unwrap().created[0].clone();
    let good = fr(&mut d, "add_floor", json!({ "origin": [10, 0], "size": [4, 6], "joistSize": "2x10", "joistSpacing": 0.4 })).unwrap().created[0].clone();
    let fw = findings_for(&mut d, &weak);
    assert!(has(&fw, "error", "Floor joists overstressed"), "{fw:#?}");
    let fg = findings_for(&mut d, &good);
    assert!(has(&fg, "ok", "Floor joists OK"), "{fg:#?}");
    let all = data_of(&fr(&mut d, "check", json!({})).unwrap());
    assert!(all["errors"].as_u64().unwrap() >= 1);
}

#[test]
fn span_table_grows_with_depth_and_species() {
    let mut d = doc();
    let t = data_of(&fr(&mut d, "span_table", json!({ "use": "floor", "spacing": 0.4 })).unwrap());
    let spans: Vec<f64> = t["spans"].as_array().unwrap().iter().map(|r| r["span_m"].as_f64().unwrap()).collect();
    assert_eq!(spans.len(), 4);
    assert!(spans.windows(2).all(|w| w[1] > w[0]), "{spans:?}");
    assert!(spans[2] > 4.0 && spans[1] < 5.0, "2x10 > 4 m, 2x8 < 5 m: {spans:?}");
    let c30 = data_of(&fr(&mut d, "span_table", json!({ "use": "floor", "spacing": 0.4, "species": "C30" })).unwrap());
    assert!(c30["spans"][2]["span_m"].as_f64().unwrap() >= spans[2]);
    let roof = data_of(&fr(&mut d, "span_table", json!({ "use": "roof", "spacing": 0.6, "species": "SPF2" })).unwrap());
    assert_eq!(roof["species"], json!("SPF2"));
    assert!(roof["spans"][0]["span_m"].as_f64().unwrap() > 1.0);
}

#[test]
fn undersized_header_and_tall_studs_are_flagged() {
    let mut d = doc();
    let w = fr(&mut d, "add_wall", json!({ "a": [0, 0], "b": [6, 0], "height": 2.44 })).unwrap().created[0].clone();
    fr(&mut d, "add_opening", json!({ "wall": w, "kind": "window", "offset": 1.0, "width": 2.0, "height": 1.2, "sill": 0.9, "headerSize": "2x6" })).unwrap();
    let f = findings_for(&mut d, &w);
    assert!(f.iter().any(|x| x["title"].as_str().unwrap().contains("Header smaller") && x["severity"] == json!("warning")), "{f:#?}");
    assert!(has(&f, "ok", "Studs OK"));
    let tall = fr(&mut d, "add_wall", json!({ "a": [0, 3], "b": [4, 3], "height": 3.5, "studSize": "2x4" })).unwrap().created[0].clone();
    let ft = findings_for(&mut d, &tall);
    assert!(has(&ft, "error", "Studs too tall"), "{ft:#?}");
    // a 2x6 wall may be that tall
    fr(&mut d, "update", json!({ "id": tall, "patch": { "studSize": "2x6" } })).unwrap();
    assert!(!has(&findings_for(&mut d, &tall), "error", "Studs too tall"));
}

#[test]
fn header_under_a_roof_is_checked_in_bending() {
    let mut d = doc();
    // eave wall along x at z = 0 under a 8 m span roof with the ridge along x
    let w = fr(&mut d, "add_wall", json!({ "a": [0, 0], "b": [8, 0], "height": 2.44 })).unwrap().created[0].clone();
    fr(&mut d, "add_opening", json!({ "wall": w, "kind": "door", "offset": 2.0, "width": 2.4, "height": 2.05 })).unwrap();
    fr(&mut d, "add_roof", json!({ "min": [0, 0], "max": [8, 8], "level": 0 })).unwrap();
    let f = findings_for(&mut d, &w);
    assert!(f.iter().any(|x| x["title"].as_str().unwrap().starts_with("Header of opening 1")), "{f:#?}");
}

#[test]
fn beam_has_plies_posts_and_is_checked() {
    let mut d = doc();
    let b = fr(&mut d, "add_beam", json!({ "a": [0, 0], "b": [4, 0], "size": "2x10", "plies": 3, "postSpacing": 2.0, "tributary": 2.0 })).unwrap().created[0].clone();
    let ex = expansion(&d, &b);
    assert_eq!(ex.members.iter().filter(|m| m.key.starts_with("ply.")).count(), 3);
    let posts: Vec<&Member> = ex.members.iter().filter(|m| m.key.starts_with("post.")).collect();
    assert_eq!(posts.len(), 3, "ends + one in the middle");
    let h = House::read(&d);
    let top = h.base(0) + h.height(0);
    assert!(posts.iter().all(|p| (p.r[1][1] - (top - 0.235)).abs() < 1e-9 && (p.r[1][0] - h.base(0)).abs() < 1e-9), "posts from the floor to the beam underside");
    let f = findings_for(&mut d, &b);
    assert!(f.iter().any(|x| x["title"].as_str().unwrap().starts_with("Beam")), "{f:#?}");
    // a long single 2x6 carrying 4 m of floor fails
    let weak = fr(&mut d, "add_beam", json!({ "a": [0, 5], "b": [5, 5], "size": "2x6", "plies": 1, "posts": false, "tributary": 4.0 })).unwrap().created[0].clone();
    assert!(has(&findings_for(&mut d, &weak), "error", "Beam overstressed"));
    // LVL validation
    assert!(fr(&mut d, "add_beam", json!({ "a": [0, 0], "b": [4, 0], "material": "steel" })).is_err());
    let est = data_of(&fr(&mut d, "estimate", json!({ "ids": [b] })).unwrap());
    assert!(est["items"].as_array().unwrap().iter().any(|i| i["category"] == json!("posts") && i["count"] == json!(3)));
}

#[test]
fn joists_ending_against_a_flush_beam_get_hangers() {
    let mut d = doc();
    let f = fr(&mut d, "add_floor", json!({ "origin": [0, 0], "size": [4, 3], "direction": "z", "joistSize": "2x10", "subfloor": { "enabled": false } })).unwrap().created[0].clone();
    let joists = expansion(&d, &f).members.iter().filter(|m| m.key.starts_with("joist.")).count();
    let b = fr(&mut d, "add_beam", json!({ "a": [0, 3.06], "b": [4, 3.06], "size": "2x10", "plies": 3, "elevation": 0.235, "posts": false })).unwrap().created[0].clone();
    let ex = expansion(&d, &b);
    let hangers = ex.hardware.iter().filter(|p| p.description == hardware::HANGER).count();
    assert_eq!(hangers, joists, "one hanger per joist end at the beam");
}

#[test]
fn anchor_bolts_skip_the_door_and_ties_sit_at_every_truss_bearing() {
    let mut d = doc();
    let w = fr(&mut d, "add_wall", json!({ "a": [0, 0], "b": [6, 0], "height": 2.44 })).unwrap().created[0].clone();
    fr(&mut d, "add_opening", json!({ "wall": w, "kind": "door", "offset": 2.0, "width": 0.9, "height": 2.05 })).unwrap();
    let ex = expansion(&d, &w);
    let anchors = ex.hardware.iter().filter(|p| p.description == hardware::ANCHOR).count();
    assert!(anchors >= 4, "{anchors}");
    let wall: Wall = serde_json::from_value(custom(&d, &w).data).unwrap();
    let offs = hardware::anchor_bolt_offsets(&wall, 0.0, 6.0);
    assert_eq!(offs.len(), anchors);
    assert!(offs.iter().all(|u| *u < 2.0 || *u > 2.9), "none in the door: {offs:?}");
    assert!(offs.windows(2).all(|p| p[1] - p[0] <= 1.8 + 1e-9 || (p[0] < 2.0 && p[1] > 2.9)), "≤ 1.8 m apart: {offs:?}");
    assert!((offs[0] - 0.3).abs() < 1e-9 && (offs.last().unwrap() - 5.7).abs() < 1e-9);
    assert_eq!(hardware::anchor_bolts(&wall, &House::read(&d)).len(), anchors);
    assert!(ex.faces.iter().any(|f| f.id().contains("#hw.anchor.0")));
    // upper storey walls get none
    let up = fr(&mut d, "add_wall", json!({ "a": [0, 0], "b": [6, 0], "level": 1 })).unwrap().created[0].clone();
    assert_eq!(expansion(&d, &up).hardware.len(), 0);
    // hurricane ties: two per truss
    let r = fr(&mut d, "add_roof", json!({ "min": [0, 0], "max": [6, 8] })).unwrap().created[0].clone();
    let rex = expansion(&d, &r);
    let trusses = rex.members.iter().filter(|m| m.key.ends_with(".bottom")).count();
    assert_eq!(rex.hardware.iter().filter(|p| p.description == hardware::TIE).count(), 2 * trusses);
    // hold-downs only when switched on; the estimate counts hardware
    fr(&mut d, "set_defaults", json!({ "hardware": { "holdDowns": true } })).unwrap();
    assert_eq!(expansion(&d, &w).hardware.iter().filter(|p| p.description == hardware::HOLD_DOWN).count(), 2);
    let est = data_of(&fr(&mut d, "estimate", json!({})).unwrap());
    let pieces = est["totals"]["hardware_pieces"].as_u64().unwrap();
    assert_eq!(pieces as usize, anchors + 2 + 2 + 2 * trusses, "anchors + hold-downs on both walls + ties");
    assert!(est["items"].as_array().unwrap().iter().any(|i| i["category"] == json!("hardware") && i["description"] == json!("Hurricane tie")));
    // lumber totals do not count hardware
    assert!(est["items"].as_array().unwrap().iter().filter(|i| i["category"] == json!("hardware")).all(|i| i.get("length_m").is_none()));
}

#[test]
fn stair_geometry_and_step_rule() {
    let mut d = doc();
    let s = fr(&mut d, "add_stair", json!({ "a": [0, 0], "direction": [1, 0], "totalRise": 2.7 })).unwrap().created[0].clone();
    let ex = expansion(&d, &s);
    // ceil(2.7 / 0.18) = 15 risers of 180 mm, 14 treads
    assert_eq!(ex.members.iter().filter(|m| m.key.starts_with("tread.")).count(), 14);
    assert_eq!(ex.members.iter().filter(|m| m.key.starts_with("riser.")).count(), 14);
    assert_eq!(ex.members.iter().filter(|m| m.category == "stringers").count(), 3);
    assert_eq!(ex.faces.iter().filter(|f| f.id().contains("#stringer.0")).count(), 2 + plannr_plugin_framing::stair::stringer_profile(15, 0.18, 0.26, 0.032, 0.286).len());
    let top_tread = ex.members.iter().find(|m| m.key == "tread.14").unwrap();
    let h = House::read(&d);
    assert!((top_tread.r[1][1] - 14.0 * 0.18).abs() < 1e-9);
    assert!((top_tread.origin[1] - h.base(0)).abs() < 1e-9);
    let f = findings_for(&mut d, &s);
    assert!(has(&f, "ok", "Stair comfortable"), "{f:#?}");
    // steep and shallow: riser 210, going 200
    let steep = fr(&mut d, "add_stair", json!({ "a": [5, 0], "angle": 90, "totalRise": 2.52, "riserHeight": 0.21, "treadDepth": 0.2 })).unwrap().created[0].clone();
    let st: Stair = serde_json::from_value(custom(&d, &steep).data).unwrap();
    assert!((st.direction[0]).abs() < 1e-9 && (st.direction[1] - 1.0).abs() < 1e-9, "angle 90 → +z");
    let fs = findings_for(&mut d, &steep);
    assert!(has(&fs, "warning", "Riser height") && has(&fs, "warning", "Tread depth"), "{fs:#?}");
    // moving turns the direction with it
    let est = data_of(&fr(&mut d, "estimate", json!({ "ids": [s] })).unwrap());
    assert!(est["items"].as_array().unwrap().iter().any(|i| i["category"] == json!("stringers") && i["count"] == json!(3)));
}

#[test]
fn wall_and_plan_drawings_draw_every_member_once() {
    let mut d = doc();
    let w = fr(&mut d, "add_wall", json!({ "a": [0, 0], "b": [5, 0], "height": 2.44 })).unwrap().created[0].clone();
    fr(&mut d, "add_opening", json!({ "wall": w, "kind": "window", "offset": 1.0, "width": 1.2, "height": 1.2, "sill": 0.9 })).unwrap();
    fr(&mut d, "add_opening", json!({ "wall": w, "kind": "door", "offset": 3.2, "width": 0.9, "height": 2.05 })).unwrap();
    let svg = data_of(&fr(&mut d, "wall_svg", json!({ "wall": w })).unwrap())["svg"].as_str().unwrap().to_string();
    assert_well_formed(&svg);
    let n = expand_element(&custom(&d, &w), &House::read(&d), "").members.len();
    assert_eq!(svg.matches("class=\"member ").count(), n);
    assert_eq!(svg.matches("class=\"opening\"").count(), 2);
    assert!(svg.contains("5000 mm") && svg.contains("W1: 1200 × 1200, sill 900") && svg.contains("D2: 900 × 2050"));
    assert!(fr(&mut d, "wall_svg", json!({ "wall": "nope" })).is_err());

    let f = fr(&mut d, "add_floor", json!({ "origin": [0, 0], "size": [5, 4] })).unwrap().created[0].clone();
    let b = fr(&mut d, "add_beam", json!({ "a": [0, 2], "b": [5, 2], "posts": true })).unwrap().created[0].clone();
    let s = fr(&mut d, "add_stair", json!({ "a": [1, 1], "direction": [1, 0] })).unwrap().created[0].clone();
    let plan = data_of(&fr(&mut d, "plan_svg", json!({ "level": 0 })).unwrap())["svg"].as_str().unwrap().to_string();
    assert_well_formed(&plan);
    let h = House::read(&d);
    let total: usize = [&w, &f, &b, &s].iter().map(|id| expand_element(&custom(&d, id), &h, "").members.len()).sum();
    assert_eq!(plan.matches("class=\"member ").count(), total);
    assert!(plan.contains("2x10 @ 400") && plan.contains("UP ") && plan.contains("1 m"));
}

#[test]
fn beam_and_stair_move_and_turn_in_plan() {
    let mut d = doc();
    let b = fr(&mut d, "add_beam", json!({ "a": [0, 0], "b": [4, 0] })).unwrap().created[0].clone();
    let s = fr(&mut d, "add_stair", json!({ "a": [0, 0], "direction": [1, 0] })).unwrap().created[0].clone();
    let rot = plannr_core::geom::mat_rotation([0.0, 1.0, 0.0], std::f64::consts::FRAC_PI_2);
    let reg = registry();
    for id in [&b, &s] {
        let mut c = custom(&d, id);
        reg.get("framing").unwrap().transform(&mut c, &rot, &d).unwrap();
        for e in d.entities.iter_mut() { if let Entity::Custom(x) = e { if &x.id == id { *x = c.clone(); } } }
    }
    let bm: Beam = serde_json::from_value(custom(&d, &b).data).unwrap();
    assert!(bm.b[0].abs() < 1e-9 && (bm.b[1].abs() - 4.0).abs() < 1e-9, "{:?}", bm.b);
    let st: Stair = serde_json::from_value(custom(&d, &s).data).unwrap();
    assert!(st.direction[0].abs() < 1e-9 && (st.direction[1].abs() - 1.0).abs() < 1e-9, "{:?}", st.direction);
    // update validates
    assert!(fr(&mut d, "update", json!({ "id": b, "patch": { "plies": 9 } })).is_err());
    assert!(fr(&mut d, "update", json!({ "id": s, "patch": { "stringers": 1 } })).is_err());
    assert!(fr(&mut d, "set_defaults", json!({ "species": "Oak" })).is_err());
    let sum = data_of(&fr(&mut d, "summary", json!({})).unwrap());
    assert_eq!(sum["beams"].as_array().unwrap().len(), 1);
    assert_eq!(sum["stairs"][0]["risers"], json!(15));
}
