//! Data carried by `construction.*` custom entities and the `construction` extension block.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub type P2 = [f64; 2];
pub type P3 = [f64; 3];

pub const PLUGIN_ID: &str = "construction";
pub const T_ROOM: &str = "construction.room";
pub const T_WALL: &str = "construction.wall";
pub const T_OPENING: &str = "construction.opening";
pub const T_ROOF: &str = "construction.roof";
pub const T_FIXTURE: &str = "construction.fixture";
pub const T_RUN: &str = "construction.run";
pub const T_STAIR: &str = "construction.stair";
pub const T_UNIT: &str = "construction.unit";

fn t() -> bool { true }
fn one() -> f64 { 1.0 }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Storey {
    #[serde(default = "new_id")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_storey_height")]
    pub height: f64,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}
pub fn new_id() -> String { uuid::Uuid::new_v4().to_string() }
pub fn default_storey_height() -> f64 { 2.6 }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cavity {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "d_inner")]
    pub inner_leaf: f64,
    #[serde(default = "d_cavity")]
    pub cavity: f64,
    #[serde(default = "d_outer")]
    pub outer_leaf: f64,
}
fn d_inner() -> f64 { 0.09 }
fn d_cavity() -> f64 { 0.06 }
fn d_outer() -> f64 { 0.09 }
impl Default for Cavity {
    fn default() -> Self { Cavity { enabled: false, inner_leaf: 0.09, cavity: 0.06, outer_leaf: 0.09 } }
}
impl Cavity {
    pub fn total(&self) -> f64 { self.inner_leaf + self.cavity + self.outer_leaf }
}

/// A parametric component type: the JavaScript that declares its parameters, rules and geometry
/// (Studio's `component(id, {...})`). Carried in the document so a file still works elsewhere.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComponentType {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "d_group")]
    pub group: String,
    pub source: String,
}
fn d_group() -> String { "Components".into() }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub storeys: Vec<Storey>,
    /// Document-defined component types (override the built-in catalogue by id).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub component_types: Vec<ComponentType>,
    #[serde(default = "d_wall")]
    pub wall_thickness: f64,
    #[serde(default = "d_floor")]
    pub floor_thickness: f64,
    #[serde(default)]
    pub cavity: Cavity,
    /// Height of the ground floor's finished floor above the world origin (m). Everything the
    /// plugin builds is stacked on top of it; site geometry (core entities) is not affected.
    #[serde(default)]
    pub ground_level: f64,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
    /// Structures occupying the top of a storey (a masonry floor of the storey above, attic joists
    /// framed at the top): filled by `expand::model`, never stored.
    #[serde(skip)]
    pub structure: Vec<TopStructure>,
}
/// Something that occupies the top `depth` of storey `level` over `outline`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TopStructure { pub level: usize, pub outline: Vec<P2>, pub depth: f64, pub plastered: bool }
fn d_wall() -> f64 { 0.10 }
fn d_floor() -> f64 { 0.06 }
impl Default for Settings {
    fn default() -> Self {
        Settings { storeys: vec![Storey { id: new_id(), name: "Ground floor".into(), height: 2.6, extra: Map::new() }], component_types: vec![], wall_thickness: 0.10, floor_thickness: 0.06, cavity: Cavity::default(), ground_level: 0.0, extra: Map::new(), structure: vec![] }
    }
}
impl Settings {
    pub fn base(&self, level: usize) -> f64 { self.ground_level + self.storeys.iter().take(level).map(|s| s.height).sum::<f64>() }
    pub fn height(&self, level: usize) -> f64 { self.storeys.get(level).map(|s| s.height).unwrap_or_else(|| self.storeys.last().map(|s| s.height).unwrap_or(2.6)) }
    pub fn total_height(&self) -> f64 { self.storeys.iter().map(|s| s.height).sum() }
    pub fn count(&self) -> usize { self.storeys.len().max(1) }
    /// Depth of the structure in the top of storey `level` over plan point `p` (0 when none).
    pub fn structure_depth(&self, level: usize, p: P2) -> f64 {
        self.structure.iter().filter(|t| t.level == level && point_in_plan(p, &t.outline)).map(|t| t.depth).fold(0.0, f64::max)
    }
    /// Clear height of storey `level` at `p`: floor-to-floor height minus the structure above.
    pub fn clear_height(&self, level: usize, p: P2) -> f64 { self.height(level) - self.structure_depth(level, p) }
}

/// Which wall hosts an opening or a wall fixture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Host {
    RoomEdge { room: String, edge: usize },
    Wall { wall: String },
}
impl Host {
    pub fn key(&self) -> String {
        match self {
            Host::RoomEdge { room, edge } => format!("room.{room}.{edge}"),
            Host::Wall { wall } => wall.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub level: usize,
    #[serde(default)]
    pub points: Vec<P2>,
    #[serde(default = "t")]
    pub has_floor: bool,
    #[serde(default)]
    pub has_ceiling: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_height: Option<f64>,
    #[serde(default)]
    pub finishes: Map<String, Value>,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}
impl Room {
    pub fn centroid(&self) -> P2 {
        let n = self.points.len().max(1) as f64;
        let (x, z) = self.points.iter().fold((0.0, 0.0), |(x, z), p| (x + p[0], z + p[1]));
        [x / n, z / n]
    }
    pub fn area(&self) -> f64 { plan_area(&self.points).abs() }
    pub fn perimeter(&self) -> f64 {
        let n = self.points.len();
        (0..n).map(|i| dist2(self.points[i], self.points[(i + 1) % n])).sum()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Wall {
    #[serde(default)]
    pub level: usize,
    pub a: P2,
    pub b: P2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickness: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exterior: Option<bool>,
    #[serde(default)]
    pub finishes: Map<String, Value>,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OpeningKind { Door, Window, Passage, Garage }
impl OpeningKind {
    /// (width, height, sill)
    pub fn defaults(self) -> (f64, f64, f64) {
        match self {
            OpeningKind::Door => (0.9, 2.05, 0.0),
            OpeningKind::Window => (1.2, 1.4, 0.9),
            OpeningKind::Passage => (1.0, 2.1, 0.0),
            OpeningKind::Garage => (2.5, 2.1, 0.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Opening {
    #[serde(default)]
    pub level: usize,
    pub kind: OpeningKind,
    pub host: Host,
    #[serde(default)]
    pub along: f64,
    #[serde(default = "one")]
    pub width: f64,
    #[serde(default = "one")]
    pub height: f64,
    #[serde(default)]
    pub sill: f64,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RoofStyle { Flat, Gable, Hip, Shed }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Roof {
    #[serde(default = "d_style")]
    pub style: RoofStyle,
    #[serde(default = "d_pitch")]
    pub pitch_height: f64,
    #[serde(default = "d_overhang")]
    pub overhang: f64,
    #[serde(default = "d_thick")]
    pub thickness: f64,
    #[serde(default = "t")]
    pub ridge_along_x: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<P2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<P2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub levels: Option<usize>,
    #[serde(default = "t")]
    pub enabled: bool,
    /// Overhang past the gable walls (verge); default the eaves overhang.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verge_overhang: Option<f64>,
    /// Ends of the ridge (`min` / `max` along it) that stand on a party wall: the roof runs on over
    /// the neighbour, so no verge overhang and no verge board there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub party: Vec<String>,
    /// Rafters (kepers): [width, depth] and spacing.
    #[serde(default = "d_rafter")]
    pub rafter_size: [f64; 2],
    #[serde(default = "d_rspacing")]
    pub rafter_spacing: f64,
    /// Purlins (gordingen) and ridge purlin: [width, depth]; the rafters span at most `purlinSpan`
    /// (along the slope) between supports.
    #[serde(default = "d_purlin")]
    pub purlin_size: [f64; 2],
    #[serde(default = "d_pspan")]
    pub purlin_span: f64,
    /// Wall plate (muurplaat) height; its width is the inner leaf plus the cavity.
    #[serde(default = "d_plate_h")]
    pub plate_height: f64,
    /// Covering: tiles, slates or metal; battens at `battenGauge`.
    #[serde(default = "d_cover")]
    pub covering: String,
    #[serde(default = "d_gauge")]
    pub batten_gauge: f64,
    #[serde(default)]
    pub finishes: Map<String, Value>,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}
fn d_rafter() -> [f64; 2] { [0.063, 0.175] }
fn d_rspacing() -> f64 { 0.4 }
fn d_purlin() -> [f64; 2] { [0.1, 0.25] }
fn d_pspan() -> f64 { 2.4 }
fn d_plate_h() -> f64 { 0.075 }
fn d_cover() -> String { "tiles".into() }
fn d_gauge() -> f64 { 0.32 }
fn d_style() -> RoofStyle { RoofStyle::Gable }
fn d_pitch() -> f64 { 1.6 }
fn d_overhang() -> f64 { 0.3 }
fn d_thick() -> f64 { 0.15 }
impl Default for Roof {
    fn default() -> Self {
        Roof { style: RoofStyle::Gable, pitch_height: 1.6, overhang: 0.3, thickness: 0.15, ridge_along_x: true, min: None, max: None, levels: None, enabled: true,
               verge_overhang: None, party: vec![], rafter_size: d_rafter(), rafter_spacing: d_rspacing(), purlin_size: d_purlin(), purlin_span: d_pspan(), plate_height: d_plate_h(),
               covering: d_cover(), batten_gauge: d_gauge(), finishes: Map::new(), extra: Map::new() }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum System { Electrical, WaterCold, WaterHot, Drain, Heating }
impl System {
    pub fn key(self) -> &'static str {
        match self { System::Electrical => "electrical", System::WaterCold => "waterCold", System::WaterHot => "waterHot", System::Drain => "drain", System::Heating => "heating" }
    }
    pub fn color(self) -> &'static str {
        match self { System::Electrical => "#F2B705", System::WaterCold => "#2E7BE4", System::WaterHot => "#E5484D", System::Drain => "#8A8F98", System::Heating => "#F2711C" }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FixtureKind {
    Outlet, SwitchPlate, CeilingLight, WallLight, Panel, DataPoint,
    TapCold, TapHot, Mixer, ShowerValve, ToiletSupply,
    FloorDrain, DrainPoint,
    Radiator, Boiler, Manifold, Thermostat,
}
impl FixtureKind {
    pub fn system(self) -> System {
        use FixtureKind::*;
        match self {
            Outlet | SwitchPlate | CeilingLight | WallLight | Panel | DataPoint => System::Electrical,
            TapCold | ToiletSupply => System::WaterCold,
            TapHot | Mixer | ShowerValve => System::WaterHot,
            FloorDrain | DrainPoint => System::Drain,
            Radiator | Boiler | Manifold | Thermostat => System::Heating,
        }
    }
    /// Marker box size (w, h, d).
    pub fn marker(self) -> P3 {
        use FixtureKind::*;
        match self {
            Panel => [0.4, 0.6, 0.12], Radiator => [0.8, 0.6, 0.1], Boiler => [0.45, 0.7, 0.35], Manifold => [0.4, 0.3, 0.12],
            CeilingLight => [0.25, 0.08, 0.25], FloorDrain => [0.15, 0.03, 0.15],
            _ => [0.12, 0.12, 0.12],
        }
    }
    pub const ALL: [&'static str; 17] = ["outlet", "switchPlate", "ceilingLight", "wallLight", "panel", "dataPoint", "tapCold", "tapHot", "mixer", "showerValve", "toiletSupply", "floorDrain", "drainPoint", "radiator", "boiler", "manifold", "thermostat"];
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Fixture {
    #[serde(default)]
    pub level: usize,
    pub kind: FixtureKind,
    pub position: P3,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<Host>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f64>,
    #[serde(default)]
    pub note: String,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    #[serde(default)]
    pub level: usize,
    pub system: System,
    #[serde(default)]
    pub points: Vec<P3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diameter: Option<f64>,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum StairKind { #[default] Straight, QuarterTurn }

/// A staircase: the foot (centre of the bottom riser), the compass bearing it climbs along,
/// and how it is stepped. The rise is the storey height, never stored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stair {
    #[serde(default = "d_stair_name")]
    pub name: String,
    #[serde(default)]
    pub level: usize,
    pub origin: P2,
    /// Compass degrees: 0 climbs towards −z (north), 90 towards +x (east).
    #[serde(default)]
    pub bearing: f64,
    #[serde(default = "d_stair_width")]
    pub width: f64,
    #[serde(default = "d_going")]
    pub going: f64,
    #[serde(default = "d_risers")]
    pub risers: usize,
    #[serde(default)]
    pub kind: StairKind,
    #[serde(default = "d_turn_after")]
    pub turn_after: usize,
    #[serde(default = "t")]
    pub turns_right: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winders: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tread_thickness: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nosing: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_risers: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stringer: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rail_left: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rail_right: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub well_opening: Option<bool>,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}
fn d_stair_name() -> String { "Stair".into() }
fn d_stair_width() -> f64 { 0.9 }
fn d_going() -> f64 { 0.25 }
fn d_risers() -> usize { 15 }
fn d_turn_after() -> usize { 6 }
impl Stair {
    pub fn tread_depth(&self) -> f64 { self.tread_thickness.unwrap_or(0.04).max(0.005) }
    pub fn nosing_depth(&self) -> f64 { self.nosing.unwrap_or(0.025).max(0.0) }
    pub fn has_risers(&self) -> bool { !self.open_risers.unwrap_or(false) }
    pub fn stringer_thickness(&self) -> f64 { self.stringer.unwrap_or(0.05).max(0.0) }
    pub fn opens_floor_above(&self) -> bool { self.well_opening.unwrap_or(true) }
    pub fn winder_count(&self) -> usize { self.winders.unwrap_or(0) }
    /// Unit vector of travel in plan: north is −z.
    pub fn direction(&self) -> P2 { let r = self.bearing.to_radians(); [r.sin(), -r.cos()] }
    /// Sideways unit vector, to the walker's right.
    pub fn right(&self) -> P2 { let d = self.direction(); [-d[1], d[0]] }
}

/// A fitted unit or piece of furniture. With a scripted `type` it is built from the component
/// script's solids (origin = middle of its back, extending forward along `bearing`); otherwise a
/// box of width × depth × height lifted `lift` off the floor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Unit {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub type_: String,
    #[serde(default)]
    pub level: usize,
    pub origin: P2,
    #[serde(default)]
    pub bearing: f64,
    #[serde(default = "d_unit")]
    pub width: f64,
    #[serde(default = "d_unit")]
    pub depth: f64,
    #[serde(default = "d_unit_h")]
    pub height: f64,
    #[serde(default)]
    pub lift: f64,
    /// Parameter values for a scripted type (numbers), e.g. {"width": 0.8}.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub values: Map<String, Value>,
    /// Choice parameters for a scripted type (strings), e.g. {"front": "drawers"}.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub choices: Map<String, Value>,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}
fn d_unit() -> f64 { 0.6 }
fn d_unit_h() -> f64 { 0.75 }
impl Unit {
    pub fn direction(&self) -> P2 { let r = self.bearing.to_radians(); [r.sin(), -r.cos()] }
    pub fn right(&self) -> P2 { let d = self.direction(); [-d[1], d[0]] }
    /// Does the box carry a worktop (base and vanity units)?
    pub fn has_worktop(&self) -> bool { let t = self.type_.to_lowercase(); t.contains("base") || t.contains("vanity") || t.contains("worktop") }
}

/// Box size (width, depth, height) and lift above the floor for a Uzzetje component type.
pub fn unit_defaults(type_: &str) -> (f64, f64, f64, f64) {
    match type_ {
        "base-unit" => (0.60, 0.60, 0.72, 0.12), "wall-unit" => (0.60, 0.37, 0.72, 1.40), "tall-unit" => (0.60, 0.60, 2.00, 0.12),
        "extractor" => (0.90, 0.50, 0.40, 1.50), "double-vanity" => (1.20, 0.48, 0.56, 0.30), "bath" => (1.70, 0.70, 0.55, 0.0),
        "shower" => (0.90, 0.90, 0.06, 0.0), "toilet" => (0.40, 0.70, 0.40, 0.0), "shelving" => (1.00, 0.30, 2.40, 0.0),
        _ => (0.60, 0.60, 0.75, 0.0),
    }
}

// ---- materials -----------------------------------------------------------------------

/// (key, name, colour, opacity)
pub const MATERIALS: &[(&str, &str, &str, f64)] = &[
    ("plaster", "Plaster", "#F4F2EE", 1.0),
    ("brick", "Brick", "#B0623F", 1.0),
    ("brickBrown", "Brick · dark brown", "#6E3F2A", 1.0),
    ("brickBuff", "Brick · buff", "#C9A86B", 1.0),
    ("cladding", "Cladding · white", "#E9E6DF", 1.0),
    ("paintWhite", "Paint · white", "#F4F2EE", 1.0),
    ("paintWarm", "Paint · warm", "#E8D9BF", 1.0),
    ("paintGrey", "Paint · grey", "#B8B8B4", 1.0),
    ("paintDark", "Paint · dark", "#4A4A4A", 1.0),
    ("paintBlue", "Paint · blue", "#8FB6DE", 1.0),
    ("paintGreen", "Paint · green", "#9BC49B", 1.0),
    ("paintPurple", "Paint · purple", "#B9A5DC", 1.0),
    ("paintPink", "Paint · pink", "#E79AC0", 1.0),
    ("concrete", "Concrete", "#A9A6A0", 1.0),
    ("wood", "Wood", "#C8A165", 1.0),
    ("tile", "Roof tiles", "#7A8AA0", 1.0),
    ("tileBlack", "Roof tiles · black", "#2C2E33", 1.0),
    ("slate", "Slates", "#454B54", 1.0),
    ("roofing", "Roofing membrane", "#2E2F31", 1.0),
    ("timber", "Timber · spruce", "#D4B07C", 1.0),
    ("fascia", "Fascia board", "#ECEAE4", 1.0),
    ("glass", "Glass", "#9FC9E8", 0.45),
    ("door", "Door", "#8B6A43", 1.0),
    ("frame", "Frame", "#6B5B4E", 1.0),
    ("electrical", "Electrical", "#F2B705", 1.0),
    ("waterCold", "Cold water", "#2E7BE4", 1.0),
    ("waterHot", "Hot water", "#E5484D", 1.0),
    ("drain", "Drain", "#8A8F98", 1.0),
    ("heating", "Heating", "#F2711C", 1.0),
    ("grass", "Grass", "#7FA85A", 1.0),
    ("gravel", "Gravel", "#B8B0A2", 1.0),
    ("pavement", "Pavement", "#B8B0A2", 1.0),
    ("hedge", "Hedge", "#5E8F4E", 1.0),
    ("foliage", "Foliage", "#5E8F4E", 1.0),
    ("bark", "Bark", "#6B4F3A", 1.0),
    ("steel", "Steel", "#9AA0A6", 1.0),
    ("black", "Black", "#1E1E1E", 1.0),
    ("white", "White", "#F4F2EE", 1.0),
    ("fixtureWhite", "Switch plate · white", "#FBFBF8", 1.0),
    ("fixtureInsert", "Socket insert", "#E3E3DD", 1.0),
    ("lampGlass", "Lamp glass", "#FFF2D0", 1.0),
];

/// Material id for a finish key (`brick` → `construction.brick`). Ids that are not finish keys
/// (a material created by the user) are returned as they are.
pub fn material_id(finish: &str) -> String {
    if MATERIALS.iter().any(|(k, ..)| *k == finish) { format!("construction.{finish}") } else { finish.to_string() }
}

// ---- small plan helpers ----------------------------------------------------------------

pub fn dist2(a: P2, b: P2) -> f64 { ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt() }
/// Signed plan area (x, z).
pub fn plan_area(pts: &[P2]) -> f64 {
    let n = pts.len();
    if n < 3 { return 0.0; }
    (0..n).map(|i| { let a = pts[i]; let b = pts[(i + 1) % n]; a[0] * b[1] - b[0] * a[1] }).sum::<f64>() / 2.0
}
pub fn bbox(pts: &[P2]) -> Option<(P2, P2)> {
    let first = *pts.first()?;
    Some(pts.iter().fold((first, first), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])))
}
/// Is the plan point inside the polygon (even-odd)?
pub fn point_in_plan(p: P2, poly: &[P2]) -> bool {
    let n = poly.len();
    let mut inside = false;
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] { inside = !inside; }
        j = i;
    }
    inside
}
pub fn finish_of(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string())
}
