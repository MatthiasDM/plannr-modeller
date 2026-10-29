# Plannr — contract between the Rust engine, the browser and the MCP

Plannr is a generic 3D modeller in the spirit of SketchUp. The **Rust server is the single source of
truth**: it owns every project document, applies every modelling operation (`Op`), keeps the undo
history, tessellates geometry for display, exports, and speaks MCP. The browser is a viewport and a
tool set that turns mouse gestures into the same `Op`s an MCP client sends. Nothing domain-specific
(houses, rooms, …) lives in the core: that arrives through plugins.

Port **8795**. SQLite `data/plannr.db`. Static frontend from `web/dist`. Bearer token for `/mcp`
from `PLANNR_TOKEN` (generated and printed at startup when unset).

Coordinates: metres, right-handed, **y up** (three.js convention). Axis colours: x red, y blue (up),
z green. All points are `[x, y, z]`. Ids are strings (UUID v4). JSON is camelCase.

## Document

```jsonc
{
  "version": 12,                       // increments on every committed change
  "units": "m",
  "layers":    [{ "id": "uuid", "name": "Layer0", "visible": true, "color": "#7a8aa0" }],
  "materials": [{ "id": "uuid", "name": "Brick", "color": "#B0623F", "opacity": 1.0 }],
  "entities":  [ /* Entity, see below */ ],
  "scenes":    [{ "id": "uuid", "name": "Scene 1", "camera": [px,py,pz, tx,ty,tz, ux,uy,uz], "section": "sectionId|null" }],
  "components": [{ "id": "uuid", "name": "Window 120", "description": "", "entities": [ /* Entity, local coords */ ] }],  // definitions shared by instances
  "extensions": { "<pluginId>": {} }   // opaque per-plugin data, preserved as-is
}
```

The default document has one layer `Layer0` and one material `Default` (`#F4F2EE`).

### Entities (tagged by `kind`; every entity has `id` and `layer`)

| kind | fields | notes |
|---|---|---|
| `face` | `outer: [[x,y,z]…]`, `holes: [[[x,y,z]…]…]`, `material?`, `backMaterial?` | planar polygon, counter-clockwise seen from the front (normal side) |
| `edge` | `a`, `b` | a line segment; faces also own their outline implicitly, edges are the "loose" lines |
| `group` | `name`, `transform: [16 numbers, column-major]`, `entities: [Entity…]` | children in local coordinates |
| `guide` | `point`, `direction?` | guide point, or an infinite guide line when `direction` is set |
| `dimension` | `a`, `b`, `offset: [x,y,z]` | linear dimension between two points, drawn offset from them |
| `text` | `position`, `text`, `anchor?` | a label; with `anchor` a leader is drawn to that point |
| `section` | `origin`, `normal`, `active: bool` | section plane; when active, geometry on the +normal side is cut away |
| `custom` | `type: "<plugin>.<name>"`, `data: {}` | plugin entity; the core stores it and asks the plugin registry to expand it into core entities at compile time |
| `instance` | `definition: id`, `transform: [16]`, `name?` | a placed copy of a `components[]` definition; compiles to the definition's entities under `transform` with ids `<instanceId>#<entityId>` (so picking resolves to the instance). `move`/`rotate`/`scale`/`copy`/`mirror`/`erase`/`set_layer`/`make_group` act on the instance's transform; `explode` replaces it by world-coordinate copies; `paint` on `<instanceId>#<faceId>` paints the **definition** face, i.e. every instance |

Unknown fields on any entity are preserved on round-trip.

## Ops

`POST /api/projects/:id/ops` with `{ "ops": [Op…], "label": "Push/Pull" }`. All ops in one request
form **one undo step**, applied atomically (any failure rolls the request back, 400 with `error`).
Response: `{ "version": 13, "results": [OpResult…] }` where
`OpResult = { "created": [ids], "modified": [ids], "removed": [ids], "measurement"?: number, "entities"?: [Entity…] }`
(`entities` echoes created/modified entities so clients can show them without a refetch).

`POST /api/projects/:id/preview` with the same body returns the **CompiledScene** of the document with
the ops applied but **not committed** (live drag feedback). `POST /api/projects/:id/undo` and `/redo`
return `{ "version", "label" }`.

Ops are tagged by `op`. Optional `layer` defaults to the first layer. Points on a face's plane are
accepted within 1 mm.

| op | fields | effect |
|---|---|---|
| `draw_line` | `points: [p…] (≥2)`, `close?: bool` | adds edges between consecutive points. After adding, if the new edges together with existing loose coplanar edges form a closed loop, a `face` is created from that loop (SketchUp's "face from loop"). Returns created edge ids and any face id. |
| `draw_rectangle` | `corner`, `opposite`, `normal`, `u?` | a rectangle in the plane through `corner` with `normal`; sides along `u` (default: world x projected on the plane, or world z when degenerate) and `normal × u`. Creates one face (no loose edges). |
| `draw_polygon` | `points (≥3)` | one planar face. |
| `draw_circle` | `center`, `normal`, `radius`, `segments?: 24` | one regular polygon face. |
| `push_pull` | `face`, `distance` | extrudes the face along its normal by `distance` (negative pushes). The original face keeps its id and moves to the new cap; side faces and the base face are created; the base face is the original outline with the normal flipped. Result: created ids. |
| `offset` | `face`, `distance` | new face whose outline is the original offset in-plane (positive = outward). |
| `move` | `ids`, `delta` | translates entities (groups move as a whole). |
| `rotate` | `ids`, `origin`, `axis`, `angle` (degrees) | rotates about the axis through origin. |
| `scale` | `ids`, `origin`, `factors: [sx,sy,sz]` | scales about origin. |
| `copy` | `ids`, `delta`, `count?: 1` | duplicates with each copy offset by `delta` more; returns new ids. |
| `erase` | `ids` | removes entities (a group's children with it). |
| `paint` | `ids`, `material`, `side?: "front"\|"back"\|"both"` | sets face materials; on a group paints all faces inside. `material` may be a material id or a `#hex` colour (then a material is created on the fly). |
| `set_layer` | `ids`, `layer` | moves entities to a layer. |
| `make_group` | `ids`, `name?` | wraps entities in a group (identity transform, world coords). |
| `explode` | `id` | dissolves a group into its parent, applying its transform. |
| `add_layer` / `update_layer` / `remove_layer` | `name`, `color?` / `id`, `name?`, `visible?`, `color?` / `id` | entities on a removed layer go to the first layer. |
| `add_material` / `update_material` | `name`, `color`, `opacity?` / `id`, … | |
| `add_guide` | `point`, `direction?` | |
| `add_dimension` | `a`, `b`, `offset` | |
| `add_text` | `position`, `text`, `anchor?` | |
| `add_section` / `update_section` | `origin`, `normal`, `active?` / `id`, `origin?`, `normal?`, `active?` | activating one section deactivates the others. |
| `slice_section` | `id`, `layer?`, `name?` | a 2D slice (SketchUp's "Create Group from Slice"): every face of the model — plugin geometry included, hidden tags left out — is cut by section plane `id` (active or not), and the cut lines become edges in a new group in the plane (`name` default "Slice N", on a "Slices" tag made when missing). Pieces of one line are merged; joints between touching solids of the same material are dropped, those between different materials stay. Errors when the plane cuts nothing. |
| `update_entity` | `id`, `patch: {}` | shallow-merge arbitrary fields (name, text, outer, …) after validation. |
| `measure` | `a`, `b` | no change; `measurement` = distance. |
| `draw_arc` | centre arc `center`, `start`, `end`, `pie?` · three-point `through: [p1,p2,p3]` · two-point `from`, `to`, `bulge` (signed sagitta towards normal × chord), `closeChord?` · all: `normal?`, `segments?` (1–200, default 12 per quarter turn), `layer?` | edges along the arc (faces form through loop detection as with `draw_line`); `pie` adds the radii + sector face, `closeChord` the chord + segment face. Arc edges carry `arc: { center, radius, normal }` metadata. `measurement` = radius. |
| `follow_me` | `face`, `path: [p…]` or `pathEdges: [edgeId…]` | sweeps the face along the path (profile turned perpendicular to the first segment, mitred at bends); side quads + end cap are created, the original face becomes the start cap and keeps its id; a closed path (first = last point) builds a ring without caps and removes the profile face. |
| `mirror` | `ids`, `origin`, `normal`, `copy?` | reflects across the plane; face windings are reversed so normals stay outward, groups/instances get a mirrored transform (the compiler un-flips windings under a negative determinant), custom entities go through the plugin's `transform`. `copy` keeps the originals and returns the new ids. |
| `make_component` | `ids`, `name`, `description?` | a definition (origin = min corner of the entities' bounding box, world axes) + one instance replacing the entities; `created` = instance id, `entities[0].definition` = definition id. |
| `place_component` | `definition` (id or name), `position`, `rotation?` (° about y), `scale?` (number or `[sx,sy,sz]`), `layer?` | a new instance. |
| `make_unique` | `instance` | copies the definition and points the instance at the copy. |
| `edit_component` | `definition`, `ops: [Op…]` | applies core ops to the definition's entities (ids inside are stable); all instances update; `modified` lists the instances. |
| `rename_component` / `remove_component` | `definition`, `name` / `definition`, `force?` | removing fails while instances exist unless `force` erases them. |
| `set_extension` | `plugin`, `data` | stores plugin data. |
| `custom` | `type`, `data` | routed to the plugin registered for `type`'s prefix; 400 `no plugin for <type>` otherwise. |

Geometry rules: faces are validated planar and non-degenerate; outer loops are re-oriented so the
normal follows the given winding (for `draw_rectangle` the normal passed). Edge ids are stable.
`erase` of a face leaves its outline edges only if they were loose edges before.

## Compiled scene (what the browser renders)

`GET /api/projects/:id/compile` (query `section=0` to ignore sections) or `POST /api/compile { document }`:

```jsonc
{
  "version": 13,
  "groups": [ { "material": "id", "name": "Brick", "color": "#B0623F", "opacity": 1, "back": false,
                "positions": [..], "normals": [..], "indices": [..],
                "ids": ["faceId", ...], "idRanges": [[startIndex, count], ...] } ],
  "edges": { "positions": [x,y,z,x,y,z, ...], "ids": ["edgeOrFaceId", ...] },   // one id per segment (2 points)
  "guides": [ { "id", "point", "direction" } ],
  "dimensions": [ { "id", "a", "b", "offset", "value" } ],
  "texts": [ { "id", "position", "text", "anchor" } ],
  "sections": [ { "id", "origin", "normal", "active" } ],
  "bounds": { "min": [..], "max": [..] },
  "metrics": { "faces": 12, "edges": 30, "groups": 1, "instances": 0, "definitions": 0, "area": 86.4, "areaByMaterial": { "Brick": 12.0 }, "edgeLength": 72.3 }
}
```

Groups' transforms are baked into world coordinates. Faces whose layer is hidden are skipped. Both
sides of a face render: a back-face group (`back: true`) uses `backMaterial` or the standard back colour
`#B7C1CC`. An active section plane drops every triangle and edge segment fully on its cut-away side and
clips those that cross it. Face outlines are emitted as edge segments with the face id so the SketchUp
black-edge look needs no extra work in the browser.

## REST summary

| Method | Path | Body → Response |
|---|---|---|
| GET | `/api/health` | `{ ok, version, name: "plannr" }` |
| GET | `/api/projects` | `[{ id, name, createdAt, updatedAt, version, entityCount }]` |
| POST | `/api/projects` | `{ name, document? }` → summary |
| GET | `/api/projects/:id` | `{ id, name, createdAt, updatedAt, document }` |
| PATCH | `/api/projects/:id` | `{ name }` → summary |
| PUT | `/api/projects/:id/document` | Document → `{ version }` (replace; one undo step) |
| DELETE | `/api/projects/:id` | 204 |
| POST | `/api/projects/:id/duplicate` | → summary |
| POST | `/api/projects/:id/ops` | see Ops |
| POST | `/api/projects/:id/preview` | `{ ops }` → CompiledScene |
| POST | `/api/projects/:id/undo`, `/redo` | → `{ version, label }` |
| GET | `/api/projects/:id/history` | `[{ version, label, at }]` |
| GET | `/api/projects/:id/compile` | → CompiledScene |
| POST | `/api/compile` | `{ document }` → CompiledScene |
| GET | `/api/projects/:id/entities?kind=&layer=` | `[Entity]` (flattened, with `path` of group ids; `kind=instance` lists placed components) |
| GET | `/api/projects/:id/export.glb` / `export.json` | downloads |
| GET | `/api/plugins` | `[{ id, name, entityTypes: [], ops: [] }]` registered Rust plugins |
| POST | `/api/sample` | creates a demo project (a few boxes, a circle, a pushed-pulled L-shape, a dimension, a text, a group, two materials) |
| GET | `/ws/projects/:id` | WebSocket, see below |
| POST | `/mcp` | MCP Streamable HTTP (JSON-RPC), see below |

## WebSocket `/ws/projects/:id`

Client → server: `{ "type": "hello", "client": "browser", "name": "Chrome on Mac" }`, `{ "type": "reply", "reqId", "result"? , "error"? }`,
`{ "type": "state", "camera": [9], "selection": [ids], "tool": "select", "hover": id|null }` (the browser sends this whenever it changes; the server keeps the latest per connection).

Server → client: `{ "type": "changed", "version", "label", "source": "browser|mcp|api" }` after every commit (clients refetch compile),
`{ "type": "command", "reqId", "command": { "name": "...", ...args } }`.

Commands a browser must answer (names match the MCP camera tools): `screenshot {width?, height?}` → `{ "png": base64, "width", "height" }`;
`set_camera {position, target, up?}`; `set_view {view: iso|top|bottom|front|back|left|right}`; `zoom_extents`; `zoom_to {ids}`; `zoom_window {x0, y0, x1, y1}` (viewport fractions 0–1, top-left origin); `previous_view`; `next_view` (camera history);
`orbit {dx, dy}` (pixels); `pan {dx, dy}`; `zoom {factor}`; `select {ids, mode: replace|add|toggle}`; `activate_tool {tool}`;
`get_state` → the state object; `set_options {gridVisible?, axesVisible?, edgesVisible?, shadows?}`. Reply `result: {}` when nothing to return.
The server picks the most recently active browser connection; with none connected a command fails with
`no browser connected — open http://localhost:8795/?project=<id>`.

## MCP (`POST /mcp`, `Authorization: Bearer <token>`)

JSON-RPC 2.0 Streamable HTTP, JSON responses (no SSE needed). Methods: `initialize` (server `plannr`,
capabilities `tools`), `notifications/initialized`, `ping`, `tools/list`, `tools/call`. Errors as MCP tool
results with `isError: true`. Every tool takes an optional `project` (id or name); otherwise the
**active project** set by `use_project` is used (the first project when none was set). Points are
`[x,y,z]` arrays.

Tools (name → maps to):
- Projects: `list_projects`, `create_project {name}`, `use_project {project}`, `get_document {project?, includeEntities?: true}`, `list_entities {kind?, layer?}`, `get_entity {id}`, `get_metrics`, `undo`, `redo`, `history`.
- Every op above as its own tool with the same fields: `draw_line`, `draw_rectangle`, `draw_polygon`, `draw_circle`, `draw_arc`, `push_pull`, `offset`, `follow_me`, `move`, `rotate`, `scale`, `mirror`, `copy`, `erase`, `paint`, `set_layer`, `make_group`, `explode`, `make_component`, `place_component`, `make_unique`, `edit_component`, `rename_component`, `remove_component`, `add_layer`, `update_layer`, `remove_layer`, `add_material`, `update_material`, `add_guide`, `add_dimension`, `add_text`, `add_section`, `update_section`, `slice_section`, `update_entity`, `measure`, `set_extension`, `custom`. Each returns the OpResult as JSON text and the new version. `list_components` lists the definitions with entity and instance counts.
- `apply_ops {ops: [Op…], label?}` for several ops in one undo step.
- Viewport (through the browser WebSocket): `screenshot {width?, height?, view?}` → **image content** (PNG) plus a text line with the camera; `set_view`, `set_camera`, `zoom_extents`, `zoom_to`, `zoom_window`, `previous_view`, `next_view`, `orbit`, `pan`, `zoom`, `select`, `activate_tool`, `get_view_state`, `set_options`.
- `export_glb {path?}` → writes `data/exports/<project>.glb` (or `path`) and returns the path, size and download URL.
- `list_plugins`.

Tool descriptions must tell the model the coordinate convention (y up, metres) and that `screenshot`
needs a browser open on the project.

## Plugins

Rust: `plannr_core::plugin::Plugin` trait — `fn id(&self) -> &str`, `fn expand(&self, e: &CustomEntity, doc: &Document) -> Result<Vec<Entity>>`
(core entities to tessellate in place of the custom entity), `fn apply(&self, op_type: &str, data: Value, doc: &mut Document) -> Result<OpResult>`
for `custom` ops with its prefix, `fn describe(&self) -> PluginInfo` (name, entity types, op types, each op with a JSON schema so the MCP can
expose it as `plugin.<id>.<op>`). A `Registry` holds `Vec<Box<dyn Plugin>>`; `plannr-server` registers the plugins listed in `crates/plannr-server/src/plugins.rs` (empty by default).

Browser: `window.plannr.registerPlugin({ id, name, tools?: Tool[], panels?: Panel[], shortcuts? })`; plugin tools use the same `Tool` interface and `ToolContext` as the built-in tools, send ops through `ctx.ops(...)`, and may read `ctx.document()`.
