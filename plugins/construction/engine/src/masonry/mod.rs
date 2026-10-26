//! Masonry plugin for Plannr: Belgian brick and block construction on top of the construction
//! plugin — cavity walls (inner leaf, cavity with insulation and ties, outer facing-brick leaf),
//! single-leaf partitions, lintels, sills, damp-proof courses, beam-and-block floors ("potten en
//! balken"), hollow-core and in-situ slabs, ring beams, strip foundations, a material takeoff,
//! checks, U-values and drawings. See docs/MASONRY.md.

pub mod check;
pub mod data;
pub mod derive;
pub mod drawing;
pub mod expand;
pub mod floor;
pub mod import;
pub mod ops;
pub mod takeoff;
pub mod uvalue;
pub mod wall;

use plannr_core::geom::Mat4;
use plannr_core::model::{Custom, Document, Entity};
use plannr_core::ops::{OpError, OpResult};
use plannr_core::plugin::{Plugin, PluginInfo, PluginOp};
use serde_json::{json, Value};

use data::*;

#[derive(Default)]
pub struct MasonryPlugin;

fn n(d: &str) -> Value { json!({ "type": "number", "description": d }) }
fn i(d: &str) -> Value { json!({ "type": "integer", "minimum": 0, "description": d }) }
fn s(d: &str) -> Value { json!({ "type": "string", "description": d }) }
fn b(d: &str) -> Value { json!({ "type": "boolean", "description": d }) }
fn en(d: &str, v: &[&str]) -> Value { json!({ "type": "string", "enum": v, "description": d }) }
fn p2v(d: &str) -> Value { json!({ "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2, "description": d }) }
fn pts(d: &str) -> Value { json!({ "type": "array", "items": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2 }, "description": d }) }
fn op(name: &str, description: &str, props: Value, required: &[&str]) -> PluginOp {
    PluginOp { name: name.into(), description: description.into(), input_schema: json!({ "type": "object", "properties": props, "required": required }) }
}
const KINDS: [&str; 6] = ["facingBrick", "brick", "clayBlock", "concreteBlock", "aeratedBlock", "limeSandBlock"];
const BONDS: [&str; 5] = ["stretcher", "third", "stack", "english", "flemish"];

fn unit_schema(d: &str) -> Value {
    json!({ "type": "object", "description": d, "properties": { "name": s("e.g. Gevelsteen 190×90×65"), "kind": en("", &KINDS), "length": n("m"), "height": n("m"), "width": n("m (leaf thickness)"), "joint": n("m (10–12 mm)"), "bond": en("", &BONDS), "color": s("#RRGGBB") } })
}
fn cavity_schema() -> Value {
    json!({ "type": "object", "properties": { "width": n("Cavity width (m)"), "insulation": { "type": "object", "properties": { "kind": en("none, fill (blown/injected, fills the cavity) or board (against the inner leaf)", &["none", "fill", "board"]), "material": en("", &["eps", "pur", "mineralWool", "pir", "xps"]), "thickness": n("board thickness (m)") } }, "ties": { "type": "object", "properties": { "perM2": n("Cavity ties per m² (5)") } } } })
}
fn opening_schema() -> Value {
    json!({ "type": "object", "properties": { "kind": en("", &["window", "door"]), "offset": n("From the wall start a to the opening (m)"), "width": n("m"), "height": n("m"), "sill": n("Window sill height above the floor (m)"), "lintel": { "type": "object", "properties": { "inner": en("", &["concrete", "clay", "none"]), "outer": en("", &["steelAngle", "concrete", "none"]), "bearing": n("m each side (0.15)") } }, "sillStone": b("Bluestone sill under a window (default on)") }, "required": ["offset", "width", "height"] })
}
fn wall_props() -> Value {
    json!({ "level": i("Storey index"), "a": p2v("Plan start [x,z]"), "b": p2v("Plan end [x,z]"), "reference": en("What a→b is: the exterior face (default), the centre line or the interior face of the wall body; exterior = left of a→b (draw clockwise)", &["exterior", "center", "interior"]),
            "type": en("cavity (outer leaf + cavity + inner leaf) or single leaf", &["cavity", "single"]), "height": n("Inner / single leaf height (default: storey height minus the structure above it — the floor of the storey above or attic joists)"), "outerHeight": n("Outer leaf height (default: full storey height)"),
            "outer": unit_schema("Outer leaf unit (default: facing brick 190×90×65)"), "inner": unit_schema("Inner / single leaf unit (default: snelbouw 288×88×138)"), "cavity": cavity_schema(),
            "plaster": { "type": "object", "properties": { "interior": b(""), "exterior": b(""), "thickness": n("m") } }, "dpc": b("Damp-proof course at the base (level 0)"), "openings": { "type": "array", "items": opening_schema() },
            "startJoin": en("", &["auto", "none"]), "endJoin": en("", &["auto", "none"]), "flip": b("Exterior on the right of a→b") })
}
fn floor_props() -> Value {
    json!({ "level": i("The storey this is the floor of (≥ 1); its top is that storey's base"), "points": pts("Outline incl. bearings [[x,z]…]"), "origin": p2v(""), "size": p2v(""),
            "system": en("beamBlock = potten en balken (prestressed joists + hollow blocks + topping), hollowCore = welfsels, insitu = cast slab", &["beamBlock", "hollowCore", "insitu"]), "direction": en("Span direction", &["auto", "x", "z"]),
            "joistWidth": n("m (0.12)"), "joistHeight": n("m (0.12)"), "spacing": n("Joist spacing on centre (0.6 m)"), "blockKind": en("", &["concrete", "clay", "eps"]), "blockHeight": n("m (0.16)"), "blockLength": n("m (0.25)"),
            "topping": n("Concrete topping (druklaag) m (0.04)"), "mesh": b("Reinforcement mesh in the topping"), "bearing": n("Bearing on the walls (0.10 m)"), "ringBeam": b("Concrete ring beam along the edges"), "ringBeamWidth": n("m"),
            "plankThickness": n("hollowCore plank depth (0.2)"), "plankWidth": n("hollowCore plank width (1.2)"), "slabThickness": n("insitu slab (0.18)"), "ceilingPlaster": b("Plaster the ceiling under the floor (default off: the structure shows)"), "openings": { "type": "array", "items": pts("Stair-well / opening polygon"), "description": "Holes (trimmed)" } })
}

impl MasonryPlugin {
    pub fn ops_info() -> Vec<PluginOp> {
        let conv = "Belgian masonry (metres, y up). ";
        vec![
            op("set_defaults", &format!("{conv}Defaults for new elements: outer / inner / partition units, cavity (width, insulation, ties), plaster, dpc, lintel, sill, floor system, foundation."),
               json!({ "outer": unit_schema(""), "inner": unit_schema(""), "partition": unit_schema(""), "cavity": cavity_schema(), "plaster": { "type": "object" }, "dpc": b(""), "lintel": { "type": "object" }, "sill": { "type": "object" }, "floor": { "type": "object" }, "foundation": { "type": "object" } }), &[]),
            op("add_wall", &format!("{conv}A masonry wall between two plan points: a cavity wall (outer facing-brick leaf, cavity with optional insulation and ties, inner block leaf, interior plaster) or a single leaf; corners mitre and T-junctions trim automatically; openings get lintels and bluestone sills; the bond pattern is drawn on the brick faces."), wall_props(), &["a", "b"]),
            op("add_opening", "Add a window or door opening to a masonry wall (cut through every leaf, with lintels and a sill).", { let mut o = opening_schema(); o["properties"]["wall"] = s("Masonry wall id"); o["required"] = json!(["wall", "offset", "width", "height"]); o }, &["wall", "offset", "width", "height"]),
            op("remove_opening", "Remove an opening from a masonry wall.", json!({ "wall": s(""), "opening": s("") }), &["wall", "opening"]),
            op("add_floor", &format!("{conv}A structural floor of storey `level` (≥ 1): beam-and-block (potten en balken: prestressed joists at 0.6 m with hollow infill blocks and a concrete topping, ring beam), hollow-core planks or an in-situ slab; stair wells as openings with trimmers."), floor_props(), &[]),
            op("add_foundation", &format!("{conv}Strip foundation under a polygon of wall lines: concrete footing at frost depth plus foundation blocks up to the ground floor."), json!({ "points": pts(""), "origin": p2v(""), "size": p2v(""), "footingWidth": n("m (0.6)"), "footingDepth": n("m (0.3)"), "bottom": n("Underside relative to the ground floor (−0.8)"), "blockWidth": n("m (0.19)"), "top": n("Top elevation (default ground floor)") }), &[]),
            op("update", "Deep-merge `patch` into a masonry element's data (arrays replaced), validated.", json!({ "id": s(""), "patch": { "type": "object" } }), &["id", "patch"]),
            op("import_construction", "Detail the construction model in masonry: one masonry wall per construction wall piece (exterior → cavity wall with the construction cavity widths, others → single leaf with the construction wall thickness, its openings converted), a beam-and-block floor per upper storey (stair wells as openings) and optionally a strip foundation. The construction plugin then no longer draws those walls and floor slabs.",
               json!({ "levels": { "type": "array", "items": { "type": "integer" } }, "walls": b("default true"), "floors": b("default true"), "foundation": b("default false"), "replace": b("Remove earlier imported elements first (default true)") }), &[]),
            op("estimate", "Material takeoff: bricks and blocks (count, m²), mortar, insulation, ties, lintels, sills, plaster, dpc, joists, floor blocks, topping, mesh, ring beam, planks, concrete, footing, foundation blocks; totals and per element. No change.", json!({ "ids": { "type": "array", "items": { "type": "string" } } }), &[]),
            op("check", "Masonry checks (no change): lintel bearing, openings near wall ends, cavity width, tie density, floor spans per system, joist spacing, topping, and the U-value of every exterior wall type against the EPB renovation requirement (0.24 W/m²K).", json!({}), &[]),
            op("uvalue", "U-value of a wall build-up (layers, R, U, the EPB requirement and the U after a full cavity fill). Give `wall` (id) or none for every wall type.", json!({ "wall": s("Masonry wall id") }), &[]),
            op("wall_svg", "Elevation of a masonry wall from the exterior with the bond pattern, openings, lintels, sills and dimensions (SVG in entities[0].data.svg).", json!({ "wall": s("Masonry wall id") }), &["wall"]),
            op("plan_svg", "Plan of a storey: wall layers in section and the floor above with its joists (SVG in entities[0].data.svg).", json!({ "level": i("Storey index") }), &["level"]),
            op("summary", "Defaults, walls, floors, foundations and the takeoff totals. No change.", json!({}), &[]),
            op("storeys", "Per storey: floor-to-floor height, the structure in its top (the floor of the storey above, attic joists) and the clear height left. No change.", json!({}), &[]),
            op("set_clear_height", "Set a storey's clear height (finished floor to the underside of the structure above): its floor-to-floor height becomes clear + structure depth, so adding a floor never eats into the room below.", json!({ "level": i("Storey index"), "clear": n("Clear height (m)") }), &["level", "clear"]),
        ]
    }
}

impl Plugin for MasonryPlugin {
    fn id(&self) -> &str { PLUGIN_ID }
    fn describe(&self) -> PluginInfo { PluginInfo { id: PLUGIN_ID.into(), name: "Masonry".into(), entity_types: TYPES.iter().map(|t| t.to_string()).collect(), ops: Self::ops_info() } }
    fn expand(&self, entity: &Custom, doc: &Document) -> Result<Vec<Entity>, OpError> {
        if !TYPES.contains(&entity.type_.as_str()) { return Ok(vec![]); }
        let house = House::read(doc);
        let layer = if entity.layer.is_empty() { doc.default_layer().to_string() } else { entity.layer.clone() };
        Ok(expand::expand_any(entity, &house, &layer).faces)
    }
    fn apply(&self, op_type: &str, data: Value, doc: &mut Document) -> Result<OpResult, OpError> { ops::apply(op_type.strip_prefix("masonry.").unwrap_or(op_type), data, doc) }
    fn transform(&self, e: &mut Custom, m: &Mat4, _doc: &Document) -> Result<(), OpError> { ops::transform_plan(e, m) }
    fn metrics(&self, doc: &Document) -> Option<Value> { if has_masonry(doc) { Some(takeoff::metrics(doc)) } else { None } }
    fn on_load(&self, doc: &mut Document) { if has_masonry(doc) || doc.extensions.contains_key(PLUGIN_ID) { ensure(doc); if has_masonry(doc) { masonry_layer(doc); } } }
}
