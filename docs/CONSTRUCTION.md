# Construction plugin — contract

A plugin on top of the core — built as **masonry** (see docs/MASONRY.md: walls and upper floors are derived masonry, the masonry engine is a module of this plugin): storeys, rooms, walls, openings, roofs, fixtures and cable/pipe runs. It
stores **semantic** entities (`custom` entities of type `construction.*`) and the engine plugin expands
them into core faces at compile time. Nothing here is known to the core; remove the plugin and the
documents still load (custom entities are simply not rendered).

Plugin id: `construction`. Engine crate: `plugins/construction/engine` (`plannr-plugin-construction`),
registered in `crates/plannr-server/src/plugins.rs`. Browser bundle: `plugins/construction/web/index.ts`
→ built by Vite to `web/dist/plugins/construction.js`, which the server lists in `GET /api/plugins` as
`script` and the app loads at startup.

Coordinates: world metres, y up. Plan points are `[x, z]`. A storey's floor sits at
`base(level) = Σ storeys[0..level].height`. Fixture and run heights are **relative to the storey floor**.

## Plugin system additions (core)

`plannr_core::plugin::Plugin` gains:
- `fn transform(&self, e: &mut Custom, m: &Mat4) -> Result<()>` — called by `move` / `rotate` / `scale` / `copy` on custom entities (default: error "not movable").
- `fn paint(&self, e: &mut Custom, face_key: &str, material: &str, side: Side) -> Result<()>` — called by `paint` when the id carries a face key.
- `fn metrics(&self, doc: &Document) -> Option<Value>` — merged into `CompiledScene.metrics.plugins.<id>`.
- `fn on_load(&self, doc: &mut Document)` — called when a document is created or loaded; the construction plugin uses it to ensure its materials and the extension block exist.

Expanded faces carry ids `<customId>#<faceKey>`. Every core op that takes ids (`erase`, `move`, `rotate`,
`scale`, `copy`, `paint`, `set_layer`, `make_group`, `update_entity`…) **strips `#…`** and targets the custom
entity; `paint` passes the key to the plugin. The browser does the same in `resolve()`.
`GET /api/plugins` entries gain `script: "/plugins/<id>.js" | null` and `settingsSchema?`.

## Document data

```jsonc
"extensions": { "construction": {
  "storeys": [{ "id": "uuid", "name": "Ground floor", "height": 2.6 }],
  "groundLevel": 0.0,                    // finished ground floor above the world origin; the whole house stacks on it
  "wallThickness": 0.10, "floorThickness": 0.06,
  "cavity": { "enabled": false, "innerLeaf": 0.09, "cavity": 0.06, "outerLeaf": 0.09 }
} }
```

Custom entity types (`{ "kind": "custom", "id", "layer", "type": "...", "data": {...} }`):

| type | data |
|---|---|
| `construction.room` | `name`, `level`, `points: [[x,z]…]` (counter-clockwise, ≥3), `hasFloor: true`, `hasCeiling: false`, `wallHeight?`, `finishes: { "floor"?: materialId, "wall.<edge>.interior"?, "wall.<edge>.exterior"? }` — edge `i` runs from `points[i]` to `points[i+1]`; a room contributes one wall per edge; walls shared by two rooms on the same level (same segment, either direction) are built once. Exterior face = the side away from the room. |
| `construction.wall` | `level`, `a: [x,z]`, `b: [x,z]`, `height?`, `thickness?`, `exterior?: bool`, `finishes: { interior?, exterior? }` — a free-standing wall. |
| `construction.opening` | `level` (derived from host), `kind: door\|window\|passage\|garage`, `host: { room: id, edge: n } \| { wall: id }`, `along` (centre distance from the edge/wall start), `width`, `height`, `sill` — cut as a real hole: the wall is split into left / right / lintel / (sill) pieces; doors get a leaf, windows glass + frame, passages nothing, garage a panel. Defaults: door 0.9×2.05 sill 0; window 1.2×1.4 sill 0.9; passage 1.0×2.1; garage 2.5×2.1. |
| `construction.roof` | `style: flat\|gable\|hip\|shed`, `pitchHeight` (rise of the rafter underside from the outer wall face to the ridge; shed: to the high wall), `overhang`, `vergeOverhang?` (past the gables, default `overhang`), `party?: ["min"|"max"]` (ends of the ridge on a party wall: the roof runs on over the neighbour, no verge there), `ridgeAlongX`, `min: [x,z]`, `max: [x,z]`, `levels` (the roof sits on top of storey `levels-1`), `rafterSize: [0.063, 0.175]`, `rafterSpacing: 0.4`, `purlinSize: [0.1, 0.25]`, `purlinSpan: 2.4`, `plateHeight: 0.075`, `covering: tiles\|slates\|metal`, `battenGauge: 0.32`, `enabled: true` (`thickness` is no longer used) — several roofs may exist (a main house and a garage). When `min/max` are absent the footprint of the rooms on the top storey is used; either way each side snaps to the outer face of the exterior wall under it (within 0.35 m). See **Roofs** below. |
| `construction.fixture` | `level`, `kind` (outlet, switchPlate, ceilingLight, wallLight, panel, dataPoint, tapCold, tapHot, mixer, showerValve, toiletSupply, floorDrain, drainPoint, radiator, boiler, manifold, thermostat), `position: [x, yRel, z]`, `host?`, `rotation?`, `note` — electrical points and thermostats are drawn as the real thing (`fixtures.rs`): wall-mounted kinds (outlet, switchPlate, dataPoint, wallLight, panel, thermostat) are mounted on the face of the nearest wall within 0.45 m (the stored plan position picks the wall, the side and the place along it; else `rotation` gives the facing): an outlet is an 80 mm plate per gang (71 mm pitch) with a Ø 39 mm cup, two pin holes and a male earth pin (type E), a switch a plate with one rocker per gang, a data point a plate with two RJ45 ports, a wall light a plate with a round lamp, the board a 40 × 60 × 12 cm cabinet with two breaker rows; a ceiling light is a Ø 30 cm flush fitting under the ceiling finish, or a pendant (rose, cord, shade) when `position[1]` is more than 0.25 m under the ceiling. Gangs: `gangs` in the data, else read from the note (`dubbel` / `2-voudig` → 2, `3-voudig` → 3). Other kinds: a 0.12 m marker box coloured by system (waterCold 2E7BE4, waterHot E5484D, drain 8A8F98, heating F2711C). |
| `construction.run` | `level`, `system: electrical\|waterCold\|waterHot\|drain\|heating`, `points: [[x,yRel,z]…]`, `diameter?` — rendered as 0.03 m square tubes in the system colour. |

Materials the plugin ensures (ids `construction.<key>`): plaster F4F2EE, brick B0623F, brickBrown 6E3F2A,
brickBuff C9A86B, cladding E9E6DF, paintWhite F4F2EE, paintWarm E8D9BF, paintGrey B8B8B4, paintDark 4A4A4A,
paintBlue 8FB6DE, paintGreen 9BC49B, concrete A9A6A0, wood C8A165, tile 7A8AA0, glass 9FC9E8 (opacity 0.45),
door 8B6A43, frame 6B5B4E, electrical/waterCold/waterHot/drain/heating as above.
Defaults: exterior wall face brick, interior plaster, floor wood (concrete for `hasFloor` ground slabs is **not** applied — wood everywhere by default), ceiling plaster, roof tile (slate / steel for the other coverings, roofing membrane on flat roofs), timber spruce, fascia white. The gables are masonry: they take the build-up and finish of the wall under them.

Face keys (`<id>#<key>`): room → `floor`, `ceiling`, `wall.<edge>` (both faces share the key; `paint` uses `side` to pick interior/exterior); wall → `wall`; opening → `leaf` / `glass` / `frame`; roof → `roof` (covering, paintable), `timber`, `fascia` (fascia, soffit, verge boards, coping; paintable); fixture → `marker`; run → `run`.

## Plugin ops (`{ "op": "custom", "type": "construction.<name>", "data": {…} }`, MCP tools `plugin.construction.<name>`)

| name | data | effect |
|---|---|---|
| `setup` | `storeys?`, `groundLevel?`, `wallThickness?`, `floorThickness?`, `cavity?` | create/update the extension block (keeps existing fields), ensure materials |
| `add_storey` | `name?`, `height?` | append a storey; returns its index |
| `update_storey` | `index`, `name?`, `height?` | |
| `remove_storey` | `index` | removes it and every entity on it; higher levels shift down |
| `add_room` | `name?`, `level`, `points` **or** `origin: [x,z]` + `size: [w,d]`, `hasFloor?`, `hasCeiling?`, `wallHeight?` | creates a `construction.room` |
| `add_wall` | `level`, `a`, `b`, `height?`, `thickness?`, `exterior?` | |
| `add_opening` | `kind`, `host`, `along`, `width?`, `height?`, `sill?` | level from host; `along` clamped so the opening fits |
| `add_roof` | `style?`, `pitchHeight?`, `overhang?`, `vergeOverhang?`, `ridgeAlongX?`, `min?`, `max?`, `levels?`, `rafterSize?`, `rafterSpacing?`, `purlinSize?`, `purlinSpan?`, `plateHeight?`, `covering?`, `battenGauge?` | |
| `add_fixture` | `level`, `kind`, `position`, `host?`, `rotation?`, `note?` | |
| `add_run` | `level`, `system`, `points`, `diameter?` | |
| `update` | `id`, `patch` | deep-merges `patch` into the entity's `data` (arrays replaced) |
| `set_finish` | `id`, `face`, `material` | sets `data.finishes[face]` (`floor`, `ceiling`, `wall.<edge>.interior/exterior`, `interior`, `exterior`, `roof`) |
| `import_house` | `house: <Uzzetje HouseModel JSON>`, `replace: true` | replaces the construction entities of the previous import (`replace: false` appends); faces, groups, guides, sections and other plugins' entities stay; converts a native Uzzetje document (floors, rooms, polyRooms, walls, openings with `roomEdge`/`polyEdge`/`wall` refs, roof + roofVolumes, fixtures, runs, cavity, finishes) into plugin entities; plan `[x, y]` → `[x, z]` |
| `summary` | — | no change; returns per-storey rooms with areas, wall lengths, opening counts |

Every op returns the usual `OpResult` (`created` / `modified` / `removed`, plus `entities`).

Metrics (`metrics.plugins.construction`): `{ floorArea, wallLength, exteriorWallArea, interiorWallArea, volume, openingCount, windowArea, storeys, rooms: [{ id, name, level, area, perimeter, openings }] }`.

## Stairs, units, shapes, guides

Exercised by the full example house (`plugins/construction/engine/tests/fixtures/house_current.json`, an invented
detached house written by `make_example_house.py` next to it).

### `construction.stair`

`data`: `name`, `level` (the storey it starts on; it arrives on the one above), `origin: [x, z]` (centre of
the bottom riser), `bearing` (compass degrees: 0 climbs towards −z, 90 towards +x, i.e. direction =
`[sin b, −cos b]`), `width` (0.9), `going` (tread depth, 0.25), `risers` (default `round(storey height / 0.18)`),
`kind: straight | quarterTurn`, `turnAfter` (6), `turnsRight` (true), `winders?` (tapered treads in the turn;
absent or 0 = a quarter-space landing), `treadThickness?` (0.04), `nosing?` (0.025), `openRisers?` (false),
`stringer?` (0.05; 0 = none), `railLeft?` / `railRight?` (stored only), `wellOpening?` (true).

The rise is the storey height, never stored. The step layout is a port of the Studio's `Stair.steps(in:)`
(straight flight; quarter turn with `turnAfter` straight treads, then either a square landing or `winders`
fanning about the inside corner, then the remaining treads in the turned direction).

Expansion: one tread box per step (`tread.<i>`, landing `landing`), pushed forward by the nosing;
a riser board under each tread's trailing edge plus the top riser onto the floor above (`riser.<i>`, up to
`riser.<risers>`) unless `openRisers`; a sloped stringer board each side of every straight run
(`stringer.left` / `stringer.right`). Materials: treads/stringers `construction.wood`, risers `construction.paintWhite`.

**Well opening:** when `wellOpening` is true, every room *with a floor* on `level + 1` whose polygon fully
contains the stair's plan footprint (bounding rectangle of all steps, clipped to the room's bounding box and
inset 1 cm) gets that rectangle cut out of its floor slab as a hole (slab top and bottom faces with a hole,
plus the hole's side faces). The clearance check (headroom) is not ported.

`transform`: moves `origin` and turns `bearing` for plan rotations. Metrics/summary: `stairs` count.

Op `add_stair` — fields as above (`origin` required).

### `construction.unit`

Parametric Studio components cannot be built here (their build scripts are JavaScript), so each becomes a
box: `data`: `name`, `type`, `level`, `origin: [x, z]` (plan centre), `bearing` (same convention as stairs),
`width` (across the bearing), `depth` (along it), `height`, `lift` (underside above the floor). Faces: five
`box` faces in `construction.wood` and the `top` face, `construction.paintWhite` for types containing
`base`, `vanity` or `worktop`. `transform` moves/turns it and scales width/depth uniformly. Metrics: `units`.

Op `add_unit` — `origin` required; `type` fills in defaults (w × d × h, lift): base-unit 0.60×0.60×0.72 ↑0.12,
wall-unit 0.60×0.37×0.72 ↑1.40, tall-unit 0.60×0.60×2.00 ↑0.12, extractor 0.90×0.50×0.40 ↑1.50,
double-vanity 1.20×0.48×0.56 ↑0.30, bath 1.70×0.70×0.55, shower 0.90×0.90×0.06, toilet 0.40×0.70×0.40,
shelving 1.00×0.30×2.40, otherwise 0.60×0.60×0.75.

### `import_house` additions

- `stairs[]` → `construction.stair` (ids kept; `risers` defaulted from the storey height when absent).
- `components[]` → `construction.unit`: `values.width` (or `length` / `diameter`), `values.depth` (or
  `diameter`), `values.height`, `values.lift` when present, else the type defaults above; `origin`, `bearing`, `level`.
- `shapes[]` (skipping `role == "void"` and shapes without faces) → one **core group** per shape, named after it,
  on a layer **"Site"** (created if missing, colour `#7fa85a`), containing one core face per brep face
  (`outer` / `holes` resolved to the brep's absolute y-up points, winding as given). Material: the face's
  `finish`, else the shape's, mapped to `construction.<finish>`; new materials for this: grass 7FA85A,
  gravel / pavement B8B0A2, hedge / foliage 5E8F4E, bark 6B4F3A, steel 9AA0A6, black 1E1E1E, white F4F2EE.
  Unknown finishes fall back to the default material.
- `guides[]` (`{ id, origin: [x,y,z], direction? }`) → core `guide` entities on the first layer.
- `roofVolumes[].highEdge / verges / eave / frame`, `trim`, `units`, `componentTypes`, `electrical`,
  `neighbourhood`, `lot`, `geo`, `savedViews` are accepted and ignored (unknown roof fields are kept in `extra`).

Metrics (`metrics.plugins.construction`) and `summary` gain `stairs` and `units` counts.

## Component scripts

Units can be **parametric**: a `construction.unit` whose `type` names a component type is built by
that type's JavaScript, exactly as in Uzzetje Studio. The engine embeds a JavaScript runtime (Boa,
pure Rust) that loads Studio's prelude and the type sources, calls the script with the unit's
`values` / `choices`, and turns the solids it returns into prisms.

**A component type** is `{ id, name, group, source }`. Fourteen built-in types ship in
`plugins/construction/catalogue/*.js` (base-unit, wall-unit, tall-unit, extractor, worktop, shelving,
radiator, wardrobe, island, double-vanity, bath, shower, wc, basin). A document adds or overrides types in
`extensions.construction.componentTypes` (same shape as Studio's), so a file still works on a
server that has never seen them. `import_house` carries them over.

**The script API** (Studio's prelude, verbatim): `component(id, { name, params, rules, build, worktop, needs, wants, symbol })`.
`params` is a list of `{ name, label, kind: length|number|integer|choice|flag|derived, value, min, max, choices }`;
`rules(p)` may derive values; `build(p)` returns solids made with `box({ x, y, z, w, h, d, material })`
or `shape(corners, low, high, material)`. Solids are prisms in the unit's own frame: x across (0 = the
middle of the back, +x to the left of someone standing in front), z forward from the back, y up from
the storey floor. Materials: `Carcass`, `Front`, `Worktop`, `Handle`, `Appliance`, `Sanitary`, `Glass`,
`Radiator`, `Timber`, `Frame`, `Steel` or any `#RRGGBB`; anything else is Carcass. The plugin creates
the materials as `construction.c.<Name>` / `construction.c.#RRGGBB` when a unit uses them.

**Placement**: world = `origin + across · x + facing · z`, with `facing = (sin bearing, −cos bearing)`
and `across` to its right-hand side, heights at `storey base + low … high` (`lift` is ignored for
scripted units; it is set from the lowest solid for information). A unit that declares a `worktop`
gets a 4 cm top over its own footprint, 2 cm proud at the front (Studio merges neighbouring tops into
one run; here each unit carries its own). Face keys: `solid.<i>`, `worktop`.

**Fallback**: a unit whose type has no script, whose script throws or whose script builds nothing is
drawn as the plain box of its `width × depth × height` lifted `lift`, as before. Scripts run with a
loop-iteration and recursion limit, so a bad script errors instead of hanging the server.

**Ops / MCP tools**

| op | data | result |
|---|---|---|
| `list_catalogue` | — | `entities[0].data = { types: [{ id, name, group, builtin, declared, params, size, problem }], problems }` |
| `component_params` | `type`, `values?`, `choices?` | `entities[0].data = { type, params (after rules), meta, worktop, solids, size }` |
| `define_component` | `id`, `name?`, `group?`, `source` | validates (must declare `id` and build with default params), stores the type, re-sizes existing units of it; `entities[0].data = { id, params, solids, size }` |
| `remove_component_type` | `id` | removes a document type (built-ins cannot be removed) |
| `add_unit` | … + `values`, `choices` | a scripted type is built with these; width/depth/height/lift are derived |
| `update` | `{ id, patch: { values, choices, bearing, … } }` | re-validates and re-sizes a scripted unit |

Metrics gain `scriptedUnits`. The runtime is kept per thread and rebuilt only when the set of type
sources changes; solids are cached per type and parameter set, so expanding a house with dozens of
units costs milliseconds after the first build.

## Clear heights

Storey heights are floor-to-floor. `expand::model` collects what fills the top of each storey into
`settings.structure` (`TopStructure { level, outline, depth, plastered }`): every masonry floor of the
storey above (depth from its system: blocks + topping, planks + topping, or the slab) and every
`framing.floor` set near the top of a storey (attic joists: depth = storey height − elevation).
`Settings::clear_height(level, p)` = height − structure there. Room ceilings hang at the clear height
(skipped where the masonry floor has its own ceiling plaster), ceiling lights mount there, and the
electrical plugin routes its horizontal chases 15 cm under it.

## Roofs

Code in `engine/src/roof.rs`; the geometry (`RoofGeom`) is shared with the masonry, so the walls and
the roof always meet.

- **Footprint**: `u` along the ridge, `v` across; both on the outer faces of the walls under the roof.
  The walls' top is the storey's floor-to-floor height (`wall_top`) — or, when every room under the
  roof has its own `wallHeight` (a garage lower than the house), that height; the masonry of such a
  room stops there too.
- **Plates** (muurplaat, `plateHeight` high) lie on the inner leaf and the cavity of each eaves wall,
  from behind the outer leaf (`d0` = outer leaf width; single-leaf walls: over the whole wall).
- **Rafters** (kepers) are birdsmouthed on the plates: the underside crosses the plate top `seat`
  in from its outer edge, the seat shortened on steep roofs so the notch takes at most a third of
  the rafter. The rafter underside at the outer wall face is `y0 = wall_top + plateHeight − k·(d0 + seat)`;
  `k = pitchHeight / (half span)` (shed: / span). They run from the eaves overhang to the ridge
  (plumb cuts) at `rafterSpacing`, over the verge overhang.
- **Gable & shed**: purlins (gordingen) at equal steps so no rafter spans more than `purlinSpan` along
  the slope, and a ridge purlin (nokgording), all with their top on the rafter underside, bearing in
  the gable masonry (through it to carry the verge when the verge overhangs). Shed roofs: the high
  plate sits up the slope and the high wall's inner leaf rises to it.
- **Hip**: plates all round (mitred), a ridge beam, four hip rafters from the overhang corners to the
  ridge ends, common and jack rafters up to the hips. **Flat**: joists across the short way on the
  inner leaves, OSB deck, 120 mm PIR, membrane; the outer leaf and cavity rise 8 cm above it as an
  upstand with an aluminium coping.
- On the rafters: counter battens (tengels 18×38), battens (panlatten 25×38 at `battenGauge`) and the
  covering (tiles 40 mm, slates 15 mm or metal 8 mm; `finishes.roof: tileBlack` for black tiles), drawn with its courses; ridge (and hip) caps;
  fascia and soffit at the eaves, verge boards at overhanging gables.
- **The walls under the roof** (exterior masonry walls of that storey whose outer face lies on a
  footprint side): layers under a plate stop under it (an inner leaf rises to a plate above the
  storey); the outer leaf, the cavity fill and render follow the rafter underside — at the eaves they
  stop just under the rafters, at the gables they rise into the gable; the inner leaf and the plaster
  rise into the gable too. The masonry coursing carries on up the gable. Walls with an explicit
  `height` are left alone.
- **Taller parts**: the rooms of the storeys above (out to their outer faces) are cut out of a lower
  roof (an annex against a two-storey house stops at its wall).
- The takeoff lists the timber (lengths, volumes), the covering and underlay areas, ridge caps,
  fascia and soffit.
- Simplified: members are boxes and prisms (no notches drawn; a purlin and the ridge purlin run into
  the rafters by half their width's slope); hip roofs have no purlins; no gutters; no sizing check of
  the timber.

