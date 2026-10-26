//! Plugin registry. Plugins contribute custom entity types (expanded into core geometry at compile
//! time) and custom ops. The core ships with no plugins.
//!
//! Expanded faces carry ids `<customId>#<faceKey>`; every id-taking core op strips the `#…` part and
//! targets the custom entity, handing the face key to the plugin where it matters (`paint`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

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
    /// Extra numbers merged into `CompiledScene.metrics.plugins.<id>`.
    fn metrics(&self, _doc: &Document) -> Option<Value> { None }
    /// Called when a document is created or loaded (ensure materials, extension blocks…).
    fn on_load(&self, _doc: &mut Document) {}
}

#[derive(Default)]
pub struct Registry {
    plugins: Vec<Box<dyn Plugin>>,
}

/// `<customId>#<faceKey>` → (`<customId>`, `Some(faceKey)`).
pub fn split_key(id: &str) -> (&str, Option<&str>) {
    match id.split_once('#') {
        Some((a, b)) => (a, Some(b)),
        None => (id, None),
    }
}

impl Registry {
    pub fn new() -> Self { Self::default() }
    pub fn register(&mut self, p: Box<dyn Plugin>) { self.plugins.push(p); }
    pub fn list(&self) -> Vec<PluginInfo> { self.plugins.iter().map(|p| p.describe()).collect() }
    pub fn get(&self, id: &str) -> Option<&dyn Plugin> {
        self.plugins.iter().find(|p| p.id() == id).or_else(|| self.plugins.iter().find(|p| p.aliases().contains(&id))).map(|p| p.as_ref())
    }
    pub fn is_empty(&self) -> bool { self.plugins.is_empty() }

    pub fn for_type<'a>(&'a self, type_: &str) -> Option<&'a dyn Plugin> {
        let prefix = type_.split('.').next().unwrap_or("");
        self.get(prefix)
    }

    pub fn apply(&self, type_: &str, data: Value, doc: &mut Document) -> Result<OpResult, OpError> {
        match self.for_type(type_) {
            Some(p) => p.apply(type_, data, doc),
            None => Err(OpError(format!("no plugin for {type_}"))),
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
        for p in &self.plugins {
            if let Some(v) = p.metrics(doc) { out.insert(p.id().to_string(), v); }
        }
        out
    }

    pub fn on_load(&self, doc: &mut Document) {
        for p in &self.plugins { p.on_load(doc); }
    }

    /// A copy of the document where every custom entity is replaced by its plugin's expansion
    /// (custom entities without a plugin are dropped).
    pub fn expand_all(&self, doc: &Document) -> Document {
        fn walk(reg: &Registry, list: &[Entity], doc: &Document) -> Vec<Entity> {
            let mut out = vec![];
            for e in list {
                match e {
                    Entity::Custom(c) => {
                        if let Some(p) = reg.for_type(&c.type_) {
                            if let Ok(exp) = p.expand(c, doc) {
                                for mut x in exp {
                                    if x.layer().is_empty() { x.set_layer(c.layer.clone()); }
                                    out.push(x);
                                }
                            }
                        }
                    }
                    Entity::Group(g) => {
                        let mut g2 = g.clone();
                        g2.entities = walk(reg, &g.entities, doc);
                        out.push(Entity::Group(g2));
                    }
                    other => out.push(other.clone()),
                }
            }
            out
        }
        let mut d = doc.clone();
        d.entities = walk(self, &doc.entities, doc);
        for c in d.components.iter_mut() { c.entities = walk(self, &c.entities, doc); }
        d
    }
}
