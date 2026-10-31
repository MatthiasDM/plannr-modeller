//! The Plannr document: layers, materials and a tree of generic entities.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::geom::{mat_identity, mat_mul, Mat4, P3};

pub const DEFAULT_MATERIAL_COLOR: &str = "#F4F2EE";
pub const BACK_FACE_COLOR: &str = "#B7C1CC";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_layer_color")]
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Material {
    pub id: String,
    pub name: String,
    pub color: String,
    #[serde(default = "default_one")]
    pub opacity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub camera: Vec<f64>,
    /// The active section plane of older documents (one at a time then); `sections` supersedes it.
    #[serde(default)]
    pub section: Option<String>,
    /// The section planes active in this scene.
    #[serde(default)]
    pub sections: Vec<String>,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

fn default_true() -> bool { true }
fn default_one() -> f64 { 1.0 }
fn default_layer_color() -> String { "#7a8aa0".into() }
fn default_units() -> String { "m".into() }
fn default_transform() -> Mat4 { mat_identity() }

macro_rules! entity_struct {
    ($name:ident { $($field:tt)* }) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(rename_all = "camelCase")]
        pub struct $name {
            pub id: String,
            #[serde(default)]
            pub layer: String,
            $($field)*
            #[serde(flatten, default)]
            pub extra: Map<String, Value>,
        }
    };
}

entity_struct!(Face {
    pub outer: Vec<P3>,
    #[serde(default)]
    pub holes: Vec<Vec<P3>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back_material: Option<String>,
});
entity_struct!(Edge { pub a: P3, pub b: P3, });
entity_struct!(Group {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_transform")]
    pub transform: Mat4,
    #[serde(default)]
    pub entities: Vec<Entity>,
});
entity_struct!(Guide {
    pub point: P3,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<P3>,
});
entity_struct!(Dimension { pub a: P3, pub b: P3, pub offset: P3, });
entity_struct!(Text {
    pub position: P3,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<P3>,
});
entity_struct!(Section {
    pub origin: P3,
    pub normal: P3,
    #[serde(default)]
    pub active: bool,
});
entity_struct!(Custom {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default)]
    pub data: Value,
});
// A placed copy of a component definition (`Document::components`); its geometry is the
// definition's entities under `transform`. Compiled ids are `<instanceId>#<entityId>`.
entity_struct!(Instance {
    pub definition: String,
    #[serde(default = "default_transform")]
    pub transform: Mat4,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
});

/// A reusable definition: entities in local coordinates, shared by every instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComponentDef {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub entities: Vec<Entity>,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Entity {
    Face(Face),
    Edge(Edge),
    Group(Group),
    Guide(Guide),
    Dimension(Dimension),
    Text(Text),
    Section(Section),
    Custom(Custom),
    Instance(Instance),
}

impl Entity {
    pub fn id(&self) -> &str {
        match self {
            Entity::Face(e) => &e.id, Entity::Edge(e) => &e.id, Entity::Group(e) => &e.id, Entity::Guide(e) => &e.id,
            Entity::Dimension(e) => &e.id, Entity::Text(e) => &e.id, Entity::Section(e) => &e.id, Entity::Custom(e) => &e.id, Entity::Instance(e) => &e.id,
        }
    }
    pub fn set_id(&mut self, id: String) {
        match self {
            Entity::Face(e) => e.id = id, Entity::Edge(e) => e.id = id, Entity::Group(e) => e.id = id, Entity::Guide(e) => e.id = id,
            Entity::Dimension(e) => e.id = id, Entity::Text(e) => e.id = id, Entity::Section(e) => e.id = id, Entity::Custom(e) => e.id = id, Entity::Instance(e) => e.id = id,
        }
    }
    pub fn layer(&self) -> &str {
        match self {
            Entity::Face(e) => &e.layer, Entity::Edge(e) => &e.layer, Entity::Group(e) => &e.layer, Entity::Guide(e) => &e.layer,
            Entity::Dimension(e) => &e.layer, Entity::Text(e) => &e.layer, Entity::Section(e) => &e.layer, Entity::Custom(e) => &e.layer, Entity::Instance(e) => &e.layer,
        }
    }
    pub fn set_layer(&mut self, layer: String) {
        match self {
            Entity::Face(e) => e.layer = layer, Entity::Edge(e) => e.layer = layer, Entity::Group(e) => e.layer = layer, Entity::Guide(e) => e.layer = layer,
            Entity::Dimension(e) => e.layer = layer, Entity::Text(e) => e.layer = layer, Entity::Section(e) => e.layer = layer, Entity::Custom(e) => e.layer = layer, Entity::Instance(e) => e.layer = layer,
        }
    }
    fn extra(&self) -> &Map<String, Value> {
        match self {
            Entity::Face(e) => &e.extra, Entity::Edge(e) => &e.extra, Entity::Group(e) => &e.extra, Entity::Guide(e) => &e.extra,
            Entity::Dimension(e) => &e.extra, Entity::Text(e) => &e.extra, Entity::Section(e) => &e.extra, Entity::Custom(e) => &e.extra, Entity::Instance(e) => &e.extra,
        }
    }
    fn extra_mut(&mut self) -> &mut Map<String, Value> {
        match self {
            Entity::Face(e) => &mut e.extra, Entity::Edge(e) => &mut e.extra, Entity::Group(e) => &mut e.extra, Entity::Guide(e) => &mut e.extra,
            Entity::Dimension(e) => &mut e.extra, Entity::Text(e) => &mut e.extra, Entity::Section(e) => &mut e.extra, Entity::Custom(e) => &mut e.extra, Entity::Instance(e) => &mut e.extra,
        }
    }
    /// Hidden with Edit → Hide: left out of the compiled scene, slices and exports (the data stays).
    pub fn hidden(&self) -> bool { self.extra().get("hidden").and_then(|v| v.as_bool()).unwrap_or(false) }
    pub fn set_hidden(&mut self, hidden: bool) {
        if hidden { self.extra_mut().insert("hidden".into(), Value::Bool(true)); } else { self.extra_mut().remove("hidden"); }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Entity::Face(_) => "face", Entity::Edge(_) => "edge", Entity::Group(_) => "group", Entity::Guide(_) => "guide",
            Entity::Dimension(_) => "dimension", Entity::Text(_) => "text", Entity::Section(_) => "section", Entity::Custom(_) => "custom",
            Entity::Instance(_) => "instance",
        }
    }
    /// Apply a transform to this entity's own geometry (a group composes it onto its transform).
    pub fn transform(&mut self, m: &Mat4) {
        use crate::geom::mat_apply;
        match self {
            Entity::Face(f) => {
                for p in f.outer.iter_mut() { *p = mat_apply(m, *p); }
                for h in f.holes.iter_mut() { for p in h.iter_mut() { *p = mat_apply(m, *p); } }
            }
            Entity::Edge(e) => {
                let before = crate::geom::dist(e.a, e.b);
                e.a = mat_apply(m, e.a); e.b = mat_apply(m, e.b);
                // an arc segment carries its circle: move the centre with it (the radius follows a uniform scale)
                if let Some(arc) = e.extra.get_mut("arc") {
                    let p3 = |v: &serde_json::Value| -> Option<[f64; 3]> { let a = v.as_array()?; Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?]) };
                    let (c, n, r) = (arc.get("center").and_then(p3), arc.get("normal").and_then(p3), arc.get("radius").and_then(|r| r.as_f64()));
                    if let (Some(c), Some(n), Some(r)) = (c, n, r) {
                        let k = if before > 1e-12 { crate::geom::dist(e.a, e.b) / before } else { 1.0 };
                        *arc = serde_json::json!({ "center": mat_apply(m, c), "normal": crate::geom::normalize(crate::geom::mat_apply_dir(m, n)), "radius": r * k });
                    }
                }
            }
            Entity::Group(g) => g.transform = mat_mul(m, &g.transform),
            Entity::Guide(g) => { g.point = mat_apply(m, g.point); if let Some(d) = g.direction { g.direction = Some(crate::geom::mat_apply_dir(m, d)); } }
            Entity::Dimension(d) => { d.a = mat_apply(m, d.a); d.b = mat_apply(m, d.b); d.offset = crate::geom::mat_apply_dir(m, d.offset); }
            Entity::Text(t) => { t.position = mat_apply(m, t.position); if let Some(a) = t.anchor { t.anchor = Some(mat_apply(m, a)); } }
            Entity::Section(s) => { s.origin = mat_apply(m, s.origin); s.normal = crate::geom::normalize(crate::geom::mat_apply_dir(m, s.normal)); }
            Entity::Custom(_) => {}
            Entity::Instance(i) => i.transform = mat_mul(m, &i.transform),
        }
    }
    /// Reverse face windings (after a reflection, so normals keep pointing outward).
    pub fn reverse_winding(&mut self) {
        if let Entity::Face(f) = self {
            f.outer.reverse();
            for h in f.holes.iter_mut() { h.reverse(); }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    #[serde(default)]
    pub version: u64,
    #[serde(default = "default_units")]
    pub units: String,
    #[serde(default)]
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub materials: Vec<Material>,
    #[serde(default)]
    pub entities: Vec<Entity>,
    #[serde(default)]
    pub scenes: Vec<Scene>,
    /// Component definitions shared by `instance` entities.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<ComponentDef>,
    #[serde(default)]
    pub extensions: Map<String, Value>,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for Document {
    fn default() -> Self {
        Document {
            version: 0,
            units: "m".into(),
            layers: vec![Layer { id: new_id(), name: "Layer0".into(), visible: true, color: default_layer_color() }],
            materials: vec![Material { id: new_id(), name: "Default".into(), color: DEFAULT_MATERIAL_COLOR.into(), opacity: 1.0 }],
            entities: vec![],
            scenes: vec![],
            components: vec![],
            extensions: Map::new(),
            extra: Map::new(),
        }
    }
}

pub fn new_id() -> String { uuid::Uuid::new_v4().to_string() }

/// A flattened entity reference with its accumulated world transform.
pub struct Visit<'a> {
    pub entity: &'a Entity,
    pub transform: Mat4,
    pub path: Vec<String>,
}

impl Document {
    /// Make sure layers/materials exist and every entity has a valid layer.
    pub fn normalize(&mut self) {
        if self.layers.is_empty() {
            self.layers.push(Layer { id: new_id(), name: "Layer0".into(), visible: true, color: default_layer_color() });
        }
        if self.materials.is_empty() {
            self.materials.push(Material { id: new_id(), name: "Default".into(), color: DEFAULT_MATERIAL_COLOR.into(), opacity: 1.0 });
        }
        let first = self.layers[0].id.clone();
        let ids: Vec<String> = self.layers.iter().map(|l| l.id.clone()).collect();
        fn fix(list: &mut [Entity], ids: &[String], first: &str) {
            for e in list.iter_mut() {
                if !ids.iter().any(|i| i == e.layer()) { e.set_layer(first.to_string()); }
                if let Entity::Group(g) = e { fix(&mut g.entities, ids, first); }
            }
        }
        fix(&mut self.entities, &ids, &first);
        for c in self.components.iter_mut() { fix(&mut c.entities, &ids, &first); }
    }

    pub fn component(&self, r: &str) -> Option<&ComponentDef> {
        self.components.iter().find(|c| c.id == r).or_else(|| self.components.iter().find(|c| c.name == r))
    }
    pub fn component_mut(&mut self, r: &str) -> Option<&mut ComponentDef> {
        let id = self.component(r)?.id.clone();
        self.components.iter_mut().find(|c| c.id == id)
    }
    /// Ids of every instance of a definition (through groups).
    pub fn instances_of(&self, def_id: &str) -> Vec<String> {
        let mut out = vec![];
        self.visit(|v| if let Entity::Instance(i) = v.entity { if i.definition == def_id { out.push(i.id.clone()); } });
        out
    }

    pub fn default_layer(&self) -> &str { &self.layers[0].id }
    pub fn default_material(&self) -> &Material { &self.materials[0] }

    pub fn layer_by_ref(&self, r: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == r).or_else(|| self.layers.iter().find(|l| l.name == r))
    }
    pub fn material_by_ref(&self, r: &str) -> Option<&Material> {
        self.materials.iter().find(|m| m.id == r).or_else(|| self.materials.iter().find(|m| m.name == r))
    }

    /// Depth-first walk through groups, with accumulated transforms.
    pub fn visit<'a>(&'a self, mut f: impl FnMut(&Visit<'a>)) {
        fn walk<'a>(list: &'a [Entity], m: Mat4, path: &mut Vec<String>, f: &mut dyn FnMut(&Visit<'a>)) {
            for e in list {
                f(&Visit { entity: e, transform: m, path: path.clone() });
                if let Entity::Group(g) = e {
                    path.push(g.id.clone());
                    walk(&g.entities, mat_mul(&m, &g.transform), path, f);
                    path.pop();
                }
            }
        }
        walk(&self.entities, mat_identity(), &mut vec![], &mut f);
    }

    pub fn find(&self, id: &str) -> Option<&Entity> {
        fn walk<'a>(list: &'a [Entity], id: &str) -> Option<&'a Entity> {
            for e in list {
                if e.id() == id { return Some(e); }
                if let Entity::Group(g) = e { if let Some(x) = walk(&g.entities, id) { return Some(x); } }
            }
            None
        }
        walk(&self.entities, id)
    }

    /// Find an entity by id anywhere in the tree, with the transform of its parent chain.
    pub fn find_mut(&mut self, id: &str) -> Option<(&mut Entity, Mat4)> {
        fn walk<'a>(list: &'a mut [Entity], id: &str, m: Mat4) -> Option<(&'a mut Entity, Mat4)> {
            for e in list.iter_mut() {
                if e.id() == id { return Some((e, m)); }
                if let Entity::Group(g) = e {
                    let gm = mat_mul(&m, &g.transform);
                    if let Some(x) = walk(&mut g.entities, id, gm) { return Some(x); }
                }
            }
            None
        }
        walk(&mut self.entities, id, mat_identity())
    }

    /// The list that directly contains the entity with `id` (top level or a group's children).
    pub fn container_mut(&mut self, id: &str) -> Option<(&mut Vec<Entity>, Mat4)> {
        fn walk<'a>(list: &'a mut Vec<Entity>, id: &str, m: Mat4) -> Option<(&'a mut Vec<Entity>, Mat4)> {
            if list.iter().any(|e| e.id() == id) { return Some((list, m)); }
            for e in list.iter_mut() {
                if let Entity::Group(g) = e {
                    let gm = mat_mul(&m, &g.transform);
                    if let Some(x) = walk(&mut g.entities, id, gm) { return Some(x); }
                }
            }
            None
        }
        walk(&mut self.entities, id, mat_identity())
    }

    /// Remove an entity by id wherever it is; returns it.
    pub fn take(&mut self, id: &str) -> Option<Entity> {
        fn walk(list: &mut Vec<Entity>, id: &str) -> Option<Entity> {
            if let Some(i) = list.iter().position(|e| e.id() == id) { return Some(list.remove(i)); }
            for e in list.iter_mut() {
                if let Entity::Group(g) = e { if let Some(x) = walk(&mut g.entities, id) { return Some(x); } }
            }
            None
        }
        walk(&mut self.entities, id)
    }

    pub fn all_ids(&self) -> Vec<String> {
        let mut out = vec![];
        self.visit(|v| out.push(v.entity.id().to_string()));
        out
    }

    pub fn entity_count(&self) -> usize {
        let mut n = 0;
        self.visit(|_| n += 1);
        n
    }

    /// Loose edges at the top level of the container holding `id` (or the top level).
    pub fn loose_edges_in(list: &[Entity]) -> Vec<&Edge> {
        list.iter().filter_map(|e| if let Entity::Edge(e) = e { Some(e) } else { None }).collect()
    }
}
