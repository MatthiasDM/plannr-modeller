//! Plugin registry. Plugins contribute custom entity types (expanded into core geometry at compile
//! time) and custom ops. The core ships with no plugins.
//!
//! Expanded faces carry ids `<customId>#<faceKey>`; every id-taking core op strips the `#…` part and
//! targets the custom entity, handing the face key to the plugin where it matters (`paint`).

use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::compile::CompiledScene;
use crate::geom::Mat4;
use crate::model::{Custom, Document, Entity};
use crate::ops::{OpError, OpResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginOp {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub entity_types: Vec<String>,
    pub ops: Vec<PluginOp>,
}

/// A file format a plugin can write (`stl`, `3mf`…): listed in `GET /api/plugins` as `exports`, served at
/// `GET /api/projects/:id/export/<format>` and by the MCP tool `export_file`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFormat {
    /// File extension without the dot; also the format id in the URL.
    pub format: String,
    pub label: String,
    pub mime: String,
    pub description: String,
    /// JSON schema of the options (query parameters / MCP `options`).
    pub options_schema: Value,
}

/// What an export produced: the file and a report (sizes, checks, warnings) shown before downloading.
#[derive(Debug, Clone)]
pub struct ExportFile { pub bytes: Vec<u8>, pub report: Value }

/// Which side of a face a paint op targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side { Front, Back, Both }

impl Side {
    pub fn parse(s: &str) -> Side {
        match s { "back" => Side::Back, "both" => Side::Both, _ => Side::Front }
    }
}

pub trait Plugin: Send + Sync {
    fn id(&self) -> &str;
    /// Other type prefixes this plugin owns (e.g. a plugin that absorbed another keeps its
    /// `<alias>.*` entity types and op names working).
    fn aliases(&self) -> &[&str] { &[] }
    fn describe(&self) -> PluginInfo;
    /// Core entities to tessellate in place of a custom entity of one of this plugin's types.
    /// Their ids must be `<customId>#<faceKey>`.
    fn expand(&self, entity: &Custom, doc: &Document) -> Result<Vec<Entity>, OpError>;
    /// Handle a `custom` op whose type starts with `<id>.`.
    fn apply(&self, op_type: &str, data: Value, doc: &mut Document) -> Result<OpResult, OpError>;
    /// Called by `move` / `rotate` / `scale` / `copy` on a custom entity (world matrix).
    /// `doc` is the document *before* the change, for resolving references (hosts, storeys…).
    fn transform(&self, _entity: &mut Custom, _m: &Mat4, _doc: &Document) -> Result<(), OpError> {
        Err(OpError("this plugin entity cannot be transformed".into()))
    }
    /// Called by `paint` when the id carried a face key.
    fn paint(&self, _entity: &mut Custom, _face_key: &str, _material: &str, _side: Side) -> Result<(), OpError> {
        Err(OpError("this plugin entity cannot be painted".into()))
    }
    /// Called once before a round of `expand` calls with the same `doc` (a compile). A plugin running in a
    /// sandbox uses it to receive the document once instead of with every entity.
    fn prepare(&self, _doc: &Document) {}
    /// Extra numbers merged into `CompiledScene.metrics.plugins.<id>`.
    fn metrics(&self, _doc: &Document) -> Option<Value> { None }
    /// Called when a document is created or loaded (ensure materials, extension blocks…).
    fn on_load(&self, _doc: &mut Document) {}
    /// File formats this plugin writes.
    fn exports(&self) -> Vec<ExportFormat> { vec![] }
    /// Write `scene` (already compiled: plugin geometry expanded, hidden entities left out, compiled with
    /// `only` = the requested ids when there are any) in `format`. `doc` is the original document.
    fn export(&self, _format: &str, _scene: &CompiledScene, _doc: &Document, _options: &Value) -> Result<ExportFile, OpError> {
        Err(OpError("this plugin writes no files".into()))
    }
}

/// Where a registered plugin came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// Compiled into the server.
    BuiltIn,
    /// Installed from a package (`data/plugins/<id>/`).
    Installed,
}

struct Entry { plugin: Arc<dyn Plugin>, enabled: bool, source: Source }

/// The plugins of a running server. Plugins can be added, removed, enabled and disabled while it runs;
/// a disabled plugin is skipped everywhere (its entities stay in the documents but are not drawn, its ops
/// and exports are unavailable) until it is enabled again.
#[derive(Default)]
pub struct Registry {
    plugins: RwLock<Vec<Entry>>,
}

/// `<customId>#<faceKey>` → (`<customId>`, `Some(faceKey)`).
pub fn split_key(id: &str) -> (&str, Option<&str>) {
    match id.split_once('#') {
        Some((a, b)) => (a, Some(b)),
        None => (id, None),
    }
}

/// A registered plugin and its state, for plugin managers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginStatus { #[serde(flatten)] pub info: PluginInfo, pub enabled: bool, pub source: Source }

impl Registry {
    pub fn new() -> Self { Self::default() }
    pub fn register(&mut self, p: Box<dyn Plugin>) { self.add(Arc::from(p), Source::BuiltIn, true); }
    /// Add (or replace, by id) a plugin while the server runs.
    pub fn add(&self, p: Arc<dyn Plugin>, source: Source, enabled: bool) {
        let mut list = self.plugins.write().unwrap();
        list.retain(|e| e.plugin.id() != p.id());
        list.push(Entry { plugin: p, enabled, source });
    }
    /// Take a plugin out; returns whether it was there.
    pub fn remove(&self, id: &str) -> bool {
        let mut list = self.plugins.write().unwrap();
        let n = list.len(); list.retain(|e| e.plugin.id() != id); n != list.len()
    }
    /// Enable or disable a plugin; returns false when there is none with that id.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> bool {
        let mut list = self.plugins.write().unwrap();
        match list.iter_mut().find(|e| e.plugin.id() == id) { Some(e) => { e.enabled = enabled; true } None => false }
    }
    pub fn is_enabled(&self, id: &str) -> bool { self.plugins.read().unwrap().iter().any(|e| e.plugin.id() == id && e.enabled) }
    /// Every registered plugin, enabled or not.
    pub fn statuses(&self) -> Vec<PluginStatus> {
        self.plugins.read().unwrap().iter().map(|e| PluginStatus { info: e.plugin.describe(), enabled: e.enabled, source: e.source.clone() }).collect()
    }
    /// The enabled plugins, in registration order.
    pub fn active(&self) -> Vec<Arc<dyn Plugin>> { self.plugins.read().unwrap().iter().filter(|e| e.enabled).map(|e| e.plugin.clone()).collect() }
    /// The enabled plugins' descriptions.
    pub fn list(&self) -> Vec<PluginInfo> { self.active().iter().map(|p| p.describe()).collect() }
    pub fn get(&self, id: &str) -> Option<Arc<dyn Plugin>> {
        let a = self.active();
        a.iter().find(|p| p.id() == id).or_else(|| a.iter().find(|p| p.aliases().contains(&id))).cloned()
    }
    pub fn is_empty(&self) -> bool { self.active().is_empty() }

    pub fn for_type(&self, type_: &str) -> Option<Arc<dyn Plugin>> {
        let prefix = type_.split('.').next().unwrap_or("");
        self.get(prefix)
    }

    pub fn apply(&self, type_: &str, data: Value, doc: &mut Document) -> Result<OpResult, OpError> {
        match self.for_type(type_) {
            Some(p) => p.apply(type_, data, doc),
            None => Err(OpError(format!("no plugin for {type_} (is it installed and enabled?)"))),
        }
    }

    pub fn transform(&self, e: &mut Custom, m: &Mat4, doc: &Document) -> Result<(), OpError> {
        match self.for_type(&e.type_) {
            Some(p) => p.transform(e, m, doc),
            None => Err(OpError(format!("no plugin for {}", e.type_))),
        }
    }

    pub fn paint(&self, e: &mut Custom, face_key: &str, material: &str, side: Side) -> Result<(), OpError> {
        match self.for_type(&e.type_) {
            Some(p) => p.paint(e, face_key, material, side),
            None => Err(OpError(format!("no plugin for {}", e.type_))),
        }
    }

    pub fn metrics(&self, doc: &Document) -> Map<String, Value> {
        let mut out = Map::new();
        for p in self.active() {
            if let Some(v) = p.metrics(doc) { out.insert(p.id().to_string(), v); }
        }
        out
    }

    /// Every export format of the enabled plugins with the id of the plugin that writes it.
    pub fn exports(&self) -> Vec<(String, ExportFormat)> {
        self.active().iter().flat_map(|p| p.exports().into_iter().map(|f| (p.id().to_string(), f))).collect()
    }
    /// The plugin writing `format` (first registered wins).
    pub fn exporter(&self, format: &str) -> Option<Arc<dyn Plugin>> {
        self.active().into_iter().find(|p| p.exports().iter().any(|f| f.format == format))
    }

    pub fn on_load(&self, doc: &mut Document) {
        for p in self.active() { p.on_load(doc); }
    }

    /// A copy of the document where every custom entity is replaced by its plugin's expansion
    /// (custom entities without a plugin are dropped).
    pub fn expand_all(&self, doc: &Document) -> Document {
        for p in self.active() { p.prepare(doc); }
        let mut d = doc.clone();
        d.entities = self.expand_list(&doc.entities, doc);
        for c in d.components.iter_mut() { c.entities = self.expand_list(&c.entities, doc); }
        d
    }
    /// Like `expand_all`, but the result holds only the top-level entities in `keep` (expanded against the whole
    /// document, so a wall still sees its neighbours). For previews that only redraw what an op changed.
    pub fn expand_some(&self, doc: &Document, keep: &std::collections::HashSet<String>) -> Document {
        for p in self.active() { p.prepare(doc); }
        let list: Vec<Entity> = doc.entities.iter().filter(|e| keep.contains(e.id())).cloned().collect();
        let mut components = doc.components.clone();
        for c in components.iter_mut() { c.entities = self.expand_list(&c.entities, doc); }
        Document { version: doc.version, units: doc.units.clone(), layers: doc.layers.clone(), materials: doc.materials.clone(), entities: self.expand_list(&list, doc), scenes: vec![], components, extensions: doc.extensions.clone(), extra: doc.extra.clone() }
    }
    fn expand_list(&self, list: &[Entity], doc: &Document) -> Vec<Entity> {
        let mut out = vec![];
        for e in list {
            match e {
                Entity::Custom(c) => {
                    if let Some(p) = self.for_type(&c.type_) {
                        if let Ok(exp) = p.expand(c, doc) {
                            for mut x in exp {
                                if x.layer().is_empty() { x.set_layer(c.layer.clone()); }
                                if e.hidden() { x.set_hidden(true); }
                                out.push(x);
                            }
                        }
                    }
                }
                Entity::Group(g) => {
                    let mut g2 = g.clone();
                    g2.entities = self.expand_list(&g.entities, doc);
                    out.push(Entity::Group(g2));
                }
                other => out.push(other.clone()),
            }
        }
        out
    }
}
