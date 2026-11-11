import './style.css';
import * as THREE from 'three';
import { Viewport, A, V, COLOR, type ViewName, FACE_STYLES, type FaceStyle, type Pick as ScenePick } from './scene';
import { api, Live, type Changes, type CompiledScene, type PluginInfo } from './api';
import { findEntity, topLevelId, walk, lengthUnit, setLengthUnit, UNITS, type Document, type Entity, type Op, type OpResult, type Scene, type Sun, type Vec3 } from './model';
import { toOBJ, toSTL, parseOBJ, parseSTL, meshGroup, download } from './io';
import { selectionBounds } from './tools/advanced';
import { BUILTIN_TOOLS, EMBED_TOOLS, placeComponentTool, pasteTool } from './tools';
import { zoomWindow } from './tools/arc';
import { connectedTo } from './tools/edit';
import type { Tool, ToolContext, Snap, SnapKind, SnapOptions, AxisLock, Plane } from './tools/base';
import { AXIS, AXIS_COLOR, GROUND, MAGENTA, snapColor, snapLabel, setDrawingAxes, toAxes, fromAxes } from './tools/base';
import { Tray } from './ui/tray';
import { renderMenubar, renderToolbar, renderHud, renderToolOptions, renderWelcome, layoutToolbar, showContextMenu } from './ui/chrome';
import { registerPlugin, getPlugins, partsFor, onPlugin, type PluginHost, type CameraController } from './plugins';
import { promptText, confirmDialog, installTooltips } from './ui/dialog';

/** The page itself never zooms: pinches and double-taps belong to the 3D view (which handles its own
 *  pinch). The viewport meta and `touch-action` cover most browsers; iOS Safari ignores both for pinch,
 *  so its gesture events and multi-finger moves outside the canvas are cancelled here. */
function lockPageZoom() {
  const stop = (e: Event) => e.preventDefault();
  for (const t of ['gesturestart', 'gesturechange', 'gestureend']) document.addEventListener(t, stop, { passive: false });
  document.addEventListener('touchmove', (e: TouchEvent) => { if (e.touches.length > 1 && !(e.target instanceof HTMLCanvasElement)) e.preventDefault(); }, { passive: false });
  // trackpad pinch on desktop arrives as ctrl+wheel: outside the canvas it would zoom the page
  window.addEventListener('wheel', (e: WheelEvent) => { if (e.ctrlKey && !(e.target instanceof HTMLCanvasElement)) e.preventDefault(); }, { passive: false });
}
lockPageZoom();

/** The labels of the Measurements box under which a bare number is a length (not an angle, a factor or a count). */
const LENGTH_LABELS = new Set(['Distance', 'Length', 'Radius', 'Bulge', 'Width', 'Height', 'Dimensions', 'Measurements']);
/** A measurement a tool reports (in metres) as the box shows it, in the document's unit. */
function shownLength(v: string, label: string): string {
  if (lengthUnit.factor === 1 || !LENGTH_LABELS.has(label)) return v;
  return v.split(';').map(s => /^\s*-?\d*\.?\d+\s*$/.test(s) ? (parseFloat(s) * lengthUnit.factor).toFixed(lengthUnit.digits) : s).join(';');
}
/** What was typed, with the document's unit put on every bare length (the tools read bare numbers as metres). */
function typedLength(v: string, label: string): string {
  if (lengthUnit.factor === 1 || !LENGTH_LABELS.has(label)) return v; const u = lengthUnit.name;
  const r = v.trim().match(/^(-?\d*\.?\d+)\s*r$/i); if (r) return `${r[1]}${u}r`; // a radius ("500r")
  const x = v.trim().match(/^(\d*\.?\d+)\s*[x×]\s*(\d*\.?\d+)$/); if (x) return `${x[1]}${u};${x[2]}${u}`; // "400x300"
  return v.replace(/(?<![\w.\/*:])(-?\d*\.?\d+)(?![\w.°:\/*])/g, `$1${u}`); // not "3x", "/3", "6s", "1:6", "250cm"
}
const emptyDoc = (): Document => ({ version: 0, units: 'm', layers: [{ id: 'layer0', name: 'Layer0', visible: true, color: '#7a8aa0' }], materials: [{ id: 'default', name: 'Default', color: '#F4F2EE', opacity: 1 }], entities: [], scenes: [], extensions: {} });

export type ModKey = 'Control' | 'Alt' | 'Shift';
export const MOD_LABEL: Record<ModKey, string> = { Control: navigator.platform.includes('Mac') ? 'Control (⌃)' : 'Ctrl', Alt: navigator.platform.includes('Mac') ? 'Option (⌥)' : 'Alt', Shift: 'Shift (⇧)' };
const DEFAULT_MODIFIER_TOOLS: Record<ModKey, string> = { Control: 'orbit', Alt: 'pan', Shift: '' };
function loadModifierTools(): Record<ModKey, string> {
  try { return { ...DEFAULT_MODIFIER_TOOLS, ...JSON.parse(localStorage.getItem('plannr.modifierTools') ?? '{}') }; } catch { return { ...DEFAULT_MODIFIER_TOOLS }; }
}
export type Theme = 'system' | 'light' | 'dark';
export const THEMES: Theme[] = ['system', 'light', 'dark'];
function readTheme(): Theme { try { const t = localStorage.getItem('plannr.theme'); return t === 'light' || t === 'dark' ? t : 'system'; } catch { return 'system'; } }
/** `data-theme` on <html> picks the CSS variables; without it the OS preference applies. */
export function applyTheme(t: Theme) { if (t === 'system') delete document.documentElement.dataset.theme; else document.documentElement.dataset.theme = t; }

export class App {
  scene: Viewport;
  doc: Document = emptyDoc();
  projectId: string | null = null; projectName = 'Untitled';
  selection = new Set<string>();
  enteredGroup: string | null = null;
  tools: Tool[] = [...BUILTIN_TOOLS];
  tool: Tool = this.tools[0];
  options: ToolContext['options'] = { circleSegments: 24, gridSnap: true, material: 'default', layer: 'layer0' };
  display = { grid: true, axes: true, edges: true, figure: true, shadows: true, sectionPlanes: (() => { try { return localStorage.getItem('plannr.sectionPlanes') === '1'; } catch { return false; } })() };
  /** Face style and back edges are kept in this browser, like the theme. Hidden geometry on show is a view of this session. */
  faceStyle: FaceStyle = (() => { try { const v = localStorage.getItem('plannr.faceStyle'); return FACE_STYLES.some(s => s.id === v) ? v as FaceStyle : 'shaded'; } catch { return 'shaded'; } })();
  backEdges = (() => { try { return localStorage.getItem('plannr.backEdges') === '1'; } catch { return false; } })();
  hiddenGeometry = false;
  /** The lens in degrees (SketchUp's field of view); 1 is the parallel projection. */
  fov: number = (() => { try { const v = +(localStorage.getItem('plannr.fov') ?? 35); return v >= 1 && v <= 120 ? v : 35; } catch { return 35; } })();
  private lens = 35;
  get parallel() { return this.fov <= 1; }
  setFov(fov: number) {
    this.fov = Math.max(1, Math.min(120, fov)); if (this.fov > 1) this.lens = this.fov; this.scene.cam.setFov(this.fov);
    try { localStorage.setItem('plannr.fov', String(this.fov)); } catch { /* storage blocked */ } this.refreshUI();
  }
  setProjection(parallel: boolean) { if (parallel !== this.parallel) this.setFov(parallel ? 1 : this.lens); }
  async fovPrompt() {
    const v = await this.promptText('Field of view in degrees (5 to 120; 35 is the usual lens)', String(this.parallel ? this.lens : this.fov)); if (v === null) return;
    const n = parseFloat(v); if (n >= 5 && n <= 120) this.setFov(n); else this.toast('The field of view is a number of degrees between 5 and 120.');
  }
  /** Where the sun stands for the shadows. */
  sun: Sun = (() => { const d: Sun = { byTime: false, hour: 14, month: 6, latitude: 51 }; try { return { ...d, ...JSON.parse(localStorage.getItem('plannr.sun') ?? '{}') }; } catch { return d; } })();
  setSun(patch: Partial<Sun>, render = true) {
    this.sun = { ...this.sun, ...patch }; try { localStorage.setItem('plannr.sun', JSON.stringify(this.sun)); } catch { /* storage blocked */ }
    this.applySun(); if (render) this.tray.render();
  }
  private applySun() {
    const s = this.sun; if (!s.byTime) { this.scene.setSun(new THREE.Vector3(20, 32, 14)); return; }
    const rad = Math.PI / 180, day = (s.month - 1) * 30.4 + 15; // the middle of the month
    const decl = 23.44 * rad * Math.sin((2 * Math.PI * (284 + day)) / 365), lat = s.latitude * rad, H = 15 * (s.hour - 12) * rad;
    const el = Math.asin(Math.sin(lat) * Math.sin(decl) + Math.cos(lat) * Math.cos(decl) * Math.cos(H));
    const az = Math.atan2(Math.sin(H), Math.cos(H) * Math.sin(lat) - Math.tan(decl) * Math.cos(lat)); // from south, towards west
    const low = Math.max(el, 2 * rad); // below the horizon: a low, dim light instead of none
    // north is −z (the Front view looks north), east +x
    this.scene.setSun(new THREE.Vector3(-Math.cos(low) * Math.sin(az), Math.sin(low), Math.cos(low) * Math.cos(az)), el > 0 ? Math.min(1, 0.35 + Math.sin(el)) : 0.15);
  }
  setFaceStyle(s: FaceStyle) { this.faceStyle = s; this.scene.setFaceStyle(s); this.scene.highlight(this.selection); try { localStorage.setItem('plannr.faceStyle', s); } catch { /* storage blocked */ } this.refreshUI(); renderHud(this, this.els['view-hud']); }
  setBackEdges(on: boolean) { this.backEdges = on; this.scene.setBackEdges(on); try { localStorage.setItem('plannr.backEdges', on ? '1' : '0'); } catch { /* storage blocked */ } this.refreshUI(); }
  async setHiddenGeometry(on: boolean) { this.hiddenGeometry = on; await this.refresh(); }
  /** What Edit → Unhide → Last brings back. */
  private lastHidden: string[] = [];
  get hiddenIds(): string[] { return [...walk(this.doc.entities)].filter(x => x.entity.hidden === true).map(x => x.entity.id); }
  async hideSelection() { const ids = [...this.selection]; if (!ids.length) return; await this.commitOps([{ op: 'set_hidden', ids, hidden: true }], 'Hide'); this.lastHidden = ids; this.select([]); }
  async unhideSelected() { const ids = [...this.selection].filter(id => findEntity(this.doc, id)?.hidden === true); if (ids.length) await this.commitOps([{ op: 'set_hidden', ids, hidden: false }], 'Unhide'); }
  async unhideLast() { const ids = this.lastHidden.filter(id => findEntity(this.doc, id)?.hidden === true); if (ids.length) { await this.commitOps([{ op: 'set_hidden', ids, hidden: false }], 'Unhide'); this.lastHidden = []; } }
  async unhideAll() { const ids = this.hiddenIds; if (ids.length) await this.commitOps([{ op: 'set_hidden', ids, hidden: false }], 'Unhide all'); }
  async toggleHidden(id: string) { const e = findEntity(this.doc, id); if (e) await this.commitOps([{ op: 'set_hidden', ids: [id], hidden: e.hidden !== true }], e.hidden === true ? 'Unhide' : 'Hide'); }
  /** Viewer mode (`?embed=1`): no menus or trays, the looking/measuring tools only, driven by the page that embeds it over postMessage. */
  readonly embed = new URLSearchParams(location.search).has('embed');
  /** Plugins whose tools and panels a viewer keeps (`?embed=1&plugins=plumbing,electrical`): the embedding app lets its users draw pipes or conduits around the house. */
  readonly embedPlugins = new Set((new URLSearchParams(location.search).get('plugins') ?? '').split(',').map(s => s.trim()).filter(Boolean));
  /** May this tool be used here? A viewer keeps the looking / measuring tools and the tools of the plugins it was asked for. */
  toolAllowed(id: string): boolean { return !this.embed || EMBED_TOOLS.has(id) || this.embedPlugins.has(getPlugins().find(p => p.tools?.some(t => t.id === id))?.id ?? ''); }
  /** Viewport commands added by plugins (see PluginHost.registerCommand). */
  private commands = new Map<string, (args: any) => any>();
  /** Light or dark chrome and viewport; 'system' follows the OS. Kept in localStorage (index.html applies it before the first paint). */
  theme: Theme = readTheme();
  setTheme(t: Theme) {
    this.theme = t; applyTheme(t);
    try { t === 'system' ? localStorage.removeItem('plannr.theme') : localStorage.setItem('plannr.theme', t); } catch { /* storage blocked */ }
    this.scene.applyTheme(); this.refreshUI();
  }
  lock: AxisLock = null;
  /** Hold a modifier to use a tool until it is released (Ctrl → Orbit by default). */
  modifierTools: Record<ModKey, string> = loadModifierTools();
  private temp: { key: ModKey; prev: Tool } | null = null;
  private pointerDown = false;
  saveState = ''; historyList: { version: number; label: string; at: string }[] = [];
  canUndo = false; canRedo = false;
  serverPlugins: PluginInfo[] = [];
  ctx: ToolContext; tray: Tray;
  private live: Live | null = null;
  private els: Record<string, HTMLElement> = {};
  private online = false; private previewSeq = 0; private compileSeq = 0;
  private listeners: Record<string, Set<(d: any) => void>> = { changed: new Set(), selection: new Set(), tool: new Set(), xray: new Set() };
  private hover: string | null = null;
  private camCtl: CameraController | null = null;
  /** Camera history for Previous / Next view. */
  private views: number[][] = []; private viewIndex = -1; private viewTimer: any = null;
  rememberView() { const v = this.scene.cam.serialize(); const cur = this.views[this.viewIndex]; if (cur && cur.every((x, i) => Math.abs(x - v[i]) < 1e-4)) return; this.views = this.views.slice(0, this.viewIndex + 1); this.views.push(v); if (this.views.length > 60) this.views.shift(); this.viewIndex = this.views.length - 1; renderHud(this, this.els['view-hud']); }
  private rememberViewSoon() { clearTimeout(this.viewTimer); this.viewTimer = setTimeout(() => this.rememberView(), 500); }
  get canPrevView() { return this.viewIndex > 0; } get canNextView() { return this.viewIndex < this.views.length - 1; }
  previousView() { if (!this.canPrevView) return; if (this.viewIndex === this.views.length - 1) this.rememberView(); this.viewIndex--; this.setCameraController(null); this.scene.cam.restore(this.views[this.viewIndex]); renderHud(this, this.els['view-hud']); }
  nextView() { if (!this.canNextView) return; this.viewIndex++; this.setCameraController(null); this.scene.cam.restore(this.views[this.viewIndex]); renderHud(this, this.els['view-hud']); }
  simulatePointer(kind: 'down' | 'move' | 'up', clientX: number, clientY: number, button = 0) {
    const ev = { clientX, clientY, button, buttons: kind === 'up' ? 0 : 1, shiftKey: false, ctrlKey: false, altKey: false, metaKey: false, pointerId: -1, pointerType: 'mouse', preventDefault() {}, stopPropagation() {} } as unknown as PointerEvent;
    if (kind === 'down') this.tool.onDown?.(ev, this.ctx); else if (kind === 'move') this.tool.onMove?.(ev, this.ctx); else this.tool.onUp?.(ev, this.ctx);
  }
  /** Drag handle on the tray's left edge; the width is kept per browser. */
  private installTrayResizer(root: HTMLElement) {
    const app = document.getElementById('app') ?? root, handle = root.querySelector<HTMLElement>('.tray-resizer')!;
    const clamp = (w: number) => Math.round(Math.max(220, Math.min(w, Math.max(320, window.innerWidth * 0.6))));
    const apply = (w: number | null) => { if (w == null) app.style.removeProperty('--tray-w'); else app.style.setProperty('--tray-w', `${clamp(w)}px`); };
    try { const saved = parseFloat(localStorage.getItem('plannr.trayWidth') ?? ''); if (saved > 0) apply(saved); } catch { /* storage blocked */ }
    const save = () => { try { const v = app.style.getPropertyValue('--tray-w'); v ? localStorage.setItem('plannr.trayWidth', String(parseFloat(v))) : localStorage.removeItem('plannr.trayWidth'); } catch { /* storage blocked */ } };
    handle.addEventListener('pointerdown', e => {
      if (e.button !== 0) return; e.preventDefault(); handle.setPointerCapture(e.pointerId);
      const startX = e.clientX, startW = this.els.tray.getBoundingClientRect().width;
      handle.classList.add('dragging'); app.classList.add('resizing-tray');
      const move = (ev: PointerEvent) => apply(startW + (startX - ev.clientX));
      const up = () => { handle.removeEventListener('pointermove', move); handle.removeEventListener('pointerup', up); handle.removeEventListener('pointercancel', up); handle.classList.remove('dragging'); app.classList.remove('resizing-tray'); save(); };
      handle.addEventListener('pointermove', move); handle.addEventListener('pointerup', up); handle.addEventListener('pointercancel', up);
    });
    handle.addEventListener('dblclick', () => { apply(null); save(); });
    window.addEventListener('resize', () => { const v = parseFloat(app.style.getPropertyValue('--tray-w')); if (v) apply(v); });
  }

  // ---- tool palette: docked left or on top, resizable; the groups re-flow to the space ----------------
  paletteDock: 'left' | 'top' = 'left';
  private paletteSize: { left: number | null; top: number | null } = { left: null, top: null };
  private installPalette(root: HTMLElement) {
    const app = document.getElementById('app') ?? root, bar = this.els.toolbar;
    const handle = root.querySelector<HTMLElement>('.toolbar-resizer')!, preview = root.querySelector<HTMLElement>('.dock-preview')!;
    let saved: { dock?: string; left?: number | null; top?: number | null } = {};
    try { saved = JSON.parse(localStorage.getItem('plannr.palette') ?? '{}') ?? {}; } catch { /* storage blocked */ }
    const phone = matchMedia('(pointer: coarse)').matches && window.innerWidth < 980;
    this.paletteDock = saved.dock === 'top' || saved.dock === 'left' ? saved.dock : phone ? 'top' : 'left';
    this.paletteSize = { left: saved.left ?? null, top: saved.top ?? null };
    this.applyPalette();
    handle.addEventListener('pointerdown', e => {
      if (e.button !== 0) return; e.preventDefault(); handle.setPointerCapture(e.pointerId);
      const top = this.paletteDock === 'top', r = bar.getBoundingClientRect();
      const start = top ? e.clientY : e.clientX, startSize = top ? r.height : r.width;
      handle.classList.add('dragging'); app.classList.add('resizing-palette');
      const move = (ev: PointerEvent) => { this.paletteSize[this.paletteDock] = startSize + ((top ? ev.clientY : ev.clientX) - start); this.applyPalette(); };
      const up = () => { handle.removeEventListener('pointermove', move); handle.removeEventListener('pointerup', up); handle.removeEventListener('pointercancel', up); handle.classList.remove('dragging'); app.classList.remove('resizing-palette'); this.savePalette(); };
      handle.addEventListener('pointermove', move); handle.addEventListener('pointerup', up); handle.addEventListener('pointercancel', up);
    });
    handle.addEventListener('dblclick', () => this.resetPaletteSize());
    // the grip: drag it to the top or the left edge, or tap it to switch
    bar.addEventListener('pointerdown', e => {
      const grip = (e.target as HTMLElement).closest<HTMLElement>('.toolbar-grip'); if (!grip || e.button !== 0) return;
      e.preventDefault(); grip.setPointerCapture(e.pointerId);
      const x0 = e.clientX, y0 = e.clientY; let moved = false; let target: 'left' | 'top' | null = null;
      const zone = (ev: PointerEvent): 'left' | 'top' | null => {
        const r = app.getBoundingClientRect(), menu = this.els.menubar.getBoundingClientRect().bottom;
        if (ev.clientY < menu + 90 && ev.clientX > r.left + 150) return 'top';
        if (ev.clientX < r.left + 150 && ev.clientY > menu) return 'left';
        return null;
      };
      const move = (ev: PointerEvent) => {
        if (!moved && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 6) return;
        moved = true; target = zone(ev); preview.className = `dock-preview${target ? ` show ${target}` : ''}`; app.classList.add('moving-palette');
      };
      const up = () => {
        grip.removeEventListener('pointermove', move); grip.removeEventListener('pointerup', up); grip.removeEventListener('pointercancel', up);
        preview.className = 'dock-preview'; app.classList.remove('moving-palette');
        if (!moved) this.dockPalette(this.paletteDock === 'left' ? 'top' : 'left'); else if (target) this.dockPalette(target);
      };
      grip.addEventListener('pointermove', move); grip.addEventListener('pointerup', up); grip.addEventListener('pointercancel', up);
    });
    new ResizeObserver(() => layoutToolbar(bar)).observe(bar);
    window.addEventListener('resize', () => this.applyPalette());
  }
  private applyPalette() {
    const app = document.getElementById('app')!;
    app.classList.toggle('dock-top', this.paletteDock === 'top');
    const w = this.paletteSize.left, h = this.paletteSize.top;
    if (w == null) app.style.removeProperty('--tool-w'); else app.style.setProperty('--tool-w', `${Math.round(Math.max(52, Math.min(w, 560, window.innerWidth * 0.5)))}px`);
    if (h == null) app.style.removeProperty('--tool-h'); else app.style.setProperty('--tool-h', `${Math.round(Math.max(46, Math.min(h, 240, window.innerHeight * 0.4)))}px`);
    layoutToolbar(this.els.toolbar);
  }
  private savePalette() { try { localStorage.setItem('plannr.palette', JSON.stringify({ dock: this.paletteDock, ...this.paletteSize })); } catch { /* storage blocked */ } }
  dockPalette(d: 'left' | 'top') { this.paletteDock = d; this.applyPalette(); this.savePalette(); renderToolbar(this, this.els.toolbar); }
  resetPaletteSize() { this.paletteSize[this.paletteDock] = null; this.applyPalette(); this.savePalette(); }

  // ---- touch: a bar to cancel / finish / type while a tool is in the middle of something --------------
  private touchUsed = matchMedia('(pointer: coarse)').matches;
  updateTouchBar() {
    const el = this.els['touch-bar']; if (!el) return;
    const busy = !!this.tool.busy?.();
    if (!this.touchUsed || !busy) { el.innerHTML = ''; el.classList.remove('show'); return; }
    if (el.classList.contains('show') && el.dataset.tool === this.tool.id) return;
    el.dataset.tool = this.tool.id; el.classList.add('show');
    // Finish ends a chain (line) or closes a shape (polygon); the other tools finish on their next tap
    const finish = ['line', 'polygon'].includes(this.tool.id);
    el.innerHTML = `<button data-k="Escape">✕ Cancel</button>${finish ? '<button data-k="Enter">✓ Finish</button>' : ''}<button data-k="vcb">⌨ Type size</button>`;
    el.querySelectorAll<HTMLElement>('button').forEach(b => b.onpointerdown = e => {
      e.preventDefault(); e.stopPropagation();
      if (b.dataset.k === 'vcb') { const v = this.els.statusbar.querySelector<HTMLInputElement>('.vcb input')!; v.focus(); v.select(); return; }
      window.dispatchEvent(new KeyboardEvent('keydown', { key: b.dataset.k!, bubbles: true }));
      // the line tool ends its chain on Escape (keeping what was drawn)
      if (b.dataset.k === 'Enter' && this.tool.busy?.()) window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      setTimeout(() => this.updateTouchBar(), 30);
    });
  }

  setCameraController(c: CameraController | null) { if (this.camCtl && this.camCtl !== c) this.camCtl.release(); this.camCtl = c; }

  constructor(root: HTMLElement) {
    if (this.embed) root.classList.add('embed');
    if (this.embed && this.embedPlugins.size) root.classList.add('embed-tools');
    root.innerHTML = `<div class="menubar"></div><div class="toolbar"></div>
      <div class="toolbar-resizer" title="Drag to resize the tools · double-click to reset"></div><div class="dock-preview"></div>
      <div class="viewport"><div class="labels"></div><div class="view-hud"></div><div class="tool-options"></div><div class="lock-badge"></div><div class="toast"></div><div class="touch-bar"></div><div class="embed-panel"></div></div>
      <div class="tray"></div><div class="tray-resizer" title="Drag to resize · double-click to reset"></div>
      <div class="statusbar"><span class="conn" title="Server connection"></span><span class="hint"></span><div class="vcb"><label>Measurements</label><input type="text" spellcheck="false"></div></div><div class="welcome-host"></div>`;
    for (const k of ['menubar', 'toolbar', 'viewport', 'labels', 'view-hud', 'tool-options', 'lock-badge', 'toast', 'touch-bar', 'embed-panel', 'tray', 'statusbar', 'welcome-host']) this.els[k] = root.querySelector(`.${k}`)!;
    this.scene = new Viewport(this.els.viewport, this.els.labels);
    this.tray = new Tray(this, this.els.tray);
    this.installTrayResizer(root);
    this.installPalette(root);
    this.ctx = this.makeContext();
    this.bindInput(); installTooltips();
    this.scene.start();
    onPlugin(p => { this.tools.push(...(p.tools ?? [])); p.activate?.(this.pluginHost()); this.refreshUI(); });
    this.scene.expandIds = ids => { const out = new Set(ids); for (const id of ids) { const e = findEntity(this.doc, id); if (e?.kind === 'group') for (const { entity } of walk(e.entities)) out.add(entity.id); } return out; };
    this.scene.faceStyle = this.faceStyle; this.scene.backEdges = this.backEdges;
    if (this.fov !== 35) this.scene.cam.setFov(this.fov); if (this.fov > 1) this.lens = this.fov; this.applySun();
    this.refreshUI(); this.setTool('select');
    if (this.embed) this.installEmbedBridge();
    this.boot();
  }

  // ---- lifecycle ---------------------------------------------------------------
  private async boot() {
    try { await api.health(); this.online = true; } catch { this.online = false; }
    this.setConn(this.online);
    try { this.serverPlugins = await api.plugins(); } catch { /* older server */ }
    await this.loadPluginScripts();
    const q = new URLSearchParams(location.search).get('project');
    const last = q ?? localStorage.getItem('plannr.lastProject');
    if (this.online && last) {
      try { await this.openProject(last); this.post({ event: 'ready', project: this.projectId, name: this.projectName, version: this.doc.version }); return; }
      catch (e: any) { if (this.embed) { this.toast(`Cannot open project: ${e.message ?? e}`); this.post({ event: 'error', message: String(e.message ?? e) }); return; } }
    }
    if (this.embed) { this.toast('No project: add ?project=<id> to the URL'); this.post({ event: 'error', message: 'no project' }); return; }
    this.showWelcome();
  }
  /** Load the browser bundle of every engine plugin that ships one. */
  private async loadPluginScripts() {
    await Promise.all(this.serverPlugins.filter(p => p.script).map(p => new Promise<void>(resolve => {
      const s = document.createElement('script'); s.type = 'module'; s.src = p.script!; s.onload = () => resolve(); s.onerror = () => { this.toast(`Plugin ${p.id}: failed to load ${p.script}`); resolve(); };
      document.head.appendChild(s);
    })));
  }
  private setConn(ok: boolean) { this.els.statusbar.querySelector('.conn')!.classList.toggle('ok', ok); }
  async showWelcome() {
    let projects: Awaited<ReturnType<typeof api.listProjects>> = [];
    if (this.online) { try { projects = await api.listProjects(); } catch { /* offline */ } }
    renderWelcome(this, this.els['welcome-host'], projects, this.online);
  }
  hideWelcome() { this.els['welcome-host'].innerHTML = ''; }
  async newFromTemplate(kind: 'empty' | 'sample' | 'import') {
    if (kind === 'sample') return this.loadSample();
    if (kind === 'import') return this.importJson(true);
    const p = await api.createProject(`Model ${new Date().toLocaleDateString()}`); await this.openProject(p.id);
  }
  async loadSample() { try { const p = await api.sample(); await this.openProject(p.id); } catch (e: any) { this.toast(`Could not load sample: ${e.message}`); } }
  async openProject(id: string) {
    const p = await api.getProject(id);
    this.projectId = p.id; this.projectName = p.name; localStorage.setItem('plannr.lastProject', p.id);
    history.replaceState(null, '', `?project=${p.id}`);
    this.doc = p.document; this.selection.clear(); this.enteredGroup = null;
    this.options.layer = this.doc.layers[0]?.id ?? 'layer0'; this.options.material = this.doc.materials[0]?.id ?? 'default';
    this.live?.close(); this.live = new Live(p.id);
    this.knownVersion = p.document.version;
    // our own changes come back here too: those are applied already (a second whole reload made every action lag)
    this.live.onChanged = (version, label, source) => {
      if (this.localBusy > 0) { this.remoteVersion = Math.max(this.remoteVersion, version); return; }
      if (version > this.knownVersion) this.refresh(source !== 'browser' ? `${label} (${source})` : undefined);
    };
    this.live.onCommand = (name, args) => this.handleCommand(name, args);
    this.live.onStatus = ok => this.setConn(ok);
    this.hideWelcome(); document.title = `${p.name} — Plannr`;
    await this.refresh(); this.zoomExtents();
  }
  /** The newest document version this browser has (or is applying): change notices up to it are our own. */
  private knownVersion = 0; private localBusy = 0; private remoteVersion = 0; private reconcileTimer = 0;
  /** Reload document + compiled scene from the server. `changes` (from a partial commit, undo or redo):
   *  the scene is patched with just what changed and only the document is fetched. */
  async refresh(notice?: string, changes?: Changes & { version: number }) {
    if (!this.projectId) return;
    const seq = ++this.compileSeq;
    let patch = changes?.scene && changes.dirty ? changes : null;
    if (changes) this.knownVersion = Math.max(this.knownVersion, changes.version);
    if (patch) try { this.scene.patch(patch.scene!, patch.dirty!); } catch (e) { console.warn('patch failed, reloading the scene', e); patch = null; } // at once, against the document it was drawn from
    if (!patch) clearTimeout(this.reconcileTimer);
    try {
      const [p, c, h] = await Promise.all([api.getProject(this.projectId), patch ? null : api.compile(this.projectId, this.hiddenGeometry), api.history(this.projectId)]);
      if (seq !== this.compileSeq) return;
      this.doc = p.document; this.projectName = p.name; this.historyList = h; this.knownVersion = Math.max(this.knownVersion, p.document.version);
      const cur = h.findIndex(x => x.version === this.doc.version); this.canUndo = h.length > 0; this.canRedo = h.length > 0; void cur;
      if (c) this.applyCompiled(c);
      if (patch?.reconcile) this.reconcileSoon();
      const compiledIds = [...this.selection].some(id => id.includes('#')) ? new Set(this.scene.compiled?.groups.flatMap(g => g.ids) ?? []) : null; // a selected part stays while it is still generated
      for (const id of [...this.selection]) if (id.includes('#') ? !compiledIds!.has(id) : !findEntity(this.doc, id)) this.selection.delete(id);
      if (this.enteredGroup && !findEntity(this.doc, this.enteredGroup)) this.enteredGroup = null;
      this.scene.highlight(this.selection); this.saveState = `v${this.doc.version}`; this.online = true; this.setConn(true);
      if (notice) this.toast(notice);
    } catch (e: any) { this.online = false; this.setConn(false); this.toast(`Engine unavailable: ${e.message}`); }
    this.refreshUI(); this.emit('changed', this.doc);
  }
  private applyCompiled(c: CompiledScene) { this.scene.setCompiled(c); this.scene.highlight(this.selection); }
  /** Plugin geometry can depend on neighbours a change didn't touch (a wall's corners): once the user pauses,
   *  fetch the whole scene in the background and swap it in if nothing happened meanwhile. */
  private reconcileSoon(delay = 1200) {
    clearTimeout(this.reconcileTimer);
    this.reconcileTimer = window.setTimeout(async () => {
      if (!this.projectId) return;
      if (this.previewWanted || this.localBusy > 0) { this.reconcileSoon(delay); return; }
      const seq = this.compileSeq, version = this.doc.version;
      try {
        const c = await api.compile(this.projectId, this.hiddenGeometry);
        if (seq !== this.compileSeq || version !== this.doc.version || c.version !== version) return;
        if (this.previewWanted) { this.reconcileSoon(delay); return; }
        this.applyCompiled(c);
      } catch { /* offline: the next change refreshes */ }
    }, delay);
  }
  /** Run a change made here (commit, undo, redo); change notices that arrive meanwhile are its own. */
  private async local<T>(f: () => Promise<T>): Promise<T> {
    this.localBusy++;
    try { return await f(); }
    finally {
      this.localBusy--;
      if (this.localBusy === 0 && this.remoteVersion > this.knownVersion) { this.remoteVersion = 0; void this.refresh(); } // someone else changed it meanwhile
    }
  }
  async rename(name: string) { this.projectName = name; document.title = `${name} — Plannr`; if (this.projectId) await api.renameProject(this.projectId, name); this.refreshUI(); }
  async renamePrompt() { const n = await this.promptText('Project name', this.projectName); if (n?.trim()) this.rename(n.trim()); }
  async duplicate() { if (!this.projectId) return; const p = await api.duplicateProject(this.projectId); await this.openProject(p.id); this.toast(`Duplicated as “${p.name}”`); }
  async deleteProject(id = this.projectId ?? '') {
    if (!id || !(await this.confirm('Delete this project permanently? This cannot be undone.', 'Delete', true))) return;
    await api.deleteProject(id); if (id === this.projectId) { this.projectId = null; localStorage.removeItem('plannr.lastProject'); }
    this.showWelcome();
  }
  exportGlb() { if (!this.projectId) return; window.location.href = api.exportGlbUrl(this.projectId); }
  exportJson() { const blob = new Blob([JSON.stringify(this.doc, null, 2)], { type: 'application/json' }); const a = document.createElement('a'); a.href = URL.createObjectURL(blob); a.download = `${this.projectName}.json`; a.click(); }
  importJson(asNew = false) {
    const inp = document.createElement('input'); inp.type = 'file'; inp.accept = 'application/json,.json';
    inp.onchange = async () => {
      const f = inp.files?.[0]; if (!f) return;
      try { const d = JSON.parse(await f.text()); if (asNew || !this.projectId) { const p = await api.createProject(f.name.replace(/\.json$/i, ''), d); await this.openProject(p.id); } else { await api.putDocument(this.projectId, d); await this.refresh('Imported'); this.zoomExtents(); } }
      catch (e: any) { this.toast(`Import failed: ${e.message}`); }
    };
    inp.click();
  }

  // ---- ops / undo ------------------------------------------------------------------
  /** What a tool draws while a group is open for editing goes into that group (and sticks to its faces), as in SketchUp. */
  inContext(ops: Op[]): Op[] {
    const g = this.enteredGroup ? findEntity(this.doc, this.enteredGroup) : undefined; if (g?.kind !== 'group') return ops;
    return ops.map(o => ['draw_line', 'draw_rectangle', 'draw_polygon', 'draw_circle', 'draw_arc', 'stretch', 'insert', 'text3d'].includes(o.op) && !(o as any).group ? { ...o, group: g.id } as Op : o);
  }
  async commitOps(ops: Op[], label: string): Promise<OpResult[]> {
    if (!this.projectId || !ops.length) return [];
    this.previewNext = null; this.previewWanted = false; this.previewSeq++; this.previewFull = false; // the preview stays on screen until the refreshed scene replaces it (no flash of the old state)
    const id = this.projectId;
    return this.local(async () => {
      try {
        const r = await api.opsPartial(id, ops, label, this.hiddenGeometry);
        await this.refresh(undefined, r);
        return r.results;
      } catch (e: any) { this.toast(String(e.message).replace(/ \[[0-9a-f-]{36}\]$/, '')); await this.refresh(); return []; }
    });
  }
  /** Live result of a tool. One request at a time, always for the newest ops: a slow engine reply never piles
   *  up behind others. The engine sends only the changed objects, drawn over the model (nothing else is rebuilt). */
  preview(ops: Op[]) {
    if (!this.projectId) return;
    if (!ops.length) { this.clearPreview(); return; }
    this.previewNext = ops; this.previewWanted = true;
    if (!this.previewBusy) void this.pumpPreview();
  }
  private previewNext: Op[] | null = null; private previewBusy = false; private previewWanted = false; private previewFull = false;
  private async pumpPreview() {
    this.previewBusy = true;
    while (this.previewNext && this.projectId) {
      const ops = this.previewNext; this.previewNext = null; const seq = ++this.previewSeq;
      try {
        const c = await api.preview(this.projectId, ops, this.hiddenGeometry, true);
        if (seq !== this.previewSeq || !this.previewWanted) continue;
        if (c.partial) { if (this.previewFull) { this.previewFull = false; } this.scene.setPreview(c, c.dirty ?? []); }
        else { this.previewFull = true; this.applyCompiled(c); } // the engine could not tell what changed: the whole scene
      } catch { /* invalid intermediate state: ignore */ }
    }
    this.previewBusy = false;
  }
  private clearPreview() {
    this.previewNext = null; this.previewWanted = false; this.previewSeq++;
    this.scene.setPreview(null);
    if (this.previewFull && this.projectId) { this.previewFull = false; const seq = this.previewSeq; api.compile(this.projectId, this.hiddenGeometry).then(c => { if (seq === this.previewSeq && !this.previewWanted) this.applyCompiled(c); }).catch(() => {}); }
  }
  async undo() { const id = this.projectId; if (!id) return; await this.local(async () => { try { const r = await api.undo(id, this.hiddenGeometry); this.toast(`Undo ${r.label}`); await this.refresh(undefined, r); } catch (e: any) { this.toast(e.message); } }); }
  async redo() { const id = this.projectId; if (!id) return; await this.local(async () => { try { const r = await api.redo(id, this.hiddenGeometry); this.toast(`Redo ${r.label}`); await this.refresh(undefined, r); } catch (e: any) { this.toast(e.message); } }); }

  // ---- selection / groups -------------------------------------------------------------
  private docIds: { doc: Document | null; ids: Set<string> } = { doc: null, ids: new Set() };
  /** Is this (compiled) id an entity of the document, not geometry that only the current preview has? */
  inDocument(id: string): boolean {
    if (this.docIds.doc !== this.doc) { const ids = new Set<string>(); for (const { entity } of walk(this.doc.entities)) ids.add(entity.id); this.docIds = { doc: this.doc, ids }; }
    return this.docIds.ids.has(id.split('#')[0]);
  }
  /** The open entity is a plugin's parts-built entity (a framed wall), not a group. */
  get enteredParts(): boolean { return !!this.enteredGroup && !!partsFor(findEntity(this.doc, this.enteredGroup)); }
  /** What the top bar says while something is open. */
  get enteredLabel(): string { const e = this.enteredGroup ? findEntity(this.doc, this.enteredGroup) : undefined; const pp = partsFor(e); return pp && e?.kind === 'custom' ? pp.label?.(e) ?? e.type : 'group'; }
  resolve(id: string): string {
    if (this.enteredGroup && id.startsWith(`${this.enteredGroup}#`) && this.enteredParts) return id; // inside a framed wall: the single member
    id = id.split('#')[0]; // expanded plugin geometry carries `<entityId>#<faceKey>`
    if (!this.enteredGroup) return topLevelId(this.doc, id);
    // inside a group: resolve to the direct child of the entered group
    for (const { entity, path } of walk(this.doc.entities)) if (entity.id === id) { const i = path.indexOf(this.enteredGroup); return i >= 0 ? (path[i + 1] ?? id) : topLevelId(this.doc, id); }
    return id;
  }
  select(ids: string[], mode: 'replace' | 'add' | 'toggle' = 'replace') {
    if (mode === 'replace') this.selection.clear();
    for (const id of ids) { if (mode === 'toggle' && this.selection.has(id)) this.selection.delete(id); else this.selection.add(id); }
    this.scene.highlight(this.selection); this.tray.render(); renderMenubar(this, this.els.menubar); this.emit('selection', [...this.selection]); this.sendState();
  }
  selectAll() { if (this.enteredParts) { const pre = `${this.enteredGroup}#`; return this.select([...new Set((this.scene.compiled?.groups ?? []).flatMap(g => g.ids).filter(id => id.startsWith(pre)))]); } this.select((this.enteredGroup ? (findEntity(this.doc, this.enteredGroup) as any)?.entities ?? [] : this.doc.entities).map((e: any) => e.id)); }
  enterGroup(id: string) {
    const e = findEntity(this.doc, id);
    const pp = partsFor(e); if (pp && e?.kind === 'custom') { this.enteredGroup = e.id; this.select([]); this.toast(`Editing ${pp.label?.(e) ?? e.type}: click a part to select it. Esc closes`); this.refreshUI(); return; }
    const g = e?.kind === 'group' ? e : null; if (!g) return; this.enteredGroup = g.id; this.select([]); this.toast(`Editing group “${g.name}” — Esc to close`); this.refreshUI(); }
  exitGroup() { if (!this.enteredGroup) return; this.enteredGroup = null; this.select([]); this.refreshUI(); }
  async deleteSelection() {
    if (!this.selection.size) return;
    if ([...this.selection].some(id => id.includes('#'))) return this.erasePartsAndRest();
    const ids = [...this.selection]; if (this.embed && ids.some(id => { const e = findEntity(this.doc, id); return e?.kind !== 'section' && !(e?.kind === 'custom' && this.embedPlugins.has(e.type.split('.')[0])); })) return; /* a viewer only erases its own section planes and the entities of the plugins it was given */ this.selection.clear(); await this.commitOps([{ op: 'erase', ids }], 'Erase'); }
  /** Delete with parts of a parts-built entity selected: the plugin takes the parts out, the rest is erased. */
  private async erasePartsAndRest() {
    const ops: Op[] = []; const byEntity = new Map<string, string[]>(); const whole: string[] = [];
    for (const id of this.selection) { const i = id.indexOf('#'); if (i < 0) { whole.push(id); continue; } const b = id.slice(0, i); byEntity.set(b, [...(byEntity.get(b) ?? []), id.slice(i + 1)]); }
    for (const [b, keys] of byEntity) {
      const e = findEntity(this.doc, b); const pp = partsFor(e); const o = pp && e?.kind === 'custom' ? pp.erase?.(this.ctx, e, keys) ?? null : null;
      if (!o) { this.toast('These parts can\'t be removed on their own'); return; }
      if (this.embed && !this.embedPlugins.has((e as any).type.split('.')[0])) return;
      ops.push(...o);
    }
    if (whole.length) { if (this.embed) return; ops.push({ op: 'erase', ids: whole }); }
    this.selection.clear(); await this.commitOps(ops, byEntity.size && !whole.length ? 'Remove members' : 'Erase');
  }
  /** A 2D slice: the model's cut lines at section plane `id`, as a new group of edges in the plane. */
  async sliceSection(id: string) {
    try {
      const r = await this.commitOps([{ op: 'slice_section', id }], 'Slice');
      const gid = r[0]?.created?.[0]; if (!gid) return;
      const g = this.doc.entities.find(e => e.id === gid); const n = g?.kind === 'group' ? g.entities.length : 0;
      this.select([gid]);
      this.toast(`${g?.kind === 'group' ? g.name : 'Slice'}: ${n} lines on the "Slices" tag. Move (M) or copy it to plan with it; hide the tag in View to get it out of the way.`, 6000);
    } catch (e: any) { this.toast(`Slice: ${e.message ?? e}`); }
  }
  async groupSelection() { if (!this.selection.size) return; const r = await this.commitOps([{ op: 'make_group', ids: [...this.selection], name: `Group ${this.doc.entities.filter(e => e.kind === 'group').length + 1}` }], 'Make group'); if (r[0]?.created?.[0]) this.select([r[0].created[0]]); }
  async makeComponent() {
    if (!this.selection.size) return this.toast('Select the entities that make up the component first');
    const name = await this.promptText('Component name', `Component ${(this.doc.components?.length ?? 0) + 1}`); if (!name) return;
    const r = await this.commitOps([{ op: 'make_component', ids: [...this.selection], name }], 'Make component'); if (r[0]?.created?.[0]) this.select([r[0].created[0]]);
  }
  placeComponent(definition: string) { placeComponentTool.definition = definition; this.setTool('place'); }
  /** Everything that hangs together with the selected faces and edges. */
  selectConnected() { this.select([...new Set([...this.selection].flatMap(id => connectedTo(this.ctx, id)))]); }
  /** Everything beside the selection (the top level, or the open group) on the same tags. */
  selectSameTag() {
    const tags = new Set([...this.selection].map(id => findEntity(this.doc, id)?.layer)); const g = this.enteredGroup ? findEntity(this.doc, this.enteredGroup) : null;
    this.select((g?.kind === 'group' ? g.entities : this.doc.entities).filter((e: Entity) => tags.has(e.layer)).map(e => e.id));
  }
  // ---- clipboard, lock, import and export --------------------------------------------------------------
  private clipMem: { entities: Entity[]; anchor: Vec3; group: string | null } | null = null;
  /** What was copied last, in this window or another one. */
  private get clipboard(): { entities: Entity[]; anchor: Vec3; group: string | null } | null { try { return JSON.parse(localStorage.getItem('plannr.clipboard') ?? 'null') ?? this.clipMem; } catch { return this.clipMem; } }
  copySelection(): boolean {
    const ents = [...new Set([...this.selection].map(id => findEntity(this.doc, id.split('#')[0])).filter((e): e is Entity => !!e))]; if (!ents.length) return false;
    const b = selectionBounds(this.ctx, this.selection); // held by its lowest corner when pasted
    const clip = { entities: JSON.parse(JSON.stringify(ents)) as Entity[], anchor: (b.isEmpty() ? [0, 0, 0] : [b.min.x, b.min.y, b.min.z]) as Vec3, group: this.enteredGroup };
    this.clipMem = clip; try { localStorage.setItem('plannr.clipboard', JSON.stringify(clip)); } catch { try { localStorage.removeItem('plannr.clipboard'); } catch { /* storage blocked */ } }
    this.toast(`Copied ${ents.length === 1 ? 'one entity' : `${ents.length} entities`}. Paste with ${navigator.platform.includes('Mac') ? '⌘' : 'Ctrl+'}V.`); return true;
  }
  async cutSelection() { if (this.copySelection()) await this.deleteSelection(); }
  /** Paste: the copy follows the cursor until a click puts it down; in place, it lands where the original was. */
  async paste(inPlace = false) {
    const c = this.clipboard; if (!c) { this.toast('Nothing to paste: copy something first.'); return; }
    // copied inside the group that is open now: its coordinates are that group's own
    const entities = c.entities; const local = !!c.group && c.group === this.enteredGroup;
    if (!inPlace) { pasteTool.clip = { entities, anchor: c.anchor, local }; this.setTool('paste'); return; }
    const r = await this.commitOps(this.inContext([{ op: 'insert', entities, ...(local ? { local } : {}) } as Op]), 'Paste in place'); if (r[0]?.created?.length) this.select(r[0].created);
  }
  get lockedIds(): string[] { return [...walk(this.doc.entities)].filter(x => x.entity.locked === true).map(x => x.entity.id); }
  async lockSelection(locked = true) {
    const ids = [...new Set([...this.selection].map(id => id.split('#')[0]))]; if (!ids.length) return;
    await this.commitOps([{ op: 'set_locked', ids, locked }], locked ? 'Lock' : 'Unlock'); this.toast(locked ? 'Locked: it can be selected and copied, not changed.' : 'Unlocked.');
  }
  async unlockAll() { const ids = this.lockedIds; if (ids.length) await this.commitOps([{ op: 'set_locked', ids, locked: false }], 'Unlock all'); }
  exportObj() { const c = this.scene.compiled; if (c) download(`${this.projectName}.obj`, toOBJ(c), 'text/plain'); }
  exportStl() { const c = this.scene.compiled; if (c) download(`${this.projectName}.stl`, toSTL(c), 'model/stl'); }
  /** Import an OBJ or STL file as one group; triangles that lie in one plane are joined into faces. */
  importModel() {
    const inp = document.createElement('input'); inp.type = 'file'; inp.accept = '.obj,.stl';
    inp.onchange = async () => { const f = inp.files?.[0]; if (f) await this.importFile(f.name, await f.arrayBuffer()); };
    inp.click();
  }
  async importFile(name: string, data: ArrayBuffer) {
    const stl = /\.stl$/i.test(name);
    const unit = (await this.promptText('Unit of the file: mm, cm or m', stl ? 'mm' : 'm'))?.trim().toLowerCase(); if (!unit) return;
    const scale = ({ mm: 0.001, cm: 0.01, m: 1 } as Record<string, number>)[unit]; if (!scale) { this.toast('The unit is mm, cm or m.'); return; }
    let faces: Vec3[][] = [];
    try { faces = stl ? parseSTL(data, scale) : parseOBJ(new TextDecoder().decode(data), scale); } catch (e: any) { this.toast(`Could not read ${name}: ${e.message ?? e}`); return; }
    if (!faces.length) { this.toast(`No faces found in ${name}.`); return; }
    if (faces.length > 150000) { this.toast(`${name} has ${faces.length} faces: too many to import (150 000 at most).`, 6000); return; }
    const r = await this.commitOps([{ op: 'insert', entities: [meshGroup(name.replace(/\.[^.]+$/, ''), faces)], merge: true }], 'Import');
    if (r[0]?.created?.length) { this.select(r[0].created); this.zoomSelection(); }
  }
  /** The unit lengths are shown and typed in, and how many decimals; stored with the document. */
  async setUnits(unit: string, digits?: number) {
    if (!this.projectId || !UNITS[unit]) return;
    await api.putDocument(this.projectId, { ...this.doc, units: unit, precision: digits ?? UNITS[unit].digits }); await this.refresh('Units');
  }
  /** The drawing axes were moved (the Axes tool). */
  get axesMoved() { return !!this.doc.axes; }
  /** Move the drawing axes (origin, red axis, blue axis), or put them back with null; stored with the document. */
  async setAxes(origin: THREE.Vector3 | null, x?: THREE.Vector3, y?: THREE.Vector3) {
    if (!this.projectId) return; const doc: Document = { ...this.doc };
    const r = (v: THREE.Vector3) => [+v.x.toFixed(6), +v.y.toFixed(6), +v.z.toFixed(6)];
    if (origin && x && y) doc.axes = { origin: r(origin), x: r(x.clone().normalize()), y: r(y.clone().normalize()) }; else delete doc.axes;
    await api.putDocument(this.projectId, doc); await this.refresh(origin ? 'Axes' : 'Reset axes');
  }
  private axesKey = '';
  private syncAxes() {
    const a = this.doc.axes as { origin?: Vec3; x?: Vec3; y?: Vec3 } | undefined; const ok = !!a && [a.origin, a.x, a.y].every(v => Array.isArray(v) && v.length === 3 && v.every(n => typeof n === 'number'));
    const key = ok ? JSON.stringify(a) : ''; if (key === this.axesKey) return; this.axesKey = key;
    if (ok) setDrawingAxes(V(a!.origin!), V(a!.x!), V(a!.y!)); else setDrawingAxes(new THREE.Vector3(), new THREE.Vector3(1, 0, 0), new THREE.Vector3(0, 1, 0));
    this.scene.setAxes(GROUND.origin, AXIS.x, AXIS.y, AXIS.z);
  }
  private syncUnits() {
    const was = `${lengthUnit.name}/${lengthUnit.digits}`; setLengthUnit(this.doc.units, this.doc.precision as number | undefined);
    if (was !== `${lengthUnit.name}/${lengthUnit.digits}`) this.scene.relabel();
  }
  /** Intersect Faces: edges where the selected faces cross other faces of the same shape, dividing both. */
  async intersectFaces() {
    const faces = [...this.selection].filter(id => findEntity(this.doc, id)?.kind === 'face');
    if (!faces.length) { this.toast('Intersect Faces works on drawn faces: select them first (open a group to reach the faces inside it).', 5000); return; }
    const r = await this.commitOps([{ op: 'intersect_faces', ids: faces }], 'Intersect faces');
    const edges = (r[0]?.created ?? []).filter(id => findEntity(this.doc, id)?.kind === 'edge'); if (edges.length) this.select(edges);
  }
  /** Soften Edges: the selected faces (and those inside selected groups) run on smoothly into each other where
   *  they meet at up to an angle; 0 makes their edges hard again. Asks for the angle unless it is given. */
  private softAngle = 20;
  async softenEdges(angle?: number) {
    const ids = [...new Set([...this.selection].map(id => id.split('#')[0]))].filter(id => ['face', 'group'].includes(findEntity(this.doc, id)?.kind ?? ''));
    if (!ids.length) { this.toast('Soften Edges works on drawn faces and groups: select them first.'); return; }
    if (angle === undefined) {
      const v = await this.promptText('Soften the edges where faces meet at up to this many degrees (20 suits most curves; 0 makes them hard again)', String(this.softAngle)); if (v === null) return;
      angle = parseFloat(v); if (!(angle >= 0 && angle <= 180)) { this.toast('The angle is a number of degrees between 0 and 180.'); return; }
      if (angle > 0) this.softAngle = angle;
    }
    await this.commitOps([{ op: 'soften', ids, angle }], angle > 0 ? 'Soften edges' : 'Unsoften edges');
  }
  /** Turn the selected faces inside out: the front becomes the back (each side keeps its material). */
  async reverseFaces() {
    const ops: Op[] = []; for (const id of this.selection) { const e = findEntity(this.doc, id); if (e?.kind === 'face') ops.push({ op: 'update_entity', id, patch: { outer: [...e.outer].reverse(), holes: e.holes.map(h => [...h].reverse()), material: e.backMaterial ?? null, backMaterial: e.material ?? null } }); }
    if (ops.length) await this.commitOps(ops, 'Reverse faces'); else this.toast('Reverse Faces works on drawn faces: select them first.');
  }
  /** Right-click: the menu for what is under the cursor (it becomes the selection unless it already is part of it). */
  private contextMenu(e: { clientX: number; clientY: number }, id?: string) {
    if (this.tool.busy?.()) return;
    const p = id ? null : this.scene.pick(e); const hit = id ?? (p ? this.resolve(p.id) : null);
    if (hit && !this.selection.has(hit)) this.select([hit]);
    showContextMenu(this, e.clientX, e.clientY);
  }
  async explodeSelection() { const ops: Op[] = [...this.selection].filter(id => ['group', 'instance'].includes(findEntity(this.doc, id)?.kind ?? '')).map(id => ({ op: 'explode', id })); if (ops.length) { await this.commitOps(ops, 'Explode'); this.select([]); } }

  // ---- view ----------------------------------------------------------------------------------
  zoomExtents() { this.setCameraController(null); this.rememberView(); this.scene.cam.frame(this.scene.bounds); this.rememberViewSoon(); }
  zoomTo(ids: string[]) {
    this.setCameraController(null); this.rememberView(); this.rememberViewSoon();
    const b = new THREE.Box3(); const set = new Set(ids.map(id => this.resolve(id)));
    for (const s of this.scene.segments) if (set.has(this.resolve(s.id))) { b.expandByPoint(s.a); b.expandByPoint(s.b); }
    if (b.isEmpty()) return this.zoomExtents();
    this.scene.cam.frame({ min: A(b.min), max: A(b.max) });
  }
  zoomSelection() { this.selection.size ? this.zoomTo([...this.selection]) : this.zoomExtents(); }
  setView(v: ViewName) { this.setCameraController(null); this.rememberView(); this.scene.cam.setView(v, this.scene.bounds); this.rememberViewSoon(); }
  /** What a scene keeps: the view, the active section planes, which tags show, the face style, the lens and the sun. */
  private sceneState() { const active = this.activeSections(); return { camera: this.scene.cam.serialize(), section: active[0] ?? null, sections: active, layers: Object.fromEntries(this.doc.layers.map(l => [l.id, l.visible])), faceStyle: this.faceStyle, fov: this.fov, sun: { ...this.sun } }; }
  async saveScene() { const name = await this.promptText('Scene name', `Scene ${this.doc.scenes.length + 1}`); if (!name) return; await this.putScenes([...this.doc.scenes, { id: crypto.randomUUID(), name, ...this.sceneState() }], 'Add scene'); }
  private activeSections(): string[] { return [...walk(this.doc.entities)].map(x => x.entity).filter(e => e.kind === 'section' && e.active).map(e => e.id); }
  /** The planes a scene keeps active (older scenes stored a single one). */
  private sceneSections(s: Scene): string[] { return s.sections ?? (s.section ? [s.section] : []); }
  async updateScene(id: string) { await this.putScenes(this.doc.scenes.map(s => s.id === id ? { ...s, ...this.sceneState() } : s), 'Update scene'); }
  async removeScene(id: string) { await this.putScenes(this.doc.scenes.filter(s => s.id !== id), 'Remove scene'); }
  private async putScenes(scenes: Document['scenes'], label: string) { if (!this.projectId) return; await api.putDocument(this.projectId, { ...this.doc, scenes }); await this.refresh(label); }
  restoreScene(id: string) {
    const s = this.doc.scenes.find(s => s.id === id); if (!s) return; this.rememberView();
    if (s.fov && s.fov !== this.fov) this.setFov(s.fov);
    if (s.faceStyle && s.faceStyle !== this.faceStyle && FACE_STYLES.some(f => f.id === s.faceStyle)) this.setFaceStyle(s.faceStyle as FaceStyle);
    if (s.sun) this.setSun(s.sun);
    this.scene.cam.restore(s.camera); this.rememberViewSoon(); const ops: Op[] = []; const keep = this.sceneSections(s);
    for (const l of this.doc.layers) if (s.layers && s.layers[l.id] !== undefined && s.layers[l.id] !== l.visible) ops.push({ op: 'update_layer', id: l.id, visible: s.layers[l.id] });
    for (const { entity } of walk(this.doc.entities)) if (entity.kind === 'section' && entity.active !== keep.includes(entity.id)) ops.push({ op: 'update_section', id: entity.id, active: keep.includes(entity.id) }); if (ops.length) this.commitOps(ops, 'Scene');
  }
  toggleDisplay(k: keyof App['display']) {
    this.display[k] = !this.display[k];
    this.scene.grid.visible = this.display.grid; this.scene.axes.visible = this.display.axes; this.scene.figure.visible = this.display.figure;
    this.scene.edgesVisible = this.display.edges; if (this.scene.compiled) this.scene.setCompiled(this.scene.compiled); this.scene.highlight(this.selection);
    this.scene.renderer.shadowMap.enabled = this.display.shadows; this.scene.ground.receiveShadow = this.display.shadows;
    this.scene.setSectionPlanesVisible(this.display.sectionPlanes); if (k === 'sectionPlanes') { try { localStorage.setItem('plannr.sectionPlanes', this.display.sectionPlanes ? '1' : '0'); } catch { /* storage blocked */ } this.els.toolbar.querySelector('[data-tool="sectiondisplay"]')?.classList.toggle('active', this.display.sectionPlanes); }
    this.refreshUI();
  }

  // ---- tools ---------------------------------------------------------------------------------
  setTool(id: string) {
    const t = this.tools.find(t => t.id === id); if (!t) return;
    if (!this.toolAllowed(id)) return;
    this.tool.deactivate?.(this.ctx); this.scene.clearOverlay(); this.scene.setSnapMarker(null); this.lock = null; this.shiftLock = null;
    this.tool = t; t.activate?.(this.ctx); this.updateLockBadge();
    this.scene.renderer.domElement.style.cursor = t.cursor;
    renderToolbar(this, this.els.toolbar); renderToolOptions(this, this.els['tool-options']); this.emit('tool', t.id); this.sendState(); this.updateTouchBar();
  }
  setModifierTool(key: ModKey, toolId: string) {
    this.modifierTools[key] = toolId;
    try { localStorage.setItem('plannr.modifierTools', JSON.stringify(this.modifierTools)); } catch { /* private mode */ }
    this.refreshUI();
  }
  /** Modifier pressed: switch to its tool until the key comes back up. */
  private beginTempTool(key: ModKey): boolean {
    const id = this.modifierTools[key]; if (!id || this.temp || this.pointerDown) return false;
    if (this.tool.id === id || this.tool.busy?.() || this.tool.ownKeys?.includes(key)) return false;
    const prev = this.tool; this.setTool(id); if (this.tool.id !== id) return false;
    this.temp = { key, prev };
    this.els.statusbar.querySelector('.hint')!.textContent = `${this.tool.label} — release ${MOD_LABEL[key]} to return to ${prev.label}.`;
    return true;
  }
  private endTempTool(key?: ModKey) {
    if (!this.temp || (key && this.temp.key !== key)) return;
    const prev = this.temp.prev; this.temp = null;
    if (this.tools.includes(prev)) this.setTool(prev.id); else this.setTool('select');
  }
  setLock(l: AxisLock) {
    this.lock = this.lock === l ? null : l; this.shiftLock = null;
    this.tool.onLock?.(this.ctx); this.updateLockBadge();
    // show the new plane or axis at once, without waiting for the mouse to move
    if (this.lastMove && this.tool.planeLock) this.replayMove(this.lastMove);
  }
  /** The last pointer move over the viewport, to redraw a tool's preview after a key changed it. */
  private lastMove: PointerEvent | null = null;
  private replayMove(e: PointerEvent) {
    this.snapAsked = false; this.tool.onMove?.(e, this.ctx);
    if (this.snapAsked && this.lastSnap) { const sn = this.lastSnap; this.scene.setSnapMarker(sn, snapColor(sn), snapLabel(sn)); } else this.scene.setSnapMarker(null);
  }
  /** The face a shape tool draws on: under the cursor, or (for a snapped corner or edge) the visible face that holds the point. */
  private faceNormal(ev: { clientX: number; clientY: number }, at?: THREE.Vector3): THREE.Vector3 | null {
    const pk = this.scene.pickFace(ev); const under = pk ? { n: pk.back ? pk.normal.clone().negate() : pk.normal.clone(), p: pk.point } : null;
    const holds = (h: { n: THREE.Vector3; p: THREE.Vector3 }) => !at || Math.abs(h.n.dot(at.clone().sub(h.p))) < 1e-3;
    if (under && holds(under)) return under.n.normalize();
    if (at) { const n = this.scene.faceAt(at); if (n) return n.normalize(); }
    return under ? under.n.normalize() : null;
  }
  private makeContext(): ToolContext {
    const app = this;
    return {
      scene: this.scene, doc: () => app.doc,
      ops: (ops, label) => app.commitOps(app.inContext(ops), label), preview: (ops) => app.preview(app.inContext(ops)),
      select: (ids, mode) => app.select(ids, mode), selection: () => app.selection, resolve: (id) => app.resolve(id),
      setHint: (t) => { app.els.statusbar.querySelector('.hint')!.textContent = t; },
      setVCB: (v, label) => { const vcb = app.els.statusbar.querySelector<HTMLInputElement>('.vcb input')!; const lab = app.els.statusbar.querySelector('.vcb label')!; if (label) lab.textContent = label; if (document.activeElement !== vcb) vcb.value = shownLength(v, lab.textContent ?? ''); },
      snap: (ev, opts) => app.snap(ev, opts), setTool: (id) => app.setTool(id), setAxes: (o, x, y) => app.setAxes(o, x, y),
      options: this.options, toast: (t) => app.toast(t), prompt: (l, v) => app.promptText(l, v), confirm: (t, ok, danger) => app.confirm(t, ok, danger),
      get lock() { return app.lock; }, set lock(v: AxisLock) { app.lock = v; },
      faceNormal: (ev, at) => app.faceNormal(ev, at),
      targets: (accept) => [...app.selection].filter(id => !accept || accept(id)),
      pickTarget: (ev, opts) => {
        const t = new Set(opts?.anywhere ? [] : [...app.selection].filter(id => !opts?.accept || opts.accept(id)));
        // with `anywhere`, `accept` alone decides (what it turns down is see-through)
        const anyOk = (id: string) => !opts?.anywhere || !opts.accept || opts.accept(id);
        return app.scene.pick(ev, { facesOnly: opts?.facesOnly, only: (id) => app.inDocument(id) && anyOk(id) && (!t.size || t.has(app.resolve(id))) });
      },
      inDocument: (id) => app.inDocument(id),
      enterGroup: (id: string) => app.enterGroup(app.resolve(id) === id ? id : app.resolve(id)),
    } as ToolContext;
  }
  private pluginHost(): PluginHost {
    return {
      ctx: this.ctx, document: () => this.doc, ops: (o, l) => this.commitOps(o, l), select: (ids) => this.select(ids), selection: () => this.selection,
      setTool: (id) => this.setTool(id), toast: (t) => this.toast(t), refresh: () => this.refreshUI(), prompt: (l, v) => this.promptText(l, v), confirm: (t, ok, danger) => this.confirm(t, ok, danger),
      setHint: (t) => { this.els.statusbar.querySelector('.hint')!.textContent = t; },
      mount: (el) => this.els.viewport.appendChild(el),
      setCameraController: (c) => this.setCameraController(c), cameraController: () => this.camCtl,
      simulatePointer: (kind, x, y, button = 0) => this.simulatePointer(kind, x, y, button), canvas: () => this.scene.renderer.domElement,
      on: (ev, h) => { (this.listeners[ev] ??= new Set()).add(h); return () => this.listeners[ev].delete(h); },
      registerCommand: (name, fn) => { this.commands.set(name, fn); },
    };
  }
  private emit(ev: string, d: any) { this.listeners[ev]?.forEach(h => { try { h(d); } catch (e) { console.error(e); } }); }

  /** SketchUp-like inference. Point snaps first (endpoint, guide point, midpoint, origin, intersections,
   *  on edge), then inference lines (the axes and parallels/perpendiculars from the previous point, the
   *  axes from points the cursor rested on, guide lines), then on-face, then the work plane with the
   *  grid. The arrow keys lock an axis; Shift holds whatever inference is under the cursor. */
  private lastSnap: Snap | null = null; private snapAsked = false; private lastPlane: Plane = GROUND;
  /** Points and edges the cursor rested on or clicked lately: "from point", parallel and perpendicular inference starts there. */
  private refPoints: THREE.Vector3[] = []; private refEdges: { dir: THREE.Vector3; id: string }[] = [];
  private dwell: { point: THREE.Vector3; id?: string; since: number } | null = null;
  /** The inference held with Shift: a line (direction through an origin) or a plane. */
  private shiftLock: { kind: SnapKind; label: string; color: number; origin?: THREE.Vector3; dir?: THREE.Vector3; axis?: 'x' | 'y' | 'z'; plane?: Plane } | null = null;
  private lastFrom: THREE.Vector3 | null = null;
  snap(ev: { clientX: number; clientY: number }, opts: SnapOptions = {}): Snap { const r = this.snapInner(ev, opts); this.lastSnap = r; this.lastFrom = opts.from?.clone() ?? null; this.snapAsked = true; this.encourage(r); return r; }
  /** A point or edge the cursor rests on for a moment becomes an inference reference (SketchUp's "encouraging" an inference). */
  private encourage(s: Snap) {
    const pointy = ['vertex', 'midpoint', 'center', 'intersection', 'origin'].includes(s.kind) || (s.kind === 'guide' && !s.dir);
    const edgy = (s.kind === 'edge' || s.kind === 'midpoint') && !!s.dir && !!s.id;
    if (!pointy && !edgy) { this.dwell = null; return; }
    const now = performance.now();
    if (this.dwell && this.dwell.id === s.id && this.dwell.point.distanceTo(s.point) < 1e-6) { if (now - this.dwell.since > 350) this.remember(s); return; }
    this.dwell = { point: s.point.clone(), id: s.id, since: now };
  }
  /** Keep a clicked or rested-on snap as an inference reference (the newest four points, two edges). */
  private remember(s: Snap) {
    if (!this.refPoints.some(p => p.distanceTo(s.point) < 1e-6)) { this.refPoints.unshift(s.point.clone()); this.refPoints.splice(4); }
    if (s.dir && s.id && (s.kind === 'edge' || s.kind === 'midpoint') && !this.refEdges.some(e => e.id === s.id)) { this.refEdges.unshift({ dir: s.dir.clone().normalize(), id: s.id }); this.refEdges.splice(2); }
  }
  private snapInner(ev: { clientX: number; clientY: number }, opts: SnapOptions = {}): Snap {
    const sc = this.scene; const rect = sc.container.getBoundingClientRect(); const cursor = new THREE.Vector2(ev.clientX - rect.left, ev.clientY - rect.top);
    const ignore = opts.ignore ?? new Set<string>(); const isIgnored = (id: string) => (ignore.size > 0 && (ignore.has(id) || ignore.has(this.resolve(id)))) || !!opts.ignoreIf?.(id); // resolve() walks the document: only when something is ignored
    const px = (p: THREE.Vector3) => sc.screenPoint(p).distanceTo(cursor);
    const from = opts.from; const sl = this.shiftLock;
    const plane = sl?.plane ?? opts.plane ?? GROUND; this.lastPlane = plane;
    // a fingertip is far less precise than a mouse: snap from further away
    const k = ((ev as { pointerType?: string }).pointerType === 'touch' ? 2.4 : 1) * (opts.reach ?? 1);
    const step = () => (sc.cam.range > 60 ? 0.1 : 0.01);
    type Ray = { p: THREE.Vector3; d: THREE.Vector3; kind: SnapKind; axis?: 'x' | 'y' | 'z'; color: number; ref?: THREE.Vector3; id?: string };
    // the line the point is held on: an axis (arrow keys) or the inference Shift holds
    const held: Ray | null = opts.lock && from ? { p: from, d: AXIS[opts.lock], kind: 'axis', axis: opts.lock, color: AXIS_COLOR[opts.lock], ref: from }
      : sl?.dir && (sl.origin ?? from) ? { p: (sl.origin ?? from)!, d: sl.dir, kind: sl.kind, axis: sl.axis, color: sl.color, ref: sl.origin ?? from } : null;
    const onHeld = (p: THREE.Vector3) => held!.p.clone().addScaledVector(held!.d, p.clone().sub(held!.p).dot(held!.d));
    const onPlane = (p: THREE.Vector3) => p.clone().sub(plane.normal.clone().multiplyScalar(p.clone().sub(plane.origin).dot(plane.normal)));
    const guides = sc.compiled?.guides ?? [];
    const lineLine = (a: Ray, b: Ray): THREE.Vector3 | null => {
      const w = a.p.clone().sub(b.p); const bb = a.d.dot(b.d); const den = 1 - bb * bb; if (den < 1e-6) return null;
      const d1 = a.d.dot(w), e1 = b.d.dot(w); const s = (bb * e1 - d1) / den, t = (e1 - bb * d1) / den;
      const P = a.p.clone().addScaledVector(a.d, s), Q = b.p.clone().addScaledVector(b.d, t);
      return P.distanceTo(Q) < 1e-4 ? P.lerp(Q, 0.5) : null;
    };
    const axisLike = (d: THREE.Vector3) => Math.abs(d.x) > 0.9999 || Math.abs(d.y) > 0.9999 || Math.abs(d.z) > 0.9999;
    // inference lines
    const rays: Ray[] = [];
    if (from && !held) {
      for (const ax of ['x', 'y', 'z'] as const) rays.push({ p: from, d: AXIS[ax], kind: 'axis', axis: ax, color: AXIS_COLOR[ax], ref: from });
      for (const e of this.refEdges) {
        if (isIgnored(e.id)) continue;
        if (!axisLike(e.dir)) rays.push({ p: from, d: e.dir, kind: 'parallel', color: MAGENTA, ref: from });
        const pd = e.dir.clone().cross(plane.normal); if (pd.lengthSq() > 1e-6 && !axisLike(pd.normalize())) rays.push({ p: from, d: pd, kind: 'perpendicular', color: MAGENTA, ref: from });
      }
    }
    for (const r of this.refPoints) { if (from && r.distanceTo(from) < 1e-6) continue; for (const ax of ['x', 'y', 'z'] as const) rays.push({ p: r, d: AXIS[ax], kind: 'from', axis: ax, color: AXIS_COLOR[ax], ref: r }); }
    for (const g of guides) if (g.direction && !isIgnored(g.id)) rays.push({ p: V(g.point), d: V(g.direction).normalize(), kind: 'guide', color: COLOR.cyan, id: g.id });
    // a shape keeps to its plane: a line along the plane's normal would only lead off it
    if (opts.inPlane) for (let i = rays.length - 1; i >= 0; i--) if (Math.abs(rays[i].d.dot(plane.normal)) > 0.999) rays.splice(i, 1);
    const notFrom = (p: THREE.Vector3) => !from || p.distanceTo(from) > 1e-3;
    // geometry hidden behind a face is no snap (a layer inside a wall, the far side of a box), as in SketchUp; X-ray shows it all
    let underHit: ScenePick | null | undefined;
    const under = () => (underHit === undefined ? (underHit = sc.pickFace(ev)) : underHit);
    // (not while something is left out, such as the geometry being moved: it is still in the depth picture)
    // also while moving or pulling: the moved object still stands at its old place in the depth picture, which only
    // hides what is right behind it; without this every corner behind a wall would grab the point
    const seen = (p: THREE.Vector3) => !sc.hidden(p);
    const refOf = (r: Ray) => (r.ref ? [{ from: r.ref, color: r.color }] : []);
    // 1. points: endpoint, guide point, midpoint, origin
    let best: Snap | null = null;
    const v = sc.nearestVertex(cursor.x, cursor.y, 10 * k, seen); if (v) best = { point: v.point, kind: 'vertex' };
    if (!best) for (const g of guides) { if (g.direction || isIgnored(g.id)) continue; const p = V(g.point); if (px(p) < 9 * k) { best = { point: p, kind: 'guide', id: g.id }; break; } }
    if (!best) { const m = sc.nearestMidpoint(cursor.x, cursor.y, 9 * k, isIgnored, seen); if (m) best = { point: m.point, kind: 'midpoint', id: m.id, dir: m.dir, origin: m.point }; }
    if (!best) for (const c of sc.compiled?.centers ?? []) { const p = V(c.point); if (px(p) < 9 * k && !isIgnored(c.id) && seen(p)) { best = { point: p, kind: 'center', id: c.id, normal: V(c.normal) }; break; } }
    if (!best && px(new THREE.Vector3()) < 8 * k) best = { point: new THREE.Vector3(), kind: 'origin' };
    if (held) {
      // held on a line (an arrow-key axis or the inference Shift holds): the point follows the cursor along the line,
      // smoothly. It only stops where the line meets an inference line or an edge, or level with a corner, midpoint or
      // centre right under the cursor (dotted line from it). Faces and nearby edges don't pull it about.
      let bd = 7 * k; let hit: Snap | null = null;
      for (const r of rays) { const X = lineLine(held, r); if (!X || !notFrom(X)) continue; const d = px(X); if (d < bd) { bd = d; hit = { point: X, kind: held.kind, axis: held.axis, dir: held.d, origin: held.p, refs: refOf(r) }; } }
      const h = sc.lineEdgeHit(held.p, held.d, cursor.x, cursor.y, bd, isIgnored, seen); if (h && notFrom(h.point)) hit = { point: h.point, kind: held.kind, axis: held.axis, dir: held.d, origin: held.p, id: h.id };
      if (hit) return hit;
      const near = (): THREE.Vector3 | null => {
        const v = sc.nearestVertex(cursor.x, cursor.y, 6 * k, seen); if (v) return v.point;
        const m = sc.nearestMidpoint(cursor.x, cursor.y, 6 * k, isIgnored, seen); if (m) return m.point;
        for (const c of sc.compiled?.centers ?? []) { const p = V(c.point); if (px(p) < 6 * k && !isIgnored(c.id) && seen(p)) return p; }
        return null;
      };
      const pt = near();
      if (pt) { const q = onHeld(pt); if (notFrom(q)) return { point: q, kind: held.kind, axis: held.axis, dir: held.d, origin: held.p, refs: q.distanceTo(pt) > 1e-6 ? [{ from: pt, color: held.color }] : undefined }; }
      // free along the line, the length rounded to the grid step
      let q = sc.linePoint(ev, held.p, held.d);
      if (this.options.gridSnap && from) { const g = step(); const L = Math.round(q.clone().sub(from).dot(held.d) / g) * g; q = from.clone().addScaledVector(held.d, L); }
      return { point: q, kind: held.kind, axis: held.axis, dir: held.d, origin: held.p };
    }
    // 2. crossings: inference lines with each other and with edges
    if (!best) {
      let bd = 8 * k;
      for (let i = 0; i < rays.length; i++) for (let j = i + 1; j < rays.length; j++) {
        const X = lineLine(rays[i], rays[j]); if (!X || !notFrom(X) || X.distanceTo(rays[i].p) < 1e-6 || X.distanceTo(rays[j].p) < 1e-6) continue; // a reference point itself is no intersection
        const d = px(X); if (d >= bd) continue;
        const ax = rays[i].kind === 'axis' ? rays[i].axis : rays[j].kind === 'axis' ? rays[j].axis : undefined;
        bd = d; best = { point: X, kind: 'intersection', axis: ax, refs: [...refOf(rays[i]), ...refOf(rays[j])] };
      }
      for (const r of rays) {
        if (px(sc.linePoint(ev, r.p, r.d)) > 6 * k) continue;
        const h = sc.lineEdgeHit(r.p, r.d, cursor.x, cursor.y, bd, isIgnored, seen); if (!h || !notFrom(h.point)) continue;
        bd = h.d; best = { point: h.point, kind: 'intersection', id: h.id, axis: r.kind === 'axis' ? r.axis : undefined, refs: refOf(r) };
      }
    }
    // 3. on edge
    if (!best) { const e = sc.nearestEdge(ev, cursor.x, cursor.y, 7 * k, isIgnored, seen); if (e) best = { point: e.point, kind: 'edge', id: e.id, dir: e.dir, origin: e.point }; }
    if (best) return sl?.plane ? { ...best, point: onPlane(best.point) } : best;
    // 4. inference lines
    let bd = 6 * k; let ray: { r: Ray; q: THREE.Vector3 } | null = null;
    for (const r of rays) { const q = sc.linePoint(ev, r.p, r.d); if (r.kind !== 'guide' && (!notFrom(q) || q.distanceTo(r.p) < 1e-6)) continue; const d = px(q); if (d < bd) { bd = d; ray = { r, q }; } } // the reference point itself is no inference
    if (ray) {
      const { r } = ray; let q = ray.q;
      if (from && this.options.gridSnap && (r.kind === 'axis' || r.kind === 'parallel' || r.kind === 'perpendicular')) { const g = step(); const L = Math.round(q.clone().sub(from).dot(r.d) / g) * g; if (Math.abs(L) > 1e-9) q = from.clone().addScaledVector(r.d, L); }
      return { point: sl?.plane ? onPlane(q) : q, kind: r.kind, axis: r.axis, dir: r.d, origin: r.p, id: r.id, refs: r.kind === 'axis' ? undefined : refOf(r) };
    }
    // 5. on face
    if (opts.faces !== false && !sl?.plane) {
      const pk = under();
      if (pk && !isIgnored(pk.id)) return { point: pk.point.clone(), kind: 'face', id: pk.id, normal: pk.back ? pk.normal.clone().negate() : pk.normal.clone() };
    }
    // 6. the work plane, rounded to the grid step and pulled onto the visible 1 m grid lines
    let p = sc.planePoint(ev, plane.origin, plane.normal) ?? from?.clone() ?? new THREE.Vector3();
    let kind: SnapKind = 'plane';
    if (this.options.gridSnap) {
      // (counted along the drawing axes, from their origin: the grid lies there)
      const g = step(); const q = toAxes(p); q.set(Math.round(q.x / g) * g, Math.round(q.y / g) * g, Math.round(q.z / g) * g);
      if (this.display.grid) {
        const G = 1; const gx = Math.round(q.x / G) * G, gz = Math.round(q.z / G) * G; let onGrid = false;
        if (px(fromAxes(new THREE.Vector3(gx, q.y, q.z))) < 5) { q.x = gx; onGrid = true; }
        if (px(fromAxes(new THREE.Vector3(q.x, q.y, gz))) < 5) { q.z = gz; onGrid = true; }
        if (onGrid) kind = 'grid';
      }
      p.copy(fromAxes(q));
    }
    return { point: p, kind };
  }
  /** Shift: hold the inference under the cursor — a direction (axis, edge, parallel, guide, from point) or a face's plane. */
  private holdInference(): boolean {
    if (this.lock || this.shiftLock) return false;
    const own = this.tool.onShiftHold?.(this.ctx) ?? null;
    if (own) { this.shiftLock = { kind: 'axis', color: 0xc8169a, label: own }; this.updateLockBadge(); return true; }
    const s = this.lastSnap; if (!s || !this.snapAsked) return false;
    // a point or crossing that lies on an axis through the start point: hold that axis (SketchUp keeps the axis the line is on)
    const from = this.lastFrom; const axisOf = (): 'x' | 'y' | 'z' | null => {
      if (!from) return null; if (s.axis && ['intersection', 'vertex', 'midpoint', 'center'].includes(s.kind)) return s.axis;
      const d = s.point.clone().sub(from); const L = d.length(); if (L < 1e-4) return null;
      for (const ax of ['x', 'y', 'z'] as const) if (Math.abs(d.clone().divideScalar(L).dot(AXIS[ax])) > 0.99985) return ax; // within 1°
      return null;
    };
    const ax = ['axis', 'from', 'parallel', 'perpendicular', 'edge', 'guide'].includes(s.kind) && s.dir ? null : axisOf();
    if (ax && from) { this.shiftLock = { kind: 'axis', axis: ax, dir: AXIS[ax].clone(), origin: from.clone(), color: AXIS_COLOR[ax], label: `Holding: ${snapLabel({ point: s.point, kind: 'axis', axis: ax })}` }; this.updateLockBadge(); this.tool.onLock?.(this.ctx); return true; }
    if (['axis', 'from', 'parallel', 'perpendicular', 'edge', 'guide'].includes(s.kind) && s.dir) this.shiftLock = { kind: s.kind, axis: s.axis, dir: s.dir.clone().normalize(), origin: (s.origin ?? s.point).clone(), color: snapColor(s), label: `Holding: ${snapLabel(s)}` };
    else if (s.kind === 'face' && s.normal) this.shiftLock = { kind: 'face', plane: { origin: s.point.clone(), normal: s.normal.clone() }, color: snapColor(s), label: 'Holding the face plane' };
    else if (s.kind === 'plane' || s.kind === 'grid') this.shiftLock = { kind: 'plane', plane: { origin: this.lastPlane.origin.clone(), normal: this.lastPlane.normal.clone() }, color: 0x6b7280, label: 'Holding the work plane' };
    else return false;
    this.updateLockBadge(); this.tool.onLock?.(this.ctx); return true;
  }
  /** A clicked point is a reference for the next ones (the rectangle's first corner, a line's start…). */
  private rememberClick() { if (this.snapAsked && this.lastSnap && this.lastSnap.kind !== 'free') this.remember(this.lastSnap); }
  private releaseInference() { if (!this.shiftLock) return; this.shiftLock = null; this.updateLockBadge(); this.tool.onLock?.(this.ctx); }
  private updateLockBadge() {
    const b = this.els['lock-badge']; const sl = this.shiftLock;
    const axis = this.lock ? ({ x: 'red', y: 'blue', z: 'green' })[this.lock] : '';
    const text = this.lock ? (this.tool.planeLock ? `Drawing plane faces the ${axis} axis` : `Locked to ${axis} axis`) : sl?.label ?? ''; const color = this.lock ? AXIS_COLOR[this.lock] : sl?.color ?? 0x888888;
    b.classList.toggle('show', !!text); if (text) { b.textContent = text; b.style.background = `#${color.toString(16).padStart(6, '0')}`; }
  }

  // ---- MCP / live commands -----------------------------------------------------------------------
  private async handleCommand(name: string, a: any): Promise<any> {
    const cam = this.scene.cam;
    switch (name) {
      case 'screenshot': { if (a.view) { this.setView(a.view); await new Promise(r => setTimeout(r, 450)); } return this.scene.screenshot(a.width, a.height); }
      case 'set_camera': cam.restore([...a.position, ...a.target, ...(a.up ?? [0, 1, 0])], false); return { camera: cam.serialize() };
      case 'set_view': this.setView(a.view); await new Promise(r => setTimeout(r, 400)); return { camera: cam.serialize() };
      case 'zoom_extents': this.zoomExtents(); await new Promise(r => setTimeout(r, 400)); return { camera: cam.serialize() };
      case 'zoom_to': this.zoomTo(a.ids ?? []); await new Promise(r => setTimeout(r, 400)); return { camera: cam.serialize() };
      case 'orbit': cam.orbit(a.dx ?? 0, a.dy ?? 0); return { camera: cam.serialize() };
      case 'pan': cam.pan(a.dx ?? 0, a.dy ?? 0, this.scene.height); return { camera: cam.serialize() };
      case 'zoom': cam.dolly(1 / (a.factor ?? 1.2)); return { camera: cam.serialize() };
      case 'select': this.select(a.ids ?? [], a.mode ?? 'replace'); return { selection: [...this.selection] };
      case 'activate_tool': this.setTool(a.tool); return { tool: this.tool.id, hint: this.tool.hint };
      case 'get_state': return this.state();
      case 'previous_view': this.previousView(); await new Promise(r => setTimeout(r, 400)); return { camera: cam.serialize() };
      case 'next_view': this.nextView(); await new Promise(r => setTimeout(r, 400)); return { camera: cam.serialize() };
      case 'zoom_window': this.setCameraController(null); this.rememberView(); zoomWindow(this.ctx, a.x0 ?? 0, a.y0 ?? 0, a.x1 ?? 1, a.y1 ?? 1); await new Promise(r => setTimeout(r, 450)); this.rememberView(); return { camera: cam.serialize() };
      case 'set_options': for (const k of ['grid', 'axes', 'edges', 'figure', 'shadows', 'sectionPlanes'] as const) { const key = ({ grid: 'gridVisible', axes: 'axesVisible', edges: 'edgesVisible', figure: 'figureVisible', shadows: 'shadows', sectionPlanes: 'sectionPlanesVisible' })[k]; if (a[key] !== undefined && !!a[key] !== this.display[k]) this.toggleDisplay(k); }
        if (Array.isArray(a.xray)) { this.scene.setXray(a.xray.map(String)); this.emit('xray', this.scene.xrayMaterials); }
        if (a.faceStyle !== undefined) { if (!FACE_STYLES.some(s => s.id === a.faceStyle)) throw new Error(`unknown face style ${a.faceStyle}`); this.setFaceStyle(a.faceStyle); }
        if (a.backEdges !== undefined) this.setBackEdges(!!a.backEdges);
        if (a.hiddenGeometry !== undefined && !!a.hiddenGeometry !== this.hiddenGeometry) await this.setHiddenGeometry(!!a.hiddenGeometry);
        return { ...this.display, xray: this.scene.xrayMaterials, faceStyle: this.faceStyle, backEdges: this.backEdges, hiddenGeometry: this.hiddenGeometry };
      case 'open_project': await this.openProject(a.id); return { project: this.projectId, name: this.projectName, version: this.doc.version };
      case 'get_document': return this.doc;
      case 'ops': { const r = await this.commitOps(a.ops ?? [], a.label ?? 'Ops'); return { results: r, version: this.doc.version }; }
      default: { const fn = this.commands.get(name); if (!fn) throw new Error(`unknown viewport command ${name}`); return await fn(a ?? {}); }
    }
  }
  /** The embed bridge: the page that embeds the viewer (an iframe) sends `{ type: 'plannr', id, command, args }`
   *  and gets `{ type: 'plannr', id, result | error }` back; events (`ready`, `selection`, `changed`, `tool`)
   *  arrive as `{ type: 'plannr', event, … }`. `?origin=<https://host>` restricts who may talk to it. */
  private embedOrigin: string | null = null;
  private installEmbedBridge() {
    this.embedOrigin = new URLSearchParams(location.search).get('origin');
    window.addEventListener('message', async (ev) => {
      const m = ev.data; if (!m || m.type !== 'plannr' || typeof m.command !== 'string') return;
      if (this.embedOrigin && ev.origin !== this.embedOrigin) return;
      const reply = (body: any) => (ev.source as Window | null)?.postMessage({ type: 'plannr', id: m.id, ...body }, ev.origin || '*');
      try { reply({ result: await this.handleCommand(m.command, m.args ?? {}) }); } catch (e: any) { reply({ error: String(e?.message ?? e) }); }
    });
    this.listeners.selection.add(ids => this.post({ event: 'selection', ids }));
    this.listeners.changed.add(doc => this.post({ event: 'changed', version: doc.version }));
    this.listeners.tool.add(tool => this.post({ event: 'tool', tool }));
  }
  private post(body: any) { if (!this.embed || window.parent === window) return; window.parent.postMessage({ type: 'plannr', ...body }, this.embedOrigin ?? '*'); }
  private state() { return { camera: this.scene.cam.serialize(), selection: [...this.selection], tool: this.tool.id, hover: this.hover, version: this.doc.version, enteredGroup: this.enteredGroup, display: this.display }; }
  private sendState() { this.live?.sendState({ camera: this.scene.cam.serialize(), selection: [...this.selection], tool: this.tool.id, hover: this.hover }); }

  // ---- input ---------------------------------------------------------------------------------------
  private bindInput() {
    const canvas = this.scene.renderer.domElement; canvas.tabIndex = 0;
    let mid: { x: number; y: number; pan: boolean; x0: number; y0: number } | null = null;
    canvas.addEventListener('contextmenu', e => e.preventDefault());
    // a text label is clicked like the thing it is: select it, or open its menu
    const labelId = (e: Event) => (e.target as HTMLElement).closest<HTMLElement>('.vlabel.static[data-id]')?.dataset.id;
    this.els.labels.addEventListener('click', e => { const id = labelId(e); if (id && this.tool.id === 'select') this.select([id], e.shiftKey ? 'add' : 'replace'); });
    this.els.labels.addEventListener('contextmenu', e => { const id = labelId(e); if (!id) return; e.preventDefault(); this.contextMenu(e, id); });
    // touch: one finger drives the tool, and a press-drag-release completes a two-click tool (rectangle,
    // circle, line, push/pull, move…); two fingers orbit (drag) and zoom (pinch), three pan — and a gesture
    // never cancels what the tool is in the middle of. The first finger is held back for a moment so the
    // start of a two-finger gesture doesn't place a point.
    const touches = new Map<number, { x: number; y: number }>(); let gesture: { n: number; cx: number; cy: number; dist: number } | null = null;
    let pending: { ev: PointerEvent; timer: ReturnType<typeof setTimeout> } | null = null;
    let stroke: { x: number; y: number; started: boolean; moved: boolean } | null = null;
    let ignoring = false; // fingers left over from a gesture
    canvas.style.touchAction = 'none';
    const centroid = () => { let cx = 0, cy = 0, n = 0; for (const t of touches.values()) { cx += t.x; cy += t.y; n++; } const pts = [...touches.values()]; const dist = pts.length >= 2 ? Math.hypot(pts[0].x - pts[1].x, pts[0].y - pts[1].y) : 0; return { n, cx: cx / n, cy: cy / n, dist }; };
    const deliverDown = (ev: PointerEvent) => {
      const before = !!this.tool.busy?.();
      if (this.camCtl?.onPointerDown?.(ev)) { stroke = null; return; }
      this.tool.onMove?.(ev, this.ctx); this.tool.onDown?.(ev, this.ctx); this.rememberClick();
      stroke = { x: ev.clientX, y: ev.clientY, started: !before && !!this.tool.busy?.(), moved: false };
      this.updateTouchBar();
    };
    const flush = () => { if (!pending) return; clearTimeout(pending.timer); const ev = pending.ev; pending = null; deliverDown(ev); };
    canvas.addEventListener('pointerdown', e => {
      canvas.focus(); this.pointerDown = true;
      if (e.pointerType === 'touch') {
        this.touchUsed = true;
        touches.set(e.pointerId, { x: e.clientX, y: e.clientY }); try { canvas.setPointerCapture(e.pointerId); } catch { /* synthetic */ }
        if (touches.size >= 2) {
          // navigation: a first finger still held back never reaches the tool; a point it placed a moment ago is taken back
          if (pending) { clearTimeout(pending.timer); pending = null; }
          else if (stroke?.started && !stroke.moved) this.tool.onKey?.(new KeyboardEvent('keydown', { key: 'Escape' }), this.ctx);
          stroke = null; gesture = centroid(); this.scene.setSnapMarker(null); this.updateTouchBar(); return;
        }
        if (ignoring) return;
        pending = { ev: e, timer: setTimeout(flush, 110) };
        return;
      }
      if (this.camCtl?.onPointerDown?.(e)) { try { canvas.setPointerCapture(e.pointerId); } catch { /* synthetic */ } e.preventDefault(); return; }
      if (e.button === 1 || e.button === 2) { mid = { x: e.clientX, y: e.clientY, pan: e.shiftKey, x0: e.clientX, y0: e.clientY }; canvas.setPointerCapture(e.pointerId); e.preventDefault(); return; }
      this.tool.onDown?.(e, this.ctx); this.rememberClick();
    });
    canvas.addEventListener('pointermove', e => {
      if (e.pointerType === 'touch' && touches.has(e.pointerId)) {
        touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
        if (gesture) {
          const g = centroid();
          if (g.n >= 3) this.scene.cam.pan(g.cx - gesture.cx, g.cy - gesture.cy, this.scene.height);
          else { this.scene.cam.orbit(g.cx - gesture.cx, g.cy - gesture.cy); if (gesture.dist > 0 && g.dist > 0) this.scene.cam.dolly(gesture.dist / g.dist); }
          gesture = g; return;
        }
        if (ignoring) return;
        if (pending) { if (Math.hypot(e.clientX - pending.ev.clientX, e.clientY - pending.ev.clientY) > 8) flush(); else return; }
        if (stroke && Math.hypot(e.clientX - stroke.x, e.clientY - stroke.y) > 12) stroke.moved = true;
      }
      if (this.camCtl?.onPointerMove?.(e)) return;
      if (mid) { mid.pan || e.shiftKey ? this.scene.cam.pan(e.clientX - mid.x, e.clientY - mid.y, this.scene.height) : this.scene.cam.orbit(e.clientX - mid.x, e.clientY - mid.y); mid.x = e.clientX; mid.y = e.clientY; return; }
      // every tool that asked for a snap gets a visible marker, also before its first click
      this.lastMove = e; this.replayMove(e);
    });
    canvas.addEventListener('pointerleave', e => { if (e.pointerType !== 'touch') this.scene.setSnapMarker(null); });
    canvas.addEventListener('pointerup', e => {
      if (e.pointerType === 'touch') {
        touches.delete(e.pointerId);
        if (gesture) { if (touches.size < 2) { gesture = null; this.sendState(); this.rememberViewSoon(); } if (touches.size > 0) ignoring = true; this.pointerDown = touches.size > 0; return; }
        if (ignoring) { if (touches.size === 0) { ignoring = false; this.pointerDown = false; } return; }
        flush(); // a quick tap
        this.pointerDown = false;
        if (this.camCtl?.onPointerUp?.(e)) { stroke = null; return; }
        this.tool.onUp?.(e, this.ctx);
        // press-drag-release finishes what the press started: the second click
        if (stroke?.started && stroke.moved && this.tool.busy?.()) { this.tool.onMove?.(e, this.ctx); this.tool.onDown?.(e, this.ctx); this.tool.onUp?.(e, this.ctx); }
        stroke = null; this.scene.setSnapMarker(null); this.updateTouchBar();
        return;
      }
      this.pointerDown = false;
      if (this.camCtl?.onPointerUp?.(e)) return; if (mid && (e.button === 1 || e.button === 2)) {
        // the right button without a drag: the context menu (a drag orbits)
        const still = e.button === 2 && Math.hypot(e.clientX - mid.x0, e.clientY - mid.y0) < 4; mid = null;
        if (still) this.contextMenu(e); else { this.sendState(); this.rememberViewSoon(); }
        return;
      }
      this.tool.onUp?.(e, this.ctx); });
    canvas.addEventListener('pointercancel', e => {
      this.pointerDown = false; touches.delete(e.pointerId);
      if (pending) { clearTimeout(pending.timer); pending = null; }
      if (touches.size < 2) gesture = null; if (touches.size === 0) { ignoring = false; stroke = null; }
    });
    // narrow screens: the tray becomes an overlay toggled from the menu bar
    const narrow = () => document.getElementById('app')!.classList.toggle('narrow', window.innerWidth < 980);
    window.addEventListener('resize', narrow); narrow();
    canvas.addEventListener('dblclick', e => this.tool.onDblClick?.(e, this.ctx));
    canvas.addEventListener('wheel', e => { e.preventDefault(); if (this.camCtl?.onWheel?.(e)) return; this.rememberViewSoon(); const towards = this.scene.pick(e, { facesOnly: true })?.point ?? this.scene.groundPoint(e) ?? undefined; this.scene.cam.dolly(Math.pow(1.0015, e.deltaY), towards); }, { passive: false });
    const vcb = this.els.statusbar.querySelector<HTMLInputElement>('.vcb input')!;
    vcb.addEventListener('keydown', e => { e.stopPropagation(); if (e.key === 'Enter') { this.tool.onVCB?.(typedLength(vcb.value.trim(), this.els.statusbar.querySelector('.vcb label')?.textContent ?? ''), this.ctx); vcb.blur(); canvas.focus(); } if (e.key === 'Escape') { vcb.blur(); canvas.focus(); } });
    window.addEventListener('keydown', e => {
      const t = e.target as HTMLElement; if (t && (t.tagName === 'INPUT' || t.tagName === 'SELECT' || t.tagName === 'TEXTAREA')) return;
      if (this.camCtl?.onKey?.(e, true)) { e.preventDefault(); return; }
      if ((e.key === 'Control' || e.key === 'Alt' || e.key === 'Shift') && !e.repeat) {
        const others = [e.ctrlKey, e.altKey, e.shiftKey, e.metaKey].filter(Boolean).length; // only a lone modifier starts a temporary tool
        if (e.key === 'Shift' && others === 1 && this.holdInference()) { e.preventDefault(); return; }
        if (others === 1 && this.beginTempTool(e.key as ModKey)) { e.preventDefault(); return; }
        if (this.tool.onKey?.(e, this.ctx)) return; // e.g. Move toggles copy with Ctrl mid-operation
        return;
      }
      if (this.temp && !e.repeat) { this.endTempTool(); } // any other key cancels the temporary tool so shortcuts behave normally
      const mod = e.metaKey || e.ctrlKey; const k = e.altKey && e.code.startsWith('Key') ? e.code.slice(3).toLowerCase() : e.key.toLowerCase(); // ⌥+letter works as the letter (walking mode uses bare letters to move)
      if (mod && k === 'z') { e.preventDefault(); e.shiftKey ? this.redo() : this.undo(); return; }
      if (mod && k === 'a') { e.preventDefault(); this.selectAll(); return; }
      if (mod && k === 'c' && this.selection.size) { e.preventDefault(); this.copySelection(); return; }
      if (mod && k === 'x' && this.selection.size) { e.preventDefault(); this.cutSelection(); return; }
      if (mod && k === 'v') { e.preventDefault(); this.paste(e.shiftKey); return; }
      if (mod && k === 'e') { e.preventDefault(); e.shiftKey ? this.unhideAll() : this.hideSelection(); return; }
      if (e.altKey && !mod && /^Digit[1-5]$/.test(e.code)) { e.preventDefault(); this.setFaceStyle(FACE_STYLES[+e.code.slice(5) - 1].id); return; }
      if (mod && (k === 'n' || k === 'o')) { e.preventDefault(); this.showWelcome(); return; }
      if (mod && e.key === '[') { e.preventDefault(); this.previousView(); return; }
      if (mod && e.key === ']') { e.preventDefault(); this.nextView(); return; }
      if (mod) { this.tool.onKey?.(e, this.ctx); return; }
      if (e.key === 'ArrowRight') { e.preventDefault(); this.setLock('x'); return; }
      if (e.key === 'ArrowLeft') { e.preventDefault(); this.setLock('z'); return; }
      if (e.key === 'ArrowUp') { e.preventDefault(); this.setLock('y'); return; }
      if (e.key === 'ArrowDown') { e.preventDefault(); this.lock = null; this.setLock(null); return; }
      if (this.tool.onKey?.(e, this.ctx)) { e.preventDefault(); return; }
      if (e.key === 'Delete' || e.key === 'Backspace') { e.preventDefault(); this.deleteSelection(); return; }
      if (e.key === 'Escape') { if (this.lock) { this.setLock(null); return; } if (this.selection.size) this.select([]); else this.exitGroup(); return; }
      if (e.shiftKey && k === 'z') { e.preventDefault(); this.zoomExtents(); return; }
      if (k === 'g' && this.selection.size) { this.groupSelection(); return; }
      if (/^[\d.,;\-\[<*\/]$/.test(e.key)) { vcb.focus(); vcb.value = ''; return; } // (also "[x;y;z]", "<x;y;z>", "*3", "/3")
      if (e.key === ' ') { e.preventDefault(); this.setTool('select'); return; }
      const t2 = this.tools.find(t => t.shortcut === k); if (t2) this.setTool(t2.id);
    });
    window.addEventListener('keyup', e => {
      if (this.camCtl?.onKey?.(e, false)) return;
      if (e.key === 'Shift') this.releaseInference();
      if (e.key === 'Control' || e.key === 'Alt' || e.key === 'Shift') { if (this.temp?.key === e.key) { this.endTempTool(e.key as ModKey); return; } this.tool.onKey?.(e, this.ctx); }
    });
    window.addEventListener('blur', () => { this.endTempTool(); this.releaseInference(); });
  }

  // ---- ui -----------------------------------------------------------------------------------------------
  refreshUI() { this.syncUnits(); this.syncAxes(); renderMenubar(this, this.els.menubar); renderToolbar(this, this.els.toolbar); renderHud(this, this.els['view-hud']); renderToolOptions(this, this.els['tool-options']); this.tray.render(); if (this.embed && this.embedPlugins.size) this.tray.renderFloating(this.els['embed-panel'], this.embedPlugins); }
  private toastTimer: any = null;
  toast(text: string, ms = 2600) { const t = this.els.toast; t.textContent = text; t.classList.add('show'); clearTimeout(this.toastTimer); this.toastTimer = setTimeout(() => t.classList.remove('show'), ms); }
  promptText(label: string, value = ''): Promise<string | null> { return promptText(label, value); }
  confirm(text: string, okLabel = 'OK', danger = false): Promise<boolean> { return confirmDialog(text, okLabel, danger); }
}

const app = new App(document.getElementById('app')!);
(window as any).plannr = { app, registerPlugin, getPlugins, THREE, version: '0.2.0' };
void V;
