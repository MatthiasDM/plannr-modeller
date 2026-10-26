//! Shared geometry plumbing: a face builder, the takeoff items an element yields, and the dispatch
//! from a `masonry.*` entity to its expansion (walls in `wall.rs`, floors and foundations in `floor.rs`).

use plannr_core::geom::{add, area as poly_area, cross, dot, newell, normalize, scale, P3};
use plannr_core::model::{Custom, Edge, Entity, Face};

use crate::masonry::data::*;

pub const UP: P3 = [0.0, 1.0, 0.0];

pub struct Faces { pub out: Vec<Entity>, layer: String }
impl Faces {
    pub fn new(layer: &str) -> Self { Faces { out: vec![], layer: layer.to_string() } }
    pub fn poly_holes(&mut self, id: &str, mut pts: Vec<P3>, holes: Vec<Vec<P3>>, n: P3, material: &str) {
        if pts.len() < 3 { return; }
        if dot(newell(&pts), n) < 0.0 { pts.reverse(); }
        if poly_area(&pts) < 1e-9 { return; }
        self.out.push(Entity::Face(Face { id: id.to_string(), layer: self.layer.clone(), outer: pts, holes, material: Some(material.to_string()), back_material: Some(material.to_string()), extra: Default::default() }));
    }
    pub fn poly(&mut self, id: &str, pts: Vec<P3>, n: P3, material: &str) { self.poly_holes(id, pts, vec![], n, material); }
    /// A loose edge (bond pattern, outlines).
    pub fn edge(&mut self, id: &str, a: P3, b: P3) {
        if plannr_core::geom::dist(a, b) < 1e-6 { return; }
        self.out.push(Entity::Edge(Edge { id: id.to_string(), layer: self.layer.clone(), a, b, extra: Default::default() }));
    }
    /// An oriented box: `origin + ex·x + ey·y + ez·z` over the three ranges (`ex, ey, ez` orthonormal).
    pub fn boxf(&mut self, id: &str, origin: P3, ex: P3, ey: P3, ez: P3, r: [[f64; 2]; 3], material: &str) {
        let at = |x: f64, y: f64, z: f64| add(add(add(origin, scale(ex, x)), scale(ey, y)), scale(ez, z));
        let [[x0, x1], [y0, y1], [z0, z1]] = r;
        if x1 - x0 < 1e-6 || y1 - y0 < 1e-6 || z1 - z0 < 1e-6 { return; }
        self.poly(id, vec![at(x0, y0, z0), at(x0, y1, z0), at(x1, y1, z0), at(x1, y0, z0)], scale(ez, -1.0), material);
        self.poly(id, vec![at(x0, y0, z1), at(x1, y0, z1), at(x1, y1, z1), at(x0, y1, z1)], ez, material);
        self.poly(id, vec![at(x0, y0, z0), at(x1, y0, z0), at(x1, y0, z1), at(x0, y0, z1)], scale(ey, -1.0), material);
        self.poly(id, vec![at(x0, y1, z0), at(x0, y1, z1), at(x1, y1, z1), at(x1, y1, z0)], ey, material);
        self.poly(id, vec![at(x0, y0, z0), at(x0, y0, z1), at(x0, y1, z1), at(x0, y1, z0)], scale(ex, -1.0), material);
        self.poly(id, vec![at(x1, y0, z0), at(x1, y1, z0), at(x1, y1, z1), at(x1, y0, z1)], ex, material);
    }
    /// A plan polygon (x, z) from height `y0` up to a planar sloped top (`tops[i]` over `plan[i]`).
    pub fn prism_sloped(&mut self, id: &str, plan: &[P2], y0: f64, tops: &[f64], material: &str) {
        let n = plan.len();
        if n < 3 || tops.len() != n { return; }
        let bot: Vec<P3> = plan.iter().map(|p| [p[0], y0, p[1]]).collect();
        let top: Vec<P3> = plan.iter().zip(tops).map(|(p, y)| [p[0], y.max(y0), p[1]]).collect();
        self.poly(id, bot.clone(), [0.0, -1.0, 0.0], material);
        let tn = newell(&top); let tn = if tn[1] < 0.0 { scale(tn, -1.0) } else { tn };
        self.poly(id, top.clone(), tn, material);
        let c = plan.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0] / n as f64, a[1] + p[1] / n as f64]);
        for i in 0..n {
            let j = (i + 1) % n;
            let (a, b) = (plan[i], plan[j]);
            let mut sn = normalize(cross([b[0] - a[0], 0.0, b[1] - a[1]], UP));
            let mid = [(a[0] + b[0]) / 2.0 - c[0], 0.0, (a[1] + b[1]) / 2.0 - c[1]];
            if dot(sn, mid) < 0.0 { sn = scale(sn, -1.0); }
            let mut q = vec![bot[i], bot[j]];
            if top[j][1] - y0 > 1e-6 { q.push(top[j]); }
            if top[i][1] - y0 > 1e-6 { q.push(top[i]); }
            if q.len() >= 3 { self.poly(id, q, sn, material); }
        }
    }
    /// Extrude a plan polygon (x, z) from height y0 to y1, with optional plan holes going through.
    pub fn prism_plan(&mut self, id: &str, plan: &[P2], holes: &[Vec<P2>], y0: f64, y1: f64, material: &str) {
        if plan.len() < 3 || y1 - y0 < 1e-6 { return; }
        let at = |p: &P2, y: f64| -> P3 { [p[0], y, p[1]] };
        self.poly_holes(id, plan.iter().map(|p| at(p, y0)).collect(), holes.iter().map(|h| h.iter().map(|p| at(p, y0)).collect()).collect(), [0.0, -1.0, 0.0], material);
        self.poly_holes(id, plan.iter().map(|p| at(p, y1)).collect(), holes.iter().map(|h| h.iter().map(|p| at(p, y1)).collect()).collect(), UP, material);
        let ring = |b: &mut Faces, pts: &[P2], inward: bool| {
            let n = pts.len(); let c = pts.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0] / n as f64, a[1] + p[1] / n as f64]);
            for i in 0..n {
                let (a, bq) = (pts[i], pts[(i + 1) % n]);
                let mut sn = normalize(cross([bq[0] - a[0], 0.0, bq[1] - a[1]], UP));
                let mid = [(a[0] + bq[0]) / 2.0 - c[0], 0.0, (a[1] + bq[1]) / 2.0 - c[1]];
                if (dot(sn, mid) < 0.0) != inward { sn = scale(sn, -1.0); }
                b.poly(id, vec![at(&a, y0), at(&bq, y0), at(&bq, y1), at(&a, y1)], sn, material);
            }
        };
        ring(self, plan, false);
        for h in holes { ring(self, h, true); }
    }
}

/// One line of the takeoff.
#[derive(Debug, Clone, Default)]
pub struct Item { pub key: String, pub category: &'static str, pub description: String, pub count: f64, pub length: f64, pub area: f64, pub volume: f64 }
impl Item {
    pub fn new(key: impl Into<String>, category: &'static str, description: impl Into<String>) -> Item { Item { key: key.into(), category, description: description.into(), ..Default::default() } }
    pub fn count(mut self, n: f64) -> Item { self.count = n; self }
    pub fn length(mut self, l: f64) -> Item { self.length = l; self }
    pub fn area(mut self, a: f64) -> Item { self.area = a; self }
    pub fn volume(mut self, v: f64) -> Item { self.volume = v; self }
}

#[derive(Debug, Clone, Default)]
pub struct Expansion { pub faces: Vec<Entity>, pub items: Vec<Item> }

/// Expansion of any masonry entity.
pub fn expand_any(c: &Custom, house: &House, layer: &str) -> Expansion {
    match c.type_.as_str() {
        T_WALL => serde_json::from_value::<Wall>(c.data.clone()).map(|w| crate::masonry::wall::expand_wall(&c.id, &w, house, layer)).unwrap_or_default(),
        T_FLOOR => serde_json::from_value::<Floor>(c.data.clone()).map(|f| crate::masonry::floor::expand_floor(&c.id, &f, house, layer)).unwrap_or_default(),
        T_FOUNDATION => serde_json::from_value::<Foundation>(c.data.clone()).map(|f| crate::masonry::floor::expand_foundation(&c.id, &f, house, layer)).unwrap_or_default(),
        _ => Expansion::default(),
    }
}
