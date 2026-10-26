//! Engineering-lite checks (Medeek Engineering style): joists, rafters, studs, headers, beams,
//! footings and stairs, plus allowable-span tables. Simplified Eurocode 5 for simply supported
//! members under uniform load — a design aid, not a structural calculation.
//!
//! Units inside: N, m, Pa. Loads in the settings are kN/m², strengths MPa.
//!
//! * ULS load (EN 1990 6.10): w_d = (1.35·g + 1.5·q) · tributary width            [N/m]
//! * design strength (EN 1995 2.4.1): f_d = k_mod · f_k / γ_M, γ_M = 1.3 (solid timber),
//!   k_mod = 0.8 medium-term (floor imposed load), 0.9 short-term (snow), service class 1/2
//! * bending:   M = w·L²/8,  σ = M / W,  W = b·h²/6                                   (σ ≤ f_m,d)
//! * shear:     V = w·L/2,   τ = 1.5·V / (k_cr·b·h), k_cr = 0.67                        (τ ≤ f_v,d)
//! * deflection (SLS, E_0,mean): δ = 5·w·L⁴ / (384·E·I), I = b·h³/12                   (δ ≤ L / limit)
//!   floors: imposed load only against L/360; roofs: total load against L/240
//! * North-American grades are converted from ASD reference values to characteristic ones with
//!   the ASTM D245 factors (bending × 2.1, shear × 2.88) and the No.2 size factor C_F on bending.

use plannr_core::model::{Custom, Document, Entity};
use serde_json::{json, Value};

use crate::data::*;
use crate::expand::*;

pub const GAMMA_M: f64 = 1.3;
pub const K_CR: f64 = 0.67;
pub const KMOD_MEDIUM: f64 = 0.8;
pub const KMOD_SHORT: f64 = 0.9;
const PSI: f64 = 6894.757;

#[derive(Debug, Clone, Copy)]
pub struct Species { pub name: &'static str, pub fm_k: f64, pub fv_k: f64, pub e_mean: f64, pub us: bool }

/// Characteristic values in Pa. EN 338:2016 for C classes; NDS (No.2) converted for SPF / DF-L.
pub fn species(name: &str) -> Option<Species> {
    let mpa = 1e6;
    Some(match name.to_uppercase().as_str() {
        "C18" => Species { name: "C18", fm_k: 18.0 * mpa, fv_k: 3.4 * mpa, e_mean: 9000.0 * mpa, us: false },
        "C24" => Species { name: "C24", fm_k: 24.0 * mpa, fv_k: 4.0 * mpa, e_mean: 11000.0 * mpa, us: false },
        "C30" => Species { name: "C30", fm_k: 30.0 * mpa, fv_k: 4.0 * mpa, e_mean: 12000.0 * mpa, us: false },
        // SPF No.2: Fb 875 psi, Fv 135 psi, E 1.4e6 psi
        "SPF2" => Species { name: "SPF2", fm_k: 875.0 * PSI * 2.1, fv_k: 135.0 * PSI * 2.88, e_mean: 1.4e6 * PSI, us: true },
        // Douglas fir-larch No.2: Fb 900 psi, Fv 180 psi, E 1.6e6 psi
        "DFL2" => Species { name: "DFL2", fm_k: 900.0 * PSI * 2.1, fv_k: 180.0 * PSI * 2.88, e_mean: 1.6e6 * PSI, us: true },
        _ => return None,
    })
}
pub const SPECIES: [&str; 5] = ["C18", "C24", "C30", "SPF2", "DFL2"];

/// NDS size factor C_F for No.2 dimension lumber in bending (by depth).
fn size_factor(depth: f64) -> f64 {
    if depth <= 0.09 { 1.5 } else if depth <= 0.141 { 1.3 } else if depth <= 0.185 { 1.2 } else if depth <= 0.236 { 1.1 } else { 1.0 }
}

/// Design values for a member of `species`, depth `h`, load duration `kmod`.
#[derive(Debug, Clone, Copy)]
pub struct Design { pub fm_d: f64, pub fv_d: f64, pub e: f64 }
pub fn design(sp: &Species, h: f64, kmod: f64) -> Design {
    let cf = if sp.us { size_factor(h) } else { 1.0 };
    Design { fm_d: kmod * sp.fm_k * cf / GAMMA_M, fv_d: kmod * sp.fv_k / GAMMA_M, e: sp.e_mean }
}
/// Engineered materials for beams: LVL (Kerto-S like) and glulam GL24h.
pub fn beam_design(material: &str, sp: &Species, h: f64, kmod: f64) -> Design {
    let mpa = 1e6;
    match material {
        "lvl" => Design { fm_d: kmod * 44.0 * mpa / 1.2, fv_d: kmod * 4.1 * mpa / 1.2, e: 13800.0 * mpa },
        "glulam" => Design { fm_d: kmod * 24.0 * mpa / 1.25, fv_d: kmod * 3.5 * mpa / 1.25, e: 11500.0 * mpa },
        _ => design(sp, h, kmod),
    }
}

/// Utilisations of a simply supported member (b × h) over span `l` with ULS load `w_uls` and SLS
/// load `w_sls` (both N/m) against deflection limit L/`lim`.
#[derive(Debug, Clone, Copy)]
pub struct Util { pub bending: f64, pub shear: f64, pub deflection: f64, pub delta: f64 }
impl Util { pub fn max(&self) -> f64 { self.bending.max(self.shear).max(self.deflection) } pub fn governs(&self) -> &'static str { if self.bending >= self.shear && self.bending >= self.deflection { "bending" } else if self.shear >= self.deflection { "shear" } else { "deflection" } } }
pub fn utilisation(b: f64, h: f64, l: f64, w_uls: f64, w_sls: f64, lim: f64, d: &Design) -> Util {
    let (w_mod, i, a) = (b * h * h / 6.0, b * h * h * h / 12.0, b * h);
    let sigma = w_uls * l * l / 8.0 / w_mod;
    let tau = 1.5 * (w_uls * l / 2.0) / (K_CR * a);
    let delta = 5.0 * w_sls * l.powi(4) / (384.0 * d.e * i);
    Util { bending: sigma / d.fm_d, shear: tau / d.fv_d, deflection: delta / (l / lim), delta }
}
/// Largest span meeting bending, shear and deflection, and what governs it.
pub fn allowable_span(b: f64, h: f64, w_uls: f64, w_sls: f64, lim: f64, d: &Design) -> (f64, &'static str) {
    let (w_mod, i, a) = (b * h * h / 6.0, b * h * h * h / 12.0, b * h);
    let lb = (8.0 * d.fm_d * w_mod / w_uls.max(1e-9)).sqrt();
    let lv = 2.0 * d.fv_d * K_CR * a / 1.5 / w_uls.max(1e-9);
    let ld = (384.0 * d.e * i / (5.0 * w_sls.max(1e-9) * lim)).cbrt();
    if lb <= lv && lb <= ld { (lb, "bending") } else if lv <= ld { (lv, "shear") } else { (ld, "deflection") }
}

fn r2(v: f64) -> f64 { (v * 100.0).round() / 100.0 }
fn r3(v: f64) -> f64 { (v * 1000.0).round() / 1000.0 }

/// Load cases per m² for a use: (g_d, q_d) → ULS load per m², SLS load per m², kmod, deflection limit.
/// Roof loads are per m² of plan: the dead load along the slope is multiplied by √(1 + m²).
pub fn area_loads(e: &Engineering, use_: &str, slope: f64) -> (f64, f64, f64, f64) {
    let l = &e.loads;
    if use_ == "roof" {
        let g = l.roof_dead * (1.0 + slope * slope).sqrt(); let s = l.roof_snow;
        ((1.35 * g + 1.5 * s) * 1e3, (g + s) * 1e3, KMOD_SHORT, e.deflection.roof)
    } else {
        let (g, q) = (l.floor_dead, l.floor_live);
        ((1.35 * g + 1.5 * q) * 1e3, q * 1e3, KMOD_MEDIUM, e.deflection.floor)
    }
}

/// `span_table` op: allowable simple spans per size at a spacing.
pub fn span_table(doc: &Document, data: &Value) -> Value {
    let mut e = House::read(doc).engineering;
    if let Some(sp) = data.get("species").and_then(|v| v.as_str()) { e.species = sp.into(); }
    if let Some(l) = data.get("loads") { if let Ok(v) = serde_json::from_value::<Loads>(merge(serde_json::to_value(&e.loads).unwrap(), l)) { e.loads = v; } }
    let sp = species(&e.species).unwrap_or_else(|| species("C24").unwrap());
    let use_ = data.get("use").and_then(|v| v.as_str()).unwrap_or("floor");
    let spacing = data.get("spacing").and_then(|v| v.as_f64()).unwrap_or(0.4).max(0.1);
    let slope = data.get("pitch").and_then(|v| v.as_f64()).unwrap_or(6.0) / 12.0;
    let sizes: Vec<String> = data.get("sizes").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_else(|| ["2x6", "2x8", "2x10", "2x12"].iter().map(|s| s.to_string()).collect());
    let (wu, ws, kmod, lim) = area_loads(&e, use_, slope);
    let rows: Vec<Value> = sizes.iter().filter_map(|s| {
        let [b, h] = lumber(s)?;
        let (span, gov) = allowable_span(b, h, wu * spacing, ws * spacing, lim, &design(&sp, h, kmod));
        Some(json!({ "size": s, "span_m": r2(span), "governs": gov }))
    }).collect();
    json!({ "use": use_, "species": sp.name, "spacing_m": spacing, "loads": e.loads, "deflectionLimit": lim, "spans": rows,
            "note": if use_ == "roof" { "horizontal span of a rafter, snow + dead load on plan" } else { "clear span of a floor joist, imposed + dead load" } })
}
fn merge(mut base: Value, patch: &Value) -> Value {
    if let (Value::Object(b), Value::Object(p)) = (&mut base, patch) { for (k, v) in p { b.insert(k.clone(), v.clone()); } }
    base
}

// ---- the check --------------------------------------------------------------------------------------

struct Findings { list: Vec<Value> }
impl Findings {
    fn push(&mut self, severity: &str, element: &str, title: &str, detail: String, value: Option<f64>, limit: Option<f64>) {
        let mut v = json!({ "severity": severity, "element": element, "title": title, "detail": detail });
        if let Some(x) = value { v["value"] = json!(r3(x)); }
        if let Some(x) = limit { v["limit"] = json!(r3(x)); }
        self.list.push(v);
    }
    /// One finding from a utilisation: error above 1, warning above 0.9, else ok.
    fn util(&mut self, element: &str, what: &str, u: &Util, l: f64, extra: &str) {
        let m = u.max();
        let sev = if m > 1.0 { "error" } else if m > 0.9 { "warning" } else { "ok" };
        let title = if m > 1.0 { format!("{what} overstressed") } else if m > 0.9 { format!("{what} close to its limit") } else { format!("{what} OK") };
        self.push(sev, element, &title, format!("{extra}span {:.2} m: bending {:.0} %, shear {:.0} %, deflection {:.0} % ({:.1} mm); {} governs", l, u.bending * 100.0, u.shear * 100.0, u.deflection * 100.0, u.delta * 1000.0, u.governs()), Some(m), Some(1.0));
    }
}

/// Maximum laterally unsupported stud height for a bearing wall (IRC R602.3(5)-like).
pub fn max_stud_height(size: [f64; 2], spacing: f64) -> f64 {
    let d = size[1];
    if d < 0.07 { if spacing <= 0.41 { 2.44 } else { 0.0 } }
    else if d < 0.1 { 3.05 }
    else if d < 0.15 { 4.27 }
    else { 5.5 }
}

pub fn wall_height(w: &Wall, house: &House) -> f64 {
    w.height.unwrap_or_else(|| { let s = house.height(w.level) - house.floor_thickness; if s > 0.5 { s } else { 2.44 } })
}

fn customs(doc: &Document) -> Vec<&Custom> { doc.entities.iter().filter_map(|e| match e { Entity::Custom(c) if TYPES.contains(&c.type_.as_str()) => Some(c), _ => None }).collect() }

/// Line load (characteristic g, q, and whether q is snow) a wall on `level` along a→b carries from roofs and floors above.
pub fn wall_line_load(doc: &Document, house: &House, level: usize, a: P2, b: P2) -> (f64, f64, bool) {
    let e = &house.engineering;
    let (mut g, mut q, mut snow) = (0.0, 0.0, false);
    let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let along_x = (b[0] - a[0]).abs() >= (b[1] - a[1]).abs();
    for c in customs(doc) {
        match c.type_.as_str() {
            T_ROOF => if let Ok(rf) = serde_json::from_value::<Roof>(c.data.clone()) {
                if rf.level != level { continue; }
                let (minx, minz, maxx, maxz) = (rf.min[0].min(rf.max[0]), rf.min[1].min(rf.max[1]), rf.min[0].max(rf.max[0]), rf.min[1].max(rf.max[1]));
                // eave walls run parallel to the ridge on the bearing lines
                let (on_eave, span) = if rf.ridge_along_x {
                    (along_x && mid[0] > minx - 0.3 && mid[0] < maxx + 0.3 && ((mid[1] - minz).abs() < 0.35 || (mid[1] - maxz).abs() < 0.35), maxz - minz)
                } else {
                    (!along_x && mid[1] > minz - 0.3 && mid[1] < maxz + 0.3 && ((mid[0] - minx).abs() < 0.35 || (mid[0] - maxx).abs() < 0.35), maxx - minx)
                };
                if on_eave {
                    let m = rf.slope(); let trib = span / 2.0 + rf.overhang;
                    g += e.loads.roof_dead * (1.0 + m * m).sqrt() * trib; q += e.loads.roof_snow * trib; snow = true;
                }
            },
            T_FLOOR => if let Ok(f) = serde_json::from_value::<Floor>(c.data.clone()) {
                if f.level != level + 1 || f.points.len() < 3 { continue; }
                let (minx, maxx) = f.points.iter().fold((f64::MAX, f64::MIN), |(p, r), v| (p.min(v[0]), r.max(v[0])));
                let (minz, maxz) = f.points.iter().fold((f64::MAX, f64::MIN), |(p, r), v| (p.min(v[1]), r.max(v[1])));
                if mid[0] < minx - 0.3 || mid[0] > maxx + 0.3 || mid[1] < minz - 0.3 || mid[1] > maxz + 0.3 { continue; }
                let span_x = match f.direction.as_str() { "x" => true, "z" => false, _ => maxx - minx <= maxz - minz };
                // a wall perpendicular to the joists carries half their span
                if span_x != along_x { let ext = if span_x { maxx - minx } else { maxz - minz }; g += e.loads.floor_dead * ext / 2.0; q += e.loads.floor_live * ext / 2.0; snow = false; }
            },
            _ => {}
        }
    }
    (g, q, snow)
}

pub fn check(doc: &Document) -> Value {
    let house = House::read(doc);
    let e = &house.engineering;
    let sp = species(&e.species).unwrap_or_else(|| species("C24").unwrap());
    let mut f = Findings { list: vec![] };
    if species(&e.species).is_none() { f.push("warning", "", "Unknown species", format!("`{}` is not one of {}; C24 used", e.species, SPECIES.join(", ")), None, None); }
    for c in customs(doc) {
        let id = c.id.as_str();
        match c.type_.as_str() {
            T_FLOOR => if let Ok(fl) = serde_json::from_value::<Floor>(c.data.clone()) {
                let ex = expand_floor(id, &fl, &house, "");
                let span = ex.members.iter().filter(|m| m.key.starts_with("joist.")).map(|m| m.length).fold(0.0, f64::max);
                if span <= 0.0 { continue; }
                let [b, h] = lumber_or(&fl.joist_size, "2x10");
                let (wu, ws, kmod, lim) = area_loads(e, "floor", 0.0);
                let s = fl.joist_spacing.max(0.1);
                let u = utilisation(b, h, span, wu * s, ws * s, lim, &design(&sp, h, kmod));
                let (allow, _) = allowable_span(b, h, wu * s, ws * s, lim, &design(&sp, h, kmod));
                f.util(id, "Floor joists", &u, span, &format!("{} @ {:.0} mm {}, allowable {:.2} m; ", fl.joist_size, s * 1000.0, sp.name, allow));
            },
            T_ROOF => if let Ok(rf) = serde_json::from_value::<Roof>(c.data.clone()) {
                let span = if rf.ridge_along_x { (rf.max[1] - rf.min[1]).abs() } else { (rf.max[0] - rf.min[0]).abs() };
                let m = rf.slope();
                if rf.type_ == "rafters" {
                    let [b, h] = lumber_or(&rf.rafter_size, "2x8");
                    let (wu, ws, kmod, lim) = area_loads(e, "roof", m);
                    let lh = span / 2.0;
                    let u = utilisation(b, h, lh, wu * rf.spacing, ws * rf.spacing, lim, &design(&sp, h, kmod));
                    f.util(id, "Rafters", &u, lh, &format!("{} @ {:.0} mm, pitch {:.1}/12, horizontal ", rf.rafter_size, rf.spacing * 1000.0, m * 12.0));
                    if rf.extra.get("ceilingJoists").and_then(|v| v.as_bool()) == Some(false) {
                        f.push("warning", id, "Ridge board without rafter ties", "Without ceiling joists or collar ties the ridge must be a structural ridge beam carried by posts.".into(), None, None);
                    } else {
                        f.push("info", id, "Ridge board", "A ridge board carries no load: the ceiling joists act as rafter ties — nail them to the rafters at every bearing.".into(), None, None);
                    }
                } else {
                    if rf.spacing > 0.61 { f.push("warning", id, "Truss spacing", format!("{:.0} mm: trusses are normally 600 mm apart or less; wider spacing needs heavier sheathing and battens.", rf.spacing * 1000.0), Some(rf.spacing), Some(0.6)); }
                    if span > 12.0 { f.push("info", id, "Long-span trusses", format!("{span:.1} m span: get the trusses designed by the manufacturer."), Some(span), Some(12.0)); }
                    else { f.push("ok", id, "Trusses", format!("{} trusses over {span:.2} m at {:.0} mm: designed and stamped by the truss manufacturer.", rf.type_, rf.spacing * 1000.0), None, None); }
                }
            },
            T_WALL => if let Ok(w) = serde_json::from_value::<Wall>(c.data.clone()) {
                let h = wall_height(&w, &house);
                let size = w.stud();
                let max = max_stud_height(size, w.stud_spacing);
                let lambda = h * 12f64.sqrt() / size[1];
                if h > max + 1e-6 { f.push("error", id, "Studs too tall", format!("{} studs at {:.0} mm may be at most {:.2} m tall; this wall is {:.2} m — use a deeper stud or add mid-height blocking with an engineered design.", w.stud_size, w.stud_spacing * 1000.0, max, h), Some(h), Some(max)); }
                else if lambda > 150.0 { f.push("warning", id, "Slender studs", format!("slenderness h·√12/d = {lambda:.0} > 150"), Some(lambda), Some(150.0)); }
                else { f.push("ok", id, "Studs OK", format!("{} @ {:.0} mm, {:.2} m tall (max {:.2} m)", w.stud_size, w.stud_spacing * 1000.0, h, max), Some(h), Some(max)); }
                let (g, q, snow) = wall_line_load(doc, &house, w.level, w.a, w.b);
                for (oi, o) in w.openings.iter().enumerate() {
                    let rule = header_size_for_span(o.width);
                    let eff = o.header_size.clone().unwrap_or_else(|| rule.to_string());
                    let [ht, hd] = lumber_or(&eff, "2x6");
                    let rule_d = lumber_or(rule, "2x6")[1];
                    if hd + 1e-6 < rule_d {
                        f.push("warning", id, "Header smaller than the span rule", format!("opening {} ({:.2} m): header {} but the span rule asks {}", oi + 1, o.width, eff, rule), Some(hd), Some(rule_d));
                    }
                    if g + q > 0.0 {
                        let l = o.width + T * o.jack_studs.max(1) as f64;
                        let kmod = if snow { KMOD_SHORT } else { KMOD_MEDIUM };
                        let u = utilisation(2.0 * ht, hd, l, (1.35 * g + 1.5 * q) * 1e3, (g + q) * 1e3, 360.0, &design(&sp, hd, kmod));
                        f.util(id, &format!("Header of opening {}", oi + 1), &u, l, &format!("2-ply {eff}, line load {:.1} kN/m; ", g + q));
                    }
                }
            },
            T_BEAM => if let Ok(bm) = serde_json::from_value::<Beam>(c.data.clone()) {
                let [b, h] = bm.section();
                let sup = bm.supports();
                let l = sup.windows(2).map(|w| w[1] - w[0]).fold(0.0, f64::max);
                let trib = bm.tributary.unwrap_or_else(|| beam_tributary(&house, &bm));
                if trib <= 0.0 {
                    f.push("info", id, "Beam without load", "No floor above found to carry; give `tributary` (m) to check the beam.".into(), None, None);
                } else {
                    let (wu, ws, kmod, lim) = area_loads(e, "floor", 0.0);
                    let u = utilisation(b, h, l, wu * trib, ws * trib, lim, &beam_design(&bm.material, &sp, h, kmod));
                    f.util(id, "Beam", &u, l, &format!("{} {}×{:.0} mm, tributary {:.2} m; ", bm.material, (b * 1000.0).round(), h * 1000.0, trib));
                }
            },
            T_FOUNDATION => if let Ok(fd) = serde_json::from_value::<Foundation>(c.data.clone()) {
                if fd.type_ != "stemwall" { f.push("info", id, "Slab bearing", "Slab-on-grade: bearing is spread over the slab; thicken it under bearing walls.".into(), None, None); continue; }
                // worst wall line: roof + floors over the footprint + 0.5 kN/m of wall per storey + stem and footing self-weight
                let mut line = 0.0f64;
                let n = fd.points.len();
                for i in 0..n {
                    let (a, b) = (fd.points[i], fd.points[(i + 1) % n]);
                    let mut total = 0.0;
                    for lvl in 0..house.storeys.len() { let (g, q, _) = wall_line_load(doc, &house, lvl, a, b); total += g + q + 0.5; }
                    line = line.max(total);
                }
                line += 25.0 * fd.stem_thickness * fd.stem_height + 25.0 * fd.footing_width.max(fd.stem_thickness) * fd.footing_depth;
                let p = line / fd.footing_width.max(0.05);
                let sev = if p > e.soil_bearing { "error" } else if p > 0.9 * e.soil_bearing { "warning" } else { "ok" };
                f.push(sev, id, if sev == "ok" { "Footing bearing OK" } else { "Footing bearing" }, format!("line load {line:.1} kN/m on a {:.0} mm footing: {p:.0} kPa against {:.0} kPa allowed", fd.footing_width * 1000.0, e.soil_bearing), Some(p), Some(e.soil_bearing));
            },
            T_STAIR => if let Ok(st) = serde_json::from_value::<Stair>(c.data.clone()) {
                let (n, r) = st.risers_of(&house);
                let g = st.tread_depth;
                let step = 2.0 * r + g;
                if !(0.15..=0.20).contains(&r) { f.push(if r > 0.22 { "error" } else { "warning" }, id, "Riser height", format!("{n} risers of {:.0} mm; aim for 150–200 mm", r * 1000.0), Some(r), Some(0.20)); }
                if g < 0.23 - 1e-9 { f.push("warning", id, "Tread depth", format!("going {:.0} mm < 230 mm", g * 1000.0), Some(g), Some(0.23)); }
                if !(0.57..=0.65).contains(&step) { f.push("warning", id, "Step rule 2R + G", format!("2R + G = {:.0} mm, outside 570–650 mm", step * 1000.0), Some(step), Some(0.65)); }
                else { f.push("ok", id, "Stair comfortable", format!("{n} risers of {:.0} mm, going {:.0} mm, 2R + G = {:.0} mm", r * 1000.0, g * 1000.0, step * 1000.0), Some(step), Some(0.65)); }
                let [sb, sh] = lumber_or(&st.stringer_size, "2x12");
                let notch = r * g / (r * r + g * g).sqrt();
                if sh - notch < 0.09 { f.push("warning", id, "Stringer throat", format!("{:.0} mm left under the notches (≥ 90 mm)", (sh - notch) * 1000.0), Some(sh - notch), Some(0.09)); }
                let _ = sb;
                f.push("info", id, "Headroom", "Keep ≥ 2.0 m clear headroom above the pitch line under the stairwell opening.".into(), None, None);
            },
            _ => {}
        }
    }
    let count = |s: &str| f.list.iter().filter(|x| x["severity"] == s).count();
    json!({ "species": sp.name, "loads": e.loads, "deflection": e.deflection, "findings": f.list, "errors": count("error"), "warnings": count("warning") })
}

/// Load width a beam carries from the floors above: half the floor's extent across the joists when the
/// beam runs perpendicular to them inside the floor (the joists then bear on it at mid-span).
pub fn beam_tributary(house: &House, bm: &Beam) -> f64 {
    let mid = [(bm.a[0] + bm.b[0]) / 2.0, (bm.a[1] + bm.b[1]) / 2.0];
    let along_x = (bm.b[0] - bm.a[0]).abs() >= (bm.b[1] - bm.a[1]).abs();
    let mut t = 0.0;
    for f in &house.floors {
        if f.points.len() < 3 || (f.level != bm.level + 1 && f.level != bm.level) { continue; }
        let (minx, maxx) = f.points.iter().fold((f64::MAX, f64::MIN), |(p, r), v| (p.min(v[0]), r.max(v[0])));
        let (minz, maxz) = f.points.iter().fold((f64::MAX, f64::MIN), |(p, r), v| (p.min(v[1]), r.max(v[1])));
        if mid[0] < minx || mid[0] > maxx || mid[1] < minz || mid[1] > maxz { continue; }
        let span_x = match f.direction.as_str() { "x" => true, "z" => false, _ => maxx - minx <= maxz - minz };
        if span_x != along_x { t += (if span_x { maxx - minx } else { maxz - minz }) / 2.0; }
    }
    t
}
