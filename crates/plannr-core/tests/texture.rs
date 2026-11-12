//! Textures: a material carries an image and the size of one copy of it; compile lays it over the faces.
use plannr_core::model::*;
use plannr_core::ops::{apply, Op, TextureArg};
use plannr_core::plugin::Registry;
use plannr_core::{compile, export_glb_textured, CompileOptions};

// a 2 × 1 pixel PNG
const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAIAAAB7QOjdAAAAD0lEQVR4nGPYkGRfYaMBAAkVAi5VLbR0AAAAAElFTkSuQmCC";

fn reg() -> Registry { Registry::default() }
fn op(doc: &mut Document, json: serde_json::Value) -> Result<Vec<String>, String> {
    let op: Op = serde_json::from_value(json).map_err(|e| e.to_string())?;
    apply(doc, &op, &reg()).map(|r| r.created).map_err(|e| e.to_string())
}
fn brick(doc: &mut Document, width: f64) -> String {
    op(doc, serde_json::json!({ "op": "add_material", "name": "Brick", "color": "#b0623f", "texture": { "image": PNG, "width": width } })).unwrap()[0].clone()
}
fn rect(doc: &mut Document, a: [f64; 3], b: [f64; 3], n: [f64; 3], material: &str) -> String {
    op(doc, serde_json::json!({ "op": "draw_rectangle", "corner": a, "opposite": b, "normal": n, "material": material })).unwrap()[0].clone()
}
/// The texture coordinate of the corner of group `g` (front side) nearest to `p`.
fn uv_at(scene: &plannr_core::CompiledScene, material: &str, back: bool, p: [f64; 3]) -> [f32; 2] {
    let g = scene.groups.iter().find(|g| g.material == material && g.back == back).expect("the material's faces");
    assert_eq!(g.uvs.len() / 2, g.positions.len() / 3, "one texture coordinate per corner");
    let i = (0..g.positions.len() / 3).min_by(|a, b| {
        let d = |i: &usize| (0..3).map(|k| (g.positions[i * 3 + k] as f64 - p[k]).powi(2)).sum::<f64>();
        d(a).partial_cmp(&d(b)).unwrap()
    }).unwrap();
    [g.uvs[i * 2], g.uvs[i * 2 + 1]]
}
fn near(a: [f32; 2], b: [f64; 2]) -> bool { (a[0] as f64 - b[0]).abs() < 1e-5 && (a[1] as f64 - b[1]).abs() < 1e-5 }

#[test]
fn a_texture_keeps_the_proportions_of_its_image_and_bad_images_are_refused() {
    let mut doc = Document::default();
    let id = brick(&mut doc, 2.0);
    let t = doc.material_by_ref(&id).unwrap().texture.clone().unwrap();
    assert_eq!((t.width, t.height), (2.0, 1.0), "a 2 × 1 image, 2 m across, is 1 m high");
    // a width alone keeps the proportions, a height stretches
    op(&mut doc, serde_json::json!({ "op": "update_material", "id": id, "texture": { "width": 1.0 } })).unwrap();
    let t = doc.material_by_ref(&id).unwrap().texture.clone().unwrap();
    assert_eq!((t.width, t.height), (1.0, 0.5));
    op(&mut doc, serde_json::json!({ "op": "update_material", "id": id, "texture": { "height": 3.0 } })).unwrap();
    assert_eq!(doc.material_by_ref(&id).unwrap().texture.as_ref().unwrap().height, 3.0);
    // leaving the texture out keeps it, null takes it away
    op(&mut doc, serde_json::json!({ "op": "update_material", "id": id, "name": "Old brick" })).unwrap();
    assert!(doc.material_by_ref(&id).unwrap().texture.is_some());
    op(&mut doc, serde_json::json!({ "op": "update_material", "id": id, "texture": null })).unwrap();
    assert!(doc.material_by_ref(&id).unwrap().texture.is_none());
    // not an image, no image, a size of nothing
    for bad in [serde_json::json!({ "image": "data:image/svg+xml;base64,AAAA" }), serde_json::json!({ "image": "https://example.com/brick.png" }), serde_json::json!({ "width": 2.0 }), serde_json::json!({ "image": PNG, "width": 0.0 })] {
        assert!(op(&mut doc, serde_json::json!({ "op": "update_material", "id": id, "texture": bad })).is_err(), "{bad} is refused");
    }
    assert!(apply(&mut doc, &Op::AddMaterial { name: "Plain".into(), color: "#ffffff".into(), opacity: None, texture: Some(TextureArg::default()) }, &reg()).is_err());
}

#[test]
fn the_image_runs_level_across_a_wall_and_up_it_and_on_into_the_next_face() {
    let mut doc = Document::default();
    let m = brick(&mut doc, 2.0); // 2 m across, 1 m high
    // a wall facing +z, and a second one beside it in the same plane
    rect(&mut doc, [0.0, 0.0, 0.0], [4.0, 3.0, 0.0], [0.0, 0.0, 1.0], &m);
    rect(&mut doc, [4.0, 0.0, 0.0], [6.0, 3.0, 0.0], [0.0, 0.0, 1.0], &m);
    let c = compile(&doc, &reg(), &CompileOptions::default());
    assert!(near(uv_at(&c, &m, false, [0.0, 0.0, 0.0]), [0.0, 0.0]));
    assert!(near(uv_at(&c, &m, false, [4.0, 3.0, 0.0]), [2.0, 3.0]), "x runs across, y runs up");
    assert!(near(uv_at(&c, &m, false, [6.0, 3.0, 0.0]), [3.0, 3.0]), "the next face carries on where the first stops");
    // an untextured material has no coordinates at all
    let plain = c.groups.iter().find(|g| g.material != m).unwrap();
    assert!(plain.uvs.is_empty());
    // a floor: x across, away from the viewer up the image
    let mut doc = Document::default();
    let m = brick(&mut doc, 2.0);
    rect(&mut doc, [0.0, 0.0, 0.0], [4.0, 0.0, -2.0], [0.0, 1.0, 0.0], &m);
    let c = compile(&doc, &reg(), &CompileOptions::default());
    assert!(near(uv_at(&c, &m, false, [4.0, 0.0, -2.0]), [2.0, 2.0]));
}

#[test]
fn the_back_reads_the_right_way_round_and_a_group_takes_its_texture_along() {
    let mut doc = Document::default();
    let m = brick(&mut doc, 2.0);
    let f = rect(&mut doc, [0.0, 0.0, 0.0], [4.0, 3.0, 0.0], [0.0, 0.0, 1.0], &m);
    op(&mut doc, serde_json::json!({ "op": "paint", "ids": [f], "material": m, "side": "back" })).unwrap();
    let c = compile(&doc, &reg(), &CompileOptions::default());
    // seen from behind, x runs to the left: the image is not mirrored
    assert!(near(uv_at(&c, &m, true, [4.0, 3.0, 0.0]), [-2.0, 3.0]));
    // in a group that is moved, the corners keep their place in the image
    let g = op(&mut doc, serde_json::json!({ "op": "make_group", "ids": [f] })).unwrap()[0].clone();
    op(&mut doc, serde_json::json!({ "op": "move", "ids": [g], "delta": [10.0, 5.0, 0.0] })).unwrap();
    let c = compile(&doc, &reg(), &CompileOptions::default());
    assert!(near(uv_at(&c, &m, false, [14.0, 8.0, 0.0]), [2.0, 3.0]));
}

#[test]
fn a_glb_carries_the_image_and_the_coordinates() {
    let mut doc = Document::default();
    let m = brick(&mut doc, 2.0);
    rect(&mut doc, [0.0, 0.0, 0.0], [4.0, 3.0, 0.0], [0.0, 0.0, 1.0], &m);
    let c = compile(&doc, &reg(), &CompileOptions::default());
    let glb = export_glb_textured(&c, &doc.materials);
    let n = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
    let json: serde_json::Value = serde_json::from_slice(&glb[20..20 + n]).unwrap();
    assert_eq!(json["images"][0]["mimeType"], "image/png");
    let textured = json["materials"].as_array().unwrap().iter().find(|m| m["name"] == "Brick").unwrap();
    assert_eq!(textured["pbrMetallicRoughness"]["baseColorTexture"]["index"], 0);
    let prim = json["meshes"].as_array().unwrap().iter().find(|m| m["name"] == "Brick").unwrap()["primitives"][0].clone();
    assert!(prim["attributes"]["TEXCOORD_0"].is_number());
    // the image file itself is in the binary part
    let view = &json["bufferViews"][json["images"][0]["bufferView"].as_u64().unwrap() as usize];
    let at = 28 + n + view["byteOffset"].as_u64().unwrap() as usize;
    assert_eq!(&glb[at..at + 8], b"\x89PNG\r\n\x1a\n");
    // without the materials, nothing of it
    let plain: serde_json::Value = { let g = plannr_core::export_glb(&c); let n = u32::from_le_bytes(g[12..16].try_into().unwrap()) as usize; serde_json::from_slice(&g[20..20 + n]).unwrap() };
    assert!(plain.get("images").is_none());
}

#[test]
fn a_document_drops_a_texture_it_cannot_take() {
    let mut doc = Document::default();
    let m = brick(&mut doc, 2.0);
    doc.materials.push(Material { id: "x".into(), name: "Odd".into(), color: "#888888".into(), opacity: 1.0, texture: Some(Texture { image: "data:text/html;base64,AAAA".into(), width: 1.0, height: 1.0 }) });
    doc.normalize();
    assert!(doc.material_by_ref(&m).unwrap().texture.is_some());
    assert!(doc.material_by_ref("x").unwrap().texture.is_none());
    // and it survives a round trip through JSON
    let again: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(again.materials, doc.materials);
}
