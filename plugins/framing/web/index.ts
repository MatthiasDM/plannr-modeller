// Framing plugin — browser side (Medeek-style wood framing): framed walls with openings, floors, roof trusses,
// foundations, "frame this room" from the construction plugin, and a cut list. Engine: plugins/framing/engine.
import type { Tool, ToolContext, Plane } from '../../../web/src/tools/base';
import type { Plugin, PluginHost } from '../../../web/src/plugins';
import type { Document, Entity, CustomEntity, Op } from '../../../web/src/model';
import type * as THREE_NS from 'three';

type V3 = THREE_NS.Vector3;
const P = (window as any).plannr as { registerPlugin(p: Plugin): void; THREE: typeof THREE_NS };
const THREE = P.THREE;
const ACCENT = '#6d8f3a';
type Plan = [number, number];

const SIZES: [string, string][] = [['2x4', '2x4 · 38×89'], ['2x6', '2x6 · 38×140'], ['2x8', '2x8 · 38×184'], ['2x10', '2x10 · 38×235'], ['2x12', '2x12 · 38×286']];
const SPACINGS: [number, string][] = [[0.3, '300 mm · 12"'], [0.4, '400 mm · 16"'], [0.6, '600 mm · 24"']];
const ROOFS: [string, string][] = [['fink', 'Fink truss (W)'], ['howe', 'Howe truss'], ['kingPost', 'King post truss'], ['queenPost', 'Queen post truss'], ['scissor', 'Scissor truss'], ['attic', 'Attic truss (room in roof)'], ['mono', 'Mono truss (shed)'], ['rafters', 'Rafters + ridge']];
const SHAPES: [string, string][] = [['gable', 'Gable'], ['hip', 'Hip'], ['shed', 'Shed (mono-slope)'], ['gambrel', 'Gambrel']];
const CLADS: [string, string][] = [['none', 'none'], ['shingles', 'Shingles'], ['metal', 'Metal'], ['tiles', 'Tiles']];
const REFS: [string, string][] = [['exterior', 'exterior face'], ['center', 'centre line'], ['interior', 'interior face']];
const CORNERS: [string, string][] = [['california', 'California corner'], ['threeStud', '3-stud corner'], ['twoStud', '2-stud (clips)']];
const TEES: [string, string][] = [['twoStud', '2-stud channel'], ['ladder', 'Ladder blocking'], ['california', 'California T (flat stud)'], ['none', 'none']];
const JOINS: [string, string][] = [['auto', 'auto'], ['through', 'runs through'], ['butt', 'butts'], ['none', 'no join']];
const STUD_DEPTH: Record<string, number> = { '2x3': 0.064, '2x4': 0.089, '2x6': 0.14, '2x8': 0.184, '2x10': 0.235, '2x12': 0.286 };
const state = { pendingCheck: false, findingsVersion: -1, roofShape: 'gable', findings: null as any[] | null, findingsCounts: '' as string, beamSize: '2x10', beamPlies: 2, stairWidth: 0.9, reference: 'exterior' as 'exterior' | 'center' | 'interior', openingKind: 'window' as 'window' | 'door', roofType: 'fink', pitch: 6, which: 'exterior' as 'exterior' | 'all', foundation: 'slab' as 'slab' | 'stemwall' };
let host: PluginHost;
const esc = (s: unknown) => String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]!));
const fop = (type: string, data: unknown): Op => ({ op: 'custom', type: `framing.${type}`, data });
const defaults = (doc: Document): any => (doc.extensions?.framing as any)?.defaults ?? { studSize: '2x4', studSpacing: 0.4, topPlates: 2, bottomPlates: 1, sheathing: { exterior: { enabled: true, thickness: 0.011 }, interior: { enabled: false, thickness: 0.0125 } } };
const cext = (doc: Document): any => doc.extensions?.construction ?? { storeys: [{ height: 2.6 }], groundLevel: 0 };
const base = (doc: Document, level: number) => (cext(doc).groundLevel ?? 0) + (cext(doc).storeys ?? []).slice(0, level).reduce((s: number, f: any) => s + f.height, 0);
const storeyH = (doc: Document, level: number) => cext(doc).storeys?.[level]?.height ?? 2.6;
const activeLevel = () => { const fb = document.querySelector<HTMLElement>('.floor-bar button.active[data-lvl]'); return fb ? parseInt(fb.dataset.lvl!) : 0; };
const planeAt = (doc: Document, level: number): Plane => ({ origin: new THREE.Vector3(0, base(doc, level), 0), normal: new THREE.Vector3(0, 1, 0) });
const plan = (p: V3): Plan => [+p.x.toFixed(4), +p.z.toFixed(4)];
const fmtM = (n: number) => `${n.toFixed(2)} m`;
const INK = 0x1b1b1b;
const customs = (doc: Document, type: string): CustomEntity[] => doc.entities.filter((e): e is CustomEntity => e.kind === 'custom' && e.type === type);
// the app draws the snap marker of every ctx.snap() a tool makes
const drawSnapDot = (_ctx: ToolContext, _s: { kind: string; point: V3 }) => {};
const num = (v: string) => v.split(/[;,x\s]+/).map(parseFloat).filter(n => !isNaN(n));

/** Two-click rectangle on the active storey; used by floor, roof and foundation tools. */
function rectTool(id: string, label: string, hint: string, icon: string, onRect: (ctx: ToolContext, a: V3, b: V3, level: number) => Promise<void>, atTop = false): Tool & { first?: V3 | null } {
  return {
    id, label, hint, cursor: 'crosshair', icon,
    busy() { return !!this.first; },
    activate(ctx) { this.first = null; ctx.setHint(hint); ctx.setVCB('', 'Dimensions'); }, deactivate(ctx) { this.first = null; ctx.scene.clearOverlay(); },
    onMove(ev, ctx) {
      const doc = ctx.doc(); const lvl = activeLevel(); const y = base(doc, lvl) + (atTop ? storeyH(doc, lvl) : 0);
      const s = ctx.snap(ev, { plane: { origin: new THREE.Vector3(0, y, 0), normal: new THREE.Vector3(0, 1, 0) }, faces: false }); ctx.scene.clearOverlay(); drawSnapDot(ctx, s);
      if (!this.first) return; const a = this.first, b = s.point; const pts = [new THREE.Vector3(a.x, y, a.z), new THREE.Vector3(b.x, y, a.z), new THREE.Vector3(b.x, y, b.z), new THREE.Vector3(a.x, y, b.z)];
      ctx.scene.addLine([...pts, pts[0]], INK, false, 2); ctx.scene.addFace(pts, 0x6d8f3a, 0.2); ctx.scene.addLabel(pts[0].clone().lerp(pts[1], 0.5), fmtM(Math.abs(b.x - a.x))); ctx.scene.addLabel(pts[1].clone().lerp(pts[2], 0.5), fmtM(Math.abs(b.z - a.z)));
      ctx.setVCB(`${Math.abs(b.x - a.x).toFixed(2)}; ${Math.abs(b.z - a.z).toFixed(2)}`, 'Dimensions');
    },
    async onDown(ev, ctx) {
      if (ev.button !== 0) return; const doc = ctx.doc(); const lvl = activeLevel(); const y = base(doc, lvl) + (atTop ? storeyH(doc, lvl) : 0);
      const s = ctx.snap(ev, { plane: { origin: new THREE.Vector3(0, y, 0), normal: new THREE.Vector3(0, 1, 0) }, faces: false });
      if (!this.first) { this.first = s.point.clone(); ctx.setHint('Click the opposite corner, or type width; depth.'); return; }
      const a = this.first; this.first = null; ctx.scene.clearOverlay(); if (Math.abs(s.point.x - a.x) < 0.3 || Math.abs(s.point.z - a.z) < 0.3) { ctx.toast('Too small'); return; }
      await onRect(ctx, a, s.point, lvl); ctx.setHint(hint);
    },
    onVCB(v, ctx) { const n = num(v); if (n.length < 2 || !this.first) return; const a = this.first; this.first = null; ctx.scene.clearOverlay(); void onRect(ctx, a, new THREE.Vector3(a.x + n[0], a.y, a.z + n[1]), activeLevel()); },
    onKey(ev, ctx) { if (ev.key === 'Escape' && this.first) { this.first = null; ctx.scene.clearOverlay(); return true; } return false; },
  };
}
const rectData = (a: V3, b: V3) => ({ origin: [Math.min(a.x, b.x), Math.min(a.z, b.z)] as Plan, size: [Math.abs(b.x - a.x), Math.abs(b.z - a.z)] as Plan, min: [Math.min(a.x, b.x), Math.min(a.z, b.z)] as Plan, max: [Math.max(a.x, b.x), Math.max(a.z, b.z)] as Plan });

const wallTool: Tool & { start?: V3 | null; cur?: V3 | null } = {
  id: 'f-wall', label: 'Framed wall', hint: 'Click along the outside face of the wall, going clockwise (the exterior is on the left, green line); the chain continues until Esc and corners join automatically. Type a length.', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"><path d="M3 5h18M3 19h18" stroke-width="2.4" stroke="#8c6d3f"/><path d="M6 5v14M10 5v14M14 5v14M18 5v14" stroke="#c9a46a" stroke-width="2"/></svg>`,
  busy() { return !!this.start; },
  activate(ctx) { this.start = null; ctx.setHint(this.hint); ctx.setVCB('', 'Length'); }, deactivate(ctx) { this.start = null; ctx.scene.clearOverlay(); },
  onMove(ev, ctx) {
    const doc = ctx.doc(); const s = ctx.snap(ev, { plane: planeAt(doc, activeLevel()), faces: false, from: this.start ?? undefined, lock: ctx.lock }); ctx.scene.clearOverlay(); drawSnapDot(ctx, s);
    if (!this.start) return; this.cur = s.point; const d = s.point.clone().sub(this.start).normalize();
    // the body lies inside the drawn line for `exterior`, around it for `center`, outside it for `interior`; exterior = left of the direction (draw clockwise)
    const depth = STUD_DEPTH[defaults(doc).studSize] ?? 0.089; const inward = new THREE.Vector3(-d.z, 0, d.x);
    const [lo, hi] = state.reference === 'exterior' ? [0, depth] : state.reference === 'interior' ? [-depth, 0] : [-depth / 2, depth / 2];
    const at = (p: V3, o: number) => p.clone().addScaledVector(inward, o);
    ctx.scene.addFace([at(this.start, lo), at(s.point, lo), at(s.point, hi), at(this.start, hi)], 0xc9a46a, 0.35);
    ctx.scene.addLine([at(this.start, lo), at(s.point, lo), at(s.point, hi), at(this.start, hi), at(this.start, lo)], INK, false, 2);
    ctx.scene.addLine([at(this.start, lo - 0.011), at(s.point, lo - 0.011)], 0x6d8f3a, false, 3);
    const L = this.start.distanceTo(s.point); ctx.scene.addLabel(this.start.clone().lerp(s.point, 0.5), `${fmtM(L)} · ${Math.ceil(L / defaults(doc).studSpacing) + 1} studs`); ctx.setVCB(L.toFixed(2), 'Length');
  },
  onDown(ev, ctx) {
    if (ev.button !== 0) return; const s = ctx.snap(ev, { plane: planeAt(ctx.doc(), activeLevel()), faces: false, from: this.start ?? undefined, lock: ctx.lock });
    if (!this.start) { this.start = s.point.clone(); return; } void placeWall(this, ctx, s.point);
  },
  onVCB(v, ctx) { const n = num(v); if (!n.length || !this.start || !this.cur) return; const d = this.cur.clone().sub(this.start).normalize(); void placeWall(this, ctx, this.start.clone().addScaledVector(d, n[0])); },
  onDblClick(_e, ctx) { this.start = null; ctx.scene.clearOverlay(); },
  onKey(ev, ctx) { if (ev.key === 'Escape') { this.start = null; ctx.scene.clearOverlay(); return true; } return false; },
};
async function placeWall(t: typeof wallTool, ctx: ToolContext, end: V3) {
  if (!t.start || t.start.distanceTo(end) < 0.1) return; const r = await ctx.ops([fop('add_wall', { level: activeLevel(), a: plan(t.start), b: plan(end), reference: state.reference })], 'Framed wall'); t.start = end.clone(); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]);
}

const openingTool: Tool = {
  id: 'f-opening', label: 'Rough opening', hint: 'Hover a framed wall and click to add a window or door rough opening (king and jack studs, header, sill, cripples). Type width; height.', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"><path d="M3 4h18M3 20h18" stroke="#8c6d3f" stroke-width="2.2"/><path d="M6 4v16M18 4v16M8 4v16M16 4v16" stroke="#c9a46a"/><path d="M8 8h8" stroke="#8c6d3f" stroke-width="3"/><path d="M8 16h8" stroke="#c9a46a" stroke-width="2"/></svg>`,
  activate(ctx) { ctx.setHint(this.hint); ctx.setVCB(state.openingKind === 'door' ? '0.9; 2.1' : '1.2; 1.2', 'Width; Height'); }, deactivate(ctx) { ctx.scene.clearOverlay(); },
  onMove(ev, ctx) { ctx.scene.clearOverlay(); const h = hitWall(ev, ctx); if (!h) return; const d = dims(); const y0 = base(ctx.doc(), (h.wall.data as any).level) + (state.openingKind === 'door' ? 0 : 0.9); const c = new THREE.Vector3(h.pt[0], 0, h.pt[1]); const l = c.clone().addScaledVector(h.dir, -d.w / 2), r = c.clone().addScaledVector(h.dir, d.w / 2); ctx.scene.addLine([l.clone().setY(y0), r.clone().setY(y0), r.clone().setY(y0 + d.h), l.clone().setY(y0 + d.h), l.clone().setY(y0)], 0xc8169a, false, 2); ctx.scene.addLabel(c.clone().setY(y0 + d.h), `${state.openingKind} ${d.w}×${d.h} · offset ${fmtM(h.along - d.w / 2)}`); },
  async onDown(ev, ctx) { if (ev.button !== 0) return; const h = hitWall(ev, ctx); if (!h) { ctx.toast('Hover a framed wall'); return; } const d = dims(); await ctx.ops([fop('add_opening', { wall: h.wall.id, kind: state.openingKind, offset: +Math.max(0, h.along - d.w / 2).toFixed(3), width: d.w, height: d.h, sill: state.openingKind === 'door' ? 0 : 0.9 })], 'Rough opening'); },
  onVCB(v, ctx) { const n = num(v); if (n.length) { OPEN[state.openingKind] = { w: n[0], h: n[1] ?? OPEN[state.openingKind].h }; ctx.setVCB(`${n[0]}; ${n[1] ?? OPEN[state.openingKind].h}`, 'Width; Height'); } },
};
const OPEN: Record<string, { w: number; h: number }> = { window: { w: 1.2, h: 1.2 }, door: { w: 0.9, h: 2.1 } };
const dims = () => OPEN[state.openingKind];
function hitWall(ev: PointerEvent, ctx: ToolContext) {
  const doc = ctx.doc(); const pk = ctx.scene.pick(ev); const id = pk ? ctx.resolve(pk.id) : null; let wall = id ? customs(doc, 'framing.wall').find(w => w.id === id) : undefined;
  const g = ctx.scene.groundPoint(ev, base(doc, activeLevel())); const p: Plan | null = pk && wall ? [pk.point.x, pk.point.z] : g ? [g.x, g.z] : null; if (!p) return null;
  let best: { wall: CustomEntity; t: number; d: number } | null = null;
  for (const w of customs(doc, 'framing.wall')) { const a = (w.data as any).a as Plan, b = (w.data as any).b as Plan; const dx = b[0] - a[0], dz = b[1] - a[1]; const L2 = dx * dx + dz * dz || 1e-9; let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / L2; t = Math.max(0, Math.min(1, t)); const d = Math.hypot(a[0] + dx * t - p[0], a[1] + dz * t - p[1]); if (d < 0.6 && (!best || d < best.d)) best = { wall: w, t, d }; }
  if (!best) return null; const a = (best.wall.data as any).a as Plan, b = (best.wall.data as any).b as Plan; const L = Math.hypot(b[0] - a[0], b[1] - a[1]); const dir = new THREE.Vector3(b[0] - a[0], 0, b[1] - a[1]).normalize();
  return { wall: best.wall, along: best.t * L, pt: [a[0] + (b[0] - a[0]) * best.t, a[1] + (b[1] - a[1]) * best.t] as Plan, dir, L };
}

const floorTool = rectTool('f-floor', 'Floor framing', 'Click two corners of the floor: joists, rim joists and subfloor per the Framing defaults.', `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="3" y="5" width="18" height="14" stroke="#8c6d3f" stroke-width="2"/><path d="M7 5v14M11 5v14M15 5v14M19 5v14" stroke="#c9a46a"/></svg>`, async (ctx, a, b, level) => { const d = rectData(a, b); const r = await ctx.ops([fop('add_floor', { level, origin: d.origin, size: d.size })], 'Floor framing'); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]); });
const roofTool = rectTool('f-roof', 'Roof', 'Click two corners of the roof footprint on top of the active storey: a gable, hip, shed or gambrel roof framed with the chosen trusses or rafters at the chosen pitch.', `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"><path d="M2 18 12 6l10 12z" stroke="#8c6d3f" stroke-width="2"/><path d="M7 18l5-6 5 6M12 6v12" stroke="#c9a46a"/></svg>`, async (ctx, a, b, level) => { const d = rectData(a, b); const r = await ctx.ops([fop('add_roof', { level, min: d.min, max: d.max, shape: state.roofShape, type: state.roofShape === 'shed' && !['rafters', 'mono'].includes(state.roofType) ? 'mono' : state.roofType === 'mono' && state.roofShape !== 'shed' ? 'fink' : state.roofType, pitch: state.pitch, ridgeAlongX: d.size[0] >= d.size[1] })], 'Roof trusses'); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]); }, true);
const foundationTool = rectTool('f-foundation', 'Foundation', 'Click two corners of the foundation: a slab or a stem wall on footings.', `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"><path d="M3 10h18v4H3z" fill="#bdb8ae"/><path d="M5 14v5h14v-5" fill="#a9a6a0"/><path d="M2 19h20"/></svg>`, async (ctx, a, b) => { const d = rectData(a, b); const pts: Plan[] = [d.min, [d.max[0], d.min[1]], d.max, [d.min[0], d.max[1]]]; const r = await ctx.ops([fop('add_foundation', { points: pts, type: state.foundation })], 'Foundation'); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]); });

/** Two clicks on the active storey: a line (beam, stair run). */
function lineTool(id: string, label: string, hint: string, icon: string, preview: (ctx: ToolContext, a: V3, b: V3) => string, onLine: (ctx: ToolContext, a: V3, b: V3, level: number) => Promise<void>): Tool & { first?: V3 | null; cur?: V3 | null } {
  return {
    id, label, hint, cursor: 'crosshair', icon,
    busy() { return !!this.first; },
    activate(ctx) { this.first = null; ctx.setHint(hint); ctx.setVCB('', 'Length'); }, deactivate(ctx) { this.first = null; ctx.scene.clearOverlay(); },
    onMove(ev, ctx) {
      const s = ctx.snap(ev, { plane: planeAt(ctx.doc(), activeLevel()), faces: false, from: this.first ?? undefined, lock: ctx.lock }); ctx.scene.clearOverlay(); drawSnapDot(ctx, s);
      if (!this.first) return; this.cur = s.point; ctx.scene.addLine([this.first, s.point], 0x6d8f3a, false, 4); ctx.scene.addLabel(this.first.clone().lerp(s.point, 0.5), preview(ctx, this.first, s.point)); ctx.setVCB(this.first.distanceTo(s.point).toFixed(2), 'Length');
    },
    async onDown(ev, ctx) {
      if (ev.button !== 0) return; const s = ctx.snap(ev, { plane: planeAt(ctx.doc(), activeLevel()), faces: false, from: this.first ?? undefined, lock: ctx.lock });
      if (!this.first) { this.first = s.point.clone(); return; }
      const a = this.first; this.first = null; ctx.scene.clearOverlay(); if (a.distanceTo(s.point) < 0.2) { ctx.toast('Too short'); return; } await onLine(ctx, a, s.point, activeLevel()); ctx.setHint(hint);
    },
    onVCB(v, ctx) { const n = num(v); if (!n.length || !this.first || !this.cur) return; const a = this.first; const d = this.cur.clone().sub(a).normalize(); this.first = null; ctx.scene.clearOverlay(); void onLine(ctx, a, a.clone().addScaledVector(d, n[0]), activeLevel()); },
    onKey(ev, ctx) { if (ev.key === 'Escape' && this.first) { this.first = null; ctx.scene.clearOverlay(); return true; } return false; },
  };
}
const beamTool = lineTool('f-beam', 'Beam', 'Click both ends of a beam (built-up, LVL or glulam per the panel), with posts under it.', `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"><path d="M2 7h20v4H2z" fill="#c9a46a" stroke="#8c6d3f"/><path d="M5 11v9M19 11v9" stroke="#8c6d3f" stroke-width="2.4"/></svg>`,
  (_c, a, b) => `${fmtM(a.distanceTo(b))} · ${state.beamPlies}× ${state.beamSize}`,
  async (ctx, a, b, level) => { const r = await ctx.ops([fop('add_beam', { level, a: plan(a), b: plan(b), size: state.beamSize, plies: state.beamPlies })], 'Beam'); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]); });
const stairTool = lineTool('f-stair', 'Stair', 'Click the foot of the stair (middle of the first riser), then a point in the direction it climbs. Risers, treads and stringers follow the storey height.', `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"><path d="M3 20h4v-4h4v-4h4V8h4V4h2" stroke="#8c6d3f" stroke-width="2"/><path d="M3 20 21 4" stroke="#c9a46a"/></svg>`,
  (ctx) => { const h = storeyH(ctx.doc(), activeLevel()); const n = Math.ceil(h / 0.18); return `${n} risers × ${(h / n * 1000).toFixed(0)} mm · run ${fmtM((n - 1) * 0.26)}`; },
  async (ctx, a, b, level) => { const d = b.clone().sub(a).normalize(); const r = await ctx.ops([fop('add_stair', { level, a: plan(a), direction: [+d.x.toFixed(4), +d.z.toFixed(4)], width: state.stairWidth })], 'Stair'); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]); });

/** An SVG returned by the engine, in a dialog with a download button. */
function showSvg(title: string, svg: string, file: string) {
  const el = document.createElement('div'); el.className = 'dlg-host';
  el.innerHTML = `<div class="dlg" style="width:min(96vw,1100px);max-height:92vh"><div class="dlg-title" style="display:flex;align-items:center;gap:10px">${esc(title)}<span style="flex:1"></span><button class="btn" data-dl>Download SVG</button><button class="btn" data-close>Close</button></div><div class="svg-view" style="overflow:auto;max-height:80vh;background:#fff;border-radius:8px;padding:8px">${svg}</div></div>`;
  document.body.appendChild(el); const close = () => el.remove(); el.querySelector<HTMLElement>('[data-close]')!.onclick = close; el.addEventListener('pointerdown', e => { if (e.target === el) close(); });
  const v = el.querySelector<SVGElement>('svg'); if (v) { v.style.maxWidth = '100%'; v.style.height = 'auto'; }
  el.querySelector<HTMLElement>('[data-dl]')!.onclick = () => { const a = document.createElement('a'); a.href = URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml' })); a.download = file; a.click(); };
}
async function runCheck(ctx: ToolContext) {
  state.pendingCheck = true; // the check itself bumps the document version: that change must not clear its own result
  const r = await ctx.ops([fop('check', {})], 'Framing check'); const d: any = r[0]?.entities?.[0]?.data;
  setTimeout(() => { state.pendingCheck = false; state.findingsVersion = host.document().version; }, 800);
  if (!d) return;
  state.findings = d.findings ?? []; state.findingsVersion = ctx.doc().version; state.findingsCounts = `${d.errors ?? 0} errors · ${d.warnings ?? 0} warnings · ${d.species ?? ''}`; host.refresh();
}
async function showSpanTable(ctx: ToolContext) {
  const d = defaults(ctx.doc()); const rows: string[] = [];
  for (const use of ['floor', 'roof']) for (const sp of [0.3, 0.4, 0.6]) {
    const r = await ctx.ops([fop('span_table', { use, spacing: sp, pitch: state.pitch })], 'Span table'); const t: any = r[0]?.entities?.[0]?.data;
    rows.push(`<tr><td>${use === 'floor' ? 'Floor joists' : `Rafters ${state.pitch}/12`}</td><td>${sp * 1000} mm</td>${(t?.spans ?? []).map((x: any) => `<td title="governed by ${esc(x.governs)}">${x.span_m.toFixed(2)} m</td>`).join('')}</tr>`);
    if (!rows.length) break;
  }
  const r0 = await ctx.ops([fop('span_table', { use: 'floor', spacing: 0.4 })], 'Span table'); const sizes = ((r0[0]?.entities?.[0]?.data as any)?.spans ?? []).map((x: any) => x.size);
  const el = document.createElement('div'); el.className = 'dlg-host';
  el.innerHTML = `<div class="dlg" style="width:min(96vw,760px)"><div class="dlg-title" style="display:flex">Allowable spans · ${esc(d.species ?? 'C24')}<span style="flex:1"></span><button class="btn" data-close>Close</button></div><div class="muted">Simply supported, uniform load (floor ${d.loads?.floorLive ?? 2} + ${d.loads?.floorDead ?? 0.5} kN/m², roof snow ${d.loads?.roofSnow ?? 0.8} + ${d.loads?.roofDead ?? 0.6} kN/m²); bending, shear and deflection. A design aid, not a structural calculation.</div><table class="data" style="margin-top:8px"><tr><th>Member</th><th>Spacing</th>${sizes.map((x: string) => `<th>${esc(x)}</th>`).join('')}</tr>${rows.join('')}</table></div>`;
  document.body.appendChild(el); const close = () => el.remove(); el.querySelector<HTMLElement>('[data-close]')!.onclick = close; el.addEventListener('pointerdown', e => { if (e.target === el) close(); });
}

const frameRoomTool: Tool = {
  id: 'f-frameroom', label: 'Frame room', hint: 'Click a room from the construction plugin: its walls (exterior, or all) are framed with the openings it has.', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"><path d="M4 6h16v12H4z" fill="#e8f0fb"/><path d="M4 6v12M20 6v12" stroke="#8c6d3f" stroke-width="2.4"/><path d="M7 6v12M10 6v12M14 6v12M17 6v12" stroke="#c9a46a"/></svg>`,
  activate(ctx) { ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }, deactivate(ctx) { ctx.scene.highlight(new Set(), true); },
  onMove(ev, ctx) { const pk = ctx.scene.pick(ev, { facesOnly: true }); const id = pk ? ctx.resolve(pk.id) : ''; const room = customs(ctx.doc(), 'construction.room').find(r => r.id === id); ctx.scene.highlight(room ? new Set([room.id]) : new Set(), true); },
  async onDown(ev, ctx) { if (ev.button !== 0) return; const pk = ctx.scene.pick(ev, { facesOnly: true }); const id = pk ? ctx.resolve(pk.id) : ''; const room = customs(ctx.doc(), 'construction.room').find(r => r.id === id); if (!room) { ctx.toast('Click a construction room'); return; } const r = await ctx.ops([fop('frame_room', { room: room.id, which: state.which })], 'Frame room'); if (r[0]?.created?.length) { ctx.select(r[0].created); ctx.toast(`${r[0].created.length} framed walls`); } },
};

// ---- panel + editors -----------------------------------------------------------------------
function panel(ctx: ToolContext, el: HTMLElement) {
  const doc = ctx.doc(); const d = defaults(doc); const n = { walls: customs(doc, 'framing.wall').length, floors: customs(doc, 'framing.floor').length, roofs: customs(doc, 'framing.roof').length, found: customs(doc, 'framing.foundation').length };
  el.innerHTML = `
    <div class="muted" style="margin:4px 0 6px;font-size:11px">Defaults for new walls</div>
    <div class="kv"><label>Studs</label><select data-d="studSize">${SIZES.map(([k, l]) => `<option value="${k}" ${d.studSize === k ? 'selected' : ''}>${l}</option>`).join('')}</select>
      <label>Spacing</label><select data-d="studSpacing">${SPACINGS.map(([k, l]) => `<option value="${k}" ${Math.abs(d.studSpacing - k) < 1e-6 ? 'selected' : ''}>${l}</option>`).join('')}</select>
      <label>Top plates</label><select data-d="topPlates"><option ${d.topPlates === 1 ? 'selected' : ''}>1</option><option ${d.topPlates === 2 ? 'selected' : ''}>2</option></select>
      <label>Ext. sheathing</label><input type="checkbox" data-sh="exterior" ${d.sheathing?.exterior?.enabled ? 'checked' : ''}><label>Int. gypsum</label><input type="checkbox" data-sh="interior" ${d.sheathing?.interior?.enabled ? 'checked' : ''}>
      <label>Corners</label><select data-d="cornerDetail">${CORNERS.map(([k, l]) => `<option value="${k}" ${(d.cornerDetail ?? 'california') === k ? 'selected' : ''}>${l}</option>`).join('')}</select>
      <label>T-junctions</label><select data-d="tDetail">${TEES.map(([k, l]) => `<option value="${k}" ${(d.tDetail ?? 'twoStud') === k ? 'selected' : ''}>${l}</option>`).join('')}</select></div>
    <div class="muted" style="margin:12px 0 6px;font-size:11px">Tool options</div>
    <div class="kv"><label>Draw walls along</label><select data-s="reference">${REFS.map(([k, l]) => `<option value="${k}" ${state.reference === k ? 'selected' : ''}>${l}</option>`).join('')}</select>
      <label>Opening</label><select data-s="openingKind"><option value="window" ${state.openingKind === 'window' ? 'selected' : ''}>Window</option><option value="door" ${state.openingKind === 'door' ? 'selected' : ''}>Door</option></select>
      <label>Roof shape</label><select data-s="roofShape">${SHAPES.map(([k, l]) => `<option value="${k}" ${state.roofShape === k ? 'selected' : ''}>${l}</option>`).join('')}</select>
      <label>Roof framing</label><select data-s="roofType">${ROOFS.map(([k, l]) => `<option value="${k}" ${state.roofType === k ? 'selected' : ''}>${l}</option>`).join('')}</select>
      <label>Pitch</label><div class="row"><input type="number" step="0.5" min="1" max="24" data-s="pitch" value="${state.pitch}"><span class="muted" style="align-self:center">/12 · ${(Math.atan(state.pitch / 12) * 180 / Math.PI).toFixed(1)}°</span></div>
      <label>Foundation</label><select data-s="foundation"><option value="slab" ${state.foundation === 'slab' ? 'selected' : ''}>Slab</option><option value="stemwall" ${state.foundation === 'stemwall' ? 'selected' : ''}>Stem wall + footing</option></select>
      <label>Frame room</label><select data-s="which"><option value="exterior" ${state.which === 'exterior' ? 'selected' : ''}>exterior walls</option><option value="all" ${state.which === 'all' ? 'selected' : ''}>all walls</option></select></div>
    <div class="muted" style="margin:12px 0 6px;font-size:11px">Beams &amp; stairs</div>
    <div class="kv"><label>Beam</label><div class="row"><select data-s="beamPlies">${[1, 2, 3, 4].map(k => `<option value="${k}" ${state.beamPlies === k ? 'selected' : ''}>${k}×</option>`).join('')}</select><select data-s="beamSize">${SIZES.map(([k, l]) => `<option value="${k}" ${state.beamSize === k ? 'selected' : ''}>${l}</option>`).join('')}</select></div>
      <label>Stair width</label><input type="number" step="0.05" data-s="stairWidth" value="${state.stairWidth}"></div>
    <div class="muted" style="margin:12px 0 6px;font-size:11px">Engineering &amp; hardware</div>
    <div class="kv"><label>Species</label><select data-d="species">${[['C18', 'C18'], ['C24', 'C24 (EN 338)'], ['C30', 'C30'], ['SPF2', 'SPF No.2'], ['DFL2', 'DF-L No.2']].map(([k, l]) => `<option value="${k}" ${(d.species ?? 'C24') === k ? 'selected' : ''}>${l}</option>`).join('')}</select>
      <label>Snow load</label><input type="number" step="0.1" data-load="roofSnow" value="${d.loads?.roofSnow ?? 0.8}">
      <label>Floor live load</label><input type="number" step="0.1" data-load="floorLive" value="${d.loads?.floorLive ?? 2}">
      <label>Anchor bolts</label><input type="checkbox" data-hw="anchorBolts" ${d.hardware?.anchorBolts !== false ? 'checked' : ''}><label>Hurricane ties</label><input type="checkbox" data-hw="hurricaneTies" ${d.hardware?.hurricaneTies !== false ? 'checked' : ''}>
      <label>Hold-downs</label><input type="checkbox" data-hw="holdDowns" ${d.hardware?.holdDowns ? 'checked' : ''}><label>Joist hangers</label><input type="checkbox" data-hw="joistHangers" ${d.hardware?.joistHangers !== false ? 'checked' : ''}></div>
    <div class="btn-row"><button class="btn" data-act="check">Check</button><button class="btn" data-act="spans">Span table</button><button class="btn" data-act="plan">Framing plan</button></div>
    ${state.findings ? `<div class="muted" style="margin:6px 0 4px;font-size:11px">${esc(state.findingsCounts)}</div>` + (state.findings.length ? state.findings.map((f: any) => `<div class="list-row" ${f.element ? `data-el="${esc(f.element)}"` : ''} title="${esc(f.detail ?? '')}"><span class="swatch" style="background:${({ error: '#d93025', warning: '#f2b705', info: '#5a93f0', ok: '#1e8e3e' } as any)[f.severity] ?? '#999'}"></span><span class="name">${esc(f.title)}${f.value != null && f.limit != null ? ` <span class="muted">${typeof f.value === 'number' ? f.value.toFixed(2) : esc(f.value)} / ${typeof f.limit === 'number' ? f.limit.toFixed(2) : esc(f.limit)}</span>` : ''}</span></div>`).join('') : '<div class="empty">No findings.</div>') : ''}
    <div class="muted" style="margin:12px 0 6px;font-size:11px">In this model</div>
    <div class="muted">${n.walls} framed walls · ${n.floors} floors · ${n.roofs} roofs · ${n.found} foundations · ${customs(doc, 'framing.beam').length} beams · ${customs(doc, 'framing.stair').length} stairs</div>
    <div class="btn-row"><button class="btn" data-act="frameLevel">Frame whole storey</button><button class="btn primary" data-act="cutlist">Cut list</button></div>`;
  el.querySelectorAll<HTMLSelectElement>('[data-d]').forEach(s => s.onchange = () => ctx.ops([fop('set_defaults', { [s.dataset.d!]: ['studSpacing'].includes(s.dataset.d!) ? parseFloat(s.value) : ['topPlates'].includes(s.dataset.d!) ? parseInt(s.value) : s.value })], 'Framing defaults'));
  el.querySelectorAll<HTMLInputElement>('[data-sh]').forEach(i => i.onchange = () => ctx.ops([fop('set_defaults', { sheathing: { [i.dataset.sh!]: { enabled: i.checked } } })], 'Framing defaults'));
  el.querySelectorAll<HTMLSelectElement | HTMLInputElement>('[data-s]').forEach(i => i.onchange = () => { const k = i.dataset.s!; (state as any)[k] = ['pitch', 'stairWidth'].includes(k) ? parseFloat(i.value) : k === 'beamPlies' ? parseInt(i.value) : i.value; host.refresh(); });
  el.querySelectorAll<HTMLInputElement>('[data-load]').forEach(i => i.onchange = () => ctx.ops([fop('set_defaults', { loads: { [i.dataset.load!]: parseFloat(i.value) } })], 'Framing loads'));
  el.querySelectorAll<HTMLInputElement>('[data-hw]').forEach(i => i.onchange = () => ctx.ops([fop('set_defaults', { hardware: { [i.dataset.hw!]: i.checked } })], 'Framing hardware'));
  el.querySelector<HTMLElement>('[data-act="check"]')?.addEventListener('click', () => runCheck(ctx));
  el.querySelector<HTMLElement>('[data-act="spans"]')?.addEventListener('click', () => showSpanTable(ctx));
  el.querySelector<HTMLElement>('[data-act="plan"]')?.addEventListener('click', async () => { const lvl = activeLevel(); const r = await ctx.ops([fop('plan_svg', { level: lvl })], 'Framing plan'); const svg = (r[0]?.entities?.[0]?.data as any)?.svg; if (svg) showSvg(`Framing plan · storey ${lvl + 1}`, svg, `framing-plan-${lvl + 1}.svg`); });
  el.querySelectorAll<HTMLElement>('[data-el]').forEach(r => r.onclick = () => ctx.select([r.dataset.el!]));
  el.querySelector<HTMLElement>('[data-act="frameLevel"]')?.addEventListener('click', async () => { const lvl = activeLevel(); if (!(await ctx.confirm(`Frame every room on storey ${lvl + 1} (${state.which} walls)?`, 'Frame'))) return; const r = await ctx.ops([fop('frame_level', { level: lvl, which: state.which })], 'Frame storey'); ctx.toast(`${r[0]?.created?.length ?? 0} framed walls`); });
  el.querySelector<HTMLElement>('[data-act="cutlist"]')?.addEventListener('click', () => showCutList(ctx));
}
async function showCutList(ctx: ToolContext) {
  const sel = [...ctx.selection()].filter(id => customs(ctx.doc(), 'framing.wall').concat(customs(ctx.doc(), 'framing.floor'), customs(ctx.doc(), 'framing.roof'), customs(ctx.doc(), 'framing.foundation'), customs(ctx.doc(), 'framing.beam'), customs(ctx.doc(), 'framing.stair')).some(e => e.id === id));
  const r = await ctx.ops([fop('estimate', sel.length ? { ids: sel } : {})], 'Cut list'); const est: any = r[0]?.entities?.[0]?.data; if (!est) { ctx.toast('No estimate returned'); return; }
  const rows = (est.items ?? []).map((it: any) => `<tr><td>${esc(it.category)}</td><td>${esc(it.description)}</td><td>${it.count ?? ''}</td><td>${it.stock ?? ''}</td><td>${it.length_m != null ? it.length_m.toFixed(2) : ''}</td><td>${it.area_m2 != null ? it.area_m2.toFixed(2) : it.volume_m3 != null ? it.volume_m3.toFixed(2) + ' m³' : ''}</td></tr>`).join('');
  const t = est.totals ?? {};
  const el = document.createElement('div'); el.className = 'dlg-host';
  el.innerHTML = `<div class="dlg" style="width:min(96vw,900px);max-height:90vh"><div class="dlg-title" style="display:flex;align-items:center;gap:10px">Cut list · material takeoff ${sel.length ? `(${sel.length} selected)` : '(whole model)'}<span style="flex:1"></span><button class="btn" data-csv>Download CSV</button><button class="btn" data-close>Close</button></div>
    <div class="muted">${t.lumber_pieces ?? 0} pieces · ${(t.lumber_m ?? 0).toFixed(1)} m lumber · ${t.sheathing_panels ?? 0} sheathing panels · ${t.gypsum_panels ?? 0} gypsum panels · ${(t.concrete_m3 ?? 0).toFixed(2)} m³ concrete${t.hardware_pieces ? ` · ${t.hardware_pieces} hardware pieces` : ''}${t.cladding_m2 ? ` · ${t.cladding_m2.toFixed(1)} m² cladding` : ''}${t.gutters_m ? ` · ${t.gutters_m.toFixed(1)} m gutters` : ''}</div>
    <div style="overflow:auto;max-height:70vh"><table class="data" style="margin-top:6px"><tr><th>Category</th><th style="text-align:left">Item</th><th>Count</th><th>Stock</th><th>Length (m)</th><th>Area / volume</th></tr>${rows}</table></div></div>`;
  document.body.appendChild(el); const close = () => el.remove(); el.querySelector<HTMLElement>('[data-close]')!.onclick = close; el.addEventListener('pointerdown', e => { if (e.target === el) close(); });
  el.querySelector<HTMLElement>('[data-csv]')!.onclick = () => { const csv = ['category,item,count,stock,length_m,area_m2,volume_m3', ...(est.items ?? []).map((it: any) => [it.category, `"${it.description}"`, it.count ?? '', it.stock ?? '', it.length_m ?? '', it.area_m2 ?? '', it.volume_m3 ?? ''].join(','))].join('\n'); const a = document.createElement('a'); a.href = URL.createObjectURL(new Blob([csv], { type: 'text/csv' })); a.download = 'cutlist.csv'; a.click(); };
}

function renderEntity(ctx: ToolContext, e: Entity, el: HTMLElement): boolean {
  if (e.kind !== 'custom' || !e.type.startsWith('framing.')) return false; const type = e.type.slice(8); const d: any = e.data;
  const n = (k: string, v: number | null | undefined, label: string, step = 0.01) => `<label>${label}</label><input type="number" step="${step}" data-f="${k}" value="${v ?? ''}" placeholder="auto">`;
  const sel = (k: string, cur: string | undefined, opts: [string, string][], label: string) => `<label>${label}</label><select data-f="${k}">${opts.map(([v, l]) => `<option value="${v}" ${cur === v ? 'selected' : ''}>${l}</option>`).join('')}</select>`;
  const chk = (k: string, v: boolean, label: string) => `<label>${label}</label><input type="checkbox" data-f="${k}" ${v ? 'checked' : ''}>`;
  const spacing = (k: string, cur: number) => `<label>Spacing</label><select data-f="${k}">${SPACINGS.map(([v, l]) => `<option value="${v}" ${Math.abs(cur - v) < 1e-6 ? 'selected' : ''}>${l}</option>`).join('')}${SPACINGS.some(([v]) => Math.abs(cur - v) < 1e-6) ? '' : `<option selected value="${cur}">${cur} m</option>`}</select>`;
  let body = `<div class="kv"><label>Type</label><div><b>${({ wall: 'Framed wall', floor: 'Floor framing', roof: 'Roof framing', foundation: 'Foundation', beam: 'Beam', stair: 'Stair' } as any)[type] ?? type}</b> <span class="muted">${e.id.slice(0, 8)}</span></div>`;
  switch (type) {
    case 'wall': { const L = Math.hypot(d.b[0] - d.a[0], d.b[1] - d.a[1]);
      body += `<label>Length</label><div>${fmtM(L)}</div>${n('height', d.height, 'Height')}${sel('studSize', d.studSize, SIZES, 'Studs')}${spacing('studSpacing', d.studSpacing)}${sel('topPlates', String(d.topPlates ?? 2), [['1', '1'], ['2', '2']], 'Top plates')}${sel('reference', d.reference ?? 'center', REFS, 'Drawn line is')}${chk('flip', !!d.flip, 'Flip exterior side')}${sel('cornerDetail', d.cornerDetail === 'none' ? 'twoStud' : d.cornerDetail ?? 'twoStud', CORNERS, 'Corner detail')}${sel('tDetail', d.tDetail ?? 'twoStud', TEES, 'T detail')}${sel('startJoin', d.startJoin ?? 'auto', JOINS, 'Start join')}${sel('endJoin', d.endJoin ?? 'auto', JOINS, 'End join')}${chk('sheathing.exterior.enabled', !!d.sheathing?.exterior?.enabled, 'Ext. sheathing')}${chk('sheathing.interior.enabled', !!d.sheathing?.interior?.enabled, 'Int. gypsum')}</div>`;
      const ops = d.openings ?? []; body += `<div class="muted" style="margin:10px 0 4px;font-size:11px">Rough openings (${ops.length})</div>` + (ops.length ? ops.map((o: any, i: number) => `<div class="kv" style="border-top:1px solid var(--line);padding-top:6px;margin-top:4px"><label>${esc(o.kind)} ${i + 1}</label><button class="icon-btn" data-op-del="${o.id}" style="justify-self:end" title="Remove">✕</button>${n(`openings.${o.id}.offset`, o.offset, 'Offset')}${n(`openings.${o.id}.width`, o.width, 'Width')}${n(`openings.${o.id}.height`, o.height, 'Height')}${n(`openings.${o.id}.sill`, o.sill, 'Sill')}${sel(`openings.${o.id}.headerSize`, o.headerSize ?? '', [['', 'auto by span'], ...SIZES], 'Header')}</div>`).join('') : `<div class="muted">none — use the Rough opening tool</div>`) + `<div class="kv">`;
      break; }
    case 'floor': body += `${sel('joistSize', d.joistSize, SIZES, 'Joists')}${spacing('joistSpacing', d.joistSpacing)}${sel('direction', d.direction ?? 'auto', [['auto', 'auto (short span)'], ['x', 'along x'], ['z', 'along z']], 'Direction')}${chk('rimJoist', d.rimJoist !== false, 'Rim joists')}${chk('blocking', !!d.blocking, 'Blocking')}${chk('subfloor.enabled', d.subfloor?.enabled !== false, 'Subfloor')}${n('elevation', d.elevation ?? 0, 'Elevation')}`; break;
    case 'roof': body += `${sel('shape', d.shape ?? 'gable', SHAPES, 'Shape')}${sel('type', d.type, ROOFS, 'Framing')}${d.shape === 'shed' ? sel('highSide', d.highSide ?? 'max', [['max', 'far side high'], ['min', 'near side high']], 'High side') : ''}${d.shape === 'shed' && d.type === 'rafters' ? sel('highSupport', d.highSupport ?? 'ponyWall', [['ponyWall', 'pony wall on the wall'], ['ledger', 'ledger on a taller wall']], 'High end on') : ''}${d.shape === 'gambrel' ? n('lowerPitch', d.lowerPitch ?? 18, 'Lower pitch /12', 0.5) + n('breakRatio', d.breakRatio ?? 0.25, 'Knee at (span ×)', 0.01) : ''}${d.type === 'scissor' ? n('bottomPitch', d.bottomPitch, 'Ceiling pitch /12', 0.5) : ''}${d.type === 'attic' ? n('atticWidth', d.atticWidth, 'Attic room width') + n('atticHeight', d.atticHeight ?? 2.3, 'Attic room height') : ''}${chk('fascia.enabled', d.fascia?.enabled !== false, 'Fascia')}${chk('soffit.enabled', !!d.soffit?.enabled, 'Soffit')}${sel('cladding.kind', d.cladding?.kind ?? 'none', CLADS, 'Cladding')}${chk('gutters', !!d.gutters, 'Gutters')}${n('pitch', d.pitch, 'Pitch /12', 0.5)}${n('overhang', d.overhang, 'Overhang')}${d.type === 'rafters' || d.shape === 'gambrel' ? '' : n('heel', d.heel ?? 0, 'Raised heel (0 = standard)')}${spacing('spacing', d.spacing)}${sel('chordSize', d.chordSize ?? '2x4', SIZES, 'Chords')}${sel('webSize', d.webSize ?? '2x4', SIZES, 'Webs')}${chk('ridgeAlongX', !!d.ridgeAlongX, 'Ridge along x')}${chk('gableEnds', d.gableEnds !== false, 'Gable end trusses')}${chk('sheathing.enabled', !!d.sheathing?.enabled, 'Roof sheathing')}`; break;
    case 'foundation': body += `${sel('type', d.type, [['slab', 'Slab'], ['stemwall', 'Stem wall + footing']], 'Type')}${n('slabThickness', d.slabThickness, 'Slab thickness')}${n('stemHeight', d.stemHeight, 'Stem height')}${n('stemThickness', d.stemThickness, 'Stem thickness')}${n('footingWidth', d.footingWidth, 'Footing width')}${n('footingDepth', d.footingDepth, 'Footing depth')}`; break;
    case 'beam': body += `<label>Length</label><div>${fmtM(Math.hypot(d.b[0] - d.a[0], d.b[1] - d.a[1]))}</div>${sel('material', d.material ?? 'lumber', [['lumber', 'Built-up lumber'], ['lvl', 'LVL'], ['glulam', 'Glulam']], 'Material')}${sel('size', d.size ?? '2x10', SIZES, 'Size')}${sel('plies', String(d.plies ?? 2), [['1', '1'], ['2', '2'], ['3', '3'], ['4', '4']], 'Plies')}${n('elevation', d.elevation, 'Top elevation')}${chk('posts', d.posts !== false, 'Posts')}${sel('postSize', d.postSize ?? '4x4', [['4x4', '4x4'], ['4x6', '4x6'], ['6x6', '6x6']], 'Post size')}${n('postSpacing', d.postSpacing, 'Post spacing')}${n('tributary', d.tributary, 'Load width')}`; break;
    case 'stair': body += `${n('width', d.width, 'Width')}${n('totalRise', d.totalRise, 'Total rise')}${n('riserHeight', d.riserHeight, 'Riser height', 0.005)}${n('treadDepth', d.treadDepth, 'Tread depth', 0.005)}${sel('stringers', String(d.stringers ?? 3), [['2', '2'], ['3', '3'], ['4', '4']], 'Stringers')}${sel('stringerSize', d.stringerSize ?? '2x12', SIZES, 'Stringer size')}${chk('risers', d.risers !== false, 'Riser boards')}`; break;
    default: return false;
  }
  if (type === 'roof' && (d.shape ?? 'gable') === 'gable') {
    const ds = d.dormers ?? []; body += `</div><div class="muted" style="margin:10px 0 4px;font-size:11px">Dormers (${ds.length})</div>` + ds.map((o: any, i: number) => `<div class="kv" style="border-top:1px solid var(--line);padding-top:6px;margin-top:4px"><label>Dormer ${i + 1}</label><button class="icon-btn" data-dm-del="${o.id}" style="justify-self:end" title="Remove">✕</button>${sel(`dormers.${o.id}.side`, o.side ?? 'l', [['l', 'front slope (l)'], ['r', 'back slope (r)']], 'Slope')}${sel(`dormers.${o.id}.kind`, o.kind ?? 'gable', [['gable', 'Gable dormer'], ['shed', 'Shed dormer']], 'Kind')}${n(`dormers.${o.id}.offset`, o.offset, 'Offset')}${n(`dormers.${o.id}.width`, o.width, 'Width')}${n(`dormers.${o.id}.wallHeight`, o.wallHeight ?? 1, 'Wall height')}${n(`dormers.${o.id}.pitch`, o.pitch ?? 8, 'Pitch /12', 0.5)}${n(`dormers.${o.id}.setback`, o.setback ?? 0, 'Setback')}</div>`).join('') + `<div class="btn-row"><button class="btn" data-dm-add>+ Dormer</button></div><div class="kv">`;
  }
  body += `</div><div class="btn-row">${type === 'wall' ? '<button class="btn" data-draw>Wall drawing</button>' : ''}<button class="btn" data-cut>Cut list</button><button class="btn" data-erase>Erase</button></div>`;
  el.innerHTML = body;
  el.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[data-f]').forEach(inp => inp.onchange = () => {
    const key = inp.dataset.f!; const raw: any = inp.type === 'checkbox' ? (inp as HTMLInputElement).checked : inp.type === 'number' ? (inp.value === '' ? null : parseFloat(inp.value)) : ['topPlates', 'plies', 'stringers'].includes(key) ? parseInt(inp.value) : key.endsWith('Spacing') || key === 'spacing' ? parseFloat(inp.value) : (inp.value || null);
    if (key.startsWith('dormers.')) { const [, oid, field] = key.split('.'); const dormers = (d.dormers ?? []).map((o: any) => o.id === oid ? { ...o, [field]: raw } : o); ctx.ops([fop('update', { id: e.id, patch: { dormers } })], 'Dormer'); return; }
    if (key.startsWith('openings.')) { const [, oid, field] = key.split('.'); const openings = (d.openings ?? []).map((o: any) => o.id === oid ? { ...o, [field]: raw } : o); ctx.ops([fop('update', { id: e.id, patch: { openings } })], 'Opening'); return; }
    const patch: any = {}; let cur = patch; const parts = key.split('.'); parts.forEach((p, i) => { if (i === parts.length - 1) cur[p] = raw; else { cur[p] = {}; cur = cur[p]; } });
    ctx.ops([fop('update', { id: e.id, patch })], 'Framing');
  });
  el.querySelectorAll<HTMLElement>('[data-dm-del]').forEach(b => b.onclick = () => ctx.ops([fop('update', { id: e.id, patch: { dormers: (d.dormers ?? []).filter((o: any) => o.id !== b.dataset.dmDel) } })], 'Remove dormer'));
  el.querySelector<HTMLElement>('[data-dm-add]')?.addEventListener('click', () => { const run = d.ridgeAlongX ? Math.abs(d.max[0] - d.min[0]) : Math.abs(d.max[1] - d.min[1]); const w = Math.min(1.6, run / 3); ctx.ops([fop('update', { id: e.id, patch: { dormers: [...(d.dormers ?? []), { id: crypto.randomUUID(), side: 'l', kind: 'gable', offset: +((run - w) / 2).toFixed(2), width: +w.toFixed(2), wallHeight: 1.0, pitch: 8 }] } })], 'Add dormer'); });
  el.querySelectorAll<HTMLElement>('[data-op-del]').forEach(b => b.onclick = () => ctx.ops([fop('remove_opening', { wall: e.id, opening: b.dataset.opDel })], 'Remove opening'));
  el.querySelector<HTMLElement>('[data-draw]')?.addEventListener('click', async () => { const r = await ctx.ops([fop('wall_svg', { wall: e.id })], 'Wall drawing'); const svg = (r[0]?.entities?.[0]?.data as any)?.svg; if (svg) showSvg('Wall framing elevation', svg, `wall-${e.id.slice(0, 8)}.svg`); });
  el.querySelector<HTMLElement>('[data-cut]')!.onclick = () => { ctx.select([e.id]); showCutList(ctx); };
  el.querySelector<HTMLElement>('[data-erase]')!.onclick = () => ctx.ops([{ op: 'erase', ids: [e.id] }], 'Erase').then(() => ctx.select([]));
  return true;
}

P.registerPlugin({
  id: 'framing', name: 'Framing', version: '0.1.0', accent: ACCENT,
  tools: [wallTool, openingTool, floorTool, roofTool, foundationTool, beamTool, stairTool, frameRoomTool], toolbar: [['f-wall', 'f-opening', 'f-floor', 'f-roof', 'f-foundation', 'f-beam', 'f-stair', 'f-frameroom']],
  panels: [{ id: 'main', title: 'Framing', render: panel }],
  renderEntity,
  activate(h) { host = h; h.on('changed', () => { if (state.pendingCheck) { state.findingsVersion = h.document().version; return; } if (state.findings && state.findingsVersion !== h.document().version) { state.findings = null; } }); },
});
