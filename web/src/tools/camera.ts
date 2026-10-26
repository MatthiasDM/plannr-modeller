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
/** Section Plane: click a face to cut along it; click the ground for a horizontal cut (arrow keys pick the axis). */
export class SectionTool implements Tool {
  id = 'section'; label = 'Section Plane'; hint = 'Click a face to place a section plane on it, or the ground for a horizontal cut (↑ blue / → red / ← green lock the direction). Type a height.'; cursor = 'crosshair';
  activate(ctx: ToolContext) { ctx.setHint(this.hint); ctx.setVCB('', 'Height'); }
  deactivate(ctx: ToolContext) { ctx.scene.clearOverlay(); }
  onMove(ev: PointerEvent, ctx: ToolContext) {
    const p = ctx.scene.pick(ev, { facesOnly: true }); ctx.scene.highlight(p ? new Set([p.id]) : new Set(), true);
  }
  async onDown(ev: PointerEvent, ctx: ToolContext) {
    if (ev.button !== 0) return;
    const p = ctx.scene.pick(ev, { facesOnly: true });
    const normal = ctx.lock ? AXIS[ctx.lock].clone() : p ? p.normal.clone() : AXIS.y.clone();
    const origin = p ? p.point.clone() : (ctx.scene.groundPoint(ev)?.setY(1.2) ?? AXIS.y.clone().multiplyScalar(1.2));
    await ctx.ops([{ op: 'add_section', origin: arr(origin), normal: arr(normal), active: true }], 'Section plane');
  }
  async onVCB(v: string, ctx: ToolContext) {
    const n = parseNumbers(v); if (!n.length) return;
    const axis = ctx.lock ?? 'y'; const o = [0, 0, 0] as [number, number, number]; o[{ x: 0, y: 1, z: 2 }[axis]] = n[0];
    await ctx.ops([{ op: 'add_section', origin: o, normal: arr(AXIS[axis]), active: true }], 'Section plane');
  }
}
