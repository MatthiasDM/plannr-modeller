//! Conduit routing inside the building fabric, as an installer would chase it.
//!
//! `ceiling`: from the board up inside its wall to the horizontal chase zone 15 cm under the
//! ceiling, along the walls (through the wall graph of the construction plugin, in the middle
//! of the room-side leaf) to the wall of the point, then down inside that wall to the box.
//! Ceiling lights leave the wall at the top and cross inside the ceiling to the fitting.
//! `floor`: down inside the wall into the floor screed, straight across in the screed, up
//! inside the wall of the point. Storeys are crossed by a riser inside a wall that exists on
//! both storeys, as close to the board as possible.

use std::collections::{BinaryHeap, HashMap};
use std::cmp::Ordering;

use plannr_core::geom::P3;
use plannr_core::model::Document;
use plannr_plugin_construction::data::{Fixture, FixtureKind, Settings, P2};
use plannr_plugin_construction::expand::{self, route_offset, WallSet};
use plannr_plugin_construction::fixtures::{mount, nearest_wall, Mount};

/// Depth of the chase behind the wall face (centre of the tube).
const CHASE: f64 = 0.025;
/// Horizontal chase zone under the ceiling.
const UNDER_CEILING: f64 = 0.15;
const NODE_TOL: f64 = 0.03;

struct Graph {
    nodes: Vec<P2>,
    /// node → (node, cost, piece)
    adj: Vec<Vec<(usize, f64, usize)>>,
    /// piece → nodes along it, by distance from its `a`
    along: HashMap<usize, Vec<(f64, usize)>>,
}

/// A place on a wall piece.
#[derive(Clone, Copy, Debug)]
struct Spot { piece: usize, t: f64 }

#[derive(PartialEq)]
struct Q(f64, usize);
impl Eq for Q {}
impl Ord for Q { fn cmp(&self, o: &Self) -> Ordering { o.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal) } }
impl PartialOrd for Q { fn partial_cmp(&self, o: &Self) -> Option<Ordering> { Some(self.cmp(o)) } }

pub struct Router {
    pub settings: Settings,
    set: WallSet,
    graphs: HashMap<usize, Graph>,
    fixtures: HashMap<String, (Fixture, Mount)>,
}

/// One conduit to create: its storey and polyline (heights above that storey's floor).
pub struct Leg { pub level: usize, pub points: Vec<P3> }

pub struct Route { pub main: Leg, pub risers: Vec<Leg> }

impl Router {
    pub fn new(doc: &Document) -> Router {
        let m = expand::model(doc);
        let set = expand::collect(&m);
        let settings = m.settings.clone();
        let fixtures = m.fixtures.iter().map(|(c, f)| (c.id.clone(), (f.clone(), mount(&settings, &set, f)))).collect();
        let mut levels: Vec<usize> = set.walls.iter().map(|w| w.level).collect();
        levels.sort(); levels.dedup();
        let graphs = levels.into_iter().map(|l| (l, build_graph(&set, l))).collect();
        Router { settings, set, graphs, fixtures }
    }

    pub fn mount(&self, id: &str) -> Option<&(Fixture, Mount)> { self.fixtures.get(id) }

    fn piece_dir(&self, i: usize) -> (P2, f64) {
        let w = &self.set.walls[i];
        let l = w.length().max(1e-9);
        ([(w.b[0] - w.a[0]) / l, (w.b[1] - w.a[1]) / l], l)
    }
    /// Plan point at distance t along piece i, in the middle of its room-side leaf.
    fn at(&self, sp: Spot) -> P2 {
        let w = &self.set.walls[sp.piece];
        let (d, _) = self.piece_dir(sp.piece);
        let o = route_offset(&self.settings, w);
        [w.a[0] + d[0] * sp.t + w.out[0] * o, w.a[1] + d[1] * sp.t + w.out[1] * o]
    }
    fn spot_near(&self, level: usize, p: P2) -> Option<Spot> {
        nearest_wall(&self.settings, &self.set, level, p, 60.0).map(|(i, t, _, _)| { let (_, l) = self.piece_dir(i); Spot { piece: i, t: t.clamp(0.0, l) } })
    }

    fn height(&self, level: usize) -> f64 { self.settings.height(level) }
    /// The horizontal chase zone: 15 cm under the ceiling, i.e. under the structure that fills the top
    /// of the storey (the floor above, attic joists) — inside the walls, which stop beneath it.
    fn y_route(&self, level: usize) -> f64 {
        let structure = self.settings.structure.iter().filter(|t| t.level == level).map(|t| t.depth).fold(0.0, f64::max);
        self.height(level) - structure - UNDER_CEILING
    }

    /// Shortest path along the walls between two spots on one storey, as plan points.
    fn walk(&self, level: usize, a: Spot, b: Spot) -> Option<Vec<P2>> {
        if a.piece == b.piece { return Some(vec![self.at(a), self.at(b)]); }
        let g = self.graphs.get(&level)?;
        let (dist, prev) = dijkstra(g, &self.entry(g, a));
        let exits = self.entry(g, b);
        let (best, _) = exits.iter().filter(|(n, _)| dist[*n].is_finite()).map(|(n, c)| (*n, dist[*n] + c)).min_by(|x, y| x.1.partial_cmp(&y.1).unwrap())?;
        Some(self.trace(g, a, best, &prev, Some(b)))
    }
    /// Nodes bracketing a spot on its piece, with the cost to reach them.
    fn entry(&self, g: &Graph, s: Spot) -> Vec<(usize, f64)> {
        let Some(list) = g.along.get(&s.piece) else { return vec![] };
        let mut lo: Option<(f64, usize)> = None; let mut hi: Option<(f64, usize)> = None;
        for &(t, n) in list {
            if t <= s.t + 1e-9 { lo = Some((t, n)); }
            if t >= s.t - 1e-9 && hi.is_none() { hi = Some((t, n)); }
        }
        [lo, hi].into_iter().flatten().map(|(t, n)| (n, (s.t - t).abs())).collect()
    }
    fn node_t(g: &Graph, piece: usize, node: usize) -> f64 {
        g.along.get(&piece).and_then(|l| l.iter().find(|(_, n)| *n == node)).map(|x| x.0).unwrap_or(0.0)
    }
    /// Plan polyline from spot `a` to node `end` (then on to spot `b` if given).
    fn trace(&self, g: &Graph, a: Spot, end: usize, prev: &[Option<(usize, usize)>], b: Option<Spot>) -> Vec<P2> {
        let mut chain = vec![end];
        let mut pieces = vec![];
        let mut cur = end;
        while let Some((p, piece)) = prev[cur] { pieces.push(piece); chain.push(p); cur = p; }
        chain.reverse(); pieces.reverse();
        let mut out = vec![self.at(a), self.at(Spot { piece: a.piece, t: Self::node_t(g, a.piece, chain[0]) })];
        for (k, piece) in pieces.iter().enumerate() {
            out.push(self.at(Spot { piece: *piece, t: Self::node_t(g, *piece, chain[k]) }));
            out.push(self.at(Spot { piece: *piece, t: Self::node_t(g, *piece, chain[k + 1]) }));
        }
        if let Some(b) = b {
            out.push(self.at(Spot { piece: b.piece, t: Self::node_t(g, b.piece, end) }));
            out.push(self.at(b));
        }
        out
    }

    /// A node of `level` that also lies on a wall of `next`, nearest (along the walls) to `from`;
    /// returns the plan path to it and the spot on `next` above/below it.
    fn riser(&self, level: usize, from: Spot, next: usize) -> Option<(Vec<P2>, Spot)> {
        let g = self.graphs.get(&level)?;
        let (dist, prev) = dijkstra(g, &self.entry(g, from));
        let mut best: Option<(f64, usize, Spot)> = None;
        for (n, p) in g.nodes.iter().enumerate() {
            if !dist[n].is_finite() { continue; }
            if best.as_ref().map(|b| dist[n] >= b.0).unwrap_or(false) { continue; }
            if let Some((i, t, _, _)) = nearest_wall(&self.settings, &self.set, next, *p, 0.2) {
                let w = &self.set.walls[i];
                let off = (p[0] - w.a[0]) * w.out[0] + (p[1] - w.a[1]) * w.out[1];
                let (_, l) = self.piece_dir(i);
                if off.abs() < 0.08 && t > -0.02 && t < l + 0.02 { best = Some((dist[n], n, Spot { piece: i, t: t.clamp(0.0, l) })); }
            }
        }
        let (_, n, up) = best?;
        Some((self.trace(g, from, n, &prev, None), up))
    }

    /// The conduits from the board to one point.
    pub fn route(&self, board: &str, point: &str, mode: &str) -> Option<Route> {
        let (bf, bm) = self.fixtures.get(board)?;
        let (pf, pm) = self.fixtures.get(point)?;
        let ceiling_light = pf.kind == FixtureKind::CeilingLight;
        // points away from every wall (a ceiling socket) are fed through the ceiling too
        let via_ceiling = ceiling_light || pm.wall.is_none();
        let b_in = [bm.pos[0] - bm.normal[0] * CHASE, bm.pos[2] - bm.normal[1] * CHASE];
        let b_y = bm.pos[1] + if bf.kind == FixtureKind::Panel { 0.3 } else { 0.0 };
        let p_in = if via_ceiling { [pm.pos[0], pm.pos[2]] } else { [pm.pos[0] - pm.normal[0] * CHASE, pm.pos[2] - pm.normal[1] * CHASE] };
        let p_y = if ceiling_light { pm.pos[1] - 0.02 } else { pm.pos[1] };

        if mode == "straight" {
            let main = if bf.level == pf.level { vec![[b_in[0], b_y, b_in[1]], [p_in[0], p_y, p_in[1]]] } else { vec![[b_in[0], b_y - self.offset(pf.level, bf.level), b_in[1]], [p_in[0], p_y, p_in[1]]] };
            return Some(Route { main: Leg { level: pf.level, points: main }, risers: vec![] });
        }
        if mode == "floor" && !via_ceiling && bf.level == pf.level {
            let y = -(self.settings.floor_thickness.max(0.03)) / 2.0;
            let pts = vec![[b_in[0], b_y, b_in[1]], [b_in[0], y, b_in[1]], [p_in[0], y, p_in[1]], [p_in[0], p_y, p_in[1]]];
            return Some(Route { main: Leg { level: pf.level, points: pts }, risers: vec![] });
        }

        // ---- along the walls under the ceiling ----
        let mut risers = vec![];
        let mut level = bf.level;
        let mut cur = self.spot_near(level, b_in);
        // the first leg starts in the board's wall
        let mut start: Vec<P3> = vec![[b_in[0], b_y, b_in[1]], [b_in[0], self.y_route(level), b_in[1]]];
        while level != pf.level {
            let next = if pf.level > level { level + 1 } else { level - 1 };
            let found = cur.and_then(|c| self.riser(level, c, next));
            let y = self.y_route(level);
            let (path, up) = match found {
                Some((path, up)) => (path, Some(up)),
                None => (vec![], None),
            };
            let mut pts = start.clone();
            pts.extend(path.iter().map(|p| [p[0], y, p[1]]));
            let top = pts.last().copied().unwrap_or([b_in[0], y, b_in[1]]);
            let rel_next = self.offset(next, level);
            pts.push([top[0], self.y_route(next) + rel_next, top[2]]);
            risers.push(Leg { level: level.min(next), points: shift(dedupe(pts), if next < level { -rel_next } else { 0.0 }) });
            let here = [top[0], top[2]];
            start = vec![[here[0], self.y_route(next), here[1]]];
            cur = up.or_else(|| self.spot_near(next, here));
            level = next;
        }
        let y = self.y_route(level);
        let target = if via_ceiling { self.spot_near(level, p_in) } else { pm.wall.map(|i| Spot { piece: i, t: pm.t }).filter(|s| self.set.walls[s.piece].level == level).or_else(|| self.spot_near(level, p_in)) };
        let path = match (cur, target) { (Some(a), Some(b)) => self.walk(level, a, b), _ => None };
        let mut pts = start;
        match path {
            Some(path) => pts.extend(path.iter().map(|p| [p[0], y, p[1]])),
            None => {}
        }
        if via_ceiling {
            let h = self.height(level);
            let yc = h - 0.012;
            let last = pts.last().copied().unwrap_or([p_in[0], y, p_in[1]]);
            pts.push([last[0], yc, last[2]]);
            pts.push([p_in[0], yc, p_in[1]]);
            pts.push([p_in[0], p_y, p_in[1]]);
        } else {
            pts.push([p_in[0], y, p_in[1]]);
            pts.push([p_in[0], p_y, p_in[1]]);
        }
        Some(Route { main: Leg { level: pf.level, points: dedupe(pts) }, risers })
    }

    /// Height of storey `a`'s floor above storey `b`'s floor.
    fn offset(&self, a: usize, b: usize) -> f64 { self.settings.base(a) - self.settings.base(b) }
}

fn shift(pts: Vec<P3>, dy: f64) -> Vec<P3> { if dy == 0.0 { pts } else { pts.into_iter().map(|p| [p[0], p[1] + dy, p[2]]).collect() } }

fn dedupe(pts: Vec<P3>) -> Vec<P3> {
    let mut out: Vec<P3> = vec![];
    for p in pts {
        if let Some(q) = out.last() { if plannr_core::geom::dist(*q, p) < 1e-3 { continue; } }
        // drop the middle of three collinear points
        if out.len() >= 2 {
            let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
            let (u, v) = (plannr_core::geom::sub(b, a), plannr_core::geom::sub(p, b));
            let c = plannr_core::geom::cross(u, v);
            if plannr_core::geom::len(c) < 1e-6 * plannr_core::geom::len(u).max(1e-9) * plannr_core::geom::len(v).max(1e-9) * 1e3 && plannr_core::geom::dot(u, v) > 0.0 { out.pop(); }
        }
        out.push(p);
    }
    out
}

fn build_graph(set: &WallSet, level: usize) -> Graph {
    let mut g = Graph { nodes: vec![], adj: vec![], along: HashMap::new() };
    let node = |g: &mut Graph, p: P2| -> usize {
        if let Some(i) = g.nodes.iter().position(|q| (q[0] - p[0]).hypot(q[1] - p[1]) < NODE_TOL) { return i; }
        g.nodes.push(p); g.adj.push(vec![]); g.nodes.len() - 1
    };
    let pieces: Vec<usize> = set.walls.iter().enumerate().filter(|(_, w)| w.level == level && w.length() > 1e-4).map(|(i, _)| i).collect();
    for &i in &pieces {
        let w = &set.walls[i];
        let (na, nb) = (node(&mut g, w.a), node(&mut g, w.b));
        g.along.insert(i, vec![(0.0, na), (w.length(), nb)]);
    }
    // T-junctions: a node lying on another piece's centreline splits it
    for &i in &pieces {
        let w = &set.walls[i];
        let l = w.length();
        let d = [(w.b[0] - w.a[0]) / l, (w.b[1] - w.a[1]) / l];
        for (n, p) in g.nodes.iter().enumerate() {
            let t = (p[0] - w.a[0]) * d[0] + (p[1] - w.a[1]) * d[1];
            let off = (p[0] - w.a[0]) * w.out[0] + (p[1] - w.a[1]) * w.out[1];
            if t > NODE_TOL && t < l - NODE_TOL && off.abs() < 0.08 {
                let list = g.along.get_mut(&i).unwrap();
                if !list.iter().any(|(_, m)| *m == n) { list.push((t, n)); }
            }
        }
    }
    let mut edges = vec![];
    for (piece, list) in g.along.iter_mut() {
        list.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        for w in list.windows(2) { edges.push((w[0].1, w[1].1, w[1].0 - w[0].0, *piece)); }
    }
    for (a, b, c, p) in edges {
        if a == b { continue; }
        g.adj[a].push((b, c, p)); g.adj[b].push((a, c, p));
    }
    g
}

fn dijkstra(g: &Graph, sources: &[(usize, f64)]) -> (Vec<f64>, Vec<Option<(usize, usize)>>) {
    let n = g.nodes.len();
    let mut dist = vec![f64::INFINITY; n];
    let mut prev: Vec<Option<(usize, usize)>> = vec![None; n];
    let mut heap = BinaryHeap::new();
    for &(s, c) in sources { if c < dist[s] { dist[s] = c; heap.push(Q(c, s)); } }
    while let Some(Q(d, u)) = heap.pop() {
        if d > dist[u] { continue; }
        for &(v, c, piece) in &g.adj[u] {
            // a small cost per junction keeps the route on as few walls as possible
            let nd = d + c + 0.05;
            if nd < dist[v] { dist[v] = nd; prev[v] = Some((u, piece)); heap.push(Q(nd, v)); }
        }
    }
    (dist, prev)
}
