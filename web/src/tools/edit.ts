import * as THREE from 'three';
import type { Tool, ToolContext, Plane, Snap } from './base';
import { partsRefused, parseNumbers, parseArray, parseAngle, onSegment, drawSnap, lineColor, planeFromSnap, isPointSnap, GROUND, arr, INK, MAGENTA, SELECT, AXIS, AXIS_COLOR } from './base';
import { planeBasis } from './draw';
import { fmt, fmtM, fmtM2, fmtDeg, findEntity, materialOf, walk, type Vec3, type Entity } from '../model';
import { V, COLOR, angleArc, type Pick } from '../scene';
import { getPlugins } from '../plugins';
import type { Op } from '../model';

/** The ops a Move produces: plugins may rewrite it (e.g. a vertical move of a whole house becomes a ground-level change). */
function moveOps(ctx: ToolContext, ids: string[], delta: THREE.Vector3, copy: boolean, points: THREE.Vector3[] | null = null): Op[] {
  const d = arr(delta);
  if (copy) return [{ op: 'copy', ids, delta: d }];
  // a corner or an edge of a shape: it moves with the faces around it
  if (points?.length) return [{ op: 'stretch', points: points.map(arr), delta: d }];
  for (const p of getPlugins()) { const ops = p.beforeMove?.(ids, d, ctx); if (ops) return ops; }
  return stickyOps(ctx, ids, { delta: d }, rest => ({ op: 'move', ids: rest, delta: d }));
}
/** The ops for moving, turning or scaling `ids`: drawn faces and loose edges take along what is attached to them
 *  (`stretch`, per container: the top level or a group); everything else goes as it is (`rigid`). */
export function stickyOps(ctx: ToolContext, ids: string[], how: { delta: Vec3 } | { rotate: { origin: Vec3; axis: Vec3; angle: number } } | { scale: { origin: Vec3; factors: Vec3 } }, rigid: (ids: string[]) => Op): Op[] {
  const sticky = new Map<string, string[]>(); const rest: string[] = [];
  for (const { entity, path } of walk(ctx.doc().entities)) {
    if (!ids.includes(entity.id)) continue;
    if (entity.kind === 'face' || entity.kind === 'edge') { const k = path.join('/'); sticky.set(k, [...(sticky.get(k) ?? []), entity.id]); } else rest.push(entity.id);
  }
  for (const id of ids) if (!rest.includes(id) && ![...sticky.values()].some(l => l.includes(id))) rest.push(id); // parts of generated objects ("id#part") and the like
  return [...[...sticky.values()].map(l => ({ op: 'stretch', ids: l, ...how }) as Op), ...(rest.length ? [rigid(rest)] : [])];
}
const hoverPick = (ev: PointerEvent, ctx: ToolContext, facesOnly = false) => { const p = ctx.scene.pick(ev, { facesOnly }); ctx.scene.highlight(p ? new Set([ctx.resolve(p.id)]) : new Set(), true); return p; };
/** Ignore geometry that exists only in the current preview (ids the document doesn't have): a tool's own
 *  pulled face must not become its inference. */
/** With a selection made, a tool works on it: other objects don't light up on hover. */
const hoverOrSelection = (ev: PointerEvent, ctx: ToolContext, facesOnly = false) => { if (ctx.selection().size) ctx.scene.highlight(new Set(), true); else hoverPick(ev, ctx, facesOnly); };
/** Selected entities that have faces: what Push/Pull and Offset work on. */
const withFaces = (ctx: ToolContext) => (id: string) => { const e = findEntity(ctx.doc(), id); return !!e && !['edge', 'guide', 'dimension', 'text', 'section'].includes(e.kind); };
/** When the selection is exactly one loose face, a face tool takes it wherever the click lands. */
function soleFace(ctx: ToolContext): { id: string; point: THREE.Vector3; normal: THREE.Vector3 } | null {
  const t = ctx.targets(); if (t.length !== 1) return null;
  const e = ctx.doc().entities.find(x => x.id === t[0]); if (!e || e.kind !== 'face' || e.outer.length < 3) return null; // top level only: a group's faces are in its own coordinates
  const o = e.outer; const c = o.reduce((a, q) => a.add(V(q)), new THREE.Vector3()).multiplyScalar(1 / o.length);
  const n = new THREE.Vector3(); // Newell, as the engine orients the face
  for (let i = 0; i < o.length; i++) { const a = o[i], b = o[(i + 1) % o.length]; n.x += (a[1] - b[1]) * (a[2] + b[2]); n.y += (a[2] - b[2]) * (a[0] + b[0]); n.z += (a[0] - b[0]) * (a[1] + b[1]); }
  return n.lengthSq() > 1e-12 ? { id: e.id, point: c, normal: n.normalize() } : null;
}
function previewOnly(ctx: ToolContext) { return (id: string) => !ctx.inDocument(id); }
/** The entities `id` shares a container with: the top level, or the group it is in. */
function containerOf(ctx: ToolContext, id: string): Entity[] | null {
  const doc = ctx.doc();
  for (const { entity, path } of walk(doc.entities)) if (entity.id === id) { const parent = path.length ? findEntity(doc, path[path.length - 1]) : null; return !path.length ? doc.entities : parent?.kind === 'group' ? parent.entities : null; }
  return null;
}
/** The drawn faces and loose edges that hang together with `id` in its container: those sharing a corner with it, directly or
 *  through others (with `only`, through those it accepts). */
export function connectedTo(ctx: ToolContext, id: string, only?: (e: Entity) => boolean): string[] {
  const list = containerOf(ctx, id);
  if (!list) return [id];
  const key = (p: Vec3) => p.map(v => Math.round(v * 1e4)).join(',');
  const at = new Map<string, string[]>(), own = new Map<string, string[]>();
  for (const e of list) {
    if (only && !only(e)) continue;
    const c = e.kind === 'face' ? [...e.outer, ...e.holes.flat()].map(key) : e.kind === 'edge' ? [key(e.a), key(e.b)] : []; if (!c.length) continue;
    own.set(e.id, c); for (const k of c) { const l = at.get(k); if (l) l.push(e.id); else at.set(k, [e.id]); }
  }
  if (!own.has(id)) return [id];
  const seen = new Set([id]), todo = [id];
  while (todo.length) for (const k of own.get(todo.pop()!)!) for (const o of at.get(k)!) if (!seen.has(o)) { seen.add(o); todo.push(o); }
  return [...seen];
}

/** Select: click, Shift adds, Ctrl toggles, Shift+Ctrl takes away, drag a box, double-click enters a group, triple-click takes everything connected. */
export class SelectTool implements Tool {
  id = 'select'; label = 'Select'; hint = 'Click to select. Shift adds, Ctrl toggles, Shift+Ctrl takes away, drag a box. Triple-click a drawn shape for everything connected to it. Double-click a group to edit inside it; Esc leaves it.'; cursor = 'default'; shortcut = ' ';
  private down: { x: number; y: number } | null = null; private box: HTMLDivElement | null = null;
  busy() { return !!this.down; }
  activate(ctx: ToolContext) { ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  deactivate(ctx: ToolContext) { ctx.scene.highlight(new Set(), true); this.box?.remove(); this.box = null; }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (this.down && (Math.abs(ev.clientX - this.down.x) > 3 || Math.abs(ev.clientY - this.down.y) > 3)) {
      if (!this.box) { this.box = document.createElement('div'); this.box.className = 'select-box'; ctx.scene.container.appendChild(this.box); }
      const r = ctx.scene.container.getBoundingClientRect();
      const x0 = Math.min(ev.clientX, this.down.x) - r.left, y0 = Math.min(ev.clientY, this.down.y) - r.top;
      Object.assign(this.box.style, { left: `${x0}px`, top: `${y0}px`, width: `${Math.abs(ev.clientX - this.down.x)}px`, height: `${Math.abs(ev.clientY - this.down.y)}px` });
      this.box.classList.toggle('crossing', ev.clientX < this.down.x);
      return;
    }
    // with a selection made, nothing else lights up on hover unless Shift / Ctrl would add to it
    if (ctx.selection().size && !ev.shiftKey && !ev.ctrlKey && !ev.metaKey) { ctx.scene.highlight(new Set(), true); return; }
    hoverPick(ev, ctx);
  }
  onDown(ev: PointerEvent, ctx: ToolContext) { if (ev.button === 0) this.down = { x: ev.clientX, y: ev.clientY }; }
  onUp(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0 || !this.down) return;
    const less = ev.shiftKey && (ev.ctrlKey || ev.metaKey); // Shift+Ctrl: take away from the selection
    const mode = ev.shiftKey ? 'add' : ev.ctrlKey || ev.metaKey ? 'toggle' : 'replace';
    const take = (ids: string[]) => less ? ctx.select([...ctx.selection()].filter(id => !ids.includes(id))) : ctx.select(ids, mode);
    if (this.box) {
      const r = ctx.scene.container.getBoundingClientRect();
      const x0 = Math.min(ev.clientX, this.down.x) - r.left, x1 = Math.max(ev.clientX, this.down.x) - r.left, y0 = Math.min(ev.clientY, this.down.y) - r.top, y1 = Math.max(ev.clientY, this.down.y) - r.top;
      const crossing = ev.clientX < this.down.x; const inside = (p: THREE.Vector3) => { const s = ctx.scene.screenPoint(p); return s.x >= x0 && s.x <= x1 && s.y >= y0 && s.y <= y1; };
      const ids = new Set<string>();
      for (const seg of ctx.scene.segments) { const a = inside(seg.a), b = inside(seg.b); if ((a && b) || (crossing && (a || b))) ids.add(ctx.resolve(seg.id)); }
      // faces: all of their outline vertices inside (window) or any (crossing)
      const faceVerts = new Map<string, THREE.Vector3[]>(); for (const seg of ctx.scene.segments) { const l = faceVerts.get(seg.id) ?? []; l.push(seg.a, seg.b); faceVerts.set(seg.id, l); }
      for (const [id, pts] of faceVerts) { const ins = pts.filter(inside).length; if ((!crossing && ins === pts.length) || (crossing && ins > 0)) ids.add(ctx.resolve(id)); }
      this.box.remove(); this.box = null; this.down = null;
      take([...ids]); return;
    }
    this.down = null;
    const p = ctx.scene.pick(ev);
    this.clicks = [this.clicks[1], p ? ctx.resolve(p.id) : null];
    if (!p) { if (mode === 'replace') ctx.select([]); return; }
    // the third click of a triple-click: everything connected to what was clicked
    if (performance.now() - this.dblAt < 500) { this.dblAt = 0; const all = connectedTo(ctx, ctx.resolve(p.id)); if (all.length > 1) { take(all); return; } }
    take([ctx.resolve(p.id)]);
  }
  private dblAt = -1e9;
  /** What the last two clicks selected: a double-click opens a group only when both landed on it (a click
   *  on empty space followed quickly by one on the group is two single clicks, not a double-click). */
  private clicks: (string | null)[] = [null, null];
  onDblClick(ev: MouseEvent, ctx: ToolContext) {
    const p = ctx.scene.pick(ev); if (!p) return;
    const g = ctx.resolve(p.id); if (this.clicks[0] !== g || this.clicks[1] !== g) return;
    (ctx as any).enterGroup?.(g); this.clicks = [null, null]; this.dblAt = performance.now();
    ctx.select([ctx.resolve(p.id)]); // inside the group now: the face (or nested group) under the cursor, as SketchUp does
  }
}

export class EraserTool implements Tool {
  id = 'eraser'; label = 'Eraser'; hint = 'Click an edge, face or object to erase it; drag to erase several. Shift hides instead. Ctrl (or Alt) softens the edges you go over, Shift+Ctrl makes them hard again (View → Hidden Geometry shows soft edges).'; cursor = 'crosshair'; shortcut = 'e';
  ownKeys: ('Control' | 'Alt')[] = ['Control', 'Alt'];
  private dragging = false; private pending = new Set<string>();
  /** What the drag does (set when it starts), and the edges gone over while softening. */
  private mode: 'erase' | 'hide' | 'soften' | 'harden' = 'erase'; private edges: { a: THREE.Vector3; b: THREE.Vector3; group?: string }[] = [];
  activate(ctx: ToolContext) { ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  deactivate(ctx: ToolContext) { ctx.scene.highlight(new Set(), true); ctx.scene.clearOverlay(); }
  /** The edge under the cursor, for softening: its ends and the group it is in. */
  private edgeAt(ev: PointerEvent, ctx: ToolContext) {
    const p = ctx.scene.pick(ev); if (!p || p.kind !== 'edge' || p.id.includes('#')) return;
    const segs = ctx.scene.segments.filter(g => g.id === p.id); if (!segs.length) return;
    const far = (g: { a: THREE.Vector3; b: THREE.Vector3 }) => new THREE.Line3(g.a, g.b).closestPointToPoint(p.point, true, new THREE.Vector3()).distanceTo(p.point);
    const g = segs.reduce((x, y) => far(y) < far(x) ? y : x);
    if (this.edges.some(e => (e.a.distanceTo(g.a) < 1e-6 && e.b.distanceTo(g.b) < 1e-6) || (e.a.distanceTo(g.b) < 1e-6 && e.b.distanceTo(g.a) < 1e-6))) return;
    let group: string | undefined; for (const { entity, path } of walk(ctx.doc().entities)) if (entity.id === p.id) { group = path[path.length - 1]; break; }
    this.edges.push({ a: g.a.clone(), b: g.b.clone(), group });
    ctx.scene.addLine([g.a, g.b], this.mode === 'soften' ? COLOR.cyan : COLOR.magenta, false, 2);
  }
  private take(ev: PointerEvent, ctx: ToolContext) {
    if (this.mode === 'soften' || this.mode === 'harden') { this.edgeAt(ev, ctx); return; }
    const p = ctx.scene.pick(ev); if (p) this.pending.add(ctx.resolve(p.id)); ctx.scene.highlight(this.pending);
  }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (this.dragging) { this.take(ev, ctx); return; }
    hoverPick(ev, ctx);
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return; this.dragging = true; this.pending.clear(); this.edges = []; ctx.scene.clearOverlay();
    const soft = ev.ctrlKey || ev.metaKey || ev.altKey;
    this.mode = soft ? (ev.shiftKey ? 'harden' : 'soften') : ev.shiftKey ? 'hide' : 'erase';
    this.take(ev, ctx);
  }
  async onUp(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0 || !this.dragging) return; this.dragging = false; ctx.scene.clearOverlay();
    if (this.mode === 'soften' || this.mode === 'harden') {
      const edges = this.edges; this.edges = [];
      if (!edges.length) { ctx.toast(this.mode === 'soften' ? 'Go over the edge between two faces to soften it.' : 'Go over a soft edge to make it hard again (View → Hidden Geometry shows them).'); return; }
      // one op per group the edges are in
      const groups = [...new Set(edges.map(e => e.group))];
      const ops: Op[] = groups.map(g => ({ op: 'soften', edges: edges.filter(e => e.group === g).map(e => [arr(e.a), arr(e.b)] as [Vec3, Vec3]), ...(this.mode === 'harden' ? { angle: 0 } : {}), ...(g ? { group: g } : {}) }));
      await ctx.ops(ops, this.mode === 'soften' ? 'Soften edges' : 'Unsoften edges');
      return;
    }
    if (this.pending.size) { const ids = [...this.pending]; const hide = this.mode === 'hide'; await ctx.ops([hide ? { op: 'set_hidden', ids, hidden: true } : { op: 'erase', ids }], hide ? 'Hide' : 'Erase'); ctx.select([]); }
    this.pending.clear();
  }
}

/** Move: drag anything; Ctrl/Alt copies; arrow keys lock an axis; type a distance. Drawn faces and loose
 *  edges take along what is attached to them; with nothing selected, a corner or an edge of a drawn shape
 *  can be taken hold of on its own (the faces around it are reshaped, and folded where they must be). */
export class MoveTool implements Tool {
  id = 'move'; label = 'Move'; hint = 'Click what to move, then click where it goes (or drag). A corner or an edge of a drawn shape moves on its own, with the faces around it. Ctrl = copy, arrows lock an axis, type a distance.'; cursor = 'move'; shortcut = 'm';
  private ids: string[] = []; private start: THREE.Vector3 | null = null; private cur: THREE.Vector3 | null = null; private plane: Plane = GROUND;
  private copy = false; private dragging = false; private lastDir: THREE.Vector3 | null = null;
  /** The corners being moved when a corner or an edge was taken hold of (else the whole of `ids` moves). */
  private points: THREE.Vector3[] | null = null;
  /** The copy just made: typing "3x" or "/3" turns it into a row of copies. */
  private lastCopy: { ids: string[]; delta: THREE.Vector3; created: string[] } | null = null;
  busy() { return !!this.start; }
  /** With nothing selected: what the cursor would take hold of. On a drawn face or loose edge of the open
   *  context that is the corner under the cursor, else the edge under it, else the whole thing. */
  private grabAt(ev: PointerEvent, ctx: ToolContext): { ids: string[]; points: THREE.Vector3[] | null } | null {
    const s = ctx.snap(ev, { faces: true });
    const p = ctx.scene.pick(ev); if (!p) return null;
    const id = ctx.resolve(p.id); const e = findEntity(ctx.doc(), id);
    if (id === p.id && !id.includes('#') && (e?.kind === 'face' || e?.kind === 'edge')) {
      const segs = ctx.scene.segments.filter(g => g.id === id);
      // (by where the snap is, not what it is called: at a corner it may well be an "intersection" of inference lines)
      if (segs.some(g => g.a.distanceTo(s.point) < 1e-4 || g.b.distanceTo(s.point) < 1e-4)) return { ids: [id], points: [s.point.clone()] };
      if (e.kind === 'face' && !['face', 'plane', 'free', 'grid'].includes(s.kind)) {
        const g = segs.find(g => onSegment(g, s.point));
        if (g) return { ids: [id], points: [g.a.clone(), g.b.clone()] };
      }
    }
    return { ids: [id], points: null };
  }
  private mark(ctx: ToolContext, pts: THREE.Vector3[]) { if (pts.length === 1) ctx.scene.addDot(pts[0], COLOR.select, 9); else ctx.scene.addLine(pts, COLOR.select, false, 3); }
  activate(ctx: ToolContext) { this.ids = []; this.start = null; this.lastCopy = null; this.shown = ''; this.idleHint(ctx); ctx.setVCB('', 'Distance'); }
  deactivate(ctx: ToolContext) { this.ids = []; this.start = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.scene.highlight(new Set(), true); }
  private shown = '';
  private idleHint(ctx: ToolContext) { const n = ctx.selection().size; const h = this.lastCopy ? 'Copied. Type 3x for three copies in a row, or /3 for three spread evenly up to this one.' : n ? `Moving the selection (${n}): click a grab point anywhere, then where it goes (or drag). Ctrl = copy, arrows lock an axis, type a distance. Esc clears the selection.` : this.hint; if (h !== this.shown) { this.shown = h; ctx.setHint(h); } }
  private snapOpts(ctx: ToolContext) { return { plane: this.plane, from: this.start ?? undefined, lock: ctx.lock, ignore: new Set(this.ids), faces: true }; }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (!this.start) { // the grab point infers like any other
      this.idleHint(ctx); ctx.scene.clearOverlay();
      if (ctx.selection().size) { ctx.scene.highlight(new Set(), true); ctx.snap(ev, { faces: true }); return; }
      const g = this.grabAt(ev, ctx);
      ctx.scene.highlight(g && !g.points ? new Set(g.ids) : new Set(), true);
      if (g?.points) this.mark(ctx, g.points);
      return;
    }
    const s = ctx.snap(ev, this.snapOpts(ctx)); this.cur = s.point;
    ctx.scene.clearOverlay(); drawSnap(ctx, s);
    const d = s.point.clone().sub(this.start);
    ctx.scene.addLine([this.start, s.point], lineColor(this.start, s.point, s), true);
    ctx.scene.addLabel(this.start.clone().lerp(s.point, 0.5), fmtM(d.length()));
    ctx.setVCB(d.length().toFixed(3), 'Distance');
    ctx.preview(d.lengthSq() > 1e-10 ? moveOps(ctx, this.ids, d, this.copy, this.points) : []);
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    if (this.start) return this.finish(ctx);
    if (partsRefused(ctx)) return;
    // a selection made beforehand is what moves, wherever the grab point is; without one, the object under the cursor
    this.points = null;
    if (!ctx.selection().size) { const g = this.grabAt(ev, ctx); if (g) { this.points = g.points; this.ids = g.ids; if (!g.points) ctx.select(g.ids); } else this.ids = []; }
    else this.ids = [...ctx.selection()];
    if (!this.ids.length) { ctx.toast('Nothing to move — click an object first'); return; }
    const s = ctx.snap(ev, { faces: true }); this.start = s.point.clone(); this.plane = planeFromSnap(s, GROUND);
    this.copy = ev.ctrlKey || ev.altKey || ev.metaKey; this.dragging = true;
    ctx.setHint(`${this.copy ? 'Copying' : 'Moving'} ${this.points ? (this.points.length === 1 ? 'a corner' : 'an edge') + ' with the faces around it' : `${this.ids.length} object(s)`}: click the destination or type a distance. Ctrl toggles copy.`);
  }
  onUp(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0 || !this.dragging) return; this.dragging = false;
    if (this.start && this.cur && this.cur.distanceTo(this.start) > 1e-3) this.finish(ctx); // a real drag ends on release; a click waits for a second click
  }
  private async finish(ctx: ToolContext, delta?: THREE.Vector3) {
    if (!this.start) return;
    const d = delta ?? (this.cur ? this.cur.clone().sub(this.start) : null);
    const ids = this.ids, points = this.points; this.lastDir = d && d.lengthSq() > 1e-10 ? d.clone().normalize() : this.lastDir;
    this.start = null; this.cur = null; ctx.preview([]); ctx.scene.clearOverlay(); this.shown = ''; this.idleHint(ctx);
    if (!d || d.lengthSq() < 1e-10) return;
    const res = await ctx.ops(moveOps(ctx, ids, d, this.copy, points), this.copy ? 'Copy' : 'Move');
    this.lastCopy = null;
    if (this.copy && res[0]?.created?.length) { ctx.select(res[0].created); this.lastCopy = { ids, delta: d.clone(), created: res[0].created }; }
    this.shown = ''; this.idleHint(ctx);
  }
  /** Replace the copy just made by a row of them: `count` copies a step apart, or dividing the step. */
  private async array(ctx: ToolContext, a: { count: number; divide: boolean }) {
    const c = this.lastCopy!; const doc = ctx.doc();
    if (!c.ids.every(id => findEntity(doc, id))) { this.lastCopy = null; return; }
    const d = a.divide ? c.delta.clone().multiplyScalar(1 / a.count) : c.delta;
    const old = c.created.filter(id => findEntity(doc, id));
    const res = await ctx.ops([...(old.length ? [{ op: 'erase', ids: old } as Op] : []), { op: 'copy', ids: c.ids, delta: arr(d), count: a.count }], 'Copy array');
    const made = res.at(-1)?.created ?? []; this.lastCopy = { ...c, created: made }; ctx.select(made);
  }
  onVCB(v: string, ctx: ToolContext) {
    const a = parseArray(v); if (a) { if (!this.start && this.lastCopy) this.array(ctx, a); return; }
    const n = parseNumbers(v); if (!n.length) return;
    if (this.start) { const dir = this.cur ? this.cur.clone().sub(this.start) : new THREE.Vector3(1, 0, 0); if (dir.lengthSq() < 1e-9) dir.set(1, 0, 0); this.finish(ctx, dir.normalize().multiplyScalar(n[0])); }
    else if (this.lastDir && ctx.selection().size && !partsRefused(ctx)) { this.ids = [...ctx.selection()]; this.points = null; this.start = new THREE.Vector3(); this.finish(ctx, this.lastDir.clone().multiplyScalar(n[0])); }
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) {
    if (ev.key === 'Escape' && this.start) { this.start = null; this.cur = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.setHint(this.hint); return true; }
    if (ev.key === 'Control' || ev.key === 'Alt') { if (this.start && ev.type === 'keydown') { this.copy = !this.copy; ctx.setHint(`${this.copy ? 'Copying' : 'Moving'}…`); } return true; }
    return false;
  }
}

/** Rotate: protractor — centre, reference direction, angle. A drawn face or loose edge turns with what is
 *  attached to it: the faces around it are reshaped, and folded where they can't stay flat. */
export class RotateTool implements Tool {
  id = 'rotate'; label = 'Rotate'; hint = 'Click the centre of rotation (on a face to rotate in its plane), then the reference point, then the angle (type it, or a slope like 1:6). Ctrl makes a copy; then 5x or /5 makes a ring of them.'; cursor = 'crosshair'; shortcut = 'q';
  private center: THREE.Vector3 | null = null; private ref: THREE.Vector3 | null = null; private plane: Plane = GROUND; private ids: string[] = []; private angle = 0;
  private copy = false;
  /** The copy just made: typing "5x" or "/5" turns it into copies around the axis. */
  private lastCopy: { ids: string[]; origin: Vec3; axis: Vec3; angle: number; created: string[] } | null = null;
  busy() { return !!this.center; }
  activate(ctx: ToolContext) { this.center = null; this.ref = null; this.lastCopy = null; ctx.setHint(this.hint); ctx.setVCB('', 'Angle'); }
  deactivate(ctx: ToolContext) { this.center = null; this.ref = null; ctx.preview([]); ctx.scene.clearOverlay(); }
  private protractor(ctx: ToolContext) {
    if (!this.center) return; const { u, w } = planeBasis(this.plane.normal); const r = ctx.scene.pixelSize(this.center) * 60; const pts: THREE.Vector3[] = [];
    for (let i = 0; i <= 48; i++) { const a = (i / 48) * Math.PI * 2; pts.push(this.center.clone().addScaledVector(u, Math.cos(a) * r).addScaledVector(w, Math.sin(a) * r)); }
    const n = this.plane.normal; const col = Math.abs(n.y) > 0.99 ? AXIS_COLOR.y : Math.abs(n.x) > 0.99 ? AXIS_COLOR.x : Math.abs(n.z) > 0.99 ? AXIS_COLOR.z : INK;
    ctx.scene.addLine(pts, col);
    for (let i = 0; i < 24; i++) { const a = (i / 24) * Math.PI * 2; const d = u.clone().multiplyScalar(Math.cos(a)).addScaledVector(w, Math.sin(a)); ctx.scene.addLine([this.center.clone().addScaledVector(d, r * (i % 6 === 0 ? 0.8 : 0.9)), this.center.clone().addScaledVector(d, r)], col); }
  }
  private turnOps(ctx: ToolContext, ids: string[], c: THREE.Vector3, n: THREE.Vector3, angle: number, copy = false): Op[] {
    const t = { origin: arr(c), axis: arr(n), angle: +angle.toFixed(4) };
    if (copy) return [{ op: 'copy', ids, rotate: t }];
    return stickyOps(ctx, ids, { rotate: t }, rest => ({ op: 'rotate', ids: rest, ...t }));
  }
  private angleTo(p: THREE.Vector3) {
    if (!this.center || !this.ref) return 0; const n = this.plane.normal; const a = this.ref.clone().sub(this.center), b = p.clone().sub(this.center);
    a.sub(n.clone().multiplyScalar(a.dot(n))); b.sub(n.clone().multiplyScalar(b.dot(n)));
    const ang = Math.atan2(a.clone().cross(b).dot(n), a.dot(b)) * 180 / Math.PI; return Math.round(ang / 15) * 15 === ang ? ang : (Math.abs(ang - Math.round(ang / 15) * 15) < 1.5 ? Math.round(ang / 15) * 15 : ang);
  }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = ctx.snap(ev, { plane: this.center ? this.plane : GROUND, faces: !this.center, ignore: new Set(this.ids) });
    ctx.scene.clearOverlay(); drawSnap(ctx, s); this.protractor(ctx);
    if (!this.center) { hoverOrSelection(ev, ctx, true); return; }
    if (!this.ref) { ctx.scene.addLine([this.center, s.point], 0x888888, true); return; }
    this.angle = this.angleTo(s.point);
    ctx.scene.addLine([this.center, this.ref], 0x888888, true); ctx.scene.addLine([this.center, s.point], MAGENTA);
    ctx.scene.addLabel(s.point, fmtDeg(this.angle)); ctx.setVCB(this.angle.toFixed(1), 'Angle');
    ctx.preview(Math.abs(this.angle) > 1e-6 ? this.turnOps(ctx, this.ids, this.center, this.plane.normal, this.angle, this.copy) : []);
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    if (!this.center) {
      if (partsRefused(ctx)) return;
      this.ids = [...ctx.selection()]; if (!this.ids.length) { const p = ctx.scene.pick(ev); if (p) { ctx.select([ctx.resolve(p.id)]); this.ids = [ctx.resolve(p.id)]; } }
      if (!this.ids.length) { ctx.toast('Select something to rotate first'); return; }
      const s = ctx.snap(ev, { faces: true }); this.center = s.point.clone(); this.plane = planeFromSnap(s, GROUND);
      if (ctx.lock) this.plane = { origin: this.center, normal: AXIS[ctx.lock].clone() };
      this.copy = ev.ctrlKey || ev.metaKey || ev.altKey; this.lastCopy = null;
      ctx.setHint('Click the reference point. Ctrl toggles copy.'); return;
    }
    const s = ctx.snap(ev, { plane: this.plane, ignore: new Set(this.ids) });
    if (!this.ref) { this.ref = s.point.clone(); ctx.setHint(`Move to ${this.copy ? 'place a rotated copy' : 'rotate'}, click to finish, or type an angle. Ctrl toggles copy.`); return; }
    this.finish(ctx, this.angleTo(s.point));
  }
  private async finish(ctx: ToolContext, angle: number) {
    const c = this.center!, n = this.plane.normal.clone(); const ids = this.ids, copy = this.copy;
    this.center = null; this.ref = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.setHint(this.hint);
    if (Math.abs(angle) < 1e-6) return;
    const res = await ctx.ops(this.turnOps(ctx, ids, c, n, angle, copy), copy ? 'Rotate copy' : 'Rotate');
    this.lastCopy = null;
    if (copy && res[0]?.created?.length) {
      ctx.select(res[0].created); this.lastCopy = { ids, origin: arr(c), axis: arr(n), angle, created: res[0].created };
      ctx.setHint('Copied. Type 5x for five copies around, or /5 for five spread evenly up to this one.');
    }
  }
  /** Replace the copy just made by `count` of them around the axis: each one the angle further, or dividing the angle. */
  private async array(ctx: ToolContext, a: { count: number; divide: boolean }) {
    const c = this.lastCopy!; const doc = ctx.doc();
    if (!c.ids.every(id => findEntity(doc, id))) { this.lastCopy = null; return; }
    const old = c.created.filter(id => findEntity(doc, id));
    const turn = { origin: c.origin, axis: c.axis, angle: +(a.divide ? c.angle / a.count : c.angle).toFixed(5) };
    const res = await ctx.ops([...(old.length ? [{ op: 'erase', ids: old } as Op] : []), { op: 'copy', ids: c.ids, rotate: turn, count: a.count }], 'Rotate array');
    const made = res.at(-1)?.created ?? []; this.lastCopy = { ...c, created: made }; ctx.select(made);
  }
  onVCB(v: string, ctx: ToolContext) {
    const a = parseArray(v); if (a) { if (!this.center && this.lastCopy) this.array(ctx, a); return; }
    const n = parseAngle(v); if (n !== null && this.center && this.ref) this.finish(ctx, n);
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) {
    if (ev.key === 'Escape') { this.center = null; this.ref = null; ctx.preview([]); ctx.scene.clearOverlay(); return true; }
    if ((ev.key === 'Control' || ev.key === 'Alt') && this.center) { if (ev.type === 'keydown') { this.copy = !this.copy; ctx.setHint(`${this.copy ? 'Copying' : 'Rotating'}: ${this.ref ? 'click to finish, or type an angle' : 'click the reference point'}. Ctrl toggles copy.`); } return true; }
    return false;
  }
}

/** Scale: uniform about the selection's centre; type a factor. */
class LegacyScaleTool implements Tool {
  id = 'scale'; label = 'Scale'; hint = 'Select objects, then drag away from or toward their centre. Type a factor (e.g. 1.5) or three (2;1;1).'; cursor = 'nwse-resize'; shortcut = 's';
  private ids: string[] = []; private center: THREE.Vector3 | null = null; private d0 = 0; private factors: Vec3 = [1, 1, 1]; private plane: Plane = GROUND;
  busy() { return !!this.center; }
  activate(ctx: ToolContext) { this.center = null; ctx.setHint(this.hint); ctx.setVCB('', 'Scale'); }
  deactivate(ctx: ToolContext) { this.center = null; ctx.preview([]); ctx.scene.clearOverlay(); }
  private bbox(ctx: ToolContext, ids: string[]) {
    const b = new THREE.Box3(); for (const s of ctx.scene.segments) if (ids.includes(ctx.resolve(s.id))) { b.expandByPoint(s.a); b.expandByPoint(s.b); } return b;
  }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (!this.center) { hoverOrSelection(ev, ctx); return; }
    const p = ctx.scene.planePoint(ev, this.plane.origin, this.plane.normal); if (!p) return;
    const f = Math.max(0.01, p.distanceTo(this.center) / (this.d0 || 1)); this.factors = [f, f, f];
    ctx.scene.clearOverlay(); ctx.scene.addLine([this.center, p], MAGENTA, true); ctx.scene.addLabel(p, `×${f.toFixed(3)}`); ctx.setVCB(f.toFixed(3), 'Scale');
    ctx.preview([{ op: 'scale', ids: this.ids, origin: arr(this.center), factors: this.factors }]);
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    if (this.center) return this.finish(ctx, this.factors);
    this.ids = [...ctx.selection()]; if (!this.ids.length) { const p = ctx.scene.pick(ev); if (p) { ctx.select([ctx.resolve(p.id)]); this.ids = [ctx.resolve(p.id)]; } }
    if (!this.ids.length) { ctx.toast('Select something to scale first'); return; }
    const b = this.bbox(ctx, this.ids); this.center = b.getCenter(new THREE.Vector3());
    const s = ctx.snap(ev, { faces: true }); this.plane = { origin: this.center, normal: this.center.clone().sub(ctx.scene.cam.camera.position).normalize().negate() };
    this.d0 = Math.max(s.point.distanceTo(this.center), 0.01); ctx.setHint('Drag to scale, click to finish, or type a factor.');
  }
  private async finish(ctx: ToolContext, f: Vec3) {
    const c = this.center!, ids = this.ids; this.center = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.setHint(this.hint);
    if (f.every(x => Math.abs(x - 1) < 1e-6)) return;
    await ctx.ops([{ op: 'scale', ids, origin: arr(c), factors: f.map(x => +x.toFixed(5)) as Vec3 }], 'Scale');
  }
  onVCB(v: string, ctx: ToolContext) {
    const n = parseNumbers(v); if (!n.length) return;
    if (!this.center) { this.ids = [...ctx.selection()]; if (!this.ids.length) return; this.center = this.bbox(ctx, this.ids).getCenter(new THREE.Vector3()); }
    this.finish(ctx, n.length >= 3 ? [n[0], n[1], n[2]] : [n[0], n[0], n[0]]);
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape') { this.center = null; ctx.preview([]); ctx.scene.clearOverlay(); return true; } return false; }
}

/** Push/Pull and Offset change drawn faces; a plugin's face (a construction wall, a pipe) follows its settings. */
function plainOnly(tool: string, ctx: ToolContext, id: string) {
  const e = findEntity(ctx.doc(), id.split('#')[0]); const what = e?.kind === 'custom' ? (e.type.split('.')[1] ?? e.type).replace(/([a-z])([A-Z])/g, '$1 $2').toLowerCase() : 'object';
  return `${tool} works on drawn faces. This face is part of a ${what}: change its size in Entity Info.`;
}
/** The snaps Push/Pull takes a height from. */
const HEIGHT_FROM = new Set(['vertex', 'midpoint', 'center', 'guide', 'origin']);
/** Push/Pull: extrude a face along its normal. */
export class PushPullTool implements Tool {
  id = 'pushpull'; label = 'Push/Pull'; hint = 'Click a face and move to extrude it; click again (or release after dragging) to finish. Type a distance. Ctrl builds on the face instead of moving it with its solid. Double-click a face to push or pull it as far as the last one.'; cursor = 'ns-resize'; shortcut = 'p';
  /** The last distance pushed or pulled (a double-click repeats it), and when the face was taken hold of. */
  private lastDist = 0; private downAt = 0;
  /** Ctrl: the face stays where it is and a new shape is built on it (SketchUp's "new starting face"). */
  private fresh = false;
  private op(face: string, d: number, fresh = this.fresh): Op { return { op: 'push_pull', face, distance: +d.toFixed(5), ...(fresh ? { newFace: true } : {}) }; }
  private face: { id: string; point: THREE.Vector3; normal: THREE.Vector3 } | null = null; private dist = 0; private dragging = false; private lastFace: string | null = null; private ignore: (id: string) => boolean = () => false;
  busy() { return !!this.face; }
  activate(ctx: ToolContext) { this.face = null; ctx.setHint(this.hint); ctx.setVCB('', 'Distance'); }
  deactivate(ctx: ToolContext) { this.face = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.scene.highlight(new Set(), true); }
  /** The face to pull: under the cursor (only the selection's faces when something with faces is selected), else the one selected face. */
  private target(ev: { clientX: number; clientY: number }, ctx: ToolContext) {
    const p = ctx.pickTarget(ev, { facesOnly: true, accept: withFaces(ctx) });
    if (p) { const n = p.back ? p.normal.clone().negate() : p.normal.clone(); return { id: p.id, point: p.point.clone(), normal: n.normalize() }; }
    return soleFace(ctx);
  }
  onShiftHold() { return this.face ? 'Holding the push/pull direction' : null; }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (!this.face) { const f = this.target(ev, ctx); ctx.scene.highlight(f ? new Set([f.id]) : new Set(), true); ctx.snap(ev, { faces: true }); return; }
    // a point the cursor rests on sets the distance (SketchUp takes heights from other geometry): only real points
    // (corners, midpoints, centres), not any edge or crossing nearby; Shift keeps strictly to the normal
    const s = ev.shiftKey ? null : ctx.snap(ev, { faces: false, ignoreIf: this.ignore, reach: 0.5 }); // only right on a point: sweeping past a busy model must not catch every corner on the way
    if (!s) ctx.scene.setSnapMarker(null);
    let d: number;
    const fromPoint = s && HEIGHT_FROM.has(s.kind) ? s.point.clone().sub(this.face.point).dot(this.face.normal) : 0;
    if (s && Math.abs(fromPoint) > 1e-4) d = fromPoint; // (the pulled face's own corners would only say "no height")
    else { d = ctx.scene.linePoint(ev, this.face.point, this.face.normal).clone().sub(this.face.point).dot(this.face.normal); if (ctx.options.gridSnap) d = Math.round(d * 100) / 100; }
    this.dist = d;
    ctx.scene.clearOverlay(); const tip = this.face.point.clone().addScaledVector(this.face.normal, d);
    ctx.scene.addLine([this.face.point, tip], MAGENTA, true); ctx.scene.addLabel(tip, fmtM(Math.abs(d))); ctx.setVCB(d.toFixed(3), 'Distance');
    ctx.preview(Math.abs(d) > 1e-4 ? [this.op(this.face.id, d)] : []);
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    if (this.face) {
      // the second click of a double-click: let go of the face, the double-click repeats the last distance
      if (performance.now() - this.downAt < 400 && Math.abs(this.dist) < 0.02) { this.face = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.setHint(this.hint); return; }
      return this.finish(ctx, this.dist);
    }
    const f = this.target(ev, ctx);
    if (!f) { if (ctx.targets(withFaces(ctx)).length) ctx.toast('Click a face of the selection (Esc clears the selection)'); return; }
    if (f.id.includes('#')) { ctx.toast(plainOnly('Push/Pull', ctx, f.id)); return; }
    this.face = f; this.dragging = true; this.dist = 0; this.ignore = previewOnly(ctx); this.downAt = performance.now(); this.fresh = ev.ctrlKey || ev.metaKey;
    ctx.setHint('Move to set the distance; click or release to finish; type a distance for precision. Ctrl builds on the face instead of moving it.');
  }
  onUp(ev: PointerEvent, ctx: ToolContext) { if (ev.button !== 0 || !this.dragging) return; this.dragging = false; if (this.face && Math.abs(this.dist) > 0.02) this.finish(ctx, this.dist); }
  private async finish(ctx: ToolContext, d: number) {
    const f = this.face; const fresh = this.fresh; this.face = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.setHint(this.hint);
    if (!f || Math.abs(d) < 1e-4) return;
    await ctx.ops([this.op(f.id, d, fresh)], 'Push/Pull'); this.lastFace = f.id; this.lastDist = d;
  }
  /** Double-click a face: the same distance as the last push or pull. */
  onDblClick(ev: MouseEvent, ctx: ToolContext) {
    if (this.face || Math.abs(this.lastDist) < 1e-4) return;
    const f = this.target(ev, ctx); if (!f || f.id.includes('#')) return;
    ctx.ops([{ op: 'push_pull', face: f.id, distance: +this.lastDist.toFixed(5) }], 'Push/Pull'); this.lastFace = f.id;
  }
  onVCB(v: string, ctx: ToolContext) {
    const n = parseNumbers(v); if (!n.length) return;
    if (this.face) this.finish(ctx, Math.sign(this.dist || 1) * n[0]);
    else if (this.lastFace) ctx.ops([{ op: 'push_pull', face: this.lastFace, distance: n[0] }], 'Push/Pull');
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) {
    if (ev.key === 'Escape' && this.face) { this.face = null; ctx.preview([]); ctx.scene.clearOverlay(); return true; }
    if (ev.key === 'Control' && this.face) {
      if (ev.type !== 'keydown') return true;
      this.fresh = !this.fresh; ctx.setHint(this.fresh ? 'Building on the face: it stays where it is. Ctrl switches back.' : 'Moving the face with its solid. Ctrl builds on it instead.');
      ctx.preview(Math.abs(this.dist) > 1e-4 ? [this.op(this.face.id, this.dist)] : []); return true;
    }
    return false;
  }
}

type Chain = { pts: THREE.Vector3[]; closed: boolean; normal: THREE.Vector3 };
/** Two or more selected loose edges that hang together as one line in one plane: what Offset takes instead of a face. */
function edgeChain(ctx: ToolContext): Chain | null {
  const doc = ctx.doc(); const ids = [...ctx.selection()]; if (ids.length < 2 || !ids.every(id => findEntity(doc, id)?.kind === 'edge')) return null;
  const left: { a: THREE.Vector3; b: THREE.Vector3 }[] = []; for (const id of ids) { const g = ctx.scene.segments.find(g => g.id === id); if (!g) return null; left.push(g); }
  const same = (p: THREE.Vector3, q: THREE.Vector3) => p.distanceTo(q) < 1e-4;
  const ends = left.flatMap(g => [g.a, g.b]); const degree = (p: THREE.Vector3) => ends.filter(q => same(p, q)).length;
  if (ends.some(p => degree(p) > 2)) return null; // a fork
  const closed = !ends.some(p => degree(p) === 1); let cur = ends.find(p => degree(p) === 1) ?? ends[0]; const pts = [cur.clone()];
  for (;;) { const i = left.findIndex(g => same(g.a, cur) || same(g.b, cur)); if (i < 0) break; const g = left.splice(i, 1)[0]; cur = same(g.a, cur) ? g.b : g.a; pts.push(cur.clone()); }
  if (left.length) return null; // more than one line
  if (closed) pts.pop();
  const n = new THREE.Vector3(); for (let i = 2; i < pts.length && n.lengthSq() < 1e-10; i++) n.crossVectors(pts[1].clone().sub(pts[0]), pts[i].clone().sub(pts[0]));
  if (n.lengthSq() < 1e-10) return null; // a straight line has no plane of its own
  n.normalize(); if (pts.some(p => Math.abs(p.clone().sub(pts[0]).dot(n)) > 1e-3)) return null;
  return { pts, closed, normal: n };
}
/** The line `d` to the left of the chain (as seen against its normal), mitred at the corners. */
function offsetChain(c: Chain, d: number): THREE.Vector3[] {
  const k = c.pts.length; const side = (i: number) => c.normal.clone().cross(c.pts[(i + 1) % k].clone().sub(c.pts[i])).normalize();
  return c.pts.map((p, j) => {
    const before = c.closed ? side((j + k - 1) % k) : j > 0 ? side(j - 1) : null; const after = c.closed || j < k - 1 ? side(j) : null;
    if (!before || !after) return p.clone().addScaledVector((before ?? after)!, d);
    const mid = before.clone().add(after); if (mid.lengthSq() < 1e-8) return p.clone().addScaledVector(after, d); // (doubling back)
    mid.normalize(); return p.clone().addScaledVector(mid, d / mid.dot(after));
  });
}
/** How far `p` is from the chain, positive on its left. */
function chainDistance(c: Chain, p: THREE.Vector3): number {
  let best = Infinity, d = 0; const k = c.pts.length;
  for (let i = 0; i < (c.closed ? k : k - 1); i++) {
    const a = c.pts[i], ab = c.pts[(i + 1) % k].clone().sub(a); const q = a.clone().addScaledVector(ab, THREE.MathUtils.clamp(p.clone().sub(a).dot(ab) / ab.lengthSq(), 0, 1)); const dist = q.distanceTo(p);
    if (dist < best) { best = dist; d = (p.clone().sub(q).dot(c.normal.clone().cross(ab)) < 0 ? -1 : 1) * dist; }
  }
  return d;
}

/** Offset: copy a face's outline inward or outward; or, with connected edges selected, copy that line to one side. */
export class OffsetTool implements Tool {
  id = 'offset'; label = 'Offset'; hint = 'Click a face and move inward or outward; click to finish. Type a distance. Double-click a face to offset it as far as the last one. With two or more connected edges selected, click and move to copy that line to one side.'; cursor = 'crosshair'; shortcut = 'f';
  private lastDist = 0; private downAt = 0;
  private face: { id: string; plane: Plane; start: THREE.Vector3; center: THREE.Vector3 } | null = null; private dist = 0; private ignore: (id: string) => boolean = () => false;
  /** The selected edges being offset as a line. */
  private line: Chain | null = null;
  busy() { return !!this.face || !!this.line; }
  activate(ctx: ToolContext) { this.face = null; this.line = null; ctx.setHint(this.hint); ctx.setVCB('', 'Distance'); }
  deactivate(ctx: ToolContext) { this.face = null; this.line = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.scene.highlight(new Set(), true); }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (this.line) {
      const p = ctx.scene.planePoint(ev, this.line.pts[0], this.line.normal); if (!p) return;
      let d = chainDistance(this.line, p); if (ctx.options.gridSnap) d = Math.round(d * 100) / 100; this.dist = d;
      const o = offsetChain(this.line, d); ctx.scene.clearOverlay(); ctx.scene.setSnapMarker(null);
      ctx.scene.addLine(this.line.closed ? [...o, o[0]] : o, MAGENTA, false, 2); ctx.scene.addLabel(p, fmtM(Math.abs(d))); ctx.setVCB(d.toFixed(3), 'Distance'); return;
    }
    if (!this.face) { const p = ctx.pickTarget(ev, { facesOnly: true, accept: withFaces(ctx) }); const id = p?.id ?? soleFace(ctx)?.id; ctx.scene.highlight(id ? new Set([id]) : new Set(), true); ctx.snap(ev, { faces: true }); return; }
    // a point under the cursor (projected onto the face's plane) sets the offset; else the cursor on the plane
    const s = ctx.snap(ev, { plane: this.face.plane, faces: false, ignoreIf: this.ignore });
    const n = this.face.plane.normal; const p = s.point.clone().sub(n.clone().multiplyScalar(s.point.clone().sub(this.face.plane.origin).dot(n)));
    let d = p.distanceTo(this.face.center) - this.face.start.distanceTo(this.face.center);
    if (!isPointSnap(s) && ctx.options.gridSnap) d = Math.round(d * 100) / 100; this.dist = d;
    ctx.scene.clearOverlay(); ctx.scene.addLine([this.face.start, p], MAGENTA, true); ctx.scene.addLabel(p, fmtM(Math.abs(d))); ctx.setVCB(d.toFixed(3), 'Distance');
    ctx.preview(Math.abs(d) > 1e-4 ? [{ op: 'offset', face: this.face.id, distance: +d.toFixed(5) }] : []);
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    if (this.face) {
      if (performance.now() - this.downAt < 400 && Math.abs(this.dist) < 0.02) { this.face = null; ctx.preview([]); ctx.scene.clearOverlay(); return; } // (the second click of a double-click)
      return this.finish(ctx, this.dist);
    }
    if (this.line) return this.finishLine(ctx, this.dist);
    this.downAt = performance.now(); this.dist = 0;
    const chain = edgeChain(ctx); if (chain) { this.line = chain; ctx.scene.highlight(new Set(), true); ctx.setHint('Move to the side the copy goes and click, or type a distance.'); return; }
    let p: { id: string; point: THREE.Vector3; normal: THREE.Vector3 } | null = ctx.pickTarget(ev, { facesOnly: true, accept: withFaces(ctx) });
    if (!p) { const f = soleFace(ctx); const q = f && ctx.scene.planePoint(ev, f.point, f.normal); if (f && q) p = { id: f.id, point: q, normal: f.normal }; } // the selected face, measured from where the click lands on its plane
    if (!p) { if (ctx.targets(withFaces(ctx)).length) ctx.toast('Click a face of the selection (Esc clears the selection)'); return; }
    if (p.id.includes('#')) { ctx.toast(plainOnly('Offset', ctx, p.id)); return; }
    const e = findEntity(ctx.doc(), p.id); const center = e && e.kind === 'face' ? e.outer.reduce((c, q) => c.add(V(q)), new THREE.Vector3()).multiplyScalar(1 / e.outer.length) : p.point.clone();
    this.face = { id: p.id, plane: { origin: p.point.clone(), normal: p.normal.clone() }, start: p.point.clone(), center }; this.ignore = previewOnly(ctx);
  }
  private async finish(ctx: ToolContext, d: number) {
    const f = this.face; this.face = null; ctx.preview([]); ctx.scene.clearOverlay();
    if (!f || Math.abs(d) < 1e-4) return;
    const res = await ctx.ops([{ op: 'offset', face: f.id, distance: +d.toFixed(5) }], 'Offset'); this.lastDist = d; if (res[0]?.created?.[0]) ctx.select([res[0].created[0]]);
  }
  private async finishLine(ctx: ToolContext, d: number) {
    const c = this.line; this.line = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint);
    if (!c || Math.abs(d) < 1e-4) return;
    const res = await ctx.ops([{ op: 'draw_line', points: offsetChain(c, d).map(arr), close: c.closed, layer: ctx.options.layer }], 'Offset');
    const doc = ctx.doc(); const made = (res[0]?.created ?? []).filter(id => findEntity(doc, id)?.kind === 'edge'); if (made.length) ctx.select(made);
  }
  /** Double-click a face: the same offset as the last one. */
  async onDblClick(ev: MouseEvent, ctx: ToolContext) {
    if (this.face || this.line || Math.abs(this.lastDist) < 1e-4) return;
    const p = ctx.pickTarget(ev, { facesOnly: true, anywhere: true }); if (!p || p.id.includes('#')) return;
    const res = await ctx.ops([{ op: 'offset', face: p.id, distance: +this.lastDist.toFixed(5) }], 'Offset'); if (res[0]?.created?.[0]) ctx.select([res[0].created[0]]);
  }
  onVCB(v: string, ctx: ToolContext) { const n = parseNumbers(v); if (!n.length) return; const d = Math.sign(this.dist || 1) * n[0]; if (this.line) this.finishLine(ctx, d); else if (this.face) this.finish(ctx, d); }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && (this.face || this.line)) { this.face = null; this.line = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.setHint(this.hint); return true; } return false; }
}

/** Paint Bucket: apply the active material to a face. Ctrl fills the connected faces that look the same, Shift+Ctrl
 *  every such face of the shape or group, Shift the whole object; Alt samples. */
export class PaintTool implements Tool {
  id = 'paint'; label = 'Paint Bucket'; hint = 'Click a face to paint it with the active material. Ctrl paints every connected face with the same material, Shift+Ctrl every such face of the shape. Shift paints the whole object, Alt samples.'; cursor = 'cell'; shortcut = 'b';
  ownKeys: ('Control' | 'Alt')[] = ['Control', 'Alt'];
  activate(ctx: ToolContext) { ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  deactivate(ctx: ToolContext) { ctx.scene.highlight(new Set(), true); }
  /** What a click on `p` paints. */
  private fill(ev: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean }, ctx: ToolContext, p: Pick): string[] {
    if (!(ev.ctrlKey || ev.metaKey)) return [ev.shiftKey ? ctx.resolve(p.id) : p.id];
    const list = containerOf(ctx, p.id); const hit = list?.find(e => e.id === p.id); if (!list || hit?.kind !== 'face') return [p.id];
    // the material on the side that was clicked
    const look = (e: Entity) => e.kind === 'face' ? ((p.back ? e.backMaterial : e.material) ?? null) : undefined; const want = look(hit);
    const alike = (e: Entity) => look(e) === want;
    return ev.shiftKey ? list.filter(alike).map(e => e.id) : connectedTo(ctx, p.id, alike);
  }
  onMove(ev: PointerEvent, ctx: ToolContext) { const p = ctx.scene.pick(ev, { facesOnly: true }); ctx.scene.highlight(p ? new Set(this.fill(ev, ctx, p)) : new Set(), true); }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const p = ctx.scene.pick(ev, { facesOnly: true }); if (!p) return;
    if (ev.altKey) { if (p.material) { ctx.options.material = p.material; ctx.toast(`Sampled ${materialOf(ctx.doc(), p.material)?.name ?? p.material}`); } return; }
    const whole = ev.shiftKey && !(ev.ctrlKey || ev.metaKey);
    await ctx.ops([{ op: 'paint', ids: this.fill(ev, ctx, p), material: ctx.options.material, side: whole ? 'both' : p.back ? 'back' : 'front' }], 'Paint');
  }
}

/** Tape Measure: measure between two points; from an edge, drag a parallel guide line; type a distance for a guide. */
export class TapeTool implements Tool {
  id = 'tape'; label = 'Tape Measure'; hint = 'Click two points to measure. Start on an edge to pull a parallel guide line; type a distance to place it exactly. After measuring, type a new length to resize the whole model to it.'; cursor = 'crosshair'; shortcut = 't';
  /** The length last measured between two points: a length typed afterwards resizes the model so that it becomes that. */
  private measured = 0;
  private a: Snap | null = null; private cur: Snap | null = null; private guideDir: THREE.Vector3 | null = null; private plane: Plane = GROUND;
  busy() { return !!this.a; }
  activate(ctx: ToolContext) { this.a = null; this.measured = 0; ctx.setHint(this.hint); ctx.setVCB('', 'Length'); }
  deactivate(ctx: ToolContext) { this.a = null; ctx.scene.clearOverlay(); }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = ctx.snap(ev, { plane: this.a ? this.plane : GROUND, from: this.a?.point, lock: ctx.lock, faces: true }); this.cur = s;
    ctx.scene.clearOverlay(); drawSnap(ctx, s);
    if (!this.a) return;
    const L = this.a.point.distanceTo(s.point);
    if (this.guideDir) { const g = s.point; ctx.scene.addLine([g.clone().addScaledVector(this.guideDir, -300), g.clone().addScaledVector(this.guideDir, 300)], COLOR.cyan, true); }
    ctx.scene.addLine([this.a.point, s.point], MAGENTA, true); ctx.scene.addLabel(this.a.point.clone().lerp(s.point, 0.5), fmtM(L), 'dim'); ctx.setVCB(L.toFixed(3), 'Length');
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const s = ctx.snap(ev, { plane: this.a ? this.plane : GROUND, from: this.a?.point, lock: ctx.lock, faces: true });
    if (!this.a) {
      this.a = s; this.plane = planeFromSnap(s, GROUND); this.guideDir = null;
      if (s.kind === 'edge' && s.dir) this.guideDir = s.dir.clone();
      ctx.setHint(this.guideDir ? 'Move to place a guide parallel to the edge, click or type a distance.' : 'Click the second point to measure (a guide point is placed there with Ctrl).');
      return;
    }
    this.finish(ctx, s.point, ev.ctrlKey || ev.metaKey);
  }
  private async finish(ctx: ToolContext, p: THREE.Vector3, guidePoint: boolean) {
    const a = this.a!; this.a = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint);
    const L = a.point.distanceTo(p); ctx.setVCB(L.toFixed(3), 'Length'); ctx.toast(`Distance: ${fmtM(L)}`);
    this.measured = this.guideDir || guidePoint ? 0 : L;
    if (this.measured > 1e-6) ctx.setHint(`Measured ${fmtM(L)}. Type a new length to resize the whole model so this becomes it, or click to measure again.`);
    if (this.guideDir) await ctx.ops([{ op: 'add_guide', point: arr(p), direction: arr(this.guideDir) }], 'Guide line');
    else if (guidePoint) await ctx.ops([{ op: 'add_guide', point: arr(p) }], 'Guide point');
  }
  onVCB(v: string, ctx: ToolContext) {
    const n = parseNumbers(v); if (!n.length) return;
    if (!this.a) { if (this.measured > 1e-6 && n[0] > 0) this.resize(ctx, n[0]); return; }
    if (!this.cur) return;
    const dir = this.cur.point.clone().sub(this.a.point); if (dir.lengthSq() < 1e-9) return; dir.normalize();
    this.finish(ctx, this.a.point.clone().addScaledVector(dir, n[0]), true);
  }
  /** Scale everything about the origin so that the length just measured becomes `to`. */
  private async resize(ctx: ToolContext, to: number) {
    const from = this.measured, k = to / from; const ids = ctx.doc().entities.map(e => e.id); if (Math.abs(k - 1) < 1e-9 || !ids.length) return;
    if (!await ctx.confirm(`Resize the whole model so that ${fmtM(from)} becomes ${fmtM(to)} (×${k.toFixed(3)})?`, 'Resize')) return;
    await ctx.ops([{ op: 'scale', ids, origin: [0, 0, 0], factors: [k, k, k] }], 'Resize model'); this.measured = to; ctx.setHint(this.hint);
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape') { this.a = null; ctx.scene.clearOverlay(); return true; } return false; }
}

/** Text: click a point, an edge or a face for a label with a leader (it starts as the position, the length or the area);
 *  click empty space for screen text, which stays where it is in the view. */
export class TextTool implements Tool {
  id = 'text'; label = 'Text'; hint = 'Click a point, an edge or a face to attach a label: it starts as the position, the length or the area. Click empty space for text that stays in place on the screen.'; cursor = 'text'; shortcut = 'x';
  activate(ctx: ToolContext) { ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  deactivate(ctx: ToolContext) { ctx.scene.clearOverlay(); }
  onMove(ev: PointerEvent, ctx: ToolContext) { const s = ctx.snap(ev, { faces: true }); ctx.scene.clearOverlay(); drawSnap(ctx, s); }
  /** What a label on this spot says unless something else is typed. */
  private auto(ctx: ToolContext, s: Snap, p: Pick | null): string {
    if (s.kind === 'edge' || s.kind === 'midpoint') { const g = ctx.scene.segments.find(g => onSegment(g, s.point)); if (g) return fmtM(g.a.distanceTo(g.b)); }
    if (isPointSnap(s)) return `${[s.point.x, s.point.y, s.point.z].map(v => fmt(v, 3)).join('; ')} m`;
    const e = p ? findEntity(ctx.doc(), p.id.split('#')[0]) : undefined; if (e?.kind !== 'face') return '';
    const area = (l: Vec3[]) => { const n = new THREE.Vector3(); for (let i = 0; i < l.length; i++) { const a = l[i], b = l[(i + 1) % l.length]; n.x += (a[1] - b[1]) * (a[2] + b[2]); n.y += (a[2] - b[2]) * (a[0] + b[0]); n.z += (a[0] - b[0]) * (a[1] + b[1]); } return n.length() / 2; };
    return fmtM2(area(e.outer) - e.holes.reduce((t, h) => t + area(h), 0));
  }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const s = ctx.snap(ev, { faces: true }); const p = ctx.scene.pick(ev);
    if (!p && !isPointSnap(s)) { // empty space: screen text
      const r = ctx.scene.container.getBoundingClientRect(); const f = (v: number) => +Math.min(1, Math.max(0, v)).toFixed(4);
      const at: [number, number] = [f((ev.clientX - r.left) / ctx.scene.width), f((ev.clientY - r.top) / ctx.scene.height)];
      const text = await ctx.prompt('Screen text', ''); if (!text) return;
      await ctx.ops([{ op: 'add_text', text, screen: at }], 'Screen text'); return;
    }
    const text = await ctx.prompt('Label text', this.auto(ctx, s, p)); if (!text) return;
    const off = ctx.scene.pixelSize(s.point) * 40; const up = new THREE.Vector3().setFromMatrixColumn(ctx.scene.cam.camera.matrix, 1), right = new THREE.Vector3().setFromMatrixColumn(ctx.scene.cam.camera.matrix, 0);
    const pos = s.point.clone().addScaledVector(up, off).addScaledVector(right, off);
    await ctx.ops([{ op: 'add_text', position: arr(pos), text, anchor: arr(s.point) }], 'Text');
  }
}

/** Dimension: two points (or an edge), then the offset; on a circle or an arc, its diameter or radius. */
export class DimensionTool implements Tool {
  id = 'dimension'; label = 'Dimension'; hint = 'Click the two points to dimension (or an edge, for its length), then click where the dimension line goes. Click a circle for its diameter, an arc for its radius.'; cursor = 'crosshair'; shortcut = 'd';
  private a: THREE.Vector3 | null = null; private b: THREE.Vector3 | null = null; private plane: Plane = GROUND;
  /** The circle or arc being dimensioned. */
  private curve: { c: THREE.Vector3; n: THREE.Vector3; r: number; style: 'radius' | 'diameter' } | null = null;
  busy() { return !!this.a || !!this.curve; }
  activate(ctx: ToolContext) { this.a = this.b = null; this.curve = null; ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  deactivate(ctx: ToolContext) { this.a = this.b = null; this.curve = null; ctx.scene.clearOverlay(); }
  /** The circle or arc the edge under `p` is a piece of: a full circle gets its diameter, an arc its radius. (The sides of a
   *  square or a hexagon are edges like any other: only fine-stepped curves count.) */
  private curveAt(ctx: ToolContext, p: THREE.Vector3) {
    const g = ctx.scene.segments.find(g => onSegment(g, p) || g.a.distanceTo(p) < 1e-4 || g.b.distanceTo(p) < 1e-4); if (!g) return null;
    for (const k of ctx.scene.compiled?.centers ?? []) {
      const c = V(k.point), n = V(k.normal), r = k.radius; const on = (q: THREE.Vector3) => Math.abs(q.distanceTo(c) - r) < 1e-3 && Math.abs(q.clone().sub(c).dot(n)) < 1e-3;
      const turn = (x: { a: THREE.Vector3; b: THREE.Vector3 }) => 2 * Math.asin(Math.min(1, x.a.distanceTo(x.b) / (2 * r)));
      if (r < 1e-6 || !on(g.a) || !on(g.b) || turn(g) > 0.55) continue;
      const seen = new Set<string>(); let round = 0; // (two faces that share the curve each bring its edges)
      for (const x of ctx.scene.segments) { if (!on(x.a) || !on(x.b)) continue; const key = [x.a, x.b].map(q => `${Math.round(q.x * 1e3)},${Math.round(q.y * 1e3)},${Math.round(q.z * 1e3)}`).sort().join('|'); if (!seen.has(key)) { seen.add(key); round += turn(x); } }
      return { c, n, r, style: round > Math.PI * 2 - 0.05 ? 'diameter' as const : 'radius' as const };
    }
    return null;
  }
  /** Where the dimension of the curve goes for the cursor: across (or out from the centre) towards it, the label at the cursor. */
  private curveDim(ev: { clientX: number; clientY: number }, ctx: ToolContext) {
    const k = this.curve!; const p = ctx.scene.planePoint(ev, k.c, k.n); if (!p) return null; const dir = p.clone().sub(k.c); if (dir.lengthSq() < 1e-10) return null; dir.normalize();
    const b = k.c.clone().addScaledVector(dir, k.r); return { a: k.style === 'diameter' ? k.c.clone().addScaledVector(dir, -k.r) : k.c.clone(), b, p };
  }
  private offsetFor(p: THREE.Vector3) { const d = this.b!.clone().sub(this.a!).normalize(); const o = p.clone().sub(this.b!); return o.sub(d.multiplyScalar(o.dot(d))); }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (this.curve) {
      ctx.scene.clearOverlay(); ctx.scene.setSnapMarker(null); const d = this.curveDim(ev, ctx); if (!d) return;
      ctx.scene.addLine([d.a, d.b, d.p], INK, false, 2); ctx.scene.addLabel(d.p, this.curve.style === 'diameter' ? `⌀ ${fmtM(this.curve.r * 2)}` : `R ${fmtM(this.curve.r)}`, 'dim'); return;
    }
    const s = ctx.snap(ev, { plane: this.plane, faces: !this.a, from: this.a && !this.b ? this.a : undefined, lock: ctx.lock }); ctx.scene.clearOverlay(); drawSnap(ctx, s);
    if (this.a && !this.b) { ctx.scene.addLine([this.a, s.point], lineColor(this.a, s.point, s), true); ctx.scene.addLabel(this.a.clone().lerp(s.point, 0.5), fmtM(this.a.distanceTo(s.point))); }
    if (this.a && this.b) {
      const o = this.offsetFor(s.point); const a2 = this.a.clone().add(o), b2 = this.b.clone().add(o);
      ctx.scene.addLine([this.a, a2], INK); ctx.scene.addLine([this.b, b2], INK); ctx.scene.addLine([a2, b2], INK, false, 2); ctx.scene.addLabel(a2.clone().lerp(b2, 0.5), fmtM(this.a.distanceTo(this.b)), 'dim');
    }
  }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    if (this.curve) {
      const d = this.curveDim(ev, ctx); if (!d) return; const style = this.curve.style; this.curve = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint);
      await ctx.ops([{ op: 'add_dimension', a: arr(d.a), b: arr(d.b), offset: arr(d.p.clone().sub(d.b)), style }], style === 'diameter' ? 'Diameter' : 'Radius'); return;
    }
    const s = ctx.snap(ev, { plane: this.plane, faces: !this.a, from: this.a && !this.b ? this.a : undefined, lock: ctx.lock });
    if (!this.a) {
      this.curve = ['edge', 'midpoint', 'vertex'].includes(s.kind) ? this.curveAt(ctx, s.point) : null;
      if (this.curve) { ctx.setHint(`Click where the ${this.curve.style} goes. R and D switch between radius and diameter.`); return; }
      this.plane = planeFromSnap(s, GROUND);
      // on an edge (not at one of its points): the edge itself is dimensioned
      const g = s.kind === 'edge' ? ctx.scene.segments.find(g => onSegment(g, s.point)) : undefined;
      if (g) { this.a = g.a.clone(); this.b = g.b.clone(); ctx.setHint('Click where the dimension line goes.'); } else this.a = s.point.clone();
      return;
    }
    if (!this.b) { if (s.point.distanceTo(this.a) < 1e-3) return; this.b = s.point.clone(); ctx.setHint('Click where the dimension line goes.'); return; }
    const o = this.offsetFor(s.point); const a = this.a, b = this.b; this.a = this.b = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint);
    await ctx.ops([{ op: 'add_dimension', a: arr(a), b: arr(b), offset: arr(o) }], 'Dimension');
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) {
    if (ev.key === 'Escape') { this.a = this.b = null; this.curve = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint); return true; }
    const k = ev.key.toLowerCase();
    if (this.curve && ev.type === 'keydown' && (k === 'r' || k === 'd')) { this.curve.style = k === 'r' ? 'radius' : 'diameter'; ctx.setHint(`Click where the ${this.curve.style} goes. R and D switch between radius and diameter.`); return true; }
    return false;
  }
}
/** Angular Dimension: the angle between two edges, or between two legs from a corner; it stays in the model
 *  as an arc with the angle in degrees. Which side of each edge is clicked decides which of the angles is meant. */
export class AngleDimensionTool implements Tool {
  id = 'angledim'; label = 'Angular Dimension'; hint = 'Click the two edges of the angle (or its corner, then a point on each leg), then click where the arc goes.'; cursor = 'crosshair';
  private edge: { a: THREE.Vector3; b: THREE.Vector3; p: THREE.Vector3 } | null = null;
  private c: THREE.Vector3 | null = null; private legs: THREE.Vector3[] = [];
  busy() { return !!this.c || !!this.edge; }
  private reset(ctx: ToolContext) { this.edge = null; this.c = null; this.legs = []; ctx.scene.clearOverlay(); ctx.setHint(this.hint); }
  activate(ctx: ToolContext) { this.reset(ctx); ctx.setVCB('', 'Angle'); }
  deactivate(ctx: ToolContext) { this.reset(ctx); }
  private segAt(ctx: ToolContext, p: THREE.Vector3) { return ctx.scene.segments.find(g => onSegment(g, p)) ?? ctx.scene.segments.find(g => g.a.distanceTo(p) < 1e-4 || g.b.distanceTo(p) < 1e-4); }
  /** Where two edges meet: the corner they share, or where their lines cross. */
  private corner(g: { a: THREE.Vector3; b: THREE.Vector3 }, h: { a: THREE.Vector3; b: THREE.Vector3 }): THREE.Vector3 | null {
    for (const p of [g.a, g.b]) for (const q of [h.a, h.b]) if (p.distanceTo(q) < 1e-4) return p.clone();
    const d1 = g.b.clone().sub(g.a), d2 = h.b.clone().sub(h.a), w = g.a.clone().sub(h.a); const n = d1.clone().cross(d2);
    if (n.lengthSq() < 1e-12 * d1.lengthSq() * d2.lengthSq()) return null; // parallel
    if (Math.abs(w.dot(n.clone().normalize())) > 1e-3) return null; // they pass each other
    const t = d2.clone().cross(n).dot(w) / -n.lengthSq(); return g.a.clone().addScaledVector(d1, t);
  }
  private snap(ev: { clientX: number; clientY: number }, ctx: ToolContext) { return ctx.snap(ev, { faces: true, from: this.c ?? undefined }); }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = this.snap(ev, ctx); ctx.scene.clearOverlay(); drawSnap(ctx, s);
    if (this.edge) ctx.scene.addLine([this.edge.a, this.edge.b], MAGENTA, false, 2);
    if (!this.c) return;
    const to = s.point.clone().sub(this.c); const r = to.length(); if (r < 1e-4) return;
    if (this.legs.length < 2) { // the legs so far, and the one towards the cursor
      for (const l of this.legs) ctx.scene.addLine([this.c, this.c.clone().addScaledVector(l, r)], INK, false, 2);
      ctx.scene.addLine([this.c, s.point], INK, true);
      if (this.legs.length === 1) { const w = to.clone().normalize(); const arc = angleArc(this.c, this.legs[0], w, r * 0.6); ctx.scene.addLine(arc, INK); ctx.scene.addLabel(arc[arc.length >> 1], fmtDeg(THREE.MathUtils.radToDeg(this.legs[0].angleTo(w))), 'dim'); }
      return;
    }
    const [u, w] = this.legs; const arc = angleArc(this.c, u, w, r);
    ctx.scene.addLine([this.c, this.c.clone().addScaledVector(u, r)], INK); ctx.scene.addLine([this.c, this.c.clone().addScaledVector(w, r)], INK); ctx.scene.addLine(arc, INK, false, 2);
    ctx.scene.addLabel(arc[arc.length >> 1], fmtDeg(THREE.MathUtils.radToDeg(u.angleTo(w))), 'dim');
  }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return; const s = this.snap(ev, ctx); const p = s.point.clone();
    if (!this.c && !this.edge) {
      const g = s.kind === 'edge' || s.kind === 'midpoint' ? ctx.scene.segments.find(g => onSegment(g, p)) : undefined;
      if (g) { this.edge = { a: g.a.clone(), b: g.b.clone(), p }; ctx.setHint('Click the other edge of the angle.'); }
      else { this.c = p; ctx.setHint('Click a point on the first leg of the angle.'); }
      return;
    }
    if (this.edge && !this.c) {
      const h = this.segAt(ctx, p); const c = h ? this.corner(this.edge, h) : null;
      if (!h || !c) { ctx.toast(h ? 'These two edges do not meet at a corner: click the corner and a point on each leg instead.' : 'Click the other edge of the angle.'); return; }
      // each leg runs from the corner towards where its edge was clicked (or along the edge, when clicked at the corner)
      const leg = (g: { a: THREE.Vector3; b: THREE.Vector3 }, at: THREE.Vector3) => (at.distanceTo(c) > 1e-4 ? at : g.a.distanceTo(c) > g.b.distanceTo(c) ? g.a : g.b).clone().sub(c).normalize();
      const [u, w] = [leg(this.edge, this.edge.p), leg(h, p)];
      if (u.clone().cross(w).lengthSq() < 1e-10) { ctx.toast('These two edges run along one line: there is no angle between them.'); return; }
      this.c = c; this.legs = [u, w]; this.edge = null; ctx.setHint('Click where the arc goes.'); return;
    }
    const c = this.c!; const to = p.clone().sub(c); if (to.length() < 1e-4) return;
    if (this.legs.length < 2) {
      const d = to.normalize(); if (this.legs.length === 1 && this.legs[0].clone().cross(d).lengthSq() < 1e-10) { ctx.toast('The two legs point along one line: click a point off the first leg.'); return; }
      this.legs.push(d); ctx.setHint(this.legs.length === 1 ? 'Click a point on the second leg.' : 'Click where the arc goes.'); return;
    }
    const r = to.length(); const [u, w] = this.legs; this.reset(ctx);
    await ctx.ops([{ op: 'add_dimension', a: arr(c), b: arr(c.clone().addScaledVector(u, r)), offset: arr(w.clone().multiplyScalar(r)), style: 'angle' }], 'Angular dimension');
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.busy()) { this.reset(ctx); return true; } return false; }
}
void SELECT; void LegacyScaleTool;
