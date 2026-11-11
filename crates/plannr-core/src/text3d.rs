//! 3D Text: the outlines of letters as faces, flat or pulled out into solids.
//!
//! The outlines come from the fonts in `fonts/` (Droid Sans and Droid Serif, Apache 2.0), embedded in the
//! binary: each glyph is an advance width and a path of moves, lines and quadratic curves.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::geom::*;
use crate::model::*;
use crate::planar::{area2, inside, P2};

/// Edges between side faces of a letter that meet at up to this angle are soft: curves look round.
pub const SIDE_SMOOTH: f64 = 30.0;
/// Straight pieces per curve of an outline.
const CURVE_STEPS: usize = 4;

struct Font { glyphs: HashMap<char, (f64, String)>, cap: f64, line: f64 }

fn parse(json: &str) -> Font {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let mut glyphs = HashMap::new();
    if let Some(g) = v.get("glyphs").and_then(|g| g.as_object()) {
        for (k, d) in g {
            let mut cs = k.chars();
            let (Some(c), None) = (cs.next(), cs.next()) else { continue };
            let adv = d.get(0).and_then(|a| a.as_f64()).unwrap_or(0.0);
            glyphs.insert(c, (adv, d.get(1).and_then(|o| o.as_str()).unwrap_or("").to_string()));
        }
    }
    let res = v.get("resolution").and_then(|r| r.as_f64()).unwrap_or(1000.0);
    // letters are sized by their capitals: the height of an H
    let cap = glyphs.get(&'H').map(|(_, o)| contours(o).iter().flatten().fold(0.0f64, |m, p| m.max(p[1]))).filter(|c| *c > 1.0).unwrap_or(res);
    Font { cap, line: v.get("lineHeight").and_then(|r| r.as_f64()).unwrap_or(res * 1.6), glyphs }
}

fn font(name: &str, bold: bool) -> Option<&'static Font> {
    static FONTS: [OnceLock<Font>; 4] = [OnceLock::new(), OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let i = match (name, bold) { ("sans", false) => 0, ("sans", true) => 1, ("serif", false) => 2, ("serif", true) => 3, _ => return None };
    Some(FONTS[i].get_or_init(|| parse(match i {
        0 => include_str!("../fonts/sans.json"), 1 => include_str!("../fonts/sans_bold.json"),
        2 => include_str!("../fonts/serif.json"), _ => include_str!("../fonts/serif_bold.json"),
    })))
}

/// The closed outlines of a glyph path, curves as short straight pieces (font units).
fn contours(path: &str) -> Vec<Vec<P2>> {
    let mut out: Vec<Vec<P2>> = vec![];
    let mut cur: Vec<P2> = vec![];
    let mut t = path.split_whitespace();
    let num = |t: &mut std::str::SplitWhitespace| t.next().and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
    let flush = |cur: &mut Vec<P2>, out: &mut Vec<Vec<P2>>| {
        let mut l: Vec<P2> = vec![];
        for p in cur.drain(..) { if l.last().map_or(true, |q: &P2| (q[0] - p[0]).abs() > 1e-6 || (q[1] - p[1]).abs() > 1e-6) { l.push(p); } }
        while l.len() > 1 && (l[0][0] - l[l.len() - 1][0]).abs() < 1e-6 && (l[0][1] - l[l.len() - 1][1]).abs() < 1e-6 { l.pop(); }
        if l.len() >= 3 && area2(&l).abs() > 1e-6 { out.push(l); }
    };
    while let Some(cmd) = t.next() {
        match cmd {
            "m" => { flush(&mut cur, &mut out); cur.push([num(&mut t), num(&mut t)]); }
            "l" => cur.push([num(&mut t), num(&mut t)]),
            "q" => {
                // the end point comes first, then the control point
                let (e, c) = ([num(&mut t), num(&mut t)], [num(&mut t), num(&mut t)]);
                let s = cur.last().copied().unwrap_or(e);
                for k in 1..=CURVE_STEPS {
                    let u = k as f64 / CURVE_STEPS as f64; let w = 1.0 - u;
                    cur.push([w * w * s[0] + 2.0 * w * u * c[0] + u * u * e[0], w * w * s[1] + 2.0 * w * u * c[1] + u * u * e[1]]);
                }
            }
            "b" => {
                let (e, c1, c2) = ([num(&mut t), num(&mut t)], [num(&mut t), num(&mut t)], [num(&mut t), num(&mut t)]);
                let s = cur.last().copied().unwrap_or(e);
                let steps = CURVE_STEPS + 2;
                for k in 1..=steps {
                    let u = k as f64 / steps as f64; let w = 1.0 - u;
                    let f = |i: usize| w * w * w * s[i] + 3.0 * w * w * u * c1[i] + 3.0 * w * u * u * c2[i] + u * u * u * e[i];
                    cur.push([f(0), f(1)]);
                }
            }
            _ => {}
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// The shapes a glyph's outlines bound: each an outer loop with the holes cut in it (the bowl of an o, both of a B).
fn shapes(cs: Vec<Vec<P2>>) -> Vec<(Vec<P2>, Vec<Vec<P2>>)> {
    // an outline that lies inside an odd number of others is a hole
    let within: Vec<Vec<usize>> = (0..cs.len()).map(|i| (0..cs.len()).filter(|&j| j != i && inside(cs[i][0], &cs[j])).collect()).collect();
    let mut out: Vec<(usize, Vec<P2>, Vec<Vec<P2>>)> = (0..cs.len()).filter(|&i| within[i].len() % 2 == 0).map(|i| (i, cs[i].clone(), vec![])).collect();
    for i in (0..cs.len()).filter(|&i| within[i].len() % 2 == 1) {
        // it belongs to the smallest outline around it
        let Some(&parent) = within[i].iter().filter(|j| within[**j].len() % 2 == 0).min_by(|a, b| area2(&cs[**a]).abs().partial_cmp(&area2(&cs[**b]).abs()).unwrap()) else { continue };
        if let Some(s) = out.iter_mut().find(|s| s.0 == parent) { s.2.push(cs[i].clone()); }
    }
    out.into_iter().map(|(_, o, h)| (o, h)).collect()
}

pub struct TextSpec<'a> {
    pub text: &'a str,
    pub font: &'a str,
    pub bold: bool,
    /// Height of the capitals.
    pub height: f64,
    /// How far the letters stand out of their plane (0: flat faces).
    pub depth: f64,
    /// Where the first line starts: the left end of its baseline (the middle or right end with `align`).
    pub position: P3,
    /// The letters face along `normal` and read along `u`.
    pub normal: P3,
    pub u: P3,
    pub align: &'a str,
}

/// The faces of a text: one face per closed part of a letter, or a closed solid of each when `depth` is set.
pub fn faces(s: &TextSpec, mat: Option<String>, layer: &str) -> Result<Vec<Face>, String> {
    let font = font(s.font, s.bold).ok_or_else(|| format!("no font {}: use sans or serif", s.font))?;
    let k = s.height / font.cap;
    let (n, u) = (s.normal, s.u); let v = cross(n, u);
    let mut out: Vec<Face> = vec![];
    for (li, line) in s.text.lines().enumerate() {
        let glyph = |c: char| font.glyphs.get(&c).or_else(|| font.glyphs.get(&'?'));
        let width: f64 = line.chars().filter_map(glyph).map(|g| g.0).sum::<f64>() * k;
        let mut x = match s.align { "center" => -width / 2.0, "right" => -width, _ => 0.0 };
        let y = -(li as f64) * font.line * k;
        for c in line.chars() {
            let Some((adv, path)) = glyph(c) else { continue };
            let to3 = |q: &P2| add(s.position, add(scale(u, x + q[0] * k), scale(v, y + q[1] * k)));
            for (outer, holes) in shapes(contours(path)) {
                let orient = |l: &Vec<P2>, want: P3| { let mut l: Vec<P3> = l.iter().map(to3).collect(); if dot(newell(&l), want) < 0.0 { l.reverse(); } l };
                let (outer, holes): (Vec<P3>, Vec<Vec<P3>>) = (orient(&outer, n), holes.iter().map(|h| orient(h, scale(n, -1.0))).collect());
                if s.depth > 0.0 {
                    let mut solid = crate::solid::extrude_faces(&outer, &holes, scale(n, s.depth), mat.clone(), layer);
                    // (the base and the cap come first, then the sides)
                    for f in solid.iter_mut().skip(2) { f.extra.insert("smooth".into(), SIDE_SMOOTH.into()); }
                    out.extend(solid);
                } else {
                    out.push(Face { id: new_id(), layer: layer.to_string(), outer, holes, material: mat.clone(), back_material: None, extra: Default::default() });
                }
            }
            x += adv * k;
        }
    }
    if out.is_empty() { return Err("the text has no letters to draw".into()); }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(text: &str, depth: f64) -> TextSpec<'_> {
        TextSpec { text, font: "sans", bold: false, height: 1.0, depth, position: [0.0; 3], normal: [0.0, 0.0, 1.0], u: [1.0, 0.0, 0.0], align: "left" }
    }

    #[test]
    fn letters_have_their_holes_and_their_height() {
        let f = faces(&spec("H", 0.0), None, "l").unwrap();
        assert_eq!(f.len(), 1);
        let top = f[0].outer.iter().fold(f64::MIN, |m, p| m.max(p[1]));
        assert!((top - 1.0).abs() < 1e-9, "{top}");
        assert!(f[0].holes.is_empty());
        let b = faces(&spec("B", 0.0), None, "l").unwrap();
        assert_eq!((b.len(), b[0].holes.len()), (1, 2));
        let i = faces(&spec("i", 0.0), None, "l").unwrap();
        assert_eq!(i.len(), 2); // the stem and the dot
        // faces look along the normal, holes wind the other way
        for f in b.iter().chain(&i) { assert!(dot(normal(&f.outer), [0.0, 0.0, 1.0]) > 0.99); }
        assert!(dot(normal(&b[0].holes[0]), [0.0, 0.0, 1.0]) < -0.99);
    }

    #[test]
    fn every_font_draws_and_lines_stack() {
        for (font, bold) in [("sans", false), ("sans", true), ("serif", false), ("serif", true)] {
            let s = TextSpec { font, bold, ..spec("Ag é", 0.0) };
            assert!(faces(&s, None, "l").unwrap().len() >= 3, "{font} {bold}");
        }
        assert!(faces(&TextSpec { font: "comic", ..spec("a", 0.0) }, None, "l").is_err());
        assert!(faces(&spec("   ", 0.0), None, "l").is_err());
        let two = faces(&spec("H\nH", 0.0), None, "l").unwrap();
        let low = two[1].outer.iter().fold(f64::MIN, |m, p| m.max(p[1]));
        assert!(low < -0.3, "{low}");
        // centred text straddles its position
        let c = faces(&TextSpec { align: "center", ..spec("HH", 0.0) }, None, "l").unwrap();
        let (lo, hi) = c.iter().flat_map(|f| f.outer.iter()).fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p[0]), hi.max(p[0])));
        assert!((lo + hi).abs() < 0.2, "{lo} {hi}");
    }

    #[test]
    fn extruded_letters_are_closed_solids() {
        let f = faces(&spec("o", 0.25), None, "l").unwrap();
        let g = Entity::Group(Group { id: new_id(), layer: "l".into(), name: "t".into(), transform: mat_identity(), entities: f.iter().cloned().map(Entity::Face).collect(), extra: Default::default() });
        let vol = crate::solid::volume(&crate::solid::polys_of(&g, &mat_identity()));
        // a ring: its area times the depth
        let area: f64 = faces(&spec("o", 0.0), None, "l").unwrap().iter().map(|f| area(&f.outer) - f.holes.iter().map(|h| area(h)).sum::<f64>()).sum();
        assert!(vol > 0.0 && (vol - area * 0.25).abs() < 1e-6, "{vol} {area}");
        assert!(f.iter().skip(2).all(|f| f.smooth() == Some(SIDE_SMOOTH)));
        assert!(f[0].smooth().is_none());
    }
}
