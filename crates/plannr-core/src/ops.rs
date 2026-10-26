//! Modelling operations. Every change to a document goes through `apply`.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::geom::*;
use crate::model::*;
use crate::plugin::{split_key, Registry, Side};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct OpError(pub String);

pub type OpResult2<T> = Result<T, OpError>;
fn err<T>(s: impl Into<String>) -> OpResult2<T> { Err(OpError(s.into())) }

/// Drop `#faceKey` suffixes and duplicates, keeping order.
fn bare_ids(ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for id in ids {
        let b = split_key(id).0.to_string();
        if !out.contains(&b) { out.push(b); }
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    DrawLine { points: Vec<P3>, #[serde(default)] close: bool, #[serde(default)] layer: Option<String> },
    DrawRectangle { corner: P3, opposite: P3, normal: P3, #[serde(default)] u: Option<P3>, #[serde(default)] layer: Option<String>, #[serde(default)] material: Option<String> },
    DrawPolygon { points: Vec<P3>, #[serde(default)] layer: Option<String>, #[serde(default)] material: Option<String> },
    DrawCircle { center: P3, normal: P3, radius: f64, #[serde(default)] segments: Option<usize>, #[serde(default)] layer: Option<String>, #[serde(default)] material: Option<String> },
    PushPull { face: String, distance: f64 },
    Offset { face: String, distance: f64 },
    Move { ids: Vec<String>, delta: P3 },
    Rotate { ids: Vec<String>, origin: P3, axis: P3, angle: f64 },
    Scale { ids: Vec<String>, origin: P3, factors: P3 },
    Copy { ids: Vec<String>, delta: P3, #[serde(default)] count: Option<usize> },
    Erase { ids: Vec<String> },
    Paint { ids: Vec<String>, material: String, #[serde(default)] side: Option<String> },
    SetLayer { ids: Vec<String>, layer: String },
    MakeGroup { ids: Vec<String>, #[serde(default)] name: Option<String> },
    Explode { id: String },
    AddLayer { name: String, #[serde(default)] color: Option<String> },
    UpdateLayer { id: String, #[serde(default)] name: Option<String>, #[serde(default)] visible: Option<bool>, #[serde(default)] color: Option<String> },
    RemoveLayer { id: String },
    AddMaterial { name: String, color: String, #[serde(default)] opacity: Option<f64> },
    UpdateMaterial { id: String, #[serde(default)] name: Option<String>, #[serde(default)] color: Option<String>, #[serde(default)] opacity: Option<f64> },
    AddGuide { point: P3, #[serde(default)] direction: Option<P3>, #[serde(default)] layer: Option<String> },
    AddDimension { a: P3, b: P3, offset: P3, #[serde(default)] layer: Option<String> },
    AddText { position: P3, text: String, #[serde(default)] anchor: Option<P3>, #[serde(default)] layer: Option<String> },
    AddSection { origin: P3, normal: P3, #[serde(default)] active: Option<bool>, #[serde(default)] layer: Option<String> },
    UpdateSection { id: String, #[serde(default)] origin: Option<P3>, #[serde(default)] normal: Option<P3>, #[serde(default)] active: Option<bool> },
    UpdateEntity { id: String, patch: Value },
    Measure { a: P3, b: P3 },
    SetExtension { plugin: String, data: Value },
    Custom { #[serde(rename = "type")] type_: String, #[serde(default)] data: Value },
    /// Arc as a polyline of edges: centre arc `{center,start,end}`, three-point `{through}` or
    /// two-point `{from,to,bulge}`; faces form through the loop detection like `draw_line`.
    DrawArc {
        #[serde(default)] center: Option<P3>, #[serde(default)] start: Option<P3>, #[serde(default)] end: Option<P3>,
        #[serde(default)] through: Option<Vec<P3>>,
        #[serde(default)] from: Option<P3>, #[serde(default)] to: Option<P3>, #[serde(default)] bulge: Option<f64>,
        #[serde(default)] normal: Option<P3>, #[serde(default)] segments: Option<usize>,
        #[serde(default)] pie: bool, #[serde(default, rename = "closeChord")] close_chord: bool,
        #[serde(default)] layer: Option<String>,
    },
    FollowMe { face: String, #[serde(default)] path: Option<Vec<P3>>, #[serde(default, rename = "pathEdges")] path_edges: Option<Vec<String>> },
    Mirror { ids: Vec<String>, origin: P3, normal: P3, #[serde(default)] copy: bool },
    MakeComponent { ids: Vec<String>, name: String, #[serde(default)] description: Option<String> },
    PlaceComponent { definition: String, position: P3, #[serde(default)] rotation: Option<f64>, #[serde(default)] scale: Option<Value>, #[serde(default)] layer: Option<String> },
    MakeUnique { instance: String },
    EditComponent { definition: String, ops: Vec<Op> },
    RenameComponent { definition: String, name: String },
    RemoveComponent { definition: String, #[serde(default)] force: bool },
}

impl Op {
    pub fn name(&self) -> &'static str {
        match self {
            Op::DrawLine { .. } => "draw_line", Op::DrawRectangle { .. } => "draw_rectangle", Op::DrawPolygon { .. } => "draw_polygon",
            Op::DrawCircle { .. } => "draw_circle", Op::PushPull { .. } => "push_pull", Op::Offset { .. } => "offset", Op::Move { .. } => "move",
            Op::Rotate { .. } => "rotate", Op::Scale { .. } => "scale", Op::Copy { .. } => "copy", Op::Erase { .. } => "erase", Op::Paint { .. } => "paint",
            Op::SetLayer { .. } => "set_layer", Op::MakeGroup { .. } => "make_group", Op::Explode { .. } => "explode", Op::AddLayer { .. } => "add_layer",
            Op::UpdateLayer { .. } => "update_layer", Op::RemoveLayer { .. } => "remove_layer", Op::AddMaterial { .. } => "add_material",
            Op::UpdateMaterial { .. } => "update_material", Op::AddGuide { .. } => "add_guide", Op::AddDimension { .. } => "add_dimension",
            Op::AddText { .. } => "add_text", Op::AddSection { .. } => "add_section", Op::UpdateSection { .. } => "update_section",
            Op::UpdateEntity { .. } => "update_entity", Op::Measure { .. } => "measure", Op::SetExtension { .. } => "set_extension", Op::Custom { .. } => "custom",
            Op::DrawArc { .. } => "draw_arc", Op::FollowMe { .. } => "follow_me", Op::Mirror { .. } => "mirror",
            Op::MakeComponent { .. } => "make_component", Op::PlaceComponent { .. } => "place_component", Op::MakeUnique { .. } => "make_unique",
            Op::EditComponent { .. } => "edit_component", Op::RenameComponent { .. } => "rename_component", Op::RemoveComponent { .. } => "remove_component",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpResult {
    #[serde(default)]
    pub created: Vec<String>,
    #[serde(default)]
    pub modified: Vec<String>,
    #[serde(default)]
    pub removed: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measurement: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<Entity>,
}

/// Apply several ops atomically: on any error the document is left untouched.
pub fn apply_many(doc: &mut Document, ops: &[Op], reg: &Registry) -> OpResult2<Vec<OpResult>> {
    let mut work = doc.clone();
    let mut results = Vec::with_capacity(ops.len());
    for op in ops {
        results.push(apply(&mut work, op, reg)?);
    }
    *doc = work;
    Ok(results)
}

pub fn apply(doc: &mut Document, op: &Op, reg: &Registry) -> OpResult2<OpResult> {
    doc.normalize();
    match op {
        Op::DrawLine { points, close, layer } => draw_line(doc, points, *close, layer.as_deref()),
        Op::DrawRectangle { corner, opposite, normal, u, layer, material } => draw_rectangle(doc, *corner, *opposite, *normal, *u, layer.as_deref(), material.as_deref()),
        Op::DrawPolygon { points, layer, material } => {
            let f = make_face(doc, points.clone(), vec![], layer.as_deref(), material.as_deref())?;
            Ok(push_created(doc, Entity::Face(f)))
        }
        Op::DrawCircle { center, normal, radius, segments, layer, material } => {
            if *radius <= 0.0 { return err("radius must be positive"); }
            let n = normalize(*normal);
            if len(n) < 0.5 { return err("normal must be non-zero"); }
            let segs = segments.unwrap_or(24).max(3);
            let (u, v) = plane_basis(n);
            let pts: Vec<P3> = (0..segs)
                .map(|i| {
                    let a = i as f64 / segs as f64 * std::f64::consts::TAU;
                    add(*center, add(scale(u, radius * a.cos()), scale(v, radius * a.sin())))
                })
                .collect();
            let pts = orient(pts, n);
            let f = make_face(doc, pts, vec![], layer.as_deref(), material.as_deref())?;
            Ok(push_created(doc, Entity::Face(f)))
        }
        Op::PushPull { face, distance } => push_pull(doc, split_key(face).0, *distance),
        Op::Offset { face, distance } => offset(doc, split_key(face).0, *distance),
        Op::Move { ids, delta } => transform_ids(doc, ids, &mat_translation(*delta), reg),
        Op::Rotate { ids, origin, axis, angle } => {
            if len(*axis) < EPS { return err("axis must be non-zero"); }
            transform_ids(doc, ids, &mat_about(&mat_rotation(*axis, angle.to_radians()), *origin), reg)
        }
        Op::Scale { ids, origin, factors } => {
            if factors.iter().any(|f| f.abs() < EPS) { return err("scale factors must be non-zero"); }
            transform_ids(doc, ids, &mat_about(&mat_scale(*factors), *origin), reg)
        }
        Op::Copy { ids, delta, count } => copy(doc, ids, *delta, count.unwrap_or(1).max(1), reg),
        Op::Erase { ids } => {
            let mut r = OpResult::default();
            for id in &bare_ids(ids) {
                match doc.take(id) {
                    Some(e) => { collect_ids(&e, &mut r.removed); }
                    None => return err(format!("no entity {id}")),
                }
            }
            Ok(r)
        }
        Op::Paint { ids, material, side } => paint(doc, ids, material, side.as_deref().unwrap_or("front"), reg),
        Op::SetLayer { ids, layer } => {
            let lid = resolve_layer(doc, Some(layer))?;
            let mut r = OpResult::default();
            for id in &bare_ids(ids) {
                let (e, _) = doc.find_mut(id).ok_or_else(|| OpError(format!("no entity {id}")))?;
                e.set_layer(lid.clone());
                r.modified.push(id.clone());
            }
            Ok(r)
        }
        Op::MakeGroup { ids, name } => make_group(doc, &bare_ids(ids), name.clone()),
        Op::Explode { id } => explode(doc, split_key(id).0),
        Op::AddLayer { name, color } => {
            let l = Layer { id: new_id(), name: name.clone(), visible: true, color: color.clone().unwrap_or_else(|| "#7a8aa0".into()) };
            let id = l.id.clone();
            doc.layers.push(l);
            Ok(OpResult { created: vec![id], ..Default::default() })
        }
        Op::UpdateLayer { id, name, visible, color } => {
            let lid = resolve_layer(doc, Some(id))?;
            let l = doc.layers.iter_mut().find(|l| l.id == lid).unwrap();
            if let Some(n) = name { l.name = n.clone(); }
            if let Some(v) = visible { l.visible = *v; }
            if let Some(c) = color { l.color = c.clone(); }
            Ok(OpResult { modified: vec![lid], ..Default::default() })
        }
        Op::RemoveLayer { id } => {
            let lid = resolve_layer(doc, Some(id))?;
            if doc.layers.len() <= 1 { return err("cannot remove the last layer"); }
            doc.layers.retain(|l| l.id != lid);
            let first = doc.layers[0].id.clone();
            let mut moved = vec![];
            fn walk(list: &mut [Entity], from: &str, to: &str, moved: &mut Vec<String>) {
                for e in list.iter_mut() {
                    if e.layer() == from { e.set_layer(to.to_string()); moved.push(e.id().to_string()); }
                    if let Entity::Group(g) = e { walk(&mut g.entities, from, to, moved); }
                }
            }
            walk(&mut doc.entities, &lid, &first, &mut moved);
            Ok(OpResult { removed: vec![lid], modified: moved, ..Default::default() })
        }
        Op::AddMaterial { name, color, opacity } => {
            if !is_hex_color(color) { return err("color must be a #rrggbb value"); }
            let m = Material { id: new_id(), name: name.clone(), color: color.clone(), opacity: opacity.unwrap_or(1.0).clamp(0.0, 1.0) };
            let id = m.id.clone();
            doc.materials.push(m);
            Ok(OpResult { created: vec![id], ..Default::default() })
        }
        Op::UpdateMaterial { id, name, color, opacity } => {
            let mid = doc.material_by_ref(id).map(|m| m.id.clone()).ok_or_else(|| OpError(format!("no material {id}")))?;
            let m = doc.materials.iter_mut().find(|m| m.id == mid).unwrap();
            if let Some(n) = name { m.name = n.clone(); }
            if let Some(c) = color { if !is_hex_color(c) { return err("color must be a #rrggbb value"); } m.color = c.clone(); }
            if let Some(o) = opacity { m.opacity = o.clamp(0.0, 1.0); }
            Ok(OpResult { modified: vec![mid], ..Default::default() })
        }
        Op::AddGuide { point, direction, layer } => {
            let layer = resolve_layer(doc, layer.as_deref())?;
            let dir = direction.map(normalize).filter(|d| len(*d) > 0.5);
            Ok(push_created(doc, Entity::Guide(Guide { id: new_id(), layer, point: *point, direction: dir, extra: Default::default() })))
        }
        Op::AddDimension { a, b, offset, layer } => {
            let layer = resolve_layer(doc, layer.as_deref())?;
            Ok(push_created(doc, Entity::Dimension(Dimension { id: new_id(), layer, a: *a, b: *b, offset: *offset, extra: Default::default() })))
        }
        Op::AddText { position, text, anchor, layer } => {
            let layer = resolve_layer(doc, layer.as_deref())?;
            Ok(push_created(doc, Entity::Text(Text { id: new_id(), layer, position: *position, text: text.clone(), anchor: *anchor, extra: Default::default() })))
        }
        Op::AddSection { origin, normal, active, layer } => {
            let layer = resolve_layer(doc, layer.as_deref())?;
            let n = normalize(*normal);
            if len(n) < 0.5 { return err("normal must be non-zero"); }
            let active = active.unwrap_or(true);
            if active { deactivate_sections(&mut doc.entities); }
            Ok(push_created(doc, Entity::Section(Section { id: new_id(), layer, origin: *origin, normal: n, active, extra: Default::default() })))
        }
        Op::UpdateSection { id, origin, normal, active } => {
            if active == &Some(true) { deactivate_sections(&mut doc.entities); }
            let (e, _) = doc.find_mut(id).ok_or_else(|| OpError(format!("no entity {id}")))?;
            let Entity::Section(s) = e else { return err(format!("{id} is not a section plane")) };
            if let Some(o) = origin { s.origin = *o; }
            if let Some(n) = normal { let n = normalize(*n); if len(n) < 0.5 { return err("normal must be non-zero"); } s.normal = n; }
            if let Some(a) = active { s.active = *a; }
            Ok(OpResult { modified: vec![id.clone()], ..Default::default() })
        }
        Op::UpdateEntity { id, patch } => update_entity(doc, split_key(id).0, patch),
        Op::Measure { a, b } => Ok(OpResult { measurement: Some(dist(*a, *b)), ..Default::default() }),
        Op::SetExtension { plugin, data } => {
            doc.extensions.insert(plugin.clone(), data.clone());
            Ok(OpResult::default())
        }
        Op::Custom { type_, data } => reg.apply(type_, data.clone(), doc),
        Op::DrawArc { center, start, end, through, from, to, bulge, normal, segments, pie, close_chord, layer } =>
            draw_arc(doc, &ArcSpec { center: *center, start: *start, end: *end, through: through.clone(), from: *from, to: *to, bulge: *bulge, normal: *normal, segments: *segments, pie: *pie, close_chord: *close_chord }, layer.as_deref()),
        Op::FollowMe { face, path, path_edges } => follow_me(doc, split_key(face).0, path.as_deref(), path_edges.as_deref()),
        Op::Mirror { ids, origin, normal, copy } => mirror(doc, ids, *origin, *normal, *copy, reg),
        Op::MakeComponent { ids, name, description } => make_component(doc, &bare_ids(ids), name, description.clone()),
        Op::PlaceComponent { definition, position, rotation, scale, layer } => place_component(doc, definition, *position, rotation.unwrap_or(0.0), scale.as_ref(), layer.as_deref()),
        Op::MakeUnique { instance } => make_unique(doc, split_key(instance).0),
        Op::EditComponent { definition, ops } => edit_component(doc, definition, ops, reg),
        Op::RenameComponent { definition, name } => {
            let c = doc.component_mut(definition).ok_or_else(|| OpError(format!("no component {definition}")))?;
            c.name = name.clone();
            Ok(OpResult { modified: vec![c.id.clone()], ..Default::default() })
        }
        Op::RemoveComponent { definition, force } => {
            let cid = doc.component(definition).map(|c| c.id.clone()).ok_or_else(|| OpError(format!("no component {definition}")))?;
            let inst = doc.instances_of(&cid);
            if !inst.is_empty() && !force { return err(format!("component has {} instance(s); pass force to erase them", inst.len())); }
            let mut r = OpResult::default();
            for id in inst { if let Some(e) = doc.take(&id) { collect_ids(&e, &mut r.removed); } }
            doc.components.retain(|c| c.id != cid);
            r.removed.push(cid);
            Ok(r)
        }
    }
}

// ---- helpers ----------------------------------------------------------------------

fn resolve_layer(doc: &Document, r: Option<&str>) -> OpResult2<String> {
    match r {
        None | Some("") => Ok(doc.default_layer().to_string()),
        Some(r) => doc.layer_by_ref(r).map(|l| l.id.clone()).ok_or_else(|| OpError(format!("no layer {r}"))),
    }
}

/// Resolve a material reference; a `#hex` colour creates a material on the fly.
fn resolve_material(doc: &mut Document, r: &str) -> OpResult2<String> {
    if let Some(m) = doc.material_by_ref(r) { return Ok(m.id.clone()); }
    if is_hex_color(r) {
        let m = Material { id: new_id(), name: r.to_uppercase(), color: r.to_uppercase(), opacity: 1.0 };
        let id = m.id.clone();
        doc.materials.push(m);
        return Ok(id);
    }
    err(format!("no material {r}"))
}

fn push_created(doc: &mut Document, e: Entity) -> OpResult {
    let id = e.id().to_string();
    let r = OpResult { created: vec![id], entities: vec![e.clone()], ..Default::default() };
    doc.entities.push(e);
    r
}

fn collect_ids(e: &Entity, out: &mut Vec<String>) {
    out.push(e.id().to_string());
    if let Entity::Group(g) = e { for c in &g.entities { collect_ids(c, out); } }
}

/// Reverse a loop when its Newell normal points against `n`.
fn orient(mut pts: Vec<P3>, n: P3) -> Vec<P3> {
    if dot(newell(&pts), n) < 0.0 { pts.reverse(); }
    pts
}

/// Drop consecutive duplicate points (and a trailing point equal to the first).
fn dedupe_loop(pts: &[P3]) -> Vec<P3> {
    let mut out: Vec<P3> = vec![];
    for p in pts {
        if out.last().map_or(true, |q| !same_point(*q, *p)) { out.push(*p); }
    }
    while out.len() > 1 && same_point(out[0], *out.last().unwrap()) { out.pop(); }
    out
}

fn make_face(doc: &Document, outer: Vec<P3>, holes: Vec<Vec<P3>>, layer: Option<&str>, material: Option<&str>) -> OpResult2<Face> {
    let outer = dedupe_loop(&outer);
    if outer.len() < 3 { return err("a face needs at least 3 distinct points"); }
    if !is_planar(&outer) { return err("face points are not coplanar (within 1 mm)"); }
    if area(&outer) < 1e-8 { return err("face is degenerate (zero area)"); }
    let n = normal(&outer);
    let holes: Vec<Vec<P3>> = holes
        .into_iter()
        .map(|h| {
            let h = dedupe_loop(&h);
            if h.len() < 3 || plane_deviation(&h, outer[0], n) > PLANE_TOL { return err("hole is not a coplanar loop"); }
            // holes wind opposite to the outer loop
            Ok(orient(h, scale(n, -1.0)))
        })
        .collect::<OpResult2<_>>()?;
    let layer = resolve_layer(doc, layer)?;
    let material = match material {
        None => None,
        Some(m) => Some(doc.material_by_ref(m).map(|m| m.id.clone()).ok_or_else(|| OpError(format!("no material {m}")))?),
    };
    Ok(Face { id: new_id(), layer, outer, holes, material, back_material: None, extra: Default::default() })
}

fn deactivate_sections(list: &mut [Entity]) {
    for e in list.iter_mut() {
        match e {
            Entity::Section(s) => s.active = false,
            Entity::Group(g) => deactivate_sections(&mut g.entities),
            _ => {}
        }
    }
}

// ---- draw_line with face-from-loop -------------------------------------------------

fn key(p: P3) -> (i64, i64, i64) {
    let q = |v: f64| (v / VERTEX_TOL).round() as i64;
    (q(p[0]), q(p[1]), q(p[2]))
}

fn draw_line(doc: &mut Document, points: &[P3], close: bool, layer: Option<&str>) -> OpResult2<OpResult> {
    let pts = dedupe_loop(points);
    if pts.len() < 2 { return err("draw_line needs at least 2 distinct points"); }
    let layer = resolve_layer(doc, layer)?;
    let mut r = OpResult::default();
    let mut new_edges: Vec<Edge> = vec![];
    for w in pts.windows(2) {
        new_edges.push(Edge { id: new_id(), layer: layer.clone(), a: w[0], b: w[1], extra: Default::default() });
    }
    if close && pts.len() >= 3 {
        new_edges.push(Edge { id: new_id(), layer: layer.clone(), a: *pts.last().unwrap(), b: pts[0], extra: Default::default() });
    }
    for e in new_edges.iter() {
        r.created.push(e.id.clone());
        r.entities.push(Entity::Edge(e.clone()));
        doc.entities.push(Entity::Edge(e.clone()));
    }
    // face from loop: for each new edge, look for the shortest cycle through it among loose top-level edges
    let existing_faces: HashSet<Vec<(i64, i64, i64)>> = doc
        .entities
        .iter()
        .filter_map(|e| if let Entity::Face(f) = e { Some(face_key(&f.outer)) } else { None })
        .collect();
    let mut made: HashSet<Vec<(i64, i64, i64)>> = HashSet::new();
    for ne in &new_edges {
        if let Some(cycle) = find_loop(&doc.entities, ne) {
            let k = face_key(&cycle);
            if existing_faces.contains(&k) || made.contains(&k) { continue; }
            let n = normal(&cycle);
            // prefer faces that look up / toward the viewer
            let up = if n[1].abs() > 1e-6 { [0.0, 1.0, 0.0] } else if n[2].abs() > 1e-6 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
            let cycle = orient(cycle, up);
            if let Ok(f) = make_face(doc, cycle, vec![], Some(&layer), None) {
                made.insert(k);
                r.created.push(f.id.clone());
                r.entities.push(Entity::Face(f.clone()));
                doc.entities.push(Entity::Face(f));
            }
        }
    }
    Ok(r)
}

fn face_key(pts: &[P3]) -> Vec<(i64, i64, i64)> {
    let mut k: Vec<_> = pts.iter().map(|p| key(*p)).collect();
    k.sort();
    k
}

/// Shortest cycle through `edge` over the loose edges of `list`, returned as an ordered point loop if coplanar.
fn find_loop(list: &[Entity], edge: &Edge) -> Option<Vec<P3>> {
    let edges: Vec<&Edge> = list.iter().filter_map(|e| if let Entity::Edge(e) = e { Some(e) } else { None }).collect();
    let mut adj: HashMap<(i64, i64, i64), Vec<((i64, i64, i64), P3, String)>> = HashMap::new();
    for e in &edges {
        if e.id == edge.id { continue; }
        adj.entry(key(e.a)).or_default().push((key(e.b), e.b, e.id.clone()));
        adj.entry(key(e.b)).or_default().push((key(e.a), e.a, e.id.clone()));
    }
    let start = key(edge.b);
    let goal = key(edge.a);
    // BFS from edge.b to edge.a over the other edges
    let mut points: HashMap<(i64, i64, i64), P3> = HashMap::new();
    points.insert(start, edge.b);
    points.insert(goal, edge.a);
    let mut prev: HashMap<(i64, i64, i64), (i64, i64, i64)> = HashMap::new();
    let mut seen: HashSet<(i64, i64, i64)> = HashSet::new();
    let mut q = VecDeque::new();
    seen.insert(start);
    q.push_back(start);
    let mut found = false;
    while let Some(cur) = q.pop_front() {
        if cur == goal { found = true; break; }
        if let Some(nb) = adj.get(&cur) {
            for (k, p, _) in nb {
                if seen.insert(*k) {
                    points.entry(*k).or_insert(*p);
                    prev.insert(*k, cur);
                    q.push_back(*k);
                }
            }
        }
    }
    if !found { return None; }
    // walk back goal → start, collecting vertices
    let mut path = vec![];
    let mut cur = goal;
    loop {
        path.push(points[&cur]);
        if cur == start { break; }
        cur = prev[&cur];
    }
    // path: a → ... → b ; cycle closes with the new edge b→a
    path.reverse();
    let cycle = dedupe_loop(&path);
    if cycle.len() < 3 || !is_planar(&cycle) || area(&cycle) < 1e-8 { return None; }
    Some(cycle)
}

fn draw_rectangle(doc: &mut Document, corner: P3, opposite: P3, normal_in: P3, u_in: Option<P3>, layer: Option<&str>, material: Option<&str>) -> OpResult2<OpResult> {
    let n = normalize(normal_in);
    if len(n) < 0.5 { return err("normal must be non-zero"); }
    let project = |d: P3| sub(d, scale(n, dot(d, n)));
    let mut u = u_in.map(project).map(normalize).filter(|u| len(*u) > 0.5);
    if u.is_none() { u = Some(normalize(project([1.0, 0.0, 0.0]))).filter(|u| len(*u) > 0.5); }
    if u.is_none() { u = Some(normalize(project([0.0, 0.0, 1.0]))).filter(|u| len(*u) > 0.5); }
    let u = u.ok_or_else(|| OpError("cannot derive a side direction".into()))?;
    let v = cross(n, u);
    let d = sub(opposite, corner);
    let du = dot(d, u);
    let dv = dot(d, v);
    if du.abs() < 1e-6 || dv.abs() < 1e-6 { return err("rectangle is degenerate"); }
    let pts = vec![corner, add(corner, scale(u, du)), add(add(corner, scale(u, du)), scale(v, dv)), add(corner, scale(v, dv))];
    let f = make_face(doc, orient(pts, n), vec![], layer, material)?;
    Ok(push_created(doc, Entity::Face(f)))
}

// ---- push/pull & offset ------------------------------------------------------------

fn push_pull(doc: &mut Document, face_id: &str, distance: f64) -> OpResult2<OpResult> {
    if distance.abs() < 1e-9 { return err("distance must be non-zero"); }
    let (list, _) = doc.container_mut(face_id).ok_or_else(|| OpError(format!("no entity {face_id}")))?;
    let idx = list.iter().position(|e| e.id() == face_id).unwrap();
    let Entity::Face(orig) = list[idx].clone() else { return err(format!("{face_id} is not a face")) };
    let n = normal(&orig.outer);
    let d = scale(n, distance);
    let shift = |l: &[P3]| l.iter().map(|p| add(*p, d)).collect::<Vec<_>>();
    let rev = |l: &[P3]| l.iter().rev().copied().collect::<Vec<_>>();
    let mut r = OpResult::default();
    let mut new_entities: Vec<Entity> = vec![];
    // base face at the original position
    let (base_outer, base_holes): (Vec<P3>, Vec<Vec<P3>>) = if distance > 0.0 {
        (rev(&orig.outer), orig.holes.iter().map(|h| rev(h)).collect())
    } else {
        (orig.outer.clone(), orig.holes.clone())
    };
    let base = Face { id: new_id(), layer: orig.layer.clone(), outer: base_outer, holes: base_holes, material: orig.material.clone(), back_material: orig.back_material.clone(), extra: Default::default() };
    r.created.push(base.id.clone());
    new_entities.push(Entity::Face(base));
    // sides
    let mut loops: Vec<&Vec<P3>> = vec![&orig.outer];
    loops.extend(orig.holes.iter());
    for lp in loops {
        let k = lp.len();
        for i in 0..k {
            let a = lp[i];
            let b = lp[(i + 1) % k];
            let mut quad = vec![a, b, add(b, d), add(a, d)];
            if distance < 0.0 { quad.reverse(); }
            let f = Face { id: new_id(), layer: orig.layer.clone(), outer: quad, holes: vec![], material: orig.material.clone(), back_material: orig.back_material.clone(), extra: Default::default() };
            r.created.push(f.id.clone());
            new_entities.push(Entity::Face(f));
        }
    }
    // the cap keeps the id and moves
    let (cap_outer, cap_holes) = if distance > 0.0 {
        (shift(&orig.outer), orig.holes.iter().map(|h| shift(h)).collect::<Vec<_>>())
    } else {
        (rev(&shift(&orig.outer)), orig.holes.iter().map(|h| rev(&shift(h))).collect())
    };
    let cap = Face { outer: cap_outer, holes: cap_holes, ..orig.clone() };
    list[idx] = Entity::Face(cap.clone());
    r.modified.push(face_id.to_string());
    r.entities.push(Entity::Face(cap));
    for e in new_entities {
        r.entities.push(e.clone());
        list.push(e);
    }
    Ok(r)
}

fn offset(doc: &mut Document, face_id: &str, distance: f64) -> OpResult2<OpResult> {
    if distance.abs() < 1e-9 { return err("distance must be non-zero"); }
    let (list, _) = doc.container_mut(face_id).ok_or_else(|| OpError(format!("no entity {face_id}")))?;
    let Some(Entity::Face(orig)) = list.iter().find(|e| e.id() == face_id).cloned() else { return err(format!("{face_id} is not a face")) };
    let outer = offset_polygon(&orig.outer, distance);
    if !is_planar(&outer) || area(&outer) < 1e-8 { return err("offset collapses the face"); }
    let holes: Vec<Vec<P3>> = orig.holes.iter().map(|h| offset_polygon(h, -distance)).filter(|h| area(h) > 1e-8).collect();
    let f = Face { id: new_id(), layer: orig.layer.clone(), outer, holes, material: orig.material.clone(), back_material: orig.back_material.clone(), extra: Default::default() };
    let r = OpResult { created: vec![f.id.clone()], entities: vec![Entity::Face(f.clone())], ..Default::default() };
    list.push(Entity::Face(f));
    Ok(r)
}

// ---- transforms, copy, groups --------------------------------------------------------

fn transform_ids(doc: &mut Document, ids: &[String], world: &Mat4, reg: &Registry) -> OpResult2<OpResult> {
    let mut r = OpResult::default();
    let snapshot = doc.clone();
    for id in &bare_ids(ids) {
        let (e, parent) = doc.find_mut(id).ok_or_else(|| OpError(format!("no entity {id}")))?;
        let local = if mat_is_identity(&parent) { *world } else { mat_mul(&mat_inverse(&parent), &mat_mul(world, &parent)) };
        if let Entity::Custom(c) = e {
            reg.transform(c, &local, &snapshot)?;
        } else {
            e.transform(&local);
        }
        r.modified.push(id.clone());
        r.entities.push(e.clone());
    }
    Ok(r)
}

fn reassign_ids(e: &mut Entity) {
    e.set_id(new_id());
    if let Entity::Group(g) = e { for c in g.entities.iter_mut() { reassign_ids(c); } }
}

fn copy(doc: &mut Document, ids: &[String], delta: P3, count: usize, reg: &Registry) -> OpResult2<OpResult> {
    let mut r = OpResult::default();
    let snapshot = doc.clone();
    for id in &bare_ids(ids) {
        let (list, parent) = doc.container_mut(id).ok_or_else(|| OpError(format!("no entity {id}")))?;
        let src = list.iter().find(|e| e.id() == id).unwrap().clone();
        for i in 1..=count {
            let mut c = src.clone();
            reassign_ids(&mut c);
            let world = mat_translation(scale(delta, i as f64));
            let local = if mat_is_identity(&parent) { world } else { mat_mul(&mat_inverse(&parent), &mat_mul(&world, &parent)) };
            if let Entity::Custom(cu) = &mut c {
                reg.transform(cu, &local, &snapshot)?;
            } else {
                c.transform(&local);
            }
            r.created.push(c.id().to_string());
            r.entities.push(c.clone());
            list.push(c);
        }
    }
    Ok(r)
}

fn make_group(doc: &mut Document, ids: &[String], name: Option<String>) -> OpResult2<OpResult> {
    if ids.is_empty() { return err("make_group needs at least one entity"); }
    let first = &ids[0];
    let layer = doc.find(first).map(|e| e.layer().to_string()).ok_or_else(|| OpError(format!("no entity {first}")))?;
    let mut children = vec![];
    for id in ids {
        children.push(doc.take(id).ok_or_else(|| OpError(format!("no entity {id}")))?);
    }
    let g = Group { id: new_id(), layer, name: name.unwrap_or_else(|| "Group".into()), transform: mat_identity(), entities: children, extra: Default::default() };
    let r = OpResult { created: vec![g.id.clone()], modified: ids.to_vec(), entities: vec![Entity::Group(g.clone())], ..Default::default() };
    // the group lives at the top level (children were taken in their own container's coordinates;
    // grouping across nested containers is not supported — see README)
    doc.entities.push(Entity::Group(g));
    Ok(r)
}

fn explode(doc: &mut Document, id: &str) -> OpResult2<OpResult> {
    let (list, _) = doc.container_mut(id).ok_or_else(|| OpError(format!("no entity {id}")))?;
    let idx = list.iter().position(|e| e.id() == id).unwrap();
    match list.remove(idx) {
        Entity::Group(g) => {
            let mut r = OpResult { removed: vec![g.id.clone()], ..Default::default() };
            for mut c in g.entities {
                c.transform(&g.transform);
                r.modified.push(c.id().to_string());
                r.entities.push(c.clone());
                list.push(c);
            }
            Ok(r)
        }
        Entity::Instance(i) => {
            // copies of the definition's entities in world coordinates, with fresh ids
            let mut r = OpResult { removed: vec![i.id.clone()], ..Default::default() };
            let def_entities = doc.component(&i.definition).map(|d| d.entities.clone()).ok_or_else(|| OpError(format!("no component {}", i.definition)))?;
            let flip = mat_det3(&i.transform) < 0.0;
            let list: &mut Vec<Entity> = match doc.container_mut(id) { Some((l, _)) => l, None => &mut doc.entities };
            for mut c in def_entities {
                reassign_ids(&mut c);
                c.transform(&i.transform);
                if flip { c.reverse_winding(); }
                c.set_layer(i.layer.clone());
                r.created.push(c.id().to_string());
                r.entities.push(c.clone());
                list.push(c);
            }
            Ok(r)
        }
        other => { list.insert(idx, other); err(format!("{id} is not a group or component instance")) }
    }
}

fn paint(doc: &mut Document, ids: &[String], material: &str, side: &str, reg: &Registry) -> OpResult2<OpResult> {
    let mid = resolve_material(doc, material)?;
    let mut r = OpResult::default();
    fn paint_entity(e: &mut Entity, mid: &str, side: &str, out: &mut Vec<String>) {
        match e {
            Entity::Face(f) => {
                if side == "front" || side == "both" { f.material = Some(mid.to_string()); }
                if side == "back" || side == "both" { f.back_material = Some(mid.to_string()); }
                out.push(f.id.clone());
            }
            Entity::Group(g) => { for c in g.entities.iter_mut() { paint_entity(c, mid, side, out); } }
            _ => {}
        }
    }
    for id in ids {
        let (bare, key) = split_key(id);
        let (e, _) = doc.find_mut(bare).ok_or_else(|| OpError(format!("no entity {bare}")))?;
        match e {
            Entity::Custom(c) => {
                reg.paint(c, key.unwrap_or(""), &mid, Side::parse(side))?;
                r.modified.push(bare.to_string());
            }
            Entity::Instance(i) => {
                // painting through an instance edits the shared definition (every instance changes)
                let def_id = i.definition.clone();
                let def = doc.component_mut(&def_id).ok_or_else(|| OpError(format!("no component {def_id}")))?;
                match key {
                    Some(fid) => {
                        let (fid, _) = split_key(fid);
                        let target = find_in_mut(&mut def.entities, fid).ok_or_else(|| OpError(format!("no entity {fid} in component")))?;
                        paint_entity(target, &mid, side, &mut r.modified);
                    }
                    None => for e in def.entities.iter_mut() { paint_entity(e, &mid, side, &mut r.modified); },
                }
                r.modified.push(bare.to_string());
            }
            _ => paint_entity(e, &mid, side, &mut r.modified),
        }
    }
    Ok(r)
}

fn find_in_mut<'a>(list: &'a mut [Entity], id: &str) -> Option<&'a mut Entity> {
    for e in list.iter_mut() {
        if e.id() == id { return Some(e); }
        if let Entity::Group(g) = e { if let Some(x) = find_in_mut(&mut g.entities, id) { return Some(x); } }
    }
    None
}

fn update_entity(doc: &mut Document, id: &str, patch: &Value) -> OpResult2<OpResult> {
    let Some(obj) = patch.as_object() else { return err("patch must be an object") };
    let (e, _) = doc.find_mut(id).ok_or_else(|| OpError(format!("no entity {id}")))?;
    let mut v = serde_json::to_value(&*e).map_err(|x| OpError(x.to_string()))?;
    let map = v.as_object_mut().unwrap();
    for (k, val) in obj {
        if k == "id" || k == "kind" { continue; }
        map.insert(k.clone(), val.clone());
    }
    let mut updated: Entity = serde_json::from_value(v).map_err(|x| OpError(format!("invalid patch: {x}")))?;
    if let Entity::Face(f) = &mut updated {
        f.outer = dedupe_loop(&f.outer);
        if f.outer.len() < 3 || !is_planar(&f.outer) || area(&f.outer) < 1e-8 { return err("patched face is not a valid planar polygon"); }
    }
    if let Entity::Section(s) = &mut updated { s.normal = normalize(s.normal); if len(s.normal) < 0.5 { return err("normal must be non-zero"); } }
    *e = updated.clone();
    Ok(OpResult { modified: vec![id.to_string()], entities: vec![updated], ..Default::default() })
}


// ---- arcs ---------------------------------------------------------------------------

pub struct ArcSpec {
    pub center: Option<P3>, pub start: Option<P3>, pub end: Option<P3>,
    pub through: Option<Vec<P3>>,
    pub from: Option<P3>, pub to: Option<P3>, pub bulge: Option<f64>,
    pub normal: Option<P3>, pub segments: Option<usize>, pub pie: bool, pub close_chord: bool,
}

fn arc_segments(given: Option<usize>, sweep: f64) -> usize {
    match given {
        Some(n) => n.clamp(1, 200),
        None => ((12.0 * sweep / std::f64::consts::FRAC_PI_2).ceil() as usize).clamp(1, 200),
    }
}

/// Points of a counter-clockwise arc about `n` from `start` by `sweep` radians.
fn arc_points(center: P3, start: P3, n: P3, sweep: f64, segs: usize) -> Vec<P3> {
    let r = dist(start, center);
    let u = normalize(sub(start, center));
    let v = normalize(cross(n, u));
    (0..=segs).map(|i| { let a = sweep * i as f64 / segs as f64; add(center, add(scale(u, r * a.cos()), scale(v, r * a.sin()))) }).collect()
}

/// Counter-clockwise angle about `n` from direction `a` to direction `b`, in (0, 2π].
fn ccw_angle(a: P3, b: P3, n: P3) -> f64 {
    let u = normalize(a);
    let v = normalize(cross(n, u));
    let ang = dot(b, v).atan2(dot(b, u));
    if ang <= 1e-12 { ang + std::f64::consts::TAU } else { ang }
}

fn draw_arc(doc: &mut Document, a: &ArcSpec, layer: Option<&str>) -> OpResult2<OpResult> {
    let pick_normal = |given: Option<P3>, fallback: P3| -> OpResult2<P3> {
        let n = normalize(given.unwrap_or(fallback));
        if len(n) < 0.5 { err("cannot derive the arc plane: pass `normal`") } else { Ok(n) }
    };
    // resolve every variant to (center, normal, start, sweep, extra radii / chord)
    let (center, n, start, sweep, radii, chord): (P3, P3, P3, f64, bool, Option<(P3, P3)>) = if let Some(through) = &a.through {
        if through.len() != 3 { return err("`through` needs exactly three points"); }
        let (p1, p2, p3) = (through[0], through[1], through[2]);
        let c = circumcenter(p1, p2, p3).ok_or_else(|| OpError("the three points are collinear".into()))?;
        let mut n = pick_normal(a.normal, cross(sub(p2, p1), sub(p3, p1)))?;
        // the arc must pass p2 on its way from p1 to p3: pick the orientation where p2 comes first
        if ccw_angle(sub(p1, c), sub(p2, c), n) > ccw_angle(sub(p1, c), sub(p3, c), n) { n = scale(n, -1.0); }
        let sweep = ccw_angle(sub(p1, c), sub(p3, c), n);
        (c, n, p1, sweep, false, None)
    } else if let (Some(from), Some(to)) = (a.from, a.to) {
        let bulge = a.bulge.unwrap_or(0.0);
        let chord = sub(to, from);
        let l = len(chord);
        if l < VERTEX_TOL { return err("`from` and `to` coincide"); }
        let n = pick_normal(a.normal, if chord[1].abs() < 0.999 * l { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] })?;
        if bulge.abs() < 1e-9 {
            // a straight segment
            let mut r = draw_line(doc, &[from, to], false, layer)?;
            r.measurement = Some(l);
            return Ok(r);
        }
        let w = normalize(cross(n, normalize(chord)));
        let mid = lerp(from, to, 0.5);
        let apex = add(mid, scale(w, bulge));
        let c = circumcenter(from, apex, to).ok_or_else(|| OpError("degenerate arc".into()))?;
        let mut nn = n;
        if ccw_angle(sub(from, c), sub(apex, c), nn) > ccw_angle(sub(from, c), sub(to, c), nn) { nn = scale(nn, -1.0); }
        let sweep = ccw_angle(sub(from, c), sub(to, c), nn);
        (c, nn, from, sweep, false, if a.close_chord { Some((to, from)) } else { None })
    } else if let (Some(c), Some(start), Some(end)) = (a.center, a.start, a.end) {
        let rs = sub(start, c);
        if len(rs) < VERTEX_TOL { return err("`start` coincides with `center`"); }
        let re = sub(end, c);
        let n = pick_normal(a.normal, { let x = cross(rs, re); if len(x) > 1e-9 { x } else if rs[1].abs() < 0.999 * len(rs) { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] } })?;
        if cross(rs, n).iter().all(|v| v.abs() < 1e-9) { return err("`start` lies on the normal through `center`"); }
        let re = sub(re, scale(n, dot(re, n)));
        if len(re) < VERTEX_TOL { return err("`end` lies on the normal through `center`"); }
        let sweep = ccw_angle(rs, re, n);
        (c, n, start, sweep, a.pie, None)
    } else {
        return err("draw_arc needs `center`+`start`+`end`, `through` (3 points) or `from`+`to`+`bulge`");
    };
    let segs = arc_segments(a.segments, sweep);
    let pts = arc_points(center, start, n, sweep, segs);
    let radius = dist(start, center);
    let mut r = if radii {
        // pie: centre → arc → centre, closed, so the sector face forms
        let mut poly = vec![center];
        poly.extend(pts.iter().copied());
        draw_line(doc, &poly, true, layer)?
    } else {
        draw_line(doc, &pts, false, layer)?
    };
    if let Some((a2, b2)) = chord {
        let r2 = draw_line(doc, &[a2, b2], false, layer)?;
        r.created.extend(r2.created);
        r.entities.extend(r2.entities);
    }
    // tag the arc edges (both ends on the circle) so tools can recognise them later
    let meta = serde_json::json!({ "center": center, "radius": radius, "normal": n });
    let on_circle = |p: P3| (dist(p, center) - radius).abs() < 1e-6;
    for id in r.created.clone() {
        if let Some((Entity::Edge(e), _)) = doc.find_mut(&id) {
            if on_circle(e.a) && on_circle(e.b) { e.extra.insert("arc".into(), meta.clone()); }
        }
    }
    r.entities = r.created.iter().filter_map(|id| doc.find(id).cloned()).collect();
    r.measurement = Some(radius);
    Ok(r)
}

// ---- follow me ------------------------------------------------------------------------

fn follow_me(doc: &mut Document, face_id: &str, path: Option<&[P3]>, path_edges: Option<&[String]>) -> OpResult2<OpResult> {
    let Some(Entity::Face(orig)) = doc.find(face_id).cloned() else { return err(format!("no face {face_id}")) };
    let centroid = {
        let mut c = [0.0; 3];
        for p in &orig.outer { c = add(c, *p); }
        scale(c, 1.0 / orig.outer.len() as f64)
    };
    // the path: given points, or a chain of edges starting at the end nearest the face
    let mut pts: Vec<P3> = match (path, path_edges) {
        (Some(p), _) => dedupe_loop_keep_end(p),
        (None, Some(ids)) => chain_edges(doc, ids, centroid)?,
        (None, None) => return err("follow_me needs `path` or `pathEdges`"),
    };
    if pts.len() < 2 { return err("the path needs at least 2 distinct points"); }
    let closed = pts.len() >= 3 && same_point(pts[0], *pts.last().unwrap());
    if closed { pts.pop(); if pts.len() < 3 { return err("a closed path needs at least 3 distinct points"); } }
    // relative path: stations are the face centroid plus the path offsets
    let origin = pts[0];
    let stations: Vec<P3> = pts.iter().map(|p| add(centroid, sub(*p, origin))).collect();
    let k = stations.len();
    let dirs: Vec<P3> = (0..k).filter(|i| closed || *i + 1 < k).map(|i| normalize(sub(stations[(i + 1) % k], stations[i]))).collect();
    if dirs.iter().any(|d| len(*d) < 0.5) { return err("the path has a zero-length segment"); }
    // align the profile so the first segment leaves it perpendicularly
    let mut n0 = normal(&orig.outer);
    let d0 = dirs[0];
    let mut loops: Vec<Vec<P3>> = std::iter::once(orig.outer.clone()).chain(orig.holes.iter().cloned()).collect();
    if dot(n0, d0) < 0.0 { n0 = scale(n0, -1.0); }
    let c = cross(n0, d0);
    if len(c) > 1e-6 || dot(n0, d0) < 0.0 {
        let angle = dot(n0, d0).clamp(-1.0, 1.0).acos();
        let axis = if len(c) > 1e-6 { c } else { plane_basis(n0).0 };
        let m = mat_about(&mat_rotation(axis, angle), centroid);
        for lp in loops.iter_mut() { for p in lp.iter_mut() { *p = mat_apply(&m, *p); } }
    }
    // project a ring along `dir` onto the plane through `o` with normal `pn`
    let project = |ring: &[P3], dir: P3, o: P3, pn: P3| -> OpResult2<Vec<P3>> {
        let den = dot(dir, pn);
        if den.abs() < 1e-9 { return err("the path turns back on itself"); }
        Ok(ring.iter().map(|q| { let t = dot(sub(o, *q), pn) / den; add(*q, scale(dir, t)) }).collect())
    };
    let mitre = |i: usize| -> P3 {
        let prev = dirs[(i + dirs.len() - 1) % dirs.len()];
        let next = dirs[i % dirs.len()];
        let m = add(prev, next);
        if len(m) < 1e-6 { next } else { normalize(m) }
    };
    // station rings
    let mut rings: Vec<Vec<Vec<P3>>> = vec![];
    let first: Vec<Vec<P3>> = if closed {
        loops.iter().map(|lp| project(lp, d0, stations[0], mitre(0))).collect::<OpResult2<_>>()?
    } else { loops.clone() };
    rings.push(first);
    let last_station = if closed { k } else { k - 1 };
    for i in 1..=last_station {
        let prev_dir = dirs[i - 1];
        let (o, pn) = if closed { (stations[i % k], mitre(i % k)) } else if i < k - 1 { (stations[i], mitre(i)) } else { (stations[i], prev_dir) };
        let prev_rings = rings.last().unwrap().clone();
        let next: Vec<Vec<P3>> = prev_rings.iter().map(|rg| project(rg, prev_dir, o, pn)).collect::<OpResult2<_>>()?;
        rings.push(next);
    }
    // faces
    let mut r = OpResult { modified: vec![face_id.to_string()], ..Default::default() };
    let mut made: Vec<Entity> = vec![];
    let mk = |outer: Vec<P3>| Face { id: new_id(), layer: orig.layer.clone(), outer, holes: vec![], material: orig.material.clone(), back_material: orig.back_material.clone(), extra: Default::default() };
    for s in 0..rings.len() - 1 {
        for (li, lp) in rings[s].iter().enumerate() {
            let nx = &rings[s + 1][li];
            let m = lp.len();
            let lc = { let mut c = [0.0; 3]; for p in lp { c = add(c, *p); } scale(c, 1.0 / m as f64) };
            for j in 0..m {
                let (a, b) = (lp[j], lp[(j + 1) % m]);
                let (c2, d2) = (nx[(j + 1) % m], nx[j]);
                let mut quad = vec![a, b, c2, d2];
                if area(&quad) < 1e-10 { continue; }
                let out = sub(lerp(a, b, 0.5), lc);
                let want_out = if li == 0 { 1.0 } else { -1.0 };
                if dot(normal(&quad), out) * want_out < 0.0 { quad.reverse(); }
                made.push(Entity::Face(mk(quad)));
            }
        }
    }
    let end_dir = *dirs.last().unwrap();
    if closed {
        if let Some(e) = doc.take(face_id) { collect_ids(&e, &mut r.removed); r.modified.clear(); }
    } else {
        // start cap: the original face, now pointing backwards
        let back = scale(d0, -1.0);
        let outer = orient(rings[0][0].clone(), back);
        let holes: Vec<Vec<P3>> = rings[0][1..].iter().map(|h| orient(h.clone(), d0)).collect();
        if let Some((Entity::Face(f), _)) = doc.find_mut(face_id) { f.outer = outer; f.holes = holes; r.entities.push(Entity::Face(f.clone())); }
        // end cap
        let last = rings.last().unwrap();
        let mut cap = mk(orient(last[0].clone(), end_dir));
        cap.holes = last[1..].iter().map(|h| orient(h.clone(), back)).collect();
        made.push(Entity::Face(cap));
    }
    let list: &mut Vec<Entity> = match doc.container_mut(face_id) { Some((l, _)) => l, None => &mut doc.entities };
    for e in made { r.created.push(e.id().to_string()); r.entities.push(e.clone()); list.push(e); }
    Ok(r)
}

fn dedupe_loop_keep_end(pts: &[P3]) -> Vec<P3> {
    let mut out: Vec<P3> = vec![];
    for p in pts { if out.last().map_or(true, |q| !same_point(*q, *p)) { out.push(*p); } }
    out
}

/// Order edges into one polyline, starting at the free end nearest `near`.
fn chain_edges(doc: &Document, ids: &[String], near: P3) -> OpResult2<Vec<P3>> {
    let mut segs: Vec<(P3, P3)> = vec![];
    for id in ids {
        match doc.find(split_key(id).0) { Some(Entity::Edge(e)) => segs.push((e.a, e.b)), _ => return err(format!("{id} is not an edge")) }
    }
    if segs.is_empty() { return err("pathEdges is empty"); }
    let mut ends: Vec<P3> = vec![];
    for (a, b) in &segs { for p in [a, b] { if !ends.iter().any(|q| same_point(*q, *p)) { ends.push(*p); } } }
    let degree = |p: P3| segs.iter().filter(|(a, b)| same_point(*a, p) || same_point(*b, p)).count();
    let mut starts: Vec<P3> = ends.iter().copied().filter(|p| degree(*p) == 1).collect();
    if starts.is_empty() { starts = ends.clone(); } // closed loop: start anywhere
    starts.sort_by(|a, b| dist(*a, near).partial_cmp(&dist(*b, near)).unwrap());
    let mut cur = starts[0];
    let mut path = vec![cur];
    let mut left = segs;
    while let Some(i) = left.iter().position(|(a, b)| same_point(*a, cur) || same_point(*b, cur)) {
        let (a, b) = left.remove(i);
        cur = if same_point(a, cur) { b } else { a };
        path.push(cur);
    }
    if !left.is_empty() { return err("pathEdges do not form one connected chain"); }
    Ok(path)
}

// ---- mirror ----------------------------------------------------------------------------

fn mirror(doc: &mut Document, ids: &[String], origin: P3, normal_in: P3, copy: bool, reg: &Registry) -> OpResult2<OpResult> {
    let n = normalize(normal_in);
    if len(n) < 0.5 { return err("normal must be non-zero"); }
    let world = mat_reflection(origin, n);
    let mut r = OpResult::default();
    let snapshot = doc.clone();
    for id in &bare_ids(ids) {
        let (list, parent) = doc.container_mut(id).ok_or_else(|| OpError(format!("no entity {id}")))?;
        let local = if mat_is_identity(&parent) { world } else { mat_mul(&mat_inverse(&parent), &mat_mul(&world, &parent)) };
        let idx = list.iter().position(|e| e.id() == id).unwrap();
        let mut target = if copy { let mut c = list[idx].clone(); reassign_ids(&mut c); c } else { list[idx].clone() };
        match &mut target {
            Entity::Custom(c) => reg.transform(c, &local, &snapshot)?,
            e => { e.transform(&local); e.reverse_winding(); }
        }
        if copy { r.created.push(target.id().to_string()); r.entities.push(target.clone()); list.push(target); }
        else { r.modified.push(id.clone()); r.entities.push(target.clone()); list[idx] = target; }
    }
    Ok(r)
}

// ---- components ------------------------------------------------------------------------

/// Axis-aligned bounds of entities (through groups and instances).
fn grow_bb(bb: &mut Option<(P3, P3)>, p: P3) {
    *bb = Some(match *bb { None => (p, p), Some((lo, hi)) => ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])]) });
}
fn bounds_of(doc: &Document, list: &[Entity], m: &Mat4, bb: &mut Option<(P3, P3)>) {
    for e in list {
        match e {
            Entity::Face(f) => { for p in f.outer.iter().chain(f.holes.iter().flatten()) { grow_bb(bb, mat_apply(m, *p)); } }
            Entity::Edge(ed) => { grow_bb(bb, mat_apply(m, ed.a)); grow_bb(bb, mat_apply(m, ed.b)); }
            Entity::Group(g) => bounds_of(doc, &g.entities, &mat_mul(m, &g.transform), bb),
            Entity::Instance(i) => { if let Some(d) = doc.component(&i.definition) { let im = mat_mul(m, &i.transform); let ents = d.entities.clone(); bounds_of(doc, &ents, &im, bb); } }
            Entity::Guide(g) => grow_bb(bb, mat_apply(m, g.point)),
            Entity::Dimension(d) => { grow_bb(bb, mat_apply(m, d.a)); grow_bb(bb, mat_apply(m, d.b)); }
            Entity::Text(t) => grow_bb(bb, mat_apply(m, t.position)),
            Entity::Section(s) => grow_bb(bb, mat_apply(m, s.origin)),
            Entity::Custom(_) => {}
        }
    }
}

fn make_component(doc: &mut Document, ids: &[String], name: &str, description: Option<String>) -> OpResult2<OpResult> {
    if ids.is_empty() { return err("make_component needs at least one entity"); }
    let layer = doc.find(&ids[0]).map(|e| e.layer().to_string()).ok_or_else(|| OpError(format!("no entity {}", ids[0])))?;
    let mut children = vec![];
    for id in ids { children.push(doc.take(id).ok_or_else(|| OpError(format!("no entity {id}")))?); }
    let mut bb = None;
    bounds_of(doc, &children, &mat_identity(), &mut bb);
    let origin = bb.map(|(lo, _)| lo).unwrap_or([0.0; 3]);
    let back = mat_translation(scale(origin, -1.0));
    for c in children.iter_mut() { c.transform(&back); }
    let def = ComponentDef { id: new_id(), name: name.to_string(), description, entities: children, extra: Default::default() };
    let inst = Instance { id: new_id(), layer, definition: def.id.clone(), transform: mat_translation(origin), name: Some(name.to_string()), extra: Default::default() };
    let r = OpResult { created: vec![inst.id.clone()], modified: ids.to_vec(), entities: vec![Entity::Instance(inst.clone())], ..Default::default() };
    doc.components.push(def);
    doc.entities.push(Entity::Instance(inst));
    Ok(r)
}

fn place_component(doc: &mut Document, def: &str, position: P3, rotation: f64, scale_v: Option<&Value>, layer: Option<&str>) -> OpResult2<OpResult> {
    let d = doc.component(def).ok_or_else(|| OpError(format!("no component {def}")))?;
    let (did, dname) = (d.id.clone(), d.name.clone());
    let layer = resolve_layer(doc, layer)?;
    let sc: P3 = match scale_v {
        None | Some(Value::Null) => [1.0, 1.0, 1.0],
        Some(Value::Number(n)) => { let f = n.as_f64().unwrap_or(1.0); [f, f, f] }
        Some(Value::Array(a)) if a.len() == 3 => { let g = |i: usize| a[i].as_f64().unwrap_or(1.0); [g(0), g(1), g(2)] }
        _ => return err("scale must be a number or [sx, sy, sz]"),
    };
    if sc.iter().any(|f| f.abs() < EPS) { return err("scale factors must be non-zero"); }
    let m = mat_mul(&mat_translation(position), &mat_mul(&mat_rotation([0.0, 1.0, 0.0], rotation.to_radians()), &mat_scale(sc)));
    let inst = Instance { id: new_id(), layer, definition: did, transform: m, name: Some(dname), extra: Default::default() };
    Ok(push_created(doc, Entity::Instance(inst)))
}

fn make_unique(doc: &mut Document, instance: &str) -> OpResult2<OpResult> {
    let Some(Entity::Instance(i)) = doc.find(instance).cloned() else { return err(format!("{instance} is not a component instance")) };
    let src = doc.component(&i.definition).cloned().ok_or_else(|| OpError(format!("no component {}", i.definition)))?;
    let n = doc.components.iter().filter(|c| c.name.starts_with(&src.name)).count() + 1;
    let copy = ComponentDef { id: new_id(), name: format!("{} #{n}", src.name), ..src };
    let cid = copy.id.clone();
    doc.components.push(copy);
    if let Some((Entity::Instance(x), _)) = doc.find_mut(instance) { x.definition = cid.clone(); x.name = Some(format!("{} #{n}", src.name)); }
    let inst = doc.find(instance).cloned().unwrap();
    Ok(OpResult { created: vec![cid], modified: vec![instance.to_string()], entities: vec![inst], ..Default::default() })
}

fn edit_component(doc: &mut Document, def: &str, ops: &[Op], reg: &Registry) -> OpResult2<OpResult> {
    let d = doc.component(def).cloned().ok_or_else(|| OpError(format!("no component {def}")))?;
    // the definition's entities edited as a document of their own
    let mut work = Document { entities: d.entities.clone(), layers: doc.layers.clone(), materials: doc.materials.clone(), components: doc.components.clone(), extensions: doc.extensions.clone(), ..Document::default() };
    let results = apply_many(&mut work, ops, reg)?;
    let did = d.id.clone();
    if let Some(c) = doc.component_mut(&did) { c.entities = work.entities; }
    // materials / layers the edit created (e.g. paint with a #hex) come back to the document
    for m in work.materials { if !doc.materials.iter().any(|x| x.id == m.id) { doc.materials.push(m); } }
    for l in work.layers { if !doc.layers.iter().any(|x| x.id == l.id) { doc.layers.push(l); } }
    let mut r = OpResult::default();
    for x in results { r.created.extend(x.created); r.modified.extend(x.modified); r.removed.extend(x.removed); r.entities.extend(x.entities); }
    r.modified.extend(doc.instances_of(&did));
    Ok(r)
}
