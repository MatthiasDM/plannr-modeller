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
  "units": "m",                // "m" | "cm" | "mm": how lengths are shown and typed in the app (`precision`: decimals); coordinates are always metres
  "axes": { "origin": [x,y,z], "x": [x,y,z], "y": [x,y,z] }, // optional: where the app's drawing axes were moved to (Axes tool): origin, red axis, blue axis. Only the app's drawing aids follow it; coordinates in the document and in ops stay the model's
  "layers":    [{ "id": "uuid", "name": "Layer0", "visible": true, "color": "#7a8aa0" }],
  "materials": [{ "id": "uuid", "name": "Brick", "color": "#B0623F", "opacity": 1.0 }],
  "entities":  [ /* Entity, see below */ ],
  "scenes":    [{ "id": "uuid", "name": "Scene 1", "camera": [px,py,pz, tx,ty,tz, ux,uy,uz], "section": "sectionId|null", "sections": ["sectionId"], "layers?": { "layerId": true }, "faceStyle?": "shaded", "fov?": 35, "sun?": { "byTime": false, "hour": 14, "month": 6, "latitude": 51 } }],
  "components": [{ "id": "uuid", "name": "Window 120", "description": "", "entities": [ /* Entity, local coords */ ] }],  // definitions shared by instances
  "extensions": { "<pluginId>": {} }   // opaque per-plugin data, preserved as-is
}
```

The default document has one layer `Layer0` and one material `Default` (`#F4F2EE`).

### Entities (tagged by `kind`; every entity has `id` and `layer`)

| kind | fields | notes |
|---|---|---|
| `face` | `outer: [[x,y,z]…]`, `holes: [[[x,y,z]…]…]`, `material?`, `backMaterial?`, `smooth?: degrees`, `soft?: [i…]` | planar polygon, counter-clockwise seen from the front (normal side); `soft` lists outer-loop edges (edge i runs from corner i to i+1) left out of the drawn edges, like SketchUp's softened edges (plugin sweeps use it so curved surfaces show no seams). `smooth` (set by `soften`) softens by angle instead: the edge that exactly two faces of one container share is not drawn, and the surface is shaded round across it, where both have `smooth` and their normals differ by more than 0.1° and at most both their `smooth` angles. With hidden geometry on show (`hidden=1`) those edges are drawn |
| `edge` | `a`, `b` | a line segment; faces also own their outline implicitly, edges are the "loose" lines |
| `group` | `name`, `transform: [16 numbers, column-major]`, `entities: [Entity…]` | children in local coordinates |
| `guide` | `point`, `direction?` | guide point, or an infinite guide line when `direction` is set |
| `dimension` | `a`, `b`, `offset: [x,y,z]`, `style?` | linear dimension between two points, drawn offset from them; `style: "radius"` (`a` the centre, `b` on the curve) or `"diameter"` (`a`, `b` across the circle), with `offset` leading from `b` to the label; `"angle"`: `a` is the corner, `b` a point on the first leg, `offset` the vector from the corner to a point on the second (the arc is drawn as far from the corner as `b`) |
| `text` | `position`, `text`, `anchor?`, `screen?` | a label; with `anchor` a leader is drawn to that point; with `screen: [fx, fy]` it is screen text, fixed at that fraction of the view |
| `section` | `origin`, `normal`, `active: bool` | section plane; when active, geometry on the +normal side is cut away (several active planes combine) |
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

**Partial replies** (what the browser uses, so an action never waits for the whole model): `partial: true` in
the ops body, or `?partial=1` on undo/redo (plus `hidden=1` for ghosts), adds what the change did:
`scene` (a CompiledScene of only the top-level entities it touched that still exist), `dirty` (every
top-level id it touched, removed ones too) and `reconcile` (plugin geometry was touched; it can depend on
neighbours the change didn't touch, so fetch the whole scene once idle). Without `scene` (layers, materials,
components or plugin settings changed, a section plane changed while the geometry is cut on the server, or
most of the model did) the client fetches `compile`.

Ops are tagged by `op`. Optional `layer` defaults to the first layer. Points on a face's plane are
accepted within 1 mm. The draw ops (`draw_line`, `draw_rectangle`, `draw_polygon`, `draw_circle`,
`draw_arc`) take an optional `group`: the id of a group to draw inside (points stay in world
coordinates); without it they draw at the top level.

| op | fields | effect |
|---|---|---|
| `draw_line` | `points: [p…] (≥2)`, `close?: bool` | adds edges between consecutive points. After adding, if the new edges together with existing loose coplanar edges form a closed loop, a `face` is created from that loop (SketchUp's "face from loop"), unless a face already lies there. Edges lying on a face divide it wherever they (with the loose edges already on it) close off part of it: the largest part keeps the id (`modified`), the others are new faces. Returns created edge ids and any face ids. |
| `draw_rectangle` | `corner`, `opposite`, `normal`, `u?` | a rectangle in the plane through `corner` with `normal`; sides along `u` (default: world x projected on the plane, or world z when degenerate) and `normal × u`. Creates one face (no loose edges), cut into the faces it lies on (see *Sticky geometry*). |
| `draw_polygon` | `points (≥3)` | one planar face, cut into the faces it lies on. |
| `draw_circle` | `center`, `normal`, `radius`, `segments?: 24` | one regular polygon face, cut into the faces it lies on. |
| `push_pull` | `face`, `distance`, `newFace?` | moves the face along its normal by `distance` (negative pushes). **A face on its own or in a flat drawing** becomes a prism: it keeps its id and moves to the new cap; side faces and the base face are created; the base face is the original outline with the normal flipped. **A face of a solid** (see *Sticky geometry*) moves with the solid: no base, neighbours stretch, a face it is pushed onto is cut open. With `newFace: true` the face of a solid is treated as a face on its own: the solid keeps its shape, a face stays where this one was and a prism is built on it. |
| `offset` | `face`, `distance` | the outline offset in-plane (positive = outward), cut into the face: inward leaves a border (keeps the id) around a new inner face (`created[0]`); outward creates a border face around the original. |
| `move` | `ids`, `delta` | translates entities (groups move as a whole). |
| `stretch` | `ids?` (faces, loose edges), `points?: [p…]` (corner positions), one of `delta`, `rotate: { origin, axis, angle }` (degrees) or `scale: { origin, factors: [sx,sy,sz] }`, `group?` | moves, turns or scales those faces and edges, and the corners at `points`, with everything attached to them in their container (see *Sticky geometry*). `ids` must share one container; with `points` alone the container is `group` or the top level. Faces folded in the process are in `created`. |
| `rotate` | `ids`, `origin`, `axis`, `angle` (degrees) | rotates about the axis through origin, as they are (nothing attached follows: see `stretch`). |
| `scale` | `ids`, `origin`, `factors: [sx,sy,sz]` | scales about origin, as they are. |
| `copy` | `ids`, `delta?`, `rotate?: { origin, axis, angle }`, `count?: 1` | duplicates with each copy offset by `delta` more and/or turned by `angle` degrees more about the axis (a radial array); returns new ids. |
| `intersect_faces` | `ids` (faces of one container) | where these faces cross other faces of the container, loose edges are added along the crossings (`created`, edges first); an edge that runs right across a face divides it. Edges that two faces already share, and edges already drawn, are skipped. |
| `erase` | `ids` | removes entities (a group's children with it). A loose edge between two faces of one plane joins them (the larger keeps its id, the other is in `removed`); any other loose edge along a face's outline takes that face with it. |
| `paint` | `ids`, `material`, `side?: "front"\|"back"\|"both"` | sets face materials; on a group paints all faces inside. `material` may be a material id or a `#hex` colour (then a material is created on the fly). |
| `set_layer` | `ids`, `layer` | moves entities to a layer. |
| `make_group` | `ids`, `name?` | wraps entities in a group (identity transform, world coords). |
| `explode` | `id` | dissolves a group into its parent, applying its transform. |
| `add_layer` / `update_layer` / `remove_layer` | `name`, `color?` / `id`, `name?`, `visible?`, `color?` / `id` | entities on a removed layer go to the first layer. |
| `add_material` / `update_material` | `name`, `color`, `opacity?` / `id`, … | |
| `add_guide` | `point`, `direction?` | |
| `add_dimension` | `a`, `b`, `offset`, `style?` | linear by default. `style: "radius"`: `a` is the centre, `b` a point on the curve; `"diameter"`: `a` and `b` are opposite points of the circle; for both `offset` leads from `b` to the label. `"angle"`: `a` is the corner, `b` a point on the first leg, `offset` the vector from the corner to a point on the second leg; the compiled `value` is the angle between the legs in degrees |
| `add_text` | `position?`, `text`, `anchor?`, `screen?` | `screen: [fx, fy]` (fractions of the view, from the top left) makes screen text: it stays there however the camera moves and has no leader |
| `add_section` / `update_section` | `origin`, `normal`, `active?` / `id`, `origin?`, `normal?`, `active?` | every active section cuts; several combine, keeping only what all of them keep. |
| `soften` | `ids?`, `angle?` (degrees, default 20), `edges?: [[p, p]…]`, `group?` | Soften Edges. With `ids`: the faces, and every face inside the groups among them, get `smooth: angle`; `angle: 0` takes it off (hard edges again). With `edges` (pairs of points, in `group` or the top level): the two faces that share each edge get a `smooth` just above the angle they meet at, or lose it with `angle: 0`. `modified` lists the faces, or the groups given. An edge between two faces of one plane is never softened |
| `text3d` | `text`, `position?`, `height?` (0.25), `depth?` (0), `font?: sans \| serif`, `bold?`, `normal?` ([0,1,0]), `u?`, `align?: left \| center \| right`, `layer?`, `material?`, `group?` | 3D Text: a new group named after the text (`created[0]`) with one face per closed part of a letter, holes cut out; with `depth` each is a closed solid standing that far out along `normal`, its side faces softened at 30°. `position` is the left end of the first line's baseline (the middle or right end with `align`), `height` the height of the capitals, `u` the reading direction (default: level, so text on a wall stands upright; along x on the ground). `\n` starts a new line below. The outlines come from the embedded Droid Sans and Droid Serif (Apache 2.0, `crates/plannr-core/fonts`) |
| `set_locked` | `ids`, `locked` | lock or unlock entities; stored as `locked: true` on the entity. A locked entity, and anything inside a locked group, refuses ops that would change or remove it; `copy` and `set_hidden` still work |
| `insert` | `entities: [entity…]`, `delta?`, `group?`, `merge?`, `local?` | adds ready-made entities (pasted, imported): new ids all the way down, unknown tags and materials fall back to the default, instances without their definition are skipped. Faces are added as they are (not cut into what they land on); `merge: true` joins the coplanar, touching faces of each inserted group (a triangle mesh becomes clean faces); with `group`, `local: true` says the entities are already in that group's coordinates |
| `set_hidden` | `ids`, `hidden` | hide or show entities (a group or component with everything in it); stored as `hidden: true` on the entity. Hidden entities leave the compiled scene, slices and exports; `compile?hidden=1` (and `preview?hidden=1`) draws them as ghosts in a material group `__hidden`, not counted in the metrics. A hidden section plane still cuts. |
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
| `solid_box` | `min`, `max`, `name?`, `layer?`, `material?` | a closed box solid as its own group (world axes); `created` = the group id (no geometry in the result). |
| `solid_cylinder` | `center`, `axis`, `radius`, `height`, `segments?: 32`, `name?`, `layer?`, `material?` | a closed cylinder solid as its own group: the base circle at `center` normal to `axis`, extruded `height` along it (negative goes the other way). |
| `extrude` | `profile: [p…]`, `holes?: [[p…]…]`, `distance?` or `direction?`, `name?`, `layer?`, `material?` | a closed solid as its own group from a planar loop (with hole loops in its plane), swept `distance` along the profile's normal (right-hand rule of the point order) or by the vector `direction`. |
| `boolean` | `mode: "union"\|"subtract"\|"intersect"\|"split"`, `target`, `tools: [id…]`, `keepTools?: false`, `material?` | solids are groups whose faces close up (faces pointing inward are turned out). `target` keeps its id and becomes the result, in its own coordinates; coplanar pieces of the result merge into faces with holes. Tools are erased unless `keepTools`; tools that don't overlap each other are combined in one pass. Faces a subtract creates take the target's dominant material; `material` paints the whole result. Fails when the target or a tool is not closed, or the result is empty. `split` takes one tool: target and tool each keep what is only theirs, the shared part is a new group in `created` (SketchUp's Split; Trim is `subtract` with `keepTools`). BSP CSG (csg.js), 1 µm plane tolerance. |
| `set_extension` | `plugin`, `data` | stores plugin data. |
| `custom` | `type`, `data` | routed to the plugin registered for `type`'s prefix; 400 `no plugin for <type>` otherwise. |

Geometry rules: faces are validated planar and non-degenerate; outer loops are re-oriented so the
normal follows the given winding (for `draw_rectangle` the normal passed). Edge ids are stable.
`erase` of a face leaves its outline edges only if they were loose edges before.

### Sticky geometry

The faces and loose edges of one container (the top level, or one group) behave as one connected
shape where they touch, as in SketchUp. Faces still store their own outlines; what touches is found
by position (0.1 mm), so documents need no migration.

- **Drawing on a face divides it.** A face drawn on faces of its plane is cut into them: each keeps
  its id for the part outside the new outline (`modified`); the parts inside become faces of their
  own, listed first in `created`, with the old face's materials unless the new face names one. What
  lies on no face is added as drawn (keeping the new face's id). Drawing the same shape twice adds
  nothing.
- **Push/Pull moves a face of a solid.** When other faces share the face's edges and the solid is
  behind it (a ray from the face against its normal crosses an odd number of faces of the
  container), the face moves and keeps its id and side. Along each edge, a face standing along the
  pull is stretched or shortened (`modified`; squeezed to nothing it is in `removed`); elsewhere a
  side face is created. No base face is left. Where the moved face comes to lie on another face,
  that face is cut open and the moved face is removed: pushing a shape through a wall's thickness
  leaves an opening.
- **Erasing an edge** joins or removes the faces beside it (see `erase`).
- **`stretch` moves corners with what is attached.** Every face and loose edge of the container with
  a corner at a moving position follows with that corner; a moving corner in the middle of another
  face's edge becomes a corner of that face first. A face whose corners all move keeps its shape. A
  face that only partly moves is reshaped, and folded into flat faces when it no longer lies in one
  plane (triangles of the old face that still share a plane are joined again). The change is a move
  (`delta`), a rotation (`rotate`) or a scale (`scale`), in world coordinates: the top of a box
  scaled to half tapers the box, turned about its centre it twists the box (each side folds in
  two). The Move, Rotate and Scale tools send `stretch` for drawn faces and loose edges, and Move
  also for a corner or edge taken hold of directly; `move`, `rotate` and `scale` themselves stay
  rigid, so scripts that transform loose faces keep working as before.

## Compiled scene (what the browser renders)

`GET /api/projects/:id/compile` (query `section=0` for uncut geometry) or `POST /api/compile { document }`.
`cuts` lists the active section planes even with `section=0`: the browser always asks for uncut geometry
(compile, preview, partial ops/undo/redo) and cuts on the GPU, keeping what every plane has
`dot(p - origin, normal) <= 0`, so switching or moving a plane needs only a partial reply.

```jsonc
{
  "version": 13,
  "groups": [ { "material": "id", "name": "Brick", "color": "#B0623F", "opacity": 1, "back": false,
                "positions": [..], "normals": [..], "indices": [..],
                "ids": ["faceId", ...], "idRanges": [[startIndex, count], ...] } ],
  "edges": { "positions": [x,y,z,x,y,z, ...], "ids": ["edgeOrFaceId", ...] },   // one id per segment (2 points)
  "guides": [ { "id", "point", "direction" } ],
  "dimensions": [ { "id", "a", "b", "offset", "value", "style?" } ],
  "texts": [ { "id", "position", "text", "anchor", "screen?" } ],
  "sections": [ { "id", "origin", "normal", "active" } ],
  "cuts": [ { "origin", "normal" } ],   // the active planes that cut (left out when none)
  "bounds": { "min": [..], "max": [..] },
  "metrics": { "faces": 12, "edges": 30, "groups": 1, "instances": 0, "definitions": 0, "area": 86.4, "areaByMaterial": { "Brick": 12.0 }, "edgeLength": 72.3 }
}
```

Groups' transforms are baked into world coordinates. Faces whose layer is hidden are skipped. Both
sides of a face render: a back-face group (`back: true`) uses `backMaterial` or the standard back colour
`#B7C1CC`. Each active section plane drops every triangle and edge segment fully on its cut-away side and
clips those that cross it. Face outlines are emitted as edge segments with the face id so the SketchUp
black-edge look needs no extra work in the browser.

## REST summary

| Method | Path | Body → Response |
|---|---|---|
| GET | `/api/health` | `{ ok, version, name: "plannr" }` |
| GET | `/api/mcp-token` | `{ token, endpoint }` — private instance only, only to a loopback peer and same-origin (the app's "Connect an AI" panel); 404 / 403 otherwise |
| GET | `/api/projects` | `[{ id, name, createdAt, updatedAt, version, entityCount }]` |
| POST | `/api/projects` | `{ name, document? }` → summary |
| GET | `/api/projects/:id` | `{ id, name, createdAt, updatedAt, document }` |
| PATCH | `/api/projects/:id` | `{ name }` → summary |
| PUT | `/api/projects/:id/document` | Document → `{ version }` (replace; one undo step) |
| DELETE | `/api/projects/:id` | 204 |
| POST | `/api/projects/:id/duplicate` | → summary |
| POST | `/api/projects/:id/ops` | see Ops; `partial: true` (and `?hidden=1`) adds `scene`, `dirty`, `reconcile` |
| POST | `/api/projects/:id/preview` | `{ ops, partial? }` → CompiledScene; with `partial: true` only the top-level entities the ops create, modify or remove, plus `partial: true` and `dirty` (those ids), or the whole scene when the results don't say what changed |
| POST | `/api/projects/:id/undo`, `/redo` | → `{ version, label }`; `?partial=1` adds `scene`, `dirty`, `reconcile` (see Ops) |
| GET | `/api/projects/:id/history` | `[{ version, label, at }]` |
| GET | `/api/projects/:id/compile` | → CompiledScene |
| POST | `/api/compile` | `{ document }` → CompiledScene |
| GET | `/api/projects/:id/entities?kind=&layer=` | `[Entity]` (flattened, with `path` of group ids; `kind=instance` lists placed components) |
| GET | `/api/projects/:id/export.glb` / `export.json` | downloads |
| GET | `/api/plugins` | `[{ id, name, entityTypes, ops, enabled, source: builtIn\|installed, engine, script, version?, description?, homepage?, exports? }]` every plugin, built in or installed; `script` is null when it is off |
| POST | `/api/plugins/install` | body: a `.plannr-plugin` package → `{ installed: manifest }` (replaces an older version) |
| POST | `/api/plugins/:id/enabled` | `{ enabled }` switch any plugin on or off (kept in `data/plugins/state.json`) |
| DELETE | `/api/plugins/:id` | uninstall a package (built-in plugins can only be switched off) |
| GET | `/plugins/:id.js` | a plugin's browser bundle (an installed package's, else the app's) |
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
- Every op above as its own tool with the same fields: `draw_line`, `draw_rectangle`, `draw_polygon`, `draw_circle`, `draw_arc`, `push_pull`, `offset`, `follow_me`, `move`, `stretch`, `rotate`, `scale`, `mirror`, `copy`, `erase`, `intersect_faces`, `insert`, `set_locked`, `soften`, `text3d`, `paint`, `set_layer`, `make_group`, `explode`, `make_component`, `place_component`, `make_unique`, `edit_component`, `rename_component`, `remove_component`, `add_layer`, `update_layer`, `remove_layer`, `add_material`, `update_material`, `add_guide`, `add_dimension`, `add_text`, `add_section`, `update_section`, `slice_section`, `update_entity`, `measure`, `set_extension`, `custom`, and the solids `solid_box`, `solid_cylinder`, `extrude`, `boolean`. Each returns the OpResult as JSON text and the new version; groups in its `entities` are summed up as `{kind, id, name, layer, children}` rather than listing their faces (REST returns them in full). `list_components` lists the definitions with entity and instance counts.
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
