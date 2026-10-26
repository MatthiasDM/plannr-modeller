//! Ops of the electrical plugin.

use plannr_core::geom::{len, sub, P3};
use plannr_core::model::{Custom, Document, Entity};
use plannr_core::ops::{OpError, OpResult};
use serde_json::{json, Map, Value};

use crate::arei;
use crate::data::*;
use crate::svg;

fn err<T>(m: impl Into<String>) -> Result<T, OpError> { Err(OpError(m.into())) }
fn num(v: &Value, k: &str) -> Option<f64> { v.get(k).and_then(|x| x.as_f64()) }
fn int(v: &Value, k: &str) -> Option<u32> { v.get(k).and_then(|x| x.as_u64()).map(|x| x as u32) }
fn st(v: &Value, k: &str) -> Option<String> { v.get(k).and_then(|x| x.as_str()).map(|x| x.to_string()) }
fn ids(v: Option<&Value>) -> Vec<String> { v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default() }
fn p3s(v: Option<&Value>) -> Vec<P3> {
    v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|p| { let q = p.as_array()?; if q.len() < 3 { return None; } Some([q[0].as_f64()?, q[1].as_f64()?, q[2].as_f64()?]) }).collect()).unwrap_or_default()
}
fn data_result(name: &str, v: Value) -> OpResult {
    let c = Custom { id: name.into(), layer: String::new(), type_: format!("electrical.{name}"), data: v, extra: Map::new() };
    OpResult { entities: vec![Entity::Custom(c)], ..Default::default() }
}
fn first_layer(doc: &Document) -> String { doc.layers.first().map(|l| l.id.clone()).unwrap_or_default() }

/// Every conduit entity with its id.
pub fn conduits(doc: &Document) -> Vec<(String, Conduit)> {
    doc.entities.iter().filter_map(|e| match e { Entity::Custom(c) if c.type_ == T_CONDUIT => serde_json::from_value::<Conduit>(c.data.clone()).ok().map(|k| (c.id.clone(), k)), _ => None }).collect()
}

fn next_number(s: &Settings) -> u32 { s.circuits.iter().map(|c| c.number).max().unwrap_or(0) + 1 }

fn apply_circuit_fields(c: &mut Circuit, data: &Value, phases: u32) -> Result<(), OpError> {
    if let Some(n) = st(data, "name") { c.name = n; }
    if let Some(t) = st(data, "type") { if !CIRCUIT_TYPES.contains(&t.as_str()) { return err(format!("unknown circuit type `{t}`")); } c.type_ = t; }
    if let Some(b) = int(data, "breaker") { c.breaker = b; }
    if let Some(x) = num(data, "cableSection") { c.cable_section = x; }
    if let Some(r) = st(data, "rcd") { if !RCD_KINDS.contains(&r.as_str()) { return err(format!("rcd must be one of {:?}", RCD_KINDS)); } c.rcd = r; }
    if let Some(p) = int(data, "poles") { c.poles = Some(p); }
    if let Some(cv) = st(data, "curve") { c.curve = Some(cv); }
    if data.get("cableType").is_some() { c.cable_type = st(data, "cableType"); }
    if let Some(n) = st(data, "note") { c.note = n; }
    if let Some(n) = int(data, "number") { c.number = n; }
    if data.get("points").is_some() { c.points = ids(data.get("points")); }
    let _ = phases;
    Ok(())
}

pub fn apply(name: &str, data: Value, doc: &mut Document) -> Result<OpResult, OpError> {
    ensure(doc);
    match name {
        "set_board" => {
            let mut s = settings(doc);
            let b = &mut s.board;
            if let Some(p) = int(&data, "phases") { if p != 1 && p != 3 { return err("phases must be 1 or 3"); } b.phases = p; }
            if let Some(a) = int(&data, "mainBreaker") { b.main_breaker = a; }
            if let Some(d) = int(&data, "mainDifferential") { b.main_differential = d; }
            if let Some(x) = st(&data, "differentialType") { b.differential_type = x; }
            if let Some(x) = st(&data, "earthing") { b.earthing = x; }
            if let Some(x) = num(&data, "shortCircuitKA") { b.short_circuit_ka = x; }
            if let Some(x) = st(&data, "defaultCableType") { b.default_cable_type = x; }
            if let Some(x) = st(&data, "address") { b.address = x; }
            if let Some(x) = st(&data, "ean") { b.ean = x; }
            if let Some(x) = st(&data, "meterNumber") { b.meter_number = x; }
            if data.get("panel").is_some() { b.panel = st(&data, "panel"); }
            save_settings(doc, &s);
            Ok(data_result("board", serde_json::to_value(&s.board).unwrap()))
        }
        "add_circuit" => {
            let mut s = settings(doc);
            let t = st(&data, "type").unwrap_or_else(|| "sockets".into());
            if !CIRCUIT_TYPES.contains(&t.as_str()) { return err(format!("unknown circuit type `{t}`")); }
            let info = type_info(&t);
            let mut c = Circuit { id: new_id(), number: next_number(&s), name: st(&data, "name").unwrap_or_else(|| info.label.to_string()), type_: t.clone(), breaker: info.breaker, cable_section: info.section, rcd: if t == "pv" { "none".into() } else { "mA30".into() }, poles: None, curve: if t == "pv" { Some("C".into()) } else { None }, cable_type: None, points: vec![], note: String::new(), extra: Map::new() };
            apply_circuit_fields(&mut c, &data, s.board.phases)?;
            let id = c.id.clone();
            for other in s.circuits.iter_mut() { other.points.retain(|p| !c.points.contains(p)); }
            s.circuits.push(c.clone());
            save_settings(doc, &s);
            Ok(OpResult { created: vec![id.clone()], entities: vec![Entity::Custom(Custom { id, layer: String::new(), type_: "electrical.circuit".into(), data: serde_json::to_value(&c).unwrap(), extra: Map::new() })], ..Default::default() })
        }
        "update_circuit" => {
            let mut s = settings(doc);
            let id = st(&data, "id").ok_or_else(|| OpError("id is required".into()))?;
            let phases = s.board.phases;
            let new_points = data.get("points").map(|p| ids(Some(p)));
            if let Some(pts) = &new_points { for other in s.circuits.iter_mut().filter(|c| c.id != id) { other.points.retain(|p| !pts.contains(p)); } }
            let c = s.circuits.iter_mut().find(|c| c.id == id).ok_or_else(|| OpError(format!("no circuit {id}")))?;
            apply_circuit_fields(c, &data, phases)?;
            let v = serde_json::to_value(&*c).unwrap();
            save_settings(doc, &s);
            Ok(OpResult { modified: vec![id.clone()], entities: vec![Entity::Custom(Custom { id, layer: String::new(), type_: "electrical.circuit".into(), data: v, extra: Map::new() })], ..Default::default() })
        }
        "remove_circuit" => {
            let mut s = settings(doc);
            let id = st(&data, "id").ok_or_else(|| OpError("id is required".into()))?;
            let n = s.circuits.len(); s.circuits.retain(|c| c.id != id);
            if s.circuits.len() == n { return err(format!("no circuit {id}")); }
            for g in s.rcd_groups.iter_mut() { g.circuits.retain(|c| c != &id); }
            save_settings(doc, &s);
            let mut modified = vec![];
            for e in doc.entities.iter_mut() { if let Entity::Custom(c) = e { if c.type_ == T_CONDUIT && c.data.get("circuit").and_then(|v| v.as_str()) == Some(id.as_str()) { c.data["circuit"] = Value::Null; modified.push(c.id.clone()); } } }
            Ok(OpResult { removed: vec![id], modified, ..Default::default() })
        }
        "renumber" => {
            let mut s = settings(doc);
            for (i, c) in s.circuits.iter_mut().enumerate() { c.number = i as u32 + 1; }
            save_settings(doc, &s);
            Ok(OpResult::default())
        }
        "assign" => {
            let mut s = settings(doc);
            let h = House::read(doc);
            let points = ids(data.get("points"));
            if points.is_empty() { return err("points is required"); }
            for p in &points { let f = h.fixture(p).ok_or_else(|| OpError(format!("no fixture {p}")))?; if !ELECTRICAL_KINDS.contains(&f.kind.as_str()) || f.kind == "panel" { return err(format!("{p} is not an electrical point ({})", f.kind)); } }
            let circuit = st(&data, "circuit");
            if let Some(cid) = &circuit { if !s.circuits.iter().any(|c| &c.id == cid) { return err(format!("no circuit {cid}")); } }
            for c in s.circuits.iter_mut() { c.points.retain(|p| !points.contains(p)); }
            if let Some(cid) = &circuit { let c = s.circuits.iter_mut().find(|c| &c.id == cid).unwrap(); for p in &points { if !c.points.contains(p) { c.points.push(p.clone()); } } }
            save_settings(doc, &s);
            Ok(OpResult { modified: points, ..Default::default() })
        }
        "add_rcd_group" => {
            let mut s = settings(doc);
            let g = RcdGroup { id: new_id(), name: st(&data, "name").unwrap_or_else(|| format!("RCD {}", s.rcd_groups.len() + 1)), milliamps: int(&data, "milliamps").unwrap_or(30), type_: st(&data, "type").unwrap_or_else(|| "A".into()), circuits: ids(data.get("circuits")) };
            let id = g.id.clone(); s.rcd_groups.push(g); save_settings(doc, &s);
            Ok(OpResult { created: vec![id], ..Default::default() })
        }
        "update_rcd_group" => {
            let mut s = settings(doc);
            let id = st(&data, "id").ok_or_else(|| OpError("id is required".into()))?;
            let g = s.rcd_groups.iter_mut().find(|g| g.id == id).ok_or_else(|| OpError(format!("no RCD group {id}")))?;
            if let Some(n) = st(&data, "name") { g.name = n; }
            if let Some(m) = int(&data, "milliamps") { g.milliamps = m; }
            if let Some(t) = st(&data, "type") { g.type_ = t; }
            if data.get("circuits").is_some() { g.circuits = ids(data.get("circuits")); }
            save_settings(doc, &s);
            Ok(OpResult { modified: vec![id], ..Default::default() })
        }
        "remove_rcd_group" => {
            let mut s = settings(doc);
            let id = st(&data, "id").ok_or_else(|| OpError("id is required".into()))?;
            let n = s.rcd_groups.len(); s.rcd_groups.retain(|g| g.id != id);
            if s.rcd_groups.len() == n { return err(format!("no RCD group {id}")); }
            save_settings(doc, &s);
            Ok(OpResult { removed: vec![id], ..Default::default() })
        }
        "add_conduit" => {
            let h = House::read(doc);
            let points = p3s(data.get("points"));
            if points.len() < 2 { return err("points needs at least 2 [x, yAboveFloor, z] entries"); }
            let from = st(&data, "from"); let to = st(&data, "to");
            let level = int(&data, "level").map(|l| l as usize).or_else(|| from.as_ref().and_then(|f| h.fixture(f)).map(|f| f.level)).or_else(|| to.as_ref().and_then(|f| h.fixture(f)).map(|f| f.level)).unwrap_or(0);
            let circuit = st(&data, "circuit");
            if let Some(cid) = &circuit { if !settings(doc).circuits.iter().any(|c| &c.id == cid) { return err(format!("no circuit {cid}")); } }
            let k = Conduit { level, circuit, from, to, points, diameter: num(&data, "diameter").unwrap_or(0.020), kind: st(&data, "kind").unwrap_or_else(|| "conduit".into()), cable_type: st(&data, "cableType"), auto: false, extra: Map::new() };
            Ok(push_conduit(doc, k))
        }
        "update_conduit" => {
            let id = st(&data, "id").ok_or_else(|| OpError("id is required".into()))?;
            let patch = data.get("patch").and_then(|v| v.as_object()).cloned().ok_or_else(|| OpError("patch must be an object".into()))?;
            if let Some(d) = patch.get("diameter") { let d = d.as_f64().unwrap_or(-1.0); if ![0.016, 0.02, 0.025].iter().any(|x| (x - d).abs() < 1e-9) { return err("diameter must be 0.016, 0.02 or 0.025"); } }
            if let Some(k) = patch.get("kind") { if !matches!(k.as_str(), Some("conduit") | Some("cable")) { return err("kind must be conduit or cable"); } }
            if let Some(c) = patch.get("circuit") { if let Some(cid) = c.as_str() { if !settings(doc).circuits.iter().any(|x| x.id == cid) { return err(format!("no circuit {cid}")); } } }
            if let Some(p) = patch.get("points") { if p3s(Some(p)).len() < 2 { return err("points needs at least 2 [x, yAboveFloor, z] entries"); } }
            let e = doc.entities.iter_mut().find_map(|e| match e { Entity::Custom(c) if c.id == id && c.type_ == T_CONDUIT => Some(c), _ => None }).ok_or_else(|| OpError(format!("no conduit {id}")))?;
            let mut obj = e.data.as_object().cloned().unwrap_or_default();
            for (k, v) in patch { if v.is_null() { obj.remove(&k); } else { obj.insert(k, v); } }
            obj.insert("auto".into(), Value::Bool(false));
            let k: Conduit = serde_json::from_value(Value::Object(obj)).map_err(|x| OpError(x.to_string()))?;
            e.data = serde_json::to_value(&k).unwrap();
            let ent = Entity::Custom(e.clone());
            Ok(OpResult { modified: vec![id], entities: vec![ent], measurement: Some(k.length()), ..Default::default() })
        }
        "auto_route" => auto_route(doc, &data),
        "check" => {
            let s = settings(doc); let h = House::read(doc);
            let f: Vec<Value> = arei::check(&s, &h, &conduits(doc)).iter().map(|x| x.to_json()).collect();
            Ok(data_result("findings", json!({ "findings": f, "errors": f.iter().filter(|x| x["severity"] == "error").count(), "warnings": f.iter().filter(|x| x["severity"] == "warning").count() })))
        }
        "summary" => Ok(data_result("summary", summary(doc))),
        "situation_svg" => {
            let s = settings(doc); let h = House::read(doc);
            let level = int(&data, "level").unwrap_or(0) as usize;
            let o = svg::SitOpts { level, scale: num(&data, "scale").unwrap_or(40.0).max(5.0), show_conduits: data.get("showConduits").and_then(|v| v.as_bool()).unwrap_or(true), title: st(&data, "title").unwrap_or_default() };
            let text = svg::situation(&s, &h, &conduits(doc), &o);
            Ok(data_result("svg", json!({ "svg": text, "level": level, "storey": h.storey_name(level) })))
        }
        "oneline_svg" => {
            let s = settings(doc); let h = House::read(doc);
            let text = svg::oneline(&s, &h, &st(&data, "title").unwrap_or_default());
            Ok(data_result("svg", json!({ "svg": text })))
        }
        "import_electrical" => import_electrical(doc, &data),
        other => err(format!("unknown electrical op `{other}`")),
    }
}

fn push_conduit(doc: &mut Document, k: Conduit) -> OpResult {
    let c = Custom { id: new_id(), layer: first_layer(doc), type_: T_CONDUIT.into(), data: serde_json::to_value(&k).unwrap(), extra: Map::new() };
    let id = c.id.clone();
    let e = Entity::Custom(c);
    doc.entities.push(e.clone());
    OpResult { created: vec![id], entities: vec![e], measurement: Some(k.length()), ..Default::default() }
}

/// Conduits from the board to every point of a circuit (ceiling / floor / straight).
fn auto_route(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let s = settings(doc); let h = House::read(doc);
    let mode = st(data, "mode").unwrap_or_else(|| "ceiling".into());
    if !["ceiling", "floor", "straight"].contains(&mode.as_str()) { return err("mode must be ceiling, floor or straight"); }
    let only = st(data, "circuit");
    if let Some(cid) = &only { if !s.circuits.iter().any(|c| &c.id == cid) { return err(format!("no circuit {cid}")); } }
    let panels = h.panels();
    if panels.is_empty() { return err("no distribution board: place a construction fixture of kind `panel` first"); }
    let board_panel = s.board.panel.as_ref().and_then(|id| h.fixture(id)).cloned();
    // remove previous auto conduits of the circuits we route
    let targets: Vec<&Circuit> = s.circuits.iter().filter(|c| only.as_ref().map(|o| o == &c.id).unwrap_or(true)).collect();
    let target_ids: Vec<&str> = targets.iter().map(|c| c.id.as_str()).collect();
    let mut removed = vec![];
    doc.entities.retain(|e| match e {
        Entity::Custom(c) if c.type_ == T_CONDUIT => {
            let auto = c.data.get("auto").and_then(|v| v.as_bool()).unwrap_or(false);
            let cid = c.data.get("circuit").and_then(|v| v.as_str()).unwrap_or("");
            if auto && target_ids.contains(&cid) { removed.push(c.id.clone()); false } else { true }
        }
        _ => true,
    });
    let router = crate::route::Router::new(doc);
    let mut created = vec![]; let mut total = 0.0;
    for c in targets {
        let mut risers: Vec<(usize, usize)> = vec![];
        let diameter = if c.cable_section >= 6.0 { 0.025 } else if c.cable_section >= 2.5 { 0.020 } else { 0.016 };
        for pid in &c.points {
            let Some(pt) = h.fixture(pid) else { continue };
            let panel = board_panel.clone().or_else(|| panels.iter().find(|p| p.level == pt.level).cloned().cloned()).unwrap_or_else(|| panels[0].clone());
            let Some(route) = router.route(&panel.id, pid, &mode) else { continue };
            let mut k = Conduit { level: route.main.level, circuit: Some(c.id.clone()), from: Some(panel.id.clone()), to: Some(pid.clone()), points: route.main.points, diameter, kind: "conduit".into(), cable_type: None, auto: true, extra: Map::new() };
            if let Some(hh) = num(data, "height") { for p in k.points.iter_mut().skip(1) { if (p[1] - (h.height(k.level) - 0.15)).abs() < 1e-6 { p[1] = hh; } } }
            total += k.length();
            if !route.risers.is_empty() && !risers.contains(&(panel.level, pt.level)) {
                risers.push((panel.level, pt.level));
                for leg in route.risers {
                    let r = Conduit { level: leg.level, circuit: Some(c.id.clone()), from: Some(panel.id.clone()), to: None, points: leg.points, diameter, kind: "conduit".into(), cable_type: None, auto: true, extra: Map::from_iter([("riser".to_string(), Value::Bool(true))]) };
                    total += r.length();
                    created.extend(push_conduit(doc, r).created);
                }
            }
            k.extra.insert("mode".into(), Value::String(mode.clone()));
            created.extend(push_conduit(doc, k).created);
        }
    }
    Ok(OpResult { created, removed, measurement: Some(total), ..Default::default() })
}

pub fn summary(doc: &Document) -> Value {
    let s = settings(doc); let h = House::read(doc); let ks = conduits(doc);
    let assigned: Vec<&str> = s.circuits.iter().flat_map(|c| c.points.iter().map(|p| p.as_str())).collect();
    let points = h.electrical_points();
    let unassigned = points.iter().filter(|f| !assigned.contains(&f.id.as_str())).count();
    let circuits: Vec<Value> = s.circuits.iter().map(|c| {
        let n = arei::point_count(c, &h);
        json!({ "id": c.id, "number": c.number, "name": c.name, "type": c.type_, "breaker": c.breaker, "cableSection": c.cable_section, "rcd": c.rcd, "poles": c.resolved_poles(s.board.phases), "curve": c.resolved_curve(), "pointCount": n, "load_kW": (c.load_kw(n) * 100.0).round() / 100.0, "cable": c.cable_label(&s.board),
                "conduitLength_m": (ks.iter().filter(|(_, k)| k.circuit.as_deref() == Some(c.id.as_str())).map(|(_, k)| k.length()).sum::<f64>() * 100.0).round() / 100.0 })
    }).collect();
    json!({ "board": s.board, "circuits": circuits, "rcdGroups": s.rcd_groups, "points": { "total": points.len(), "assigned": points.len() - unassigned, "unassigned": unassigned },
            "demand_kW": (arei::total_demand(&s, &h) * 100.0).round() / 100.0, "capacity_kW": (s.board.capacity_kw() * 100.0).round() / 100.0, "conduitLength_m": (ks.iter().map(|(_, k)| k.length()).sum::<f64>() * 100.0).round() / 100.0, "conduits": ks.len() })
}

pub fn metrics(doc: &Document) -> Value {
    let s = settings(doc); let h = House::read(doc); let ks = conduits(doc);
    let assigned: Vec<&str> = s.circuits.iter().flat_map(|c| c.points.iter().map(|p| p.as_str())).collect();
    let points = h.electrical_points(); let a = points.iter().filter(|f| assigned.contains(&f.id.as_str())).count();
    json!({ "circuits": s.circuits.len(), "points": points.len(), "assigned": a, "unassigned": points.len() - a, "conduitLength": (ks.iter().map(|(_, k)| k.length()).sum::<f64>() * 1000.0).round() / 1000.0 + 0.0, "demand_kW": (arei::total_demand(&s, &h) * 100.0).round() / 100.0 })
}

/// Import Studio's `electrical` block.
fn import_electrical(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let e = data.get("electrical").ok_or_else(|| OpError("electrical is required (Studio's electrical block)".into()))?;
    let h = House::read(doc);
    let mut s = settings(doc);
    let b = &mut s.board;
    if let Some(p) = int(e, "phases") { b.phases = p; }
    if let Some(a) = int(e, "mainBreaker") { b.main_breaker = a; }
    if let Some(d) = e.get("hasMainDifferential").and_then(|v| v.as_bool()) { b.main_differential = if d { 300 } else { 0 }; }
    if let Some(x) = st(e, "differentialType") { b.differential_type = x; }
    if let Some(x) = st(e, "earthing") { b.earthing = x; }
    if let Some(x) = num(e, "shortCircuitKA") { b.short_circuit_ka = x; }
    if let Some(x) = st(e, "defaultCableType") { b.default_cable_type = x; }
    if let Some(x) = st(e, "address") { b.address = x; }
    if let Some(x) = st(e, "ean") { b.ean = x; }
    if let Some(x) = st(e, "meterNumber") { b.meter_number = x; }
    if b.panel.is_none() { b.panel = h.panels().first().map(|p| p.id.clone()); }
    // Studio's fixture list lets us match ids that do not exist here (a house rebuilt through
    // add_fixture has fresh ids): same kind, storey and position within 2 cm.
    let src: Vec<(String, String, usize, P3)> = data.get("fixtures").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|f| {
        let pos = f.get("position")?.as_array()?; if pos.len() < 3 { return None; }
        Some((st(f, "id")?, st(f, "kind")?, f.get("level").and_then(|x| x.as_u64()).unwrap_or(0) as usize, [pos[0].as_f64()?, pos[1].as_f64()?, pos[2].as_f64()?]))
    }).collect()).unwrap_or_default();
    let resolve = |id: &str| -> Option<String> {
        if h.fixture(id).is_some() { return Some(id.to_string()); }
        let (_, kind, level, pos) = src.iter().find(|f| f.0 == id)?;
        h.fixtures.iter().filter(|f| &f.kind == kind && f.level == *level).map(|f| (f, len(sub(f.position, *pos)))).filter(|(_, d)| *d < 0.02).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).map(|(f, _)| f.id.clone())
    };
    let mut dropped: Vec<String> = vec![]; let mut matched = 0usize; let mut circuits = vec![]; let mut n = 0u32;
    for c in e.get("circuits").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
        n += 1;
        let t = st(&c, "type").unwrap_or_else(|| "sockets".into());
        let t = if CIRCUIT_TYPES.contains(&t.as_str()) { t } else { "other".into() };
        let mut pts = vec![];
        for id in ids(c.get("fixtureIDs")) { match resolve(&id) { Some(r) => { if r != id { matched += 1; } if !pts.contains(&r) { pts.push(r); } } None => dropped.push(id) } }
        circuits.push(Circuit { id: st(&c, "id").unwrap_or_else(new_id), number: n, name: st(&c, "name").unwrap_or_else(|| format!("Circuit {n}")), type_: t, breaker: int(&c, "breaker").unwrap_or(20), cable_section: num(&c, "cableSection").unwrap_or(2.5), rcd: st(&c, "rcd").filter(|r| RCD_KINDS.contains(&r.as_str())).unwrap_or_else(|| "mA30".into()), poles: int(&c, "poles"), curve: st(&c, "curve"), cable_type: st(&c, "cableType"), points: pts, note: st(&c, "note").unwrap_or_default(), extra: Map::new() });
    }
    s.circuits = circuits;
    save_settings(doc, &s);
    Ok(data_result("import", json!({ "circuits": s.circuits.len(), "dropped": dropped, "matchedByPosition": matched, "board": s.board })))
}

/// Square tube along the conduit polyline (heights relative to the storey floor).
pub fn expand_conduit(c: &Custom, doc: &Document) -> Vec<Entity> {
    let Ok(k) = serde_json::from_value::<Conduit>(c.data.clone()) else { return vec![] };
    let h = House::read(doc);
    let y0 = h.base(k.level);
    let rad = (k.diameter / 2.0).max(0.004);
    let m = material_id(if k.kind == "cable" { "cable" } else { "conduit" });
    let fid = format!("{}#run", c.id);
    let mut out = vec![];
    let pts: Vec<P3> = k.points.iter().map(|p| [p[0], p[1] + y0, p[2]]).collect();
    for seg in pts.windows(2) {
        let (a, b) = (seg[0], seg[1]);
        let d = sub(b, a); if len(d) < 1e-6 { continue; }
        let dn = plannr_core::geom::normalize(d);
        let helper = if dn[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
        let u = plannr_core::geom::normalize(plannr_core::geom::cross(helper, dn));
        let v = plannr_core::geom::cross(dn, u);
        let corner = |p: P3, su: f64, sv: f64| plannr_core::geom::add(p, plannr_core::geom::add(plannr_core::geom::scale(u, su * rad), plannr_core::geom::scale(v, sv * rad)));
        let ring_a = [corner(a, -1.0, -1.0), corner(a, 1.0, -1.0), corner(a, 1.0, 1.0), corner(a, -1.0, 1.0)];
        let ring_b = [corner(b, -1.0, -1.0), corner(b, 1.0, -1.0), corner(b, 1.0, 1.0), corner(b, -1.0, 1.0)];
        let face = |pts: Vec<P3>| Entity::Face(plannr_core::model::Face { id: fid.clone(), layer: c.layer.clone(), outer: pts, holes: vec![], material: Some(m.clone()), back_material: Some(m.clone()), extra: Map::new() });
        out.push(face(vec![ring_a[3], ring_a[2], ring_a[1], ring_a[0]]));
        out.push(face(ring_b.to_vec()));
        for i in 0..4 { out.push(face(vec![ring_a[i], ring_a[(i + 1) % 4], ring_b[(i + 1) % 4], ring_b[i]])); }
    }
    out
}
