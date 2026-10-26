//! Walls drawn along their exterior face and the automatic corners / T intersections between them.

use plannr_core::geom::{add, scale, P3};
use plannr_core::model::{Document, Entity};
use plannr_core::ops::OpResult;
use plannr_core::plugin::Registry;
use plannr_plugin_framing::{data::*, expand::*, FramingPlugin};
use serde_json::{json, Value};

fn registry() -> Registry { let mut r = Registry::new(); r.register(Box::new(FramingPlugin)); r }
fn doc() -> Document { let mut d = Document::default(); registry().on_load(&mut d); d }
fn fr(doc: &mut Document, op: &str, data: Value) -> OpResult { registry().apply(&format!("framing.{op}"), data, doc).unwrap() }
fn custom(doc: &Document, id: &str) -> plannr_core::model::Custom { doc.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.id == id => Some(c.clone()), _ => None }).unwrap() }
fn expansion(doc: &Document, id: &str) -> Expansion { expand_any(&custom(doc, id), &House::read(doc), "l") }
fn wall(doc: &mut Document, a: [f64; 2], b: [f64; 2], extra: Value) -> String {
    let mut v = json!({ "a": a, "b": b, "height": 2.44, "studSize": "2x4", "studSpacing": 0.4 });
    if let (Value::Object(m), Value::Object(e)) = (&mut v, extra) { m.extend(e); }
    fr(doc, "add_wall", v).created[0].clone()
}
/// World axis-aligned box of a member.
fn aabb(m: &Member) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::MAX; 3]; let mut hi = [f64::MIN; 3];
    for x in m.r[0] { for y in m.r[1] { for z in m.r[2] {
        let p: P3 = add(add(add(m.origin, scale(m.ex, x)), scale(m.ey, y)), scale(m.ez, z));
        for k in 0..3 { lo[k] = lo[k].min(p[k]); hi[k] = hi[k].max(p[k]); }
    } } }
    (lo, hi)
}
fn clash(a: &Member, b: &Member) -> f64 {
    let (al, ah) = aabb(a); let (bl, bh) = aabb(b);
    (0..3).map(|k| (ah[k].min(bh[k]) - al[k].max(bl[k])).max(0.0)).product()
}
/// Clockwise (seen from above) 6 × 4 m rectangle, drawn as one chain along the outside faces.
fn rectangle(d: &mut Document) -> Vec<String> {
    let pts = [[0.0, 0.0], [6.0, 0.0], [6.0, 4.0], [0.0, 4.0]];
    (0..4).map(|i| wall(d, pts[i], pts[(i + 1) % 4], json!({}))).collect()
}

#[test]
fn a_new_wall_is_drawn_along_its_exterior_face() {
    let mut d = doc();
    let id = wall(&mut d, [0.0, 0.0], [4.0, 0.0], json!({}));
    assert_eq!(serde_json::from_value::<Wall>(custom(&d, &id).data).unwrap().reference, "exterior");
    let ex = expansion(&d, &id);
    // exterior = left of a→b = −z; the studs lie on the +z side of the drawn line, sheathing on the line's outside
    let (mut zlo, mut zhi) = (f64::MAX, f64::MIN);
    for m in ex.members.iter().filter(|m| m.key.starts_with("stud.")) { let (l, h) = aabb(m); zlo = zlo.min(l[2]); zhi = zhi.max(h[2]); }
    assert!(zlo.abs() < 1e-9 && (zhi - 0.089).abs() < 1e-9, "studs from z = 0 to 0.089: {zlo} … {zhi}");
    // the centre reference keeps the old behaviour
    let c = wall(&mut d, [0.0, 10.0], [4.0, 10.0], json!({ "reference": "center" }));
    let ex = expansion(&d, &c);
    let s = ex.members.iter().find(|m| m.key == "stud.1").unwrap();
    let (l, h) = aabb(s);
    assert!((l[2] - (10.0 - 0.0445)).abs() < 1e-9 && (h[2] - (10.0 + 0.0445)).abs() < 1e-9);
}

#[test]
fn a_clockwise_rectangle_closes_with_california_corners_and_no_clashes() {
    let mut d = doc();
    let ids = rectangle(&mut d);
    let exs: Vec<Expansion> = ids.iter().map(|i| expansion(&d, i)).collect();
    // outer footprint is exactly the drawn rectangle
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for ex in &exs { for m in &ex.members { let (l, h) = aabb(m); for k in 0..3 { lo[k] = lo[k].min(l[k]); hi[k] = hi[k].max(h[k]); } } }
    assert!(lo[0].abs() < 1e-9 && lo[2].abs() < 1e-9 && (hi[0] - 6.0).abs() < 1e-9 && (hi[2] - 4.0).abs() < 1e-9, "footprint {lo:?} {hi:?}");
    // no member of one wall runs into a member of another
    for i in 0..4 { for j in (i + 1)..4 { for a in &exs[i].members { for b in &exs[j].members {
        assert!(clash(a, b) < 1e-9, "wall {i} {} clashes with wall {j} {}: {:?} {:?}", a.key, b.key, aabb(a), aabb(b));
    } } } }
    // each wall runs through at its end (b) and butts at its start (a): one California flat stud per wall
    for ex in &exs {
        assert_eq!(ex.members.iter().filter(|m| m.key.starts_with("corner.end")).count(), 1);
        assert_eq!(ex.members.iter().filter(|m| m.key.starts_with("corner.start")).count(), 0);
    }
    // the butting wall starts at the through wall's inside face
    let first = exs[1].members.iter().find(|m| m.key == "plate.bottom.0").unwrap();
    assert!((first.r[0][0] - 0.089).abs() < 1e-9, "{:?}", first.r);
    // exterior sheathing laps: the butting wall's board starts 11 mm outside the corner, covering the other's edge
    let sheets: Vec<&Entity> = exs[1].faces.iter().filter(|f| matches!(f, Entity::Face(x) if x.id.ends_with("#sheathing.ext"))).collect();
    let zmin = sheets.iter().flat_map(|f| match f { Entity::Face(x) => x.outer.iter().map(|p| p[2]).collect::<Vec<_>>(), _ => vec![] }).fold(f64::MAX, f64::min);
    assert!((zmin + 0.011).abs() < 1e-9, "sheathing of wall 1 starts at z = −0.011: {zmin}");
    // double top plates interlock: the butting wall's upper plate reaches the corner, the through wall's is cut back
    let up_butt = exs[1].members.iter().find(|m| m.key == "plate.top.0").unwrap();
    let up_through = exs[0].members.iter().find(|m| m.key == "plate.top.0").unwrap();
    assert!(up_butt.r[0][0].abs() < 1e-9 && (up_through.r[0][1] - (6.0 - 0.089)).abs() < 1e-9, "{:?} {:?}", up_butt.r, up_through.r);
}

#[test]
fn corner_details_and_explicit_joins() {
    let mut d = doc();
    fr(&mut d, "set_defaults", json!({ "cornerDetail": "threeStud" }));
    let ids = rectangle(&mut d);
    let ex = expansion(&d, &ids[0]);
    let c = ex.members.iter().find(|m| m.key == "corner.end.0").expect("three-stud corner");
    assert!((c.r[2][1] - c.r[2][0] - 0.089).abs() < 1e-9, "a regular stud, not flat");
    // forcing the first wall to butt at its end flips the corner
    fr(&mut d, "update", json!({ "id": ids[0], "patch": { "endJoin": "butt" } }));
    let ex0 = expansion(&d, &ids[0]); let ex1 = expansion(&d, &ids[1]);
    assert_eq!(ex0.members.iter().filter(|m| m.key.starts_with("corner.end")).count(), 0);
    assert_eq!(ex1.members.iter().filter(|m| m.key.starts_with("corner.start")).count(), 1);
    let p = ex0.members.iter().find(|m| m.key == "plate.bottom.0").unwrap();
    assert!((p.r[0][1] - (6.0 - 0.089)).abs() < 1e-9);
    // `none` leaves the ends as drawn
    fr(&mut d, "update", json!({ "id": ids[0], "patch": { "endJoin": "none" } }));
    fr(&mut d, "update", json!({ "id": ids[1], "patch": { "startJoin": "none" } }));
    let p = expansion(&d, &ids[0]).members.into_iter().find(|m| m.key == "plate.bottom.0").unwrap();
    assert!((p.r[0][1] - 6.0).abs() < 1e-9);
    assert!(registry().apply("framing.update", json!({ "id": ids[0], "patch": { "tDetail": "nope" } }), &mut d).is_err());
}

#[test]
fn a_partition_butts_into_both_walls_and_gets_backers() {
    let mut d = doc();
    let ids = rectangle(&mut d);
    let p = wall(&mut d, [3.0, 0.0], [3.0, 4.0], json!({ "reference": "center", "sheathing": { "exterior": { "enabled": false } } }));
    let ex = expansion(&d, &p);
    let plate = ex.members.iter().find(|m| m.key == "plate.bottom.0").unwrap();
    assert!((plate.r[0][0] - 0.089).abs() < 1e-9 && (plate.r[0][1] - (4.0 - 0.089)).abs() < 1e-9, "trimmed to the inside faces: {:?}", plate.r);
    // its upper top plate laps over the exterior walls' plates
    let up = ex.members.iter().find(|m| m.key == "plate.top.0").unwrap();
    assert!(up.r[0][0].abs() < 1e-9 && (up.r[0][1] - 4.0).abs() < 1e-9, "{:?}", up.r);
    for (i, w) in [(0, &ids[0]), (2, &ids[2])] {
        let exw = expansion(&d, w);
        assert_eq!(exw.members.iter().filter(|m| m.key.starts_with("tbacker.")).count(), 2, "wall {i}: a two-stud channel");
        assert!(exw.members.iter().any(|m| m.key == "plate.top.0.1"), "wall {i}: the upper plate is cut for the lap");
        for a in &exw.members { for b in &ex.members { assert!(clash(a, b) < 1e-9, "{} / {}", a.key, b.key); } }
    }
    // ladder blocking instead
    fr(&mut d, "update", json!({ "id": ids[0], "patch": { "tDetail": "ladder" } }));
    let exw = expansion(&d, &ids[0]);
    assert!(exw.members.iter().filter(|m| m.key.contains(".block.")).count() >= 3);
}

#[test]
fn walls_framed_from_rooms_keep_their_centre_lines() {
    let mut d = doc();
    // old documents have no `reference`: centre line, no junction surprises for collinear walls
    let a = wall(&mut d, [0.0, 0.0], [3.0, 0.0], json!({ "reference": "center" }));
    let b = wall(&mut d, [3.0, 0.0], [6.0, 0.0], json!({ "reference": "center" }));
    for id in [&a, &b] {
        let p = expansion(&d, id).members.into_iter().find(|m| m.key == "plate.bottom.0").unwrap();
        assert!((p.r[0][1] - p.r[0][0] - 3.0).abs() < 1e-9, "collinear walls are not trimmed");
    }
}
