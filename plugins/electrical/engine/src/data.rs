//! Data model of the electrical plugin (`extensions.electrical` + `electrical.conduit` entities)
//! and lenient readers for the construction plugin's entities it relies on.

use plannr_core::geom::P3;
pub type P2 = [f64; 2];
use plannr_core::model::{Document, Entity, Material};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const PLUGIN_ID: &str = "electrical";
pub const T_CONDUIT: &str = "electrical.conduit";

pub const CIRCUIT_TYPES: [&str; 16] = ["lighting", "sockets", "kitchen", "oven", "hob", "dishwasher", "washingMachine", "dryer", "boiler", "heatPump", "evCharger", "bathroom", "outdoor", "pv", "dedicated", "other"];
pub const RCD_KINDS: [&str; 3] = ["none", "mA30", "mA300"];
/// Fixture kinds of the construction plugin that belong to the electrical installation.
pub const ELECTRICAL_KINDS: [&str; 6] = ["outlet", "switchPlate", "ceilingLight", "wallLight", "panel", "dataPoint"];
/// Load points (AREI counts sockets, data and light points; switches are not points).
pub const POINT_KINDS: [&str; 4] = ["outlet", "dataPoint", "ceilingLight", "wallLight"];

pub const MATERIALS: [(&str, &str, &str); 2] = [("conduit", "Conduit", "#B8BCC2"), ("cable", "Surface cable", "#5A5F66")];
pub fn material_id(key: &str) -> String { format!("electrical.{key}") }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    #[serde(default = "d_phases")] pub phases: u32,
    #[serde(default = "d_main")] pub main_breaker: u32,
    /// Head differential in mA (300 or 100); 0 = none.
    #[serde(default = "d_diff")] pub main_differential: u32,
    #[serde(default = "d_a")] pub differential_type: String,
    #[serde(default = "d_tt")] pub earthing: String,
    #[serde(default = "d_ka")] pub short_circuit_ka: f64,
    #[serde(default = "d_cable")] pub default_cable_type: String,
    #[serde(default)] pub address: String,
    #[serde(default)] pub ean: String,
    #[serde(default)] pub meter_number: String,
    /// Fixture id of the construction `panel` the circuits leave from.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub panel: Option<String>,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_phases() -> u32 { 1 }
fn d_main() -> u32 { 40 }
fn d_diff() -> u32 { 300 }
fn d_a() -> String { "A".into() }
fn d_tt() -> String { "TT".into() }
fn d_ka() -> f64 { 3.0 }
fn d_cable() -> String { "XVB".into() }
impl Default for Board {
    fn default() -> Self { Board { phases: 1, main_breaker: 40, main_differential: 300, differential_type: "A".into(), earthing: "TT".into(), short_circuit_ka: 3.0, default_cable_type: "XVB".into(), address: String::new(), ean: String::new(), meter_number: String::new(), panel: None, extra: Map::new() } }
}
impl Board {
    pub fn voltage(&self) -> f64 { if self.phases == 3 { 400.0 } else { 230.0 } }
    /// Board capacity in kW.
    pub fn capacity_kw(&self) -> f64 { self.main_breaker as f64 * self.voltage() * if self.phases == 3 { 3f64.sqrt() } else { 1.0 } / 1000.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Circuit {
    pub id: String,
    #[serde(default)] pub number: u32,
    #[serde(default)] pub name: String,
    #[serde(rename = "type", default = "d_sockets")] pub type_: String,
    #[serde(default = "d_20")] pub breaker: u32,
    #[serde(default = "d_25")] pub cable_section: f64,
    #[serde(default = "d_ma30")] pub rcd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub poles: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub curve: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub cable_type: Option<String>,
    #[serde(default)] pub points: Vec<String>,
    #[serde(default)] pub note: String,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_sockets() -> String { "sockets".into() }
fn d_20() -> u32 { 20 }
fn d_25() -> f64 { 2.5 }
fn d_ma30() -> String { "mA30".into() }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RcdGroup {
    pub id: String,
    #[serde(default)] pub name: String,
    #[serde(default = "d_30")] pub milliamps: u32,
    #[serde(default = "d_a")] pub type_: String,
    #[serde(default)] pub circuits: Vec<String>,
}
fn d_30() -> u32 { 30 }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)] pub board: Board,
    #[serde(default)] pub circuits: Vec<Circuit>,
    #[serde(default)] pub rcd_groups: Vec<RcdGroup>,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Conduit {
    #[serde(default)] pub level: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub circuit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub to: Option<String>,
    /// Polyline, heights above the storey floor.
    #[serde(default)] pub points: Vec<P3>,
    #[serde(default = "d_dia")] pub diameter: f64,
    #[serde(default = "d_kind")] pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub cable_type: Option<String>,
    /// Created by `auto_route` (replaced on the next run).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")] pub auto: bool,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_dia() -> f64 { 0.020 }
fn d_kind() -> String { "conduit".into() }
impl Conduit {
    pub fn length(&self) -> f64 { self.points.windows(2).map(|w| plannr_core::geom::len(plannr_core::geom::sub(w[1], w[0]))).sum() }
}

// ---- circuit-type conventions (AREI / Studio) ---------------------------------------------------

pub struct TypeInfo { pub label: &'static str, pub breaker: u32, pub section: f64, pub dedicated: bool, pub max_points: u32, pub nominal_kw: f64, pub needs_rcd30: bool }

pub fn type_info(t: &str) -> TypeInfo {
    let (label, breaker, section, dedicated, max_points, nominal_kw, needs_rcd30) = match t {
        "lighting" => ("Lighting", 16, 1.5, false, 8, 0.0, false),
        "sockets" => ("Sockets", 20, 2.5, false, 8, 0.0, true),
        "kitchen" => ("Kitchen sockets", 20, 2.5, false, 8, 0.0, true),
        "oven" => ("Oven", 20, 2.5, true, 1, 2.3, true),
        "hob" => ("Hob", 32, 6.0, true, 1, 7.4, true),
        "dishwasher" => ("Dishwasher", 16, 2.5, true, 1, 2.2, true),
        "washingMachine" => ("Washing machine", 16, 2.5, true, 1, 2.3, true),
        "dryer" => ("Tumble dryer", 16, 2.5, true, 1, 2.7, true),
        "boiler" => ("Boiler / water heater", 16, 2.5, true, 1, 2.2, true),
        "heatPump" => ("Heat pump", 20, 2.5, true, 1, 3.5, true),
        "evCharger" => ("EV charger", 32, 6.0, true, 1, 7.4, true),
        "bathroom" => ("Bathroom", 20, 2.5, false, 6, 0.0, true),
        "outdoor" => ("Outdoor", 20, 2.5, false, 6, 0.0, true),
        "pv" => ("PV inverter", 20, 4.0, true, 1, 0.0, false),
        "dedicated" => ("Dedicated", 20, 2.5, true, 1, 2.3, false),
        _ => ("Other", 20, 2.5, false, 6, 0.0, false),
    };
    TypeInfo { label, breaker, section, dedicated, max_points, nominal_kw, needs_rcd30 }
}
pub fn type_label(t: &str) -> &'static str { type_info(t).label }

/// Largest breaker AREI allows on a cable section (Studio's CableSizing).
pub fn max_breaker(section: f64) -> u32 {
    if section < 2.5 { 16 } else if section < 4.0 { 20 } else if section < 6.0 { 25 } else if section < 10.0 { 32 } else if section < 16.0 { 40 } else { 63 }
}
pub fn section_text(s: f64) -> String { if (s - s.round()).abs() < 1e-9 { format!("{}", s as i64) } else { format!("{s:.1}") } }

impl Circuit {
    pub fn resolved_poles(&self, phases: u32) -> u32 {
        self.poles.unwrap_or(if phases == 3 && ["hob", "heatPump", "evCharger"].contains(&self.type_.as_str()) && self.breaker >= 20 { 4 } else { 2 })
    }
    pub fn resolved_curve(&self) -> String {
        self.curve.clone().unwrap_or_else(|| if ["hob", "oven", "washingMachine", "dryer", "dishwasher", "heatPump", "evCharger", "boiler", "pv"].contains(&self.type_.as_str()) { "C".into() } else { "B".into() })
    }
    /// Belgian cable notation, e.g. "XVB 3G2.5".
    pub fn cable_label(&self, board: &Board) -> String {
        let conductors = if self.resolved_poles(board.phases) == 4 { 5 } else { 3 };
        format!("{} {conductors}G{}", self.cable_type.clone().unwrap_or_else(|| board.default_cable_type.clone()), section_text(self.cable_section))
    }
    /// Estimated demand in kW (Studio's rule).
    pub fn load_kw(&self, point_count: usize) -> f64 {
        let t = type_info(&self.type_);
        if t.nominal_kw > 0.0 { return t.nominal_kw; }
        match self.type_.as_str() { "lighting" => point_count as f64 * 0.1, "pv" => 0.0, _ => point_count as f64 * 0.3 }
    }
    pub fn rcd_label(&self) -> &str { match self.rcd.as_str() { "mA30" => "30 mA", "mA300" => "300 mA", _ => "—" } }
}

// ---- what we read from the construction plugin ------------------------------------------------

#[derive(Debug, Clone)]
pub struct Fx { pub id: String, pub kind: String, pub level: usize, pub position: P3, pub rotation: f64, pub note: String }
#[derive(Debug, Clone)]
pub struct Rm { pub id: String, pub name: String, pub level: usize, pub points: Vec<P2> }
#[derive(Debug, Clone)]
pub struct Wl { pub level: usize, pub a: P2, pub b: P2 }
#[derive(Debug, Clone)]
pub struct UnitRef { pub name: String, pub type_: String }

#[derive(Debug, Clone, Default)]
pub struct House {
    pub fixtures: Vec<Fx>,
    pub rooms: Vec<Rm>,
    pub walls: Vec<Wl>,
    pub units: Vec<UnitRef>,
    pub storeys: Vec<(String, f64)>,
    pub ground_level: f64,
}

fn p3(v: Option<&Value>) -> Option<P3> {
    let a = v?.as_array()?; if a.len() < 3 { return None; }
    Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?])
}
fn p2s(v: Option<&Value>) -> Vec<P2> {
    v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|p| { let q = p.as_array()?; Some([q.first()?.as_f64()?, q.get(1)?.as_f64()?]) }).collect()).unwrap_or_default()
}
fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }
fn lvl(v: &Value) -> usize { v.get("level").and_then(|x| x.as_u64()).unwrap_or(0) as usize }

impl House {
    pub fn read(doc: &Document) -> House {
        let mut h = House::default();
        if let Some(c) = doc.extensions.get("construction") {
            h.storeys = c.get("storeys").and_then(|v| v.as_array()).map(|a| a.iter().map(|st| (s(st, "name"), st.get("height").and_then(|x| x.as_f64()).unwrap_or(2.6))).collect()).unwrap_or_default();
            h.ground_level = c.get("groundLevel").and_then(|v| v.as_f64()).unwrap_or(0.0);
        }
        if h.storeys.is_empty() { h.storeys.push(("Ground floor".into(), 2.6)); }
        for e in &doc.entities {
            let Entity::Custom(c) = e else { continue };
            let d = &c.data;
            match c.type_.as_str() {
                "construction.fixture" => if let Some(position) = p3(d.get("position")) {
                    h.fixtures.push(Fx { id: c.id.clone(), kind: s(d, "kind"), level: lvl(d), position, rotation: d.get("rotation").and_then(|x| x.as_f64()).unwrap_or(0.0), note: s(d, "note") });
                },
                "construction.room" => h.rooms.push(Rm { id: c.id.clone(), name: s(d, "name"), level: lvl(d), points: p2s(d.get("points")) }),
                "construction.wall" => { let a = p2s(Some(&Value::Array(vec![d.get("a").cloned().unwrap_or(Value::Null)]))); let b = p2s(Some(&Value::Array(vec![d.get("b").cloned().unwrap_or(Value::Null)]))); if let (Some(a), Some(b)) = (a.first(), b.first()) { h.walls.push(Wl { level: lvl(d), a: *a, b: *b }); } }
                "construction.unit" => h.units.push(UnitRef { name: s(d, "name"), type_: s(d, "type") }),
                _ => {}
            }
        }
        h
    }
    pub fn base(&self, level: usize) -> f64 { self.ground_level + self.storeys.iter().take(level).map(|s| s.1).sum::<f64>() }
    pub fn height(&self, level: usize) -> f64 { self.storeys.get(level).map(|s| s.1).or_else(|| self.storeys.last().map(|s| s.1)).unwrap_or(2.6) }
    pub fn storey_name(&self, level: usize) -> String { self.storeys.get(level).map(|s| s.0.clone()).unwrap_or_else(|| format!("Level {level}")) }
    pub fn fixture(&self, id: &str) -> Option<&Fx> { self.fixtures.iter().find(|f| f.id == id) }
    pub fn electrical_points(&self) -> Vec<&Fx> { self.fixtures.iter().filter(|f| POINT_KINDS.contains(&f.kind.as_str())).collect() }
    pub fn is_point(&self, id: &str) -> bool { self.fixture(id).map(|f| POINT_KINDS.contains(&f.kind.as_str())).unwrap_or(false) }
    pub fn panels(&self) -> Vec<&Fx> { self.fixtures.iter().filter(|f| f.kind == "panel").collect() }
    /// The room a plan point lies in on a storey.
    pub fn room_at(&self, level: usize, p: P2) -> Option<&Rm> {
        self.rooms.iter().filter(|r| r.level == level).find(|r| point_in_poly(p, &r.points))
    }
}

pub fn point_in_poly(p: P2, poly: &[P2]) -> bool {
    let mut inside = false; let n = poly.len();
    if n < 3 { return false; }
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1] + 1e-12) + a[0] { inside = !inside; }
        j = i;
    }
    inside
}

// ---- settings persistence -------------------------------------------------------------------------

pub fn settings(doc: &Document) -> Settings {
    doc.extensions.get(PLUGIN_ID).cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}
pub fn save_settings(doc: &mut Document, s: &Settings) {
    doc.extensions.insert(PLUGIN_ID.to_string(), serde_json::to_value(s).unwrap_or(Value::Null));
}
pub fn ensure(doc: &mut Document) {
    if !doc.extensions.contains_key(PLUGIN_ID) { save_settings(doc, &Settings::default()); }
    for (key, name, color) in MATERIALS {
        let id = material_id(key);
        if !doc.materials.iter().any(|m| m.id == id) { doc.materials.push(Material { id, name: name.to_string(), color: color.to_string(), opacity: 1.0 }); }
    }
}
pub fn new_id() -> String { uuid::Uuid::new_v4().to_string() }

/// Distinct colours per circuit number (Studio's palette, as hex).
pub const PALETTE: [&str; 12] = ["#1a73e8", "#f2711c", "#1e8e3e", "#8e44ad", "#e91e63", "#009688", "#3f51b5", "#d93025", "#795548", "#00acc1", "#26a69a", "#8c7319"];
pub fn circuit_color(number: u32) -> &'static str { PALETTE[((number.max(1) - 1) as usize) % PALETTE.len()] }
