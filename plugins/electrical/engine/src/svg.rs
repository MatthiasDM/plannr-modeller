//! Situatieschema (plan with AREI symbols) and eendraadschema (one-line diagram) as SVG text.

use crate::data::P2;

use crate::data::*;

fn esc(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;") }
fn f(x: f64) -> String { format!("{:.1}", x) }

/// One AREI/IEC symbol centred at (x, y), `r` = nominal radius, rotated by `deg`.
pub fn symbol(kind: &str, x: f64, y: f64, r: f64, color: &str, deg: f64) -> String {
    let lw = (r * 0.14).max(1.2);
    let body = match kind {
        "ceilingLight" | "wallLight" => {
            let d = r * 0.72;
            let mut s = format!(r##"<circle cx="0" cy="0" r="{}" fill="none"/><line x1="{}" y1="{}" x2="{}" y2="{}"/><line x1="{}" y1="{}" x2="{}" y2="{}"/>"##, f(r), f(-d), f(-d), f(d), f(d), f(-d), f(d), f(d), f(-d));
            if kind == "wallLight" { s.push_str(&format!(r##"<line x1="0" y1="{}" x2="0" y2="{}"/>"##, f(r), f(r * 1.7))); }
            s
        }
        "outlet" | "dataPoint" => {
            // half disc, flat side towards the wall, with the earth stem (AREI stopcontact)
            let mut s = format!(r##"<path d="M {} 0 A {} {} 0 0 1 {} 0 Z" fill="{color}"/><line x1="0" y1="0" x2="0" y2="{}"/><line x1="{}" y1="{}" x2="{}" y2="{}"/>"##, f(-r), f(r), f(r), f(r), f(-r * 1.5), f(-r * 0.7), f(-r * 0.9), f(r * 0.7), f(-r * 0.9));
            if kind == "dataPoint" { s.push_str(&format!(r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#fff"/>"##, f(-r * 0.5), f(r * 0.4), f(r * 0.5), f(r * 0.4))); }
            s
        }
        "switchPlate" => format!(r##"<circle cx="0" cy="0" r="{}" fill="{color}"/><line x1="0" y1="0" x2="{}" y2="{}"/><line x1="{}" y1="{}" x2="{}" y2="{}"/>"##, f(r * 0.34), f(r * 1.5), f(-r * 1.5), f(r * 1.5), f(-r * 1.5), f(r), f(-r * 2.0)),
        "panel" => format!(r##"<rect x="{}" y="{}" width="{}" height="{}" fill="#fff"/><line x1="{}" y1="{}" x2="{}" y2="{}"/><text x="0" y="{}" font-size="{}" text-anchor="middle" stroke="none" fill="{color}">BORD</text>"##, f(-r), f(-r * 0.7), f(r * 2.0), f(r * 1.4), f(-r), f(-r * 0.7), f(r), f(r * 0.7), f(r * 1.9), f(r * 0.9)),
        _ => format!(r##"<circle cx="0" cy="0" r="{}" fill="{color}"/>"##, f(r * 0.5)),
    };
    format!(r##"<g transform="translate({} {}) rotate({})" stroke="{color}" stroke-width="{}" stroke-linecap="round">{body}</g>"##, f(x), f(y), f(deg), f(lw))
}

pub const LEGEND: [(&str, &str); 6] = [("ceilingLight", "Lichtpunt"), ("wallLight", "Wandlicht"), ("outlet", "Stopcontact"), ("switchPlate", "Schakelaar"), ("dataPoint", "Datacontact"), ("panel", "Verdeelbord")];

pub struct SitOpts { pub level: usize, pub scale: f64, pub show_conduits: bool, pub title: String }

/// Situation plan of one storey.
pub fn situation(s: &Settings, h: &House, conduits: &[(String, Conduit)], o: &SitOpts) -> String {
    let rooms: Vec<&Rm> = h.rooms.iter().filter(|r| r.level == o.level).collect();
    let walls: Vec<&Wl> = h.walls.iter().filter(|w| w.level == o.level).collect();
    let fx: Vec<&Fx> = h.fixtures.iter().filter(|x| x.level == o.level && ELECTRICAL_KINDS.contains(&x.kind.as_str())).collect();
    let mut pts: Vec<P2> = rooms.iter().flat_map(|r| r.points.iter().copied()).chain(walls.iter().flat_map(|w| [w.a, w.b])).chain(fx.iter().map(|x| [x.position[0], x.position[2]])).collect();
    if pts.is_empty() { pts.push([0.0, 0.0]); pts.push([5.0, 5.0]); }
    let (minx, maxx) = pts.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
    let (minz, maxz) = pts.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p[1]), a.1.max(p[1])));
    let sc = o.scale; let margin = 60.0; let legend_h = 90.0; let title_h = 70.0;
    let w = ((maxx - minx) * sc + margin * 2.0).max(520.0); let hgt = (maxz - minz) * sc + margin * 2.0 + legend_h + title_h;
    let tx = |x: f64| (x - minx) * sc + margin; let ty = |z: f64| (z - minz) * sc + margin + title_h;
    let circuit_of = |id: &str| s.circuits.iter().find(|c| c.points.contains(&id.to_string()));
    let mut out = String::new();
    out.push_str(&format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}" font-family="Helvetica, Arial, sans-serif" font-size="11">"##, f(w), f(hgt), f(w), f(hgt)));
    out.push_str(&format!(r##"<rect width="100%" height="100%" fill="#fff"/><rect x="0.5" y="0.5" width="{}" height="{}" fill="none" stroke="#222"/>"##, f(w - 1.0), f(hgt - 1.0)));
    // title block
    let addr = if s.board.address.is_empty() { "—".to_string() } else { s.board.address.clone() };
    out.push_str(&format!(r##"<g><text x="{m}" y="28" font-size="16" font-weight="bold">SITUATIESCHEMA — {}</text><text x="{m}" y="48" fill="#444">{} · schaal 1:{} · {} V {} · hoofdautomaat {} A · differentieel {} mA type {} · {}</text><text x="{m}" y="62" fill="#666" font-size="10">Symbolen volgens AREI / IEC 60617 · gegenereerd met Plannr</text></g>"##,
        esc(&h.storey_name(o.level).to_uppercase()), esc(&addr), (100.0 * 37.8 / sc).round() as i64, if s.board.phases == 3 { 400 } else { 230 }, if s.board.phases == 3 { "3F+N" } else { "1F+N" }, s.board.main_breaker, s.board.main_differential, esc(&s.board.differential_type), esc(&o.title), m = margin));
    // rooms
    for r in &rooms {
        if r.points.len() < 3 { continue; }
        let path: Vec<String> = r.points.iter().enumerate().map(|(i, p)| format!("{}{} {}", if i == 0 { "M" } else { "L" }, f(tx(p[0])), f(ty(p[1])))).collect();
        let c = r.points.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0], a[1] + p[1]]); let n = r.points.len() as f64;
        out.push_str(&format!(r##"<path d="{} Z" fill="#f6f4ef" stroke="#222" stroke-width="3"/><text x="{}" y="{}" text-anchor="middle" fill="#555" font-size="12">{}</text>"##, path.join(" "), f(tx(c[0] / n)), f(ty(c[1] / n) - 10.0), esc(&r.name)));
    }
    for wl in &walls { out.push_str(&format!(r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#222" stroke-width="3"/>"##, f(tx(wl.a[0])), f(ty(wl.a[1])), f(tx(wl.b[0])), f(ty(wl.b[1])))); }
    // conduits
    if o.show_conduits {
        for (_, k) in conduits.iter().filter(|(_, k)| k.level == o.level) {
            let col = k.circuit.as_deref().and_then(|cid| s.circuits.iter().find(|c| c.id == cid)).map(|c| circuit_color(c.number)).unwrap_or("#999");
            let d: Vec<String> = k.points.iter().enumerate().map(|(i, p)| format!("{}{} {}", if i == 0 { "M" } else { "L" }, f(tx(p[0])), f(ty(p[2])))).collect();
            out.push_str(&format!(r##"<path d="{}" fill="none" stroke="{col}" stroke-width="1.2" stroke-dasharray="5 3" opacity="0.8"/>"##, d.join(" ")));
        }
    }
    // points: wall-mounted symbols turn their flat side / stem towards the nearest wall
    let r = (sc * 0.16).clamp(5.0, 9.0);
    let segs: Vec<(P2, P2)> = rooms.iter().flat_map(|rm| (0..rm.points.len()).map(move |i| (rm.points[i], rm.points[(i + 1) % rm.points.len()]))).chain(walls.iter().map(|w| (w.a, w.b))).collect();
    for x in &fx {
        let c = circuit_of(&x.id);
        let col = c.map(|c| circuit_color(c.number)).unwrap_or(if x.kind == "panel" { "#222" } else { "#999" });
        let p = [x.position[0], x.position[2]];
        let deg = match nearest_wall_dir(p, &segs) { Some(d) if x.kind != "ceilingLight" => d[1].atan2(d[0]).to_degrees() - 90.0, _ => x.rotation.to_degrees() };
        out.push_str(&symbol(&x.kind, tx(x.position[0]), ty(x.position[2]), r, col, deg));
        if let Some(c) = c { out.push_str(&format!(r##"<text x="{}" y="{}" font-size="9" font-weight="bold" fill="{col}">{}</text>"##, f(tx(x.position[0]) + r * 1.1), f(ty(x.position[2]) - r * 0.9), c.number)); }
    }
    // legend
    let ly = hgt - legend_h + 8.0;
    out.push_str(&format!(r##"<rect x="{}" y="{}" width="{}" height="{}" fill="#fafafa" stroke="#999"/><text x="{}" y="{}" font-weight="bold" fill="#444">Legende</text>"##, f(margin), f(ly), f(w - margin * 2.0), f(legend_h - 16.0), f(margin + 10.0), f(ly + 16.0)));
    for (i, (kind, label)) in LEGEND.iter().enumerate() {
        let lx = margin + 20.0 + i as f64 * 120.0;
        out.push_str(&symbol(kind, lx, ly + 38.0, 7.0, "#333", 0.0));
        out.push_str(&format!(r##"<text x="{}" y="{}" fill="#333">{}</text>"##, f(lx + 16.0), f(ly + 42.0), label));
    }
    let used: Vec<&Circuit> = s.circuits.iter().filter(|c| fx.iter().any(|x| c.points.contains(&x.id))).collect();
    let mut cx = margin + 20.0;
    for c in used {
        let txt = format!("{} {}", c.number, c.name);
        out.push_str(&format!(r##"<rect x="{}" y="{}" width="10" height="10" fill="{}"/><text x="{}" y="{}" fill="#333" font-size="10">{}</text>"##, f(cx), f(ly + 54.0), circuit_color(c.number), f(cx + 14.0), f(ly + 63.0), esc(&txt)));
        cx += 14.0 + txt.len() as f64 * 6.0 + 10.0;
        if cx > w - margin - 80.0 { break; }
    }
    out.push_str("</svg>");
    out
}

/// One-line diagram (eendraadschema) of the whole installation.
pub fn oneline(s: &Settings, h: &House, title: &str) -> String {
    let b = &s.board;
    let col_w = 92.0; let left = 70.0; let top = 180.0; let n = s.circuits.len().max(1);
    let w = (left * 2.0 + n as f64 * col_w).max(900.0); let hgt = 700.0;
    let mut out = String::new();
    out.push_str(&format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}" font-family="Helvetica, Arial, sans-serif" font-size="10"><rect width="100%" height="100%" fill="#fff"/><rect x="0.5" y="0.5" width="{}" height="{}" fill="none" stroke="#222"/>"##, f(w), f(hgt), f(w), f(hgt), f(w - 1.0), f(hgt - 1.0)));
    let addr = if b.address.is_empty() { "—".to_string() } else { b.address.clone() };
    out.push_str(&format!(r##"<text x="{left}" y="28" font-size="16" font-weight="bold">EENDRAADSCHEMA</text><text x="{left}" y="46" fill="#444">{} · {} · aarding {} · kortsluitvermogen {} kA · kabel {}</text><text x="{left}" y="60" fill="#666">EAN {} · meter {} · gegenereerd met Plannr</text>"##,
        esc(&addr), esc(title), esc(&b.earthing), b.short_circuit_ka, esc(&b.default_cable_type), esc(if b.ean.is_empty() { "—" } else { &b.ean }), esc(if b.meter_number.is_empty() { "—" } else { &b.meter_number })));
    // head: kWh meter → main breaker → head differential → busbar
    let hx = left; let hy = 90.0;
    out.push_str(&format!(r##"<g stroke="#222" stroke-width="1.5" fill="#fff"><rect x="{hx}" y="{}" width="44" height="26"/><text x="{}" y="{}" text-anchor="middle" stroke="none" fill="#222" font-weight="bold">kWh</text><line x1="{}" y1="{}" x2="{}" y2="{}"/>"##, f(hy - 13.0), f(hx + 22.0), f(hy + 4.0), f(hx + 44.0), f(hy), f(hx + 70.0), f(hy)));
    let poles = if b.phases == 3 { "4P" } else { "2P" };
    out.push_str(&format!(r##"<rect x="{}" y="{}" width="54" height="26"/><text x="{}" y="{}" text-anchor="middle" stroke="none" fill="#222">{} A {poles}</text><line x1="{}" y1="{}" x2="{}" y2="{}"/>"##, f(hx + 70.0), f(hy - 13.0), f(hx + 97.0), f(hy + 4.0), b.main_breaker, f(hx + 124.0), f(hy), f(hx + 150.0), f(hy)));
    if b.main_differential > 0 {
        out.push_str(&format!(r##"<rect x="{}" y="{}" width="70" height="26"/><text x="{}" y="{}" text-anchor="middle" stroke="none" fill="#222">Δ {} mA · {}</text><line x1="{}" y1="{}" x2="{}" y2="{}"/>"##, f(hx + 150.0), f(hy - 13.0), f(hx + 185.0), f(hy + 4.0), b.main_differential, esc(&b.differential_type), f(hx + 220.0), f(hy), f(hx + 246.0), f(hy)));
    }
    // down from the head to a busbar that spans every circuit
    let head_end = if b.main_differential > 0 { hx + 246.0 } else { hx + 150.0 };
    let bus_y = hy + 42.0;
    out.push_str(&format!(r##"<line x1="{}" y1="{}" x2="{}" y2="{}"/><line x1="{}" y1="{}" x2="{}" y2="{}" stroke-width="4"/></g>"##, f(head_end), f(hy), f(head_end), f(bus_y), f(left), f(bus_y), f(w - left), f(bus_y)));
    out.push_str(&format!(r##"<text x="{}" y="{}" fill="#444" font-size="9">{} V {} · {:.1} kW</text>"##, f(head_end + 6.0), f(hy - 16.0), if b.phases == 3 { 400 } else { 230 }, if b.phases == 3 { "3F+N" } else { "1F+N" }, b.capacity_kw()));
    // circuits as columns
    for (i, c) in s.circuits.iter().enumerate() {
        let x = left + i as f64 * col_w + col_w / 2.0;
        let col = circuit_color(c.number);
        out.push_str(&format!(r##"<g stroke="{col}" stroke-width="1.5" fill="#fff"><line x1="{x}" y1="{}" x2="{x}" y2="{}"/>"##, f(bus_y), f(top)));
        out.push_str(&format!(r##"<circle cx="{x}" cy="{}" r="10" fill="{col}" stroke="none"/><text x="{x}" y="{}" text-anchor="middle" fill="#fff" stroke="none" font-weight="bold">{}</text>"##, f(top + 10.0), f(top + 14.0), c.number));
        // breaker symbol: a box with the rating
        out.push_str(&format!(r##"<line x1="{x}" y1="{}" x2="{x}" y2="{}"/><rect x="{}" y="{}" width="60" height="22" rx="2"/><text x="{x}" y="{}" text-anchor="middle" stroke="none" fill="#222">{}P {}{} A</text>"##, f(top + 20.0), f(top + 34.0), f(x - 30.0), f(top + 34.0), f(top + 49.0), c.resolved_poles(b.phases), c.resolved_curve(), c.breaker));
        let rcd = c.rcd_label();
        out.push_str(&format!(r##"<line x1="{x}" y1="{}" x2="{x}" y2="{}"/><text x="{x}" y="{}" text-anchor="middle" stroke="none" fill="{}">Δ {}</text>"##, f(top + 56.0), f(top + 70.0), f(top + 82.0), if rcd == "—" { "#c0392b" } else { "#1a73e8" }, rcd));
        out.push_str(&format!(r##"<line x1="{x}" y1="{}" x2="{x}" y2="{}"/><text x="{x}" y="{}" text-anchor="middle" stroke="none" fill="#222" font-size="9">{}</text>"##, f(top + 86.0), f(top + 110.0), f(top + 122.0), esc(&c.cable_label(b))));
        // points in sequence, grouped by kind with counts
        let mut groups: Vec<(String, usize)> = vec![];
        for kind in ["ceilingLight", "wallLight", "outlet", "switchPlate", "dataPoint"] { let n = c.points.iter().filter(|id| h.fixture(id).map(|f| f.kind == kind).unwrap_or(false)).count(); if n > 0 { groups.push((kind.to_string(), n)); } }
        let mut y = top + 130.0;
        out.push_str(&format!(r##"<line x1="{x}" y1="{}" x2="{x}" y2="{}"/>"##, f(top + 126.0), f(y + groups.len() as f64 * 34.0 + 4.0)));
        for (kind, count) in &groups {
            y += 20.0;
            out.push_str(&symbol(kind, x, y, 7.0, col, 0.0));
            if *count > 1 { out.push_str(&format!(r##"<text x="{}" y="{}" stroke="none" fill="#222" font-size="9">{count}×</text>"##, f(x + 12.0), f(y + 4.0))); }
            y += 14.0;
        }
        if groups.is_empty() { out.push_str(&format!(r##"<text x="{x}" y="{}" text-anchor="middle" stroke="none" fill="#888" font-size="9">{}</text>"##, f(y + 20.0), if type_info(&c.type_).dedicated { "toestel" } else { "geen punten" })); }
        // name
        out.push_str(&format!(r##"<text transform="translate({} {}) rotate(-90)" text-anchor="start" stroke="none" fill="#222" font-size="10">{}</text></g>"##, f(x + 4.0), f(hgt - 62.0), esc(&truncate(&c.name, 26))));
    }
    // legend
    let ly = hgt - 28.0;
    out.push_str(&format!(r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#bbb"/>"##, f(left), f(hgt - 52.0), f(w - left), f(hgt - 52.0)));
    for (i, (kind, label)) in LEGEND.iter().enumerate().take(5) {
        let lx = left + i as f64 * 130.0;
        out.push_str(&symbol(kind, lx, ly, 6.0, "#333", 0.0));
        out.push_str(&format!(r##"<text x="{}" y="{}" fill="#333">{}</text>"##, f(lx + 14.0), f(ly + 4.0), label));
    }
    out.push_str(&format!(r##"<text x="{}" y="{}" text-anchor="end" fill="#333">Keurder (naam, datum, handtekening): ______________________</text></svg>"##, f(w - left), f(ly + 4.0)));
    out
}

/// Unit vector from `p` towards the closest wall segment within 0.6 m, if any.
fn nearest_wall_dir(p: P2, segs: &[(P2, P2)]) -> Option<P2> {
    let mut best: Option<(f64, P2)> = None;
    for (a, b) in segs {
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]); let l2 = dx * dx + dz * dz; if l2 < 1e-12 { continue; }
        let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / l2).clamp(0.0, 1.0);
        let q = [a[0] + dx * t, a[1] + dz * t]; let d = [q[0] - p[0], q[1] - p[1]]; let dist = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if dist < 0.6 && dist > 1e-6 && best.map(|b| dist < b.0).unwrap_or(true) { best = Some((dist, [d[0] / dist, d[1] / dist])); }
    }
    best.map(|b| b.1)
}

fn truncate(s: &str, n: usize) -> String { if s.chars().count() <= n { s.to_string() } else { s.chars().take(n - 1).collect::<String>() + "…" } }
