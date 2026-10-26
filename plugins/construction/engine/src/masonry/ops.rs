//! Ops of the masonry plugin: element CRUD, plan transforms and the dispatch to import, takeoff,
//! checks, U-values and drawings.

use plannr_core::model::{Custom, Document, Entity};
use plannr_core::ops::{OpError, OpResult};
use serde_json::{json, Map, Value};

use crate::masonry::data::*;

pub fn err<T>(m: impl Into<String>) -> Result<T, OpError> { Err(OpError(m.into())) }
pub fn st(v: &Value, k: &str) -> Option<String> { v.get(k).and_then(|x| x.as_str()).map(|x| x.to_string()) }
pub fn p2(v: Option<&Value>) -> Option<P2> { let a = v?.as_array()?; Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?]) }
fn p2s(v: Option<&Value>) -> Vec<P2> { v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|p| p2(Some(p))).collect()).unwrap_or_default() }
pub fn data_result(name: &str, v: Value) -> OpResult {
    OpResult { entities: vec![Entity::Custom(Custom { id: name.into(), layer: String::new(), type_: format!("masonry.{name}"), data: v, extra: Map::new() })], ..Default::default() }
}
pub fn deep_merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => { for (k, v) in p { match b.get_mut(k) { Some(bv) if bv.is_object() && v.is_object() => deep_merge(bv, v), _ => { b.insert(k.clone(), v.clone()); } } } }
        (b, p) => *b = p.clone(),
    }
}
fn polygon_of(data: &Value) -> Result<Vec<P2>, OpError> {
    let pts = p2s(data.get("points"));
    if pts.len() >= 3 { return Ok(pts); }
    if let (Some(o), Some(s)) = (p2(data.get("origin")), p2(data.get("size"))) { return Ok(vec![o, [o[0] + s[0], o[1]], [o[0] + s[0], o[1] + s[1]], [o[0], o[1] + s[1]]]); }
    err("give `points` (≥ 3) or `origin` + `size`")
}
pub fn find_custom<'a>(doc: &'a Document, id: &str) -> Option<&'a Custom> {
    doc.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.id == id && TYPES.contains(&c.type_.as_str()) => Some(c), _ => None })
}
pub fn set_data(doc: &mut Document, id: &str, v: Value) {
    for e in doc.entities.iter_mut() { if let Entity::Custom(c) = e { if c.id == id { c.data = v; return; } } }
}
pub fn push_entity(doc: &mut Document, type_: &str, data: Value) -> Result<OpResult, OpError> {
    let layer = masonry_layer(doc);
    let id = new_id();
    let c = Custom { id: id.clone(), layer, type_: type_.into(), data, extra: Map::new() };
    doc.entities.push(Entity::Custom(c.clone()));
    Ok(OpResult { created: vec![id], entities: vec![Entity::Custom(c)], ..Default::default() })
}

pub fn validate_wall(w: &Wall) -> Result<(), OpError> {
    if w.length() < 0.05 { return err("wall is too short"); }
    if !["exterior", "center", "interior"].contains(&w.reference.as_str()) { return err("reference must be exterior, center or interior"); }
    if !["cavity", "single"].contains(&w.type_.as_str()) { return err("type must be cavity or single"); }
    for j in [&w.start_join, &w.end_join] { if !["auto", "none"].contains(&j.as_str()) { return err("startJoin / endJoin must be auto or none"); } }
    for u in [&w.outer, &w.inner].into_iter().flatten() {
        if u.length <= 0.02 || u.height <= 0.02 || u.width <= 0.02 || u.joint < 0.0 || u.joint > 0.03 { return err(format!("unit `{}`: length, height, width must be > 20 mm and the joint 0–30 mm", u.name)); }
        if !["stretcher", "third", "stack", "english", "flemish"].contains(&u.bond.as_str()) { return err("bond must be stretcher, third, stack, english or flemish"); }
    }
    if let Some(c) = &w.cavity {
        if c.width < 0.0 || c.width > 0.3 { return err("cavity width must be 0–0.3 m"); }
        if !["none", "fill", "board"].contains(&c.insulation.kind.as_str()) { return err("insulation.kind must be none, fill or board"); }
    }
    for o in &w.openings { validate_opening(w, o)?; }
    Ok(())
}
fn validate_opening(w: &Wall, o: &Opening) -> Result<(), OpError> {
    if o.width <= 0.05 || o.height <= 0.05 { return err("opening needs width and height"); }
    if o.offset < -1e-6 || o.offset + o.width > w.length() + 1e-6 { return err(format!("opening does not fit the wall (length {:.2} m)", w.length())); }
    if !["window", "door"].contains(&o.kind.as_str()) { return err("opening kind must be window or door"); }
    Ok(())
}
pub fn validate_floor(f: &Floor) -> Result<(), OpError> {
    if f.points.len() < 3 { return err("floor needs ≥ 3 points"); }
    if !["beamBlock", "hollowCore", "insitu"].contains(&f.system.as_str()) { return err("system must be beamBlock, hollowCore or insitu"); }
    if !["auto", "x", "z"].contains(&f.direction.as_str()) { return err("direction must be auto, x or z"); }
    if let Some(s) = f.spacing { if !(0.3..=1.2).contains(&s) { return err("spacing must be 0.3–1.2 m"); } }
    Ok(())
}

pub fn wall_from(data: &Value, d: &Defaults) -> Result<Wall, OpError> {
    let mut v = json!({ "plaster": d.plaster, "dpc": d.dpc });
    deep_merge(&mut v, data);
    let w: Wall = serde_json::from_value(v).map_err(|e| OpError(format!("wall: {e}")))?;
    validate_wall(&w)?;
    Ok(w)
}

pub fn apply(name: &str, data: Value, doc: &mut Document) -> Result<OpResult, OpError> {
    ensure(doc);
    match name {
        "set_defaults" => {
            let mut s = settings(doc);
            let mut v = serde_json::to_value(&s.defaults).unwrap(); deep_merge(&mut v, &data);
            s.defaults = serde_json::from_value(v).map_err(|e| OpError(format!("defaults: {e}")))?;
            save_settings(doc, &s);
            Ok(data_result("defaults", serde_json::to_value(&s.defaults).unwrap()))
        }
        "add_wall" => { let d = settings(doc).defaults; let w = wall_from(&data, &d)?; push_entity(doc, T_WALL, serde_json::to_value(w).unwrap()) }
        "add_opening" => {
            let wid = st(&data, "wall").ok_or_else(|| OpError("wall is required".into()))?;
            let c = find_custom(doc, &wid).filter(|c| c.type_ == T_WALL).ok_or_else(|| OpError(format!("no masonry wall {wid}")))?;
            let mut w: Wall = serde_json::from_value(c.data.clone()).map_err(|e| OpError(e.to_string()))?;
            let mut ov = data.clone(); if let Value::Object(m) = &mut ov { m.remove("wall"); }
            let o: Opening = serde_json::from_value(ov).map_err(|e| OpError(format!("opening: {e}")))?;
            validate_opening(&w, &o)?;
            let oid = o.id.clone(); w.openings.push(o);
            set_data(doc, &wid, serde_json::to_value(w).unwrap());
            Ok(OpResult { created: vec![oid], modified: vec![wid], ..Default::default() })
        }
        "remove_opening" => {
            let wid = st(&data, "wall").ok_or_else(|| OpError("wall is required".into()))?; let oid = st(&data, "opening").ok_or_else(|| OpError("opening is required".into()))?;
            let c = find_custom(doc, &wid).filter(|c| c.type_ == T_WALL).ok_or_else(|| OpError(format!("no masonry wall {wid}")))?;
            let mut w: Wall = serde_json::from_value(c.data.clone()).map_err(|e| OpError(e.to_string()))?;
            let n = w.openings.len(); w.openings.retain(|o| o.id != oid); if w.openings.len() == n { return err(format!("no opening {oid}")); }
            set_data(doc, &wid, serde_json::to_value(w).unwrap());
            Ok(OpResult { removed: vec![oid], modified: vec![wid], ..Default::default() })
        }
        "add_floor" => {
            let points = polygon_of(&data)?;
            let mut v = data.clone(); v["points"] = json!(points); if let Value::Object(m) = &mut v { m.remove("origin"); m.remove("size"); }
            let f: Floor = serde_json::from_value(v).map_err(|e| OpError(format!("floor: {e}")))?;
            validate_floor(&f)?;
            push_entity(doc, T_FLOOR, serde_json::to_value(f).unwrap())
        }
        "add_foundation" => {
            let points = polygon_of(&data)?;
            let mut v = data.clone(); v["points"] = json!(points); if let Value::Object(m) = &mut v { m.remove("origin"); m.remove("size"); }
            let f: Foundation = serde_json::from_value(v).map_err(|e| OpError(format!("foundation: {e}")))?;
            push_entity(doc, T_FOUNDATION, serde_json::to_value(f).unwrap())
        }
        "update" => {
            let id = st(&data, "id").ok_or_else(|| OpError("id is required".into()))?;
            let patch = data.get("patch").cloned().ok_or_else(|| OpError("patch is required".into()))?;
            let c = find_custom(doc, &id).ok_or_else(|| OpError(format!("no masonry element {id}")))?;
            let mut v = c.data.clone(); deep_merge(&mut v, &patch);
            match c.type_.as_str() {
                T_WALL => { let w: Wall = serde_json::from_value(v.clone()).map_err(|e| OpError(format!("wall: {e}")))?; validate_wall(&w)?; }
                T_FLOOR => { let f: Floor = serde_json::from_value(v.clone()).map_err(|e| OpError(format!("floor: {e}")))?; validate_floor(&f)?; }
                _ => { serde_json::from_value::<Foundation>(v.clone()).map_err(|e| OpError(format!("foundation: {e}")))?; }
            }
            set_data(doc, &id, v);
            Ok(OpResult { modified: vec![id], ..Default::default() })
        }
        "import_construction" => crate::masonry::import::import_construction(doc, &data),
        "estimate" => {
            let only: Option<Vec<String>> = data.get("ids").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect());
            Ok(data_result("estimate", crate::masonry::takeoff::estimate(doc, only.as_deref())))
        }
        "summary" => Ok(data_result("summary", crate::masonry::takeoff::summary(doc))),
        "check" => Ok(data_result("check", crate::masonry::check::check(doc))),
        "uvalue" => Ok(data_result("uvalue", crate::masonry::uvalue::uvalue_op(doc, &data)?)),
        "wall_svg" => {
            let wid = st(&data, "wall").ok_or_else(|| OpError("wall is required".into()))?;
            // stored or derived from a construction wall (`<room>#wall.<e>`)
            let h = House::read(doc);
            let w = h.walls.iter().find(|(i, _)| i == &wid).map(|(_, w)| w.clone()).ok_or_else(|| OpError(format!("no masonry wall {wid}")))?;
            let svg = crate::masonry::wall::wall_svg(&wid, &w, &h);
            Ok(data_result("wall_svg", json!({ "wall": wid, "svg": svg })))
        }
        "plan_svg" => {
            let level = data.get("level").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            Ok(data_result("plan_svg", json!({ "level": level, "svg": crate::masonry::drawing::plan_svg(doc, level) })))
        }
        "storeys" => Ok(data_result("storeys", storeys(&House::read(doc)))),
        "set_clear_height" => {
            let level = data.get("level").and_then(|v| v.as_u64()).ok_or_else(|| OpError("level is required".into()))? as usize;
            let clear = data.get("clear").and_then(|v| v.as_f64()).ok_or_else(|| OpError("clear is required (m)".into()))?;
            if !(1.8..=6.0).contains(&clear) { return err("clear height must be 1.8–6 m"); }
            let h = House::read(doc);
            if level >= h.storeys.len() { return err(format!("no storey {level}")); }
            // floor-to-floor = the clear height plus whatever fills the top of the storey
            let height = ((clear + h.max_structure(level)) * 1000.0).round() / 1000.0;
            let c = doc.extensions.get_mut("construction").ok_or_else(|| OpError("no construction storeys".into()))?;
            let st = c.get_mut("storeys").and_then(|v| v.as_array_mut()).and_then(|a| a.get_mut(level)).ok_or_else(|| OpError(format!("no storey {level}")))?;
            st["height"] = json!(height);
            Ok(data_result("storeys", storeys(&House::read(doc))))
        }
        other => err(format!("unknown masonry op `{other}`")),
    }
}

/// Per storey: floor-to-floor height, what fills its top and the clear height that leaves.
pub fn storeys(h: &House) -> Value {
    json!(h.storeys.iter().enumerate().map(|(i, (name, height))| {
        let s = h.max_structure(i);
        json!({ "level": i, "name": name, "base": (h.base(i) * 1000.0).round() / 1000.0, "height": height, "structure": (s * 1000.0).round() / 1000.0, "clear": ((height - s) * 1000.0).round() / 1000.0 })
    }).collect::<Vec<_>>())
}

pub fn transform_plan(c: &mut Custom, m: &plannr_core::geom::Mat4) -> Result<(), OpError> {
    let ap = |p: P2| -> P2 { let q = plannr_core::geom::mat_apply(m, [p[0], 0.0, p[1]]); [q[0], q[2]] };
    match c.type_.as_str() {
        T_WALL => { let mut w: Wall = serde_json::from_value(c.data.clone()).map_err(|e| OpError(e.to_string()))?; w.a = ap(w.a); w.b = ap(w.b); c.data = serde_json::to_value(w).unwrap(); }
        T_FLOOR => { let mut f: Floor = serde_json::from_value(c.data.clone()).map_err(|e| OpError(e.to_string()))?; f.points = f.points.iter().map(|p| ap(*p)).collect(); f.openings = f.openings.iter().map(|o| o.iter().map(|p| ap(*p)).collect()).collect(); c.data = serde_json::to_value(f).unwrap(); }
        T_FOUNDATION => { let mut f: Foundation = serde_json::from_value(c.data.clone()).map_err(|e| OpError(e.to_string()))?; f.points = f.points.iter().map(|p| ap(*p)).collect(); c.data = serde_json::to_value(f).unwrap(); }
        _ => return err("not a masonry element"),
    }
    Ok(())
}
