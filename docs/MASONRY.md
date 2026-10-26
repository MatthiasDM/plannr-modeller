# Masonry — the construction plugin's build-up (engine)

The masonry engine is **part of the construction plugin** (module `plugins/construction/engine/src/masonry`,
browser `plugins/construction/web/masonry.ts`). The construction model *is* masonry: every piece of the
construction wall graph is built as a masonry wall — exterior pieces as **cavity walls** (spouwmuren:
outer facing-brick leaf, cavity with optional insulation and ties, inner leaf, plaster), other pieces as
single leaves — and every upper storey gets a masonry **structural floor** (potten en balken, welfsels or a
poured slab) over the outline of its rooms. These are *derived* on the fly (`derive.rs`), no entities
needed; their ids are `<room or wall id>#<face key>` (`~k` for a second piece of one edge) and
`<carrier room>#mfloor<k>`, so their faces select the room. A stored **`masonry.wall`** /
**`masonry.floor`** (from "Customise" / `import_construction`, or drawn with the Wall / Structural floor
tools) replaces the derived one for its piece (`source: { owner, key }`) or storey. The construction
plugin owns the `masonry.*` prefix through a core plugin **alias**: `masonry.<op>` types keep working,
and MCP lists them as `plugin.construction.masonry_<op>`.

**One source of truth for thicknesses**: the masonry defaults' unit widths (`outer.width`,
`inner.width`, `partition.width`) and `cavity.width` are what the construction settings report as
`cavity.outerLeaf / innerLeaf / cavity` and `wallThickness` (the wall graph, door and window frames,
fixture mounts follow them); `construction.setup` with `cavity` / `wallThickness` writes them into the
masonry defaults. On first load a document without masonry defaults takes them from its construction
settings; a new model gets cavity walls. A room's paint (`finishes.wall.<e>.interior/exterior`) goes on
the derived wall's plaster (`finishInterior` / `finishExterior`; another brick colour → `outerMaterial`,
a non-brick outside finish → exterior render), and the Paint tool on a masonry face (`wall.2#plaster.int`,
`#outer`, …) sets that finish. A wall piece whose line carries straight on past its end gets no corner
there (a wall meeting that point Ts into it); lintels and sills of neighbouring openings stop halfway
the pier; bluestone sills only on cavity walls.

## Defaults (`extensions.masonry.defaults`)

```jsonc
{
  "outer":  { "name": "Gevelsteen 190×90×65", "kind": "facingBrick", "length": 0.19, "height": 0.065, "width": 0.09, "joint": 0.01, "bond": "stretcher", "color": "#9C4A32" },
  "inner":  { "name": "Snelbouwsteen 288×88×138", "kind": "clayBlock", "length": 0.288, "height": 0.138, "width": 0.09, "joint": 0.012, "bond": "stretcher", "color": "#C9785A" },
  "partition": { "name": "Snelbouwsteen 288×88×138", "kind": "clayBlock", "length": 0.288, "height": 0.138, "width": 0.09, "joint": 0.012, "bond": "stretcher", "color": "#C9785A" },
  "cavity": { "width": 0.06, "insulation": { "kind": "none", "material": "eps", "thickness": 0.0 }, "ties": { "perM2": 5 } },
  "plaster": { "interior": true, "thickness": 0.015 },
  "dpc": true,
  "lintel": { "bearing": 0.15, "inner": "concrete", "outer": "steelAngle" },
  "sill": { "outer": "bluestone", "projection": 0.03, "thickness": 0.05 },
  "floor": { "system": "beamBlock", "joistWidth": 0.12, "joistHeight": 0.12, "spacing": 0.6, "blockKind": "concrete", "blockHeight": 0.16, "blockLength": 0.25, "topping": 0.04, "mesh": true, "bearing": 0.1, "ringBeam": true, "plankThickness": 0.2, "plankWidth": 1.2, "slabThickness": 0.18 },
  "foundation": { "footingWidth": 0.6, "footingDepth": 0.3, "bottom": -0.8, "blockWidth": 0.19 }
}
```
Unit `kind`: `facingBrick`, `brick` (common brick), `clayBlock` (snelbouw / poroton), `concreteBlock`,
`aeratedBlock` (cellenbeton), `limeSandBlock` (kalkzandsteen). `bond`: `stretcher` (halfsteens, half
offset), `third` (third offset), `stack` (no offset), `english`, `flemish` (for walls ≥ one brick
thick; drawn with headers). `insulation.kind`: `none`, `fill` (blown/injected, fills the cavity:
`material` `eps` beads / `pur` / `mineralWool`), `board` (`material` `pir` / `xps` / `mineralWool`,
`thickness`, fixed against the inner leaf, the rest is a ventilated air gap).

## Entities (`kind: custom`, `type: masonry.<x>`)

**`masonry.wall`** `{ level, a, b, reference: exterior|center|interior, type: cavity|single, height?, outerHeight?, outer?: Unit, inner?: Unit, cavity?: Cavity, plaster: { interior: bool, exterior: bool, thickness }, dpc: bool, openings: [Opening], startJoin / endJoin: auto|none, source?: { owner, key }, room?, flip }`
— exterior = left of a→b seen from above (`flip` swaps), like framing walls. Missing `outer` /
`inner` / `cavity` take the defaults (`single` walls use `inner`, defaulting to `defaults.partition`).
Layers across the wall along the exterior normal `n`, total thickness `t` = outer + cavity + inner
(single: inner): the **body** spans `[lo, hi]` with `exterior` → `[-t, 0]`, `center` → `[-t/2, t/2]`,
`interior` → `[0, t]`; outer leaf `[hi − w_o, hi]`, cavity `[lo + w_i, hi − w_o]`, inner leaf
`[lo, lo + w_i]`; interior plaster `[lo − p, lo]` (outside the body, in the room).
**Heights**: storey heights are **floor-to-floor**; whatever fills the top of a storey (the masonry
floor of the storey above, or attic joists framed in its top — a `framing.floor` with `elevation` within
0.6 m of the storey height) is the **structure**, and **clear height = storey height − structure**. The
inner leaf (or single leaf) is `height` (default: storey height minus the structure over the wall —
masonry floors and attic joists whose outline lies within 0.3 m of the wall's inner leaf; without one:
storey height − `construction.floorThickness` when there is a storey above, else the full storey
height); the outer leaf runs `outerHeight` (default: the full storey height, it passes the floor edge).
Under a construction roof (a wall of the roof's storey on a side of its footprint, no explicit `height`)
the tops come from the roof: leaves under a wall plate stop under it, the outer leaf / cavity / render
follow the rafter underside (just under the rafters at the eaves, up into the gable), the inner leaf
and plaster rise into the gable — each layer's `top` is then a (t, height) profile and its pieces get
sloped tops with the coursing carried up. See docs/CONSTRUCTION.md › Roofs.
Use `set_clear_height` (or the panel's storey table) to give a storey its clear height: the storey's
floor-to-floor height becomes clear + structure, so adding a floor never eats into the room below.
**Junctions** (automatic unless `none`), from all masonry walls of the storey: an **L corner** (two
non-parallel walls whose ends meet within their bodies) is **mitred** — each wall's end becomes the
line through the intersection of the two `hi` faces and the intersection of the two `lo` faces, so
every layer (leaf, cavity, plaster) closes around the corner; a **T** (an end inside another wall's body
away from its ends) is trimmed to the face of the other wall on its side (`lo`/`hi`, plaster included).
Collinear walls are not joined.
**Openings** `{ id, kind: window|door, offset (from a, to the opening start), width, height, sill (windows; doors 0), lintel?: { inner?: concrete|clay|none, outer?: steelAngle|concrete|none, bearing? }, sillStone?: bool (default true for windows) }`
cut through every layer. Lintels: inner leaf a prefab lintel (`concrete` 0.14 high, or `clay` = one
course) over width + 2 × bearing, as wide as the leaf; outer leaf a galvanised **steel angle** (L 100×100×8,
horizontal leg under the bricks, vertical leg in the cavity) over width + 2 × bearing. Outer sill:
a **bluestone** (arduin) sill under windows, projecting `sill.projection` past the outer face, over
width + 0.06 m, `sill.thickness` high, its top at the sill height. **DPC**: on level 0 a 3 mm
membrane under each leaf at its base. **Ties**: a 4 mm stainless tie across the cavity on a grid giving
`ties.perM2` per m² of (net) outer-leaf area, staggered per row. **Insulation**: `fill` fills the
cavity, `board` fills `thickness` against the inner leaf.
**Bond pattern**: the masonry faces carry the joint pattern as **edges** (one `Edge` per bed joint run
and per head joint, id `<wall>#pattern`) on the outer face of the outer leaf, and on the interior face
of the inner leaf when interior plaster is off; courses start at the leaf base, head joints offset per
`bond`, clipped to the leaf face outside the openings; not drawn on single walls that are plastered on
both sides. Face keys: `outer`, `inner`, `single`, `cavity.insulation`, `plaster.int`, `plaster.ext`,
`lintel.<o>.inner`, `lintel.<o>.outer`, `sill.<o>`, `dpc`, `tie`, `pattern` (edges).

**`masonry.floor`** `{ level, points, system: beamBlock|hollowCore|insitu, direction: auto|x|z, joistWidth, joistHeight, spacing, blockKind: concrete|clay|eps, blockHeight, blockLength, topping, mesh, bearing, ringBeam: bool, ringBeamWidth?, plankThickness, plankWidth, slabThickness, openings: [[x,z]…][], rooms?: [id] }`
— the structural floor **of storey `level`** (≥ 1): its top at the storey base, everything below
(it occupies the top of the storey under it). `points` = the outline of the floor including its
bearings (normally the cavity-side faces of the inner leaves around it). **beamBlock** ("potten en
balken"): prestressed joists (`joistWidth` × `joistHeight`, inverted-T simplified as a box) spanning the
shorter way (`auto`) or the given axis at `spacing` on centre, first and last half a block from the
edge, clipped to the outline per joist line; hollow infill blocks (`blockLength` long, filling the bay
between joists, `blockHeight` high, the joists sit in their rebates: block top = joist top +
`blockHeight` − `joistHeight` … simplified: blocks and joists share the underside, blocks `blockHeight`
high) laid from bearing to bearing; a concrete **topping** (druklaag) `topping` thick over the whole
outline with a mesh (counted); depth = `blockHeight + topping`. **Ring beam** (ringbalk): a concrete
band `ringBeamWidth` (default `bearing`) wide along every outline edge, full depth; joists and blocks
stop at its inner edge. **hollowCore** (welfsels / kanaalplaten): planks `plankWidth` wide,
`plankThickness` deep, spanning the short way side by side (the last one cut to width), plus the
topping when `topping > 0`. **insitu**: a solid slab `slabThickness`. **openings** (stair wells): holes;
beamBlock gets doubled **trimmer joists** (raveelbalken) along both sides parallel to the joists and
**headers** across the joist ends at the other two sides, joists and blocks inside are left out;
hollowCore planks are cut. Face keys: `joist.<i>`, `block.<i>.<k>`, `topping`, `ring.<e>`, `trimmer.<o>.<k>`,
`header.<o>.<k>`, `plank.<i>`, `slab`. `ceilingPlaster` (default off, so the joists and blocks show
from below) adds a 15 mm plaster ceiling under the whole floor (key `ceiling`, item `plaster`
"Plafondpleister 15 mm").

**`masonry.foundation`** `{ points, footingWidth, footingDepth, bottom, blockWidth, top? }`
— strip foundation (strokenfundering) along every edge of the polygon (normally the wall lines of
storey 0): a concrete footing `footingWidth` × `footingDepth` with its underside at `bottom` (relative
to the ground-floor level, frost-free −0.8 m) centred on the edge, and foundation masonry (concrete
blocks `blockWidth` thick, coursed 0.19 + 0.01) from the footing up to `top` (default the storey-0
base). Keys `footing.<e>`, `block.<e>`.

## Expansion (`expand.rs`)

Every element expands to `Expansion { faces: Vec<Entity>, items: Vec<Item> }` with
`Item { key, category, description, count, length, area, volume }` (zeros where not relevant), which
the takeoff aggregates by `(category, description)`. Categories: `bricks` (count, area), `blocks`
(count, area), `mortar` (volume), `insulation` (area, volume), `ties` (count), `lintels` (count, length),
`sills` (count, length), `plaster` (area), `dpc` (length, area), `joists` (count, length), `floorBlocks`
(count), `topping` (area, volume), `mesh` (area), `ringBeam` (length, volume), `planks` (count, area),
`slab` (volume), `trimmers` (count, length), `footing` (length, volume), `foundationBlocks` (count).
Units per m² of leaf = 1 / ((length + joint) × (height + joint)); mortar = leaf volume − units × unit volume.

## Ops (`{ op: custom, type: masonry.<name> }`, MCP `plugin.masonry.<name>`)

| op | data | effect |
|---|---|---|
| `set_defaults` | any default field (deep merge) | returns the defaults |
| `add_wall` | `a`, `b`, `level?`, `type?`, any wall field | new wall from the defaults (`reference` default `exterior`) |
| `add_opening` / `remove_opening` | `wall`, opening fields / `wall`, `opening` | must fit the wall |
| `add_floor` | `points` or `origin` + `size`, floor fields | |
| `add_foundation` | `points` or `origin` + `size`, fields | |
| `update` | `id`, `patch` | deep merge, validated |
| `import_construction` | `levels?`, `walls: true`, `floors: true`, `foundation: false`, `replace: true`, `only?: [{ owner, key }]` | writes the derived masonry out as stored elements so each can be customised (`only`: just those wall pieces — the room's "Customise" button): one masonry wall per construction wall piece (exterior pieces → `cavity` with the construction cavity widths, others → `single` with the construction wall thickness, `reference: center`, `source: { owner, key }`, the piece's openings converted), one `beamBlock` floor per storey ≥ 1 over the outline of that storey's rooms (cavity-side faces of the inner leaves; stair wells as openings; `rooms`), optionally a foundation under the ground-floor exterior walls; `replace` removes earlier imported elements first |
| `estimate` | `ids?` | `{ items: [{ category, description, count?, length_m?, area_m2?, volume_m3? }], totals: { bricks, blocks, mortar_m3, insulation_m2, ties, lintels, concrete_m3, joists_m, floorBlocks }, byElement }` |
| `check` | — | findings `[{ severity, element, title, detail, value?, limit? }]` + counts: lintel bearing ≥ 0.10 (0.15 advised), opening too close to a wall end (< 0.3 m), cavity 30–150 mm, ties ≥ 4 per m² (NBN EN 1996 / NIT 271: 5 advised), floor span vs system (beamBlock depth 12+4 ≤ 4.4 m, 16+4 ≤ 5.6 m, 20+4 ≤ 6.8 m, 24+4 ≤ 7.8 m; hollowCore 0.15 ≤ 6.5, 0.20 ≤ 8.5, 0.26 ≤ 10.5; insitu L/25 rule), joist spacing ≤ 0.75, topping ≥ 0.04, U-value of every exterior wall type against the EPB requirement for renovation (U ≤ 0.24 W/m²K) → warning above, info when the cavity is empty with the U after a full cavity fill |
| `uvalue` | `wall?` or `buildup?` | per wall type `{ layers: [{ name, thickness, lambda?, R }], R_total, U, requirement: 0.24, meets, withFill?: { material, U } }` (Rsi 0.13, Rse 0.04; plaster λ 0.57, facing brick 0.9, common brick 0.7, clay block 0.27, concrete block 1.1, aerated 0.16, lime-sand 1.0; unventilated cavity R 0.17, ventilated gap after boards 0.0 + Rse 0.13; insulation λ: eps 0.034, pur 0.026, mineralWool 0.035, pir 0.022, xps 0.034) |
| `wall_svg` | `wall` | elevation from the exterior with the bond pattern, openings, lintels, sills, dimensions, title block |
| `plan_svg` | `level` | storey plan: wall layers (outer, cavity/insulation, inner, plaster) in section, the floor of the storey above in dashed outline with joists and the span direction, title, scale bar |
| `summary` | — | defaults, walls (type, length, area, leaves), floors (system, area, joists, depth), foundations, totals |
| `storeys` | — | per storey `{ level, name, base, height (floor-to-floor), structure, clear }`, no change |
| `set_clear_height` | `level`, `clear` | sets `construction.storeys[level].height` = clear + the deepest structure in the storey's top; returns `storeys` |

`move` / `rotate` / `copy` work in plan. `metrics.plugins.masonry = { walls, floors, bricks, blocks, concrete_m3 }`.

## Construction plugin interplay

The construction plugin stays the source of rooms, openings and stairs. When a masonry wall carries
`source: { owner, key }` the construction plugin **does not draw** that wall piece (a piece whose owner
and face key match and whose midpoint lies on the masonry wall's line), and when a masonry floor lists
a room in `rooms` the construction plugin does not draw that room's floor slab; rooms of the storey below
whose centre lies under a masonry floor get no ceiling slab from the construction plugin either (the
structural floor is there). Doors and windows are still drawn by the construction plugin.

## Notes on the checks

Floor spans are measured along the joists between the outline and the masonry walls of the storey
below that cross the joist lines (sampled every 0.3 m), so intermediate bearing walls count. A wall end
that continues straight into another (collinear) wall piece is not an end for the lintel/pier check;
openings that touch count as one combined opening (info). In single leaves an opening against a wall
end is only an info (its lintel bears on the crossing wall). Single leaves plastered on both sides are
interior partitions: `uvalue` marks them `partition: true` with no EPB requirement.

## Browser side (`plugins/masonry/web`)

- Tools (brick-red group): **Masonry wall** (click along the outside face going clockwise; cavity or
  single leaf from the panel; the preview shows the full build-up on the right side of the line),
  **Wall opening** (hover a masonry wall: window or door, type `width; height`), **Structural floor**
  (two corners on an upper storey), **Strip foundation**.
- **Masonry panel**: a **storey table** (clear height editable, "clear + structure = floor-to-floor"), build-up for new walls from Belgian unit presets (gevelsteen 190×90×65 / WF / DF,
  snelbouw 9 and 14 cm, betonblok, cellenbeton, kalkzandsteen), cavity width, insulation (empty / full
  fill / boards, material, board thickness), ties per m², interior plaster; tool options (wall type,
  reference line, opening kind, floor system and block height); **Detail the house** (`import_construction`);
  **Cut list** (takeoff table with 5 % waste totals and CSV); **Check** (findings coloured by severity,
  click to select); **U-values** (layer table per build-up, U against 0.24, the U after a full cavity
  fill); **Plan drawing** (`plan_svg` with SVG download).
- **Entity Info**: walls edit type, reference, flip, inner-leaf height, units, cavity, insulation, ties,
  plaster, DPC, joins and openings, with **Elevation** (`wall_svg`) and **U-value**; floors edit system,
  span direction, joist spacing, blocks, topping, mesh, planks, slab, bearing, ring beam, ceiling plaster;
  foundations edit footing and block wall.
