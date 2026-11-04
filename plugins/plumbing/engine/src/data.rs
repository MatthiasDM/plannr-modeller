//! Data model of the plumbing plugin: `plumbing.pipe`, `plumbing.fitting` and `plumbing.bracket`
//! entities, plus a lenient reader for what we need from the construction plugin (storeys, rooms,
//! walls, plumbing fixtures). Coordinates in metres, y up; heights are above the storey floor.

use plannr_core::geom::{dist, len, normalize, sub, P3};
use plannr_core::model::{Document, Entity, Material};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::catalog::*;

pub type P2 = [f64; 2];
pub const Y_UP: P3 = [0.0, 1.0, 0.0];
pub const X_AX: P3 = [1.0, 0.0, 0.0];

// ---- entities ------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pipe {
    #[serde(default)] pub level: usize,
    #[serde(default = "d_cold")] pub system: String,
    #[serde(default = "d_alupex")] pub material: String,
    /// Outer diameter in mm.
    #[serde(default = "d_16")] pub dn: f64,
    /// Polyline, heights above the storey floor.
    #[serde(default)] pub points: Vec<P3>,
    /// `bend` (the pipe is bent at each corner) or `elbow` (an elbow fitting of `joint` at each corner).
    #[serde(default = "d_bend")] pub corners: String,
    #[serde(default = "d_press")] pub joint: String,
    /// Bend radius in m at the corners (default: the material's minimum for this dn).
    #[serde(default, skip_serializing_if = "Option::is_none")] pub bend_radius: Option<f64>,
    /// Insulation thickness in mm (0 = none).
    #[serde(default)] pub insulation: f64,
    /// Fitting or construction fixture the start / end is connected to (informative; geometry decides).
    #[serde(default, skip_serializing_if = "Option::is_none")] pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub note: Option<String>,
    /// Plannr material override set with the Paint Bucket.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub paint: Option<String>,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_cold() -> String { "coldWater".into() }
fn d_alupex() -> String { "alupex".into() }
fn d_16() -> f64 { 16.0 }
fn d_bend() -> String { "bend".into() }
fn d_press() -> String { "press".into() }

impl Pipe {
    pub fn length(&self) -> f64 { self.points.windows(2).map(|w| dist(w[0], w[1])).sum() }
    pub fn radius(&self) -> f64 { self.dn / 2000.0 }
    pub fn mat(&self) -> &'static PipeMaterial { material(&self.material).unwrap_or(&MATERIALS[0]) }
    /// Effective bend radius (m) used at bent corners.
    pub fn bend_r(&self) -> f64 { self.bend_radius.unwrap_or_else(|| min_bend_radius(self.mat(), self.dn).unwrap_or(self.dn / 1000.0 * 3.0)) }
    pub fn start(&self) -> P3 { self.points.first().copied().unwrap_or([0.0; 3]) }
    pub fn end(&self) -> P3 { self.points.last().copied().unwrap_or([0.0; 3]) }
    /// Unit direction leaving the pipe at an end (pointing away from the pipe).
    pub fn end_dir(&self, end: End) -> P3 {
        let n = self.points.len();
        if n < 2 { return X_AX; }
        match end { End::Start => normalize(sub(self.points[0], self.points[1])), End::End => normalize(sub(self.points[n - 1], self.points[n - 2])) }
    }
    pub fn segment_lengths(&self) -> Vec<f64> { self.points.windows(2).map(|w| dist(w[0], w[1])).collect() }
    /// Interior corners: (index, deflection angle in degrees).
    pub fn corner_angles(&self) -> Vec<(usize, f64)> {
        let mut out = vec![];
        for i in 1..self.points.len().saturating_sub(1) {
            let a = normalize(sub(self.points[i], self.points[i - 1])); let b = normalize(sub(self.points[i + 1], self.points[i]));
            let c = plannr_core::geom::dot(a, b).clamp(-1.0, 1.0);
            let ang = c.acos().to_degrees();
            if ang > 0.5 { out.push((i, ang)); }
        }
        out
    }
    pub fn is_bent(&self) -> bool { self.corners == "bend" }
    pub fn look(&self) -> String { if let Some(p) = &self.paint { return p.clone(); } look_id(self.mat().look.unwrap_or(self.system.as_str())) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End { Start, End }
impl End {
    pub fn parse(s: &str) -> Option<End> { match s { "start" | "from" | "a" => Some(End::Start), "end" | "to" | "b" => Some(End::End), _ => None } }
    pub fn name(self) -> &'static str { match self { End::Start => "start", End::End => "end" } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Fitting {
    #[serde(default)] pub level: usize,
    #[serde(default = "d_coupling")] pub kind: String,
    #[serde(default = "d_press")] pub joint: String,
    #[serde(default = "d_alupex")] pub material: String,
    #[serde(default = "d_cold")] pub system: String,
    /// Main diameter (mm) and the second one (branch / outlet / reduced side).
    #[serde(default = "d_16")] pub dn: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub dn2: Option<f64>,
    /// Centre, height above the storey floor.
    pub position: P3,
    /// Main axis (unit). For an inline fitting the pipes run along ±dir; for an elbow the first
    /// port is at -dir; for a wall plate the pipe comes from -dir.
    #[serde(default = "d_x")] pub dir: P3,
    /// Second axis: the branch of a tee, the outlet of an elbow / wall plate / mixer, the outlets of a manifold.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub dir2: Option<P3>,
    /// Elbow angle in degrees (90, 45, 87.5…) when the kind is an elbow.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub angle: Option<f64>,
    /// Thread size of a threaded port, e.g. "1/2\"".
    #[serde(default, skip_serializing_if = "Option::is_none")] pub thread: Option<String>,
    /// Number of outlets of a manifold.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub outlets: Option<u32>,
    /// Construction fixture this fitting serves (a wall plate under a tap).
    #[serde(default, skip_serializing_if = "Option::is_none")] pub fixture: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub paint: Option<String>,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_coupling() -> String { "coupling".into() }
fn d_x() -> P3 { X_AX }

impl Fitting {
    pub fn kind_info(&self) -> &'static FittingKind { fitting(&self.kind).unwrap_or(&FITTINGS[0]) }
    pub fn mat(&self) -> &'static PipeMaterial { material(&self.material).unwrap_or(&MATERIALS[0]) }
    pub fn d(&self) -> f64 { self.dn / 1000.0 }
    pub fn d2(&self) -> f64 { self.dn2.unwrap_or(self.dn) / 1000.0 }
    pub fn dir_n(&self) -> P3 { let d = normalize(self.dir); if len(d) < 0.5 { X_AX } else { d } }
    /// A unit second axis perpendicular to `dir` (given, or a sensible default: up, else x).
    pub fn dir2_n(&self) -> P3 {
        let d = self.dir_n();
        let want = self.dir2.map(normalize).filter(|v| len(*v) > 0.5).unwrap_or_else(|| if d[1].abs() < 0.9 { Y_UP } else { X_AX });
        // remove the component along dir so the two axes are perpendicular
        let perp = sub(want, plannr_core::geom::scale(d, plannr_core::geom::dot(want, d)));
        if len(perp) < 1e-6 { if d[1].abs() < 0.9 { Y_UP } else { X_AX } } else { normalize(perp) }
    }
    pub fn angle_deg(&self) -> f64 { self.angle.unwrap_or(match self.kind.as_str() { "elbow45" | "branch45" => 45.0, _ => 90.0 }) }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Bracket {
    #[serde(default)] pub level: usize,
    #[serde(default = "d_clip")] pub kind: String,
    /// On the pipe axis, height above the storey floor.
    pub position: P3,
    /// Pipe axis (unit).
    #[serde(default = "d_x")] pub dir: P3,
    /// From the pipe axis towards the surface the bracket is screwed to (unit).
    #[serde(default = "d_down")] pub normal: P3,
    #[serde(default = "d_16")] pub dn: f64,
    /// Distance from the pipe axis to the surface (m).
    #[serde(default = "d_standoff")] pub standoff: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub pipe: Option<String>,
    /// Placed by `auto_brackets` (replaced on the next run).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")] pub auto: bool,
    #[serde(flatten, default)] pub extra: Map<String, Value>,
}
fn d_clip() -> String { "clip".into() }
fn d_down() -> P3 { [0.0, -1.0, 0.0] }
fn d_standoff() -> f64 { 0.03 }

impl Bracket {
    pub fn dir_n(&self) -> P3 { let d = normalize(self.dir); if len(d) < 0.5 { X_AX } else { d } }
    pub fn normal_n(&self) -> P3 {
        let d = self.dir_n(); let n = normalize(self.normal); let n = if len(n) < 0.5 { d_down() } else { n };
        let perp = sub(n, plannr_core::geom::scale(d, plannr_core::geom::dot(n, d)));
        if len(perp) < 1e-6 { if d[1].abs() < 0.9 { d_down() } else { X_AX } } else { normalize(perp) }
    }
}

// ---- reading the document --------------------------------------------------------------------------

pub fn pipes(doc: &Document) -> Vec<(String, Pipe)> {
    doc.entities.iter().filter_map(|e| match e { Entity::Custom(c) if c.type_ == T_PIPE => serde_json::from_value::<Pipe>(c.data.clone()).ok().map(|p| (c.id.clone(), p)), _ => None }).collect()
}
pub fn fittings(doc: &Document) -> Vec<(String, Fitting)> {
    doc.entities.iter().filter_map(|e| match e { Entity::Custom(c) if c.type_ == T_FITTING => serde_json::from_value::<Fitting>(c.data.clone()).ok().map(|p| (c.id.clone(), p)), _ => None }).collect()
}
pub fn brackets(doc: &Document) -> Vec<(String, Bracket)> {
    doc.entities.iter().filter_map(|e| match e { Entity::Custom(c) if c.type_ == T_BRACKET => serde_json::from_value::<Bracket>(c.data.clone()).ok().map(|p| (c.id.clone(), p)), _ => None }).collect()
}
pub fn pipe(doc: &Document, id: &str) -> Option<Pipe> {
    doc.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.id == id && c.type_ == T_PIPE => serde_json::from_value(c.data.clone()).ok(), _ => None })
}
pub fn fitting_of(doc: &Document, id: &str) -> Option<Fitting> {
    doc.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.id == id && c.type_ == T_FITTING => serde_json::from_value(c.data.clone()).ok(), _ => None })
}

// ---- what we read from the construction plugin ----------------------------------------------------

#[derive(Debug, Clone)]
pub struct Fx { pub id: String, pub kind: String, pub level: usize, pub position: P3, pub rotation: f64 }
#[derive(Debug, Clone)]
pub struct Wl { pub level: usize, pub a: P2, pub b: P2 }

/// Construction fixture kinds that are plumbing points, with the system they belong to.
pub const FIXTURE_SYSTEMS: [(&str, &str); 10] = [("tapCold", "coldWater"), ("tapHot", "hotWater"), ("mixer", "hotWater"), ("showerValve", "hotWater"), ("toiletSupply", "coldWater"), ("floorDrain", "drain"), ("drainPoint", "drain"), ("radiator", "heatingFlow"), ("boiler", "hotWater"), ("manifold", "heatingFlow")];
pub fn fixture_system(kind: &str) -> Option<&'static str> { FIXTURE_SYSTEMS.iter().find(|f| f.0 == kind).map(|f| f.1) }

#[derive(Debug, Clone, Default)]
pub struct House {
    pub fixtures: Vec<Fx>,
    /// Wall lines in plan (free walls and room edges), per storey.
    pub walls: Vec<Wl>,
    pub storeys: Vec<(String, f64)>,
    pub ground_level: f64,
}

fn p3(v: Option<&Value>) -> Option<P3> { let a = v?.as_array()?; if a.len() < 3 { return None; } Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?]) }
fn p2(v: Option<&Value>) -> Option<P2> { let a = v?.as_array()?; if a.len() < 2 { return None; } Some([a[0].as_f64()?, a[1].as_f64()?]) }
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
                    if fixture_system(&s(d, "kind")).is_some() { h.fixtures.push(Fx { id: c.id.clone(), kind: s(d, "kind"), level: lvl(d), position, rotation: d.get("rotation").and_then(|x| x.as_f64()).unwrap_or(0.0) }); }
                },
                "construction.room" => {
                    let pts: Vec<P2> = d.get("points").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|p| p2(Some(p))).collect()).unwrap_or_default();
                    for i in 0..pts.len() { if pts.len() >= 3 { h.walls.push(Wl { level: lvl(d), a: pts[i], b: pts[(i + 1) % pts.len()] }); } }
                }
                "construction.wall" => if let (Some(a), Some(b)) = (p2(d.get("a")), p2(d.get("b"))) { h.walls.push(Wl { level: lvl(d), a, b }); },
                _ => {}
            }
        }
        h
    }
    pub fn base(&self, level: usize) -> f64 { self.ground_level + self.storeys.iter().take(level).map(|s| s.1).sum::<f64>() }
    pub fn height(&self, level: usize) -> f64 { self.storeys.get(level).map(|s| s.1).or_else(|| self.storeys.last().map(|s| s.1)).unwrap_or(2.6) }
    pub fn fixture(&self, id: &str) -> Option<&Fx> { self.fixtures.iter().find(|f| f.id == id) }
    /// Storey whose floor is the highest one at or under a world height.
    pub fn level_at(&self, y: f64) -> usize {
        let mut l = 0;
        for i in 0..self.storeys.len() { if y >= self.base(i) - 1e-6 { l = i; } }
        l
    }
    /// Nearest wall line on a storey to a plan point: (distance, unit normal from the wall towards the point).
    pub fn nearest_wall(&self, level: usize, p: P2) -> Option<(f64, P2)> {
        let mut best: Option<(f64, P2)> = None;
        for w in self.walls.iter().filter(|w| w.level == level) {
            let d = [w.b[0] - w.a[0], w.b[1] - w.a[1]]; let l2 = d[0] * d[0] + d[1] * d[1]; if l2 < 1e-12 { continue; }
            let t = (((p[0] - w.a[0]) * d[0] + (p[1] - w.a[1]) * d[1]) / l2).clamp(0.0, 1.0);
            let q = [w.a[0] + d[0] * t, w.a[1] + d[1] * t];
            let v = [p[0] - q[0], p[1] - q[1]]; let dd = (v[0] * v[0] + v[1] * v[1]).sqrt();
            if best.map(|b| dd < b.0).unwrap_or(true) {
                let n = if dd > 1e-6 { [v[0] / dd, v[1] / dd] } else { let l = l2.sqrt(); [-d[1] / l, d[0] / l] };
                best = Some((dd, n));
            }
        }
        best
    }
}

// ---- materials / settings ------------------------------------------------------------------------------

pub fn ensure(doc: &mut Document) {
    for (key, name, color, opacity) in LOOKS {
        let id = look_id(key);
        if !doc.materials.iter().any(|m| m.id == id) { doc.materials.push(Material { id, name: name.to_string(), color: color.to_string(), opacity }); }
    }
    if !doc.extensions.contains_key(PLUGIN_ID) { doc.extensions.insert(PLUGIN_ID.into(), serde_json::json!({ "version": 1 })); }
}
pub fn new_id() -> String { uuid::Uuid::new_v4().to_string() }
