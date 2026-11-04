//! Pipes and brackets as core faces (fittings live in `fittings.rs`).

use plannr_core::geom::{add, cross, normalize, scale, sub, dot, P3};
use plannr_core::model::{Custom, Document, Entity};

use crate::catalog::*;
use crate::data::*;
use crate::fittings;
use crate::mesh::{bend_path, sides_for, Out};

/// The path a bent pipe follows (arcs at the corners) and the radius used at each corner.
pub fn pipe_path(p: &Pipe) -> (Vec<P3>, Vec<f64>) {
    if p.is_bent() { bend_path(&p.points, p.bend_r()) } else { (p.points.clone(), vec![]) }
}

pub fn expand_pipe(c: &Custom, doc: &Document) -> Vec<Entity> {
    let Ok(p) = serde_json::from_value::<Pipe>(c.data.clone()) else { return vec![] };
    if p.points.len() < 2 { return vec![]; }
    let h = House::read(doc); let y0 = h.base(p.level);
    let mut out = Out::new(&c.id, &c.layer);
    let r = p.radius().max(0.003); let sides = sides_for(r);
    let (rel_path, _) = pipe_path(&p);
    let path: Vec<P3> = rel_path.iter().map(|q| add(*q, [0.0, y0, 0.0])).collect();
    out.sweep("pipe", &path, r, sides, &p.look(), true);
    if p.insulation > 0.0 { out.sweep("insulation", &path, r + p.insulation / 1000.0, sides.max(12), &look_id("insulation"), true); }
    if !p.is_bent() {
        // an elbow of the pipe's joint type at every corner
        for (i, ang) in p.corner_angles() {
            let d_in = normalize(sub(p.points[i], p.points[i - 1])); let d_out = normalize(sub(p.points[i + 1], p.points[i]));
            let perp = sub(d_out, scale(d_in, dot(d_out, d_in)));
            let f = Fitting { level: p.level, kind: if (ang - 45.0).abs() < 22.0 { "elbow45".into() } else { "elbow90".into() }, joint: p.joint.clone(), material: p.material.clone(), system: p.system.clone(), dn: p.dn, dn2: None, position: p.points[i], dir: d_in, dir2: Some(perp), angle: Some(ang), thread: None, outlets: None, fixture: None, note: None, paint: p.paint.clone(), extra: Default::default() };
            out.prefix = format!("elbow.{i}.");
            fittings::build(&mut out, &f, y0);
        }
    }
    out.faces
}

pub fn expand_fitting(c: &Custom, doc: &Document) -> Vec<Entity> {
    let Ok(f) = serde_json::from_value::<Fitting>(c.data.clone()) else { return vec![] };
    let h = House::read(doc);
    let mut out = Out::new(&c.id, &c.layer);
    fittings::build(&mut out, &f, h.base(f.level));
    out.faces
}

pub fn expand_bracket(c: &Custom, doc: &Document) -> Vec<Entity> {
    let Ok(b) = serde_json::from_value::<Bracket>(c.data.clone()) else { return vec![] };
    let h = House::read(doc);
    let mut out = Out::new(&c.id, &c.layer);
    let p = add(b.position, [0.0, h.base(b.level), 0.0]); let d = b.dir_n(); let n = b.normal_n(); let w = normalize(cross(d, n));
    let r = b.dn / 2000.0; let galv = look_id("bracket"); let rubber = look_id("rubber");
    let standoff = b.standoff.max(r + 0.004);
    match b.kind.as_str() {
        "clamp" => {
            out.ring("clip", p, d, r + 0.0005, r + 0.0035, 0.02, 14, &rubber);
            out.ring("clip", p, d, r + 0.0035, r + 0.006, 0.022, 14, &galv);
            let top = add(p, scale(n, r + 0.006));
            out.sweep("clip", &[top, add(top, scale(n, 0.012))], 0.007, 6, &galv, true);
            out.cylinder("stem", add(top, scale(n, 0.012)), add(p, scale(n, standoff - 0.004)), 0.004, 8, &galv);
            out.ring("plate", add(p, scale(n, standoff - 0.002)), n, 0.0001, 0.02, 0.004, 14, &galv);
        }
        "double" => {
            let off = scale(w, 2.0 * r + 0.03);
            for c0 in [p, add(p, off)] { out.ring("clip", c0, d, r + 0.0005, r + 0.004, 0.018, 12, &galv); }
            let mid = add(p, scale(w, r + 0.015));
            let bar = add(mid, scale(n, (r + 0.004 + standoff) / 2.0));
            out.block("stem", bar, d, w, n, [0.018, 4.0 * r + 0.04, (standoff - r - 0.004).max(0.004)], &galv);
            out.block("plate", add(mid, scale(n, standoff - 0.0015)), d, w, n, [0.03, 4.0 * r + 0.08, 0.003], &galv);
        }
        _ => {
            out.ring("clip", p, d, r + 0.0005, r + 0.004, 0.018, 12, &galv);
            let gap = standoff - r - 0.004;
            if gap > 0.002 { out.block("stem", add(p, scale(n, r + 0.004 + gap / 2.0)), d, w, n, [0.018, 0.012, gap], &galv); }
            out.block("plate", add(p, scale(n, standoff - 0.0015)), d, w, n, [0.035, 0.035, 0.003], &galv);
        }
    }
    out.faces
}
