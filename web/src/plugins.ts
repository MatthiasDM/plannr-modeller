// Plugin API: window.plannr.registerPlugin({...}). Plugins add tools, tray panels and ops of their own;
// the core knows nothing about what they model. See docs/PLUGINS.md.
import type { Tool, ToolContext } from './tools/base';
import type { Document, Entity, Op, OpResult, Vec3 } from './model';

export interface PluginPanel { id: string; title: string; render(ctx: ToolContext, el: HTMLElement): void }
/** A plugin can take over camera input (walk-through, fly, turntable…). Return true from a handler to consume the event. */
export interface CameraController {
  onPointerDown?(ev: PointerEvent): boolean; onPointerMove?(ev: PointerEvent): boolean; onPointerUp?(ev: PointerEvent): boolean;
  onWheel?(ev: WheelEvent): boolean;
  /** Key down/up on the window (not in inputs). Return true to consume (tool shortcuts are then skipped). */
  onKey?(ev: KeyboardEvent, down: boolean): boolean;
  /** Called when the app takes the camera back (standard view, zoom extents, another controller). */
  release(): void;
}
export interface Plugin {
  id: string; name: string; version?: string;
  /** Accent colour (CSS) that marks this plugin's toolbar group and panels. */
  accent?: string;
  tools?: Tool[];
  /** Toolbar groups of tool ids (defaults to one group with all plugin tools). */
  toolbar?: string[][];
  panels?: PluginPanel[];
  /** Rewrite a Move of these ids (e.g. turn a vertical move of a whole house into a ground-level change). Return null to keep the default `move` op. */
  beforeMove?(ids: string[], delta: Vec3, ctx: ToolContext): Op[] | null;
  /** Render Entity Info for one of this plugin's custom entities; return false to fall back to the default. */
  renderEntity?(ctx: ToolContext, entity: Entity, el: HTMLElement): boolean;
  /** Called once the app is ready. */
  activate?(api: PluginHost): void;
}
export interface PluginHost {
  ctx: ToolContext;
  document(): Document;
  ops(ops: Op[], label: string): Promise<OpResult[]>;
  select(ids: string[]): void;
  selection(): Set<string>;
  setTool(id: string): void;
  toast(text: string): void;
  prompt(label: string, value?: string): Promise<string | null>;
  confirm(text: string, okLabel?: string, danger?: boolean): Promise<boolean>;
  setHint(text: string): void;
  /** Re-render the tray and toolbar (after plugin state changed). */
  refresh(): void;
  /** Add an element over the viewport (floating bars, badges). */
  mount(el: HTMLElement): void;
  /** Feed the active tool a synthetic pointer event at viewport pixel coordinates (e.g. a crosshair "use" while the pointer is locked). */
  simulatePointer(kind: 'down' | 'move' | 'up', clientX: number, clientY: number, button?: number): void;
  canvas(): HTMLCanvasElement;
  /** Take over (or release, with null) camera input. The previous controller is released. */
  setCameraController(c: CameraController | null): void;
  cameraController(): CameraController | null;
  on(event: 'changed' | 'selection' | 'tool' | 'xray', handler: (detail: any) => void): () => void;
}

const plugins: Plugin[] = [];
const listeners = new Set<(p: Plugin) => void>();
export function registerPlugin(p: Plugin) {
  if (plugins.some(x => x.id === p.id)) throw new Error(`plugin ${p.id} already registered`);
  plugins.push(p); listeners.forEach(l => l(p));
}
export const getPlugins = () => plugins;
export const onPlugin = (l: (p: Plugin) => void) => { listeners.add(l); return () => listeners.delete(l); };
