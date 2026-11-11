//! Turn a document into render-ready triangles, edge segments and annotations.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::geom::*;
use crate::model::*;
use crate::plugin::Registry;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledGroup {
    pub material: String,
    pub name: String,
    pub color: String,
    pub opacity: f64,
    pub back: bool,
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u32>,
    pub ids: Vec<String>,
    pub id_ranges: Vec<[u32; 2]>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledEdges {
    pub positions: Vec<f32>,
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledGuide { pub id: String, pub point: P3, #[serde(skip_serializing_if = "Option::is_none")] pub direction: Option<P3> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledDimension { pub id: String, pub a: P3, pub b: P3, pub offset: P3, pub value: f64, #[serde(default, skip_serializing_if = "Option::is_none")] pub style: Option<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledText { pub id: String, pub position: P3, pub text: String, #[serde(default, skip_serializing_if = "Option::is_none")] pub anchor: Option<P3>, #[serde(default, skip_serializing_if = "Option::is_none")] pub screen: Option<[f64; 2]> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledSection { pub id: String, pub origin: P3, pub normal: P3, pub active: bool }
/// The centre of a circle, arc or regular polygon: the "Center" inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledCenter { pub id: String, pub point: P3, pub normal: P3, pub radius: f64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledCut { pub origin: P3, pub normal: P3 }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bounds { pub min: P3, pub max: P3 }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub faces: usize,
    pub edges: usize,
    pub groups: usize,
    pub area: f64,
    pub area_by_material: BTreeMap<String, f64>,
    pub edge_length: f64,
    /// Placed component instances and definitions in the document.
    #[serde(default)]
    pub instances: usize,
    #[serde(default)]
    pub definitions: usize,
    /// Per-plugin numbers (`metrics.plugins.<id>`).
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub plugins: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledScene {
    pub version: u64,
    pub groups: Vec<CompiledGroup>,
    pub edges: CompiledEdges,
    pub guides: Vec<CompiledGuide>,
    pub dimensions: Vec<CompiledDimension>,
    pub texts: Vec<CompiledText>,
    pub sections: Vec<CompiledSection>,
    #[serde(default)]
    pub centers: Vec<CompiledCenter>,
    /// The active section planes as they cut (world coordinates): what remains is where every one has
    /// `dot(p - origin, normal) <= 0`. Listed even when the geometry is compiled uncut (`ignore_sections`), so a
    /// viewer can cut on the GPU and switch a plane on or off without new geometry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cuts: Vec<CompiledCut>,
    pub bounds: Bounds,
    pub metrics: Metrics,
}

#[derive(Debug, Clone, Default)]
pub struct CompileOptions {
    /// Ignore active section planes.
    pub ignore_sections: bool,
    /// Draw hidden entities as ghosts (View → Hidden Geometry) instead of leaving them out.
    pub show_hidden: bool,
    /// Compile only these top-level entities (a preview of what an op changed); the plugin metrics are left out.
    pub only: Option<std::collections::HashSet<String>>,
}

/// Faces closer to one plane than this (degrees) are not softened into each other: the edge between them is a
/// drawn line, not a bend in a surface.
pub const SOFT_MIN: f64 = 0.1;

type PKey = (i64, i64, i64);
fn pkey(p: P3) -> PKey { let q = |x: f64| (x * 1e6).round() as i64; (q(p[0]), q(p[1]), q(p[2])) }
fn ekey(a: P3, b: P3) -> (PKey, PKey) { let (a, b) = (pkey(a), pkey(b)); if a <= b { (a, b) } else { (b, a) } }

/// What softening does to one face (world coordinates): the edges it does not draw, and the normal its
/// corners on those edges are shaded with.
#[derive(Default)]
struct Smoothing { soft: HashSet<(PKey, PKey)>, normals: HashMap<PKey, P3> }

const BACK_ID: &str = "back";
/// The material group of ghosts (hidden geometry on show).
const GHOST_ID: &str = "__hidden";
const GHOST_COLOR: &str = "#8a93a0";
const GHOST_OPACITY: f64 = 0.22;

struct Builder<'a> {
    doc: &'a Document,
    groups: Vec<CompiledGroup>,
    by_key: HashMap<(String, bool), usize>,
    edges: CompiledEdges,
    out: CompiledScene,
    /// Every active section plane (world origin, normal): what remains is what all of them keep.
    cuts: Vec<(P3, P3)>,
    show_hidden: bool,
    ghost: usize,
    bb: Option<(P3, P3)>,
    hidden: Vec<String>,
    /// `<instanceId>#` while expanding an instance, so its faces resolve to the instance.
    prefix: String,
    depth: usize,
    /// Centres already emitted (quantised point + radius): an arc's segments share one.
    centers: HashSet<[i64; 4]>,
    /// Softened faces of the containers being compiled, by face id.
    smooth: HashMap<String, Smoothing>,
}

impl<'a> Builder<'a> {
    fn group_index(&mut self, material: Option<&str>, back: bool) -> usize {
        let (id, name, color, opacity) = if self.ghost > 0 { (GHOST_ID.to_string(), "Hidden geometry".to_string(), GHOST_COLOR.to_string(), GHOST_OPACITY) } else { match material.and_then(|m| self.doc.material_by_ref(m)) {
            Some(m) => (m.id.clone(), m.name.clone(), m.color.clone(), m.opacity),
            None if back => (BACK_ID.to_string(), "Back face".to_string(), BACK_FACE_COLOR.to_string(), 1.0),
            None => { let m = self.doc.default_material(); (m.id.clone(), m.name.clone(), m.color.clone(), m.opacity) }
        } };
        let key = (id.clone(), back);
        if let Some(i) = self.by_key.get(&key) { return *i; }
        self.groups.push(CompiledGroup { material: id, name, color, opacity, back, ..Default::default() });
        self.by_key.insert(key, self.groups.len() - 1);
        self.groups.len() - 1
    }

    fn grow(&mut self, p: P3) {
        self.bb = Some(match self.bb {
            None => (p, p),
            Some((lo, hi)) => ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])]),
        });
    }

    fn push_triangles(&mut self, gi: usize, id: &str, tris: &[[P3; 3]], n: P3, round: Option<(&HashMap<PKey, P3>, f64)>) {
        if tris.is_empty() { return; }
        let id = if self.prefix.is_empty() { id.to_string() } else { format!("{}{}", self.prefix, id) };
        let g = &mut self.groups[gi];
        let start = g.indices.len() as u32;
        for t in tris {
            for p in t {
                let vi = (g.positions.len() / 3) as u32;
                g.positions.extend_from_slice(&[p[0] as f32, p[1] as f32, p[2] as f32]);
                let n = round.and_then(|(m, sign)| m.get(&pkey(*p)).map(|x| scale(*x, sign))).unwrap_or(n);
                g.normals.extend_from_slice(&[n[0] as f32, n[1] as f32, n[2] as f32]);
                g.indices.push(vi);
            }
        }
        g.ids.push(id);
        g.id_ranges.push([start, g.indices.len() as u32 - start]);
    }

    fn push_segment(&mut self, id: &str, a: P3, b: P3) {
        let mut seg = Some((a, b));
        for (o, n) in &self.cuts { seg = seg.and_then(|(a, b)| clip_segment(a, b, *o, *n)); }
        if let Some((a, b)) = seg {
            self.edges.positions.extend_from_slice(&[a[0] as f32, a[1] as f32, a[2] as f32, b[0] as f32, b[1] as f32, b[2] as f32]);
            self.edges.ids.push(if self.prefix.is_empty() { id.to_string() } else { format!("{}{}", self.prefix, id) });
            self.grow(a); self.grow(b);
        }
    }

    fn center(&mut self, id: &str, point: P3, normal: P3, radius: f64) {
        let q = |x: f64| (x * 1e5).round() as i64;
        if !self.centers.insert([q(point[0]), q(point[1]), q(point[2]), q(radius)]) { return; }
        let id = if self.prefix.is_empty() { id.to_string() } else { format!("{}{}", self.prefix, id) };
        self.out.centers.push(CompiledCenter { id, point, normal, radius });
    }

    /// A face whose outer loop is a circle or regular polygon (5+ corners equidistant from their centroid) has a centre.
    fn face_center(&mut self, id: &str, outer: &[P3], n: P3) {
        if outer.len() < 5 { return; }
        let mut c = [0.0; 3];
        for p in outer { c = add(c, *p); }
        let c = scale(c, 1.0 / outer.len() as f64);
        let r = dist(outer[0], c);
        if r < 1e-6 || outer.iter().any(|p| (dist(*p, c) - r).abs() > 1e-5 + r * 1e-6) { return; }
        self.center(id, c, n, r);
    }

    fn face(&mut self, f: &Face, m: &Mat4) {
        let mut outer: Vec<P3> = f.outer.iter().map(|p| mat_apply(m, *p)).collect();
        let mut holes: Vec<Vec<P3>> = f.holes.iter().map(|h| h.iter().map(|p| mat_apply(m, *p)).collect()).collect();
        if outer.len() < 3 { return; }
        // a mirroring transform (negative determinant) flips the winding: undo it so the normal stays outward
        if mat_det3(m) < 0.0 { outer.reverse(); for h in holes.iter_mut() { h.reverse(); } }
        let n = normal(&outer);
        self.face_center(&f.id, &outer, n);
        let pts: Vec<P3> = outer.iter().chain(holes.iter().flatten()).copied().collect();
        let idx = tessellate(&outer, &holes);
        let mut tris: Vec<[P3; 3]> = idx.chunks(3).filter(|c| c.len() == 3).map(|c| [pts[c[0] as usize], pts[c[1] as usize], pts[c[2] as usize]]).collect();
        for (o, cn) in &self.cuts {
            let mut kept = vec![];
            for t in tris {
                let poly = clip_polygon(&t, *o, *cn);
                if poly.len() >= 3 {
                    for i in 1..poly.len() - 1 { kept.push([poly[0], poly[i], poly[i + 1]]); }
                }
            }
            tris = kept;
        }
        for t in &tris { for p in t { self.grow(*p); } }
        let a = tris.iter().map(|t| area(t)).sum::<f64>();
        self.out.metrics.faces += 1;
        self.out.metrics.area += a;
        let mname = f.material.as_deref().and_then(|m| self.doc.material_by_ref(m)).map(|m| m.name.clone()).unwrap_or_else(|| self.doc.default_material().name.clone());
        *self.out.metrics.area_by_material.entry(mname).or_default() += a;
        let gi = self.group_index(f.material.as_deref(), false);
        let sm = self.smooth.remove(&f.id);
        self.push_triangles(gi, &f.id, &tris, n, sm.as_ref().map(|s| (&s.normals, 1.0)));
        let back: Vec<[P3; 3]> = tris.iter().map(|t| [t[0], t[2], t[1]]).collect();
        let bi = self.group_index(f.back_material.as_deref(), true);
        self.push_triangles(bi, &f.id, &back, scale(n, -1.0), sm.as_ref().map(|s| (&s.normals, -1.0)));
        // outline segments carry the face id; `soft` (outer-loop edge indices, edge i runs from corner i to i+1)
        // leaves edges out of the drawing, as SketchUp's softened edges: a swept surface shows no seams
        let soft: Vec<usize> = f.extra.get("soft").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64().map(|x| x as usize)).collect()).unwrap_or_default();
        let flipped = mat_det3(m) < 0.0;
        for (li, lp) in std::iter::once(&outer).chain(holes.iter()).enumerate() {
            let n = lp.len();
            for i in 0..n {
                if li == 0 && !soft.is_empty() {
                    // the loop was reversed for a mirroring transform: edge i is now edge n-2-i
                    let orig = if flipped { (2 * n - 2 - i) % n } else { i };
                    if soft.contains(&orig) { continue; }
                }
                // a softened edge shows only as hidden geometry
                if !self.show_hidden && sm.as_ref().map_or(false, |s| s.soft.contains(&ekey(lp[i], lp[(i + 1) % n]))) { continue; }
                self.push_segment(&f.id, lp[i], lp[(i + 1) % n]);
            }
        }
    }

    /// Work out the softening among the faces of one container (`m` takes it to world coordinates): an edge
    /// that exactly two softened faces share, meeting at an angle within both their limits, is soft. Around
    /// each corner the faces that soft edges join are shaded with one normal.
    fn soften(&mut self, list: &[Entity], m: &Mat4) {
        let faces: Vec<&Face> = list.iter().filter_map(|e| match e { Entity::Face(f) if f.outer.len() >= 3 && f.smooth().is_some() => Some(f), _ => None }).collect();
        if faces.len() < 2 { return; }
        let flip = mat_det3(m) < 0.0;
        let world: Vec<(P3, f64, Vec<Vec<P3>>)> = faces.iter().map(|f| {
            let loops: Vec<Vec<P3>> = std::iter::once(&f.outer).chain(f.holes.iter()).map(|l| l.iter().map(|p| mat_apply(m, *p)).collect()).collect();
            let n = normal(&loops[0]);
            (if flip { scale(n, -1.0) } else { n }, f.smooth().unwrap_or(0.0), loops)
        }).collect();
        // every edge with the faces that have it, and which way each runs along it
        let mut by_edge: HashMap<(PKey, PKey), Vec<(usize, bool)>> = HashMap::new();
        for (fi, (_, _, loops)) in world.iter().enumerate() {
            for l in loops { for i in 0..l.len() {
                let (a, b) = (pkey(l[i]), pkey(l[(i + 1) % l.len()]));
                if a != b { by_edge.entry(if a <= b { (a, b) } else { (b, a) }).or_default().push((fi, a <= b)); }
            } }
        }
        // per face: its soft edges, each with the face across it (sign -1 when that face is wound the other way round)
        let mut links: Vec<Vec<((PKey, PKey), usize, f64)>> = vec![vec![]; world.len()];
        for (k, v) in &by_edge {
            if v.len() != 2 || v[0].0 == v[1].0 { continue; }
            let ((f, df), (g, dg)) = (v[0], v[1]);
            let sign = if df == dg { -1.0 } else { 1.0 };
            let deg = (dot(world[f].0, world[g].0) * sign).clamp(-1.0, 1.0).acos().to_degrees();
            if deg <= SOFT_MIN || deg > world[f].1.min(world[g].1) + 1e-9 { continue; }
            links[f].push((*k, g, sign)); links[g].push((*k, f, sign));
        }
        for (fi, f) in faces.iter().enumerate() {
            if links[fi].is_empty() { self.smooth.remove(&f.id); continue; }
            let mut s = Smoothing::default();
            let mut corners: Vec<PKey> = vec![];
            for (k, _, _) in &links[fi] { s.soft.insert(*k); for c in [k.0, k.1] { if !corners.contains(&c) { corners.push(c); } } }
            for c in corners {
                let (mut sum, mut seen, mut todo) = (world[fi].0, vec![fi], vec![(fi, 1.0)]);
                while let Some((x, sx)) = todo.pop() {
                    for (k, y, sg) in &links[x] {
                        if (k.0 != c && k.1 != c) || seen.contains(y) { continue; }
                        seen.push(*y); todo.push((*y, sx * sg)); sum = add(sum, scale(world[*y].0, sx * sg));
                    }
                }
                let n = normalize(sum);
                if len(n) > 0.5 { s.normals.insert(c, n); }
            }
            self.smooth.insert(f.id.clone(), s);
        }
    }

    fn entity(&mut self, e: &Entity, m: &Mat4) {
        if self.hidden.iter().any(|h| h == e.layer()) { return; }
        if e.hidden() {
            if !self.show_hidden || matches!(e, Entity::Guide(_) | Entity::Dimension(_) | Entity::Text(_) | Entity::Section(_)) { return; }
            // a ghost: drawn and pickable, but it does not count in the metrics
            let metrics = self.out.metrics.clone();
            self.ghost += 1;
            self.entity_inner(e, m);
            self.ghost -= 1;
            self.out.metrics = metrics;
            return;
        }
        self.entity_inner(e, m);
    }

    fn entity_inner(&mut self, e: &Entity, m: &Mat4) {
        match e {
            Entity::Face(f) => self.face(f, m),
            Entity::Edge(ed) => {
                let a = mat_apply(m, ed.a); let b = mat_apply(m, ed.b);
                self.out.metrics.edges += 1;
                self.out.metrics.edge_length += dist(a, b);
                self.push_segment(&ed.id, a, b);
                // an arc segment (tagged by draw_arc) whose ends still lie on its circle gives the circle's centre
                if let Some(arc) = ed.extra.get("arc") {
                    let p3 = |v: &serde_json::Value| -> Option<P3> { let a = v.as_array()?; Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?]) };
                    if let (Some(c), Some(n), Some(r)) = (arc.get("center").and_then(p3), arc.get("normal").and_then(p3), arc.get("radius").and_then(|r| r.as_f64())) {
                        let (c, n) = (mat_apply(m, c), normalize(mat_apply_dir(m, n)));
                        let (ra, rb) = (dist(a, c), dist(b, c));
                        if r > 1e-6 && (ra - r).abs() < 1e-5 + r * 1e-6 && (rb - r).abs() < 1e-5 + r * 1e-6 { self.center(&ed.id, c, n, r); }
                    }
                }
            }
            Entity::Group(g) => {
                self.out.metrics.groups += 1;
                let gm = mat_mul(m, &g.transform);
                self.soften(&g.entities, &gm);
                for c in &g.entities { self.entity(c, &gm); }
            }
            Entity::Guide(g) => { let p = mat_apply(m, g.point); self.grow(p); self.out.guides.push(CompiledGuide { id: g.id.clone(), point: p, direction: g.direction.map(|d| normalize(mat_apply_dir(m, d))) }); }
            Entity::Dimension(d) => { let a = mat_apply(m, d.a); let b = mat_apply(m, d.b); self.grow(a); self.grow(b); self.out.dimensions.push(CompiledDimension { id: d.id.clone(), a, b, offset: mat_apply_dir(m, d.offset), value: dist(a, b), style: d.style.clone() }); }
            Entity::Text(t) => { let p = mat_apply(m, t.position); if t.screen.is_none() { self.grow(p); } self.out.texts.push(CompiledText { id: t.id.clone(), position: p, text: t.text.clone(), anchor: t.anchor.map(|a| mat_apply(m, a)), screen: t.screen }); }
            Entity::Section(s) => self.out.sections.push(CompiledSection { id: s.id.clone(), origin: mat_apply(m, s.origin), normal: normalize(mat_apply_dir(m, s.normal)), active: s.active }),
            Entity::Custom(_) => {} // expanded by the registry before compile
            Entity::Instance(i) => {
                if self.depth > 16 { return; } // guard against definitions that contain themselves
                let Some(def) = self.doc.component(&i.definition) else { return; };
                let entities = def.entities.clone();
                let im = mat_mul(m, &i.transform);
                let saved = std::mem::replace(&mut self.prefix, format!("{}#", i.id));
                self.out.metrics.instances += 1;
                self.depth += 1;
                self.soften(&entities, &im);
                for c in &entities { self.entity(c, &im); }
                self.depth -= 1;
                self.prefix = saved;
            }
        }
    }
}

pub fn compile(doc: &Document, reg: &Registry, opts: &CompileOptions) -> CompiledScene {
    let original = doc;
    let expanded = match &opts.only { Some(keep) => reg.expand_some(doc, keep), None => reg.expand_all(doc) };
    let doc = &expanded;
    let hidden: Vec<String> = doc.layers.iter().filter(|l| !l.visible).map(|l| l.id.clone()).collect();
    // the active section planes (world coordinates); several combine, keeping what every one keeps. Read from the
    // whole document: a partial compile (`only`) still reports every plane.
    let mut all_cuts = vec![];
    original.visit(|v| {
        if let Entity::Section(s) = v.entity {
            if s.active && !hidden.iter().any(|h| h == &s.layer) {
                all_cuts.push((mat_apply(&v.transform, s.origin), normalize(mat_apply_dir(&v.transform, s.normal))));
            }
        }
    });
    let cuts = if opts.ignore_sections { vec![] } else { all_cuts.clone() };
    let mut b = Builder { doc, groups: vec![], by_key: HashMap::new(), edges: CompiledEdges::default(), out: CompiledScene::default(), cuts, show_hidden: opts.show_hidden, ghost: 0, bb: None, hidden, prefix: String::new(), depth: 0, centers: HashSet::new(), smooth: HashMap::new() };
    b.out.metrics.definitions = doc.components.len();
    let m = mat_identity();
    // (from the whole document: a partial compile still sees the neighbours of the faces it draws)
    b.soften(&original.entities, &m);
    for e in &doc.entities { b.entity(e, &m); }
    let mut out = b.out;
    out.version = doc.version;
    out.cuts = all_cuts.into_iter().map(|(origin, normal)| CompiledCut { origin, normal }).collect();
    out.groups = b.groups.into_iter().filter(|g| !g.indices.is_empty()).collect();
    out.edges = b.edges;
    out.bounds = match b.bb { Some((lo, hi)) => Bounds { min: lo, max: hi }, None => Bounds::default() };
    out.metrics.area = (out.metrics.area * 1e4).round() / 1e4;
    out.metrics.edge_length = (out.metrics.edge_length * 1e4).round() / 1e4;
    for v in out.metrics.area_by_material.values_mut() { *v = (*v * 1e4).round() / 1e4; }
    if opts.only.is_none() { out.metrics.plugins = reg.metrics(original); }
    out
}
