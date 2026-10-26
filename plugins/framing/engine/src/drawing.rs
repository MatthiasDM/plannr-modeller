//! 2D framing drawings as SVG: a wall elevation (every member, rough openings, stud layout and
//! dimensions, title block) and a storey framing plan (walls, joists, beams, roof members, stairs).
//! Every member is drawn by exactly one element with `class="member …"`.

use plannr_core::geom::{cross, dot, sub, P3};
use plannr_core::model::{Custom, Document, Entity};

use crate::data::*;
use crate::engineering::wall_height;
use crate::expand::*;
use crate::hardware::world;

const UP: P3 = [0.0, 1.0, 0.0];

fn esc(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;") }
fn f1(v: f64) -> String { format!("{:.1}", v) }

fn fill(category: &str) -> &'static str {
    match category {
        "plates" => "#d8b784", "headers" | "beams" => "#bf9552", "cripples" => "#ecd9b4", "posts" => "#a87c45",
        "joists" | "rim" | "blocking" => "#e3c597", "trusses" | "rafters" | "ridge" => "#d9c08f", "stringers" | "treads" | "risers" => "#cfa86b",
        _ => "#e3c597",
    }
}

fn custom<'a>(doc: &'a Document, id: &str) -> Option<&'a Custom> {
    doc.entities.iter().find_map(|e| match e { Entity::Custom(c) if c.id == id => Some(c), _ => None })
}

/// Elevation of a framing wall seen from its exterior side.
pub fn wall_svg(doc: &Document, wall_id: &str) -> Result<String, String> {
    let c = custom(doc, wall_id).filter(|c| c.type_ == T_WALL).ok_or_else(|| format!("no framing wall {wall_id}"))?;
    let w: Wall = serde_json::from_value(c.data.clone()).map_err(|e| e.to_string())?;
    let house = House::read(doc);
    let ex = expand_element(c, &house, "");
    let Some(fr) = ex.members.iter().find(|m| m.key == "plate.bottom.0").or_else(|| ex.members.first()).cloned() else { return Err("the wall has no members".into()) };
    let o = fr.origin;
    // looking at the exterior face: the viewer faces −ez; their right hand is (−ez) × up
    let right = cross([-fr.ez[0], -fr.ez[1], -fr.ez[2]], UP);
    let proj = |p: P3| -> (f64, f64) { (dot(sub(p, o), right), p[1] - o[1]) };
    let rects: Vec<(String, &'static str, f64, f64, f64, f64)> = ex.members.iter().map(|m| {
        let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for &x in &m.r[0] { for &y in &m.r[1] { for &z in &m.r[2] { let (px, py) = proj(world(m, x, y, z)); x0 = x0.min(px); x1 = x1.max(px); y0 = y0.min(py); y1 = y1.max(py); } } }
        (m.key.clone(), m.category, x0, x1, y0, y1)
    }).collect();
    let xmin = rects.iter().map(|r| r.2).fold(f64::MAX, f64::min);
    let xmax = rects.iter().map(|r| r.3).fold(f64::MIN, f64::max);
    let h = wall_height(&w, &house).max(rects.iter().map(|r| r.5).fold(0.0, f64::max));
    let len = xmax - xmin;
    let s = (1400.0 / len.max(0.5)).min(100.0); // px per m
    let (ml, mt) = (90.0, 70.0);
    let (width, height) = (ml + len * s + 60.0, mt + h * s + 170.0);
    let sx = |x: f64| ml + (x - xmin) * s;
    let sy = |y: f64| mt + (h - y) * s;
    let mut out = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}" font-family="Helvetica, Arial, sans-serif" font-size="11"><rect x="0" y="0" width="{}" height="{}" fill="#ffffff"/>"##, f1(width), f1(height), f1(width), f1(height), f1(width), f1(height));
    // members
    out.push_str(r##"<g stroke="#5b4630" stroke-width="0.8">"##);
    for (key, cat, x0, x1, y0, y1) in &rects {
        out.push_str(&format!(r##"<rect class="member {}" data-key="{}" x="{}" y="{}" width="{}" height="{}" fill="{}"/>"##, cat, esc(key), f1(sx(*x0)), f1(sy(*y1)), f1((x1 - x0) * s), f1((y1 - y0) * s), fill(cat)));
    }
    out.push_str("</g>");
    // rough openings (offsets are measured from the wall start `a` = the frame origin)
    let sgn = dot([fr.ex[0], fr.ex[1], fr.ex[2]], right).signum();
    let bp = w.bottom_plates as f64 * w.plate()[0];
    for (i, op) in w.openings.iter().enumerate() {
        let (a, b) = (op.offset * sgn, (op.offset + op.width) * sgn);
        let (x0, x1) = (a.min(b), a.max(b));
        let y0 = if op.kind == "door" { bp } else { op.sill };
        let y1 = y0 + op.height;
        out.push_str(&format!(r##"<rect class="opening" x="{}" y="{}" width="{}" height="{}" fill="none" stroke="#d93025" stroke-width="1.2" stroke-dasharray="6 3"/>"##, f1(sx(x0)), f1(sy(y1)), f1((x1 - x0) * s), f1((y1 - y0) * s)));
        let label = if op.kind == "door" { format!("D{}: {:.0} × {:.0}", i + 1, op.width * 1000.0, op.height * 1000.0) } else { format!("W{}: {:.0} × {:.0}, sill {:.0}", i + 1, op.width * 1000.0, op.height * 1000.0, op.sill * 1000.0) };
        out.push_str(&format!(r##"<text x="{}" y="{}" text-anchor="middle" fill="#d93025">{}</text>"##, f1(sx((x0 + x1) / 2.0)), f1(sy((y0 + y1) / 2.0)), esc(&label)));
    }
    // stud layout: on-centre ticks over the wall, measured from the left end of the drawing
    let mut centres: Vec<f64> = rects.iter().filter(|r| r.0.starts_with("stud.")).map(|r| (r.2 + r.3) / 2.0).collect();
    centres.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let ty = mt - 18.0;
    out.push_str(&format!(r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#333"/>"##, f1(sx(xmin)), f1(ty), f1(sx(xmax)), f1(ty)));
    for (k, c) in centres.iter().enumerate() {
        out.push_str(&format!(r##"<line x1="{x}" y1="{}" x2="{x}" y2="{}" stroke="#333"/>"##, f1(ty - 4.0), f1(ty + 4.0), x = f1(sx(*c))));
        if k % 2 == 0 || s * w.stud_spacing > 40.0 { out.push_str(&format!(r##"<text x="{}" y="{}" text-anchor="middle" font-size="9" fill="#333">{:.0}</text>"##, f1(sx(*c)), f1(ty - 7.0), (c - xmin) * 1000.0)); }
    }
    // overall length and height
    let dy = sy(0.0) + 30.0;
    out.push_str(&format!(r##"<g stroke="#111"><line x1="{a}" y1="{y}" x2="{b}" y2="{y}"/><line x1="{a}" y1="{}" x2="{a}" y2="{}"/><line x1="{b}" y1="{}" x2="{b}" y2="{}"/></g>"##, f1(dy - 6.0), f1(dy + 6.0), f1(dy - 6.0), f1(dy + 6.0), a = f1(sx(xmin)), b = f1(sx(xmax)), y = f1(dy)));
    out.push_str(&format!(r##"<text x="{}" y="{}" text-anchor="middle">{:.0} mm</text>"##, f1(sx((xmin + xmax) / 2.0)), f1(dy - 5.0), len * 1000.0));
    let dx = ml - 30.0;
    out.push_str(&format!(r##"<g stroke="#111"><line x1="{x}" y1="{}" x2="{x}" y2="{}"/></g><text x="{}" y="{}" text-anchor="middle" transform="rotate(-90 {} {})">{:.0} mm</text>"##, f1(sy(0.0)), f1(sy(h)), f1(dx - 6.0), f1(sy(h / 2.0)), f1(dx - 6.0), f1(sy(h / 2.0)), h * 1000.0, x = f1(dx)));
    // title block
    let (bx, by) = (width - 360.0, height - 92.0);
    let studs = rects.iter().filter(|r| r.1 == "studs").count();
    out.push_str(&format!(r##"<rect x="{}" y="{}" width="340" height="80" fill="none" stroke="#111"/>"##, f1(bx), f1(by)));
    let lines = [
        format!("Framing wall {}", &wall_id[..wall_id.len().min(8)]),
        format!("Length {:.2} m · height {:.2} m · storey {}", len, h, w.level),
        format!("{} studs @ {:.0} mm · {} studs · {} openings", w.stud_size, w.stud_spacing * 1000.0, studs, w.openings.len()),
        "Elevation seen from the exterior · dimensions in mm".to_string(),
    ];
    for (i, l) in lines.iter().enumerate() {
        out.push_str(&format!(r##"<text x="{}" y="{}" font-size="{}" fill="#111">{}</text>"##, f1(bx + 10.0), f1(by + 20.0 + i as f64 * 17.0), if i == 0 { 13 } else { 11 }, esc(l)));
    }
    out.push_str("</svg>");
    Ok(out)
}

/// Plan footprint of a member: its box's bottom face projected on the plan.
fn footprint(m: &Member) -> Vec<[f64; 2]> {
    let [[x0, x1], [y0, _], [z0, z1]] = m.r;
    [(x0, z0), (x1, z0), (x1, z1), (x0, z1)].iter().map(|&(x, z)| { let p = world(m, x, y0, z); [p[0], p[2]] }).collect()
}
/// Centre line of a member along its length.
fn centre_line(m: &Member) -> ([f64; 2], [f64; 2]) {
    let lens = [m.r[0][1] - m.r[0][0], m.r[1][1] - m.r[1][0], m.r[2][1] - m.r[2][0]];
    let along = (0..3).min_by(|a, b| (lens[*a] - m.length).abs().partial_cmp(&(lens[*b] - m.length).abs()).unwrap()).unwrap_or(0);
    let mid = |k: usize| (m.r[k][0] + m.r[k][1]) / 2.0;
    let mut a = [mid(0), mid(1), mid(2)]; let mut b = a;
    a[along] = m.r[along][0]; b[along] = m.r[along][1];
    let (p, q) = (world(m, a[0], a[1], a[2]), world(m, b[0], b[1], b[2]));
    ([p[0], p[2]], [q[0], q[2]])
}

/// Framing plan of a storey.
pub fn plan_svg(doc: &Document, level: usize) -> String {
    let house = House::read(doc);
    // (element type, members, extra info)
    let mut groups: Vec<(String, Vec<Member>, serde_json::Value)> = vec![];
    for e in &doc.entities {
        let Entity::Custom(c) = e else { continue };
        if !TYPES.contains(&c.type_.as_str()) || c.type_ == T_FOUNDATION { continue; }
        let lvl = c.data.get("level").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        if lvl != level { continue; }
        groups.push((c.type_.clone(), expand_element(c, &house, "").members, c.data.clone()));
    }
    let pts: Vec<[f64; 2]> = groups.iter().flat_map(|g| g.1.iter().flat_map(footprint)).collect();
    let (minx, maxx) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
    let (minz, maxz) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
    let (minx, maxx, minz, maxz) = if pts.is_empty() { (0.0, 5.0, 0.0, 5.0) } else { (minx, maxx, minz, maxz) };
    let s = (1100.0 / (maxx - minx).max(maxz - minz).max(1.0)).min(80.0);
    let (ml, mt) = (50.0, 60.0);
    let (width, height) = (ml * 2.0 + (maxx - minx) * s, mt + (maxz - minz) * s + 110.0);
    let sx = |x: f64| ml + (x - minx) * s;
    let sz = |z: f64| mt + (z - minz) * s;
    let pt = |p: [f64; 2]| format!("{},{}", f1(sx(p[0])), f1(sz(p[1])));
    let mut out = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}" font-family="Helvetica, Arial, sans-serif" font-size="11"><rect x="0" y="0" width="{}" height="{}" fill="#ffffff"/>"##, f1(width), f1(height), f1(width), f1(height), f1(width), f1(height));
    out.push_str(r##"<defs><marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" fill="#1a73e8"/></marker></defs>"##);
    let mut labels = String::new();
    for (type_, members, data) in &groups {
        let roof = type_ == T_ROOF;
        for m in members {
            let line_like = roof || matches!(m.category, "joists" | "rim" | "blocking");
            if line_like {
                let (a, b) = centre_line(m);
                let (stroke, dash, wdt) = if roof { ("#9a7d4f", r##" stroke-dasharray="5 4""##, 1.0) } else { ("#8c6d3f", "", 1.2) };
                out.push_str(&format!(r##"<line class="member {}" x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}"{}/>"##, m.category, f1(sx(a[0])), f1(sz(a[1])), f1(sx(b[0])), f1(sz(b[1])), stroke, wdt, dash));
            } else {
                let poly: Vec<String> = footprint(m).into_iter().map(pt).collect();
                let (fillc, stroke, extra) = match m.category {
                    "plates" => ("none", "#5b4630", ""),
                    "beams" => ("none", "#7a4f1d", r##" stroke-dasharray="8 3""##),
                    _ => (fill(m.category), "#5b4630", ""),
                };
                out.push_str(&format!(r##"<polygon class="member {}" points="{}" fill="{}" stroke="{}" stroke-width="0.8"{}/>"##, m.category, poly.join(" "), fillc, stroke, extra));
            }
        }
        // labels: joist size and direction, beam size, stair arrow
        if type_ == T_FLOOR {
            if let Ok(fl) = serde_json::from_value::<Floor>(data.clone()) {
                if let Some(j) = members.iter().find(|m| m.key.starts_with("joist.")) {
                    let (a, b) = centre_line(j);
                    let n = fl.points.len().max(1) as f64;
                    let c = [fl.points.iter().map(|p| p[0]).sum::<f64>() / n, fl.points.iter().map(|p| p[1]).sum::<f64>() / n];
                    let d = [b[0] - a[0], b[1] - a[1]]; let l = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-9);
                    let h = (l / 3.0).min(1.2);
                    let (p, q) = ([c[0] - d[0] / l * h, c[1] - d[1] / l * h], [c[0] + d[0] / l * h, c[1] + d[1] / l * h]);
                    labels.push_str(&format!(r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#1a73e8" stroke-width="1.5" marker-start="url(#arrow)" marker-end="url(#arrow)"/>"##, f1(sx(p[0])), f1(sz(p[1])), f1(sx(q[0])), f1(sz(q[1]))));
                    labels.push_str(&format!(r##"<text x="{}" y="{}" text-anchor="middle" fill="#1a73e8" font-weight="bold">{} @ {:.0}</text>"##, f1(sx(c[0])), f1(sz(c[1]) - 8.0), esc(&fl.joist_size), fl.joist_spacing * 1000.0));
                }
            }
        }
        if type_ == T_BEAM {
            if let Ok(bm) = serde_json::from_value::<Beam>(data.clone()) {
                let c = [(bm.a[0] + bm.b[0]) / 2.0, (bm.a[1] + bm.b[1]) / 2.0];
                let [bw, bh] = bm.section();
                labels.push_str(&format!(r##"<text x="{}" y="{}" text-anchor="middle" fill="#7a4f1d">{}</text>"##, f1(sx(c[0])), f1(sz(c[1]) - 6.0), esc(&format!("beam {} {:.0}×{:.0}", bm.material, bw * 1000.0, bh * 1000.0))));
            }
        }
        if type_ == T_STAIR {
            if let Ok(st) = serde_json::from_value::<Stair>(data.clone()) {
                let (n, r) = st.risers_of(&house);
                let d = st.dir();
                let run = (n.max(1) - 1) as f64 * st.tread_depth;
                let (p, q) = (st.a, [st.a[0] + d[0] * run, st.a[1] + d[1] * run]);
                labels.push_str(&format!(r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#1a73e8" stroke-width="1.5" marker-end="url(#arrow)"/>"##, f1(sx(p[0])), f1(sz(p[1])), f1(sx(q[0])), f1(sz(q[1]))));
                labels.push_str(&format!(r##"<text x="{}" y="{}" fill="#1a73e8">UP {} × {:.0}</text>"##, f1(sx(p[0]) + 4.0), f1(sz(p[1]) - 4.0), n, r * 1000.0));
            }
        }
    }
    out.push_str(&labels);
    // scale bar and title
    let by = height - 40.0;
    out.push_str(&format!(r##"<g stroke="#111"><line x1="{}" y1="{y}" x2="{}" y2="{y}"/><line x1="{a}" y1="{}" x2="{a}" y2="{}"/><line x1="{b}" y1="{}" x2="{b}" y2="{}"/></g><text x="{}" y="{}" text-anchor="middle">1 m</text>"##,
        f1(ml), f1(ml + s), f1(by - 5.0), f1(by + 5.0), f1(by - 5.0), f1(by + 5.0), f1(ml + s / 2.0), f1(by - 8.0), y = f1(by), a = f1(ml), b = f1(ml + s)));
    let name = house.storeys.get(level).map(|s| s.0.clone()).unwrap_or_else(|| format!("storey {level}"));
    out.push_str(&format!(r##"<text x="{}" y="30" font-size="15" font-weight="bold" fill="#111">{}</text>"##, f1(ml), esc(&format!("Framing plan — {name}"))));
    out.push_str(&format!(r##"<text x="{}" y="{}" fill="#555">walls (plates and studs) · joists with span arrows · beams dashed · roof members dashed · stairs with UP arrow</text>"##, f1(ml + s + 40.0), f1(by + 4.0)));
    out.push_str("</svg>");
    out
}
