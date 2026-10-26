//! Masonry floors (beam-and-block "potten en balken", hollow-core planks, in-situ slabs) with ring
//! beams and stair-well trimming, and strip foundations. See docs/MASONRY.md.
//!
//! Plan conventions: points are `[x, z]`; a floor's **span axis** is the axis its joists / planks run
//! along (`s`), the **repeat axis** the one they are laid side by side along (`r`). Every member is an
//! axis-aligned box in world coordinates.

use crate::masonry::data::*;
use crate::masonry::expand::*;

// ---- plan helpers ------------------------------------------------------------------------------------

fn bbox(p: &[P2]) -> (P2, P2) {
    let mut lo = [f64::MAX, f64::MAX]; let mut hi = [f64::MIN, f64::MIN];
    for q in p { for k in 0..2 { lo[k] = lo[k].min(q[k]); hi[k] = hi[k].max(q[k]); } }
    (lo, hi)
}

/// Intervals (along the other axis) where the line `axis = at` lies inside the polygon.
fn scan(points: &[P2], axis: usize, at: f64) -> Vec<[f64; 2]> {
    let other = 1 - axis; let n = points.len(); let mut xs = vec![];
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let (ya, yb) = (a[axis], b[axis]);
        if (ya <= at && yb > at) || (yb <= at && ya > at) { let t = (at - ya) / (yb - ya); xs.push(a[other] + t * (b[other] - a[other])); }
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs.chunks(2).filter(|c| c.len() == 2 && c[1] - c[0] > 1e-6).map(|c| [c[0], c[1]]).collect()
}

/// `a` minus every interval of `cut`.
fn subtract(a: &[[f64; 2]], cut: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = a.to_vec();
    for c in cut {
        out = out.into_iter().flat_map(|[x0, x1]| {
            if c[1] <= x0 || c[0] >= x1 { vec![[x0, x1]] } else { vec![[x0, c[0]], [c[1], x1]] }
        }).filter(|[x0, x1]| x1 - x0 > 1e-4).collect();
    }
    out
}

/// The polygon offset inward by `d` (negative = outward), with mitred corners.
fn offset(poly: &[P2], d: f64) -> Vec<P2> {
    let n = poly.len();
    if n < 3 || d.abs() < 1e-12 { return poly.to_vec(); }
    let sign = if plan_area(poly) >= 0.0 { 1.0 } else { -1.0 };
    // per edge: a point on the offset line and its direction
    let lines: Vec<(P2, P2)> = (0..n).map(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let l = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt().max(1e-12);
        let u = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
        let inward = [-u[1] * sign, u[0] * sign];
        ([a[0] + inward[0] * d, a[1] + inward[1] * d], u)
    }).collect();
    (0..n).map(|i| {
        let (p0, u0) = lines[(i + n - 1) % n]; let (p1, u1) = lines[i];
        let den = u0[0] * u1[1] - u0[1] * u1[0];
        if den.abs() < 1e-9 { return p1; }
        let t = ((p1[0] - p0[0]) * u1[1] - (p1[1] - p0[1]) * u1[0]) / den;
        [p0[0] + u0[0] * t, p0[1] + u0[1] * t]
    }).collect()
}

fn len2(a: P2, b: P2) -> f64 { ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt() }
fn perimeter(p: &[P2]) -> f64 { (0..p.len()).map(|i| len2(p[i], p[(i + 1) % p.len()])).sum() }
fn mm(v: f64) -> i64 { (v * 1000.0).round() as i64 }

/// An axis-aligned box from span / repeat ranges (`span` = 0 → s along x).
#[allow(clippy::too_many_arguments)]
fn sbox(f: &mut Faces, id: &str, key: &str, span: usize, s: [f64; 2], r: [f64; 2], y: [f64; 2], mat: &str) {
    let (x, z) = if span == 0 { (s, r) } else { (r, s) };
    f.boxf(&format!("{id}#{key}"), [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], UP, [0.0, 0.0, 1.0], [x, y, z], mat);
}

/// One mitred quad per polygon edge between the offsets `d_out` and `d_in` (inward distances).
fn band(poly: &[P2], d_out: f64, d_in: f64) -> Vec<Vec<P2>> {
    let (o, i) = (offset(poly, d_out), offset(poly, d_in));
    let n = poly.len();
    (0..n).map(|e| vec![o[e], o[(e + 1) % n], i[(e + 1) % n], i[e]]).collect()
}

fn block_name(kind: &str) -> &'static str { match kind { "clay" => "klei", "eps" => "EPS", _ => "beton" } }

// ---- floors -------------------------------------------------------------------------------------------

pub fn expand_floor(id: &str, fl: &Floor, house: &House, layer: &str) -> Expansion {
    let mut ex = expand_structure(id, fl, house, layer);
    if fl.points.len() >= 3 && fl.ceiling_plaster.unwrap_or(house.defaults.floor.ceiling_plaster) {
        // 15 mm ceiling plaster straight under the structure (plafondpleister)
        let under = house.base(fl.level) - fl.spec(&house.defaults.floor).depth();
        let holes: Vec<Vec<P2>> = fl.openings.iter().filter(|h| h.len() >= 3).cloned().collect();
        let mut f = Faces::new(layer);
        f.prism_plan(&format!("{id}#ceiling"), &fl.points, &holes, under - 0.015, under, &material_id("plaster"));
        ex.faces.extend(f.out);
        let area = plan_area(&fl.points).abs() - holes.iter().map(|h| plan_area(h).abs()).sum::<f64>();
        ex.items.push(Item::new("ceiling", "plaster", "Plafondpleister 15 mm").area(area));
    }
    ex
}

fn expand_structure(id: &str, fl: &Floor, house: &House, layer: &str) -> Expansion {
    let mut ex = Expansion::default();
    if fl.points.len() < 3 { return ex; }
    let sp = fl.spec(&house.defaults.floor);
    let top = house.base(fl.level);
    let depth = sp.depth();
    let under = top - depth;
    let mut f = Faces::new(layer);
    let conc = material_id("concrete");
    let holes: Vec<Vec<P2>> = fl.openings.iter().filter(|h| h.len() >= 3).cloned().collect();
    let net_area = plan_area(&fl.points).abs() - holes.iter().map(|h| plan_area(h).abs()).sum::<f64>();
    let (lo, hi) = bbox(&fl.points);
    let span = match fl.direction.as_str() { "x" => 0, "z" => 1, _ => if hi[0] - lo[0] <= hi[1] - lo[1] { 0 } else { 1 } };
    let rep = 1 - span;

    if sp.system == "insitu" {
        f.prism_plan(&format!("{id}#slab"), &fl.points, &holes, top - sp.slab_thickness, top, &conc);
        ex.items.push(Item::new("slab", "slab", format!("Gestorte vloerplaat {} mm", mm(sp.slab_thickness))).volume(net_area * sp.slab_thickness).area(net_area));
        ex.faces = f.out;
        return ex;
    }

    // ring beam: a band inside every edge, from the underside to the topping
    let ring_w = if sp.ring_beam { sp.ring_beam_width.max(0.0) } else { 0.0 };
    let inner = if ring_w > 1e-6 { offset(&fl.points, ring_w) } else { fl.points.clone() };
    let below_topping = top - sp.topping.max(0.0);
    if ring_w > 1e-6 {
        for (e, q) in band(&fl.points, 0.0, ring_w).into_iter().enumerate() {
            f.prism_plan(&format!("{id}#ring.{e}"), &q, &[], under, below_topping, &conc);
        }
        let area = plan_area(&fl.points).abs() - plan_area(&inner).abs();
        ex.items.push(Item::new("ringBeam", "ringBeam", format!("Ringbalk beton {}×{}", mm(ring_w), mm(below_topping - under))).length(perimeter(&fl.points)).volume(area * (below_topping - under)));
    }
    // opening bounding boxes in (s, r)
    let hole_sr: Vec<([f64; 2], [f64; 2])> = holes.iter().map(|h| { let (a, b) = bbox(h); ([a[span], b[span]], [a[rep], b[rep]]) }).collect();

    if sp.system == "hollowCore" {
        let (ilo, ihi) = bbox(&inner);
        let (r0, r1) = (ilo[rep], ihi[rep]);
        let w = sp.plank_width.max(0.2);
        let y = [under, under + sp.plank_thickness];
        let mut r = r0; let mut planks = 0usize; let mut pieces = 0usize; let mut area = 0.0;
        while r < r1 - 1e-4 {
            let rb = (r + w).min(r1);
            planks += 1;
            // split the strip at the openings' repeat edges so each sub-strip is either clear of a hole or across it
            let mut cuts = vec![r, rb];
            for (_, hr) in &hole_sr { for v in [hr[0], hr[1]] { if v > r + 1e-4 && v < rb - 1e-4 { cuts.push(v); } } }
            cuts.sort_by(|a, b| a.partial_cmp(b).unwrap()); cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
            for c in cuts.windows(2) {
                let (ra, rbb) = (c[0], c[1]);
                let mid = (ra + rbb) / 2.0;
                let blocked: Vec<[f64; 2]> = hole_sr.iter().filter(|(_, hr)| mid > hr[0] && mid < hr[1]).map(|(hs, _)| *hs).collect();
                for iv in subtract(&scan(&inner, rep, mid), &blocked) {
                    sbox(&mut f, id, &format!("plank.{pieces}"), span, iv, [ra, rbb], y, &conc);
                    area += (iv[1] - iv[0]) * (rbb - ra); pieces += 1;
                }
            }
            r = rb;
        }
        ex.items.push(Item::new("planks", "planks", format!("Welfsel {} mm", mm(sp.plank_thickness))).count(planks as f64).area(area));
    } else {
        // ---- beam-and-block ----
        let (jw, jh) = (sp.joist_width, sp.joist_height);
        let (ilo, ihi) = bbox(&inner);
        let (r0, r1) = (ilo[rep], ihi[rep]);
        let spacing = sp.spacing.max(0.2);
        let width = r1 - r0;
        // joist lines spaced exactly `spacing`, centred, so the end bays are between half and one bay
        let n = if width > spacing { ((width - spacing) / spacing + 1e-9).floor() as usize + 1 } else { 1 };
        let first = r0 + (width - (n as f64 - 1.0) * spacing) / 2.0;
        let lines: Vec<f64> = (0..n).map(|k| first + k as f64 * spacing).collect();
        // members along the repeat axis: (r range, s intervals) for blocks to fit between
        let mut members: Vec<[f64; 2]> = vec![];
        let jy = [under, under + jh];
        let mut joist_len = 0.0; let mut joist_count = 0usize;
        let mut trim_len = 0.0; let mut trim_count = 0usize;
        // trimmers: two joists side by side along both sides of each opening, over the full run
        let mut trimmer_r: Vec<[f64; 2]> = vec![];
        for (o, (hs, hr)) in hole_sr.iter().enumerate() {
            let mut k = 0;
            for rr in [[hr[0] - 2.0 * jw, hr[0] - jw], [hr[0] - jw, hr[0]], [hr[1], hr[1] + jw], [hr[1] + jw, hr[1] + 2.0 * jw]] {
                for iv in scan(&inner, rep, (rr[0] + rr[1]) / 2.0) {
                    sbox(&mut f, id, &format!("trimmer.{o}.{k}"), span, iv, rr, jy, &material_id("joist"));
                    trim_len += iv[1] - iv[0]; trim_count += 1; k += 1;
                }
                trimmer_r.push(rr); members.push(rr);
            }
            // headers across both ends of the hole, between the trimmers
            for (h, ss) in [[hs[0] - jw, hs[0]], [hs[1], hs[1] + jw]].into_iter().enumerate() {
                sbox(&mut f, id, &format!("header.{o}.{h}"), span, ss, [hr[0], hr[1]], jy, &material_id("joist"));
                trim_len += hr[1] - hr[0]; trim_count += 1;
            }
        }
        let mut ji = 0;
        for &p in &lines {
            let rr = [p - jw / 2.0, p + jw / 2.0];
            // a joist that would run into a trimmer pair is replaced by it
            if trimmer_r.iter().any(|t| rr[1] > t[0] - 1e-6 && rr[0] < t[1] + 1e-6) { continue; }
            let blocked: Vec<[f64; 2]> = hole_sr.iter().filter(|(_, hr)| p > hr[0] && p < hr[1]).map(|(hs, _)| [hs[0] - jw, hs[1] + jw]).collect();
            let ivs = subtract(&scan(&inner, rep, p), &blocked);
            if ivs.is_empty() { continue; }
            for iv in ivs {
                sbox(&mut f, id, &format!("joist.{ji}"), span, iv, rr, jy, &material_id("joist"));
                joist_len += iv[1] - iv[0]; joist_count += 1; ji += 1;
            }
            members.push(rr);
        }
        // blocks fill every bay between members (and between the ring beam and the outer members)
        members.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
        let mut bays: Vec<[f64; 2]> = vec![];
        let mut cursor = r0;
        for m in &members { if m[0] - cursor > 0.02 { bays.push([cursor, m[0]]); } cursor = cursor.max(m[1]); }
        if r1 - cursor > 0.02 { bays.push([cursor, r1]); }
        let bl = sp.block_length.max(0.05);
        let by = [under, under + sp.block_height];
        let bmat = if sp.block_kind == "eps" { material_id("insulation") } else { material_id("floorBlock") };
        let mut blocks = 0usize;
        for (bi, bay) in bays.iter().enumerate() {
            let mid = (bay[0] + bay[1]) / 2.0;
            let blocked: Vec<[f64; 2]> = hole_sr.iter().filter(|(_, hr)| mid > hr[0] && mid < hr[1]).map(|(hs, _)| [hs[0] - jw, hs[1] + jw]).collect();
            let mut k = 0;
            for iv in subtract(&scan(&inner, rep, mid), &blocked) {
                let mut s = iv[0];
                while s < iv[1] - 1e-4 {
                    let e = (s + bl).min(iv[1]);
                    sbox(&mut f, id, &format!("block.{bi}.{k}"), span, [s, e], *bay, by, &bmat);
                    k += 1; blocks += 1; s = e;
                }
            }
        }
        ex.items.push(Item::new("joists", "joists", format!("Voorgespannen balkje {}×{}", mm(jw), mm(jh))).count(joist_count as f64).length(joist_len));
        if trim_count > 0 { ex.items.push(Item::new("trimmers", "trimmers", format!("Raveelbalk {}×{}", mm(jw), mm(jh))).count(trim_count as f64).length(trim_len)); }
        ex.items.push(Item::new("floorBlocks", "floorBlocks", format!("Vulblok {} {} mm", block_name(&sp.block_kind), mm(sp.block_height))).count(blocks as f64));
    }
    if sp.topping > 1e-6 {
        f.prism_plan(&format!("{id}#topping"), &fl.points, &holes, top - sp.topping, top, &conc);
        ex.items.push(Item::new("topping", "topping", format!("Druklaag {} mm", mm(sp.topping))).area(net_area).volume(net_area * sp.topping));
        if sp.mesh { ex.items.push(Item::new("mesh", "mesh", "Wapeningsnet druklaag").area(net_area)); }
    }
    ex.faces = f.out;
    ex
}

// ---- foundations ------------------------------------------------------------------------------------------

pub fn expand_foundation(id: &str, fd: &Foundation, house: &House, layer: &str) -> Expansion {
    let mut ex = Expansion::default();
    if fd.points.len() < 3 { return ex; }
    let d = &house.defaults.foundation;
    let (fw, fdp, bottom, bw) = (fd.footing_width.unwrap_or(d.footing_width), fd.footing_depth.unwrap_or(d.footing_depth), fd.bottom.unwrap_or(d.bottom), fd.block_width.unwrap_or(d.block_width));
    let base = house.base(0);
    let y0 = base + bottom; let y1 = y0 + fdp;
    let top = fd.top.unwrap_or(base);
    let mut f = Faces::new(layer);
    let per = perimeter(&fd.points);
    for (e, q) in band(&fd.points, -fw / 2.0, fw / 2.0).into_iter().enumerate() {
        f.prism_plan(&format!("{id}#footing.{e}"), &q, &[], y0, y1, &material_id("concrete"));
    }
    ex.items.push(Item::new("footing", "footing", format!("Funderingszool beton {}×{}", mm(fw), mm(fdp))).length(per).volume(per * fw * fdp));
    if top - y1 > 1e-3 {
        for (e, q) in band(&fd.points, -bw / 2.0, bw / 2.0).into_iter().enumerate() {
            f.prism_plan(&format!("{id}#block.{e}"), &q, &[], y1, top, &material_id("concreteBlock"));
        }
        let h = top - y1;
        let face = per * h;
        let blocks = (face / (0.40 * 0.20)).ceil();
        let unit = 0.39 * 0.19 * bw;
        ex.items.push(Item::new("foundationBlocks", "foundationBlocks", format!("Betonblok 390×190×{}", mm(bw))).count(blocks).area(face));
        ex.items.push(Item::new("mortar", "mortar", "Metselmortel fundering").volume((face * bw - blocks * unit).max(0.0)));
    }
    ex.faces = f.out;
    ex
}
