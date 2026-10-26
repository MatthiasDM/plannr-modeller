// Arc tools (centre, three-point, two-point, pie), Rotated Rectangle, Protractor and Zoom Window.
import * as THREE from 'three';
import type { Tool, ToolContext, Plane, Snap } from './base';
import { parseNumbers, drawSnap, planeFromSnap, GROUND, arr, INK, MAGENTA, lineColor } from './base';
import { fmtM, fmtDeg } from '../model';
import { planeBasis } from './draw';

const polyArc = (c: THREE.Vector3, n: THREE.Vector3, a: THREE.Vector3, sweep: number, segs: number) => {
  const r = a.clone().sub(c); const pts: THREE.Vector3[] = [];
  for (let i = 0; i <= segs; i++) pts.push(c.clone().add(r.clone().applyAxisAngle(n, (sweep * i) / segs)));
  return pts;
};
const sweepTo = (c: THREE.Vector3, n: THREE.Vector3, a: THREE.Vector3, b: THREE.Vector3) => {
  const u = a.clone().sub(c).normalize(), v = b.clone().sub(c).normalize(); let ang = Math.atan2(u.clone().cross(v).dot(n), u.dot(v)); if (ang < 0) ang += Math.PI * 2; return ang;
};
/** Signed sweep: the short way round, negative when it runs clockwise about `n`. */
const shortSweep = (c: THREE.Vector3, n: THREE.Vector3, a: THREE.Vector3, b: THREE.Vector3) => { const s = sweepTo(c, n, a, b); return s > Math.PI ? s - Math.PI * 2 : s; };
const segsFor = (sweep: number) => Math.max(1, Math.min(200, Math.ceil((12 * sweep) / (Math.PI / 2))));

/** Centre arc: centre, start, then the end angle. Pie variant closes with the radii. */
export class ArcTool implements Tool {
  id: string; label: string; hint = 'Click the centre, the start of the arc, then its end. Type a radius or an angle (e.g. 90°).'; cursor = 'crosshair'; shortcut?: string;
  private c: THREE.Vector3 | null = null; private a: THREE.Vector3 | null = null; private plane: Plane = GROUND; private cur: Snap | null = null;
  constructor(private pie = false) { this.id = pie ? 'pie' : 'arc'; this.label = pie ? 'Pie' : 'Arc'; if (!pie) this.shortcut = 'a'; }
  busy() { return !!this.c; }
  activate(ctx: ToolContext) { this.c = this.a = null; ctx.setHint(this.hint); ctx.setVCB('', 'Radius'); }
  deactivate(ctx: ToolContext) { this.c = this.a = null; ctx.scene.clearOverlay(); }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = ctx.snap(ev, { plane: this.c ? this.plane : GROUND, faces: !this.c, from: this.c ?? undefined, lock: ctx.lock }); this.cur = s; ctx.scene.clearOverlay(); drawSnap(ctx, s);
    if (!this.c) return;
    if (!this.a) { ctx.scene.addLine([this.c, s.point], lineColor(this.c, s.point, s), true); ctx.scene.addLabel(this.c.clone().lerp(s.point, 0.5), `r ${fmtM(this.c.distanceTo(s.point))}`); ctx.setVCB(this.c.distanceTo(s.point).toFixed(3), 'Radius'); return; }
    const signed = shortSweep(this.c, this.plane.normal, this.a, s.point); const n = signed < 0 ? this.plane.normal.clone().negate() : this.plane.normal; const sweep = Math.abs(signed); const pts = polyArc(this.c, n, this.a, sweep, segsFor(sweep));
    ctx.scene.addLine(pts, INK, false, 2); ctx.scene.addLine([this.c, this.a], 0x888888, true); ctx.scene.addLine([this.c, pts[pts.length - 1]], 0x888888, true);
    if (this.pie) ctx.scene.addFace([this.c, ...pts]);
    ctx.scene.addLabel(pts[Math.floor(pts.length / 2)], fmtDeg((sweep * 180) / Math.PI)); ctx.setVCB(((sweep * 180) / Math.PI).toFixed(1), 'Angle');
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return; const s = ctx.snap(ev, { plane: this.c ? this.plane : GROUND, faces: !this.c, from: this.c ?? undefined, lock: ctx.lock });
    if (!this.c) { this.c = s.point.clone(); this.plane = planeFromSnap(s, GROUND); ctx.setHint('Click where the arc starts (sets the radius).'); return; }
    if (!this.a) { if (s.point.distanceTo(this.c) < 0.005) return; this.a = this.onPlane(s.point); ctx.setHint('Click where the arc ends, or type the angle.'); return; }
    this.finish(ctx, shortSweep(this.c, this.plane.normal, this.a, this.onPlane(s.point)));
  }
  private onPlane(p: THREE.Vector3) { const n = this.plane.normal; return p.clone().sub(n.clone().multiplyScalar(p.clone().sub(this.plane.origin).dot(n))); }
  private async finish(ctx: ToolContext, signed: number) {
    if (!this.c || !this.a || Math.abs(signed) < 0.001) return; const c = this.c, a = this.a; const n = signed < 0 ? this.plane.normal.clone().negate() : this.plane.normal.clone(); const sweep = Math.abs(signed);
    const end = a.clone().sub(c).applyAxisAngle(n, sweep).add(c); this.c = this.a = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint);
    await ctx.ops([{ op: 'draw_arc', center: arr(c), start: arr(a), end: arr(end), normal: arr(n), pie: this.pie, layer: ctx.options.layer }], this.pie ? 'Pie' : 'Arc');
  }
  onVCB(v: string, ctx: ToolContext) {
    const n = parseNumbers(v); if (!n.length || !this.c) return;
    if (!this.a) { const dir = this.cur ? this.cur.point.clone().sub(this.c) : new THREE.Vector3(1, 0, 0); if (dir.lengthSq() < 1e-9) dir.set(1, 0, 0); this.a = this.onPlane(this.c.clone().addScaledVector(dir.normalize(), n[0])); ctx.setHint('Click where the arc ends, or type the angle.'); return; }
    this.finish(ctx, (n[0] * Math.PI) / 180);
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.c) { this.c = this.a = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint); return true; } return false; }
}

/** Three-point arc: start, a point on the arc, end. */
export class ThreePointArcTool implements Tool {
  id = 'arc3'; label = '3 Point Arc'; hint = 'Click the start, a point the arc passes through, then the end.'; cursor = 'crosshair';
  private pts: THREE.Vector3[] = []; private plane: Plane = GROUND;
  busy() { return this.pts.length > 0; }
  activate(ctx: ToolContext) { this.pts = []; ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  deactivate(ctx: ToolContext) { this.pts = []; ctx.scene.clearOverlay(); }
  private preview(ctx: ToolContext, p: THREE.Vector3) {
    const [a, b] = this.pts; if (!b) { ctx.scene.addLine([a, p], INK, true); return; }
    const fit = circleThrough(a, b, p); if (!fit) { ctx.scene.addLine([a, b, p], INK); return; }
    const sweep = sweepTo(fit.c, fit.n, a, p); ctx.scene.addLine(polyArc(fit.c, fit.n, a, sweep, segsFor(sweep)), INK, false, 2); ctx.scene.addLabel(fit.c, `r ${fmtM(fit.r)}`, 'small');
  }
  onMove(ev: PointerEvent, ctx: ToolContext) { const s = ctx.snap(ev, { plane: this.pts.length ? this.plane : GROUND, faces: !this.pts.length, from: this.pts.at(-1), lock: ctx.lock }); ctx.scene.clearOverlay(); drawSnap(ctx, s); if (this.pts.length) this.preview(ctx, s.point); }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return; const s = ctx.snap(ev, { plane: this.pts.length ? this.plane : GROUND, faces: !this.pts.length, from: this.pts.at(-1), lock: ctx.lock });
    if (!this.pts.length) this.plane = planeFromSnap(s, GROUND);
    this.pts.push(s.point.clone());
    if (this.pts.length === 3) { const [a, b, c] = this.pts; this.pts = []; ctx.scene.clearOverlay(); await ctx.ops([{ op: 'draw_arc', through: [arr(a), arr(b), arr(c)], layer: ctx.options.layer }], '3 point arc'); }
    else ctx.setHint(this.pts.length === 1 ? 'Click a point the arc passes through.' : 'Click the end of the arc.');
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.pts.length) { this.pts = []; ctx.scene.clearOverlay(); return true; } return false; }
}
function circleThrough(a: THREE.Vector3, b: THREE.Vector3, c: THREE.Vector3) {
  const ab = b.clone().sub(a), ac = c.clone().sub(a); const n = ab.clone().cross(ac); if (n.lengthSq() < 1e-12) return null; n.normalize();
  const ab2 = ab.lengthSq(), ac2 = ac.lengthSq(); const cr = ab.clone().cross(ac); const d = 2 * cr.lengthSq();
  const o = ac.clone().multiplyScalar(ab2).cross(cr).add(cr.clone().cross(ab).multiplyScalar(ac2)).divideScalar(d).add(a);
  return { c: o, r: o.distanceTo(a), n };
}

/** Two-point arc: chord endpoints, then the bulge. */
export class TwoPointArcTool implements Tool {
  id = 'arc2'; label = '2 Point Arc'; hint = 'Click both ends of the chord, then pull the bulge. Type a bulge distance.'; cursor = 'crosshair';
  private pts: THREE.Vector3[] = []; private plane: Plane = GROUND; private bulge = 0;
  busy() { return this.pts.length > 0; }
  activate(ctx: ToolContext) { this.pts = []; ctx.setHint(this.hint); ctx.setVCB('', 'Bulge'); }
  deactivate(ctx: ToolContext) { this.pts = []; ctx.scene.clearOverlay(); }
  private bulgeAt(p: THREE.Vector3) { const [a, b] = this.pts; const chord = b.clone().sub(a); const perp = this.plane.normal.clone().cross(chord).normalize(); return p.clone().sub(a.clone().lerp(b, 0.5)).dot(perp); }
  private preview(ctx: ToolContext) {
    const [a, b] = this.pts; const chord = b.clone().sub(a); const L = chord.length(); const h = this.bulge; if (Math.abs(h) < 1e-4) { ctx.scene.addLine([a, b], INK, true); return; }
    const r = (L * L) / (8 * Math.abs(h)) + Math.abs(h) / 2; const perp = this.plane.normal.clone().cross(chord).normalize(); const mid = a.clone().lerp(b, 0.5); const c = mid.clone().addScaledVector(perp, Math.sign(h) * (Math.abs(h) - r));
    const n = h > 0 ? this.plane.normal.clone() : this.plane.normal.clone().negate(); const sweep = sweepTo(c, n, a, b); ctx.scene.addLine(polyArc(c, n, a, sweep, segsFor(sweep)), INK, false, 2); ctx.scene.addLine([a, b], 0x888888, true); ctx.scene.addLabel(mid.clone().addScaledVector(perp, h), `bulge ${fmtM(Math.abs(h))}`);
  }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = ctx.snap(ev, { plane: this.pts.length ? this.plane : GROUND, faces: !this.pts.length, from: this.pts.length === 1 ? this.pts[0] : undefined, lock: ctx.lock }); ctx.scene.clearOverlay(); drawSnap(ctx, s);
    if (this.pts.length === 1) { ctx.scene.addLine([this.pts[0], s.point], lineColor(this.pts[0], s.point, s), true); ctx.setVCB(this.pts[0].distanceTo(s.point).toFixed(3), 'Length'); }
    if (this.pts.length === 2) { this.bulge = this.bulgeAt(s.point); this.preview(ctx); ctx.setVCB(this.bulge.toFixed(3), 'Bulge'); }
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return; const s = ctx.snap(ev, { plane: this.pts.length ? this.plane : GROUND, faces: !this.pts.length, from: this.pts.length === 1 ? this.pts[0] : undefined, lock: ctx.lock });
    if (!this.pts.length) this.plane = planeFromSnap(s, GROUND);
    if (this.pts.length < 2) { this.pts.push(s.point.clone()); ctx.setHint(this.pts.length === 1 ? 'Click the other end of the chord.' : 'Pull the bulge and click, or type it.'); return; }
    this.finish(ctx, this.bulgeAt(s.point));
  }
  private async finish(ctx: ToolContext, bulge: number) {
    const [a, b] = this.pts; if (!b || Math.abs(bulge) < 1e-4) return; this.pts = []; ctx.scene.clearOverlay(); ctx.setHint(this.hint);
    await ctx.ops([{ op: 'draw_arc', from: arr(a), to: arr(b), bulge: +bulge.toFixed(5), normal: arr(this.plane.normal), layer: ctx.options.layer }], '2 point arc');
  }
  onVCB(v: string, ctx: ToolContext) { const n = parseNumbers(v); if (n.length && this.pts.length === 2) this.finish(ctx, Math.sign(this.bulge || 1) * n[0]); }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.pts.length) { this.pts = []; ctx.scene.clearOverlay(); return true; } return false; }
}

/** Rotated rectangle: first edge (two clicks), then the width. */
export class RotatedRectangleTool implements Tool {
  id = 'rotrect'; label = 'Rotated Rectangle'; hint = 'Click the two ends of the first edge, then set the width.'; cursor = 'crosshair';
  private pts: THREE.Vector3[] = []; private plane: Plane = GROUND; private last: THREE.Vector3 | null = null;
  busy() { return this.pts.length > 0; }
  activate(ctx: ToolContext) { this.pts = []; ctx.setHint(this.hint); ctx.setVCB('', 'Length'); }
  deactivate(ctx: ToolContext) { this.pts = []; ctx.scene.clearOverlay(); }
  private corners(p: THREE.Vector3) { const [a, b] = this.pts; const u = b.clone().sub(a).normalize(); const w = this.plane.normal.clone().cross(u).normalize(); const h = p.clone().sub(b).dot(w); return { pts: [a, b, b.clone().addScaledVector(w, h), a.clone().addScaledVector(w, h)], u, h }; }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = ctx.snap(ev, { plane: this.pts.length ? this.plane : GROUND, faces: !this.pts.length, from: this.pts.length === 1 ? this.pts[0] : undefined, lock: ctx.lock }); ctx.scene.clearOverlay(); drawSnap(ctx, s); this.last = s.point;
    if (this.pts.length === 1) { ctx.scene.addLine([this.pts[0], s.point], lineColor(this.pts[0], s.point, s), false, 2); ctx.scene.addLabel(this.pts[0].clone().lerp(s.point, 0.5), fmtM(this.pts[0].distanceTo(s.point))); ctx.setVCB(this.pts[0].distanceTo(s.point).toFixed(3), 'Length'); }
    if (this.pts.length === 2) { const { pts, h } = this.corners(s.point); ctx.scene.addLine([...pts, pts[0]], INK, false, 2); ctx.scene.addFace(pts); ctx.scene.addLabel(pts[1].clone().lerp(pts[2], 0.5), fmtM(Math.abs(h))); ctx.setVCB(Math.abs(h).toFixed(3), 'Width'); }
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return; const s = ctx.snap(ev, { plane: this.pts.length ? this.plane : GROUND, faces: !this.pts.length, from: this.pts.length === 1 ? this.pts[0] : undefined, lock: ctx.lock });
    if (!this.pts.length) this.plane = planeFromSnap(s, GROUND);
    if (this.pts.length < 2) { if (this.pts.length === 1 && s.point.distanceTo(this.pts[0]) < 0.005) return; this.pts.push(s.point.clone()); ctx.setHint(this.pts.length === 1 ? 'Click the other end of the first edge.' : 'Click to set the width, or type it.'); return; }
    this.finish(ctx, this.corners(s.point).h);
  }
  private async finish(ctx: ToolContext, h: number) {
    const [a, b] = this.pts; if (!b || Math.abs(h) < 0.005) return; const u = b.clone().sub(a).normalize(); const w = this.plane.normal.clone().cross(u).normalize(); const opposite = b.clone().addScaledVector(w, h);
    this.pts = []; ctx.scene.clearOverlay(); ctx.setHint(this.hint);
    const r = await ctx.ops([{ op: 'draw_rectangle', corner: arr(a), opposite: arr(opposite), normal: arr(this.plane.normal), u: arr(u), layer: ctx.options.layer }], 'Rotated rectangle'); if (r[0]?.created?.[0]) ctx.select([r[0].created[0]]);
  }
  onVCB(v: string, ctx: ToolContext) {
    const n = parseNumbers(v); if (!n.length) return;
    if (this.pts.length === 1 && this.last) { const d = this.last.clone().sub(this.pts[0]).normalize(); this.pts.push(this.pts[0].clone().addScaledVector(d, n[0])); ctx.setHint('Click to set the width, or type it.'); }
    else if (this.pts.length === 2) { const sign = this.last ? Math.sign(this.corners(this.last).h || 1) : 1; this.finish(ctx, sign * n[0]); }
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.pts.length) { this.pts = []; ctx.scene.clearOverlay(); return true; } return false; }
}

/** Protractor: angled guide lines. Centre, reference direction, angle. */
export class ProtractorTool implements Tool {
  id = 'protractor'; label = 'Protractor'; hint = 'Click the vertex, a reference direction, then the angle: a guide line is placed. Type degrees.'; cursor = 'crosshair';
  private c: THREE.Vector3 | null = null; private ref: THREE.Vector3 | null = null; private plane: Plane = GROUND; private angle = 0;
  busy() { return !!this.c; }
  activate(ctx: ToolContext) { this.c = this.ref = null; ctx.setHint(this.hint); ctx.setVCB('', 'Angle'); }
  deactivate(ctx: ToolContext) { this.c = this.ref = null; ctx.scene.clearOverlay(); }
  private dirAt(p: THREE.Vector3) { const a = this.ref!.clone().sub(this.c!); const n = this.plane.normal; a.sub(n.clone().multiplyScalar(a.dot(n))).normalize(); const b = p.clone().sub(this.c!); b.sub(n.clone().multiplyScalar(b.dot(n))); let ang = Math.atan2(a.clone().cross(b).dot(n), a.dot(b)); const snapped = Math.round(ang / (Math.PI / 12)) * (Math.PI / 12); if (Math.abs(ang - snapped) < 0.02) ang = snapped; return { ang, dir: a.clone().applyAxisAngle(n, ang) }; }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const s = ctx.snap(ev, { plane: this.c ? this.plane : GROUND, faces: !this.c, from: this.c ?? undefined, lock: ctx.lock }); ctx.scene.clearOverlay(); drawSnap(ctx, s);
    if (!this.c) return;
    const { u, w } = planeBasis(this.plane.normal); const r = ctx.scene.pixelSize(this.c) * 60; const ring: THREE.Vector3[] = []; for (let i = 0; i <= 48; i++) { const a = (i / 48) * Math.PI * 2; ring.push(this.c.clone().addScaledVector(u, Math.cos(a) * r).addScaledVector(w, Math.sin(a) * r)); } ctx.scene.addLine(ring, 0x888888);
    if (!this.ref) { ctx.scene.addLine([this.c, s.point], 0x888888, true); return; }
    const { ang, dir } = this.dirAt(s.point); this.angle = ang; ctx.scene.addLine([this.c, this.ref], 0x888888, true); ctx.scene.addLine([this.c.clone().addScaledVector(dir, -200), this.c.clone().addScaledVector(dir, 200)], MAGENTA, true); ctx.scene.addLabel(s.point, fmtDeg((ang * 180) / Math.PI)); ctx.setVCB(((ang * 180) / Math.PI).toFixed(1), 'Angle');
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return; const s = ctx.snap(ev, { plane: this.c ? this.plane : GROUND, faces: !this.c, from: this.c ?? undefined, lock: ctx.lock });
    if (!this.c) { this.c = s.point.clone(); this.plane = planeFromSnap(s, GROUND); if (ctx.lock) this.plane = { origin: this.c, normal: new THREE.Vector3(ctx.lock === 'x' ? 1 : 0, ctx.lock === 'y' ? 1 : 0, ctx.lock === 'z' ? 1 : 0) }; ctx.setHint('Click the reference direction.'); return; }
    if (!this.ref) { this.ref = s.point.clone(); ctx.setHint('Click the angle, or type it.'); return; }
    this.finish(ctx, this.dirAt(s.point).dir);
  }
  private async finish(ctx: ToolContext, dir: THREE.Vector3) { const c = this.c!; this.c = this.ref = null; ctx.scene.clearOverlay(); ctx.setHint(this.hint); await ctx.ops([{ op: 'add_guide', point: arr(c), direction: arr(dir) }], 'Guide'); }
  onVCB(v: string, ctx: ToolContext) { const n = parseNumbers(v); if (!n.length || !this.c || !this.ref) return; const a = this.ref.clone().sub(this.c).normalize(); this.finish(ctx, a.applyAxisAngle(this.plane.normal, (n[0] * Math.PI) / 180)); }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.c) { this.c = this.ref = null; ctx.scene.clearOverlay(); return true; } return false; }
}

/** Zoom Window: drag a rectangle to frame what is inside it. */
export class ZoomWindowTool implements Tool {
  id = 'zoomwin'; label = 'Zoom Window'; hint = 'Drag a rectangle around what you want to fill the view.'; cursor = 'zoom-in';
  private start: { x: number; y: number } | null = null; private box: HTMLDivElement | null = null;
  activate(ctx: ToolContext) { ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  deactivate() { this.box?.remove(); this.box = null; this.start = null; }
  onDown(ev: PointerEvent) { if (ev.button === 0) this.start = { x: ev.clientX, y: ev.clientY }; }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    if (!this.start) return; if (!this.box) { this.box = document.createElement('div'); this.box.className = 'select-box'; ctx.scene.container.appendChild(this.box); }
    const r = ctx.scene.container.getBoundingClientRect(); Object.assign(this.box.style, { left: `${Math.min(ev.clientX, this.start.x) - r.left}px`, top: `${Math.min(ev.clientY, this.start.y) - r.top}px`, width: `${Math.abs(ev.clientX - this.start.x)}px`, height: `${Math.abs(ev.clientY - this.start.y)}px` });
  }
  onUp(ev: PointerEvent, ctx: ToolContext) {
    if (!this.start) return; const s = this.start; this.start = null; this.box?.remove(); this.box = null;
    const r = ctx.scene.container.getBoundingClientRect(); const x0 = Math.min(s.x, ev.clientX) - r.left, x1 = Math.max(s.x, ev.clientX) - r.left, y0 = Math.min(s.y, ev.clientY) - r.top, y1 = Math.max(s.y, ev.clientY) - r.top;
    if (x1 - x0 < 6 || y1 - y0 < 6) return;
    zoomWindow(ctx, x0 / ctx.scene.width, y0 / ctx.scene.height, x1 / ctx.scene.width, y1 / ctx.scene.height);
  }
}
/** Frame the model vertices whose projection falls inside the fractional viewport box; falls back to the ground patch under it. */
export function zoomWindow(ctx: ToolContext, fx0: number, fy0: number, fx1: number, fy1: number) {
  const sc = ctx.scene; const W = sc.width, H = sc.height; const b = new THREE.Box3();
  for (const v of sc.vertices) { const p = sc.screenPoint(v); if (p.x >= fx0 * W && p.x <= fx1 * W && p.y >= fy0 * H && p.y <= fy1 * H) b.expandByPoint(v); }
  if (b.isEmpty()) { const r = sc.container.getBoundingClientRect(); for (const [fx, fy] of [[fx0, fy0], [fx1, fy0], [fx0, fy1], [fx1, fy1]]) { const g = sc.groundPoint({ clientX: r.left + fx * W, clientY: r.top + fy * H }); if (g) b.expandByPoint(g); } }
  if (b.isEmpty()) return;
  sc.cam.frame({ min: [b.min.x, b.min.y, b.min.z], max: [b.max.x, b.max.y, b.max.z] });
}
