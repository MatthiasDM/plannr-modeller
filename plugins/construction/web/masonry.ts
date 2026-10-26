// Masonry — the construction plugin's build-up (part of the construction bundle): Belgian brick and block construction (cavity walls with an inner and
// outer leaf, cavity insulation and ties, lintels, sills; beam-and-block "potten en balken" floors,
// hollow-core and in-situ slabs; strip foundations), "detail the house" from the construction model,
// checks with U-values, drawings and a cut list. Engine: plugins/masonry/engine.
import type { Tool, ToolContext, Plane } from '../../../web/src/tools/base';
import type { Plugin, PluginHost } from '../../../web/src/plugins';
import type { Document, Entity, CustomEntity, Op } from '../../../web/src/model';
import type * as THREE_NS from 'three';

type V3 = THREE_NS.Vector3;
const P = (window as any).plannr as { registerPlugin(p: Plugin): void; THREE: typeof THREE_NS };
void P.registerPlugin;
const THREE = P.THREE;
const ACCENT = '#b5523b';
type Plan = [number, number];

interface Unit { name: string; kind: string; length: number; height: number; width: number; joint: number; bond: string }
const UNITS: Unit[] = [
  { name: 'Gevelsteen 190×90×65', kind: 'facingBrick', length: 0.19, height: 0.065, width: 0.09, joint: 0.01, bond: 'stretcher' },
  { name: 'Gevelsteen WF 210×100×50', kind: 'facingBrick', length: 0.21, height: 0.05, width: 0.1, joint: 0.012, bond: 'stretcher' },
  { name: 'Gevelsteen DF 210×100×65', kind: 'facingBrick', length: 0.21, height: 0.065, width: 0.1, joint: 0.012, bond: 'stretcher' },
  { name: 'Snelbouwsteen 288×88×138', kind: 'clayBlock', length: 0.288, height: 0.138, width: 0.09, joint: 0.012, bond: 'stretcher' },
  { name: 'Snelbouwsteen 288×138×138', kind: 'clayBlock', length: 0.288, height: 0.138, width: 0.14, joint: 0.012, bond: 'stretcher' },
  { name: 'Betonblok 390×90×190', kind: 'concreteBlock', length: 0.39, height: 0.19, width: 0.09, joint: 0.01, bond: 'stretcher' },
  { name: 'Betonblok 390×140×190', kind: 'concreteBlock', length: 0.39, height: 0.19, width: 0.14, joint: 0.01, bond: 'stretcher' },
  { name: 'Cellenbeton 600×100×250', kind: 'aeratedBlock', length: 0.6, height: 0.25, width: 0.1, joint: 0.003, bond: 'stretcher' },
  { name: 'Kalkzandsteen 897×100×498', kind: 'limeSandBlock', length: 0.897, height: 0.498, width: 0.1, joint: 0.003, bond: 'third' },
];
const INSUL: [string, string][] = [['none', 'empty cavity'], ['fill', 'full fill (na-isolatie)'], ['board', 'boards against the inner leaf']];
const INSUL_MAT: [string, string][] = [['eps', 'EPS beads / boards'], ['pur', 'PUR foam'], ['mineralWool', 'mineral wool'], ['pir', 'PIR boards'], ['xps', 'XPS boards']];
const SYSTEMS: [string, string][] = [['beamBlock', 'Potten en balken (beam-and-block)'], ['hollowCore', 'Welfsels (hollow-core)'], ['insitu', 'Gestorte plaat (in-situ)']];
const REFS: [string, string][] = [['exterior', 'exterior face'], ['center', 'centre line'], ['interior', 'interior face']];
const state = { wallType: 'cavity' as 'cavity' | 'single', reference: 'exterior', openingKind: 'window' as 'window' | 'door', findings: null as any[] | null, counts: '', findingsVersion: -1, pending: false };
let host: PluginHost;
const esc = (s: unknown) => String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]!));
const mop = (type: string, data: unknown): Op => ({ op: 'custom', type: `masonry.${type}`, data });
const defaults = (doc: Document): any => (doc.extensions?.masonry as any)?.defaults ?? {};
const cext = (doc: Document): any => doc.extensions?.construction ?? { storeys: [{ height: 2.6 }], groundLevel: 0 };
const base = (doc: Document, level: number) => (cext(doc).groundLevel ?? 0) + (cext(doc).storeys ?? []).slice(0, level).reduce((s: number, f: any) => s + f.height, 0);
const activeLevel = () => { const fb = document.querySelector<HTMLElement>('.floor-bar button.active[data-lvl]'); return fb ? parseInt(fb.dataset.lvl!) : 0; };
const planeAt = (doc: Document, level: number): Plane => ({ origin: new THREE.Vector3(0, base(doc, level), 0), normal: new THREE.Vector3(0, 1, 0) });
const plan = (p: V3): Plan => [+p.x.toFixed(4), +p.z.toFixed(4)];
const fmtM = (n: number) => `${n.toFixed(2)} m`;
const customs = (doc: Document, type: string): CustomEntity[] => doc.entities.filter((e): e is CustomEntity => e.kind === 'custom' && e.type === type);
const num = (v: string) => v.split(/[;,x\s]+/).map(parseFloat).filter(n => !isNaN(n));
// the app draws the snap marker of every ctx.snap() a tool makes
const drawSnapDot = (_ctx: ToolContext, _s: { kind: string; point: V3 }) => {};
const unitOpts = (cur: string | undefined) => UNITS.map((u, i) => `<option value="${i}" ${u.name === cur ? 'selected' : ''}>${esc(u.name)}</option>`).join('') + (cur && !UNITS.some(u => u.name === cur) ? `<option selected value="">${esc(cur)}</option>` : '');
const thickness = (doc: Document, type: string) => { const d = defaults(doc); return type === 'single' ? (d.partition?.width ?? 0.09) : (d.outer?.width ?? 0.09) + (d.cavity?.width ?? 0.06) + (d.inner?.width ?? 0.09); };

// ---- tools ---------------------------------------------------------------------------------------
const wallTool: Tool & { start?: V3 | null; cur?: V3 | null } = {
  id: 'm-wall', label: 'Wall', hint: 'Click along the outside face going clockwise (the exterior is on the left): a cavity wall or a single leaf per the Masonry panel; the chain continues until Esc, corners mitre by themselves. Type a length.', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke-width="1.2"><g stroke="#7a3322"><rect x="2" y="4" width="9" height="4" fill="#b5523b"/><rect x="11" y="4" width="11" height="4" fill="#b5523b"/><rect x="2" y="8" width="5" height="4" fill="#b5523b"/><rect x="7" y="8" width="10" height="4" fill="#b5523b"/><rect x="17" y="8" width="5" height="4" fill="#b5523b"/><rect x="2" y="12" width="9" height="4" fill="#b5523b"/><rect x="11" y="12" width="11" height="4" fill="#b5523b"/></g><path d="M2 18h20" stroke="#c9785a" stroke-width="2"/></svg>`,
  busy() { return !!this.start; },
  activate(ctx) { this.start = null; ctx.setHint(this.hint); ctx.setVCB('', 'Length'); }, deactivate(ctx) { this.start = null; ctx.scene.clearOverlay(); },
  onMove(ev, ctx) {
    const doc = ctx.doc(); const s = ctx.snap(ev, { plane: planeAt(doc, activeLevel()), faces: false, from: this.start ?? undefined, lock: ctx.lock }); ctx.scene.clearOverlay(); drawSnapDot(ctx, s);
    if (!this.start) return; this.cur = s.point; const d = s.point.clone().sub(this.start).normalize(); const inward = new THREE.Vector3(-d.z, 0, d.x);
    const t = thickness(doc, state.wallType); const [lo, hi] = state.reference === 'exterior' ? [0, t] : state.reference === 'interior' ? [-t, 0] : [-t / 2, t / 2];
    const at = (p: V3, o: number) => p.clone().addScaledVector(inward, o);
    ctx.scene.addFace([at(this.start, lo), at(s.point, lo), at(s.point, hi), at(this.start, hi)], 0xb5523b, 0.35);
    ctx.scene.addLine([at(this.start, lo), at(s.point, lo), at(s.point, hi), at(this.start, hi), at(this.start, lo)], 0x1b1b1b, false, 2);
    const L = this.start.distanceTo(s.point); ctx.scene.addLabel(this.start.clone().lerp(s.point, 0.5), `${fmtM(L)} · ${state.wallType === 'cavity' ? 'spouwmuur' : 'enkel blad'} ${(t * 100).toFixed(0)} cm`); ctx.setVCB(L.toFixed(2), 'Length');
  },
  onDown(ev, ctx) { if (ev.button !== 0) return; const s = ctx.snap(ev, { plane: planeAt(ctx.doc(), activeLevel()), faces: false, from: this.start ?? undefined, lock: ctx.lock }); if (!this.start) { this.start = s.point.clone(); return; } void placeWall(this, ctx, s.point); },
  onVCB(v, ctx) { const n = num(v); if (!n.length || !this.start || !this.cur) return; const d = this.cur.clone().sub(this.start).normalize(); void placeWall(this, ctx, this.start.clone().addScaledVector(d, n[0])); },
  onDblClick(_e, ctx) { this.start = null; ctx.scene.clearOverlay(); },
  onKey(ev, ctx) { if (ev.key === 'Escape') { this.start = null; ctx.scene.clearOverlay(); return true; } return false; },
};
async function placeWall(t: typeof wallTool, ctx: ToolContext, end: V3) {
  if (!t.start || t.start.distanceTo(end) < 0.1) return; const r = await ctx.ops([mop('add_wall', { level: activeLevel(), a: plan(t.start), b: plan(end), type: state.wallType, reference: state.reference })], 'Masonry wall'); t.start = end.clone(); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]);
}

const OPEN: Record<string, { w: number; h: number; sill: number }> = { window: { w: 1.2, h: 1.4, sill: 0.9 }, door: { w: 0.9, h: 2.1, sill: 0 } };
function hitWall(ev: PointerEvent, ctx: ToolContext) {
  const doc = ctx.doc(); const g = ctx.scene.groundPoint(ev, base(doc, activeLevel())); const pk = ctx.scene.pick(ev);
  const p: Plan | null = pk ? [pk.point.x, pk.point.z] : g ? [g.x, g.z] : null; if (!p) return null;
  let best: { wall: CustomEntity; t: number; d: number } | null = null;
  for (const w of customs(doc, 'masonry.wall')) { const a = (w.data as any).a as Plan, b = (w.data as any).b as Plan; const dx = b[0] - a[0], dz = b[1] - a[1]; const L2 = dx * dx + dz * dz || 1e-9; let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / L2; t = Math.max(0, Math.min(1, t)); const d = Math.hypot(a[0] + dx * t - p[0], a[1] + dz * t - p[1]); if (d < 0.6 && (!best || d < best.d)) best = { wall: w, t, d }; }
  if (!best) return null; const a = (best.wall.data as any).a as Plan, b = (best.wall.data as any).b as Plan; const L = Math.hypot(b[0] - a[0], b[1] - a[1]);
  return { wall: best.wall, d: best.d, along: best.t * L, pt: [a[0] + (b[0] - a[0]) * best.t, a[1] + (b[1] - a[1]) * best.t] as Plan, dir: new THREE.Vector3(b[0] - a[0], 0, b[1] - a[1]).normalize(), L };
}
const openingTool: Tool = {
  id: 'm-opening', label: 'Wall opening', hint: 'Hover a masonry wall and click to add a window or door: cut through every leaf with lintels and a bluestone sill. Type width; height.', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke-width="1.4"><rect x="2" y="3" width="20" height="18" fill="#b5523b" stroke="#7a3322"/><rect x="7" y="7" width="10" height="9" fill="#cfe3f5" stroke="#1b1b1b"/><path d="M5.5 6h13" stroke="#8e9398" stroke-width="2.2"/><path d="M6 17h12" stroke="#4f5660" stroke-width="2.2"/></svg>`,
  activate(ctx) { ctx.setHint(this.hint); const o = OPEN[state.openingKind]; ctx.setVCB(`${o.w}; ${o.h}`, 'Width; Height'); }, deactivate(ctx) { ctx.scene.clearOverlay(); },
  onMove(ev, ctx) { ctx.scene.clearOverlay(); const h = hitWall(ev, ctx); if (!h) return; const o = OPEN[state.openingKind]; const y0 = base(ctx.doc(), (h.wall.data as any).level) + o.sill; const c = new THREE.Vector3(h.pt[0], 0, h.pt[1]); const l = c.clone().addScaledVector(h.dir, -o.w / 2), r = c.clone().addScaledVector(h.dir, o.w / 2); ctx.scene.addLine([l.clone().setY(y0), r.clone().setY(y0), r.clone().setY(y0 + o.h), l.clone().setY(y0 + o.h), l.clone().setY(y0)], 0xc8169a, false, 2); ctx.scene.addLabel(c.clone().setY(y0 + o.h), `${state.openingKind} ${o.w}×${o.h} · offset ${fmtM(h.along - o.w / 2)}`); },
  async onDown(ev, ctx) { if (ev.button !== 0) return; const h = hitWall(ev, ctx); if (!h) { ctx.toast('Hover a masonry wall'); return; } const o = OPEN[state.openingKind]; await ctx.ops([mop('add_opening', { wall: h.wall.id, kind: state.openingKind, offset: +Math.max(0, h.along - o.w / 2).toFixed(3), width: o.w, height: o.h, sill: o.sill })], 'Wall opening'); },
  onVCB(v, ctx) { const n = num(v); if (n.length) { const o = OPEN[state.openingKind]; o.w = n[0]; if (n[1]) o.h = n[1]; ctx.setVCB(`${o.w}; ${o.h}`, 'Width; Height'); } },
};
function rectTool(id: string, label: string, hint: string, icon: string, onRect: (ctx: ToolContext, a: V3, b: V3, level: number) => Promise<void>): Tool & { first?: V3 | null } {
  return {
    id, label, hint, cursor: 'crosshair', icon,
    busy() { return !!this.first; },
    activate(ctx) { this.first = null; ctx.setHint(hint); ctx.setVCB('', 'Dimensions'); }, deactivate(ctx) { this.first = null; ctx.scene.clearOverlay(); },
    onMove(ev, ctx) {
      const doc = ctx.doc(); const y = base(doc, activeLevel()); const s = ctx.snap(ev, { plane: { origin: new THREE.Vector3(0, y, 0), normal: new THREE.Vector3(0, 1, 0) }, faces: false }); ctx.scene.clearOverlay(); drawSnapDot(ctx, s);
      if (!this.first) return; const a = this.first, b = s.point; const pts = [new THREE.Vector3(a.x, y, a.z), new THREE.Vector3(b.x, y, a.z), new THREE.Vector3(b.x, y, b.z), new THREE.Vector3(a.x, y, b.z)];
      ctx.scene.addLine([...pts, pts[0]], 0x1b1b1b, false, 2); ctx.scene.addFace(pts, 0xb5523b, 0.2); ctx.scene.addLabel(pts[0].clone().lerp(pts[1], 0.5), fmtM(Math.abs(b.x - a.x))); ctx.scene.addLabel(pts[1].clone().lerp(pts[2], 0.5), fmtM(Math.abs(b.z - a.z)));
    },
    async onDown(ev, ctx) {
      if (ev.button !== 0) return; const y = base(ctx.doc(), activeLevel()); const s = ctx.snap(ev, { plane: { origin: new THREE.Vector3(0, y, 0), normal: new THREE.Vector3(0, 1, 0) }, faces: false });
      if (!this.first) { this.first = s.point.clone(); return; } const a = this.first; this.first = null; ctx.scene.clearOverlay(); if (Math.abs(s.point.x - a.x) < 0.3 || Math.abs(s.point.z - a.z) < 0.3) { ctx.toast('Too small'); return; } await onRect(ctx, a, s.point, activeLevel());
    },
    onVCB(v, ctx) { const n = num(v); if (n.length < 2 || !this.first) return; const a = this.first; this.first = null; ctx.scene.clearOverlay(); void onRect(ctx, a, new THREE.Vector3(a.x + n[0], a.y, a.z + n[1]), activeLevel()); },
    onKey(ev, ctx) { if (ev.key === 'Escape' && this.first) { this.first = null; ctx.scene.clearOverlay(); return true; } return false; },
  };
}
const rectPts = (a: V3, b: V3): Plan[] => { const x0 = Math.min(a.x, b.x), x1 = Math.max(a.x, b.x), z0 = Math.min(a.z, b.z), z1 = Math.max(a.z, b.z); return [[x0, z0], [x1, z0], [x1, z1], [x0, z1]].map(p => [+p[0].toFixed(4), +p[1].toFixed(4)] as Plan); };
const floorTool = rectTool('m-floor', 'Structural floor', 'On an upper storey: click two corners of the floor outline (over the bearings): potten en balken, welfsels or a poured slab per the Masonry panel.', `<svg viewBox="0 0 24 24" fill="none" stroke-width="1.2"><rect x="2" y="9" width="20" height="3" fill="#a9a6a0" stroke="#5a5f66"/><g fill="#b9b6ae" stroke="#5a5f66"><path d="M3 12h4v5H3zM9 12h4v5H9zM15 12h4v5h-4z"/></g><path d="M7 12h2v5H7zM13 12h2v5h-2zM19 12h2v5h-2z" fill="#97948d" stroke="#5a5f66"/></svg>`,
  async (ctx, a, b, level) => { if (level < 1) { ctx.toast('Pick an upper storey: a structural floor is the floor of the storey it carries'); return; } const r = await ctx.ops([mop('add_floor', { level, points: rectPts(a, b), system: defaults(ctx.doc()).floor?.system ?? 'beamBlock' })], 'Structural floor'); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]); });
const foundationTool = rectTool('m-foundation', 'Strip foundation', 'Click two corners along the ground-floor wall lines: footing at frost depth and foundation blocks.', `<svg viewBox="0 0 24 24" fill="none" stroke-width="1.2"><path d="M2 6h20" stroke="#5a5f66" stroke-dasharray="2 2"/><rect x="8" y="6" width="8" height="10" fill="#a7a6a1" stroke="#5a5f66"/><rect x="4" y="16" width="16" height="5" fill="#a9a6a0" stroke="#5a5f66"/></svg>`,
  async (ctx, a, b) => { const r = await ctx.ops([mop('add_foundation', { points: rectPts(a, b) })], 'Strip foundation'); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]); });

// ---- dialogs ------------------------------------------------------------------------------------------
function dialog(title: string, body: string, wide = 900): HTMLElement {
  const el = document.createElement('div'); el.className = 'dlg-host';
  el.innerHTML = `<div class="dlg" style="width:min(96vw,${wide}px);max-height:92vh"><div class="dlg-title" style="display:flex;align-items:center;gap:10px">${esc(title)}<span style="flex:1"></span><span data-extra></span><button class="btn" data-close>Close</button></div><div style="overflow:auto;max-height:80vh">${body}</div></div>`;
  document.body.appendChild(el); const close = () => el.remove(); el.querySelector<HTMLElement>('[data-close]')!.onclick = close; el.addEventListener('pointerdown', e => { if (e.target === el) close(); });
  return el;
}
function showSvg(title: string, svg: string, file: string) {
  const el = dialog(title, `<div style="background:#fff;border-radius:8px;padding:8px">${svg}</div>`, 1100);
  const v = el.querySelector<SVGElement>('svg'); if (v) { v.style.maxWidth = '100%'; v.style.height = 'auto'; }
  const b = document.createElement('button'); b.className = 'btn'; b.textContent = 'Download SVG'; b.onclick = () => { const a = document.createElement('a'); a.href = URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml' })); a.download = file; a.click(); }; el.querySelector('[data-extra]')!.appendChild(b);
}
async function showCutList(ctx: ToolContext) {
  const ids = new Set(['masonry.wall', 'masonry.floor', 'masonry.foundation'].flatMap(t => customs(ctx.doc(), t).map(e => e.id)));
  const sel = [...ctx.selection()].filter(id => ids.has(id));
  const r = await ctx.ops([mop('estimate', sel.length ? { ids: sel } : {})], 'Masonry cut list'); const est: any = r[0]?.entities?.[0]?.data; if (!est) return;
  const t = est.totals ?? {};
  const rows = (est.items ?? []).map((it: any) => `<tr><td>${esc(it.category)}</td><td style="text-align:left">${esc(it.description)}</td><td>${it.count ?? ''}</td><td>${it.length_m != null ? it.length_m.toFixed(2) : ''}</td><td>${it.area_m2 != null ? it.area_m2.toFixed(2) : ''}</td><td>${it.volume_m3 != null ? it.volume_m3.toFixed(3) : ''}</td></tr>`).join('');
  const el = dialog(`Masonry takeoff ${sel.length ? `(${sel.length} selected)` : '(whole model)'}`, `<div class="muted">${t.bricks ?? 0} bricks (${t.bricksWithWaste ?? 0} with 5 % waste) · ${t.blocks ?? 0} blocks (${t.blocksWithWaste ?? 0}) · ${(t.mortar_m3 ?? 0).toFixed(2)} m³ mortar · ${t.ties ?? 0} ties · ${t.lintels ?? 0} lintels · ${(t.insulation_m2 ?? 0).toFixed(1)} m² insulation · ${(t.joists_m ?? 0).toFixed(1)} m joists · ${t.floorBlocks ?? 0} floor blocks · ${(t.concrete_m3 ?? 0).toFixed(2)} m³ concrete</div>
    <table class="data" style="margin-top:6px"><tr><th>Category</th><th style="text-align:left">Item</th><th>Count</th><th>Length (m)</th><th>Area (m²)</th><th>Volume (m³)</th></tr>${rows}</table>`);
  const b = document.createElement('button'); b.className = 'btn'; b.textContent = 'Download CSV';
  b.onclick = () => { const csv = ['category,item,count,length_m,area_m2,volume_m3', ...(est.items ?? []).map((it: any) => [it.category, `"${it.description}"`, it.count ?? '', it.length_m ?? '', it.area_m2 ?? '', it.volume_m3 ?? ''].join(','))].join('\n'); const a = document.createElement('a'); a.href = URL.createObjectURL(new Blob([csv], { type: 'text/csv' })); a.download = 'masonry-takeoff.csv'; a.click(); };
  el.querySelector('[data-extra]')!.appendChild(b);
}
async function showUValues(ctx: ToolContext) {
  const r = await ctx.ops([mop('uvalue', {})], 'U-values'); const u: any = r[0]?.entities?.[0]?.data; if (!u) return;
  const body = (u.buildups ?? []).map((b: any) => `<div style="margin:10px 0 14px"><div><b>${b.type === 'cavity' ? 'Cavity wall' : b.partition ? 'Partition' : 'Single leaf'}</b> · ${b.walls.length} wall(s) · ${b.area_m2} m² · ${b.partition ? `<b>U = ${b.U.toFixed(2)} W/m²K</b> <span class="muted">(interior partition — no EPB requirement)</span>` : `<b style="color:${b.meets ? '#1e8e3e' : '#d93025'}">U = ${b.U.toFixed(2)} W/m²K</b> <span class="muted">(EPB renovation ≤ ${u.requirement})</span>`}</div>
    <table class="data" style="margin-top:4px"><tr><th style="text-align:left">Layer (inside → outside)</th><th>d (mm)</th><th>λ (W/mK)</th><th>R (m²K/W)</th></tr>${b.layers.map((l: any) => `<tr><td style="text-align:left">${esc(l.name)}</td><td>${l.thickness ? (l.thickness * 1000).toFixed(0) : ''}</td><td>${l.lambda ?? ''}</td><td>${l.R.toFixed(3)}</td></tr>`).join('')}<tr><td style="text-align:left"><b>Total</b></td><td></td><td></td><td><b>${b.R_total.toFixed(3)}</b></td></tr></table>
    ${b.withFill ? `<div class="muted" style="margin-top:4px">After a full cavity fill: ${b.withFill.map((x: any) => `${esc(x.material)} U = ${x.U.toFixed(2)}`).join(' · ')}</div>` : ''}</div>`).join('');
  dialog('U-values of the wall build-ups', body || '<div class="empty">No masonry walls yet.</div>', 760);
}
async function runCheck(ctx: ToolContext) {
  state.pending = true; const r = await ctx.ops([mop('check', {})], 'Masonry check'); const d: any = r[0]?.entities?.[0]?.data;
  setTimeout(() => { state.pending = false; state.findingsVersion = host.document().version; }, 800);
  if (!d) return; state.findings = d.findings ?? []; state.counts = `${d.errors ?? 0} errors · ${d.warnings ?? 0} warnings`; state.findingsVersion = ctx.doc().version; host.refresh();
}

// ---- panel ----------------------------------------------------------------------------------------------
/** Depth of what fills the top of storey `level`: the masonry floor of the storey above, attic joists framed in its top. */
function structureDepth(doc: Document, level: number): number {
  const fd = defaults(doc).floor ?? {}; const h = cext(doc).storeys?.[level]?.height ?? 2.6; let depth = 0;
  for (const f of customs(doc, 'masonry.floor')) { const x: any = f.data; if (x.level !== level + 1) continue; const g = (k: string, v: number) => x[k] ?? fd[k] ?? v; depth = Math.max(depth, x.system === 'hollowCore' ? g('plankThickness', 0.2) + g('topping', 0.04) : x.system === 'insitu' ? g('slabThickness', 0.18) : g('blockHeight', 0.16) + g('topping', 0.04)); }
  for (const f of customs(doc, 'framing.floor')) { const x: any = f.data; if ((x.level ?? 0) === level && (x.elevation ?? 0) > 0.5 && h - x.elevation < 0.6) depth = Math.max(depth, h - x.elevation); }
  return depth;
}
function storeyRows(doc: Document): string {
  const st: any[] = cext(doc).storeys ?? [];
  return st.map((s, i) => { const dep = structureDepth(doc, i); const clear = s.height - dep; return `<label>${esc(s.name || `Storey ${i + 1}`)}</label><div class="row"><input type="number" step="0.01" min="1.8" max="6" data-clear="${i}" value="${clear.toFixed(2)}" style="width:70px" title="Clear height: finished floor to the underside of the structure above"><span class="muted" style="align-self:center;font-size:11px">clear + ${(dep * 100).toFixed(0)} cm = ${s.height.toFixed(2)} m</span></div>`; }).join('');
}

function panel(ctx: ToolContext, el: HTMLElement) {
  const doc = ctx.doc(); const d = defaults(doc); const cav = d.cavity ?? {}; const ins = cav.insulation ?? {}; const fl = d.floor ?? {};
  const n = { walls: customs(doc, 'masonry.wall').length, floors: customs(doc, 'masonry.floor').length, found: customs(doc, 'masonry.foundation').length };
  el.innerHTML = `
    <div class="muted" style="margin:4px 0 6px;font-size:11px">Wall build-up for new walls</div>
    <div class="kv"><label>Outer leaf</label><select data-unit="outer">${unitOpts(d.outer?.name)}</select>
      <label>Cavity</label><div class="row"><input type="number" step="0.01" data-cav="width" value="${cav.width ?? 0.06}" style="width:70px"><span class="muted" style="align-self:center">m</span></div>
      <label>Insulation</label><select data-ins="kind">${INSUL.map(([k, l]) => `<option value="${k}" ${(ins.kind ?? 'none') === k ? 'selected' : ''}>${l}</option>`).join('')}</select>
      ${(ins.kind ?? 'none') !== 'none' ? `<label>Material</label><select data-ins="material">${INSUL_MAT.map(([k, l]) => `<option value="${k}" ${(ins.material ?? 'eps') === k ? 'selected' : ''}>${l}</option>`).join('')}</select>` : ''}
      ${ins.kind === 'board' ? `<label>Board</label><input type="number" step="0.01" data-ins="thickness" value="${ins.thickness ?? 0.06}">` : ''}
      <label>Ties per m²</label><input type="number" step="0.5" data-ties value="${cav.ties?.perM2 ?? 5}">
      <label>Inner leaf</label><select data-unit="inner">${unitOpts(d.inner?.name)}</select>
      <label>Partitions</label><select data-unit="partition">${unitOpts(d.partition?.name)}</select>
      <label>Interior plaster</label><input type="checkbox" data-plaster ${d.plaster?.interior !== false ? 'checked' : ''}></div>
    <div class="muted" style="margin:12px 0 6px;font-size:11px">Storeys · clear height + structure above = floor-to-floor</div>
    <div class="kv">${storeyRows(doc)}</div>
    <div class="muted" style="margin:12px 0 6px;font-size:11px">Tool options</div>
    <div class="kv"><label>Wall</label><select data-s="wallType"><option value="cavity" ${state.wallType === 'cavity' ? 'selected' : ''}>Cavity wall (spouwmuur)</option><option value="single" ${state.wallType === 'single' ? 'selected' : ''}>Single leaf</option></select>
      <label>Draw along</label><select data-s="reference">${REFS.map(([k, l]) => `<option value="${k}" ${state.reference === k ? 'selected' : ''}>${l}</option>`).join('')}</select>
      <label>Opening</label><select data-s="openingKind"><option value="window" ${state.openingKind === 'window' ? 'selected' : ''}>Window</option><option value="door" ${state.openingKind === 'door' ? 'selected' : ''}>Door</option></select>
      <label>Floor system</label><select data-floor="system">${SYSTEMS.map(([k, l]) => `<option value="${k}" ${(fl.system ?? 'beamBlock') === k ? 'selected' : ''}>${l}</option>`).join('')}</select>
      ${(fl.system ?? 'beamBlock') === 'beamBlock' ? `<label>Blocks</label><select data-floor="blockHeight">${[0.12, 0.16, 0.2, 0.24].map(h => `<option value="${h}" ${Math.abs((fl.blockHeight ?? 0.16) - h) < 1e-6 ? 'selected' : ''}>${h * 100} + ${((fl.topping ?? 0.04) * 100).toFixed(0)} cm</option>`).join('')}</select>` : ''}</div>
    <div class="btn-row"><button class="btn" data-act="import" title="Walls and floors are built from the build-up above automatically; this writes every one of them out as its own element so each can get its own build-up">Customise all walls</button><button class="btn" data-act="cut">Cut list</button></div>
    <div class="btn-row"><button class="btn" data-act="check">Check</button><button class="btn" data-act="u">U-values</button><button class="btn" data-act="plan">Plan drawing</button></div>
    ${state.findings ? `<div class="muted" style="margin:6px 0 4px;font-size:11px">${esc(state.counts)}</div>` + state.findings.map((f: any) => `<div class="list-row" ${f.element ? `data-el="${esc(f.element)}"` : ''} title="${esc(f.detail ?? '')}"><span class="swatch" style="background:${({ error: '#d93025', warning: '#f2b705', info: '#5a93f0', ok: '#1e8e3e' } as any)[f.severity] ?? '#999'}"></span><span class="name">${esc(f.title)}${f.value != null && f.limit != null ? ` <span class="muted">${(+f.value).toFixed(2)} / ${(+f.limit).toFixed(2)}</span>` : ''}</span></div>`).join('') : ''}
    <div class="muted" style="margin:12px 0 0">${n.walls} walls · ${n.floors} floors · ${n.found} foundations</div>`;
  const set = (patch: any) => ctx.ops([mop('set_defaults', patch)], 'Masonry defaults');
  el.querySelectorAll<HTMLSelectElement>('[data-unit]').forEach(s => s.onchange = () => { const u = UNITS[parseInt(s.value)]; if (u) set({ [s.dataset.unit!]: u }); });
  el.querySelector<HTMLInputElement>('[data-cav="width"]')!.onchange = (e) => set({ cavity: { width: parseFloat((e.target as HTMLInputElement).value) } });
  el.querySelectorAll<HTMLSelectElement | HTMLInputElement>('[data-ins]').forEach(i => i.onchange = () => set({ cavity: { insulation: { [i.dataset.ins!]: i.type === 'number' ? parseFloat(i.value) : i.value } } }));
  el.querySelector<HTMLInputElement>('[data-ties]')!.onchange = (e) => set({ cavity: { ties: { perM2: parseFloat((e.target as HTMLInputElement).value) } } });
  el.querySelector<HTMLInputElement>('[data-plaster]')!.onchange = (e) => set({ plaster: { interior: (e.target as HTMLInputElement).checked } });
  el.querySelectorAll<HTMLSelectElement>('[data-floor]').forEach(s => s.onchange = () => set({ floor: { [s.dataset.floor!]: s.dataset.floor === 'system' ? s.value : parseFloat(s.value) } }));
  el.querySelectorAll<HTMLInputElement>('[data-clear]').forEach(i => i.onchange = () => ctx.ops([mop('set_clear_height', { level: parseInt(i.dataset.clear!), clear: parseFloat(i.value) })], 'Clear height'));
  el.querySelectorAll<HTMLSelectElement>('[data-s]').forEach(s => s.onchange = () => { (state as any)[s.dataset.s!] = s.value; host.refresh(); });
  el.querySelector<HTMLElement>('[data-act="import"]')!.onclick = async () => {
    if (!(await ctx.confirm('Write every wall and upper floor out as its own masonry element, so each can get its own build-up? (They are built from the build-up automatically; earlier customised elements are replaced.)', 'Customise'))) return;
    const r = await ctx.ops([mop('import_construction', {})], 'Customise all walls'); ctx.toast(`${r[0]?.created?.length ?? 0} masonry elements`);
  };
  el.querySelector<HTMLElement>('[data-act="cut"]')!.onclick = () => showCutList(ctx);
  el.querySelector<HTMLElement>('[data-act="check"]')!.onclick = () => runCheck(ctx);
  el.querySelector<HTMLElement>('[data-act="u"]')!.onclick = () => showUValues(ctx);
  el.querySelector<HTMLElement>('[data-act="plan"]')!.onclick = async () => { const lvl = activeLevel(); const r = await ctx.ops([mop('plan_svg', { level: lvl })], 'Masonry plan'); const svg = (r[0]?.entities?.[0]?.data as any)?.svg; if (svg) showSvg(`Masonry plan · storey ${lvl + 1}`, svg, `masonry-plan-${lvl + 1}.svg`); };
  el.querySelectorAll<HTMLElement>('[data-el]').forEach(r => r.onclick = () => ctx.select([r.dataset.el!]));
}

// ---- Entity Info ----------------------------------------------------------------------------------------
function renderEntity(ctx: ToolContext, e: Entity, el: HTMLElement): boolean {
  if (e.kind !== 'custom' || !e.type.startsWith('masonry.')) return false; const type = e.type.slice(8); const d: any = e.data; const dd = defaults(ctx.doc());
  const n = (k: string, v: number | null | undefined, label: string, step = 0.01) => `<label>${label}</label><input type="number" step="${step}" data-f="${k}" value="${v ?? ''}" placeholder="auto">`;
  const sel = (k: string, cur: string | undefined, opts: [string, string][], label: string) => `<label>${label}</label><select data-f="${k}">${opts.map(([v, l]) => `<option value="${v}" ${cur === v ? 'selected' : ''}>${l}</option>`).join('')}</select>`;
  const chk = (k: string, v: boolean, label: string) => `<label>${label}</label><input type="checkbox" data-f="${k}" ${v ? 'checked' : ''}>`;
  let body = `<div class="kv"><label>Type</label><div><b>${({ wall: d.type === 'single' ? 'Masonry wall · single leaf' : 'Masonry wall · cavity', floor: 'Structural floor', foundation: 'Strip foundation' } as any)[type]}</b> <span class="muted">${e.id.slice(0, 8)}</span></div>`;
  if (type === 'wall') {
    const L = Math.hypot(d.b[0] - d.a[0], d.b[1] - d.a[1]); const cav = d.cavity ?? dd.cavity ?? {}; const ins = cav.insulation ?? {};
    body += `<label>Length</label><div>${fmtM(L)}</div>${sel('type', d.type ?? 'cavity', [['cavity', 'Cavity wall'], ['single', 'Single leaf']], 'Wall')}${sel('reference', d.reference ?? 'exterior', REFS, 'Drawn line is')}${chk('flip', !!d.flip, 'Flip exterior side')}${n('height', d.height, 'Inner leaf height')}`;
    if ((d.type ?? 'cavity') === 'cavity') body += `<label>Outer leaf</label><select data-u="outer">${unitOpts((d.outer ?? dd.outer)?.name)}</select>${n('cavity.width', cav.width, 'Cavity')}${sel('cavity.insulation.kind', ins.kind ?? 'none', INSUL, 'Insulation')}${(ins.kind ?? 'none') !== 'none' ? sel('cavity.insulation.material', ins.material ?? 'eps', INSUL_MAT, 'Material') : ''}${ins.kind === 'board' ? n('cavity.insulation.thickness', ins.thickness, 'Board') : ''}${n('cavity.ties.perM2', cav.ties?.perM2 ?? 5, 'Ties per m²', 0.5)}`;
    body += `<label>${(d.type ?? 'cavity') === 'cavity' ? 'Inner leaf' : 'Leaf'}</label><select data-u="inner">${unitOpts((d.inner ?? ((d.type ?? 'cavity') === 'cavity' ? dd.inner : dd.partition))?.name)}</select>${chk('plaster.interior', d.plaster?.interior !== false, 'Interior plaster')}${chk('plaster.exterior', !!d.plaster?.exterior, 'Plaster other side')}${chk('dpc', d.dpc !== false, 'Damp-proof course')}${sel('startJoin', d.startJoin ?? 'auto', [['auto', 'auto'], ['none', 'none']], 'Start join')}${sel('endJoin', d.endJoin ?? 'auto', [['auto', 'auto'], ['none', 'none']], 'End join')}</div>`;
    const ops = d.openings ?? [];
    body += `<div class="muted" style="margin:10px 0 4px;font-size:11px">Openings (${ops.length})</div>` + ops.map((o: any, i: number) => `<div class="kv" style="border-top:1px solid var(--line);padding-top:6px;margin-top:4px"><label>${esc(o.kind)} ${i + 1}</label><button class="icon-btn" data-op-del="${o.id}" style="justify-self:end" title="Remove">✕</button>${n(`openings.${o.id}.offset`, o.offset, 'Offset')}${n(`openings.${o.id}.width`, o.width, 'Width')}${n(`openings.${o.id}.height`, o.height, 'Height')}${o.kind === 'window' ? n(`openings.${o.id}.sill`, o.sill, 'Sill') : ''}</div>`).join('') + `<div class="kv">`;
  } else if (type === 'floor') {
    body += `${sel('system', d.system ?? 'beamBlock', SYSTEMS, 'System')}${sel('direction', d.direction ?? 'auto', [['auto', 'auto (short span)'], ['x', 'along x'], ['z', 'along z']], 'Span')}`;
    if ((d.system ?? 'beamBlock') === 'beamBlock') body += `${n('spacing', d.spacing ?? dd.floor?.spacing, 'Joists h.o.h.')}${n('blockHeight', d.blockHeight ?? dd.floor?.blockHeight, 'Block height')}${sel('blockKind', d.blockKind ?? dd.floor?.blockKind ?? 'concrete', [['concrete', 'Concrete'], ['clay', 'Clay'], ['eps', 'EPS']], 'Blocks')}${n('topping', d.topping ?? dd.floor?.topping, 'Topping')}${chk('mesh', (d.mesh ?? dd.floor?.mesh) !== false, 'Mesh')}`;
    if (d.system === 'hollowCore') body += `${n('plankThickness', d.plankThickness ?? dd.floor?.plankThickness, 'Plank depth')}${n('plankWidth', d.plankWidth ?? dd.floor?.plankWidth, 'Plank width')}${n('topping', d.topping ?? 0, 'Topping')}`;
    if (d.system === 'insitu') body += n('slabThickness', d.slabThickness ?? dd.floor?.slabThickness, 'Slab');
    body += `${n('bearing', d.bearing ?? dd.floor?.bearing, 'Bearing')}${chk('ringBeam', (d.ringBeam ?? dd.floor?.ringBeam) !== false, 'Ring beam')}${chk('ceilingPlaster', !!(d.ceilingPlaster ?? dd.floor?.ceilingPlaster), 'Ceiling plaster')}<label>Openings</label><div>${(d.openings ?? []).length} (stair wells)</div>`;
  } else {
    body += `${n('footingWidth', d.footingWidth ?? dd.foundation?.footingWidth, 'Footing width')}${n('footingDepth', d.footingDepth ?? dd.foundation?.footingDepth, 'Footing depth')}${n('bottom', d.bottom ?? dd.foundation?.bottom, 'Underside')}${n('blockWidth', d.blockWidth ?? dd.foundation?.blockWidth, 'Block wall')}`;
  }
  body += `</div><div class="btn-row">${type === 'wall' ? '<button class="btn" data-draw>Elevation</button><button class="btn" data-u-btn>U-value</button>' : ''}<button class="btn" data-cut>Takeoff</button><button class="btn" data-erase>Erase</button></div>`;
  el.innerHTML = body;
  el.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[data-f]').forEach(inp => inp.onchange = () => {
    const key = inp.dataset.f!; const raw: any = inp.type === 'checkbox' ? (inp as HTMLInputElement).checked : inp.type === 'number' ? (inp.value === '' ? null : parseFloat(inp.value)) : inp.value;
    if (key.startsWith('openings.')) { const [, oid, field] = key.split('.'); ctx.ops([mop('update', { id: e.id, patch: { openings: (d.openings ?? []).map((o: any) => o.id === oid ? { ...o, [field]: raw } : o) } })], 'Opening'); return; }
    const patch: any = {}; let cur = patch; const parts = key.split('.');
    // cavity edits start from the wall's effective cavity so the other fields are kept
    if (parts[0] === 'cavity' && !d.cavity) Object.assign(patch, { cavity: JSON.parse(JSON.stringify(dd.cavity ?? {})) });
    parts.forEach((p, i) => { if (i === parts.length - 1) cur[p] = raw; else { cur[p] = cur[p] ?? {}; cur = cur[p]; } });
    ctx.ops([mop('update', { id: e.id, patch })], 'Masonry');
  });
  el.querySelectorAll<HTMLSelectElement>('[data-u]').forEach(s => s.onchange = () => { const u = UNITS[parseInt(s.value)]; if (u) ctx.ops([mop('update', { id: e.id, patch: { [s.dataset.u!]: u } })], 'Masonry unit'); });
  el.querySelectorAll<HTMLElement>('[data-op-del]').forEach(b => b.onclick = () => ctx.ops([mop('remove_opening', { wall: e.id, opening: b.dataset.opDel })], 'Remove opening'));
  el.querySelector<HTMLElement>('[data-draw]')?.addEventListener('click', async () => { const r = await ctx.ops([mop('wall_svg', { wall: e.id })], 'Wall elevation'); const svg = (r[0]?.entities?.[0]?.data as any)?.svg; if (svg) showSvg('Masonry wall elevation', svg, `wall-${e.id.slice(0, 8)}.svg`); });
  el.querySelector<HTMLElement>('[data-u-btn]')?.addEventListener('click', async () => { const r = await ctx.ops([mop('uvalue', { wall: e.id })], 'U-value'); const u: any = r[0]?.entities?.[0]?.data; if (u) ctx.toast(`U = ${u.U.toFixed(2)} W/m²K (EPB ≤ ${u.requirement})${u.withFill ? ` · after a cavity fill ≈ ${u.withFill[0].U.toFixed(2)}` : ''}`); });
  el.querySelector<HTMLElement>('[data-cut]')!.onclick = () => { ctx.select([e.id]); showCutList(ctx); };
  el.querySelector<HTMLElement>('[data-erase]')!.onclick = () => ctx.ops([{ op: 'erase', ids: [e.id] }], 'Erase').then(() => ctx.select([]));
  return true;
}

/** The masonry tools, panel and Entity Info, used by the construction plugin. */
export const masonryTools = [wallTool, openingTool, floorTool, foundationTool];
export const masonryPanel = panel;
export const masonryRenderEntity = renderEntity;
export { hitWall as masonryHitWall, showSvg, dialog };
export function masonryActivate(h: PluginHost) { host = h; h.on('changed', () => { if (state.pending) { state.findingsVersion = h.document().version; return; } if (state.findings && state.findingsVersion !== h.document().version) state.findings = null; }); }
/** Masonry of a construction room's walls in its Entity Info: elevation and "Customise" per wall. */
export function roomMasonry(ctx: ToolContext, room: CustomEntity, el: HTMLElement) {
  const pts: Plan[] = (room.data as any).points ?? []; if (pts.length < 3) return;
  const sec = document.createElement('div');
  const custom = new Set(customs(ctx.doc(), 'masonry.wall').map(w => { const s: any = (w.data as any).source; return s && s.owner === room.id ? s.key : ''; }));
  sec.innerHTML = `<div class="muted" style="margin:10px 0 4px;font-size:11px">Walls (masonry)</div>` + pts.map((a, i) => { const b = pts[(i + 1) % pts.length]; const L = Math.hypot(b[0] - a[0], b[1] - a[1]); const key = `wall.${i}`;
    return `<div class="list-row"><span class="name">Wall ${i + 1} <span class="muted">${fmtM(L)}${custom.has(key) ? ' · customised' : ''}</span></span><button class="icon-btn" data-elev="${i}" title="Elevation">▤</button>${custom.has(key) ? '' : `<button class="icon-btn" data-custom="${i}" title="Customise this wall (its own build-up and openings)">✎</button>`}</div>`; }).join('');
  el.appendChild(sec);
  sec.querySelectorAll<HTMLElement>('[data-elev]').forEach(b => b.onclick = async () => { const key = `wall.${b.dataset.elev}`; const own = customs(ctx.doc(), 'masonry.wall').find(w => (w.data as any).source?.owner === room.id && (w.data as any).source?.key === key); const r = await ctx.ops([mop('wall_svg', { wall: own ? own.id : `${room.id}#${key}` })], 'Wall elevation'); const svg = (r[0]?.entities?.[0]?.data as any)?.svg; if (svg) showSvg('Wall elevation', svg, `wall-${key}.svg`); });
  sec.querySelectorAll<HTMLElement>('[data-custom]').forEach(b => b.onclick = async () => { const r = await ctx.ops([mop('import_construction', { only: [{ owner: room.id, key: `wall.${b.dataset.custom}` }] })], 'Customise wall'); const id = r[0]?.created?.[0]; if (id) ctx.select([id]); });
}
