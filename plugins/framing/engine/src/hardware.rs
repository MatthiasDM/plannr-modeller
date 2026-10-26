//! Connectors counted as pieces and drawn as small steel parts (Medeek-style): anchor bolts in the
//! bottom plates of ground-floor walls, hold-downs at wall ends, hurricane ties where trusses and
//! rafters bear, joist hangers where joists end on a beam (see `beam.rs`).

use plannr_core::geom::{add, scale, P3};
use plannr_core::model::Custom;

use crate::data::*;
use crate::expand::*;

/// One connector: its face key (`hw.<kind>.<i>`) and how the takeoff names it.
#[derive(Debug, Clone)]
pub struct Piece { pub key: String, pub description: &'static str }

pub const ANCHOR: &str = "Anchor bolt M12";
pub const HOLD_DOWN: &str = "Hold-down";
pub const TIE: &str = "Hurricane tie";
pub const HANGER: &str = "Joist hanger";

/// Distances from the wall start where anchor bolts go along `[u0, u1]`: within 0.3 m of every end of
/// a plate run, at most 1.8 m apart, never across a door (the plate is cut there).
pub fn anchor_bolt_offsets(w: &Wall, u0: f64, u1: f64) -> Vec<f64> {
    let mut cuts: Vec<(f64, f64)> = w.openings.iter().filter(|o| o.kind == "door").map(|o| (o.offset, o.offset + o.width)).collect();
    cuts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut runs = vec![];
    let mut s = u0;
    for (a, b) in cuts { if a > s { runs.push((s, a.min(u1))); } s = s.max(b); }
    if u1 > s { runs.push((s, u1)); }
    let mut out = vec![];
    for (a, b) in runs {
        let l = b - a;
        if l < 0.15 { continue; }
        if l <= 0.6 { out.push((a + b) / 2.0); continue; }
        let (first, last) = (a + 0.3, b - 0.3);
        let gaps = ((last - first) / 1.8).ceil().max(1.0) as usize;
        for k in 0..=gaps { out.push(first + (last - first) * k as f64 / gaps as f64); }
    }
    out
}

/// Anchor bolt positions (top of the bottom plate) on the wall's centre line a→b, storey-0 walls only.
pub fn anchor_bolts(w: &Wall, house: &House) -> Vec<P3> {
    if w.level != 0 { return vec![]; }
    let l = w.length();
    if l < 0.05 { return vec![]; }
    // along the middle of the studs, whatever line the wall was drawn on
    let band = crate::junction::Band::of(w);
    let y = house.base(0) + w.bottom_plates as f64 * w.plate()[0];
    anchor_bolt_offsets(w, 0.0, l).into_iter().map(|t| { let p = band.at(t, band.centre()); [p[0], y, p[1]] }).collect()
}

#[allow(clippy::too_many_arguments)]
fn steel(f: &mut Faces, id: &str, key: &str, origin: P3, ex: P3, ey: P3, ez: P3, r: [[f64; 2]; 3]) {
    f.boxf(&format!("{id}#{key}"), origin, ex, ey, ez, r, &material_id("steel"));
}

/// Add the connectors an element owns (pieces + faces). Beams add their hangers themselves.
pub fn attach(ex: &mut Expansion, c: &Custom, house: &House, layer: &str) {
    let hw = &house.engineering.hardware;
    let id = c.id.as_str();
    let mut f = Faces::new(layer);
    match c.type_.as_str() {
        T_WALL => {
            let Ok(w) = serde_json::from_value::<Wall>(c.data.clone()) else { return };
            // the bottom plate gives the wall's real frame (whatever reference line the wall uses)
            let Some(p) = ex.members.iter().find(|m| m.key == "plate.bottom.0").cloned() else { return };
            let y_top = ex.members.iter().filter(|m| m.key.starts_with("plate.bottom.")).map(|m| m.r[1][1]).fold(p.r[1][1], f64::max);
            let zc = (p.r[2][0] + p.r[2][1]) / 2.0;
            let (u0, u1) = (p.r[0][0], p.r[0][1]);
            if hw.anchor_bolts && w.level == 0 {
                for (i, u) in anchor_bolt_offsets(&w, u0, u1).into_iter().enumerate() {
                    let key = format!("hw.anchor.{i}");
                    steel(&mut f, id, &key, p.origin, p.ex, p.ey, p.ez, [[u - 0.006, u + 0.006], [p.r[1][0] - 0.12, y_top + 0.03], [zc - 0.006, zc + 0.006]]);
                    steel(&mut f, id, &key, p.origin, p.ex, p.ey, p.ez, [[u - 0.025, u + 0.025], [y_top, y_top + 0.004], [zc - 0.025, zc + 0.025]]);
                    ex.hardware.push(Piece { key, description: ANCHOR });
                }
            }
            if hw.hold_downs {
                let t = w.stud()[0];
                let zin = p.r[2][0];
                for (i, (a, b)) in [(u0 + t, u0 + t + 0.06), (u1 - t - 0.06, u1 - t)].into_iter().enumerate() {
                    let key = format!("hw.holddown.{i}");
                    steel(&mut f, id, &key, p.origin, p.ex, p.ey, p.ez, [[a, b], [y_top, y_top + 0.35], [zin - 0.004, zin]]);
                    ex.hardware.push(Piece { key, description: HOLD_DOWN });
                }
            }
        }
        T_ROOF if hw.hurricane_ties => {
            // one tie at each bearing of every truss bottom chord / ceiling joist (they span bearing to bearing)
            let bearers: Vec<Member> = ex.members.iter().filter(|m| (m.key.starts_with("truss.") && m.key.ends_with(".bottom")) || m.key.starts_with("ceilingJoist.")).cloned().collect();
            let mut i = 0;
            for m in bearers {
                let (x0, x1) = (m.r[0][0], m.r[0][1]);
                let z = m.r[2][1];
                for (a, b) in [(x0 + 0.01, x0 + 0.06), (x1 - 0.06, x1 - 0.01)] {
                    let key = format!("hw.tie.{i}");
                    i += 1;
                    steel(&mut f, id, &key, m.origin, m.ex, m.ey, m.ez, [[a, b], [m.r[1][0] - 0.06, m.r[1][0] + 0.1], [z, z + 0.003]]);
                    ex.hardware.push(Piece { key, description: TIE });
                }
            }
        }
        _ => {}
    }
    ex.faces.extend(f.out);
}

/// Faces for members (same as the walls / floors use).
pub fn emit_members(members: &[Member], id: &str, f: &mut Faces) {
    for m in members { f.boxf(&format!("{id}#{}", m.key), m.origin, m.ex, m.ey, m.ez, m.r, &material_id(m.material)); }
}

/// Nominal label of a section (2x10, else millimetres).
pub fn size_label(size: [f64; 2]) -> String {
    let mm = |v: f64| (v * 1000.0).round() as i64;
    match (mm(size[0]), mm(size[1])) {
        (38, 64) => "2x3".into(), (38, 89) => "2x4".into(), (38, 140) => "2x6".into(), (38, 184) => "2x8".into(), (38, 235) => "2x10".into(), (38, 286) => "2x12".into(),
        (89, 89) => "4x4".into(), (89, 140) => "4x6".into(), (140, 140) => "6x6".into(),
        (a, b) => format!("{a}x{b}"),
    }
}

/// A member box in a frame; `along` = which local axis is its length.
#[allow(clippy::too_many_arguments)]
pub fn push_member(v: &mut Vec<Member>, key: String, category: &'static str, size: [f64; 2], material: &'static str, origin: P3, ex: P3, ey: P3, ez: P3, r: [[f64; 2]; 3], along: usize) {
    let [[x0, x1], [y0, y1], [z0, z1]] = r;
    if x1 - x0 < 1e-6 || y1 - y0 < 1e-6 || z1 - z0 < 1e-6 { return; }
    let length = [x1 - x0, y1 - y0, z1 - z0][along];
    v.push(Member { key, category, size, size_label: size_label(size), length, material, origin, ex, ey, ez, r });
}

/// A point of a member's box in world coordinates.
pub fn world(m: &Member, x: f64, y: f64, z: f64) -> P3 { add(add(add(m.origin, scale(m.ex, x)), scale(m.ey, y)), scale(m.ez, z)) }
