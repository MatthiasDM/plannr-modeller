//! Plumbing plugin for Plannr: pipes of Alupex, PEX, copper, PVC, PE or steel laid as polylines with
//! bends or elbows, fittings of the right joint type (press, clamp, solder, glue…) inserted where
//! pipes meet, taps and mixers, valves, manifolds, brackets, checks and a material takeoff.
//! See docs/PLUMBING.md.

pub mod catalog;
pub mod check;
pub mod data;
pub mod expand;
pub mod fittings;
pub mod mesh;
pub mod net;
pub mod ops;

use plannr_core::geom::{mat_apply, mat_apply_dir, normalize, Mat4};
use plannr_core::model::{Custom, Document, Entity};
use plannr_core::ops::{OpError, OpResult};
use plannr_core::plugin::{Plugin, PluginInfo, PluginOp, Side};
use serde_json::{json, Value};

use catalog::*;
use data::*;

#[derive(Default)]
pub struct PlumbingPlugin;

fn n(d: &str) -> Value { json!({ "type": "number", "description": d }) }
fn i(d: &str) -> Value { json!({ "type": "integer", "minimum": 0, "description": d }) }
fn s(d: &str) -> Value { json!({ "type": "string", "description": d }) }
fn b(d: &str) -> Value { json!({ "type": "boolean", "description": d }) }
fn en(d: &str, v: &[&str]) -> Value { json!({ "type": "string", "enum": v, "description": d }) }
fn v3(d: &str) -> Value { json!({ "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3, "description": d }) }
fn pts(d: &str) -> Value { json!({ "type": "array", "items": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 }, "minItems": 2, "description": d }) }
fn ids(d: &str) -> Value { json!({ "type": "array", "items": { "type": "string" }, "description": d }) }
fn op(name: &str, description: &str, props: Value, required: &[&str]) -> PluginOp {
    PluginOp { name: name.into(), description: description.into(), input_schema: json!({ "type": "object", "properties": props, "required": required }) }
}

fn pipe_props() -> Value {
    json!({
        "level": i("Storey index (default: the fixture's, else 0)"),
        "system": en("What the pipe carries (sets the colour and the default material)", &system_ids()),
        "material": en("Pipe material (default: alupex for water / heating / gas, pvc for drains)", &material_ids()),
        "dn": n("Outer diameter in mm from the material's series: alupex 16/20/26/32, pex 16/20/25, copper 12/15/18/22/28, pvc 32/40/50/75/90/110/125, pe 40–160, steel 21.3 (½\")/26.9 (¾\")/33.7 (1\")"),
        "points": pts("The pipe's path as corners: [x, yAboveFloor, z] … in metres; every corner becomes a bend or an elbow"),
        "corners": en("bend: the pipe is bent at each corner (Alupex ≤ 26, PEX); elbow: an elbow fitting of `joint` at each corner (copper, PVC, larger Alupex). Default per material and dn.", &["bend", "elbow"]),
        "joint": en("Joint type of the fittings on this pipe (default: the material's usual one — alupex press, pex press, copper solder, pvc glue, pe weld, steel thread)", &joint_ids()),
        "bendRadius": n("Bend radius in m at bent corners (default: the material's minimum, 5 × dn for Alupex)"),
        "insulation": n("Insulation thickness in mm (0, 9, 13, 20, 25)"),
        "from": s("What the start connects to: a fitting id (its nearest port), a pipe id (its end → coupling / elbow, or its body → tee), or a construction fixture id (tap, mixer, drain… → wall plate for water)"),
        "to": s("Same for the end"),
        "autoFittings": b("Connect the ends to whatever they touch (default true): a fitting port, another pipe's end (coupling, reducer or elbow inserted), another pipe's body (tee inserted, that pipe is split) or a plumbing fixture (wall plate elbow)"),
        "autoBrackets": b("Also clip the pipe to the nearest wall / ceiling / floor at the material's spacing (default false)"),
        "bracketKind": en("Bracket kind for autoBrackets", &bracket_kind_ids()),
        "name": s("Label, e.g. 'KW badkamer'"), "note": s("Free text")
    })
}

impl PlumbingPlugin {
    fn ops_info() -> Vec<PluginOp> {
        let fitting_desc = format!("Fitting kind: {}", FITTINGS.iter().map(|f| format!("{} ({})", f.id, f.label)).collect::<Vec<_>>().join(", "));
        vec![
            op("catalog", "The plumbing catalogue: systems, pipe materials with their diameter series and joint types, fitting kinds, bracket kinds. No change.", json!({}), &[]),
            op("add_pipe", "Lay a pipe along a polyline of corners (heights above the storey floor). Corners become bends (bendable materials) or elbows of the pipe's joint type. By default the ends connect to whatever they touch: a fitting port, another pipe's end (a coupling / reducer / elbow is inserted and both pipes are cut back to it), another pipe's body (a tee is inserted and that pipe split) or a plumbing fixture (a wall plate elbow for water). Returns the pipe id first in `created`, then any fittings; `measurement` = length.", pipe_props(), &["points"]),
            op("route", "Lay a pipe between two places and let the engine work out the corners, so no measurements are needed. `from` / `to` are [x, yAboveFloor, z] or the id of a fitting (nearest open port), pipe (nearest end, or with fromId / toId and a point: that point on its body, which inserts a tee) or plumbing fixture. The pipe goes to the travel height, across orthogonally (fewest walls crossed, then hugging walls), then to the end. `via`: direct (stay at the start's height, default), ceiling (10 cm under it), floor (10 cm above it), or a `height`. `variant` picks another way across. `pair` also lays a pipe alongside (same fields as the parallel op, without id). Returns the pipe first in `created`; the last entity's data has points, variants, wallCrossings and floorCrossings: the holes to drill.",
               { let mut p = pipe_props(); let o = p.as_object_mut().unwrap(); o.remove("points"); o.insert("from".into(), json!({ "description": "[x, yAboveFloor, z] or an id" })); o.insert("to".into(), json!({ "description": "[x, yAboveFloor, z] or an id" })); o.insert("fromId".into(), s("What the `from` point attaches to")); o.insert("toId".into(), s("What the `to` point attaches to")); o.insert("via".into(), en("Travel height", &["direct", "ceiling", "floor"])); o.insert("height".into(), n("Travel height above the floor in m (overrides via)")); o.insert("variant".into(), i("Another way across: 0 = best")); o.insert("pair".into(), json!({ "type": "object", "description": "Also lay a parallel pipe: system, distance, rise, dn, name" })); p }, &["from", "to"]),
            op("parallel", "Lay a second pipe alongside an existing one (hot next to cold, return next to flow), keeping the same corners. `distance` is in plan, mitred at the corners (default dn + 40 mm); `side` auto (default) stays away from the walls, left / right is seen in the direction of travel, a negative distance means right; `rise` lifts it.",
               json!({ "id": s("The pipe to follow"), "distance": n("Plan distance between the axes in m"), "side": en("Which side", &["auto", "left", "right"]), "rise": n("Height difference in m"), "system": en("System of the new pipe (default the same)", &system_ids()), "material": en("", &material_ids()), "dn": n("mm"), "insulation": n("mm"), "name": s(""), "from": s("What the start connects to"), "to": s("What the end connects to"), "autoFittings": b("Default true"), "autoBrackets": b("Default false") }), &["id"]),
            op("update_pipe", "Change a pipe: deep-merge `patch` (points, system, material, dn, corners, joint, bendRadius, insulation, name, note; null clears).", json!({ "id": s("Pipe id"), "patch": { "type": "object", "description": "Fields to merge", "properties": pipe_props() } }), &["id", "patch"]),
            op("set_segment_length", "Give one straight segment of a pipe an exact length: the points after it move along the segment (anchor start, default) or the points before it (anchor end).", json!({ "id": s("Pipe id"), "segment": i("Segment index, 0 = between the first two corners"), "length": n("New length in m"), "anchor": en("Which side stays put", &["start", "end"]) }), &["id", "length"]),
            op("connect", "Snap one end of a pipe onto a fitting (its nearest port), another pipe (its end → coupling / reducer / elbow, its body → tee) or a plumbing fixture (→ wall plate), and link them.", json!({ "pipe": s("Pipe id"), "end": en("Which end", &["start", "end"]), "to": s("Fitting, pipe or construction fixture id"), "joint": en("Joint type of the inserted fitting (default: the pipe's)", &joint_ids()) }), &["pipe", "to"]),
            op("add_fitting", &format!("Place a fitting. With `onPipe` + `position` near that pipe: at a pipe end it is attached there (aligned with the pipe); inside the pipe an inline fitting (coupling, reducer, valve, filter, meter…) splits the pipe and sits between the halves, a tee splits it and branches towards `position` (or `dir2`). Without `onPipe`: free at `position` with `dir` / `dir2`, or at a construction `fixture` (a mixer / wall plate at a tap). {fitting_desc}."),
               json!({ "kind": en("Fitting kind", &fitting_ids()), "onPipe": s("Pipe id to put it on"), "position": v3("[x, yAboveFloor, z]; with onPipe the nearest point of the pipe is used and the offset picks the branch side"), "fixture": s("Construction fixture id (tap, mixer…) to place it at"),
                      "level": i("Storey index"), "joint": en("Joint type (default: the pipe's / material's)", &joint_ids()), "material": en("", &material_ids()), "system": en("", &system_ids()), "dn": n("Main diameter mm (default: the pipe's)"), "dn2": n("Branch / outlet / reduced diameter mm (tee, reducer, manifold outlets)"),
                      "dir": v3("Main axis (unit; default along the pipe)"), "dir2": v3("Branch / outlet axis (tee branch, elbow outlet, wall plate and mixer out of the wall, manifold outlets)"), "angle": n("Elbow angle in degrees (90, 45, 87.5)"), "thread": s("Thread size of a threaded port, e.g. \"1/2\\\"\""), "outlets": i("Manifold outlets (default 4)"), "note": s("") }), &["kind"]),
            op("update_fitting", "Change a fitting: deep-merge `patch` (kind, joint, dn, dn2, position, dir, dir2, angle, thread, outlets, note).", json!({ "id": s("Fitting id"), "patch": { "type": "object" } }), &["id", "patch"]),
            op("add_bracket", "Clip a pipe to a surface at a point: with `pipe` the point is projected onto it and the surface (nearest wall, ceiling or floor) is chosen unless `normal` is given.", json!({ "pipe": s("Pipe id"), "position": v3("[x, yAboveFloor, z]"), "kind": en("", &bracket_kind_ids()), "normal": v3("From the pipe axis towards the surface"), "standoff": n("Distance from the pipe axis to the surface in m"), "dir": v3("Pipe axis when no pipe is given"), "dn": n("mm when no pipe is given"), "level": i("") }), &["position"]),
            op("update_bracket", "Change a bracket: deep-merge `patch` (kind, position, dir, normal, standoff).", json!({ "id": s("Bracket id"), "patch": { "type": "object" } }), &["id", "patch"]),
            op("auto_brackets", "Clip pipes (default: all) to the nearest wall, ceiling or floor at the material's spacing (Alupex 16: 0.5 m horizontal, ×1.6 vertical; copper 15: 1.25 m; PVC 50: 0.5 m…). Earlier auto brackets of those pipes are replaced.", json!({ "pipes": ids("Pipe ids (default all)"), "spacing": n("Override spacing in m"), "kind": en("Force a bracket kind", &bracket_kind_ids()) }), &[]),
            op("takeoff", "Material takeoff (materiaallijst): pipe length per material / dn / system with bend counts, elbows from pipe corners, fittings per kind / joint / dn, brackets, insulation. No change.", json!({}), &[]),
            op("check", "Check the installation: open pipe ends, diameter and system mismatches at fittings, joint types vs material, bend radii, bracket spacing, drain fall, hot water insulation. Findings in entities[0].data.findings (each with `ids` to select). Advisory.", json!({}), &[]),
            op("summary", "Counts and lengths per system, every pipe with its points and links, every fitting with its ports and whether each port is open. No change.", json!({}), &[]),
            op("import_runs", "Turn the construction plugin's water / drain / heating runs (construction.run, e.g. from import_house) into plumbing pipes with default materials. Idempotent: pipes made from the same runs earlier are replaced (data.source = run id), pipes drawn by hand stay; run it after every import_house.", json!({ "remove": b("Remove the runs afterwards (default true)") }), &[]),
        ]
    }
}

impl Plugin for PlumbingPlugin {
    fn id(&self) -> &str { PLUGIN_ID }
    fn describe(&self) -> PluginInfo { PluginInfo { id: PLUGIN_ID.into(), name: "Plumbing".into(), entity_types: vec![T_PIPE.into(), T_FITTING.into(), T_BRACKET.into()], ops: Self::ops_info() } }
    fn expand(&self, e: &Custom, doc: &Document) -> Result<Vec<Entity>, OpError> {
        Ok(match e.type_.as_str() { T_PIPE => expand::expand_pipe(e, doc), T_FITTING => expand::expand_fitting(e, doc), T_BRACKET => expand::expand_bracket(e, doc), _ => vec![] })
    }
    fn apply(&self, op_type: &str, data: Value, doc: &mut Document) -> Result<OpResult, OpError> {
        ops::apply(op_type.strip_prefix("plumbing.").unwrap_or(op_type), data, doc)
    }
    fn transform(&self, e: &mut Custom, m: &Mat4, _doc: &Document) -> Result<(), OpError> {
        match e.type_.as_str() {
            T_PIPE => { let mut p: Pipe = serde_json::from_value(e.data.clone()).map_err(|x| OpError(x.to_string()))?; for q in p.points.iter_mut() { *q = mat_apply(m, *q); } e.data = serde_json::to_value(p).unwrap(); }
            T_FITTING => { let mut f: Fitting = serde_json::from_value(e.data.clone()).map_err(|x| OpError(x.to_string()))?; f.position = mat_apply(m, f.position); f.dir = normalize(mat_apply_dir(m, f.dir)); f.dir2 = f.dir2.map(|d| normalize(mat_apply_dir(m, d))); e.data = serde_json::to_value(f).unwrap(); }
            T_BRACKET => { let mut b: Bracket = serde_json::from_value(e.data.clone()).map_err(|x| OpError(x.to_string()))?; b.position = mat_apply(m, b.position); b.dir = normalize(mat_apply_dir(m, b.dir)); b.normal = normalize(mat_apply_dir(m, b.normal)); b.auto = false; e.data = serde_json::to_value(b).unwrap(); }
            _ => return Err(OpError("not a plumbing entity".into())),
        }
        Ok(())
    }
    fn paint(&self, e: &mut Custom, _face_key: &str, material: &str, _side: Side) -> Result<(), OpError> {
        match e.type_.as_str() {
            T_PIPE | T_FITTING => { if let Some(o) = e.data.as_object_mut() { o.insert("paint".into(), Value::String(material.into())); } Ok(()) }
            _ => Err(OpError("brackets are not painted".into())),
        }
    }
    fn metrics(&self, doc: &Document) -> Option<Value> {
        if !doc.entities.iter().any(|e| matches!(e, Entity::Custom(c) if c.type_.starts_with("plumbing."))) { return None; }
        Some(ops::metrics(doc))
    }
    fn on_load(&self, doc: &mut Document) { ensure(doc); }
}
