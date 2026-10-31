//! In-memory project cache with history, backed by SQLite; the single place that commits changes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use plannr_core::history::HistoryEntry;
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

    pub fn preview(&self, id: &str, ops: &[Op], opts: &CompileOptions) -> ApiResult<CompiledScene> {
        let reg = &self.registry;
        self.with(id, |p| {
            let mut doc = p.doc.clone();
            apply_many(&mut doc, ops, reg)?;
            Ok(compile(&doc, reg, opts))
        })
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
        self.with(id, |p| {
            let label = p.history.undo(&mut p.doc).ok_or_else(|| ApiError::BadRequest("nothing to undo".into()))?;
            self.db.save(id, &p.doc)?;
            self.hub.broadcast(id, &json!({ "type": "changed", "version": p.doc.version, "label": format!("Undo {label}"), "source": source }));
            Ok((p.doc.version, label))
        })
    }

    pub fn redo(&self, id: &str, source: &str) -> ApiResult<(u64, String)> {
        self.with(id, |p| {
            let label = p.history.redo(&mut p.doc).ok_or_else(|| ApiError::BadRequest("nothing to redo".into()))?;
            self.db.save(id, &p.doc)?;
            self.hub.broadcast(id, &json!({ "type": "changed", "version": p.doc.version, "label": format!("Redo {label}"), "source": source }));
            Ok((p.doc.version, label))
        })
    }

    pub fn history(&self, id: &str) -> ApiResult<Vec<HistoryEntry>> { self.with(id, |p| Ok(p.history.entries())) }

    pub fn compile(&self, id: &str, opts: &CompileOptions) -> ApiResult<CompiledScene> {
        let reg = &self.registry;
        self.with(id, |p| Ok(compile(&p.doc, reg, opts)))
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
