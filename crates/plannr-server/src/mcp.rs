//! MCP server over Streamable HTTP (JSON-RPC 2.0, JSON responses). Every modelling op, the
//! project management calls and the browser viewport commands are exposed as tools.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use plannr_core::{export_glb, CompileOptions, Op};
use serde_json::{json, Map, Value};

use crate::error::ApiError;
use crate::routes::{default_label, flatten_entities};
use crate::state::Shared;

const COORDS: &str = " Coordinates are metres, right-handed, y up (x red, y blue/up, z green); points are [x, y, z].";

enum Kind { Op(&'static str), Viewport(&'static str), Builtin(&'static str), Plugin(String, String) }

struct ToolDef { name: String, description: String, schema: Value, kind: Kind }

// ---- schema helpers ----------------------------------------------------------------
fn pt(desc: &str) -> Value { json!({ "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3, "description": desc }) }
fn pts(desc: &str) -> Value { json!({ "type": "array", "items": pt("point"), "description": desc }) }
fn ids(desc: &str) -> Value { json!({ "type": "array", "items": { "type": "string" }, "description": desc }) }
fn s(desc: &str) -> Value { json!({ "type": "string", "description": desc }) }
fn n(desc: &str) -> Value { json!({ "type": "number", "description": desc }) }
fn b(desc: &str) -> Value { json!({ "type": "boolean", "description": desc }) }
fn schema(props: Value, required: &[&str]) -> Value {
    let mut p = props;
    p["project"] = s("Project id or name; defaults to the active project (see use_project).");
    json!({ "type": "object", "properties": p, "required": required })
}

fn tool(name: &str, description: &str, props: Value, required: &[&str], kind: Kind) -> ToolDef {
    ToolDef { name: name.into(), description: description.into(), schema: schema(props, required), kind }
}

fn tools(state: &Shared) -> Vec<ToolDef> {
    let layer = || s("Layer id or name (default: the first layer)");
    let material = || s("Material id or name");
    let mut t = vec![
        // ---- projects -------------------------------------------------------------
        tool("list_projects", "List Plannr projects (id, name, version, entity count).", json!({}), &[], Kind::Builtin("list_projects")),
        tool("create_project", "Create an empty project and make it the active one.", json!({ "name": s("Project name") }), &["name"], Kind::Builtin("create_project")),
        tool("use_project", "Set the active project used by every other tool when `project` is omitted.", json!({}), &["project"], Kind::Builtin("use_project")),
        tool("get_document", "The project's document: layers, materials, scenes, and (optionally) all entities.", json!({ "includeEntities": b("Include the entity tree (default true)") }), &[], Kind::Builtin("get_document")),
        tool("list_entities", "Flat list of entities (through groups) with their world path; filter by kind (face, edge, group, instance, guide, dimension, text, section, custom) and layer.", json!({ "kind": s("Entity kind filter"), "layer": s("Layer id or name filter") }), &[], Kind::Builtin("list_entities")),
        tool("get_entity", "One entity by id.", json!({ "id": s("Entity id") }), &["id"], Kind::Builtin("get_entity")),
        tool("get_metrics", "Face/edge/group counts, total area, area per material, loose edge length and bounds of the compiled model.", json!({}), &[], Kind::Builtin("get_metrics")),
        tool("undo", "Undo the last change.", json!({}), &[], Kind::Builtin("undo")),
        tool("redo", "Redo the last undone change.", json!({}), &[], Kind::Builtin("redo")),
        tool("history", "Undo history (version and label per step).", json!({}), &[], Kind::Builtin("history")),
        tool("apply_ops", &format!("Apply several ops as one undo step. Each op is an object with an `op` field naming one of the op tools (draw_line, push_pull, move, …) plus that tool's fields.{COORDS}"), json!({ "ops": { "type": "array", "items": { "type": "object" }, "description": "Op objects" }, "label": s("Undo label") }), &["ops"], Kind::Builtin("apply_ops")),
        tool("export_glb", "Export the model as a glTF binary file on the server; returns the path and download URL.", json!({ "path": s("File path to write (default data/exports/<project>.glb)") }), &[], Kind::Builtin("export_glb")),
        tool("list_plugins", "Registered Rust plugins with their entity types and ops.", json!({}), &[], Kind::Builtin("list_plugins")),
        tool("list_components", "Component definitions of the project with their entity and instance counts.", json!({}), &[], Kind::Builtin("list_components")),
        // ---- ops ------------------------------------------------------------------------
        tool("draw_line", &format!("Draw connected edges through points. When the new edges close a planar loop with existing loose edges, a face is created (like SketchUp).{COORDS}"), json!({ "points": pts("At least 2 points"), "close": b("Also connect the last point back to the first"), "layer": layer() }), &["points"], Kind::Op("draw_line")),
        tool("draw_rectangle", &format!("Draw a rectangular face in the plane through `corner` with `normal`, from `corner` to `opposite` (ground: normal [0,1,0]).{COORDS}"), json!({ "corner": pt("First corner"), "opposite": pt("Opposite corner (projected onto the plane)"), "normal": pt("Plane normal, e.g. [0,1,0] for the ground"), "u": pt("Optional side direction"), "layer": layer(), "material": material() }), &["corner", "opposite", "normal"], Kind::Op("draw_rectangle")),
        tool("draw_polygon", &format!("Draw one planar face from 3+ points.{COORDS}"), json!({ "points": pts("Coplanar points, 3 or more"), "layer": layer(), "material": material() }), &["points"], Kind::Op("draw_polygon")),
        tool("draw_circle", &format!("Draw a circular face (regular polygon).{COORDS}"), json!({ "center": pt("Centre"), "normal": pt("Plane normal"), "radius": n("Radius in metres"), "segments": { "type": "integer", "description": "Segments (default 24)" }, "layer": layer(), "material": material() }), &["center", "normal", "radius"], Kind::Op("draw_circle")),
        tool("push_pull", "Extrude a face along its normal by `distance` (negative pushes in). The face keeps its id and becomes the cap; side faces and a base face are created.", json!({ "face": s("Face id"), "distance": n("Metres, non-zero") }), &["face", "distance"], Kind::Op("push_pull")),
        tool("offset", "Create a new face by offsetting a face's outline in its plane (positive = outward).", json!({ "face": s("Face id"), "distance": n("Metres, non-zero") }), &["face", "distance"], Kind::Op("offset")),
        tool("move", &format!("Translate entities by a delta.{COORDS}"), json!({ "ids": ids("Entity ids"), "delta": pt("Translation") }), &["ids", "delta"], Kind::Op("move")),
        tool("rotate", "Rotate entities about an axis through `origin` by `angle` degrees (right-handed).", json!({ "ids": ids("Entity ids"), "origin": pt("Axis origin"), "axis": pt("Axis direction, e.g. [0,1,0]"), "angle": n("Degrees") }), &["ids", "origin", "axis", "angle"], Kind::Op("rotate")),
        tool("scale", "Scale entities about `origin` by per-axis factors.", json!({ "ids": ids("Entity ids"), "origin": pt("Scale origin"), "factors": pt("[sx, sy, sz]") }), &["ids", "origin", "factors"], Kind::Op("scale")),
        tool("copy", "Duplicate entities, each copy offset by `delta` more than the previous one.", json!({ "ids": ids("Entity ids"), "delta": pt("Offset per copy"), "count": { "type": "integer", "description": "Number of copies (default 1)" } }), &["ids", "delta"], Kind::Op("copy")),
        tool("erase", "Remove entities (a group with its children).", json!({ "ids": ids("Entity ids") }), &["ids"], Kind::Op("erase")),
        tool("set_hidden", "Hide or show entities (Edit → Hide): a group or component with everything in it, a face, an edge, a plugin entity. Hidden geometry stays in the document but is left out of the view, slices and exports.", json!({ "ids": ids("Entity ids"), "hidden": b("true hides, false shows") }), &["ids", "hidden"], Kind::Op("set_hidden")),
        tool("paint", "Apply a material to faces (or every face inside a group). `material` may be a material id, a name, or a #rrggbb colour (created on the fly).", json!({ "ids": ids("Face or group ids"), "material": s("Material id, name or #hex"), "side": { "type": "string", "enum": ["front", "back", "both"], "description": "Which side (default front)" } }), &["ids", "material"], Kind::Op("paint")),
        tool("set_layer", "Move entities to a layer.", json!({ "ids": ids("Entity ids"), "layer": s("Layer id or name") }), &["ids", "layer"], Kind::Op("set_layer")),
        tool("make_group", "Wrap top-level entities in a group.", json!({ "ids": ids("Entity ids"), "name": s("Group name") }), &["ids"], Kind::Op("make_group")),
        tool("explode", "Dissolve a group into its parent.", json!({ "id": s("Group id") }), &["id"], Kind::Op("explode")),
        tool("add_layer", "Add a layer (tag).", json!({ "name": s("Layer name"), "color": s("#rrggbb") }), &["name"], Kind::Op("add_layer")),
        tool("update_layer", "Rename, hide/show or recolour a layer.", json!({ "id": s("Layer id or name"), "name": s("New name"), "visible": b("Visible"), "color": s("#rrggbb") }), &["id"], Kind::Op("update_layer")),
        tool("remove_layer", "Remove a layer; its entities move to the first layer.", json!({ "id": s("Layer id or name") }), &["id"], Kind::Op("remove_layer")),
        tool("add_material", "Add a material.", json!({ "name": s("Name"), "color": s("#rrggbb"), "opacity": n("0–1 (default 1)") }), &["name", "color"], Kind::Op("add_material")),
        tool("update_material", "Change a material's name, colour or opacity.", json!({ "id": s("Material id or name"), "name": s("New name"), "color": s("#rrggbb"), "opacity": n("0–1") }), &["id"], Kind::Op("update_material")),
        tool("add_guide", "Add a guide point, or an infinite guide line when `direction` is given.", json!({ "point": pt("Point"), "direction": pt("Line direction"), "layer": layer() }), &["point"], Kind::Op("add_guide")),
        tool("add_dimension", "Add a linear dimension between two points, drawn offset from them.", json!({ "a": pt("Start"), "b": pt("End"), "offset": pt("Offset vector for the dimension line"), "layer": layer() }), &["a", "b", "offset"], Kind::Op("add_dimension")),
        tool("add_text", "Add a text label, optionally with a leader to `anchor`.", json!({ "position": pt("Label position"), "text": s("Text"), "anchor": pt("Leader target"), "layer": layer() }), &["position", "text"], Kind::Op("add_text")),
        tool("add_section", "Add a section plane; when active, everything on the +normal side is cut away. Several active planes combine: what remains is what all of them keep.", json!({ "origin": pt("Point on the plane"), "normal": pt("Plane normal (cut-away side)"), "active": b("Active (default true)"), "layer": layer() }), &["origin", "normal"], Kind::Op("add_section")),
        tool("update_section", "Move, re-orient or (de)activate a section plane.", json!({ "id": s("Section id"), "origin": pt("Point on the plane"), "normal": pt("Normal"), "active": b("Active") }), &["id"], Kind::Op("update_section")),
        tool("slice_section", "Create a 2D slice: the cut lines of the whole model (plugin geometry included) at a section plane, as a new group of edges in the plane (on a \"Slices\" tag unless `layer` is given). Joints between touching pieces of the same material are left out.", json!({ "id": s("Section id"), "layer": layer(), "name": s("Group name (default \"Slice N\")") }), &["id"], Kind::Op("slice_section")),
        tool("update_entity", "Shallow-merge fields into an entity (name, text, outer, material, …).", json!({ "id": s("Entity id"), "patch": { "type": "object", "description": "Fields to set" } }), &["id", "patch"], Kind::Op("update_entity")),
        tool("measure", "Distance between two points (no change).", json!({ "a": pt("A"), "b": pt("B") }), &["a", "b"], Kind::Op("measure")),
        tool("set_extension", "Store plugin data on the document.", json!({ "plugin": s("Plugin id"), "data": { "description": "Any JSON" } }), &["plugin"], Kind::Op("set_extension")),
        tool("custom", "Run a plugin op (`type` = `<plugin>.<op>`).", json!({ "type": s("<plugin>.<op>"), "data": { "type": "object" } }), &["type"], Kind::Op("custom")),
        tool("draw_arc", &format!("Draw an arc as connected edges (a face forms when the arc closes a planar loop, like draw_line). Three ways: centre arc `center`+`start`+`end` (radius = |start−center|, sweeps counter-clockwise about `normal` from start towards the direction of end; `pie` adds the two radii and the sector face), three-point `through` [p1,p2,p3], or two-point `from`+`to`+`bulge` (signed sagitta towards normal×chord; `closeChord` adds the chord so a segment face forms). Default normal: derived from the points, else y-up. Segments default to 12 per quarter turn.{COORDS}"), json!({ "center": pt("Centre (centre-arc form)"), "start": pt("Start point on the circle (centre-arc form)"), "end": pt("Point giving the end direction (centre-arc form)"), "through": pts("Three points: start, a point on the arc, end (three-point form)"), "from": pt("Chord start (two-point form)"), "to": pt("Chord end (two-point form)"), "bulge": n("Signed sagitta in metres (two-point form)"), "normal": pt("Plane normal (optional)"), "segments": { "type": "integer", "description": "Number of edges (1–200, default 12 per quarter turn)" }, "pie": b("Centre-arc form: also draw the radii and the sector face"), "closeChord": b("Two-point form: also draw the chord so a face forms"), "layer": layer() }), &[], Kind::Op("draw_arc")),
        tool("follow_me", &format!("Sweep a face along a path (SketchUp Follow Me): mouldings, pipes, rails. `path` is a polyline (points relative to each other; the face is the profile), or `pathEdges` names existing edges to chain. The profile is turned perpendicular to the first segment, mitred at every bend, the original face becomes the start cap (keeps its id) and an end cap is added; a closed path (first point = last) makes a ring without caps.{COORDS}"), json!({ "face": s("Profile face id"), "path": pts("Path points (2 or more)"), "pathEdges": ids("Edge ids forming one connected chain") }), &["face"], Kind::Op("follow_me")),
        tool("mirror", &format!("Reflect entities across a plane (face windings are fixed so normals stay outward; groups and component instances get a mirrored transform). `copy` keeps the originals and returns the mirrored copies.{COORDS}"), json!({ "ids": ids("Entity ids"), "origin": pt("Point on the mirror plane"), "normal": pt("Mirror plane normal"), "copy": b("Keep the originals (default false)") }), &["ids", "origin", "normal"], Kind::Op("mirror")),
        tool("make_component", "Turn entities into a reusable component: a definition (origin at the min corner of their bounding box) plus one instance replacing them. Returns the instance id; the definition id is `entities[0].definition`.", json!({ "ids": ids("Entity ids"), "name": s("Component name"), "description": s("Description") }), &["ids", "name"], Kind::Op("make_component")),
        tool("place_component", &format!("Place another instance of a component definition.{COORDS}"), json!({ "definition": s("Definition id or name"), "position": pt("Where the definition origin goes"), "rotation": n("Degrees about the vertical axis"), "scale": { "description": "Uniform number or [sx, sy, sz]" }, "layer": layer() }), &["definition", "position"], Kind::Op("place_component")),
        tool("make_unique", "Give an instance its own copy of the definition, so edits no longer affect the others.", json!({ "instance": s("Instance id") }), &["instance"], Kind::Op("make_unique")),
        tool("edit_component", "Apply ops inside a component definition (ids inside the definition are stable: erase, move, push_pull, paint, draw_*…). Every instance updates.", json!({ "definition": s("Definition id or name"), "ops": { "type": "array", "items": { "type": "object" }, "description": "Op objects as for apply_ops" } }), &["definition", "ops"], Kind::Op("edit_component")),
        tool("rename_component", "Rename a component definition.", json!({ "definition": s("Definition id or name"), "name": s("New name") }), &["definition", "name"], Kind::Op("rename_component")),
        tool("remove_component", "Remove a component definition; fails while instances exist unless `force` erases them too.", json!({ "definition": s("Definition id or name"), "force": b("Also erase its instances") }), &["definition"], Kind::Op("remove_component")),
        // ---- viewport (needs a browser open on the project) --------------------------------
        tool("screenshot", "Take a PNG screenshot of the 3D viewport. Needs a browser open on the project (http://localhost:<port>/?project=<id>). Optionally switch to a standard view first.", json!({ "width": { "type": "integer", "description": "Pixel width (default: viewport)" }, "height": { "type": "integer" }, "view": { "type": "string", "enum": ["iso", "top", "bottom", "front", "back", "left", "right"], "description": "Standard view to apply before capturing" } }), &[], Kind::Viewport("screenshot")),
        tool("set_view", "Switch the browser camera to a standard view.", json!({ "view": { "type": "string", "enum": ["iso", "top", "bottom", "front", "back", "left", "right"] } }), &["view"], Kind::Viewport("set_view")),
        tool("set_camera", "Place the browser camera.", json!({ "position": pt("Eye"), "target": pt("Look-at"), "up": pt("Up vector (default [0,1,0])") }), &["position", "target"], Kind::Viewport("set_camera")),
        tool("zoom_extents", "Frame the whole model in the browser.", json!({}), &[], Kind::Viewport("zoom_extents")),
        tool("zoom_to", "Frame the given entities in the browser.", json!({ "ids": ids("Entity ids") }), &["ids"], Kind::Viewport("zoom_to")),
        tool("zoom_window", "Zoom the browser camera to a rectangle of the viewport given as fractions 0–1 (top-left origin).", json!({ "x0": n("Left (0–1)"), "y0": n("Top (0–1)"), "x1": n("Right (0–1)"), "y1": n("Bottom (0–1)") }), &["x0", "y0", "x1", "y1"], Kind::Viewport("zoom_window")),
        tool("previous_view", "Go back to the previous browser camera position.", json!({}), &[], Kind::Viewport("previous_view")),
        tool("next_view", "Go forward again in the browser camera history.", json!({}), &[], Kind::Viewport("next_view")),
        tool("orbit", "Orbit the browser camera by pixel deltas.", json!({ "dx": n("Horizontal pixels"), "dy": n("Vertical pixels") }), &["dx", "dy"], Kind::Viewport("orbit")),
        tool("pan", "Pan the browser camera by pixel deltas.", json!({ "dx": n("Horizontal pixels"), "dy": n("Vertical pixels") }), &["dx", "dy"], Kind::Viewport("pan")),
        tool("zoom", "Dolly the browser camera (factor < 1 zooms in).", json!({ "factor": n("Distance multiplier") }), &["factor"], Kind::Viewport("zoom")),
        tool("select", "Set the browser selection.", json!({ "ids": ids("Entity ids"), "mode": { "type": "string", "enum": ["replace", "add", "toggle"] } }), &["ids"], Kind::Viewport("select")),
        tool("activate_tool", "Activate a browser tool (select, eraser, line, rectangle, circle, polygon, pushpull, offset, move, rotate, scale, tape, text, dimension, paint, orbit, pan, zoom, section, or a plugin tool).", json!({ "tool": s("Tool id") }), &["tool"], Kind::Viewport("activate_tool")),
        tool("get_view_state", "Camera, selection, hover and active tool of the browser.", json!({}), &[], Kind::Viewport("get_state")),
        tool("set_options", "Toggle browser display options. `faceStyle` sets how the whole model is drawn. `xray` lists material ids drawn through everything in front of them (e.g. [\"electrical.conduit\", \"electrical.cable\"] shows conduits inside walls; [] turns it off).", json!({ "gridVisible": b("Grid"), "axesVisible": b("Axes"), "edgesVisible": b("Black edges"), "shadows": b("Shadows"), "faceStyle": { "type": "string", "enum": ["shaded", "monochrome", "hiddenline", "wireframe", "xray"], "description": "Face style of the whole model, as in SketchUp" }, "backEdges": b("Dashed lines for the edges behind faces"), "hiddenGeometry": b("Show hidden entities as ghosts"), "xray": { "type": "array", "items": { "type": "string" }, "description": "Material ids to draw in X-ray" } }), &[], Kind::Viewport("set_options")),
    ];
    for p in state.registry.list() {
        for op in p.ops {
            let mut schema = op.input_schema;
            if !schema.is_object() { schema = json!({ "type": "object", "properties": {} }); }
            if schema.get("properties").map_or(true, |x| !x.is_object()) { schema["properties"] = json!({}); }
            schema["properties"]["project"] = s("Project id or name; defaults to the active project (see use_project).");
            t.push(ToolDef { name: format!("plugin.{}.{}", p.id, op.name), description: op.description, schema, kind: Kind::Plugin(p.id.clone(), op.name) });
        }
    }
    t
}

// ---- JSON-RPC plumbing -----------------------------------------------------------------

pub async fn method_not_allowed() -> Response {
    (StatusCode::METHOD_NOT_ALLOWED, Json(json!({ "error": "use POST with JSON-RPC 2.0 (Streamable HTTP, JSON responses)" }))).into_response()
}

fn rpc_error(id: Value, code: i64, msg: impl Into<String>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg.into() } })
}
fn rpc_result(id: Value, result: Value) -> Value { json!({ "jsonrpc": "2.0", "id": id, "result": result }) }

pub async fn handle(State(s): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let auth = headers.get("authorization").and_then(|h| h.to_str().ok()).unwrap_or("");
    let ok = auth.strip_prefix("Bearer ").map(|t| t.trim() == s.token).unwrap_or(false);
    if !ok {
        return (StatusCode::UNAUTHORIZED, Json(rpc_error(Value::Null, -32001, "unauthorized: send `Authorization: Bearer <PLANNR_TOKEN>`"))).into_response();
    }
    let req: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(rpc_error(Value::Null, -32700, format!("parse error: {e}")))).into_response(),
    };
    if let Value::Array(items) = req {
        let mut out = vec![];
        for item in items { if let Some(r) = dispatch(&s, item).await { out.push(r); } }
        return if out.is_empty() { StatusCode::ACCEPTED.into_response() } else { Json(Value::Array(out)).into_response() };
    }
    match dispatch(&s, req).await {
        Some(r) => Json(r).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

async fn dispatch(s: &Shared, req: Value) -> Option<Value> {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(json!({}));
    if method.starts_with("notifications/") { return None; }
    Some(match method {
        "initialize" => rpc_result(id, json!({
            "protocolVersion": params.get("protocolVersion").cloned().unwrap_or(json!("2025-03-26")),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "plannr", "version": plannr_core::VERSION },
            "instructions": format!("Plannr is a generic 3D modeller (SketchUp-like). Model with the op tools; look at the result with `screenshot` (needs a browser open on the project at http://localhost:{}/?project=<id>). Call `use_project` once, or pass `project` on each call.{COORDS}", s.port)
        })),
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(id, json!({ "tools": tools(s).iter().map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": t.schema })).collect::<Vec<_>>() })),
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call(s, &name, args).await {
                Ok(content) => rpc_result(id, json!({ "content": content, "isError": false })),
                Err(e) => rpc_result(id, json!({ "content": [{ "type": "text", "text": e }], "isError": true })),
            }
        }
        _ => rpc_error(id, -32601, format!("method not found: {method}")),
    })
}

fn text(v: Value) -> Vec<Value> {
    let t = if v.is_string() { v.as_str().unwrap().to_string() } else { serde_json::to_string_pretty(&v).unwrap_or_default() };
    vec![json!({ "type": "text", "text": t })]
}

async fn call(s: &Shared, name: &str, args: Value) -> Result<Vec<Value>, String> {
    let defs = tools(s);
    let def = defs.iter().find(|t| t.name == name).ok_or_else(|| format!("unknown tool `{name}`"))?;
    let mut args = match args { Value::Object(m) => m, _ => Map::new() };
    let project_ref = args.remove("project").and_then(|p| p.as_str().map(|x| x.to_string()));
    let api = |e: ApiError| e.to_string();
    match &def.kind {
        Kind::Builtin("list_projects") => Ok(text(json!(s.db.list().map_err(api)?))),
        Kind::Builtin("create_project") => {
            let name = args.get("name").and_then(|n| n.as_str()).unwrap_or("Untitled").trim().to_string();
            let p = s.create(&name, None).map_err(api)?;
            *s.active_project.lock().unwrap_or_else(|e| e.into_inner()) = Some(p.id.clone());
            Ok(text(json!(p)))
        }
        Kind::Builtin("use_project") => {
            let id = s.resolve(project_ref.as_deref()).map_err(api)?;
            *s.active_project.lock().unwrap_or_else(|e| e.into_inner()) = Some(id.clone());
            Ok(text(json!(s.db.summary(&id).map_err(api)?)))
        }
        Kind::Builtin("list_plugins") => Ok(text(json!(s.registry.list()))),
        Kind::Builtin(b) => {
            let id = s.resolve(project_ref.as_deref()).map_err(api)?;
            builtin(s, b, &id, &args).await
        }
        Kind::Op(op) => {
            let id = s.resolve(project_ref.as_deref()).map_err(api)?;
            let mut v = Value::Object(args);
            v["op"] = json!(op);
            let op: Op = serde_json::from_value(v).map_err(|e| format!("invalid arguments: {e}"))?;
            let label = default_label(std::slice::from_ref(&op));
            let (version, results) = s.apply_ops(&id, std::slice::from_ref(&op), &label, "mcp").map_err(api)?;
            Ok(text(json!({ "version": version, "result": results[0] })))
        }
        Kind::Viewport(cmd) => {
            let id = s.resolve(project_ref.as_deref()).map_err(api)?;
            viewport(s, cmd, &id, args).await
        }
        Kind::Plugin(plugin, op) => {
            let id = s.resolve(project_ref.as_deref()).map_err(api)?;
            let op = Op::Custom { type_: format!("{plugin}.{op}"), data: Value::Object(args) };
            let (version, results) = s.apply_ops(&id, std::slice::from_ref(&op), name, "mcp").map_err(api)?;
            Ok(text(json!({ "version": version, "result": results[0] })))
        }
    }
}

async fn builtin(s: &Shared, name: &str, id: &str, args: &Map<String, Value>) -> Result<Vec<Value>, String> {
    let api = |e: ApiError| e.to_string();
    match name {
        "get_document" => {
            let mut doc = serde_json::to_value(s.document(id).map_err(api)?).map_err(|e| e.to_string())?;
            let include = args.get("includeEntities").and_then(|v| v.as_bool()).unwrap_or(true);
            if !include { doc["entityCount"] = json!(doc["entities"].as_array().map(|a| a.len()).unwrap_or(0)); doc["entities"] = json!("(omitted)"); }
            let p = s.db.summary(id).map_err(api)?;
            Ok(text(json!({ "id": p.id, "name": p.name, "document": doc })))
        }
        "list_entities" => {
            let doc = s.document(id).map_err(api)?;
            Ok(text(json!(flatten_entities(&doc, args.get("kind").and_then(|v| v.as_str()), args.get("layer").and_then(|v| v.as_str())))))
        }
        "get_entity" => {
            let eid = args.get("id").and_then(|v| v.as_str()).ok_or("id is required")?;
            let doc = s.document(id).map_err(api)?;
            let e = doc.find(eid).ok_or_else(|| format!("no entity {eid}"))?;
            Ok(text(serde_json::to_value(e).map_err(|e| e.to_string())?))
        }
        "get_metrics" => {
            let c = s.compile(id, &CompileOptions::default()).map_err(api)?;
            Ok(text(json!({ "metrics": c.metrics, "bounds": c.bounds, "version": c.version })))
        }
        "undo" => { let (v, l) = s.undo(id, "mcp").map_err(api)?; Ok(text(json!({ "version": v, "undone": l }))) }
        "redo" => { let (v, l) = s.redo(id, "mcp").map_err(api)?; Ok(text(json!({ "version": v, "redone": l }))) }
        "history" => Ok(text(json!(s.history(id).map_err(api)?))),
        "list_components" => {
            let doc = s.document(id).map_err(api)?;
            let list: Vec<Value> = doc.components.iter().map(|c| json!({ "id": c.id, "name": c.name, "description": c.description, "entities": c.entities.len(), "instances": doc.instances_of(&c.id).len() })).collect();
            Ok(text(json!(list)))
        }
        "apply_ops" => {
            let ops: Vec<Op> = serde_json::from_value(args.get("ops").cloned().unwrap_or(json!([]))).map_err(|e| format!("invalid ops: {e}"))?;
            if ops.is_empty() { return Err("ops is empty".into()); }
            let label = args.get("label").and_then(|l| l.as_str()).map(|l| l.to_string()).unwrap_or_else(|| default_label(&ops));
            let (version, results) = s.apply_ops(id, &ops, &label, "mcp").map_err(api)?;
            Ok(text(json!({ "version": version, "results": results })))
        }
        "export_glb" => {
            let p = s.db.get(id).map_err(api)?;
            let scene = s.compile(id, &CompileOptions { ignore_sections: true, ..Default::default() }).map_err(api)?;
            let bytes = export_glb(&scene);
            let safe: String = p.name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
            let path = args.get("path").and_then(|v| v.as_str()).map(|p| p.to_string()).unwrap_or_else(|| format!("data/exports/{safe}.glb"));
            if let Some(dir) = std::path::Path::new(&path).parent() { std::fs::create_dir_all(dir).map_err(|e| e.to_string())?; }
            std::fs::write(&path, &bytes).map_err(|e| format!("cannot write {path}: {e}"))?;
            let abs = std::fs::canonicalize(&path).map(|p| p.display().to_string()).unwrap_or(path.clone());
            Ok(text(json!({ "path": abs, "bytes": bytes.len(), "url": format!("http://localhost:{}/api/projects/{}/export.glb", s.port, id) })))
        }
        _ => Err(format!("unknown builtin {name}")),
    }
}

async fn viewport(s: &Shared, cmd: &str, id: &str, mut args: Map<String, Value>) -> Result<Vec<Value>, String> {
    if cmd == "screenshot" {
        if let Some(view) = args.remove("view").and_then(|v| v.as_str().map(|x| x.to_string())) {
            s.hub.command(id, s.port, "set_view", json!({ "view": view })).await?;
            tokio::time::sleep(std::time::Duration::from_millis(450)).await;
        }
        let r = s.hub.command(id, s.port, "screenshot", Value::Object(args)).await?;
        let png = r.get("png").and_then(|p| p.as_str()).ok_or("browser returned no image")?.to_string();
        let png = png.split(',').next_back().unwrap_or(&png).to_string();
        let state = s.hub.last_state(id).unwrap_or(Value::Null);
        let mut out = vec![json!({ "type": "image", "data": png, "mimeType": "image/png" })];
        out.extend(text(json!({ "width": r.get("width"), "height": r.get("height"), "camera": state.get("camera"), "tool": state.get("tool"), "selection": state.get("selection") })));
        return Ok(out);
    }
    let r = s.hub.command(id, s.port, cmd, Value::Object(args)).await?;
    Ok(text(r))
}
