//! Masonry checks — a design aid, not a structural calculation: lintel bearings, piers between
//! openings, cavity width and ties (NBN EN 1996 / WTCB-NIT 271), floor spans per system (typical
//! manufacturer ranges for residential loads), and the U-value of the exterior walls against the EPB
//! requirement for renovation.

use plannr_core::model::Document;
use serde_json::{json, Value};

use crate::masonry::data::*;
use crate::masonry::uvalue;

fn finding(sev: &str, element: &str, title: &str, detail: String, value: Option<f64>, limit: Option<f64>) -> Value {
    let mut v = json!({ "severity": sev, "element": element, "title": title, "detail": detail });
    if let Some(x) = value { v["value"] = json!((x * 1000.0).round() / 1000.0); }
    if let Some(x) = limit { v["limit"] = json!((x * 1000.0).round() / 1000.0); }
    v
}

/// Largest clear span (m) of a floor system for residential loads (2 kN/m² + finishes).
pub fn max_span(s: &FloorSpec) -> f64 {
    let lerp = |x: f64, table: &[(f64, f64)]| -> f64 {
        if x <= table[0].0 { return table[0].1 * x / table[0].0; }
        for w in table.windows(2) { if x <= w[1].0 { let t = (x - w[0].0) / (w[1].0 - w[0].0); return w[0].1 + t * (w[1].1 - w[0].1); } }
        table[table.len() - 1].1
    };
    match s.system.as_str() {
        "hollowCore" => lerp(s.plank_thickness, &[(0.15, 6.5), (0.20, 8.5), (0.26, 10.5), (0.32, 12.5)]),
        "insitu" => s.slab_thickness * 25.0,
        // beam-and-block: by block height with a 40 mm topping
        _ => lerp(s.block_height, &[(0.12, 4.4), (0.16, 5.6), (0.20, 6.8), (0.24, 7.8)]) * if s.topping >= 0.04 { 1.0 } else { 0.9 },
    }
}

/// The clear span of a floor: its extent across the joists' span direction, minus the bearings.
/// The longest clear span of a floor measured along its joists, between the outline and the
/// walls of the storey below that cross the joist lines (sampled every 0.3 m across).
pub fn floor_span_on_walls(f: &Floor, s: &FloorSpec, walls: &[(String, Wall)], d: &Defaults) -> f64 {
    let (minx, maxx) = f.points.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
    let (minz, maxz) = f.points.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
    let span_x = match f.direction.as_str() { "x" => true, "z" => false, _ => maxx - minx <= maxz - minz };
    let (r0, r1) = if span_x { (minz, maxz) } else { (minx, maxx) };
    let below: Vec<&Wall> = walls.iter().map(|(_, w)| w).filter(|w| w.level + 1 == f.level).collect();
    let mut worst: f64 = 0.0;
    let mut r = r0 + 0.15;
    while r < r1 - 0.15 {
        // the joist line: along the span axis at repeat coordinate r
        let mut xs: Vec<f64> = vec![];
        let n = f.points.len();
        for i in 0..n {
            let (a, b) = (f.points[i], f.points[(i + 1) % n]);
            let (ar, br, aspan, bspan) = if span_x { (a[1], b[1], a[0], b[0]) } else { (a[0], b[0], a[1], b[1]) };
            if (ar <= r && br > r) || (br <= r && ar > r) { xs.push(aspan + (r - ar) / (br - ar) * (bspan - aspan)); }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for seg in xs.chunks(2) {
            if seg.len() < 2 { continue; }
            let (s0, s1) = (seg[0], seg[1]);
            // supports: the two ends, and every wall below crossing the line inside the segment (its face on each side)
            let mut sup: Vec<(f64, f64)> = vec![(s0, s0 + s.bearing), (s1 - s.bearing, s1)];
            for w in &below {
                let (wa, wb) = if span_x { ((w.a[0], w.a[1]), (w.b[0], w.b[1])) } else { ((w.a[1], w.a[0]), (w.b[1], w.b[0])) };
                // the wall must run across the joists (mostly along the repeat axis) and reach line r
                if (wb.1 - wa.1).abs() < 1e-6 { continue; }
                let t = (r - wa.1) / (wb.1 - wa.1); if !(-0.01..=1.01).contains(&t) { continue; }
                let pos = wa.0 + t * (wb.0 - wa.0);
                let half = w.thickness(d) / 2.0;
                if pos > s0 + 0.2 && pos < s1 - 0.2 { sup.push((pos - half, pos + half)); }
            }
            sup.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            for p in sup.windows(2) { worst = worst.max(p[1].0 - p[0].1); }
        }
        r += 0.3;
    }
    worst
}

pub fn floor_span(f: &Floor, s: &FloorSpec) -> f64 {
    let (minx, maxx) = f.points.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
    let (minz, maxz) = f.points.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
    let (wx, wz) = (maxx - minx, maxz - minz);
    let along = match f.direction.as_str() { "x" => wx, "z" => wz, _ => wx.min(wz) };
    (along - 2.0 * s.bearing).max(0.0)
}

pub fn check(doc: &Document) -> Value {
    let h = House::read(doc);
    let d = &h.defaults;
    let mut out: Vec<Value> = vec![];
    // does another wall continue this one straight on at its start / end?
    let continues = |id: &str, w: &Wall, at_end: bool| -> bool {
        let p = if at_end { w.b } else { w.a };
        let l = w.length().max(1e-9); let u = [(w.b[0] - w.a[0]) / l, (w.b[1] - w.a[1]) / l];
        h.walls.iter().any(|(oid, o)| {
            if oid == id || o.level != w.level { return false; }
            let lo = o.length().max(1e-9); let uo = [(o.b[0] - o.a[0]) / lo, (o.b[1] - o.a[1]) / lo];
            ((u[0] * uo[1] - u[1] * uo[0]).abs() < 0.02) && [o.a, o.b].iter().any(|q| ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt() < 0.02)
        })
    };
    for (id, w) in &h.walls {
        let l = w.length();
        let mut ops: Vec<&Opening> = w.openings.iter().collect();
        ops.sort_by(|a, b| a.offset.partial_cmp(&b.offset).unwrap());
        for (k, o) in ops.iter().enumerate() {
            let bearing = o.lintel.bearing.unwrap_or(d.lintel.bearing);
            let name = format!("{} {}", if o.kind == "door" { "Door" } else { "Window" }, k + 1);
            if bearing < 0.10 { out.push(finding("error", id, "Lintel bearing too short", format!("{name}: {:.0} mm bearing; at least 100 mm (150 mm advised).", bearing * 1000.0), Some(bearing), Some(0.10))); }
            else if bearing < 0.15 { out.push(finding("warning", id, "Lintel bearing", format!("{name}: {:.0} mm bearing; 150 mm advised.", bearing * 1000.0), Some(bearing), Some(0.15))); }
            let (left, right) = (o.offset, l - o.offset - o.width);
            for (side, room) in [("start", left), ("end", right)] {
                // a wall end at a corner / junction still carries the lintel only if the pier is wide enough;
                // where the wall simply continues in a collinear piece there is no end
                if continues(id, w, side == "end") { continue; }
                // in a partition the lintel of a door against a corner bears on the crossing wall: worth a note only
                if room < bearing.max(0.10) && room < 0.30 { out.push(finding(if w.is_cavity() { "warning" } else { "info" }, id, "Opening close to the wall end", format!("{name} is {:.0} mm from the wall {side}: the lintel needs {:.0} mm bearing and the pier at least 300 mm.", room * 1000.0, bearing * 1000.0), Some(room), Some(0.30))); }
            }
            if let Some(next) = ops.get(k + 1) {
                let pier = next.offset - (o.offset + o.width);
                // openings that touch form one combined frame under one lintel
                if pier < 0.005 { out.push(finding("info", id, "Combined opening", format!("{name} and the next opening touch: one lintel over {:.2} m.", next.offset + next.width - o.offset), None, None)); continue; }
                if pier < 2.0 * bearing { out.push(finding("warning", id, "Narrow pier between openings", format!("{:.0} mm between {name} and the next opening: the two lintels need {:.0} mm.", pier * 1000.0, 2.0 * bearing * 1000.0), Some(pier), Some(2.0 * bearing))); }
            }
        }
        if w.is_cavity() {
            let c = w.cavity_of(d);
            if c.width < 0.03 || c.width > 0.15 { out.push(finding("warning", id, "Cavity width", format!("{:.0} mm cavity; 30–150 mm is usual (wider needs special ties).", c.width * 1000.0), Some(c.width), Some(0.15))); }
            if c.ties.per_m2 < 4.0 { out.push(finding("error", id, "Too few cavity ties", format!("{:.1} ties per m²; at least 4 (5 advised, more at openings and edges).", c.ties.per_m2), Some(c.ties.per_m2), Some(4.0))); }
            else if c.ties.per_m2 < 5.0 { out.push(finding("info", id, "Cavity ties", format!("{:.1} ties per m²; 5 advised.", c.ties.per_m2), Some(c.ties.per_m2), Some(5.0))); }
            if c.insulation.kind == "board" && c.width - c.insulation.thickness < 0.02 { out.push(finding("warning", id, "No air gap in front of the boards", format!("{:.0} mm left between the boards and the outer leaf; keep ≥ 20 mm (or fill the cavity completely).", (c.width - c.insulation.thickness) * 1000.0), Some(c.width - c.insulation.thickness), Some(0.02))); }
        }
    }
    // U-values per build-up of the cavity (exterior) walls
    let mut seen: Vec<String> = vec![];
    for (id, w) in &h.walls {
        if !w.is_cavity() { continue; }
        let key = uvalue::buildup_key(w, d);
        if seen.contains(&key) { continue; }
        seen.push(key);
        let (_, _, u) = uvalue::layers(w, d);
        let n = h.walls.iter().filter(|(_, x)| x.is_cavity() && uvalue::buildup_key(x, d) == uvalue::buildup_key(w, d)).count();
        if u > uvalue::U_MAX {
            let mut detail = format!("U = {u:.2} W/m²K for {n} wall(s) with this build-up; the EPB requirement for renovated walls is {:.2}.", uvalue::U_MAX);
            if w.cavity_of(d).insulation.kind == "none" {
                let r = uvalue::report(w, d);
                if let Some(alts) = r["withFill"].as_array() {
                    let s: Vec<String> = alts.iter().map(|a| format!("{} {:.2}", a["material"].as_str().unwrap_or(""), a["U"].as_f64().unwrap_or(0.0))).collect();
                    detail.push_str(&format!(" A full cavity fill would give U ≈ {}.", s.join(", ")));
                }
            }
            out.push(finding("warning", id, "Wall U-value above the EPB requirement", detail, Some(u), Some(uvalue::U_MAX)));
        } else {
            out.push(finding("ok", id, "Wall U-value", format!("U = {u:.2} W/m²K ≤ {:.2} ({n} wall(s)).", uvalue::U_MAX), Some(u), Some(uvalue::U_MAX)));
        }
    }
    for (id, f) in &h.floors {
        let s = f.spec(&d.floor);
        let span = { let on_walls = floor_span_on_walls(f, &s, &h.walls, d); if on_walls > 0.0 { on_walls } else { floor_span(f, &s) } }; let max = max_span(&s);
        let what = match s.system.as_str() { "hollowCore" => format!("hollow-core {:.0} mm", s.plank_thickness * 1000.0), "insitu" => format!("in-situ slab {:.0} mm", s.slab_thickness * 1000.0), _ => format!("beam-and-block {:.0}+{:.0}", s.block_height * 100.0, s.topping * 100.0) };
        if span > max { out.push(finding("error", id, "Floor span too long", format!("{what}: clear span {span:.2} m, about {max:.2} m possible for residential loads — deeper blocks / planks or an extra bearing."), Some(span), Some(max))); }
        else if span > 0.9 * max { out.push(finding("warning", id, "Floor span near its limit", format!("{what}: clear span {span:.2} m of about {max:.2} m — check with the manufacturer's span table."), Some(span), Some(max))); }
        else { out.push(finding("ok", id, "Floor span", format!("{what}: clear span {span:.2} m ≤ {max:.2} m."), Some(span), Some(max))); }
        if s.system == "beamBlock" {
            if s.spacing > 0.75 { out.push(finding("warning", id, "Joist spacing", format!("{:.0} mm between joists; infill blocks are made for ≤ 750 mm.", s.spacing * 1000.0), Some(s.spacing), Some(0.75))); }
            if s.topping < 0.04 { out.push(finding("warning", id, "Thin topping", format!("{:.0} mm topping; at least 40 mm with a mesh.", s.topping * 1000.0), Some(s.topping), Some(0.04))); }
            if !s.mesh { out.push(finding("warning", id, "No mesh in the topping", "The topping (druklaag) needs a reinforcement mesh.".into(), None, None)); }
        }
        if s.bearing < 0.08 { out.push(finding("warning", id, "Short floor bearing", format!("{:.0} mm bearing on the walls; ≥ 100 mm usual.", s.bearing * 1000.0), Some(s.bearing), Some(0.1))); }
    }
    let errors = out.iter().filter(|f| f["severity"] == "error").count();
    let warnings = out.iter().filter(|f| f["severity"] == "warning").count();
    if out.is_empty() { out.push(finding("info", "", "Nothing to check", "No masonry walls or floors yet.".into(), None, None)); }
    json!({ "findings": out, "errors": errors, "warnings": warnings, "requirementU": uvalue::U_MAX })
}
