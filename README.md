# Plannr

**Try it: [plannr.m-dm.org](https://plannr.m-dm.org)** · ☕ [Buy me a coffee on Ko-fi](https://ko-fi.com/matthiasdm)

A SketchUp-style 3D modeller as a web app. A **Rust engine** owns the model and every modelling
operation; the browser is a viewport with the familiar tool set; and the same operations are exposed
over **MCP**, so an AI can draw, push/pull, move, measure, paint, annotate, change the camera and take
screenshots of what it made. The core is generic — domain tools (houses, rooms, pipes…) come as plugins.

![Plannr with the example house: a brick house with tiled roofs in the 3D view, the kitchen selected and its walls listed in Entity Info](web/public/home/hero.jpg)

```
crates/plannr-core     document model, ops kernel (draw / push-pull / offset / move / rotate / scale / …), undo history,
                       tessellation + section cuts, metrics, GLB export, plugin trait
crates/plannr-server   Axum REST API + WebSocket + MCP endpoint + SQLite + static hosting (port 8795)
web/                   Vite + TypeScript + three.js frontend (SketchUp-style UI)
docs/CONTRACT.md       the document / ops / REST / WebSocket / MCP contract
docs/PLUGINS.md        how to write engine and browser plugins, and how installable packages work
```

## A look inside

| | |
|---|---|
| ![The ground floor isolated: rooms, cavity walls and the beam-and-block floor seen from above](web/public/home/dollhouse.jpg) | ![A row of framed sheds, each with a different timber roof type](web/public/home/roofs.jpg) |
| Isolate a storey for a dollhouse view: cavity walls, beam-and-block floors and the material takeoff in Model Info. | Every timber roof type, framed on the same shed by the framing plugin. |
| ![The electrical plugin's situation plan for the ground floor: every point as an IEC symbol, numbered and coloured per circuit, with dashed conduits and a legend](web/public/home/electrical.jpg) | ![The same house in the light theme](web/public/home/light.jpg) |
| The situation plan the electrical plugin draws: points as IEC symbols, numbered per circuit, conduits dashed. | The same model in the light theme. |

## Public instance mode

`PLANNR_PUBLIC=1` runs Plannr as a public site (this is how plannr.m-dm.org runs): the front page
(`web/home.html`) is served at `/` and the modeller at `/app`; every browser gets a random owner key
(kept in `localStorage`, sent as `X-Plannr-Owner`) and only ever sees the projects it made (at most 30);
the MCP endpoint is off, and installing or switching plugins needs the server's `PLANNR_TOKEN`. Without it, Plannr is a single-user local app with MCP.

## Run

```bash
cd web && npm install && npm run build && cd ..   # builds web/dist
cargo run --release -p plannr-server              # http://localhost:8795
```

The server prints an MCP token at startup (or set `PLANNR_TOKEN`). In the app, **Help → Connect an AI (MCP)…**
opens a panel that stays open: it shows the endpoint and the running server's token (when the browser is on the
same machine), lets you test it, and hands you the
command below and a JSON config for other MCP clients with the token filled in. Connect Claude Code with:

```bash
claude mcp add --transport http plannr http://localhost:8795/mcp --header "Authorization: Bearer <token>"
```

Frontend development: keep the server running and use `cd web && npm run dev` (Vite proxies `/api`).
Tests: `cargo test`. Type-check: `cd web && npx tsc --noEmit`.

## Tools

| Tool | Key | Notes |
|---|---|---|
| Select | Space | Shift adds, Ctrl toggles, drag a window (left→right) or crossing box (right→left), double-click enters a group or opens a plugin object (a construction wall or roof: select one face; a framed wall: single studs and plates), Esc leaves |
| Eraser | E | Click or drag over edges, faces, objects |
| Line | L | Chained edges; a closed coplanar loop becomes a face. Type a length. |
| Rectangle | R | Two corners on the ground or on any face; type `w;h` |
| Rotated Rectangle | — | First edge with two clicks, then the width: a rectangle at any angle in the plane |
| Circle | C | Centre + radius; type `24s` for the number of sides |
| Polygon | N | Click corners, Enter closes |
| Arc / Pie | A | Centre, start, end (type a radius, then an angle); Pie closes the sector into a face |
| 3 Point Arc | — | Start, a point on the arc, end |
| 2 Point Arc | — | Both chord ends, then pull the bulge |
| Push/Pull | P | Extrude a face; type a distance; the height snaps to other geometry. With a face selected first, click anywhere to pull it |
| Offset | F | Offset a face's outline in or out (the selected face, when there is one) |
| Move | M | Move or (Ctrl) copy; type a distance. With a selection, that is what moves, wherever you grab |
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
inference under the cursor** (a direction, a face's plane, or the axis a snapped point lies on): while it is down the point stays on that line and other geometry only projects onto it. Held, the point follows the cursor along the line and stops only where it meets an edge or inference line, or level with a corner right under the cursor. In Push/Pull, Shift keeps strictly to the face normal; without it the height is taken only from a corner, midpoint or centre right under the cursor. Points hidden behind faces never snap, also while moving. Lengths along inferred lines snap to
1 cm. Numbers typed while a tool is active go into the **Measurements** box.

Trays: Entity Info, Tags (layers), Materials, Outliner, Scenes, Section Planes, Display, Model Info,
History, Plugins.

**Hide and show:** Edit ▸ Hide (⌘E) hides the selection, a group or component with everything in it; Unhide selected, last or all (⇧⌘E) bring it back, and the Outliner has an eye per row. View ▸ Hidden geometry shows hidden entities as ghosts you can select. Hidden entities stay in the document but leave slices and exports.

**Face styles** as in SketchUp, in the view bar and View ▸ Face style (⌥1–5): X-ray, Wireframe, Hidden Line, Shaded, Monochrome, plus Back edges (dashed lines for what is behind faces).

**Light or dark:** View ▸ Theme (or the Display tray) switches between System, Light and Dark for the
chrome and the viewport; the choice is remembered in the browser, and the front page shares it.

## Embedding

Another app can show a project in an iframe (`/app?project=<id>&embed=1&owner=<key>`): viewer mode
with the looking, measuring and section tools only, driven over `postMessage` (storeys, views, zoom to
an entity, selection, screenshots, any op). An app that keeps its own house model pushes it with the
construction plugin's `import_house` (install the Construction plugin), which replaces the previous import and leaves what was drawn
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

The core knows only faces, edges, groups, guides, dimensions, texts and section planes. Everything
domain-specific comes as a plugin with an engine half (Rust: custom entity types expanded into core
geometry, custom ops that become MCP tools `plugin.<id>.<op>`, export formats) and a browser half
(tools, panels, Entity Info). Plugins are **installable packages**: download a `.plannr-plugin` file and
install it with **Plugins → Manage plugins… → Install from file**; the same dialog switches each one on
and off and removes it. Their engines run as WebAssembly in a sandbox inside the server. A fresh Plannr
has none: it is a plain SketchUp-style modeller until you add some.

They live in [plannr-plugins](https://github.com/MatthiasDM/plannr-plugins), each with its own release:

| Plugin | |
|---|---|
| **Construction** | houses from rooms: storeys, rooms of any shape with shared walls built once, Belgian masonry (cavity walls coursed brick by brick, beam-and-block floors, lintels, foundations), doors and windows as real holes, flat / gable / hip / shed roofs, fixtures, pipe and cable runs, parametric furniture, takeoff, checks and U-values, the import of Uzzetje house models |
| **Electrical** | the installation to AREI/RGIE: board, circuits, conduits, checks, situation plan and one-line diagram (needs Construction) |
| **Framing** | Medeek-style wood framing: walls, floors, every timber roof type, beams, stairs, foundations, hardware, engineering check, plans, cut list |
| **Plumbing** | pipes with bends or fittings, valves, mixers, manifolds, brackets, checks, material list |
| **Walking mode** | first-person walk-through with every tool at hand, on desktop and phone |
| **3D Print** | 3MF / STL for slicers, watertight check and repair, plate layout |
| **Car Track** | printable 1:64 die-cast track with connectors, supports and a test drive |

See [docs/PLUGINS.md](docs/PLUGINS.md) to write one.

## Document

One JSON document per project: layers, materials, entities (`face`, `edge`, `group`, `guide`,
`dimension`, `text`, `section`, `custom`), scenes and per-plugin extensions. See
[docs/CONTRACT.md](docs/CONTRACT.md).

## License

MIT — see [LICENSE](LICENSE).
