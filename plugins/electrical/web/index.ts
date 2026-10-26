// Electrical plugin — browser side: circuits, points, conduits, AREI checks and the two schematics.
// Runtime deps come from window.plannr (no imports). Engine side: plugins/electrical/engine.
import type { Tool, ToolContext } from '../../../web/src/tools/base';
import type { Plugin, PluginHost } from '../../../web/src/plugins';
import type { Document, Entity, CustomEntity, Op, Vec3 } from '../../../web/src/model';
import type * as THREE_NS from 'three';

type V3 = THREE_NS.Vector3;
const P = (window as any).plannr as { registerPlugin(p: Plugin): void; THREE: typeof THREE_NS };
const THREE = P.THREE;

type RcdKind = 'none' | 'mA30' | 'mA300';
interface Circuit { id: string; number: number; name: string; type: string; breaker: number; cableSection: number; rcd: RcdKind; poles?: number; curve?: string; cableType?: string | null; points: string[]; note: string }
interface Board { phases: number; mainBreaker: number; mainDifferential: number | null; differentialType: string; earthing: string; shortCircuitKA: number; defaultCableType: string; address: string; panel?: string | null }
interface Ext { board: Board; circuits: Circuit[]; rcdGroups: { id: string; name: string; milliamps: number; type: string; circuits: string[] }[] }
interface Finding { severity: 'error' | 'warning' | 'info' | 'ok'; title: string; detail: string; circuit?: string; points?: string[] }
interface Conduit { level: number; circuit?: string | null; from?: string | null; to?: string | null; points: Vec3[]; diameter: number; kind: 'conduit' | 'cable' }

const TYPES: [string, string][] = [['lighting', 'Lighting'], ['sockets', 'Sockets'], ['kitchen', 'Kitchen sockets'], ['bathroom', 'Bathroom'], ['outdoor', 'Outdoor'], ['oven', 'Oven'], ['hob', 'Hob'], ['dishwasher', 'Dishwasher'], ['washingMachine', 'Washing machine'], ['dryer', 'Tumble dryer'], ['boiler', 'Boiler / water heater'], ['heatPump', 'Heat pump'], ['evCharger', 'EV charger'], ['pv', 'PV inverter'], ['dedicated', 'Dedicated'], ['other', 'Other']];
const ELECTRICAL_KINDS = new Set(['outlet', 'switchPlate', 'ceilingLight', 'wallLight', 'panel', 'dataPoint']);
const KIND_LABEL: Record<string, string> = { outlet: 'Socket', switchPlate: 'Switch', ceilingLight: 'Light', wallLight: 'Wall light', panel: 'Board', dataPoint: 'Data' };
const PALETTE = ['#2f6fde', '#d93025', '#1e8e3e', '#f2b705', '#8e44ad', '#e67e22', '#16a085', '#c2185b', '#5d4037', '#455a64', '#0097a7', '#7cb342', '#f4511e', '#3949ab', '#00acc1', '#6d4c41', '#9e9d24', '#8e24aa'];
const circuitColor = (n: number) => PALETTE[(n - 1) % PALETTE.length];

const state = { circuit: null as string | null, conduitHeight: 2.45, conduitKind: 'conduit' as Conduit['kind'], diameter: 0.02, mode: 'ceiling' as 'ceiling' | 'floor' | 'straight', findings: null as Finding[] | null, findingsVersion: -1 };
let host: PluginHost;
const XRAY = ['electrical.conduit', 'electrical.cable'];
const xrayOn = (): boolean => { try { return localStorage.getItem('plannr.electrical.xray') === '1'; } catch { return false; } };
function setXray(ctx: ToolContext, on: boolean) {
  const others = ctx.scene.xrayMaterials.filter(m => !XRAY.includes(m));
  ctx.scene.setXray(on ? [...others, ...XRAY] : others);
  try { localStorage.setItem('plannr.electrical.xray', on ? '1' : '0'); } catch { /* storage blocked */ }
}
const esc = (s: string) => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]!));
const ext = (doc: Document): Ext => ({ board: { phases: 1, mainBreaker: 40, mainDifferential: 300, differentialType: 'A', earthing: 'TT', shortCircuitKA: 3, defaultCableType: 'XVB', address: '' }, circuits: [], rcdGroups: [], ...((doc.extensions?.electrical as Partial<Ext>) ?? {}) });
const cext = (doc: Document): any => doc.extensions?.construction ?? { storeys: [{ height: 2.6 }], groundLevel: 0 };
const base = (doc: Document, level: number) => (cext(doc).groundLevel ?? 0) + (cext(doc).storeys ?? []).slice(0, level).reduce((s: number, f: any) => s + f.height, 0);
const eop = (type: string, data: unknown): Op => ({ op: 'custom', type: `electrical.${type}`, data });
const fixtures = (doc: Document): CustomEntity[] => doc.entities.filter((e): e is CustomEntity => e.kind === 'custom' && e.type === 'construction.fixture' && ELECTRICAL_KINDS.has((e.data as any).kind));
const circuitOf = (doc: Document, fixtureId: string) => ext(doc).circuits.find(c => c.points.includes(fixtureId));
const fixtureWorld = (doc: Document, f: CustomEntity): V3 => { const d: any = f.data; return new THREE.Vector3(d.position[0], base(doc, d.level) + d.position[1], d.position[2]); };
const fmtM = (n: number) => `${n.toFixed(2)} m`;
const INK = 0x1b1b1b;

// ---- tools -------------------------------------------------------------------------------------
/** Assign: click electrical points to put them on the active circuit (Shift removes). */
const assignTool: Tool = {
  id: 'e-assign', label: 'Assign to circuit', hint: 'Pick a circuit in the Electrical panel, then click sockets, switches and lights to put them on it. Shift-click removes a point from its circuit.', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M13 2 4 14h7l-1 8 9-12h-7z" fill="#fff6d6"/></svg>`,
  activate(ctx) { ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }, deactivate(ctx) { ctx.scene.clearOverlay(); ctx.scene.highlight(new Set(), true); },
  onMove(ev, ctx) { const p = ctx.scene.pick(ev); const id = p ? ctx.resolve(p.id) : ''; const f = id ? fixtures(ctx.doc()).find(f => f.id === id) : null; ctx.scene.highlight(f ? new Set([f.id]) : new Set(), true); ctx.scene.clearOverlay(); if (f) { const c = circuitOf(ctx.doc(), f.id); ctx.scene.addLabel(fixtureWorld(ctx.doc(), f), `${KIND_LABEL[(f.data as any).kind]} · ${c ? `circuit ${c.number} ${c.name}` : 'unassigned'}`); } },
  async onDown(ev, ctx) {
    if (ev.button !== 0) return; const p = ctx.scene.pick(ev); if (!p) return; const id = ctx.resolve(p.id); const f = fixtures(ctx.doc()).find(f => f.id === id); if (!f) { ctx.toast('Click an electrical point (socket, switch, light, data, board)'); return; }
    if (ev.shiftKey) { await ctx.ops([eop('assign', { points: [f.id], circuit: null })], 'Unassign point'); return; }
    if (!state.circuit) { ctx.toast('Pick a circuit first (Electrical panel)'); return; }
    await ctx.ops([eop('assign', { points: [f.id], circuit: state.circuit })], 'Assign point');
  },
};

/** Conduit: a polyline at a height above the floor; snaps to electrical points. */
const conduitTool: Tool & { pts?: V3[]; level?: number; from?: string | null } = {
  id: 'e-conduit', label: 'Conduit', hint: 'Click the points of a conduit run (start on the board or a point to attach it); double-click or Enter finishes. Type a height above the floor.', cursor: 'crosshair',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M3 18V8h7v10h7V6h4" stroke="#8e9398" stroke-width="3"/><circle cx="3" cy="18" r="2" fill="#f2b705"/><circle cx="21" cy="6" r="2" fill="#f2b705"/></svg>`,
  activate(ctx) { this.pts = []; this.from = null; ctx.setHint(this.hint); ctx.setVCB(String(state.conduitHeight), 'Height'); }, deactivate(ctx) { this.pts = []; ctx.scene.clearOverlay(); },
  onMove(ev, ctx) {
    const doc = ctx.doc(); const lvl = this.level ?? activeLevel(); const y = base(doc, lvl) + state.conduitHeight;
    const near = nearFixture(ev, ctx); const s = near ? { point: fixtureWorld(doc, near), kind: 'vertex' } : ctx.snap(ev, { plane: { origin: new THREE.Vector3(0, y, 0), normal: new THREE.Vector3(0, 1, 0) }, faces: false, from: this.pts!.at(-1), lock: ctx.lock });
    ctx.scene.clearOverlay(); ctx.scene.addDot(s.point, near ? 0xf2b705 : 0x8e9398, near ? 11 : 7);
    if (near) ctx.scene.addLabel(s.point, KIND_LABEL[(near.data as any).kind]);
    if (this.pts!.length) { ctx.scene.addLine([...this.pts!, s.point], 0x8e9398, false, 3); ctx.scene.addLabel(this.pts!.at(-1)!.clone().lerp(s.point, 0.5), fmtM(this.pts!.at(-1)!.distanceTo(s.point))); }
  },
  onDown(ev, ctx) {
    if (ev.button !== 0) return; const doc = ctx.doc(); const near = nearFixture(ev, ctx);
    if (!this.pts!.length) { this.level = near ? (near.data as any).level : activeLevel(); this.from = near?.id ?? null; }
    const y = base(doc, this.level!) + state.conduitHeight;
    const s = near ? { point: fixtureWorld(doc, near) } : ctx.snap(ev, { plane: { origin: new THREE.Vector3(0, y, 0), normal: new THREE.Vector3(0, 1, 0) }, faces: false, from: this.pts!.at(-1), lock: ctx.lock });
    if (this.pts!.length && s.point.distanceTo(this.pts!.at(-1)!) < 0.02) return;
    this.pts!.push(s.point.clone());
    if (near && this.pts!.length > 1) finishConduit(this, ctx, near.id);
  },
  onDblClick(_e, ctx) { this.pts!.pop(); finishConduit(this, ctx, null); },
  onKey(ev, ctx) { if (ev.key === 'Enter') { finishConduit(this, ctx, null); return true; } if (ev.key === 'Escape') { this.pts = []; ctx.scene.clearOverlay(); return true; } return false; },
  onVCB(v, ctx) { const n = parseFloat(v); if (!isNaN(n)) { state.conduitHeight = n; const y = base(ctx.doc(), this.level ?? activeLevel()) + n; for (const p of this.pts!) p.y = y; } },
};
function nearFixture(ev: PointerEvent, ctx: ToolContext): CustomEntity | null {
  const p = ctx.scene.pick(ev); if (!p) return null; const id = ctx.resolve(p.id); return fixtures(ctx.doc()).find(f => f.id === id) ?? null;
}
async function finishConduit(t: typeof conduitTool, ctx: ToolContext, to: string | null) {
  const doc = ctx.doc(); const lvl = t.level ?? activeLevel(); const b = base(doc, lvl);
  if (t.pts!.length >= 2) {
    const r = await ctx.ops([eop('add_conduit', { level: lvl, points: t.pts!.map(p => [+p.x.toFixed(3), +(p.y - b).toFixed(3), +p.z.toFixed(3)]), circuit: state.circuit, from: t.from, to, diameter: state.diameter, kind: state.conduitKind })], 'Add conduit');
    if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]);
  }
  t.pts = []; t.from = null; ctx.scene.clearOverlay();
}
function activeLevel(): number { const fb = document.querySelector<HTMLElement>('.floor-bar button.active[data-lvl]'); return fb ? parseInt(fb.dataset.lvl!) : 0; }

// ---- panel -------------------------------------------------------------------------------------
async function runCheck(ctx: ToolContext) {
  const v = ctx.doc().version; if (state.findingsVersion === v && state.findings) return state.findings;
  try { const r = await ctx.ops([eop('check', {})], 'AREI check'); state.findings = (r[0]?.entities?.[0]?.data as any)?.findings ?? []; state.findingsVersion = v; } catch { state.findings = []; }
  return state.findings!;
}
function panel(ctx: ToolContext, el: HTMLElement) {
  const doc = ctx.doc(); const x = ext(doc); const pts = fixtures(doc); const assigned = new Set(x.circuits.flatMap(c => c.points));
  const unassigned = pts.filter(f => !assigned.has(f.id) && (f.data as any).kind !== 'panel');
  const fnd = state.findings ?? []; const nErr = fnd.filter(f => f.severity === 'error').length, nWarn = fnd.filter(f => f.severity === 'warning').length;
  el.innerHTML = `
    <div class="muted" style="margin:4px 0 6px;font-size:11px">Board</div>
    <div class="kv"><label>Phases</label><select data-b="phases"><option value="1" ${x.board.phases === 1 ? 'selected' : ''}>1 × 230 V</option><option value="3" ${x.board.phases === 3 ? 'selected' : ''}>3 × 400 V</option></select>
      <label>Main breaker</label><input type="number" step="1" data-b="mainBreaker" value="${x.board.mainBreaker}">
      <label>Head differential</label><select data-b="mainDifferential"><option value="300" ${x.board.mainDifferential === 300 ? 'selected' : ''}>300 mA</option><option value="100" ${x.board.mainDifferential === 100 ? 'selected' : ''}>100 mA</option><option value="" ${!x.board.mainDifferential ? 'selected' : ''}>none</option></select>
      <label>Earthing</label><input type="text" data-b="earthing" value="${esc(x.board.earthing)}"><label>Cable type</label><input type="text" data-b="defaultCableType" value="${esc(x.board.defaultCableType)}"><label>Address</label><input type="text" data-b="address" value="${esc(x.board.address ?? '')}"></div>
    <div class="muted" style="margin:12px 0 6px;font-size:11px">Circuits <span style="float:right">${pts.length - 1 >= 0 ? pts.length : 0} points · ${unassigned.length} unassigned</span></div>
    ${x.circuits.length ? x.circuits.slice().sort((a, b) => a.number - b.number).map(c => `<div class="list-row ${state.circuit === c.id ? 'active' : ''}" data-circuit="${c.id}" title="${esc(c.name)} · ${c.breaker} A · ${c.cableSection} mm² · ${c.rcd === 'none' ? 'no RCD' : c.rcd.replace('mA', '') + ' mA'} · ${c.points.length} points"><span class="swatch" style="background:${circuitColor(c.number)}"></span><span class="name"><b>${c.number}</b> ${esc(c.name)} <span class="muted">${c.breaker} A · ${c.cableSection} mm² · ${c.points.length} pt</span></span><button class="icon-btn" data-circuit-edit="${c.id}" title="Edit circuit">✎</button></div>`).join('') : `<div class="empty">No circuits yet.</div>`}
    <div class="btn-row"><button class="btn" data-act="addCircuit">+ Circuit</button><button class="btn" data-act="autoRoute" title="Draw conduits from the board to every point of every circuit">Auto-route</button><select data-opt="mode" style="padding:3px 6px"><option value="ceiling" ${state.mode === 'ceiling' ? 'selected' : ''}>in walls, under the ceiling</option><option value="floor" ${state.mode === 'floor' ? 'selected' : ''}>in the floor screed</option><option value="straight" ${state.mode === 'straight' ? 'selected' : ''}>straight</option></select></div>
    <label class="check-row" title="Conduits run inside walls, floors and ceilings; this draws them through everything in front of them"><input type="checkbox" data-act="xray" ${ctx.scene.xrayMaterials.includes(XRAY[0]) ? 'checked' : ''}> Show conduits through walls (X-ray)</label>
    <div class="muted" style="margin:12px 0 6px;font-size:11px">Conduit tool</div>
    <div class="kv"><label>Kind</label><select data-opt="conduitKind"><option value="conduit" ${state.conduitKind === 'conduit' ? 'selected' : ''}>Flush conduit (buis)</option><option value="cable" ${state.conduitKind === 'cable' ? 'selected' : ''}>Surface cable</option></select>
      <label>Diameter</label><select data-opt="diameter">${[0.016, 0.02, 0.025].map(d => `<option value="${d}" ${state.diameter === d ? 'selected' : ''}>${Math.round(d * 1000)} mm</option>`).join('')}</select>
      <label>Height</label><input type="number" step="0.05" data-opt="conduitHeight" value="${state.conduitHeight}"></div>
    <div class="muted" style="margin:12px 0 6px;font-size:11px">AREI checks ${state.findings ? `<span style="float:right">${nErr} errors · ${nWarn} warnings</span>` : ''}</div>
    ${state.findings ? (fnd.length ? fnd.map(f => `<div class="list-row" ${f.points?.length ? `data-select-points="${f.points.join(',')}"` : ''} title="${esc(f.detail)}"><span class="swatch" style="background:${({ error: '#d93025', warning: '#f2b705', info: '#5a93f0', ok: '#1e8e3e' } as any)[f.severity]}"></span><span class="name">${esc(f.title)}${f.circuit ? ` <span class="muted">· ${esc(x.circuits.find(c => c.id === f.circuit)?.name ?? '')}</span>` : ''}</span></div>`).join('') : '<div class="empty">No findings.</div>') : `<div class="btn-row"><button class="btn" data-act="check">Run checks</button></div>`}
    <div class="muted" style="margin:12px 0 6px;font-size:11px">Schematics</div>
    <div class="btn-row"><button class="btn" data-act="situatie">Situatieschema</button><button class="btn" data-act="eendraad">Eendraadschema</button></div>`;
  el.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[data-b]').forEach(i => i.onchange = () => { const k = i.dataset.b!; const v: any = k === 'mainDifferential' ? (i.value ? parseInt(i.value) : null) : ['phases', 'mainBreaker'].includes(k) ? parseInt(i.value) : i.value; ctx.ops([eop('set_board', { [k]: v })], 'Board'); });
  el.querySelectorAll<HTMLElement>('[data-circuit]').forEach(r => r.onclick = (e) => { if ((e.target as HTMLElement).closest('button')) return; state.circuit = r.dataset.circuit!; const c = x.circuits.find(c => c.id === state.circuit); if (c) ctx.select(c.points); host.refresh(); });
  el.querySelectorAll<HTMLElement>('[data-circuit-edit]').forEach(b => b.onclick = () => editCircuit(ctx, x.circuits.find(c => c.id === b.dataset.circuitEdit)!));
  el.querySelector<HTMLElement>('[data-act="addCircuit"]')?.addEventListener('click', async () => { const name = await ctx.prompt('Circuit name', `Circuit ${x.circuits.length + 1}`); if (!name) return; const r = await ctx.ops([eop('add_circuit', { name, type: 'sockets' })], 'Add circuit'); const id = (r[0]?.entities?.[0]?.data as any)?.id ?? r[0]?.created?.[0]; if (id) { state.circuit = id; host.refresh(); } });
  el.querySelector<HTMLElement>('[data-act="autoRoute"]')?.addEventListener('click', async () => { if (!(await ctx.confirm('Draw conduits from the board to every point of every circuit? Existing auto-routed conduits are replaced.', 'Auto-route'))) return; await ctx.ops([eop('auto_route', { mode: state.mode })], 'Auto-route conduits'); });
  el.querySelectorAll<HTMLSelectElement | HTMLInputElement>('[data-opt]').forEach(i => i.onchange = () => { const k = i.dataset.opt!; (state as any)[k] = ['diameter', 'conduitHeight'].includes(k) ? parseFloat(i.value) : i.value; });
  el.querySelector<HTMLInputElement>('[data-act="xray"]')?.addEventListener('change', (e) => setXray(ctx, (e.target as HTMLInputElement).checked));
  el.querySelector<HTMLElement>('[data-act="check"]')?.addEventListener('click', async () => { await runCheck(ctx); host.refresh(); });
  el.querySelectorAll<HTMLElement>('[data-select-points]').forEach(r => r.onclick = () => ctx.select(r.dataset.selectPoints!.split(',')));
  el.querySelector<HTMLElement>('[data-act="situatie"]')?.addEventListener('click', () => showSchematic(ctx, 'situatie'));
  el.querySelector<HTMLElement>('[data-act="eendraad"]')?.addEventListener('click', () => showSchematic(ctx, 'eendraad'));
}
async function editCircuit(ctx: ToolContext, c: Circuit) {
  const el = document.createElement('div'); el.className = 'dlg-host';
  el.innerHTML = `<div class="dlg" style="width:420px"><div class="dlg-title">Circuit ${c.number}</div><div class="kv">
    <label>Name</label><input type="text" data-c="name" value="${esc(c.name)}">
    <label>Type</label><select data-c="type">${TYPES.map(([k, l]) => `<option value="${k}" ${c.type === k ? 'selected' : ''}>${l}</option>`).join('')}</select>
    <label>Breaker (A)</label><select data-c="breaker">${[6, 10, 16, 20, 25, 32, 40, 63].map(a => `<option ${c.breaker === a ? 'selected' : ''}>${a}</option>`).join('')}</select>
    <label>Cable (mm²)</label><select data-c="cableSection">${[1.5, 2.5, 4, 6, 10, 16].map(a => `<option ${c.cableSection === a ? 'selected' : ''}>${a}</option>`).join('')}</select>
    <label>Differential</label><select data-c="rcd"><option value="mA30" ${c.rcd === 'mA30' ? 'selected' : ''}>30 mA</option><option value="mA300" ${c.rcd === 'mA300' ? 'selected' : ''}>300 mA</option><option value="none" ${c.rcd === 'none' ? 'selected' : ''}>none</option></select>
    <label>Poles</label><select data-c="poles"><option value="2" ${(c.poles ?? 2) === 2 ? 'selected' : ''}>2P</option><option value="4" ${c.poles === 4 ? 'selected' : ''}>4P</option></select>
    <label>Curve</label><select data-c="curve">${['B', 'C', 'D'].map(k => `<option ${(c.curve ?? 'C') === k ? 'selected' : ''}>${k}</option>`).join('')}</select>
    <label>Note</label><input type="text" data-c="note" value="${esc(c.note ?? '')}"></div>
    <div class="dlg-actions"><button class="btn danger-text" data-del>Remove circuit</button><span style="flex:1"></span><button class="btn" data-cancel>Cancel</button><button class="btn primary" data-ok>Save</button></div></div>`;
  document.body.appendChild(el);
  const close = () => el.remove();
  el.querySelector<HTMLElement>('[data-cancel]')!.onclick = close; el.addEventListener('pointerdown', e => { if (e.target === el) close(); });
  el.querySelector<HTMLElement>('[data-ok]')!.onclick = async () => { const patch: any = { id: c.id }; el.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[data-c]').forEach(i => { const k = i.dataset.c!; patch[k] = ['breaker', 'poles'].includes(k) ? parseInt(i.value) : k === 'cableSection' ? parseFloat(i.value) : i.value; }); close(); await ctx.ops([eop('update_circuit', patch)], 'Edit circuit'); };
  el.querySelector<HTMLElement>('[data-del]')!.onclick = async () => { close(); if (await ctx.confirm(`Remove circuit ${c.number} "${c.name}"? Its points become unassigned.`, 'Remove', true)) { if (state.circuit === c.id) state.circuit = null; await ctx.ops([eop('remove_circuit', { id: c.id })], 'Remove circuit'); } };
}
async function showSchematic(ctx: ToolContext, kind: 'situatie' | 'eendraad') {
  const level = activeLevel();
  const r = await ctx.ops([kind === 'situatie' ? eop('situation_svg', { level, showConduits: true }) : eop('oneline_svg', {})], kind === 'situatie' ? 'Situatieschema' : 'Eendraadschema');
  const svg = (r[0]?.entities?.[0]?.data as any)?.svg; if (!svg) { ctx.toast('The engine returned no drawing'); return; }
  const el = document.createElement('div'); el.className = 'dlg-host';
  el.innerHTML = `<div class="dlg" style="width:min(96vw,1100px);max-height:92vh"><div class="dlg-title" style="display:flex;align-items:center;gap:10px">${kind === 'situatie' ? `Situatieschema · storey ${level + 1}` : 'Eendraadschema'}<span style="flex:1"></span><button class="btn" data-dl>Download SVG</button><button class="btn" data-close>Close</button></div><div class="schematic" style="overflow:auto;max-height:80vh;background:#fff;border-radius:8px;padding:8px">${svg}</div></div>`;
  document.body.appendChild(el); const close = () => el.remove();
  el.querySelector<HTMLElement>('[data-close]')!.onclick = close; el.addEventListener('pointerdown', e => { if (e.target === el) close(); });
  el.querySelector<HTMLElement>('[data-dl]')!.onclick = () => { const a = document.createElement('a'); a.href = URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml' })); a.download = `${kind === 'situatie' ? `situatieschema-${level}` : 'eendraadschema'}.svg`; a.click(); };
}

function renderEntity(ctx: ToolContext, e: Entity, el: HTMLElement): boolean {
  const doc = ctx.doc(); const x = ext(doc);
  if (e.kind === 'custom' && e.type === 'electrical.conduit') {
    const d = e.data as Conduit; let L = 0; for (let i = 1; i < d.points.length; i++) L += Math.hypot(d.points[i][0] - d.points[i - 1][0], d.points[i][1] - d.points[i - 1][1], d.points[i][2] - d.points[i - 1][2]);
    el.innerHTML = `<div class="kv"><label>Type</label><div><b>Conduit</b> <span class="muted">${e.id.slice(0, 8)}</span></div>
      <label>Circuit</label><select data-f="circuit"><option value="">(none)</option>${x.circuits.map(c => `<option value="${c.id}" ${d.circuit === c.id ? 'selected' : ''}>${c.number} ${esc(c.name)}</option>`).join('')}</select>
      <label>Kind</label><select data-f="kind"><option value="conduit" ${d.kind === 'conduit' ? 'selected' : ''}>Flush conduit</option><option value="cable" ${d.kind === 'cable' ? 'selected' : ''}>Surface cable</option></select>
      <label>Diameter</label><select data-f="diameter">${[0.016, 0.02, 0.025].map(v => `<option value="${v}" ${Math.abs(d.diameter - v) < 1e-6 ? 'selected' : ''}>${Math.round(v * 1000)} mm</option>`).join('')}</select>
      <label>Points</label><div>${d.points.length}</div><label>Length</label><div>${fmtM(L)}</div></div><div class="btn-row"><button class="btn" data-erase>Erase</button></div>`;
    el.querySelectorAll<HTMLSelectElement>('[data-f]').forEach(i => i.onchange = () => ctx.ops([eop('update_conduit', { id: e.id, patch: { [i.dataset.f!]: i.dataset.f === 'diameter' ? parseFloat(i.value) : (i.value || null) } })], 'Edit conduit'));
    el.querySelector<HTMLElement>('[data-erase]')!.onclick = () => ctx.ops([{ op: 'erase', ids: [e.id] }], 'Erase').then(() => ctx.select([]));
    return true;
  }
  return false;
}

/** Extra Entity Info for an electrical fixture (rendered by the construction plugin): its circuit. */
function fixtureExtra(ctx: ToolContext, e: Entity, el: HTMLElement) {
  if (e.kind !== 'custom' || e.type !== 'construction.fixture' || !ELECTRICAL_KINDS.has((e.data as any).kind)) return;
  const x = ext(ctx.doc()); const c = circuitOf(ctx.doc(), e.id);
  const box = document.createElement('div'); box.className = 'kv'; box.style.marginTop = '8px';
  box.innerHTML = `<label>Circuit</label><select data-ec><option value="">(unassigned)</option>${x.circuits.map(k => `<option value="${k.id}" ${c?.id === k.id ? 'selected' : ''}>${k.number} ${esc(k.name)}</option>`).join('')}</select>`;
  box.querySelector<HTMLSelectElement>('[data-ec]')!.onchange = (ev) => ctx.ops([eop('assign', { points: [e.id], circuit: (ev.target as HTMLSelectElement).value || null })], 'Assign point');
  el.appendChild(box);
}

const style = document.createElement('style'); style.textContent = `.schematic svg { max-width: 100%; height: auto; } .btn.danger-text { color: var(--red); }`; document.head.appendChild(style);

P.registerPlugin({
  id: 'electrical', name: 'Electrical', version: '0.1.0', accent: '#e0b400',
  tools: [assignTool, conduitTool], toolbar: [['e-assign', 'e-conduit']],
  panels: [{ id: 'main', title: 'Electrical', render: panel }],
  renderEntity,
  activate(h) {
    host = h;
    if (xrayOn()) setXray(h.ctx, true);
    h.on('xray', () => h.refresh());
    h.on('selection', () => { const ids = [...h.selection()]; if (ids.length !== 1) return; const e = h.document().entities.find(e => e.id === ids[0]); if (!e) return; setTimeout(() => { const target = document.querySelector<HTMLElement>('[data-plugin-entity="construction"]'); if (target && !target.querySelector('[data-ec]')) fixtureExtra(h.ctx, e, target); }, 50); });
    h.on('changed', () => { if (state.findings && state.findingsVersion !== h.document().version) { state.findings = null; } });
  },
});
