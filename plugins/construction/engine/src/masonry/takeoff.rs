//! Material takeoff (aggregated `Item`s of every element), summary and metrics.

use std::collections::BTreeMap;

use plannr_core::model::{Document, Entity};
use serde_json::{json, Value};

use crate::masonry::data::*;
use crate::masonry::expand::*;

#[derive(Default, Clone)]
struct Agg { count: f64, length: f64, area: f64, volume: f64 }
impl Agg { fn add(&mut self, i: &Item) { self.count += i.count; self.length += i.length; self.area += i.area; self.volume += i.volume; } }

fn r2(x: f64) -> f64 { (x * 100.0).round() / 100.0 }
fn r3(x: f64) -> f64 { (x * 1000.0).round() / 1000.0 }

fn item_json(cat: &str, desc: &str, a: &Agg) -> Value {
    let mut v = json!({ "category": cat, "description": desc });
    if a.count > 0.0 { v["count"] = json!(a.count.ceil() as u64); }
    if a.length > 0.0 { v["length_m"] = json!(r2(a.length)); }
    if a.area > 0.0 { v["area_m2"] = json!(r2(a.area)); }
    if a.volume > 0.0 { v["volume_m3"] = json!(r3(a.volume)); }
    v
}

/// Every masonry element with its expansion: the walls and floors (stored and derived from the
/// construction model) and the foundations.
pub fn expansions(doc: &Document) -> Vec<(String, String, Expansion)> {
    let house = House::read(doc);
    let mut out: Vec<(String, String, Expansion)> = vec![];
    for (id, w) in &house.walls { out.push((id.clone(), T_WALL.to_string(), crate::masonry::wall::expand_wall(id, w, &house, ""))); }
    for (id, f) in &house.floors { out.push((id.clone(), T_FLOOR.to_string(), crate::masonry::floor::expand_floor(id, f, &house, ""))); }
    for e in &doc.entities {
        if let Entity::Custom(c) = e { if c.type_ == T_FOUNDATION { out.push((c.id.clone(), c.type_.clone(), expand_any(c, &house, ""))); } }
    }
    // the roofs: their timber, covering and boards
    let m = crate::expand::model(doc);
    let set = crate::expand::collect(&m);
    for (c, r) in &m.roofs {
        let Some(g) = crate::roof::geom(&m, &set, &c.id, r) else { continue };
        let mut f = crate::expand::Faces::new("");
        let items = crate::roof::build(&mut f, &g, r, &c.id);
        out.push((c.id.clone(), crate::data::T_ROOF.to_string(), Expansion { faces: vec![], items }));
    }
    out
}

pub fn estimate(doc: &Document, only: Option<&[String]>) -> Value {
    let mut items: BTreeMap<(String, String), Agg> = BTreeMap::new();
    let mut by_element = vec![];
    for (id, type_, ex) in expansions(doc) {
        if let Some(o) = only { if !o.iter().any(|x| x == &id) { continue; } }
        let mut local: BTreeMap<(String, String), Agg> = BTreeMap::new();
        for i in &ex.items {
            let k = (i.category.to_string(), i.description.clone());
            items.entry(k.clone()).or_default().add(i);
            local.entry(k).or_default().add(i);
        }
        by_element.push(json!({ "id": id, "type": type_, "items": local.iter().map(|((c, d), a)| item_json(c, d, a)).collect::<Vec<_>>() }));
    }
    let sum = |cat: &str, f: fn(&Agg) -> f64| items.iter().filter(|((c, _), _)| c == cat).map(|(_, a)| f(a)).sum::<f64>();
    let count = |a: &Agg| a.count; let vol = |a: &Agg| a.volume; let area = |a: &Agg| a.area; let len = |a: &Agg| a.length;
    let bricks = sum("bricks", count); let blocks = sum("blocks", count);
    let concrete = sum("topping", vol) + sum("ringBeam", vol) + sum("slab", vol) + sum("footing", vol);
    json!({
        "items": items.iter().map(|((c, d), a)| item_json(c, d, a)).collect::<Vec<_>>(),
        "totals": {
            "bricks": bricks.ceil() as u64, "bricksWithWaste": (bricks * 1.05).ceil() as u64,
            "blocks": blocks.ceil() as u64, "blocksWithWaste": (blocks * 1.05).ceil() as u64,
            "mortar_m3": r3(sum("mortar", vol)), "insulation_m2": r2(sum("insulation", area).max(0.0)), "insulation_m3": r3(sum("insulation", vol).max(0.0)),
            "ties": sum("ties", count).ceil() as u64, "lintels": sum("lintels", count).ceil() as u64, "plaster_m2": r2(sum("plaster", area)),
            "joists_m": r2(sum("joists", len) + sum("trimmers", len)), "floorBlocks": sum("floorBlocks", count).ceil() as u64, "planks": sum("planks", count).ceil() as u64,
            "concrete_m3": r3(concrete), "mesh_m2": r2(sum("mesh", area)), "foundationBlocks": sum("foundationBlocks", count).ceil() as u64,
        },
        "wasteFactor": 1.05,
        "byElement": by_element,
    })
}

pub fn summary(doc: &Document) -> Value {
    let h = House::read(doc);
    let d = &h.defaults;
    let walls: Vec<Value> = h.walls.iter().map(|(id, w)| {
        let mut leaves = vec![json!({ "role": if w.is_cavity() { "inner" } else { "single" }, "unit": w.inner_unit(d).name, "width": w.inner_unit(d).width })];
        if w.is_cavity() { leaves.push(json!({ "role": "outer", "unit": w.outer_unit(d).name, "width": w.outer_unit(d).width })); }
        json!({ "id": id, "level": w.level, "type": w.type_, "length_m": r2(w.length()), "thickness_m": r3(w.thickness(d)), "openings": w.openings.len(), "leaves": leaves,
                "cavity": if w.is_cavity() { serde_json::to_value(w.cavity_of(d)).unwrap() } else { Value::Null }, "source": w.source })
    }).collect();
    let floors: Vec<Value> = h.floors.iter().map(|(id, f)| {
        let s = f.spec(&d.floor);
        json!({ "id": id, "level": f.level, "system": s.system, "area_m2": r2(plan_area(&f.points).abs()), "depth_m": r3(s.depth()), "spacing": s.spacing, "openings": f.openings.len() })
    }).collect();
    let foundations: Vec<Value> = doc.entities.iter().filter_map(|e| match e { Entity::Custom(c) if c.type_ == T_FOUNDATION => Some(json!({ "id": c.id })), _ => None }).collect();
    json!({ "defaults": d, "walls": walls, "floors": floors, "foundations": foundations, "totals": estimate(doc, None)["totals"] })
}

pub fn metrics(doc: &Document) -> Value {
    let t = estimate(doc, None)["totals"].clone();
    let h = House::read(doc);
    json!({ "walls": h.walls.len(), "floors": h.floors.len(), "bricks": t["bricks"], "blocks": t["blocks"], "concrete_m3": t["concrete_m3"] })
}
