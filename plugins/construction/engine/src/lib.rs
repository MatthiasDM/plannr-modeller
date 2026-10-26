//! Construction plugin for Plannr: storeys, rooms, walls, openings, roofs, fixtures and runs as
//! semantic `custom` entities, expanded into core faces at compile time. See docs/CONSTRUCTION.md.

pub mod catalogue;
pub mod data;
pub mod expand;
pub mod fixtures;
pub mod masonry;
pub mod ops;
pub mod roof;
pub mod script;

use plannr_core::geom::{mat_apply, mat_apply_dir, Mat4};
use plannr_core::model::{Custom, Document, Entity};
use plannr_core::ops::{OpError, OpResult};
use plannr_core::plugin::{Plugin, PluginInfo, PluginOp, Side};
use serde_json::{json, Value};

use data::*;

#[derive(Default)]
pub struct ConstructionPlugin;

const OPENING_KINDS: [&str; 4] = ["door", "window", "passage", "garage"];
const ROOF_STYLES: [&str; 4] = ["flat", "gable", "hip", "shed"];
const SYSTEMS: [&str; 5] = ["electrical", "waterCold", "waterHot", "drain", "heating"];
const PLAN: &str = "Plan points are [x, z] in metres (y is up). ";

fn p2(desc: &str) -> Value { json!({ "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2, "description": desc }) }
fn p3(desc: &str) -> Value { json!({ "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3, "description": desc }) }
fn n(desc: &str) -> Value { json!({ "type": "number", "description": desc }) }
fn i(desc: &str) -> Value { json!({ "type": "integer", "minimum": 0, "description": desc }) }
fn s(desc: &str) -> Value { json!({ "type": "string", "description": desc }) }
fn b(desc: &str) -> Value { json!({ "type": "boolean", "description": desc }) }
fn en(desc: &str, values: &[&str]) -> Value { json!({ "type": "string", "enum": values, "description": desc }) }
fn host() -> Value {
    json!({ "description": "Host wall: {\"room\": roomId, \"edge\": n} (edge n runs from points[n] to points[n+1]) or {\"wall\": wallId}",
            "oneOf": [ { "type": "object", "properties": { "room": { "type": "string" }, "edge": { "type": "integer", "minimum": 0 } }, "required": ["room", "edge"] },
                       { "type": "object", "properties": { "wall": { "type": "string" } }, "required": ["wall"] } ] })
}
fn op(name: &str, description: &str, props: Value, required: &[&str]) -> PluginOp {
    PluginOp { name: name.into(), description: description.into(), input_schema: json!({ "type": "object", "properties": props, "required": required }) }
}

impl ConstructionPlugin {
    fn ops_info() -> Vec<PluginOp> {
        vec![
            op("setup", "Create or update the construction settings: storeys (name + height, in order from the ground), wall thickness, floor thickness and cavity-wall build-up. Existing fields are kept when omitted.",
               json!({ "storeys": { "type": "array", "items": { "type": "object", "properties": { "name": s("Storey name"), "height": n("Storey height in metres") } } },
                       "groundLevel": n("Height of the ground floor above the world origin (m), default 0; raises the whole house"), "wallThickness": n("Default wall thickness (m), default 0.10"), "floorThickness": n("Floor slab thickness (m), default 0.06"),
                       "cavity": { "type": "object", "properties": { "enabled": b("Build exterior walls as inner leaf + cavity + outer leaf"), "innerLeaf": n("m"), "cavity": n("m"), "outerLeaf": n("m") } } }), &[]),
            op("add_storey", "Append a storey on top; returns its index in `measurement`.", json!({ "name": s("Name (default: Ground floor / First floor / …)"), "height": n("Height in metres (default: the storey below)") }), &[]),
            op("update_storey", "Rename a storey or change its height.", json!({ "index": i("Storey index, 0 = ground floor"), "name": s("New name"), "height": n("New height (m)") }), &["index"]),
            op("remove_storey", "Remove a storey and everything on it; higher storeys shift down.", json!({ "index": i("Storey index") }), &["index"]),
            op("add_room", &format!("Add a room: a floor slab plus one wall per edge. Give `points` (3+ plan corners in order; edge n runs from points[n] to points[n+1]) or `origin` + `size` for a rectangle (edges: 0 along +x from the origin, 1 along +z, 2 back, 3 down). Walls shared with another room on the same storey are built once. {PLAN}"),
               json!({ "name": s("Room name"), "level": i("Storey index (0 = ground floor)"), "points": { "type": "array", "items": p2("corner"), "description": "Plan corners" },
                       "origin": p2("Rectangle origin [x, z]"), "size": p2("Rectangle size [width along x, depth along z]"), "hasFloor": b("Build a floor slab (default true)"), "hasCeiling": b("Build a ceiling (default false)"),
                       "wallHeight": n("Wall height (default: storey height)") }), &[]),
            op("add_wall", &format!("Add a free-standing wall between two plan points. {PLAN}"),
               json!({ "level": i("Storey index"), "a": p2("Start [x, z]"), "b": p2("End [x, z]"), "height": n("Height (default: storey height)"), "thickness": n("Thickness (default: settings)"), "exterior": b("Treat as an exterior wall (brick outside, cavity build-up)") }), &["a", "b"]),
            op("add_opening", "Cut a door, window, passage or garage gate into a wall. `along` is the distance from the wall's start to the opening's centre; defaults per kind: door 0.9×2.05, window 1.2×1.4 sill 0.9, passage 1.0×2.1, garage 2.5×2.1.",
               json!({ "kind": en("Opening kind", &OPENING_KINDS), "host": host(), "along": n("Centre distance from the wall start (m); default: the middle"), "width": n("m"), "height": n("m"), "sill": n("Height of the bottom above the floor (m)") }), &["kind", "host"]),
            op("add_roof", &format!("Add a roof over a plan rectangle (`min`/`max`; default: the footprint of the rooms on the top storey), its sides snapped to the outer faces of the exterior walls under it. `levels` = number of storeys under it (default: all). Built like a Belgian brick house: a wall plate (muurplaat) on the inner leaf and cavity of the eaves walls, purlins and a ridge purlin bearing in the masonry gables, rafters birdsmouthed on the plates, counter battens, battens and the covering, fascia, soffit and verge boards; hip: hip and jack rafters on plates all round; flat: joists, deck, insulation, membrane behind a masonry upstand. The walls under it rise into gables in their own build-up and stop under the rafters; a lower roof stops at the taller part of the building. {PLAN}"),
               json!({ "style": en("Roof style", &ROOF_STYLES), "pitchHeight": n("Rise of the rafter underside from the outer wall face to the ridge (m), default 1.6"), "overhang": n("Eaves overhang (m), default 0.3"), "vergeOverhang": n("Overhang past the gables (m), default the eaves overhang"), "party": json!({ "type": "array", "items": { "enum": ["min", "max"] }, "description": "Ends of the ridge that stand on a party wall (the roof runs on over the neighbour): no verge there" }),
                       "rafterSize": json!({ "type": "array", "items": { "type": "number" }, "description": "Rafters [width, depth] (m), default [0.063, 0.175]" }), "rafterSpacing": n("Rafter spacing (m), default 0.4"),
                       "purlinSize": json!({ "type": "array", "items": { "type": "number" }, "description": "Purlins and ridge purlin [width, depth] (m), default [0.1, 0.25]" }), "purlinSpan": n("Largest rafter span between supports along the slope (m), default 2.4"),
                       "plateHeight": n("Wall plate height (m), default 0.075"), "covering": en("Covering", &["tiles", "slates", "metal"]), "battenGauge": n("Batten gauge (m), default 0.32"), "thickness": n("Unused (the build-up is computed)"),
                       "ridgeAlongX": b("Ridge runs along x (default true)"), "min": p2("Footprint min corner [x, z]"), "max": p2("Footprint max corner [x, z]"), "levels": i("Storeys under the roof (1 = sits on the ground floor)") }), &[]),
            op("add_fixture", "Place an electrical / plumbing / heating fixture. `position` is [x, height above the storey floor, z].",
               json!({ "level": i("Storey index"), "kind": en("Fixture kind", &FixtureKind::ALL), "position": p3("[x, yAboveFloor, z]"), "host": host(), "rotation": n("Rotation about the vertical axis (radians)"), "note": s("Free text") }), &["kind", "position"]),
            op("add_run", "Add a cable or pipe run as a polyline; point heights are above the storey floor.",
               json!({ "level": i("Storey index"), "system": en("System", &SYSTEMS), "points": { "type": "array", "items": p3("[x, yAboveFloor, z]"), "minItems": 2 }, "diameter": n("Outer diameter (m), default 0.03") }), &["system", "points"]),
            op("update", "Deep-merge a patch into a construction entity's data (e.g. {\"name\": …}, {\"points\": […]}, {\"wallHeight\": 2.8}, {\"along\": 1.2}, {\"style\": \"hip\"}). Arrays are replaced.",
               json!({ "id": s("Entity id"), "patch": { "type": "object", "description": "Fields to merge into `data`" } }), &["id", "patch"]),
            op("set_finish", "Set a material on one face of a construction entity. Faces: room `floor`, `ceiling`, `wall.<edge>.interior`, `wall.<edge>.exterior`; wall `interior`, `exterior`; roof `roof`, `gable`. Material: a finish key (plaster, brick, brickBrown, brickBuff, cladding, paintWhite, paintWarm, paintGrey, paintDark, paintBlue, paintGreen, concrete, wood, tile) or any material id/name.",
               json!({ "id": s("Entity id"), "face": s("Face key"), "material": s("Finish key or material") }), &["id", "face", "material"]),
            op("add_stair", "Add a staircase climbing from this storey to the next. `origin` is the centre of the bottom riser; `bearing` is compass degrees (0 climbs towards -z, 90 towards +x). The rise is the storey height; `risers` defaults to round(height / 0.18). A quarterTurn stair turns after `turnAfter` risers via a landing, or via `winders` tapered treads. The floor above gets a well cut out unless `wellOpening` is false.",
               json!({ "level": i("Storey the stair starts on"), "name": s("Name"), "origin": p2("Foot of the stair [x, z]"), "bearing": n("Compass degrees"), "width": n("m, default 0.9"), "going": n("Tread depth (m), default 0.25"),
                       "risers": i("Number of risers"), "kind": en("Stair kind", &["straight", "quarterTurn"]), "turnAfter": i("Risers before the turn (default 6)"), "turnsRight": b("Turn to the right (default true)"), "winders": i("Tapered treads in the turn (0 = landing)"),
                       "treadThickness": n("m, default 0.04"), "nosing": n("m, default 0.025"), "openRisers": b("No riser boards"), "stringer": n("Stringer thickness (m), default 0.05; 0 = none"), "wellOpening": b("Cut the floor above (default true)") }), &["origin"]),
            op("add_unit", "Place a fitted unit or piece of furniture. `origin` is the middle of its back (the point against the wall) and it extends forward along `bearing` (compass degrees, 0 = towards -z). With a `type` from the catalogue (see list_catalogue: base-unit, wall-unit, tall-unit, extractor, worktop, shelving, radiator, wardrobe, island, double-vanity, bath, shower, wc, basin, …) it is built by that type's parametric script with `values` (numbers) and `choices` (strings) — width/depth/height are then derived and may be omitted. Without a scripted type it is a plain box of width × depth × height lifted `lift` off the floor.",
               json!({ "level": i("Storey index"), "name": s("Name"), "type": s("Component type id (catalogue) or a free label"), "origin": p2("Middle of the back [x, z]"), "bearing": n("Compass degrees"),
                       "values": { "type": "object", "description": "Numeric parameters of the scripted type, e.g. {\"width\": 0.8, \"height\": 0.72}", "additionalProperties": { "type": "number" } },
                       "choices": { "type": "object", "description": "Choice parameters of the scripted type, e.g. {\"front\": \"drawers\"}", "additionalProperties": { "type": "string" } },
                       "width": n("m (box fallback)"), "depth": n("m (box fallback)"), "height": n("m (box fallback)"), "lift": n("Height of the underside above the floor (m, box fallback)") }), &["origin"]),
            op("list_catalogue", "Every parametric component type available to this document (built-in catalogue + document-defined), each with its group, default parameters and default size. Returned as entities[0].data.types.", json!({}), &[]),
            op("component_params", "Resolve a component type's parameters for given values/choices (after the type's rules), with its meta (services it needs, room it wants in front), worktop and size. Use it to build a parameter form or to check values before add_unit.",
               json!({ "type": s("Component type id"), "values": { "type": "object", "additionalProperties": { "type": "number" } }, "choices": { "type": "object", "additionalProperties": { "type": "string" } } }), &["type"]),
            op("define_component", "Define (or redefine) a parametric component type from JavaScript source. The script must call component('<id>', { name, params: [{ name, label, kind: length|number|integer|choice|flag|derived, value, min, max, choices }], rules(p), build(p) }) and build returns solids made with box({x, y, z, w, h, d, material}) or shape(corners, low, high, material); x runs across (0 = the middle of the back), z forward from the back, y up; materials are Carcass, Front, Worktop, Handle, Appliance, Sanitary, Glass, Radiator, Timber, Frame, Steel or '#RRGGBB'. The script is validated by building it with default parameters. Units of that type re-expand at once.",
               json!({ "id": s("Type id, e.g. my-desk"), "name": s("Display name"), "group": s("Catalogue group"), "source": s("The JavaScript source") }), &["id", "source"]),
            op("remove_component_type", "Remove a document-defined component type (units of it fall back to boxes).", json!({ "id": s("Type id") }), &["id"]),
            op("import_house", "Import a native Uzzetje HouseModel JSON document (floors, rooms, polyRooms, walls, openings, roof/roofVolumes, fixtures, runs, stairs, components → units, shapes → face groups on a Site layer, guides, cavity, finishes) as construction entities, keeping the source ids.",
               json!({ "house": { "type": "object", "description": "The HouseModel document" } }), &["house"]),
            op("summary", "Per-storey rooms with areas, perimeters, wall lengths and opening counts, plus the overall metrics (no change).", json!({}), &[]),
        ]
    }
}

fn is_plan_transform(m: &Mat4) -> bool {
    let up = mat_apply_dir(m, [0.0, 1.0, 0.0]);
    up[0].abs() < 1e-6 && up[2].abs() < 1e-6 && up[1] > 0.0 && m[13].abs() < 1e-6
}
fn plan_apply(m: &Mat4, p: P2) -> P2 { let q = mat_apply(m, [p[0], 0.0, p[1]]); [q[0], q[2]] }
fn plan_angle(m: &Mat4) -> f64 { let x = mat_apply_dir(m, [1.0, 0.0, 0.0]); (-x[2]).atan2(x[0]) }

/// The masonry op behind a construction op name (`masonry.add_wall`, or `masonry_add_wall` as listed for MCP).
fn masonry_op(op_type: &str) -> Option<&str> {
    op_type.strip_prefix("masonry.").or_else(|| op_type.strip_prefix("construction.").and_then(|n| n.strip_prefix("masonry_")))
}

impl Plugin for ConstructionPlugin {
    fn id(&self) -> &str { PLUGIN_ID }
    /// The masonry engine is part of this plugin: `masonry.*` entities and ops are ours.
    fn aliases(&self) -> &[&str] { &["masonry"] }

    fn describe(&self) -> PluginInfo {
        let mut types: Vec<String> = [T_ROOM, T_WALL, T_OPENING, T_ROOF, T_FIXTURE, T_RUN, T_STAIR, T_UNIT].iter().map(|s| s.to_string()).collect();
        types.extend(masonry::data::TYPES.iter().map(|t| t.to_string()));
        let mut ops = Self::ops_info();
        ops.extend(masonry::MasonryPlugin::ops_info().into_iter().map(|mut o| { o.name = format!("masonry_{}", o.name); o }));
        PluginInfo { id: PLUGIN_ID.into(), name: "Construction".into(), entity_types: types, ops }
    }

    fn expand(&self, entity: &Custom, doc: &Document) -> Result<Vec<Entity>, OpError> {
        let m = expand::model(doc);
        let set = expand::collect(&m);
        let mh = masonry::data::House::from_parts(doc, &m, &set);
        if masonry::data::TYPES.contains(&entity.type_.as_str()) {
            let layer = if entity.layer.is_empty() { doc.default_layer().to_string() } else { entity.layer.clone() };
            return Ok(masonry::expand::expand_any(entity, &mh, &layer).faces);
        }
        Ok(expand::expand_entity_with(entity, &m, &set, Some(&mh)))
    }

    fn apply(&self, op_type: &str, data: Value, doc: &mut Document) -> Result<OpResult, OpError> {
        if let Some(name) = masonry_op(op_type) { return masonry::ops::apply(name, data, doc); }
        let name = op_type.strip_prefix("construction.").unwrap_or(op_type);
        ops::apply(name, data, doc)
    }

    fn transform(&self, e: &mut Custom, m: &Mat4, doc: &Document) -> Result<(), OpError> {
        if e.type_.starts_with("masonry.") { return masonry::ops::transform_plan(e, m); }
        if !is_plan_transform(m) { return Err(OpError("construction entities can only be moved, rotated or scaled in plan (about the vertical axis, no vertical offset)".into())); }
        let d = e.data.clone();
        match e.type_.as_str() {
            T_ROOM => {
                let mut r: Room = serde_json::from_value(d).map_err(|x| OpError(x.to_string()))?;
                for p in r.points.iter_mut() { *p = plan_apply(m, *p); }
                e.data = serde_json::to_value(r).unwrap();
            }
            T_WALL => {
                let mut w: Wall = serde_json::from_value(d).map_err(|x| OpError(x.to_string()))?;
                w.a = plan_apply(m, w.a); w.b = plan_apply(m, w.b);
                e.data = serde_json::to_value(w).unwrap();
            }
            T_FIXTURE => {
                let mut f: Fixture = serde_json::from_value(d).map_err(|x| OpError(x.to_string()))?;
                let q = plan_apply(m, [f.position[0], f.position[2]]);
                f.position = [q[0], f.position[1], q[1]];
                let a = plan_angle(m);
                if a.abs() > 1e-9 { f.rotation = Some(f.rotation.unwrap_or(0.0) + a); }
                e.data = serde_json::to_value(f).unwrap();
            }
            T_RUN => {
                let mut r: Run = serde_json::from_value(d).map_err(|x| OpError(x.to_string()))?;
                for p in r.points.iter_mut() { let q = plan_apply(m, [p[0], p[2]]); *p = [q[0], p[1], q[1]]; }
                e.data = serde_json::to_value(r).unwrap();
            }
            T_ROOF => {
                let mut r: Roof = serde_json::from_value(d).map_err(|x| OpError(x.to_string()))?;
                if let (Some(lo), Some(hi)) = (r.min, r.max) {
                    let corners = [plan_apply(m, lo), plan_apply(m, [hi[0], lo[1]]), plan_apply(m, hi), plan_apply(m, [lo[0], hi[1]])];
                    let (a, c) = bbox(&corners).unwrap();
                    r.min = Some(a); r.max = Some(c);
                    e.data = serde_json::to_value(r).unwrap();
                }
            }
            T_STAIR => {
                let mut st: Stair = serde_json::from_value(d).map_err(|x| OpError(x.to_string()))?;
                st.origin = plan_apply(m, st.origin);
                let a = plan_angle(m);
                if a.abs() > 1e-9 { st.bearing = (st.bearing - a.to_degrees()).rem_euclid(360.0); }
                e.data = serde_json::to_value(st).unwrap();
            }
            T_UNIT => {
                let mut u: Unit = serde_json::from_value(d).map_err(|x| OpError(x.to_string()))?;
                u.origin = plan_apply(m, u.origin);
                let a = plan_angle(m);
                if a.abs() > 1e-9 { u.bearing = (u.bearing - a.to_degrees()).rem_euclid(360.0); }
                let sx = plannr_core::geom::len(mat_apply_dir(m, [1.0, 0.0, 0.0]));
                if (sx - 1.0).abs() > 1e-6 { u.width *= sx; u.depth *= sx; }
                e.data = serde_json::to_value(u).unwrap();
            }
            T_OPENING => {
                let mut o: Opening = serde_json::from_value(d).map_err(|x| OpError(x.to_string()))?;
                let (a, bb, _) = ops::host_segment(doc, &o.host).ok_or_else(|| OpError("host wall not found".into()))?;
                let l = dist2(a, bb).max(1e-9);
                let dir = [(bb[0] - a[0]) / l, (bb[1] - a[1]) / l];
                let p = [a[0] + dir[0] * o.along, a[1] + dir[1] * o.along];
                let q = plan_apply(m, p);
                let along = (q[0] - a[0]) * dir[0] + (q[1] - a[1]) * dir[1];
                o.along = along.clamp((o.width / 2.0).min(l / 2.0), (l - o.width / 2.0).max(l / 2.0));
                e.data = serde_json::to_value(o).unwrap();
            }
            other => return Err(OpError(format!("unknown construction type {other}"))),
        }
        Ok(())
    }

    fn paint(&self, e: &mut Custom, face_key: &str, material: &str, side: Side) -> Result<(), OpError> {
        // a face of a masonry wall derived from this entity: `wall.2#plaster.int`, `wall.2~1#outer`, `wall#single`…
        let mapped;
        let key = match face_key.trim().split_once('#') {
            Some((base, layer)) if base.starts_with("wall") => {
                let base = base.split('~').next().unwrap_or(base);
                let side_key = if layer.starts_with("plaster.int") || layer.starts_with("inner") { "interior" } else { "exterior" };
                mapped = format!("{base}.{side_key}");
                mapped.as_str()
            }
            _ => face_key.trim(),
        };
        let valid = match e.type_.as_str() {
            T_ROOM => key == "floor" || key == "ceiling" || (key.starts_with("wall.") && (key.ends_with(".interior") || key.ends_with(".exterior"))),
            T_WALL => key == "wall.interior" || key == "wall.exterior" || key == "interior" || key == "exterior",
            T_ROOF => key == "roof" || key == "gable" || key == "fascia",
            _ => false,
        };
        if !valid { return Err(OpError(format!("cannot paint face `{key}` of a {}", e.type_))); }
        let key = key.strip_prefix("wall.").filter(|_| e.type_ == T_WALL).unwrap_or(key).to_string();
        match side {
            Side::Both if key.ends_with(".interior") || key.ends_with(".exterior") => {
                let base = key.rsplit_once('.').map(|(a, _)| a.to_string()).unwrap_or_default();
                ops::set_finish(e, &format!("{base}.interior"), material);
                ops::set_finish(e, &format!("{base}.exterior"), material);
            }
            _ => ops::set_finish(e, &key, material),
        }
        Ok(())
    }

    fn metrics(&self, doc: &Document) -> Option<Value> {
        if !doc.entities.iter().any(|e| matches!(e, Entity::Custom(c) if c.type_.starts_with("construction.") || c.type_.starts_with("masonry."))) { return None; }
        let mut v = ops::metrics(doc);
        v["masonry"] = masonry::takeoff::metrics(doc);
        Some(v)
    }

    fn on_load(&self, doc: &mut Document) {
        let fresh = !doc.extensions.contains_key(PLUGIN_ID) && !doc.extensions.contains_key(masonry::data::PLUGIN_ID);
        masonry::data::migrate(doc);
        ops::ensure(doc);
        // a new model builds cavity walls (the masonry build-up) unless told otherwise
        if fresh { if let Some(c) = doc.extensions.get_mut(PLUGIN_ID) { c["cavity"]["enabled"] = json!(true); } }
        masonry::data::ensure(doc);
        if masonry::data::has_masonry(doc) { masonry::data::masonry_layer(doc); }
    }
}
