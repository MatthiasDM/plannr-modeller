import * as THREE from 'three';
import type { Tool, ToolContext } from './base';
import { arr, AXIS } from './base';
import { parseNumbers } from './base';

export class OrbitTool implements Tool {
  id = 'orbit'; label = 'Orbit'; hint = 'Drag to orbit. (Middle mouse orbits in any tool; Shift+middle pans.)'; cursor = 'grab'; shortcut = 'o';
  private down: [number, number] | null = null;
  activate(ctx: ToolContext) { ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  onDown(ev: PointerEvent) { if (ev.button === 0) this.down = [ev.clientX, ev.clientY]; }
  onMove(ev: PointerEvent, ctx: ToolContext) { if (!this.down) return; ctx.scene.cam.orbit(ev.clientX - this.down[0], ev.clientY - this.down[1]); this.down = [ev.clientX, ev.clientY]; }
  onUp() { this.down = null; }
}
export class PanTool implements Tool {
  id = 'pan'; label = 'Pan'; hint = 'Drag to pan the view.'; cursor = 'grab'; shortcut = 'h';
  private down: [number, number] | null = null;
  activate(ctx: ToolContext) { ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  onDown(ev: PointerEvent) { if (ev.button === 0) this.down = [ev.clientX, ev.clientY]; }
  onMove(ev: PointerEvent, ctx: ToolContext) { if (!this.down) return; ctx.scene.cam.pan(ev.clientX - this.down[0], ev.clientY - this.down[1], ctx.scene.height); this.down = [ev.clientX, ev.clientY]; }
  onUp() { this.down = null; }
}
export class ZoomTool implements Tool {
  id = 'zoom'; label = 'Zoom'; hint = 'Drag up to zoom in, down to zoom out. The wheel zooms in any tool.'; cursor = 'zoom-in'; shortcut = 'z';
  private down: number | null = null;
  activate(ctx: ToolContext) { ctx.setHint(this.hint); ctx.setVCB('', 'Measurements'); }
  onDown(ev: PointerEvent) { if (ev.button === 0) this.down = ev.clientY; }
  onMove(ev: PointerEvent, ctx: ToolContext) { if (this.down === null) return; ctx.scene.cam.dolly(Math.pow(1.01, ev.clientY - this.down)); this.down = ev.clientY; }
  onUp() { this.down = null; }
}
/** Section Plane: click a face to cut along it; click the ground for a horizontal cut (arrow keys pick the
 *  axis). The plane is previewed while hovering, with arrows on the side that is cut away: by default the
 *  side facing the camera; Tab flips it. */
export class SectionTool implements Tool {
  id = 'section'; label = 'Section Plane'; hint = 'Click a face to place a section plane on it, or the ground for a horizontal cut (↑ blue / → red / ← green lock the direction). The arrows show the side that is cut away: Tab flips it. Type a height.'; cursor = 'crosshair';
  private flip = false; private last: PointerEvent | null = null;
  activate(ctx: ToolContext) { ctx.setHint(this.hint); ctx.setVCB('', 'Height'); this.flip = false; this.last = null; }
  deactivate(ctx: ToolContext) { ctx.scene.clearOverlay(); ctx.scene.highlight(new Set(), true); }
  /** Where the plane would go: on the hovered face (else the ground at 1.2 m), along the locked axis if
   *  any; the side facing the camera is cut away unless flipped. */
  private place(ev: { clientX: number; clientY: number }, ctx: ToolContext) {
    const p = ctx.scene.pick(ev, { facesOnly: true });
    const normal = ctx.lock ? AXIS[ctx.lock].clone() : p ? p.normal.clone() : AXIS.y.clone();
    const origin = p ? p.point.clone() : (ctx.scene.groundPoint(ev)?.setY(1.2) ?? AXIS.y.clone().multiplyScalar(1.2));
    return { p, origin, normal: this.orient(normal, origin, ctx) };
  }
  private orient(normal: THREE.Vector3, origin: THREE.Vector3, ctx: ToolContext) {
    if (normal.dot(ctx.scene.cam.camera.position.clone().sub(origin)) < 0) normal.negate();
    if (this.flip) normal.negate();
    return normal;
  }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    this.last = ev;
    const { p, origin, normal } = this.place(ev, ctx);
    ctx.scene.highlight(p ? new Set([p.id]) : new Set(), true);
    ctx.scene.clearOverlay(); ctx.scene.addSectionPreview(origin, normal);
    ctx.scene.addLabel(origin, this.flip ? 'Cuts away the far side · Tab flips' : 'Cuts away the side facing you · Tab flips', 'infer');
  }
  onKey(ev: KeyboardEvent, ctx: ToolContext) {
    if (ev.key !== 'Tab') return false;
    this.flip = !this.flip; if (this.last) this.onMove(this.last, ctx); return true;
  }
  onLock(ctx: ToolContext) { if (this.last) this.onMove(this.last, ctx); }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const { origin, normal } = this.place(ev, ctx);
    ctx.scene.clearOverlay();
    await ctx.ops([{ op: 'add_section', origin: arr(origin), normal: arr(normal), active: true }], 'Section plane');
  }
  async onVCB(v: string, ctx: ToolContext) {
    const n = parseNumbers(v); if (!n.length) return;
    const axis = ctx.lock ?? 'y'; const o = AXIS[axis].clone().multiplyScalar(n[0]);
    await ctx.ops([{ op: 'add_section', origin: arr(o), normal: arr(this.orient(AXIS[axis].clone(), o, ctx)), active: true }], 'Section plane');
  }
}
