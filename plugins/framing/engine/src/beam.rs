//! `framing.beam`: a built-up lumber, LVL or glulam beam with posts down to the storey floor, and the
//! joist hangers where floor joists end on it.

use plannr_core::geom::{cross, normalize, P3};

use crate::data::*;
use crate::expand::*;
use crate::hardware::*;

const UP: P3 = [0.0, 1.0, 0.0];

pub fn expand_beam(id: &str, bm: &Beam, house: &House, layer: &str) -> Expansion {
    let mut ex = Expansion::default();
    let l = bm.length();
    if l < 0.1 { return ex; }
    let base = house.base(bm.level);
    let top = base + bm.elevation.unwrap_or_else(|| house.height(bm.level));
    let [b, h] = bm.section();
    let u = normalize([bm.b[0] - bm.a[0], 0.0, bm.b[1] - bm.a[1]]);
    let n = normalize(cross(UP, u));
    let origin = to_p3(bm.a, 0.0);
    let m = &mut ex.members;
    match bm.material.as_str() {
        "lvl" | "glulam" => push_member(m, "ply.0".into(), "beams", [b, h], "header", origin, u, UP, n, [[0.0, l], [top - h, top], [-b / 2.0, b / 2.0]], 0),
        _ => {
            let [t, d] = lumber_or(&bm.size, "2x10");
            for i in 0..bm.plies.clamp(1, 4) as usize {
                let z0 = -b / 2.0 + i as f64 * t;
                push_member(m, format!("ply.{i}"), "beams", [t, d], "header", origin, u, UP, n, [[0.0, l], [top - h, top], [z0, z0 + t]], 0);
            }
        }
    }
    if bm.posts && top - h - base > 0.1 {
        let [pt, pd] = lumber_or(&bm.post_size, "4x4");
        for (i, s) in bm.supports().into_iter().enumerate() {
            let s = s.clamp(pd / 2.0, (l - pd / 2.0).max(pd / 2.0));
            push_member(m, format!("post.{i}"), "posts", [pt, pd], "lumber", origin, u, UP, n, [[s - pd / 2.0, s + pd / 2.0], [base, top - h], [-pt / 2.0, pt / 2.0]], 1);
        }
    }
    let mut f = Faces::new(layer);
    emit_members(&ex.members, id, &mut f);
    if house.engineering.hardware.joist_hangers {
        let mut k = 0;
        for fl in &house.floors {
            let joists = expand_floor("", fl, house, "").members;
            for j in joists.iter().filter(|j| j.key.starts_with("joist.")) {
                let along_x = ((j.r[0][1] - j.r[0][0]) - j.length).abs() < 1e-9;
                let (y0, y1) = (j.r[1][0], j.r[1][1]);
                // a joist that sits on top of the beam (no vertical overlap) needs no hanger
                if y1 < top - h + 0.02 || y0 > top - 0.02 { continue; }
                let ends: [[f64; 2]; 2] = if along_x {
                    let zm = (j.r[2][0] + j.r[2][1]) / 2.0; [[j.r[0][0], zm], [j.r[0][1], zm]]
                } else {
                    let xm = (j.r[0][0] + j.r[0][1]) / 2.0; [[xm, j.r[2][0]], [xm, j.r[2][1]]]
                };
                for e in ends {
                    let t = (e[0] - bm.a[0]) * u[0] + (e[1] - bm.a[1]) * u[2];
                    let off = (e[0] - bm.a[0]) * n[0] + (e[1] - bm.a[1]) * n[2];
                    if t < -0.02 || t > l + 0.02 || off.abs() > b / 2.0 + 0.08 { continue; }
                    let key = format!("hw.hanger.{k}");
                    k += 1;
                    let (hw, hh) = (j.size[0] / 2.0 + 0.004, (y1 - y0) * 0.8);
                    let r = if along_x { [[e[0] - 0.04, e[0] + 0.04], [y0, y0 + hh], [e[1] - hw, e[1] + hw]] } else { [[e[0] - hw, e[0] + hw], [y0, y0 + hh], [e[1] - 0.04, e[1] + 0.04]] };
                    f.boxf(&format!("{id}#{key}"), [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], UP, [0.0, 0.0, 1.0], r, &material_id("steel"));
                    ex.hardware.push(Piece { key, description: HANGER });
                }
            }
        }
    }
    ex.faces = f.out;
    ex
}
