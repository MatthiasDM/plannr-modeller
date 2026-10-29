import * as THREE from 'three';
import type { Tool, ToolContext, Plane, Snap } from './base';
import { parseNumbers, drawSnap, lineColor, planeFromSnap, isPointSnap, GROUND, arr, INK, MAGENTA, SELECT, AXIS, AXIS_COLOR } from './base';
import { planeBasis } from './draw';
import { fmtM, fmtDeg, findEntity, materialOf, walk, type Vec3 } from '../model';
import { V, COLOR } from '../scene';
import { getPlugins } from '../plugins';
import type { Op } from '../model';

/** The ops a Move produces: plugins may rewrite it (e.g. a vertical move of a whole house becomes a ground-level change). */
function moveOps(ctx: ToolContext, ids: string[], delta: THREE.Vector3, copy: boolean): Op[] {
  const d = arr(delta);
  if (!copy) for (const p of getPlugins()) { const ops = p.beforeMove?.(ids, d, ctx); if (ops) return ops; }
  return [copy ? { op: 'copy', ids, delta: d } : { op: 'move', ids, delta: d }];
}
const hoverPick = (ev: PointerEvent, ctx: ToolContext, facesOnly = false) => { const p = ctx.scene.pick(ev, { facesOnly }); ctx.scene.highlight(p ? new Set([ctx.resolve(p.id)]) : new Set(), true); return p; };
/** Ignore geometry that exists only in the current preview (ids the document doesn't have): a tool's own
 *  pulled face must not become its inference. */
function previewOnly(ctx: ToolContext) { const ids = new Set<string>(); for (const { entity } of walk(ctx.doc().entities)) ids.add(entity.id); return (id: string) => !ids.has(id.split('#')[0]); }

/** Select: click, Shift adds, Ctrl toggles, drag a box, double-click enters a group. */
export class SelectTool implements Tool {
  id = 'select'; label = 'Select'; hint = 'Click to select. Shift adds, Ctrl toggles, drag a box. Double-click a group to edit inside it; Esc leaves it.'; cursor = 'default'; shortcut = ' ';
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
    hoverPick(ev, ctx);
  }
  onDown(ev: PointerEvent, ctx: ToolContext) { if (ev.button === 0) this.down = { x: ev.clientX, y: ev.clientY }; }
  onUp(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0 || !this.down) return;
    const mode = ev.shiftKey ? 'add' : ev.ctrlKey || ev.metaKey ? 'toggle' : 'replace';
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
      ctx.select([...ids], mode); return;
    }
    this.down = null;
    const p = ctx.scene.pick(ev);
    if (!p) { if (mode === 'replace') ctx.select([]); return; }
    ctx.select([ctx.resolve(p.id)], mode);
  }
  onDblClick(ev: MouseEvent, ctx: ToolContext) { const p = ctx.scene.pick(ev); if (p) (ctx as any).enterGroup?.(p.id); }
}

export class EraserTool implements Tool {
  id = 'eraser'; label = 'Eraser'; hint = 'Click an edge, face or object to erase it; drag to erase several.'; cursor = 'crosshair'; shortcut = 'e';
  private dragging = false; private pending = new Set<string>();
  activate(ctx: ToolContext) { ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  deactivate(ctx: ToolContext) { ctx.scene.highlight(new Set(), true); }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const p = hoverPick(ev, ctx);
    if (this.dragging && p) { this.pending.add(ctx.resolve(p.id)); ctx.scene.highlight(this.pending); }
  }
  onDown(ev: PointerEvent, ctx: ToolContext) { if (ev.button !== 0) return; this.dragging = true; this.pending.clear(); const p = ctx.scene.pick(ev); if (p) this.pending.add(ctx.resolve(p.id)); ctx.scene.highlight(this.pending); }
  async onUp(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0 || !this.dragging) return; this.dragging = false;
    if (this.pending.size) { await ctx.ops([{ op: 'erase', ids: [...this.pending] }], 'Erase'); ctx.select([]); }
    this.pending.clear();
  }
}

/** Move: drag anything; Ctrl/Alt copies; arrow keys lock an axis; type a distance. */
export class MoveTool implements Tool {
  id = 'move'; label = 'Move'; hint = 'Click what to move, then click where it goes (or drag). Ctrl = copy, arrows lock an axis, type a distance.'; cursor = 'move'; shortcut = 'm';
  private ids: string[] = []; private start: THREE.Vector3 | null = null; private cur: THREE.Vector3 | null = null; private plane: Plane = GROUND;
  private copy = false; private dragging = false; private lastDir: THREE.Vector3 | null = null;
  busy() { return !!this.start; }
  activate(ctx: ToolContext) { this.ids = []; this.start = null; ctx.setHint(this.hint); ctx.setVCB('', 'Distance'); }
  deactivate(ctx: ToolContext) { this.ids = []; this.start = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.scene.highlight(new Set(), true); }
  private snapOpts(ctx: ToolContext) { return { plane: this.plane, from: this.start ?? undefined, lock: ctx.lock, ignore: new Set(this.ids), faces: true }; }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (!this.start) { hoverPick(ev, ctx); ctx.snap(ev, { faces: true }); return; } // the grab point infers like any other
    const s = ctx.snap(ev, this.snapOpts(ctx)); this.cur = s.point;
    ctx.scene.clearOverlay(); drawSnap(ctx, s);
    const d = s.point.clone().sub(this.start);
    ctx.scene.addLine([this.start, s.point], lineColor(this.start, s.point, s), true);
    ctx.scene.addLabel(this.start.clone().lerp(s.point, 0.5), fmtM(d.length()));
    ctx.setVCB(d.length().toFixed(3), 'Distance');
    ctx.preview(d.lengthSq() > 1e-10 ? moveOps(ctx, this.ids, d, this.copy) : []);
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    if (this.start) return this.finish(ctx);
    const p = ctx.scene.pick(ev);
    const sel = ctx.selection();
    if (p && !sel.has(ctx.resolve(p.id))) ctx.select([ctx.resolve(p.id)]);
    this.ids = [...ctx.selection()]; if (!this.ids.length) { ctx.toast('Nothing to move — click an object first'); return; }
    const s = ctx.snap(ev, { faces: true }); this.start = s.point.clone(); this.plane = planeFromSnap(s, GROUND);
    this.copy = ev.ctrlKey || ev.altKey || ev.metaKey; this.dragging = true;
    ctx.setHint(`${this.copy ? 'Copying' : 'Moving'} ${this.ids.length} object(s): click the destination or type a distance. Ctrl toggles copy.`);
  }
  onUp(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0 || !this.dragging) return; this.dragging = false;
    if (this.start && this.cur && this.cur.distanceTo(this.start) > 1e-3) this.finish(ctx); // a real drag ends on release; a click waits for a second click
  }
  private async finish(ctx: ToolContext, delta?: THREE.Vector3) {
    if (!this.start) return;
    const d = delta ?? (this.cur ? this.cur.clone().sub(this.start) : null);
    const ids = this.ids; this.lastDir = d && d.lengthSq() > 1e-10 ? d.clone().normalize() : this.lastDir;
    this.start = null; this.cur = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.setHint(this.hint);
    if (!d || d.lengthSq() < 1e-10) return;
    const res = await ctx.ops(moveOps(ctx, ids, d, this.copy), this.copy ? 'Copy' : 'Move');
    if (this.copy && res[0]?.created?.length) ctx.select(res[0].created);
  }
  onVCB(v: string, ctx: ToolContext) {
    const n = parseNumbers(v); if (!n.length) return;
    if (this.start) { const dir = this.cur ? this.cur.clone().sub(this.start) : new THREE.Vector3(1, 0, 0); if (dir.lengthSq() < 1e-9) dir.set(1, 0, 0); this.finish(ctx, dir.normalize().multiplyScalar(n[0])); }
    else if (this.lastDir && ctx.selection().size) { this.ids = [...ctx.selection()]; this.start = new THREE.Vector3(); this.finish(ctx, this.lastDir.clone().multiplyScalar(n[0])); }
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) {
    if (ev.key === 'Escape' && this.start) { this.start = null; this.cur = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.setHint(this.hint); return true; }
    if (ev.key === 'Control' || ev.key === 'Alt') { if (this.start) { this.copy = !this.copy; ctx.setHint(`${this.copy ? 'Copying' : 'Moving'}…`); } return true; }
    return false;
  }
}

/** Rotate: protractor — centre, reference direction, angle. */
export class RotateTool implements Tool {
  id = 'rotate'; label = 'Rotate'; hint = 'Click the centre of rotation (on a face to rotate in its plane), then the reference point, then the angle.'; cursor = 'crosshair'; shortcut = 'q';
  private center: THREE.Vector3 | null = null; private ref: THREE.Vector3 | null = null; private plane: Plane = GROUND; private ids: string[] = []; private angle = 0;
  busy() { return !!this.center; }
  activate(ctx: ToolContext) { this.center = null; this.ref = null; ctx.setHint(this.hint); ctx.setVCB('', 'Angle'); }
  deactivate(ctx: ToolContext) { this.center = null; this.ref = null; ctx.preview([]); ctx.scene.clearOverlay(); }
  private protractor(ctx: ToolContext) {
    if (!this.center) return; const { u, w } = planeBasis(this.plane.normal); const r = ctx.scene.pixelSize(this.center) * 60; const pts: THREE.Vector3[] = [];
    for (let i = 0; i <= 48; i++) { const a = (i / 48) * Math.PI * 2; pts.push(this.center.clone().addScaledVector(u, Math.cos(a) * r).addScaledVector(w, Math.sin(a) * r)); }
    const n = this.plane.normal; const col = Math.abs(n.y) > 0.99 ? AXIS_COLOR.y : Math.abs(n.x) > 0.99 ? AXIS_COLOR.x : Math.abs(n.z) > 0.99 ? AXIS_COLOR.z : INK;
    ctx.scene.addLine(pts, col);
    for (let i = 0; i < 24; i++) { const a = (i / 24) * Math.PI * 2; const d = u.clone().multiplyScalar(Math.cos(a)).addScaledVector(w, Math.sin(a)); ctx.scene.addLine([this.center.clone().addScaledVector(d, r * (i % 6 === 0 ? 0.8 : 0.9)), this.center.clone().addScaledVector(d, r)], col); }
  }
  private angleTo(p: THREE.Vector3) {
    if (!this.center || !this.ref) return 0; const n = this.plane.normal; const a = this.ref.clone().sub(this.center), b = p.clone().sub(this.center);
    a.sub(n.clone().multiplyScalar(a.dot(n))); b.sub(n.clone().multiplyScalar(b.dot(n)));
    const ang = Math.atan2(a.clone().cross(b).dot(n), a.dot(b)) * 180 / Math.PI; return Math.round(ang / 15) * 15 === ang ? ang : (Math.abs(ang - Math.round(ang / 15) * 15) < 1.5 ? Math.round(ang / 15) * 15 : ang);
  }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = ctx.snap(ev, { plane: this.center ? this.plane : GROUND, faces: !this.center, ignore: new Set(this.ids) });
    ctx.scene.clearOverlay(); drawSnap(ctx, s); this.protractor(ctx);
    if (!this.center) { hoverPick(ev, ctx, true); return; }
    if (!this.ref) { ctx.scene.addLine([this.center, s.point], 0x888888, true); return; }
    this.angle = this.angleTo(s.point);
    ctx.scene.addLine([this.center, this.ref], 0x888888, true); ctx.scene.addLine([this.center, s.point], MAGENTA);
    ctx.scene.addLabel(s.point, fmtDeg(this.angle)); ctx.setVCB(this.angle.toFixed(1), 'Angle');
    ctx.preview(Math.abs(this.angle) > 1e-6 ? [{ op: 'rotate', ids: this.ids, origin: arr(this.center), axis: arr(this.plane.normal), angle: +this.angle.toFixed(4) }] : []);
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    if (!this.center) {
      this.ids = [...ctx.selection()]; if (!this.ids.length) { const p = ctx.scene.pick(ev); if (p) { ctx.select([ctx.resolve(p.id)]); this.ids = [ctx.resolve(p.id)]; } }
      if (!this.ids.length) { ctx.toast('Select something to rotate first'); return; }
      const s = ctx.snap(ev, { faces: true }); this.center = s.point.clone(); this.plane = planeFromSnap(s, GROUND);
      if (ctx.lock) this.plane = { origin: this.center, normal: AXIS[ctx.lock].clone() };
      ctx.setHint('Click the reference point.'); return;
    }
    const s = ctx.snap(ev, { plane: this.plane, ignore: new Set(this.ids) });
    if (!this.ref) { this.ref = s.point.clone(); ctx.setHint('Move to rotate, click to finish, or type an angle.'); return; }
    this.finish(ctx, this.angleTo(s.point));
  }
  private async finish(ctx: ToolContext, angle: number) {
    const c = this.center!, n = this.plane.normal.clone(); const ids = this.ids;
    this.center = null; this.ref = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.setHint(this.hint);
    if (Math.abs(angle) < 1e-6) return;
    await ctx.ops([{ op: 'rotate', ids, origin: arr(c), axis: arr(n), angle: +angle.toFixed(4) }], 'Rotate');
  }
  onVCB(v: string, ctx: ToolContext) { const n = parseNumbers(v); if (n.length && this.center && this.ref) this.finish(ctx, n[0]); }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape') { this.center = null; this.ref = null; ctx.preview([]); ctx.scene.clearOverlay(); return true; } return false; }
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
    if (!this.center) { hoverPick(ev, ctx); return; }
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

/** Push/Pull: extrude a face along its normal. */
export class PushPullTool implements Tool {
  id = 'pushpull'; label = 'Push/Pull'; hint = 'Click a face and move to extrude it; click again (or release after dragging) to finish. Type a distance.'; cursor = 'ns-resize'; shortcut = 'p';
  private face: { id: string; point: THREE.Vector3; normal: THREE.Vector3 } | null = null; private dist = 0; private dragging = false; private lastFace: string | null = null; private ignore: (id: string) => boolean = () => false;
  busy() { return !!this.face; }
  activate(ctx: ToolContext) { this.face = null; ctx.setHint(this.hint); ctx.setVCB('', 'Distance'); }
  deactivate(ctx: ToolContext) { this.face = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.scene.highlight(new Set(), true); }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (!this.face) { const p = ctx.scene.pick(ev, { facesOnly: true }); ctx.scene.highlight(p ? new Set([p.id]) : new Set(), true); ctx.snap(ev, { faces: true }); return; }
    // a point under the cursor sets the distance (SketchUp takes heights from other geometry); else the cursor along the normal
    const s = ctx.snap(ev, { faces: false, ignoreIf: this.ignore });
    let d: number;
    if (isPointSnap(s)) d = s.point.clone().sub(this.face.point).dot(this.face.normal);
    else { d = ctx.scene.linePoint(ev, this.face.point, this.face.normal).clone().sub(this.face.point).dot(this.face.normal); if (ctx.options.gridSnap) d = Math.round(d * 100) / 100; }
    this.dist = d;
    ctx.scene.clearOverlay(); const tip = this.face.point.clone().addScaledVector(this.face.normal, d);
    ctx.scene.addLine([this.face.point, tip], MAGENTA, true); ctx.scene.addLabel(tip, fmtM(Math.abs(d))); ctx.setVCB(d.toFixed(3), 'Distance');
    ctx.preview(Math.abs(d) > 1e-4 ? [{ op: 'push_pull', face: this.face.id, distance: +d.toFixed(5) }] : []);
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    if (this.face) return this.finish(ctx, this.dist);
    const p = ctx.scene.pick(ev, { facesOnly: true }); if (!p) return;
    const n = p.back ? p.normal.clone().negate() : p.normal.clone();
    this.face = { id: p.id, point: p.point.clone(), normal: n.normalize() }; this.dragging = true; this.dist = 0; this.ignore = previewOnly(ctx);
    ctx.setHint('Move to set the distance; click or release to finish; type a distance for precision.');
  }
  onUp(ev: PointerEvent, ctx: ToolContext) { if (ev.button !== 0 || !this.dragging) return; this.dragging = false; if (this.face && Math.abs(this.dist) > 0.02) this.finish(ctx, this.dist); }
  private async finish(ctx: ToolContext, d: number) {
    const f = this.face; this.face = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.setHint(this.hint);
    if (!f || Math.abs(d) < 1e-4) return;
    await ctx.ops([{ op: 'push_pull', face: f.id, distance: +d.toFixed(5) }], 'Push/Pull'); this.lastFace = f.id;
  }
  onVCB(v: string, ctx: ToolContext) {
    const n = parseNumbers(v); if (!n.length) return;
    if (this.face) this.finish(ctx, Math.sign(this.dist || 1) * n[0]);
    else if (this.lastFace) ctx.ops([{ op: 'push_pull', face: this.lastFace, distance: n[0] }], 'Push/Pull');
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.face) { this.face = null; ctx.preview([]); ctx.scene.clearOverlay(); return true; } return false; }
}

/** Offset: copy a face's outline inward or outward. */
export class OffsetTool implements Tool {
  id = 'offset'; label = 'Offset'; hint = 'Click a face and move inward or outward; click to finish. Type a distance.'; cursor = 'crosshair'; shortcut = 'f';
  private face: { id: string; plane: Plane; start: THREE.Vector3; center: THREE.Vector3 } | null = null; private dist = 0; private ignore: (id: string) => boolean = () => false;
  busy() { return !!this.face; }
  activate(ctx: ToolContext) { this.face = null; ctx.setHint(this.hint); ctx.setVCB('', 'Distance'); }
  deactivate(ctx: ToolContext) { this.face = null; ctx.preview([]); ctx.scene.clearOverlay(); ctx.scene.highlight(new Set(), true); }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (!this.face) { const p = ctx.scene.pick(ev, { facesOnly: true }); ctx.scene.highlight(p ? new Set([p.id]) : new Set(), true); ctx.snap(ev, { faces: true }); return; }
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
    if (this.face) return this.finish(ctx, this.dist);
    const p = ctx.scene.pick(ev, { facesOnly: true }); if (!p) return;
    const e = findEntity(ctx.doc(), p.id); const center = e && e.kind === 'face' ? e.outer.reduce((c, q) => c.add(V(q)), new THREE.Vector3()).multiplyScalar(1 / e.outer.length) : p.point.clone();
    this.face = { id: p.id, plane: { origin: p.point.clone(), normal: p.normal.clone() }, start: p.point.clone(), center }; this.ignore = previewOnly(ctx);
  }
  private async finish(ctx: ToolContext, d: number) {
    const f = this.face; this.face = null; ctx.preview([]); ctx.scene.clearOverlay();
    if (!f || Math.abs(d) < 1e-4) return;
    const res = await ctx.ops([{ op: 'offset', face: f.id, distance: +d.toFixed(5) }], 'Offset'); if (res[0]?.created?.[0]) ctx.select([res[0].created[0]]);
  }
  onVCB(v: string, ctx: ToolContext) { const n = parseNumbers(v); if (n.length && this.face) this.finish(ctx, Math.sign(this.dist || 1) * n[0]); }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.face) { this.face = null; ctx.preview([]); ctx.scene.clearOverlay(); return true; } return false; }
}

/** Paint Bucket: apply the active material to a face (or every face of a group). Alt-click samples. */
export class PaintTool implements Tool {
  id = 'paint'; label = 'Paint Bucket'; hint = 'Click a face to paint it with the active material. Shift paints the whole object, Alt samples.'; cursor = 'cell'; shortcut = 'b';
  activate(ctx: ToolContext) { ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  deactivate(ctx: ToolContext) { ctx.scene.highlight(new Set(), true); }
  onMove(ev: PointerEvent, ctx: ToolContext) { const p = ctx.scene.pick(ev, { facesOnly: true }); ctx.scene.highlight(p ? new Set([ev.shiftKey ? ctx.resolve(p.id) : p.id]) : new Set(), true); }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const p = ctx.scene.pick(ev, { facesOnly: true }); if (!p) return;
    if (ev.altKey) { if (p.material) { ctx.options.material = p.material; ctx.toast(`Sampled ${materialOf(ctx.doc(), p.material)?.name ?? p.material}`); } return; }
    const id = ev.shiftKey ? ctx.resolve(p.id) : p.id;
    await ctx.ops([{ op: 'paint', ids: [id], material: ctx.options.material, side: ev.shiftKey ? 'both' : p.back ? 'back' : 'front' }], 'Paint');
  }
}

/** Tape Measure: measure between two points; from an edge, drag a parallel guide line; type a distance for a guide. */
export class TapeTool implements Tool {
  id = 'tape'; label = 'Tape Measure'; hint = 'Click two points to measure. Start on an edge to pull a parallel guide line; type a distance to place it exactly.'; cursor = 'crosshair'; shortcut = 't';
  private a: Snap | null = null; private cur: Snap | null = null; private guideDir: THREE.Vector3 | null = null; private plane: Plane = GROUND;
  busy() { return !!this.a; }
  activate(ctx: ToolContext) { this.a = null; ctx.setHint(this.hint); ctx.setVCB('', 'Length'); }
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
    if (this.guideDir) await ctx.ops([{ op: 'add_guide', point: arr(p), direction: arr(this.guideDir) }], 'Guide line');
    else if (guidePoint) await ctx.ops([{ op: 'add_guide', point: arr(p) }], 'Guide point');
  }
  onVCB(v: string, ctx: ToolContext) {
    const n = parseNumbers(v); if (!n.length || !this.a || !this.cur) return;
    const dir = this.cur.point.clone().sub(this.a.point); if (dir.lengthSq() < 1e-9) return; dir.normalize();
    this.finish(ctx, this.a.point.clone().addScaledVector(dir, n[0]), true);
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape') { this.a = null; ctx.scene.clearOverlay(); return true; } return false; }
}

/** Text: click a point, type the label. */
export class TextTool implements Tool {
  id = 'text'; label = 'Text'; hint = 'Click a point to attach a label to it.'; cursor = 'text'; shortcut = 'x';
  activate(ctx: ToolContext) { ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  deactivate(ctx: ToolContext) { ctx.scene.clearOverlay(); }
  onMove(ev: PointerEvent, ctx: ToolContext) { const s = ctx.snap(ev, { faces: true }); ctx.scene.clearOverlay(); drawSnap(ctx, s); }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const s = ctx.snap(ev, { faces: true }); const text = await ctx.prompt('Label text', ''); if (!text) return;
    const off = ctx.scene.pixelSize(s.point) * 40; const up = new THREE.Vector3().setFromMatrixColumn(ctx.scene.cam.camera.matrix, 1), right = new THREE.Vector3().setFromMatrixColumn(ctx.scene.cam.camera.matrix, 0);
    const pos = s.point.clone().addScaledVector(up, off).addScaledVector(right, off);
    await ctx.ops([{ op: 'add_text', position: arr(pos), text, anchor: arr(s.point) }], 'Text');
  }
}

/** Dimension: two points, then the offset. */
export class DimensionTool implements Tool {
  id = 'dimension'; label = 'Dimension'; hint = 'Click the two points to dimension, then click where the dimension line goes.'; cursor = 'crosshair'; shortcut = 'd';
  private a: THREE.Vector3 | null = null; private b: THREE.Vector3 | null = null; private plane: Plane = GROUND;
  busy() { return !!this.a; }
  activate(ctx: ToolContext) { this.a = this.b = null; ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  deactivate(ctx: ToolContext) { this.a = this.b = null; ctx.scene.clearOverlay(); }
  private offsetFor(p: THREE.Vector3) { const d = this.b!.clone().sub(this.a!).normalize(); const o = p.clone().sub(this.b!); return o.sub(d.multiplyScalar(o.dot(d))); }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = ctx.snap(ev, { plane: this.plane, faces: !this.a, from: this.a && !this.b ? this.a : undefined, lock: ctx.lock }); ctx.scene.clearOverlay(); drawSnap(ctx, s);
    if (this.a && !this.b) { ctx.scene.addLine([this.a, s.point], lineColor(this.a, s.point, s), true); ctx.scene.addLabel(this.a.clone().lerp(s.point, 0.5), fmtM(this.a.distanceTo(s.point))); }
    if (this.a && this.b) {
      const o = this.offsetFor(s.point); const a2 = this.a.clone().add(o), b2 = this.b.clone().add(o);
      ctx.scene.addLine([this.a, a2], INK); ctx.scene.addLine([this.b, b2], INK); ctx.scene.addLine([a2, b2], INK, false, 2); ctx.scene.addLabel(a2.clone().lerp(b2, 0.5), fmtM(this.a.distanceTo(this.b)), 'dim');
    }
  }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const s = ctx.snap(ev, { plane: this.plane, faces: !this.a, from: this.a && !this.b ? this.a : undefined, lock: ctx.lock });
    if (!this.a) { this.a = s.point.clone(); this.plane = planeFromSnap(s, GROUND); return; }
    if (!this.b) { if (s.point.distanceTo(this.a) < 1e-3) return; this.b = s.point.clone(); ctx.setHint('Click where the dimension line goes.'); return; }
    const o = this.offsetFor(s.point); const a = this.a, b = this.b; this.a = this.b = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint);
    await ctx.ops([{ op: 'add_dimension', a: arr(a), b: arr(b), offset: arr(o) }], 'Dimension');
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape') { this.a = this.b = null; ctx.scene.clearOverlay(); return true; } return false; }
}
void SELECT; void LegacyScaleTool;
