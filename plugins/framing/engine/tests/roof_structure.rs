//! Every roof, on a 4 × 4 m shed of framed walls, must be carried by those walls: trusses bear on the
//! top plates with the top chord landing on the bottom chord at the heel, rafters seat on the plates
//! with a birdsmouth and meet a ridge board that covers their ends, the high end of a shed roof and the
//! purlins of a gambrel stand on studs — nothing floats and nothing sinks into the walls.

use plannr_core::geom::{add, scale, P3};
use plannr_core::model::{Document, Entity};
use plannr_core::plugin::Registry;
use plannr_plugin_framing::{data::*, expand::*, FramingPlugin};
use serde_json::{json, Value};

fn registry() -> Registry { let mut r = Registry::new(); r.register(Box::new(FramingPlugin)); r }
fn doc() -> Document { let mut d = Document::default(); registry().on_load(&mut d); d }
fn fr(d: &mut Document, op: &str, v: Value) -> String { registry().apply(&format!("framing.{op}"), v, d).unwrap().created.first().cloned().unwrap_or_default() }
fn expansion(d: &Document, id: &str) -> Expansion {
    let c = d.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.id == id => Some(c.clone()), _ => None }).unwrap();
    expand_any(&c, &House::read(d), "l")
}
const E: f64 = 0.003;

/// A clockwise 4 × 4 shed of 2x4 walls (exterior faces on the square), and a roof over it.
fn shed(roof: Value) -> (Document, Expansion, f64) {
    let mut d = doc();
    let p = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
    let mut wall = String::new();
    for k in 0..4 { wall = fr(&mut d, "add_wall", json!({ "a": p[k], "b": p[(k + 1) % 4] })); }
    let top = expansion(&d, &wall).members.iter().filter(|m| m.key.starts_with("plate.top")).flat_map(|m| corners(m)).map(|q| q[1]).fold(f64::MIN, f64::max);
    let mut v = json!({ "level": 0, "min": [0, 0], "max": [4, 4], "pitch": 8, "ridgeAlongX": true, "sheathing": { "enabled": false }, "fascia": { "enabled": false } });
    if let (Value::Object(a), Value::Object(b)) = (&mut v, roof) { a.extend(b); }
    let id = fr(&mut d, "add_roof", v);
    let ex = expansion(&d, &id);
    (d, ex, top)
}
fn corners(m: &Member) -> Vec<P3> {
    let mut v = vec![];
    for x in m.r[0] { for y in m.r[1] { for z in m.r[2] { v.push(add(add(add(m.origin, scale(m.ex, x)), scale(m.ey, y)), scale(m.ez, z))); } } }
    v
}
fn get<'a>(ex: &'a Expansion, key: &str) -> &'a Member { ex.members.iter().find(|m| m.key == key).unwrap_or_else(|| panic!("no member {key}: {:?}", ex.members.iter().map(|m| &m.key).take(40).collect::<Vec<_>>())) }
/// Height of a member's lower long edge (its underside line, local y = r[1][0], mid thickness) at plan coordinate z.
fn under_at_z(m: &Member, z: f64) -> f64 {
    let p0 = add(add(m.origin, scale(m.ey, m.r[1][0])), scale(m.ez, (m.r[2][0] + m.r[2][1]) / 2.0));
    let t = (z - p0[2]) / m.ex[2];
    p0[1] + m.ex[1] * t
}
fn min_y(m: &Member) -> f64 { corners(m).iter().map(|p| p[1]).fold(f64::MAX, f64::min) }

#[test]
fn trusses_bear_on_the_plates_with_a_standard_heel() {
    for ty in ["fink", "howe", "kingPost", "queenPost"] {
        let (_, ex, top) = shed(json!({ "type": ty }));
        let bottom = get(&ex, "truss.3.bottom");
        assert!((min_y(bottom) - top).abs() < E, "{ty}: bottom chord on the plates ({} vs {top})", min_y(bottom));
        let cd = bottom.r[1][1] - bottom.r[1][0];
        // at the outer wall face (z = 0) the top chord's underside lands on the bottom chord's top
        let tc = get(&ex, "truss.3.top.l");
        assert!((under_at_z(tc, 0.0) - (top + cd)).abs() < E, "{ty}: heel joint ({} vs {})", under_at_z(tc, 0.0), top + cd);
        // the two top chords meet at the peak
        let tr = get(&ex, "truss.3.top.r");
        assert!((under_at_z(tc, 2.0) - under_at_z(tr, 2.0)).abs() < E, "{ty}: peak");
        // no raised heel → no heel posts
        assert!(!ex.members.iter().any(|m| m.key.contains(".heel")));
    }
}

#[test]
fn a_raised_heel_gets_heel_posts() {
    let (_, ex, top) = shed(json!({ "type": "fink", "heel": 0.3 }));
    let post = get(&ex, "truss.3.heel.l");
    let tc = get(&ex, "truss.3.top.l");
    assert!((under_at_z(tc, 0.0) - (top + 0.3)).abs() < E);
    let ys: Vec<f64> = corners(post).iter().map(|p| p[1]).collect();
    let (lo, hi) = (ys.iter().cloned().fold(f64::MAX, f64::min), ys.iter().cloned().fold(f64::MIN, f64::max));
    assert!(lo < top + 0.1 && hi > top + 0.25, "the post spans bottom chord → top chord: {lo} … {hi}");
    // it stands over the wall (flush with its outer face) and stays under the top chord
    let zs: Vec<f64> = corners(post).iter().map(|p| p[2]).collect();
    assert!(zs.iter().cloned().fold(f64::MAX, f64::min) >= -E && zs.iter().cloned().fold(f64::MIN, f64::max) <= 0.089 + E, "post over the wall: {zs:?}");
    assert!(hi <= under_at_z(tc, 0.0) + E, "post top {hi} under the top chord {}", under_at_z(tc, 0.0));
}

#[test]
fn rafters_seat_on_the_plates_and_meet_a_ridge_board() {
    let (_, ex, top) = shed(json!({ "type": "rafters" }));
    let raf = get(&ex, "rafter.3.l");
    // birdsmouth: the underside line crosses the plate top at the plate's inner edge (z = 0.089)
    assert!((under_at_z(raf, 0.089) - top).abs() < E, "seat on the plate: {} vs {top}", under_at_z(raf, 0.089));
    // and is the heel-cut depth below it at the outer face — never deeper than a third of the rafter
    let cut = top - under_at_z(raf, 0.0);
    assert!(cut > 0.0 && cut < (raf.r[1][1] - raf.r[1][0]) / 3.0, "heel cut {cut}");
    // the ceiling joist bears on the plates beside the rafter, not in it
    let cj = get(&ex, "ceilingJoist.3");
    assert!((min_y(cj) - top).abs() < E);
    let cj_x: Vec<f64> = corners(cj).iter().map(|p| p[0]).collect(); let raf_x: Vec<f64> = corners(raf).iter().map(|p| p[0]).collect();
    let overlap = cj_x.iter().cloned().fold(f64::MIN, f64::max).min(raf_x.iter().cloned().fold(f64::MIN, f64::max)) - cj_x.iter().cloned().fold(f64::MAX, f64::min).max(raf_x.iter().cloned().fold(f64::MAX, f64::min));
    assert!(overlap < 1e-6, "the ceiling joist sits beside the rafter ({overlap})");
    // the ridge board covers the rafter's plumb cut: its top at least the rafter's top edge there
    let ridge = get(&ex, "ridge");
    let ry: Vec<f64> = corners(ridge).iter().map(|p| p[1]).collect();
    let rtop = ry.iter().cloned().fold(f64::MIN, f64::max); let rbot = ry.iter().cloned().fold(f64::MAX, f64::min);
    let face = 2.0 - 0.019; // the rafter ends at the ridge board's face
    let k: f64 = 8.0 / 12.0; let depth = (raf.r[1][1] - raf.r[1][0]) * (1.0 + k * k).sqrt();
    assert!(rtop >= under_at_z(raf, face) + depth - E && rbot <= under_at_z(raf, face) + E, "ridge {rbot}…{rtop} vs rafter end {}…{}", under_at_z(raf, face), under_at_z(raf, face) + depth);
}

#[test]
fn hip_roofs_bear_on_all_four_walls() {
    for ty in ["rafters", "fink"] {
        let (_, ex, top) = shed(json!({ "shape": "hip", "type": ty }));
        // nothing structural sinks into the walls beyond a birdsmouth (k × 89 mm) inside the footprint
        for m in ex.members.iter().filter(|m| m.category != "fascia") {
            for p in corners(m) {
                if p[0] > 0.1 && p[0] < 3.9 && p[2] > 0.1 && p[2] < 3.9 { assert!(p[1] > top - 0.065, "{ty}: {} dips to {} inside", m.key, p[1]); }
            }
        }
        let bearing = ex.members.iter().filter(|m| m.key.ends_with(".bottom") || m.key.starts_with("ceilingJoist")).count();
        assert!(bearing > 0);
        for m in ex.members.iter().filter(|m| m.key.ends_with(".bottom") || m.key.starts_with("ceilingJoist")) { assert!((min_y(m) - top).abs() < E, "{ty}: {} bears at {}", m.key, min_y(m)); }
    }
}

#[test]
fn shed_roofs_stand_on_both_walls() {
    // mono trusses: on both plates, the top chord lands on the bottom chord at the low heel
    let (_, ex, top) = shed(json!({ "shape": "shed", "type": "mono" }));
    let b = get(&ex, "truss.3.bottom");
    assert!((min_y(b) - top).abs() < E);
    let cd = b.r[1][1] - b.r[1][0];
    assert!((under_at_z(get(&ex, "truss.3.top"), 0.0) - (top + cd)).abs() < E);
    // the end post stands over the high wall, under the top chord
    let post = get(&ex, "truss.3.end");
    let c = corners(post);
    assert!(c.iter().all(|p| p[2] >= 4.0 - 0.089 - E && p[2] <= 4.0 + E), "end post over the high wall");
    assert!(c.iter().map(|p| p[1]).fold(f64::MIN, f64::max) <= under_at_z(get(&ex, "truss.3.top"), 4.0 - 0.089) + E, "end post under the top chord");
    // shed rafters: seated on the low wall, and on a pony wall on the high wall
    let (_, ex, top) = shed(json!({ "shape": "shed", "type": "rafters" }));
    let raf = get(&ex, "rafter.3");
    assert!((under_at_z(raf, 0.089) - top).abs() < E, "low seat");
    let plate = get(&ex, "pony.plate.top");
    let ptop = corners(plate).iter().map(|p| p[1]).fold(f64::MIN, f64::max);
    assert!((under_at_z(raf, 4.0) - ptop).abs() < E, "high end on the pony wall: {} vs {ptop}", under_at_z(raf, 4.0));
    let sole = get(&ex, "pony.plate.bottom");
    assert!((min_y(sole) - top).abs() < E, "the pony wall stands on the wall's top plate");
    let stud = get(&ex, "pony.stud.2");
    let sy: Vec<f64> = corners(stud).iter().map(|p| p[1]).collect();
    assert!(sy.iter().cloned().fold(f64::MIN, f64::max) <= ptop - 0.03 && sy.iter().cloned().fold(f64::MAX, f64::min) >= top + 0.03);
}

#[test]
fn gambrel_purlins_stand_on_knee_walls() {
    let (_, ex, top) = shed(json!({ "shape": "gambrel", "type": "rafters" }));
    let purlin = get(&ex, "purlin.l");
    let pbot = min_y(purlin);
    let stud = get(&ex, "kneeWall.l.stud.2");
    let sy: Vec<f64> = corners(stud).iter().map(|p| p[1]).collect();
    assert!((sy.iter().cloned().fold(f64::MIN, f64::max) - pbot).abs() < E, "knee wall up to the purlin");
    let cj = get(&ex, "ceilingJoist.3");
    assert!((min_y(cj) - top).abs() < E);
    // the steep lower rafter is birdsmouthed onto the plate: its underside crosses the plate top over
    // the plate, and the notch (square to the rafter) takes at most a third of its depth
    let lower = get(&ex, "rafter.3.l.lower");
    let kl: f64 = 18.0 / 12.0;
    let cut = top - under_at_z(lower, 0.0);
    assert!(cut > 0.0 && under_at_z(lower, 0.089) >= top - E, "lower rafter seated on the plate (cut {cut})");
    let rd = lower.r[1][1] - lower.r[1][0];
    assert!(cut / (1.0 + kl * kl).sqrt() <= rd / 3.0 + E, "notch {} deeper than a third of {rd}", cut / (1.0 + kl * kl).sqrt());
}

#[test]
fn a_roof_without_framed_walls_sits_on_top_of_the_storey() {
    let mut d = doc();
    let id = fr(&mut d, "add_roof", json!({ "level": 0, "min": [0, 0], "max": [4, 4], "type": "fink", "fascia": { "enabled": false } }));
    let ex = expansion(&d, &id);
    let h = House::read(&d);
    assert!((min_y(get(&ex, "truss.0.bottom")) - (h.base(0) + h.height(0))).abs() < E);
}

