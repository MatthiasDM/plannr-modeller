//! Data model of the framing plugin (`extensions.framing.defaults` + `framing.*` entities), lumber
//! conventions, and lenient readers for the construction plugin's rooms, openings and storeys.

use plannr_core::geom::P3;
use plannr_core::model::{Document, Entity, Layer, Material};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub type P2 = [f64; 2];

pub const PLUGIN_ID: &str = "framing";
pub const T_WALL: &str = "framing.wall";
pub const T_FLOOR: &str = "framing.floor";
pub const T_ROOF: &str = "framing.roof";
pub const T_FOUNDATION: &str = "framing.foundation";
pub const T_BEAM: &str = "framing.beam";
pub const T_STAIR: &str = "framing.stair";
pub const TYPES: [&str; 6] = [T_WALL, T_FLOOR, T_ROOF, T_FOUNDATION, T_BEAM, T_STAIR];
pub const LAYER_NAME: &str = "Framing";

pub const MATERIALS: [(&str, &str, &str); 11] = [
    ("lumber", "Framing lumber", "#E3C597"), ("plate", "Plates", "#D8B784"), ("header", "Headers", "#BF9552"),
    ("sheathing", "Sheathing (OSB)", "#D9B58A"), ("gypsum", "Gypsum board", "#F1F0EC"), ("concrete", "Concrete", "#A9A6A0"), ("steel", "Steel", "#8E9398"),
    ("shingles", "Roof shingles", "#5B5B5E"), ("metal", "Metal roofing", "#7B8794"), ("tiles", "Roof tiles", "#9C4A33"), ("soffit", "Soffit", "#EDEAE3"),
];
pub fn material_id(key: &str) -> String { format!("framing.{key}") }

/// Lumber thickness (the 38 mm face) in metres.
pub const T: f64 = 0.038;
/// Stock lengths used by the takeoff (8', 10', 12', 14', 16').
pub const STOCK: [f64; 5] = [2.44, 3.05, 3.66, 4.27, 4.88];
/// Sheet size for sheathing / gypsum panels (4' × 8').
pub const PANEL_AREA: f64 = 1.22 * 2.44;

/// Nominal size → actual `[thickness, depth]` in metres. Accepts "2x4"-style and metric "38x140".
pub fn lumber(size: &str) -> Option<[f64; 2]> {
    let s = size.trim().to_lowercase().replace('×', "x").replace(' ', "");
    Some(match s.as_str() {
        "2x3" => [0.038, 0.064], "2x4" => [0.038, 0.089], "2x6" => [0.038, 0.140], "2x8" => [0.038, 0.184], "2x10" => [0.038, 0.235], "2x12" => [0.038, 0.286],
        "4x4" => [0.089, 0.089], "4x6" => [0.089, 0.140], "6x6" => [0.140, 0.140],
        _ => {
            let (a, b) = s.split_once('x')?;
            let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
            // metric millimetres (38x140) or metres (0.038x0.14); anything else is not a size
            if a >= 10.0 && b >= 10.0 { [a / 1000.0, b / 1000.0] } else if a > 0.0 && a < 1.0 && b > 0.0 && b < 1.0 { [a, b] } else { return None; }
        }
    })
}
pub fn lumber_or(size: &str, fallback: &str) -> [f64; 2] { lumber(size).or_else(|| lumber(fallback)).unwrap_or([0.038, 0.089]) }
/// Header size by clear span (built-up 2-ply).
pub fn header_size_for_span(span: f64) -> &'static str {
    if span <= 1.2 { "2x6" } else if span <= 1.8 { "2x8" } else if span <= 2.4 { "2x10" } else { "2x12" }
}
pub fn stock_length(l: f64) -> f64 { STOCK.iter().copied().find(|s| *s + 1e-9 >= l).unwrap_or_else(|| (l / STOCK[0]).ceil() * STOCK[0]) }

// ---- settings ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sheet { #[serde(default = "d_osb")] pub kind: String, #[serde(default = "d_osb_t")] pub thickness: f64, #[serde(default = "d_true")] pub enabled: bool }
fn d_osb() -> String { "osb".into() } fn d_osb_t() -> f64 { 0.011 } fn d_true() -> bool { true } fn d_false() -> bool { false }
impl Default for Sheet { fn default() -> Self { Sheet { kind: "osb".into(), thickness: 0.011, enabled: true } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sheathing { #[serde(default)] pub exterior: Sheet, #[serde(default = "d_gypsum")] pub interior: Sheet }
fn d_gypsum() -> Sheet { Sheet { kind: "gypsum".into(), thickness: 0.0125, enabled: false } }
impl Default for Sheathing { fn default() -> Self { Sheathing { exterior: Sheet::default(), interior: d_gypsum() } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Defaults {
    #[serde(default = "d_metric")] pub units: String,
    #[serde(default = "d_2x4")] pub stud_size: String,
    #[serde(default = "d_400")] pub stud_spacing: f64,
    #[serde(default = "d_2x4")] pub plate_size: String,
    #[serde(default = "d_2")] pub top_plates: u32,
    #[serde(default = "d_1")] pub bottom_plates: u32,
    #[serde(default)] pub sheathing: Sheathing,
    #[serde(default = "d_span")] pub header_rule: String,
    /// Details new walls get at L corners and T intersections.
    #[serde(default = "d_california")] pub corner_detail: String,
    #[serde(default = "d_two_stud")] pub t_detail: String,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_california() -> String { "california".into() }
fn d_metric() -> String { "metric".into() } fn d_2x4() -> String { "2x4".into() } fn d_400() -> f64 { 0.4 } fn d_2() -> u32 { 2 } fn d_1() -> u32 { 1 } fn d_span() -> String { "span".into() }
impl Default for Defaults {
    fn default() -> Self { Defaults { units: "metric".into(), stud_size: "2x4".into(), stud_spacing: 0.4, plate_size: "2x4".into(), top_plates: 2, bottom_plates: 1, sheathing: Sheathing::default(), header_rule: "span".into(), corner_detail: "california".into(), t_detail: "twoStud".into(), extra: Map::new() } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings { #[serde(default)] pub defaults: Defaults, #[serde(flatten, default)] pub extra: Map<String, Value> }

pub fn settings(doc: &Document) -> Settings { doc.extensions.get(PLUGIN_ID).cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default() }
pub fn save_settings(doc: &mut Document, s: &Settings) { doc.extensions.insert(PLUGIN_ID.to_string(), serde_json::to_value(s).unwrap_or(Value::Null)); }

// ---- entities ------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Opening {
    #[serde(default = "new_id")] pub id: String,
    #[serde(default = "d_window")] pub kind: String,
    /// From `a` to the start of the rough opening (m).
    pub offset: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)] pub sill: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub header_size: Option<String>,
    #[serde(default = "d_1")] pub king_studs: u32,
    #[serde(default = "d_1")] pub jack_studs: u32,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_window() -> String { "window".into() }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Wall {
    #[serde(default)] pub level: usize,
    pub a: P2,
    pub b: P2,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub height: Option<f64>,
    #[serde(default = "d_2x4")] pub stud_size: String,
    #[serde(default = "d_400")] pub stud_spacing: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub plate_size: Option<String>,
    #[serde(default = "d_2")] pub top_plates: u32,
    #[serde(default = "d_1")] pub bottom_plates: u32,
    #[serde(default)] pub sheathing: Sheathing,
    #[serde(default = "d_false")] pub flip: bool,
    #[serde(default)] pub openings: Vec<Opening>,
    #[serde(default = "d_true")] pub start_stud: bool,
    #[serde(default = "d_true")] pub end_stud: bool,
    /// L-corner detail where this wall runs through: `california` (flat stud), `threeStud`, `twoStud` / `none`.
    #[serde(default = "d_none")] pub corner_detail: String,
    /// What the drawn line a→b is: the `exterior` face of the studs (new walls), the `center` line
    /// (walls framed from construction rooms, older documents) or the `interior` face.
    #[serde(default = "d_center")] pub reference: String,
    /// Partition backer where another wall butts into this one: `twoStud`, `ladder`, `california`, `none`.
    #[serde(default = "d_two_stud")] pub t_detail: String,
    /// Junction at each end: `auto` (found from the other walls), `through`, `butt`, `none`.
    #[serde(default = "d_auto")] pub start_join: String,
    #[serde(default = "d_auto")] pub end_join: String,
    /// Set by `frame_room`: the construction room edge this wall frames.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub room: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub edge: Option<usize>,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_none() -> String { "none".into() }
fn d_center() -> String { "center".into() }
fn d_two_stud() -> String { "twoStud".into() }
impl Wall {
    pub fn length(&self) -> f64 { ((self.b[0] - self.a[0]).powi(2) + (self.b[1] - self.a[1]).powi(2)).sqrt() }
    pub fn stud(&self) -> [f64; 2] { lumber_or(&self.stud_size, "2x4") }
    pub fn plate(&self) -> [f64; 2] { self.plate_size.as_deref().map(|p| lumber_or(p, &self.stud_size)).unwrap_or_else(|| self.stud()) }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Subfloor { #[serde(default = "d_sub_t")] pub thickness: f64, #[serde(default = "d_true")] pub enabled: bool }
fn d_sub_t() -> f64 { 0.018 }
impl Default for Subfloor { fn default() -> Self { Subfloor { thickness: 0.018, enabled: true } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Floor {
    #[serde(default)] pub level: usize,
    pub points: Vec<P2>,
    #[serde(default = "d_2x10")] pub joist_size: String,
    #[serde(default = "d_400")] pub joist_spacing: f64,
    #[serde(default = "d_auto")] pub direction: String,
    #[serde(default = "d_true")] pub rim_joist: bool,
    #[serde(default = "d_false")] pub blocking: bool,
    #[serde(default)] pub subfloor: Subfloor,
    #[serde(default)] pub elevation: f64,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_2x10() -> String { "2x10".into() } fn d_auto() -> String { "auto".into() }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RoofSheathing { #[serde(default = "d_false")] pub enabled: bool, #[serde(default = "d_roof_t")] pub thickness: f64 }
fn d_roof_t() -> f64 { 0.012 }
impl Default for RoofSheathing { fn default() -> Self { RoofSheathing { enabled: false, thickness: 0.012 } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Roof {
    #[serde(default)] pub level: usize,
    pub min: P2,
    pub max: P2,
    #[serde(rename = "type", default = "d_fink")] pub type_: String,
    /// Rise per 12 (6 = 6:12 ≈ 26.6°); `pitchDeg` wins when given.
    #[serde(default = "d_pitch")] pub pitch: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub pitch_deg: Option<f64>,
    #[serde(default = "d_true")] pub ridge_along_x: bool,
    #[serde(default = "d_overhang")] pub overhang: f64,
    #[serde(default = "d_heel")] pub heel: f64,
    #[serde(default = "d_600")] pub spacing: f64,
    #[serde(default = "d_2x4")] pub chord_size: String,
    #[serde(default = "d_2x4")] pub web_size: String,
    #[serde(default = "d_2x8")] pub rafter_size: String,
    #[serde(default = "d_2x10")] pub ridge_board: String,
    #[serde(default = "d_true")] pub gable_ends: bool,
    #[serde(default)] pub sheathing: RoofSheathing,
    /// gable | hip | shed | gambrel
    #[serde(default = "d_gable")] pub shape: String,
    /// Shed: which side of the rectangle (across the span) is high: `max` or `min`.
    #[serde(default = "d_max")] pub high_side: String,
    /// Shed rafters: what carries their high end — a stud `ponyWall` on the wall (default) or a `ledger`
    /// on an existing building wall (lean-to).
    #[serde(default = "d_pony")] pub high_support: String,
    /// Gambrel: rise per 12 of the steep lower slopes.
    #[serde(default = "d_lower_pitch")] pub lower_pitch: f64,
    /// Gambrel: plan position of the knee as a fraction of the span from each eave.
    #[serde(default = "d_break")] pub break_ratio: f64,
    /// Scissor trusses: rise per 12 of the bottom chords (default half the pitch).
    #[serde(default, skip_serializing_if = "Option::is_none")] pub bottom_pitch: Option<f64>,
    /// Attic trusses: clear width of the room between the knee walls (default half the span).
    #[serde(default, skip_serializing_if = "Option::is_none")] pub attic_width: Option<f64>,
    /// Attic trusses: ceiling (collar) height above the floor joist.
    #[serde(default = "d_attic_h")] pub attic_height: f64,
    /// Attic trusses: size of the floor-joist bottom chord.
    #[serde(default = "d_2x10")] pub floor_joist_size: String,
    /// Hip rafters (default one size deeper than the rafters).
    #[serde(default, skip_serializing_if = "Option::is_none")] pub hip_rafter_size: Option<String>,
    #[serde(default)] pub fascia: Fascia,
    #[serde(default)] pub soffit: Soffit,
    #[serde(default)] pub cladding: Cladding,
    #[serde(default)] pub gutters: bool,
    #[serde(default)] pub dormers: Vec<Dormer>,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_gable() -> String { "gable".into() } fn d_max() -> String { "max".into() } fn d_lower_pitch() -> f64 { 18.0 } fn d_break() -> f64 { 0.25 } fn d_attic_h() -> f64 { 2.3 }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Fascia { #[serde(default = "d_true")] pub enabled: bool, #[serde(default = "d_2x6")] pub size: String }
fn d_2x6() -> String { "2x6".into() }
impl Default for Fascia { fn default() -> Self { Fascia { enabled: true, size: "2x6".into() } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Soffit { #[serde(default = "d_false")] pub enabled: bool, #[serde(default = "d_soffit_t")] pub thickness: f64 }
fn d_soffit_t() -> f64 { 0.012 }
impl Default for Soffit { fn default() -> Self { Soffit { enabled: false, thickness: 0.012 } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cladding { #[serde(default = "d_none_kind")] pub kind: String, #[serde(default, skip_serializing_if = "Option::is_none")] pub thickness: Option<f64> }
fn d_none_kind() -> String { "none".into() }
impl Default for Cladding { fn default() -> Self { Cladding { kind: "none".into(), thickness: None } } }
impl Cladding {
    pub fn depth(&self) -> f64 { self.thickness.unwrap_or(match self.kind.as_str() { "tiles" => 0.03, "metal" => 0.003, _ => 0.006 }) }
}

/// A dormer on a gable roof slope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Dormer {
    #[serde(default = "new_id")] pub id: String,
    /// `l` / `r` slope (`front` = `l`, `back` = `r`).
    #[serde(default = "d_side_l")] pub side: String,
    /// Along the ridge from the roof's min end to the dormer's left edge (m).
    pub offset: f64,
    pub width: f64,
    /// Front wall height above the main roof framing at the front wall line (m).
    #[serde(default = "d_dormer_wall")] pub wall_height: f64,
    /// Rise per 12 of the dormer roof.
    #[serde(default = "d_dormer_pitch")] pub pitch: f64,
    /// gable | shed
    #[serde(default = "d_gable")] pub kind: String,
    /// Distance of the front wall from the exterior wall line, inward (m); 0 = a wall dormer.
    #[serde(default)] pub setback: f64,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_side_l() -> String { "l".into() } fn d_dormer_wall() -> f64 { 1.0 } fn d_dormer_pitch() -> f64 { 8.0 }
fn d_fink() -> String { "fink".into() } fn d_pitch() -> f64 { 6.0 } fn d_overhang() -> f64 { 0.45 } fn d_heel() -> f64 { 0.0 }
fn d_pony() -> String { "ponyWall".into() } fn d_600() -> f64 { 0.6 } fn d_2x8() -> String { "2x8".into() }
impl Roof {
    /// Slope as rise / run.
    pub fn slope(&self) -> f64 { match self.pitch_deg { Some(d) => d.to_radians().tan(), None => self.pitch / 12.0 } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Foundation {
    pub points: Vec<P2>,
    #[serde(rename = "type", default = "d_slab")] pub type_: String,
    #[serde(default = "d_slab_t")] pub slab_thickness: f64,
    #[serde(default = "d_stem_h")] pub stem_height: f64,
    #[serde(default = "d_stem_t")] pub stem_thickness: f64,
    #[serde(default = "d_foot_w")] pub footing_width: f64,
    #[serde(default = "d_foot_d")] pub footing_depth: f64,
    /// Stemwall only: pour a slab inside the stem walls too.
    #[serde(default = "d_true")] pub slab: bool,
    /// Elevation of the top of the foundation; default: storey 0 base.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub top: Option<f64>,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_slab() -> String { "slab".into() } fn d_slab_t() -> f64 { 0.15 } fn d_stem_h() -> f64 { 0.6 } fn d_stem_t() -> f64 { 0.2 } fn d_foot_w() -> f64 { 0.5 } fn d_foot_d() -> f64 { 0.2 }

// ---- what we read from the construction plugin --------------------------------------------------

#[derive(Debug, Clone)]
pub struct Rm { pub id: String, pub name: String, pub level: usize, pub points: Vec<P2> }
#[derive(Debug, Clone)]
pub struct Op { pub id: String, pub kind: String, pub room: Option<String>, pub edge: Option<usize>, pub along: f64, pub width: f64, pub height: f64, pub sill: f64 }

#[derive(Debug, Clone, Default)]
pub struct House {
    pub rooms: Vec<Rm>, pub openings: Vec<Op>, pub storeys: Vec<(String, f64)>, pub ground_level: f64, pub floor_thickness: f64,
    /// Species, loads, hardware switches (`extensions.framing.defaults`).
    pub engineering: Engineering,
    /// The framing floors of the document (joist hangers on beams need them).
    pub floors: Vec<Floor>,
    /// Every framing wall of the document (for junctions).
    pub walls: Vec<(String, Wall)>,
}

fn p2s(v: Option<&Value>) -> Vec<P2> {
    v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|p| { let q = p.as_array()?; Some([q.first()?.as_f64()?, q.get(1)?.as_f64()?]) }).collect()).unwrap_or_default()
}
pub fn p2(v: Option<&Value>) -> Option<P2> { let a = v?.as_array()?; Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?]) }
fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }
fn f(v: &Value, k: &str, d: f64) -> f64 { v.get(k).and_then(|x| x.as_f64()).unwrap_or(d) }

impl House {
    pub fn read(doc: &Document) -> House {
        let mut h = House::default();
        if let Some(c) = doc.extensions.get("construction") {
            h.storeys = c.get("storeys").and_then(|v| v.as_array()).map(|a| a.iter().map(|st| (s(st, "name"), f(st, "height", 2.6))).collect()).unwrap_or_default();
            h.ground_level = f(c, "groundLevel", 0.0);
            h.floor_thickness = f(c, "floorThickness", 0.06);
        }
        if h.storeys.is_empty() { h.storeys.push(("Ground floor".into(), 2.6)); }
        h.engineering = Engineering::of(&settings(doc).defaults);
        for e in &doc.entities {
            let Entity::Custom(c) = e else { continue };
            let d = &c.data;
            match c.type_.as_str() {
                T_FLOOR => if let Ok(f) = serde_json::from_value::<Floor>(d.clone()) { h.floors.push(f); },
                "construction.room" => h.rooms.push(Rm { id: c.id.clone(), name: s(d, "name"), level: d.get("level").and_then(|x| x.as_u64()).unwrap_or(0) as usize, points: p2s(d.get("points")) }),
                T_WALL => { if let Ok(w) = serde_json::from_value::<Wall>(d.clone()) { h.walls.push((c.id.clone(), w)); } }
                "construction.opening" => {
                    let host = d.get("host").cloned().unwrap_or(Value::Null);
                    h.openings.push(Op { id: c.id.clone(), kind: s(d, "kind"), room: host.get("room").and_then(|x| x.as_str()).map(|x| x.to_string()), edge: host.get("edge").and_then(|x| x.as_u64()).map(|x| x as usize), along: f(d, "along", 0.0), width: f(d, "width", 0.9), height: f(d, "height", 2.05), sill: f(d, "sill", 0.0) });
                }
                _ => {}
            }
        }
        h
    }
    pub fn base(&self, level: usize) -> f64 { self.ground_level + self.storeys.iter().take(level).map(|s| s.1).sum::<f64>() }
    pub fn height(&self, level: usize) -> f64 { self.storeys.get(level).map(|s| s.1).or_else(|| self.storeys.last().map(|s| s.1)).unwrap_or(2.6) }
    pub fn room(&self, id: &str) -> Option<&Rm> { self.rooms.iter().find(|r| r.id == id) }
    /// Is edge `i` of `room` shared (collinear and overlapping) with an edge of another room on the same level?
    pub fn edge_is_shared(&self, room: &Rm, i: usize) -> bool {
        let n = room.points.len(); if n < 2 { return false; }
        let (a, b) = (room.points[i], room.points[(i + 1) % n]);
        for other in self.rooms.iter().filter(|r| r.id != room.id && r.level == room.level) {
            let m = other.points.len();
            for j in 0..m {
                let (c, d) = (other.points[j], other.points[(j + 1) % m]);
                if collinear_overlap(a, b, c, d) { return true; }
            }
        }
        false
    }
}

/// Two plan segments on the same line (within 1 mm / 0.1°) that overlap by more than 1 mm.
pub fn collinear_overlap(a: P2, b: P2, c: P2, d: P2) -> bool {
    let u = [b[0] - a[0], b[1] - a[1]]; let lu = (u[0] * u[0] + u[1] * u[1]).sqrt(); if lu < 1e-9 { return false; }
    let v = [d[0] - c[0], d[1] - c[1]]; let lv = (v[0] * v[0] + v[1] * v[1]).sqrt(); if lv < 1e-9 { return false; }
    let (ux, uz) = (u[0] / lu, u[1] / lu);
    let cross = (ux * v[1] - uz * v[0]).abs() / lv; if cross > 0.0017 { return false; }
    let off = |p: P2| ((p[0] - a[0]) * uz - (p[1] - a[1]) * ux).abs();
    if off(c) > 1e-3 || off(d) > 1e-3 { return false; }
    let t = |p: P2| (p[0] - a[0]) * ux + (p[1] - a[1]) * uz;
    let (t0, t1) = (t(c).min(t(d)), t(c).max(t(d)));
    t1.min(lu) - t0.max(0.0) > 1e-3
}

// ---- persistence helpers ------------------------------------------------------------------------------

pub fn new_id() -> String { uuid::Uuid::new_v4().to_string() }
pub fn has_framing(doc: &Document) -> bool { doc.entities.iter().any(|e| matches!(e, Entity::Custom(c) if TYPES.contains(&c.type_.as_str()))) }

pub fn ensure(doc: &mut Document) {
    if !doc.extensions.contains_key(PLUGIN_ID) { save_settings(doc, &Settings::default()); }
    for (key, name, color) in MATERIALS {
        let id = material_id(key);
        if !doc.materials.iter().any(|m| m.id == id) { doc.materials.push(Material { id, name: name.to_string(), color: color.to_string(), opacity: 1.0 }); }
    }
}
/// The "Framing" layer, created on demand.
pub fn framing_layer(doc: &mut Document) -> String {
    if let Some(l) = doc.layers.iter().find(|l| l.name == LAYER_NAME) { return l.id.clone(); }
    let id = new_id();
    doc.layers.push(Layer { id: id.clone(), name: LAYER_NAME.into(), visible: true, color: "#c98a3a".into() });
    id
}
pub fn to_p3(p: P2, y: f64) -> P3 { [p[0], y, p[1]] }

// ---- beams and stairs ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Beam {
    #[serde(default)] pub level: usize,
    pub a: P2,
    pub b: P2,
    /// Top of the beam above the storey base (default: the storey height, i.e. a dropped beam under the floor above).
    #[serde(default, skip_serializing_if = "Option::is_none")] pub elevation: Option<f64>,
    #[serde(default = "d_2x10")] pub size: String,
    #[serde(default = "d_2")] pub plies: u32,
    /// lumber (built-up plies of `size`), lvl, glulam (one piece `width` × `depth`).
    #[serde(default = "d_lumber")] pub material: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub depth: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub width: Option<f64>,
    #[serde(default = "d_true")] pub posts: bool,
    /// Extra posts between the ends at most this far apart (m).
    #[serde(default, skip_serializing_if = "Option::is_none")] pub post_spacing: Option<f64>,
    #[serde(default = "d_4x4")] pub post_size: String,
    /// Load width carried by the beam (m); computed from the floors above when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub tributary: Option<f64>,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_lumber() -> String { "lumber".into() } fn d_4x4() -> String { "4x4".into() }
impl Beam {
    pub fn length(&self) -> f64 { ((self.b[0] - self.a[0]).powi(2) + (self.b[1] - self.a[1]).powi(2)).sqrt() }
    /// Cross-section (width, depth) in metres.
    pub fn section(&self) -> [f64; 2] {
        let [t, d] = lumber_or(&self.size, "2x10");
        let plies = self.plies.clamp(1, 4) as f64;
        match self.material.as_str() {
            "lvl" | "glulam" => [self.width.unwrap_or(t * plies), self.depth.unwrap_or(d)],
            _ => [t * plies, d],
        }
    }
    /// Plan positions (distance from `a`) of the supports: ends plus intermediate posts.
    pub fn supports(&self) -> Vec<f64> {
        let l = self.length();
        let mut v = vec![0.0];
        if let Some(sp) = self.post_spacing.filter(|s| *s > 0.3) {
            let n = (l / sp).ceil().max(1.0) as usize;
            for k in 1..n { v.push(l * k as f64 / n as f64); }
        }
        v.push(l);
        v
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stair {
    #[serde(default)] pub level: usize,
    /// Plan point at the foot of the run (middle of the width).
    pub a: P2,
    /// Plan direction of travel going up (normalised by the ops).
    #[serde(default = "d_dir")] pub direction: P2,
    #[serde(default = "d_stair_w")] pub width: f64,
    /// Floor to floor (default: the storey height).
    #[serde(default, skip_serializing_if = "Option::is_none")] pub total_rise: Option<f64>,
    /// Wanted riser height; the count is rounded so all risers are equal. Default: ceil(rise / 0.18) risers.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub riser_height: Option<f64>,
    #[serde(default = "d_tread")] pub tread_depth: f64,
    #[serde(default = "d_3")] pub stringers: u32,
    #[serde(default = "d_2x12")] pub stringer_size: String,
    #[serde(default = "d_tread_t")] pub tread_thickness: f64,
    #[serde(default = "d_true")] pub risers: bool,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_dir() -> P2 { [1.0, 0.0] } fn d_stair_w() -> f64 { 0.9 } fn d_tread() -> f64 { 0.26 } fn d_3() -> u32 { 3 } fn d_2x12() -> String { "2x12".into() } fn d_tread_t() -> f64 { 0.032 }
impl Stair {
    pub fn rise(&self, house: &House) -> f64 { self.total_rise.unwrap_or_else(|| house.height(self.level)) }
    /// (number of risers, riser height)
    pub fn risers_of(&self, house: &House) -> (usize, f64) {
        let rise = self.rise(house).max(0.1);
        let n = match self.riser_height { Some(r) if r > 0.05 => (rise / r).round().max(1.0), _ => (rise / 0.18 - 1e-9).ceil().max(1.0) } as usize;
        (n, rise / n as f64)
    }
    pub fn dir(&self) -> P2 { let l = (self.direction[0].powi(2) + self.direction[1].powi(2)).sqrt(); if l < 1e-9 { [1.0, 0.0] } else { [self.direction[0] / l, self.direction[1] / l] } }
}

// ---- engineering defaults (read from `defaults` extra keys so the Defaults struct stays as it is) -----------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Loads {
    /// Imposed floor load (kN/m², EN 1991-1-1 category A: 2.0).
    #[serde(default = "d_fl_live")] pub floor_live: f64,
    /// Floor finishes + boarding (kN/m²).
    #[serde(default = "d_fl_dead")] pub floor_dead: f64,
    /// Snow on the roof, on plan (kN/m²; Belgium s ≈ 0.5–0.8).
    #[serde(default = "d_snow")] pub roof_snow: f64,
    /// Roofing + battens + insulation, along the slope (kN/m²).
    #[serde(default = "d_roof_dead")] pub roof_dead: f64,
}
fn d_fl_live() -> f64 { 2.0 } fn d_fl_dead() -> f64 { 0.5 } fn d_snow() -> f64 { 0.8 } fn d_roof_dead() -> f64 { 0.6 }
impl Default for Loads { fn default() -> Self { Loads { floor_live: 2.0, floor_dead: 0.5, roof_snow: 0.8, roof_dead: 0.6 } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Deflection { #[serde(default = "d_360")] pub floor: f64, #[serde(default = "d_240")] pub roof: f64 }
fn d_360() -> f64 { 360.0 } fn d_240() -> f64 { 240.0 }
impl Default for Deflection { fn default() -> Self { Deflection { floor: 360.0, roof: 240.0 } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HardwareDefaults {
    #[serde(default = "d_true")] pub anchor_bolts: bool,
    #[serde(default = "d_false")] pub hold_downs: bool,
    #[serde(default = "d_true")] pub hurricane_ties: bool,
    #[serde(default = "d_true")] pub joist_hangers: bool,
}
impl Default for HardwareDefaults { fn default() -> Self { HardwareDefaults { anchor_bolts: true, hold_downs: false, hurricane_ties: true, joist_hangers: true } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Engineering {
    #[serde(default = "d_c24")] pub species: String,
    #[serde(default)] pub loads: Loads,
    #[serde(default)] pub deflection: Deflection,
    #[serde(default)] pub hardware: HardwareDefaults,
    /// Allowable soil bearing pressure (kPa).
    #[serde(default = "d_soil")] pub soil_bearing: f64,
}
fn d_c24() -> String { "C24".into() } fn d_soil() -> f64 { 100.0 }
impl Default for Engineering { fn default() -> Self { Engineering { species: "C24".into(), loads: Loads::default(), deflection: Deflection::default(), hardware: HardwareDefaults::default(), soil_bearing: 100.0 } } }
impl Engineering {
    pub fn of(d: &Defaults) -> Engineering { serde_json::from_value(Value::Object(d.extra.clone())).unwrap_or_default() }
}
