//! GLB (glTF 2.0 binary) writer for a compiled scene.

use serde_json::{json, Value};

use crate::compile::CompiledScene;
use crate::geom::hex_to_rgb;
use crate::model::Material;

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
        let mut acc = json!({ "bufferView": self.views.len() - 1, "componentType": 5126, "count": count, "type": match comps { 3 => "VEC3", 2 => "VEC2", _ => "SCALAR" } });
        if count > 0 && comps == 3 { acc["min"] = json!(min); acc["max"] = json!(max); }
        self.accessors.push(acc);
        self.accessors.len() - 1
    }
    /// A file (an image) as a buffer view of its own.
    fn push_file(&mut self, file: &[u8]) -> usize {
        self.pad();
        self.views.push(json!({ "buffer": 0, "byteOffset": self.data.len(), "byteLength": file.len() }));
        self.data.extend_from_slice(file);
        self.views.len() - 1
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

/// The scene without its textures: every material as its colour.
pub fn export_glb(scene: &CompiledScene) -> Vec<u8> { export_glb_textured(scene, &[]) }

/// The scene as GLB; the textures of `materials` (the document's) go into the file as images.
pub fn export_glb_textured(scene: &CompiledScene, materials: &[Material]) -> Vec<u8> {
    let mut bin = Bin { data: vec![], views: vec![], accessors: vec![] };
    let (mut images, mut textures): (Vec<Value>, Vec<Value>) = (vec![], vec![]);
    let mut texture_of: std::collections::HashMap<&str, Option<usize>> = Default::default();
    let by_id = |id: &str| materials.iter().find(|m| m.id == id).and_then(|m| m.texture.as_ref());
    let (mut materials, mut meshes, mut nodes) = (vec![], vec![], vec![]);
    for g in scene.groups.iter().filter(|g| !g.back) {
        let pos = bin.push_f32(&g.positions, 3, 34962);
        let nor = bin.push_f32(&g.normals, 3, 34962);
        let idx = bin.push_u32(&g.indices);
        let mut mat = json!({ "name": g.name, "pbrMetallicRoughness": { "baseColorFactor": base_color(&g.color, g.opacity), "metallicFactor": 0.0, "roughnessFactor": 0.85 }, "doubleSided": true });
        if g.opacity < 1.0 { mat["alphaMode"] = json!("BLEND"); }
        let mut attributes = json!({ "POSITION": pos, "NORMAL": nor });
        // a textured material: its image goes in once, drawn as it is (glTF counts v from the top of the image)
        let texture = if g.uvs.len() / 2 == g.positions.len() / 3 { by_id(&g.material) } else { None };
        let index = texture.and_then(|t| *texture_of.entry(&*t.image).or_insert_with(|| {
            let file = crate::texture::bytes(&t.image)?;
            let view = bin.push_file(&file);
            images.push(json!({ "bufferView": view, "mimeType": crate::texture::mime(&t.image)? }));
            textures.push(json!({ "source": images.len() - 1, "sampler": 0 }));
            Some(textures.len() - 1)
        }));
        if let Some(index) = index {
            let uvs: Vec<f32> = g.uvs.chunks(2).flat_map(|c| [c[0], 1.0 - c[1]]).collect();
            attributes["TEXCOORD_0"] = json!(bin.push_f32(&uvs, 2, 34962));
            mat["pbrMetallicRoughness"]["baseColorFactor"] = json!([1.0, 1.0, 1.0, g.opacity as f32]);
            mat["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": index });
        }
        materials.push(mat);
        meshes.push(json!({ "name": g.name, "primitives": [{ "attributes": attributes, "indices": idx, "material": materials.len() - 1, "mode": 4 }] }));
        nodes.push(json!({ "name": g.name, "mesh": meshes.len() - 1 }));
    }
    if !scene.edges.positions.is_empty() {
        let pos = bin.push_f32(&scene.edges.positions, 3, 34962);
        materials.push(json!({ "name": "Edges", "pbrMetallicRoughness": { "baseColorFactor": [0.1, 0.1, 0.1, 1.0], "metallicFactor": 0.0, "roughnessFactor": 1.0 } }));
        meshes.push(json!({ "name": "Edges", "primitives": [{ "attributes": { "POSITION": pos }, "material": materials.len() - 1, "mode": 1 }] }));
        nodes.push(json!({ "name": "Edges", "mesh": meshes.len() - 1 }));
    }
    bin.pad();
    let mut gltf = json!({
        "asset": { "version": "2.0", "generator": format!("plannr-core {}", crate::VERSION) },
        "scene": 0,
        "scenes": [{ "name": "Plannr", "nodes": (0..nodes.len()).collect::<Vec<_>>() }],
        "nodes": nodes, "meshes": meshes, "materials": materials,
        "buffers": [{ "byteLength": bin.data.len() }],
        "bufferViews": bin.views, "accessors": bin.accessors,
    });
    if !textures.is_empty() {
        gltf["images"] = json!(images); gltf["textures"] = json!(textures);
        gltf["samplers"] = json!([{ "wrapS": 10497, "wrapT": 10497, "magFilter": 9729, "minFilter": 9987 }]);
    }
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
