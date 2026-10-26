//! `framing.stair`: a straight stair on notched (sawtooth) stringers with treads and optional risers.

use plannr_core::geom::{add, cross, normalize, scale, P3};

use crate::data::*;
use crate::expand::*;
use crate::hardware::*;

const UP: P3 = [0.0, 1.0, 0.0];
pub const NOSING: f64 = 0.025;
const RISER_BOARD: f64 = 0.018;

/// The stringer profile in (run, height): notched sawtooth on top (the tread seats are `tt` under
/// each step), a straight underside `depth` under the notch tips, cut level on the floor and plumb
/// against the header at the top.
pub fn stringer_profile(n: usize, r: f64, g: f64, tt: f64, depth: f64) -> Vec<[f64; 2]> {
    if n < 2 { return vec![]; }
    let m = r / g;
    let cos = g / (r * r + g * g).sqrt();
    // underside: parallel to the line through the notch tips (y = m·u + r − tt), `depth` below it
    let under = |u: f64| m * u + r - tt - depth / cos;
    let mut p = vec![[0.0, 0.0]];
    for k in 1..n { p.push([(k - 1) as f64 * g, k as f64 * r - tt]); p.push([k as f64 * g, k as f64 * r - tt]); }
    let ue = (n - 1) as f64 * g;
    p.push([ue, under(ue).max(0.0)]);
    let u0 = ((depth / cos - r + tt) / m).clamp(0.0, ue);
    if u0 > 1e-6 { p.push([u0, 0.0]); }
    p
}

fn signed_area(p: &[[f64; 2]]) -> f64 { let n = p.len(); (0..n).map(|i| { let (a, b) = (p[i], p[(i + 1) % n]); a[0] * b[1] - b[0] * a[1] }).sum::<f64>() / 2.0 }

pub fn expand_stair(id: &str, st: &Stair, house: &House, layer: &str) -> Expansion {
    let mut ex = Expansion::default();
    let (n, r) = st.risers_of(house);
    if n < 2 { return ex; }
    let g = st.tread_depth.max(0.15);
    let tt = st.tread_thickness.max(0.01);
    let w = st.width.max(0.4);
    let d = st.dir();
    let u = [d[0], 0.0, d[1]];
    let wv = normalize(cross(UP, u));
    let origin = to_p3(st.a, house.base(st.level));
    let [sb, sd] = lumber_or(&st.stringer_size, "2x12");
    let at = |uu: f64, y: f64, ww: f64| add(add(add(origin, scale(u, uu)), scale(UP, y)), scale(wv, ww));
    let mut f = Faces::new(layer);
    let prof = stringer_profile(n, r, g, tt, sd);
    let ccw = signed_area(&prof) > 0.0;
    let ns = st.stringers.max(2) as usize;
    let run = (n - 1) as f64 * g;
    let slope_len = (run * run + (n as f64 * r).powi(2)).sqrt();
    let mat = material_id("lumber");
    for j in 0..ns {
        let wc = -w / 2.0 + sb / 2.0 + j as f64 * (w - sb) / (ns - 1) as f64;
        let (w0, w1) = (wc - sb / 2.0, wc + sb / 2.0);
        let key = format!("{id}#stringer.{j}");
        let side = |ww: f64| prof.iter().map(|p| at(p[0], p[1], ww)).collect::<Vec<_>>();
        f.poly(&key, side(w0), scale(wv, -1.0), &mat);
        f.poly(&key, side(w1), wv, &mat);
        for i in 0..prof.len() {
            let (p, q) = (prof[i], prof[(i + 1) % prof.len()]);
            let e = [q[0] - p[0], q[1] - p[1]];
            // outward in-plane normal: right of the edge for a counter-clockwise (u, y) profile
            let nn = if ccw { [e[1], -e[0]] } else { [-e[1], e[0]] };
            let nrm = normalize(add(scale(u, nn[0]), scale(UP, nn[1])));
            f.poly(&key, vec![at(p[0], p[1], w0), at(q[0], q[1], w0), at(q[0], q[1], w1), at(p[0], p[1], w1)], nrm, &mat);
        }
        // takeoff: one piece of stringer stock along the pitch line
        let ex_s = normalize(add(scale(u, run), scale(UP, n as f64 * r)));
        let ey_s = normalize(cross(wv, ex_s));
        ex.members.push(Member { key: format!("stringer.{j}"), category: "stringers", size: [sb, sd], size_label: size_label([sb, sd]), length: slope_len, material: "lumber", origin: at(0.0, 0.0, 0.0), ex: ex_s, ey: ey_s, ez: wv, r: [[0.0, slope_len], [-sd, 0.0], [w0, w1]] });
    }
    // treads (with a nosing) and risers
    let mut parts = vec![];
    for k in 1..n {
        let (u0, u1) = ((k - 1) as f64 * g, k as f64 * g);
        let y = k as f64 * r;
        push_member(&mut parts, format!("tread.{k}"), "treads", [tt, g + NOSING], "lumber", origin, u, UP, wv, [[u0 - NOSING, u1], [y - tt, y], [-w / 2.0, w / 2.0]], 2);
        if st.risers {
            push_member(&mut parts, format!("riser.{k}"), "risers", [RISER_BOARD, r - tt], "lumber", origin, u, UP, wv, [[u0 - RISER_BOARD, u0], [(k - 1) as f64 * r, y - tt], [-w / 2.0, w / 2.0]], 2);
        }
    }
    emit_members(&parts, id, &mut f);
    ex.members.extend(parts);
    ex.faces = f.out;
    ex
}
