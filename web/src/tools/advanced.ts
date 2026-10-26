// Scale with handles, Follow Me, Flip (mirror) and Place Component.
import * as THREE from 'three';
import type { Tool, ToolContext } from './base';
import { parseNumbers, drawSnap, arr, INK, MAGENTA, SELECT, AXIS } from './base';
import { findEntity, fmt, type Vec3 } from '../model';

/** Axis-aligned bounds of the selection from the compiled segments. */
export function selectionBounds(ctx: ToolContext, ids: Set<string>): THREE.Box3 {
  const b = new THREE.Box3(); for (const s of ctx.scene.segments) if (ids.has(ctx.resolve(s.id))) { b.expandByPoint(s.a); b.expandByPoint(s.b); } return b;
}

/** Scale with SketchUp-style handles: corners scale uniformly, face handles along one axis, edge handles along two. Ctrl scales about the centre. */
export class ScaleTool implements Tool {
  id = 'scale'; label = 'Scale'; hint = 'Select something, then drag a green handle: corners scale uniformly, side handles along one axis, edge handles along two. Ctrl scales about the centre. Type a factor (or sx;sy;sz).'; cursor = 'default'; shortcut = 's';
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
  private handleAt(ev: PointerEvent, ctx: ToolContext) { const cur = new THREE.Vector2(ev.clientX - ctx.scene.container.getBoundingClientRect().left, ev.clientY - ctx.scene.container.getBoundingClientRect().top); let best = -1, bd = 12; this.handles.forEach((h, i) => { const d = ctx.scene.screenPoint(h.p).distanceTo(cur); if (d < bd) { bd = d; best = i; } }); return best; }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (!this.drag) { if (!this.handles.length && ctx.selection().size) this.rebuild(ctx); const h = this.handleAt(ev, ctx); if (h !== this.hover) { this.hover = h; this.draw(ctx); } if (h < 0) { const p = ctx.scene.pick(ev); ctx.scene.highlight(p ? new Set([ctx.resolve(p.id)]) : new Set(), true); } return; }
    const d = this.drag; const p = ctx.scene.linePoint(ev, d.origin, d.axis); const dist = p.clone().sub(d.origin).dot(d.axis); let f = Math.max(0.01, dist / d.d0);
    if (ctx.options.gridSnap) f = Math.round(f * 100) / 100;
    d.factors = d.h.mask.map(m => (m ? f : 1)) as Vec3;
    this.draw(ctx); ctx.scene.addLine([d.origin, p], MAGENTA, true); ctx.scene.addLabel(p, `×${f.toFixed(2)}`); ctx.setVCB(f.toFixed(3), 'Scale');
    ctx.preview([{ op: 'scale', ids: this.ids, origin: arr(d.origin), factors: d.factors }]);
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    if (this.drag) return this.finish(ctx);
    const hi = this.handleAt(ev, ctx);
    if (hi < 0) { const p = ctx.scene.pick(ev); if (p) { ctx.select([ctx.resolve(p.id)]); this.rebuild(ctx); this.draw(ctx); } return; }
    const h = this.handles[hi]; const centre = this.box!.getCenter(new THREE.Vector3());
    const opposite = centre.clone().multiplyScalar(2).sub(h.p); const origin = ev.ctrlKey || ev.metaKey ? centre : opposite;
    const axis = h.p.clone().sub(origin); const d0 = axis.length(); if (d0 < 1e-6) return; axis.normalize();
    this.drag = { h, origin, axis, d0, factors: [1, 1, 1] }; ctx.setHint('Drag the handle; click to finish; type a factor.');
  }
  onUp(_ev: PointerEvent, ctx: ToolContext) { if (this.drag && this.drag.factors.some(f => Math.abs(f - 1) > 1e-6)) this.finish(ctx); }
  private async finish(ctx: ToolContext, factors?: Vec3) {
    const d = this.drag; this.drag = null; ctx.preview([]); const f = factors ?? d?.factors; const origin = d?.origin ?? this.box?.getCenter(new THREE.Vector3());
    if (!f || !origin || f.every(x => Math.abs(x - 1) < 1e-6)) { this.draw(ctx); return; }
    await ctx.ops([{ op: 'scale', ids: this.ids, origin: arr(origin), factors: f.map(x => +x.toFixed(5)) as Vec3 }], 'Scale'); this.rebuild(ctx); this.draw(ctx); ctx.setHint(this.hint);
  }
  onVCB(v: string, ctx: ToolContext) { const n = parseNumbers(v); if (!n.length) return; if (!this.ids.length) { this.rebuild(ctx); if (!this.ids.length) return; } this.finish(ctx, n.length >= 3 ? [n[0], n[1], n[2]] : this.drag ? this.drag.h.mask.map(m => (m ? n[0] : 1)) as Vec3 : [n[0], n[0], n[0]]); }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.drag) { this.drag = null; ctx.preview([]); this.draw(ctx); return true; } return false; }
}

/** Follow Me: select the path edges first (or a face whose outline is the path), then click the profile face. */
export class FollowMeTool implements Tool {
  id = 'followme'; label = 'Follow Me'; hint = 'Select the path edges first, then click the face to extrude along them.'; cursor = 'crosshair';
  activate(ctx: ToolContext) { ctx.setHint(ctx.selection().size ? this.hint : 'Follow Me needs a path: select the edges to follow (Select tool, Shift-click), then come back and click the face.'); ctx.setVCB('', 'Measurements'); }
  deactivate(ctx: ToolContext) { ctx.scene.highlight(new Set(), true); }
  onMove(ev: PointerEvent, ctx: ToolContext) { const p = ctx.scene.pick(ev, { facesOnly: true }); ctx.scene.highlight(p ? new Set([p.id]) : new Set(), true); }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return; const p = ctx.scene.pick(ev, { facesOnly: true }); if (!p) return;
    const doc = ctx.doc(); const sel = [...ctx.selection()].filter(id => id !== p.id);
    const edges = sel.filter(id => findEntity(doc, id)?.kind === 'edge');
    let path: Vec3[] | undefined;
    if (!edges.length) { const face = sel.map(id => findEntity(doc, id)).find(e => e?.kind === 'face'); if (face && face.kind === 'face') path = [...face.outer, face.outer[0]]; }
    if (!edges.length && !path) { ctx.toast('Select the path first: edges to follow, or a face whose outline is the path.'); return; }
    const r = await ctx.ops([edges.length ? { op: 'follow_me', face: p.id, pathEdges: edges } : { op: 'follow_me', face: p.id, path }], 'Follow Me');
    if (r[0]?.created?.length) ctx.select([p.id, ...r[0].created]);
  }
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
export const fmtFactor = (f: number) => `×${fmt(f, 2)}`;
void INK;
