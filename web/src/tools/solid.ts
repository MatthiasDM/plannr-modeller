import * as THREE from 'three';
import type { Tool, ToolContext, Plane, Snap } from './base';
import { parseNumbers, drawSnap, planeFromSnap, GROUND, arr, INK, MAGENTA } from './base';
import { planeBasis } from './draw';
import { fmtM, type Entity } from '../model';

/** Solid Box / Cylinder: a base on the ground or on a face, then a height along its normal. Each makes a closed
 *  solid group (engine ops `extrude` / `solid_cylinder`), ready for the boolean tools and 3D printing. */
abstract class SolidTool implements Tool {
  abstract id: string; abstract label: string; abstract hint: string; cursor = 'crosshair'; planeLock = false;
  protected a: THREE.Vector3 | null = null; protected b: THREE.Vector3 | null = null; protected plane: Plane = GROUND;
  protected h = 0; protected last: THREE.Vector3 | null = null; private shown = '';
  busy() { return !!this.a; }
  activate(ctx: ToolContext) { this.reset(ctx); }
  deactivate(ctx: ToolContext) { this.reset(ctx); }
  protected reset(ctx: ToolContext) { this.a = this.b = null; this.h = 0; this.shown = ''; ctx.preview([]); ctx.scene.clearOverlay(); ctx.setHint(this.hint); ctx.setVCB('', this.vcbLabel(0)); }
  protected abstract vcbLabel(stage: number): string;
  /** The op for base (a, b) and height h, or null when too small. */
  protected abstract op(ctx: ToolContext, a: THREE.Vector3, b: THREE.Vector3, h: number): any | null;
  protected abstract drawBase(ctx: ToolContext, a: THREE.Vector3, b: THREE.Vector3): void;
  protected onPlane(p: THREE.Vector3) { const n = this.plane.normal; return p.clone().sub(n.clone().multiplyScalar(p.clone().sub(this.plane.origin).dot(n))); }
  private height(ev: PointerEvent, ctx: ToolContext) {
    const n = this.plane.normal; let h = ctx.scene.linePoint(ev, this.b!, n).clone().sub(this.b!).dot(n);
    if (ctx.options.gridSnap) h = Math.round(h * 100) / 100;
    return h;
  }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    ctx.scene.clearOverlay();
    if (!this.a) { const s = ctx.snap(ev, { faces: true }); drawSnap(ctx, s); return; }
    if (!this.b) {
      const s: Snap = ctx.snap(ev, { plane: this.plane, inPlane: true, from: this.a }); drawSnap(ctx, s);
      this.last = this.onPlane(s.point); this.drawBase(ctx, this.a, this.last); return;
    }
    this.h = this.height(ev, ctx);
    const tip = this.b.clone().addScaledVector(this.plane.normal, this.h);
    ctx.scene.addLine([this.b, tip], MAGENTA, true); ctx.scene.addLabel(tip, fmtM(Math.abs(this.h))); ctx.setVCB(Math.abs(this.h).toFixed(3), this.vcbLabel(2));
    const op = Math.abs(this.h) > 1e-4 ? this.op(ctx, this.a, this.b, this.h) : null;
    const k = JSON.stringify(op); if (k !== this.shown) { this.shown = k; ctx.preview(op ? [op] : []); }
    if (!op) this.drawBase(ctx, this.a, this.b);
  }
  onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    if (!this.a) {
      const s = ctx.snap(ev, { faces: true }); this.plane = planeFromSnap(s, GROUND);
      // a face picked from behind: build out of its visible side
      const fn = s.kind === 'face' ? ctx.faceNormal(ev, s.point) : null; if (fn) this.plane = { origin: s.point.clone(), normal: fn.clone() };
      this.a = this.onPlane(s.point); ctx.setHint(this.stageHint(1)); ctx.setVCB('', this.vcbLabel(1)); return;
    }
    if (!this.b) { const s = ctx.snap(ev, { plane: this.plane, inPlane: true, from: this.a }); this.setBase(ctx, this.onPlane(s.point)); return; }
    this.commit(ctx, this.height(ev, ctx));
  }
  protected abstract stageHint(stage: number): string;
  protected setBase(ctx: ToolContext, b: THREE.Vector3) {
    if (!this.a || this.op(ctx, this.a, b, 0.01) === null) { ctx.toast('Too small'); return; }
    this.b = b; ctx.setHint(this.stageHint(2)); ctx.setVCB('', this.vcbLabel(2));
  }
  protected async commit(ctx: ToolContext, h: number) {
    if (!this.a || !this.b || Math.abs(h) < 1e-4) return;
    const op = this.op(ctx, this.a, this.b, h); if (!op) return;
    ctx.preview([]); this.shown = '';
    try { const r = await ctx.ops([op], this.label); const id = r[0]?.created?.[0]; if (id) ctx.select([id]); }
    catch (e: any) { ctx.toast(e.message ?? String(e)); }
    this.reset(ctx);
  }
  onVCB(v: string, ctx: ToolContext) {
    const n = parseNumbers(v); if (!n.length || !this.a) return;
    if (!this.b) { const b = this.typedBase(n); if (b) this.setBase(ctx, b); return; }
    this.commit(ctx, (this.h < 0 ? -1 : 1) * n[0]);
  }
  protected abstract typedBase(n: number[]): THREE.Vector3 | null;
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.a) { this.reset(ctx); return true; } return false; }
}

export class BoxTool extends SolidTool {
  id = 'box'; label = 'Box'; hint = 'Box: click the first corner of the base (on the ground or on a face).';
  protected vcbLabel(stage: number) { return stage < 2 ? 'Width; depth' : 'Height'; }
  protected stageHint(stage: number) { return stage === 1 ? 'Click the opposite corner of the base, or type width; depth and press Enter.' : 'Move up and click to set the height, or type it and press Enter.'; }
  private corners(a: THREE.Vector3, b: THREE.Vector3) {
    const { u, w } = planeBasis(this.plane.normal); const d = b.clone().sub(a); const du = d.dot(u), dw = d.dot(w);
    return { pts: [a.clone(), a.clone().addScaledVector(u, du), a.clone().addScaledVector(u, du).addScaledVector(w, dw), a.clone().addScaledVector(w, dw)], du, dw };
  }
  protected drawBase(ctx: ToolContext, a: THREE.Vector3, b: THREE.Vector3) {
    const { pts, du, dw } = this.corners(a, b); ctx.scene.addLine([...pts, pts[0]], INK, false, 2); ctx.scene.addFace(pts);
    if (Math.abs(du) > 0.002) ctx.scene.addLabel(pts[0].clone().lerp(pts[1], 0.5), fmtM(Math.abs(du)));
    if (Math.abs(dw) > 0.002) ctx.scene.addLabel(pts[1].clone().lerp(pts[2], 0.5), fmtM(Math.abs(dw)));
    ctx.setVCB(`${Math.abs(du).toFixed(3)}; ${Math.abs(dw).toFixed(3)}`, this.vcbLabel(1));
  }
  protected op(ctx: ToolContext, a: THREE.Vector3, b: THREE.Vector3, h: number) {
    const { pts, du, dw } = this.corners(a, b); if (Math.abs(du) < 5e-4 || Math.abs(dw) < 5e-4) return null;
    return { op: 'extrude', profile: pts.map(arr), direction: arr(this.plane.normal.clone().multiplyScalar(h)), name: 'Box', layer: ctx.options.layer };
  }
  protected typedBase(n: number[]) {
    if (n.length < 2 || !this.a) return null; const { u, w } = planeBasis(this.plane.normal);
    const d = this.last ? this.last.clone().sub(this.a) : new THREE.Vector3(1, 0, 1);
    return this.a.clone().addScaledVector(u, (d.dot(u) < 0 ? -1 : 1) * n[0]).addScaledVector(w, (d.dot(w) < 0 ? -1 : 1) * n[1]);
  }
}

export class CylinderTool extends SolidTool {
  id = 'cylinder'; label = 'Cylinder'; hint = 'Cylinder: click the centre of the base (on the ground or on a face). Type "48s" for the number of sides.';
  protected vcbLabel(stage: number) { return stage === 1 ? 'Radius' : stage === 2 ? 'Height' : 'Sides'; }
  protected stageHint(stage: number) { return stage === 1 ? 'Click to set the radius, or type it and press Enter.' : 'Move up and click to set the height, or type it and press Enter.'; }
  private sides(ctx: ToolContext) { return Math.max(ctx.options.circleSegments, 32); }
  protected drawBase(ctx: ToolContext, a: THREE.Vector3, b: THREE.Vector3) {
    const r = a.distanceTo(b); const { u, w } = planeBasis(this.plane.normal); const n = this.sides(ctx);
    const pts = Array.from({ length: n }, (_, i) => { const t = (i / n) * Math.PI * 2; return a.clone().addScaledVector(u, Math.cos(t) * r).addScaledVector(w, Math.sin(t) * r); });
    ctx.scene.addLine([...pts, pts[0]], INK, false, 2); ctx.scene.addFace(pts); ctx.scene.addLine([a, b], 0x888888, true);
    ctx.scene.addLabel(a.clone().lerp(b, 0.5), `r ${fmtM(r)}`); ctx.setVCB(r.toFixed(3), this.vcbLabel(1));
  }
  protected op(ctx: ToolContext, a: THREE.Vector3, b: THREE.Vector3, h: number) {
    const r = a.distanceTo(b); if (r < 5e-4) return null;
    return { op: 'solid_cylinder', center: arr(a), axis: arr(this.plane.normal), radius: +r.toFixed(6), height: +h.toFixed(6), segments: this.sides(ctx), name: 'Cylinder', layer: ctx.options.layer };
  }
  protected typedBase(n: number[]) { if (!this.a) return null; const { u } = planeBasis(this.plane.normal); return this.a.clone().addScaledVector(u, n[0]); }
  onVCB(v: string, ctx: ToolContext) {
    const m = v.match(/^(\d+)\s*s$/i); if (m) { ctx.options.circleSegments = Math.max(3, Math.min(256, parseInt(m[1]))); ctx.toast(`${ctx.options.circleSegments} sides`); return; }
    super.onVCB(v, ctx);
  }
}

/** Solid tools (SketchUp's Union / Subtract / Intersect / Trim / Split): click the solid to keep, then the one to combine
 *  with it. Trim cuts like Subtract but leaves the cutter; Split leaves both, cut along each other, with the shared part between. The first stays picked, so a subtract can cut one solid after another. */
const VERB = { union: ['Union', 'join onto it'], subtract: ['Subtract', 'cut out of it'], intersect: ['Intersect', 'intersect it with'], trim: ['Trim', 'trim it with (that one stays)'], split: ['Split', 'split it with (the part they share becomes a third solid)'] } as const;
export class BooleanTool implements Tool {
  id: string; label: string; hint: string; cursor = 'pointer';
  private target: string | null = null;
  constructor(private mode: keyof typeof VERB) {
    this.id = mode; this.label = VERB[mode][0];
    this.hint = `${VERB[mode][0]}: click the solid to keep (a group or a box, cylinder or extrusion), then the solid to ${VERB[mode][1]}.`;
  }
  busy() { return !!this.target; }
  // always start by clicking the solid to keep: taking it from the selection made the last solid drawn the target
  activate(ctx: ToolContext) { this.target = null; ctx.select([]); ctx.setHint(this.hint); }
  deactivate(ctx: ToolContext) { this.target = null; ctx.scene.highlight(new Set(), true); }
  private next() { return `Now click the solid to ${VERB[this.mode as keyof typeof VERB][1]} (Esc picks another solid to keep).`; }
  private groupAt(ev: PointerEvent, ctx: ToolContext): string | null {
    // once the solid to keep is picked, see through it: a cutter inside it (flush with its faces) is then clickable
    const t = this.target;
    const p = ctx.pickTarget(ev, { facesOnly: true, anywhere: true, accept: t ? (id => ctx.resolve(id) !== t) : undefined }); if (!p) return null;
    const id = ctx.resolve(p.id); return isGroup(ctx, id) ? id : null;
  }
  onMove(ev: PointerEvent, ctx: ToolContext) { const id = this.groupAt(ev, ctx); ctx.scene.highlight(id && id !== this.target ? new Set([id]) : new Set(), true); }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const id = this.groupAt(ev, ctx);
    if (!id) { ctx.toast('Click a solid: a group whose faces close up (Box, Cylinder, or a group you made)'); return; }
    if (!this.target) { this.target = id; ctx.select([id]); ctx.setHint(this.next()); return; }
    if (id === this.target) return;
    try { await ctx.ops([this.mode === 'trim' ? { op: 'boolean', mode: 'subtract', target: this.target, tools: [id], keepTools: true } : { op: 'boolean', mode: this.mode, target: this.target, tools: [id] }], this.label); ctx.select([this.target]); ctx.setHint(this.next()); }
    catch (e: any) { ctx.toast(e.message ?? String(e)); }
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) { if (ev.key === 'Escape' && this.target) { this.target = null; ctx.select([]); ctx.setHint(this.hint); return true; } return false; }
}
function isGroup(ctx: ToolContext, id: string): boolean {
  const find = (list: Entity[]): Entity | undefined => { for (const e of list) { if (e.id === id) return e; if (e.kind === 'group') { const x = find((e as any).entities); if (x) return x; } } return undefined; };
  return find(ctx.doc().entities)?.kind === 'group';
}
