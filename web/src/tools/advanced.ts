// Scale with handles, Follow Me, Flip (mirror) and Place Component.
import * as THREE from 'three';
import type { Tool, ToolContext, Snap } from './base';
import { partsRefused, parseNumbers, parseSizes, onSegment, drawSnap, arr, INK, MAGENTA, SELECT, AXIS } from './base';
import { findEntity, fmt, type Vec3, type Op, type Entity } from '../model';
import { stickyOps } from './edit';

/** Axis-aligned bounds of the selection from the compiled segments. */
export function selectionBounds(ctx: ToolContext, ids: Set<string>): THREE.Box3 {
  const b = new THREE.Box3(); for (const s of ctx.scene.segments) if (ids.has(ctx.resolve(s.id))) { b.expandByPoint(s.a); b.expandByPoint(s.b); } return b;
}

/** Scale with SketchUp-style handles: corners scale uniformly, face handles along one axis, edge handles along two. Ctrl scales about the centre.
 *  A drawn face or loose edge is scaled with what is attached to it (the top of a box made smaller tapers the box). */
export class ScaleTool implements Tool {
  id = 'scale'; label = 'Scale'; hint = 'Select something, then drag a green handle: corners scale uniformly, side handles along one axis, edge handles along two. Ctrl scales about the centre. Type a factor (or sx;sy;sz), or a size with its unit (2m) to make it that large.'; cursor = 'default'; shortcut = 's';
  private ids: string[] = []; private box: THREE.Box3 | null = null; private handles: { p: THREE.Vector3; mask: Vec3; key: string }[] = [];
  private drag: { h: { p: THREE.Vector3; mask: Vec3 }; origin: THREE.Vector3; axis: THREE.Vector3; d0: number; factors: Vec3 } | null = null; private hover = -1;
  busy() { return !!this.drag; }
  activate(ctx: ToolContext) { this.rebuild(ctx); ctx.setHint(this.hint); ctx.setVCB('', 'Scale'); this.draw(ctx); }
  deactivate(ctx: ToolContext) { this.drag = null; ctx.preview([]); ctx.scene.clearOverlay(); }
  private rebuild(ctx: ToolContext) {
    this.ids = [...ctx.selection()]; this.box = this.ids.length ? selectionBounds(ctx, new Set(this.ids)) : null; this.handles = [];
    if (!this.box || this.box.isEmpty()) return; const { min, max } = this.box; const c = this.box.getCenter(new THREE.Vector3());
    const at = (fx: number, fy: number, fz: number) => new THREE.Vector3(fx < 0 ? min.x : fx > 0 ? max.x : c.x, fy < 0 ? min.y : fy > 0 ? max.y : c.y, fz < 0 ? min.z : fz > 0 ? max.z : c.z);
    for (const fx of [-1, 0, 1]) for (const fy of [-1, 0, 1]) for (const fz of [-1, 0, 1]) { const n = Math.abs(fx) + Math.abs(fy) + Math.abs(fz); if (n === 0) continue; this.handles.push({ p: at(fx, fy, fz), mask: [Math.abs(fx), Math.abs(fy), Math.abs(fz)] as Vec3, key: `${fx},${fy},${fz}` }); }
  }
  private draw(ctx: ToolContext) {
    ctx.scene.clearOverlay(); if (!this.box || this.box.isEmpty()) return;
    const { min, max } = this.box; const P = (x: number, y: number, z: number) => new THREE.Vector3(x, y, z);
    const c = [P(min.x, min.y, min.z), P(max.x, min.y, min.z), P(max.x, min.y, max.z), P(min.x, min.y, max.z), P(min.x, max.y, min.z), P(max.x, max.y, min.z), P(max.x, max.y, max.z), P(min.x, max.y, max.z)];
    for (const [i, j] of [[0, 1], [1, 2], [2, 3], [3, 0], [4, 5], [5, 6], [6, 7], [7, 4], [0, 4], [1, 5], [2, 6], [3, 7]]) ctx.scene.addLine([c[i], c[j]], 0x1e8e3e, true);
    this.handles.forEach((h, i) => ctx.scene.addSquare(h.p, i === this.hover ? 0xd93025 : 0x1e8e3e, h.mask.reduce((a, b) => a + b, 0) === 3 ? 9 : 7));
  }
  private scaleOps(ctx: ToolContext, origin: THREE.Vector3, factors: Vec3): Op[] {
    const s = { origin: arr(origin), factors };
    return stickyOps(ctx, this.ids, { scale: s }, rest => ({ op: 'scale', ids: rest, ...s }));
  }
  private handleAt(ev: PointerEvent, ctx: ToolContext) { const cur = new THREE.Vector2(ev.clientX - ctx.scene.container.getBoundingClientRect().left, ev.clientY - ctx.scene.container.getBoundingClientRect().top); let best = -1, bd = 12; this.handles.forEach((h, i) => { const d = ctx.scene.screenPoint(h.p).distanceTo(cur); if (d < bd) { bd = d; best = i; } }); return best; }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (!this.drag) { if (!this.handles.length && ctx.selection().size) this.rebuild(ctx); const h = this.handleAt(ev, ctx); if (h !== this.hover) { this.hover = h; this.draw(ctx); } if (h < 0 && !ctx.selection().size) { const p = ctx.scene.pick(ev); ctx.scene.highlight(p ? new Set([ctx.resolve(p.id)]) : new Set(), true); } return; }
    const d = this.drag; const p = ctx.scene.linePoint(ev, d.origin, d.axis); const dist = p.clone().sub(d.origin).dot(d.axis); let f = Math.max(0.01, dist / d.d0);
    if (ctx.options.gridSnap) f = Math.round(f * 100) / 100;
    d.factors = d.h.mask.map(m => (m ? f : 1)) as Vec3;
    this.draw(ctx); ctx.scene.addLine([d.origin, p], MAGENTA, true); ctx.scene.addLabel(p, `×${f.toFixed(2)}`); ctx.setVCB(f.toFixed(3), 'Scale');
    ctx.preview(d.factors.some(x => Math.abs(x - 1) > 1e-6) ? this.scaleOps(ctx, d.origin, d.factors) : []);
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    if (this.drag) return this.finish(ctx);
    const hi = this.handleAt(ev, ctx);
    if (hi < 0) { if (ctx.selection().size) return; /* the selection stays the one being scaled */ const p = ctx.scene.pick(ev); if (p) { ctx.select([ctx.resolve(p.id)]); this.rebuild(ctx); this.draw(ctx); } return; }
    if (partsRefused(ctx)) return;
    const h = this.handles[hi]; const centre = this.box!.getCenter(new THREE.Vector3());
    const opposite = centre.clone().multiplyScalar(2).sub(h.p); const origin = ev.ctrlKey || ev.metaKey ? centre : opposite;
    const axis = h.p.clone().sub(origin); const d0 = axis.length(); if (d0 < 1e-6) return; axis.normalize();
    this.drag = { h, origin, axis, d0, factors: [1, 1, 1] }; ctx.setHint('Drag the handle; click to finish; type a factor.');
  }
  onUp(_ev: PointerEvent, ctx: ToolContext) { if (this.drag && this.drag.factors.some(f => Math.abs(f - 1) > 1e-6)) this.finish(ctx); }
  private async finish(ctx: ToolContext, factors?: Vec3) {
    const d = this.drag; this.drag = null; ctx.preview([]); const f = factors ?? d?.factors; const origin = d?.origin ?? this.box?.getCenter(new THREE.Vector3());
    if (!f || !origin || f.every(x => Math.abs(x - 1) < 1e-6)) { this.draw(ctx); return; }
    await ctx.ops(this.scaleOps(ctx, origin, f.map(x => +x.toFixed(5)) as Vec3), 'Scale'); this.rebuild(ctx); this.draw(ctx); ctx.setHint(this.hint);
  }
  /** The factors that bring the selection to the typed sizes: along the axes of the handle being dragged; without one, all three sides, or (one size) its longest side. */
  private toSize(sizes: number[]): Vec3 | null {
    const s = this.box!.getSize(new THREE.Vector3()); const sz = [s.x, s.y, s.z]; const f: Vec3 = [1, 1, 1];
    const axes = (this.drag ? [0, 1, 2].filter(i => this.drag!.h.mask[i]) : [0, 1, 2]).filter(i => sz[i] > 1e-9); if (!axes.length) return null;
    if (sizes.length >= axes.length && (this.drag || sizes.length >= 3)) axes.forEach((i, k) => { f[i] = sizes[this.drag ? k : i] / sz[i]; });
    else { const big = this.drag ? axes[0] : axes.reduce((a, b) => (sz[b] > sz[a] ? b : a)); const k = sizes[0] / sz[big]; for (const i of this.drag ? axes : [0, 1, 2]) f[i] = k; }
    return f.every(x => x > 1e-6) ? f : null;
  }
  onVCB(v: string, ctx: ToolContext) {
    const sizes = parseSizes(v);
    if (sizes) { if (!this.ids.length) this.rebuild(ctx); const f = this.box && !this.box.isEmpty() ? this.toSize(sizes) : null; if (f) this.finish(ctx, f); return; }
    const n = parseNumbers(v); if (!n.length) return; if (!this.ids.length) { this.rebuild(ctx); if (!this.ids.length) return; } this.finish(ctx, n.length >= 3 ? [n[0], n[1], n[2]] : this.drag ? this.drag.h.mask.map(m => (m ? n[0] : 1)) as Vec3 : [n[0], n[0], n[0]]); }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.drag) { this.drag = null; ctx.preview([]); this.draw(ctx); return true; } return false; }
}

/** Follow Me: click the profile face, then move along the edges it should follow and click. With a path selected first
 *  (edges, or a face whose outline is the path), one click on the profile does it. */
export class FollowMeTool implements Tool {
  id = 'followme'; label = 'Follow Me'; hint = 'Click the face to extrude, then move along the edges it should follow and click. With a path selected first (edges, or a face for its outline), one click on the face does it.'; cursor = 'crosshair';
  /** The profile, once clicked, and the corners of the path so far. */
  private face: string | null = null; private keys: string[] = []; private shown = '';
  /** Every edge the profile could follow, as corners and their neighbours; `back` leads from a corner towards the one nearest the profile. */
  private nodes = new Map<string, { p: THREE.Vector3; next: string[] }>(); private back = new Map<string, string | null>();
  busy() { return !!this.face; }
  activate(ctx: ToolContext) { this.face = null; ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  deactivate(ctx: ToolContext) { this.face = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.scene.highlight(new Set(), true); }
  private key(p: THREE.Vector3) { return `${Math.round(p.x * 1e4)},${Math.round(p.y * 1e4)},${Math.round(p.z * 1e4)}`; }
  /** Take the profile and map the edges around it, starting from the corner nearest to `near`. */
  private begin(ctx: ToolContext, face: string, near: THREE.Vector3): boolean {
    this.nodes.clear(); this.back.clear();
    const node = (p: THREE.Vector3) => { const k = this.key(p); if (!this.nodes.has(k)) this.nodes.set(k, { p, next: [] }); return k; };
    for (const g of ctx.scene.segments) { if (g.id === face || !ctx.inDocument(g.id)) continue; const a = node(g.a), b = node(g.b); if (a === b) continue; this.nodes.get(a)!.next.push(b); this.nodes.get(b)!.next.push(a); }
    let first: string | null = null, best = Infinity; for (const [k, n] of this.nodes) { const d = n.p.distanceTo(near); if (d < best) { best = d; first = k; } }
    if (!first) return false;
    const todo = [first]; this.back.set(first, null);
    for (let i = 0; i < todo.length; i++) for (const k of this.nodes.get(todo[i])!.next) if (!this.back.has(k)) { this.back.set(k, todo[i]); todo.push(k); }
    this.face = face; this.keys = []; this.shown = ''; return true;
  }
  /** The corners from the start to `k`, by the fewest edges. */
  private trail(k: string): string[] { const out: string[] = []; for (let c: string | null | undefined = k; c; c = this.back.get(c)) out.push(c); return out.reverse(); }
  /** Lead the path to corner `k`: back along itself, one edge further (round a loop too), or else the shortest way there. */
  private reach(k: string) {
    const i = this.keys.indexOf(k); const last = this.keys[this.keys.length - 1];
    if (!this.back.has(k) || last === k) return;
    if (i === 0 && this.keys.length >= 3 && this.nodes.get(last)!.next.includes(k)) this.keys.push(k); // back at the start: a closed path
    else if (i >= 0) this.keys.length = i + 1;
    else if (last && this.nodes.get(last)!.next.includes(k)) this.keys.push(k);
    else this.keys = this.trail(k);
  }
  private follow(ctx: ToolContext, s: Snap) {
    const at = this.key(s.point); if (this.nodes.has(at)) return this.reach(at);
    const g = ctx.scene.segments.find(g => g.id !== this.face && ctx.inDocument(g.id) && onSegment(g, s.point)); if (!g) return;
    const a = this.key(g.a), b = this.key(g.b); if (!this.back.has(a) || !this.back.has(b)) return;
    const ia = this.keys.indexOf(a), ib = this.keys.indexOf(b); const closing = this.keys.length > 3 && this.keys[0] === this.keys[this.keys.length - 1];
    if (ia >= 0 && ib >= 0 && (Math.abs(ia - ib) === 1 || closing)) { if (!closing || Math.abs(ia - ib) === 1) this.keys.length = Math.max(ia, ib) + 1; return; } // an edge the path already runs along
    const last = this.keys[this.keys.length - 1];
    if (last === a || last === b) return this.reach(last === a ? b : a);
    const [near, far] = this.trail(a).length <= this.trail(b).length ? [a, b] : [b, a]; this.keys = [...this.trail(near), far];
  }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (!this.face) { const p = ctx.scene.pick(ev, { facesOnly: true }); ctx.scene.highlight(p ? new Set([p.id]) : new Set(), true); return; }
    const face = this.face; const s = ctx.snap(ev, { ignoreIf: id => !ctx.inDocument(id) || id === face }); this.follow(ctx, s);
    const pts = this.keys.map(k => this.nodes.get(k)!.p); ctx.scene.clearOverlay(); if (pts.length >= 2) ctx.scene.addLine(pts, MAGENTA, false, 3);
    const now = this.keys.join('|'); if (now === this.shown) return; this.shown = now;
    ctx.preview(pts.length >= 2 ? [{ op: 'follow_me', face, path: pts.map(arr) }] : []);
  }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    if (this.face) {
      if (this.keys.length < 2) { ctx.toast('Move along an edge for the face to follow, then click (Esc lets go of the face).'); return; }
      const face = this.face, path = this.keys.map(k => arr(this.nodes.get(k)!.p)); this.face = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.setHint(this.hint);
      const r = await ctx.ops([{ op: 'follow_me', face, path }], 'Follow Me'); if (r[0]?.created?.length) ctx.select([face, ...r[0].created]);
      return;
    }
    const p = ctx.scene.pick(ev, { facesOnly: true }); if (!p) return;
    const doc = ctx.doc(); const sel = [...ctx.selection()].filter(id => id !== p.id);
    const edges = sel.filter(id => findEntity(doc, id)?.kind === 'edge');
    let path: Vec3[] | undefined;
    if (!edges.length) { const face = sel.map(id => findEntity(doc, id)).find(e => e?.kind === 'face'); if (face && face.kind === 'face') path = [...face.outer, face.outer[0]]; }
    if (edges.length || path) {
      const r = await ctx.ops([edges.length ? { op: 'follow_me', face: p.id, pathEdges: edges } : { op: 'follow_me', face: p.id, path }], 'Follow Me');
      if (r[0]?.created?.length) ctx.select([p.id, ...r[0].created]);
      return;
    }
    // no path selected: the face is taken hold of, and follows the edges the cursor moves along
    if (findEntity(doc, p.id)?.kind !== 'face') { ctx.toast('Follow Me works on drawn faces.'); return; }
    if (!this.begin(ctx, p.id, p.point)) { ctx.toast('There are no edges to follow: draw the path first.'); return; }
    ctx.scene.highlight(new Set([p.id]), true); ctx.setHint('Move along the edges to follow, then click. Esc lets go of the face.');
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.face) { this.face = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.setHint(this.hint); return true; } return false; }
}

/** Flip (mirror): mirrors the selection across a plane through its centre, perpendicular to the locked axis (→ x, ↑ y, ← z); click a face to mirror across that face's plane instead. Ctrl keeps a copy. */
export class FlipTool implements Tool {
  id = 'flip'; label = 'Flip / Mirror'; hint = 'Select something, press → ← or ↑ to choose the mirror axis and click, or click a face to mirror across it. Ctrl keeps a copy.'; cursor = 'crosshair';
  activate(ctx: ToolContext) { ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); this.preview(ctx); }
  deactivate(ctx: ToolContext) { ctx.scene.clearOverlay(); ctx.preview([]); }
  onLock(ctx: ToolContext) { this.preview(ctx); }
  private plane(ctx: ToolContext, p?: { point: THREE.Vector3; normal: THREE.Vector3; id: string } | null) {
    const ids = new Set([...ctx.selection()]); if (!ids.size) return null; const b = selectionBounds(ctx, ids); const c = b.getCenter(new THREE.Vector3());
    if (p && !ids.has(ctx.resolve(p.id))) return { origin: p.point.clone(), normal: p.normal.clone(), ids: [...ids] };
    const axis = ctx.lock ?? 'x'; return { origin: c, normal: AXIS[axis].clone(), ids: [...ids] };
  }
  private preview(ctx: ToolContext, p?: { point: THREE.Vector3; normal: THREE.Vector3; id: string } | null) {
    const pl = this.plane(ctx, p); ctx.scene.clearOverlay(); if (!pl) return;
    const b = selectionBounds(ctx, new Set(pl.ids)); const size = b.getSize(new THREE.Vector3()).length() * 0.6 + 0.3; const { u, w } = basis(pl.normal);
    const q = [pl.origin.clone().addScaledVector(u, -size).addScaledVector(w, -size), pl.origin.clone().addScaledVector(u, size).addScaledVector(w, -size), pl.origin.clone().addScaledVector(u, size).addScaledVector(w, size), pl.origin.clone().addScaledVector(u, -size).addScaledVector(w, size)];
    ctx.scene.addLine([...q, q[0]], SELECT, true); ctx.scene.addFace(q, SELECT, 0.08);
    ctx.preview([{ op: 'mirror', ids: pl.ids, origin: arr(pl.origin), normal: arr(pl.normal) }]);
  }
  onMove(ev: PointerEvent, ctx: ToolContext) { const p = ctx.scene.pick(ev, { facesOnly: true }); if (!ctx.selection().size) { ctx.scene.highlight(p ? new Set([ctx.resolve(p.id)]) : new Set(), true); return; } this.preview(ctx, p); }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return; const p = ctx.scene.pick(ev, { facesOnly: true });
    if (!ctx.selection().size) { if (p) { ctx.select([ctx.resolve(p.id)]); this.preview(ctx); } return; }
    if (partsRefused(ctx)) return;
    const pl = this.plane(ctx, p); if (!pl) return; ctx.preview([]); ctx.scene.clearOverlay();
    const r = await ctx.ops([{ op: 'mirror', ids: pl.ids, origin: arr(pl.origin), normal: arr(pl.normal), copy: ev.ctrlKey || ev.metaKey }], ev.ctrlKey || ev.metaKey ? 'Mirror copy' : 'Flip');
    if (r[0]?.created?.length) ctx.select(r[0].created);
  }
}
function basis(n: THREE.Vector3) { const u = Math.abs(n.y) < 0.9 ? new THREE.Vector3(0, 1, 0).cross(n).normalize() : new THREE.Vector3(1, 0, 0).cross(n).normalize(); return { u, w: n.clone().cross(u).normalize() }; }

/** Place Component: click to drop an instance of the chosen definition; type a rotation. */
export class PlaceComponentTool implements Tool {
  id = 'place'; label = 'Place Component'; hint = 'Click where the component goes (its origin). Arrow keys ← → turn it 90°; type a rotation in degrees.'; cursor = 'crosshair';
  definition: string | null = null; private rotation = 0;
  activate(ctx: ToolContext) { const defs = ctx.doc().components ?? []; if (!this.definition || !defs.some(d => d.id === this.definition)) this.definition = defs[0]?.id ?? null; ctx.setHint(this.definition ? `${this.hint} Placing: ${defs.find(d => d.id === this.definition)?.name}` : 'No components yet: select entities and use Edit → Make Component.'); ctx.setVCB(String(this.rotation), 'Rotation'); }
  deactivate(ctx: ToolContext) { ctx.scene.clearOverlay(); ctx.preview([]); }
  onLock(ctx: ToolContext) { if (ctx.lock === 'x') this.rotation = (this.rotation + 90) % 360; if (ctx.lock === 'z') this.rotation = (this.rotation + 270) % 360; ctx.lock = null; ctx.setVCB(String(this.rotation), 'Rotation'); }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = ctx.snap(ev, { faces: true }); ctx.scene.clearOverlay(); drawSnap(ctx, s); if (!this.definition) return;
    ctx.scene.addLabel(s.point, `${ctx.doc().components?.find(d => d.id === this.definition)?.name ?? 'component'} · ${this.rotation}°`, 'small');
    ctx.preview([{ op: 'place_component', definition: this.definition, position: arr(s.point), rotation: this.rotation, layer: ctx.options.layer }]);
  }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0 || !this.definition) return; const s = ctx.snap(ev, { faces: true }); ctx.preview([]);
    const r = await ctx.ops([{ op: 'place_component', definition: this.definition, position: arr(s.point), rotation: this.rotation, layer: ctx.options.layer }], 'Place component'); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]);
  }
  onVCB(v: string, ctx: ToolContext) { const n = parseNumbers(v); if (n.length) { this.rotation = ((n[0] % 360) + 360) % 360; ctx.setVCB(String(this.rotation), 'Rotation'); } }
}
/** Paste: what was copied follows the cursor by its lowest corner; a click puts it down. */
export class PasteTool implements Tool {
  id = 'paste'; label = 'Paste'; hint = 'Click where the copy goes. Esc cancels.'; cursor = 'copy';
  /** What to paste, and the corner it is held by (set by Edit → Paste). */
  clip: { entities: Entity[]; anchor: Vec3; local?: boolean } | null = null;
  activate(ctx: ToolContext) { if (!this.clip) { ctx.toast('Nothing to paste: copy something first.'); return; } ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  deactivate(ctx: ToolContext) { ctx.preview([]); ctx.scene.clearOverlay(); }
  private op(p: THREE.Vector3): Op[] { const c = this.clip!; return [{ op: 'insert', entities: c.entities, ...(c.local ? { local: true } : {}), delta: [+(p.x - c.anchor[0]).toFixed(5), +(p.y - c.anchor[1]).toFixed(5), +(p.z - c.anchor[2]).toFixed(5)] }]; }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (!this.clip) return; const s = ctx.snap(ev, { faces: true, ignoreIf: id => !ctx.inDocument(id) }); ctx.scene.clearOverlay(); drawSnap(ctx, s);
    ctx.preview(this.op(s.point));
  }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0 || !this.clip) return; const s = ctx.snap(ev, { faces: true, ignoreIf: id => !ctx.inDocument(id) }); ctx.preview([]);
    const r = await ctx.ops(this.op(s.point), 'Paste'); ctx.setTool('select'); if (r[0]?.created?.length) ctx.select(r[0].created);
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape') { ctx.setTool('select'); return true; } return false; }
}
export const fmtFactor = (f: number) => `×${fmt(f, 2)}`;
void INK;
