//! Fittings: where their ports are and what they look like. Stylised but recognisable bodies:
//! press sleeves with the black indicator ring, clamp couplings with hex nuts, PVC sockets,
//! chrome tapware, brass valves, manifolds with their outlet nipples.

use plannr_core::geom::{add, cross, normalize, scale, sub, P3};

use crate::catalog::*;
use crate::data::*;
use crate::mesh::{bend_path, sides_for, Out};

#[derive(Debug, Clone)]
pub struct Port {
    /// Height above the storey floor, like the fitting's position.
    pub pos: P3,
    /// Outward unit direction (where the connected pipe runs away from the fitting).
    pub dir: P3,
    pub dn: f64,
    pub name: &'static str,
}

/// Socket depth (m) of a joint on a pipe of diameter `d` (m): how far the pipe enters the fitting.
pub fn socket_len(joint: &str, d: f64) -> f64 {
    match joint { "glue" | "ring" => 0.5 * d + 0.025, "weld" => 0.8 * d + 0.01, "solder" => (1.0 * d).max(0.012), "clamp" | "compression" | "thread" => (0.9 * d).max(0.015), _ => (1.2 * d).max(0.018) }
}
/// Half-length of an inline fitting (centre to port) for a pipe diameter `d`.
fn half_len(joint: &str, d: f64) -> f64 { socket_len(joint, d) + 0.25 * d }

struct Look { body_r: f64, mat: String, nut_r: f64, nut_len: f64, ring: bool }
fn look(f: &Fitting) -> Look {
    let d = f.d();
    if f.kind_info().tapware { return Look { body_r: 0.6 * d, mat: look_id("chrome"), nut_r: 0.9 * d, nut_len: 0.5 * d, ring: false }; }
    match f.joint.as_str() {
        "clamp" | "compression" => Look { body_r: 0.68 * d, mat: look_id("brass"), nut_r: 0.95 * d, nut_len: 0.6 * d, ring: false },
        "thread" => Look { body_r: 0.66 * d, mat: look_id("brass"), nut_r: 0.9 * d, nut_len: 0.55 * d, ring: false },
        "solder" => Look { body_r: 0.6 * d, mat: look_id("copper"), nut_r: 0.0, nut_len: 0.0, ring: false },
        "glue" | "ring" => Look { body_r: 0.6 * d, mat: look_id("pvcFitting"), nut_r: 0.0, nut_len: 0.0, ring: f.joint == "ring" },
        "weld" => Look { body_r: 0.58 * d, mat: look_id("pe"), nut_r: 0.0, nut_len: 0.0, ring: false },
        "pushfit" => Look { body_r: 0.8 * d, mat: look_id("press"), nut_r: 0.0, nut_len: 0.0, ring: true },
        "sleeve" => Look { body_r: 0.62 * d, mat: look_id("brass"), nut_r: 0.82 * d, nut_len: 1.1 * d, ring: false },
        _ => Look { body_r: 0.72 * d, mat: look_id("press"), nut_r: 0.0, nut_len: 0.0, ring: true },
    }
}

/// Outgoing direction of an elbow with deflection `angle` from `d` towards `d2`.
fn elbow_out(d: P3, d2: P3, angle: f64) -> P3 { let a = angle.to_radians(); normalize(add(scale(d, a.cos()), scale(d2, a.sin()))) }

pub fn ports(f: &Fitting) -> Vec<Port> {
    let c = f.position; let d = f.dir_n(); let d2 = f.dir2_n(); let dd = f.d();
    let l = half_len(&f.joint, dd); let l2 = half_len(&f.joint, f.d2());
    let p = |pos: P3, dir: P3, dn: f64, name: &'static str| Port { pos, dir, dn, name };
    match f.kind_info().shape {
        Shape::Inline => {
            let dn2 = if f.kind == "reducer" { f.dn2.unwrap_or(f.dn) } else { f.dn };
            vec![p(sub(c, scale(d, l)), scale(d, -1.0), f.dn, "in"), p(add(c, scale(d, if f.kind == "reducer" { l2 } else { l })), d, dn2, "out")]
        }
        Shape::Elbow => {
            let o = elbow_out(d, d2, f.angle_deg());
            let dn2 = if f.kind == "angleValve" { 10.0 } else { f.dn };
            vec![p(sub(c, scale(d, l)), scale(d, -1.0), f.dn, "in"), p(add(c, scale(o, if f.kind == "radiatorValve" { l * 1.3 } else { l })), o, dn2, "out")]
        }
        Shape::Tee => {
            let b = if f.kind == "branch45" { elbow_out(d, d2, 45.0) } else { d2 };
            vec![p(sub(c, scale(d, l)), scale(d, -1.0), f.dn, "in"), p(add(c, scale(d, l)), d, f.dn, "out"), p(add(c, scale(b, l2)), b, f.dn2.unwrap_or(f.dn), "branch")]
        }
        Shape::End => vec![p(sub(c, scale(d, l)), scale(d, -1.0), f.dn, "in")],
        Shape::WallPlate => vec![p(sub(c, scale(d, l)), scale(d, -1.0), f.dn, "in"), p(add(c, scale(d2, 0.02)), d2, 15.0, "thread")],
        Shape::Mixer => {
            if f.kind == "mixerBasin" {
                let b = [0.0, -0.30, 0.0];
                vec![p(add(add(c, scale(d, -0.012)), b), [0.0, -1.0, 0.0], 10.0, "hot"), p(add(add(c, scale(d, 0.012)), b), [0.0, -1.0, 0.0], 10.0, "cold")]
            } else {
                vec![p(sub(c, scale(d, 0.075)), scale(d2, -1.0), f.dn, "hot"), p(add(c, scale(d, 0.075)), scale(d2, -1.0), f.dn, "cold")]
            }
        }
        Shape::Manifold => {
            let n = f.outlets.unwrap_or(4).max(1) as usize; let lm = 0.05 * n as f64 + 0.04;
            let mut v = vec![p(sub(c, scale(d, lm / 2.0)), scale(d, -1.0), f.dn, "in")];
            for k in 0..n { let along = -lm / 2.0 + 0.03 + 0.05 * k as f64; v.push(Port { pos: add(add(c, scale(d, along)), scale(d2, 0.05)), dir: d2, dn: f.dn2.unwrap_or(16.0), name: "outlet" }); }
            v
        }
        Shape::Trap => vec![p(c, [0.0, 1.0, 0.0], f.dn, "in"), p(add(add(c, [0.0, -0.10, 0.0]), scale(d, 0.12)), d, f.dn, "out")],
    }
}

/// Draw the fitting in world coordinates (`y0` = the storey floor height).
pub fn build(out: &mut Out, f: &Fitting, y0: f64) {
    let lk = look(f); let paint = f.paint.clone(); let body = paint.clone().unwrap_or(lk.mat.clone());
    let c = add(f.position, [0.0, y0, 0.0]); let d = f.dir_n(); let d2 = f.dir2_n(); let dd = f.d(); let d2d = f.d2();
    let l = half_len(&f.joint, dd); let l2 = half_len(&f.joint, d2d);
    let sides = sides_for(lk.body_r);
    let brass = paint.clone().unwrap_or(look_id("brass")); let chrome = paint.clone().unwrap_or(look_id("chrome")); let handle = paint.clone().unwrap_or(look_id("handle")); let dark = look_id("pressRing");
    let w = f.position; // relative
    let world = |p: P3| add(p, [0.0, y0, 0.0]);
    let ports_w: Vec<Port> = ports(f).into_iter().map(|p| Port { pos: world(p.pos), ..p }).collect();
    // hex nuts / press rings at every pipe port
    let port_ends = |out: &mut Out| {
        for p in &ports_w {
            if p.name == "thread" || p.name == "hot" && f.kind == "mixerBasin" || p.name == "cold" && f.kind == "mixerBasin" { continue; }
            let r = p.dn / 2000.0;
            if lk.nut_r > 0.0 && !(f.kind == "manifold" && p.name == "outlet") {
                let nr = if f.kind_info().tapware { 0.9 * r } else { lk.nut_r * (r / dd.max(1e-6)).max(0.6) };
                out.sweep("nut", &[sub(p.pos, scale(p.dir, lk.nut_len.max(0.008))), p.pos], nr, 6, &brass, true);
            } else if lk.ring {
                let br = if f.kind == "manifold" && p.name == "outlet" { 0.6 * r } else { lk.body_r * (r / dd.max(1e-6)).max(0.6) };
                out.ring("ring", sub(p.pos, scale(p.dir, 0.004)), p.dir, br * 0.9, br * 1.08, 0.006, sides, if f.joint == "ring" { &dark } else if f.joint == "pushfit" { &chrome } else { &dark });
            }
        }
    };
    match f.kind_info().shape {
        Shape::Inline => {
            let a = sub(c, scale(d, l)); let b = add(c, scale(d, l));
            match f.kind.as_str() {
                "reducer" => { let b2 = add(c, scale(d, l2)); out.cylinder("body", a, add(a, scale(d, socket_len(&f.joint, dd))), lk.body_r, sides, &body); out.frustum("body", add(a, scale(d, socket_len(&f.joint, dd))), sub(b2, scale(d, socket_len(&f.joint, d2d))), lk.body_r, lk.body_r * d2d / dd, sides, &body); out.cylinder("body", sub(b2, scale(d, socket_len(&f.joint, d2d))), b2, lk.body_r * d2d / dd, sides, &body); }
                "adapter" => { out.cylinder("body", a, c, lk.body_r, sides, &body); out.sweep("nut", &[c, b], 0.95 * dd, 6, &brass, true); out.disc("ring", add(b, scale(d, 0.0005)), d, 0.6 * dd, sides, &dark); }
                _ => out.cylinder("body", a, b, lk.body_r, sides, &body),
            }
            match f.kind.as_str() {
                "stopValve" => { out.cylinder("body", sub(c, scale(d2, 0.2 * dd)), add(c, scale(d2, 1.3 * dd)), 0.9 * dd, sides, &brass); out.cylinder("handle", c, add(c, scale(d2, 2.6 * dd)), 0.22 * dd, 8, &chrome); out.ring("handle", add(c, scale(d2, 2.6 * dd)), d2, 0.5 * dd, 1.2 * dd, 0.3 * dd, 12, &handle); }
                "ballValve" => { out.cylinder("body", sub(c, scale(d, 0.7 * dd)), add(c, scale(d, 0.7 * dd)), 1.0 * dd, sides, &brass); out.cylinder("handle", c, add(c, scale(d2, 1.2 * dd)), 0.25 * dd, 8, &brass); let top = add(c, scale(d2, 1.2 * dd)); let w3 = cross(d, d2); out.block("handle", add(top, scale(d, 1.1 * dd)), d, d2, w3, [3.0 * dd, 0.35 * dd, 0.45 * dd], &handle); }
                "checkValve" => { out.cylinder("body", sub(c, scale(d, 0.8 * dd)), add(c, scale(d, 0.8 * dd)), 0.95 * dd, sides, &brass); }
                "pressureReducer" => { out.cylinder("body", sub(c, scale(d, 0.8 * dd)), add(c, scale(d, 0.8 * dd)), 1.0 * dd, sides, &brass); out.cylinder("body", c, add(c, scale(d2, 2.6 * dd)), 1.3 * dd, sides, &brass); out.cylinder("handle", add(c, scale(d2, 2.6 * dd)), add(c, scale(d2, 3.1 * dd)), 0.9 * dd, 8, &dark); let w3 = cross(d, d2); out.cylinder("handle", c, add(c, scale(w3, 1.6 * dd)), 0.25 * dd, 8, &brass); out.ring("handle", add(c, scale(w3, 1.6 * dd)), w3, 0.0001, 1.3 * dd, 0.5 * dd, 14, &chrome); }
                "filter" => { out.cylinder("body", sub(c, scale(d, 0.8 * dd)), add(c, scale(d, 0.8 * dd)), 1.0 * dd, sides, &brass); let down = normalize(add(scale(d2, -1.0), scale(d, 0.5))); out.cylinder("body", c, add(c, scale(down, 3.2 * dd)), 1.0 * dd, sides, &chrome); }
                "meter" => { let w3 = cross(d, d2); out.block("body", c, d, d2, w3, [5.0 * dd, 3.0 * dd, 3.0 * dd], &brass); out.ring("handle", add(c, scale(d2, 1.5 * dd)), d2, 0.0001, 1.3 * dd, 0.3 * dd, 16, &chrome); }
                "cleanout" => { out.cylinder("body", c, add(c, scale(d2, 1.3 * dd)), 0.85 * dd, sides, &body); out.ring("handle", add(c, scale(d2, 1.3 * dd)), d2, 0.0001, 0.95 * dd, 0.02, 8, &body); }
                _ => {}
            }
        }
        Shape::Elbow => {
            let o = elbow_out(d, d2, f.angle_deg());
            let a = sub(c, scale(d, l)); let b = add(c, scale(o, if f.kind == "radiatorValve" { l * 1.3 } else { l }));
            let (path, _) = bend_path(&[a, c, b], 0.9 * dd);
            out.sweep("body", &path, if f.kind == "angleValve" || f.kind == "radiatorValve" { 0.8 * dd } else { lk.body_r }, sides, &if f.kind == "radiatorValve" { brass.clone() } else { body.clone() }, true);
            if f.kind == "angleValve" { let w3 = normalize(cross(d, o)); out.cylinder("handle", c, add(c, scale(w3, 1.5 * dd)), 0.25 * dd, 8, &chrome); out.ring("handle", add(c, scale(w3, 1.5 * dd)), w3, 0.0001, 1.0 * dd, 0.35 * dd, 12, &chrome); }
            if f.kind == "radiatorValve" { out.frustum("handle", add(c, scale(d, 0.6 * dd)), add(c, scale(d, 3.6 * dd)), 1.0 * dd, 1.25 * dd, 14, &chrome); out.disc("handle", add(c, scale(d, 3.6 * dd)), d, 1.25 * dd, 14, &chrome); }
        }
        Shape::Tee => {
            let a = sub(c, scale(d, l)); let b = add(c, scale(d, l));
            let bd = if f.kind == "branch45" { elbow_out(d, d2, 45.0) } else { d2 };
            out.cylinder("body", a, b, lk.body_r, sides, &body);
            out.cylinder("body", c, add(c, scale(bd, l2)), lk.body_r * (d2d / dd).max(0.6), sides, &body);
        }
        Shape::End => { let a = sub(c, scale(d, l)); out.cylinder("body", a, add(c, scale(d, 0.3 * dd)), lk.body_r, sides, &body); }
        Shape::WallPlate => {
            let a = sub(c, scale(d, l)); let (path, _) = bend_path(&[a, c, add(c, scale(d2, 0.012))], 0.8 * dd);
            out.sweep("body", &path, lk.body_r, sides, &body, true);
            out.sweep("nut", &[c, add(c, scale(d2, 0.02))], 0.014, 6, &brass, true);
            out.disc("ring", add(c, scale(d2, 0.0205)), d2, 0.0085, 10, &dark);
            let w3 = cross(d, d2); out.block("plate", sub(c, scale(d2, 0.013)), d, w3, d2, [0.06, 0.06, 0.004], &brass);
            let _ = w;
        }
        Shape::Mixer => build_mixer(out, f, c, d, d2, &chrome, &handle),
        Shape::Manifold => {
            let n = f.outlets.unwrap_or(4).max(1) as usize; let lm = 0.05 * n as f64 + 0.04;
            let a = sub(c, scale(d, lm / 2.0)); let b = add(c, scale(d, lm / 2.0));
            out.cylinder("body", a, b, 0.018, 14, &if f.joint == "press" { look_id("press") } else { brass.clone() });
            out.sweep("nut", &[sub(b, scale(d, 0.01)), add(b, scale(d, 0.004))], 0.02, 6, &brass, true);
            for k in 0..n {
                let along = -lm / 2.0 + 0.03 + 0.05 * k as f64; let p = add(c, scale(d, along));
                out.cylinder("body", p, add(p, scale(d2, 0.05)), 0.009, 10, &brass);
                out.cylinder("handle", p, add(p, scale(d2, -0.035)), 0.011, 10, &handle);
                out.disc("handle", add(p, scale(d2, -0.035)), scale(d2, -1.0), 0.011, 10, &handle);
            }
            let w3 = cross(d, d2);
            for s in [-1.0, 1.0] { let p = add(c, scale(d, s * (lm / 2.0 - 0.025))); out.block("plate", add(p, scale(w3, -0.03)), d, d2, w3, [0.02, 0.05, 0.004], &look_id("bracket")); out.block("plate", add(p, scale(w3, -0.015)), d, d2, w3, [0.02, 0.012, 0.03], &look_id("bracket")); }
        }
        Shape::Trap => {
            let r = dd / 2.0;
            out.cylinder("body", c, add(c, [0.0, -0.09, 0.0]), r, 12, &chrome);
            out.cylinder("body", add(c, [0.0, -0.07, 0.0]), add(c, [0.0, -0.16, 0.0]), 1.15 * dd, 16, &chrome);
            out.cylinder("body", add(c, [0.0, -0.10, 0.0]), add(add(c, [0.0, -0.10, 0.0]), scale(d, 0.12)), r, 12, &chrome);
            out.sweep("nut", &[add(c, [0.0, -0.012, 0.0]), c], 0.75 * dd, 6, &chrome, true);
        }
    }
    port_ends(out);
}

fn build_mixer(out: &mut Out, f: &Fitting, c: P3, d: P3, d2: P3, chrome: &str, handle: &str) {
    let y = [0.0, 1.0, 0.0];
    match f.kind.as_str() {
        "mixerBasin" => {
            out.cylinder("body", add(c, [0.0, -0.02, 0.0]), add(c, [0.0, 0.13, 0.0]), 0.02, 14, chrome);
            out.disc("body", add(c, [0.0, 0.13, 0.0]), y, 0.02, 14, chrome);
            let s0 = add(c, [0.0, 0.10, 0.0]); let (sp, _) = bend_path(&[s0, add(s0, scale(d2, 0.10)), add(add(s0, scale(d2, 0.10)), [0.0, -0.03, 0.0])], 0.02);
            out.sweep("body", &sp, 0.011, 10, chrome, true);
            let w3 = cross(d, d2); let _ = w3;
            out.block("handle", add(add(c, [0.0, 0.145, 0.0]), scale(d2, -0.02)), d, y, d2, [0.012, 0.012, 0.09], handle);
            for s in [-1.0, 1.0] { let t = add(c, scale(d, s * 0.012)); out.cylinder("body", add(t, [0.0, -0.02, 0.0]), add(t, [0.0, -0.30, 0.0]), 0.005, 8, &look_id("pressRing")); out.sweep("nut", &[add(t, [0.0, -0.30, 0.0]), add(t, [0.0, -0.285, 0.0])], 0.009, 6, &look_id("brass"), true); }
        }
        _ => {
            let cb = add(c, scale(d2, 0.045));
            out.cylinder("body", add(cb, scale(d, -0.085)), add(cb, scale(d, 0.085)), 0.024, 16, chrome);
            for s in [-1.0, 1.0] { let wall = add(c, scale(d, s * 0.075)); out.cylinder("body", wall, add(wall, scale(d2, 0.045)), 0.012, 10, chrome); out.sweep("nut", &[add(wall, scale(d2, 0.03)), add(wall, scale(d2, 0.048))], 0.02, 6, chrome, true); out.ring("plate", add(wall, scale(d2, 0.004)), d2, 0.012, 0.03, 0.006, 16, chrome); }
            if f.kind == "mixerThermo" {
                for s in [-1.0, 1.0] { let e = add(cb, scale(d, s * 0.085)); out.cylinder("handle", e, add(e, scale(d, s * 0.035)), 0.021, 14, chrome); out.disc("handle", add(e, scale(d, s * 0.035)), scale(d, s), 0.021, 14, chrome); }
                out.cylinder("body", add(cb, [0.0, -0.02, 0.0]), add(cb, [0.0, -0.055, 0.0]), 0.012, 10, chrome);
                out.sweep("nut", &[add(cb, [0.0, -0.055, 0.0]), add(cb, [0.0, -0.045, 0.0])], 0.015, 6, chrome, true);
            } else {
                let (sp, _) = bend_path(&[cb, add(cb, scale(d2, 0.09)), add(add(cb, scale(d2, 0.09)), [0.0, -0.06, 0.0])], 0.03);
                out.sweep("body", &sp, 0.011, 10, chrome, true);
                out.block("handle", add(cb, [0.0, 0.03, 0.0]), d, y, d2, [0.1, 0.012, 0.02], handle);
                out.cylinder("handle", add(cb, [0.0, 0.02, 0.0]), add(cb, [0.0, 0.03, 0.0]), 0.014, 10, chrome);
            }
        }
    }
}
