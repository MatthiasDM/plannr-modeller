//! Data model of the masonry plugin (`extensions.masonry.defaults` + `masonry.*` entities) and a
//! lenient reader of what it needs from the construction plugin. See docs/MASONRY.md.

use plannr_core::model::{Document, Entity, Layer, Material};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use plannr_core::geom::P3;
pub type P2 = [f64; 2];

pub const PLUGIN_ID: &str = "masonry";
pub const T_WALL: &str = "masonry.wall";
pub const T_FLOOR: &str = "masonry.floor";
pub const T_FOUNDATION: &str = "masonry.foundation";
pub const TYPES: [&str; 3] = [T_WALL, T_FLOOR, T_FOUNDATION];
pub const LAYER_NAME: &str = "Masonry";

/// (key, name, colour)
pub const MATERIALS: [(&str, &str, &str); 14] = [
    ("facingBrick", "Facing brick", "#9C4A32"), ("brick", "Common brick", "#B8644A"), ("clayBlock", "Clay block (snelbouw)", "#C9785A"),
    ("concreteBlock", "Concrete block", "#A7A6A1"), ("aeratedBlock", "Aerated concrete block", "#E4E2DC"), ("limeSandBlock", "Lime-sand block", "#E8E4D6"),
    ("insulation", "Cavity insulation", "#F2D16B"), ("plaster", "Plaster", "#F4F2EE"), ("concrete", "Concrete", "#A9A6A0"), ("steel", "Galvanised steel", "#8E9398"),
    ("bluestone", "Bluestone (arduin)", "#4F5660"), ("dpc", "Damp-proof course", "#1E1E1E"), ("floorBlock", "Floor infill block", "#B9B6AE"), ("joist", "Prestressed joist", "#97948D"),
];
pub fn material_id(key: &str) -> String { format!("masonry.{key}") }

fn t() -> bool { true }
fn f() -> bool { false }

// ---- defaults --------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Unit {
    #[serde(default)] pub name: String,
    #[serde(default = "d_kind")] pub kind: String,
    pub length: f64,
    pub height: f64,
    pub width: f64,
    #[serde(default = "d_joint")] pub joint: f64,
    #[serde(default = "d_bond")] pub bond: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub color: Option<String>,
}
fn d_kind() -> String { "brick".into() } fn d_joint() -> f64 { 0.01 } fn d_bond() -> String { "stretcher".into() }
impl Unit {
    pub fn facing() -> Unit { Unit { name: "Gevelsteen 190×90×65".into(), kind: "facingBrick".into(), length: 0.19, height: 0.065, width: 0.09, joint: 0.01, bond: "stretcher".into(), color: Some("#9C4A32".into()) } }
    pub fn snelbouw() -> Unit { Unit { name: "Snelbouwsteen 288×88×138".into(), kind: "clayBlock".into(), length: 0.288, height: 0.138, width: 0.09, joint: 0.012, bond: "stretcher".into(), color: Some("#C9785A".into()) } }
    /// Course height (unit + bed joint).
    pub fn course(&self) -> f64 { self.height + self.joint }
    /// Units per m² of wall face.
    pub fn per_m2(&self) -> f64 { 1.0 / ((self.length + self.joint) * (self.height + self.joint)) }
    pub fn volume(&self) -> f64 { self.length * self.height * self.width }
    /// Material id the leaf is drawn with: its own colour when given (`masonry.<kind>` otherwise).
    pub fn material(&self) -> String { material_id(&self.kind) }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Insulation { #[serde(default = "d_none")] pub kind: String, #[serde(default = "d_eps")] pub material: String, #[serde(default)] pub thickness: f64 }
fn d_none() -> String { "none".into() } fn d_eps() -> String { "eps".into() }
impl Default for Insulation { fn default() -> Self { Insulation { kind: "none".into(), material: "eps".into(), thickness: 0.0 } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Ties { #[serde(default = "d_ties")] pub per_m2: f64 }
fn d_ties() -> f64 { 5.0 }
impl Default for Ties { fn default() -> Self { Ties { per_m2: 5.0 } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cavity { #[serde(default = "d_cav")] pub width: f64, #[serde(default)] pub insulation: Insulation, #[serde(default)] pub ties: Ties }
fn d_cav() -> f64 { 0.06 }
impl Default for Cavity { fn default() -> Self { Cavity { width: 0.06, insulation: Insulation::default(), ties: Ties::default() } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plaster { #[serde(default = "t")] pub interior: bool, #[serde(default = "f")] pub exterior: bool, #[serde(default = "d_pl")] pub thickness: f64 }
fn d_pl() -> f64 { 0.015 }
impl Default for Plaster { fn default() -> Self { Plaster { interior: true, exterior: false, thickness: 0.015 } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LintelDefaults { #[serde(default = "d_bearing")] pub bearing: f64, #[serde(default = "d_concrete")] pub inner: String, #[serde(default = "d_angle")] pub outer: String }
fn d_bearing() -> f64 { 0.15 } fn d_concrete() -> String { "concrete".into() } fn d_angle() -> String { "steelAngle".into() }
impl Default for LintelDefaults { fn default() -> Self { LintelDefaults { bearing: 0.15, inner: "concrete".into(), outer: "steelAngle".into() } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SillDefaults { #[serde(default = "d_bluestone")] pub outer: String, #[serde(default = "d_proj")] pub projection: f64, #[serde(default = "d_sill_t")] pub thickness: f64 }
fn d_bluestone() -> String { "bluestone".into() } fn d_proj() -> f64 { 0.03 } fn d_sill_t() -> f64 { 0.05 }
impl Default for SillDefaults { fn default() -> Self { SillDefaults { outer: "bluestone".into(), projection: 0.03, thickness: 0.05 } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FloorDefaults {
    #[serde(default = "d_bb")] pub system: String,
    #[serde(default = "d_012")] pub joist_width: f64,
    #[serde(default = "d_012")] pub joist_height: f64,
    #[serde(default = "d_06")] pub spacing: f64,
    #[serde(default = "d_concrete")] pub block_kind: String,
    #[serde(default = "d_016")] pub block_height: f64,
    #[serde(default = "d_025")] pub block_length: f64,
    #[serde(default = "d_004")] pub topping: f64,
    #[serde(default = "t")] pub mesh: bool,
    #[serde(default = "d_01")] pub bearing: f64,
    #[serde(default = "t")] pub ring_beam: bool,
    #[serde(default = "d_02")] pub plank_thickness: f64,
    #[serde(default = "d_12")] pub plank_width: f64,
    #[serde(default = "d_018")] pub slab_thickness: f64,
    /// Plaster the ceiling under the floor (hides the joists and blocks from below).
    #[serde(default = "f")] pub ceiling_plaster: bool,
}
fn d_bb() -> String { "beamBlock".into() } fn d_012() -> f64 { 0.12 } fn d_06() -> f64 { 0.6 } fn d_016() -> f64 { 0.16 } fn d_025() -> f64 { 0.25 } fn d_004() -> f64 { 0.04 } fn d_01() -> f64 { 0.1 } fn d_02() -> f64 { 0.2 } fn d_12() -> f64 { 1.2 } fn d_018() -> f64 { 0.18 }
impl Default for FloorDefaults { fn default() -> Self { serde_json::from_value(Value::Object(Map::new())).unwrap() } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FoundationDefaults { #[serde(default = "d_06")] pub footing_width: f64, #[serde(default = "d_03")] pub footing_depth: f64, #[serde(default = "d_m08")] pub bottom: f64, #[serde(default = "d_019")] pub block_width: f64 }
fn d_03() -> f64 { 0.3 } fn d_m08() -> f64 { -0.8 } fn d_019() -> f64 { 0.19 }
impl Default for FoundationDefaults { fn default() -> Self { FoundationDefaults { footing_width: 0.6, footing_depth: 0.3, bottom: -0.8, block_width: 0.19 } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Defaults {
    #[serde(default = "Unit::facing")] pub outer: Unit,
    #[serde(default = "Unit::snelbouw")] pub inner: Unit,
    #[serde(default = "Unit::snelbouw")] pub partition: Unit,
    #[serde(default)] pub cavity: Cavity,
    #[serde(default)] pub plaster: Plaster,
    #[serde(default = "t")] pub dpc: bool,
    #[serde(default)] pub lintel: LintelDefaults,
    #[serde(default)] pub sill: SillDefaults,
    #[serde(default)] pub floor: FloorDefaults,
    #[serde(default)] pub foundation: FoundationDefaults,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
impl Default for Defaults { fn default() -> Self { serde_json::from_value(Value::Object(Map::new())).unwrap() } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings { #[serde(default)] pub defaults: Defaults, #[serde(flatten, default)] pub extra: Map<String, Value> }

pub fn settings(doc: &Document) -> Settings { doc.extensions.get(PLUGIN_ID).cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default() }
pub fn save_settings(doc: &mut Document, s: &Settings) { doc.extensions.insert(PLUGIN_ID.to_string(), serde_json::to_value(s).unwrap_or(Value::Null)); }

// ---- entities ------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct OpeningLintel { #[serde(default, skip_serializing_if = "Option::is_none")] pub inner: Option<String>, #[serde(default, skip_serializing_if = "Option::is_none")] pub outer: Option<String>, #[serde(default, skip_serializing_if = "Option::is_none")] pub bearing: Option<f64> }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Opening {
    #[serde(default = "new_id")] pub id: String,
    #[serde(default = "d_window")] pub kind: String,
    pub offset: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)] pub sill: f64,
    #[serde(default)] pub lintel: OpeningLintel,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub sill_stone: Option<bool>,
    /// The construction opening this one was imported from.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub source: Option<String>,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_window() -> String { "window".into() }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Source { pub owner: String, pub key: String }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Wall {
    #[serde(default)] pub level: usize,
    pub a: P2,
    pub b: P2,
    #[serde(default = "d_exterior")] pub reference: String,
    #[serde(rename = "type", default = "d_cavity_t")] pub type_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub outer_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub outer: Option<Unit>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub inner: Option<Unit>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub cavity: Option<Cavity>,
    #[serde(default)] pub plaster: Plaster,
    #[serde(default = "t")] pub dpc: bool,
    #[serde(default)] pub openings: Vec<Opening>,
    #[serde(default = "d_auto")] pub start_join: String,
    #[serde(default = "d_auto")] pub end_join: String,
    #[serde(default = "f")] pub flip: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub source: Option<Source>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub room: Option<String>,
    /// Material of the interior plaster face (a room's paint), of the other side / exterior render,
    /// and of the outer leaf when it is not the unit's own (another brick colour).
    #[serde(default, skip_serializing_if = "Option::is_none")] pub finish_interior: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub finish_exterior: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub outer_material: Option<String>,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_exterior() -> String { "exterior".into() } fn d_cavity_t() -> String { "cavity".into() } fn d_auto() -> String { "auto".into() }
impl Wall {
    pub fn length(&self) -> f64 { ((self.b[0] - self.a[0]).powi(2) + (self.b[1] - self.a[1]).powi(2)).sqrt() }
    pub fn is_cavity(&self) -> bool { self.type_ != "single" }
    /// The units / cavity this wall uses (its own, else the defaults).
    pub fn outer_unit(&self, d: &Defaults) -> Unit { self.outer.clone().unwrap_or_else(|| d.outer.clone()) }
    pub fn inner_unit(&self, d: &Defaults) -> Unit { self.inner.clone().unwrap_or_else(|| if self.is_cavity() { d.inner.clone() } else { d.partition.clone() }) }
    pub fn cavity_of(&self, d: &Defaults) -> Cavity { self.cavity.clone().unwrap_or_else(|| d.cavity.clone()) }
    /// Total thickness of the structural body (leaves + cavity).
    pub fn thickness(&self, d: &Defaults) -> f64 {
        if self.is_cavity() { self.outer_unit(d).width + self.cavity_of(d).width + self.inner_unit(d).width } else { self.inner_unit(d).width }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Floor {
    #[serde(default = "d_one")] pub level: usize,
    pub points: Vec<P2>,
    #[serde(default = "d_bb")] pub system: String,
    #[serde(default = "d_dir")] pub direction: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub joist_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub joist_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub spacing: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub block_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub block_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub block_length: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub topping: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub mesh: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub bearing: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub ring_beam: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub ring_beam_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub plank_thickness: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub plank_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub slab_thickness: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub ceiling_plaster: Option<bool>,
    #[serde(default)] pub openings: Vec<Vec<P2>>,
    #[serde(default)] pub rooms: Vec<String>,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_one() -> usize { 1 } fn d_dir() -> String { "auto".into() }

/// A floor with every field resolved against the defaults.
#[derive(Debug, Clone)]
pub struct FloorSpec { pub system: String, pub joist_width: f64, pub joist_height: f64, pub spacing: f64, pub block_kind: String, pub block_height: f64, pub block_length: f64, pub topping: f64, pub mesh: bool, pub bearing: f64, pub ring_beam: bool, pub ring_beam_width: f64, pub plank_thickness: f64, pub plank_width: f64, pub slab_thickness: f64 }
impl Floor {
    pub fn spec(&self, d: &FloorDefaults) -> FloorSpec {
        let bearing = self.bearing.unwrap_or(d.bearing);
        FloorSpec {
            system: self.system.clone(), joist_width: self.joist_width.unwrap_or(d.joist_width), joist_height: self.joist_height.unwrap_or(d.joist_height), spacing: self.spacing.unwrap_or(d.spacing),
            block_kind: self.block_kind.clone().unwrap_or_else(|| d.block_kind.clone()), block_height: self.block_height.unwrap_or(d.block_height), block_length: self.block_length.unwrap_or(d.block_length),
            topping: self.topping.unwrap_or(d.topping), mesh: self.mesh.unwrap_or(d.mesh), bearing, ring_beam: self.ring_beam.unwrap_or(d.ring_beam), ring_beam_width: self.ring_beam_width.unwrap_or(bearing),
            plank_thickness: self.plank_thickness.unwrap_or(d.plank_thickness), plank_width: self.plank_width.unwrap_or(d.plank_width), slab_thickness: self.slab_thickness.unwrap_or(d.slab_thickness),
        }
    }
}
impl FloorSpec {
    /// Structural depth of the floor (top at the storey base, underside this far below).
    pub fn depth(&self) -> f64 {
        match self.system.as_str() { "hollowCore" => self.plank_thickness + self.topping, "insitu" => self.slab_thickness, _ => self.block_height + self.topping }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Foundation {
    pub points: Vec<P2>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub footing_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub footing_depth: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub bottom: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub block_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub top: Option<f64>,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}

// ---- the house as the masonry plugin sees it ----------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct House {
    pub storeys: Vec<(String, f64)>,
    pub ground_level: f64,
    pub floor_thickness: f64,
    pub defaults: Defaults,
    /// Every masonry wall and floor of the document (junctions, wall heights).
    pub walls: Vec<(String, Wall)>,
    pub floors: Vec<(String, Floor)>,
    /// What fills the top of each storey (masonry floors of the storey above, attic joists), as the
    /// construction plugin sees it.
    pub structure: Vec<crate::data::TopStructure>,
    /// The roofs of the construction model: the walls under them take their tops from them.
    pub roofs: Vec<crate::roof::RoofGeom>,
}

impl House {
    /// The masonry of the document: explicit masonry walls and floors plus those derived from the
    /// construction model (every wall piece and upper storey not replaced by an explicit one).
    pub fn read(doc: &Document) -> House {
        let m = crate::expand::model(doc);
        let set = crate::expand::collect(&m);
        House::from_parts(doc, &m, &set)
    }
    pub fn from_parts(doc: &Document, m: &crate::expand::Model, set: &crate::expand::WallSet) -> House {
        let defaults = settings(doc).defaults;
        let mut h = House { floor_thickness: m.settings.floor_thickness, ground_level: m.settings.ground_level, ..Default::default() };
        h.storeys = m.settings.storeys.iter().map(|s| (s.name.clone(), s.height)).collect();
        if h.storeys.is_empty() { h.storeys.push(("Ground floor".into(), 2.6)); }
        h.structure = m.settings.structure.clone();
        let (walls, floors) = crate::masonry::derive::explicit(doc);
        h.walls = walls; h.floors = floors;
        h.walls.extend(crate::masonry::derive::derived_walls(m, set, &defaults));
        h.floors.extend(m.derived_floors.iter().map(|f| (f.id.clone(), f.floor.clone())));
        h.defaults = defaults;
        h.roofs = crate::roof::geoms(m, set);
        h
    }
    /// Is this id one of the derived (not stored) walls / floors?
    pub fn is_derived(id: &str) -> bool { id.contains('#') }
    pub fn base(&self, level: usize) -> f64 { self.ground_level + self.storeys.iter().take(level).map(|s| s.1).sum::<f64>() }
    pub fn height(&self, level: usize) -> f64 { self.storeys.get(level).map(|s| s.1).or_else(|| self.storeys.last().map(|s| s.1)).unwrap_or(2.6) }
    pub fn has_storey_above(&self, level: usize) -> bool { level + 1 < self.storeys.len() }
    /// Depth of the masonry floor of storey `level + 1` over plan point `p` (None when there is none).
    pub fn floor_depth_above(&self, level: usize, p: P2) -> Option<f64> {
        // the floor outline runs over the inner leaves, the wall line may lie just outside it: 0.3 m slack
        let masonry = self.floors.iter().filter(|(_, fl)| fl.level == level + 1 && near_poly(p, &fl.points, 0.3)).map(|(_, fl)| fl.spec(&self.defaults.floor).depth());
        // attic joists (framed floors in the top of the storey) count too: the leaf stops under them
        let other = self.structure.iter().filter(|t| t.level == level && near_poly(p, &t.outline, 0.3)).map(|t| t.depth);
        masonry.chain(other).fold(None, |a: Option<f64>, d| Some(a.map_or(d, |x| x.max(d))))
    }
    /// The deepest structure in the top of storey `level` anywhere (0 when none).
    pub fn max_structure(&self, level: usize) -> f64 {
        let masonry = self.floors.iter().filter(|(_, fl)| fl.level == level + 1).map(|(_, fl)| fl.spec(&self.defaults.floor).depth());
        let other = self.structure.iter().filter(|t| t.level == level).map(|t| t.depth);
        masonry.chain(other).fold(0.0, f64::max)
    }
}

/// Inside the polygon, or within `tol` of its outline.
pub fn near_poly(p: P2, poly: &[P2], tol: f64) -> bool {
    if point_in_poly(p, poly) { return true; }
    let n = poly.len();
    (0..n).any(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let d = [b[0] - a[0], b[1] - a[1]]; let l2 = d[0] * d[0] + d[1] * d[1]; if l2 < 1e-12 { return false; }
        let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0);
        ((a[0] + d[0] * t - p[0]).powi(2) + (a[1] + d[1] * t - p[1]).powi(2)).sqrt() <= tol
    })
}

pub fn point_in_poly(p: P2, poly: &[P2]) -> bool {
    let n = poly.len(); if n < 3 { return false; }
    let mut inside = false; let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1] + 1e-12) + a[0] { inside = !inside; }
        j = i;
    }
    inside
}
/// Signed plan area (positive = counter-clockwise in x/z).
pub fn plan_area(p: &[P2]) -> f64 { let n = p.len(); let mut s = 0.0; for i in 0..n { let (a, b) = (p[i], p[(i + 1) % n]); s += a[0] * b[1] - b[0] * a[1]; } s / 2.0 }

// ---- persistence ----------------------------------------------------------------------------------------------

pub fn new_id() -> String { uuid::Uuid::new_v4().to_string() }
pub fn has_masonry(doc: &Document) -> bool { doc.entities.iter().any(|e| matches!(e, Entity::Custom(c) if TYPES.contains(&c.type_.as_str()))) }

pub fn ensure(doc: &mut Document) {
    if !doc.extensions.contains_key(PLUGIN_ID) { save_settings(doc, &Settings::default()); }
    for (key, name, color) in MATERIALS {
        let id = material_id(key);
        if !doc.materials.iter().any(|m| m.id == id) { doc.materials.push(Material { id, name: name.to_string(), color: color.to_string(), opacity: 1.0 }); }
    }
}
/// Change the leaf / cavity / partition thicknesses of the defaults (renaming a unit whose width changes).
pub fn set_widths(doc: &mut Document, partition: Option<f64>, inner: Option<f64>, cavity: Option<f64>, outer: Option<f64>) {
    let mut s = settings(doc);
    let d = &mut s.defaults;
    let resize = |u: &mut Unit, w: Option<f64>| { if let Some(w) = w { if (u.width - w).abs() > 1e-6 { u.width = w; u.name = crate::masonry::derive::unit_name(u); } } };
    resize(&mut d.partition, partition); resize(&mut d.inner, inner); resize(&mut d.outer, outer);
    if let Some(c) = cavity { d.cavity.width = c; }
    save_settings(doc, &s);
}

/// First load of a document without masonry defaults: take the thicknesses from its construction
/// settings, and give a model that never chose a cavity setting cavity walls.
pub fn migrate(doc: &mut Document) {
    if doc.extensions.contains_key(PLUGIN_ID) { return; }
    save_settings(doc, &Settings::default());
    let c = doc.extensions.get("construction").cloned();
    if let Some(c) = c {
        let cav = c.get("cavity").cloned();
        let num = |v: &Option<Value>, k: &str| v.as_ref().and_then(|x| x.get(k)).and_then(|x| x.as_f64());
        let enabled = cav.as_ref().and_then(|x| x.get("enabled")).and_then(|x| x.as_bool()).unwrap_or(false);
        let wall = c.get("wallThickness").and_then(|x| x.as_f64());
        if enabled { set_widths(doc, wall, num(&cav, "innerLeaf"), num(&cav, "cavity"), num(&cav, "outerLeaf")); } else { set_widths(doc, wall, None, None, None); }
        if cav.is_none() { if let Some(cx) = doc.extensions.get_mut("construction") { cx["cavity"] = serde_json::json!({ "enabled": true }); } }
    }
}

pub fn masonry_layer(doc: &mut Document) -> String {
    if let Some(l) = doc.layers.iter().find(|l| l.name == LAYER_NAME) { return l.id.clone(); }
    let id = new_id();
    doc.layers.push(Layer { id: id.clone(), name: LAYER_NAME.into(), visible: true, color: "#9c4a32".into() });
    id
}
pub fn to_p3(p: P2, y: f64) -> P3 { [p[0], y, p[1]] }
