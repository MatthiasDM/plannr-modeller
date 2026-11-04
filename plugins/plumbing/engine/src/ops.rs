//! Ops of the plumbing plugin.

use plannr_core::geom::{add, dist, dot, len, normalize, scale, sub, P3};
use plannr_core::model::{Custom, Document, Entity};
use plannr_core::ops::{OpError, OpResult};
use serde_json::{json, Map, Value};

use crate::catalog::*;
use crate::check;
use crate::data::*;
use crate::fittings::{ports, socket_len};
use crate::net::{Attach, Net};

fn err<T>(m: impl Into<String>) -> Result<T, OpError> { Err(OpError(m.into())) }
fn num(v: &Value, k: &str) -> Option<f64> { v.get(k).and_then(|x| x.as_f64()) }
fn int(v: &Value, k: &str) -> Option<u32> { v.get(k).and_then(|x| x.as_u64()).map(|x| x as u32) }
fn st(v: &Value, k: &str) -> Option<String> { v.get(k).and_then(|x| x.as_str()).map(|x| x.to_string()) }
fn bo(v: &Value, k: &str) -> Option<bool> { v.get(k).and_then(|x| x.as_bool()) }
fn ids(v: Option<&Value>) -> Vec<String> { v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default() }
fn p3(v: Option<&Value>) -> Option<P3> { let a = v?.as_array()?; if a.len() < 3 { return None; } Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?]) }
fn p3s(v: Option<&Value>) -> Vec<P3> { v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|p| p3(Some(p))).collect()).unwrap_or_default() }
fn data_result(name: &str, v: Value) -> OpResult {
    let c = Custom { id: name.into(), layer: String::new(), type_: format!("plumbing.{name}"), data: v, extra: Map::new() };
    OpResult { entities: vec![Entity::Custom(c)], ..Default::default() }
}
fn first_layer(doc: &Document) -> String { doc.layers.first().map(|l| l.id.clone()).unwrap_or_default() }
fn r3(p: P3) -> P3 { [(p[0] * 10000.0).round() / 10000.0, (p[1] * 10000.0).round() / 10000.0, (p[2] * 10000.0).round() / 10000.0] }

fn push_entity<T: serde::Serialize>(doc: &mut Document, type_: &str, data: &T) -> String {
    let c = Custom { id: new_id(), layer: first_layer(doc), type_: type_.into(), data: serde_json::to_value(data).unwrap(), extra: Map::new() };
    let id = c.id.clone(); doc.entities.push(Entity::Custom(c)); id
}
fn set_data<T: serde::Serialize>(doc: &mut Document, id: &str, data: &T) -> bool {
    if let Some(Entity::Custom(c)) = doc.entities.iter_mut().find(|e| e.id() == id) { c.data = serde_json::to_value(data).unwrap(); true } else { false }
}
fn entity(doc: &Document, id: &str) -> Option<Entity> { doc.entities.iter().find(|e| e.id() == id).cloned() }
fn pipe_mut(doc: &Document, id: &str) -> Result<Pipe, OpError> { pipe(doc, id).ok_or_else(|| OpError(format!("no pipe {id}"))) }

fn validate_pipe(p: &Pipe) -> Result<(), OpError> {
    if !system_ids().contains(&p.system.as_str()) { return err(format!("unknown system `{}` ({})", p.system, system_ids().join(", "))); }
    let mat = material(&p.material).ok_or_else(|| OpError(format!("unknown material `{}` ({})", p.material, material_ids().join(", "))))?;
    if !mat.joints.contains(&p.joint.as_str()) { return err(format!("{} is joined with {}, not `{}`", mat.label, mat.joints.join(" / "), p.joint)); }
    if p.dn <= 0.0 { return err("dn must be the outer diameter in mm"); }
    if p.corners != "bend" && p.corners != "elbow" { return err("corners must be bend or elbow"); }
    if p.points.len() < 2 { return err("points needs at least 2 [x, yAboveFloor, z] entries"); }
    if p.points.windows(2).all(|w| dist(w[0], w[1]) < 1e-6) { return err("the pipe has no length"); }
    Ok(())
}
fn validate_fitting(f: &Fitting) -> Result<(), OpError> {
    let k = fitting(&f.kind).ok_or_else(|| OpError(format!("unknown fitting kind `{}` ({})", f.kind, fitting_ids().join(", "))))?;
    if !k.tapware { let mat = material(&f.material).ok_or_else(|| OpError(format!("unknown material `{}`", f.material)))?; if !mat.joints.contains(&f.joint.as_str()) && f.kind != "manifold" { return err(format!("{} fittings are {}, not `{}`", mat.label, mat.joints.join(" / "), f.joint)); } }
    if f.dn <= 0.0 { return err("dn must be the outer diameter in mm"); }
    if len(f.dir) < 1e-6 { return err("dir must be a non-zero direction"); }
    Ok(())
}

/// Read a pipe from op data (defaults from the system / material tables).
fn pipe_from(data: &Value, h: &House) -> Result<Pipe, OpError> {
    let points = p3s(data.get("points"));
    let from = st(data, "from"); let to = st(data, "to");
    let fx_sys = from.as_ref().and_then(|f| h.fixture(f)).or_else(|| to.as_ref().and_then(|f| h.fixture(f))).and_then(|f| fixture_system(&f.kind));
    let system = st(data, "system").or_else(|| fx_sys.map(|s| s.to_string())).unwrap_or_else(|| "coldWater".into());
    let material_id = st(data, "material").unwrap_or_else(|| default_material(&system).to_string());
    let mat = material(&material_id).ok_or_else(|| OpError(format!("unknown material `{material_id}` ({})", material_ids().join(", "))))?;
    let dn = num(data, "dn").unwrap_or(mat.dns[0]);
    let level = int(data, "level").map(|l| l as usize).or_else(|| from.as_ref().and_then(|f| h.fixture(f)).map(|f| f.level)).or_else(|| to.as_ref().and_then(|f| h.fixture(f)).map(|f| f.level)).unwrap_or(0);
    let p = Pipe { level, system, material: material_id.clone(), dn, points, corners: st(data, "corners").unwrap_or_else(|| default_corners(mat, dn).into()), joint: st(data, "joint").unwrap_or_else(|| mat.joints[0].into()), bend_radius: num(data, "bendRadius"), insulation: num(data, "insulation").unwrap_or(0.0), from: None, to: None, name: st(data, "name"), note: st(data, "note"), paint: None, extra: Map::new() };
    validate_pipe(&p)?;
    Ok(p)
}

pub fn apply(name: &str, data: Value, doc: &mut Document) -> Result<OpResult, OpError> {
    ensure(doc);
    match name {
        "catalog" => Ok(data_result("catalog", catalog_json())),
        "add_pipe" => add_pipe(doc, &data),
        "route" => route(doc, &data),
        "parallel" => parallel(doc, &data),
        "update_pipe" => update_pipe(doc, &data),
        "set_segment_length" => set_segment_length(doc, &data),
        "connect" => connect(doc, &data),
        "add_fitting" => add_fitting(doc, &data),
        "update_fitting" => update_generic(doc, &data, T_FITTING, |v| { let f: Fitting = serde_json::from_value(v).map_err(|e| OpError(e.to_string()))?; validate_fitting(&f)?; Ok(serde_json::to_value(&f).unwrap()) }),
        "add_bracket" => add_bracket(doc, &data),
        "update_bracket" => update_generic(doc, &data, T_BRACKET, |v| { let mut b: Bracket = serde_json::from_value(v).map_err(|e| OpError(e.to_string()))?; b.auto = false; if !bracket_kind_ids().contains(&b.kind.as_str()) { return err("unknown bracket kind"); } Ok(serde_json::to_value(&b).unwrap()) }),
        "auto_brackets" => auto_brackets(doc, &data),
        "takeoff" => Ok(data_result("takeoff", takeoff(doc))),
        "check" => {
            let f: Vec<Value> = check::check(doc).iter().map(|x| x.to_json()).collect();
            Ok(data_result("findings", json!({ "findings": f, "errors": f.iter().filter(|x| x["severity"] == "error").count(), "warnings": f.iter().filter(|x| x["severity"] == "warning").count() })))
        }
        "summary" => Ok(data_result("summary", summary(doc))),
        "import_runs" => import_runs(doc, &data),
        other => err(format!("unknown plumbing op `{other}`")),
    }
}

// ---- pipes -------------------------------------------------------------------------------------------

fn add_pipe(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let h = House::read(doc);
    let p = pipe_from(data, &h)?;
    let id = push_entity(doc, T_PIPE, &p);
    let mut created = vec![id.clone()]; let mut modified = vec![];
    if bo(data, "autoFittings").unwrap_or(true) {
        for (end, key) in [(End::Start, "from"), (End::End, "to")] {
            let j = join_end(doc, &id, end, st(data, key).as_deref(), st(data, "joint").as_deref())?;
            created.extend(j.created); modified.extend(j.modified);
        }
    }
    if bo(data, "autoBrackets").unwrap_or(false) {
        let r = auto_brackets(doc, &json!({ "pipes": [id], "kind": data.get("bracketKind") }))?; created.extend(r.created);
    }
    let p = pipe_mut(doc, &id)?;
    modified.retain(|m| m != &id);
    Ok(OpResult { created, modified, measurement: Some(p.length()), entities: entity(doc, &id).into_iter().collect(), ..Default::default() })
}

/// One end of a route: a storey-relative point, what it attaches to, and the way a port points.
struct RouteEnd { pos: P3, id: Option<String>, stub: Option<P3> }

/// `v` is `[x, y, z]` (relative to `level`) or an entity id; `id` names what a point attaches to.
/// A fitting gives its nearest open port (with the port's direction), a pipe its nearest end or, for
/// a point on its body, that point on the axis; a fixture its position.
fn route_end(net: &Net, level: usize, v: Option<&Value>, id: Option<String>, near: Option<P3>) -> Result<RouteEnd, OpError> {
    let point = p3(v);
    let id = id.or_else(|| v.and_then(|x| x.as_str()).map(|x| x.to_string()));
    let y0 = net.h.base(level);
    let Some(id) = id else {
        return point.map(|pos| RouteEnd { pos, id: None, stub: None }).ok_or_else(|| OpError("from and to are [x, yAboveFloor, z] or an id".into()));
    };
    let aim = point.or(near).map(|q| add(q, [0.0, y0, 0.0]));
    let closest = |q: P3| aim.map(|a| dist(a, q)).unwrap_or(0.0);
    if net.fittings.iter().any(|f| f.0 == id) {
        let open: Vec<_> = net.ports.iter().filter(|q| q.fitting == id && q.name != "thread" && net.ends_on_port(q).is_empty()).collect();
        let q = open.into_iter().min_by(|a, b| closest(a.pos).partial_cmp(&closest(b.pos)).unwrap()).ok_or_else(|| OpError(format!("fitting {id} has no open port")))?;
        return Ok(RouteEnd { pos: sub(q.pos, [0.0, y0, 0.0]), id: Some(id), stub: Some(q.dir) });
    }
    if let Some(pipe) = net.pipe(&id) {
        let py = net.h.base(pipe.level);
        if let Some(q) = point {
            // on the body: the nearest point of the axis; near an end: that end
            let rel = sub(add(q, [0.0, y0, 0.0]), [0.0, py, 0.0]);
            if let Some((_, on, _)) = nearest_on_pipe(pipe, rel) { return Ok(RouteEnd { pos: sub(add(on, [0.0, py, 0.0]), [0.0, y0, 0.0]), id: Some(id), stub: None }); }
        }
        let e = net.ends.iter().filter(|e| e.pipe == id).min_by(|a, b| closest(a.pos).partial_cmp(&closest(b.pos)).unwrap()).ok_or_else(|| OpError(format!("pipe {id} has no ends")))?;
        return Ok(RouteEnd { pos: sub(e.pos, [0.0, y0, 0.0]), id: Some(id), stub: Some(e.dir) });
    }
    if let Some(f) = net.h.fixture(&id) {
        return Ok(RouteEnd { pos: sub(add(f.position, [0.0, net.h.base(f.level), 0.0]), [0.0, y0, 0.0]), id: Some(id), stub: None });
    }
    err(format!("no fitting, pipe or plumbing fixture {id}"))
}

/// Where a horizontal segment crosses a wall line of the storey (plan points).
fn wall_hits(h: &House, level: usize, a: P3, b: P3) -> Vec<P3> {
    let mut out: Vec<P3> = vec![];
    for w in h.walls.iter().filter(|w| w.level == level) {
        let (dx, dz) = (b[0] - a[0], b[2] - a[2]); let (ex, ez) = (w.b[0] - w.a[0], w.b[1] - w.a[1]);
        let den = dx * ez - dz * ex; if den.abs() < 1e-9 { continue; }
        let t = ((w.a[0] - a[0]) * ez - (w.a[1] - a[2]) * ex) / den;
        let u = ((w.a[0] - a[0]) * dz - (w.a[1] - a[2]) * dx) / den;
        if t <= 0.0 || t >= 1.0 || u < 0.0 || u > 1.0 { continue; }
        let q = [a[0] + dx * t, a[1], a[2] + dz * t];
        if !out.iter().any(|o| dist(*o, q) < 0.05) { out.push(q); }
    }
    out
}

/// The orthogonal ways across a storey at one height, best first: fewest walls crossed, then
/// the most length close to a wall (where a pipe can be clipped).
fn plan_paths(h: &House, level: usize, a: P3, b: P3) -> Vec<(Vec<P3>, Vec<P3>)> {
    let mut paths: Vec<Vec<P3>> = vec![vec![a, [b[0], a[1], a[2]], b], vec![a, [a[0], a[1], b[2]], b]];
    // a jog: out to the wall line beside the start, along it, then across
    if let Some((d, n)) = h.nearest_wall(level, [a[0], a[2]]) { if d < 0.5 { let along_x = n[1].abs() > n[0].abs(); paths.push(if along_x { vec![a, [b[0], a[1], a[2]], b] } else { vec![a, [a[0], a[1], b[2]], b] }); } }
    let mut scored: Vec<(f64, Vec<P3>, Vec<P3>)> = paths.into_iter().map(|path| {
        let path: Vec<P3> = { let mut q = path; q.dedup_by(|x, y| dist(*x, *y) < 1e-6); q };
        let mut hits = vec![]; let (mut far, mut total) = (0.0, 0.0);
        for w in path.windows(2) {
            hits.extend(wall_hits(h, level, w[0], w[1]));
            let l = dist(w[0], w[1]); total += l;
            let steps = ((l / 0.25).ceil() as usize).max(1);
            for k in 0..steps { let t = (k as f64 + 0.5) / steps as f64; let q = plannr_core::geom::lerp(w[0], w[1], t); if h.nearest_wall(level, [q[0], q[2]]).map(|x| x.0 > 0.3).unwrap_or(true) { far += l / steps as f64; } }
        }
        (hits.len() as f64 + if total > 0.0 { 0.5 * far / total } else { 0.0 }, path, hits)
    }).collect();
    scored.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    let mut out: Vec<(Vec<P3>, Vec<P3>)> = vec![];
    for (_, path, hits) in scored { if !out.iter().any(|o| o.0 == path) { out.push((path, hits)); } }
    out
}

/// Lay a pipe between two places: the engine works out the corners.
fn route(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let net = Net::read(doc);
    let level = int(data, "level").map(|l| l as usize).unwrap_or(0);
    let to_hint = p3(data.get("to"));
    let a = route_end(&net, level, data.get("from"), st(data, "fromId"), to_hint)?;
    let b = route_end(&net, level, data.get("to"), st(data, "toId"), Some(a.pos))?;
    let via = st(data, "via").unwrap_or_else(|| "direct".into());
    let storey = net.h.height(level);
    let travel = match (num(data, "height"), via.as_str()) {
        (Some(y), _) => y,
        (None, "ceiling") => storey - 0.10,
        (None, "floor") => 0.10,
        (None, "direct") => a.pos[1],
        _ => return err("via must be direct, ceiling or floor"),
    };
    // leave a port along its own direction before turning
    let stub = |e: &RouteEnd| e.stub.map(|d| add(e.pos, scale(d, 0.10))).unwrap_or(e.pos);
    let (s, t) = (stub(&a), stub(&b));
    let (s_up, t_up) = ([s[0], travel, s[2]], [t[0], travel, t[2]]);
    let options = plan_paths(&net.h, level, s_up, t_up);
    let variant = int(data, "variant").unwrap_or(0) as usize % options.len().max(1);
    let (across, hits) = options.get(variant).cloned().unwrap_or((vec![s_up, t_up], vec![]));
    let mut points: Vec<P3> = vec![a.pos, s];
    points.extend(across); points.extend([t, b.pos]);
    points = points.into_iter().map(r3).collect();
    points.dedup_by(|x, y| dist(*x, *y) < 1e-4);
    // drop corners that lie on a straight line
    let mut clean: Vec<P3> = vec![];
    for q in points {
        while clean.len() >= 2 { let n = clean.len(); let u = normalize(sub(clean[n - 1], clean[n - 2])); let v = normalize(sub(q, clean[n - 1])); if dot(u, v) > 0.9999 { clean.pop(); } else { break; } }
        clean.push(q);
    }
    if clean.len() < 2 { return err("from and to are the same place"); }
    // the crossings an installer has to drill
    let floors: Vec<P3> = (1..net.h.storeys.len()).map(|l| net.h.base(l) - net.h.base(level)).flat_map(|y| clean.windows(2).filter(move |w| (w[0][1] - y) * (w[1][1] - y) < 0.0).map(move |w| [w[0][0], y, w[0][2]])).collect();
    let mut d = data.clone();
    let o = d.as_object_mut().ok_or_else(|| OpError("data must be an object".into()))?;
    o.insert("points".into(), serde_json::to_value(&clean).unwrap());
    o.insert("level".into(), json!(level));
    match &a.id { Some(i) => { o.insert("from".into(), json!(i)); } None => { o.remove("from"); } }
    match &b.id { Some(i) => { o.insert("to".into(), json!(i)); } None => { o.remove("to"); } }
    let mut r = add_pipe(doc, &d)?;
    let id = r.created[0].clone();
    let mut pair_id = Value::Null;
    if let Some(pair) = data.get("pair").filter(|v| v.is_object()) {
        let mut q = pair.clone(); q["id"] = json!(id);
        let pr = parallel(doc, &q)?;
        pair_id = json!(pr.created[0]);
        r.created.extend(pr.created); r.modified.extend(pr.modified);
    }
    let summary = data_result("route", json!({ "pipe": id, "pair": pair_id, "points": clean, "variant": variant, "variants": options.len(), "wallCrossings": hits, "floorCrossings": floors }));
    r.entities.extend(summary.entities);
    Ok(r)
}

/// A second pipe alongside an existing one: flow next to return, hot next to cold. The offset is in
/// plan (mitred at the corners, positive to the left of the direction of travel); `rise` lifts it.
fn parallel(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let id = st(data, "id").ok_or_else(|| OpError("id is required (the pipe to follow)".into()))?;
    let src = pipe_mut(doc, &id)?;
    let spacing = num(data, "distance").unwrap_or(src.dn / 1000.0 + 0.04);
    let rise = num(data, "rise").unwrap_or(0.0);
    let n = src.points.len();
    // the plan normal of every horizontal leg; a point takes the mitre of the nearest horizontal leg
    // on either side, so both ends of a riser get the same offset and the riser stays vertical
    let normals: Vec<Option<[f64; 2]>> = src.points.windows(2).map(|w| { let (x, z) = (w[1][0] - w[0][0], w[1][2] - w[0][2]); let l = (x * x + z * z).sqrt(); if l < 1e-6 { None } else { Some([z / l, -x / l]) } }).collect();
    let offsets: Vec<[f64; 2]> = (0..n).map(|i| {
        let back = normals[..i.min(n - 1)].iter().rev().find_map(|x| *x);
        let front = normals[i.min(n - 1)..].iter().find_map(|x| *x);
        let a = back.or(front).unwrap_or([1.0, 0.0]); let b = front.or(back).unwrap_or([1.0, 0.0]);
        let k = 1.0 + a[0] * b[0] + a[1] * b[1];
        let m = if k < 0.1 { a } else { [(a[0] + b[0]) / k, (a[1] + b[1]) / k] };
        m
    }).collect();
    let at = |d: f64| -> Vec<P3> { (0..n).map(|i| r3([src.points[i][0] + offsets[i][0] * d, src.points[i][1] + rise, src.points[i][2] + offsets[i][1] * d])).collect() };
    // side: the one that stays furthest from the walls, unless asked or the distance is negative
    let h = House::read(doc);
    let clearance = |pts: &[P3]| pts.windows(2).filter(|w| (w[0][0] - w[1][0]).abs() + (w[0][2] - w[1][2]).abs() > 1e-6).map(|w| h.nearest_wall(src.level, [(w[0][0] + w[1][0]) / 2.0, (w[0][2] + w[1][2]) / 2.0]).map(|x| x.0).unwrap_or(f64::MAX)).fold(f64::MAX, f64::min);
    let signed = match st(data, "side").as_deref() {
        Some("left") => spacing.abs(),
        Some("right") => -spacing.abs(),
        Some("auto") | None => if spacing < 0.0 { spacing } else if clearance(&at(-spacing)) > clearance(&at(spacing)) + 1e-6 { -spacing } else { spacing },
        Some(other) => return err(format!("side must be auto, left or right, not `{other}`")),
    };
    let points = at(signed);
    let mut d = json!({ "level": src.level, "system": st(data, "system").unwrap_or_else(|| src.system.clone()), "material": st(data, "material").unwrap_or_else(|| src.material.clone()), "dn": num(data, "dn").unwrap_or(src.dn), "corners": src.corners, "joint": src.joint, "insulation": num(data, "insulation").unwrap_or(src.insulation), "points": points, "autoFittings": bo(data, "autoFittings").unwrap_or(true), "autoBrackets": bo(data, "autoBrackets").unwrap_or(false) });
    if let Some(x) = st(data, "name") { d["name"] = json!(x); }
    if let Some(x) = st(data, "from") { d["from"] = json!(x); }
    if let Some(x) = st(data, "to") { d["to"] = json!(x); }
    add_pipe(doc, &d)
}

fn update_pipe(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    update_generic(doc, data, T_PIPE, |v| { let p: Pipe = serde_json::from_value(v).map_err(|e| OpError(e.to_string()))?; validate_pipe(&p)?; Ok(serde_json::to_value(&p).unwrap()) })
}

/// Deep-merge `patch` into an entity's data (`null` removes a key), validated by `check`.
fn update_generic(doc: &mut Document, data: &Value, type_: &str, check_fn: impl Fn(Value) -> Result<Value, OpError>) -> Result<OpResult, OpError> {
    let id = st(data, "id").ok_or_else(|| OpError("id is required".into()))?;
    let patch = data.get("patch").and_then(|v| v.as_object()).cloned().ok_or_else(|| OpError("patch must be an object".into()))?;
    let e = doc.entities.iter_mut().find_map(|e| match e { Entity::Custom(c) if c.id == id && c.type_ == type_ => Some(c), _ => None }).ok_or_else(|| OpError(format!("no {type_} {id}")))?;
    let mut obj = e.data.as_object().cloned().unwrap_or_default();
    for (k, v) in patch { if v.is_null() { obj.remove(&k); } else { obj.insert(k, v); } }
    e.data = check_fn(Value::Object(obj))?;
    let ent = Entity::Custom(e.clone());
    let measurement = if type_ == T_PIPE { pipe(doc, &id).map(|p| p.length()) } else { None };
    Ok(OpResult { modified: vec![id], entities: vec![ent], measurement, ..Default::default() })
}

/// Change one straight segment's length, moving everything after it (anchor start) or before it (anchor end).
fn set_segment_length(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let id = st(data, "id").ok_or_else(|| OpError("id is required".into()))?;
    let mut p = pipe_mut(doc, &id)?;
    let seg = int(data, "segment").unwrap_or(0) as usize;
    let length = num(data, "length").ok_or_else(|| OpError("length is required (m)".into()))?;
    if length <= 0.0 { return err("length must be positive"); }
    if seg + 1 >= p.points.len() { return err(format!("the pipe has {} segments", p.points.len() - 1)); }
    let d = sub(p.points[seg + 1], p.points[seg]); let l = len(d); if l < 1e-9 { return err("segment has no direction"); }
    let delta = scale(d, (length - l) / l);
    let anchor_end = st(data, "anchor").map(|a| a == "end").unwrap_or(false);
    if anchor_end { for q in p.points.iter_mut().take(seg + 1) { *q = r3(sub(*q, delta)); } } else { for q in p.points.iter_mut().skip(seg + 1) { *q = r3(add(*q, delta)); } }
    set_data(doc, &id, &p);
    Ok(OpResult { modified: vec![id.clone()], measurement: Some(p.length()), entities: entity(doc, &id).into_iter().collect(), ..Default::default() })
}

/// Snap a pipe end onto a target (fitting port, pipe end / body, fixture) and join it there.
fn connect(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let id = st(data, "pipe").ok_or_else(|| OpError("pipe is required".into()))?;
    let end = End::parse(&st(data, "end").unwrap_or_else(|| "end".into())).ok_or_else(|| OpError("end must be start or end".into()))?;
    let target = st(data, "to").ok_or_else(|| OpError("to is required (a fitting, pipe or fixture id)".into()))?;
    pipe_mut(doc, &id)?;
    let j = join_end(doc, &id, end, Some(&target), st(data, "joint").as_deref())?;
    if matches!(j.attach, Attach::Open) { return err(format!("nothing to connect to at `{target}` near the pipe's {}", end.name())); }
    let p = pipe_mut(doc, &id)?;
    Ok(OpResult { created: j.created, modified: j.modified, measurement: Some(p.length()), entities: entity(doc, &id).into_iter().collect(), ..Default::default() })
}

struct Joined { created: Vec<String>, modified: Vec<String>, attach: Attach }

/// Shorten a pipe's end by `by` metres along its own direction (the fitting takes that space).
fn shorten(p: &mut Pipe, end: End, by: f64) {
    let n = p.points.len(); if n < 2 || by <= 0.0 { return; }
    let (i, j) = match end { End::Start => (0, 1), End::End => (n - 1, n - 2) };
    let d = sub(p.points[j], p.points[i]); let l = len(d);
    let by = by.min(l * 0.8);
    p.points[i] = r3(add(p.points[i], scale(d, by / l)));
}
fn set_end(p: &mut Pipe, end: End, q: P3) { let n = p.points.len(); match end { End::Start => p.points[0] = r3(q), End::End => p.points[n - 1] = r3(q) } }
fn set_link(p: &mut Pipe, end: End, id: Option<String>) { match end { End::Start => p.from = id, End::End => p.to = id } }
fn new_fitting(p: &Pipe, kind: &str, joint: &str, position: P3, dir: P3, dir2: Option<P3>) -> Fitting {
    Fitting { level: p.level, kind: kind.into(), joint: joint.into(), material: p.material.clone(), system: p.system.clone(), dn: p.dn, dn2: None, position: r3(position), dir: normalize(dir), dir2: dir2.map(normalize), angle: None, thread: None, outlets: None, fixture: None, note: None, paint: None, extra: Map::new() }
}

/// Attach one end of a pipe to whatever lies there (or to `target`): link to a port, insert a
/// coupling / reducer / elbow between two pipe ends, a tee into another pipe's body, a wall plate
/// at a tap. The pipe end is moved onto the port it ends on.
fn join_end(doc: &mut Document, pipe_id: &str, end: End, target: Option<&str>, joint: Option<&str>) -> Result<Joined, OpError> {
    let net = Net::read(doc);
    let mut p = pipe_mut(doc, pipe_id)?;
    let y0 = net.h.base(p.level);
    let q = add(match end { End::Start => p.start(), End::End => p.end() }, [0.0, y0, 0.0]);
    let attach = net.attach_at(pipe_id, q, target);
    let joint = joint.filter(|j| p.mat().joints.contains(j)).map(|s| s.to_string()).unwrap_or_else(|| p.joint.clone());
    let d_out = p.end_dir(end); // pointing away from the pipe
    let mut created = vec![]; let mut modified = vec![pipe_id.to_string()];
    match &attach {
        Attach::Port(port) => {
            set_end(&mut p, end, sub(port.pos, [0.0, y0, 0.0])); set_link(&mut p, end, Some(port.fitting.clone()));
            set_data(doc, pipe_id, &p);
        }
        Attach::PipeEnd(o) => {
            let mut other = pipe_mut(doc, &o.pipe)?;
            let shared = o.pos; // world
            // the other pipe arrives travelling o.dir (its outward end direction); ours leaves travelling -d_out
            let d_in = o.dir; let v = scale(d_out, -1.0);
            set_end(&mut p, end, sub(shared, [0.0, y0, 0.0]));
            let cosang = dot(d_in, v).clamp(-1.0, 1.0); let ang = cosang.acos().to_degrees();
            let kind = if ang < 10.0 { if (other.dn - p.dn).abs() > 0.5 { "reducer" } else { "coupling" } } else if ang < 67.0 { "elbow45" } else { "elbow90" };
            let dn_big = other.dn;
            let mut f = new_fitting(&other, kind, &joint, sub(shared, [0.0, y0, 0.0]), d_in, None);
            f.dn = dn_big; f.dn2 = if (other.dn - p.dn).abs() > 0.5 { Some(p.dn) } else { None };
            if kind.starts_with("elbow") { let perp = sub(v, scale(d_in, dot(v, d_in))); f.dir2 = Some(normalize(perp)); f.angle = Some((ang * 2.0).round() / 2.0); }
            f.level = other.level;
            let ps = ports(&f);
            let fid = push_entity(doc, T_FITTING, &f);
            let y_other = net.h.base(other.level);
            shorten(&mut other, o.end, dist(add(ps[0].pos, [0.0, y_other, 0.0]), shared)); set_end(&mut other, o.end, ps[0].pos); set_link(&mut other, o.end, Some(fid.clone()));
            let port_out = add(ps[1].pos, [0.0, y_other, 0.0]);
            shorten(&mut p, end, dist(port_out, shared)); set_end(&mut p, end, sub(port_out, [0.0, y0, 0.0])); set_link(&mut p, end, Some(fid.clone()));
            set_data(doc, &o.pipe, &other); set_data(doc, pipe_id, &p);
            created.push(fid); modified.push(o.pipe.clone());
        }
        Attach::PipeBody(other_id, seg, at, u) => {
            let r = insert_tee(doc, &net, other_id, *seg, *at, *u, Some((&p, d_out, &joint)))?;
            let fid = r.0.clone(); created.extend(r.1); modified.push(other_id.clone());
            // our end goes onto the tee's branch port
            let f = fitting_of(doc, &fid).unwrap(); let ps = ports(&f); let y_f = net.h.base(f.level);
            let branch = add(ps[2].pos, [0.0, y_f, 0.0]);
            shorten(&mut p, end, dist(branch, *at)); set_end(&mut p, end, sub(branch, [0.0, y0, 0.0])); set_link(&mut p, end, Some(fid.clone()));
            set_data(doc, pipe_id, &p);
        }
        Attach::Fixture(fx) => {
            if is_drain(&p.system) || fixture_system(&fx.kind).map(is_drain).unwrap_or(false) || matches!(fx.kind.as_str(), "radiator" | "boiler" | "manifold") {
                set_link(&mut p, end, Some(fx.id.clone())); set_data(doc, pipe_id, &p);
            } else {
                // a wall plate at the pipe's end, outlet out of the wall
                let out_dir = wall_out_dir(&net.h, p.level, q, d_out);
                // the pipe travels d_out into the plate
                let d_in = d_out;
                let d_in = if dot(d_in, out_dir).abs() > 0.95 { if d_in[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] } } else { d_in };
                let mut f = new_fitting(&p, "wallPlate", &joint, sub(q, [0.0, y0, 0.0]), d_in, Some(out_dir));
                f.thread = Some("1/2\"".into()); f.fixture = Some(fx.id.clone());
                let ps = ports(&f);
                let fid = push_entity(doc, T_FITTING, &f);
                shorten(&mut p, end, dist(ps[0].pos, f.position)); set_end(&mut p, end, ps[0].pos); set_link(&mut p, end, Some(fid.clone()));
                set_data(doc, pipe_id, &p); created.push(fid);
            }
        }
        Attach::Open => { set_link(&mut p, end, None); set_data(doc, pipe_id, &p); }
    }
    Ok(Joined { created, modified, attach })
}

/// Direction out of the nearest wall at a world point (towards the room), else away from the pipe.
fn wall_out_dir(h: &House, level: usize, q: P3, d_out: P3) -> P3 {
    if let Some((d, n)) = h.nearest_wall(level, [q[0], q[2]]) { if d < 0.45 { return [n[0], 0.0, n[1]]; } }
    let horiz = normalize([d_out[0], 0.0, d_out[2]]);
    if len(horiz) > 0.5 { horiz } else { [1.0, 0.0, 0.0] }
}

/// Split a pipe at a point on its `seg`-th segment and put a tee there (branch towards the joining
/// pipe, or along `dir2`). Returns (tee id, created ids).
fn insert_tee(doc: &mut Document, net: &Net, other_id: &str, seg: usize, at: P3, u: P3, joining: Option<(&Pipe, P3, &str)>) -> Result<(String, Vec<String>), OpError> {
    let mut other = pipe_mut(doc, other_id)?;
    let y_other = net.h.base(other.level);
    let (branch_dn, branch_dir, joint, kind) = match joining {
        Some((p, d_out, j)) => { let v = scale(d_out, -1.0); let perp = sub(v, scale(u, dot(v, u))); (p.dn, if len(perp) > 1e-6 { normalize(perp) } else { [0.0, 1.0, 0.0] }, j.to_string(), if is_drain(&other.system) { "branch45" } else { "tee" }) }
        None => (other.dn, [0.0, 1.0, 0.0], other.joint.clone(), "tee"),
    };
    let mut f = new_fitting(&other, kind, &joint, sub(at, [0.0, y_other, 0.0]), u, Some(branch_dir));
    f.dn2 = if (branch_dn - other.dn).abs() > 0.5 { Some(branch_dn) } else { None };
    // for a 45° drain branch the branch leaves in the flow direction: port 2 = elbow_out(u, dir2, 45°)
    let ps = ports(&f);
    let fid = push_entity(doc, T_FITTING, &f);
    // split: A = start..at, B = at..end, each pulled back to the tee's inline ports
    let mut a = other.clone(); let mut b = other.clone();
    a.points = other.points[..=seg].to_vec(); a.points.push(r3(sub(at, [0.0, y_other, 0.0])));
    b.points = vec![r3(sub(at, [0.0, y_other, 0.0]))]; b.points.extend_from_slice(&other.points[seg + 1..]);
    a.points.dedup_by(|x, y| dist(*x, *y) < 1e-6); b.points.dedup_by(|x, y| dist(*x, *y) < 1e-6);
    let s_len = socket_len(&joint, other.dn) + 0.25 * other.dn / 1000.0;
    shorten(&mut a, End::End, s_len); set_end(&mut a, End::End, ps[0].pos); a.to = Some(fid.clone());
    shorten(&mut b, End::Start, s_len); set_end(&mut b, End::Start, ps[1].pos); b.from = Some(fid.clone());
    other.points = a.points; other.to = a.to;
    set_data(doc, other_id, &other);
    let bid = push_entity(doc, T_PIPE, &b);
    // brackets that sat on the second half follow it
    let brs = brackets(doc);
    for (bid_, br) in brs { if br.pipe.as_deref() == Some(other_id) && check::arc_param(&other, br.position) >= other.length() - 1e-6 && !on_pipe(&other, br.position) { let mut br2 = br.clone(); br2.pipe = Some(bid.clone()); set_data(doc, &bid_, &br2); } }
    Ok((fid.clone(), vec![fid, bid]))
}
fn on_pipe(p: &Pipe, q: P3) -> bool { p.points.windows(2).any(|w| { let d = sub(w[1], w[0]); let l = len(d); if l < 1e-9 { return false; } let u = scale(d, 1.0 / l); let t = dot(sub(q, w[0]), u); t >= -0.01 && t <= l + 0.01 && dist(add(w[0], scale(u, t)), q) < 0.03 }) }

// ---- fittings -----------------------------------------------------------------------------------------

fn add_fitting(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let net = Net::read(doc);
    let kind = st(data, "kind").ok_or_else(|| OpError(format!("kind is required ({})", fitting_ids().join(", "))))?;
    let kinfo = fitting(&kind).ok_or_else(|| OpError(format!("unknown fitting kind `{kind}`")))?;
    let on_pipe_id = st(data, "onPipe");
    let fixture_id = st(data, "fixture");
    let fx = fixture_id.as_ref().and_then(|id| net.h.fixture(id)).cloned();
    let position_in = p3(data.get("position")).or_else(|| fx.as_ref().map(|f| f.position));
    // defaults from the pipe it goes on
    let on_pipe = on_pipe_id.as_ref().map(|id| pipe_mut(doc, id)).transpose()?;
    let system = st(data, "system").or_else(|| on_pipe.as_ref().map(|p| p.system.clone())).or_else(|| fx.as_ref().and_then(|f| fixture_system(&f.kind)).map(|s| s.to_string())).unwrap_or_else(|| if kinfo.systems.is_empty() { "coldWater".into() } else { kinfo.systems[0].into() });
    let material_id = st(data, "material").or_else(|| on_pipe.as_ref().map(|p| p.material.clone())).unwrap_or_else(|| default_material(&system).into());
    let mat = material(&material_id).ok_or_else(|| OpError(format!("unknown material `{material_id}`")))?;
    let dn = num(data, "dn").or_else(|| on_pipe.as_ref().map(|p| p.dn)).unwrap_or(mat.dns[0]);
    let joint = st(data, "joint").or_else(|| on_pipe.as_ref().map(|p| p.joint.clone())).unwrap_or_else(|| mat.joints[0].into());
    let level = int(data, "level").map(|l| l as usize).or_else(|| on_pipe.as_ref().map(|p| p.level)).or_else(|| fx.as_ref().map(|f| f.level)).unwrap_or(0);
    let mut f = Fitting { level, kind: kind.clone(), joint, material: material_id, system, dn, dn2: num(data, "dn2"), position: position_in.unwrap_or([0.0; 3]), dir: p3(data.get("dir")).unwrap_or(X_AX), dir2: p3(data.get("dir2")), angle: num(data, "angle"), thread: st(data, "thread"), outlets: int(data, "outlets"), fixture: fixture_id.clone(), note: st(data, "note"), paint: None, extra: Map::new() };
    if kind == "wallPlate" && f.thread.is_none() { f.thread = Some("1/2\"".into()); }
    if kind == "manifold" && f.outlets.is_none() { f.outlets = Some(4); }
    if kind == "manifold" && f.dn2.is_none() { f.dn2 = Some(16.0); }
    let mut created = vec![]; let mut modified = vec![];
    if let (Some(pid), Some(p)) = (on_pipe_id.as_ref(), on_pipe.as_ref()) {
        let y0 = net.h.base(p.level);
        let at_in = position_in.ok_or_else(|| OpError("position is required with onPipe (a point near the pipe)".into()))?;
        let at_w = add(at_in, [0.0, y0, 0.0]);
        // nearest point on the pipe
        let (seg, q, u) = nearest_on_pipe(p, at_in).ok_or_else(|| OpError("the pipe has no segments".into()))?;
        if dist(q, at_in) > 0.15 { return err(format!("position is {:.2} m away from pipe {} (its nearest point is {:?}); give a point on the pipe or the id of the pipe that runs there", dist(q, at_in), pid, [(q[0] * 1000.0).round() / 1000.0, (q[1] * 1000.0).round() / 1000.0, (q[2] * 1000.0).round() / 1000.0])); }
        let s_len = socket_len(&f.joint, dn / 1000.0) + 0.25 * dn / 1000.0;
        let near_start = dist(q, p.start()) < 0.06; let near_end = dist(q, p.end()) < 0.06;
        if (near_start || near_end) && kinfo.shape != Shape::Tee || kinfo.shape == Shape::End || kinfo.shape == Shape::WallPlate {
            // at an end: the in-port on the pipe end, the fitting beyond it
            let end = if near_start || (!near_end && dist(q, p.start()) < dist(q, p.end())) { End::Start } else { End::End };
            let e = match end { End::Start => p.start(), End::End => p.end() };
            let d = p.end_dir(end);
            f.dir = d; f.position = r3(add(e, scale(d, s_len)));
            if f.dir2.is_none() { let want = sub(at_in, e); let perp = sub(want, scale(d, dot(want, d))); f.dir2 = Some(if len(perp) > 0.01 { normalize(perp) } else if kinfo.shape == Shape::WallPlate { wall_out_dir(&net.h, p.level, at_w, d) } else if d[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] }); }
            if kinfo.shape == Shape::WallPlate { f.position = r3(add(e, scale(d, s_len))); }
            validate_fitting(&f)?;
            let fid = push_entity(doc, T_FITTING, &f);
            let mut p2 = p.clone(); set_link(&mut p2, end, Some(fid.clone())); set_data(doc, pid, &p2);
            created.push(fid); modified.push(pid.clone());
        } else if kinfo.shape == Shape::Tee {
            let (fid, c) = insert_tee(doc, &net, pid, seg, add(q, [0.0, y0, 0.0]), u, None)?;
            // honour the asked branch direction / dn2
            let mut t = fitting_of(doc, &fid).unwrap();
            let want = sub(at_in, q); let perp = sub(want, scale(u, dot(want, u)));
            if let Some(d2) = f.dir2 { t.dir2 = Some(d2); } else if len(perp) > 0.01 { t.dir2 = Some(normalize(perp)); }
            if f.dn2.is_some() { t.dn2 = f.dn2; } t.kind = kind.clone(); t.joint = f.joint.clone(); t.note = f.note.clone();
            set_data(doc, &fid, &t);
            created.extend(c); modified.push(pid.clone());
        } else if kinfo.shape == Shape::Inline {
            // split and insert between the two halves
            f.dir = u; f.position = r3(q);
            if f.dir2.is_none() { let want = sub(at_in, q); let perp = sub(want, scale(u, dot(want, u))); f.dir2 = Some(if len(perp) > 0.01 { normalize(perp) } else if u[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] }); }
            validate_fitting(&f)?;
            let ps = ports(&f);
            let fid = push_entity(doc, T_FITTING, &f);
            let mut a = p.clone(); let mut b = p.clone();
            a.points = p.points[..=seg].to_vec(); a.points.push(r3(q)); a.points.dedup_by(|x, y| dist(*x, *y) < 1e-6);
            b.points = vec![r3(q)]; b.points.extend_from_slice(&p.points[seg + 1..]); b.points.dedup_by(|x, y| dist(*x, *y) < 1e-6);
            if a.points.len() < 2 || b.points.len() < 2 { return err("too close to the pipe end; place the fitting at the end instead"); }
            shorten(&mut a, End::End, s_len); set_end(&mut a, End::End, ps[0].pos); a.to = Some(fid.clone());
            shorten(&mut b, End::Start, s_len); set_end(&mut b, End::Start, ps[1].pos); b.from = Some(fid.clone());
            set_data(doc, pid, &a); let bid = push_entity(doc, T_PIPE, &b);
            created.push(fid); created.push(bid); modified.push(pid.clone());
        } else {
            return err(format!("a {} is placed at a pipe end or free (position / fixture), not inside a pipe", kinfo.label));
        }
    } else {
        if position_in.is_none() { return err("position is required ([x, yAboveFloor, z]), or onPipe / fixture"); }
        if f.dir2.is_none() { if fx.is_some() { let out = wall_out_dir(&net.h, f.level, add(f.position, [0.0, net.h.base(f.level), 0.0]), X_AX); f.dir2 = Some(out); if dot(f.dir, out).abs() > 0.95 { f.dir = [0.0, 1.0, 0.0]; } if kinfo.shape == Shape::Mixer { f.dir = normalize([-out[2], 0.0, out[0]]); } } }
        validate_fitting(&f)?;
        let fid = push_entity(doc, T_FITTING, &f); created.push(fid);
    }
    let fid = created[0].clone();
    Ok(OpResult { created, modified, entities: entity(doc, &fid).into_iter().collect(), ..Default::default() })
}
fn nearest_on_pipe(p: &Pipe, q: P3) -> Option<(usize, P3, P3)> {
    let mut best: Option<(f64, (usize, P3, P3))> = None;
    for (i, w) in p.points.windows(2).enumerate() {
        let d = sub(w[1], w[0]); let l = len(d); if l < 1e-9 { continue; }
        let u = scale(d, 1.0 / l); let t = dot(sub(q, w[0]), u).clamp(0.0, l); let pt = add(w[0], scale(u, t)); let dd = dist(pt, q);
        if best.as_ref().map(|b| dd < b.0).unwrap_or(true) { best = Some((dd, (i, pt, u))); }
    }
    best.map(|b| b.1)
}

// ---- brackets -----------------------------------------------------------------------------------------

fn add_bracket(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let h = House::read(doc);
    let pipe_id = st(data, "pipe");
    let p = pipe_id.as_ref().map(|id| pipe_mut(doc, id)).transpose()?;
    let mut pos = p3(data.get("position")).ok_or_else(|| OpError("position is required ([x, yAboveFloor, z], on or near the pipe)".into()))?;
    let level = int(data, "level").map(|l| l as usize).or_else(|| p.as_ref().map(|p| p.level)).unwrap_or(0);
    let mut dir = p3(data.get("dir")).unwrap_or(X_AX);
    if let Some(p) = &p { if let Some((_, q, u)) = nearest_on_pipe(p, pos) { pos = q; dir = u; } }
    let dn = num(data, "dn").or_else(|| p.as_ref().map(|p| p.dn)).unwrap_or(16.0);
    let (normal, standoff) = match p3(data.get("normal")) {
        Some(n) => (n, num(data, "standoff").unwrap_or(dn / 2000.0 + 0.02)),
        None => surface_for(&h, level, pos, dir, dn),
    };
    let standoff = num(data, "standoff").unwrap_or(standoff);
    let kind = st(data, "kind").unwrap_or_else(|| if standoff > dn / 2000.0 + 0.06 { "clamp".into() } else { "clip".into() });
    if !bracket_kind_ids().contains(&kind.as_str()) { return err(format!("kind must be one of {}", bracket_kind_ids().join(", "))); }
    let b = Bracket { level, kind, position: r3(pos), dir: normalize(dir), normal: normalize(normal), dn, standoff, pipe: pipe_id, auto: false, extra: Map::new() };
    let id = push_entity(doc, T_BRACKET, &b);
    Ok(OpResult { created: vec![id.clone()], entities: entity(doc, &id).into_iter().collect(), ..Default::default() })
}

/// Which surface a bracket at a (storey-relative) point on a pipe is screwed to: (normal from the
/// pipe axis towards it, distance). Walls within reach first, then the ceiling or the floor.
pub fn surface_for(h: &House, level: usize, pos: P3, dir: P3, dn: f64) -> (P3, f64) {
    let r = dn / 2000.0; let d = normalize(dir); let height = h.height(level);
    let vertical = d[1].abs() > 0.9;
    let wall = h.nearest_wall(level, [pos[0], pos[2]]).map(|(dist_w, n)| { let to_wall = [-n[0], 0.0, -n[1]]; (dist_w, to_wall) });
    let wall_ok = wall.map(|(dw, n)| dw < 0.35 && dot(n, d).abs() < 0.7).unwrap_or(false);
    let clamp_off = |x: f64| x.clamp(r + 0.012, 0.25);
    if vertical { if let Some((dw, n)) = wall { if dw < 1.0 { return (n, clamp_off(dw)); } } return ([1.0, 0.0, 0.0], r + 0.02); }
    if wall_ok { let (dw, n) = wall.unwrap(); return (n, clamp_off(dw)); }
    if pos[1] > height - 0.45 { return ([0.0, 1.0, 0.0], clamp_off(height - pos[1])); }
    if pos[1] < 0.3 { return ([0.0, -1.0, 0.0], clamp_off(pos[1])); }
    if let Some((dw, n)) = wall { if dot(n, d).abs() < 0.7 { return (n, clamp_off(dw)); } }
    ([0.0, 1.0, 0.0], clamp_off(height - pos[1]))
}

fn auto_brackets(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let h = House::read(doc);
    let only = ids(data.get("pipes"));
    let kind_override = st(data, "kind").filter(|k| bracket_kind_ids().contains(&k.as_str()));
    let spacing_override = num(data, "spacing");
    let targets: Vec<(String, Pipe)> = pipes(doc).into_iter().filter(|(id, _)| only.is_empty() || only.contains(id)).collect();
    if !only.is_empty() { for id in &only { if !targets.iter().any(|t| &t.0 == id) { return err(format!("no pipe {id}")); } } }
    let target_ids: Vec<&str> = targets.iter().map(|t| t.0.as_str()).collect();
    let mut removed = vec![];
    doc.entities.retain(|e| match e { Entity::Custom(c) if c.type_ == T_BRACKET => { let auto = c.data.get("auto").and_then(|v| v.as_bool()).unwrap_or(false); let pid = c.data.get("pipe").and_then(|v| v.as_str()).unwrap_or(""); if auto && target_ids.contains(&pid) { removed.push(c.id.clone()); false } else { true } }, _ => true });
    let mut created = vec![];
    for (id, p) in &targets {
        let mat = p.mat();
        for w in p.points.windows(2) {
            let d = sub(w[1], w[0]); let l = len(d); if l < 0.12 { continue; }
            let u = scale(d, 1.0 / l); let vertical = u[1].abs() > 0.9;
            let s = spacing_override.unwrap_or(bracket_spacing(mat, p.dn) * if vertical { 1.6 } else { 1.0 });
            let n = ((l / s).ceil() as usize).max(1);
            for k in 0..n {
                let t = l * (k as f64 + 0.5) / n as f64;
                let pos = add(w[0], scale(u, t));
                let (normal, standoff) = surface_for(&h, p.level, pos, u, p.dn);
                let kind = kind_override.clone().unwrap_or_else(|| if standoff > p.dn / 2000.0 + 0.06 { "clamp".into() } else { "clip".into() });
                let b = Bracket { level: p.level, kind, position: r3(pos), dir: u, normal, dn: p.dn, standoff: (standoff * 1000.0).round() / 1000.0, pipe: Some(id.clone()), auto: true, extra: Map::new() };
                created.push(push_entity(doc, T_BRACKET, &b));
            }
        }
    }
    Ok(OpResult { created, removed, ..Default::default() })
}

// ---- takeoff / summary --------------------------------------------------------------------------------

fn std_angle(mat: &PipeMaterial, ang: f64) -> f64 {
    let set: &[f64] = if mat.id == "pvc" || mat.id == "pe" { &[15.0, 30.0, 45.0, 67.5, 87.5] } else { &[45.0, 90.0] };
    *set.iter().min_by(|a, b| (**a - ang).abs().partial_cmp(&(**b - ang).abs()).unwrap()).unwrap()
}

pub fn takeoff(doc: &Document) -> Value {
    use std::collections::BTreeMap;
    let ps = pipes(doc); let fs = fittings(doc); let bs = brackets(doc);
    let mut pipe_rows: BTreeMap<(String, String, String), (f64, usize, usize)> = BTreeMap::new();
    let mut elbow_rows: BTreeMap<(String, String, String, String), usize> = BTreeMap::new();
    let mut insul: BTreeMap<(String, String), f64> = BTreeMap::new();
    for (_, p) in &ps {
        let mat = p.mat();
        let e = pipe_rows.entry((p.material.clone(), num_key(p.dn), p.system.clone())).or_insert((0.0, 0, 0));
        e.0 += p.length(); e.1 += 1;
        let corners = p.corner_angles();
        if p.is_bent() { e.2 += corners.len(); } else { for (_, ang) in corners { *elbow_rows.entry((p.material.clone(), p.joint.clone(), num_key(p.dn), fmt_num(std_angle(mat, ang)))).or_insert(0) += 1; } }
        if p.insulation > 0.0 { *insul.entry((num_key(p.dn), fmt_num(p.insulation))).or_insert(0.0) += p.length(); }
    }
    let mut fit_rows: BTreeMap<(String, String, String, String, String, String), usize> = BTreeMap::new();
    for (_, f) in &fs {
        let extra = match f.kind.as_str() { "manifold" => format!("{} outlets", f.outlets.unwrap_or(4)), "wallPlate" | "adapter" => f.thread.clone().unwrap_or_default(), k if k.starts_with("elbow") => format!("{}°", fmt_num(f.angle_deg())), _ => String::new() };
        *fit_rows.entry((f.kind.clone(), f.material.clone(), if f.kind_info().tapware { String::new() } else { f.joint.clone() }, num_key(f.dn), f.dn2.map(num_key).unwrap_or_default(), extra)).or_insert(0) += 1;
    }
    let mut br_rows: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (_, b) in &bs { *br_rows.entry((b.kind.clone(), num_key(b.dn))).or_insert(0) += 1; }
    let mut items = vec![];
    for ((m, dn, sys), (l, n, bends)) in &pipe_rows {
        let mat = material(m).unwrap_or(&MATERIALS[0]); let dnf: f64 = dn.parse().unwrap_or(0.0);
        items.push(json!({ "group": "pipe", "label": format!("{} {} — {}", mat.label, dn_label(mat, dnf), system_label(sys)), "labelNl": format!("{} {} — {}", mat.label_nl, dn_label(mat, dnf), SYSTEMS.iter().find(|s| s.0 == sys).map(|s| s.2).unwrap_or("")), "material": m, "dn": dnf, "system": sys, "length_m": (l * 100.0).round() / 100.0, "pipes": n, "bends": bends }));
    }
    for ((m, j, dn, ang), n) in &elbow_rows {
        let mat = material(m).unwrap_or(&MATERIALS[0]); let dnf: f64 = dn.parse().unwrap_or(0.0);
        items.push(json!({ "group": "fitting", "kind": if ang == "45" { "elbow45" } else { "elbow90" }, "label": format!("{} elbow {}° {} {}", joint_label(j), ang, mat.label, dn_label(mat, dnf)), "labelNl": format!("{} bocht {}° {} {}", JOINTS.iter().find(|x| x.0 == j).map(|x| x.2).unwrap_or(""), ang, mat.label_nl, dn_label(mat, dnf)), "material": m, "joint": j, "dn": dnf, "angle": ang.parse::<f64>().unwrap_or(90.0), "count": n, "fromPipeCorners": true }));
    }
    for ((k, m, j, dn, dn2, extra), n) in &fit_rows {
        let kind = fitting(k); let mat = material(m).unwrap_or(&MATERIALS[0]); let dnf: f64 = dn.parse().unwrap_or(0.0);
        let tap = kind.map(|x| x.tapware).unwrap_or(false);
        let size = if tap && k != "trap" { String::new() } else if dn2.is_empty() { dn_label(mat, dnf) } else { format!("{} × {}", dn_label(mat, dnf), dn_label(mat, dn2.parse().unwrap_or(0.0))) };
        let jl = if j.is_empty() { String::new() } else { format!("{} ", joint_label(j)) };
        let jnl = if j.is_empty() { String::new() } else { format!("{} ", JOINTS.iter().find(|x| x.0 == j).map(|x| x.2).unwrap_or("")) };
        items.push(json!({ "group": "fitting", "kind": k, "label": format!("{jl}{} {} {}{}", kind.map(|x| x.label).unwrap_or(k), if kind.map(|x| x.tapware).unwrap_or(false) { String::new() } else { mat.label.to_string() }, size, if extra.is_empty() { String::new() } else { format!(" ({extra})") }).replace("  ", " "), "labelNl": format!("{jnl}{} {} {}{}", kind.map(|x| x.label_nl).unwrap_or(k), if kind.map(|x| x.tapware).unwrap_or(false) { String::new() } else { mat.label_nl.to_string() }, size, if extra.is_empty() { String::new() } else { format!(" ({extra})") }).replace("  ", " "), "material": m, "joint": j, "dn": dnf, "dn2": if dn2.is_empty() { Value::Null } else { json!(dn2.parse::<f64>().unwrap_or(0.0)) }, "detail": extra, "count": n }));
    }
    for ((k, dn), n) in &br_rows {
        let dnf: f64 = dn.parse().unwrap_or(0.0); let bk = BRACKET_KINDS.iter().find(|b| b.0 == k);
        items.push(json!({ "group": "bracket", "kind": k, "label": format!("{} Ø {}", bk.map(|b| b.1).unwrap_or(k), dn), "labelNl": format!("{} Ø {}", bk.map(|b| b.2).unwrap_or(k), dn), "dn": dnf, "count": n }));
    }
    for ((dn, t), l) in &insul { items.push(json!({ "group": "insulation", "label": format!("Pipe insulation {t} mm for Ø {dn}"), "labelNl": format!("Buisisolatie {t} mm voor Ø {dn}"), "dn": dn.parse::<f64>().unwrap_or(0.0), "thickness_mm": t.parse::<f64>().unwrap_or(0.0), "length_m": (l * 100.0).round() / 100.0 })); }
    let total_len: f64 = ps.iter().map(|(_, p)| p.length()).sum();
    json!({ "items": items, "totals": { "pipes": ps.len(), "pipeLength_m": (total_len * 100.0).round() / 100.0, "fittings": fs.len() + elbow_rows.values().sum::<usize>(), "brackets": bs.len() } })
}
fn num_key(v: f64) -> String { fmt_num(v) }
fn fmt_num(v: f64) -> String { crate::catalog::num(v) }

pub fn summary(doc: &Document) -> Value {
    let net = Net::read(doc);
    let open = net.ends.iter().filter(|e| matches!(net.end_attachment(e), Attach::Open)).count();
    let mut per_system: Vec<Value> = vec![];
    for (sid, label, _, color) in SYSTEMS {
        let ps: Vec<&(String, Pipe)> = net.pipes.iter().filter(|p| p.1.system == sid).collect();
        if ps.is_empty() { continue; }
        per_system.push(json!({ "system": sid, "label": label, "color": color, "pipes": ps.len(), "length_m": (ps.iter().map(|p| p.1.length()).sum::<f64>() * 100.0).round() / 100.0, "fittings": net.fittings.iter().filter(|f| f.1.system == sid).count() }));
    }
    json!({ "pipes": net.pipes.len(), "pipeLength_m": (net.pipes.iter().map(|p| p.1.length()).sum::<f64>() * 100.0).round() / 100.0, "fittings": net.fittings.len(), "brackets": brackets(doc).len(), "openEnds": open, "systems": per_system,
            "pipeList": net.pipes.iter().map(|(id, p)| json!({ "id": id, "name": p.name, "system": p.system, "material": p.material, "dn": p.dn, "joint": p.joint, "corners": p.corners, "level": p.level, "length_m": (p.length() * 1000.0).round() / 1000.0, "points": p.points, "from": p.from, "to": p.to })).collect::<Vec<_>>(),
            "fittingList": net.fittings.iter().map(|(id, f)| json!({ "id": id, "kind": f.kind, "joint": f.joint, "material": f.material, "dn": f.dn, "dn2": f.dn2, "level": f.level, "position": f.position, "ports": ports(f).iter().enumerate().map(|(i, q)| json!({ "index": i, "name": q.name, "position": q.pos, "dir": q.dir, "dn": q.dn, "open": net.ends_on_port(&crate::net::PortW { fitting: id.clone(), index: i, pos: add(q.pos, [0.0, net.h.base(f.level), 0.0]), dir: q.dir, dn: q.dn, name: q.name, tapware: false, kind: f.kind.clone(), system: f.system.clone(), joint: f.joint.clone() }).is_empty() })).collect::<Vec<_>>() })).collect::<Vec<_>>() })
}

pub fn metrics(doc: &Document) -> Value {
    let net = Net::read(doc);
    let open = net.ends.iter().filter(|e| matches!(net.end_attachment(e), Attach::Open)).count();
    json!({ "pipes": net.pipes.len(), "pipeLength_m": (net.pipes.iter().map(|p| p.1.length()).sum::<f64>() * 1000.0).round() / 1000.0, "fittings": net.fittings.len(), "brackets": brackets(doc).len(), "openEnds": open })
}

/// Turn the construction plugin's water / drain / heating `construction.run` polylines into pipes.
fn import_runs(doc: &mut Document, data: &Value) -> Result<OpResult, OpError> {
    let remove = bo(data, "remove").unwrap_or(true);
    let runs: Vec<(String, Value)> = doc.entities.iter().filter_map(|e| match e { Entity::Custom(c) if c.type_ == "construction.run" => Some((c.id.clone(), c.data.clone())), _ => None }).collect();
    let mut created = vec![]; let mut removed = vec![];
    // pipes made from these runs by an earlier import (an embedding app re-imports its house after every
    // change) are replaced, with their automatic brackets; pipes drawn by hand have no `source` and stay
    let run_ids: Vec<&str> = runs.iter().map(|r| r.0.as_str()).collect();
    let stale: Vec<String> = doc.entities.iter().filter_map(|e| match e { Entity::Custom(c) if c.type_ == T_PIPE && c.data.get("source").and_then(|v| v.as_str()).map(|s| run_ids.contains(&s)).unwrap_or(false) => Some(c.id.clone()), _ => None }).collect();
    if !stale.is_empty() {
        doc.entities.retain(|e| match e { Entity::Custom(c) if c.type_ == T_BRACKET => !(c.data.get("auto").and_then(|v| v.as_bool()).unwrap_or(false) && c.data.get("pipe").and_then(|v| v.as_str()).map(|p| stale.iter().any(|s| s == p)).unwrap_or(false)), Entity::Custom(c) if c.type_ == T_PIPE => !stale.contains(&c.id), _ => true });
        removed.extend(stale);
    }
    for (rid, d) in runs {
        let sys = match d.get("system").and_then(|v| v.as_str()).unwrap_or("") { "waterCold" => "coldWater", "waterHot" => "hotWater", "heating" => "heatingFlow", "drain" => "drain", _ => continue };
        let points = p3s(d.get("points")); if points.len() < 2 { continue; }
        let mat = material(default_material(sys)).unwrap();
        let dn = d.get("diameter").and_then(|v| v.as_f64()).map(|m| m * 1000.0).filter(|x| *x > 5.0).map(|x| *mat.dns.iter().min_by(|a, b| (**a - x).abs().partial_cmp(&(**b - x).abs()).unwrap()).unwrap()).unwrap_or(if is_drain(sys) { 40.0 } else { 16.0 });
        let mut extra = Map::new(); extra.insert("source".into(), Value::String(rid.clone()));
        let p = Pipe { level: d.get("level").and_then(|v| v.as_u64()).unwrap_or(0) as usize, system: sys.into(), material: mat.id.into(), dn, points, corners: default_corners(mat, dn).into(), joint: mat.joints[0].into(), bend_radius: None, insulation: 0.0, from: None, to: None, name: d.get("name").and_then(|v| v.as_str()).map(|x| x.to_string()), note: None, paint: None, extra };
        created.push(push_entity(doc, T_PIPE, &p));
        if remove { doc.entities.retain(|e| e.id() != rid); removed.push(rid); }
    }
    Ok(OpResult { created, removed, ..Default::default() })
}
