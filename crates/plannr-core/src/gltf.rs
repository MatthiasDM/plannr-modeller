//! GLB (glTF 2.0 binary) writer for a compiled scene.

use serde_json::{json, Value};

use crate::compile::CompiledScene;
use crate::geom::hex_to_rgb;

fn base_color(hex: &str, alpha: f64) -> [f32; 4] {
    let [r, g, b] = hex_to_rgb(hex);
    let lin = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    [lin(r), lin(g), lin(b), alpha as f32]
}

struct Bin { data: Vec<u8>, views: Vec<Value>, accessors: Vec<Value> }

impl Bin {
    fn pad(&mut self) { while self.data.len() % 4 != 0 { self.data.push(0); } }
    fn push_f32(&mut self, values: &[f32], comps: usize, target: u32) -> usize {
        self.pad();
        let offset = self.data.len();
        for v in values { self.data.extend_from_slice(&v.to_le_bytes()); }
        let count = values.len() / comps;
        let mut min = vec![f32::MAX; comps];
        let mut max = vec![f32::MIN; comps];
        for chunk in values.chunks(comps) { for (i, v) in chunk.iter().enumerate() { min[i] = min[i].min(*v); max[i] = max[i].max(*v); } }
        self.views.push(json!({ "buffer": 0, "byteOffset": offset, "byteLength": values.len() * 4, "target": target }));
        let mut acc = json!({ "bufferView": self.views.len() - 1, "componentType": 5126, "count": count, "type": if comps == 3 { "VEC3" } else { "SCALAR" } });
        if count > 0 && comps == 3 { acc["min"] = json!(min); acc["max"] = json!(max); }
        self.accessors.push(acc);
        self.accessors.len() - 1
    }
    fn push_u32(&mut self, values: &[u32]) -> usize {
        self.pad();
        let offset = self.data.len();
        for v in values { self.data.extend_from_slice(&v.to_le_bytes()); }
        self.views.push(json!({ "buffer": 0, "byteOffset": offset, "byteLength": values.len() * 4, "target": 34963 }));
        self.accessors.push(json!({ "bufferView": self.views.len() - 1, "componentType": 5125, "count": values.len(), "type": "SCALAR" }));
        self.accessors.len() - 1
    }
}

pub fn export_glb(scene: &CompiledScene) -> Vec<u8> {
    let mut bin = Bin { data: vec![], views: vec![], accessors: vec![] };
    let (mut materials, mut meshes, mut nodes) = (vec![], vec![], vec![]);
    for g in scene.groups.iter().filter(|g| !g.back) {
        let pos = bin.push_f32(&g.positions, 3, 34962);
        let nor = bin.push_f32(&g.normals, 3, 34962);
        let idx = bin.push_u32(&g.indices);
        let mut mat = json!({ "name": g.name, "pbrMetallicRoughness": { "baseColorFactor": base_color(&g.color, g.opacity), "metallicFactor": 0.0, "roughnessFactor": 0.85 }, "doubleSided": true });
        if g.opacity < 1.0 { mat["alphaMode"] = json!("BLEND"); }
        materials.push(mat);
        meshes.push(json!({ "name": g.name, "primitives": [{ "attributes": { "POSITION": pos, "NORMAL": nor }, "indices": idx, "material": materials.len() - 1, "mode": 4 }] }));
        nodes.push(json!({ "name": g.name, "mesh": meshes.len() - 1 }));
    }
    if !scene.edges.positions.is_empty() {
        let pos = bin.push_f32(&scene.edges.positions, 3, 34962);
        materials.push(json!({ "name": "Edges", "pbrMetallicRoughness": { "baseColorFactor": [0.1, 0.1, 0.1, 1.0], "metallicFactor": 0.0, "roughnessFactor": 1.0 } }));
        meshes.push(json!({ "name": "Edges", "primitives": [{ "attributes": { "POSITION": pos }, "material": materials.len() - 1, "mode": 1 }] }));
        nodes.push(json!({ "name": "Edges", "mesh": meshes.len() - 1 }));
    }
    bin.pad();
    let gltf = json!({
        "asset": { "version": "2.0", "generator": format!("plannr-core {}", crate::VERSION) },
        "scene": 0,
        "scenes": [{ "name": "Plannr", "nodes": (0..nodes.len()).collect::<Vec<_>>() }],
        "nodes": nodes, "meshes": meshes, "materials": materials,
        "buffers": [{ "byteLength": bin.data.len() }],
        "bufferViews": bin.views, "accessors": bin.accessors,
    });
    let mut json_bytes = serde_json::to_vec(&gltf).unwrap_or_default();
    while json_bytes.len() % 4 != 0 { json_bytes.push(b' '); }
    let total = 12 + 8 + json_bytes.len() + 8 + bin.data.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json_bytes);
    out.extend_from_slice(&(bin.data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin.data);
    out
}
