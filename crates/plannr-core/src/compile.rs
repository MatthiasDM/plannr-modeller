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
pub struct CompiledDimension { pub id: String, pub a: P3, pub b: P3, pub offset: P3, pub value: f64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledText { pub id: String, pub position: P3, pub text: String, #[serde(skip_serializing_if = "Option::is_none")] pub anchor: Option<P3> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledSection { pub id: String, pub origin: P3, pub normal: P3, pub active: bool }
/// The centre of a circle, arc or regular polygon: the "Center" inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledCenter { pub id: String, pub point: P3, pub normal: P3, pub radius: f64 }

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
    pub bounds: Bounds,
    pub metrics: Metrics,
}

#[derive(Debug, Clone, Default)]
pub struct CompileOptions {
    /// Ignore active section planes.
    pub ignore_sections: bool,
    /// Draw hidden entities as ghosts (View → Hidden Geometry) instead of leaving them out.
    pub show_hidden: bool,
}

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

    fn push_triangles(&mut self, gi: usize, id: &str, tris: &[[P3; 3]], n: P3) {
        if tris.is_empty() { return; }
        let id = if self.prefix.is_empty() { id.to_string() } else { format!("{}{}", self.prefix, id) };
        let g = &mut self.groups[gi];
        let start = g.indices.len() as u32;
        for t in tris {
            for p in t {
                let vi = (g.positions.len() / 3) as u32;
                g.positions.extend_from_slice(&[p[0] as f32, p[1] as f32, p[2] as f32]);
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
        self.push_triangles(gi, &f.id, &tris, n);
        let back: Vec<[P3; 3]> = tris.iter().map(|t| [t[0], t[2], t[1]]).collect();
        let bi = self.group_index(f.back_material.as_deref(), true);
        self.push_triangles(bi, &f.id, &back, scale(n, -1.0));
        // outline segments carry the face id
        for lp in std::iter::once(&outer).chain(holes.iter()) {
            for i in 0..lp.len() { self.push_segment(&f.id, lp[i], lp[(i + 1) % lp.len()]); }
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
                for c in &g.entities { self.entity(c, &gm); }
            }
            Entity::Guide(g) => { let p = mat_apply(m, g.point); self.grow(p); self.out.guides.push(CompiledGuide { id: g.id.clone(), point: p, direction: g.direction.map(|d| normalize(mat_apply_dir(m, d))) }); }
            Entity::Dimension(d) => { let a = mat_apply(m, d.a); let b = mat_apply(m, d.b); self.grow(a); self.grow(b); self.out.dimensions.push(CompiledDimension { id: d.id.clone(), a, b, offset: mat_apply_dir(m, d.offset), value: dist(a, b) }); }
            Entity::Text(t) => { let p = mat_apply(m, t.position); self.grow(p); self.out.texts.push(CompiledText { id: t.id.clone(), position: p, text: t.text.clone(), anchor: t.anchor.map(|a| mat_apply(m, a)) }); }
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
                for c in &entities { self.entity(c, &im); }
                self.depth -= 1;
                self.prefix = saved;
            }
        }
    }
}

pub fn compile(doc: &Document, reg: &Registry, opts: &CompileOptions) -> CompiledScene {
    let original = doc;
    let expanded = reg.expand_all(doc);
    let doc = &expanded;
    let hidden: Vec<String> = doc.layers.iter().filter(|l| !l.visible).map(|l| l.id.clone()).collect();
    // the active section planes (world coordinates); several combine, keeping what every one keeps
    let mut cuts = vec![];
    if !opts.ignore_sections {
        doc.visit(|v| {
            if let Entity::Section(s) = v.entity {
                if s.active && !hidden.iter().any(|h| h == &s.layer) {
                    cuts.push((mat_apply(&v.transform, s.origin), normalize(mat_apply_dir(&v.transform, s.normal))));
                }
            }
        });
    }
    let mut b = Builder { doc, groups: vec![], by_key: HashMap::new(), edges: CompiledEdges::default(), out: CompiledScene::default(), cuts, show_hidden: opts.show_hidden, ghost: 0, bb: None, hidden, prefix: String::new(), depth: 0, centers: HashSet::new() };
    b.out.metrics.definitions = doc.components.len();
    let m = mat_identity();
    for e in &doc.entities { b.entity(e, &m); }
    let mut out = b.out;
    out.version = doc.version;
    out.groups = b.groups.into_iter().filter(|g| !g.indices.is_empty()).collect();
    out.edges = b.edges;
    out.bounds = match b.bb { Some((lo, hi)) => Bounds { min: lo, max: hi }, None => Bounds::default() };
    out.metrics.area = (out.metrics.area * 1e4).round() / 1e4;
    out.metrics.edge_length = (out.metrics.edge_length * 1e4).round() / 1e4;
    for v in out.metrics.area_by_material.values_mut() { *v = (*v * 1e4).round() / 1e4; }
    out.metrics.plugins = reg.metrics(original);
    out
}
