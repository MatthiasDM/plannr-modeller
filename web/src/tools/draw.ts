import * as THREE from 'three';
import type { Tool, ToolContext, Plane, Snap } from './base';
import { parseNumbers, parsePoint, lineColor, drawSnap, planeFromSnap, surfacePlane, fromAxes, GROUND, arr, INK, AXIS, DrawPlane } from './base';
import { fmtM } from '../model';

/** In-plane basis (u, w) for a plane normal, u following world x where possible. */
export function planeBasis(normal: THREE.Vector3): { u: THREE.Vector3; w: THREE.Vector3 } {
  const n = normal.clone().normalize();
  let u = AXIS.x.clone().sub(n.clone().multiplyScalar(AXIS.x.dot(n)));
  if (u.lengthSq() < 1e-6) u = AXIS.z.clone().sub(n.clone().multiplyScalar(AXIS.z.dot(n)));
  u.normalize(); const w = n.clone().cross(u).normalize(); return { u, w };
}

/** Line (pencil): chained edges; closed coplanar loops become faces (done by the engine). */
export class LineTool implements Tool {
  id = 'line'; label = 'Line'; hint = 'Click the start of the line, or type its position as [x; y; z]. Arrow keys lock an axis (→ red, ← green, ↑ blue); hold Shift to keep the inference under the cursor.'; cursor = 'crosshair'; shortcut = 'l';
  private start: THREE.Vector3 | null = null; private first: THREE.Vector3 | null = null;
  private plane: Plane = GROUND; private cur: Snap | null = null;
  busy() { return !!this.start; }
  activate(ctx: ToolContext) { this.start = null; this.first = null; ctx.setHint(this.hint); ctx.setVCB('', 'Length'); }
  deactivate(ctx: ToolContext) { this.start = null; ctx.scene.clearOverlay(); }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = ctx.snap(ev, { plane: this.plane, from: this.start ?? undefined, lock: ctx.lock, faces: true });
    this.cur = s; ctx.scene.clearOverlay(); drawSnap(ctx, s);
    if (!this.start) return;
    ctx.scene.addLine([this.start, s.point], lineColor(this.start, s.point, s), false, 2);
    const L = this.start.distanceTo(s.point); ctx.scene.addLabel(this.start.clone().lerp(s.point, 0.5), fmtM(L)); ctx.setVCB(L.toFixed(3), 'Length');
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const s = ctx.snap(ev, { plane: this.plane, from: this.start ?? undefined, lock: ctx.lock, faces: true });
    if (!this.start) { this.start = s.point.clone(); this.first = this.start.clone(); this.plane = planeFromSnap(s, GROUND); ctx.setHint(this.next); return; }
    this.place(ctx, s.point);
  }
  private async place(ctx: ToolContext, end: THREE.Vector3) {
    if (!this.start || this.start.distanceTo(end) < 1e-3) return;
    const closing = this.first && end.distanceTo(this.first) < 1e-3;
    const res = await ctx.ops([{ op: 'draw_line', points: [arr(this.start), arr(end)], layer: ctx.options.layer }], 'Line');
    if (closing || res[0]?.created?.length > 1) { this.start = null; this.first = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint); }
    else this.start = end.clone();
  }
  private next = 'Click the end of the line, or type a length. [x; y; z] goes to a position, <x; y; z> steps from the last point (y is up). Esc ends the chain.';
  onVCB(v: string, ctx: ToolContext) {
    const c = parsePoint(v);
    if (c) { // a typed position, or a step from the last point (both along the drawing axes)
      if (c.relative) c.p.copy(fromAxes(c.p).sub(GROUND.origin)); else c.p.copy(fromAxes(c.p));
      if (this.start) { this.place(ctx, c.relative ? this.start.clone().add(c.p) : c.p); return; }
      if (c.relative) return;
      this.start = c.p; this.first = c.p.clone(); this.plane = { origin: c.p.clone(), normal: GROUND.normal.clone() }; ctx.setHint(this.next); return;
    }
    const n = parseNumbers(v); if (!n.length || !this.start || !this.cur) return;
    const dir = this.cur.point.clone().sub(this.start); if (dir.lengthSq() < 1e-9) return; dir.normalize();
    this.place(ctx, this.start.clone().addScaledVector(dir, n[0]));
  }
  onDblClick(_e: MouseEvent, ctx: ToolContext) { this.start = null; this.first = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint); }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape') { this.start = null; this.first = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint); return true; } return false; }
}

/** Rectangle: two corners on the ground or on the face under the first click. */
export class RectangleTool implements Tool {
  id = 'rectangle'; label = 'Rectangle'; hint = 'Click the first corner (on the ground or on a face). Arrow keys turn the plane to face an axis (→ red, ← green, ↑ blue).'; cursor = 'crosshair'; shortcut = 'r'; planeLock = true;
  private first: THREE.Vector3 | null = null; private dp = new DrawPlane(); private get plane() { return this.dp.plane; } private last: THREE.Vector3 | null = null; private lastFace: string | null = null;
  busy() { return !!this.first; }
  activate(ctx: ToolContext) { this.first = null; ctx.setHint(this.hint); ctx.setVCB('', 'Dimensions'); }
  deactivate(ctx: ToolContext) { this.first = null; ctx.scene.clearOverlay(); }
  private corners(a: THREE.Vector3, b: THREE.Vector3) {
    const { u, w } = planeBasis(this.plane.normal); const d = b.clone().sub(a); const du = d.dot(u), dw = d.dot(w);
    return { pts: [a.clone(), a.clone().addScaledVector(u, du), a.clone().addScaledVector(u, du).addScaledVector(w, dw), a.clone().addScaledVector(w, dw)], du, dw, u, w };
  }
  /** The corner on the plane; a free corner close to a square snaps to one (SketchUp's "Square" inference). */
  private corner(ctx: ToolContext, s: Snap) {
    const p = this.onPlane(s.point);
    if (!this.first || !['plane', 'grid', 'face', 'from', 'intersection'].includes(s.kind)) return p;
    const { u, w } = planeBasis(this.plane.normal); const d = p.clone().sub(this.first); const du = d.dot(u), dw = d.dot(w); const tol = ctx.scene.pixelSize(p) * 6;
    if (Math.abs(du) < tol || Math.abs(Math.abs(du) - Math.abs(dw)) > tol) return p;
    const L = (Math.abs(du) + Math.abs(dw)) / 2; const q = this.first.clone().addScaledVector(u, Math.sign(du) * L).addScaledVector(w, Math.sign(dw) * L);
    s.point.copy(q); s.kind = 'square'; s.refs = undefined; return q; // the app's marker shows the snap it was handed
  }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = ctx.snap(ev, { plane: this.first ? this.plane : GROUND, faces: !this.first, inPlane: !!this.first });
    ctx.scene.clearOverlay(); drawSnap(ctx, s);
    if (!this.first) { this.dp.hover(ctx, ev, s, 'square'); return; }
    const p = this.corner(ctx, s); this.last = p;
    const { pts, du, dw } = this.corners(this.first, p);
    ctx.scene.addLine([...pts, pts[0]], INK, false, 2); ctx.scene.addFace(pts);
    if (Math.abs(du) > 0.01) ctx.scene.addLabel(pts[0].clone().lerp(pts[1], 0.5), fmtM(Math.abs(du)));
    if (Math.abs(dw) > 0.01) ctx.scene.addLabel(pts[1].clone().lerp(pts[2], 0.5), fmtM(Math.abs(dw)));
    ctx.setVCB(`${Math.abs(du).toFixed(3)}; ${Math.abs(dw).toFixed(3)}`, 'Dimensions');
  }
  private onPlane(p: THREE.Vector3) { const n = this.plane.normal; return p.clone().sub(n.clone().multiplyScalar(p.clone().sub(this.plane.origin).dot(n))); }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const s = ctx.snap(ev, { plane: this.first ? this.plane : GROUND, faces: !this.first, inPlane: !!this.first });
    if (!this.first) { this.first = s.point.clone(); this.dp.start(ctx, ev, s); ctx.setHint('Click the opposite corner, or type width; depth and press Enter.'); return; }
    this.finish(ctx, this.corner(ctx, s));
  }
  private async finish(ctx: ToolContext, b: THREE.Vector3) {
    if (!this.first) return;
    const { du, dw } = this.corners(this.first, b);
    if (Math.abs(du) < 0.005 || Math.abs(dw) < 0.005) { ctx.toast('Rectangle too small'); return; }
    const { u } = planeBasis(this.plane.normal);
    const res = await ctx.ops([{ op: 'draw_rectangle', corner: arr(this.first), opposite: arr(b), normal: arr(this.plane.normal), u: arr(u), layer: ctx.options.layer }], 'Rectangle');
    this.lastFace = res[0]?.created?.[0] ?? null; this.first = null; ctx.scene.clearOverlay(); ctx.setHint('Rectangle added. Click the first corner of the next one, or type new dimensions to resize it.');
    if (this.lastFace) ctx.select([this.lastFace]);
  }
  onVCB(v: string, ctx: ToolContext) {
    const n = parseNumbers(v); if (n.length < 2) return;
    if (this.first) {
      const { u, w } = planeBasis(this.plane.normal); const d = this.last ? this.last.clone().sub(this.first) : new THREE.Vector3(1, 0, 1);
      const su = d.dot(u) < 0 ? -1 : 1, sw = d.dot(w) < 0 ? -1 : 1;
      this.finish(ctx, this.first.clone().addScaledVector(u, su * n[0]).addScaledVector(w, sw * n[1]));
    }
  }
  onLock(ctx: ToolContext) { if (this.first) this.dp.relock(ctx); }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.first) { this.first = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint); return true; } return false; }
}

/** Circle: centre then radius. */
export class CircleTool implements Tool {
  id = 'circle'; label = 'Circle'; hint = 'Click the centre of the circle (on the ground or on a face). Arrow keys turn the plane to face an axis (→ red, ← green, ↑ blue). Type "24s" to set the number of sides.'; cursor = 'crosshair'; shortcut = 'c'; planeLock = true;
  private center: THREE.Vector3 | null = null; private dp = new DrawPlane(); private get plane() { return this.dp.plane; } private r = 0;
  busy() { return !!this.center; }
  activate(ctx: ToolContext) { this.center = null; ctx.setHint(this.hint); ctx.setVCB(`${ctx.options.circleSegments}s`, 'Sides'); }
  deactivate(ctx: ToolContext) { this.center = null; ctx.scene.clearOverlay(); }
  private ring(c: THREE.Vector3, r: number, n: number) { const { u, w } = planeBasis(this.plane.normal); const pts: THREE.Vector3[] = []; for (let i = 0; i < n; i++) { const a = (i / n) * Math.PI * 2; pts.push(c.clone().addScaledVector(u, Math.cos(a) * r).addScaledVector(w, Math.sin(a) * r)); } return pts; }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = ctx.snap(ev, { plane: this.center ? this.plane : GROUND, faces: !this.center, inPlane: !!this.center, from: this.center ?? undefined });
    ctx.scene.clearOverlay(); drawSnap(ctx, s);
    if (!this.center) { this.dp.hover(ctx, ev, s, 'ring'); return; }
    this.r = this.center.distanceTo(this.dp.on(s.point)); const pts = this.ring(this.center, this.r, ctx.options.circleSegments);
    ctx.scene.addLine([...pts, pts[0]], INK, false, 2); ctx.scene.addFace(pts); ctx.scene.addLine([this.center, s.point], 0x888888, true);
    ctx.scene.addLabel(this.center.clone().lerp(s.point, 0.5), `r ${fmtM(this.r)}`); ctx.setVCB(this.r.toFixed(3), 'Radius');
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const s = ctx.snap(ev, { plane: this.center ? this.plane : GROUND, faces: !this.center, inPlane: !!this.center, from: this.center ?? undefined });
    if (!this.center) { this.center = s.point.clone(); this.dp.start(ctx, ev, s); ctx.setHint('Click to set the radius, or type it and press Enter.'); return; }
    this.finish(ctx, this.center.distanceTo(this.dp.on(s.point)));
  }
  private async finish(ctx: ToolContext, r: number) {
    if (!this.center || r < 0.005) return;
    const res = await ctx.ops([{ op: 'draw_circle', center: arr(this.center), normal: arr(this.plane.normal), radius: +r.toFixed(5), segments: ctx.options.circleSegments, layer: ctx.options.layer }], 'Circle');
    this.center = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint); if (res[0]?.created?.[0]) ctx.select([res[0].created[0]]);
  }
  onVCB(v: string, ctx: ToolContext) {
    const m = v.match(/^(\d+)\s*s$/i); if (m) { ctx.options.circleSegments = Math.max(3, Math.min(200, parseInt(m[1]))); ctx.toast(`${ctx.options.circleSegments} sides`); return; }
    const n = parseNumbers(v); if (n.length && this.center) this.finish(ctx, n[0]);
  }
  onLock(ctx: ToolContext) { if (this.center) this.dp.relock(ctx); }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.center) { this.center = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint); return true; } return false; }
}

/** Polygon: click each corner (closes on the first point or Enter); or, after typing a number of sides
 *  ("6s"), a regular polygon from its centre and one corner. */
export class PolygonTool implements Tool {
  id = 'polygon'; label = 'Polygon'; hint = 'Click each corner; click the first corner or press Enter to close. Type 6s for a regular polygon with six sides (0s for free corners again). Arrow keys turn the plane to face an axis (→ red, ← green, ↑ blue).'; cursor = 'crosshair'; shortcut = 'n'; planeLock = true;
  private pts: THREE.Vector3[] = []; private dp = new DrawPlane(); private get plane() { return this.dp.plane; }
  /** Regular polygon: the number of sides (0 = free corners) and its centre once placed; where the cursor was last. */
  private sides = 0; private centre: THREE.Vector3 | null = null; private cur: THREE.Vector3 | null = null;
  busy() { return this.pts.length > 0 || !!this.centre; }
  private idle(ctx: ToolContext) {
    ctx.setHint(this.sides ? `Regular polygon with ${this.sides} sides: click the centre, then a corner (or type the radius). Type 0s for free corners.` : this.hint);
    ctx.setVCB(this.sides ? `${this.sides}s` : '', this.sides ? 'Sides' : 'Length');
  }
  activate(ctx: ToolContext) { this.pts = []; this.centre = null; this.idle(ctx); }
  deactivate(ctx: ToolContext) { this.pts = []; this.centre = null; ctx.scene.clearOverlay(); }
  /** The corners of the regular polygon about the centre with one corner at `corner`, counter-clockwise seen along the plane's normal. */
  private ring(corner: THREE.Vector3) {
    const c = this.centre!, d = corner.clone().sub(c), n = this.plane.normal.clone().normalize(); const pts: THREE.Vector3[] = [];
    for (let i = 0; i < this.sides; i++) pts.push(c.clone().add(d.clone().applyAxisAngle(n, (i / this.sides) * Math.PI * 2)));
    return pts;
  }
  private snapAt(ev: PointerEvent, ctx: ToolContext) {
    const started = this.pts.length > 0 || !!this.centre;
    const s = ctx.snap(ev, { plane: started ? this.plane : GROUND, faces: !started, inPlane: started, from: this.pts.at(-1) ?? this.centre ?? undefined }); if (started) s.point.copy(this.dp.on(s.point));
    return s;
  }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = this.snapAt(ev, ctx); this.cur = s.point.clone();
    ctx.scene.clearOverlay(); drawSnap(ctx, s);
    if (!this.pts.length && !this.centre) { this.dp.hover(ctx, ev, s, this.sides ? 'ring' : 'square'); return; }
    if (this.centre) {
      const r = this.centre.distanceTo(s.point); if (r < 1e-6) return; const pts = this.ring(s.point);
      ctx.scene.addLine([...pts, pts[0]], INK, false, 2); ctx.scene.addFace(pts); ctx.scene.addLine([this.centre, s.point], 0x888888, true);
      ctx.scene.addLabel(this.centre.clone().lerp(s.point, 0.5), `r ${fmtM(r)}`); ctx.setVCB(r.toFixed(3), 'Radius'); return;
    }
    ctx.scene.addLine([...this.pts, s.point], lineColor(this.pts.at(-1)!, s.point, s), false, 2);
    if (this.pts.length >= 2) ctx.scene.addFace([...this.pts, s.point]);
    const L = this.pts.at(-1)!.distanceTo(s.point); ctx.setVCB(L.toFixed(3), 'Length'); ctx.scene.addLabel(this.pts.at(-1)!.clone().lerp(s.point, 0.5), fmtM(L));
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const s = this.snapAt(ev, ctx);
    if (this.sides) {
      if (!this.centre) { this.dp.start(ctx, ev, s); this.centre = s.point.clone(); ctx.setHint('Click a corner of the polygon, or type the radius and press Enter.'); return; }
      this.regular(ctx, s.point); return;
    }
    if (!this.pts.length) this.dp.start(ctx, ev, s);
    if (this.pts.length >= 3 && s.point.distanceTo(this.pts[0]) < ctx.scene.pixelSize(s.point) * 10) return this.close(ctx);
    this.pts.push(s.point.clone());
  }
  private async close(ctx: ToolContext) {
    if (this.pts.length < 3) return;
    const res = await ctx.ops([{ op: 'draw_polygon', points: this.pts.map(arr), layer: ctx.options.layer }], 'Polygon');
    this.pts = []; ctx.scene.clearOverlay(); if (res[0]?.created?.[0]) ctx.select([res[0].created[0]]);
  }
  private async regular(ctx: ToolContext, corner: THREE.Vector3) {
    if (!this.centre || this.centre.distanceTo(corner) < 0.005) return;
    const res = await ctx.ops([{ op: 'draw_polygon', points: this.ring(corner).map(arr), layer: ctx.options.layer }], 'Polygon');
    this.centre = null; ctx.scene.clearOverlay(); this.idle(ctx); if (res[0]?.created?.[0]) ctx.select([res[0].created[0]]);
  }
  onVCB(v: string, ctx: ToolContext) {
    const m = v.match(/^(\d+)\s*s$/i);
    if (m) { const n = parseInt(m[1]); this.sides = n >= 3 ? Math.min(n, 200) : 0; this.pts = []; this.centre = null; ctx.scene.clearOverlay(); this.idle(ctx); return; }
    const n = parseNumbers(v); if (!n.length || n[0] <= 0) return;
    if (this.centre) { // the radius, towards the cursor
      const d = this.cur ? this.cur.clone().sub(this.centre) : new THREE.Vector3(); if (d.lengthSq() < 1e-9) d.copy(planeBasis(this.plane.normal).u);
      this.regular(ctx, this.centre.clone().addScaledVector(d.normalize(), n[0])); return;
    }
    if (this.pts.length && this.cur) { // the length of the side being drawn, towards the cursor
      const d = this.cur.clone().sub(this.pts.at(-1)!); if (d.lengthSq() < 1e-9) return;
      this.pts.push(this.pts.at(-1)!.clone().addScaledVector(d.normalize(), n[0]));
    }
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) {
    if (ev.key === 'Enter') { this.close(ctx); return true; }
    if (ev.key === 'Escape' && this.busy()) { this.pts = []; this.centre = null; ctx.scene.clearOverlay(); this.idle(ctx); return true; }
    return false;
  }
  /** The plane turns about the first corner (or the centre); once there are more corners, they fix it. */
  onLock(ctx: ToolContext) { if (this.pts.length === 1 || this.centre) this.dp.relock(ctx); }
}

/** Freehand: press and drag to draw a hand-drawn line on the face (or the ground) under the first point. Let go
 *  where it began and it closes into a face. */
export class FreehandTool implements Tool {
  id = 'freehand'; label = 'Freehand'; hint = 'Press and drag to draw by hand. Let go near where you began to close the shape into a face.'; cursor = 'crosshair';
  private pts: THREE.Vector3[] = []; private plane: Plane = GROUND; private at = { x: 0, y: 0 }; private from = { x: 0, y: 0 };
  busy() { return this.pts.length > 0; }
  activate(ctx: ToolContext) { this.pts = []; ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  deactivate(ctx: ToolContext) { this.pts = []; ctx.scene.clearOverlay(); }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return; const s = ctx.snap(ev, { faces: true });
    this.plane = surfacePlane(ctx, ev, s); this.pts = [s.point.clone()]; this.at = this.from = { x: ev.clientX, y: ev.clientY };
  }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (!this.pts.length) { const s = ctx.snap(ev, { faces: true }); ctx.scene.clearOverlay(); drawSnap(ctx, s); return; }
    if (Math.hypot(ev.clientX - this.at.x, ev.clientY - this.at.y) < 7) return; // a point every few pixels
    const p = ctx.scene.planePoint(ev, this.plane.origin, this.plane.normal); if (!p || p.distanceTo(this.pts[this.pts.length - 1]) < 0.002) return;
    this.pts.push(p); this.at = { x: ev.clientX, y: ev.clientY }; ctx.scene.setSnapMarker(null);
    ctx.scene.clearOverlay(); ctx.scene.addLine(this.pts, INK, false, 2);
    if (this.closes(ev)) ctx.scene.addLabel(this.pts[0], 'Close', 'infer');
  }
  private closes(ev: { clientX: number; clientY: number }) { return this.pts.length > 4 && Math.hypot(ev.clientX - this.from.x, ev.clientY - this.from.y) < 12; }
  async onUp(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0 || !this.pts.length) return;
    const close = this.closes(ev); const pts = close ? this.pts.slice(0, -1) : this.pts; this.pts = []; ctx.scene.clearOverlay();
    if (pts.length < 2) return;
    await ctx.ops([{ op: 'draw_line', points: pts.map(arr), close, layer: ctx.options.layer }], 'Freehand');
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.pts.length) { this.pts = []; ctx.scene.clearOverlay(); return true; } return false; }
}
