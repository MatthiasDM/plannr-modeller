//! The construction model *is* masonry: every piece of the construction wall graph becomes a masonry
//! wall (exterior pieces cavity walls with the masonry build-up, others single leaves) and every upper
//! storey a masonry floor over the outline of its rooms — derived on the fly, no entities needed.
//! An explicit `masonry.wall` / `masonry.floor` (made with "Customise" / `import_construction`, or
//! drawn by hand) replaces the derived one for its piece / storey.

use plannr_core::model::{Custom, Document, Entity};
use serde_json::{json, Value};

use crate::data::{OpeningKind, Room, Settings as CSettings};
use crate::expand::{is_detailed, Model, WallEntry, WallSet};
use crate::masonry::data::*;
use crate::masonry::import::{offset_polygon, union_outline};

/// "Snelbouwsteen 288×70×138" for a unit whose width was changed to the model's.
pub fn unit_name(u: &Unit) -> String {
    let label = match u.kind.as_str() { "facingBrick" => "Gevelsteen", "brick" => "Baksteen", "clayBlock" => "Snelbouwsteen", "concreteBlock" => "Betonblok", "aeratedBlock" => "Cellenbeton", "limeSandBlock" => "Kalkzandsteen", _ => "Steen" };
    format!("{label} {:.0}×{:.0}×{:.0}", u.length * 1000.0, u.width * 1000.0, u.height * 1000.0)
}

const BRICKS: [&str; 3] = ["construction.brick", "construction.brickBrown", "construction.brickBuff"];

/// The masonry wall for one construction wall piece (centre line, exterior away from its room).
pub fn piece_wall(w: &WallEntry, cs: &CSettings, d: &Defaults) -> Option<Wall> {
    let l = w.length(); if l < 0.05 { return None; }
    let u = [(w.b[0] - w.a[0]) / l, (w.b[1] - w.a[1]) / l];
    let left = [u[1], -u[0]];
    let flip = left[0] * w.out[0] + left[1] * w.out[1] < 0.0;
    let cavity = w.exterior && w.thickness.is_none() && cs.cavity.enabled;
    let mut v = json!({ "level": w.level, "a": w.a, "b": w.b, "reference": "center", "flip": flip, "plaster": d.plaster, "dpc": d.dpc,
                        "source": { "owner": w.owner, "key": w.key } });
    if let Some(r) = w.rooms.first() { v["room"] = json!(r); }
    // a room lower than its storey (its own wall height): the wall stops there
    if w.height < cs.height(w.level) - 0.005 { v["height"] = json!(w.height); v["outerHeight"] = json!(w.height); }
    // the room's paint goes on the plaster; a non-brick outside finish becomes a render
    if w.interior != "construction.plaster" { v["finishInterior"] = json!(w.interior); }
    if cavity {
        let mut outer = d.outer.clone(); if (outer.width - cs.cavity.outer_leaf).abs() > 0.0025 { outer.width = cs.cavity.outer_leaf; outer.name = unit_name(&outer); }
        let mut inner = d.inner.clone(); if (inner.width - cs.cavity.inner_leaf).abs() > 0.0025 { inner.width = cs.cavity.inner_leaf; inner.name = unit_name(&inner); }
        let mut cav = d.cavity.clone(); cav.width = cs.cavity.cavity;
        v["type"] = json!("cavity"); v["outer"] = json!(outer); v["inner"] = json!(inner); v["cavity"] = json!(cav);
        if BRICKS.contains(&w.outside.as_str()) { if w.outside != "construction.brick" { v["outerMaterial"] = json!(w.outside); } }
        else { v["plaster"]["exterior"] = json!(true); v["finishExterior"] = json!(w.outside); }
    } else {
        let mut p = d.partition.clone(); let width = w.thickness.unwrap_or(cs.wall_thickness);
        if (p.width - width).abs() > 1e-6 { p.width = width; p.name = unit_name(&p); }
        v["type"] = json!("single"); v["inner"] = json!(p);
        // a partition is plastered on both sides; a free exterior wall only inside
        v["plaster"]["exterior"] = json!(!w.exterior);
        if !w.exterior && w.outside != "construction.plaster" && !BRICKS.contains(&w.outside.as_str()) { v["finishExterior"] = json!(w.outside); }
    }
    let ops: Vec<Value> = w.openings.iter().map(|o| {
        let window = matches!(o.kind, OpeningKind::Window);
        let offset = (o.along - o.width / 2.0).max(0.0);
        json!({ "id": o.id, "kind": if window { "window" } else { "door" }, "offset": offset, "width": o.width.min(l - offset), "height": o.height, "sill": if window { o.sill } else { 0.0 }, "source": o.id })
    }).collect();
    v["openings"] = json!(ops);
    serde_json::from_value(v).ok()
}

/// Masonry walls for every construction wall piece no explicit masonry wall replaces. Ids are
/// `<owner>#<faceKey>` (`~k` for a second piece of the same edge), so their faces select the room.
pub fn derived_walls(m: &Model, set: &WallSet, d: &Defaults) -> Vec<(String, Wall)> {
    let mut out: Vec<(String, Wall)> = vec![];
    for w in set.walls.iter().filter(|w| !is_detailed(w, &m.detailed_walls)) {
        let Some(mw) = piece_wall(w, &m.settings, d) else { continue };
        let base = format!("{}#{}", w.owner, w.key);
        let n = out.iter().filter(|(id, _)| id == &base || id.starts_with(&format!("{base}~"))).count();
        out.push((if n == 0 { base } else { format!("{base}~{n}") }, mw));
    }
    out
}

/// A derived storey floor: its id (`<carrier room>#mfloor<k>`), the room whose expansion draws it, and the floor.
#[derive(Debug, Clone)]
pub struct DerivedFloor { pub id: String, pub carrier: String, pub floor: Floor }

/// The structural floor of every upper storey without an explicit masonry floor: over the union of
/// its rooms, inset to the cavity faces of the inner leaves, stair wells as openings.
pub fn derived_floors(m: &Model, d: &Defaults, explicit_levels: &[usize]) -> Vec<DerivedFloor> {
    let cs = &m.settings;
    let mut out = vec![];
    let max_level = m.rooms.iter().map(|(_, r)| r.level).max().unwrap_or(0);
    for level in 1..=max_level {
        if explicit_levels.contains(&level) { continue; }
        let rooms: Vec<&(Custom, Room)> = m.rooms.iter().filter(|(_, r)| r.level == level && r.has_floor).collect();
        if rooms.is_empty() { continue; }
        let polys: Vec<Vec<P2>> = rooms.iter().map(|(_, r)| r.points.clone()).collect();
        let inset = if cs.cavity.enabled { cs.cavity.total() / 2.0 - cs.cavity.inner_leaf } else { -cs.wall_thickness / 2.0 };
        let bearing = if cs.cavity.enabled { cs.cavity.inner_leaf } else { cs.wall_thickness / 2.0 };
        let wells: Vec<Vec<P2>> = rooms.iter().flat_map(|(_, r)| crate::expand::stair_wells(m, r)).collect();
        for (k, outline) in union_outline(&polys).into_iter().enumerate() {
            let pts = offset_polygon(&outline, -inset);
            if pts.len() < 3 || plan_area(&pts).abs() < 0.5 { continue; }
            let holes: Vec<Vec<P2>> = wells.iter().filter(|h| h.iter().all(|p| point_in_poly(*p, &pts))).cloned().collect();
            let inside: Vec<&&(Custom, Room)> = rooms.iter().filter(|(_, r)| point_in_poly(r.centroid(), &pts)).collect();
            let Some(carrier) = inside.first() else { continue };
            let mut fl: Floor = serde_json::from_value(json!({ "level": level, "points": pts, "system": d.floor.system, "bearing": bearing, "openings": holes })).unwrap();
            fl.rooms = inside.iter().map(|(c, _)| c.id.clone()).collect();
            out.push(DerivedFloor { id: format!("{}#mfloor{k}", carrier.0.id), carrier: carrier.0.id.clone(), floor: fl });
        }
    }
    out
}

/// The masonry walls and floors written as entities in the document.
pub fn explicit(doc: &Document) -> (Vec<(String, Wall)>, Vec<(String, Floor)>) {
    let (mut walls, mut floors) = (vec![], vec![]);
    for e in &doc.entities {
        let Entity::Custom(c) = e else { continue };
        match c.type_.as_str() {
            T_WALL => if let Ok(w) = serde_json::from_value::<Wall>(c.data.clone()) { walls.push((c.id.clone(), w)); },
            T_FLOOR => if let Ok(f) = serde_json::from_value::<Floor>(c.data.clone()) { floors.push((c.id.clone(), f)); },
            _ => {}
        }
    }
    (walls, floors)
}
