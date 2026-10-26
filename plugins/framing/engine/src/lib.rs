//! Framing plugin for Plannr: Medeek-style parametric platform framing — stud walls with openings
//! (kings, jacks, headers, sills, cripples, sheathing), floor joists, roof trusses and rafters,
//! slab and stem-wall foundations, beams and posts, stairs, connectors, engineering checks, framing
//! drawings and a material takeoff. See docs/FRAMING.md.

pub mod beam;
pub mod data;
pub mod drawing;
pub mod engineering;
pub mod expand;
pub mod hardware;
pub mod junction;
pub mod ops;
pub mod roof;
pub mod stair;

use plannr_core::geom::Mat4;
use plannr_core::model::{Custom, Document, Entity};
use plannr_core::ops::{OpError, OpResult};
use plannr_core::plugin::{Plugin, PluginInfo, PluginOp};
use serde_json::{json, Value};

use data::*;

#[derive(Default)]
pub struct FramingPlugin;

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
const SIZES: [&str; 9] = ["2x3", "2x4", "2x6", "2x8", "2x10", "2x12", "4x4", "4x6", "6x6"];
const LUMBER: &str = "Nominal lumber size (2x4 = 38×89 mm, 2x6 = 38×140, 2x8 = 38×184, 2x10 = 38×235, 2x12 = 38×286) or metric like 38x140";

fn sheathing_schema() -> Value {
    let sheet = |d: &str| json!({ "type": "object", "description": d, "properties": { "kind": s("osb / plywood / gypsum"), "thickness": n("m"), "enabled": b("") } });
    json!({ "type": "object", "properties": { "exterior": sheet("Exterior sheathing (OSB 11 mm by default)"), "interior": sheet("Interior board (gypsum 12.5 mm, off by default)") } })
}
fn wall_props() -> Value {
    json!({ "level": i("Storey index (0 = ground floor)"), "a": p2v("Plan start [x, z] in m"), "b": p2v("Plan end [x, z]"), "height": n("Wall height in m (default: storey height − floor thickness, else 2.44)"),
            "studSize": en(LUMBER, &SIZES), "studSpacing": n("Stud spacing on centre in m: 0.4 (16\"), 0.6 (24\"), 0.3 (12\")"), "plateSize": s("Plate size (default = stud size)"), "topPlates": i("1 or 2"), "bottomPlates": i("0 or 1"),
            "sheathing": sheathing_schema(), "flip": b("Exterior side is the right of a→b instead of the left"), "startStud": b(""), "endStud": b(""),
            "reference": en("What a→b is: the exterior face of the studs (default for add_wall; draw clockwise so the exterior, on the left, is outside), the centre line, or the interior face", &["exterior", "center", "interior"]),
            "cornerDetail": en("L-corner detail where this wall runs through", &["california", "threeStud", "twoStud", "none"]),
            "tDetail": en("Backer where another wall butts into this one", &["twoStud", "ladder", "california", "none"]),
            "startJoin": en("Junction at a: found automatically, or forced", &["auto", "through", "butt", "none"]), "endJoin": en("Junction at b", &["auto", "through", "butt", "none"]),
            "openings": { "type": "array", "items": opening_schema(), "description": "Rough openings" } })
}
fn opening_schema() -> Value {
    json!({ "type": "object", "properties": { "kind": en("", &["window", "door"]), "offset": n("From the wall start to the rough-opening start (m)"), "width": n("Rough-opening width (m)"), "height": n("Rough-opening height (m)"), "sill": n("Sill height above the floor for windows (m); doors 0"), "headerSize": en("Header size (default by span: ≤1.2 m 2x6, ≤1.8 2x8, ≤2.4 2x10, else 2x12)", &SIZES), "kingStuds": i("default 1"), "jackStuds": i("default 1") }, "required": ["offset", "width", "height"] })
}

fn roof_props() -> Value {
    let mut v = json!({ "level": i("Storey the roof bears on"), "min": p2v("Plan min corner [x,z]"), "max": p2v("Plan max corner [x,z]"), "origin": p2v("alternative to min"), "size": p2v("alternative to max"),
        "shape": en("Roof shape", &["gable", "hip", "shed", "gambrel"]), "type": en("Truss type or rafters", &["fink", "howe", "kingPost", "queenPost", "scissor", "attic", "mono", "rafters"]),
        "pitch": n("Rise per 12 (6 = 6:12)"), "pitchDeg": n("Pitch in degrees (overrides pitch)"), "ridgeAlongX": b("Ridge along x (default) or z (hip roofs always run the ridge along the longer side)"),
        "overhang": n("Eave overhang (m)"), "heel": n("Trusses: raised-heel height of the top chord underside above the plate at the wall face (m); 0 = standard heel"), "spacing": n("Truss / rafter spacing (m)") });
    let more = json!({ "chordSize": en(LUMBER, &SIZES), "webSize": en(LUMBER, &SIZES), "rafterSize": en(LUMBER, &SIZES), "ridgeBoard": en(LUMBER, &SIZES), "hipRafterSize": en("Hip rafters (default one size deeper than the rafters)", &SIZES),
        "gableEnds": b(""), "highSide": en("Shed: the high side across the span", &["max", "min"]), "highSupport": en("Shed rafters: the high end sits on a pony wall on the high wall's top plate (default) or hangs on a ledger against a taller wall", &["ponyWall", "ledger"]), "lowerPitch": n("Gambrel: rise per 12 of the lower slopes (default 18)"), "breakRatio": n("Gambrel: knee position as a fraction of the span from each eave (default 0.25)"),
        "bottomPitch": n("Scissor: rise per 12 of the bottom chords (default half the pitch)"), "atticWidth": n("Attic: clear room width between the knee walls (default half the span)"), "atticHeight": n("Attic: ceiling height above the floor joist (default 2.3)"), "floorJoistSize": en("Attic: floor-joist bottom chord", &SIZES) });
    let finish = json!({ "sheathing": { "type": "object", "properties": { "enabled": b(""), "thickness": n("m") } },
        "fascia": { "type": "object", "properties": { "enabled": b("default true"), "size": en("", &SIZES) } },
        "soffit": { "type": "object", "properties": { "enabled": b("default false"), "thickness": n("m") } },
        "cladding": { "type": "object", "properties": { "kind": en("", &["none", "shingles", "metal", "tiles"]), "thickness": n("m (default shingles 6 mm, metal 3 mm, tiles 30 mm)") } },
        "gutters": b("Box gutters along the eaves") });
    let dormer = json!({ "type": "object", "properties": {
        "side": en("Slope (front = l, back = r)", &["l", "r", "front", "back"]), "offset": n("Along the ridge from the min end to the dormer's left edge (m)"), "width": n("m (>= 0.5)"),
        "wallHeight": n("Front wall height above the roof framing (default 1.0)"), "pitch": n("Dormer roof rise per 12 (default 8; shed dormers flatter than the main roof)"), "kind": en("", &["gable", "shed"]), "setback": n("Front wall inward from the exterior wall line (m, default 0)") }, "required": ["offset", "width"] });
    for part in [more, finish] { if let (Value::Object(a), Value::Object(b)) = (&mut v, part) { a.extend(b); } }
    v["dormers"] = json!({ "type": "array", "description": "Dormers (gable roofs only)", "items": dormer });
    v
}

impl FramingPlugin {
    fn ops_info() -> Vec<PluginOp> {
        let conv = "Medeek-style platform framing, metres, y up. ";
        vec![
            op("set_defaults", &format!("{conv}Set the framing defaults used by new walls (studSize, studSpacing, plateSize, topPlates, bottomPlates, sheathing, headerRule, units) and by the engineering check and hardware: species, loads, deflection limits, soilBearing, hardware switches."),
               json!({ "units": en("", &["metric", "imperial"]), "studSize": en(LUMBER, &SIZES), "studSpacing": n("m on centre"), "plateSize": s(""), "topPlates": i(""), "bottomPlates": i(""), "sheathing": sheathing_schema(), "headerRule": s("span"), "cornerDetail": en("Default L-corner detail for new walls", &["california", "threeStud", "twoStud", "none"]), "tDetail": en("Default T backer for new walls", &["twoStud", "ladder", "california", "none"]),
                       "species": en("Strength class / grade for checks", &["C18", "C24", "C30", "SPF2", "DFL2"]),
                       "loads": { "type": "object", "properties": { "floorLive": n("kN/m² (2.0)"), "floorDead": n("kN/m² (0.5)"), "roofSnow": n("kN/m² on plan (0.8)"), "roofDead": n("kN/m² along the slope (0.6)") } },
                       "deflection": { "type": "object", "properties": { "floor": n("L / this under imposed load (360)"), "roof": n("L / this under total load (240)") } },
                       "soilBearing": n("Allowable soil pressure, kPa (100)"),
                       "hardware": { "type": "object", "properties": { "anchorBolts": b("Ground-floor bottom plates: ≤ 0.3 m from plate ends, ≤ 1.8 m apart, none in doors (default on)"), "holdDowns": b("At both ends of every wall (off)"), "hurricaneTies": b("At both bearings of each truss / ceiling joist (on)"), "joistHangers": b("Where joists end against a beam (on)") } } }), &[]),
            op("add_wall", &format!("{conv}Frame a stud wall between two plan points: plates, studs at the spacing (first at a, last at b), and per opening king + jack studs, a built-up header sized by span, a sill and cripples for windows; exterior sheathing with the openings cut out; optional interior gypsum. The line a→b is the exterior face of the studs by default (reference), exterior = left of a→b (flip to swap) — draw clockwise. Corners (L) and T-intersections with other walls are joined automatically: trimmed/extended, California / 3-stud corners, 2-stud / ladder backers, interlocking top plates, lapped sheathing."), wall_props(), &["a", "b"]),
            op("add_opening", "Add a rough opening (door or window) to a framing wall: offset from the wall start, width, height, sill (windows), optional headerSize / kingStuds / jackStuds.",
               { let mut o = opening_schema(); o["properties"]["wall"] = s("Framing wall id"); o["required"] = json!(["wall", "offset", "width", "height"]); o }, &["wall", "offset", "width", "height"]),
            op("remove_opening", "Remove an opening from a framing wall.", json!({ "wall": s("Framing wall id"), "opening": s("Opening id") }), &["wall", "opening"]),
            op("add_floor", &format!("{conv}Frame a floor: joists at the spacing spanning the shorter way (or `direction`), rim joists around the polygon, optional mid-span blocking, and a subfloor panel on top. Give `points` (plan polygon) or `origin` + `size`."),
               json!({ "level": i("Storey index"), "points": pts("Plan polygon [[x,z]…]"), "origin": p2v("Rectangle origin [x,z]"), "size": p2v("Rectangle size [w,d]"), "joistSize": en(LUMBER, &SIZES), "joistSpacing": n("m on centre (0.4 default)"), "direction": en("Joist span direction", &["auto", "x", "z"]), "rimJoist": b(""), "blocking": b("one row at mid-span"), "subfloor": { "type": "object", "properties": { "thickness": n("m"), "enabled": b("") } }, "elevation": n("Offset of the joist underside above the storey base (m)") }), &[]),
            op("add_roof", &format!("{conv}Frame a roof over a plan rectangle bearing on storey `level`'s walls. shape: gable (default), hip (ridge = run − span along the longer side, 4 hip rafters, jack rafters; with trusses doubled hip girders + mono jack trusses), shed (mono slope, `highSide` max|min; trusses become mono trusses, rafters are birdsmouthed on the low wall and sit on a pony wall at the high wall, or on a ledger with highSupport: ledger), gambrel (rafters, steep `lowerPitch` up to the knee at `breakRatio`, purlins on knee walls, collar ties). type: fink (W), howe, kingPost, queenPost, scissor (bottom chords at `bottomPitch`), attic (floor-joist bottom chord, knee walls `atticWidth` apart, ceiling collar at `atticHeight`), mono (shed only) or rafters (ridge board + ceiling joists). The roof bears on the top plates of the framed walls under its eaves. Pitch = rise per 12 (or pitchDeg); heel 0 = standard truss heel (top chord on the bottom chord), larger = raised heel with heel posts; rafters get a birdsmouth seat. Overhang tails, gable studs. Finishing: fascia (default on), soffit, sheathing, cladding (shingles/metal/tiles), gutters. Dormers (gable roofs): gable or shed dormers on a slope — the rafters/trusses in the opening are cut and headed off with doubled trimmers and headers; the dormer gets a front wall with a framed window opening, cheek walls and its own roof."),
               roof_props(), &[]),
            op("add_foundation", &format!("{conv}A slab-on-grade or a stem-wall foundation (stem walls on footings around the polygon, slab inside) under the plan polygon; `top` = elevation of its top (default: storey 0 base)."),
               json!({ "points": pts("Plan polygon"), "origin": p2v(""), "size": p2v(""), "type": en("", &["slab", "stemwall"]), "slabThickness": n("m"), "stemHeight": n("m"), "stemThickness": n("m"), "footingWidth": n("m"), "footingDepth": n("m"), "slab": b("stemwall: pour a slab inside too"), "top": n("Elevation of the top (m)") }), &[]),
            op("update", "Deep-merge `patch` into a framing entity's data (arrays replaced); the element re-frames.", json!({ "id": s("Framing entity id"), "patch": { "type": "object" } }), &["id", "patch"]),
            op("frame_room", "Frame a construction room: one framing wall per edge (exterior edges only, or all), converting the room's construction openings into rough openings; wall height = storey height. Exterior = not shared with another room on the storey.",
               json!({ "room": s("construction.room id"), "which": en("", &["exterior", "all"]), "height": n("Override wall height (m)"), "studSize": en(LUMBER, &SIZES), "studSpacing": n("m"), "sheathing": sheathing_schema() }), &["room"]),
            op("frame_level", "frame_room for every construction room on a storey.", json!({ "level": i("Storey index"), "which": en("", &["exterior", "all"]), "height": n(""), "studSize": en(LUMBER, &SIZES), "studSpacing": n(""), "sheathing": sheathing_schema() }), &["level"]),
            op("estimate", "Material takeoff: lumber by category / size / stock length (2.44, 3.05, 3.66, 4.27, 4.88 m), sheathing and gypsum areas and 1.22×2.44 panels, concrete volume; totals and per element. No change. Result in entities[0].data.", json!({ "ids": { "type": "array", "items": { "type": "string" }, "description": "Limit to these framing entities" } }), &[]),
            op("summary", "Defaults, every framing element with its key numbers (studs, joists, trusses, lumber, beams, stairs) and the takeoff totals. No change.", json!({}), &[]),
            op("add_beam", &format!("{conv}A beam (girder / header beam) between two plan points: built-up lumber plies, LVL or glulam, top at `elevation` above the storey base (default: the storey height, a dropped beam under the floor above), with posts down to the storey floor at both ends and every `postSpacing`. Joist hangers are added where floor joists end against it."),
               json!({ "level": i("Storey index"), "a": p2v("Plan start [x,z]"), "b": p2v("Plan end [x,z]"), "elevation": n("Top of the beam above the storey base (m)"), "size": en(LUMBER, &SIZES), "plies": i("1–4 plies of `size` (lumber)"), "material": en("", &["lumber", "lvl", "glulam"]), "width": n("LVL/glulam width (m)"), "depth": n("LVL/glulam depth (m)"), "posts": b("Posts under the supports (default true)"), "postSpacing": n("Max distance between posts (m); none = posts at the ends only"), "postSize": en("", &["4x4", "4x6", "6x6"]), "tributary": n("Load width carried (m) for the check; default from the floors above") }), &["a", "b"]),
            op("add_stair", &format!("{conv}A straight stair on notched stringers: foot of the run at `a` (middle of the width), going up along `direction` (or `angle` in degrees, 0 = +x, 90 = +z); equal risers (default ceil(rise / 0.18)), treads with a 25 mm nosing, optional riser boards."),
               json!({ "level": i("Storey index"), "a": p2v("Plan point at the foot [x,z]"), "direction": p2v("Plan direction going up [dx,dz]"), "angle": n("Direction in degrees (alternative)"), "width": n("m (0.9)"), "totalRise": n("Floor to floor (m); default the storey height"), "riserHeight": n("Wanted riser height (m)"), "treadDepth": n("Going (m, 0.26)"), "stringers": i("≥ 2 (3)"), "stringerSize": en(LUMBER, &SIZES), "treadThickness": n("m (0.032)"), "risers": b("Riser boards (default true)") }), &["a"]),
            op("check", "Engineering-lite check (no change): floor joists and rafters (EN 1995 bending, shear, deflection L/360 floor imposed, L/240 roof total), truss spacing, stud heights, headers (span rule + bending under the roof/floor line load), beams, stem-wall footing bearing against `defaults.soilBearing`, stairs (riser 150–200 mm, going ≥ 230, 2R + G 570–650). Uses `defaults.species` (C18, C24, C30, SPF2, DFL2) and `defaults.loads`. Result: entities[0].data = { findings: [{ severity, element, title, detail, value?, limit? }], errors, warnings }.", json!({}), &[]),
            op("span_table", "Allowable simple spans per lumber size (no change): `use` floor (joists) or roof (rafters, horizontal), at `spacing`, for `species` and `loads` (defaults from the framing defaults). Result: entities[0].data.spans = [{ size, span_m, governs }].",
               json!({ "use": en("", &["floor", "roof"]), "spacing": n("m on centre (0.4)"), "species": en("", &["C18", "C24", "C30", "SPF2", "DFL2"]), "pitch": n("Roof pitch per 12 (6)"), "sizes": { "type": "array", "items": { "type": "string" } }, "loads": { "type": "object", "properties": { "floorLive": n("kN/m²"), "floorDead": n("kN/m²"), "roofSnow": n("kN/m² on plan"), "roofDead": n("kN/m² on slope") } } }), &[]),
            op("wall_svg", "Framing elevation of one wall as SVG (entities[0].data.svg): every member, rough openings with sizes, stud layout on-centre ticks, overall length and height, title block. Seen from the exterior.", json!({ "wall": s("Framing wall id") }), &["wall"]),
            op("plan_svg", "Framing plan of a storey as SVG (entities[0].data.svg): wall plates and studs, floor joists with span arrows and size @ spacing, beams, roof members, stairs with an UP arrow, 1 m scale bar.", json!({ "level": i("Storey index") }), &[]),
        ]
    }
}

impl Plugin for FramingPlugin {
    fn id(&self) -> &str { PLUGIN_ID }
    fn describe(&self) -> PluginInfo { PluginInfo { id: PLUGIN_ID.into(), name: "Framing".into(), entity_types: TYPES.iter().map(|t| t.to_string()).collect(), ops: Self::ops_info() } }
    fn expand(&self, entity: &Custom, doc: &Document) -> Result<Vec<Entity>, OpError> {
        if !TYPES.contains(&entity.type_.as_str()) { return Ok(vec![]); }
        let house = House::read(doc);
        let layer = if entity.layer.is_empty() { doc.default_layer().to_string() } else { entity.layer.clone() };
        Ok(expand::expand_any(entity, &house, &layer).faces)
    }
    fn apply(&self, op_type: &str, data: Value, doc: &mut Document) -> Result<OpResult, OpError> {
        ops::apply(op_type.strip_prefix("framing.").unwrap_or(op_type), data, doc)
    }
    fn transform(&self, e: &mut Custom, m: &Mat4, _doc: &Document) -> Result<(), OpError> { ops::transform_plan(e, m) }
    fn metrics(&self, doc: &Document) -> Option<Value> { if has_framing(doc) { Some(ops::metrics(doc)) } else { None } }
    fn on_load(&self, doc: &mut Document) { if has_framing(doc) || doc.extensions.contains_key(PLUGIN_ID) { ensure(doc); if has_framing(doc) { framing_layer(doc); } } }
}
