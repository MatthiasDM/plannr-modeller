# Writing a Plannr plugin

The core models faces, edges, groups, guides, dimensions, texts and section planes — nothing else.
Anything domain-specific (walls, rooms, pipes, furniture, …) is a plugin. A plugin can live on either
side, or both:

| Side | Can add | Where |
|---|---|---|
| **Engine (Rust)** | custom entity types that expand into core geometry at compile time; custom ops (exposed automatically as MCP tools `plugin.<id>.<op>`); transform / paint / metrics hooks | implement `plannr_core::plugin::Plugin`, register it in `crates/plannr-server/src/plugins.rs` |
| **Browser (TypeScript)** | tools (same `Tool` interface as Line, Push/Pull…), tray panels, an Entity Info editor for its entities, overlays on the viewport | `plugins/<id>/web/index.ts`, built by Vite to `web/dist/plugins/<id>.js`; the server lists it in `GET /api/plugins` as `script` and the app loads it at startup. Any other script can also call `window.plannr.registerPlugin({...})` |

A plugin lives in `plugins/<id>/` with `engine/` (Rust crate) and `web/` (browser bundle). The
**construction** plugin (`plugins/construction`, contract in [CONSTRUCTION.md](CONSTRUCTION.md)) is the
reference: storeys, rooms, walls, openings, roofs, fixtures and runs on top of the core.

### How the two sides meet

- Plugin entities are stored as `{ "kind": "custom", "type": "<id>.<name>", "data": {…} }`.
- The engine plugin's `expand()` turns each one into core faces whose ids are `<entityId>#<faceKey>`. Picking
  such a face in the browser resolves to the custom entity (the core and the browser strip `#…`), so Select,
  Move, Erase and Paint work on plugin entities without the plugin writing a line of UI code.
- `move` / `rotate` / `scale` / `copy` call the plugin's `transform(entity, matrix)`; `paint` calls `paint(entity, faceKey, material, side)`.
- `metrics(doc)` is merged into `CompiledScene.metrics.plugins.<id>`; `on_load(doc)` runs when a document is created or loaded (ensure materials, defaults).
- Browser tools send ops as `{ op: 'custom', type: '<id>.<name>', data }`; the same ops are the MCP tools `plugin.<id>.<name>`.

## Engine plugin

```rust
use plannr_core::plugin::{Plugin, PluginInfo, OpInfo};
use plannr_core::{model::*, ops::OpResult};

pub struct Walls;
impl Plugin for Walls {
    fn id(&self) -> &str { "walls" }
    fn describe(&self) -> PluginInfo {
        PluginInfo { id: "walls".into(), name: "Walls".into(), entity_types: vec!["walls.wall".into()],
            ops: vec![OpInfo { name: "draw".into(), description: "Draw a wall between two points".into(),
                input_schema: serde_json::json!({ "type": "object", "properties": { "a": {...}, "b": {...}, "height": {...} }, "required": ["a","b"] }) }] }
    }
    /// Turn a `custom` entity of one of your types into core faces/edges for display and export.
    fn expand(&self, e: &CustomEntity, _doc: &Document) -> anyhow::Result<Vec<Entity>> { /* build faces */ }
    /// Handle `custom` ops with your prefix (`walls.draw`): mutate the document, return what changed.
    fn apply(&self, op: &str, data: serde_json::Value, doc: &mut Document) -> anyhow::Result<OpResult> { /* push a custom entity */ }
}
```

Register it in `crates/plannr-server/src/plugins.rs`:

```rust
pub fn registry() -> Registry { let mut r = Registry::new(); r.register(Box::new(Walls)); r }
```

Custom entities are stored as `{ "kind": "custom", "type": "walls.wall", "data": { ... } }` and survive
round-trips untouched. The MCP tool `plugin.walls.draw` appears as soon as the server restarts.

## Browser plugin

```js
window.plannr.registerPlugin({
  id: 'walls', name: 'Walls',
  tools: [{
    id: 'wall', label: 'Wall', hint: 'Click two points.', cursor: 'crosshair', shortcut: 'w',
    icon: '<svg viewBox="0 0 24 24">…</svg>',
    onDown(ev, ctx) {
      const s = ctx.snap(ev, { faces: true });                 // same inference as the core tools
      // … collect two points, then:
      ctx.ops([{ op: 'custom', type: 'walls.draw', data: { a, b, height: 2.6 } }], 'Wall');
    },
    onMove(ev, ctx) { ctx.scene.clearOverlay(); ctx.scene.addDot(ctx.snap(ev).point); },
  }],
  panels: [{ id: 'walls', title: 'Walls', render(ctx, el) { el.textContent = `${ctx.doc().entities.length} entities`; } }],
  activate(host) { host.on('changed', doc => console.log('document changed', doc.version)); },
});
```

`ToolContext` (see `web/src/tools/base.ts`) gives a plugin everything the built-in tools use: the scene
(picking, overlays, labels), the document, `ops()` to commit, `preview()` for live feedback, `snap()`,
selection, the Measurements box (`setVCB`/`onVCB`) and the axis lock.

A plugin that needs no new geometry types can use only the browser side and emit core ops
(`draw_rectangle`, `push_pull`, `make_group`…): a "house" plugin would draw rooms as grouped faces.

## Aliases

A plugin can own more than one type prefix: `Plugin::aliases()` returns extra prefixes, and the
registry routes `<alias>.*` entity types and ops to it (exact ids win over aliases). The construction
plugin uses this for `masonry.*` (the masonry engine is one of its modules).

