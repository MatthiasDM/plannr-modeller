# Electrical plugin — contract (engine)

Plugin id `electrical`. Engine crate `plugins/electrical/engine` (`plannr-plugin-electrical`),
registered after the construction plugin. It models the **electrical installation** of a house to
Belgian AREI/RGIE conventions (residential, Boek 1) with IEC 60617 symbols, on top of the
construction plugin: the electrical *points* are the construction fixtures of kind `outlet`,
`switchPlate`, `ceilingLight`, `wallLight`, `dataPoint`, and the board is the `panel` fixture.
Only `plannr-core` is a dependency; the construction entities are read from the document JSON.

Coordinates as everywhere: metres, y up; conduit point heights are **above the storey floor**
(`extensions.construction.storeys` + `groundLevel` give the base).

## Document data

```jsonc
"extensions": { "electrical": {
  "board": { "phases": 3, "mainBreaker": 40, "mainDifferential": 300, "differentialType": "A", "earthing": "TT",
             "shortCircuitKA": 3, "defaultCableType": "XVB", "address": "", "ean": "", "meterNumber": "",
             "panel": "<fixture id of the construction panel, or null>" },
  "circuits": [{ "id", "number": 1, "name", "type": "lighting|sockets|kitchen|oven|hob|dishwasher|washingMachine|dryer|boiler|heatPump|evCharger|bathroom|outdoor|pv|dedicated|other",
                 "breaker": 16, "cableSection": 1.5, "rcd": "none|mA30|mA300", "poles": 2, "curve": "C", "cableType": null,
                 "points": ["fixtureId"…], "note": "" }],
  "rcdGroups": [{ "id", "name", "milliamps": 30, "type": "A", "circuits": ["circuitId"…] }]
} }
```

Custom entity `electrical.conduit`:
`{ "level", "circuit": id|null, "from": fixtureId|null, "to": fixtureId|null, "points": [[x, yAboveFloor, z]…] (≥ 2), "diameter": 0.016|0.02|0.025, "kind": "conduit|cable", "cableType": null, "auto": bool }`
— expanded as square tubes (material `electrical.conduit` #B8BCC2, `electrical.cable` #5A5F66), face key `run`.
`move` shifts it in plan and height (and clears `auto`); `paint` is not supported.

Type defaults (Studio's conventions): lighting 16 A / 1.5 mm²; sockets, kitchen, bathroom,
outdoor, oven, heatPump, other, dedicated 20 A / 2.5; hob, evCharger 32 A / 6 (4-pole on 3-phase);
dishwasher, washingMachine, dryer, boiler 16 A / 2.5; pv 20 A / 4 curve C, no RCD. Default RCD
30 mA. Curve C for appliances and PV, B otherwise. Cable label `XVB 3G2.5` (5G for 4-pole).
Load estimate: dedicated appliances carry a nominal load (oven 2.3, hob 7.4, dishwasher 2.2,
washing machine 2.3, dryer 2.7, boiler 2.2, heat pump 3.5, EV 7.4, dedicated 2.3 kW); lighting
0.1 kW and other points 0.3 kW per point; total × 0.7 diversity. Capacity = 230·I or √3·400·I.

## Ops (`{ "op": "custom", "type": "electrical.<name>", "data": {…} }`, MCP tools `plugin.electrical.<name>`)

| name | data | effect |
|---|---|---|
| `set_board` | any board field | update the board (`panel` = fixture id) |
| `add_circuit` | `name?`, `type`, `breaker?`, `cableSection?`, `rcd?`, `poles?`, `curve?`, `cableType?`, `points?`, `note?`, `number?` | defaults per type; points are taken from other circuits; returns the circuit as `entities[0]` (`electrical.circuit`) |
| `update_circuit` | `id`, same fields | `points` replaces the list |
| `remove_circuit` | `id` | conduits of it lose their circuit |
| `renumber` | — | 1… in board order |
| `assign` | `points`, `circuit?` | put points on a circuit (or none); a point is on at most one circuit |
| `add_rcd_group` / `update_rcd_group` / `remove_rcd_group` | `name`, `milliamps`, `type`, `circuits` / `id`, … / `id` | differential groups |
| `add_conduit` | `level?`, `points`, `circuit?`, `from?`, `to?`, `diameter?`, `kind?`, `cableType?` | level from `from`/`to` fixture when omitted; `measurement` = length |
| `update_conduit` | `id`, `patch` | deep-merge into the conduit's data (arrays replaced, `null` clears); validates diameter ∈ {0.016, 0.02, 0.025}, kind ∈ {conduit, cable}, circuit exists; clears `auto` |
| `auto_route` | `circuit?`, `mode: ceiling\|floor\|straight`, `height?` | one conduit per point from the board (`board.panel`, else the first panel on the point's storey, else the first panel), routed **inside the building fabric** (`route.rs`, using the construction plugin's wall graph and fixture mounts): **ceiling** = up inside the board's wall to the horizontal chase zone 0.15 m under the ceiling (`height` overrides it), along the walls by the shortest path through the wall graph (middle of the room-side leaf; T-junctions connect), down inside the point's wall 25 mm behind its face to the box; ceiling lights and points away from any wall (a ceiling socket) leave the wall at the top and cross inside the ceiling build-up. **floor** = down inside the board's wall into the floor screed (half the floor thickness under the floor), straight across, up inside the point's wall. **straight** = direct. Other storeys: a riser conduit (data `riser: true`, one per circuit) follows the walls from the board to the nearest wall junction that also lies on a wall of the next storey and climbs inside it; the point's run continues from the riser's top. Earlier auto conduits (`auto: true`) of the routed circuits are removed. Conduit diameter from the cable section (≥ 6 mm² → 25, ≥ 2.5 → 20, else 16 mm). |
| `check` | — | AREI findings in `entities[0].data.findings` (`[{ severity: error\|warning\|info\|ok, title, detail, circuit?, points? }]`) plus `errors` / `warnings` counts |
| `summary` | — | `{ board, circuits: [{ id, number, name, type, breaker, cableSection, rcd, poles, curve, pointCount, load_kW, cable, conduitLength_m }], rcdGroups, points: { total, assigned, unassigned }, demand_kW, capacity_kW, conduitLength_m, conduits }` |
| `situation_svg` | `level`, `scale?` (px/m, 40), `showConduits?`, `title?` | `entities[0].data.svg`: the storey's rooms and free walls, every electrical point as an AREI/IEC symbol coloured and numbered per circuit, conduits dashed, legend, title block |
| `oneline_svg` | `title?` | the eendraadschema: kWh meter → main breaker → head differential → busbar, one column per circuit (number, breaker `2P C16 A`, differential, cable `XVB 3G2.5`, points as symbols with counts, name), legend, signature line |
| `import_electrical` | `electrical` (Studio JSON) | board + circuits; `fixtureIDs` → `points` (unknown ids dropped and listed in `entities[0].data.dropped`); circuits numbered in order; `board.panel` set to the first panel fixture |

### Checks implemented

error: no head differential (300 or 100 mA type A required); breaker above the cable's limit
(1.5 → 16 A, 2.5 → 20, 4 → 25, 6 → 32, 10 → 40, 16 → 63); 30 mA missing on sockets, kitchen,
bathroom, outdoor and appliance circuits. warning: fewer than two circuits or lighting/sockets not
separated; dedicated circuit with more than one point; too many points (sockets/kitchen 8, lighting 8
as guidance, bathroom/outdoor/other 6); an appliance in the house (unit type/name or fixture note:
oven, kookplaat/hob, vaatwasser/dishwasher, wasmachine, droogkast/dryer) without a circuit of that
type; unassigned points (ids listed); estimated demand above capacity; more than 8 circuits behind one
30 mA group; unusual head differential value. info: circuit without a conduit; conduit without a
circuit; PV circuit without a differential. `ok` when there is no error or warning. Advisory only —
a certified keuring is still required.

Metrics: `metrics.plugins.electrical = { circuits, points, assigned, unassigned, conduitLength, demand_kW }`
(present when the document has circuits or conduits).

## Browser side (`plugins/electrical/web`)

- **Assign to circuit** tool (bolt icon): pick a circuit in the Electrical panel, click sockets, switches, lights, data points to put them on it; Shift-click unassigns. Hovering shows the point's circuit. A fixture's Entity Info also gets a Circuit dropdown.
- **Conduit** tool: click the bends of a run at the height set in the Measurements box (default 2.45 m); start on the board or a point to attach it (`from`), click a point to end on it (`to`), otherwise double-click or Enter. Kind (flush conduit / surface cable) and diameter (16/20/25 mm) are picked in the panel; the conduit takes the active circuit.
- **Electrical panel**: board settings (phases, main breaker, head differential, earthing, cable type, address), the circuit list (click = active circuit and selects its points; ✎ opens the editor: name, type, breaker, cable, differential, poles, curve, note, remove), **Auto-route** (via ceiling / via floor / straight), conduit tool options, **AREI checks** (click a finding to select its points), and the **Situatieschema** / **Eendraadschema** buttons that open the engine's SVG in a viewer with a download button.
- **Show conduits through walls (X-ray)** in the panel draws the conduit and cable materials through everything in front of them (kept per browser; MCP: `set_options` with `xray: ["electrical.conduit", "electrical.cable"]`).
- Conduits are selectable like any element (Entity Info: circuit, kind, diameter, length) and move with the Move tool.
