//! In-memory project cache with history, backed by SQLite; the single place that commits changes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use plannr_core::history::HistoryEntry;
use plannr_core::model::Entity;
use plannr_core::{apply_many, compile, CompileOptions, CompiledScene, Document, History, Op, OpResult, Registry};
use serde_json::json;

use crate::db::{Db, ProjectSummary};
use crate::error::{ApiError, ApiResult};
use crate::ws::Hub;

pub struct ProjectState {
    pub name: String,
    pub doc: Document,
    pub history: History,
}

pub struct AppState {
    pub db: Db,
    pub web_dir: std::path::PathBuf,
    pub projects: Mutex<HashMap<String, ProjectState>>,
    pub registry: Registry,
    /// Installed packages and which plugins are switched off.
    pub packages: crate::packages::Packages,
    /// Ids of the plugins that ship with the server (engine or browser bundle): packages can't replace them.
    pub builtin: std::collections::HashSet<String>,
    pub hub: Arc<Hub>,
    pub active_project: Mutex<Option<String>>,
    pub token: String,
    pub port: u16,
    /// Public instance: no MCP, and every browser (owner key) sees only its own projects.
    pub public: bool,
}

/// The browser a request comes from on a public instance (None on a private one).
#[derive(Debug, Clone, Default)]
pub struct Owner(pub Option<String>);

/// Most projects one browser may keep on a public instance.
pub const MAX_PROJECTS_PER_OWNER: usize = 30;

pub type Shared = Arc<AppState>;

impl AppState {
    fn load(&self, id: &str) -> ApiResult<()> {
        let mut map = self.projects.lock().unwrap_or_else(|e| e.into_inner());
        if !map.contains_key(id) {
            // a public instance serves many projects: keep the cache bounded (their undo history goes)
            if self.public && map.len() >= 64 { map.clear(); }
            let p = self.db.get(id)?;
            let mut doc = p.document;
            self.registry.on_load(&mut doc);
            map.insert(id.to_string(), ProjectState { name: p.name, doc, history: History::default() });
        }
        Ok(())
    }

    pub fn evict(&self, id: &str) { self.projects.lock().unwrap_or_else(|e| e.into_inner()).remove(id); }

    /// Run `f` on the cached project.
    pub fn with<R>(&self, id: &str, f: impl FnOnce(&mut ProjectState) -> ApiResult<R>) -> ApiResult<R> {
        self.load(id)?;
        let mut map = self.projects.lock().unwrap_or_else(|e| e.into_inner());
        let p = map.get_mut(id).ok_or_else(|| ApiError::NotFound("project not found".into()))?;
        f(p)
    }

    pub fn document(&self, id: &str) -> ApiResult<Document> { self.with(id, |p| Ok(p.doc.clone())) }

    /// Resolve a project reference: id, name, the active project, or the most recent one.
    pub fn resolve(&self, r: Option<&str>) -> ApiResult<String> {
        if let Some(r) = r.filter(|r| !r.is_empty()) {
            if self.db.get(r).is_ok() { return Ok(r.to_string()); }
            if let Some(id) = self.db.id_by_name(r)? { return Ok(id); }
            return Err(ApiError::NotFound(format!("no project `{r}`")));
        }
        if let Some(id) = self.active_project.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            if self.db.get(&id).is_ok() { return Ok(id); }
        }
        self.db.first_id()?.ok_or_else(|| ApiError::NotFound("no projects yet — create one first".into()))
    }

    fn commit(&self, id: &str, p: &mut ProjectState, before: Document, label: &str, source: &str) -> ApiResult<u64> {
        // read-only ops (checks, takeoffs, catalogues) leave no undo step behind
        if before == p.doc { return Ok(p.doc.version); }
        p.history.push(before, label);
        p.doc.version += 1;
        self.db.save(id, &p.doc)?;
        self.hub.broadcast(id, &json!({ "type": "changed", "version": p.doc.version, "label": label, "source": source }));
        Ok(p.doc.version)
    }

    pub fn apply_ops(&self, id: &str, ops: &[Op], label: &str, source: &str) -> ApiResult<(u64, Vec<OpResult>)> {
        let reg = &self.registry;
        self.with(id, |p| {
            let before = p.doc.clone();
            let results = apply_many(&mut p.doc, ops, reg)?;
            let v = self.commit(id, p, before, label, source)?;
            Ok((v, results))
        })
    }

    /// `apply_ops`, and what it changed compiled on its own (see `Changes`), so the browser swaps just that.
    pub fn apply_ops_partial(&self, id: &str, ops: &[Op], label: &str, source: &str, opts: &CompileOptions) -> ApiResult<(u64, Vec<OpResult>, Option<Changes>)> {
        let reg = &self.registry;
        let (v, results, before, after) = self.with(id, |p| {
            let before = p.doc.clone();
            let results = apply_many(&mut p.doc, ops, reg)?;
            let v = self.commit(id, p, before.clone(), label, source)?;
            Ok((v, results, before, p.doc.clone()))
        })?;
        let changes = changes(&before, &after, &results, reg, opts);
        Ok((v, results, changes))
    }

    /// A snapshot of the cached document: compiles run on it without holding every project's lock.
    fn snapshot(&self, id: &str) -> ApiResult<Document> { self.with(id, |p| Ok(p.doc.clone())) }

    pub fn preview(&self, id: &str, ops: &[Op], opts: &CompileOptions) -> ApiResult<CompiledScene> {
        let reg = &self.registry;
        let mut doc = self.snapshot(id)?;
        apply_many(&mut doc, ops, reg)?;
        Ok(compile(&doc, reg, opts))
    }

    /// A preview of only what the ops change: the top-level entities they create, modify or remove (`dirty`),
    /// compiled as the ops leave them. The client keeps the rest of its scene and swaps just those. Falls back to
    /// the whole scene (`None` for dirty) when the results don't say what changed.
    pub fn preview_partial(&self, id: &str, ops: &[Op], opts: &CompileOptions) -> ApiResult<(CompiledScene, Option<Vec<String>>)> {
        let reg = &self.registry;
        let base = self.snapshot(id)?;
        let mut doc = base.clone();
        let results = apply_many(&mut doc, ops, reg)?;
        let (before, after) = (top_levels(&base), top_levels(&doc));
        let mut dirty: Vec<String> = vec![]; let mut known = true;
        for r in &results {
            for id in r.created.iter().chain(&r.modified).chain(&r.removed) {
                let bare = id.split('#').next().unwrap_or(id);
                match after.get(bare).or_else(|| before.get(bare)) { Some(t) => if !dirty.contains(t) { dirty.push(t.clone()) }, None => known = false }
            }
        }
        if !known || dirty.is_empty() { return Ok((compile(&doc, reg, opts), None)); }
        soft_neighbours(&base, &doc, &mut dirty);
        let only: std::collections::HashSet<String> = dirty.iter().filter(|t| after.contains_key(*t)).cloned().collect();
        Ok((compile(&doc, reg, &CompileOptions { only: Some(only), ..opts.clone() }), Some(dirty)))
    }

    pub fn replace_document(&self, id: &str, mut doc: Document, source: &str) -> ApiResult<u64> {
        self.with(id, |p| {
            let before = p.doc.clone();
            doc.normalize();
            self.registry.on_load(&mut doc);
            doc.version = p.doc.version;
            p.doc = doc;
            self.commit(id, p, before, "Replace document", source)
        })
    }

    pub fn undo(&self, id: &str, source: &str) -> ApiResult<(u64, String)> {
        self.step(id, source, true, None).map(|(v, l, _)| (v, l))
    }

    pub fn redo(&self, id: &str, source: &str) -> ApiResult<(u64, String)> {
        self.step(id, source, false, None).map(|(v, l, _)| (v, l))
    }

    /// Undo (`back`) or redo one step; with `opts`, also what it changed compiled on its own (see `Changes`).
    pub fn step(&self, id: &str, source: &str, back: bool, opts: Option<&CompileOptions>) -> ApiResult<(u64, String, Option<Changes>)> {
        let (v, label, docs) = self.with(id, |p| {
            let before = opts.map(|_| p.doc.clone());
            let label = if back { p.history.undo(&mut p.doc) } else { p.history.redo(&mut p.doc) };
            let label = label.ok_or_else(|| ApiError::BadRequest(if back { "nothing to undo" } else { "nothing to redo" }.into()))?;
            self.db.save(id, &p.doc)?;
            self.hub.broadcast(id, &json!({ "type": "changed", "version": p.doc.version, "label": format!("{} {label}", if back { "Undo" } else { "Redo" }), "source": source }));
            Ok((p.doc.version, label, before.map(|b| (b, p.doc.clone()))))
        })?;
        let changes = match (docs, opts) { (Some((b, a)), Some(o)) => changes(&b, &a, &[], &self.registry, o), _ => None };
        Ok((v, label, changes))
    }

    pub fn history(&self, id: &str) -> ApiResult<Vec<HistoryEntry>> { self.with(id, |p| Ok(p.history.entries())) }

    pub fn compile(&self, id: &str, opts: &CompileOptions) -> ApiResult<CompiledScene> {
        let doc = self.snapshot(id)?;
        Ok(compile(&doc, &self.registry, opts))
    }

    pub fn create(&self, name: &str, doc: Option<Document>) -> ApiResult<ProjectSummary> { self.create_for(name, doc, &Owner(None)) }

    pub fn create_for(&self, name: &str, doc: Option<Document>, owner: &Owner) -> ApiResult<ProjectSummary> {
        if let Some(o) = &owner.0 {
            if self.db.count_for(o)? >= MAX_PROJECTS_PER_OWNER { return Err(ApiError::BadRequest(format!("this browser already keeps {MAX_PROJECTS_PER_OWNER} projects — delete one first"))); }
        }
        let mut doc = doc.unwrap_or_default();
        doc.normalize();
        self.registry.on_load(&mut doc);
        let p = self.db.create(name, &doc)?;
        if let Some(o) = &owner.0 { self.db.set_owner(&p.id, o)?; }
        self.db.summary(&p.id)
    }

    pub fn rename(&self, id: &str, name: &str) -> ApiResult<ProjectSummary> {
        self.db.rename(id, name)?;
        if let Ok(()) = self.load(id) {
            if let Some(p) = self.projects.lock().unwrap_or_else(|e| e.into_inner()).get_mut(id) { p.name = name.to_string(); }
        }
        self.db.summary(id)
    }
}

/// Every entity id (nested in groups too) → the id of its top-level entity.
fn top_levels(doc: &plannr_core::model::Document) -> std::collections::HashMap<String, String> {
    fn walk(e: &plannr_core::model::Entity, top: &str, m: &mut std::collections::HashMap<String, String>) {
        m.insert(e.id().to_string(), top.to_string());
        if let plannr_core::model::Entity::Group(g) = e { for c in &g.entities { walk(c, top, m); } }
    }
    let mut m = std::collections::HashMap::new();
    for e in &doc.entities { walk(e, e.id(), &mut m); }
    m
}

/// What a committed change did to the drawing: the top-level entities it touched (`dirty`) and those of them
/// that still exist, compiled on their own (`scene`). The browser swaps just these into the model it has.
/// `reconcile`: plugin geometry was touched, which can depend on neighbours the change didn't touch (a wall's
/// corners), so the browser should fetch the whole scene once it is idle.
pub struct Changes { pub scene: CompiledScene, pub dirty: Vec<String>, pub reconcile: bool }

/// More than this share of the top-level entities touched: the whole scene is as cheap.
const MAX_DIRTY_SHARE: f64 = 0.5;

/// `None` when only the whole scene is right: anything but the entities changed (layers, materials, components,
/// plugin settings), a section plane changed (it cuts everything), or most of the model changed.
fn changes(before: &Document, after: &Document, results: &[OpResult], reg: &Registry, opts: &CompileOptions) -> Option<Changes> {
    if before.units != after.units || before.layers != after.layers || before.materials != after.materials
        || before.components != after.components || before.extensions != after.extensions || before.extra != after.extra { return None; }
    fn tops(d: &Document) -> HashMap<&str, &Entity> { d.entities.iter().map(|e| (e.id(), e)).collect() }
    let (b, a) = (tops(before), tops(after));
    let mut dirty: Vec<String> = vec![];
    for e in &after.entities { if b.get(e.id()).map_or(true, |o| *o != e) { dirty.push(e.id().to_string()); } }
    for e in &before.entities { if !a.contains_key(e.id()) { dirty.push(e.id().to_string()); } }
    // ids the ops report (nested ones too) map to their top-level entity
    let (nb, na) = (top_levels(before), top_levels(after));
    for r in results {
        for id in r.created.iter().chain(&r.modified).chain(&r.removed) {
            let bare = id.split('#').next().unwrap_or(id);
            if let Some(t) = na.get(bare).or_else(|| nb.get(bare)) { if !dirty.contains(t) { dirty.push(t.clone()); } }
        }
    }
    soft_neighbours(before, after, &mut dirty);
    let n = after.entities.len().max(before.entities.len()).max(1);
    if dirty.len() as f64 > n as f64 * MAX_DIRTY_SHARE && dirty.len() > 8 { return None; }
    let touched = || dirty.iter().filter_map(|id| a.get(id.as_str()).or_else(|| b.get(id.as_str())));
    // a section plane cuts everything, unless the viewer cuts on the GPU (uncut geometry + `cuts`)
    if !opts.ignore_sections && touched().any(|e| has_kind(e, &|e| matches!(e, Entity::Section(_)))) { return None; }
    let reconcile = touched().any(|e| has_kind(e, &|e| matches!(e, Entity::Custom(_))));
    let only: std::collections::HashSet<String> = dirty.iter().filter(|id| a.contains_key(id.as_str())).cloned().collect();
    let scene = compile(after, reg, &CompileOptions { only: Some(only), ..opts.clone() });
    Some(Changes { scene, dirty, reconcile })
}

/// A softened face at the top level is drawn by what its neighbours are (which edges show, how it is shaded):
/// when one of them changed, all of them are redrawn.
fn soft_neighbours(before: &Document, after: &Document, dirty: &mut Vec<String>) {
    let soft = |e: &&Entity| matches!(e, Entity::Face(f) if f.smooth().is_some());
    let all = || before.entities.iter().chain(after.entities.iter()).filter(soft);
    if !all().any(|e| dirty.iter().any(|d| d == e.id())) { return; }
    let more: Vec<String> = all().map(|e| e.id().to_string()).collect();
    for id in more { if !dirty.contains(&id) { dirty.push(id); } }
}

/// Does `e`, or anything inside it (a group), match?
fn has_kind(e: &Entity, f: &dyn Fn(&Entity) -> bool) -> bool {
    f(e) || matches!(e, Entity::Group(g) if g.entities.iter().any(|c| has_kind(c, f)))
}
