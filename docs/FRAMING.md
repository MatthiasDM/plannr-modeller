# Framing plugin — contract (engine)

Medeek-style parametric **platform framing** on top of Plannr: stud walls with rough openings, floor
joists, roof trusses and rafters, and foundations, each a semantic entity that the engine frames into
lumber members, sheathing panels and concrete at compile time, plus a material takeoff. Plugin id
`framing`; crate `plugins/framing/engine` (`plannr-plugin-framing`), registered after `electrical`.
Units: metres, y up; plan points `[x, z]`. Storey bases and heights come from
`extensions.construction` (`groundLevel`, `storeys[].height`, `floorThickness`) when present, else
0 / 2.6 / 0.06. Everything the plugin creates goes on a **"Framing"** layer (created on demand).

## Lumber conventions

| Nominal | Actual (mm) | | Nominal | Actual |
|---|---|---|---|---|
| 2x3 | 38 × 64 | | 2x10 | 38 × 235 |
| 2x4 | 38 × 89 | | 2x12 | 38 × 286 |
| 2x6 | 38 × 140 | | 4x4 | 89 × 89 |
| 2x8 | 38 × 184 | | 4x6 / 6x6 | 89 × 140 / 140 × 140 |

Metric names are accepted too (`38x140`). Spacing is any number in metres (0.4 = 16", 0.6 = 24",
0.3 = 12"). **Headers** by clear span: ≤ 1.2 m 2x6, ≤ 1.8 m 2x8, ≤ 2.4 m 2x10, else 2x12, built up from
two plies (one against each wall face); `headerSize` overrides per opening. Stock lengths for the
takeoff: 2.44, 3.05, 3.66, 4.27, 4.88 m (longer members round up in 2.44 m steps). Panels are 1.22 × 2.44 m.

Materials (ids `framing.<key>`): lumber #E3C597, plate #D8B784, header #BF9552, sheathing #D9B58A,
gypsum #F1F0EC, concrete #A9A6A0, steel #8E9398, shingles #5B5B5E, metal #7B8794, tiles #9C4A33, soffit #EDEAE3.

## Settings

```jsonc
"extensions": { "framing": { "defaults": {
  "units": "metric", "studSize": "2x4", "studSpacing": 0.4, "plateSize": "2x4", "topPlates": 2, "bottomPlates": 1,
  "sheathing": { "exterior": { "kind": "osb", "thickness": 0.011, "enabled": true }, "interior": { "kind": "gypsum", "thickness": 0.0125, "enabled": false } },
  "headerRule": "span", "cornerDetail": "california", "tDetail": "twoStud" } } }
```

### Engineering and hardware defaults

Read from the same `defaults` object (extra keys, all optional):

```jsonc
"species": "C24",                     // C18 | C24 | C30 (EN 338) | SPF2 | DFL2 (NDS No.2, converted)
"loads": { "floorLive": 2.0, "floorDead": 0.5, "roofSnow": 0.8, "roofDead": 0.6 },   // kN/m²; snow on plan, roof dead along the slope
"deflection": { "floor": 360, "roof": 240 },   // L / limit: floors under imposed load, roofs under total load
"soilBearing": 100,                   // kPa, allowable
"hardware": { "anchorBolts": true, "holdDowns": false, "hurricaneTies": true, "joistHangers": true }
```

Characteristic values (MPa): C18 f_m 18 / f_v 3.4 / E 9000, C24 24 / 4.0 / 11000, C30 30 / 4.0 / 12000;
SPF No.2 and DF-L No.2 from NDS reference values (F_b 875 / 900 psi, F_v 135 / 180 psi, E 1.4 / 1.6 Mpsi)
× 2.1 (bending) and × 2.88 (shear), bending × the No.2 size factor C_F (2x4 1.5 … 2x12 1.0). LVL
(beams) f_m 44 / f_v 4.1 / E 13800 with γ_M 1.2; glulam GL24h 24 / 3.5 / 11500 with γ_M 1.25.

## Entities (`kind: custom`, `type: framing.<x>`)

**`framing.wall`** `{ level, a, b, reference: exterior|center|interior, height?, studSize, studSpacing, plateSize?, topPlates, bottomPlates, sheathing, flip, openings: [{ id, kind: window|door, offset, width, height, sill, headerSize?, kingStuds, jackStuds }], startStud, endStud, cornerDetail: california|threeStud|twoStud|none, tDetail: twoStud|ladder|california|none, startJoin / endJoin: auto|through|butt|none, room?, edge? }`
— height defaults to storey height − floor thickness (2.44 when nothing is known). The **exterior** is
the left of a→b seen from above (`flip` swaps it), so walls drawn **clockwise** have their exterior
outside. `reference` says what the drawn line is: the **exterior face of the studs** (`add_wall`
default, Medeek-style: draw along the outside of the building, the framing lies inside the line and the
sheathing outside it), the stud **center** line (walls framed from construction rooms, and documents
without the field), or the **interior** face.

**Junctions are automatic** (`junction.rs`), from the bands (line + face offsets) of all walls on the storey:
- **L corner** — a wall end lies in another wall's band near one of its ends and vice versa (not
  parallel). One wall runs **through** to the far face of the other, the other **butts** against its
  near face (framing, plates and boards trimmed/extended along the line). In a chain drawn one wall
  after the other, the earlier wall (its `b` at the corner) runs through and the next (its `a`) butts;
  `startJoin` / `endJoin` force `through` / `butt` or switch the junction off (`none`), and a forced
  choice on the other wall is respected. The through wall gets its `cornerDetail`: **california** (a
  stud turned flat against the face the other wall sits on, centred on the inside corner — drywall
  backing; `add_wall` default via `defaults.cornerDetail`), **threeStud** (an extra stud at the inside
  corner), **twoStud** / `none` (end studs only, drywall clips). The uppermost of two top plates
  interlocks: the butting wall's laps over to the far face, the through wall's is cut back to the
  inside corner. Exterior sheathing of the butting wall runs past the corner by the other wall's
  sheathing thickness (covering its edge); the through wall's stops at the other wall's outer face.
  Interior boards stop on the other wall's board (butting) or framing face (through).
- **T** — a wall end lies in another wall's band away from its ends: it butts against the face on its
  side (boards stop on that wall's board) and its upper top plate laps across the other wall, whose
  upper plate gets a gap there; the other wall gets a partition backer per its `tDetail`: **twoStud**
  channel (a stud each side of the partition; default via `defaults.tDetail`), **ladder** (flat 2x
  blocks every 0.6 m between the studs either side, flush with the face), **california** (a flat stud
  on the face), `none`.
- Collinear walls are not joined.

Members: bottom plate(s) and top plate(s) over the trimmed extent; studs at `studSpacing` from the
framing start with an end stud at its end (regular studs that would collide with a corner or backer
stud are left out); per opening king studs (full height, outside), jack studs (under the header), a
two-ply header between the kings, for windows a flat sill and cripples below, cripples above the
header at the regular spacing; regular studs inside the opening zone are removed. Exterior sheathing
is a thin panel over the (lapped) wall face with the rough openings as holes; interior gypsum
likewise when enabled. Face keys: `plate.bottom.<i>`, `plate.top.<i>` (pieces of the uppermost plate
`plate.top.0.<k>`), `stud.<i>`, `king.<o>.<l|r>.<k>`, `jack.<o>.<l|r>.<j>`, `header.<o>` (both plies),
`sill.<o>`, `cripple.<o>.<i>`, `corner.start|end.0`, `tbacker.<b>.<k>`, `tbacker.<b>.block.<k>`,
`sheathing.ext`, `sheathing.int`.

**`framing.floor`** `{ level, points, joistSize: 2x10, joistSpacing: 0.4, direction: auto|x|z, rimJoist, blocking, subfloor: { thickness: 0.018, enabled }, elevation }`
— joists span the shorter bounding-box side (`auto`) or the given axis, repeating at the spacing from
one side to the other (first and last just inside the rim), clipped to the polygon at each joist line;
rim joists along every polygon edge; one row of blocking at mid-span; a subfloor slab on top.
Keys `joist.<i>`, `rim.<i>`, `blocking.<i>`, `subfloor`. The joist underside sits at storey base + `elevation`.

**`framing.roof`** `{ level, min, max, shape: gable|hip|shed|gambrel, type: fink|howe|kingPost|queenPost|scissor|attic|mono|rafters, pitch: 6 | pitchDeg, ridgeAlongX, overhang: 0.45, heel: 0, highSupport: ponyWall|ledger, spacing: 0.6, chordSize, webSize, rafterSize, ridgeBoard, hipRafterSize?, gableEnds, highSide: max|min, lowerPitch: 18, breakRatio: 0.25, bottomPitch?, atticWidth?, atticHeight: 2.3, floorJoistSize: 2x10, sheathing: { enabled, thickness }, fascia: { enabled: true, size: 2x6 }, soffit: { enabled: false, thickness: 0.012 }, cladding: { kind: none|shingles|metal|tiles, thickness? }, gutters: false, dormers: [...] }`
— code in `roof.rs`. Local frame: `s` across the span, `r` along the run (ridge), heights above the bearing.
Members are boxes; truss members are 38 mm along the ridge.
- **Bearing**: the roof bears on the top plates of the framed walls on `level` that run along its footprint edges
  (within 0.35 m): the bearing height is the highest wall top there and the seat width the deepest plate. Without
  framed walls it sits on top of the storey (base + height) with an 89 mm seat.
- **Heel** (`heel` = height of the sloped members' underside above the plate at the wall's outer face). Trusses: a
  standard heel by default, the top chord landing on the bottom chord (chord depth; attic trusses: the floor-joist
  chord). A `heel` larger than that is a raised (energy) heel: a heel post `….heel.l|r` (mono: `….heel`) in the truss
  plane, flush with the wall's outer face, from the bottom chord up to the top chord. Rafters: a birdsmouth — the
  underside crosses the plate top at its inner edge, the seat shortened on steep pitches so the notch takes at most a
  third of the rafter depth (members are drawn un-notched; the plate corner runs into the rafter by the notch).
- Ceiling joists and collar ties sit beside the rafters (not in them), on the plates. The ridge board is at least the
  rafters' plumb depth and flush with their tops.
- **gable** (default): trusses from one end at `spacing` plus one at the far end: top chords from the heel at each
  bearing to the ridge with overhang tails, a bottom chord, webs by type (fink W: 4, howe: king post + 2 verticals +
  2 diagonals, kingPost: 1, queenPost: 2, **scissor**: bottom chords `bottom.l` / `bottom.r` rising at `bottomPitch`
  (default half the pitch) with a king post and verticals at the quarter points, **attic**: a `floorJoistSize` bottom
  chord, knee walls `knee.l|r` `atticWidth` apart (default half the span), a ceiling `collar` at `atticHeight` above
  the floor and a strut from each knee foot), gable studs at 0.6 m on the end trusses. `rafters`: common rafters on
  both slopes against a ridge board plus ceiling joists at the bearing.
- **hip**: the ridge always runs along the longer side, length = run − span (0 = a pyramid). Rafters: ridge board,
  common rafters over the ridge section, 4 hip rafters from the overhang corners to the ridge ends (`hipRafterSize`,
  default one size deeper than the rafters), jack rafters on the side slopes up to the hips, a king common and jacks
  on each end slope, ceiling joists. Trusses: common trusses over the ridge section, a doubled hip girder truss at
  each end of it (`girder.<g>`, 4 plies; 2 for a pyramid), mono jack trusses on the end slopes from the end wall to
  the girder (`jackTruss.<j>.top|top2|bottom|end`, flat top extension where the hip passes), side jack rafters and
  the 4 hip rafters.
- **shed**: one slope rising toward `highSide` (`max` / `min` across the span). Trusses (any truss type, and `mono`,
  which is shed-only) become mono trusses: `top` (eave to eave with both tails), `bottom`, `end` vertical at the high
  wall (flush with its face, cut under the top chord), `web.0..2` (verticals at thirds + a diagonal). Rafters run eave
  to eave, birdsmouthed on the low wall; at the high wall they sit on a pony wall on its top plate
  (`highSupport: ponyWall`, default: `pony.plate.bottom|top`, `pony.stud.<j>` at `spacing`) or hang on a `ledger`
  against a taller wall (`highSupport: ledger`).
- **gambrel** (framed with rafters whatever the type): lower rafters at `lowerPitch` (rise per 12, default 18) up to
  the knee at `breakRatio` × span from each eave, upper rafters at `pitch` to the ridge board, `purlin.l|r` under the
  knees standing on knee walls (`kneeWall.l|r.plate`, `kneeWall.l|r.stud.<j>`) on the ceiling joists, a `collar.<i>`
  tie a third of the way down from the ridge and ceiling joists on the plates.
- **dormers** (gable roofs only): `{ id, side: l|r (front = l, back = r), offset (along the ridge from the min end to
  the dormer's left edge), width (≥ 0.5), wallHeight: 1.0 (front wall above the roof framing at its line), pitch: 8,
  kind: gable|shed, setback: 0 (front wall inward from the exterior wall line) }`. The rafters / top chords in the
  opening are cut into a tail (to the front wall) and an upper piece (`….upper`, from where the dormer roof meets the
  main roof); truss webs on that side are left out. Doubled `trimmers` either side (full length), doubled `headers`
  across the opening at the front wall (when set back) and at the top. The dormer has a 2x4 front wall (plates,
  studs, a framed rough window `width − 0.3` × `wallHeight − 0.3` with king studs, header, sill and cripples), a gable
  triangle (gable kind), two cheek walls with studs between the main roof and the wall top / shed roof, and its own
  roof: gable = a 2x8 ridge and 2x6 rafter pairs (valley jacks where the main roof rises), shed = 2x6 rafters from a
  15 cm overhang back to the main roof (pitch kept flatter than the main roof: a third of it when not). The dormer's
  ridge is kept 5 cm under the main ridge by lowering its walls. Main sheathing / cladding get the opening cut out.
- **finishing**: `fascia` boards (2x6, category `fascia`) along every eave and rake; `soffit` panels (category
  `soffit`, material `framing.soffit` #EDEAE3) level under each eave overhang from the fascia back to the wall line;
  `sheathing` planes on every slope (trapezoids / triangles on hips, two pitches on gambrels, dormer roofs);
  `cladding` on top of the sheathing (material `framing.shingles` #5B5B5E, `framing.metal` #7B8794, `framing.tiles`
  #9C4A33; thickness default shingles 6 mm, metal 3 mm, tiles 30 mm; category `cladding`); `gutters`: a box gutter
  (steel) outside the fascia along every eave that drains (both gable / gambrel eaves, all four hip eaves, the low
  shed eave), counted in metres (category `gutters`, not lumber).

Keys: `truss.<i>.top.l|r` (`.upper` when cut), `truss.<i>.top` (shed), `truss.<i>.bottom` (`.bottom.l|r` scissor),
`truss.<i>.web.<k>`, `truss.<i>.gable.<k>`, `truss.<i>.knee.l|r`, `truss.<i>.collar`, `truss.<i>.end`,
`girder.<g>.*`, `jackTruss.<j>.*`, `ridge`, `ledger`, `rafter.<i>.l|r` (`.upper`; gambrel `.l.lower|upper`),
`rafter.<i>` (shed), `kingCommon.front|back`, `hip.<0..3>`, `jack.l|r|f|b.<j>`, `ceilingJoist.<i>`, `collar.<i>`,
`purlin.l|r`, `kneeWall.l|r.*`, `pony.*`, `truss.<i>.heel.l|r`, `fascia.eave.<k>`, `fascia.rake.<k>`, `dormer.<i>.trimmer.l|r.<0|1>`, `dormer.<i>.header.bottom|top.<0|1>`,
`dormer.<i>.front.plate.bottom|top.<n>`, `dormer.<i>.front.stud|cripple.<j>`, `dormer.<i>.front.king.l|r`,
`dormer.<i>.front.header|sill`, `dormer.<i>.gable.<k>`, `dormer.<i>.cheek.l|r.stud.<j>`, `dormer.<i>.cheek.l|r.plate.top|bottom`,
`dormer.<i>.ridge`, `dormer.<i>.rafter.<j>[.l|.r]`, faces `sheathing.<plane>`, `cladding.<plane>`, `soffit.<k>`,
`gutter.<k>` (planes: `l`, `r`, `front`, `back`, `main`, `l.lower`, `l.upper`, …, `dormer.<i>.l|r|roof`).
Member categories: trusses, rafters, ridge, joists, hips, jacks, ledger, purlins, collars, fascia, trimmers, headers,
dormer studs, dormer plates, dormer rafters. Trusses are counted as distinct `truss.<i>` / `girder.<g>` / `jackTruss.<j>`.

**`framing.foundation`** `{ points, type: slab|stemwall, slabThickness: 0.15, stemHeight: 0.6, stemThickness: 0.2, footingWidth: 0.5, footingDepth: 0.2, slab: true, top? }`
— slab: one concrete slab under the polygon; stemwall: a stem wall centred on each edge on a footing,
plus a slab inside when `slab`. `top` = elevation of the top (default storey 0 base). Keys `slab`,
`stem.<i>`, `footing.<i>`.

**`framing.beam`** `{ level, a, b, elevation?, size: 2x10, plies: 2, material: lumber|lvl|glulam, width?, depth?, posts: true, postSpacing?, postSize: 4x4, tributary? }`
— a beam on the line a→b with its top at storey base + `elevation` (default: the storey height, a
dropped beam under the floor above). Lumber: `plies` (1–4) of `size` side by side, centred on the
line; LVL / glulam: one piece `width` (default plies × 38 mm) × `depth` (default the size's depth).
Posts (`postSize`) from the storey floor to the beam's underside at both ends and so that no two are
more than `postSpacing` apart. Joist hangers (hardware) where a framing floor's joist ends within the
beam's width + 80 mm and overlaps it in height (a flush beam). Keys `ply.<i>` (category `beams`),
`post.<i>` (`posts`), `hw.hanger.<i>`. Supports for the check = the post positions (else the ends).

**`framing.stair`** `{ level, a, direction: [1, 0], width: 0.9, totalRise?, riserHeight?, treadDepth: 0.26, stringers: 3, stringerSize: 2x12, treadThickness: 0.032, risers: true }`
— a straight flight starting at `a` (foot of the first riser, middle of the width) going up along
`direction` (`add_stair` also takes `angle` in degrees: 0 = +x, 90 = +z). `totalRise` defaults to the
storey height; n risers = round(rise / riserHeight) or ceil(rise / 0.18), all equal; n − 1 treads with
a 25 mm nosing (the top step is the upper floor). Stringers are the notched (sawtooth) profile: tread
seats `treadThickness` under each step, a straight underside `stringerSize` depth under the notch tips,
cut level on the floor and plumb at the top; spread evenly over the width. Optional 18 mm riser boards.
Keys `stringer.<i>` (category `stringers`, length along the pitch line for the takeoff), `tread.<k>`
(`treads`), `riser.<k>` (`risers`).

**Hardware** (category `hardware`, counted in pieces, drawn as small steel parts; key
`hw.<kind>.<i>` on the owning element): `hw.anchor` "Anchor bolt M12" in the bottom plates of storey-0
walls — 0.3 m from each end of every plate run, at most 1.8 m apart, none across doors (the plate is cut
there); `hw.holddown` "Hold-down" at both ends of every wall (when `holdDowns`); `hw.tie` "Hurricane
tie" at both bearings of each truss bottom chord / ceiling joist; `hw.hanger` "Joist hanger" (beams).
`hardware::anchor_bolts(wall, house)` gives the bolt positions on the wall's centre line.

## Ops (`{ op: custom, type: framing.<name> }`, MCP tools `plugin.framing.<name>`)

| op | data | effect |
|---|---|---|
| `set_defaults` | any default field (deep merge) | returns the defaults |
| `add_wall` | `a`, `b`, `level?`, any wall field | new wall from the defaults |
| `add_opening` | `wall`, `kind`, `offset`, `width`, `height`, `sill?`, `headerSize?`, `kingStuds?`, `jackStuds?` | must fit the wall; returns the opening id |
| `remove_opening` | `wall`, `opening` | |
| `add_floor` | `points` or `origin`+`size`, floor fields | |
| `add_roof` | `min`+`max` or `origin`+`size`, roof fields | validated: shape, type (`mono` needs `shed`), highSide, cladding kind, pitch > 0, fascia size; dormers only on gable roofs, inside the run, width ≥ 0.5 |
| `add_foundation` | `points` or `origin`+`size`, foundation fields | |
| `update` | `id`, `patch` | deep merge (arrays replaced), validated |
| `frame_room` | `room`, `which: exterior|all`, `height?`, `studSize?`, `studSpacing?`, `sheathing?` | one wall per room edge (exterior = not collinear-overlapping with another room's edge on the storey), the room's construction openings become rough openings (`offset = along − width/2`; door/garage/passage → door), height = storey height, exterior side away from the room centre; shared edges get no exterior sheathing |
| `frame_level` | `level`, same options | `frame_room` for every room on the storey |
| `estimate` | `ids?` | `{ items: [{ category, description: "2x4 × 2.44 m", count, length_m?, area_m2?, panels?, volume_m3? }], totals: { lumber_m, lumber_pieces, sheathing_m2, sheathing_panels, gypsum_m2, gypsum_panels, concrete_m3, cladding_m2, soffit_m2, gutters_m, hardware_pieces }, byElement: [{ id, type, items }], stockLengths_m }` (lengths rounded up to stock lengths) |
| `summary` | — | defaults, walls (studs, openings, lumber), floors (joists, area), roofs (shape, type, trusses, rafters, hips, jacks, dormers), foundations (concrete), beams (length, material, section, posts), stairs (risers, riser height, going, stringers), totals |
| `add_beam` | `a`, `b`, beam fields | validated (size, post size, material, plies 1–4) |
| `add_stair` | `a`, `direction` or `angle`, stair fields | direction normalised |
| `check` | — | engineering-lite findings (below), no change: `{ species, loads, deflection, findings: [{ severity: error\|warning\|info\|ok, element, title, detail, value?, limit? }], errors, warnings }` |
| `span_table` | `use: floor\|roof`, `spacing: 0.4`, `species?`, `loads?`, `pitch: 6`, `sizes?` (2x6…2x12) | `{ spans: [{ size, span_m, governs: bending\|shear\|deflection }], … }`, no change |
| `wall_svg` | `wall` | `{ wall, svg }`: elevation seen from the exterior — every member a `<rect class="member <category>">`, rough openings dashed red with `W1: w × h, sill s` / `D1: w × h`, stud on-centre ticks with positions (mm), overall length and height, title block |
| `plan_svg` | `level` | `{ level, svg }`: storey framing plan — wall plates and studs, joists (centre lines) with a span arrow and `size @ spacing`, beams dashed with their section, roof members dashed, stairs with an `UP n × r` arrow, 1 m scale bar, title; every member drawn once (`class="member …"`) |

`estimate` also lists `hardware` items by description (`count` only) and `totals.hardware_pieces`.

### Engineering check (`check`)

Simplified EN 1995 for simply supported members under uniform load, a design aid only: ULS
w = (1.35 g + 1.5 q) × tributary width; f_d = k_mod f_k / γ_M (γ_M 1.3, k_mod 0.8 floors, 0.9 snow);
σ = w L²/8 / (b h²/6) ≤ f_m,d; τ = 1.5 (w L/2) / (0.67 b h) ≤ f_v,d; δ = 5 w L⁴ / (384 E I) ≤ L / limit.
A utilisation above 1 is an error, above 0.9 a warning, else ok.

- **Floor joists**: span = the longest joist, at the joist spacing.
- **Rafters**: horizontal span = half the roof span; plan loads (dead × √(1 + m²) + snow); a `ceilingJoists: false` roof warns that the ridge must become a structural ridge beam. **Trusses**: spacing above 600 mm warns, spans above 12 m get an info note; otherwise ok (manufacturer-designed).
- **Studs**: wall height against the maximum for the stud depth (2x3 2.44 m at ≤ 400 mm, 2x4 3.05, 2x6 4.27, deeper 5.5) → error; slenderness h·√12/d above 150 → warning.
- **Headers**: a `headerSize` shallower than the span rule → warning; with a roof (eave walls of a roof on that storey: (dead × √(1 + m²) + snow) × (span/2 + overhang)) or a floor above (walls across its joists: (g + q) × half the joist span) the 2-ply header is checked in bending, shear and L/360 over width + jack.
- **Beams**: span = largest distance between supports; load width = `tributary` or half the extent of the floor on the same or next storey the beam crosses perpendicular to its joists (none → info).
- **Footings** (stem walls): the worst edge's line load (roofs and floors per storey + 0.5 kN/m wall per storey + stem and footing at 25 kN/m³) over the footing width against `soilBearing`; slabs get an info note.
- **Stairs**: riser outside 150–200 mm → warning (above 220 error), going under 230 mm → warning, 2R + G outside 570–650 mm → warning (else ok), less than 90 mm of stringer under the notches → warning, plus a headroom reminder (2.0 m).

Results that carry JSON use `entities[0].data` (type `framing.<name>`), like the other plugins.
`move` / `rotate` / `scale` / `copy` work in plan (walls: a/b; floors/foundations: points; roofs: min/max,
turns only in 90° steps). `paint` is rejected. `metrics.plugins.framing` =
`{ walls, studs, trusses, joists, lumber_m, concrete_m3 }`. `on_load` ensures defaults, materials and
the Framing layer once the document holds framing.

## Browser side (`plugins/framing/web`)

- Tools (olive-green group): **Framed wall** (click along the outside face going clockwise, chained; the preview shows the stud band on the correct side of the line with the exterior face in green, length and stud count; the panel's "Draw walls along" picks exterior face / centre line / interior face; corners and T-junctions join by themselves), **Rough opening** (hover a framed wall, click; window or door from the panel; type `width; height`), **Floor framing**, **Roof** (footprint on top of the active storey; shape gable / hip / shed / gambrel, framing (Fink, Howe, king post, queen post, scissor, attic, mono trusses or rafters) and pitch from the panel), **Foundation** (slab or stem wall), **Beam** (two clicks; plies × size from the panel, posts underneath), **Stair** (click the foot, then the climbing direction; the label shows risers and run), **Frame room** (click a construction room: its exterior or all walls are framed with the openings it has).
- **Framing panel**: defaults for new walls (stud size, spacing, top plates, exterior sheathing, interior gypsum, corner and T details), "Draw walls along" (exterior face / centre line / interior face), beam plies and size, stair width, **Engineering & hardware** (species, snow and floor loads, anchor bolts, hurricane ties, hold-downs, joist hangers) with **Check** (findings listed in the panel, coloured by severity with utilisation / limit; click one to select its element), **Span table** (allowable joist and rafter spans per size at 300 / 400 / 600 mm) and **Framing plan** (the storey's `plan_svg` in a viewer with SVG download), tool options (opening kind, roof type, pitch in /12 with the angle shown, foundation type, exterior/all for Frame room), counts, **Frame whole storey**, **Cut list**.
- **Entity Info** for a framed wall edits height, stud size and spacing, plates, flip, sheathing and every rough opening (offset, width, height, sill, header size, remove); floors edit joists, spacing, direction, rims, blocking, subfloor, elevation; roofs edit shape, framing type, shape-specific fields (high side, gambrel lower pitch and knee, scissor ceiling pitch, attic room), pitch, overhang, heel, spacing, chord/web sizes, ridge direction, gable ends, sheathing, fascia, soffit, cladding, gutters and a **dormer list** (slope, gable/shed, offset, width, wall height, pitch, setback; + Dormer / remove) on gable roofs; beams edit material, size, plies, elevation, posts, post size and spacing, load width; stairs edit width, rise, riser height, tread depth, stringers, riser boards; framed walls also edit the reference line, corner and T details, start/end joins and open a **Wall drawing** (`wall_svg`); foundations edit type and thicknesses.
- **Cut list** opens the `estimate` result as a table (category, item, count, stock length, total length, area or volume) with totals and a CSV download; with framing elements selected it covers only those.
- Framing lives on its own "Framing" tag, so the construction plugin's walls can be hidden to look at the framing alone.
