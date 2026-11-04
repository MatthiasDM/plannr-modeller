# Plumbing plugin — contract (engine + browser)

Plugin id `plumbing`. Engine crate `plugins/plumbing/engine` (`plannr-plugin-plumbing`), browser bundle
`plugins/plumbing/web`. It lays the **pipework** of a house: Alupex, PEX, copper, PVC, PE and threaded
steel pipes with bends or elbows, the fittings that join them (press, clamp, push-fit, solder,
compression, solvent-weld, rubber-ring, welded, threaded), tapware (mixers, angle valves, traps),
valves, manifolds and the brackets that hold the pipes — plus checks and a material takeoff.
Only `plannr-core` is a dependency; the construction plugin's storeys, rooms, walls and plumbing
fixtures (`tapCold`, `tapHot`, `mixer`, `showerValve`, `toiletSupply`, `floorDrain`, `drainPoint`,
`radiator`, `boiler`, `manifold`) are read from the document JSON.

Coordinates as everywhere: metres, y up; heights in the entity data are **above the storey floor**
(`extensions.construction.storeys` + `groundLevel` give the base). `dn` is always the **outer
diameter in mm**.

## Catalogue (`catalog.rs`, op `catalog`)

| | ids |
|---|---|
| systems | `coldWater` #2E7BE4, `hotWater` #E5484D, `heatingFlow` #F2711C, `heatingReturn` #1B9AAA, `drain` #8A8F98, `rainwater` #3BA55D, `gas` #E8C62C |
| materials | `alupex` 16×2 / 20×2 / 26×3 / 32×3 (14, 40, 50, 63) — press, clamp, pushfit; bent up to 26, R ≥ 5 × dn · `pex` 16 / 20 / 25 — press, clamp, sleeve; bent, R ≥ 6 × dn · `copper` 15 / 12 / 18 / 22 / 28 (10, 35, 42) — solder, press, compression; elbows · `pvc` 40 / 32 / 50 / 75 / 90 / 110 / 125 / 160 — glue, ring; drains · `pe` 50 / 40 / 56 / 63 / 75 / 90 / 110 / 125 / 160 — weld; drains · `steel` 21.3 (½") / 26.9 (¾") / 33.7 (1") / 42.4 — thread |
| joints | `press`, `clamp`, `pushfit`, `sleeve`, `solder`, `compression`, `glue`, `ring`, `weld`, `thread` |
| fittings | `coupling`, `elbow90`, `elbow45`, `tee`, `reducer`, `adapter`, `cap`, `wallPlate` (muurplaat ½"), `stopValve`, `ballValve`, `checkValve`, `pressureReducer`, `filter`, `meter`, `angleValve` (hoekstopkraan), `radiatorValve`, `mixerWall` (150 mm centres), `mixerThermo`, `mixerBasin`, `manifold`, `trap` (sifon), `cleanout`, `branch45` |
| brackets | `clip`, `clamp` (rubber inlay + threaded rod), `double` |
| bracket spacing (horizontal; vertical × 1.6) | alupex 16: 0.5, 20: 0.6, 26: 0.8, 32: 1.0 · pex 0.4–0.6 · copper 12: 1.0, 15: 1.25, 22: 1.5, 28: 2.0 · pvc ≤ 50: 0.5, ≤ 90: 0.8, ≤ 125: 1.1 · pe 0.6–1.2 · steel 2.0–3.0 |

Rendering materials (`plumbing.<key>`): the seven system colours, `copper`, `pvc`, `pe`, `steel`,
`brass`, `press` (stainless sleeve), `pressRing` (black indicator ring), `pvcFitting`, `chrome`,
`handle`, `bracket`, `rubber`, `insulation` (45 % grey). A pipe takes its material's look (copper,
grey PVC, black PE, galvanised steel), Alupex and PEX the system colour.

## Entities

**`plumbing.pipe`** — `{ level, system, material, dn, points: [[x, yAboveFloor, z]…] (≥ 2), corners: "bend"|"elbow", joint, bendRadius?, insulation (mm), from?, to?, name?, note?, paint? }`.
The points are the **corners** of the run. `corners: bend` sweeps a round tube with a circular arc at
every corner (radius `bendRadius`, default the material's minimum, clamped so the tangents fit the
legs); `corners: elbow` draws a mitred tube with an **elbow fitting of the pipe's joint type** at
every corner (press sleeve with black ring, brass clamp fitting with hex nuts, PVC socket…). Face
keys `pipe`, `insulation`, `elbow.<i>.body|nut|ring`. `from` / `to` name the fitting or construction
fixture an end is linked to (informative — geometry decides connectivity: an end within 15 mm of a
port is connected). `move` moves the points; `paint` sets `paint` (a material override).

**`plumbing.fitting`** — `{ level, kind, joint, material, system, dn, dn2?, position, dir, dir2?, angle?, thread?, outlets?, fixture?, note?, paint? }`.
`dir` is the main axis: an inline fitting runs along ±dir (in-port at −dir, out-port at +dir); an
elbow's pipe comes along `dir` and leaves along `dir` turned by `angle` towards `dir2`; a tee branches
along `dir2`; a wall plate's pipe comes along `dir` and its ½" outlet points along `dir2` (out of the
wall); a wall mixer lies along `dir` (hot left at −0.075, cold right at +0.075) with `dir2` out of the
wall; a basin mixer's tails hang down 0.30 m; a manifold runs along `dir` with `outlets` nipples
along `dir2`; a trap's inlet points up and its outlet along `dir`. **Ports** (`fittings::ports`) are
derived from these: `{ pos, dir (outward), dn, name }`. Face keys `body`, `nut`, `ring`, `handle`,
`plate`. `move` / `rotate` transform position and axes.

**`plumbing.bracket`** — `{ level, kind, position (on the pipe axis), dir (pipe axis), normal (towards the surface), dn, standoff (axis → surface, m), pipe?, auto }` — a clip ring around the pipe, a stem to the surface and a plate on it (clamp: rubber inlay, hex nut, rod, round plate). Face keys `clip`, `stem`, `plate`.

## Ops (`{ "op": "custom", "type": "plumbing.<name>", "data": {…} }`, MCP tools `plugin.plumbing.<name>`)

| name | data | effect |
|---|---|---|
| `catalog` | — | the catalogue as JSON (`entities[0].data`) |
| `add_pipe` | `points`, `system?`, `material?`, `dn?`, `level?`, `corners?`, `joint?`, `bendRadius?`, `insulation?`, `from?`, `to?`, `autoFittings?` (true), `autoBrackets?` (false), `bracketKind?`, `name?`, `note?` | defaults: material per system (alupex / pvc), first dn and joint of the material, corners per material and dn, level from a fixture in `from`/`to`. With `autoFittings` each end is **joined to what it touches** (or to `from`/`to`): a **fitting port** → the end snaps onto it; another **pipe's end** → a `coupling` (or `reducer` when the diameters differ, an `elbow45`/`elbow90` when the directions differ) is inserted and both pipes are cut back to its ports; another **pipe's body** → that pipe is split and a `tee` (`branch45` on drains) inserted, the new pipe starts on the branch port; a **plumbing fixture** → for water / heating / gas a `wallPlate` (½") is placed at the end with its outlet out of the nearest wall, for drains / radiators / boilers / manifolds the end is only linked. `created[0]` is the pipe, then the fittings (and brackets); `measurement` = length |
| `route` | `from`, `to` (`[x, yAboveFloor, z]` or a fitting / pipe / fixture id), `fromId?`, `toId?` (what a point attaches to; a point on a pipe's body inserts a tee), `via?: direct\|ceiling\|floor`, `height?`, `variant?`, `pair?`, plus the pipe fields of `add_pipe` | works out the corners: leaves a port along its direction, goes to the travel height (direct = the start's height, ceiling = 10 cm under it, floor = 10 cm above it), crosses orthogonally (fewest wall lines crossed, then the most length within 30 cm of a wall), then to the end. `variant` picks another way. The last entity's data holds `points`, `variants`, `wallCrossings`, `floorCrossings` (the holes to drill) and `pair` |
| `parallel` | `id`, `distance?`, `side?`, `rise?`, `system?`, `material?`, `dn?`, `insulation?`, `name?`, `from?`, `to?`, `autoFittings?`, `autoBrackets?` | a second pipe with the same corners; `distance` is in plan, mitred at corners (default dn + 40 mm); `side?: auto\|left\|right` (auto stays away from the walls, a negative distance means right); risers stay vertical |
| `update_pipe` | `id`, `patch` | deep-merge (arrays replaced, `null` clears), validated |
| `set_segment_length` | `id`, `segment`, `length`, `anchor?: start\|end` | exact leg lengths: later points (or earlier ones) move along the leg |
| `connect` | `pipe`, `end: start\|end`, `to` (fitting / pipe / fixture id), `joint?` | moves the end onto the target and joins as above |
| `add_fitting` | `kind`, `onPipe?` + `position`, or `fixture?`, or `position` + `dir?` + `dir2?`; `joint?`, `material?`, `system?`, `dn?`, `dn2?`, `angle?`, `thread?`, `outlets?`, `level?`, `note?` | on a pipe: at an end (within 6 cm) the fitting is attached beyond the end, aligned with it; inside the pipe an **inline** fitting (coupling, reducer, adapter, valves, filter, meter, cleanout) splits the pipe and sits between the halves; a **tee** splits it and branches towards `position` (or `dir2`); elbows are refused mid-pipe. At a fixture: placed on it, `dir2` out of the nearest wall (a mixer along the wall). Free: at `position`. Defaults (joint, material, dn, system, level) come from the pipe or fixture |
| `update_fitting` / `update_bracket` | `id`, `patch` | deep-merge |
| `add_bracket` | `position`, `pipe?`, `kind?`, `normal?`, `standoff?`, `dir?`, `dn?`, `level?` | with `pipe` the point is projected onto it; without `normal` the surface is chosen: the nearest wall line (room edge or free wall) within 0.35 m whose normal is not along the pipe, else the ceiling when within 0.45 m of it, else the floor when under 0.3 m, else the nearest wall; `standoff` = the distance to it; kind `clamp` when the stand-off exceeds dn/2 + 60 mm |
| `auto_brackets` | `pipes?`, `spacing?`, `kind?` | per straight leg, `ceil(L / spacing)` brackets evenly spaced (spacing from the table, × 1.6 for vertical legs), surfaces as above; earlier `auto` brackets of those pipes are replaced |
| `takeoff` | — | `{ items: [{ group: pipe\|fitting\|bracket\|insulation, label, labelNl, material, dn, dn2?, joint?, kind?, length_m?, count?, pipes?, bends? }], totals }` — pipes per material / dn / system with bend counts, elbows from pipe corners (standard angles: 45 / 90, PVC 15 / 30 / 45 / 67.5 / 87.5), fittings per kind / joint / dn, brackets, insulation |
| `check` | — | findings `[{ severity: error\|warning\|info\|ok, title, detail, ids }]` + counts. **error**: material not used for the system, joint type not available for the material, bend on a material that is not bent, bend radius under the minimum, diameter change without a reducer, diameter mismatch at a fitting. **warning**: open pipe end, branch without a tee, systems joined, dn not in the series, dn too large to bend, drain without fall (< 0.5 %), bracket spacing over 1.2 × the rule, fitting with an open port, different systems on one fitting, fitting not meant for the system, pipe system ≠ fixture. **info**: pipes meet without a coupling, mixed joint types, hot water without insulation, steep drain (> 5 %), no brackets, loose fitting |
| `summary` | — | counts and lengths per system, every pipe (points, links) and every fitting with its ports and whether each is open |
| `import_runs` | `remove?` (true) | the construction plugin's `construction.run` water / drain / heating polylines become pipes with default materials |

Metrics: `metrics.plugins.plumbing = { pipes, pipeLength_m, fittings, brackets, openEnds }`.

## Browser side (`plugins/plumbing/web`)

Toolbar group (blue accent). Placing needs no typed measurements; typing stays possible for an exact length.

- **Route** (Y): click the start and the end (an open pipe end, a free port, a tap, a wall, or a pipe, which gets a tee). The engine works out the corners and previews them; **Tab** tries another way, and the panel's *Route via* sets the travel height. A toast gives the length and the number of wall and floor holes.
- **Pipe** (U): click a wall and follow it. The pipe stays dn/2 + 25 mm (plus insulation) in front of the wall, runs plumb or level by itself, stays on the grid (1, 5 or 10 cm) and lines up with pipe ends, ports and taps nearby (a magenta dashed line shows it). Hovering another wall turns the corner where the two walls meet. Clicking a pipe end, port or tap finishes there with square corners. Away from walls it draws at the panel's height, with buttons for the usual heights. **Shift** draws free of the grid and square rule, **↑ ← →** force an axis, **typing a number** makes the current leg that long; Enter or double-click finishes, Backspace removes a corner, Esc cancels.
- **Adjust pipe**: drag a leg sideways or a pipe end to make it longer or shorter. It stays on the grid and square: a neighbouring leg in line with the move changes length, any other gets a new short leg, so connected ends stay where they are. Fittings at a moved end do not follow.
- **Fitting** (J): pick the fitting in the panel, then click a pipe (inline fittings cut in, a tee branches towards the click, a cap / wall plate goes on the end), a tap or drain fixture (wall plate, mixer, trap) or a free point (axis lock sets `dir`, a clicked face sets `dir2`).
- **Bracket**: click a pipe where it is clipped; the nearest surface is chosen.

**Plumbing panel**: system → material → diameter → corners (bend with its minimum radius, or elbows) → joint type → route via → grid → height with presets → insulation; *Also lay hot / cold alongside* at a set distance (hot next to cold, return next to flow, for Route and Pipe); *Connect ends automatically*, *Brackets on new pipes*, *Snap to surfaces*, *X-ray*; the fitting kind (with branch diameter / outlets); bracket kind and **Auto brackets** (selection or all); **Run checks** (click a finding to select its entities); **Material list** (materiaallijst with CSV download); **Import runs**.

**Entity Info**: a pipe shows its name, system, material, diameter, corners, joints, bend radius, insulation, connections a **table of leg lengths that can be typed** (`set_segment_length`) and a button that lays the matching pipe alongside; a fitting its kind, joint, diameters, angle / thread / outlets, ports (open ✓) and buttons to turn the branch or flip the axis; a bracket its kind, stand-off and the surface it is fixed to.
