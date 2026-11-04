//! Advisory checks on the plumbing model: open ends, diameter and system mismatches at fittings,
//! bend radii, bracket spacing, drain fall, insulation.

use plannr_core::geom::{dist, dot, len, scale, sub, add};
use plannr_core::model::Document;
use serde_json::{json, Value};

use crate::catalog::*;
use crate::data::*;
use crate::expand::pipe_path;
use crate::net::{Attach, Net};

#[derive(Debug, Clone)]
pub struct Finding { pub severity: &'static str, pub title: String, pub detail: String, pub ids: Vec<String> }
impl Finding {
    pub fn to_json(&self) -> Value { json!({ "severity": self.severity, "title": self.title, "detail": self.detail, "ids": self.ids }) }
}
fn f(sev: &'static str, title: &str, detail: String, ids: Vec<String>) -> Finding { Finding { severity: sev, title: title.into(), detail, ids } }
fn m(v: f64) -> String { format!("{:.2} m", v) }

pub fn check(doc: &Document) -> Vec<Finding> {
    let net = Net::read(doc); let brs = brackets(doc);
    let mut out = vec![];
    // ---- pipes
    for (id, p) in &net.pipes {
        let name = p.name.clone().unwrap_or_else(|| format!("{} {} {}", system_label(&p.system), p.mat().label_nl, dn_label(p.mat(), p.dn)));
        if p.points.len() < 2 || p.length() < 0.005 { out.push(f("error", "Pipe without length", format!("{name} has fewer than two distinct points."), vec![id.clone()])); continue; }
        let mat = p.mat();
        if !mat.systems.contains(&p.system.as_str()) { out.push(f("error", "Material not used for this system", format!("{} is not used for {}.", mat.label, system_label(&p.system)), vec![id.clone()])); }
        if !mat.dns.iter().any(|d| (d - p.dn).abs() < 1e-6) { out.push(f("warning", "Diameter not in the series", format!("{} {} mm is not a standard size ({}).", mat.label, num(p.dn), mat.dns.iter().map(|d| num(*d)).collect::<Vec<_>>().join(", ")), vec![id.clone()])); }
        if !mat.joints.contains(&p.joint.as_str()) { out.push(f("error", "Joint type not available for this material", format!("{} is not joined with {} ({}).", mat.label, joint_label(&p.joint), mat.joints.iter().map(|j| joint_label(j)).collect::<Vec<_>>().join(", ")), vec![id.clone()])); }
        if p.is_bent() {
            match min_bend_radius(mat, p.dn) {
                None => out.push(f("error", "Material is not bent", format!("{} is joined with elbows; set the pipe's corners to elbow.", mat.label), vec![id.clone()])),
                Some(rmin) => {
                    if p.dn > mat.bend_up_to + 1e-6 { out.push(f("warning", "Diameter too large to bend", format!("{} {} is normally fitted with elbows (bent up to {} mm).", mat.label, num(p.dn), num(mat.bend_up_to)), vec![id.clone()])); }
                    let (_, radii) = pipe_path(p);
                    let too_tight: Vec<String> = p.corner_angles().iter().zip(radii.iter()).filter(|((_, ang), r)| *ang > 1.0 && **r < rmin - 0.001).map(|((i, ang), r)| format!("corner {} ({:.0}°): R {} mm < {} mm", i, ang, (r * 1000.0).round(), (rmin * 1000.0).round())).collect();
                    if !too_tight.is_empty() { out.push(f("error", "Bend radius too small", format!("{name}: {}. Lengthen the legs or use an elbow.", too_tight.join("; ")), vec![id.clone()])); }
                }
            }
        }
        // drains fall
        if is_drain(&p.system) {
            let mut flat = vec![]; let mut steep = vec![];
            for (i, w) in p.points.windows(2).enumerate() {
                let d = sub(w[1], w[0]); let l = len(d); if l < 0.25 { continue; }
                let horiz = (d[0] * d[0] + d[2] * d[2]).sqrt(); if horiz < 0.5 * l { continue; } // a stack
                let slope = d[1].abs() / horiz;
                if slope < 0.005 { flat.push(format!("segment {} ({})", i + 1, m(l))); } else if slope > 0.05 { steep.push(format!("segment {} ({:.0} %)", i + 1, slope * 100.0)); }
            }
            if !flat.is_empty() { out.push(f("warning", "Drain without fall", format!("{name}: {} run level; a waste pipe needs 1–2 % fall (≥ 5 mm/m, ideally 10–20 mm/m).", flat.join(", ")), vec![id.clone()])); }
            if !steep.is_empty() { out.push(f("info", "Steep drain", format!("{name}: {} fall more than 5 %; water outruns the solids.", steep.join(", ")), vec![id.clone()])); }
        }
        if p.system == "hotWater" && p.insulation <= 0.0 && p.length() > 1.0 { out.push(f("info", "Hot water pipe without insulation", format!("{name} ({}) loses heat; 13 mm insulation is usual on distribution pipes.", m(p.length())), vec![id.clone()])); }
        // brackets
        let my_brackets: Vec<&(String, Bracket)> = brs.iter().filter(|(_, b)| b.pipe.as_deref() == Some(id.as_str()) || (b.level == p.level && on_axis(p, b.position))).collect();
        let vertical = p.points.windows(2).all(|w| { let d = sub(w[1], w[0]); let l = len(d); l < 1e-6 || d[1].abs() / l > 0.9 });
        let spacing = bracket_spacing(mat, p.dn) * if vertical { 1.6 } else { 1.0 };
        if my_brackets.is_empty() {
            if p.length() > spacing { out.push(f("info", "No brackets", format!("{name} ({}) has no brackets; clip every {} ({}).", m(p.length()), m(spacing), if vertical { "vertical" } else { "horizontal" }), vec![id.clone()])); }
        } else {
            // supports along the pipe: brackets (by arc length) and the ends when connected
            let mut ts: Vec<f64> = my_brackets.iter().map(|(_, b)| arc_param(p, b.position)).collect();
            let y0 = net.h.base(p.level);
            for (end, t) in [(End::Start, 0.0), (End::End, p.length())] {
                let q = add(if end == End::Start { p.start() } else { p.end() }, [0.0, y0, 0.0]);
                if !matches!(net.attach_at(id, q, None), Attach::Open) { ts.push(t); }
            }
            ts.push(0.0); ts.push(p.length()); // ends count as a half-support: use the limit ×1.5 there
            ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let worst = ts.windows(2).map(|w| w[1] - w[0]).fold(0.0, f64::max);
            if worst > spacing * 1.2 { out.push(f("warning", "Bracket spacing too large", format!("{name}: {} between supports, the rule is {} for {} {}.", m(worst), m(spacing), mat.label, num(p.dn)), vec![id.clone()])); }
        }
    }
    // ---- ends
    for e in &net.ends {
        match net.end_attachment(e) {
            Attach::Open => out.push(f("warning", "Open pipe end", format!("The {} of a {} {} mm pipe is not connected to a fitting, a pipe or a fixture.", e.end.name(), system_label(&e.system), num(e.dn)), vec![e.pipe.clone()])),
            Attach::PipeEnd(o) => {
                if (o.dn - e.dn).abs() > 0.5 { out.push(f("error", "Diameter change without a reducer", format!("{} mm meets {} mm pipe end to end; insert a reducer.", num(e.dn), num(o.dn)), vec![e.pipe.clone(), o.pipe.clone()])); }
                else { out.push(f("info", "Pipes meet without a coupling", format!("Two {} mm pipe ends touch; join them with a coupling (connect / add_fitting), or they are one pipe.", num(e.dn)), vec![e.pipe.clone(), o.pipe.clone()])); }
                if o.system != e.system && !same_family(&o.system, &e.system) { out.push(f("warning", "Systems joined", format!("{} meets {}.", system_label(&e.system), system_label(&o.system)), vec![e.pipe.clone(), o.pipe.clone()])); }
            }
            Attach::PipeBody(other, _, _, _) => out.push(f("warning", "Branch without a tee", format!("A {} mm pipe ends on another pipe without a tee; use add_fitting with onPipe or draw the pipe from the other pipe so a tee is inserted.", num(e.dn)), vec![e.pipe.clone(), other])),
            Attach::Port(q) => {
                if q.name != "thread" && (q.dn - e.dn).abs() > 0.5 {
                    out.push(f(if q.tapware { "info" } else { "error" }, "Diameter mismatch at a fitting", format!("{} mm pipe on the {} port ({} mm) of a {}.", num(e.dn), q.name, num(q.dn), fitting(&q.kind).map(|k| k.label).unwrap_or("fitting")), vec![e.pipe.clone(), q.fitting.clone()]));
                }
                if let Some(p) = net.pipe(&e.pipe) { if !q.tapware && q.joint != p.joint && fitting(&q.kind).map(|k| !matches!(k.shape, Shape::Manifold)).unwrap_or(true) { out.push(f("info", "Mixed joint types", format!("A {} pipe on a {} {}.", joint_label(&p.joint), joint_label(&q.joint), fitting(&q.kind).map(|k| k.label).unwrap_or("fitting")), vec![e.pipe.clone(), q.fitting.clone()])); } }
            }
            Attach::Fixture(fx) => {
                if let Some(sys) = fixture_system(&fx.kind) { if !same_family(sys, &e.system) && !(fx.kind == "mixer" || fx.kind == "showerValve" || fx.kind == "boiler" || fx.kind == "manifold" || fx.kind == "radiator") { out.push(f("warning", "Pipe system does not match the fixture", format!("A {} pipe ends at a {} ({}).", system_label(&e.system), fx.kind, system_label(sys)), vec![e.pipe.clone(), fx.id.clone()])); } }
            }
        }
    }
    // ---- fittings
    for (id, ft) in &net.fittings {
        let ports: Vec<&crate::net::PortW> = net.ports.iter().filter(|q| &q.fitting == id && q.name != "thread").collect();
        let connected: Vec<Vec<&crate::net::PipeEnd>> = ports.iter().map(|q| net.ends_on_port(q)).collect();
        let n_conn = connected.iter().filter(|c| !c.is_empty()).count();
        if n_conn == 0 { out.push(f("info", "Loose fitting", format!("{} has no pipe on any port.", ft.kind_info().label), vec![id.clone()])); }
        else if n_conn < ports.len() && !ft.kind_info().tapware && ft.kind != "manifold" && ft.kind != "wallPlate" { out.push(f("warning", "Fitting with an open port", format!("{} has {} of {} ports open.", ft.kind_info().label, ports.len() - n_conn, ports.len()), vec![id.clone()])); }
        let systems: Vec<&str> = connected.iter().flatten().map(|e| e.system.as_str()).collect();
        if ft.kind_info().shape != Shape::Mixer && !systems.is_empty() { let first = systems[0]; if systems.iter().any(|s| !same_family(s, first)) { out.push(f("warning", "Different systems on one fitting", format!("{} joins {}.", ft.kind_info().label, systems.iter().map(|s| system_label(s)).collect::<Vec<_>>().join(" and ")), vec![id.clone()])); } }
        if let Some(k) = fitting(&ft.kind) { if !k.systems.is_empty() && !k.systems.contains(&ft.system.as_str()) { out.push(f("warning", "Fitting not meant for this system", format!("A {} is not used on {}.", k.label, system_label(&ft.system)), vec![id.clone()])); } }
        if !ft.kind_info().tapware && !ft.mat().joints.contains(&ft.joint.as_str()) && ft.kind != "manifold" { out.push(f("error", "Joint type not available for this material", format!("{} fittings do not exist in {}.", joint_label(&ft.joint), ft.mat().label), vec![id.clone()])); }
    }
    if out.is_empty() { out.push(f("ok", "No findings", "Pipes are connected, diameters match, bends and brackets are within the rules.".into(), vec![])); }
    out
}

/// Flow-wise compatible systems (a hot water pipe may feed a mixer's hot side…).
fn same_family(a: &str, b: &str) -> bool { a == b || (is_drain(a) && is_drain(b)) || (a.starts_with("heating") && b.starts_with("heating")) }

/// Is a (storey-relative) point on the pipe's axis (within 2 cm)?
fn on_axis(p: &Pipe, q: plannr_core::geom::P3) -> bool {
    p.points.windows(2).any(|w| { let d = sub(w[1], w[0]); let l = len(d); if l < 1e-6 { return false; } let u = scale(d, 1.0 / l); let t = dot(sub(q, w[0]), u).clamp(0.0, l); dist(add(w[0], scale(u, t)), q) < 0.02 })
}
/// Arc-length position of the nearest point on the polyline.
pub fn arc_param(p: &Pipe, q: plannr_core::geom::P3) -> f64 {
    let mut best = (f64::MAX, 0.0); let mut acc = 0.0;
    for w in p.points.windows(2) {
        let d = sub(w[1], w[0]); let l = len(d); if l < 1e-9 { continue; }
        let u = scale(d, 1.0 / l); let t = dot(sub(q, w[0]), u).clamp(0.0, l); let dd = dist(add(w[0], scale(u, t)), q);
        if dd < best.0 { best = (dd, acc + t); }
        acc += l;
    }
    best.1
}
