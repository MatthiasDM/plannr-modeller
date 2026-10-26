//! Masonry walls: layers, heights, mitred corners, T-junctions, openings, ties, bricks, pattern, elevation.

use plannr_core::model::{Document, Entity};
use plannr_core::ops::OpResult;
use plannr_core::plugin::Registry;
use plannr_plugin_construction::ConstructionPlugin;
use plannr_plugin_construction::masonry::{data::*, expand::*, wall::*};
use serde_json::{json, Value};

fn registry() -> Registry { let mut r = Registry::new(); r.register(Box::new(ConstructionPlugin)); r }
fn doc() -> Document {
    let mut d = Document::default();
    d.extensions.insert("construction".into(), json!({ "storeys": [{ "id": "s0", "name": "Gelijkvloers", "height": 2.7 }, { "id": "s1", "name": "Verdieping", "height": 2.6 }], "groundLevel": 0.0, "floorThickness": 0.06 }));
    registry().on_load(&mut d);
    d
}
fn ms(doc: &mut Document, op: &str, data: Value) -> OpResult { registry().apply(&format!("masonry.{op}"), data, doc).unwrap() }
fn wall(doc: &mut Document, a: [f64; 2], b: [f64; 2], extra: Value) -> String {
    let mut v = json!({ "a": a, "b": b });
    if let (Value::Object(m), Value::Object(e)) = (&mut v, extra) { m.extend(e); }
    ms(doc, "add_wall", v).created[0].clone()
}
fn custom(doc: &Document, id: &str) -> plannr_core::model::Custom { doc.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.id == id => Some(c.clone()), _ => None }).unwrap() }
fn data(doc: &Document, id: &str) -> Wall { serde_json::from_value(custom(doc, id).data).unwrap() }
fn expansion(doc: &Document, id: &str) -> Expansion { expand_any(&custom(doc, id), &House::read(doc), "l") }
fn built(doc: &Document, id: &str) -> Built { Built::new(id, &data(doc, id), &House::read(doc)) }
/// Bounding box (x, y, z) of the faces with this key.
fn bbox(ex: &Expansion, key: &str) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::MAX; 3]; let mut hi = [f64::MIN; 3];
    for e in &ex.faces { if let Entity::Face(f) = e { if f.id.ends_with(&format!("#{key}")) { for p in &f.outer { for k in 0..3 { lo[k] = lo[k].min(p[k]); hi[k] = hi[k].max(p[k]); } } } } }
    (lo, hi)
}
fn item(ex: &Expansion, cat: &str) -> Vec<Item> { ex.items.iter().filter(|i| i.category == cat).cloned().collect() }
fn shoelace(p: &[[f64; 2]]) -> f64 { let n = p.len(); (0..n).map(|i| p[i][0] * p[(i + 1) % n][1] - p[(i + 1) % n][0] * p[i][1]).sum::<f64>().abs() / 2.0 }
fn close(a: f64, b: f64) -> bool { (a - b).abs() < 1e-6 }

#[test]
fn cavity_layers_sit_at_the_reference_offsets() {
    // exterior = left of a→b = −z for a wall along +x; outer leaf 90, cavity 60, inner 90, plaster 15
    for (reference, outer_z, inner_z) in [("exterior", [0.0, 0.09], [0.15, 0.24]), ("center", [-0.12, -0.03], [0.03, 0.12]), ("interior", [-0.24, -0.15], [-0.09, 0.0])] {
        let mut d = doc();
        let id = wall(&mut d, [0.0, 0.0], [4.0, 0.0], json!({ "reference": reference }));
        let ex = expansion(&d, &id);
        let (lo, hi) = bbox(&ex, "outer");
        assert!(close(lo[2], outer_z[0]) && close(hi[2], outer_z[1]), "{reference} outer {lo:?} {hi:?}");
        let (lo, hi) = bbox(&ex, "inner");
        assert!(close(lo[2], inner_z[0]) && close(hi[2], inner_z[1]), "{reference} inner {lo:?} {hi:?}");
        let (lo, hi) = bbox(&ex, "plaster.int");
        assert!(close(lo[2], inner_z[1]) && close(hi[2], inner_z[1] + 0.015), "{reference} plaster {lo:?} {hi:?}");
    }
}

#[test]
fn outer_leaf_runs_the_storey_and_the_inner_leaf_stops_under_the_floor() {
    let mut d = doc();
    let id = wall(&mut d, [0.0, 0.0], [4.0, 0.0], json!({}));
    let ex = expansion(&d, &id);
    assert!(close(bbox(&ex, "outer").1[1], 2.7));
    assert!(close(bbox(&ex, "inner").1[1], 2.7 - 0.06), "no masonry floor: storey height − construction floor thickness");
    // a beam-and-block floor on the storey above (12+4 → 0.20 deep) covering the inner leaf
    ms(&mut d, "add_floor", json!({ "level": 1, "points": [[-1.0, -1.0], [5.0, -1.0], [5.0, 4.0], [-1.0, 4.0]] }));
    let ex = expansion(&d, &id);
    assert!(close(bbox(&ex, "inner").1[1], 2.5), "{:?}", bbox(&ex, "inner"));
    assert!(close(bbox(&ex, "outer").1[1], 2.7));
    // the top storey goes to its full height
    let up = wall(&mut d, [0.0, 0.0], [4.0, 0.0], json!({ "level": 1 }));
    let ex = expansion(&d, &up);
    assert!(close(bbox(&ex, "inner").1[1] - bbox(&ex, "inner").0[1], 2.6));
}

#[test]
fn a_clockwise_rectangle_mitres_closed() {
    let mut d = doc();
    let pts = [[0.0, 0.0], [6.0, 0.0], [6.0, 4.0], [0.0, 4.0]];
    let ids: Vec<String> = (0..4).map(|i| wall(&mut d, pts[i], pts[(i + 1) % 4], json!({}))).collect();
    let (mut outer, mut inner, mut cavity_plaster) = (0.0, 0.0, 0.0);
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for id in &ids {
        let bt = built(&d, id);
        for l in &bt.layers {
            let q: Vec<[f64; 2]> = bt.quad(l).iter().map(|p| bt.band.at(p[0], p[1])).collect();
            match l.key { "outer" => { outer += shoelace(&q); for p in &q { for k in 0..2 { lo[k] = lo[k].min(p[k]); hi[k] = hi[k].max(p[k]); } } } "inner" => inner += shoelace(&q), "plaster.int" => cavity_plaster += shoelace(&q), _ => {} }
        }
    }
    // the outer leaves exactly fill the 90 mm ring inside the drawn rectangle: no gaps, no overlaps
    assert!((outer - (24.0 - 5.82 * 3.82)).abs() < 1e-6, "outer ring {outer}");
    assert!((inner - (5.7 * 3.7 - 5.52 * 3.52)).abs() < 1e-6, "inner ring {inner}");
    assert!((cavity_plaster - (5.52 * 3.52 - 5.49 * 3.49)).abs() < 1e-6, "plaster ring {cavity_plaster}");
    assert!(close(lo[0], 0.0) && close(lo[1], 0.0) && close(hi[0], 6.0) && close(hi[1], 4.0));
}

#[test]
fn a_partition_is_trimmed_to_the_plaster_of_the_walls_it_meets() {
    let mut d = doc();
    let pts = [[0.0, 0.0], [6.0, 0.0], [6.0, 4.0], [0.0, 4.0]];
    for i in 0..4 { wall(&mut d, pts[i], pts[(i + 1) % 4], json!({})); }
    let p = wall(&mut d, [3.0, 0.0], [3.0, 4.0], json!({ "type": "single", "reference": "center" }));
    let bt = built(&d, &p);
    let c = bt.band.centre();
    assert!((bt.start(c) - 0.255).abs() < 1e-6 && (bt.end(c) - (4.0 - 0.255)).abs() < 1e-6, "{} {}", bt.start(c), bt.end(c));
    let ex = expansion(&d, &p);
    assert!(!item(&ex, "blocks").is_empty(), "a snelbouw partition counts blocks");
}

#[test]
fn a_window_cuts_every_leaf_and_gets_lintels_and_a_sill() {
    let mut d = doc();
    let id = wall(&mut d, [0.0, 0.0], [4.0, 0.0], json!({ "openings": [{ "kind": "window", "offset": 1.4, "width": 1.2, "height": 1.4, "sill": 0.9 }] }));
    let ex = expansion(&d, &id);
    let lintels = item(&ex, "lintels");
    assert_eq!(lintels.len(), 2, "{lintels:?}");
    assert!(lintels.iter().any(|l| l.description.contains("L100")) && lintels.iter().any(|l| l.description.contains("beton")));
    assert!(lintels.iter().all(|l| close(l.length, 1.5)), "width + 2 × 0.15 bearing");
    assert_eq!(item(&ex, "sills").len(), 1);
    let bt = built(&d, &id);
    let col = |role: &str| -> Vec<[f64; 2]> { let l = bt.layers.iter().find(|l| l.key == role).unwrap(); bt.layer_pieces(l).into_iter().filter(|(c, _)| (c[0] + c[1]) / 2.0 > 1.45 && (c[0] + c[1]) / 2.0 < 2.55).map(|(_, y)| y).collect() };
    let outer = col("outer");
    assert!(outer.iter().any(|y| close(y[1], 0.85)), "outer leaf stops under the 50 mm bluestone: {outer:?}");
    assert!(outer.iter().any(|y| close(y[0], 2.3 + 0.008)), "bricks on the steel angle: {outer:?}");
    assert!(!outer.iter().any(|y| y[0] < 2.3 && y[1] > 0.9), "nothing in the opening");
    let inner = col("inner");
    assert!(inner.iter().any(|y| close(y[1], 0.9)) && inner.iter().any(|y| close(y[0], 2.44)), "{inner:?}");
    let (lo, hi) = bbox(&ex, "sill.0");
    assert!(close(hi[1], 0.9) && close(lo[2], -0.03), "sill top at 0.9 and 30 mm proud of the face: {lo:?} {hi:?}");
}

#[test]
fn ties_bricks_and_pattern_counts() {
    let mut d = doc();
    let id = wall(&mut d, [0.0, 0.0], [6.0, 0.0], json!({ "cavity": { "width": 0.06, "insulation": { "kind": "fill", "material": "eps" }, "ties": { "perM2": 5 } } }));
    let ex = expansion(&d, &id);
    let bricks = item(&ex, "bricks");
    assert_eq!(bricks.len(), 1);
    let b = &bricks[0];
    assert!((b.count / b.area - 66.67).abs() < 1.0, "≈ 67 facing bricks per m²: {}", b.count / b.area);
    assert!(close(b.area, 6.0 * 2.7));
    let ties = item(&ex, "ties")[0].count;
    let expect = 5.0 * 6.0 * 2.7;
    assert!((ties - expect).abs() / expect < 0.15, "ties {ties} vs {expect}");
    let ins = item(&ex, "insulation");
    assert!(close(ins[0].volume, 6.0 * 2.7 * 0.06), "a full fill: {:?}", ins);
    assert!(!item(&ex, "mortar").is_empty() && !item(&ex, "dpc").is_empty() && !item(&ex, "plaster").is_empty());
    let edges = ex.faces.iter().filter(|e| matches!(e, Entity::Edge(x) if x.id.ends_with("#pattern"))).count() as f64;
    assert!(edges > 0.0 && edges < 4.0 * b.count, "{edges} pattern edges for {} bricks", b.count);
    // a plastered snelbouw partition carries no pattern
    let p = wall(&mut d, [10.0, 0.0], [12.0, 0.0], json!({ "type": "single", "plaster": { "interior": true, "exterior": true } }));
    assert_eq!(expansion(&d, &p).faces.iter().filter(|e| matches!(e, Entity::Edge(_))).count(), 0);
}

#[test]
fn the_elevation_is_an_svg_with_the_openings() {
    let mut d = doc();
    let id = wall(&mut d, [0.0, 0.0], [4.0, 0.0], json!({ "openings": [{ "kind": "window", "offset": 1.4, "width": 1.2, "height": 1.4, "sill": 0.9 }, { "kind": "door", "offset": 0.3, "width": 0.9, "height": 2.1 }] }));
    let svg = wall_svg(&id, &data(&d, &id), &House::read(&d));
    assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
    assert_eq!(svg.matches("<g").count(), svg.matches("</g>").count());
    assert_eq!(svg.matches("<text").count(), svg.matches("</text>").count());
    assert!(svg.contains("W2: 1.20 × 1.40, sill 0.90") || svg.contains("W1: 1.20 × 1.40, sill 0.90"), "{}", &svg[..svg.len().min(400)]);
    assert!(svg.contains("D1: 0.90 × 2.10"));
    assert!(svg.contains("Gevelsteen 190×90×65") && svg.contains("class=\"lintel\"") && svg.contains("class=\"sill\""));
    // through the op as well
    let r = ms(&mut d, "wall_svg", json!({ "wall": id }));
    match &r.entities[0] { Entity::Custom(c) => assert!(c.data["svg"].as_str().unwrap().contains("<line")), _ => panic!() }
}
