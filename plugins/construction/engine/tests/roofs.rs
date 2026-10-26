//! Construction roofs on cavity-wall houses: the roof lands on the outer faces of the walls, its
//! plates bear on the walls, the gables rise in the wall's own build-up up to the rafters, the
//! eaves' outer leaf stops under them, nothing of the masonry goes through the roof, and a lower
//! roof stops at the taller part of the building.

use plannr_core::model::{Document, Entity};
use plannr_core::ops::OpResult;
use plannr_core::plugin::Registry;
use plannr_plugin_construction::masonry::data::House;
use plannr_plugin_construction::masonry::wall::{expand_wall, Built, Role};
use plannr_plugin_construction::roof::RoofGeom;
use plannr_plugin_construction::ConstructionPlugin;
use serde_json::{json, Value};

fn registry() -> Registry { let mut r = Registry::new(); r.register(Box::new(ConstructionPlugin)); r }
fn ap(doc: &mut Document, t: &str, data: Value) -> OpResult { registry().apply(t, data, doc).unwrap_or_else(|e| panic!("{t}: {e:?}")) }
const E: f64 = 1e-3;

/// A 6 × 4 m one-storey cavity-wall house (two rooms) with a roof.
fn house(roof: Value) -> (Document, String) {
    let mut d = Document::default(); registry().on_load(&mut d);
    ap(&mut d, "construction.setup", json!({ "storeys": [{ "name": "Gelijkvloers", "height": 2.6 }], "cavity": { "enabled": true, "innerLeaf": 0.09, "cavity": 0.06, "outerLeaf": 0.09 } }));
    ap(&mut d, "construction.add_room", json!({ "name": "Living", "level": 0, "origin": [0, 0], "size": [4, 4] }));
    ap(&mut d, "construction.add_room", json!({ "name": "Kitchen", "level": 0, "origin": [4, 0], "size": [2, 4] }));
    ap(&mut d, "masonry.set_defaults", json!({ "cavity": { "width": 0.06, "insulation": { "kind": "fill", "material": "eps", "thickness": 0 } } }));
    let mut v = json!({ "style": "gable", "pitchHeight": 1.6, "overhang": 0.3, "ridgeAlongX": true });
    if let (Value::Object(a), Value::Object(b)) = (&mut v, roof) { a.extend(b); }
    let id = ap(&mut d, "construction.add_roof", v).created[0].clone();
    (d, id)
}
fn geom(h: &House, id: &str) -> RoofGeom { h.roofs.iter().find(|g| g.id == id).cloned().expect("roof geometry") }
fn ys(faces: &[Entity]) -> impl Iterator<Item = [f64; 3]> + '_ { faces.iter().filter_map(|e| match e { Entity::Face(f) => Some(f.outer.clone()), _ => None }).flatten() }
fn roof_faces(d: &Document, id: &str) -> Vec<Entity> {
    let c = d.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.id == id => Some(c.clone()), _ => None }).unwrap();
    let m = plannr_plugin_construction::expand::model(d);
    let set = plannr_plugin_construction::expand::collect(&m);
    plannr_plugin_construction::expand::expand_entity(&c, &m, &set)
}
fn face_ids<'a>(faces: &'a [Entity], prefix: &'a str) -> impl Iterator<Item = &'a plannr_core::model::Face> + 'a {
    faces.iter().filter_map(move |e| match e { Entity::Face(f) if f.id.starts_with(prefix) => Some(f), _ => None })
}

#[test]
fn the_roof_lands_on_the_outer_wall_faces_and_its_plates_on_the_walls() {
    let (d, id) = house(json!({}));
    let h = House::read(&d);
    let g = geom(&h, &id);
    assert!((g.u[0] + 0.12).abs() < E && (g.u[1] - 6.12).abs() < E && (g.v[0] + 0.12).abs() < E && (g.v[1] - 4.12).abs() < E, "footprint {:?} {:?}", g.u, g.v);
    assert!((g.wall_top - 2.6).abs() < E);
    // the plate sits on the walls, the rafter's underside meets the plate top over the seat
    let faces = roof_faces(&d, &id);
    let timber_min = face_ids(&faces, &format!("{id}#timber")).flat_map(|f| f.outer.iter()).map(|p| p[1]).fold(f64::MAX, f64::min);
    assert!(timber_min < g.wall_top - 0.05, "the rafter tails run down past the wall top");
    // the plates: timber standing exactly on the wall top
    assert!(face_ids(&faces, &format!("{id}#timber")).any(|f| f.outer.iter().all(|p| (p[1] - g.wall_top).abs() < E)), "a plate bottom on the wall top");
    let plate_top = g.wall_top + g.ph;
    assert!((g.y0 + g.k * (g.d0 + g.seat) - plate_top).abs() < E, "birdsmouth seat on the plate");
    // the notch stays within a third of the rafter
    assert!(g.k * g.seat / (1.0 + g.k * g.k).sqrt() <= 0.175 / 3.0 + E);
}

#[test]
fn gables_rise_in_the_wall_build_up_to_the_rafters_and_the_eaves_stop_under_them() {
    let (d, id) = house(json!({}));
    let h = House::read(&d);
    let g = geom(&h, &id);
    let mut gables = 0; let mut eaves = 0;
    for (wid, w) in h.walls.iter().filter(|(_, w)| w.type_ == "cavity") {
        let bt = Built::new(wid, w, &h);
        let outer = bt.layers.iter().find(|l| l.role == Role::Outer).unwrap();
        let inner = bt.layers.iter().find(|l| l.role == Role::Inner).unwrap();
        let ridge_under = g.y0 + g.k * g.span() / 2.0;
        let along_x = (w.b[1] - w.a[1]).abs() < 1e-6;
        if !along_x {
            // a gable: every leaf peaks at the rafter underside over the ridge
            gables += 1;
            let peak = outer.top.iter().map(|p| p[1]).fold(f64::MIN, f64::max);
            assert!((peak - ridge_under).abs() < 0.01, "outer leaf peak {peak} vs {ridge_under}");
            let ipeak = inner.top.iter().map(|p| p[1]).fold(f64::MIN, f64::max);
            assert!((ipeak - ridge_under).abs() < 0.01, "inner leaf peak {ipeak}");
            for l in bt.layers.iter().filter(|l| l.role == Role::Cavity) { assert!(!l.top.is_empty(), "the cavity fill rises into the gable"); }
        } else {
            // an eaves wall: the outer leaf stops under the rafters at its outer face, the inner leaf carries the plate
            eaves += 1;
            assert!(outer.top.is_empty() && (outer.h - g.y0).abs() < E, "eaves outer leaf {} vs {}", outer.h, g.y0);
            assert!((inner.h - g.wall_top).abs() < E && inner.top.is_empty());
        }
    }
    assert_eq!((gables, eaves), (2, 4), "two gables, the eaves walls in a piece per room");
}

fn no_masonry_through(d: &Document, id: &str) {
    let h = House::read(d);
    let g = geom(&h, id);
    let mut n = 0;
    for (wid, w) in &h.walls {
        let ex = expand_wall(wid, w, &h, "");
        for p in ys(&ex.faces) {
            n += 1;
            assert!(p[1] <= g.under([p[0], p[2]]) + 2e-3, "{:?} wall {wid} at {:?} is above the roof underside {}", g.style, p, g.under([p[0], p[2]]));
        }
    }
    assert!(n > 0);
}

#[test]
fn no_masonry_goes_through_any_roof() {
    for style in ["gable", "hip", "shed", "flat"] {
        let (d, id) = house(json!({ "style": style }));
        no_masonry_through(&d, &id);
    }
}

#[test]
fn a_shed_roof_raises_its_high_wall_to_the_plate() {
    let (d, id) = house(json!({ "style": "shed" }));
    let h = House::read(&d);
    let g = geom(&h, &id);
    let high = g.edges[1].plate.unwrap();
    assert!(high.bottom > g.wall_top + 1.0, "the high plate is up the slope");
    let (wid, w) = h.walls.iter().find(|(_, w)| (w.a[1] - 4.0).abs() < 1e-6 && (w.b[1] - 4.0).abs() < 1e-6 && w.type_ == "cavity").unwrap();
    let bt = Built::new(wid, w, &h);
    let inner = bt.layers.iter().find(|l| l.role == Role::Inner).unwrap();
    assert!((inner.h - high.bottom).abs() < E, "inner leaf {} up to the plate {}", inner.h, high.bottom);
}

#[test]
fn a_lower_roof_stops_at_the_taller_part_of_the_building() {
    // a two-storey 4 × 4 house with a one-storey annex 3 m wide along its side, roofed over its width + 1 m under the house
    let mut d = Document::default(); registry().on_load(&mut d);
    ap(&mut d, "construction.setup", json!({ "storeys": [{ "name": "G", "height": 2.6 }, { "name": "V", "height": 2.5 }], "cavity": { "enabled": true, "innerLeaf": 0.09, "cavity": 0.06, "outerLeaf": 0.09 } }));
    ap(&mut d, "construction.add_room", json!({ "name": "House", "level": 0, "origin": [3, 0], "size": [4, 4] }));
    ap(&mut d, "construction.add_room", json!({ "name": "Upstairs", "level": 1, "origin": [3, 0], "size": [4, 4] }));
    ap(&mut d, "construction.add_room", json!({ "name": "Annex", "level": 0, "origin": [0, 0], "size": [3, 4] }));
    let id = ap(&mut d, "construction.add_roof", json!({ "style": "gable", "pitchHeight": 1.2, "levels": 1, "min": [-0.12, -0.12], "max": [4.0, 4.12], "ridgeAlongX": true })).created[0].clone();
    let faces = roof_faces(&d, &id);
    let upstairs = (2.88 + 0.01, 7.12);
    let inside = face_ids(&faces, &id).flat_map(|f| f.outer.iter()).filter(|p| p[0] > upstairs.0 && p[2] > -0.11 && p[2] < 4.11).count();
    assert_eq!(inside, 0, "nothing of the annex roof inside the upper storey");
    assert!(face_ids(&faces, &format!("{id}#roof")).count() > 0);
}

#[test]
fn the_takeoff_lists_the_roof() {
    let (d, _) = house(json!({}));
    let t = plannr_plugin_construction::masonry::takeoff::estimate(&d, None);
    let items = t["items"].as_array().unwrap();
    let find = |desc: &str| items.iter().find(|i| i["description"].as_str().unwrap_or("").starts_with(desc)).unwrap_or_else(|| panic!("no {desc} in {items:?}"));
    assert!(find("Keper")["count"].as_u64().unwrap() >= 30);
    assert!(find("Muurplaat")["length_m"].as_f64().unwrap() > 11.0);
    assert!(find("Nokgording")["length_m"].as_f64().unwrap() > 6.0);
    let tiles = find("Dakpannen")["area_m2"].as_f64().unwrap();
    assert!(tiles > 2.0 * 6.6 * 2.5 && tiles < 2.0 * 6.9 * 3.2, "tiles {tiles} m²");
}

#[test]
fn a_party_wall_end_has_no_verge_and_a_lower_room_carries_its_roof_lower() {
    let (mut d, id) = house(json!({ "party": ["max"], "overhang": 0.3 }));
    let faces = roof_faces(&d, &id);
    let h = House::read(&d);
    let g = geom(&h, &id);
    let xmax = face_ids(&faces, &id).flat_map(|f| f.outer.iter()).map(|p| p[0]).fold(f64::MIN, f64::max);
    let xmin = face_ids(&faces, &id).flat_map(|f| f.outer.iter()).map(|p| p[0]).fold(f64::MAX, f64::min);
    assert!(xmax <= g.u[1] + 1e-3, "nothing past the party wall: {xmax} vs {}", g.u[1]);
    assert!(xmin < g.u[0] - 0.25, "the other gable keeps its verge: {xmin}");
    // both rooms lowered to 2.3 m: the walls stop there and the roof sits on them
    let rooms: Vec<String> = d.entities.iter().filter_map(|e| match e { Entity::Custom(c) if c.type_ == "construction.room" => Some(c.id.clone()), _ => None }).collect();
    for r in rooms { ap(&mut d, "construction.update", json!({ "id": r, "patch": { "wallHeight": 2.3 } })); }
    let h = House::read(&d);
    let g = geom(&h, &id);
    assert!((g.wall_top - 2.3).abs() < E, "wall top {}", g.wall_top);
    no_masonry_through(&d, &id);
    let (wid, w) = h.walls.iter().find(|(_, w)| (w.a[1] - w.b[1]).abs() < 1e-6 && w.type_ == "cavity").unwrap();
    let bt = Built::new(wid, w, &h);
    let inner = bt.layers.iter().find(|l| l.role == Role::Inner).unwrap();
    assert!((inner.h - 2.3).abs() < E, "eaves inner leaf {}", inner.h);
}
