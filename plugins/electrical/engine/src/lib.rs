//! Electrical plugin for Plannr: the distribution board, circuits, conduits, AREI checks and the
//! Belgian schematics (situatieschema, eendraadschema) on top of the construction plugin's
//! fixtures. See docs/ELECTRICAL.md.

pub mod arei;
pub mod data;
pub mod ops;
pub mod route;
pub mod svg;

use plannr_core::geom::{mat_apply, Mat4};
use plannr_core::model::{Custom, Document, Entity};
use plannr_core::ops::{OpError, OpResult};
use plannr_core::plugin::{Plugin, PluginInfo, PluginOp};
use serde_json::{json, Value};

use data::*;

#[derive(Default)]
pub struct ElectricalPlugin;

fn n(d: &str) -> Value { json!({ "type": "number", "description": d }) }
fn i(d: &str) -> Value { json!({ "type": "integer", "minimum": 0, "description": d }) }
fn s(d: &str) -> Value { json!({ "type": "string", "description": d }) }
fn b(d: &str) -> Value { json!({ "type": "boolean", "description": d }) }
fn en(d: &str, v: &[&str]) -> Value { json!({ "type": "string", "enum": v, "description": d }) }
fn ids(d: &str) -> Value { json!({ "type": "array", "items": { "type": "string" }, "description": d }) }
fn op(name: &str, description: &str, props: Value, required: &[&str]) -> PluginOp {
    PluginOp { name: name.into(), description: description.into(), input_schema: json!({ "type": "object", "properties": props, "required": required }) }
}
const CONV: &str = "Belgian AREI/RGIE conventions. ";

fn circuit_props() -> Value {
    json!({ "name": s("Circuit name, e.g. 'Kitchen sockets'"), "type": en("Circuit type; sets defaults (lighting 16 A/1.5 mm², sockets/kitchen/bathroom/outdoor 20 A/2.5, hob/evCharger 32 A/6 4P, dishwasher/washingMachine/dryer/boiler 16 A/2.5, pv 20 A/4 C)", &CIRCUIT_TYPES),
            "breaker": i("Breaker rating in A"), "cableSection": n("Cable section in mm²"), "rcd": en("Differential protecting this circuit (30 mA mandatory for sockets, wet rooms and appliances)", &RCD_KINDS),
            "poles": i("2 or 4"), "curve": s("Breaker curve B or C"), "cableType": s("XVB, VOB, …; default from the board"), "points": ids("Fixture ids (construction outlets / switches / lights / data points) on this circuit"), "note": s("Free text"), "number": i("Position on the board (default: next free)") })
}

impl ElectricalPlugin {
    fn ops_info() -> Vec<PluginOp> {
        let mut c = circuit_props();
        c["id"] = s("Circuit id");
        vec![
            op("set_board", &format!("{CONV}Set the distribution board: phases (1 = 230 V, 3 = 400 V), main breaker (A), head differential (mA, 300 or 100; 0 = none), differential type (A/B), earthing (TT), short-circuit rating (kA), default cable type, address / EAN / meter number, and `panel` = the fixture id of the construction `panel` the circuits leave from."),
               json!({ "phases": i("1 or 3"), "mainBreaker": i("A"), "mainDifferential": i("mA at the head: 300 (default), 100 or 0"), "differentialType": s("A or B"), "earthing": s("TT (Belgian homes)"), "shortCircuitKA": n("kA"), "defaultCableType": s("XVB or VOB"), "address": s(""), "ean": s(""), "meterNumber": s(""), "panel": s("Fixture id of the board") }), &[]),
            op("add_circuit", &format!("{CONV}Add a circuit (kring) leaving the board. Omitted breaker / cable / rcd take the type's defaults. Points listed here are taken away from any other circuit."), circuit_props(), &["type"]),
            op("update_circuit", "Change a circuit's fields; `points` replaces the list (and removes those points from other circuits).", c, &["id"]),
            op("remove_circuit", "Remove a circuit; its conduits keep their geometry but lose the circuit.", json!({ "id": s("Circuit id") }), &["id"]),
            op("renumber", "Renumber all circuits 1… in board order.", json!({}), &[]),
            op("assign", "Put electrical points (fixture ids of outlets, switches, lights, data points) on a circuit, or on none with circuit = null. A point belongs to at most one circuit.",
               json!({ "points": ids("Fixture ids"), "circuit": s("Circuit id, or omit/null to unassign") }), &["points"]),
            op("add_rcd_group", "Group circuits behind one differential (differentieelschakelaar).", json!({ "name": s(""), "milliamps": i("30 or 300"), "type": s("A or B"), "circuits": ids("Circuit ids") }), &[]),
            op("update_rcd_group", "Change an RCD group.", json!({ "id": s("Group id"), "name": s(""), "milliamps": i(""), "type": s(""), "circuits": ids("Circuit ids") }), &["id"]),
            op("remove_rcd_group", "Remove an RCD group.", json!({ "id": s("Group id") }), &["id"]),
            op("add_conduit", "Draw a conduit (buis) or surface cable as a polyline; point heights are above the storey floor. `from`/`to` may name fixtures (level is taken from them).",
               json!({ "level": i("Storey index"), "points": { "type": "array", "items": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 }, "minItems": 2, "description": "[x, yAboveFloor, z] …" }, "circuit": s("Circuit id"), "from": s("Fixture id (board)"), "to": s("Fixture id (point)"), "diameter": en("Outer diameter in m", &["0.016", "0.02", "0.025"]), "kind": en("conduit (in the wall/ceiling) or cable (surface)", &["conduit", "cable"]), "cableType": s("") }), &["points"]),
            op("update_conduit", "Change a conduit: deep-merge `patch` into its data (circuit, kind, diameter, points, from, to; arrays replaced; circuit: null clears it).",
               json!({ "id": s("Conduit id"), "patch": { "type": "object", "description": "Fields to merge", "properties": { "circuit": s("Circuit id or null"), "kind": en("", &["conduit", "cable"]), "diameter": en("m", &["0.016", "0.02", "0.025"]), "points": { "type": "array", "items": { "type": "array", "items": { "type": "number" } } }, "from": s(""), "to": s("") } } }), &["id", "patch"]),
            op("auto_route", "Create conduits from the board to every point of a circuit (default: all circuits), replacing earlier auto-routed ones. `ceiling` (default): inside the walls, as an installer chases them — up inside the board's wall to the chase zone 15 cm under the ceiling, along the walls (middle of the room-side leaf), down inside the wall of the point; ceiling lights and points away from walls are fed through the ceiling. `floor`: down inside the wall into the floor screed, straight across, up inside the point's wall. `straight`: a direct line. Other storeys are reached by a riser inside a wall that exists on both storeys, nearest the board.",
               json!({ "circuit": s("Circuit id (default: all)"), "mode": en("Routing mode", &["ceiling", "floor", "straight"]), "height": n("Override the routing height above the floor (m)") }), &[]),
            op("check", &format!("{CONV}Run the AREI checks (head differential, cable vs breaker, 30 mA where required, dedicated circuits, points per circuit, unassigned points, demand vs capacity, RCD groups, conduits). Advisory — a certified keuring is still required. Findings in entities[0].data.findings."), json!({}), &[]),
            op("summary", "Board, circuits (rating, cable, points, load), assigned / unassigned points, demand vs capacity and conduit length. No change.", json!({}), &[]),
            op("situation_svg", "The situatieschema of one storey as SVG text (entities[0].data.svg): rooms, AREI/IEC symbols coloured and numbered per circuit, conduits, legend and title block.",
               json!({ "level": i("Storey index"), "scale": n("Pixels per metre (default 40)"), "showConduits": b("Draw conduits (default true)"), "title": s("Project title for the title block") }), &["level"]),
            op("oneline_svg", "The eendraadschema (one-line diagram) as SVG text: meter, main breaker, head differential, busbar and every circuit with its breaker, cable, differential and points.", json!({ "title": s("Project title") }), &[]),
            op("import_electrical", "Import Studio's `electrical` block (phases, mainBreaker, hasMainDifferential, circuits with fixtureIDs, …). Fixture ids unknown here are matched by kind/storey/position when `fixtures` is given, otherwise dropped and reported in entities[0].data.dropped.",
               json!({ "electrical": { "type": "object", "description": "The Studio electrical JSON" }, "fixtures": { "type": "array", "items": { "type": "object" }, "description": "Studio's house.fixtures: lets ids that do not exist here be matched by kind, storey and position (within 2 cm), e.g. for a house rebuilt through add_fixture" } }), &["electrical"]),
        ]
    }
}

impl Plugin for ElectricalPlugin {
    fn id(&self) -> &str { PLUGIN_ID }
    fn describe(&self) -> PluginInfo { PluginInfo { id: PLUGIN_ID.into(), name: "Electrical".into(), entity_types: vec![T_CONDUIT.into()], ops: Self::ops_info() } }
    fn expand(&self, entity: &Custom, doc: &Document) -> Result<Vec<Entity>, OpError> {
        Ok(if entity.type_ == T_CONDUIT { ops::expand_conduit(entity, doc) } else { vec![] })
    }
    fn apply(&self, op_type: &str, data: Value, doc: &mut Document) -> Result<OpResult, OpError> {
        ops::apply(op_type.strip_prefix("electrical.").unwrap_or(op_type), data, doc)
    }
    fn transform(&self, e: &mut Custom, m: &Mat4, _doc: &Document) -> Result<(), OpError> {
        if e.type_ != T_CONDUIT { return Err(OpError("not an electrical entity".into())); }
        let mut k: Conduit = serde_json::from_value(e.data.clone()).map_err(|x| OpError(x.to_string()))?;
        for p in k.points.iter_mut() { *p = mat_apply(m, *p); }
        k.auto = false;
        e.data = serde_json::to_value(k).unwrap();
        Ok(())
    }
    fn metrics(&self, doc: &Document) -> Option<Value> {
        let has = doc.extensions.get(PLUGIN_ID).and_then(|v| v.get("circuits")).and_then(|c| c.as_array()).map(|a| !a.is_empty()).unwrap_or(false)
            || doc.entities.iter().any(|e| matches!(e, Entity::Custom(c) if c.type_ == T_CONDUIT));
        if !has { return None; }
        Some(ops::metrics(doc))
    }
    fn on_load(&self, doc: &mut Document) { ensure(doc); }
}
