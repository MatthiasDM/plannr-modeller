// Construction plugin — browser side. Storeys, rooms, walls, openings, roofs, fixtures and runs as
// tools and panels on top of the Plannr core. Runtime deps come from window.plannr (no imports).
import type { Tool, ToolContext, Plane } from '../../../web/src/tools/base';
import type { Plugin, PluginHost } from '../../../web/src/plugins';
import type { Document, Entity, CustomEntity, Op, Vec3 } from '../../../web/src/model';
import type * as THREE_NS from 'three';

type V3 = THREE_NS.Vector3;
import { masonryTools, masonryPanel, masonryRenderEntity, masonryActivate, masonryHitWall, roomMasonry } from './masonry';
const P = (window as any).plannr as { registerPlugin(p: Plugin): void; THREE: typeof THREE_NS };
const THREE = P.THREE;

// ---- data types (see docs/CONSTRUCTION.md) ----------------------------------------------------
type Plan = [number, number];
interface Storey { id: string; name: string; height: number }
interface Ext { storeys: Storey[]; groundLevel?: number; wallThickness: number; floorThickness: number; cavity: { enabled: boolean; innerLeaf: number; cavity: number; outerLeaf: number } }
interface Room { name: string; level: number; points: Plan[]; hasFloor: boolean; hasCeiling: boolean; wallHeight?: number | null; finishes?: Record<string, string> }
interface Wall { level: number; a: Plan; b: Plan; height?: number | null; thickness?: number | null; exterior?: boolean | null }
type Host = { room: string; edge: number } | { wall: string };
interface Opening { level: number; kind: 'door' | 'window' | 'passage' | 'garage'; host: Host; along: number; width: number; height: number; sill: number }
interface Roof { style: 'flat' | 'gable' | 'hip' | 'shed'; pitchHeight: number; overhang: number; vergeOverhang?: number; thickness: number; ridgeAlongX: boolean; min?: Plan; max?: Plan; levels: number; enabled: boolean; covering?: string; rafterSpacing?: number; purlinSpan?: number; battenGauge?: number }
interface Fixture { level: number; kind: string; position: Vec3; host?: Host | null; rotation?: number; note?: string }
interface Run { level: number; system: string; points: Vec3[]; diameter?: number | null }
interface Stair { name: string; level: number; origin: Plan; bearing: number; width: number; going: number; risers: number; kind: 'straight' | 'quarterTurn'; turnAfter: number; turnsRight: boolean; winders?: number | null; openRisers?: boolean | null; wellOpening?: boolean | null }
interface Unit { name: string; type: string; level: number; origin: Plan; bearing: number; width: number; depth: number; height: number; lift: number; values?: Record<string, number>; choices?: Record<string, string> }
interface CatParam { name: string; label: string; kind: 'number' | 'length' | 'choice' | 'flag' | 'derived' | string; value: number; text?: string; min?: number; max?: number; choices: string[] }
interface CatType { id: string; name: string; group: string; builtin: boolean; params: CatParam[] }
let catalogue: CatType[] = []; let catalogueKey = ''; let catalogueLoading = false;
/** The catalogue only changes when the document's component types do, so key the cache on those. */
async function loadCatalogue(ctx: ToolContext) {
  const types = (ext(ctx.doc()) as any).componentTypes ?? []; const key = JSON.stringify(types.map((t: any) => [t.id, (t.source ?? '').length]));
  if (catalogueKey === key && catalogue.length) return catalogue; if (catalogueLoading) return catalogue;
  catalogueLoading = true;
  try { const r = await ctx.ops([cop('list_catalogue', {})], 'Catalogue'); const d: any = r[0]?.entities?.[0]?.data; if (d?.types) { catalogue = d.types; catalogueKey = key; host?.refresh(); } } catch { /* older engine */ }
  catalogueLoading = false;
  return catalogue;
}

const OPENING: Record<Opening['kind'], { w: number; h: number; sill: number; label: string }> = {
  door: { w: 0.9, h: 2.05, sill: 0, label: 'Door' }, window: { w: 1.2, h: 1.4, sill: 0.9, label: 'Window' }, passage: { w: 1.0, h: 2.1, sill: 0, label: 'Passage' }, garage: { w: 2.5, h: 2.1, sill: 0, label: 'Garage gate' },
};
const SYSTEMS: Record<string, { label: string; color: string }> = { electrical: { label: 'Electrical', color: '#F2B705' }, waterCold: { label: 'Cold water', color: '#2E7BE4' }, waterHot: { label: 'Hot water', color: '#E5484D' }, drain: { label: 'Drain', color: '#8A8F98' }, heating: { label: 'Heating', color: '#F2711C' } };
const FIXTURES: Record<string, { label: string; system: string; onWall: boolean; height: number }> = {
  outlet: { label: 'Outlet', system: 'electrical', onWall: true, height: 0.3 }, switchPlate: { label: 'Switch', system: 'electrical', onWall: true, height: 1.1 }, ceilingLight: { label: 'Ceiling light', system: 'electrical', onWall: false, height: -0.05 }, wallLight: { label: 'Wall light', system: 'electrical', onWall: true, height: 1.8 }, panel: { label: 'Consumer unit', system: 'electrical', onWall: true, height: 1.5 }, dataPoint: { label: 'Data point', system: 'electrical', onWall: true, height: 0.3 },
  tapCold: { label: 'Cold tap', system: 'waterCold', onWall: true, height: 0.9 }, tapHot: { label: 'Hot tap', system: 'waterHot', onWall: true, height: 0.9 }, mixer: { label: 'Mixer', system: 'waterHot', onWall: true, height: 1.1 }, showerValve: { label: 'Shower valve', system: 'waterHot', onWall: true, height: 1.2 }, toiletSupply: { label: 'Toilet supply', system: 'waterCold', onWall: true, height: 0.3 },
  floorDrain: { label: 'Floor drain', system: 'drain', onWall: false, height: 0 }, drainPoint: { label: 'Drain point', system: 'drain', onWall: true, height: 0.1 },
  radiator: { label: 'Radiator', system: 'heating', onWall: true, height: 0.6 }, boiler: { label: 'Boiler', system: 'heating', onWall: true, height: 1.4 }, manifold: { label: 'Manifold', system: 'heating', onWall: true, height: 0.6 }, thermostat: { label: 'Thermostat', system: 'heating', onWall: true, height: 1.5 },
};
const FINISHES = [['plaster', 'Plaster'], ['brick', 'Brick'], ['brickBrown', 'Brick · dark brown'], ['brickBuff', 'Brick · buff'], ['cladding', 'Cladding'], ['paintWhite', 'Paint · white'], ['paintWarm', 'Paint · warm'], ['paintGrey', 'Paint · grey'], ['paintDark', 'Paint · dark'], ['paintBlue', 'Paint · blue'], ['paintGreen', 'Paint · green'], ['concrete', 'Concrete'], ['wood', 'Wood'], ['tile', 'Tiles']];

// ---- plugin state ------------------------------------------------------------------------------
const state = { stairKind: 'quarterTurn' as Stair['kind'], unitType: 'base-unit', level: 0, openingKind: 'door' as Opening['kind'], fixtureKind: 'outlet', system: 'electrical', routeHeight: 0.3, roofStyle: 'gable' as Roof['style'], isolateSection: null as string | null };
let host: PluginHost;
const esc = (s: string) => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]!));
const ext = (doc: Document): Ext => (doc.extensions?.construction as Ext) ?? { storeys: [{ id: 's0', name: 'Ground floor', height: 2.6 }], wallThickness: 0.1, floorThickness: 0.06, cavity: { enabled: false, innerLeaf: 0.09, cavity: 0.06, outerLeaf: 0.09 } };
const base = (doc: Document, level: number) => (ext(doc).groundLevel ?? 0) + ext(doc).storeys.slice(0, level).reduce((s, f) => s + f.height, 0);
const storeyH = (doc: Document, level: number) => ext(doc).storeys[level]?.height ?? 2.6;
const customs = (doc: Document, type: string): CustomEntity[] => doc.entities.filter((e): e is CustomEntity => e.kind === 'custom' && e.type === `construction.${type}`);
const cop = (type: string, data: unknown): Op => ({ op: 'custom', type: `construction.${type}`, data });
const planeAt = (doc: Document, level: number): Plane => ({ origin: new THREE.Vector3(0, base(doc, level), 0), normal: new THREE.Vector3(0, 1, 0) });
const plan = (p: V3): Plan => [+p.x.toFixed(4), +p.z.toFixed(4)];
const fmtM = (n: number) => `${n.toFixed(2)} m`;
const INK = 0x1b1b1b, MAGENTA = 0xc8169a, SEL = 0x2f6fde;

/** Wall segments (rooms' edges + free walls) on a level, with their host refs. */
function segments(doc: Document, level: number): { a: Plan; b: Plan; host: Host; id: string }[] {
  const out: { a: Plan; b: Plan; host: Host; id: string }[] = [];
  for (const r of customs(doc, 'room')) { const d = r.data as Room; if (d.level !== level) continue; d.points.forEach((a, i) => out.push({ a, b: d.points[(i + 1) % d.points.length], host: { room: r.id, edge: i }, id: r.id })); }
  for (const w of customs(doc, 'wall')) { const d = w.data as Wall; if (d.level === level) out.push({ a: d.a, b: d.b, host: { wall: w.id }, id: w.id }); }
  return out;
}
function nearestSegment(doc: Document, level: number, p: Plan, maxD: number) {
  let best: { seg: ReturnType<typeof segments>[number]; t: number; d: number; L: number } | null = null;
  for (const seg of segments(doc, level)) {
    const dx = seg.b[0] - seg.a[0], dz = seg.b[1] - seg.a[1]; const L2 = dx * dx + dz * dz || 1e-9; const L = Math.sqrt(L2);
    let t = ((p[0] - seg.a[0]) * dx + (p[1] - seg.a[1]) * dz) / L2; t = Math.max(0, Math.min(1, t));
    const d = Math.hypot(seg.a[0] + dx * t - p[0], seg.a[1] + dz * t - p[1]);
    if (d <= maxD && (!best || d < best.d)) best = { seg, t, d, L };
  }
  return best;
}
// the app draws the snap marker of every ctx.snap() a tool makes
const drawSnapDot = (_ctx: ToolContext, _s: { kind: string; point: V3 }) => {};
const num = (v: string) => v.split(/[;,x\s]+/).map(parseFloat).filter(n => !isNaN(n));

// ---- tools ------------------------------------------------------------------------------------
const roomTool: Tool & { first?: V3 | null; last?: V3 | null } = {
  id: 'c-room', label: 'Room', hint: 'Click two corners of the room on the active storey. Type width; depth for exact size.', cursor: 'crosshair', shortcut: 'k',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M4 7h16v12H4z" fill="#e8f0fb"/><path d="M4 7l4-3h12l-4 3M20 7l4-3" /><path d="M9 19v-5h4v5" fill="#fff"/></svg>`,
  activate(ctx) { this.first = null; ctx.setHint(this.hint); ctx.setVCB('', 'Dimensions'); }, deactivate(ctx) { this.first = null; ctx.scene.clearOverlay(); },
  onMove(ev, ctx) {
    const doc = ctx.doc(); const s = ctx.snap(ev, { plane: planeAt(doc, state.level), faces: false }); ctx.scene.clearOverlay(); drawSnapDot(ctx, s);
    if (!this.first) return; this.last = s.point;
    const a = this.first, b = s.point; const y = a.y; const pts = [new THREE.Vector3(a.x, y, a.z), new THREE.Vector3(b.x, y, a.z), new THREE.Vector3(b.x, y, b.z), new THREE.Vector3(a.x, y, b.z)];
    ctx.scene.addLine([...pts, pts[0]], INK, false, 2); ctx.scene.addFace(pts);
    const w = Math.abs(b.x - a.x), d = Math.abs(b.z - a.z); ctx.scene.addLabel(pts[0].clone().lerp(pts[1], 0.5), fmtM(w)); ctx.scene.addLabel(pts[1].clone().lerp(pts[2], 0.5), fmtM(d)); ctx.setVCB(`${w.toFixed(2)}; ${d.toFixed(2)}`, 'Dimensions');
  },
  onDown(ev, ctx) {
    if (ev.button !== 0) return; const s = ctx.snap(ev, { plane: planeAt(ctx.doc(), state.level), faces: false });
    if (!this.first) { this.first = s.point.clone(); ctx.setHint('Click the opposite corner, or type width; depth and press Enter.'); return; }
    finishRoom(this, ctx, s.point);
  },
  onVCB(v, ctx) { const n = num(v); if (n.length < 2 || !this.first) return; const sx = this.last && this.last.x < this.first.x ? -1 : 1, sz = this.last && this.last.z < this.first.z ? -1 : 1; finishRoom(this, ctx, new THREE.Vector3(this.first.x + sx * n[0], this.first.y, this.first.z + sz * n[1])); },
  onKey(ev, ctx) { if (ev.key === 'Escape' && this.first) { this.first = null; ctx.scene.clearOverlay(); return true; } return false; },
};
async function finishRoom(t: typeof roomTool, ctx: ToolContext, b: V3) {
  const a = t.first!; const w = Math.abs(b.x - a.x), d = Math.abs(b.z - a.z); if (w < 0.2 || d < 0.2) { ctx.toast('Room too small'); return; }
  t.first = null; ctx.scene.clearOverlay();
  const n = customs(ctx.doc(), 'room').length + 1;
  const r = await ctx.ops([cop('add_room', { name: `Room ${n}`, level: state.level, origin: [Math.min(a.x, b.x), Math.min(a.z, b.z)], size: [w, d] })], 'Add room');
  if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]); ctx.setHint(roomTool.hint);
}

const roomShapeTool: Tool & { pts?: V3[] } = {
  id: 'c-roomshape', label: 'Room shape', hint: 'Click each corner of the room; click the first corner or press Enter to close.', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M4 5h9v6h7v8H4z" fill="#e8f0fb"/></svg>`,
  activate(ctx) { this.pts = []; ctx.setHint(this.hint); ctx.setVCB('', 'Length'); }, deactivate(ctx) { this.pts = []; ctx.scene.clearOverlay(); },
  onMove(ev, ctx) {
    const s = ctx.snap(ev, { plane: planeAt(ctx.doc(), state.level), faces: false, from: this.pts!.at(-1), lock: ctx.lock }); ctx.scene.clearOverlay(); drawSnapDot(ctx, s);
    if (!this.pts!.length) return; ctx.scene.addLine([...this.pts!, s.point], INK, false, 2); if (this.pts!.length >= 2) ctx.scene.addFace([...this.pts!, s.point]);
    ctx.scene.addLabel(this.pts!.at(-1)!.clone().lerp(s.point, 0.5), fmtM(this.pts!.at(-1)!.distanceTo(s.point)));
  },
  onDown(ev, ctx) {
    if (ev.button !== 0) return; const s = ctx.snap(ev, { plane: planeAt(ctx.doc(), state.level), faces: false, from: this.pts!.at(-1), lock: ctx.lock });
    if (this.pts!.length >= 3 && s.point.distanceTo(this.pts![0]) < ctx.scene.pixelSize(s.point) * 10) return closeRoom(this, ctx);
    this.pts!.push(s.point.clone());
  },
  onKey(ev, ctx) { if (ev.key === 'Enter') { closeRoom(this, ctx); return true; } if (ev.key === 'Escape') { this.pts = []; ctx.scene.clearOverlay(); return true; } return false; },
};
async function closeRoom(t: typeof roomShapeTool, ctx: ToolContext) {
  if (t.pts!.length < 3) return; const pts = t.pts!.map(plan); t.pts = []; ctx.scene.clearOverlay();
  // make counter-clockwise (positive area in x/z)
  let area = 0; for (let i = 0; i < pts.length; i++) { const a = pts[i], b = pts[(i + 1) % pts.length]; area += a[0] * b[1] - b[0] * a[1]; } if (area < 0) pts.reverse();
  const r = await ctx.ops([cop('add_room', { name: `Room ${customs(ctx.doc(), 'room').length + 1}`, level: state.level, points: pts })], 'Add room'); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]);
}

const wallTool: Tool & { start?: V3 | null; cur?: V3 | null } = {
  id: 'c-wall', label: 'Wall', hint: 'Click the start and end of a wall; the chain continues until Esc. Type a length.', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M3 8h18v4H3zM3 12h18v4H3zM3 16h18v4H3z" fill="#f3d9c8"/><path d="M8 8v4M14 8v4M5 12v4M11 12v4M17 12v4M8 16v4M14 16v4"/></svg>`,
  activate(ctx) { this.start = null; ctx.setHint(this.hint); ctx.setVCB('', 'Length'); }, deactivate(ctx) { this.start = null; ctx.scene.clearOverlay(); },
  onMove(ev, ctx) {
    const s = ctx.snap(ev, { plane: planeAt(ctx.doc(), state.level), faces: false, from: this.start ?? undefined, lock: ctx.lock }); ctx.scene.clearOverlay(); drawSnapDot(ctx, s);
    if (!this.start) return; this.cur = s.point; const L = this.start.distanceTo(s.point);
    const t = ext(ctx.doc()).wallThickness / 2; const d = s.point.clone().sub(this.start).normalize(); const n = new THREE.Vector3(-d.z, 0, d.x).multiplyScalar(t);
    ctx.scene.addLine([this.start.clone().add(n), s.point.clone().add(n), s.point.clone().sub(n), this.start.clone().sub(n), this.start.clone().add(n)], INK, false, 2);
    ctx.scene.addLabel(this.start.clone().lerp(s.point, 0.5), fmtM(L)); ctx.setVCB(L.toFixed(2), 'Length');
  },
  onDown(ev, ctx) {
    if (ev.button !== 0) return; const s = ctx.snap(ev, { plane: planeAt(ctx.doc(), state.level), faces: false, from: this.start ?? undefined, lock: ctx.lock });
    if (!this.start) { this.start = s.point.clone(); return; } placeWall(this, ctx, s.point);
  },
  onVCB(v, ctx) { const n = num(v); if (!n.length || !this.start || !this.cur) return; const d = this.cur.clone().sub(this.start).normalize(); placeWall(this, ctx, this.start.clone().addScaledVector(d, n[0])); },
  onDblClick(_e, ctx) { this.start = null; ctx.scene.clearOverlay(); },
  onKey(ev, ctx) { if (ev.key === 'Escape') { this.start = null; ctx.scene.clearOverlay(); return true; } return false; },
};
async function placeWall(t: typeof wallTool, ctx: ToolContext, end: V3) {
  if (!t.start || t.start.distanceTo(end) < 0.05) return;
  await ctx.ops([cop('add_wall', { level: state.level, a: plan(t.start), b: plan(end) })], 'Add wall'); t.start = end.clone();
}

const openingTool: Tool & { lastId?: string | null } = {
  id: 'c-opening', label: 'Opening', hint: 'Hover a wall and click to place a door, window, passage or garage gate. Type width; height.', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M3 20h18"/><path d="M8 20V6h8v14" fill="#fff"/><path d="M8 6c5 1 8 5 8 9" stroke="#1a73e8"/></svg>`,
  activate(ctx) { ctx.setHint(this.hint); const d = OPENING[state.openingKind]; ctx.setVCB(`${d.w}; ${d.h}`, 'Width; Height'); }, deactivate(ctx) { ctx.scene.clearOverlay(); },
  onMove(ev, ctx) {
    ctx.scene.clearOverlay();
    // a customised masonry wall under the cursor takes the opening itself
    const mw = masonryHitWall(ev, ctx); if (mw && mw.d < 0.35) { return (masonryTools[1] as Tool).onMove?.(ev, ctx); }
    const h = hitWall(ev, ctx); if (!h) return; previewOpening(ctx, h);
  },
  async onDown(ev, ctx) {
    if (ev.button !== 0) return;
    const mw = masonryHitWall(ev, ctx); if (mw && mw.d < 0.35) { return (masonryTools[1] as Tool).onDown?.(ev, ctx); }
    const h = hitWall(ev, ctx); if (!h) { ctx.toast('Hover a wall to place the opening'); return; }
    const d = OPENING[state.openingKind]; const along = Math.max(d.w / 2, Math.min(h.L - d.w / 2, h.t * h.L));
    const r = await ctx.ops([cop('add_opening', { kind: state.openingKind, host: h.seg.host, along: +along.toFixed(3), width: d.w, height: d.h, sill: d.sill })], 'Add opening');
    this.lastId = r[0]?.created?.[0] ?? null; if (this.lastId) ctx.select([this.lastId]);
  },
  onVCB(v, ctx) { const n = num(v); if (!n.length) return; const d = OPENING[state.openingKind]; d.w = n[0]; if (n[1]) d.h = n[1]; if (this.lastId) ctx.ops([cop('update', { id: this.lastId, patch: { width: d.w, height: d.h } })], 'Resize opening'); },
};
function hitWall(ev: PointerEvent, ctx: ToolContext) {
  const doc = ctx.doc(); const pk = ctx.scene.pick(ev, { facesOnly: true }); const g = ctx.scene.groundPoint(ev, base(doc, state.level));
  const p: Plan | null = pk && pk.id.includes('#wall') ? [pk.point.x, pk.point.z] : g ? [g.x, g.z] : null; if (!p) return null;
  return nearestSegment(doc, state.level, p, 0.6 * Math.max(1, ctx.scene.cam.distance / 15));
}
function previewOpening(ctx: ToolContext, h: NonNullable<ReturnType<typeof hitWall>>) {
  const d = OPENING[state.openingKind]; const along = Math.max(d.w / 2, Math.min(h.L - d.w / 2, h.t * h.L)); const y0 = base(ctx.doc(), state.level) + d.sill, y1 = y0 + d.h;
  const dir = new THREE.Vector3(h.seg.b[0] - h.seg.a[0], 0, h.seg.b[1] - h.seg.a[1]).normalize(); const c = new THREE.Vector3(h.seg.a[0], 0, h.seg.a[1]).addScaledVector(dir, along);
  const l = c.clone().addScaledVector(dir, -d.w / 2), r = c.clone().addScaledVector(dir, d.w / 2);
  ctx.scene.addLine([l.clone().setY(y0), r.clone().setY(y0), r.clone().setY(y1), l.clone().setY(y1), l.clone().setY(y0)], MAGENTA, false, 2);
  ctx.scene.addLabel(c.clone().setY(y1), `${d.label} ${d.w}×${d.h}`); ctx.scene.addLabel(l.clone().setY(y0), fmtM(along - d.w / 2), 'small');
}

const fixtureTool: Tool = {
  id: 'c-fixture', label: 'Fixture', hint: 'Click to place the fixture; wall fittings snap to the nearest wall. Type a height above the floor.', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><rect x="5" y="5" width="14" height="14" rx="2" fill="#fff6d6"/><circle cx="12" cy="12" r="4"/><path d="M10.5 11v2M13.5 11v2" stroke-width="2"/></svg>`,
  activate(ctx) { ctx.setHint(this.hint); ctx.setVCB(String(FIXTURES[state.fixtureKind].height), 'Height'); }, deactivate(ctx) { ctx.scene.clearOverlay(); },
  onMove(ev, ctx) { ctx.scene.clearOverlay(); const r = placeFixture(ev, ctx); if (!r) return; const col = parseInt(SYSTEMS[FIXTURES[state.fixtureKind].system].color.slice(1), 16); const p = new THREE.Vector3(r.position[0], base(ctx.doc(), state.level) + r.position[1], r.position[2]); ctx.scene.addDot(p, col, 12); ctx.scene.addLine([new THREE.Vector3(p.x, base(ctx.doc(), state.level), p.z), p], col, true); ctx.scene.addLabel(p, FIXTURES[state.fixtureKind].label); },
  async onDown(ev, ctx) { if (ev.button !== 0) return; const r = placeFixture(ev, ctx); if (!r) return; const res = await ctx.ops([cop('add_fixture', { level: state.level, kind: state.fixtureKind, ...r })], 'Add fixture'); if (res[0]?.created?.[0]) ctx.select([res[0].created[0]]); },
  onVCB(v) { const n = num(v); if (n.length) FIXTURES[state.fixtureKind].height = n[0]; },
};
function placeFixture(ev: PointerEvent, ctx: ToolContext) {
  const doc = ctx.doc(); const g = ctx.scene.groundPoint(ev, base(doc, state.level)); if (!g) return null;
  const info = FIXTURES[state.fixtureKind]; let p: Plan = [g.x, g.z]; let host: Host | null = null; let rotation = 0;
  if (info.onWall) { const h = nearestSegment(doc, state.level, p, 0.8); if (h) { const dx = h.seg.b[0] - h.seg.a[0], dz = h.seg.b[1] - h.seg.a[1]; const L = Math.hypot(dx, dz) || 1; const nx = -dz / L, nz = dx / L; const q: Plan = [h.seg.a[0] + dx * h.t, h.seg.a[1] + dz * h.t]; const side = (p[0] - q[0]) * nx + (p[1] - q[1]) * nz >= 0 ? 1 : -1; const off = ext(doc).wallThickness / 2 + 0.02; p = [q[0] + nx * off * side, q[1] + nz * off * side]; host = h.seg.host; rotation = Math.atan2(dz, dx); } }
  const h = state.fixtureKind === 'ceilingLight' ? storeyH(doc, state.level) - 0.05 : info.height;
  return { position: [+p[0].toFixed(3), +h.toFixed(3), +p[1].toFixed(3)] as Vec3, host, rotation: +rotation.toFixed(4) };
}

const runTool: Tool & { pts?: V3[] } = {
  id: 'c-run', label: 'Route (pipe / cable run)', hint: 'Route = a pipe or cable run drawn as a polyline at a fixed height above the floor (set it in the Measurements box, e.g. 0.3 for skirting level). Pick the system (cold / hot water, drain, heating, electrical) in the Construction panel, click each bend, then double-click or press Enter to finish. For electrical wiring use the Electrical plugin’s Conduit tool instead.', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M3 18h6l3-7 3 4h6" stroke="#f2b705"/><path d="M3 12h4l3 6 3-10 3 6h5" stroke="#2e7be4"/></svg>`,
  activate(ctx) { this.pts = []; ctx.setHint(this.hint); ctx.setVCB(String(state.routeHeight), 'Height'); }, deactivate(ctx) { this.pts = []; ctx.scene.clearOverlay(); },
  onMove(ev, ctx) { const y = base(ctx.doc(), state.level) + state.routeHeight; const s = ctx.snap(ev, { plane: { origin: new THREE.Vector3(0, y, 0), normal: new THREE.Vector3(0, 1, 0) }, faces: false, from: this.pts!.at(-1), lock: ctx.lock }); ctx.scene.clearOverlay(); const col = parseInt(SYSTEMS[state.system].color.slice(1), 16); ctx.scene.addDot(s.point, col); if (this.pts!.length) { ctx.scene.addLine([...this.pts!, s.point], col, false, 2); ctx.scene.addLabel(this.pts!.at(-1)!.clone().lerp(s.point, 0.5), fmtM(this.pts!.at(-1)!.distanceTo(s.point))); } },
  onDown(ev, ctx) { if (ev.button !== 0) return; const y = base(ctx.doc(), state.level) + state.routeHeight; const s = ctx.snap(ev, { plane: { origin: new THREE.Vector3(0, y, 0), normal: new THREE.Vector3(0, 1, 0) }, faces: false, from: this.pts!.at(-1), lock: ctx.lock }); if (this.pts!.length && s.point.distanceTo(this.pts!.at(-1)!) < 0.05) return; this.pts!.push(s.point.clone()); },
  onDblClick(_e, ctx) { this.pts!.pop(); finishRun(this, ctx); },
  onKey(ev, ctx) { if (ev.key === 'Enter') { finishRun(this, ctx); return true; } if (ev.key === 'Escape') { this.pts = []; ctx.scene.clearOverlay(); return true; } return false; },
  onVCB(v, ctx) { const n = num(v); if (n.length) { state.routeHeight = n[0]; const y = base(ctx.doc(), state.level) + n[0]; for (const p of this.pts!) p.y = y; } },
};
async function finishRun(t: typeof runTool, ctx: ToolContext) {
  const b = base(ctx.doc(), state.level);
  if (t.pts!.length >= 2) { const r = await ctx.ops([cop('add_run', { level: state.level, system: state.system, points: t.pts!.map(p => [+p.x.toFixed(3), +(p.y - b).toFixed(3), +p.z.toFixed(3)]) })], 'Add run'); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]); }
  t.pts = []; ctx.scene.clearOverlay();
}

const roofTool: Tool & { first?: V3 | null } = {
  id: 'c-roof', label: 'Roof', hint: 'Drag the footprint of a roof over the active storey (or just click once for the storey\'s whole footprint).', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M3 13 12 5l9 8" stroke="#7a8aa0" stroke-width="2.4"/><path d="M6 11v8h12v-8" fill="#fff"/></svg>`,
  activate(ctx) { this.first = null; ctx.setHint(this.hint); ctx.setVCB('', 'Pitch'); }, deactivate(ctx) { this.first = null; ctx.scene.clearOverlay(); },
  onMove(ev, ctx) {
    const doc = ctx.doc(); const top = base(doc, state.level) + storeyH(doc, state.level); const s = ctx.snap(ev, { plane: { origin: new THREE.Vector3(0, top, 0), normal: new THREE.Vector3(0, 1, 0) }, faces: false }); ctx.scene.clearOverlay(); drawSnapDot(ctx, s);
    if (!this.first) return; const a = this.first, b = s.point; const pts = [new THREE.Vector3(a.x, top, a.z), new THREE.Vector3(b.x, top, a.z), new THREE.Vector3(b.x, top, b.z), new THREE.Vector3(a.x, top, b.z)]; ctx.scene.addLine([...pts, pts[0]], 0x7a8aa0, false, 2); ctx.scene.addFace(pts, 0x7a8aa0, 0.25);
  },
  async onDown(ev, ctx) {
    if (ev.button !== 0) return; const doc = ctx.doc(); const top = base(doc, state.level) + storeyH(doc, state.level);
    const s = ctx.snap(ev, { plane: { origin: new THREE.Vector3(0, top, 0), normal: new THREE.Vector3(0, 1, 0) }, faces: false });
    if (!this.first) { this.first = s.point.clone(); ctx.setHint('Click the opposite corner of the footprint (or press Enter to use the storey footprint).'); return; }
    const a = this.first, b = s.point; this.first = null; ctx.scene.clearOverlay();
    const data: Partial<Roof> = { style: state.roofStyle, levels: state.level + 1, min: [Math.min(a.x, b.x), Math.min(a.z, b.z)], max: [Math.max(a.x, b.x), Math.max(a.z, b.z)] };
    if (Math.abs(a.x - b.x) < 0.3 || Math.abs(a.z - b.z) < 0.3) { delete data.min; delete data.max; }
    const r = await ctx.ops([cop('add_roof', data)], 'Add roof'); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]);
  },
  onKey(ev, ctx) { if (ev.key === 'Enter') { this.first = null; ctx.ops([cop('add_roof', { style: state.roofStyle, levels: state.level + 1 })], 'Add roof'); return true; } if (ev.key === 'Escape') { this.first = null; ctx.scene.clearOverlay(); return true; } return false; },
};

const stairTool: Tool & { start?: V3 | null } = {
  id: 'c-stair', label: 'Stair', hint: 'Click the foot of the stair, then a second point in the direction of climb. Type a width.', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M3 20h4v-4h4v-4h4V8h4V4" fill="#fff6d6"/><path d="M3 20h18"/></svg>`,
  activate(ctx) { this.start = null; ctx.setHint(this.hint); ctx.setVCB('0.9', 'Width'); }, deactivate(ctx) { this.start = null; ctx.scene.clearOverlay(); },
  onMove(ev, ctx) {
    const doc = ctx.doc(); const s = ctx.snap(ev, { plane: planeAt(doc, state.level), faces: false, from: this.start ?? undefined, lock: ctx.lock }); ctx.scene.clearOverlay(); drawSnapDot(ctx, s);
    if (!this.start) return; const a = this.start, b = s.point; const d = b.clone().sub(a); d.y = 0; if (d.lengthSq() < 1e-6) return; d.normalize();
    const n = new THREE.Vector3(-d.z, 0, d.x).multiplyScalar(0.45); const risers = Math.max(3, Math.round(storeyH(doc, state.level) / 0.18)); const L = (risers - 1) * 0.245;
    const steps = Math.min(risers, state.stairKind === 'quarterTurn' ? 4 : risers - 1);
    for (let i = 0; i <= steps; i++) { const p = a.clone().addScaledVector(d, i * 0.245); ctx.scene.addLine([p.clone().add(n), p.clone().sub(n)], INK); }
    ctx.scene.addLine([a.clone().add(n), a.clone().addScaledVector(d, steps * 0.245).add(n)], INK); ctx.scene.addLine([a.clone().sub(n), a.clone().addScaledVector(d, steps * 0.245).sub(n)], INK);
    ctx.scene.addLabel(a.clone().addScaledVector(d, L / 2), `${risers} risers · ${state.stairKind === 'quarterTurn' ? 'quarter turn' : 'straight'}`);
  },
  async onDown(ev, ctx) {
    if (ev.button !== 0) return; const doc = ctx.doc(); const s = ctx.snap(ev, { plane: planeAt(doc, state.level), faces: false, from: this.start ?? undefined, lock: ctx.lock });
    if (!this.start) { this.start = s.point.clone(); ctx.setHint('Click a point in the direction of climb.'); return; }
    const a = this.start; const d = s.point.clone().sub(a); d.y = 0; if (d.lengthSq() < 1e-4) return; this.start = null; ctx.scene.clearOverlay();
    const bearing = +(Math.atan2(d.x, d.z) * 180 / Math.PI).toFixed(2);
    const r = await ctx.ops([cop('add_stair', { name: 'Stair', level: state.level, origin: plan(a), bearing, width: 0.9, going: 0.245, risers: Math.max(3, Math.round(storeyH(doc, state.level) / 0.18)), kind: state.stairKind, turnAfter: 4, turnsRight: true })], 'Add stair');
    if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]); ctx.setHint(this.hint);
  },
  onKey(ev, ctx) { if (ev.key === 'Escape' && this.start) { this.start = null; ctx.scene.clearOverlay(); return true; } return false; },
};
const UNIT_TYPES: Record<string, { label: string; w: number; d: number; h: number; lift: number }> = {
  'base-unit': { label: 'Base unit', w: 0.6, d: 0.6, h: 0.72, lift: 0.12 }, 'wall-unit': { label: 'Wall cupboard', w: 0.6, d: 0.37, h: 0.72, lift: 1.4 }, 'tall-unit': { label: 'Tall unit', w: 0.6, d: 0.6, h: 2.0, lift: 0.12 },
  'extractor': { label: 'Extractor hood', w: 0.9, d: 0.5, h: 0.4, lift: 1.5 }, 'double-vanity': { label: 'Double vanity', w: 1.2, d: 0.48, h: 0.56, lift: 0.3 }, 'bath': { label: 'Bath', w: 1.7, d: 0.7, h: 0.55, lift: 0 }, 'shower': { label: 'Shower tray', w: 0.9, d: 0.9, h: 0.06, lift: 0 }, 'toilet': { label: 'WC', w: 0.4, d: 0.7, h: 0.4, lift: 0 },
  'shelving': { label: 'Shelving', w: 1.0, d: 0.3, h: 2.4, lift: 0 }, 'bed': { label: 'Bed', w: 1.6, d: 2.1, h: 0.5, lift: 0 }, 'wardrobe': { label: 'Wardrobe', w: 1.0, d: 0.6, h: 2.1, lift: 0 }, 'table': { label: 'Table', w: 1.8, d: 0.85, h: 0.75, lift: 0 }, 'chair': { label: 'Chair', w: 0.45, d: 0.5, h: 0.85, lift: 0 }, 'sofa': { label: 'Sofa', w: 2.2, d: 0.9, h: 0.8, lift: 0 }, 'desk': { label: 'Desk', w: 1.4, d: 0.8, h: 0.74, lift: 0 }, 'box': { label: 'Box', w: 0.6, d: 0.6, h: 0.75, lift: 0 },
};
const unitTool: Tool = {
  id: 'c-unit', label: 'Unit / furniture', hint: 'Pick a type in the Construction panel, then click the middle of its back (against a wall). Hold → or ← to turn it. Scripted types take their size from their parameters.', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M4 9h16v10H4z" fill="#f3e3c8"/><path d="M4 14h16M12 9v10M9 11.5v1M15 11.5v1M9 16.5v1M15 16.5v1"/></svg>`,
  activate(ctx) { ctx.setHint(this.hint); const u = UNIT_TYPES[state.unitType]; ctx.setVCB(u ? `${u.w}; ${u.d}; ${u.h}` : '', 'W; D; H'); void loadCatalogue(ctx).then(() => host.refresh()); }, deactivate(ctx) { ctx.scene.clearOverlay(); },
  onMove(ev, ctx) {
    const doc = ctx.doc(); const s = ctx.snap(ev, { plane: planeAt(doc, state.level), faces: false }); ctx.scene.clearOverlay(); drawSnapDot(ctx, s);
    const ct = catalogue.find(t => t.id === state.unitType); const u = UNIT_TYPES[state.unitType] ?? { label: ct?.name ?? state.unitType, w: num1(ct, 'width', 0.6), d: num1(ct, 'depth', 0.6), h: num1(ct, 'height', 0.75), lift: 0 };
    const bearing = ctx.lock === 'x' ? 90 : 0; const rad = bearing * Math.PI / 180;
    const f = new THREE.Vector3(Math.sin(rad), 0, -Math.cos(rad)), r = new THREE.Vector3(-f.z, 0, f.x); // Studio: origin = middle of the back, extends forward
    const c = s.point; const y = c.y + u.lift; const P0 = (sw: number, sd: number) => c.clone().addScaledVector(r, sw * u.w / 2).addScaledVector(f, sd * u.d).setY(y);
    const pts = [P0(-1, 0), P0(1, 0), P0(1, 1), P0(-1, 1)];
    ctx.scene.addLine([...pts, pts[0]], INK, false, 2); ctx.scene.addFace(pts); for (const p of pts) ctx.scene.addLine([p, p.clone().setY(y + u.h)], INK, true); ctx.scene.addLabel(c.clone().setY(y + u.h), ct?.name ?? u.label);
  },
  async onDown(ev, ctx) {
    if (ev.button !== 0) return; const doc = ctx.doc(); const s = ctx.snap(ev, { plane: planeAt(doc, state.level), faces: false });
    const ct = catalogue.find(t => t.id === state.unitType); const u = UNIT_TYPES[state.unitType];
    const data: any = { name: ct?.name ?? u?.label ?? state.unitType, type: state.unitType, level: state.level, origin: plan(s.point), bearing: ctx.lock === 'x' ? 90 : 0 };
    if (!ct && u) Object.assign(data, { width: u.w, depth: u.d, height: u.h, lift: u.lift });
    const r = await ctx.ops([cop('add_unit', data)], 'Add unit');
    if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]);
  },
  onVCB(v) { const n = num(v); const u = UNIT_TYPES[state.unitType]; if (!u) return; if (n[0]) u.w = n[0]; if (n[1]) u.d = n[1]; if (n[2]) u.h = n[2]; },
};
const num1 = (ct: CatType | undefined, name: string, d: number) => { const p = ct?.params.find(p => p.name === name); return p && typeof p.value === 'number' && p.value > 0 ? p.value : d; };

// ---- panel + entity editor ---------------------------------------------------------------------
function panel(ctx: ToolContext, el: HTMLElement) {
  const doc = ctx.doc(); const x = ext(doc); const roofs = customs(doc, 'roof');
  el.innerHTML = `
    <div class="muted" style="margin:4px 0 6px;font-size:11px">Storeys</div>
    ${x.storeys.map((s, i) => `<div class="list-row ${i === state.level ? 'active' : ''}" data-lvl="${i}"><span class="name">${esc(s.name)}</span><input type="number" step="0.05" style="width:64px" data-storey-h="${i}" value="${s.height}" title="Storey height (m)">${x.storeys.length > 1 ? `<button class="icon-btn" data-storey-del="${i}" title="Remove storey">✕</button>` : ''}</div>`).join('')}
    <div class="btn-row"><button class="btn" data-act="addStorey">+ Add storey</button><label style="margin-left:auto;display:flex;align-items:center;gap:5px"><input type="checkbox" data-act="isolate" ${state.isolateSection ? 'checked' : ''}> Isolate storey</label></div>
    <div class="muted" style="margin:12px 0 6px;font-size:11px">Construction</div>
    <div class="kv"><label>Ground level</label><input type="number" step="0.01" data-set="groundLevel" value="${x.groundLevel ?? 0}" title="Height of the ground floor above the world origin (m)"><label>Wall thickness</label><input type="number" step="0.01" data-set="wallThickness" value="${x.wallThickness}"><label>Floor thickness</label><input type="number" step="0.01" data-set="floorThickness" value="${x.floorThickness}">
      <label>Cavity walls</label><input type="checkbox" data-cav="enabled" ${x.cavity.enabled ? 'checked' : ''}><label>Inner leaf</label><input type="number" step="0.01" data-cav="innerLeaf" value="${x.cavity.innerLeaf}"><label>Cavity</label><input type="number" step="0.01" data-cav="cavity" value="${x.cavity.cavity}"><label>Outer leaf</label><input type="number" step="0.01" data-cav="outerLeaf" value="${x.cavity.outerLeaf}"></div>
    <div class="muted" style="margin:12px 0 6px;font-size:11px">Roofs</div>
    ${roofs.length ? roofs.map(r => { const d = r.data as Roof; return `<div class="list-row" data-select="${r.id}"><span class="name">${d.style} roof · ${d.levels} storey${d.levels > 1 ? 's' : ''}${d.min ? ` · ${(d.max![0] - d.min[0]).toFixed(1)}×${(d.max![1] - d.min[1]).toFixed(1)} m` : ''}</span><button class="icon-btn" data-roof-toggle="${r.id}" title="${d.enabled ? 'Hide' : 'Show'}">${d.enabled ? '◉' : '○'}</button></div>`; }).join('') : `<div class="empty">No roof yet — use the Roof tool.</div>`}
    <div class="muted" style="margin:12px 0 6px;font-size:11px">Tool options</div>
    <div class="kv"><label>Opening</label><select data-opt="openingKind">${(Object.keys(OPENING) as Opening['kind'][]).map(k => `<option value="${k}" ${state.openingKind === k ? 'selected' : ''}>${OPENING[k].label}</option>`).join('')}</select>
      <label>Fixture</label><select data-opt="fixtureKind">${Object.entries(FIXTURES).map(([k, v]) => `<option value="${k}" ${state.fixtureKind === k ? 'selected' : ''}>${v.label} · ${SYSTEMS[v.system].label}</option>`).join('')}</select>
      <label>Run system</label><select data-opt="system">${Object.entries(SYSTEMS).map(([k, v]) => `<option value="${k}" ${state.system === k ? 'selected' : ''}>${v.label}</option>`).join('')}</select>
      <label>Roof style</label><select data-opt="roofStyle">${['flat', 'gable', 'hip', 'shed'].map(k => `<option ${state.roofStyle === k ? 'selected' : ''}>${k}</option>`).join('')}</select>
      <label>Stair</label><select data-opt="stairKind"><option value="straight" ${state.stairKind === 'straight' ? 'selected' : ''}>Straight</option><option value="quarterTurn" ${state.stairKind === 'quarterTurn' ? 'selected' : ''}>Quarter turn</option></select>
      <label>Unit</label><select data-opt="unitType">${(() => { const groups = new Map<string, CatType[]>(); for (const t of catalogue) { const g = groups.get(t.group || 'Catalogue') ?? []; g.push(t); groups.set(t.group || 'Catalogue', g); } let o = ''; for (const [g, ts] of groups) o += `<optgroup label="${esc(g)}">${ts.map(t => `<option value="${t.id}" ${state.unitType === t.id ? 'selected' : ''}>${esc(t.name)}</option>`).join('')}</optgroup>`; o += `<optgroup label="Plain boxes">${Object.entries(UNIT_TYPES).filter(([k]) => !catalogue.some(t => t.id === k)).map(([k, v]) => `<option value="${k}" ${state.unitType === k ? 'selected' : ''}>${v.label}</option>`).join('')}</optgroup>`; return o; })()}</select></div>
    ${catalogue.length ? '' : `<div class="muted" style="margin-top:6px;font-size:11px">Catalogue not loaded yet — select the Unit tool once.</div>`}`;
  el.querySelectorAll<HTMLElement>('[data-lvl]').forEach(r => r.onclick = (e) => { if ((e.target as HTMLElement).closest('input,button')) return; setLevel(parseInt(r.dataset.lvl!)); });
  el.querySelectorAll<HTMLInputElement>('[data-storey-h]').forEach(i => i.onchange = () => ctx.ops([cop('update_storey', { index: parseInt(i.dataset.storeyH!), height: parseFloat(i.value) || 2.6 })], 'Storey height'));
  el.querySelectorAll<HTMLElement>('[data-storey-del]').forEach(b => b.onclick = async () => { if (await ctx.confirm('Remove this storey and everything on it?', 'Remove', true)) ctx.ops([cop('remove_storey', { index: parseInt(b.dataset.storeyDel!) })], 'Remove storey').then(() => setLevel(Math.min(state.level, ext(ctx.doc()).storeys.length - 1))); });
  el.querySelector<HTMLElement>('[data-act="addStorey"]')?.addEventListener('click', async () => { const n = x.storeys.length; const name = await ctx.prompt('Storey name', n === 1 ? 'First floor' : n === 2 ? 'Second floor' : `Storey ${n + 1}`); if (!name) return; await ctx.ops([cop('add_storey', { name })], 'Add storey'); setLevel(n); });
  el.querySelector<HTMLInputElement>('[data-act="isolate"]')?.addEventListener('change', (e) => isolate((e.target as HTMLInputElement).checked));
  el.querySelectorAll<HTMLInputElement>('[data-set]').forEach(i => i.onchange = () => ctx.ops([cop('setup', { [i.dataset.set!]: parseFloat(i.value) })], 'Construction settings'));
  el.querySelectorAll<HTMLInputElement>('[data-cav]').forEach(i => i.onchange = () => ctx.ops([cop('setup', { cavity: { ...x.cavity, [i.dataset.cav!]: i.type === 'checkbox' ? i.checked : parseFloat(i.value) } })], 'Cavity walls'));
  el.querySelectorAll<HTMLElement>('[data-select]').forEach(r => r.onclick = (e) => { if ((e.target as HTMLElement).closest('button')) return; ctx.select([r.dataset.select!]); });
  el.querySelectorAll<HTMLElement>('[data-roof-toggle]').forEach(b => b.onclick = () => { const r = customs(ctx.doc(), 'roof').find(r => r.id === b.dataset.roofToggle); if (r) ctx.ops([cop('update', { id: r.id, patch: { enabled: !(r.data as Roof).enabled } })], 'Roof'); });
  el.querySelectorAll<HTMLSelectElement>('[data-opt]').forEach(s => s.onchange = () => { (state as any)[s.dataset.opt!] = s.value; const t = (host as any).ctx?.tool; void t; host.refresh(); });
}
function setLevel(i: number) {
  state.level = i; const doc = host.document(); const st = ext(doc).storeys[i];
  if (state.isolateSection) isolate(true);
  renderFloorBar(); host.refresh();
  host.setHint(`Active storey: ${st?.name ?? i} (floor at ${base(doc, i).toFixed(2)} m). Construction tools draw here; turn on the eye to see only this storey.`);
  host.toast(`${st?.name ?? `Storey ${i + 1}`}${state.isolateSection ? ' · isolated' : ''}`);
}
async function isolate(on: boolean) {
  const doc = host.document(); const ops: Op[] = [];
  const stale = doc.entities.filter(e => e.kind === 'section' && (e as any).isolate === true).map(e => e.id);
  if (state.isolateSection && !stale.includes(state.isolateSection)) stale.push(state.isolateSection);
  if (stale.length) { ops.push({ op: 'erase', ids: stale }); state.isolateSection = null; }
  if (on) { const y = base(doc, state.level) + storeyH(doc, state.level) - 0.05; const r = await host.ops([...ops, { op: 'add_section', origin: [0, y, 0], normal: [0, 1, 0], active: true }], 'Isolate storey'); state.isolateSection = r.at(-1)?.created?.[0] ?? null; if (state.isolateSection) await host.ops([{ op: 'update_entity', id: state.isolateSection, patch: { isolate: true } }], 'Isolate storey'); }
  else if (ops.length) await host.ops(ops, 'Show all storeys');
  renderFloorBar(); host.refresh();
}
const floorBar = document.createElement('div'); floorBar.className = 'floor-bar';
function renderFloorBar() {
  const x = ext(host.document());
  floorBar.innerHTML = x.storeys.map((s, i) => `<button class="${i === state.level ? 'active' : ''}" data-lvl="${i}" title="Draw on ${esc(s.name)} (floor at ${base(host.document(), i).toFixed(2)} m)">${esc(s.name)}</button>`).join('')
    + `<span class="fb-sep"></span><button class="fb-icon ${state.isolateSection ? 'on' : ''}" data-isolate title="${state.isolateSection ? 'Show all storeys' : 'Show only the active storey (dollhouse cut)'}">${state.isolateSection ? '◉' : '◎'}</button><button class="fb-icon" data-add title="Add a storey">+</button>`;
  floorBar.querySelectorAll<HTMLElement>('[data-lvl]').forEach(b => b.onclick = () => setLevel(parseInt(b.dataset.lvl!)));
  floorBar.querySelector<HTMLElement>('[data-isolate]')!.onclick = () => isolate(!state.isolateSection);
  floorBar.querySelector<HTMLElement>('[data-add]')!.onclick = async () => { const n = x.storeys.length; const name = await host.prompt('Storey name', n === 1 ? 'First floor' : `Storey ${n + 1}`); if (!name) return; await host.ops([cop('add_storey', { name })], 'Add storey'); setLevel(n); };
}

function renderEntity(ctx: ToolContext, e: Entity, el: HTMLElement): boolean {
  if (e.kind !== 'custom') return false; const type = e.type.replace('construction.', ''); const d: any = e.data; const doc = ctx.doc();
  const sel = (k: string, cur: string | undefined, opts: [string, string][], label: string) => `<label>${label}</label><select data-f="${k}"><option value="">(default)</option>${opts.map(([v, l]) => `<option value="${v}" ${cur === `construction.${v}` || cur === v ? 'selected' : ''}>${l}</option>`).join('')}</select>`;
  const n = (k: string, v: number | null | undefined, label: string, step = 0.05) => `<label>${label}</label><input type="number" step="${step}" data-f="${k}" value="${v ?? ''}" placeholder="auto">`;
  const t = (k: string, v: string, label: string) => `<label>${label}</label><input type="text" data-f="${k}" value="${esc(v ?? '')}">`;
  const chk = (k: string, v: boolean, label: string) => `<label>${label}</label><input type="checkbox" data-f="${k}" ${v ? 'checked' : ''}>`;
  const storey = `<label>Storey</label><div>${esc(ext(doc).storeys[d.level]?.name ?? `Level ${d.level}`)}</div>`;
  let body = `<div class="kv"><label>Type</label><div><b>${({ room: 'Room', wall: 'Wall', opening: 'Opening', roof: 'Roof', fixture: 'Fixture', run: 'Run', stair: 'Stair', unit: 'Unit' } as any)[type] ?? type}</b> <span class="muted">${e.id.slice(0, 8)}</span></div>`;
  const fin = (k: string, label: string) => sel(`finishes.${k}`, d.finishes?.[k], FINISHES as [string, string][], label);
  switch (type) {
    case 'room': body += `${t('name', d.name, 'Name')}${storey}<label>Corners</label><div>${d.points?.length}</div>${n('wallHeight', d.wallHeight, 'Wall height')}${chk('hasFloor', d.hasFloor, 'Floor')}${chk('hasCeiling', d.hasCeiling, 'Ceiling')}${fin('floor', 'Floor finish')}${fin('interior', 'Inside walls')}${fin('exterior', 'Outside walls')}`; break;
    case 'wall': body += `${storey}${n('height', d.height, 'Height')}${n('thickness', d.thickness, 'Thickness', 0.01)}${chk('exterior', !!d.exterior, 'Exterior')}${fin('interior', 'Side A')}${fin('exterior', 'Side B')}`; break;
    case 'opening': body += `<label>Kind</label><select data-f="kind">${(Object.keys(OPENING) as Opening['kind'][]).map(k => `<option value="${k}" ${d.kind === k ? 'selected' : ''}>${OPENING[k].label}</option>`).join('')}</select>${n('along', d.along, 'Along wall')}${n('width', d.width, 'Width')}${n('height', d.height, 'Height')}${n('sill', d.sill, 'Sill')}`; break;
    case 'roof': body += `<label>Style</label><select data-f="style">${['flat', 'gable', 'hip', 'shed'].map(k => `<option ${d.style === k ? 'selected' : ''}>${k}</option>`).join('')}</select>${n('pitchHeight', d.pitchHeight, 'Rise to the ridge', 0.1)}${n('overhang', d.overhang, 'Eaves overhang')}${d.style === 'flat' ? '' : n('vergeOverhang', d.vergeOverhang ?? d.overhang, 'Verge overhang')}${d.style === 'flat' ? '' : `<label>Covering</label><select data-f="covering">${['tiles', 'slates', 'metal'].map(k => `<option ${(d.covering ?? 'tiles') === k ? 'selected' : ''}>${k}</option>`).join('')}</select>`}${n('rafterSpacing', d.rafterSpacing ?? 0.4, d.style === 'flat' ? 'Joist spacing' : 'Rafter spacing', 0.05)}${d.style === 'gable' || d.style === 'shed' ? n('purlinSpan', d.purlinSpan ?? 2.4, 'Max rafter span', 0.1) : ''}${d.style === 'flat' ? '' : n('battenGauge', d.battenGauge ?? 0.32, 'Batten gauge', 0.01)}${chk('ridgeAlongX', d.ridgeAlongX, 'Ridge along X')}${n('levels', d.levels, 'On storey #', 1)}${chk('enabled', d.enabled, 'Shown')}${fin('roof', 'Roof finish')}${fin('fascia', 'Fascia / soffit')}`; break;
    case 'fixture': body += `<label>Kind</label><select data-f="kind">${Object.entries(FIXTURES).map(([k, v]) => `<option value="${k}" ${d.kind === k ? 'selected' : ''}>${v.label}</option>`).join('')}</select>${storey}${n('position.1', d.position?.[1], 'Height above floor')}${t('note', d.note ?? '', 'Note')}`; break;
    case 'stair': body += `${t('name', d.name, 'Name')}${storey}<label>Kind</label><select data-f="kind"><option value="straight" ${d.kind === 'straight' ? 'selected' : ''}>Straight</option><option value="quarterTurn" ${d.kind === 'quarterTurn' ? 'selected' : ''}>Quarter turn</option></select>${n('bearing', d.bearing, 'Bearing (°)', 1)}${n('width', d.width, 'Width')}${n('going', d.going, 'Going', 0.005)}${n('risers', d.risers, 'Risers', 1)}${n('turnAfter', d.turnAfter, 'Turn after', 1)}${chk('turnsRight', !!d.turnsRight, 'Turns right')}${n('winders', d.winders, 'Winders', 1)}${chk('openRisers', !!d.openRisers, 'Open risers')}${chk('wellOpening', d.wellOpening !== false, 'Well in floor above')}`; break;
    case 'unit': {
      const ct = catalogue.find(x => x.id === d.type); if (!ct) void loadCatalogue(ctx);
      body += `${t('name', d.name, 'Name')}${storey}<label>Type</label><div>${esc(ct?.name ?? d.type)}${ct ? ' <span class="muted">· scripted</span>' : ''}</div>${n('bearing', d.bearing, 'Bearing (°)', 1)}`;
      if (ct) {
        for (const p of ct.params) {
          if (p.kind === 'derived') continue;
          const cur = p.kind === 'choice' ? (d.choices?.[p.name] ?? p.text ?? p.choices[0]) : (d.values?.[p.name] ?? p.value);
          if (p.kind === 'choice') body += `<label>${esc(p.label)}</label><select data-cp="${p.name}">${p.choices.map(c => `<option ${c === cur ? 'selected' : ''}>${esc(c)}</option>`).join('')}</select>`;
          else if (p.kind === 'flag') body += `<label>${esc(p.label)}</label><input type="checkbox" data-cp="${p.name}" data-flag ${cur ? 'checked' : ''}>`;
          else body += `<label>${esc(p.label)}</label><div class="row"><input type="number" step="0.01" data-cp="${p.name}" value="${cur}" ${p.min !== undefined ? `min="${p.min}"` : ''} ${p.max !== undefined ? `max="${p.max}"` : ''}>${p.min !== undefined && p.max !== undefined ? `<input type="range" step="0.01" min="${p.min}" max="${p.max}" value="${cur}" data-cp-range="${p.name}" style="width:70px">` : ''}</div>`;
        }
        body += `<label>Size</label><div class="muted">${(+d.width).toFixed(2)} × ${(+d.depth).toFixed(2)} × ${(+d.height).toFixed(2)} m</div>`;
      } else body += `${n('width', d.width, 'Width')}${n('depth', d.depth, 'Depth')}${n('height', d.height, 'Height')}${n('lift', d.lift, 'Lift off floor')}`;
      break; }
    case 'run': body += `<label>System</label><select data-f="system">${Object.entries(SYSTEMS).map(([k, v]) => `<option value="${k}" ${d.system === k ? 'selected' : ''}>${v.label}</option>`).join('')}</select>${storey}<label>Points</label><div>${d.points?.length}</div>${n('diameter', d.diameter, 'Diameter', 0.001)}`; break;
    default: return false;
  }
  body += `</div><div class="btn-row"><button class="btn" data-erase>Erase</button></div>`;
  el.innerHTML = body;
  el.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[data-f]').forEach(inp => inp.onchange = () => {
    const key = inp.dataset.f!; const raw = inp.type === 'checkbox' ? (inp as HTMLInputElement).checked : inp.type === 'number' ? (inp.value === '' ? null : parseFloat(inp.value)) : inp.value;
    if (key.startsWith('finishes.')) { ctx.ops([cop('set_finish', { id: e.id, face: key.slice(9), material: raw ? `construction.${raw}` : '' })], 'Finish'); return; }
    const patch: any = {}; if (key.includes('.')) { const [a, i] = key.split('.'); const arr = [...(d[a] ?? [])]; arr[+i] = raw; patch[a] = arr; } else patch[key] = raw;
    if (type === 'opening' && key === 'kind') { const o = OPENING[raw as Opening['kind']]; Object.assign(patch, { width: o.w, height: o.h, sill: o.sill }); }
    ctx.ops([cop('update', { id: e.id, patch })], 'Edit');
  });
  const applyParam = (name: string, raw: number | string | boolean) => {
    const isChoice = typeof raw === 'string'; const patch: any = isChoice ? { choices: { ...(d.choices ?? {}), [name]: raw } } : { values: { ...(d.values ?? {}), [name]: typeof raw === 'boolean' ? (raw ? 1 : 0) : raw } };
    ctx.ops([cop('update', { id: e.id, patch })], 'Component parameter');
  };
  el.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[data-cp]').forEach(inp => inp.onchange = () => applyParam(inp.dataset.cp!, inp.hasAttribute('data-flag') ? (inp as HTMLInputElement).checked : inp.tagName === 'SELECT' ? inp.value : parseFloat(inp.value)));
  el.querySelectorAll<HTMLInputElement>('[data-cp-range]').forEach(r => { r.oninput = () => { const i = el.querySelector<HTMLInputElement>(`[data-cp="${r.dataset.cpRange}"]`); if (i) i.value = r.value; }; r.onchange = () => applyParam(r.dataset.cpRange!, parseFloat(r.value)); });
  el.querySelector<HTMLElement>('[data-erase]')!.onclick = () => ctx.ops([{ op: 'erase', ids: [e.id] }], 'Erase').then(() => ctx.select([]));
  return true;
}

// ---- register ------------------------------------------------------------------------------------
const style = document.createElement('style'); style.textContent = `
.floor-bar { position: absolute; left: 10px; bottom: 12px; display: flex; gap: 2px; padding: 3px; background: var(--glass); border: 1px solid var(--glass-border); border-radius: 999px; box-shadow: var(--shadow-md); backdrop-filter: blur(10px) saturate(1.4); z-index: 5; }
.floor-bar button { border: 0; background: none; padding: 4px 11px; border-radius: 999px; cursor: default; font-weight: 500; color: var(--ink-2); }
.floor-bar button.active { background: var(--accent); color: #fff; }
.floor-bar button:hover:not(.active) { background: var(--surface-3); color: var(--ink); }
.floor-bar .fb-sep { width: 1px; height: 16px; background: var(--line-strong); margin: 0 3px; align-self: center; }
.floor-bar .fb-icon { padding: 4px 9px; font-size: 13px; }
.floor-bar .fb-icon.on { color: var(--accent); }`;
document.head.appendChild(style);

/** Vertical moves: construction elements stack on the ground level, so moving the whole house up or
 *  down changes that setting; a partial selection can only move in plan (fixtures keep their own height). */
function beforeMove(ids: string[], delta: Vec3, ctx: ToolContext): Op[] | null {
  const doc = ctx.doc(); const dy = delta[1]; if (Math.abs(dy) < 1e-6) return null;
  const isCon = (id: string) => { const e = doc.entities.find(e => e.id === id); return e?.kind === 'custom' && e.type.startsWith('construction.'); };
  const con = ids.filter(isCon); if (!con.length) return null;
  const others = ids.filter(id => !isCon(id)); const plan: Vec3 = [delta[0], 0, delta[2]]; const inPlan = Math.hypot(plan[0], plan[2]) > 1e-6;
  const all = doc.entities.filter(e => e.kind === 'custom' && e.type.startsWith('construction.')).map(e => e.id);
  const ops: Op[] = [];
  if (all.every(id => con.includes(id))) {
    const g = +(((ext(doc).groundLevel ?? 0) + dy).toFixed(4));
    ops.push(cop('setup', { groundLevel: g }));
    if (inPlan) ops.push({ op: 'move', ids: con, delta: plan });
    if (others.length) ops.push({ op: 'move', ids: others, delta });
    ctx.toast(`Whole house ${dy > 0 ? 'raised' : 'lowered'} ${Math.abs(dy * 100).toFixed(1)} cm · ground level ${g.toFixed(2)} m`);
    return ops;
  }
  // partial selection: fixtures can change height, everything else only in plan
  const fixtures = con.filter(id => (doc.entities.find(e => e.id === id) as CustomEntity).type === 'construction.fixture');
  for (const id of fixtures) { const f = doc.entities.find(e => e.id === id) as CustomEntity; const pos = (f.data as Fixture).position; ops.push(cop('update', { id, patch: { position: [pos[0] + delta[0], +(pos[1] + dy).toFixed(4), pos[2] + delta[2]] } })); }
  const rest = con.filter(id => !fixtures.includes(id));
  if (rest.length && inPlan) ops.push({ op: 'move', ids: rest, delta: plan });
  if (others.length) ops.push({ op: 'move', ids: others, delta });
  if (rest.length) ctx.toast('Rooms, walls, roofs, stairs and units sit on their storey: they only move in plan. Select the whole house (⌘A) to move it up or down, or change Ground level in the Construction panel.');
  return ops;
}

P.registerPlugin({
  id: 'construction', name: 'Construction', version: '0.1.0', accent: '#d9822b',
  // the walls and floors are masonry: the masonry wall, floor and foundation tools sit with the rooms
  tools: [roomTool, roomShapeTool, wallTool, openingTool, fixtureTool, runTool, roofTool, stairTool, unitTool, ...masonryTools],
  toolbar: [['c-room', 'c-roomshape', 'm-wall', 'c-roof'], ['c-opening', 'c-stair', 'c-unit'], ['m-floor', 'm-foundation'], ['c-fixture', 'c-run']],
  panels: [{ id: 'main', title: 'Construction', render: panel }, { id: 'masonry', title: 'Build-up & checks', render: masonryPanel }],
  renderEntity(ctx, e, el) {
    if (masonryRenderEntity(ctx, e, el)) return true;
    const ok = renderEntity(ctx, e, el);
    if (ok && e.kind === 'custom' && e.type === 'construction.room') roomMasonry(ctx, e, el);
    return ok;
  },
  beforeMove,
  activate(h) {
    host = h; masonryActivate(h); h.mount(floorBar); renderFloorBar(); void loadCatalogue(h.ctx).then(() => h.refresh());
    h.on('changed', () => { const doc = h.document(); const x = ext(doc); if (state.level >= x.storeys.length) state.level = Math.max(0, x.storeys.length - 1); const iso = doc.entities.find(e => e.kind === 'section' && (e as any).isolate === true && e.active); state.isolateSection = iso ? iso.id : (state.isolateSection && doc.entities.some(e => e.id === state.isolateSection) ? state.isolateSection : null); renderFloorBar(); });
  },
});
