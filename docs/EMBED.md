# Embedding the viewer

Another app (Uzzetje, a website, an intranet page) can show a Plannr project in an iframe and drive it
from the page: switch storeys, zoom to a room, highlight an entity, take a screenshot for a report.
The app keeps its own house model and pushes it into Plannr with the construction plugin's
`import_house` op; what the viewer shows is always a Plannr project.

## The page

```
https://plannr.example.org/app?project=<id>&embed=1&owner=<key>[&origin=https://your.app]
```

| Query | Meaning |
|---|---|
| `project` | the Plannr project to show |
| `embed=1` | viewer mode: no menus, trays or status bar; the tools are Select, Orbit, Pan, Zoom, Zoom Window, Zoom Extents, Section Plane and Tape Measure; the storey bar and the Iso / Top / Front / Right buttons stay. Delete only removes section planes the viewer made. |
| `owner` | on a public instance, the owner key the project was made with (8–128 characters of `A-Z a-z 0-9 _ -`). The embedding app manages this key and sends it as `X-Plannr-Owner` on its own REST calls; it is not stored in the browser. A private instance ignores it. |
| `origin` | optional: only messages from this origin are accepted, and events go only there. Without it any parent page may talk to the viewer. |

## Messages

The page sends a command and gets an answer with the same `id`:

```js
const frame = document.querySelector('iframe');
let seq = 0; const pending = new Map();
window.addEventListener('message', ev => {
  const m = ev.data; if (!m || m.type !== 'plannr') return;
  if (m.event) { console.log('event', m.event, m); return; }          // ready, selection, changed, tool, error
  const p = pending.get(m.id); if (!p) return; pending.delete(m.id);
  m.error ? p.reject(new Error(m.error)) : p.resolve(m.result);
});
function plannr(command, args = {}) {
  return new Promise((resolve, reject) => { const id = ++seq; pending.set(id, { resolve, reject }); frame.contentWindow.postMessage({ type: 'plannr', id, command, args }, '*'); });
}
await plannr('set_storey', { level: 1 });
await plannr('isolate_storey', { on: true });
await plannr('zoom_to', { ids: [roomId] });
const shot = await plannr('screenshot', { width: 1200, height: 800 }); // { png: <base64 PNG>, width, height }
img.src = 'data:image/png;base64,' + shot.png;
```

Wait for the `ready` event before sending commands. Commands:

| Command | Args | Returns |
|---|---|---|
| `get_state` | — | `{ camera, selection, tool, hover, version, enteredGroup, display }` |
| `set_view` | `view: iso \| top \| bottom \| front \| back \| left \| right` | `{ camera }` |
| `set_camera` | `position: [x,y,z]`, `target: [x,y,z]`, `up?` | `{ camera }` |
| `zoom_extents` / `zoom_to` / `zoom_window` | — / `ids: [...]` / `x0, y0, x1, y1` (0–1 of the viewport) | `{ camera }` |
| `previous_view` / `next_view` | — | `{ camera }` |
| `orbit` / `pan` / `zoom` | `dx, dy` (pixels) / `dx, dy` / `factor` | `{ camera }` |
| `select` | `ids: [...]`, `mode?: replace \| add \| toggle` | `{ selection }` |
| `activate_tool` | `tool` (one of the viewer tools) | `{ tool, hint }` |
| `screenshot` | `width?, height?, view?` | `{ png, width, height }` (`png` is base64) |
| `set_options` | `gridVisible?, axesVisible?, edgesVisible?, figureVisible?, shadows?: bool`, `xray?: [materialId…]` | the display state |
| `get_storeys` | — | `{ level, isolated, storeys: [{ index, name, height }] }` (construction plugin) |
| `set_storey` | `level` | `{ level }` |
| `isolate_storey` | `on: bool` | `{ level, isolated }` — a dollhouse view of the active storey |
| `open_project` | `id` | `{ project, name, version }` |
| `ops` | `ops: [...]`, `label?` | `{ results, version }` — any ops, e.g. an `import_house` |
| `get_document` | — | the document JSON |

Events from the viewer: `{ type: 'plannr', event: 'ready', project, name, version }` once the project is
open, `selection` (`ids`), `changed` (`version`), `tool` (`tool`), `error` (`message`).

## Pushing the house

The embedding app owns the house model. It creates one Plannr project per house and re-imports after
each change; `import_house` replaces the construction entities of the previous import and leaves
everything else in the project alone (furniture, neighbours, guides, section planes, other plugins'
entities), so a user can draw around the imported house without losing it on the next sync.

```bash
# once: a project for the house (the key is yours to keep, one per house or per user)
curl -s -X POST https://plannr.example.org/api/projects -H 'X-Plannr-Owner: <key>' -H 'content-type: application/json' -d '{"name":"House 12"}'
# after every change in your app
curl -s -X POST https://plannr.example.org/api/projects/<id>/ops -H 'X-Plannr-Owner: <key>' -H 'content-type: application/json' \
  -d '{"label":"Sync from Uzzetje","ops":[{"op":"custom","type":"construction.import_house","data":{"house":<HouseModel JSON>}}]}'
```

The HouseModel format is the one Uzzetje stores: `floors`, `rooms` / `polyRooms`, `walls`, `openings`
(with `roomEdge`, `polyEdge` or `wall` hosts), `roof` and `roofVolumes`, `fixtures`, `runs`, `cavity`,
`finishes`, `componentTypes`; see `docs/CONSTRUCTION.md`. Ids are kept, so a room id in your app is the
id to pass to `zoom_to` and `select`.

`web/public/embed-example.html` is a working example: `/embed-example.html?project=<id>&owner=<key>`.
