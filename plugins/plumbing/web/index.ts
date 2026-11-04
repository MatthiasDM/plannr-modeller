// Plumbing plugin — browser side: the Pipe, Fitting and Bracket tools, the Plumbing panel (catalogue
// choices, auto brackets, checks, material takeoff) and Entity Info for pipes, fittings and brackets.
// Runtime deps come from window.plannr (no imports). Engine side: plugins/plumbing/engine.
import type { Tool, ToolContext, Plane } from '../../../web/src/tools/base';
import type { Plugin, PluginHost } from '../../../web/src/plugins';
import type { Document, Entity, CustomEntity, Op, Vec3 } from '../../../web/src/model';
import type * as THREE_NS from 'three';

type V3 = THREE_NS.Vector3;
const P = (window as any).plannr as { registerPlugin(p: Plugin): void; THREE: typeof THREE_NS };
const THREE = P.THREE;
const ACCENT = '#2E7BE4';

// ---- catalogue (mirrors plugins/plumbing/engine/src/catalog.rs) ----------------------------------
const SYSTEMS: [string, string, string][] = [['coldWater', 'Cold water · koud', '#2E7BE4'], ['hotWater', 'Hot water · warm', '#E5484D'], ['heatingFlow', 'Heating flow · CV aanvoer', '#F2711C'], ['heatingReturn', 'Heating return · CV retour', '#1B9AAA'], ['drain', 'Drain · afvoer', '#8A8F98'], ['rainwater', 'Rainwater', '#3BA55D'], ['gas', 'Gas', '#E8C62C']];
interface Mat { id: string; label: string; dns: number[]; walls: number[]; joints: string[]; systems: string[]; bendUpTo: number; minBend: number }
const MATS: Mat[] = [
  { id: 'alupex', label: 'Alupex (multilayer)', dns: [16, 20, 26, 32, 14, 40, 50, 63], walls: [2, 2, 3, 3, 2, 3.5, 4, 4.5], joints: ['press', 'clamp', 'pushfit'], systems: ['coldWater', 'hotWater', 'heatingFlow', 'heatingReturn', 'gas'], bendUpTo: 26, minBend: 5 },
  { id: 'pex', label: 'PEX (in sleeve)', dns: [16, 20, 25], walls: [2.2, 2.8, 3.5], joints: ['press', 'clamp', 'sleeve'], systems: ['coldWater', 'hotWater', 'heatingFlow', 'heatingReturn'], bendUpTo: 25, minBend: 6 },
  { id: 'copper', label: 'Copper', dns: [15, 12, 18, 22, 28, 10, 35, 42], walls: [1, 1, 1, 1, 1, 1, 1.5, 1.5], joints: ['solder', 'press', 'compression'], systems: ['coldWater', 'hotWater', 'heatingFlow', 'heatingReturn', 'gas'], bendUpTo: 0, minBend: 3.5 },
  { id: 'pvc', label: 'PVC (drain)', dns: [40, 32, 50, 75, 90, 110, 125, 160], walls: [3, 3, 3, 3, 3, 3.2, 3.2, 4], joints: ['glue', 'ring'], systems: ['drain', 'rainwater'], bendUpTo: 0, minBend: 0 },
  { id: 'pe', label: 'PE (HDPE drain)', dns: [50, 40, 56, 63, 75, 90, 110, 125, 160], walls: [3, 3, 3, 3, 3, 3.5, 4.2, 4.8, 6.2], joints: ['weld'], systems: ['drain', 'rainwater'], bendUpTo: 0, minBend: 0 },
  { id: 'steel', label: 'Steel (threaded)', dns: [21.3, 26.9, 33.7, 42.4], walls: [2.6, 2.6, 3.2, 3.2], joints: ['thread'], systems: ['coldWater', 'heatingFlow', 'heatingReturn', 'gas'], bendUpTo: 0, minBend: 0 },
];
const JOINTS: Record<string, string> = { press: 'Press · pers', clamp: 'Clamp · klem/schroef', pushfit: 'Push-fit · steek', sleeve: 'Sliding sleeve · schuifhuls', solder: 'Solder · soldeer', compression: 'Compression · knel', glue: 'Solvent weld · lijm', ring: 'Rubber ring · manchet', weld: 'Welded · las', thread: 'Threaded · draad' };
type Shape = 'Inline' | 'Elbow' | 'Tee' | 'End' | 'Mixer' | 'Manifold' | 'Trap' | 'WallPlate';
interface FKind { id: string; label: string; shape: Shape; systems: string[]; dn2?: boolean; tap?: boolean }
const FITTINGS: FKind[] = [
  { id: 'coupling', label: 'Straight coupling · rechte koppeling', shape: 'Inline', systems: [] }, { id: 'elbow90', label: 'Elbow 90° · knie', shape: 'Elbow', systems: [] }, { id: 'elbow45', label: 'Elbow 45° · bocht', shape: 'Elbow', systems: [] },
  { id: 'tee', label: 'Tee · T-stuk', shape: 'Tee', systems: [], dn2: true }, { id: 'reducer', label: 'Reducer · verloop', shape: 'Inline', systems: [], dn2: true }, { id: 'adapter', label: 'Threaded adapter · overgang draad', shape: 'Inline', systems: [] }, { id: 'cap', label: 'End cap · eindstop', shape: 'End', systems: [] },
  { id: 'wallPlate', label: 'Wall plate elbow ½" · muurplaat', shape: 'WallPlate', systems: ['coldWater', 'hotWater', 'heatingFlow', 'heatingReturn', 'gas'] },
  { id: 'stopValve', label: 'Stop valve · stopkraan', shape: 'Inline', systems: ['coldWater', 'hotWater', 'heatingFlow', 'heatingReturn'] }, { id: 'ballValve', label: 'Ball valve · kogelkraan', shape: 'Inline', systems: ['coldWater', 'hotWater', 'heatingFlow', 'heatingReturn', 'gas'] },
  { id: 'checkValve', label: 'Check valve · terugslagklep', shape: 'Inline', systems: ['coldWater', 'hotWater', 'heatingFlow', 'heatingReturn'] }, { id: 'pressureReducer', label: 'Pressure reducer · drukregelaar', shape: 'Inline', systems: ['coldWater'] }, { id: 'filter', label: 'Filter', shape: 'Inline', systems: ['coldWater', 'heatingFlow', 'heatingReturn'] }, { id: 'meter', label: 'Water meter', shape: 'Inline', systems: ['coldWater'] },
  { id: 'angleValve', label: 'Angle stop valve · hoekstopkraan', shape: 'Elbow', systems: ['coldWater', 'hotWater'], tap: true }, { id: 'radiatorValve', label: 'Radiator valve · radiatorkraan', shape: 'Elbow', systems: ['heatingFlow', 'heatingReturn'] },
  { id: 'mixerWall', label: 'Wall mixer tap · wandmengkraan', shape: 'Mixer', systems: ['coldWater', 'hotWater'], tap: true }, { id: 'mixerThermo', label: 'Thermostatic shower mixer · thermostaatkraan', shape: 'Mixer', systems: ['coldWater', 'hotWater'], tap: true }, { id: 'mixerBasin', label: 'Basin mixer · wastafelmengkraan', shape: 'Mixer', systems: ['coldWater', 'hotWater'], tap: true },
  { id: 'manifold', label: 'Manifold · verdeler', shape: 'Manifold', systems: ['coldWater', 'hotWater', 'heatingFlow', 'heatingReturn'], dn2: true }, { id: 'trap', label: 'Bottle trap · sifon', shape: 'Trap', systems: ['drain'], tap: true }, { id: 'cleanout', label: 'Access piece · ontstoppingsstuk', shape: 'Inline', systems: ['drain', 'rainwater'] }, { id: 'branch45', label: 'Branch 45° · T-stuk 45°', shape: 'Tee', systems: ['drain', 'rainwater'], dn2: true },
];
const BRACKETS: [string, string][] = [['clip', 'Pipe clip · buisclip'], ['clamp', 'Clamp + rod · beugel met stokschroef'], ['double', 'Double clip']];
const FIXTURE_KINDS: Record<string, string> = { tapCold: 'Cold tap', tapHot: 'Hot tap', mixer: 'Mixer', showerValve: 'Shower valve', toiletSupply: 'Toilet supply', floorDrain: 'Floor drain', drainPoint: 'Drain point', radiator: 'Radiator', boiler: 'Boiler', manifold: 'Manifold' };
const mat = (id: string) => MATS.find(m => m.id === id) ?? MATS[0];
const sysColor = (id: string) => SYSTEMS.find(s => s[0] === id)?.[2] ?? '#888';
const isDrain = (s: string) => s === 'drain' || s === 'rainwater';
const dnLabel = (m: Mat, dn: number) => { const i = m.dns.indexOf(dn); return m.id === 'steel' ? (dn < 22 ? '½"' : dn < 28 ? '¾"' : dn < 35 ? '1"' : '1¼"') : i >= 0 ? `${dn}×${m.walls[i]}` : `${dn}`; };
const defaultCorners = (m: Mat, dn: number) => (dn <= m.bendUpTo ? 'bend' : 'elbow');

// ---- entity types ---------------------------------------------------------------------------------
interface Pipe { level: number; system: string; material: string; dn: number; points: Vec3[]; corners: 'bend' | 'elbow'; joint: string; bendRadius?: number; insulation: number; from?: string | null; to?: string | null; name?: string; note?: string }
interface Fitting { level: number; kind: string; joint: string; material: string; system: string; dn: number; dn2?: number; position: Vec3; dir: Vec3; dir2?: Vec3; angle?: number; thread?: string; outlets?: number; fixture?: string; note?: string }
interface Bracket { level: number; kind: string; position: Vec3; dir: Vec3; normal: Vec3; dn: number; standoff: number; pipe?: string; auto?: boolean }
interface Finding { severity: 'error' | 'warning' | 'info' | 'ok'; title: string; detail: string; ids: string[] }

const state = { system: 'coldWater', material: 'alupex', dn: 16, corners: 'bend' as 'bend' | 'elbow', joint: 'press', height: 0.5, insulation: 0, autoFittings: true, autoBrackets: false, bracketKind: 'clip', surfaces: true, fitting: 'coupling', outlets: 4, dn2: 16, findings: null as Finding[] | null, findingsVersion: -1, touchedMaterial: false, grid: 0.05, via: 'direct' as 'direct' | 'ceiling' | 'floor', pair: false, pairDistance: 0.06, variant: 0 };
let host: PluginHost;
const esc = (s: unknown) => String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]!));
const pop = (type: string, data: unknown): Op => ({ op: 'custom', type: `plumbing.${type}`, data });
const cext = (doc: Document): any => doc.extensions?.construction ?? { storeys: [{ height: 2.6 }], groundLevel: 0 };
const base = (doc: Document, level: number) => (cext(doc).groundLevel ?? 0) + (cext(doc).storeys ?? []).slice(0, level).reduce((s: number, f: any) => s + f.height, 0);
const activeLevel = () => { const fb = document.querySelector<HTMLElement>('.floor-bar button.active[data-lvl]'); return fb ? parseInt(fb.dataset.lvl!) : 0; };
const customs = <T,>(doc: Document, type: string) => doc.entities.filter((e): e is CustomEntity => e.kind === 'custom' && e.type === type).map(e => ({ id: e.id, d: e.data as T, e }));
const pipes = (doc: Document) => customs<Pipe>(doc, 'plumbing.pipe');
const fittings = (doc: Document) => customs<Fitting>(doc, 'plumbing.fitting');
const brackets = (doc: Document) => customs<Bracket>(doc, 'plumbing.bracket');
const plumbingFixtures = (doc: Document) => customs<any>(doc, 'construction.fixture').filter(f => FIXTURE_KINDS[f.d.kind]);
const v3 = (a: Vec3) => new THREE.Vector3(a[0], a[1], a[2]);
const arr = (p: V3): Vec3 => [+p.x.toFixed(4), +p.y.toFixed(4), +p.z.toFixed(4)];
const fmtM = (n: number) => `${n.toFixed(2)} m`;
const INK = 0x1b1b1b;
const sysDefaults = () => { if (!state.touchedMaterial || !mat(state.material).systems.includes(state.system)) { state.material = isDrain(state.system) ? 'pvc' : 'alupex'; const m = mat(state.material); state.dn = m.dns[0]; state.joint = m.joints[0]; state.corners = defaultCorners(m, state.dn); } };

// ---- ports (mirrors fittings.rs) --------------------------------------------------------------------
interface Port { pos: V3; dir: V3; dn: number; name: string }
const socketLen = (joint: string, d: number) => joint === 'glue' || joint === 'ring' ? 0.5 * d + 0.025 : joint === 'weld' ? 0.8 * d + 0.01 : joint === 'solder' ? Math.max(d, 0.012) : ['clamp', 'compression', 'thread'].includes(joint) ? Math.max(0.9 * d, 0.015) : Math.max(1.2 * d, 0.018);
const halfLen = (joint: string, d: number) => socketLen(joint, d) + 0.25 * d;
function fittingAxes(f: Fitting) {
  const d = v3(f.dir).normalize(); if (d.length() < 0.5) d.set(1, 0, 0);
  const want = f.dir2 ? v3(f.dir2).normalize() : (Math.abs(d.y) < 0.9 ? new THREE.Vector3(0, 1, 0) : new THREE.Vector3(1, 0, 0));
  const d2 = want.clone().sub(d.clone().multiplyScalar(want.dot(d))); if (d2.length() < 1e-6) d2.copy(Math.abs(d.y) < 0.9 ? new THREE.Vector3(0, 1, 0) : new THREE.Vector3(1, 0, 0)); d2.normalize();
  return { d, d2 };
}
function ports(f: Fitting): Port[] {
  const c = v3(f.position); const { d, d2 } = fittingAxes(f); const dd = f.dn / 1000; const d2d = (f.dn2 ?? f.dn) / 1000;
  const l = halfLen(f.joint, dd), l2 = halfLen(f.joint, d2d); const k = FITTINGS.find(x => x.id === f.kind) ?? FITTINGS[0];
  const at = (off: V3) => c.clone().add(off); const neg = (v: V3) => v.clone().negate();
  const angle = f.angle ?? (f.kind === 'elbow45' || f.kind === 'branch45' ? 45 : 90);
  const out = d.clone().multiplyScalar(Math.cos(angle * Math.PI / 180)).add(d2.clone().multiplyScalar(Math.sin(angle * Math.PI / 180))).normalize();
  switch (k.shape) {
    case 'Inline': return [{ pos: at(d.clone().multiplyScalar(-l)), dir: neg(d), dn: f.dn, name: 'in' }, { pos: at(d.clone().multiplyScalar(f.kind === 'reducer' ? l2 : l)), dir: d.clone(), dn: f.kind === 'reducer' ? (f.dn2 ?? f.dn) : f.dn, name: 'out' }];
    case 'Elbow': return [{ pos: at(d.clone().multiplyScalar(-l)), dir: neg(d), dn: f.dn, name: 'in' }, { pos: at(out.clone().multiplyScalar(f.kind === 'radiatorValve' ? l * 1.3 : l)), dir: out, dn: f.kind === 'angleValve' ? 10 : f.dn, name: 'out' }];
    case 'Tee': { const b = f.kind === 'branch45' ? out : d2; return [{ pos: at(d.clone().multiplyScalar(-l)), dir: neg(d), dn: f.dn, name: 'in' }, { pos: at(d.clone().multiplyScalar(l)), dir: d.clone(), dn: f.dn, name: 'out' }, { pos: at(b.clone().multiplyScalar(l2)), dir: b.clone(), dn: f.dn2 ?? f.dn, name: 'branch' }]; }
    case 'End': return [{ pos: at(d.clone().multiplyScalar(-l)), dir: neg(d), dn: f.dn, name: 'in' }];
    case 'WallPlate': return [{ pos: at(d.clone().multiplyScalar(-l)), dir: neg(d), dn: f.dn, name: 'in' }];
    case 'Mixer': return f.kind === 'mixerBasin' ? [-0.012, 0.012].map((s, i) => ({ pos: at(d.clone().multiplyScalar(s)).add(new THREE.Vector3(0, -0.3, 0)), dir: new THREE.Vector3(0, -1, 0), dn: 10, name: i ? 'cold' : 'hot' })) : [-0.075, 0.075].map((s, i) => ({ pos: at(d.clone().multiplyScalar(s)), dir: neg(d2), dn: f.dn, name: i ? 'cold' : 'hot' }));
    case 'Manifold': { const n = Math.max(1, f.outlets ?? 4); const lm = 0.05 * n + 0.04; const v: Port[] = [{ pos: at(d.clone().multiplyScalar(-lm / 2)), dir: neg(d), dn: f.dn, name: 'in' }]; for (let i = 0; i < n; i++) v.push({ pos: at(d.clone().multiplyScalar(-lm / 2 + 0.03 + 0.05 * i)).add(d2.clone().multiplyScalar(0.05)), dir: d2.clone(), dn: f.dn2 ?? 16, name: 'outlet' }); return v; }
    case 'Trap': return [{ pos: c.clone(), dir: new THREE.Vector3(0, 1, 0), dn: f.dn, name: 'in' }, { pos: at(new THREE.Vector3(0, -0.1, 0)).add(d.clone().multiplyScalar(0.12)), dir: d.clone(), dn: f.dn, name: 'out' }];
  }
}

// ---- snap targets: open pipe ends, open fitting ports, plumbing fixtures ---------------------------
interface Target { id: string; kind: 'end' | 'port' | 'fixture'; pos: V3; label: string; level: number; dn?: number; system?: string; dir?: V3 }
function targets(doc: Document): Target[] {
  const t: Target[] = [];
  const ends: { pipe: string; pos: V3 }[] = [];
  for (const p of pipes(doc)) { if (p.d.points.length < 2) continue; const y0 = base(doc, p.d.level); for (const [i, q] of [[0, p.d.points[0]], [1, p.d.points[p.d.points.length - 1]]] as [number, Vec3][]) { const pos = v3(q).add(new THREE.Vector3(0, y0, 0)); ends.push({ pipe: p.id, pos }); t.push({ id: p.id, kind: 'end', pos, label: `${i ? 'End' : 'Start'} of ${mat(p.d.material).label} ${p.d.dn} · ${p.d.system}`, level: p.d.level, dn: p.d.dn, system: p.d.system }); } }
  for (const f of fittings(doc)) { const y0 = base(doc, f.d.level); for (const q of ports(f.d)) { const pos = q.pos.add(new THREE.Vector3(0, y0, 0)); const used = ends.some(e => e.pos.distanceTo(pos) < 0.016); if (!used) t.push({ id: f.id, kind: 'port', pos, label: `${FITTINGS.find(k => k.id === f.d.kind)?.label.split(' ·')[0] ?? f.d.kind} · ${q.name} ${q.dn} mm`, level: f.d.level, dn: q.dn, system: f.d.system, dir: q.dir }); } }
  for (const f of plumbingFixtures(doc)) { const pos = v3(f.d.position).add(new THREE.Vector3(0, base(doc, f.d.level), 0)); t.push({ id: f.id, kind: 'fixture', pos, label: FIXTURE_KINDS[f.d.kind], level: f.d.level }); }
  // pipe ends already on a port are not offered twice
  return t.filter(x => x.kind !== 'end' || !t.some(y => y.kind === 'port' && y.pos.distanceTo(x.pos) < 0.016));
}
let targetCache: { version: number; list: Target[] } | null = null;
function nearTarget(ev: { clientX: number; clientY: number }, ctx: ToolContext, px = 14): Target | null {
  const doc = ctx.doc(); if (!targetCache || targetCache.version !== doc.version) targetCache = { version: doc.version, list: targets(doc) };
  const r = host.canvas().getBoundingClientRect(); const cx = ev.clientX - r.left, cy = ev.clientY - r.top;
  let best: Target | null = null, bd = px;
  for (const t of targetCache.list) { const s = ctx.scene.screenPoint(t.pos); const d = Math.hypot(s.x - cx, s.y - cy); if (d < bd) { bd = d; best = t; } }
  return best;
}
/** Show what a click would build (the engine applies the ops without committing); only when it changed. */
let shown = '';
function showResult(ctx: ToolContext, ops: Op[]) { const k = JSON.stringify(ops); if (k !== shown) { shown = k; ctx.preview(ops); } }
async function commit(ctx: ToolContext, ops: Op[], label: string) { shown = ''; ctx.preview([]); return ctx.ops(ops, label); }
const isPipe = (ctx: ToolContext) => (id: string) => pipes(ctx.doc()).some(p => p.id === id);
/** The pipe entity under the cursor (its expanded faces resolve to the custom entity). `selected`: with pipes
 *  selected only those count (a tool that works on a pipe works on the selected one). */
function pickedPipe(ev: PointerEvent, ctx: ToolContext, selected = false) { const p = ctx.pickTarget(ev, { accept: isPipe(ctx), anywhere: !selected }); if (!p) return null; const id = ctx.resolve(p.id); const e = pipes(ctx.doc()).find(x => x.id === id); return e ? { pipe: e, point: p.point.clone(), normal: p.normal } : null; }
function pipeUnder(ev: PointerEvent, ctx: ToolContext, selected = false) {
  const h = pickedPipe(ev, ctx, selected); if (h) return h;
  // over the tool's own previewed result (a fitting splits the pipe there): find the pipe from the document
  const raw = ctx.scene.pick(ev); if (!raw || ctx.inDocument(raw.id)) return null;
  const doc = ctx.doc(); const r = host.canvas().getBoundingClientRect(); const c = new THREE.Vector2(ev.clientX - r.left, ev.clientY - r.top);
  const sel = new Set(selected ? ctx.targets(isPipe(ctx)) : []);
  let best: { pipe: { id: string; d: Pipe }; point: V3; d: number } | null = null;
  for (const p of pipes(doc)) {
    if (sel.size && !sel.has(p.id)) continue; const w = pipesWorld(doc, p.d);
    for (let i = 0; i + 1 < w.length; i++) {
      const a = ctx.scene.screenPoint(w[i]), b = ctx.scene.screenPoint(w[i + 1]); const ab = b.clone().sub(a); const L2 = ab.lengthSq(); if (L2 < 1) continue;
      const t = Math.max(0, Math.min(1, c.clone().sub(a).dot(ab) / L2)); const d = a.clone().addScaledVector(ab, t).distanceTo(c);
      if (d < 12 && (!best || d < best.d)) best = { pipe: p, point: w[i].clone().lerp(w[i + 1], t), d };
    }
  }
  return best ? { pipe: best.pipe, point: best.point, normal: raw.normal } : null;
}

// ---- Pipe tool ------------------------------------------------------------------------------------------
const AX = { x: new THREE.Vector3(1, 0, 0), y: new THREE.Vector3(0, 1, 0), z: new THREE.Vector3(0, 0, 1) };
const HEIGHTS: [string, number][] = [['Plint', 0.15], ['Spoelbak', 0.55], ['Wastafel', 1.05], ['Douche', 1.2], ['Plafond', 2.5]];
const SWAP: Record<string, string> = { coldWater: 'hotWater', hotWater: 'coldWater', heatingFlow: 'heatingReturn', heatingReturn: 'heatingFlow' };
/** The pipe that goes with this one: hot beside cold, return beside flow. */
const pairOp = () => state.pair && SWAP[state.system] ? { system: SWAP[state.system], distance: state.pairDistance } : undefined;
const standoff = () => state.dn / 2000 + state.insulation / 1000 + 0.025;
const isPlumbing = (ctx: ToolContext, id: string) => { const e = ctx.doc().entities.find(x => x.id === ctx.resolve(id)); return e?.kind === 'custom' && e.type.startsWith('plumbing.'); };
interface Wall { origin: V3; normal: V3 }
/** The wall under the cursor, as the plane a pipe clipped to it runs in. */
function wallAt(ev: { clientX: number; clientY: number }, ctx: ToolContext): (Wall & { hit: V3 }) | null {
  if (!state.surfaces) return null;
  const pk = ctx.scene.pick(ev, { facesOnly: true }); if (!pk || Math.abs(pk.normal.y) > 0.2 || isPlumbing(ctx, pk.id)) return null;
  const n = pk.normal.clone().setY(0).normalize();
  if (ctx.scene.cam.camera.position.clone().sub(pk.point).dot(n) < 0) n.negate();
  return { origin: pk.point.clone().addScaledVector(n, standoff()), normal: n, hit: pk.point.clone() };
}
const sameWall = (a: Wall, b: Wall) => Math.abs(a.normal.dot(b.normal)) > 0.9 && Math.abs(b.origin.clone().sub(a.origin).dot(a.normal)) < 0.02;
const onWall = (p: V3, w: Wall) => p.clone().addScaledVector(w.normal, w.origin.clone().sub(p).dot(w.normal));
/** Where two walls meet, at height y. */
function wallCorner(a: Wall, b: Wall, y: number): V3 | null {
  const det = a.normal.x * b.normal.z - a.normal.z * b.normal.x; if (Math.abs(det) < 0.1) return null;
  const d1 = a.origin.dot(a.normal), d2 = b.origin.dot(b.normal); const hy = a.normal.y * y;
  return new THREE.Vector3((d1 - hy) * b.normal.z / det - (d2 - hy) * a.normal.z / det, y, (d2 - hy) * a.normal.x / det - (d1 - hy) * b.normal.x / det);
}
/** Keep a length on the grid, or on a coordinate something else already has (so runs line up without typing). */
function settle(ctx: ToolContext, from: V3, dir: V3, len: number, refs: V3[]): { len: number; ref: V3 | null } {
  const tol = 9 * ctx.scene.pixelSize(from.clone().addScaledVector(dir, len));
  let best: V3 | null = null, bd = tol;
  for (const r of refs) { const L = r.clone().sub(from).dot(dir); const d = Math.abs(L - len); if (d < bd) { bd = d; best = r; len = L; } }
  if (best) return { len, ref: best };
  return { len: state.grid > 0 ? Math.round(len / state.grid) * state.grid : len, ref: null };
}
interface Aim { point: V3; target: Target | null; onFace: boolean; corners: V3[]; wall: Wall | null; ref: V3 | null }
interface PipeToolState { pts: V3[]; level: number; from: string | null; cur: V3 | null; lastDir: V3 | null; wall: Wall | null; snapPoint(ev: PointerEvent, ctx: ToolContext): Aim; draw(ctx: ToolContext, s: Aim): void; previewPipe(ctx: ToolContext): void; op(ctx: ToolContext, to: string | null, preview?: boolean): Op; finish(ctx: ToolContext, to: string | null): Promise<void> }
const pipeTool: Tool & PipeToolState = {
  id: 'pl-pipe', label: 'Pipe', hint: 'Click a wall and follow it: the pipe stays clipped to it, runs level or plumb by itself and goes round corners when you hover the next wall. Click a pipe end, port or tap to connect and finish. Enter finishes; typing a length is optional. Shift draws free of the grid and angles.', cursor: 'crosshair', shortcut: 'u',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M3 17h7a3 3 0 0 0 3-3V7h8" stroke="#2E7BE4" stroke-width="3.2"/><path d="M3 17h7a3 3 0 0 0 3-3V7h8" stroke="#fff" stroke-width="1" opacity=".6"/><rect x="2" y="15" width="3" height="4" rx=".6" fill="#C9A227"/><rect x="19" y="5" width="3" height="4" rx=".6" fill="#C9A227"/></svg>`,
  pts: [], level: 0, from: null, cur: null, lastDir: null, wall: null,
  busy() { return this.pts.length > 0; },
  activate(ctx) { this.pts = []; this.from = null; this.wall = null; ctx.setHint(this.hint); ctx.setVCB(String(state.height), 'Height'); }, deactivate(ctx) { this.pts = []; this.from = null; this.wall = null; ctx.scene.clearOverlay(); ctx.preview([]); },
  onMove(ev, ctx) { const s = this.snapPoint(ev, ctx); this.cur = s.point; this.draw(ctx, s); },
  snapPoint(ev, ctx) {
    const doc = ctx.doc(); const t = nearTarget(ev, ctx); const last = this.pts.at(-1); const none = { target: null, onFace: false, corners: [] as V3[], wall: this.wall, ref: null };
    if (t) {
      // reach it with square corners, along the wall first
      const corners: V3[] = [];
      if (last) {
        const q = last.clone(); const alongFirst = this.wall && Math.abs(this.wall.normal.x) > 0.7 ? ['z', 'x'] : ['x', 'z'];
        for (const a of alongFirst as ('x' | 'z')[]) if (Math.abs(t.pos[a] - q[a]) > 0.005 && (Math.abs(t.pos.y - q.y) > 0.005 || a === alongFirst[0] && Math.abs(t.pos[alongFirst[1] as 'x' | 'z'] - q[alongFirst[1] as 'x' | 'z']) > 0.005)) { q[a] = t.pos[a]; corners.push(q.clone()); }
      }
      return { ...none, point: t.pos.clone(), target: t, corners };
    }
    const w = wallAt(ev, ctx);
    if (!last) {
      if (w) { const y = base(doc, activeLevel()); const h = Math.round((w.hit.y - y) / state.grid) * state.grid; const p = w.origin.clone(); p.y = y + (state.grid > 0 ? h : w.hit.y - y); return { ...none, point: p, onFace: true, wall: w }; }
      const y = base(doc, activeLevel()) + state.height;
      return { ...none, point: ctx.snap(ev, { plane: { origin: new THREE.Vector3(0, y, 0), normal: AX.y }, faces: false }).point, wall: null };
    }
    // another wall: run on to where they meet (or straight across to it), then carry on in that wall
    let plane: Wall | null = this.wall; let start = last; const corners: V3[] = [];
    if (w && (!plane || !sameWall(plane, w)) && ev.shiftKey === false) {
      const c = plane ? wallCorner(plane, w, last.y) : null;
      const via = c ?? onWall(last, w);
      if (via.distanceTo(last) > 0.005) { corners.push(via); start = via; }
      plane = w;
    }
    const free = plane ? ctx.scene.planePoint(ev, plane.origin, plane.normal) : ctx.scene.planePoint(ev, new THREE.Vector3(0, last.y, 0), AX.y);
    if (!free) return { ...none, point: last.clone() };
    if (ev.shiftKey) return { ...none, point: free, wall: plane, onFace: !!plane };
    // square: the axis the cursor is furthest along (the arrow keys force one)
    const axes: V3[] = ctx.lock ? [AX[ctx.lock]] : plane ? [AX.y, new THREE.Vector3().crossVectors(plane.normal, AX.y).normalize()] : [AX.x, AX.z];
    const off = free.clone().sub(start); let dir = axes[0];
    for (const a of axes) if (Math.abs(off.dot(a)) > Math.abs(off.dot(dir))) dir = a;
    const refs = (targetCache?.list ?? targets(doc)).map(x => x.pos).concat(this.pts);
    const { len, ref } = settle(ctx, start, dir, off.dot(dir), refs);
    return { ...none, point: start.clone().addScaledVector(dir, len), corners, wall: plane, onFace: !!plane, ref };
  },
  draw(ctx, s) {
    ctx.scene.clearOverlay(); const col = parseInt(sysColor(state.system).slice(1), 16);
    ctx.scene.addDot(s.point, s.target ? 0x1e8e3e : s.onFace ? 0x1a73e8 : col, s.target ? 11 : 7);
    if (s.target) ctx.scene.addLabel(s.point, s.target.label);
    if (s.ref) ctx.scene.addLine([s.ref, s.point], 0xc8169a, true, 1);
    for (const p of this.pts) ctx.scene.addSquare(p, INK, 6);
    if (!this.pts.length) { if (s.onFace) ctx.scene.addLabel(s.point, `On wall · ${fmtM(s.point.y - base(ctx.doc(), activeLevel()))} high`); return; }
    const path = [...this.pts, ...s.corners, s.point];
    ctx.scene.addLine(path, col, false, 3);
    const a = path.at(-2)!; const L = a.distanceTo(s.point);
    ctx.scene.addLabel(a.clone().lerp(s.point, 0.5), fmtM(L)); ctx.setVCB(L.toFixed(3), 'Length');
    if (L > 1e-6) this.lastDir = s.point.clone().sub(a).normalize();
  },
  onDown(ev, ctx) {
    if (ev.button !== 0) return; const s = this.snapPoint(ev, ctx);
    if (!this.pts.length) { this.level = s.target?.level ?? activeLevel(); this.from = s.target?.id ?? null; this.wall = s.wall; if (s.target?.system && !isDrain(s.target.system) === !isDrain(state.system)) { state.system = s.target.system; sysDefaults(); host.refresh(); } this.pts.push(s.point.clone()); ctx.setHint('Move along the wall and click at each corner; click a pipe end, port or tap to finish there. Enter finishes.'); return; }
    if (s.point.distanceTo(this.pts.at(-1)!) < 0.005) return;
    this.pts.push(...s.corners.map(c => c.clone()), s.point.clone()); this.wall = s.wall;
    if (s.target) { void this.finish(ctx, s.target.id); return; }
    this.previewPipe(ctx);
  },
  previewPipe(ctx) { if (this.pts.length >= 2) ctx.preview([this.op(ctx, null, true)]); },
  op(ctx, to, preview = false) {
    const b = base(ctx.doc(), this.level);
    return pop('add_pipe', { level: this.level, system: state.system, material: state.material, dn: state.dn, corners: state.corners, joint: state.joint, insulation: state.insulation || undefined, points: this.pts.map(p => [+p.x.toFixed(4), +(p.y - b).toFixed(4), +p.z.toFixed(4)]), from: this.from, to, autoFittings: state.autoFittings, autoBrackets: !preview && state.autoBrackets, bracketKind: state.bracketKind });
  },
  async finish(ctx, to) {
    if (this.pts.length >= 2) {
      ctx.preview([]); const r = await ctx.ops([this.op(ctx, to)], 'Pipe'); const id = r[0]?.created?.[0];
      const pair = pairOp(); if (id && pair) await ctx.ops([pop('parallel', { id, ...pair })], 'Pipe alongside');
      if (id) ctx.select([id]);
    }
    this.pts = []; this.from = null; this.wall = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint); ctx.setVCB(String(state.height), 'Height');
  },
  onDblClick(_e, ctx) { if (this.pts.length > 2) this.pts.pop(); void this.finish(ctx, null); },
  onKey(ev, ctx) {
    if (ev.key === 'Enter') { void this.finish(ctx, null); return true; }
    if (ev.key === 'Escape') { this.pts = []; this.from = null; this.wall = null; ctx.scene.clearOverlay(); ctx.preview([]); ctx.setVCB(String(state.height), 'Height'); return true; }
    if (ev.key === 'Backspace' && this.pts.length) { this.pts.pop(); this.previewPipe(ctx); return true; }
    return false;
  },
  onVCB(v, ctx) {
    const n = parseFloat(v.replace(',', '.')); if (isNaN(n)) return;
    if (!this.pts.length) { state.height = n; host.refresh(); return; }
    const last = this.pts.at(-1)!; if (!this.lastDir) return;
    this.pts.push(last.clone().addScaledVector(this.lastDir, n)); this.previewPipe(ctx); this.draw(ctx, { point: this.pts.at(-1)!, target: null, onFace: false, corners: [], wall: this.wall, ref: null });
  },
};

// ---- Route tool: two clicks, the engine works out the corners ------------------------------------------
interface RouteEnd { point: V3; id: string | null; level: number; label: string }
function routeEnd(ev: PointerEvent, ctx: ToolContext): RouteEnd {
  const doc = ctx.doc(); const lvl = activeLevel(); const t = nearTarget(ev, ctx);
  if (t) return { point: t.pos.clone(), id: t.id, level: t.level, label: t.label };
  const h = pipeUnder(ev, ctx);
  if (h) return { point: h.point.clone(), id: h.pipe.id, level: h.pipe.d.level, label: `Tee on ${mat(h.pipe.d.material).label} ${h.pipe.d.dn}` };
  const w = wallAt(ev, ctx);
  if (w) { const p = w.origin.clone(); const y = base(doc, lvl); p.y = y + Math.round((w.hit.y - y) / state.grid) * state.grid; return { point: p, id: null, level: lvl, label: `On wall · ${fmtM(p.y - y)} high` }; }
  const y = base(doc, lvl) + state.height;
  return { point: ctx.snap(ev, { plane: { origin: new THREE.Vector3(0, y, 0), normal: AX.y }, faces: false }).point, id: null, level: lvl, label: '' };
}
const routeTool: Tool & { a: RouteEnd | null; b: RouteEnd | null; op(ctx: ToolContext, a: RouteEnd, b: RouteEnd, preview: boolean): Op; commit(ctx: ToolContext, b: RouteEnd): Promise<void> } = {
  id: 'pl-route', label: 'Route', hint: 'Click where the pipe starts, then where it ends: a pipe end, port, tap, wall or another pipe (a tee goes in). The corners are worked out for you. Tab tries another way; pick the travel height in the panel.', cursor: 'crosshair', shortcut: 'y',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M5 19V9h14V5" stroke="#2E7BE4" stroke-width="2.6" stroke-dasharray="3 2.4"/><circle cx="5" cy="19" r="2.6" fill="#1e8e3e" stroke="none"/><circle cx="19" cy="5" r="2.6" fill="#1e8e3e" stroke="none"/></svg>`,
  a: null, b: null,
  busy() { return !!this.a; },
  activate(ctx) { this.a = null; this.b = null; state.variant = 0; ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }, deactivate(ctx) { this.a = null; ctx.scene.clearOverlay(); ctx.preview([]); },
  op(ctx, a, b, preview) {
    const doc = ctx.doc(); const y0 = base(doc, a.level); const rel = (p: V3) => [+p.x.toFixed(4), +(p.y - y0).toFixed(4), +p.z.toFixed(4)];
    return pop('route', { level: a.level, from: rel(a.point), fromId: a.id ?? undefined, to: rel(b.point), toId: b.id ?? undefined, via: state.via, variant: state.variant, system: state.system, material: state.material, dn: state.dn, corners: state.corners, joint: state.joint, insulation: state.insulation || undefined, autoFittings: state.autoFittings, autoBrackets: !preview && state.autoBrackets, bracketKind: state.bracketKind, pair: pairOp() });
  },
  onMove(ev, ctx) {
    const e = routeEnd(ev, ctx); ctx.scene.clearOverlay(); ctx.scene.addDot(e.point, e.id ? 0x1e8e3e : 0x1a73e8, e.id ? 11 : 7); if (e.label) ctx.scene.addLabel(e.point, e.label);
    if (!this.a) return; ctx.scene.addDot(this.a.point, 0x1e8e3e, 9);
    if (!this.b || this.b.point.distanceTo(e.point) > 0.02) { this.b = e; ctx.preview([this.op(ctx, this.a, e, true)]); }
  },
  onDown(ev, ctx) {
    if (ev.button !== 0) return; const e = routeEnd(ev, ctx);
    if (!this.a) { this.a = e; this.b = null; state.variant = 0; ctx.setHint('Click where it ends. Tab tries another way, Esc cancels.'); return; }
    if (e.point.distanceTo(this.a.point) < 0.02) return;
    void this.commit(ctx, e);
  },
  async commit(ctx, b) {
    const a = this.a!; ctx.preview([]); const r = await ctx.ops([this.op(ctx, a, b, false)], 'Route pipe');
    this.a = null; this.b = null; state.variant = 0; ctx.scene.clearOverlay(); ctx.setHint(this.hint);
    const info = (r[0]?.entities?.at(-1) as any)?.data; const id = r[0]?.created?.[0]; if (id) ctx.select([id]);
    if (info) { const w = info.wallCrossings?.length ?? 0, f = info.floorCrossings?.length ?? 0; ctx.toast(`${fmtM(r[0].measurement ?? 0)}${w ? ` · ${w} wall hole${w > 1 ? 's' : ''}` : ''}${f ? ` · ${f} floor hole${f > 1 ? 's' : ''}` : ''}`); }
  },
  onKey(ev, ctx) {
    if (ev.key === 'Escape' && this.a) { this.a = null; this.b = null; ctx.scene.clearOverlay(); ctx.preview([]); ctx.setHint(this.hint); return true; }
    if (ev.key === 'Tab' && this.a && this.b) { state.variant++; ctx.preview([this.op(ctx, this.a, this.b, true)]); ev.preventDefault(); return true; }
    return false;
  },
};

// ---- Adjust tool: drag a leg sideways or a pipe end along itself ----------------------------------------
type Grab = { pipe: { id: string; d: Pipe }; kind: 'leg' | 'end'; index: number; start: V3; plane: Wall | null; axis: V3; moved: Vec3[] | null; amount: number };
const pipesWorld = (doc: Document, p: Pipe) => { const y = base(doc, p.level); return p.points.map(q => new THREE.Vector3(q[0], q[1] + y, q[2])); };
function grabAt(ev: PointerEvent, ctx: ToolContext): Grab | null {
  const doc = ctx.doc(); const r = host.canvas().getBoundingClientRect(); const cx = ev.clientX - r.left, cy = ev.clientY - r.top;
  const h = pipeUnder(ev, ctx, true); const sel = new Set(ctx.targets(isPipe(ctx)));
  // the free ends of any pipe (of the selected ones, when pipes are selected) first, then the leg under the cursor
  for (const p of pipes(doc).filter(p => !sel.size || sel.has(p.id))) { const w = pipesWorld(doc, p.d); for (const i of [0, w.length - 1]) { const s = ctx.scene.screenPoint(w[i]); if (Math.hypot(s.x - cx, s.y - cy) < 10) { const o = i === 0 ? w[1] : w[i - 1]; return { pipe: p, kind: 'end', index: i, start: w[i].clone(), plane: null, axis: w[i].clone().sub(o).normalize(), moved: null, amount: 0 }; } } }
  if (!h) return null;
  const w = pipesWorld(doc, h.pipe.d); let best = 0, bd = Infinity;
  for (let i = 0; i + 1 < w.length; i++) { const d = new THREE.Line3(w[i], w[i + 1]).closestPointToPoint(h.point, true, new THREE.Vector3()).distanceTo(h.point); if (d < bd) { bd = d; best = i; } }
  const dir = w[best + 1].clone().sub(w[best]).normalize();
  // drag in the plane that holds the leg and faces the camera most
  const view = ctx.scene.cam.camera.position.clone().sub(h.point).normalize();
  let normal = AX.x; for (const a of [AX.x, AX.y, AX.z]) if (Math.abs(a.dot(dir)) < 0.5 && Math.abs(a.dot(view)) > Math.abs(normal.dot(view)) || Math.abs(normal.dot(dir)) > 0.5) normal = a;
  return { pipe: h.pipe, kind: 'leg', index: best, start: h.point.clone(), plane: { origin: h.point.clone(), normal: normal.clone() }, axis: dir, moved: null, amount: 0 };
}
/** The pipe's points with a leg moved sideways. A neighbouring leg in line with the move just gets longer or
 *  shorter; any other keeps its corner and a new short leg joins it, so the run stays square and its ends stay connected. */
function movedLeg(p: Pipe, i: number, delta: V3): Vec3[] {
  const pts = p.points.map(q => [...q] as Vec3); const d = delta.clone().normalize();
  const moved = (q: Vec3): Vec3 => [+(q[0] + delta.x).toFixed(4), +(q[1] + delta.y).toFixed(4), +(q[2] + delta.z).toFixed(4)];
  const inLine = (a: Vec3, b: Vec3) => Math.abs(v3(b).sub(v3(a)).normalize().dot(d)) > 0.999;
  const keepStart = i === 0 || !inLine(pts[i - 1], pts[i]);
  const keepEnd = i + 2 === pts.length || !inLine(pts[i + 1], pts[i + 2]);
  return [...pts.slice(0, keepStart ? i + 1 : i), moved(pts[i]), moved(pts[i + 1]), ...pts.slice(keepEnd ? i + 1 : i + 2)];
}
const adjustTool: Tool & { grab: Grab | null; apply(ctx: ToolContext, g: Grab, amount: number, delta: V3): void } = {
  id: 'pl-adjust', label: 'Adjust pipe', hint: 'Drag a leg of a pipe sideways, or a pipe end to make it longer or shorter. It stays square and on the grid; lengths show as you drag. Type a distance while dragging for an exact one.', cursor: 'grab',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M3 8h6v8h12" stroke="#2E7BE4" stroke-width="2.6"/><path d="M15 4v6m-2-2 2 2 2-2" stroke="#1b1b1b"/></svg>`,
  grab: null,
  busy() { return !!this.grab; },
  activate(ctx) { this.grab = null; ctx.setHint(this.hint); ctx.setVCB('', 'Distance'); }, deactivate(ctx) { this.grab = null; ctx.scene.clearOverlay(); ctx.preview([]); ctx.scene.highlight(new Set(), true); },
  apply(ctx, g, amount, delta) {
    const y = g.pipe.d;
    if (g.kind === 'end') { const pts = y.points.map(q => [...q] as Vec3); const q = pts[g.index]; pts[g.index] = [+(q[0] + delta.x).toFixed(4), +(q[1] + delta.y).toFixed(4), +(q[2] + delta.z).toFixed(4)]; g.moved = pts; }
    else g.moved = movedLeg(y, g.index, delta);
    g.amount = amount; ctx.preview([pop('update_pipe', { id: g.pipe.id, patch: { points: g.moved } })]);
  },
  onDown(ev, ctx) { if (ev.button !== 0) return; this.grab = grabAt(ev, ctx); if (this.grab) ctx.select([this.grab.pipe.id]); },
  onMove(ev, ctx) {
    const g = this.grab; ctx.scene.clearOverlay();
    if (!g) { const o = grabAt(ev, ctx); ctx.scene.highlight(o ? new Set([o.pipe.id]) : new Set(), true); if (o) { const w = pipesWorld(ctx.doc(), o.pipe.d); if (o.kind === 'end') { ctx.scene.addDot(w[o.index], 0x1e8e3e, 10); ctx.scene.addLabel(w[o.index], 'Drag: longer or shorter'); } else { ctx.scene.addLine([w[o.index], w[o.index + 1]], 0xc8169a, false, 3); ctx.scene.addLabel(w[o.index].clone().lerp(w[o.index + 1], 0.5), `${fmtM(w[o.index].distanceTo(w[o.index + 1]))} · drag sideways`); } } return; }
    let delta: V3; let amount: number;
    if (g.kind === 'end') { const p = ctx.scene.linePoint(ev, g.start, g.axis); const s = settle(ctx, g.start, g.axis, p.clone().sub(g.start).dot(g.axis), targets(ctx.doc()).map(t => t.pos)); amount = s.len; delta = g.axis.clone().multiplyScalar(amount); }
    else {
      const p = ctx.scene.planePoint(ev, g.plane!.origin, g.plane!.normal); if (!p) return;
      const side = new THREE.Vector3().crossVectors(g.plane!.normal, g.axis).normalize();
      const s = settle(ctx, g.start, side, p.clone().sub(g.start).dot(side), targets(ctx.doc()).map(t => t.pos)); amount = s.len; delta = side.multiplyScalar(amount);
    }
    ctx.scene.addLabel(g.start.clone().add(delta), fmtM(Math.abs(amount))); ctx.setVCB(Math.abs(amount).toFixed(3), 'Distance');
    this.apply(ctx, g, amount, delta);
  },
  async onUp(_ev, ctx) { const g = this.grab; this.grab = null; ctx.scene.clearOverlay(); ctx.preview([]); if (g?.moved && Math.abs(g.amount) > 1e-4) await ctx.ops([pop('update_pipe', { id: g.pipe.id, patch: { points: g.moved } })], 'Adjust pipe'); },
  onKey(ev, ctx) { if (ev.key === 'Escape' && this.grab) { this.grab = null; ctx.scene.clearOverlay(); ctx.preview([]); return true; } return false; },
};

// ---- Fitting tool -----------------------------------------------------------------------------------------
const fittingTool: Tool = {
  id: 'pl-fitting', label: 'Fitting', hint: 'Pick a fitting in the Plumbing panel, then click a pipe (a valve or coupling cuts in, a tee branches towards the click, a cap goes on the end), a tap (wall plate / mixer) or a free point. ← → turn the branch.', cursor: 'crosshair', shortcut: 'j',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M3 12h18" stroke="#2E7BE4" stroke-width="3"/><path d="M12 12v8" stroke="#2E7BE4" stroke-width="3"/><rect x="7" y="8.5" width="10" height="7" rx="1.2" fill="#C9A227" stroke="#8a6d12"/><rect x="9" y="14" width="6" height="6" rx="1" fill="#C9A227" stroke="#8a6d12"/></svg>`,
  activate(ctx) { ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }, deactivate(ctx) { shown = ''; ctx.preview([]); ctx.scene.clearOverlay(); ctx.scene.highlight(new Set(), true); },
  onMove(ev, ctx) {
    ctx.scene.clearOverlay(); const k = FITTINGS.find(f => f.id === state.fitting)!;
    const f = fittingAt(ev, ctx); const { t, h } = f;
    ctx.scene.highlight(h ? new Set([h.pipe.id]) : new Set(), true);
    if (t) { ctx.scene.addDot(t.pos, 0x1e8e3e, 11); ctx.scene.addLabel(t.pos, `${k.label.split(' ·')[0]} → ${t.label}`); }
    else if (h) { ctx.scene.addDot(h.point, 0xc9a227, 9); ctx.scene.addLabel(h.point, `${k.label.split(' ·')[0]} on ${mat(h.pipe.d.material).label} ${h.pipe.d.dn}`); }
    else if (f.point) ctx.scene.addDot(f.point, 0x888888, 7);
    showResult(ctx, f.op ? [f.op] : []);
  },
  async onDown(ev, ctx) {
    if (ev.button !== 0) return; const f = fittingAt(ev, ctx);
    if (!f.op) { if (f.error) ctx.toast(f.error); return; }
    const r = await commit(ctx, [f.op], 'Fitting'); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]);
  },
};
/** What the Fitting tool would place under the cursor: the target or pipe it goes on, and the op. */
function fittingAt(ev: PointerEvent, ctx: ToolContext): { t: ReturnType<typeof nearTarget>; h: ReturnType<typeof pipeUnder>; op: Op | null; point?: V3; error?: string } {
  const doc = ctx.doc(); const k = FITTINGS.find(f => f.id === state.fitting)!;
  const t = nearTarget(ev, ctx); const h = t ? null : pipeUnder(ev, ctx);
  const common: any = { kind: state.fitting, outlets: k.id === 'manifold' ? state.outlets : undefined, dn2: k.dn2 ? state.dn2 : undefined };
  if (t && t.kind === 'end') { const p = pipes(doc).find(x => x.id === t.id)!; return { t, h, op: pop('add_fitting', { ...common, onPipe: t.id, position: arr(t.pos.clone().sub(new THREE.Vector3(0, base(doc, p.d.level), 0))) }) }; }
  if (t && t.kind === 'fixture') { const f = plumbingFixtures(doc).find(x => x.id === t.id)!; return { t, h, op: pop('add_fitting', { ...common, fixture: t.id, level: f.d.level, position: f.d.position, system: state.system, material: state.material, joint: state.joint, dn: state.dn }) }; }
  if (t && t.kind === 'port') return { t, h, op: null, error: 'That port is on a fitting already — draw a pipe from it first.' };
  if (h) return { t, h, op: pop('add_fitting', { ...common, onPipe: h.pipe.id, position: arr(h.point.clone().sub(new THREE.Vector3(0, base(doc, h.pipe.d.level), 0))), dir2: ctx.lock ? [ctx.lock === 'x' ? 1 : 0, ctx.lock === 'y' ? 1 : 0, ctx.lock === 'z' ? 1 : 0] : undefined }) };
  // a free point: on the work height, or against the face under the cursor (the previewed fitting itself is see-through)
  const lvl = activeLevel(); const s = ctx.snap(ev, { plane: { origin: new THREE.Vector3(0, base(doc, lvl) + state.height, 0), normal: new THREE.Vector3(0, 1, 0) }, faces: state.surfaces, lock: ctx.lock, ignoreIf: id => !ctx.inDocument(id) });
  let pos = s.point.clone(); let dir2: Vec3 | undefined;
  if (s.kind === 'face') { const pk = ctx.pickTarget(ev, { facesOnly: true, anywhere: true }); if (pk) { dir2 = arr(pk.normal); if (k.shape !== 'Mixer' && k.shape !== 'WallPlate') pos = pk.point.clone().addScaledVector(pk.normal, state.dn / 2000 + 0.025); } }
  const dir: Vec3 = ctx.lock ? [ctx.lock === 'x' ? 1 : 0, ctx.lock === 'y' ? 1 : 0, ctx.lock === 'z' ? 1 : 0] : dir2 && Math.abs(dir2[1]) < 0.9 ? [-dir2[2], 0, dir2[0]] : [1, 0, 0];
  return { t, h, point: s.point.clone(), op: pop('add_fitting', { ...common, level: lvl, position: arr(pos.sub(new THREE.Vector3(0, base(doc, lvl), 0))), dir, dir2, system: state.system, material: state.material, joint: state.joint, dn: state.dn }) };
}

// ---- Bracket tool ----------------------------------------------------------------------------------------
const bracketTool: Tool = {
  id: 'pl-bracket', label: 'Bracket', hint: 'Click a pipe where it is clipped to the wall, ceiling or floor (the nearest surface is chosen). "Auto brackets" in the panel does a whole pipe.', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M2 12h20" stroke="#2E7BE4" stroke-width="3"/><path d="M8 12a4 4 0 0 1 8 0v2H8z" fill="#9AA0A6" stroke="#5f6368"/><path d="M12 14v5" stroke="#5f6368" stroke-width="2"/><path d="M8 20h8" stroke="#5f6368" stroke-width="2"/></svg>`,
  activate(ctx) { ctx.setHint(this.hint); }, deactivate(ctx) { shown = ''; ctx.preview([]); ctx.scene.clearOverlay(); ctx.scene.highlight(new Set(), true); },
  onMove(ev, ctx) { ctx.scene.clearOverlay(); const h = pipeUnder(ev, ctx, true); ctx.scene.highlight(h ? new Set([h.pipe.id]) : new Set(), true); if (h) { ctx.scene.addDot(h.point, 0x9aa0a6, 9); ctx.scene.addLabel(h.point, `${BRACKETS.find(b => b[0] === state.bracketKind)?.[1].split(' ·')[0]} Ø ${h.pipe.d.dn}`); } showResult(ctx, h ? [bracketOp(ctx, h)] : []); },
  async onDown(ev, ctx) { if (ev.button !== 0) return; const h = pipeUnder(ev, ctx, true); if (!h) { ctx.toast(ctx.targets(isPipe(ctx)).length ? 'Click on a selected pipe (Esc clears the selection)' : 'Click on a pipe'); return; } const r = await commit(ctx, [bracketOp(ctx, h)], 'Bracket'); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]); },
};

/** A bracket at the point on the pipe, rounded to the centimetre so a still cursor doesn't re-preview. */
function bracketOp(ctx: ToolContext, h: NonNullable<ReturnType<typeof pipeUnder>>): Op {
  const p = h.point.clone().sub(new THREE.Vector3(0, base(ctx.doc(), h.pipe.d.level), 0));
  return pop('add_bracket', { pipe: h.pipe.id, position: arr(p).map(x => Math.round(x * 100) / 100) as Vec3, kind: state.bracketKind });
}

// ---- X-ray ----------------------------------------------------------------------------------------------
const xrayIds = (doc: Document) => doc.materials.filter(m => m.id.startsWith('plumbing.')).map(m => m.id);
const xrayOn = (): boolean => { try { return localStorage.getItem('plannr.plumbing.xray') === '1'; } catch { return false; } };
function setXray(ctx: ToolContext, on: boolean) { const mine = xrayIds(ctx.doc()); const others = ctx.scene.xrayMaterials.filter(m => !mine.includes(m)); ctx.scene.setXray(on ? [...others, ...mine] : others); try { localStorage.setItem('plannr.plumbing.xray', on ? '1' : '0'); } catch { /* storage blocked */ } }

// ---- panel ------------------------------------------------------------------------------------------------
async function runCheck(ctx: ToolContext) { const v = ctx.doc().version; if (state.findingsVersion === v && state.findings) return state.findings; try { const r = await ctx.ops([pop('check', {})], 'Plumbing check'); state.findings = (r[0]?.entities?.[0]?.data as any)?.findings ?? []; state.findingsVersion = ctx.doc().version; } catch { state.findings = []; } return state.findings!; }
const sel = (name: string, opts: [string, string][], cur: string, extra = '') => `<select data-opt="${name}" ${extra}>${opts.map(([k, l]) => `<option value="${k}" ${k === cur ? 'selected' : ''}>${esc(l)}</option>`).join('')}</select>`;
function panel(ctx: ToolContext, el: HTMLElement) {
  const doc = ctx.doc(); const m = mat(state.material); const ps = pipes(doc); const fs = fittings(doc); const bs = brackets(doc);
  const fk = FITTINGS.find(f => f.id === state.fitting)!;
  const fnd = state.findings ?? []; const nErr = fnd.filter(f => f.severity === 'error').length, nWarn = fnd.filter(f => f.severity === 'warning').length;
  const total = ps.reduce((s, p) => s + pipeLength(p.d), 0);
  el.innerHTML = `
    <div class="muted" style="margin:4px 0 6px;font-size:11px">${ps.length} pipes · ${fmtM(total)} · ${fs.length} fittings · ${bs.length} brackets</div>
    <div class="kv"><label>System</label>${sel('system', SYSTEMS.map(s => [s[0], s[1]]), state.system)}
      <label>Material</label>${sel('material', MATS.filter(x => x.systems.includes(state.system)).map(x => [x.id, x.label]), state.material)}
      <label>Diameter</label>${sel('dn', m.dns.map(d => [String(d), `${dnLabel(m, d)} mm`]), String(state.dn))}
      <label>Corners</label>${sel('corners', [['bend', `Bend (R ≥ ${Math.round(m.minBend * state.dn)} mm)`], ['elbow', 'Elbow fittings']], state.corners, m.minBend ? '' : 'disabled')}
      <label>Joints</label>${sel('joint', m.joints.map(j => [j, JOINTS[j]]), state.joint)}
      <label>Route via</label>${sel('via', [['direct', 'Straight at start height'], ['ceiling', 'Under the ceiling'], ['floor', 'Along the floor']], state.via)}
      <label>Grid</label>${sel('grid', [['0.01', '1 cm'], ['0.05', '5 cm'], ['0.1', '10 cm']], String(state.grid))}
      <label>Height</label><input type="number" step="0.05" data-opt="height" value="${state.height}" title="Height above the floor when you do not start on a wall (m)">
      <label>Insulation</label>${sel('insulation', [['0', 'none'], ['9', '9 mm'], ['13', '13 mm'], ['20', '20 mm'], ['25', '25 mm']], String(state.insulation))}</div>
    <div class="btn-row" style="margin-top:6px" title="Height above the floor when you do not start on a wall">${HEIGHTS.map(([l, h]) => `<button class="btn" data-height="${h}" style="padding:2px 6px;font-size:11px" title="${h} m">${l}</button>`).join('')}</div>
    ${SWAP[state.system] ? `<label class="check-row" title="Lays the matching pipe alongside: hot next to cold, return next to flow"><input type="checkbox" data-opt="pair" ${state.pair ? 'checked' : ''}> Also lay ${esc(SYSTEMS.find(x => x[0] === SWAP[state.system])![1].split(' ·')[0].toLowerCase())} alongside, <input type="number" step="0.01" data-opt="pairDistance" value="${state.pairDistance}" style="width:72px;flex:none"> m</label>` : ''}
    <label class="check-row" title="Ends that touch a fitting port, another pipe or a tap are joined with the right fitting"><input type="checkbox" data-opt="autoFittings" ${state.autoFittings ? 'checked' : ''}> Connect ends automatically (couplings, tees, wall plates)</label>
    <label class="check-row" title="Clip every new pipe to the nearest wall / ceiling / floor at the material's spacing"><input type="checkbox" data-opt="autoBrackets" ${state.autoBrackets ? 'checked' : ''}> Brackets on new pipes</label>
    <label class="check-row" title="Clicking on a wall or floor lays the pipe just in front of it"><input type="checkbox" data-opt="surfaces" ${state.surfaces ? 'checked' : ''}> Snap to surfaces</label>
    <label class="check-row" title="Pipes run inside walls and floors; this draws them through everything in front of them"><input type="checkbox" data-act="xray" ${ctx.scene.xrayMaterials.includes('plumbing.coldWater') ? 'checked' : ''}> Show pipes through walls (X-ray)</label>
    <div class="muted" style="margin:12px 0 6px;font-size:11px">Fitting tool</div>
    <div class="kv"><label>Fitting</label>${sel('fitting', FITTINGS.filter(f => !f.systems.length || f.systems.includes(state.system)).map(f => [f.id, f.label]), state.fitting)}
      ${fk.dn2 ? `<label>${fk.id === 'manifold' ? 'Outlets Ø' : fk.id === 'reducer' ? 'To Ø' : 'Branch Ø'}</label>${sel('dn2', m.dns.map(d => [String(d), `${d} mm`]), String(state.dn2))}` : ''}
      ${fk.id === 'manifold' ? `<label>Outlets</label><input type="number" min="2" max="12" step="1" data-opt="outlets" value="${state.outlets}">` : ''}</div>
    <div class="muted" style="margin:12px 0 6px;font-size:11px">Brackets</div>
    <div class="kv"><label>Kind</label>${sel('bracketKind', BRACKETS, state.bracketKind)}</div>
    <div class="btn-row"><button class="btn" data-act="autoBrackets" title="Clip the selected pipes (or all pipes) at the material's spacing">Auto brackets ${ctx.selection().size ? '(selection)' : '(all)'}</button></div>
    <div class="muted" style="margin:12px 0 6px;font-size:11px">Checks ${state.findings ? `<span style="float:right">${nErr} errors · ${nWarn} warnings</span>` : ''}</div>
    ${state.findings ? (fnd.length ? fnd.map(f => `<div class="list-row" ${f.ids?.length ? `data-select="${f.ids.join(',')}"` : ''} title="${esc(f.detail)}"><span class="swatch" style="background:${({ error: '#d93025', warning: '#f2b705', info: '#5a93f0', ok: '#1e8e3e' } as any)[f.severity]}"></span><span class="name">${esc(f.title)}</span></div>`).join('') : '<div class="empty">No findings.</div>') : ''}
    <div class="btn-row"><button class="btn" data-act="check">${state.findings ? 'Re-run checks' : 'Run checks'}</button><button class="btn" data-act="takeoff">Material list</button><button class="btn" data-act="importRuns" title="Turn the construction plugin's water / drain / heating runs into pipes">Import runs</button></div>`;
  el.querySelectorAll<HTMLSelectElement | HTMLInputElement>('[data-opt]').forEach(i => i.onchange = () => {
    const k = i.dataset.opt!; const v = (i as HTMLInputElement).type === 'checkbox' ? (i as HTMLInputElement).checked : i.value;
    if (k === 'system') { state.system = v as string; state.touchedMaterial = false; sysDefaults(); if (!FITTINGS.find(f => f.id === state.fitting)!.systems.includes(state.system) && FITTINGS.find(f => f.id === state.fitting)!.systems.length) state.fitting = 'coupling'; }
    else if (k === 'material') { state.material = v as string; state.touchedMaterial = true; const mm = mat(state.material); state.dn = mm.dns[0]; state.joint = mm.joints[0]; state.corners = defaultCorners(mm, state.dn); }
    else if (k === 'dn') { state.dn = parseFloat(v as string); state.corners = defaultCorners(mat(state.material), state.dn); }
    else if (['height', 'insulation', 'dn2', 'grid', 'pairDistance'].includes(k)) (state as any)[k] = parseFloat(v as string);
    else if (k === 'outlets') state.outlets = parseInt(v as string);
    else (state as any)[k] = v;
    host.refresh();
  });
  el.querySelectorAll<HTMLElement>('[data-height]').forEach(b => b.onclick = () => { state.height = parseFloat(b.dataset.height!); host.refresh(); });
  el.querySelector<HTMLInputElement>('[data-act="xray"]')?.addEventListener('change', e => setXray(ctx, (e.target as HTMLInputElement).checked));
  el.querySelector<HTMLElement>('[data-act="autoBrackets"]')?.addEventListener('click', async () => { const ids = [...ctx.selection()].filter(id => ps.some(p => p.id === id)); await ctx.ops([pop('auto_brackets', { pipes: ids.length ? ids : undefined, kind: state.bracketKind })], 'Auto brackets'); });
  el.querySelector<HTMLElement>('[data-act="check"]')?.addEventListener('click', async () => { state.findings = null; await runCheck(ctx); host.refresh(); });
  el.querySelectorAll<HTMLElement>('[data-select]').forEach(r => r.onclick = () => ctx.select(r.dataset.select!.split(',')));
  el.querySelector<HTMLElement>('[data-act="takeoff"]')?.addEventListener('click', () => showTakeoff(ctx));
  el.querySelector<HTMLElement>('[data-act="importRuns"]')?.addEventListener('click', async () => { const r = await ctx.ops([pop('import_runs', { remove: true })], 'Import runs'); ctx.toast(`${r[0]?.created?.length ?? 0} runs became pipes`); });
}
const pipeLength = (p: Pipe) => { let L = 0; for (let i = 1; i < p.points.length; i++) L += v3(p.points[i]).distanceTo(v3(p.points[i - 1])); return L; };

async function showTakeoff(ctx: ToolContext) {
  const r = await ctx.ops([pop('takeoff', {})], 'Material list'); const t = (r[0]?.entities?.[0]?.data as any); if (!t) return;
  const rows = (t.items as any[]);
  const el = document.createElement('div'); el.className = 'dlg-host';
  const group = (g: string, title: string) => { const its = rows.filter(i => i.group === g); return its.length ? `<tr><th colspan="3" style="text-align:left;padding-top:10px">${title}</th></tr>` + its.map(i => `<tr><td>${esc(i.labelNl)}<div class="muted" style="font-size:11px">${esc(i.label)}</div></td><td>${i.length_m != null ? fmtM(i.length_m) : ''}</td><td>${i.count != null ? i.count : i.pipes != null ? `${i.pipes} pcs${i.bends ? ` · ${i.bends} bends` : ''}` : ''}</td></tr>`).join('') : ''; };
  el.innerHTML = `<div class="dlg" style="width:min(96vw,640px);max-height:90vh"><div class="dlg-title" style="display:flex;align-items:center;gap:10px">Materiaallijst · material takeoff<span style="flex:1"></span><button class="btn" data-csv>Download CSV</button><button class="btn" data-close>Close</button></div>
    <div style="overflow:auto;max-height:70vh"><table class="data"><tr><th>Item</th><th>Length</th><th>Count</th></tr>${group('pipe', 'Pipes · buizen')}${group('fitting', 'Fittings · koppelingen')}${group('bracket', 'Brackets · beugels')}${group('insulation', 'Insulation · isolatie')}</table></div>
    <div class="muted" style="font-size:11px">${t.totals.pipes} pipes · ${fmtM(t.totals.pipeLength_m)} · ${t.totals.fittings} fittings · ${t.totals.brackets} brackets</div></div>`;
  document.body.appendChild(el); const close = () => el.remove();
  el.querySelector<HTMLElement>('[data-close]')!.onclick = close; el.addEventListener('pointerdown', e => { if (e.target === el) close(); });
  el.querySelector<HTMLElement>('[data-csv]')!.onclick = () => { const csv = ['group;item;item_en;material;dn;dn2;joint;length_m;count', ...rows.map(i => [i.group, i.labelNl, i.label, i.material ?? '', i.dn ?? '', i.dn2 ?? '', i.joint ?? '', i.length_m ?? '', i.count ?? i.pipes ?? ''].map(x => `"${String(x).replace(/"/g, '""')}"`).join(';'))].join('\n'); const a = document.createElement('a'); a.href = URL.createObjectURL(new Blob([csv], { type: 'text/csv' })); a.download = 'materiaallijst.csv'; a.click(); };
}

// ---- Entity Info ---------------------------------------------------------------------------------------
function renderEntity(ctx: ToolContext, e: Entity, el: HTMLElement): boolean {
  if (e.kind !== 'custom') return false;
  const upd = (op: string, patch: any) => ctx.ops([pop(op, { id: e.id, patch })], 'Edit');
  const erase = `<div class="btn-row"><button class="btn" data-erase>Erase</button></div>`;
  if (e.type === 'plumbing.pipe') {
    const d = e.data as Pipe; const m = mat(d.material);
    const segs = d.points.slice(1).map((p, i) => v3(p).distanceTo(v3(d.points[i])));
    el.innerHTML = `<div class="kv"><label>Type</label><div><b>Pipe</b> <span class="muted">${e.id.slice(0, 8)}</span></div>
      <label>Name</label><input type="text" data-f="name" value="${esc(d.name ?? '')}" placeholder="e.g. KW keuken">
      <label>System</label>${sel('system', SYSTEMS.map(s => [s[0], s[1]]), d.system)}
      <label>Material</label>${sel('material', MATS.map(x => [x.id, x.label]), d.material)}
      <label>Diameter</label>${sel('dn', m.dns.map(x => [String(x), `${dnLabel(m, x)} mm`]), String(d.dn))}
      <label>Corners</label>${sel('corners', [['bend', 'Bend'], ['elbow', 'Elbow fittings']], d.corners)}
      <label>Joints</label>${sel('joint', m.joints.map(j => [j, JOINTS[j]]), d.joint)}
      <label>Bend radius</label><input type="number" step="0.01" data-f="bendRadius" value="${d.bendRadius ?? ''}" placeholder="${(m.minBend * d.dn / 1000).toFixed(3)} (min)">
      <label>Insulation</label>${sel('insulation', [['0', 'none'], ['9', '9 mm'], ['13', '13 mm'], ['20', '20 mm'], ['25', '25 mm']], String(d.insulation ?? 0))}
      <label>Length</label><div>${fmtM(pipeLength(d))} · ${d.points.length - 1} legs</div>
      <label>Connected</label><div class="muted">${d.from ? 'start ✓' : 'start open'} · ${d.to ? 'end ✓' : 'end open'}</div></div>
      <table class="data"><tr><th>Leg</th><th>Length (m)</th></tr>${segs.map((L, i) => `<tr><td>${i + 1}</td><td><input type="number" step="0.01" data-seg="${i}" value="${L.toFixed(3)}" style="width:90px;text-align:right"></td></tr>`).join('')}</table>
      <div class="btn-row"><button class="btn" data-brackets>Auto brackets</button>${SWAP[d.system] ? `<button class="btn" data-alongside title="A matching pipe beside this one, with the same corners">+ ${esc(SYSTEMS.find(x => x[0] === SWAP[d.system])![1].split(' ·')[0])} alongside</button>` : ''}<button class="btn" data-erase>Erase</button></div>`;
    el.querySelector<HTMLElement>('[data-alongside]')?.addEventListener('click', async () => { const r = await ctx.ops([pop('parallel', { id: e.id, system: SWAP[d.system], distance: state.pairDistance })], 'Pipe alongside'); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]); });
    el.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[data-opt],[data-f]').forEach(i => i.onchange = () => { const k = i.dataset.opt ?? i.dataset.f!; const v: any = ['dn', 'insulation', 'bendRadius'].includes(k) ? (i.value === '' ? null : parseFloat(i.value)) : i.value || null; const patch: any = { [k]: v }; if (k === 'material') { const mm = mat(i.value); patch.dn = mm.dns[0]; patch.joint = mm.joints[0]; patch.corners = defaultCorners(mm, mm.dns[0]); } upd('update_pipe', patch); });
    el.querySelectorAll<HTMLInputElement>('[data-seg]').forEach(i => i.onchange = () => ctx.ops([pop('set_segment_length', { id: e.id, segment: parseInt(i.dataset.seg!), length: parseFloat(i.value) })], 'Leg length'));
    el.querySelector<HTMLElement>('[data-brackets]')!.onclick = () => ctx.ops([pop('auto_brackets', { pipes: [e.id], kind: state.bracketKind })], 'Auto brackets');
  } else if (e.type === 'plumbing.fitting') {
    const d = e.data as Fitting; const m = mat(d.material); const k = FITTINGS.find(f => f.id === d.kind); const ps = ports(d); const y0 = base(ctx.doc(), d.level);
    const ends = targets(ctx.doc()).filter(t => t.kind === 'port' && t.id === e.id);
    el.innerHTML = `<div class="kv"><label>Type</label><div><b>${esc(k?.label.split(' ·')[0] ?? d.kind)}</b> <span class="muted">${e.id.slice(0, 8)}</span></div>
      <label>Kind</label>${sel('kind', FITTINGS.map(f => [f.id, f.label]), d.kind)}
      ${k?.tap ? '' : `<label>Joint</label>${sel('joint', m.joints.map(j => [j, JOINTS[j]]), d.joint)}<label>Material</label>${sel('material', MATS.map(x => [x.id, x.label]), d.material)}`}
      <label>Diameter</label><input type="number" step="1" data-f="dn" value="${d.dn}">
      ${k?.dn2 ? `<label>${d.kind === 'manifold' ? 'Outlets Ø' : 'Second Ø'}</label><input type="number" step="1" data-f="dn2" value="${d.dn2 ?? ''}">` : ''}
      ${d.kind === 'manifold' ? `<label>Outlets</label><input type="number" min="2" max="12" data-f="outlets" value="${d.outlets ?? 4}">` : ''}
      ${k?.shape === 'Elbow' ? `<label>Angle</label><input type="number" step="0.5" data-f="angle" value="${d.angle ?? (d.kind === 'elbow45' ? 45 : 90)}">` : ''}
      ${d.thread || d.kind === 'wallPlate' || d.kind === 'adapter' ? `<label>Thread</label><input type="text" data-f="thread" value="${esc(d.thread ?? '')}" placeholder='1/2"'>` : ''}
      <label>Ports</label><div class="muted">${ps.map((p, i) => `${p.name} ${p.dn}${ends.some(t => t.pos.distanceTo(p.pos.clone().add(new THREE.Vector3(0, y0, 0))) < 0.001) ? ' (open)' : ' ✓'}${i < ps.length - 1 ? ' · ' : ''}`).join('')}</div>
      <label>Turn</label><div class="row"><button class="btn" data-turn="90" title="Turn the branch / outlet 90° about the main axis">↻ branch 90°</button><button class="btn" data-flip title="Flip the main axis">⇄ flip</button></div></div>${erase}`;
    el.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[data-opt],[data-f]').forEach(i => i.onchange = () => { const kk = i.dataset.opt ?? i.dataset.f!; const v: any = ['dn', 'dn2', 'angle'].includes(kk) ? (i.value === '' ? null : parseFloat(i.value)) : kk === 'outlets' ? parseInt(i.value) : i.value || null; upd('update_fitting', { [kk]: v }); });
    el.querySelector<HTMLElement>('[data-turn]')!.onclick = () => { const { d: ax, d2 } = fittingAxes(d); const n = new THREE.Vector3().crossVectors(ax, d2).normalize(); upd('update_fitting', { dir2: arr(n) }); };
    el.querySelector<HTMLElement>('[data-flip]')!.onclick = () => upd('update_fitting', { dir: arr(v3(d.dir).negate()) });
  } else if (e.type === 'plumbing.bracket') {
    const d = e.data as Bracket;
    el.innerHTML = `<div class="kv"><label>Type</label><div><b>Bracket</b> <span class="muted">${e.id.slice(0, 8)}${d.auto ? ' · auto' : ''}</span></div>
      <label>Kind</label>${sel('kind', BRACKETS, d.kind)}
      <label>Pipe Ø</label><div>${d.dn} mm</div>
      <label>Stand-off</label><input type="number" step="0.005" data-f="standoff" value="${d.standoff}" title="Pipe axis to the surface (m)">
      <label>Fixed to</label><div class="row">${[['wall −x', [-1, 0, 0]], ['wall +x', [1, 0, 0]], ['wall −z', [0, 0, -1]], ['wall +z', [0, 0, 1]], ['ceiling', [0, 1, 0]], ['floor', [0, -1, 0]]].map(([l, n]) => `<button class="btn" data-normal="${(n as number[]).join(',')}" style="padding:3px 6px;font-size:11px" title="${l}">${(l as string).replace('wall ', '')}</button>`).join('')}</div></div>${erase}`;
    el.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[data-opt],[data-f]').forEach(i => i.onchange = () => upd('update_bracket', { [i.dataset.opt ?? i.dataset.f!]: i.dataset.f === 'standoff' ? parseFloat(i.value) : i.value }));
    el.querySelectorAll<HTMLElement>('[data-normal]').forEach(b => b.onclick = () => upd('update_bracket', { normal: b.dataset.normal!.split(',').map(Number) }));
  } else return false;
  el.querySelector<HTMLElement>('[data-erase]')!.onclick = () => ctx.ops([{ op: 'erase', ids: [e.id] }], 'Erase').then(() => ctx.select([]));
  return true;
}

P.registerPlugin({
  id: 'plumbing', name: 'Plumbing', version: '0.1.0', accent: ACCENT,
  tools: [routeTool, pipeTool, adjustTool, fittingTool, bracketTool], toolbar: [['pl-route', 'pl-pipe', 'pl-adjust', 'pl-fitting', 'pl-bracket']],
  panels: [{ id: 'main', title: 'Plumbing', render: panel }],
  renderEntity,
  activate(h) {
    host = h; sysDefaults();
    if (xrayOn()) setTimeout(() => setXray(h.ctx, true), 500);
    h.on('xray', () => h.refresh());
    h.on('changed', () => { targetCache = null; if (state.findings && state.findingsVersion !== h.document().version) state.findings = null; });
    h.on('selection', () => h.refresh());
  },
});
