//! Plugin ops: settings, storeys, rooms, walls, openings, roofs, fixtures, runs, finishes,
//! Uzzetje import and the summary.

use plannr_core::model::{Custom, Document, Entity, Face, Group, Guide, Layer, Material};
use plannr_core::geom::mat_identity;
use plannr_core::ops::{OpError, OpResult};
use serde_json::{json, Map, Value};

use crate::data::*;
use crate::expand;
use crate::script;

fn err<T>(s: impl Into<String>) -> Result<T, OpError> { Err(OpError(s.into())) }
fn num(v: &Value, k: &str) -> Option<f64> { v.get(k).and_then(|x| x.as_f64()) }
fn p2(v: &Value) -> Option<P2> { let a = v.as_array()?; Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?]) }
fn p3(v: &Value) -> Option<P3> { let a = v.as_array()?; Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?]) }
fn p2s(v: Option<&Value>) -> Vec<P2> { v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(p2).collect()).unwrap_or_default() }
fn p3s(v: Option<&Value>) -> Vec<P3> { v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(p3).collect()).unwrap_or_default() }

// ---- settings & materials ---------------------------------------------------------------

pub fn settings(doc: &Document) -> Settings {
    let mut s: Settings = doc.extensions.get(PLUGIN_ID).cloned().and_then(|v| serde_json::from_value::<Settings>(v).ok()).map(|mut s: Settings| { if s.storeys.is_empty() { s.storeys = Settings::default().storeys; } s }).unwrap_or_default();
    // the masonry build-up is the single source of truth for leaf, cavity and partition thicknesses
    if let Some(md) = doc.extensions.get("masonry").and_then(|m| m.get("defaults")) {
        let w = |k: &str| md.get(k).and_then(|u| u.get("width")).and_then(|v| v.as_f64());
        if let Some(x) = w("inner") { s.cavity.inner_leaf = x; }
        if let Some(x) = w("outer") { s.cavity.outer_leaf = x; }
        if let Some(x) = md.get("cavity").and_then(|c| c.get("width")).and_then(|v| v.as_f64()) { s.cavity.cavity = x; }
        if let Some(x) = w("partition") { s.wall_thickness = x; }
    }
    s
}
pub fn save_settings(doc: &mut Document, s: &Settings) {
    doc.extensions.insert(PLUGIN_ID.to_string(), serde_json::to_value(s).unwrap_or(Value::Null));
}
pub fn ensure(doc: &mut Document) {
    if !doc.extensions.contains_key(PLUGIN_ID) { let s = Settings::default(); save_settings(doc, &s); }
    for (key, name, color, opacity) in MATERIALS {
        let id = format!("construction.{key}");
        if !doc.materials.iter().any(|m| m.id == id) {
            doc.materials.push(Material { id, name: name.to_string(), color: color.to_string(), opacity: *opacity });
        }
    }
    for (name, ..) in script::C_MATERIALS { ensure_script_material(doc, name); }
    ensure_unit_materials(doc);
}
fn ensure_script_material(doc: &mut Document, name: &str) {
    let (id, label, color, opacity) = script::material_spec(name);
    if !doc.materials.iter().any(|m| m.id == id) { doc.materials.push(Material { id, name: label, color, opacity }); }
}
/// Materials the scripted units in the document ask for (named or `#RRGGBB`) must exist for compile.
pub fn ensure_unit_materials(doc: &mut Document) {
    let m = expand::model(doc);
    if m.units.is_empty() { return; }
    let names: Vec<String> = script::with_runtime(&m.types, |rt| {
        let mut out = vec![];
        for (_, u) in &m.units {
            if let Some(solids) = script::unit_solids(rt, u) { for s in solids { if !out.contains(&s.material) { out.push(s.material); } } }
        }
        out
    });
    for n in names { ensure_script_material(doc, &n); }
}

/// `values` must be numbers and `choices` strings (scripts coerce silently, so catch it here).
fn check_unit_params(u: &Unit) -> Result<(), OpError> {
    if let Some((k, v)) = u.values.iter().find(|(_, v)| !v.is_number()) { return err(format!("values.{k} must be a number, got {v}")); }
    if let Some((k, v)) = u.choices.iter().find(|(_, v)| !v.is_string()) { return err(format!("choices.{k} must be a string, got {v}")); }
    Ok(())
}

/// Refresh a scripted unit's informative size (width/depth/height/lift) from its solids.
pub fn refresh_unit_bounds(types: &[ComponentType], u: &mut Unit) -> bool {
    script::with_runtime(types, |rt| {
        if let Some(solids) = script::unit_solids(rt, u) {
            if let Some(b) = script::bounds(&solids) {
                u.width = b.width().max(0.01); u.depth = b.depth().max(0.01); u.height = b.height().max(0.01); u.lift = b.low;
                return true;
            }
        }
        false
    })
}

fn layer_for(doc: &Document, data: &Value) -> String {
    data.get("layer").and_then(|l| l.as_str()).and_then(|l| doc.layer_by_ref(l)).map(|l| l.id.clone()).unwrap_or_else(|| doc.default_layer().to_string())
}

fn push_custom(doc: &mut Document, id: Option<String>, type_: &str, data: Value, layer: String) -> Result<OpResult, OpError> {
    let c = Custom { id: id.unwrap_or_else(new_id), layer, type_: type_.to_string(), data, extra: Map::new() };
    let e = Entity::Custom(c);
    let r = OpResult { created: vec![e.id().to_string()], entities: vec![e.clone()], ..Default::default() };
    doc.entities.push(e);
    Ok(r)
}

fn custom_mut<'a>(doc: &'a mut Document, id: &str) -> Result<&'a mut Custom, OpError> {
    match doc.find_mut(id) {
        Some((Entity::Custom(c), _)) if c.type_.starts_with("construction.") => Ok(c),
        Some(_) => err(format!("{id} is not a construction entity")),
        None => err(format!("no entity {id}")),
    }
}

fn parse_level(doc: &Document, d: &Value) -> Result<usize, OpError> {
    let s = settings(doc);
    let l = d.get("level").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    if l >= s.count() { return err(format!("level {l} does not exist (storeys: {})", s.count())); }
    Ok(l)
}

// ---- dispatch ------------------------------------------------------------------------------

pub fn apply(op: &str, data: Value, doc: &mut Document) -> Result<OpResult, OpError> {
    ensure(doc);
    match op {
        "setup" => setup(doc, &data),
        "add_storey" => {
            let mut s = settings(doc);
            let n = s.storeys.len();
            let name = data.get("name").and_then(|v| v.as_str()).map(|x| x.to_string()).unwrap_or_else(|| match n { 0 => "Ground floor".into(), 1 => "First floor".into(), 2 => "Second floor".into(), k => format!("Storey {}", k + 1) });
            let height = num(&data, "height").unwrap_or_else(|| s.storeys.last().map(|x| x.height).unwrap_or(2.6));
            let st = Storey { id: new_id(), name, height, extra: Map::new() };
            s.storeys.push(st);
            save_settings(doc, &s);
            Ok(OpResult { measurement: Some(n as f64), ..Default::default() })
        }
        "update_storey" => {
            let mut s = settings(doc);
            let i = data.get("index").and_then(|v| v.as_u64()).ok_or_else(|| OpError("index is required".into()))? as usize;
            let st = s.storeys.get_mut(i).ok_or_else(|| OpError(format!("no storey {i}")))?;
            if let Some(n) = data.get("name").and_then(|v| v.as_str()) { st.name = n.to_string(); }
            if let Some(h) = num(&data, "height") { st.height = h.max(0.5); }
            save_settings(doc, &s);
            Ok(OpResult::default())
        }
        "remove_storey" => remove_storey(doc, &data),
        "add_room" => {
            let level = parse_level(doc, &data)?;
            let mut points = p2s(data.get("points"));
            if points.len() < 3 {
                let (o, sz) = (data.get("origin").and_then(p2), data.get("size").and_then(p2));
                match (o, sz) {
                    (Some(o), Some(sz)) if sz[0].abs() > 1e-6 && sz[1].abs() > 1e-6 => {
                        points = vec![o, [o[0] + sz[0], o[1]], [o[0] + sz[0], o[1] + sz[1]], [o[0], o[1] + sz[1]]];
                    }
                    _ => return err("add_room needs `points` (3+ plan points) or `origin` + `size`"),
                }
            }
            let n = doc.entities.iter().filter(|e| matches!(e, Entity::Custom(c) if c.type_ == T_ROOM)).count();
            let room = Room {
                name: data.get("name").and_then(|v| v.as_str()).map(|x| x.to_string()).unwrap_or_else(|| format!("Room {}", n + 1)),
                level, points, has_floor: data.get("hasFloor").and_then(|v| v.as_bool()).unwrap_or(true),
                has_ceiling: data.get("hasCeiling").and_then(|v| v.as_bool()).unwrap_or(false),
                wall_height: num(&data, "wallHeight"), finishes: data.get("finishes").and_then(|v| v.as_object()).cloned().unwrap_or_default(), extra: Map::new(),
            };
            let layer = layer_for(doc, &data);
            push_custom(doc, data.get("id").and_then(|v| v.as_str()).map(|x| x.to_string()), T_ROOM, serde_json::to_value(room).unwrap(), layer)
        }
        "add_wall" => {
            let level = parse_level(doc, &data)?;
            let (a, b) = (data.get("a").and_then(p2), data.get("b").and_then(p2));
            let (Some(a), Some(b)) = (a, b) else { return err("add_wall needs `a` and `b` plan points") };
            if dist2(a, b) < 0.01 { return err("wall is too short"); }
            let w = Wall { level, a, b, height: num(&data, "height"), thickness: num(&data, "thickness"), exterior: data.get("exterior").and_then(|v| v.as_bool()), finishes: Map::new(), extra: Map::new() };
            let layer = layer_for(doc, &data);
            push_custom(doc, data.get("id").and_then(|v| v.as_str()).map(|x| x.to_string()), T_WALL, serde_json::to_value(w).unwrap(), layer)
        }
        "add_opening" => add_opening(doc, &data),
        "add_roof" => {
            let mut r: Roof = serde_json::from_value(data.clone()).map_err(|e| OpError(format!("invalid roof: {e}")))?;
            if let Some(l) = r.levels { r.levels = Some(l.clamp(1, settings(doc).count())); }
            let layer = layer_for(doc, &data);
            push_custom(doc, data.get("id").and_then(|v| v.as_str()).map(|x| x.to_string()), T_ROOF, serde_json::to_value(r).unwrap(), layer)
        }
        "add_fixture" => {
            let level = parse_level(doc, &data)?;
            let mut v = data.clone();
            v["level"] = json!(level);
            if v.get("note").is_none() { v["note"] = json!(""); }
            let f: Fixture = serde_json::from_value(v).map_err(|e| OpError(format!("invalid fixture: {e}")))?;
            let layer = layer_for(doc, &data);
            push_custom(doc, data.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()), T_FIXTURE, serde_json::to_value(f).unwrap(), layer)
        }
        "add_run" => {
            let level = parse_level(doc, &data)?;
            let mut v = data.clone();
            v["level"] = json!(level);
            let r: Run = serde_json::from_value(v).map_err(|e| OpError(format!("invalid run: {e}")))?;
            if r.points.len() < 2 { return err("a run needs at least 2 points"); }
            let layer = layer_for(doc, &data);
            push_custom(doc, data.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()), T_RUN, serde_json::to_value(r).unwrap(), layer)
        }
        "add_stair" => {
            let level = parse_level(doc, &data)?;
            let st = settings(doc);
            let mut v = data.clone();
            v["level"] = json!(level);
            if v.get("risers").is_none() { v["risers"] = json!(((st.height(level) / 0.18).round() as usize).max(2)); }
            if v.get("name").is_none() { v["name"] = json!("Stair"); }
            let stair: Stair = serde_json::from_value(v).map_err(|e| OpError(format!("invalid stair: {e}")))?;
            if stair.risers < 2 { return err("a stair needs at least 2 risers"); }
            let layer = layer_for(doc, &data);
            push_custom(doc, data.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()), T_STAIR, serde_json::to_value(stair).unwrap(), layer)
        }
        "add_unit" => {
            let level = parse_level(doc, &data)?;
            let mut v = data.clone();
            v["level"] = json!(level);
            let type_ = v.get("type").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let (dw, dd, dh, dl) = unit_defaults(&type_);
            for (k, d) in [("width", dw), ("depth", dd), ("height", dh), ("lift", dl)] { if v.get(k).and_then(|x| x.as_f64()).is_none() { v[k] = json!(d); } }
            if v.get("name").is_none() { v["name"] = json!(if type_.is_empty() { "Unit".to_string() } else { type_.clone() }); }
            let mut u: Unit = serde_json::from_value(v).map_err(|e| OpError(format!("invalid unit: {e}")))?;
            check_unit_params(&u)?;
            let types = script::available_types(&settings(doc));
            if !type_.is_empty() && script::find_type(&types, &type_).is_some() {
                // a scripted type must build for these values, so a typo in a choice is caught here
                let built = script::with_runtime(&types, |rt| rt.solids(&type_, &script::given(&u))).map_err(|e| OpError(format!("component `{type_}`: {e}")))?;
                if built.is_empty() { return err(format!("component `{type_}` builds nothing for these values/choices")); }
                refresh_unit_bounds(&types, &mut u);
            }
            let layer = layer_for(doc, &data);
            let r = push_custom(doc, data.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()), T_UNIT, serde_json::to_value(u).unwrap(), layer);
            ensure_unit_materials(doc);
            r
        }
        "update" => {
            let id = data.get("id").and_then(|v| v.as_str()).ok_or_else(|| OpError("id is required".into()))?.to_string();
            let patch = data.get("patch").cloned().unwrap_or(json!({}));
            if !patch.is_object() { return err("patch must be an object"); }
            let types = script::available_types(&settings(doc));
            let c = custom_mut(doc, &id)?;
            deep_merge(&mut c.data, &patch);
            if c.type_ == T_UNIT {
                if let Ok(mut u) = serde_json::from_value::<Unit>(c.data.clone()) {
                    check_unit_params(&u)?;
                    if !u.type_.is_empty() && script::find_type(&types, &u.type_).is_some() {
                        let built = script::with_runtime(&types, |rt| rt.solids(&u.type_, &script::given(&u))).map_err(|e| OpError(format!("component `{}`: {e}", u.type_)))?;
                        if built.is_empty() { return err(format!("component `{}` builds nothing for these values/choices", u.type_)); }
                        refresh_unit_bounds(&types, &mut u);
                        c.data = serde_json::to_value(&u).unwrap();
                    }
                }
            }
            let e = Entity::Custom(c.clone());
            validate(&e)?;
            if matches!(&e, Entity::Custom(c) if c.type_ == T_UNIT) { ensure_unit_materials(doc); }
            Ok(OpResult { modified: vec![id], entities: vec![e], ..Default::default() })
        }
        "set_finish" => {
            let id = data.get("id").and_then(|v| v.as_str()).ok_or_else(|| OpError("id is required".into()))?.to_string();
            let face = data.get("face").and_then(|v| v.as_str()).ok_or_else(|| OpError("face is required".into()))?.to_string();
            let mat = data.get("material").and_then(|v| v.as_str()).ok_or_else(|| OpError("material is required".into()))?;
            let mid = resolve_material(doc, mat)?;
            let c = custom_mut(doc, &id)?;
            set_finish(c, &face, &mid);
            Ok(OpResult { modified: vec![id], entities: vec![Entity::Custom(c.clone())], ..Default::default() })
        }
        "import_house" => import_house(doc, &data),
        "summary" => Ok(with_summary(doc)),
        "list_catalogue" => Ok(list_catalogue(doc)),
        "component_params" => component_params(doc, &data),
        "define_component" => define_component(doc, &data),
        "remove_component_type" => {
            let id = data.get("id").and_then(|v| v.as_str()).ok_or_else(|| OpError("id is required".into()))?.to_string();
            let mut s = settings(doc);
            let n = s.component_types.len();
            s.component_types.retain(|t| t.id != id);
            if s.component_types.len() == n { return err(format!("no document component type `{id}` (built-in types cannot be removed)")); }
            save_settings(doc, &s);
            Ok(OpResult::default())
        }
        other => err(format!("unknown construction op `{other}`")),
    }
}

fn with_summary(doc: &Document) -> OpResult {
    // The summary travels in `entities` as a single synthetic text entity would be odd; use a
    // custom entity-shaped value so clients get structured JSON.
    let v = summary(doc);
    let c = Custom { id: "summary".into(), layer: String::new(), type_: "construction.summary".into(), data: v, extra: Map::new() };
    OpResult { entities: vec![Entity::Custom(c)], ..Default::default() }
}

fn data_result(type_: &str, v: Value) -> OpResult {
    let c = Custom { id: type_.into(), layer: String::new(), type_: format!("construction.{type_}"), data: v, extra: Map::new() };
    OpResult { entities: vec![Entity::Custom(c)], ..Default::default() }
}

/// Every component type a document can use, with its default parameters.
pub fn list_catalogue(doc: &Document) -> OpResult {
    let s = settings(doc);
    let types = script::available_types(&s);
    let rows: Vec<Value> = script::with_runtime(&types, |rt| types.iter().map(|t| {
        let builtin = !s.component_types.iter().any(|x| x.id == t.id);
        let params = rt.params(&t.id, &json!({})).unwrap_or_default();
        let bounds = rt.solids(&t.id, &json!({})).ok().and_then(|sol| script::bounds(&sol));
        json!({ "id": t.id, "name": t.name, "group": t.group, "builtin": builtin, "declared": rt.has(&t.id),
                "params": params, "size": bounds.map(|b| json!({ "width": round(b.width()), "depth": round(b.depth()), "height": round(b.height()), "lift": round(b.low) })),
                "problem": rt.problems().iter().find(|p| p.starts_with(&format!("{}:", t.id))) })
    }).collect());
    data_result("catalogue", json!({ "types": rows, "problems": script::with_runtime(&types, |rt| rt.problems().to_vec()) }))
}

/// Resolved parameters (after rules), meta, worktop and bounding box of a type for given values.
fn component_params(doc: &Document, data: &Value) -> Result<OpResult, OpError> {
    let type_ = data.get("type").and_then(|v| v.as_str()).ok_or_else(|| OpError("type is required".into()))?.to_string();
    let types = script::available_types(&settings(doc));
    if script::find_type(&types, &type_).is_none() { return err(format!("no component type `{type_}`")); }
    let mut given = Map::new();
    if let Some(v) = data.get("values").and_then(|v| v.as_object()) { for (k, v) in v { given.insert(k.clone(), v.clone()); } }
    if let Some(v) = data.get("choices").and_then(|v| v.as_object()) { for (k, v) in v { given.insert(k.clone(), v.clone()); } }
    let given = Value::Object(given);
    let out = script::with_runtime(&types, |rt| -> Result<Value, String> {
        let params = rt.params(&type_, &given)?;
        let solids = rt.solids(&type_, &given)?;
        let b = script::bounds(&solids);
        Ok(json!({ "type": type_, "params": params, "meta": rt.meta(&type_, &given), "worktop": rt.worktop(&type_, &given), "solids": solids.len(),
                   "size": b.map(|b| json!({ "width": round(b.width()), "depth": round(b.depth()), "height": round(b.height()), "lift": round(b.low) })) }))
    }).map_err(|e| OpError(format!("component `{type_}`: {e}")))?;
    Ok(data_result("component_params", out))
}

/// Store a component script in the document after checking it declares `id` and builds.
fn define_component(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let id = data.get("id").and_then(|v| v.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).ok_or_else(|| OpError("id is required".into()))?;
    let source = data.get("source").and_then(|v| v.as_str()).ok_or_else(|| OpError("source is required (JavaScript calling component(id, {...}))".into()))?.to_string();
    let (params, solids) = script::validate_source(&id, &source).map_err(|e| OpError(format!("component `{id}`: {e}")))?;
    let mut s = settings(doc);
    let t = ComponentType {
        id: id.clone(),
        name: data.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()).or_else(|| s.component_types.iter().find(|t| t.id == id).map(|t| t.name.clone())).unwrap_or_else(|| id.clone()),
        group: data.get("group").and_then(|v| v.as_str()).map(|s| s.to_string()).or_else(|| s.component_types.iter().find(|t| t.id == id).map(|t| t.group.clone())).unwrap_or_else(|| "Components".into()),
        source,
    };
    s.component_types.retain(|x| x.id != id);
    s.component_types.push(t);
    save_settings(doc, &s);
    // units of this type pick up the new script: refresh their sizes and materials
    let types = script::available_types(&s);
    let mut modified = vec![];
    for e in doc.entities.iter_mut() {
        if let Entity::Custom(c) = e {
            if c.type_ == T_UNIT {
                if let Ok(mut u) = serde_json::from_value::<Unit>(c.data.clone()) {
                    if u.type_ == id && refresh_unit_bounds(&types, &mut u) { c.data = serde_json::to_value(&u).unwrap(); modified.push(c.id.clone()); }
                }
            }
        }
    }
    ensure_unit_materials(doc);
    let b = script::bounds(&solids);
    let mut r = data_result("component", json!({ "id": id, "params": params, "solids": solids.len(), "size": b.map(|b| json!({ "width": round(b.width()), "depth": round(b.depth()), "height": round(b.height()), "lift": round(b.low) })) }));
    r.modified = modified;
    Ok(r)
}

/// Resolve a finish key (`brick`) or a material id/name to a material id.
pub fn resolve_material(doc: &Document, r: &str) -> Result<String, OpError> {
    if MATERIALS.iter().any(|(k, ..)| *k == r) { return Ok(material_id(r)); }
    if let Some(m) = doc.material_by_ref(r) { return Ok(m.id.clone()); }
    err(format!("no material or finish `{r}`"))
}

pub fn set_finish(c: &mut Custom, face: &str, material_id: &str) {
    if !c.data.is_object() { c.data = json!({}); }
    let obj = c.data.as_object_mut().unwrap();
    let f = obj.entry("finishes").or_insert_with(|| json!({}));
    if !f.is_object() { *f = json!({}); }
    f.as_object_mut().unwrap().insert(face.to_string(), json!(material_id));
}

pub fn deep_merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                match (t.get_mut(k), v) {
                    (Some(tv @ Value::Object(_)), Value::Object(_)) => deep_merge(tv, v),
                    _ => { t.insert(k.clone(), v.clone()); }
                }
            }
        }
        (t, p) => *t = p.clone(),
    }
}

pub fn validate(e: &Entity) -> Result<(), OpError> {
    let Entity::Custom(c) = e else { return Ok(()) };
    let bad = |what: &str, e: serde_json::Error| OpError(format!("invalid {what}: {e}"));
    match c.type_.as_str() {
        T_ROOM => { let r: Room = serde_json::from_value(c.data.clone()).map_err(|e| bad("room", e))?; if r.points.len() < 3 { return err("a room needs at least 3 points"); } }
        T_WALL => { serde_json::from_value::<Wall>(c.data.clone()).map_err(|e| bad("wall", e))?; }
        T_OPENING => { serde_json::from_value::<Opening>(c.data.clone()).map_err(|e| bad("opening", e))?; }
        T_ROOF => { serde_json::from_value::<Roof>(c.data.clone()).map_err(|e| bad("roof", e))?; }
        T_FIXTURE => { serde_json::from_value::<Fixture>(c.data.clone()).map_err(|e| bad("fixture", e))?; }
        T_RUN => { serde_json::from_value::<Run>(c.data.clone()).map_err(|e| bad("run", e))?; }
        T_STAIR => { let st: Stair = serde_json::from_value(c.data.clone()).map_err(|e| bad("stair", e))?; if st.risers < 2 { return err("a stair needs at least 2 risers"); } }
        T_UNIT => { serde_json::from_value::<Unit>(c.data.clone()).map_err(|e| bad("unit", e))?; }
        _ => {}
    }
    Ok(())
}

fn setup(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let mut s = settings(doc);
    if let Some(st) = data.get("storeys").and_then(|v| v.as_array()) {
        let mut out = vec![];
        for (i, v) in st.iter().enumerate() {
            let existing = s.storeys.get(i).cloned();
            out.push(Storey {
                id: v.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()).or_else(|| existing.as_ref().map(|e| e.id.clone())).unwrap_or_else(new_id),
                name: v.get("name").and_then(|x| x.as_str()).map(|x| x.to_string()).or_else(|| existing.as_ref().map(|e| e.name.clone())).unwrap_or_else(|| format!("Storey {}", i + 1)),
                height: num(v, "height").or(existing.as_ref().map(|e| e.height)).unwrap_or(2.6),
                extra: Map::new(),
            });
        }
        if !out.is_empty() { s.storeys = out; }
    }
    if let Some(g) = num(data, "groundLevel") { s.ground_level = g; }
    if let Some(t) = num(data, "wallThickness") { s.wall_thickness = t.max(0.01); }
    if let Some(t) = num(data, "floorThickness") { s.floor_thickness = t.max(0.01); }
    if let Some(c) = data.get("cavity") {
        if let Some(e) = c.get("enabled").and_then(|v| v.as_bool()) { s.cavity.enabled = e; }
        if let Some(x) = num(c, "innerLeaf") { s.cavity.inner_leaf = x; }
        if let Some(x) = num(c, "cavity") { s.cavity.cavity = x; }
        if let Some(x) = num(c, "outerLeaf") { s.cavity.outer_leaf = x; }
    }
    save_settings(doc, &s);
    // thicknesses live in the masonry build-up: write them there
    if data.get("cavity").is_some() || data.get("wallThickness").is_some() {
        crate::masonry::data::set_widths(doc, num(data, "wallThickness"), data.get("cavity").and_then(|c| num(c, "innerLeaf")), data.get("cavity").and_then(|c| num(c, "cavity")), data.get("cavity").and_then(|c| num(c, "outerLeaf")));
    }
    Ok(OpResult::default())
}

fn host_from(v: &Value) -> Result<Host, OpError> {
    let h = v.get("host").or_else(|| v.get("wall")).ok_or_else(|| OpError("host is required ({room, edge} or {wall})".into()))?;
    if let Some(s) = h.as_str() { return Ok(Host::Wall { wall: s.to_string() }); }
    serde_json::from_value::<Host>(h.clone()).map_err(|_| OpError("host must be {room, edge} or {wall}".into()))
}

/// Resolve a host to (plan segment a, b, level).
pub fn host_segment(doc: &Document, host: &Host) -> Option<(P2, P2, usize)> {
    match host {
        Host::RoomEdge { room, edge } => {
            let Entity::Custom(c) = doc.find(room)? else { return None };
            let r: Room = serde_json::from_value(c.data.clone()).ok()?;
            let n = r.points.len();
            if n < 3 { return None; }
            Some((r.points[edge % n], r.points[(edge + 1) % n], r.level))
        }
        Host::Wall { wall } => {
            let Entity::Custom(c) = doc.find(wall)? else { return None };
            let w: Wall = serde_json::from_value(c.data.clone()).ok()?;
            Some((w.a, w.b, w.level))
        }
    }
}

fn add_opening(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let kind: OpeningKind = serde_json::from_value(data.get("kind").cloned().unwrap_or(json!("door"))).map_err(|_| OpError("kind must be door, window, passage or garage".into()))?;
    let host = host_from(data)?;
    let (a, b, level) = host_segment(doc, &host).ok_or_else(|| OpError("host wall not found".into()))?;
    let (dw, dh, ds) = kind.defaults();
    let width = num(data, "width").unwrap_or(dw).max(0.05);
    let l = dist2(a, b);
    let along = num(data, "along").unwrap_or(l / 2.0).clamp((width / 2.0).min(l / 2.0), (l - width / 2.0).max(l / 2.0));
    let o = Opening { level, kind, host, along, width, height: num(data, "height").unwrap_or(dh).max(0.05), sill: num(data, "sill").unwrap_or(ds).max(0.0), extra: Map::new() };
    let layer = layer_for(doc, data);
    push_custom(doc, data.get("id").and_then(|v| v.as_str()).map(|x| x.to_string()), T_OPENING, serde_json::to_value(o).unwrap(), layer)
}

fn remove_storey(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let mut s = settings(doc);
    let i = data.get("index").and_then(|v| v.as_u64()).ok_or_else(|| OpError("index is required".into()))? as usize;
    if i >= s.storeys.len() { return err(format!("no storey {i}")); }
    if s.storeys.len() == 1 { return err("cannot remove the last storey"); }
    s.storeys.remove(i);
    save_settings(doc, &s);
    let mut r = OpResult::default();
    let mut keep = vec![];
    for e in std::mem::take(&mut doc.entities) {
        if let Entity::Custom(c) = &e {
            if c.type_.starts_with("construction.") && c.type_ != T_ROOF {
                let lvl = c.data.get("level").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                if lvl == i { r.removed.push(c.id.clone()); continue; }
                if lvl > i {
                    let mut c2 = c.clone();
                    c2.data["level"] = json!(lvl - 1);
                    r.modified.push(c2.id.clone());
                    keep.push(Entity::Custom(c2));
                    continue;
                }
            }
        }
        keep.push(e);
    }
    // openings whose host went away
    let ids: Vec<String> = keep.iter().map(|e| e.id().to_string()).collect();
    keep.retain(|e| match e {
        Entity::Custom(c) if c.type_ == T_OPENING => {
            let ok = serde_json::from_value::<Opening>(c.data.clone()).map(|o| match o.host { Host::RoomEdge { room, .. } => ids.contains(&room), Host::Wall { wall } => ids.contains(&wall) }).unwrap_or(false);
            if !ok { r.removed.push(c.id.clone()); }
            ok
        }
        _ => true,
    });
    doc.entities = keep;
    Ok(r)
}

// ---- Uzzetje import ----------------------------------------------------------------------------

fn import_house(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let h = data.get("house").ok_or_else(|| OpError("house is required (a Uzzetje HouseModel JSON document)".into()))?;
    if !h.is_object() { return err("house must be an object"); }
    let mut r = OpResult::default();
    let layer = doc.default_layer().to_string();
    // settings
    let mut s = settings(doc);
    let floors: Vec<Storey> = h.get("floors").and_then(|f| f.as_array()).map(|a| a.iter().enumerate().map(|(i, f)| Storey {
        id: f.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()).unwrap_or_else(new_id),
        name: f.get("name").and_then(|x| x.as_str()).map(|x| x.to_string()).unwrap_or_else(|| format!("Storey {}", i + 1)),
        height: num(f, "height").or(num(h, "storeyHeight")).unwrap_or(2.6),
        extra: Map::new(),
    }).collect()).unwrap_or_default();
    if !floors.is_empty() { s.storeys = floors; } else if let Some(sh) = num(h, "storeyHeight") { s.storeys[0].height = sh; }
    if let Some(t) = num(h, "wallThickness") { s.wall_thickness = t; }
    if let Some(t) = num(h, "floorThickness") { s.floor_thickness = t; }
    if let Some(c) = h.get("cavity") { if let Ok(c) = serde_json::from_value::<Cavity>(c.clone()) { s.cavity = c; } }
    if let Some(types) = h.get("componentTypes").and_then(|x| x.as_array()) {
        for t in types {
            let Ok(ct) = serde_json::from_value::<ComponentType>(t.clone()) else { continue };
            s.component_types.retain(|x| x.id != ct.id);
            s.component_types.push(ct);
        }
    }
    save_settings(doc, &s);
    let types = script::available_types(&s);
    let count = s.count();
    let lvl = |v: &Value| v.get("level").and_then(|x| x.as_u64()).map(|x| (x as usize).min(count - 1)).unwrap_or(0);
    // finishes map of the old format: "wall.room.<id>.<edge>.exterior", "wall.poly.<id>.<edge>.interior", "floor.<id>", "roof"
    let finishes = h.get("finishes").and_then(|f| f.as_object()).cloned().unwrap_or_default();
    let room_fin = |rid: &str, n: usize| -> Map<String, Value> {
        let mut out = Map::new();
        for e in 0..n {
            for side in ["interior", "exterior"] {
                for pfx in ["room", "poly"] {
                    if let Some(v) = finishes.get(&format!("wall.{pfx}.{rid}.{e}.{side}")).and_then(|v| v.as_str()) { out.insert(format!("wall.{e}.{side}"), json!(material_id(v))); }
                }
            }
        }
        if let Some(v) = finishes.get(&format!("floor.{rid}")).and_then(|v| v.as_str()) { out.insert("floor".into(), json!(material_id(v))); }
        out
    };
    let mut push = |doc: &mut Document, id: String, type_: &str, data: Value| {
        r.created.push(id.clone());
        doc.entities.push(Entity::Custom(Custom { id, layer: layer.clone(), type_: type_.to_string(), data, extra: Map::new() }));
    };
    // rooms (rect) and polyRooms → rooms
    for v in h.get("rooms").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let (Some(o), Some(sz)) = (v.get("origin").and_then(p2), v.get("size").and_then(p2)) else { continue };
        let id = v.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()).unwrap_or_else(new_id);
        let room = Room {
            name: v.get("name").and_then(|x| x.as_str()).unwrap_or("Room").to_string(), level: lvl(&v),
            points: vec![o, [o[0] + sz[0], o[1]], [o[0] + sz[0], o[1] + sz[1]], [o[0], o[1] + sz[1]]],
            has_floor: v.get("hasFloor").and_then(|x| x.as_bool()).unwrap_or(true), has_ceiling: v.get("hasCeiling").and_then(|x| x.as_bool()).unwrap_or(false),
            wall_height: num(&v, "wallHeight"), finishes: room_fin(&id, 4), extra: Map::new(),
        };
        push(doc, id, T_ROOM, serde_json::to_value(room).unwrap());
    }
    for v in h.get("polyRooms").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let points = p2s(v.get("points"));
        if points.len() < 3 { continue; }
        let id = v.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()).unwrap_or_else(new_id);
        let n = points.len();
        let room = Room {
            name: v.get("name").and_then(|x| x.as_str()).unwrap_or("Room").to_string(), level: lvl(&v), points,
            has_floor: v.get("hasFloor").and_then(|x| x.as_bool()).unwrap_or(true), has_ceiling: false,
            wall_height: num(&v, "wallHeight"), finishes: room_fin(&id, n), extra: Map::new(),
        };
        push(doc, id, T_ROOM, serde_json::to_value(room).unwrap());
    }
    for v in h.get("walls").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let (Some(a), Some(b)) = (v.get("a").and_then(p2), v.get("b").and_then(p2)) else { continue };
        let id = v.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()).unwrap_or_else(new_id);
        let w = Wall { level: lvl(&v), a, b, height: num(&v, "height"), thickness: None, exterior: v.get("exterior").and_then(|x| x.as_bool()), finishes: Map::new(), extra: Map::new() };
        push(doc, id, T_WALL, serde_json::to_value(w).unwrap());
    }
    let host_of = |w: &Value| -> Option<Host> {
        for k in ["roomEdge", "polyEdge", "room"] {
            if let Some(re) = w.get(k) {
                let room = re.get("room").and_then(|x| x.as_str())?.to_string();
                let edge = re.get("edge").and_then(|x| x.as_u64())? as usize;
                return Some(Host::RoomEdge { room, edge });
            }
        }
        if let Some(id) = w.get("wall").and_then(|x| x.as_str()) { return Some(Host::Wall { wall: id.to_string() }); }
        if let Some(id) = w.get("wall").and_then(|x| x.get("wall")).and_then(|x| x.as_str()) { return Some(Host::Wall { wall: id.to_string() }); }
        None
    };
    let kinds = |s: &str| -> Option<OpeningKind> { serde_json::from_value(json!(s)).ok() };
    for v in h.get("openings").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let Some(kind) = v.get("kind").and_then(|x| x.as_str()).and_then(kinds) else { continue };
        let Some(host) = v.get("wall").and_then(host_of) else { continue };
        let id = v.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()).unwrap_or_else(new_id);
        let (dw, dh, ds) = kind.defaults();
        let o = Opening { level: lvl(&v), kind, host, along: num(&v, "along").unwrap_or(0.5), width: num(&v, "width").unwrap_or(dw), height: num(&v, "height").unwrap_or(dh), sill: num(&v, "sill").unwrap_or(ds), extra: Map::new() };
        push(doc, id, T_OPENING, serde_json::to_value(o).unwrap());
    }
    // roofs
    let base_roof: Roof = h.get("roof").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    let volumes = h.get("roofVolumes").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    if volumes.is_empty() {
        if base_roof.enabled {
            let mut rf = base_roof.clone();
            rf.levels = Some(count);
            if let Some(v) = finishes.get("roof").and_then(|v| v.as_str()) { rf.finishes.insert("roof".into(), json!(material_id(v))); }
            push(doc, new_id(), T_ROOF, serde_json::to_value(rf).unwrap());
        }
    } else {
        for v in volumes {
            let mut rf: Roof = serde_json::from_value(v.clone()).unwrap_or_else(|_| base_roof.clone());
            if rf.min.is_none() { rf.min = v.get("min").and_then(p2); }
            if rf.max.is_none() { rf.max = v.get("max").and_then(p2); }
            rf.levels = Some(rf.levels.unwrap_or(count).clamp(1, count));
            if let Some(f) = finishes.get("roof").and_then(|v| v.as_str()) { rf.finishes.insert("roof".into(), json!(material_id(f))); }
            let id = v.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()).unwrap_or_else(new_id);
            push(doc, id, T_ROOF, serde_json::to_value(rf).unwrap());
        }
    }
    for v in h.get("fixtures").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let (Some(kind), Some(pos)) = (v.get("kind").and_then(|k| serde_json::from_value::<FixtureKind>(k.clone()).ok()), v.get("position").and_then(p3)) else { continue };
        let id = v.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()).unwrap_or_else(new_id);
        let f = Fixture { level: lvl(&v), kind, position: pos, host: v.get("wall").and_then(host_of), rotation: num(&v, "rotation"), note: v.get("note").and_then(|x| x.as_str()).unwrap_or("").to_string(), extra: Map::new() };
        push(doc, id, T_FIXTURE, serde_json::to_value(f).unwrap());
    }
    for v in h.get("runs").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let Some(system) = v.get("system").and_then(|k| serde_json::from_value::<System>(k.clone()).ok()) else { continue };
        let points = p3s(v.get("points"));
        if points.len() < 2 { continue; }
        let id = v.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()).unwrap_or_else(new_id);
        let run = Run { level: lvl(&v), system, points, diameter: num(&v, "diameter"), extra: Map::new() };
        push(doc, id, T_RUN, serde_json::to_value(run).unwrap());
    }
    // stairs
    for v in h.get("stairs").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let mut v = v.clone();
        let level = lvl(&v);
        v["level"] = json!(level);
        if v.get("risers").is_none() { v["risers"] = json!(((s.height(level) / 0.18).round() as usize).max(2)); }
        let Ok(st) = serde_json::from_value::<Stair>(v.clone()) else { continue };
        if st.risers < 2 { continue; }
        let id = v.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()).unwrap_or_else(new_id);
        push(doc, id, T_STAIR, serde_json::to_value(st).unwrap());
    }
    // parametric components → unit boxes
    for v in h.get("components").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let Some(origin) = v.get("origin").and_then(p2) else { continue };
        let type_ = v.get("type").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let (dw, dd, dh, dl) = unit_defaults(&type_);
        let vals = v.get("values").cloned().unwrap_or(json!({}));
        let mut u = Unit {
            name: v.get("name").and_then(|x| x.as_str()).unwrap_or(&type_).to_string(), type_, level: lvl(&v), origin,
            bearing: num(&v, "bearing").unwrap_or(0.0),
            width: num(&vals, "width").or(num(&vals, "length")).or(num(&vals, "diameter")).unwrap_or(dw).max(0.05),
            depth: num(&vals, "depth").or(num(&vals, "diameter")).unwrap_or(dd).max(0.05),
            height: num(&vals, "height").unwrap_or(dh).max(0.01), lift: num(&vals, "lift").unwrap_or(dl),
            values: vals.as_object().cloned().unwrap_or_default().into_iter().filter(|(_, x)| x.is_number()).collect(),
            choices: v.get("choices").and_then(|x| x.as_object()).cloned().unwrap_or_default().into_iter().filter(|(_, x)| x.is_string()).collect(),
            extra: Map::new(),
        };
        refresh_unit_bounds(&types, &mut u);
        let id = v.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()).unwrap_or_else(new_id);
        push(doc, id, T_UNIT, serde_json::to_value(u).unwrap());
    }
    // free-form shapes (trees, neighbours, solar panels, garden) → core face groups on a "Site" layer
    let shapes = h.get("shapes").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    if !shapes.is_empty() {
        let site = match doc.layers.iter().find(|l| l.name == "Site") {
            Some(l) => l.id.clone(),
            None => { let id = new_id(); doc.layers.push(Layer { id: id.clone(), name: "Site".into(), visible: true, color: "#7fa85a".into() }); id }
        };
        for v in shapes {
            if v.get("role").and_then(|x| x.as_str()) == Some("void") { continue; }
            let Some(brep) = v.get("brep") else { continue };
            let points = p3s(brep.get("points"));
            let shape_finish = v.get("finish").and_then(|x| x.as_str()).map(|x| x.to_string());
            let mut faces = vec![];
            for f in brep.get("faces").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                let idx = |v: &Value| -> Vec<P3> { v.as_array().map(|a| a.iter().filter_map(|i| i.as_u64()).filter_map(|i| points.get(i as usize).copied()).collect()).unwrap_or_default() };
                let outer = f.get("outer").map(idx).unwrap_or_default();
                if outer.len() < 3 { continue; }
                let holes: Vec<Vec<P3>> = f.get("holes").and_then(|x| x.as_array()).map(|a| a.iter().map(idx).filter(|h| h.len() >= 3).collect()).unwrap_or_default();
                let finish = f.get("finish").and_then(|x| x.as_str()).map(|x| x.to_string()).or_else(|| shape_finish.clone());
                let material = finish.as_deref().and_then(|k| if MATERIALS.iter().any(|(m, ..)| *m == k) { Some(material_id(k)) } else { doc.material_by_ref(k).map(|m| m.id.clone()) });
                faces.push(Entity::Face(Face { id: new_id(), layer: site.clone(), outer, holes, material: material.clone(), back_material: material, extra: Map::new() }));
            }
            if faces.is_empty() { continue; }
            let id = v.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()).unwrap_or_else(new_id);
            r.created.push(id.clone());
            doc.entities.push(Entity::Group(Group { id, layer: site.clone(), name: v.get("name").and_then(|x| x.as_str()).unwrap_or("Shape").to_string(), transform: mat_identity(), entities: faces, extra: Map::new() }));
        }
    }
    // guides → core guides
    for v in h.get("guides").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let Some(point) = v.get("origin").or(v.get("point")).and_then(p3) else { continue };
        let id = v.get("id").and_then(|x| x.as_str()).map(|x| x.to_string()).unwrap_or_else(new_id);
        r.created.push(id.clone());
        doc.entities.push(Entity::Guide(Guide { id, layer: layer.clone(), point, direction: v.get("direction").and_then(p3), extra: Map::new() }));
    }
    ensure_unit_materials(doc);
    Ok(r)
}

// ---- summary & metrics ---------------------------------------------------------------------------

pub fn summary(doc: &Document) -> Value {
    let m = expand::model(doc);
    let set = expand::collect(&m);
    let s = &m.settings;
    let storeys: Vec<Value> = s.storeys.iter().enumerate().map(|(i, st)| {
        let rooms: Vec<Value> = m.rooms.iter().filter(|(_, r)| r.level == i).map(|(c, r)| {
            let n = r.points.len();
            let openings = m.openings.iter().filter(|(_, o)| matches!(&o.host, Host::RoomEdge { room, .. } if room == &c.id)).count();
            json!({ "id": c.id, "name": r.name, "area": round(r.area()), "perimeter": round(r.perimeter()), "walls": n, "openings": openings,
                    "wallHeight": r.wall_height.unwrap_or_else(|| s.height(i)) })
        }).collect();
        let wall_length: f64 = set.walls.iter().filter(|w| w.level == i).map(|w| w.length()).sum();
        json!({ "index": i, "id": st.id, "name": st.name, "height": st.height, "base": round(s.base(i)), "rooms": rooms,
                "floorArea": round(m.rooms.iter().filter(|(_, r)| r.level == i).map(|(_, r)| r.area()).sum()), "wallLength": round(wall_length) })
    }).collect();
    json!({ "storeys": storeys, "wallThickness": s.wall_thickness, "cavity": s.cavity, "roofs": m.roofs.len(), "fixtures": m.fixtures.len(), "runs": m.runs.len(), "openings": m.openings.len(), "stairs": m.stairs.len(), "units": m.units.len(), "metrics": metrics(doc) })
}

fn round(v: f64) -> f64 { (v * 1000.0).round() / 1000.0 }

pub fn metrics(doc: &Document) -> Value {
    let m = expand::model(doc);
    let set = expand::collect(&m);
    let s = &m.settings;
    let floor_area: f64 = m.rooms.iter().map(|(_, r)| r.area()).sum();
    let volume: f64 = m.rooms.iter().map(|(_, r)| r.area() * r.wall_height.unwrap_or_else(|| s.height(r.level))).sum();
    let wall_length: f64 = set.walls.iter().map(|w| w.length()).sum();
    let ext: f64 = set.walls.iter().filter(|w| w.exterior).map(|w| w.gross_area() - w.opening_area()).sum();
    let int: f64 = set.walls.iter().filter(|w| !w.exterior).map(|w| w.gross_area() - w.opening_area()).sum();
    let window_area: f64 = m.openings.iter().filter(|(_, o)| o.kind == OpeningKind::Window).map(|(_, o)| o.width * o.height).sum();
    let rooms: Vec<Value> = m.rooms.iter().map(|(c, r)| {
        let openings = m.openings.iter().filter(|(_, o)| matches!(&o.host, Host::RoomEdge { room, .. } if room == &c.id)).count();
        json!({ "id": c.id, "name": r.name, "level": r.level, "area": round(r.area()), "perimeter": round(r.perimeter()), "openings": openings })
    }).collect();
    let scripted = script::with_runtime(&m.types, |rt| m.units.iter().filter(|(_, u)| script::unit_solids(rt, u).is_some()).count());
    json!({ "floorArea": round(floor_area), "wallLength": round(wall_length), "exteriorWallArea": round(ext), "interiorWallArea": round(int), "volume": round(volume),
            "openingCount": m.openings.len(), "windowArea": round(window_area), "storeys": s.count(), "stairs": m.stairs.len(), "units": m.units.len(), "scriptedUnits": scripted, "rooms": rooms })
}
