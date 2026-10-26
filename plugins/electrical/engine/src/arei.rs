//! AREI / RGIE residential checks (advisory — a certified keuring is still required).

use serde_json::{json, Value};

use crate::data::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity { Error, Warning, Info, Ok }
impl Severity { pub fn key(&self) -> &'static str { match self { Severity::Error => "error", Severity::Warning => "warning", Severity::Info => "info", Severity::Ok => "ok" } } }

#[derive(Debug, Clone)]
pub struct Finding { pub severity: Severity, pub title: String, pub detail: String, pub circuit: Option<String>, pub points: Vec<String> }
impl Finding {
    fn new(severity: Severity, title: &str, detail: String) -> Self { Finding { severity, title: title.into(), detail, circuit: None, points: vec![] } }
    fn on(mut self, circuit: &str) -> Self { self.circuit = Some(circuit.into()); self }
    pub fn to_json(&self) -> Value {
        let mut v = json!({ "severity": self.severity.key(), "title": self.title, "detail": self.detail });
        if let Some(c) = &self.circuit { v["circuit"] = json!(c); }
        if !self.points.is_empty() { v["points"] = json!(self.points); }
        v
    }
}

/// Load points (sockets, data, lights) of a circuit that exist in the house.
pub fn point_count(c: &Circuit, h: &House) -> usize { c.points.iter().filter(|id| h.is_point(id)).count() }

/// Total estimated demand in kW with Studio's 0.7 diversity factor.
pub fn total_demand(s: &Settings, h: &House) -> f64 { s.circuits.iter().map(|c| c.load_kw(point_count(c, h))).sum::<f64>() * 0.7 }

/// Does the house contain an appliance of this circuit type (unit names/types or fixture notes)?
fn has_appliance(h: &House, t: &str) -> bool {
    let needles: &[&str] = match t {
        "oven" => &["oven"], "hob" => &["hob", "kookplaat", "cooktop", "inductie"], "dishwasher" => &["dishwasher", "vaatwasser", "afwasmachine"],
        "washingMachine" => &["washing", "wasmachine"], "dryer" => &["dryer", "droogkast", "droger"], _ => return false,
    };
    let hit = |text: &str| { let t = text.to_lowercase(); needles.iter().any(|n| t.contains(n)) };
    h.units.iter().any(|u| hit(&u.name) || hit(&u.type_)) || h.fixtures.iter().any(|f| hit(&f.note))
}

pub fn check(s: &Settings, h: &House, conduits: &[(String, Conduit)]) -> Vec<Finding> {
    let mut out = vec![];
    let b = &s.board;
    if b.main_differential == 0 {
        out.push(Finding::new(Severity::Error, "No main differential", "The AREI requires a general 300 mA (or 100 mA) type A differential at the board head for fire protection.".into()));
    } else if b.main_differential != 300 && b.main_differential != 100 {
        out.push(Finding::new(Severity::Warning, "Unusual head differential", format!("The head differential is {} mA; Belgian homes use 300 mA (100 mA where required).", b.main_differential)));
    }
    if s.circuits.len() < 2 {
        out.push(Finding::new(Severity::Warning, "Too few circuits", "A dwelling needs separate circuits for lighting and sockets at a minimum.".into()));
    } else {
        let has = |t: &str| s.circuits.iter().any(|c| c.type_ == t);
        if !has("lighting") || !(has("sockets") || has("kitchen")) {
            out.push(Finding::new(Severity::Warning, "Lighting and sockets not separated", "Lighting and socket outlets should be on separate circuits.".into()));
        }
    }
    for c in &s.circuits {
        let t = type_info(&c.type_);
        let max = max_breaker(c.cable_section);
        if c.breaker > max {
            out.push(Finding::new(Severity::Error, "Breaker too large for the cable", format!("{}: a {} A breaker exceeds the {} A limit for {} mm² cable.", c.name, c.breaker, max, section_text(c.cable_section))).on(&c.id));
        }
        if t.needs_rcd30 && c.rcd != "mA30" {
            out.push(Finding::new(Severity::Error, "Missing 30 mA protection", format!("{} ({}) must be on a 30 mA differential.", c.name, t.label)).on(&c.id));
        }
        let n = point_count(c, h);
        if t.dedicated && n > 1 {
            out.push(Finding::new(Severity::Warning, "Dedicated circuit shares points", format!("{} should power one appliance only, but has {n} points.", c.name)).on(&c.id));
        } else if !t.dedicated && n > t.max_points as usize {
            let guidance = if c.type_ == "lighting" { " (guidance)" } else { "" };
            out.push(Finding::new(Severity::Warning, "Too many points on a circuit", format!("{} has {n} points; the AREI limit for {} is {}{guidance}.", c.name, t.label.to_lowercase(), t.max_points)).on(&c.id));
        }
        if c.type_ == "pv" && c.rcd == "none" {
            out.push(Finding::new(Severity::Info, "PV circuit without differential", format!("{}: check the inverter's required differential type (often type B or a dedicated 30/300 mA).", c.name)).on(&c.id));
        }
        if !conduits.iter().any(|(_, k)| k.circuit.as_deref() == Some(c.id.as_str())) {
            out.push(Finding::new(Severity::Info, "Circuit without conduit", format!("{} has no conduit or cable drawn yet.", c.name)).on(&c.id));
        }
    }
    for t in ["oven", "hob", "dishwasher", "washingMachine", "dryer"] {
        if has_appliance(h, t) && !s.circuits.iter().any(|c| c.type_ == t) {
            out.push(Finding::new(Severity::Warning, &format!("No dedicated {} circuit", type_label(t).to_lowercase()), format!("The house has a {} but no circuit of that type; it should be on its own circuit.", type_label(t).to_lowercase())));
        }
    }
    let assigned: Vec<&str> = s.circuits.iter().flat_map(|c| c.points.iter().map(|p| p.as_str())).collect();
    let unassigned: Vec<String> = h.electrical_points().iter().filter(|f| !assigned.contains(&f.id.as_str())).map(|f| f.id.clone()).collect();
    if !unassigned.is_empty() {
        let mut f = Finding::new(Severity::Warning, "Unassigned points", format!("{} placed outlets / lights / data points are not on any circuit yet.", unassigned.len()));
        f.points = unassigned; out.push(f);
    }
    let demand = total_demand(s, h); let cap = b.capacity_kw();
    if demand > cap {
        out.push(Finding::new(Severity::Warning, "Estimated load over capacity", format!("Estimated {demand:.1} kW exceeds the {cap:.1} kW board capacity.")));
    }
    for g in &s.rcd_groups {
        if g.milliamps == 30 && g.circuits.len() > 8 {
            out.push(Finding::new(Severity::Warning, "Many circuits behind one 30 mA differential", format!("{} protects {} circuits; keep it to 8 or fewer so one fault does not darken the whole house (guidance).", g.name, g.circuits.len())));
        }
    }
    for (id, k) in conduits {
        if k.circuit.is_none() { let mut f = Finding::new(Severity::Info, "Conduit without circuit", "A conduit is not assigned to a circuit.".into()); f.points = vec![id.clone()]; out.push(f); }
    }
    if !out.iter().any(|f| matches!(f.severity, Severity::Error | Severity::Warning)) {
        out.push(Finding::new(Severity::Ok, "No issues found", "The installation passes the checks Plannr can run. A certified keuring is still required.".into()));
    }
    out.sort_by(|a, b| a.severity.cmp(&b.severity));
    out
}
