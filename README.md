# Plannr

**Try it: [plannr.m-dm.org](https://plannr.m-dm.org)** · ☕ [Buy me a coffee on Ko-fi](https://ko-fi.com/matthiasdm)

A SketchUp-style 3D modeller as a web app. A **Rust engine** owns the model and every modelling
operation; the browser is a viewport with the familiar tool set; and the same operations are exposed
over **MCP**, so an AI can draw, push/pull, move, measure, paint, annotate, change the camera and take
screenshots of what it made. The core is generic — domain tools (houses, rooms, pipes…) come as plugins.

```
crates/plannr-core     document model, ops kernel (draw / push-pull / offset / move / rotate / scale / …), undo history,
                       tessellation + section cuts, metrics, GLB export, plugin trait
crates/plannr-server   Axum REST API + WebSocket + MCP endpoint + SQLite + static hosting (port 8795)
web/                   Vite + TypeScript + three.js frontend (SketchUp-style UI)
plugins/construction   the reference plugin: storeys, rooms, walls, openings, roofs, fixtures, runs (engine + browser)
docs/CONTRACT.md       the document / ops / REST / WebSocket / MCP contract
docs/PLUGINS.md        how to write engine and browser plugins
docs/CONSTRUCTION.md   the construction plugin's entities and ops
```

## Public instance mode

`PLANNR_PUBLIC=1` runs Plannr as a public site (this is how plannr.m-dm.org runs): the front page
(`web/home.html`) is served at `/` and the modeller at `/app`; every browser gets a random owner key
(kept in `localStorage`, sent as `X-Plannr-Owner`) and only ever sees the projects it made (at most 30);
the MCP endpoint is off. Without it, Plannr is a single-user local app with MCP.

## Run

```bash
cd web && npm install && npm run build && cd ..   # builds web/dist
cargo run --release -p plannr-server              # http://localhost:8795
```

The server prints an MCP token at startup (or set `PLANNR_TOKEN`). Connect Claude Code with:

```bash
claude mcp add --transport http plannr http://localhost:8795/mcp --header "Authorization: Bearer <token>"
```

Frontend development: keep the server running and use `cd web && npm run dev` (Vite proxies `/api`).
Tests: `cargo test`. Type-check: `cd web && npx tsc --noEmit`.

## Tools

| Tool | Key | Notes |
|---|---|---|
| Select | Space | Shift adds, Ctrl toggles, drag a window (left→right) or crossing box (right→left), double-click enters a group, Esc leaves |
| Eraser | E | Click or drag over edges, faces, objects |
| Line | L | Chained edges; a closed coplanar loop becomes a face. Type a length. |
| Rectangle | R | Two corners on the ground or on any face; type `w;h` |
| Rotated Rectangle | — | First edge with two clicks, then the width: a rectangle at any angle in the plane |
| Circle | C | Centre + radius; type `24s` for the number of sides |
| Polygon | N | Click corners, Enter closes |
| Arc / Pie | A | Centre, start, end (type a radius, then an angle); Pie closes the sector into a face |
| 3 Point Arc | — | Start, a point on the arc, end |
| 2 Point Arc | — | Both chord ends, then pull the bulge |
| Push/Pull | P | Extrude a face; type a distance; the height snaps to other geometry |
| Offset | F | Offset a face's outline in or out |
| Move | M | Move or (Ctrl) copy; type a distance |
| Rotate | Q | Protractor: centre (on a face = its plane), reference, angle; type degrees |
| Scale | S | Green handles on the selection's box: corners uniform, side handles one axis, edge handles two; Ctrl scales about the centre; type a factor or `sx;sy;sz` |
| Follow Me | — | Select the path (edges, or a face whose outline is the path), then click the profile face to sweep it |
| Flip / Mirror | — | Mirrors the selection across a plane through its centre (→ ← ↑ pick the axis) or across a clicked face; Ctrl keeps a copy |
| Place Component | — | Drops an instance of a component definition (Edit → Make Component turns a selection into one); ← → turn it by 90° |
| Tape Measure | T | Measure; start on an edge to pull a parallel guide line; Ctrl-click places a guide point |
| Dimension | D | Two points, then the offset |
| Protractor | — | Vertex, reference direction, angle: places an angled guide line; type degrees |
| Text | X | Click a point, type a label |
| Paint Bucket | B | Active material onto a face (Shift: whole object, Alt: sample) |
| Orbit / Pan / Zoom | O / H / Z | Middle-drag orbits, Shift+middle pans, wheel zooms in any tool; Shift+Z zoom extents |
| Zoom Window | — | Drag a rectangle to fill the view with it; ⌘[ and ⌘] step through previous and next views |
| Section Plane | — | Click a face to cut along it, the ground for a horizontal cut; the plane is previewed while hovering, its arrows on the side that is cut away (Tab flips it; Reverse later in Entity Info). Several active planes cut together |

**Hold a modifier for a tool:** holding Ctrl orbits and holding Option/Alt pans until you release the key,
then you are back in the tool you had. Which key does what is set in the Shortcuts tray (Shift can be
assigned too; by default it keeps its selection meaning).

Inference as in SketchUp: green endpoint, cyan midpoint, red on-edge, blue on-face dots, black
intersection crosses, axis-coloured lines, magenta parallel/perpendicular to an edge you rested the
cursor on, dotted **from point** lines from points you rested on or clicked, guide points and lines,
the centres of circles, arcs and polygons, and the origin. The **arrow keys lock an axis** (→ red x, ← green z, ↑ blue y) and **Shift holds the
inference under the cursor** (a direction, or a face's plane). Lengths along inferred lines snap to
1 cm. Numbers typed while a tool is active go into the **Measurements** box.

Trays: Entity Info, Tags (layers), Materials, Outliner, Scenes, Section Planes, Display, Model Info,
History, Plugins.

**Light or dark:** View ▸ Theme (or the Display tray) switches between System, Light and Dark for the
chrome and the viewport; the choice is remembered in the browser, and the front page shares it.

## Embedding

Another app can show a project in an iframe (`/app?project=<id>&embed=1&owner=<key>`): viewer mode
with the looking, measuring and section tools only, driven over `postMessage` (storeys, views, zoom to
an entity, selection, screenshots, any op). An app that keeps its own house model pushes it with the
construction plugin's `import_house`, which replaces the previous import and leaves what was drawn
beside it. See [docs/EMBED.md](docs/EMBED.md) and `web/public/embed-example.html`.

## MCP

Every tool is an MCP tool with the same semantics (`draw_line`, `draw_rectangle`, `draw_circle`,
`draw_polygon`, `push_pull`, `offset`, `move`, `rotate`, `scale`, `copy`, `erase`, `paint`, `make_group`,
`explode`, `draw_arc`, `follow_me`, `mirror`, `make_component`, `place_component`, `edit_component`, `make_unique`,
`add_guide`, `add_dimension`, `add_text`, `add_section`, `measure`, …), plus project tools
(`list_projects`, `create_project`, `use_project`, `get_document`, `list_entities`, `undo`, `redo`),
viewport tools that drive the open browser (`screenshot`, `set_view`, `set_camera`, `zoom_extents`,
`zoom_to`, `zoom_window`, `previous_view`, `next_view`, `orbit`, `pan`, `zoom`, `select`, `activate_tool`, `get_view_state`) and `export_glb`.
`screenshot` returns a PNG of the viewport, so the model can look at what it built.

## Plugins

The core knows only faces, edges, groups, guides, dimensions, texts and section planes. Domain tools come
as plugins with an engine half (Rust: custom entity types expanded into core geometry, custom ops that
become MCP tools `plugin.<id>.<op>`) and a browser half (tools, panels, entity editors). The
**construction** plugin ships with the repo: storeys, rooms (rectangles or any shape, shared walls built
once), free walls, doors / windows / passages / garage gates cut as real holes, flat / gable / hip / shed
roofs, electrical and plumbing fixtures and cable / pipe runs, cavity walls and finishes. Its tools sit
in their own toolbar group, a floor bar switches storeys, and "Isolate storey" gives a dollhouse view.
Furniture and kitchen pieces are **parametric component scripts** (the same tiny JavaScript dialect as
Uzzetje Studio: `component(id, { params, rules, build })` with `box()` / `shape()` prisms) evaluated by a
JavaScript runtime embedded in the engine: 19 built-in types ship in `plugins/construction/catalogue/`,
documents can define their own (`define_component`, also an MCP tool), and parameters are edited as a
form in Entity Info.
See [docs/PLUGINS.md](docs/PLUGINS.md) and [docs/CONSTRUCTION.md](docs/CONSTRUCTION.md).

**Route tool (construction).** A *route* is a pipe or cable run: a polyline at a fixed height above the
active storey's floor. Pick the system in the Construction panel (cold water, hot water, drain, heating,
electrical), type the height in the Measurements box (0.3 = skirting level, 2.45 = just under a 2.5 m
ceiling), click each bend, and double-click or press Enter to finish. Runs are drawn as tubes in the
system's colour and their lengths are totalled in Model Info. Electrical wiring is better done with the
**electrical** plugin's Conduit tool, which knows about circuits.

The **electrical** plugin models the installation to Belgian AREI/RGIE conventions: a distribution board
(phases, main breaker, 300 mA head differential, earthing, Ik), circuits typed by use with breaker, cable
section, 30/300 mA differential and their points (the construction plugin's sockets, switches, lights,
data points), conduits (flush conduit or surface cable, 16/20/25 mm) drawn by hand or auto-routed from the
board via the ceiling or floor, AREI checks (cable vs breaker, 30 mA on sockets and wet rooms, dedicated
appliance circuits, points per circuit, unassigned points, demand vs capacity…), and the two drawings an
inspection asks for: the **situatieschema** per storey with IEC 60617 symbols and the **eendraadschema**
(one-line diagram), both as SVG. See [docs/ELECTRICAL.md](docs/ELECTRICAL.md).

The **framing** plugin brings Medeek-style parametric wood framing. **Walls** are drawn along their
exterior face (clockwise) and join by themselves: L corners (California, 3-stud or 2-stud) and
T-intersections (2-stud channel, ladder blocking, California T) with trimmed ends, interlocking top
plates and lapped sheathing; studs at 300/400/600 mm, rough openings with kings, jacks, headers sized by
span, sills and cripples, sheathing and gypsum. **Floors** (joists, rims, blocking, subfloor), **roofs**
(gable, hip, shed and gambrel; Fink, Howe, king/queen-post, scissor, attic and mono trusses or rafters
with hips and jacks; fascia, soffit, shingle/metal/tile cladding, gutters, gable and shed dormers),
**beams and posts**, **stairs** (notched stringers, treads, risers), **foundations** (slab or stem wall),
**hardware** (anchor bolts, hurricane ties, hold-downs, joist hangers), an **engineering check** and span
tables (EN 338 C18/C24/C30 or SPF/DF-L No.2, snow and floor loads), **wall elevations and framing plans**
as SVG, "Frame room" from the construction plugin, and a cut list with stock lengths, panel counts,
hardware and CSV export. See [docs/FRAMING.md](docs/FRAMING.md).

The construction plugin builds everything as **masonry**, the way it's built in Belgium. Exterior
walls are **cavity walls** (spouwmuren): an outer facing-brick leaf, a cavity (empty, fully filled or
with boards) with stainless ties, an inner leaf of snelbouw or blocks and interior plaster; inner walls
are single leaves. Corners mitre and T-junctions trim automatically. Openings get concrete lintels
inside, steel angles outside and bluestone sills, ground-floor walls a damp-proof course, and the bond
pattern is drawn on the brick. Upper **floors** are beam-and-block ("potten en balken": prestressed
joists, infill blocks, topping with mesh, ring beam, trimmed stair wells), hollow-core planks
(welfsels) or poured slabs, plus **strip foundations**. Walls and floors are derived from the rooms; any
one can be customised with its own build-up. Storey heights are floor to floor, and clear heights
follow the structure above. Includes a takeoff (bricks, blocks, mortar, ties, lintels, joists, blocks,
concrete), checks (lintel bearings, piers, ties, floor spans between bearing walls) and U-values
against the EPB requirement with the effect of a cavity fill, plus wall elevations and storey plans as
SVG. See [docs/MASONRY.md](docs/MASONRY.md).

## Document

One JSON document per project: layers, materials, entities (`face`, `edge`, `group`, `guide`,
`dimension`, `text`, `section`, `custom`), scenes and per-plugin extensions. See
[docs/CONTRACT.md](docs/CONTRACT.md).

## License

MIT — see [LICENSE](LICENSE).

