//! Geometry: semantic entities → core faces. Ported from the original Uzzetje Studio compiler
//! (walls with openings cut as real holes, cavity leaves, floor slabs, flat/gable/hip/shed roofs,
//! fixture markers, run tubes), now emitting planar `Face`s instead of triangle buffers.

use std::collections::HashMap;

use plannr_core::geom::{add, cross, dot, newell, normalize, scale, sub, P3};
use plannr_core::model::{Custom, Document, Entity, Face};

use crate::data::*;

// ---- face builder ---------------------------------------------------------------------

pub struct Faces {
    pub out: Vec<Entity>,
    layer: String,
}

impl Faces {
    pub fn new(layer: &str) -> Self { Faces { out: vec![], layer: layer.to_string() } }

    /// A planar polygon wound so its normal follows `n`.
    pub fn poly(&mut self, id: &str, mut pts: Vec<P3>, n: P3, material: &str) {
        if pts.len() < 3 { return; }
        if dot(newell(&pts), n) < 0.0 { pts.reverse(); }
        if plannr_core::geom::area(&pts) < 1e-9 { return; }
        self.out.push(Entity::Face(Face {
            id: id.to_string(), layer: self.layer.clone(), outer: pts, holes: vec![],
            material: Some(material.to_string()), back_material: Some(material.to_string()), extra: Default::default(),
        }));
    }
    pub fn quad(&mut self, id: &str, a: P3, b: P3, c: P3, d: P3, n: P3, material: &str) { self.poly(id, vec![a, b, c, d], n, material); }
    /// A loose edge (tile courses, outlines).
    pub fn edge(&mut self, id: &str, a: P3, b: P3) {
        if plannr_core::geom::dist(a, b) < 1e-6 { return; }
        self.out.push(Entity::Edge(plannr_core::model::Edge { id: id.to_string(), layer: self.layer.clone(), a, b, extra: Default::default() }));
    }

    /// A planar polygon with holes, wound so its normal follows `n`.
    pub fn poly_holes(&mut self, id: &str, mut pts: Vec<P3>, holes: Vec<Vec<P3>>, n: P3, material: &str) {
        if holes.is_empty() { return self.poly(id, pts, n, material); }
        if pts.len() < 3 { return; }
        if dot(newell(&pts), n) < 0.0 { pts.reverse(); }
        self.out.push(Entity::Face(Face {
            id: id.to_string(), layer: self.layer.clone(), outer: pts, holes,
            material: Some(material.to_string()), back_material: Some(material.to_string()), extra: Default::default(),
        }));
    }

    /// A horizontal slab (plan polygon from y0 to y1) with rectangular holes through it.
    pub fn slab_holes(&mut self, id: &str, plan: &[P2], holes: &[Vec<P2>], y0: f64, y1: f64, material: &str) {
        if holes.is_empty() { return self.slab(id, plan, y0, y1, material); }
        let at = |p: &P2, y: f64| -> P3 { [p[0], y, p[1]] };
        let bottom: Vec<P3> = plan.iter().map(|p| at(p, y0)).collect();
        let top: Vec<P3> = plan.iter().map(|p| at(p, y1)).collect();
        let hb: Vec<Vec<P3>> = holes.iter().map(|h| h.iter().map(|p| at(p, y0)).collect()).collect();
        let ht: Vec<Vec<P3>> = holes.iter().map(|h| h.iter().map(|p| at(p, y1)).collect()).collect();
        self.poly_holes(id, bottom.clone(), hb, [0.0, -1.0, 0.0], material);
        self.poly_holes(id, top, ht, [0.0, 1.0, 0.0], material);
        let c = plan.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0], a[1] + p[1]]);
        let c = [c[0] / plan.len() as f64, c[1] / plan.len() as f64];
        let n = plan.len();
        for i in 0..n {
            let (a, b) = (plan[i], plan[(i + 1) % n]);
            let mut sn = [b[1] - a[1], 0.0, -(b[0] - a[0])];
            let mid = [(a[0] + b[0]) * 0.5 - c[0], 0.0, (a[1] + b[1]) * 0.5 - c[1]];
            if dot(sn, mid) < 0.0 { sn = scale(sn, -1.0); }
            self.quad(id, at(&a, y0), at(&b, y0), at(&b, y1), at(&a, y1), sn, material);
        }
        for h in holes {
            let hc = h.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0], a[1] + p[1]]);
            let hc = [hc[0] / h.len() as f64, hc[1] / h.len() as f64];
            let m = h.len();
            for i in 0..m {
                let (a, b) = (h[i], h[(i + 1) % m]);
                let mut sn = [b[1] - a[1], 0.0, -(b[0] - a[0])];
                let mid = [(a[0] + b[0]) * 0.5 - hc[0], 0.0, (a[1] + b[1]) * 0.5 - hc[1]];
                if dot(sn, mid) > 0.0 { sn = scale(sn, -1.0); } // hole sides face inward
                self.quad(id, at(&a, y0), at(&b, y0), at(&b, y1), at(&a, y1), sn, material);
            }
        }
    }

    /// Extrude a planar polygon along `dir`: caps face ∓dir, sides face away from the centroid.
    pub fn prism(&mut self, id: &str, base: &[P3], dir: P3, cap_material: &str, side_material: &str) {
        let n = base.len();
        if n < 3 { return; }
        let dn = normalize(dir);
        let top: Vec<P3> = base.iter().map(|p| add(*p, dir)).collect();
        self.poly(id, base.to_vec(), scale(dn, -1.0), cap_material);
        self.poly(id, top.clone(), dn, cap_material);
        let c = base.iter().fold([0.0; 3], |acc, p| add(acc, *p));
        let c = scale(c, 1.0 / n as f64);
        for i in 0..n {
            let j = (i + 1) % n;
            let (a, b) = (base[i], base[j]);
            let mut sn = normalize(cross(sub(b, a), dir));
            let mid = sub(scale(add(a, b), 0.5), c);
            if dot(sn, mid) < 0.0 { sn = scale(sn, -1.0); }
            self.quad(id, a, b, top[j], top[i], sn, side_material);
        }
    }
    /// Extrude a plan polygon (x, z) from height y0 to y1.
    pub fn slab(&mut self, id: &str, plan: &[P2], y0: f64, y1: f64, material: &str) {
        let base: Vec<P3> = plan.iter().map(|p| [p[0], y0, p[1]]).collect();
        self.prism(id, &base, [0.0, y1 - y0, 0.0], material, material);
    }
}

// ---- wall collection --------------------------------------------------------------------
//
// Walls form a graph per storey: every room edge and free wall is projected onto its infinite
// line, collinear edges are split at every endpoint into elementary pieces, and each piece is
// built exactly once from whatever covers it (a room on each side → partition, a room on one
// side → exterior with the cavity build-up, only free walls → as drawn). Rooms that share only
// part of an edge therefore no longer build two coplanar, fighting walls over the common stretch.

#[derive(Debug, Clone)]
pub struct ResolvedOpening {
    pub id: String,
    pub kind: OpeningKind,
    /// Centre along the piece (from `a`).
    pub along: f64,
    pub width: f64,
    pub height: f64,
    pub sill: f64,
    /// False when the opening continues into the neighbouring piece on that side (no frame post there).
    pub frame_start: bool,
    pub frame_end: bool,
}

#[derive(Debug, Clone)]
pub struct WallEntry {
    /// Custom entity that builds this wall (the first room touching the piece, or the free wall).
    pub owner: String,
    /// Face-key base: `wall.<edge>` for room edges, `wall` for free walls.
    pub key: String,
    pub a: P2,
    pub b: P2,
    pub level: usize,
    pub height: f64,
    /// Only one room touches the piece (or the free wall is flagged exterior).
    pub exterior: bool,
    /// Unit plan normal pointing away from the owner room.
    pub out: P2,
    pub thickness: Option<f64>,
    pub interior: String,
    pub outside: String,
    pub openings: Vec<ResolvedOpening>,
    /// Every room touching the piece (owner first).
    pub rooms: Vec<String>,
    /// Draw the end caps; false where the neighbouring piece on the same line continues with the same section.
    pub cap_start: bool,
    pub cap_end: bool,
}

impl WallEntry {
    pub fn length(&self) -> f64 { dist2(self.a, self.b) }
    pub fn gross_area(&self) -> f64 { self.length() * self.height }
    pub fn opening_area(&self) -> f64 { self.openings.iter().map(|o| o.width * o.height).sum() }
}

#[derive(Default)]
pub struct WallSet {
    pub walls: Vec<WallEntry>,
}

/// Collinearity tolerances: 1 mm off the line, 0.1° of direction.
const LINE_EPS: f64 = 1e-3;
const DIR_EPS: f64 = 0.00175;

#[derive(Clone)]
enum SrcKind { Room { id: String, edge: usize }, Free { id: String } }

#[derive(Clone)]
struct Src {
    kind: SrcKind,
    a: P2,
    b: P2,
    level: usize,
    height: f64,
    thickness: Option<f64>,
    /// Free walls only: drawn as an exterior wall.
    exterior_flag: bool,
    /// Unit normal away from the room (free walls: the right-hand normal).
    out: P2,
    interior: String,
    outside: String,
    /// Room edges: an explicit exterior finish (otherwise the far side of a partition shows the other room's finish).
    outside_explicit: bool,
}
impl Src {
    fn host_key(&self) -> String {
        match &self.kind { SrcKind::Room { id, edge } => format!("room.{id}.{edge}"), SrcKind::Free { id } => id.clone() }
    }
    fn room(&self) -> Option<&str> { match &self.kind { SrcKind::Room { id, .. } => Some(id), SrcKind::Free { .. } => None } }
}

/// One infinite line on a storey with the sources lying on it.
struct Line { level: usize, p0: P2, d: P2, n: P2, srcs: Vec<(usize, f64, f64, bool)> /* src index, tmin, tmax, reversed */ }

pub struct Model {
    pub settings: Settings,
    pub rooms: Vec<(Custom, Room)>,
    pub walls: Vec<(Custom, Wall)>,
    pub openings: Vec<(Custom, Opening)>,
    pub roofs: Vec<(Custom, Roof)>,
    pub fixtures: Vec<(Custom, Fixture)>,
    pub runs: Vec<(Custom, Run)>,
    pub stairs: Vec<(Custom, Stair)>,
    pub units: Vec<(Custom, Unit)>,
    /// Component types available to units: the document's plus the built-in catalogue.
    pub types: Vec<ComponentType>,
    /// Wall pieces detailed by the masonry plugin (owner, face key, a, b): not drawn here.
    pub detailed_walls: Vec<(String, String, P2, P2)>,
    /// Rooms whose floor slab the masonry plugin builds: their floor is not drawn here.
    pub detailed_floors: Vec<String>,
    /// Masonry floors (storey, outline): the rooms under them get no ceiling slab of ours (the
    /// structural floor is there; it carries its own ceiling plaster when asked).
    pub detailed_floor_outlines: Vec<(usize, Vec<P2>)>,
    /// The masonry floors of the upper storeys derived from the rooms (storeys without an explicit one).
    pub derived_floors: Vec<crate::masonry::derive::DerivedFloor>,
}

/// Every construction entity in the document, decoded.
fn plan_points(v: Option<&serde_json::Value>) -> Vec<P2> {
    v.and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|p| { let q = p.as_array()?; Some([q.first()?.as_f64()?, q.get(1)?.as_f64()?]) }).collect()).unwrap_or_default()
}
/// Structural depth of a masonry floor from its data and the masonry floor defaults (lenient).
fn masonry_floor_depth(d: &serde_json::Value, fd: &serde_json::Value) -> f64 {
    let g = |k: &str, dflt: f64| d.get(k).and_then(|v| v.as_f64()).or_else(|| fd.get(k).and_then(|v| v.as_f64())).unwrap_or(dflt);
    match d.get("system").and_then(|v| v.as_str()).unwrap_or("beamBlock") {
        "hollowCore" => g("plankThickness", 0.2) + g("topping", 0.04),
        "insitu" => g("slabThickness", 0.18),
        _ => g("blockHeight", 0.16) + g("topping", 0.04),
    }
}

pub fn model(doc: &Document) -> Model {
    let settings = crate::ops::settings(doc);
    let mut structures: Vec<TopStructure> = vec![];
    let mut attic: Vec<(usize, Vec<P2>, f64)> = vec![];
    let types = crate::script::available_types(&settings);
    let mut m = Model { settings, rooms: vec![], walls: vec![], openings: vec![], roofs: vec![], fixtures: vec![], runs: vec![], stairs: vec![], units: vec![], types, detailed_walls: vec![], detailed_floors: vec![], detailed_floor_outlines: vec![], derived_floors: vec![] };
    doc.visit(|v| {
        if let Entity::Custom(c) = v.entity {
            let d = c.data.clone();
            match c.type_.as_str() {
                T_ROOM => { if let Ok(r) = serde_json::from_value::<Room>(d) { if r.points.len() >= 3 { m.rooms.push((c.clone(), r)); } } }
                T_WALL => { if let Ok(w) = serde_json::from_value::<Wall>(d) { m.walls.push((c.clone(), w)); } }
                T_OPENING => { if let Ok(o) = serde_json::from_value::<Opening>(d) { m.openings.push((c.clone(), o)); } }
                T_ROOF => { if let Ok(r) = serde_json::from_value::<Roof>(d) { m.roofs.push((c.clone(), r)); } }
                T_FIXTURE => { if let Ok(f) = serde_json::from_value::<Fixture>(d) { m.fixtures.push((c.clone(), f)); } }
                T_RUN => { if let Ok(r) = serde_json::from_value::<Run>(d) { m.runs.push((c.clone(), r)); } }
                T_STAIR => { if let Ok(r) = serde_json::from_value::<Stair>(d) { m.stairs.push((c.clone(), r)); } }
                T_UNIT => { if let Ok(r) = serde_json::from_value::<Unit>(d) { m.units.push((c.clone(), r)); } }
                // the masonry plugin details some of our walls and floors: leave those to it
                "masonry.wall" => {
                    let p2 = |k: &str| d.get(k).and_then(|v| v.as_array()).and_then(|a| Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?]));
                    if let (Some(src), Some(a), Some(b)) = (d.get("source"), p2("a"), p2("b")) {
                        if let (Some(o), Some(k)) = (src.get("owner").and_then(|v| v.as_str()), src.get("key").and_then(|v| v.as_str())) { m.detailed_walls.push((o.to_string(), k.to_string(), a, b)); }
                    }
                }
                "masonry.floor" => {
                    if let Some(rs) = d.get("rooms").and_then(|v| v.as_array()) { m.detailed_floors.extend(rs.iter().filter_map(|r| r.as_str().map(|x| x.to_string()))); }
                    let level = d.get("level").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                    let pts = plan_points(d.get("points"));
                    if pts.len() >= 3 {
                        m.detailed_floor_outlines.push((level, pts.clone()));
                        if level >= 1 {
                            let fd = doc.extensions.get("masonry").and_then(|x| x.get("defaults")).and_then(|x| x.get("floor")).cloned().unwrap_or(serde_json::Value::Null);
                            let depth = masonry_floor_depth(&d, &fd);
                            let plastered = d.get("ceilingPlaster").or_else(|| fd.get("ceilingPlaster")).and_then(|v| v.as_bool()).unwrap_or(false);
                            structures.push(TopStructure { level: level - 1, outline: pts, depth, plastered });
                        }
                    }
                }
                // wooden floor framing in the top of a storey (attic joists): the ceiling goes under it
                "framing.floor" => {
                    let level = d.get("level").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let pts = plan_points(d.get("points"));
                    let elevation = d.get("elevation").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    if pts.len() >= 3 && elevation > 0.5 { attic.push((level, pts, elevation)); }
                }
                _ => {}
            }
        }
    });
    for (level, outline, elevation) in attic {
        let depth = (m.settings.height(level) - elevation).max(0.0);
        if depth > 0.0 && depth < 0.6 { structures.push(TopStructure { level, outline, depth, plastered: false }); }
    }
    // the upper storeys' structural floors are masonry: derived from the rooms unless written explicitly
    let md = crate::masonry::data::settings(doc).defaults;
    let explicit_levels: Vec<usize> = m.detailed_floor_outlines.iter().map(|(l, _)| *l).collect();
    m.derived_floors = crate::masonry::derive::derived_floors(&m, &md, &explicit_levels);
    for f in &m.derived_floors {
        let spec = f.floor.spec(&md.floor);
        m.detailed_floors.extend(f.floor.rooms.iter().cloned());
        structures.push(TopStructure { level: f.floor.level - 1, outline: f.floor.points.clone(), depth: spec.depth(), plastered: f.floor.ceiling_plaster.unwrap_or(md.floor.ceiling_plaster) });
    }
    m.settings.structure = structures;
    m
}

fn sources(m: &Model) -> Vec<Src> {
    let s = &m.settings;
    let fin = |map: &serde_json::Map<String, serde_json::Value>, key: &str, default: &str| -> String {
        finish_of(map, key).map(|f| material_id(&f)).unwrap_or_else(|| material_id(default))
    };
    let mut out = vec![];
    for (c, r) in &m.rooms {
        let n = r.points.len();
        let centre = r.centroid();
        let h = r.wall_height.unwrap_or_else(|| s.height(r.level));
        for e in 0..n {
            let (a, b) = (r.points[e], r.points[(e + 1) % n]);
            let l = dist2(a, b);
            if l < 1e-4 { continue; }
            let d = [b[0] - a[0], b[1] - a[1]];
            let mut nrm = [d[1] / l, -d[0] / l];
            let mid = [(a[0] + b[0]) * 0.5 - centre[0], (a[1] + b[1]) * 0.5 - centre[1]];
            if nrm[0] * mid[0] + nrm[1] * mid[1] < 0.0 { nrm = [-nrm[0], -nrm[1]]; }
            out.push(Src {
                kind: SrcKind::Room { id: c.id.clone(), edge: e }, a, b, level: r.level, height: h, thickness: None, exterior_flag: false, out: nrm,
                interior: fin(&r.finishes, &format!("wall.{e}.interior"), "plaster"),
                outside: fin(&r.finishes, &format!("wall.{e}.exterior"), "brick"),
                outside_explicit: finish_of(&r.finishes, &format!("wall.{e}.exterior")).is_some(),
            });
        }
    }
    for (c, w) in &m.walls {
        let l = dist2(w.a, w.b);
        if l < 1e-4 { continue; }
        let d = [w.b[0] - w.a[0], w.b[1] - w.a[1]];
        let exterior = w.exterior.unwrap_or(false);
        out.push(Src {
            kind: SrcKind::Free { id: c.id.clone() }, a: w.a, b: w.b, level: w.level, height: w.height.unwrap_or_else(|| s.height(w.level)),
            thickness: w.thickness, exterior_flag: exterior, out: [d[1] / l, -d[0] / l],
            interior: fin(&w.finishes, "interior", "plaster"), outside: fin(&w.finishes, "exterior", if exterior { "brick" } else { "plaster" }), outside_explicit: false,
        });
    }
    out
}

/// Group the sources into infinite lines per storey.
fn lines(srcs: &[Src]) -> Vec<Line> {
    let mut out: Vec<Line> = vec![];
    for (i, src) in srcs.iter().enumerate() {
        let l = dist2(src.a, src.b);
        let mut d = [(src.b[0] - src.a[0]) / l, (src.b[1] - src.a[1]) / l];
        // canonical direction so that a→b and b→a land on the same line
        if d[0] < -DIR_EPS || (d[0].abs() <= DIR_EPS && d[1] < 0.0) { d = [-d[0], -d[1]]; }
        let found = out.iter().position(|ln| {
            ln.level == src.level
                && (ln.d[0] * d[1] - ln.d[1] * d[0]).abs() < DIR_EPS && ln.d[0] * d[0] + ln.d[1] * d[1] > 0.0
                && ((src.a[0] - ln.p0[0]) * ln.n[0] + (src.a[1] - ln.p0[1]) * ln.n[1]).abs() < LINE_EPS
                && ((src.b[0] - ln.p0[0]) * ln.n[0] + (src.b[1] - ln.p0[1]) * ln.n[1]).abs() < LINE_EPS
        });
        let li = match found {
            Some(li) => li,
            None => { out.push(Line { level: src.level, p0: src.a, d, n: [d[1], -d[0]], srcs: vec![] }); out.len() - 1 }
        };
        let ln = &out[li];
        let t = |p: P2| (p[0] - ln.p0[0]) * ln.d[0] + (p[1] - ln.p0[1]) * ln.d[1];
        let (ta, tb) = (t(src.a), t(src.b));
        let reversed = tb < ta;
        out[li].srcs.push((i, ta.min(tb), ta.max(tb), reversed));
    }
    out
}

pub fn collect(m: &Model) -> WallSet {
    let srcs = sources(m);
    let lines = lines(&srcs);
    let mut set = WallSet::default();
    // host key → (line index, tmin, tmax, reversed) for the openings
    let mut hosts: HashMap<String, (usize, f64, f64, bool)> = HashMap::new();
    // line index → wall indices in piece order
    let mut line_pieces: Vec<Vec<usize>> = vec![vec![]; lines.len()];

    for (li, ln) in lines.iter().enumerate() {
        for &(si, tmin, tmax, rev) in &ln.srcs { hosts.insert(srcs[si].host_key(), (li, tmin, tmax, rev)); }
        let mut cuts: Vec<f64> = ln.srcs.iter().flat_map(|&(_, t0, t1, _)| [t0, t1]).collect();
        cuts.sort_by(|a, b| a.partial_cmp(b).unwrap());
        cuts.dedup_by(|a, b| (*a - *b).abs() < LINE_EPS);
        let at = |t: f64| -> P2 { [ln.p0[0] + ln.d[0] * t, ln.p0[1] + ln.d[1] * t] };
        for w in cuts.windows(2) {
            let (t0, t1) = (w[0], w[1]);
            if t1 - t0 < LINE_EPS { continue; }
            let cover: Vec<&(usize, f64, f64, bool)> = ln.srcs.iter().filter(|(_, a, b, _)| *a <= t0 + LINE_EPS / 2.0 && *b >= t1 - LINE_EPS / 2.0).collect();
            if cover.is_empty() { continue; }
            let room_srcs: Vec<&Src> = cover.iter().map(|c| &srcs[c.0]).filter(|x| x.room().is_some()).collect();
            let free_srcs: Vec<&Src> = cover.iter().map(|c| &srcs[c.0]).filter(|x| x.room().is_none()).collect();
            // which side of the line each room lies on: its outward normal points the other way
            let plus: Vec<&Src> = room_srcs.iter().copied().filter(|x| x.out[0] * ln.n[0] + x.out[1] * ln.n[1] < 0.0).collect();
            let minus: Vec<&Src> = room_srcs.iter().copied().filter(|x| x.out[0] * ln.n[0] + x.out[1] * ln.n[1] >= 0.0).collect();
            let free_thickness = free_srcs.iter().filter_map(|x| x.thickness).fold(None, |acc: Option<f64>, t| Some(acc.map_or(t, |a| a.max(t))));
            let (a, b) = (at(t0), at(t1));
            let entry = if let Some(owner) = room_srcs.first() {
                let other_side: &[&Src] = if plus.iter().any(|x| std::ptr::eq(*x, *owner)) { &minus } else { &plus };
                let height = room_srcs.iter().map(|x| x.height).fold(0.0, f64::max);
                let rooms: Vec<String> = { let mut v: Vec<String> = vec![]; for x in &room_srcs { let id = x.room().unwrap().to_string(); if !v.contains(&id) { v.push(id); } } v };
                let key = match &owner.kind { SrcKind::Room { edge, .. } => format!("wall.{edge}"), _ => unreachable!() };
                if let Some(other) = other_side.first() {
                    // partition between two rooms: single leaf, each side shows its own room's finish
                    let outside = if owner.outside_explicit { owner.outside.clone() } else { other.interior.clone() };
                    WallEntry { owner: owner.room().unwrap().to_string(), key, a, b, level: ln.level, height, exterior: false, out: owner.out, thickness: free_thickness,
                                interior: owner.interior.clone(), outside, openings: vec![], rooms, cap_start: true, cap_end: true }
                } else {
                    WallEntry { owner: owner.room().unwrap().to_string(), key, a, b, level: ln.level, height, exterior: true, out: owner.out, thickness: free_thickness,
                                interior: owner.interior.clone(), outside: owner.outside.clone(), openings: vec![], rooms, cap_start: true, cap_end: true }
                }
            } else {
                let f = free_srcs[0];
                let height = free_srcs.iter().map(|x| x.height).fold(0.0, f64::max);
                // keep the free wall's own direction for its outward normal
                let fwd = (f.b[0] - f.a[0]) * ln.d[0] + (f.b[1] - f.a[1]) * ln.d[1] >= 0.0;
                let out = if fwd { f.out } else { [-f.out[0], -f.out[1]] };
                WallEntry { owner: f.host_key(), key: "wall".into(), a, b, level: ln.level, height, exterior: f.exterior_flag, out, thickness: free_thickness,
                            interior: f.interior.clone(), outside: f.outside.clone(), openings: vec![], rooms: vec![], cap_start: true, cap_end: true }
            };
            set.walls.push(entry);
            line_pieces[li].push(set.walls.len() - 1);
        }
        // neighbouring pieces with the same section join without end caps
        let idx = &line_pieces[li];
        for k in 1..idx.len() {
            let (p, q) = (idx[k - 1], idx[k]);
            let same = {
                let (x, y) = (&set.walls[p], &set.walls[q]);
                let (lx, ly) = (leaves(&m.settings, x).0, leaves(&m.settings, y).0);
                let same_leaves = lx.len() == ly.len() && lx.iter().zip(&ly).all(|(a, b)| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9);
                dist2(x.b, y.a) < LINE_EPS && same_leaves && (x.height - y.height).abs() < 1e-6
                    && (x.out[0] - y.out[0]).abs() < 1e-6 && (x.out[1] - y.out[1]).abs() < 1e-6
            };
            if same { set.walls[p].cap_end = false; set.walls[q].cap_start = false; }
        }
    }

    // openings: cut into every piece they overlap on their host's line
    for (c, o) in &m.openings {
        let Some(&(li, tmin, tmax, reversed)) = hosts.get(&o.host.key()) else { continue };
        let l = (tmax - tmin).max(1e-9);
        let along = if reversed { l - o.along } else { o.along };
        let width = o.width.min(l).max(0.05);
        let half = width / 2.0;
        let along = along.clamp(half.min(l / 2.0), (l - half).max(l / 2.0));
        let (c0, c1) = (tmin + along - half, tmin + along + half);
        for &wi in &line_pieces[li] {
            let w = &set.walls[wi];
            let ln = &lines[li];
            let (p0, p1) = ((w.a[0] - ln.p0[0]) * ln.d[0] + (w.a[1] - ln.p0[1]) * ln.d[1], (w.b[0] - ln.p0[0]) * ln.d[0] + (w.b[1] - ln.p0[1]) * ln.d[1]);
            let (s0, s1) = (c0.max(p0), c1.min(p1));
            if s1 - s0 < 1e-3 { continue; }
            let sill = o.sill.max(0.0);
            let height = o.height.min(w.height - sill).max(0.05);
            let frame_start = (s0 - c0).abs() < 1e-6;
            let frame_end = (s1 - c1).abs() < 1e-6;
            set.walls[wi].openings.push(ResolvedOpening { id: c.id.clone(), kind: o.kind, along: (s0 + s1) / 2.0 - p0, width: s1 - s0, height, sill, frame_start, frame_end });
        }
    }
    for w in &mut set.walls {
        w.openings.sort_by(|x, y| x.along.partial_cmp(&y.along).unwrap_or(std::cmp::Ordering::Equal));
    }
    set
}

/// Leaves across the wall (offsets from the centre line, positive = out) and the total thickness.
fn leaves(s: &Settings, w: &WallEntry) -> (Vec<(f64, f64)>, f64) {
    let cav = &s.cavity;
    if cav.enabled && w.exterior && w.thickness.is_none() {
        let t = cav.total();
        (vec![(-t / 2.0, -t / 2.0 + cav.inner_leaf), (t / 2.0 - cav.outer_leaf, t / 2.0)], t)
    } else {
        let t = w.thickness.unwrap_or(s.wall_thickness).max(0.01);
        (vec![(-t / 2.0, t / 2.0)], t)
    }
}

/// Offsets (along the wall's `out` normal) of its room-side and far-side faces.
pub fn leaf_span(s: &Settings, w: &WallEntry) -> (f64, f64) { let (lv, _) = leaves(s, w); (lv[0].0, lv[lv.len() - 1].1) }
/// Offset of the middle of the room-side leaf: where a conduit runs inside the wall.
pub fn route_offset(s: &Settings, w: &WallEntry) -> f64 { let (lv, _) = leaves(s, w); (lv[0].0 + lv[0].1) / 2.0 }

struct Frame { a: P2, d: P2, n: P2, y0: f64 }
impl Frame {
    fn at(&self, along: f64, off: f64, y: f64) -> P3 {
        [self.a[0] + self.d[0] * along + self.n[0] * off, self.y0 + y, self.a[1] + self.d[1] * along + self.n[1] * off]
    }
}

/// A rectangular wall piece: `s0..s1` along the wall, `o0..o1` across, `h0..h1` high.
/// The +n face gets `out_id`/`out_mat`, every other face `in_id`/`in_mat`. `caps` = (back, front).
#[allow(clippy::too_many_arguments)]
fn piece(b: &mut Faces, f: &Frame, out_id: &str, in_id: &str, s0: f64, s1: f64, o0: f64, o1: f64, h0: f64, h1: f64, out_mat: &str, in_mat: &str, caps: (bool, bool)) {
    if s1 - s0 < 1e-4 || h1 - h0 < 1e-4 || o1 - o0 < 1e-6 { return; }
    let p = |s: f64, o: f64, h: f64| f.at(s, o, h);
    let out = [f.n[0], 0.0, f.n[1]];
    let inn = [-f.n[0], 0.0, -f.n[1]];
    let fwd = [f.d[0], 0.0, f.d[1]];
    let back = [-f.d[0], 0.0, -f.d[1]];
    b.quad(out_id, p(s0, o1, h0), p(s1, o1, h0), p(s1, o1, h1), p(s0, o1, h1), out, out_mat);
    b.quad(in_id, p(s0, o0, h0), p(s1, o0, h0), p(s1, o0, h1), p(s0, o0, h1), inn, in_mat);
    if caps.0 { b.quad(in_id, p(s0, o0, h0), p(s0, o1, h0), p(s0, o1, h1), p(s0, o0, h1), back, in_mat); }
    if caps.1 { b.quad(in_id, p(s1, o0, h0), p(s1, o1, h0), p(s1, o1, h1), p(s1, o0, h1), fwd, in_mat); }
    b.quad(in_id, p(s0, o0, h1), p(s1, o0, h1), p(s1, o1, h1), p(s0, o1, h1), [0.0, 1.0, 0.0], in_mat);
    b.quad(in_id, p(s0, o0, h0), p(s1, o0, h0), p(s1, o1, h0), p(s0, o1, h0), [0.0, -1.0, 0.0], in_mat);
}

fn frame_for(s: &Settings, w: &WallEntry) -> Frame {
    let l = w.length().max(1e-9);
    Frame { a: w.a, d: [(w.b[0] - w.a[0]) / l, (w.b[1] - w.a[1]) / l], n: w.out, y0: s.base(w.level) }
}

/// The wall pieces owned by `owner` (a room or a free wall), with their openings cut out.
/// Is this wall piece detailed by a masonry wall (same owner and face key, midpoint on its line)?
pub fn is_detailed(w: &WallEntry, detailed: &[(String, String, P2, P2)]) -> bool {
    let mid = [(w.a[0] + w.b[0]) / 2.0, (w.a[1] + w.b[1]) / 2.0];
    detailed.iter().any(|(o, k, a, b)| {
        if o != &w.owner || k != &w.key { return false; }
        let l = dist2(*a, *b); if l < 1e-6 { return false; }
        let d = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
        let t = (mid[0] - a[0]) * d[0] + (mid[1] - a[1]) * d[1];
        let off = ((mid[0] - a[0]) * d[1] - (mid[1] - a[1]) * d[0]).abs();
        t > -0.02 && t < l + 0.02 && off < 0.02
    })
}

pub fn build_walls(b: &mut Faces, s: &Settings, set: &WallSet, owner: &str, detailed: &[(String, String, P2, P2)]) {
    for w in set.walls.iter().filter(|w| w.owner == owner && !is_detailed(w, detailed)) {
        let f = frame_for(s, w);
        let l = w.length();
        let (lv, _) = leaves(s, w);
        let ext_id = format!("{}#{}.exterior", owner, w.key);
        let int_id = format!("{}#{}.interior", owner, w.key);
        // end caps only where the piece really ends (a neighbouring piece with the same section continues it)
        let caps = |s0: f64, s1: f64| -> (bool, bool) { (!(s0 < 1e-6 && !w.cap_start), !((l - s1).abs() < 1e-6 && !w.cap_end)) };
        for (li, (o0, o1)) in lv.iter().enumerate() {
            let outer_leaf = li == lv.len() - 1;
            let (om, im) = if outer_leaf { (w.outside.as_str(), w.interior.as_str()) } else { (w.interior.as_str(), w.interior.as_str()) };
            let (oid, iid) = if outer_leaf { (ext_id.as_str(), int_id.as_str()) } else { (int_id.as_str(), int_id.as_str()) };
            let mut cursor = 0.0;
            for o in &w.openings {
                let (s0, s1) = ((o.along - o.width / 2.0).max(0.0), (o.along + o.width / 2.0).min(l));
                piece(b, &f, oid, iid, cursor, s0, *o0, *o1, 0.0, w.height, om, im, caps(cursor, s0));
                piece(b, &f, oid, iid, s0, s1, *o0, *o1, o.sill + o.height, w.height, om, im, caps(s0, s1));
                if o.sill > 1e-3 { piece(b, &f, oid, iid, s0, s1, *o0, *o1, 0.0, o.sill, om, im, caps(s0, s1)); }
                cursor = s1.max(cursor);
            }
            piece(b, &f, oid, iid, cursor, l, *o0, *o1, 0.0, w.height, om, im, caps(cursor, l));
        }
    }
}

/// Door leaf / window glass / frames for one opening (over every wall piece it spans).
pub fn build_opening(b: &mut Faces, s: &Settings, set: &WallSet, opening_id: &str) {
    for w in &set.walls {
        let Some(o) = w.openings.iter().find(|o| o.id == opening_id) else { continue };
        let f = frame_for(s, w);
        let (_, thickness) = leaves(s, w);
        let half = thickness / 2.0;
        let l = w.length();
        let (s0, s1) = ((o.along - o.width / 2.0).max(0.0), (o.along + o.width / 2.0).min(l));
        let (h0, h1) = (o.sill, o.sill + o.height);
        let fw = 0.05;
        let door = material_id("door");
        let glass = material_id("glass");
        let frame = material_id("frame");
        let fid = format!("{opening_id}#frame");
        let lid = format!("{opening_id}#leaf");
        let gid = format!("{opening_id}#glass");
        // frame posts only on the opening's real edges; the inner extent runs between them
        let (i0, i1) = (if o.frame_start { s0 + fw } else { s0 }, if o.frame_end { s1 - fw } else { s1 });
        let all = (true, true);
        let span = (o.frame_start, o.frame_end); // no caps where the opening continues into the next piece
        match o.kind {
            OpeningKind::Passage => {}
            OpeningKind::Door | OpeningKind::Garage => {
                if o.frame_start { piece(b, &f, &fid, &fid, s0, s0 + fw, -half, half, h0, h1, &frame, &frame, all); }
                if o.frame_end { piece(b, &f, &fid, &fid, s1 - fw, s1, -half, half, h0, h1, &frame, &frame, all); }
                piece(b, &f, &fid, &fid, i0, i1, -half, half, h1 - fw, h1, &frame, &frame, span);
                piece(b, &f, &lid, &lid, i0, i1, -half + 0.01, -half + 0.05, h0, h1 - fw, &door, &door, span);
            }
            OpeningKind::Window => {
                if o.frame_start { piece(b, &f, &fid, &fid, s0, s0 + fw, -half, half, h0, h1, &frame, &frame, all); }
                if o.frame_end { piece(b, &f, &fid, &fid, s1 - fw, s1, -half, half, h0, h1, &frame, &frame, all); }
                piece(b, &f, &fid, &fid, i0, i1, -half, half, h1 - fw, h1, &frame, &frame, span);
                piece(b, &f, &fid, &fid, i0, i1, -half, half, h0, h0 + fw, &frame, &frame, span);
                piece(b, &f, &gid, &gid, i0, i1, -0.006, 0.006, h0 + fw, h1 - fw, &glass, &glass, span);
            }
        }
    }
}

/// Rectangular stair wells to cut out of a room's floor: the footprint of every stair on the
/// storey below that opens the floor above, clipped to the room and inset by a centimetre.
pub fn stair_wells(m: &Model, r: &Room) -> Vec<Vec<P2>> {
    let mut out = vec![];
    if r.level == 0 || !r.has_floor { return out; }
    let Some((rlo, rhi)) = bbox(&r.points) else { return out };
    for (_, st) in m.stairs.iter().filter(|(_, st)| st.level + 1 == r.level && st.opens_floor_above()) {
        let steps = stair_steps(st, m.settings.height(st.level));
        let pts: Vec<P2> = steps.iter().flat_map(|x| x.corners()).collect();
        let Some((lo, hi)) = bbox(&pts) else { continue };
        let inset = 0.01;
        let (a, c) = ([lo[0].max(rlo[0]) + inset, lo[1].max(rlo[1]) + inset], [hi[0].min(rhi[0]) - inset, hi[1].min(rhi[1]) - inset]);
        if c[0] - a[0] < 0.05 || c[1] - a[1] < 0.05 { continue; }
        let rect = vec![a, [c[0], a[1]], c, [a[0], c[1]]];
        if rect.iter().all(|p| point_in_plan(*p, &r.points)) { out.push(rect); }
    }
    out
}

pub fn build_floor(b: &mut Faces, s: &Settings, id: &str, r: &Room) { build_floor_in(b, None, s, id, r) }

pub fn build_floor_in(b: &mut Faces, m: Option<&Model>, s: &Settings, id: &str, r: &Room) {
    let y0 = s.base(r.level);
    let ft = s.floor_thickness.max(0.01);
    if r.has_floor {
        let mat = finish_of(&r.finishes, "floor").map(|f| material_id(&f)).unwrap_or_else(|| material_id("wood"));
        let holes = m.map(|m| stair_wells(m, r)).unwrap_or_default();
        b.slab_holes(&format!("{id}#floor"), &r.points, &holes, y0 - ft, y0, &mat);
    }
    if r.has_ceiling {
        // the ceiling hangs under whatever structure fills the top of the storey (floor above, attic joists)
        let h = r.wall_height.unwrap_or_else(|| s.clear_height(r.level, r.centroid()));
        let m = finish_of(&r.finishes, "ceiling").map(|f| material_id(&f)).unwrap_or_else(|| material_id("plaster"));
        b.slab(&format!("{id}#ceiling"), &r.points, y0 + h - 0.02, y0 + h, &m);
    }
}

/// Plan bounding box of the rooms on `level` (falls back to every room).
pub fn footprint(m: &Model, level: usize) -> Option<(P2, P2)> {
    let mut pts: Vec<P2> = m.rooms.iter().filter(|(_, r)| r.level == level).flat_map(|(_, r)| r.points.clone()).collect();
    pts.extend(m.walls.iter().filter(|(_, w)| w.level == level).flat_map(|(_, w)| [w.a, w.b]));
    if pts.is_empty() {
        pts = m.rooms.iter().flat_map(|(_, r)| r.points.clone()).collect();
        pts.extend(m.walls.iter().flat_map(|(_, w)| [w.a, w.b]));
    }
    bbox(&pts)
}

pub fn build_fixture(b: &mut Faces, s: &Settings, set: &WallSet, id: &str, f: &Fixture) {
    if crate::fixtures::build_detailed(b, s, set, id, f) { return; }
    let [w, h, d] = f.kind.marker();
    let y0 = s.base(f.level);
    let [x, y, z] = f.position;
    let (hw, hd) = (w / 2.0, d / 2.0);
    let ang = f.rotation.unwrap_or(0.0);
    let (sn, cs) = (ang.sin(), ang.cos());
    let rot = |dx: f64, dz: f64| -> P2 { [x + dx * cs - dz * sn, z + dx * sn + dz * cs] };
    let corners = [rot(-hw, -hd), rot(hw, -hd), rot(hw, hd), rot(-hw, hd)];
    let base: Vec<P3> = corners.iter().map(|p| [p[0], y0 + y - h / 2.0, p[1]]).collect();
    let m = material_id(f.kind.system().key());
    b.prism(&format!("{id}#marker"), &base, [0.0, h, 0.0], &m, &m);
}

pub fn build_run(b: &mut Faces, s: &Settings, id: &str, r: &Run) {
    let y0 = s.base(r.level);
    let m = material_id(r.system.key());
    let rad = r.diameter.map(|d| d / 2.0).unwrap_or(0.015).max(0.005);
    let pts: Vec<P3> = r.points.iter().map(|p| [p[0], p[1] + y0, p[2]]).collect();
    let fid = format!("{id}#run");
    for seg in pts.windows(2) {
        let (a, c) = (seg[0], seg[1]);
        let d = sub(c, a);
        if plannr_core::geom::len(d) < 1e-6 { continue; }
        let dn = normalize(d);
        let helper = if dn[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
        let u = normalize(cross(helper, dn));
        let v = cross(dn, u);
        let base = vec![
            add(a, add(scale(u, -rad), scale(v, -rad))), add(a, add(scale(u, rad), scale(v, -rad))),
            add(a, add(scale(u, rad), scale(v, rad))), add(a, add(scale(u, -rad), scale(v, rad))),
        ];
        b.prism(&fid, &base, d, &m, &m);
    }
}

/// Expand one custom entity. `model`/`set` are computed by the caller for the whole document.
/// Masonry walls (and derived storey floors) drawn by the construction entity `owner`.
fn masonry_for(owner: &str, m: &Model, mh: &crate::masonry::data::House, layer: &str) -> Vec<Entity> {
    let mut out = vec![];
    for (id, w) in mh.walls.iter().filter(|(id, w)| crate::masonry::data::House::is_derived(id) && w.source.as_ref().map(|s| s.owner.as_str()) == Some(owner)) {
        out.extend(crate::masonry::wall::expand_wall(id, w, mh, layer).faces);
    }
    for f in m.derived_floors.iter().filter(|f| f.carrier == owner) {
        out.extend(crate::masonry::floor::expand_floor(&f.id, &f.floor, mh, layer).faces);
    }
    out
}

pub fn expand_entity(c: &Custom, m: &Model, set: &WallSet) -> Vec<Entity> { expand_entity_with(c, m, set, None) }

/// Expand one construction entity; its walls and floors are masonry (`mh` = the masonry of the document).
pub fn expand_entity_with(c: &Custom, m: &Model, set: &WallSet, mh: Option<&crate::masonry::data::House>) -> Vec<Entity> {
    let mut b = Faces::new(&c.layer);
    if matches!(c.type_.as_str(), T_ROOM | T_WALL) {
        if let Some(mh) = mh { b.out.extend(masonry_for(&c.id, m, mh, &c.layer)); }
    }
    match c.type_.as_str() {
        T_ROOM => {
            if let Some((_, r)) = m.rooms.iter().find(|(x, _)| x.id == c.id) {
                let c0 = r.centroid();
                let under_masonry = m.settings.structure.iter().any(|t| t.level == r.level && t.plastered && point_in_plan(c0, &t.outline));
                if m.detailed_floors.contains(&c.id) || under_masonry {
                    let mut r2 = r.clone();
                    if m.detailed_floors.contains(&c.id) { r2.has_floor = false; }
                    if under_masonry { r2.has_ceiling = false; }
                    build_floor_in(&mut b, Some(m), &m.settings, &c.id, &r2);
                } else { build_floor_in(&mut b, Some(m), &m.settings, &c.id, r); }
                // walls are masonry (above); without the masonry house (plain expand) the old leaves
                if mh.is_none() { build_walls(&mut b, &m.settings, set, &c.id, &m.detailed_walls); }
            }
        }
        T_WALL => { if mh.is_none() { build_walls(&mut b, &m.settings, set, &c.id, &m.detailed_walls); } }
        T_OPENING => build_opening(&mut b, &m.settings, set, &c.id),
        T_ROOF => {
            if let Some((_, r)) = m.roofs.iter().find(|(x, _)| x.id == c.id) {
                if let Some(g) = crate::roof::geom(m, set, &c.id, r) { crate::roof::build(&mut b, &g, r, &c.id); }
            }
        }
        T_FIXTURE => { if let Some((_, f)) = m.fixtures.iter().find(|(x, _)| x.id == c.id) { build_fixture(&mut b, &m.settings, set, &c.id, f); } }
        T_RUN => { if let Some((_, r)) = m.runs.iter().find(|(x, _)| x.id == c.id) { build_run(&mut b, &m.settings, &c.id, r); } }
        T_STAIR => { if let Some((_, st)) = m.stairs.iter().find(|(x, _)| x.id == c.id) { build_stair(&mut b, &m.settings, &c.id, st); } }
        T_UNIT => {
            if let Some((_, u)) = m.units.iter().find(|(x, _)| x.id == c.id) {
                let solids = crate::script::with_runtime(&m.types, |rt| crate::script::unit_solids(rt, u).map(|s| (s, rt.worktop(&u.type_, &crate::script::given(u)))));
                match solids {
                    Some((solids, worktop)) => build_scripted_unit(&mut b, &m.settings, &c.id, u, &solids, worktop),
                    None => build_unit(&mut b, &m.settings, &c.id, u),
                }
            }
        }
        _ => {}
    }
    b.out
}


// ---- stairs ------------------------------------------------------------------------------

/// One walking surface of a stair: a tread, a winder or the quarter landing. Ported from the
/// Uzzetje Studio `Stair.steps(in:)` layout.
#[derive(Debug, Clone)]
pub struct StairStep {
    /// 1-based riser this surface sits on top of.
    pub index: usize,
    pub centre: P2,
    pub direction: P2,
    pub width: f64,
    pub going: f64,
    /// Walking height above the storey floor.
    pub top: f64,
    pub is_landing: bool,
    pub is_winder: bool,
    pub outline: Option<Vec<P2>>,
    pub leading: Option<[P2; 2]>,
    pub trailing: Option<[P2; 2]>,
}
fn p2add(a: P2, b: P2) -> P2 { [a[0] + b[0], a[1] + b[1]] }
fn p2sub(a: P2, b: P2) -> P2 { [a[0] - b[0], a[1] - b[1]] }
fn p2mul(a: P2, s: f64) -> P2 { [a[0] * s, a[1] * s] }
fn p2dot(a: P2, b: P2) -> f64 { a[0] * b[0] + a[1] * b[1] }
fn p2len(a: P2) -> f64 { p2dot(a, a).sqrt() }
fn p2norm(a: P2) -> P2 { let l = p2len(a).max(1e-12); [a[0] / l, a[1] / l] }
impl StairStep {
    /// Plan corners: back-left, back-right, front-right, front-left (looking up the stair).
    pub fn corners(&self) -> Vec<P2> {
        if let Some(o) = &self.outline { return o.clone(); }
        let along = p2mul(self.direction, self.going / 2.0);
        let across = p2mul([-self.direction[1], self.direction[0]], self.width / 2.0);
        vec![p2sub(p2sub(self.centre, along), across), p2add(p2sub(self.centre, along), across), p2add(p2add(self.centre, along), across), p2sub(p2add(self.centre, along), across)]
    }
    pub fn leading_edge(&self) -> [P2; 2] {
        if let Some(l) = self.leading { return l; }
        let along = p2mul(self.direction, self.going / 2.0);
        let across = p2mul([-self.direction[1], self.direction[0]], self.width / 2.0);
        [p2add(p2add(self.centre, along), across), p2sub(p2add(self.centre, along), across)]
    }
    pub fn trailing_edge(&self) -> [P2; 2] {
        if let Some(t) = self.trailing { return t; }
        let along = p2mul(self.direction, self.going / 2.0);
        let across = p2mul([-self.direction[1], self.direction[0]], self.width / 2.0);
        [p2sub(p2sub(self.centre, along), across), p2add(p2sub(self.centre, along), across)]
    }
}

/// The tapered treads that make a quarter turn inside a square the width of the stair.
fn winders(count: usize, first: usize, entry: P2, direction: P2, turned: P2, width: f64, riser: f64) -> Vec<StairStep> {
    if count == 0 || width <= 0.2 { return vec![]; }
    let outward = turned;
    let pivot = p2add(entry, p2mul(outward, width / 2.0));
    let far = p2add(entry, p2mul(direction, width));
    let square = [p2sub(entry, p2mul(outward, width / 2.0)), pivot, p2add(far, p2mul(outward, width / 2.0)), p2sub(far, p2mul(outward, width / 2.0))];
    let edge_point = |angle: f64| -> P2 {
        let ray = p2add(p2mul(outward, -angle.cos()), p2mul(direction, angle.sin()));
        let mut best = f64::MAX;
        for i in 0..4 {
            let (a, b) = (square[i], square[(i + 1) % 4]);
            let edge = p2sub(b, a);
            let den = ray[0] * edge[1] - ray[1] * edge[0];
            if den.abs() < 1e-9 { continue; }
            let off = p2sub(a, pivot);
            let t = (off[0] * edge[1] - off[1] * edge[0]) / den;
            let u = (off[0] * ray[1] - off[1] * ray[0]) / den;
            if t > 1e-4 && u >= -1e-4 && u <= 1.0 + 1e-4 { best = best.min(t); }
        }
        if best < f64::MAX { p2add(pivot, p2mul(ray, best)) } else { pivot }
    };
    let step = std::f64::consts::FRAC_PI_2 / count as f64;
    let mut out = vec![];
    for k in 0..count {
        let (from, to) = (k as f64 * step, (k + 1) as f64 * step);
        let (a, b) = (edge_point(from), edge_point(to));
        let between: Vec<P2> = square.iter().copied().filter(|corner| {
            let off = p2sub(*corner, pivot);
            if p2len(off) < 1e-4 { return false; }
            let ang = p2dot(off, direction).atan2(p2dot(off, p2mul(outward, -1.0)));
            ang > from + 1e-4 && ang < to - 1e-4
        }).collect();
        let mut ring = vec![pivot, a];
        ring.extend(between);
        ring.push(b);
        let centroid = p2mul(ring.iter().fold([0.0, 0.0], |acc, p| p2add(acc, *p)), 1.0 / ring.len() as f64);
        let middle = (from + to) / 2.0;
        let walk = p2norm(p2add(p2mul(outward, middle.sin()), p2mul(direction, middle.cos())));
        let index = first + k;
        out.push(StairStep { index, centre: centroid, direction: walk, width, going: width / count as f64, top: index as f64 * riser, is_landing: false, is_winder: true, outline: Some(ring), leading: Some([b, pivot]), trailing: Some([pivot, a]) });
    }
    out
}

/// Every walking surface of a stair in climbing order; the top riser lands on the floor above.
pub fn stair_steps(st: &Stair, rise: f64) -> Vec<StairStep> {
    let risers = st.risers.max(1);
    let riser = rise / risers as f64;
    if risers < 2 || st.going <= 0.05 || st.width <= 0.2 { return vec![]; }
    let mut steps: Vec<StairStep> = vec![];
    let mut direction = st.direction();
    let mut cursor = p2sub(st.origin, p2mul(direction, st.going / 2.0));
    let going = st.going;
    let width = st.width;
    let advance = |steps: &mut Vec<StairStep>, cursor: &mut P2, direction: P2, treads: usize, first: usize| {
        for i in 0..treads {
            let index = first + i;
            *cursor = p2add(*cursor, p2mul(direction, going / 2.0));
            steps.push(StairStep { index, centre: *cursor, direction, width, going, top: index as f64 * riser, is_landing: false, is_winder: false, outline: None, leading: None, trailing: None });
            *cursor = p2add(*cursor, p2mul(direction, going / 2.0));
        }
    };
    match st.kind {
        StairKind::Straight => advance(&mut steps, &mut cursor, direction, risers - 1, 1),
        StairKind::QuarterTurn => {
            let turning = st.winder_count().min(risers.saturating_sub(3));
            let before = st.turn_after.min(risers.saturating_sub(2 + turning)).max(1);
            advance(&mut steps, &mut cursor, direction, before, 1);
            let turned = if st.turns_right { [-direction[1], direction[0]] } else { [direction[1], -direction[0]] };
            if turning > 0 {
                steps.extend(winders(turning, before + 1, cursor, direction, turned, width, riser));
                cursor = p2add(p2add(cursor, p2mul(turned, width / 2.0)), p2mul(direction, width / 2.0));
                direction = turned;
                advance(&mut steps, &mut cursor, direction, risers.saturating_sub(before + turning + 1), before + turning + 1);
            } else {
                let landing_index = before + 1;
                cursor = p2add(cursor, p2mul(direction, width / 2.0));
                steps.push(StairStep { index: landing_index, centre: cursor, direction, width, going: width, top: landing_index as f64 * riser, is_landing: true, is_winder: false, outline: None, leading: None, trailing: None });
                direction = turned;
                cursor = p2add(cursor, p2mul(direction, width / 2.0));
                advance(&mut steps, &mut cursor, direction, risers.saturating_sub(landing_index + 1), landing_index + 1);
            }
        }
    }
    steps
}

/// Plan bounding box of a whole stair.
pub fn stair_bounds(st: &Stair, rise: f64) -> Option<(P2, P2)> {
    let pts: Vec<P2> = stair_steps(st, rise).iter().flat_map(|s| s.corners()).collect();
    bbox(&pts)
}

pub fn build_stair(b: &mut Faces, s: &Settings, id: &str, st: &Stair) {
    let rise = s.height(st.level);
    let y0 = s.base(st.level);
    let steps = stair_steps(st, rise);
    if steps.is_empty() { return; }
    let riser_h = rise / st.risers.max(1) as f64;
    let tt = st.tread_depth().min(riser_h * 0.8);
    let nose = st.nosing_depth();
    let wood = material_id("wood");
    let white = material_id("paintWhite");
    let at = |p: P2, y: f64| -> P3 { [p[0], y0 + y, p[1]] };
    for step in &steps {
        // tread: the outline, pushed forward by the nosing at the leading edge
        let mut ring = step.corners();
        if !step.is_winder && !step.is_landing && nose > 0.0 {
            let d = p2mul(step.direction, nose);
            ring[2] = p2add(ring[2], d);
            ring[3] = p2add(ring[3], d);
        }
        let key = if step.is_landing { "landing".to_string() } else { format!("tread.{}", step.index) };
        let base: Vec<P3> = ring.iter().map(|p| at(*p, step.top - tt)).collect();
        b.prism(&format!("{id}#{key}"), &base, [0.0, tt, 0.0], &wood, &wood);
        // riser: a thin board under the trailing edge, from the step below up to this tread
        if st.has_risers() {
            let [ta, tb] = step.trailing_edge();
            let fwd = p2mul(step.direction, 0.02);
            let ring = vec![ta, tb, p2add(tb, fwd), p2add(ta, fwd)];
            let base: Vec<P3> = ring.iter().map(|p| at(*p, step.top - riser_h)).collect();
            b.prism(&format!("{id}#riser.{}", step.index), &base, [0.0, riser_h - tt, 0.0], &white, &white);
        }
    }
    // the top riser lands on the floor above
    if st.has_risers() {
        if let Some(last) = steps.last() {
            let [la, lb] = last.leading_edge();
            let fwd = p2mul(last.direction, 0.02);
            let ring = vec![la, lb, p2add(lb, fwd), p2add(la, fwd)];
            let base: Vec<P3> = ring.iter().map(|p| at(*p, last.top)).collect();
            b.prism(&format!("{id}#riser.{}", st.risers), &base, [0.0, (rise - last.top).max(0.0), 0.0], &white, &white);
        }
    }
    // stringers: one sloped board each side of every straight run
    let th = st.stringer_thickness();
    if th > 0.0 {
        let mut i = 0;
        while i < steps.len() {
            if steps[i].is_winder || steps[i].is_landing { i += 1; continue; }
            let mut j = i;
            while j + 1 < steps.len() && !steps[j + 1].is_winder && !steps[j + 1].is_landing && p2dot(steps[j + 1].direction, steps[i].direction) > 0.999 { j += 1; }
            let (first, last) = (&steps[i], &steps[j]);
            let dir = first.direction;
            let right = [-dir[1], dir[0]];
            let [ta, _] = first.trailing_edge();
            let [la, _] = last.leading_edge();
            let start = p2dot(p2sub(ta, first.centre), dir);
            let end = p2dot(p2sub(la, first.centre), dir);
            let h0 = first.top - riser_h;
            let h1 = last.top + (if j + 1 < steps.len() { riser_h } else { rise - last.top });
            let depth = 0.25;
            for (side, sign) in [("left", -1.0), ("right", 1.0)] {
                let off = p2mul(right, sign * (st.width / 2.0));
                let c = p2add(first.centre, off);
                let p = |along: f64, h: f64| at(p2add(c, p2mul(dir, along)), h);
                let profile = vec![p(start, h0 - depth + riser_h), p(start, h0 + riser_h), p(end, h1), p(end, h1 - depth)];
                let out = [right[0] * sign * th, 0.0, right[1] * sign * th];
                b.prism(&format!("{id}#stringer.{side}"), &profile, out, &wood, &wood);
            }
            i = j + 1;
        }
    }
}

// ---- units -------------------------------------------------------------------------------

pub fn build_unit(b: &mut Faces, s: &Settings, id: &str, u: &Unit) {
    let y0 = s.base(u.level) + u.lift;
    let d = u.direction();
    let r = u.right();
    // Studio convention: the origin is the middle of the unit's back; it extends `depth` forward along its bearing.
    let hw = u.width / 2.0;
    let corner = |sw: f64, sd: f64| -> P2 { [u.origin[0] + r[0] * sw * hw + d[0] * sd * u.depth, u.origin[1] + r[1] * sw * hw + d[1] * sd * u.depth] };
    let ring = [corner(-1.0, 0.0), corner(1.0, 0.0), corner(1.0, 1.0), corner(-1.0, 1.0)];
    let at = |p: P2, y: f64| -> P3 { [p[0], y, p[1]] };
    let wood = material_id("wood");
    let top_mat = if u.has_worktop() { material_id("paintWhite") } else { wood.clone() };
    let box_id = format!("{id}#box");
    let top_id = format!("{id}#top");
    let (y1, y2) = (y0, y0 + u.height.max(0.01));
    let bottom: Vec<P3> = ring.iter().map(|p| at(*p, y1)).collect();
    let top: Vec<P3> = ring.iter().map(|p| at(*p, y2)).collect();
    b.poly(&box_id, bottom, [0.0, -1.0, 0.0], &wood);
    b.poly(&top_id, top, [0.0, 1.0, 0.0], &top_mat);
    for i in 0..4 {
        let (a, c) = (ring[i], ring[(i + 1) % 4]);
        let mut n = [c[1] - a[1], 0.0, -(c[0] - a[0])];
        let centre = [u.origin[0] + d[0] * u.depth / 2.0, u.origin[1] + d[1] * u.depth / 2.0];
        let mid = [(a[0] + c[0]) / 2.0 - centre[0], 0.0, (a[1] + c[1]) / 2.0 - centre[1]];
        if dot(n, mid) < 0.0 { n = scale(n, -1.0); }
        b.quad(&box_id, at(a, y1), at(c, y1), at(c, y2), at(a, y2), n, &wood);
    }
}

/// A unit built from its component script: every solid is a prism in the unit's frame
/// (x across = `right()`, plan y forward = `direction()`, heights above the storey floor).
pub fn build_scripted_unit(b: &mut Faces, s: &Settings, id: &str, u: &Unit, solids: &[crate::script::Solid], worktop: Option<crate::script::Worktop>) {
    let base = s.base(u.level);
    let d = u.direction();
    let r = u.right();
    let place = |p: P2| -> P2 { [u.origin[0] + r[0] * p[0] + d[0] * p[1], u.origin[1] + r[1] * p[0] + d[1] * p[1]] };
    let at = |p: P2, y: f64| -> P3 { [p[0], y, p[1]] };
    let prism = |b: &mut Faces, fid: &str, plan: &[P2], low: f64, high: f64, material: &str| {
        if plan.len() < 3 || high <= low { return; }
        let ring: Vec<P2> = plan.iter().map(|p| place(*p)).collect();
        let (y1, y2) = (base + low, base + high);
        let bottom: Vec<P3> = ring.iter().map(|p| at(*p, y1)).collect();
        let top: Vec<P3> = ring.iter().map(|p| at(*p, y2)).collect();
        b.poly(fid, bottom, [0.0, -1.0, 0.0], material);
        b.poly(fid, top, [0.0, 1.0, 0.0], material);
        let n = ring.len();
        let c = ring.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0], a[1] + p[1]]);
        let c = [c[0] / n as f64, c[1] / n as f64];
        for i in 0..n {
            let (p, q) = (ring[i], ring[(i + 1) % n]);
            let mut sn = [q[1] - p[1], 0.0, -(q[0] - p[0])];
            let mid = [(p[0] + q[0]) / 2.0 - c[0], 0.0, (p[1] + q[1]) / 2.0 - c[1]];
            if dot(sn, mid) < 0.0 { sn = scale(sn, -1.0); }
            b.quad(fid, at(p, y1), at(q, y1), at(q, y2), at(p, y2), sn, material);
        }
    };
    for (i, sol) in solids.iter().enumerate() {
        prism(b, &format!("{id}#solid.{i}"), &sol.plan, sol.low, sol.high, &crate::script::material_for(&sol.material));
    }
    // A worktop over a unit that declares one (Studio merges neighbouring tops into one run; one per unit here).
    if let (Some(w), Some(bb)) = (worktop, crate::script::bounds(solids)) {
        let (x0, x1) = (bb.min_x, bb.max_x);
        let (d0, d1) = (bb.min_d, bb.min_d + w.depth + 0.02);
        prism(b, &format!("{id}#worktop"), &[[x0, d0], [x1, d0], [x1, d1], [x0, d1]], w.at, w.at + 0.04, &crate::script::material_for("Worktop"));
    }
}
