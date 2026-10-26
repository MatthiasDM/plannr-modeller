import * as THREE from 'three';
import type { Tool, ToolContext, Plane, Snap } from './base';
import { parseNumbers, lineColor, drawSnap, planeFromSnap, GROUND, arr, INK, AXIS } from './base';
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
  id = 'line'; label = 'Line'; hint = 'Click the start of the line. Arrow keys lock an axis (→ red, ← green, ↑ blue).'; cursor = 'crosshair'; shortcut = 'l';
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
    if (!this.start) { this.start = s.point.clone(); this.first = this.start.clone(); this.plane = planeFromSnap(s, GROUND); ctx.setHint('Click the end of the line, or type a length and press Enter. Esc ends the chain.'); return; }
    this.place(ctx, s.point);
  }
  private async place(ctx: ToolContext, end: THREE.Vector3) {
    if (!this.start || this.start.distanceTo(end) < 1e-3) return;
    const closing = this.first && end.distanceTo(this.first) < 1e-3;
    const res = await ctx.ops([{ op: 'draw_line', points: [arr(this.start), arr(end)], layer: ctx.options.layer }], 'Line');
    if (closing || res[0]?.created?.length > 1) { this.start = null; this.first = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint); }
    else this.start = end.clone();
  }
  onVCB(v: string, ctx: ToolContext) {
    const n = parseNumbers(v); if (!n.length || !this.start || !this.cur) return;
    const dir = this.cur.point.clone().sub(this.start); if (dir.lengthSq() < 1e-9) return; dir.normalize();
    this.place(ctx, this.start.clone().addScaledVector(dir, n[0]));
  }
  onDblClick(_e: MouseEvent, ctx: ToolContext) { this.start = null; this.first = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint); }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape') { this.start = null; this.first = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint); return true; } return false; }
}

/** Rectangle: two corners on the ground or on the face under the first click. */
export class RectangleTool implements Tool {
  id = 'rectangle'; label = 'Rectangle'; hint = 'Click the first corner (on the ground or on a face).'; cursor = 'crosshair'; shortcut = 'r';
  private first: THREE.Vector3 | null = null; private plane: Plane = GROUND; private last: THREE.Vector3 | null = null; private lastFace: string | null = null;
  busy() { return !!this.first; }
  activate(ctx: ToolContext) { this.first = null; ctx.setHint(this.hint); ctx.setVCB('', 'Dimensions'); }
  deactivate(ctx: ToolContext) { this.first = null; ctx.scene.clearOverlay(); }
  private corners(a: THREE.Vector3, b: THREE.Vector3) {
    const { u, w } = planeBasis(this.plane.normal); const d = b.clone().sub(a); const du = d.dot(u), dw = d.dot(w);
    return { pts: [a.clone(), a.clone().addScaledVector(u, du), a.clone().addScaledVector(u, du).addScaledVector(w, dw), a.clone().addScaledVector(w, dw)], du, dw, u, w };
  }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = ctx.snap(ev, { plane: this.first ? this.plane : GROUND, faces: !this.first });
    ctx.scene.clearOverlay(); drawSnap(ctx, s);
    if (!this.first) return;
    const p = this.onPlane(s.point); this.last = p;
    const { pts, du, dw } = this.corners(this.first, p);
    ctx.scene.addLine([...pts, pts[0]], INK, false, 2); ctx.scene.addFace(pts);
    if (Math.abs(du) > 0.01) ctx.scene.addLabel(pts[0].clone().lerp(pts[1], 0.5), fmtM(Math.abs(du)));
    if (Math.abs(dw) > 0.01) ctx.scene.addLabel(pts[1].clone().lerp(pts[2], 0.5), fmtM(Math.abs(dw)));
    ctx.setVCB(`${Math.abs(du).toFixed(3)}; ${Math.abs(dw).toFixed(3)}`, 'Dimensions');
  }
  private onPlane(p: THREE.Vector3) { const n = this.plane.normal; return p.clone().sub(n.clone().multiplyScalar(p.clone().sub(this.plane.origin).dot(n))); }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const s = ctx.snap(ev, { plane: this.first ? this.plane : GROUND, faces: !this.first });
    if (!this.first) { this.first = s.point.clone(); this.plane = planeFromSnap(s, GROUND); ctx.setHint('Click the opposite corner, or type width; depth and press Enter.'); return; }
    this.finish(ctx, this.onPlane(s.point));
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
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.first) { this.first = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint); return true; } return false; }
}

/** Circle: centre then radius. */
export class CircleTool implements Tool {
  id = 'circle'; label = 'Circle'; hint = 'Click the centre of the circle (on the ground or on a face). Type "24s" to set the number of sides.'; cursor = 'crosshair'; shortcut = 'c';
  private center: THREE.Vector3 | null = null; private plane: Plane = GROUND; private r = 0;
  busy() { return !!this.center; }
  activate(ctx: ToolContext) { this.center = null; ctx.setHint(this.hint); ctx.setVCB(`${ctx.options.circleSegments}s`, 'Sides'); }
  deactivate(ctx: ToolContext) { this.center = null; ctx.scene.clearOverlay(); }
  private ring(c: THREE.Vector3, r: number, n: number) { const { u, w } = planeBasis(this.plane.normal); const pts: THREE.Vector3[] = []; for (let i = 0; i < n; i++) { const a = (i / n) * Math.PI * 2; pts.push(c.clone().addScaledVector(u, Math.cos(a) * r).addScaledVector(w, Math.sin(a) * r)); } return pts; }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = ctx.snap(ev, { plane: this.center ? this.plane : GROUND, faces: !this.center, from: this.center ?? undefined, lock: ctx.lock });
    ctx.scene.clearOverlay(); drawSnap(ctx, s);
    if (!this.center) return;
    this.r = this.center.distanceTo(s.point); const pts = this.ring(this.center, this.r, ctx.options.circleSegments);
    ctx.scene.addLine([...pts, pts[0]], INK, false, 2); ctx.scene.addFace(pts); ctx.scene.addLine([this.center, s.point], 0x888888, true);
    ctx.scene.addLabel(this.center.clone().lerp(s.point, 0.5), `r ${fmtM(this.r)}`); ctx.setVCB(this.r.toFixed(3), 'Radius');
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const s = ctx.snap(ev, { plane: this.center ? this.plane : GROUND, faces: !this.center, from: this.center ?? undefined, lock: ctx.lock });
    if (!this.center) { this.center = s.point.clone(); this.plane = planeFromSnap(s, GROUND); ctx.setHint('Click to set the radius, or type it and press Enter.'); return; }
    this.finish(ctx, this.center.distanceTo(s.point));
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
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.center) { this.center = null; ctx.scene.clearOverlay(); return true; } return false; }
}

/** Polygon: click each corner; closes on the first point or Enter. */
export class PolygonTool implements Tool {
  id = 'polygon'; label = 'Polygon'; hint = 'Click each corner; click the first corner or press Enter to close.'; cursor = 'crosshair'; shortcut = 'n';
  private pts: THREE.Vector3[] = []; private plane: Plane = GROUND;
  busy() { return this.pts.length > 0; }
  activate(ctx: ToolContext) { this.pts = []; ctx.setHint(this.hint); ctx.setVCB('', 'Length'); }
  deactivate(ctx: ToolContext) { this.pts = []; ctx.scene.clearOverlay(); }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = ctx.snap(ev, { plane: this.pts.length ? this.plane : GROUND, faces: !this.pts.length, from: this.pts.at(-1), lock: ctx.lock });
    ctx.scene.clearOverlay(); drawSnap(ctx, s);
    if (!this.pts.length) return;
    ctx.scene.addLine([...this.pts, s.point], lineColor(this.pts.at(-1)!, s.point, s), false, 2);
    if (this.pts.length >= 2) ctx.scene.addFace([...this.pts, s.point]);
    const L = this.pts.at(-1)!.distanceTo(s.point); ctx.setVCB(L.toFixed(3), 'Length'); ctx.scene.addLabel(this.pts.at(-1)!.clone().lerp(s.point, 0.5), fmtM(L));
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const s = ctx.snap(ev, { plane: this.pts.length ? this.plane : GROUND, faces: !this.pts.length, from: this.pts.at(-1), lock: ctx.lock });
    if (!this.pts.length) this.plane = planeFromSnap(s, GROUND);
    if (this.pts.length >= 3 && s.point.distanceTo(this.pts[0]) < ctx.scene.pixelSize(s.point) * 10) return this.close(ctx);
    this.pts.push(s.point.clone());
  }
  private async close(ctx: ToolContext) {
    if (this.pts.length < 3) return;
    const res = await ctx.ops([{ op: 'draw_polygon', points: this.pts.map(arr), layer: ctx.options.layer }], 'Polygon');
    this.pts = []; ctx.scene.clearOverlay(); if (res[0]?.created?.[0]) ctx.select([res[0].created[0]]);
  }
  onVCB(v: string, ctx: ToolContext) { const n = parseNumbers(v); if (!n.length || !this.pts.length) return; void ctx; }
  onKey(ev: KeyboardEvent, ctx: ToolContext) {
    if (ev.key === 'Enter') { this.close(ctx); return true; }
    if (ev.key === 'Escape') { this.pts = []; ctx.scene.clearOverlay(); return true; }
    return false;
  }
}
