//! Ops of the framing plugin, the material takeoff and metrics.

use plannr_core::model::{Custom, Document, Entity};
use plannr_core::ops::{OpError, OpResult};
use serde_json::{json, Map, Value};

use crate::data::*;
use crate::expand::*;

fn err<T>(m: impl Into<String>) -> Result<T, OpError> { Err(OpError(m.into())) }
fn st(v: &Value, k: &str) -> Option<String> { v.get(k).and_then(|x| x.as_str()).map(|x| x.to_string()) }
fn num(v: &Value, k: &str) -> Option<f64> { v.get(k).and_then(|x| x.as_f64()) }
fn p2s(v: Option<&Value>) -> Vec<P2> { v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|p| p2(Some(p))).collect()).unwrap_or_default() }
fn data_result(name: &str, v: Value) -> OpResult {
    OpResult { entities: vec![Entity::Custom(Custom { id: name.into(), layer: String::new(), type_: format!("framing.{name}"), data: v, extra: Map::new() })], ..Default::default() }
}
fn deep_merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => { for (k, v) in p { match b.get_mut(k) { Some(bv) if bv.is_object() && v.is_object() => deep_merge(bv, v), _ => { b.insert(k.clone(), v.clone()); } } } }
        (b, p) => *b = p.clone(),
    }
}
/// `points` or `origin` + `size` → a plan polygon.
fn polygon_of(data: &Value) -> Result<Vec<P2>, OpError> {
    let pts = p2s(data.get("points"));
    if pts.len() >= 3 { return Ok(pts); }
    if let (Some(o), Some(s)) = (p2(data.get("origin")), p2(data.get("size"))) { return Ok(vec![o, [o[0] + s[0], o[1]], [o[0] + s[0], o[1] + s[1]], [o[0], o[1] + s[1]]]); }
    err("give `points` (≥ 3) or `origin` + `size`")
}

/// Every framing entity (id, type, data).
pub fn elements(doc: &Document) -> Vec<(String, String, Value)> {
    doc.entities.iter().filter_map(|e| match e { Entity::Custom(c) if TYPES.contains(&c.type_.as_str()) => Some((c.id.clone(), c.type_.clone(), c.data.clone())), _ => None }).collect()
}
fn find_custom<'a>(doc: &'a Document, id: &str) -> Option<&'a Custom> {
    doc.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.id == id && TYPES.contains(&c.type_.as_str()) => Some(c), _ => None })
}
fn push_entity(doc: &mut Document, type_: &str, data: Value) -> Result<OpResult, OpError> {
    let layer = framing_layer(doc);
    let id = new_id();
    let c = Custom { id: id.clone(), layer, type_: type_.into(), data, extra: Map::new() };
    doc.entities.push(Entity::Custom(c.clone()));
    Ok(OpResult { created: vec![id], entities: vec![Entity::Custom(c)], ..Default::default() })
}
fn wall_from(data: &Value, defaults: &Defaults) -> Result<Wall, OpError> {
    // new walls are drawn along their exterior face and join their neighbours with California corners
    let mut v = json!({ "studSize": defaults.stud_size, "studSpacing": defaults.stud_spacing, "plateSize": defaults.plate_size, "topPlates": defaults.top_plates, "bottomPlates": defaults.bottom_plates, "sheathing": defaults.sheathing,
                        "reference": "exterior", "cornerDetail": defaults.corner_detail, "tDetail": defaults.t_detail });
    deep_merge(&mut v, data);
    let w: Wall = serde_json::from_value(v).map_err(|e| OpError(format!("wall: {e}")))?;
    if w.length() < 0.05 { return err("wall is too short"); }
    validate_wall(&w)?;
    if lumber(&w.stud_size).is_none() { return err(format!("unknown stud size `{}`", w.stud_size)); }
    Ok(w)
}

fn validate_wall(w: &Wall) -> Result<(), OpError> {
    if !["exterior", "center", "interior"].contains(&w.reference.as_str()) { return err("reference must be exterior, center or interior"); }
    if !["none", "twoStud", "california", "threeStud"].contains(&w.corner_detail.as_str()) { return err("cornerDetail must be none, twoStud, california or threeStud"); }
    if !["none", "twoStud", "ladder", "california"].contains(&w.t_detail.as_str()) { return err("tDetail must be none, twoStud, ladder or california"); }
    for j in [&w.start_join, &w.end_join] { if !["auto", "through", "butt", "none"].contains(&j.as_str()) { return err("startJoin / endJoin must be auto, through, butt or none"); } }
    Ok(())
}

pub fn apply(name: &str, data: Value, doc: &mut Document) -> Result<OpResult, OpError> {
    ensure(doc);
    match name {
        "set_defaults" => {
            let mut s = settings(doc);
            let mut v = serde_json::to_value(&s.defaults).unwrap(); deep_merge(&mut v, &data);
            s.defaults = serde_json::from_value(v).map_err(|e| OpError(format!("defaults: {e}")))?;
            if lumber(&s.defaults.stud_size).is_none() { return err(format!("unknown stud size `{}`", s.defaults.stud_size)); }
            if let Some(sp) = s.defaults.extra.get("species").and_then(|v| v.as_str()) { if crate::engineering::species(sp).is_none() { return err(format!("unknown species `{sp}` (use {})", crate::engineering::SPECIES.join(", "))); } }
            save_settings(doc, &s);
            Ok(data_result("defaults", serde_json::to_value(&s.defaults).unwrap()))
        }
        "add_wall" => {
            let d = settings(doc).defaults;
            let w = wall_from(&data, &d)?;
            push_entity(doc, T_WALL, serde_json::to_value(w).unwrap())
        }
        "add_opening" => {
            let wid = st(&data, "wall").ok_or_else(|| OpError("wall is required".into()))?;
            let c = find_custom(doc, &wid).filter(|c| c.type_ == T_WALL).ok_or_else(|| OpError(format!("no framing wall {wid}")))?;
            let mut w: Wall = serde_json::from_value(c.data.clone()).map_err(|e| OpError(e.to_string()))?;
            let mut ov = data.clone(); if let Value::Object(m) = &mut ov { m.remove("wall"); if !m.contains_key("kind") { m.insert("kind".into(), json!("window")); } }
            let o: Opening = serde_json::from_value(ov).map_err(|e| OpError(format!("opening: {e}")))?;
            if o.width <= 0.05 || o.height <= 0.05 { return err("opening needs width and height"); }
            if o.offset < 0.0 || o.offset + o.width > w.length() + 1e-6 { return err(format!("opening does not fit the wall (length {:.2} m)", w.length())); }
            let oid = o.id.clone(); w.openings.push(o);
            set_data(doc, &wid, serde_json::to_value(w).unwrap());
            Ok(OpResult { created: vec![oid], modified: vec![wid], ..Default::default() })
        }
        "remove_opening" => {
            let wid = st(&data, "wall").ok_or_else(|| OpError("wall is required".into()))?; let oid = st(&data, "opening").ok_or_else(|| OpError("opening is required".into()))?;
            let c = find_custom(doc, &wid).filter(|c| c.type_ == T_WALL).ok_or_else(|| OpError(format!("no framing wall {wid}")))?;
            let mut w: Wall = serde_json::from_value(c.data.clone()).map_err(|e| OpError(e.to_string()))?;
            let n = w.openings.len(); w.openings.retain(|o| o.id != oid); if w.openings.len() == n { return err(format!("no opening {oid}")); }
            set_data(doc, &wid, serde_json::to_value(w).unwrap());
            Ok(OpResult { removed: vec![oid], modified: vec![wid], ..Default::default() })
        }
        "add_floor" => {
            let points = polygon_of(&data)?;
            let mut v = data.clone(); v["points"] = json!(points); if let Value::Object(m) = &mut v { m.remove("origin"); m.remove("size"); }
            let f: Floor = serde_json::from_value(v).map_err(|e| OpError(format!("floor: {e}")))?;
            if lumber(&f.joist_size).is_none() { return err(format!("unknown joist size `{}`", f.joist_size)); }
            push_entity(doc, T_FLOOR, serde_json::to_value(f).unwrap())
        }
        "add_roof" => {
            let mut v = data.clone();
            if v.get("min").is_none() || v.get("max").is_none() {
                if let (Some(o), Some(s)) = (p2(data.get("origin")), p2(data.get("size"))) { v["min"] = json!(o); v["max"] = json!([o[0] + s[0], o[1] + s[1]]); } else { return err("give `min` + `max` (or `origin` + `size`)"); }
            }
            let r: Roof = serde_json::from_value(v).map_err(|e| OpError(format!("roof: {e}")))?;
            crate::roof::validate(&r)?;
            push_entity(doc, T_ROOF, serde_json::to_value(r).unwrap())
        }
        "add_foundation" => {
            let points = polygon_of(&data)?;
            let mut v = data.clone(); v["points"] = json!(points); if let Value::Object(m) = &mut v { m.remove("origin"); m.remove("size"); }
            let f: Foundation = serde_json::from_value(v).map_err(|e| OpError(format!("foundation: {e}")))?;
            if !["slab", "stemwall"].contains(&f.type_.as_str()) { return err("type must be slab or stemwall"); }
            push_entity(doc, T_FOUNDATION, serde_json::to_value(f).unwrap())
        }
        "add_beam" => {
            let mut v = data.clone(); if let Value::Object(m) = &mut v { if !m.contains_key("level") { m.insert("level".into(), json!(0)); } }
            let b: Beam = serde_json::from_value(v).map_err(|e| OpError(format!("beam: {e}")))?;
            validate_beam(&b)?;
            push_entity(doc, T_BEAM, serde_json::to_value(b).unwrap())
        }
        "add_stair" => {
            let mut v = data.clone();
            if let Some(a) = num(&data, "angle") { let r = a.to_radians(); v["direction"] = json!([r.cos(), r.sin()]); }
            if let Value::Object(m) = &mut v { m.remove("angle"); }
            let mut st: Stair = serde_json::from_value(v).map_err(|e| OpError(format!("stair: {e}")))?;
            st.direction = st.dir();
            validate_stair(&st)?;
            push_entity(doc, T_STAIR, serde_json::to_value(st).unwrap())
        }
        "check" => Ok(data_result("check", crate::engineering::check(doc))),
        "span_table" => Ok(data_result("span_table", crate::engineering::span_table(doc, &data))),
        "wall_svg" => {
            let wid = st(&data, "wall").ok_or_else(|| OpError("wall is required".into()))?;
            let svg = crate::drawing::wall_svg(doc, &wid).map_err(OpError)?;
            Ok(data_result("wall_svg", json!({ "wall": wid, "svg": svg })))
        }
        "plan_svg" => {
            let level = data.get("level").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            Ok(data_result("plan_svg", json!({ "level": level, "svg": crate::drawing::plan_svg(doc, level) })))
        }
        "update" => {
            let id = st(&data, "id").ok_or_else(|| OpError("id is required".into()))?;
            let patch = data.get("patch").cloned().ok_or_else(|| OpError("patch is required".into()))?;
            let c = find_custom(doc, &id).ok_or_else(|| OpError(format!("no framing entity {id}")))?;
            let mut v = c.data.clone(); deep_merge(&mut v, &patch);
            // validate by parsing
            match c.type_.as_str() {
                T_WALL => { let w: Wall = serde_json::from_value(v.clone()).map_err(|e| OpError(format!("wall: {e}")))?; if w.length() < 0.05 { return err("wall is too short"); } validate_wall(&w)?; }
                T_FLOOR => { serde_json::from_value::<Floor>(v.clone()).map_err(|e| OpError(format!("floor: {e}")))?; }
                T_ROOF => { let r: Roof = serde_json::from_value(v.clone()).map_err(|e| OpError(format!("roof: {e}")))?; crate::roof::validate(&r)?; }
                T_BEAM => { let b: Beam = serde_json::from_value(v.clone()).map_err(|e| OpError(format!("beam: {e}")))?; validate_beam(&b)?; }
                T_STAIR => { let s: Stair = serde_json::from_value(v.clone()).map_err(|e| OpError(format!("stair: {e}")))?; validate_stair(&s)?; }
                _ => { serde_json::from_value::<Foundation>(v.clone()).map_err(|e| OpError(format!("foundation: {e}")))?; }
            }
            set_data(doc, &id, v);
            Ok(OpResult { modified: vec![id], ..Default::default() })
        }
        "frame_room" => {
            let rid = st(&data, "room").ok_or_else(|| OpError("room is required".into()))?;
            frame_rooms(doc, &[rid], &data)
        }
        "frame_level" => {
            let level = data.get("level").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let h = House::read(doc);
            let ids: Vec<String> = h.rooms.iter().filter(|r| r.level == level).map(|r| r.id.clone()).collect();
            if ids.is_empty() { return err(format!("no construction rooms on storey {level}")); }
            frame_rooms(doc, &ids, &data)
        }
        "estimate" => {
            let only: Option<Vec<String>> = data.get("ids").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect());
            Ok(data_result("estimate", estimate(doc, only.as_deref())))
        }
        "summary" => Ok(data_result("summary", summary(doc))),
        other => err(format!("unknown framing op `{other}`")),
    }
}

fn validate_beam(b: &Beam) -> Result<(), OpError> {
    if b.length() < 0.1 { return err("beam is too short"); }
    if lumber(&b.size).is_none() { return err(format!("unknown beam size `{}`", b.size)); }
    if lumber(&b.post_size).is_none() { return err(format!("unknown post size `{}`", b.post_size)); }
    if !["lumber", "lvl", "glulam"].contains(&b.material.as_str()) { return err("material must be lumber, lvl or glulam"); }
    if b.plies == 0 || b.plies > 4 { return err("plies must be 1 to 4"); }
    Ok(())
}
fn validate_stair(s: &Stair) -> Result<(), OpError> {
    if s.width < 0.4 { return err("stair width must be at least 0.4 m"); }
    if s.tread_depth < 0.15 { return err("treadDepth must be at least 0.15 m"); }
    if lumber(&s.stringer_size).is_none() { return err(format!("unknown stringer size `{}`", s.stringer_size)); }
    if s.stringers < 2 { return err("at least 2 stringers"); }
    Ok(())
}

fn set_data(doc: &mut Document, id: &str, v: Value) {
    for e in doc.entities.iter_mut() { if let Entity::Custom(c) = e { if c.id == id { c.data = v; return; } } }
}

/// One framing wall per (exterior or all) edge of the given construction rooms, with their openings.
fn frame_rooms(doc: &mut Document, rooms: &[String], data: &Value) -> Result<OpResult, OpError> {
    let h = House::read(doc);
    let d = settings(doc).defaults;
    let which = st(data, "which").unwrap_or_else(|| "exterior".into());
    let mut created = vec![]; let mut ents = vec![];
    for rid in rooms {
        let room = h.room(rid).ok_or_else(|| OpError(format!("no construction room {rid}")))?.clone();
        let n = room.points.len(); if n < 3 { continue; }
        let cx = room.points.iter().map(|p| p[0]).sum::<f64>() / n as f64; let cz = room.points.iter().map(|p| p[1]).sum::<f64>() / n as f64;
        for i in 0..n {
            let shared = h.edge_is_shared(&room, i);
            if which == "exterior" && shared { continue; }
            let (a, b) = (room.points[i], room.points[(i + 1) % n]);
            let u = [b[0] - a[0], b[1] - a[1]]; let len = (u[0] * u[0] + u[1] * u[1]).sqrt(); if len < 0.05 { continue; }
            // default exterior side = left of a→b (normal [uz, -ux]); flip when the room centre lies on that side
            let nl = [u[1] / len, -u[0] / len];
            let flip = (cx - a[0]) * nl[0] + (cz - a[1]) * nl[1] > 0.0;
            let mut v = json!({ "level": room.level, "a": a, "b": b, "height": num(data, "height").unwrap_or_else(|| h.height(room.level)), "flip": flip, "room": room.id, "edge": i,
                                "reference": "center", "cornerDetail": d.corner_detail, "tDetail": d.t_detail,
                                "studSize": st(data, "studSize").unwrap_or_else(|| d.stud_size.clone()), "studSpacing": num(data, "studSpacing").unwrap_or(d.stud_spacing),
                                "plateSize": d.plate_size, "topPlates": d.top_plates, "bottomPlates": d.bottom_plates, "sheathing": d.sheathing });
            if let Some(sh) = data.get("sheathing") { deep_merge(&mut v["sheathing"], sh); }
            if shared { v["sheathing"]["exterior"]["enabled"] = json!(false); }
            let mut w: Wall = serde_json::from_value(v).map_err(|e| OpError(e.to_string()))?;
            for o in h.openings.iter().filter(|o| o.room.as_deref() == Some(rid.as_str()) && o.edge == Some(i)) {
                let kind = if o.kind == "window" { "window" } else { "door" };
                let offset = (o.along - o.width / 2.0).max(0.0);
                w.openings.push(Opening { id: o.id.clone(), kind: kind.into(), offset, width: o.width.min(len - offset), height: o.height, sill: if kind == "door" { 0.0 } else { o.sill }, header_size: None, king_studs: 1, jack_studs: 1, extra: Map::new() });
            }
            let r = push_entity(doc, T_WALL, serde_json::to_value(w).unwrap())?;
            created.extend(r.created); ents.extend(r.entities);
        }
    }
    if created.is_empty() { return err("nothing to frame (no exterior edges found; try which = \"all\")"); }
    Ok(OpResult { created, entities: ents, ..Default::default() })
}

// ---- takeoff --------------------------------------------------------------------------------------------

fn expansion_of(c: &Custom, house: &House) -> Expansion { expand_any(c, house, "") }

pub fn estimate(doc: &Document, only: Option<&[String]>) -> Value {
    let house = House::read(doc);
    #[derive(Default)] struct Agg { count: u64, length: f64, area: f64, volume: f64 }
    let mut items: std::collections::BTreeMap<(String, String), Agg> = Default::default();
    let mut by_element = vec![];
    let mut lumber_m = 0.0; let mut pieces = 0u64; let mut sheathing_area = 0.0; let mut gypsum_area = 0.0; let mut concrete = 0.0; let mut hardware = 0u64; let mut cladding_area = 0.0; let mut soffit_area = 0.0; let mut gutters = 0.0;
    for e in &doc.entities {
        let Entity::Custom(c) = e else { continue };
        if !TYPES.contains(&c.type_.as_str()) { continue; }
        if let Some(o) = only { if !o.iter().any(|x| x == &c.id) { continue; } }
        let ex = expansion_of(c, &house);
        let mut local: std::collections::BTreeMap<(String, String), Agg> = Default::default();
        for m in &ex.members {
            let stock = stock_length(m.length);
            let key = (m.category.to_string(), format!("{} × {:.2} m", m.size_label, stock));
            for map in [&mut items, &mut local] { let a = map.entry(key.clone()).or_default(); a.count += 1; a.length += m.length; }
            lumber_m += m.length; pieces += 1;
        }
        for p in &ex.panels {
            let key = (p.category.to_string(), format!("{} {:.0} mm", p.category, p.thickness * 1000.0));
            for map in [&mut items, &mut local] { let a = map.entry(key.clone()).or_default(); a.area += p.area; a.count += 1; }
            match p.category { "gypsum" => gypsum_area += p.area, "cladding" => cladding_area += p.area, "soffit" => soffit_area += p.area, _ => sheathing_area += p.area }
        }
        for l in &ex.linear {
            let key = (l.category.to_string(), l.category.trim_end_matches('s').to_string());
            for map in [&mut items, &mut local] { let a = map.entry(key.clone()).or_default(); a.length += l.length; a.count += 1; }
            gutters += l.length;
        }
        for hw in &ex.hardware {
            let key = ("hardware".to_string(), hw.description.to_string());
            for map in [&mut items, &mut local] { map.entry(key.clone()).or_default().count += 1; }
            hardware += 1;
        }
        for k in &ex.concrete {
            let key = ("concrete".to_string(), "concrete".to_string());
            for map in [&mut items, &mut local] { let a = map.entry(key.clone()).or_default(); a.volume += k.volume; a.count += 1; }
            concrete += k.volume;
        }
        by_element.push(json!({ "id": c.id, "type": c.type_, "items": local.iter().map(|((cat, desc), a)| item_json(cat, desc, a.count, a.length, a.area, a.volume)).collect::<Vec<_>>() }));
    }
    fn item_json(cat: &str, desc: &str, count: u64, length: f64, area: f64, volume: f64) -> Value {
        let mut v = json!({ "category": cat, "description": desc, "count": count });
        if length > 0.0 { v["length_m"] = json!((length * 100.0).round() / 100.0); }
        if area > 0.0 { v["area_m2"] = json!((area * 100.0).round() / 100.0); v["panels"] = json!((area / PANEL_AREA).ceil() as u64); }
        if volume > 0.0 { v["volume_m3"] = json!((volume * 1000.0).round() / 1000.0); }
        v
    }
    json!({
        "items": items.iter().map(|((cat, desc), a)| item_json(cat, desc, a.count, a.length, a.area, a.volume)).collect::<Vec<_>>(),
        "totals": { "lumber_m": (lumber_m * 100.0).round() / 100.0, "lumber_pieces": pieces, "sheathing_m2": (sheathing_area * 100.0).round() / 100.0, "sheathing_panels": (sheathing_area / PANEL_AREA).ceil() as u64, "gypsum_m2": (gypsum_area * 100.0).round() / 100.0, "gypsum_panels": (gypsum_area / PANEL_AREA).ceil() as u64, "concrete_m3": (concrete * 1000.0).round() / 1000.0, "hardware_pieces": hardware, "cladding_m2": (cladding_area * 100.0).round() / 100.0, "soffit_m2": (soffit_area * 100.0).round() / 100.0, "gutters_m": (gutters * 100.0).round() / 100.0 },
        "byElement": by_element,
        "stockLengths_m": STOCK,
    })
}

pub fn summary(doc: &Document) -> Value {
    let house = House::read(doc);
    let mut walls = vec![]; let mut floors = vec![]; let mut roofs = vec![]; let mut foundations = vec![]; let mut beams = vec![]; let mut stairs = vec![];
    for e in &doc.entities {
        let Entity::Custom(c) = e else { continue };
        match c.type_.as_str() {
            T_WALL => if let Ok(w) = serde_json::from_value::<Wall>(c.data.clone()) {
                let ex = expand_wall(&c.id, &w, &house, "");
                walls.push(json!({ "id": c.id, "level": w.level, "length_m": (w.length() * 100.0).round() / 100.0, "height_m": w.height, "studSize": w.stud_size, "studSpacing": w.stud_spacing, "studs": ex.members.iter().filter(|m| m.category == "studs").count(), "openings": w.openings.len(), "lumber_m": (ex.lumber_length() * 100.0).round() / 100.0, "room": w.room, "edge": w.edge }));
            },
            T_FLOOR => if let Ok(f) = serde_json::from_value::<Floor>(c.data.clone()) { let ex = expand_floor(&c.id, &f, &house, ""); floors.push(json!({ "id": c.id, "level": f.level, "joistSize": f.joist_size, "joists": ex.members.iter().filter(|m| m.category == "joists").count(), "area_m2": (plan_area(&f.points).abs() * 100.0).round() / 100.0 })); },
            T_ROOF => if let Ok(r) = serde_json::from_value::<Roof>(c.data.clone()) { let ex = expand_roof(&c.id, &r, &house, ""); let trusses = crate::roof::truss_count(&ex); roofs.push(json!({ "id": c.id, "type": r.type_, "shape": r.shape, "dormers": r.dormers.len(), "hips": ex.members.iter().filter(|m| m.category == "hips").count(), "jacks": ex.members.iter().filter(|m| m.category == "jacks").count(), "pitch": r.pitch, "span_m": if r.ridge_along_x { (r.max[1] - r.min[1]).abs() } else { (r.max[0] - r.min[0]).abs() }, "trusses": trusses, "rafters": ex.members.iter().filter(|m| m.category == "rafters").count() })); },
            T_FOUNDATION => if let Ok(f) = serde_json::from_value::<Foundation>(c.data.clone()) { let ex = expand_foundation(&c.id, &f, &house, ""); foundations.push(json!({ "id": c.id, "type": f.type_, "concrete_m3": (ex.concrete.iter().map(|k| k.volume).sum::<f64>() * 1000.0).round() / 1000.0 })); },
            T_BEAM => if let Ok(b) = serde_json::from_value::<Beam>(c.data.clone()) { let [w, h] = b.section(); beams.push(json!({ "id": c.id, "level": b.level, "length_m": (b.length() * 100.0).round() / 100.0, "material": b.material, "section_mm": [(w * 1000.0).round(), (h * 1000.0).round()], "posts": if b.posts { b.supports().len() } else { 0 } })); },
            T_STAIR => if let Ok(s) = serde_json::from_value::<Stair>(c.data.clone()) { let (n, r) = s.risers_of(&house); stairs.push(json!({ "id": c.id, "level": s.level, "risers": n, "riserHeight_m": (r * 1000.0).round() / 1000.0, "treadDepth_m": s.tread_depth, "stringers": s.stringers })); },
            _ => {}
        }
    }
    json!({ "defaults": settings(doc).defaults, "walls": walls, "floors": floors, "roofs": roofs, "foundations": foundations, "beams": beams, "stairs": stairs, "totals": estimate(doc, None)["totals"] })
}

pub fn metrics(doc: &Document) -> Value {
    let house = House::read(doc);
    let (mut walls, mut studs, mut trusses, mut joists, mut lumber, mut concrete) = (0u64, 0u64, 0u64, 0u64, 0.0, 0.0);
    for e in &doc.entities {
        let Entity::Custom(c) = e else { continue };
        if !TYPES.contains(&c.type_.as_str()) { continue; }
        let ex = expand_any(c, &house, "");
        if c.type_ == T_WALL { walls += 1; }
        studs += ex.members.iter().filter(|m| m.category == "studs").count() as u64;
        trusses += crate::roof::truss_count(&ex) as u64;
        joists += ex.members.iter().filter(|m| m.category == "joists").count() as u64;
        lumber += ex.lumber_length(); concrete += ex.concrete.iter().map(|k| k.volume).sum::<f64>();
    }
    json!({ "walls": walls, "studs": studs, "trusses": trusses, "joists": joists, "lumber_m": (lumber * 100.0).round() / 100.0, "concrete_m3": (concrete * 1000.0).round() / 1000.0 })
}

pub fn transform_plan(c: &mut Custom, m: &plannr_core::geom::Mat4) -> Result<(), OpError> {
    let ap = |p: P2| -> P2 { let q = plannr_core::geom::mat_apply(m, [p[0], 0.0, p[1]]); [q[0], q[2]] };
    match c.type_.as_str() {
        T_WALL => { let mut w: Wall = serde_json::from_value(c.data.clone()).map_err(|e| OpError(e.to_string()))?; w.a = ap(w.a); w.b = ap(w.b); c.data = serde_json::to_value(w).unwrap(); }
        T_FLOOR => { let mut f: Floor = serde_json::from_value(c.data.clone()).map_err(|e| OpError(e.to_string()))?; f.points = f.points.iter().map(|p| ap(*p)).collect(); c.data = serde_json::to_value(f).unwrap(); }
        T_BEAM => { let mut b: Beam = serde_json::from_value(c.data.clone()).map_err(|e| OpError(e.to_string()))?; b.a = ap(b.a); b.b = ap(b.b); c.data = serde_json::to_value(b).unwrap(); }
        T_STAIR => {
            let mut s: Stair = serde_json::from_value(c.data.clone()).map_err(|e| OpError(e.to_string()))?;
            s.a = ap(s.a);
            let d = plannr_core::geom::mat_apply_dir(m, [s.direction[0], 0.0, s.direction[1]]);
            s.direction = [d[0], d[2]]; s.direction = s.dir();
            c.data = serde_json::to_value(s).unwrap();
        }
        T_FOUNDATION => { let mut f: Foundation = serde_json::from_value(c.data.clone()).map_err(|e| OpError(e.to_string()))?; f.points = f.points.iter().map(|p| ap(*p)).collect(); c.data = serde_json::to_value(f).unwrap(); }
        T_ROOF => {
            let mut r: Roof = serde_json::from_value(c.data.clone()).map_err(|e| OpError(e.to_string()))?;
            let corners = [r.min, [r.max[0], r.min[1]], r.max, [r.min[0], r.max[1]]].map(ap);
            // still an axis-aligned rectangle? (moves and 90° turns only)
            let xs: Vec<f64> = corners.iter().map(|p| p[0]).collect(); let zs: Vec<f64> = corners.iter().map(|p| p[1]).collect();
            let (minx, maxx) = (xs.iter().cloned().fold(f64::MAX, f64::min), xs.iter().cloned().fold(f64::MIN, f64::max));
            let (minz, maxz) = (zs.iter().cloned().fold(f64::MAX, f64::min), zs.iter().cloned().fold(f64::MIN, f64::max));
            let aligned = corners.iter().all(|p| ((p[0] - minx).abs() < 1e-6 || (p[0] - maxx).abs() < 1e-6) && ((p[1] - minz).abs() < 1e-6 || (p[1] - maxz).abs() < 1e-6));
            if !aligned { return err("a framing roof can only be moved or turned in 90° steps"); }
            let turned = (corners[1][0] - corners[0][0]).abs() < 1e-6; // the former x edge now runs along z
            if turned { r.ridge_along_x = !r.ridge_along_x; }
            r.min = [minx, minz]; r.max = [maxx, maxz];
            c.data = serde_json::to_value(r).unwrap();
        }
        _ => return err("not a framing entity"),
    }
    Ok(())
}
