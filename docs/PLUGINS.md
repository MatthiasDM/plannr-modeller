# Writing a Plannr plugin

The core models faces, edges, groups, guides, dimensions, texts and section planes — nothing else.
Anything domain-specific (walls, rooms, pipes, furniture, …) is a plugin. A plugin can live on either
side, or both:

| Side | Can add | Where |
|---|---|---|
| **Engine (Rust)** | custom entity types that expand into core geometry at compile time; custom ops (exposed automatically as MCP tools `plugin.<id>.<op>`); transform / paint / metrics / export hooks | implement `plannr_core::plugin::Plugin` and end the crate with `plannr_core::wasm_plugin!(…)` (an installable package), or register it in `crates/plannr-server/src/plugins.rs` (compiled in) |
| **Browser (TypeScript)** | tools (same `Tool` interface as Line, Push/Pull…), tray panels, an Entity Info editor for its entities, overlays on the viewport | `<id>/web/index.ts`, the package's `web.js`; the server lists it in `GET /api/plugins` as `script` and the app loads it at startup. Any other script can also call `window.plannr.registerPlugin({...})` |

Plannr's plugins live in [plannr-plugins](https://github.com/MatthiasDM/plannr-plugins), each in `<id>/`
with `engine/` (Rust crate) and `web/` (browser bundle), packaged and installed as described under
[Installable plugins](#installable-plugins-packages). **Construction** (`construction/`, contract in its
README) is the reference: storeys, rooms, walls, openings, roofs, fixtures and runs on top of the core.
**Electrical**, **Framing** and **Plumbing** build on it and are smaller examples of the same two halves.

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

To package it, end the crate with `plannr_core::wasm_plugin!(Walls);` (see below). To compile it into a
server instead, register it in `crates/plannr-server/src/plugins.rs`:

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

A tool that works on an existing object picks it with `ctx.pickTarget(ev, { accept })` instead of
`ctx.scene.pick(ev)`: once the user has selected objects that `accept` takes (say, the plugin's own walls),
only those can be hovered or clicked and everything else is see-through; with none selected it picks
whatever is under the cursor. `ctx.targets(accept)` lists those selected objects. Move, Push/Pull,
Offset, the framing opening tools and the plumbing Adjust and Bracket tools work this way.

An entity built of generated parts (the framing plugin's studs and plates) can open like a group: declare
`parts: { types, label?, erase?, render? }` on the plugin. A double-click then opens one of those entities, a
click inside selects a single part as `<entityId>#<key>` (the compiled id), Delete calls `erase(ctx, entity,
keys)` for the ops that take parts out, and Entity Info calls `render(ctx, entity, keys, el)`. Move, Rotate,
Scale and Flip refuse a selection of parts.

Show the result before the click: call `ctx.preview(ops)` on hover with the same ops the click commits
(the engine compiles the document with them applied, nothing is saved) and `ctx.preview([])` when there is
nothing to place or the tool deactivates. The previewed geometry is part of the scene while it shows, so
`pickTarget` skips it and `ctx.inDocument(id)` tells it apart (use `ignoreIf: id => !ctx.inDocument(id)` in
`snap`). A preview can also remove what the cursor was on (an opening is a hole): keep reading the cursor
on the surface last hovered, as the framing opening tools do, or the preview switches itself off.

A plugin that needs no new geometry types can use only the browser side and emit core ops
(`draw_rectangle`, `push_pull`, `make_group`…): a "house" plugin would draw rooms as grouped faces.

## Export formats

An engine plugin can write files: `Plugin::exports()` lists its formats (`ExportFormat { format, label,
mime, description, options_schema }`) and `Plugin::export(format, scene, doc, options)` returns an
`ExportFile { bytes, report }`. The server compiles the document first (plugin geometry expanded, hidden
entities and section cuts left out, only the requested `ids` when given — nested ids count as their
top-level entity) and serves the result at `GET /api/projects/:id/export/<format>?ids=a,b&<option>=…`
(`report=1` returns just the JSON report; the download carries it in an `X-Plannr-Report` header). The
MCP tool `export_file { format, ids?, options?, path? }` writes it to `data/exports/` and returns the
path and report. `GET /api/plugins` lists a plugin's formats as `exports`. The **3D Print** plugin in
[plannr-plugins](https://github.com/MatthiasDM/plannr-plugins/tree/main/print3d) is the example: `stl` and `3mf`.

## Installable plugins (packages)

A plugin doesn't have to be compiled into the server. Packaged, it is one `.plannr-plugin` file that
anyone can download (the [plannr-plugins releases](https://github.com/MatthiasDM/plannr-plugins/releases))
and install with **Plugins → Manage plugins… → Install from file** (or by dropping the file on that
dialog). The same dialog switches every plugin — built in or installed — on and off, and removes installed
ones. A plugin that is off is skipped everywhere: its entities stay in the documents but aren't drawn, its
ops, MCP tools and exports are gone, its browser half isn't loaded; switched back on, everything is there
again. Changes apply when the app reloads (the dialog offers it).

**The package** is a zip with:

| file | |
|---|---|
| `plugin.json` | `{ id, name, version, description, author?, homepage?, license?, plannr?: "0.2.0" (oldest server it runs on), suggests?: [ids] }` |
| `engine.wasm` | optional: the engine half compiled to WebAssembly (`wasm32-wasip1`) |
| `web.js` | optional: the browser half, a self-contained ES module (it reaches the app through `window.plannr`) |
| `README.md`, `LICENSE` | optional |

**Making one.** [plannr-plugins](https://github.com/MatthiasDM/plannr-plugins) is the template: Plannr
as a git submodule, and per plugin `plugin.json`, an engine crate and a browser half. In the engine crate,
`crate-type = ["rlib", "cdylib"]` under `[lib]` and, at the end of `lib.rs`,
`plannr_core::wasm_plugin!(MyPlugin);` — that exports the plugin to the sandbox and does nothing in a
native build, so `cargo test` keeps working. `scripts/package.sh <id>` there builds the engine for
`wasm32-wasip1`, the browser half with Vite, and zips them into `dist/plannr-plugin-<id>-<version>.plannr-plugin`;
`scripts/release.sh <id>` publishes it as a GitHub release. A package can't take the id of a plugin
built into the server.

**How it runs.** The server installs packages into `data/plugins/<id>/` (`PLANNR_PLUGINS` to move it) and
loads them at startup. The engine runs in a wasmtime sandbox: no files, network or environment (WASI gives
it a clock, random numbers and stderr), a 2 GB memory cap and a fuel budget per call; a panic, a runaway
loop or running out of memory is an error for that call and the module is started afresh. Calls carry
JSON; before a compile the document is sent once (`Plugin::prepare`), then each `expand` sends just the
entity. The ABI is in `crates/plannr-core/src/guest.rs`. A package is checked before anything is written:
it must be a zip with a valid `plugin.json`, its engine must load and say the same id.

**Who may manage plugins.** On a private instance anyone who reaches the API. On a public instance
(`PLANNR_PUBLIC=1`) installing, switching and removing need the server's token (`Authorization: Bearer
<PLANNR_TOKEN>`; the dialog asks for it), because a plugin's browser half runs in every visitor's page.

## Aliases

A plugin can own more than one type prefix: `Plugin::aliases()` returns extra prefixes, and the
registry routes `<alias>.*` entity types and ops to it (exact ids win over aliases). The construction
plugin uses this for `masonry.*` (the masonry engine is one of its modules).

A plugin that builds on another's crate (electrical on construction) links it as a library: the library
puts its `wasm_plugin!` behind a feature (`#[cfg(not(feature = "embedded"))]`) that the dependant turns on,
so only one plugin's exports end up in the module. Declare the dependency in `plugin.json` as
`requires: ["construction"]`; the plugin manager says when it is missing or off.

