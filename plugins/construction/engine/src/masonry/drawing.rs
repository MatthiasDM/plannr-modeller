//! Storey plan of the masonry: every wall cut at 1 m with its layers (outer leaf, cavity or
//! insulation, inner leaf, plaster) and the structural floor of the storey above (outline, joists
//! with the span direction, stair wells), a legend, title and scale bar.

use plannr_core::model::Document;

use crate::masonry::data::*;

fn esc(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;") }

/// The layer bands of a wall across its line: (offset from, offset to, fill, label) along the exterior normal.
pub fn bands(w: &Wall, d: &Defaults) -> Vec<(f64, f64, &'static str, &'static str)> {
    let t = w.thickness(d);
    let (lo, hi) = match w.reference.as_str() { "exterior" => (-t, 0.0), "interior" => (0.0, t), _ => (-t / 2.0, t / 2.0) };
    let mut v = vec![];
    let fill_of = |kind: &str| -> &'static str { match kind { "facingBrick" => "#9C4A32", "brick" => "#B8644A", "clayBlock" => "#C9785A", "concreteBlock" => "#A7A6A1", "aeratedBlock" => "#E4E2DC", "limeSandBlock" => "#E8E4D6", _ => "#B8644A" } };
    let inner = w.inner_unit(d);
    if w.plaster.interior { v.push((lo - w.plaster.thickness, lo, "#F4F2EE", "plaster")); }
    v.push((lo, lo + inner.width, fill_of(&inner.kind), "inner"));
    if w.is_cavity() {
        let outer = w.outer_unit(d); let cav = w.cavity_of(d);
        let c0 = lo + inner.width; let c1 = hi - outer.width;
        match cav.insulation.kind.as_str() {
            "fill" => v.push((c0, c1, "#F2D16B", "insulation")),
            "board" => v.push((c0, (c0 + cav.insulation.thickness).min(c1), "#F2D16B", "insulation")),
            _ => {}
        }
        v.push((c1, hi, fill_of(&outer.kind), "outer"));
    }
    if w.plaster.exterior { v.push((hi, hi + w.plaster.thickness, "#F4F2EE", "plaster")); }
    v
}

pub fn plan_svg(doc: &Document, level: usize) -> String {
    let h = House::read(doc);
    let d = &h.defaults;
    let walls: Vec<&(String, Wall)> = h.walls.iter().filter(|(_, w)| w.level == level).collect();
    let floors: Vec<&(String, Floor)> = h.floors.iter().filter(|(_, f)| f.level == level + 1).collect();
    // bounds
    let mut pts: Vec<P2> = walls.iter().flat_map(|(_, w)| [w.a, w.b]).collect();
    pts.extend(floors.iter().flat_map(|(_, f)| f.points.clone()));
    if pts.is_empty() { pts = vec![[0.0, 0.0], [5.0, 5.0]]; }
    let (minx, maxx) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
    let (minz, maxz) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
    let (minx, minz, maxx, maxz) = (minx - 0.6, minz - 0.6, maxx + 0.6, maxz + 0.6);
    let scale = (900.0 / (maxx - minx).max(1.0)).min(700.0 / (maxz - minz).max(1.0)).min(120.0);
    let (w_px, h_px) = ((maxx - minx) * scale + 80.0, (maxz - minz) * scale + 170.0);
    let x = |v: f64| 40.0 + (v - minx) * scale; let y = |v: f64| 60.0 + (v - minz) * scale;
    let poly = |p: &[P2]| p.iter().map(|q| format!("{:.1},{:.1}", x(q[0]), y(q[1]))).collect::<Vec<_>>().join(" ");
    let mut s = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w_px:.0}\" height=\"{h_px:.0}\" viewBox=\"0 0 {w_px:.0} {h_px:.0}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"11\">\n<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>\n");
    let name = h.storeys.get(level).map(|s| s.0.clone()).unwrap_or_else(|| format!("Storey {}", level + 1));
    s.push_str(&format!("<text x=\"40\" y=\"28\" font-size=\"16\" font-weight=\"bold\">Masonry plan — {}</text>\n", esc(&name)));
    // floor above: outline, joists, wells
    for (_, f) in &floors {
        let sp = f.spec(&d.floor);
        s.push_str(&format!("<polygon class=\"floor\" points=\"{}\" fill=\"none\" stroke=\"#5a6270\" stroke-width=\"1\" stroke-dasharray=\"6 4\"/>\n", poly(&f.points)));
        for o in &f.openings { s.push_str(&format!("<polygon class=\"well\" points=\"{}\" fill=\"none\" stroke=\"#5a6270\" stroke-width=\"1\"/>\n<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#5a6270\"/>\n", poly(o), x(o[0][0]), y(o[0][1]), x(o[2 % o.len()][0]), y(o[2 % o.len()][1]))); }
        let (fx0, fx1) = f.points.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
        let (fz0, fz1) = f.points.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
        let span_x = match f.direction.as_str() { "x" => true, "z" => false, _ => fx1 - fx0 <= fz1 - fz0 };
        if sp.system == "beamBlock" {
            // joist centre lines across the span, every `spacing`
            let (r0, r1) = if span_x { (fz0, fz1) } else { (fx0, fx1) };
            let mut r = r0 + sp.ring_beam_width + sp.spacing / 2.0;
            while r < r1 - sp.ring_beam_width - 0.05 {
                let (a, b) = if span_x { ([fx0 + sp.bearing, r], [fx1 - sp.bearing, r]) } else { ([r, fz0 + sp.bearing], [r, fz1 - sp.bearing]) };
                s.push_str(&format!("<line class=\"joist\" x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#8a8f98\" stroke-width=\"0.8\"/>\n", x(a[0]), y(a[1]), x(b[0]), y(b[1])));
                r += sp.spacing;
            }
        }
        let c = [(fx0 + fx1) / 2.0, (fz0 + fz1) / 2.0];
        let (a, b) = if span_x { ([c[0] - 0.8, c[1]], [c[0] + 0.8, c[1]]) } else { ([c[0], c[1] - 0.8], [c[0], c[1] + 0.8]) };
        s.push_str(&format!("<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#1a73e8\" stroke-width=\"1.6\" marker-start=\"url(#arr)\" marker-end=\"url(#arr)\"/>\n", x(a[0]), y(a[1]), x(b[0]), y(b[1])));
        let label = match sp.system.as_str() { "hollowCore" => format!("welfsels {:.0} mm", sp.plank_thickness * 1000.0), "insitu" => format!("in-situ {:.0} mm", sp.slab_thickness * 1000.0), _ => format!("potten en balken {:.0}+{:.0} · {:.0} mm h.o.h.", sp.block_height * 100.0, sp.topping * 100.0, sp.spacing * 1000.0) };
        s.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" fill=\"#1a73e8\" text-anchor=\"middle\">{}</text>\n", x(c[0]), y(c[1]) - 8.0, esc(&label)));
    }
    s.push_str("<defs><marker id=\"arr\" viewBox=\"0 0 10 10\" refX=\"5\" refY=\"5\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\"><path d=\"M0,0 L10,5 L0,10 z\" fill=\"#1a73e8\"/></marker></defs>\n");
    // walls in section
    for (id, w) in &walls {
        let l = w.length(); if l < 1e-6 { continue; }
        let u = [(w.b[0] - w.a[0]) / l, (w.b[1] - w.a[1]) / l];
        let n0 = [u[1], -u[0]]; let n = if w.flip { [-n0[0], -n0[1]] } else { n0 };
        let at = |s_: f64, o: f64| [w.a[0] + u[0] * s_ + n[0] * o, w.a[1] + u[1] * s_ + n[1] * o];
        let mut cuts: Vec<(f64, f64)> = w.openings.iter().map(|o| (o.offset, o.offset + o.width)).collect();
        cuts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut runs = vec![]; let mut cur = 0.0;
        for (a, b) in &cuts { if *a > cur { runs.push((cur, *a)); } cur = cur.max(*b); }
        if cur < l { runs.push((cur, l)); }
        for (o0, o1, fill, class) in bands(w, d) {
            for (s0, s1) in &runs {
                let p = [at(*s0, o0), at(*s1, o0), at(*s1, o1), at(*s0, o1)];
                s.push_str(&format!("<polygon class=\"{class}\" data-wall=\"{}\" points=\"{}\" fill=\"{fill}\" stroke=\"#222\" stroke-width=\"0.5\"/>\n", esc(id), poly(&p)));
            }
        }
        // openings: a thin line across
        for (a, b) in &cuts {
            let t = w.thickness(d); let (lo, hi) = match w.reference.as_str() { "exterior" => (-t, 0.0), "interior" => (0.0, t), _ => (-t / 2.0, t / 2.0) };
            let mid = (lo + hi) / 2.0;
            let (p, q) = (at(*a, mid), at(*b, mid));
            s.push_str(&format!("<line class=\"opening\" x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#1e8e3e\" stroke-width=\"1.2\"/>\n", x(p[0]), y(p[1]), x(q[0]), y(q[1])));
        }
    }
    // legend + scale bar
    let ly = h_px - 70.0;
    let legend = [("#9C4A32", "facing brick"), ("#C9785A", "clay block"), ("#A7A6A1", "concrete block"), ("#F2D16B", "insulation"), ("#F4F2EE", "plaster")];
    for (k, (c, t)) in legend.iter().enumerate() {
        let lx = 40.0 + k as f64 * 120.0;
        s.push_str(&format!("<rect x=\"{lx:.0}\" y=\"{ly:.0}\" width=\"14\" height=\"10\" fill=\"{c}\" stroke=\"#222\" stroke-width=\"0.5\"/><text x=\"{:.0}\" y=\"{:.0}\">{}</text>\n", lx + 20.0, ly + 9.0, t));
    }
    let sb = scale; let sx = 40.0; let sy = h_px - 30.0;
    s.push_str(&format!("<line x1=\"{sx}\" y1=\"{sy}\" x2=\"{:.1}\" y2=\"{sy}\" stroke=\"#222\" stroke-width=\"2\"/><text x=\"{:.1}\" y=\"{:.1}\">1 m</text>\n", sx + sb, sx + sb + 6.0, sy + 4.0));
    s.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"#555\">{} walls · {} floor(s) above · plan cut at 1 m</text>\n", w_px - 40.0, sy + 4.0, walls.len(), floors.len()));
    s.push_str("</svg>\n");
    s
}
