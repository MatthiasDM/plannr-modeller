# Rust crates

| Crate | Role |
|---|---|
| `plannr-core` | The geometry kernel, no async. `model` (the generic document: layers, materials, faces / edges / groups / guides / dimensions / texts / section planes / plugin entities; unknown JSON fields round-trip), `ops` (every modelling operation — `apply` is the only way a document changes), `history` (undo/redo snapshots), `compile` (tessellation into per-material triangle groups + edge segments + annotations, section clipping, metrics), `gltf` (GLB export), `plugin` (the `Plugin` trait and `Registry`), `sample` (a demo document built from ops). |
| `plannr-server` | Axum server: REST API under `/api`, the WebSocket viewport channel under `/ws`, the MCP endpoint at `/mcp`, SQLite persistence, static hosting of `web/dist` with an SPA fallback. See `../docs/CONTRACT.md` for every route, op and tool. |

## Run

```bash
cargo run --release -p plannr-server            # http://localhost:8795
```

Environment: `PORT` (8795), `PLANNR_DB` (`data/plannr.db`), `PLANNR_WEB` (`web/dist`),
`PLANNR_TOKEN` (bearer token for `/mcp`; generated and printed when unset), `RUST_LOG` (`info`).

## Test

```bash
cargo test                 # 14 kernel tests: ops, loops → faces, push/pull, offset, transforms, groups, sections, compile, GLB, history
cargo build --release
```

## MCP

The server prints the exact command at startup:

```bash
claude mcp add --transport http plannr http://localhost:8795/mcp --header "Authorization: Bearer <PLANNR_TOKEN>"
```

Every op is a tool (`draw_rectangle`, `push_pull`, `move`, …), plus project tools (`list_projects`,
`use_project`, `get_document`, `list_entities`, `undo`, `redo`, `apply_ops`, `export_glb`) and viewport
tools that are relayed to a browser open on the project (`screenshot` returns a PNG, `set_view`,
`set_camera`, `zoom_extents`, `zoom_to`, `orbit`, `pan`, `zoom`, `select`, `activate_tool`,
`get_view_state`, `set_options`). Pass `project` (id or name) or call `use_project` once.

## Conventions

* Metres, right-handed, **y up**; points are `[x, y, z]`. Faces are planar polygons wound
  counter-clockwise around their normal; holes wind the other way.
* `push_pull` keeps the pulled face's id on the cap. For a negative distance both cap and base are
  wound so the resulting solid still faces outward.
* Undo/redo restore an earlier snapshot but always **increase** `version`, so clients can treat the
  version as monotonic.
* `make_group` takes entities in the coordinates of the container they were in and places the group at
  the top level; group entities that live in one container.
* Domain concepts (rooms, walls, openings…) do not exist in the core: they are plugins.

## Writing a Rust plugin

```rust
use plannr_core::{Plugin, PluginInfo, Document, Entity, model::Custom, ops::{OpError, OpResult}};
use serde_json::{json, Value};

#[derive(Default)]
pub struct Shapes;

impl Plugin for Shapes {
    fn id(&self) -> &str { "shapes" }
    fn describe(&self) -> PluginInfo {
        PluginInfo { id: "shapes".into(), name: "Shapes".into(), entity_types: vec!["shapes.star".into()],
            ops: vec![plannr_core::plugin::PluginOp { name: "star".into(), description: "Add a star".into(),
                input_schema: json!({ "type": "object", "properties": { "center": {}, "radius": {} }, "required": ["center", "radius"] }) }] }
    }
    fn expand(&self, e: &Custom, _doc: &Document) -> Result<Vec<Entity>, OpError> {
        // turn e.data into faces / edges
        Ok(vec![])
    }
    fn apply(&self, op: &str, data: Value, doc: &mut Document) -> Result<OpResult, OpError> {
        // push an Entity::Custom { type_: "shapes.star", data } into doc.entities, return its id
        Ok(OpResult::default())
    }
}
```

Register it in `plannr-server/src/plugins.rs` (`reg.register(Box::new(Shapes))`). Its ops become MCP
tools named `plugin.shapes.<op>` and are reachable over REST as `custom` ops with `type: "shapes.<op>"`.
