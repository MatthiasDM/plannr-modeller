import './style.css';
import * as THREE from 'three';
import { Viewport, A, V, type ViewName } from './scene';
import { api, Live, type CompiledScene, type PluginInfo } from './api';
import { findEntity, topLevelId, walk, type Document, type Op, type OpResult } from './model';
import { BUILTIN_TOOLS, placeComponentTool } from './tools';
import { zoomWindow } from './tools/arc';
import type { Tool, ToolContext, Snap, SnapOptions, AxisLock } from './tools/base';
import { AXIS, AXIS_COLOR, snapColor, snapLabel } from './tools/base';
import { Tray } from './ui/tray';
import { renderMenubar, renderToolbar, renderHud, renderToolOptions, renderWelcome } from './ui/chrome';
import { registerPlugin, getPlugins, onPlugin, type PluginHost, type CameraController } from './plugins';
import { promptText, confirmDialog, installTooltips } from './ui/dialog';

const emptyDoc = (): Document => ({ version: 0, units: 'm', layers: [{ id: 'layer0', name: 'Layer0', visible: true, color: '#7a8aa0' }], materials: [{ id: 'default', name: 'Default', color: '#F4F2EE', opacity: 1 }], entities: [], scenes: [], extensions: {} });

export type ModKey = 'Control' | 'Alt' | 'Shift';
export const MOD_LABEL: Record<ModKey, string> = { Control: navigator.platform.includes('Mac') ? 'Control (⌃)' : 'Ctrl', Alt: navigator.platform.includes('Mac') ? 'Option (⌥)' : 'Alt', Shift: 'Shift (⇧)' };
const DEFAULT_MODIFIER_TOOLS: Record<ModKey, string> = { Control: 'orbit', Alt: 'pan', Shift: '' };
function loadModifierTools(): Record<ModKey, string> {
  try { return { ...DEFAULT_MODIFIER_TOOLS, ...JSON.parse(localStorage.getItem('plannr.modifierTools') ?? '{}') }; } catch { return { ...DEFAULT_MODIFIER_TOOLS }; }
}

export class App {
  scene: Viewport;
  doc: Document = emptyDoc();
  projectId: string | null = null; projectName = 'Untitled';
  selection = new Set<string>();
  enteredGroup: string | null = null;
  tools: Tool[] = [...BUILTIN_TOOLS];
  tool: Tool = this.tools[0];
  options: ToolContext['options'] = { circleSegments: 24, gridSnap: true, material: 'default', layer: 'layer0' };
  display = { grid: true, axes: true, edges: true, figure: true, shadows: true };
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
  private online = false; private previewTimer: any = null; private previewSeq = 0; private compileSeq = 0;
  private listeners: Record<string, Set<(d: any) => void>> = { changed: new Set(), selection: new Set(), tool: new Set() };
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

  setCameraController(c: CameraController | null) { if (this.camCtl && this.camCtl !== c) this.camCtl.release(); this.camCtl = c; }

  constructor(root: HTMLElement) {
    root.innerHTML = `<div class="menubar"></div><div class="toolbar"></div>
      <div class="viewport"><div class="labels"></div><div class="view-hud"></div><div class="tool-options"></div><div class="lock-badge"></div><div class="toast"></div></div>
      <div class="tray"></div><div class="tray-resizer" title="Drag to resize · double-click to reset"></div>
      <div class="statusbar"><span class="conn" title="Server connection"></span><span class="hint"></span><div class="vcb"><label>Measurements</label><input type="text" spellcheck="false"></div></div><div class="welcome-host"></div>`;
    for (const k of ['menubar', 'toolbar', 'viewport', 'labels', 'view-hud', 'tool-options', 'lock-badge', 'toast', 'tray', 'statusbar', 'welcome-host']) this.els[k] = root.querySelector(`.${k}`)!;
    this.scene = new Viewport(this.els.viewport, this.els.labels);
    this.tray = new Tray(this, this.els.tray);
    this.installTrayResizer(root);
    this.ctx = this.makeContext();
    this.bindInput(); installTooltips();
    this.scene.start();
    onPlugin(p => { this.tools.push(...(p.tools ?? [])); p.activate?.(this.pluginHost()); this.refreshUI(); });
    this.refreshUI(); this.setTool('select');
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
    if (this.online && last) { try { await this.openProject(last); return; } catch { /* fall through */ } }
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
    this.live.onChanged = (version, label, source) => { if (version !== this.doc.version) this.refresh(source !== 'browser' ? `${label} (${source})` : undefined); };
    this.live.onCommand = (name, args) => this.handleCommand(name, args);
    this.live.onStatus = ok => this.setConn(ok);
    this.hideWelcome(); document.title = `${p.name} — Plannr`;
    await this.refresh(); this.zoomExtents();
  }
  /** Reload document + compiled scene from the server. */
  async refresh(notice?: string) {
    if (!this.projectId) return;
    const seq = ++this.compileSeq;
    try {
      const [p, c, h] = await Promise.all([api.getProject(this.projectId), api.compile(this.projectId), api.history(this.projectId)]);
      if (seq !== this.compileSeq) return;
      this.doc = p.document; this.projectName = p.name; this.historyList = h;
      const cur = h.findIndex(x => x.version === this.doc.version); this.canUndo = h.length > 0; this.canRedo = h.length > 0; void cur;
      this.applyCompiled(c);
      for (const id of [...this.selection]) if (!findEntity(this.doc, id)) this.selection.delete(id);
      this.scene.highlight(this.selection); this.saveState = `v${this.doc.version}`; this.online = true; this.setConn(true);
      if (notice) this.toast(notice);
    } catch (e: any) { this.online = false; this.setConn(false); this.toast(`Engine unavailable: ${e.message}`); }
    this.refreshUI(); this.emit('changed', this.doc);
  }
  private applyCompiled(c: CompiledScene) { this.scene.setCompiled(c); this.scene.highlight(this.selection); }
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
  async commitOps(ops: Op[], label: string): Promise<OpResult[]> {
    if (!this.projectId || !ops.length) return [];
    this.clearPreview();
    try {
      const r = await api.ops(this.projectId, ops, label);
      await this.refresh();
      return r.results;
    } catch (e: any) { this.toast(e.message); await this.refresh(); return []; }
  }
  preview(ops: Op[]) {
    if (!this.projectId) return;
    clearTimeout(this.previewTimer);
    if (!ops.length) { this.clearPreview(); return; }
    this.previewTimer = setTimeout(async () => {
      const seq = ++this.previewSeq;
      try { const c = await api.preview(this.projectId!, ops); if (seq === this.previewSeq && this.previewSeq > 0) this.applyCompiled(c); } catch { /* invalid intermediate state: ignore */ }
    }, 30);
  }
  private clearPreview() { clearTimeout(this.previewTimer); if (this.previewSeq > 0) { this.previewSeq = 0; if (this.projectId) api.compile(this.projectId).then(c => { if (this.previewSeq === 0) this.applyCompiled(c); }).catch(() => {}); } }
  async undo() { if (!this.projectId) return; try { const r = await api.undo(this.projectId); this.toast(`Undo ${r.label}`); await this.refresh(); } catch (e: any) { this.toast(e.message); } }
  async redo() { if (!this.projectId) return; try { const r = await api.redo(this.projectId); this.toast(`Redo ${r.label}`); await this.refresh(); } catch (e: any) { this.toast(e.message); } }

  // ---- selection / groups -------------------------------------------------------------
  resolve(id: string): string {
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
  selectAll() { this.select((this.enteredGroup ? (findEntity(this.doc, this.enteredGroup) as any)?.entities ?? [] : this.doc.entities).map((e: any) => e.id)); }
  enterGroup(id: string) { const e = findEntity(this.doc, id); const g = e?.kind === 'group' ? e : null; if (!g) return; this.enteredGroup = g.id; this.select([]); this.toast(`Editing group “${g.name}” — Esc to close`); this.refreshUI(); }
  exitGroup() { if (!this.enteredGroup) return; this.enteredGroup = null; this.select([]); this.refreshUI(); }
  async deleteSelection() { if (!this.selection.size) return; const ids = [...this.selection]; this.selection.clear(); await this.commitOps([{ op: 'erase', ids }], 'Erase'); }
  async groupSelection() { if (!this.selection.size) return; const r = await this.commitOps([{ op: 'make_group', ids: [...this.selection], name: `Group ${this.doc.entities.filter(e => e.kind === 'group').length + 1}` }], 'Make group'); if (r[0]?.created?.[0]) this.select([r[0].created[0]]); }
  async makeComponent() {
    if (!this.selection.size) return this.toast('Select the entities that make up the component first');
    const name = await this.promptText('Component name', `Component ${(this.doc.components?.length ?? 0) + 1}`); if (!name) return;
    const r = await this.commitOps([{ op: 'make_component', ids: [...this.selection], name }], 'Make component'); if (r[0]?.created?.[0]) this.select([r[0].created[0]]);
  }
  placeComponent(definition: string) { placeComponentTool.definition = definition; this.setTool('place'); }
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
  async saveScene() { const name = await this.promptText('Scene name', `Scene ${this.doc.scenes.length + 1}`); if (!name) return; const active = [...walk(this.doc.entities)].map(x => x.entity).find(e => e.kind === 'section' && e.active); await this.putScenes([...this.doc.scenes, { id: crypto.randomUUID(), name, camera: this.scene.cam.serialize(), section: active?.id ?? null }], 'Add scene'); }
  async updateScene(id: string) { await this.putScenes(this.doc.scenes.map(s => s.id === id ? { ...s, camera: this.scene.cam.serialize() } : s), 'Update scene'); }
  async removeScene(id: string) { await this.putScenes(this.doc.scenes.filter(s => s.id !== id), 'Remove scene'); }
  private async putScenes(scenes: Document['scenes'], label: string) { if (!this.projectId) return; await api.putDocument(this.projectId, { ...this.doc, scenes }); await this.refresh(label); }
  restoreScene(id: string) { const s = this.doc.scenes.find(s => s.id === id); if (!s) return; this.rememberView(); this.scene.cam.restore(s.camera); this.rememberViewSoon(); const ops: Op[] = []; for (const { entity } of walk(this.doc.entities)) if (entity.kind === 'section' && entity.active !== (entity.id === s.section)) ops.push({ op: 'update_section', id: entity.id, active: entity.id === s.section }); if (ops.length) this.commitOps(ops, 'Scene section'); }
  toggleDisplay(k: keyof App['display']) {
    this.display[k] = !this.display[k];
    this.scene.grid.visible = this.display.grid; this.scene.axes.visible = this.display.axes; this.scene.figure.visible = this.display.figure;
    this.scene.edgesVisible = this.display.edges; if (this.scene.compiled) this.scene.setCompiled(this.scene.compiled); this.scene.highlight(this.selection);
    this.scene.renderer.shadowMap.enabled = this.display.shadows; this.scene.ground.receiveShadow = this.display.shadows;
    this.refreshUI();
  }

  // ---- tools ---------------------------------------------------------------------------------
  setTool(id: string) {
    const t = this.tools.find(t => t.id === id); if (!t) return;
    this.tool.deactivate?.(this.ctx); this.scene.clearOverlay(); this.scene.setSnapMarker(null); this.setLock(null);
    this.tool = t; t.activate?.(this.ctx);
    this.scene.renderer.domElement.style.cursor = t.cursor;
    renderToolbar(this, this.els.toolbar); renderToolOptions(this, this.els['tool-options']); this.emit('tool', t.id); this.sendState();
  }
  setModifierTool(key: ModKey, toolId: string) {
    this.modifierTools[key] = toolId;
    try { localStorage.setItem('plannr.modifierTools', JSON.stringify(this.modifierTools)); } catch { /* private mode */ }
    this.refreshUI();
  }
  /** Modifier pressed: switch to its tool until the key comes back up. */
  private beginTempTool(key: ModKey): boolean {
    const id = this.modifierTools[key]; if (!id || this.temp || this.pointerDown) return false;
    if (this.tool.id === id || this.tool.busy?.()) return false;
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
    this.lock = this.lock === l ? null : l; const b = this.els['lock-badge'];
    b.classList.toggle('show', !!this.lock); if (this.lock) { b.textContent = `Locked to ${({ x: 'red', y: 'blue', z: 'green' })[this.lock]} axis`; b.style.background = `#${AXIS_COLOR[this.lock].toString(16).padStart(6, '0')}`; }
    this.tool.onLock?.(this.ctx);
  }
  private makeContext(): ToolContext {
    const app = this;
    return {
      scene: this.scene, doc: () => app.doc,
      ops: (ops, label) => app.commitOps(ops, label), preview: (ops) => app.preview(ops),
      select: (ids, mode) => app.select(ids, mode), selection: () => app.selection, resolve: (id) => app.resolve(id),
      setHint: (t) => { app.els.statusbar.querySelector('.hint')!.textContent = t; },
      setVCB: (v, label) => { const vcb = app.els.statusbar.querySelector<HTMLInputElement>('.vcb input')!; if (document.activeElement !== vcb) vcb.value = v; if (label) app.els.statusbar.querySelector('.vcb label')!.textContent = label; },
      snap: (ev, opts) => app.snap(ev, opts), setTool: (id) => app.setTool(id),
      options: this.options, toast: (t) => app.toast(t), prompt: (l, v) => app.promptText(l, v), confirm: (t, ok, danger) => app.confirm(t, ok, danger),
      get lock() { return app.lock; }, set lock(v: AxisLock) { app.lock = v; },
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
      on: (ev, h) => { this.listeners[ev].add(h); return () => this.listeners[ev].delete(h); },
    };
  }
  private emit(ev: string, d: any) { this.listeners[ev]?.forEach(h => { try { h(d); } catch (e) { console.error(e); } }); }

  /** SketchUp-like inference: endpoints, midpoints, on-edge, on-face, axis lock and axis inference, grid. */
  private lastSnap: Snap | null = null; private snapAsked = false;
  snap(ev: { clientX: number; clientY: number }, opts: SnapOptions = {}): Snap { const r = this.snapInner(ev, opts); this.lastSnap = r; this.snapAsked = true; return r; }
  private snapInner(ev: { clientX: number; clientY: number }, opts: SnapOptions = {}): Snap {
    const sc = this.scene; const cursor = new THREE.Vector2(ev.clientX - sc.container.getBoundingClientRect().left, ev.clientY - sc.container.getBoundingClientRect().top);
    const ignore = opts.ignore ?? new Set<string>(); const isIgnored = (id: string) => ignore.has(id) || ignore.has(this.resolve(id));
    const px = (p: THREE.Vector3) => sc.screenPoint(p).distanceTo(cursor);
    const lock = opts.lock ?? null; const from = opts.from;
    // candidate from locked axis: closest point on the axis line to the ray; everything else projects onto it
    const onLock = (p: THREE.Vector3) => { if (!lock || !from) return p; const d = AXIS[lock]; return from.clone().addScaledVector(d, p.clone().sub(from).dot(d)); };
    let best: Snap | null = null;
    if (!lock) {
      const v = sc.nearestVertex(cursor.x, cursor.y, 10);
      if (v) best = { point: v.point, kind: 'vertex' };
      if (!best) { const m = sc.nearestMidpoint(cursor.x, cursor.y, 9, isIgnored); if (m) best = { point: m.point, kind: 'midpoint', id: m.id, dir: m.dir }; }
      if (!best) { const e = sc.nearestEdge(ev, cursor.x, cursor.y, 7, isIgnored); if (e) best = { point: e.point, kind: 'edge', id: e.id, dir: e.dir }; }
    } else if (from) {
      // snap to vertices projected onto the locked axis
      for (const v of sc.vertices) { const p = onLock(v); if (px(p) < 8 && Math.abs(v.clone().sub(p).length()) < sc.pixelSize(p) * 8) { best = { point: p, kind: 'axis', axis: lock }; break; } }
    }
    if (!best && opts.faces !== false && !lock) {
      const pk = sc.pick(ev, { facesOnly: true });
      if (pk && !isIgnored(pk.id)) best = { point: pk.point.clone(), kind: 'face', id: pk.id, normal: pk.back ? pk.normal.clone().negate() : pk.normal.clone() };
    }
    if (best) return best;
    // free point: on the work plane (or along the locked axis)
    let p: THREE.Vector3 | null = null;
    if (lock && from) p = sc.linePoint(ev, from, AXIS[lock]);
    else { const pl = opts.plane ?? { origin: new THREE.Vector3(), normal: new THREE.Vector3(0, 1, 0) }; p = sc.planePoint(ev, pl.origin, pl.normal); }
    if (!p) p = from?.clone() ?? new THREE.Vector3();
    let kind: Snap['kind'] = lock ? 'axis' : 'plane'; let axis: Snap['axis'] = lock ?? undefined;
    if (from && !lock) {
      // axis inference: near an axis line through `from`
      for (const ax of ['x', 'y', 'z'] as const) { const q = from.clone().addScaledVector(AXIS[ax], p.clone().sub(from).dot(AXIS[ax])); if (px(q) < 6 && q.distanceTo(from) > 1e-3) { p = q; kind = 'axis'; axis = ax; break; } }
    }
    if (kind === 'plane' && this.options.gridSnap) {
      const g = sc.cam.distance > 60 ? 0.1 : 0.01; p.set(Math.round(p.x / g) * g, Math.round(p.y / g) * g, Math.round(p.z / g) * g);
      // snap onto the visible 1 m grid lines (and their crossings) when within a few pixels
      if (this.display.grid) {
        const G = 1; const gx = Math.round(p.x / G) * G, gz = Math.round(p.z / G) * G; let onGrid = false;
        if (px(new THREE.Vector3(gx, p.y, p.z)) < 5) { p.x = gx; onGrid = true; }
        if (px(new THREE.Vector3(p.x, p.y, gz)) < 5) { p.z = gz; onGrid = true; }
        if (onGrid) kind = 'grid';
      }
    }
    return { point: p, kind, axis };
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
      case 'set_options': for (const k of ['grid', 'axes', 'edges', 'figure', 'shadows'] as const) { const key = ({ grid: 'gridVisible', axes: 'axesVisible', edges: 'edgesVisible', figure: 'figureVisible', shadows: 'shadows' })[k]; if (a[key] !== undefined && !!a[key] !== this.display[k]) this.toggleDisplay(k); }
        if (Array.isArray(a.xray)) { this.scene.setXray(a.xray.map(String)); this.emit('xray', this.scene.xrayMaterials); }
        return { ...this.display, xray: this.scene.xrayMaterials };
      default: throw new Error(`unknown viewport command ${name}`);
    }
  }
  private state() { return { camera: this.scene.cam.serialize(), selection: [...this.selection], tool: this.tool.id, hover: this.hover, version: this.doc.version, enteredGroup: this.enteredGroup, display: this.display }; }
  private sendState() { this.live?.sendState({ camera: this.scene.cam.serialize(), selection: [...this.selection], tool: this.tool.id, hover: this.hover }); }

  // ---- input ---------------------------------------------------------------------------------------
  private bindInput() {
    const canvas = this.scene.renderer.domElement; canvas.tabIndex = 0;
    let mid: { x: number; y: number; pan: boolean } | null = null;
    canvas.addEventListener('contextmenu', e => e.preventDefault());
    // touch: one finger drives the tool; two fingers orbit (drag) and zoom (pinch); three fingers pan
    const touches = new Map<number, { x: number; y: number }>(); let gesture: { n: number; cx: number; cy: number; dist: number } | null = null;
    canvas.style.touchAction = 'none';
    const centroid = () => { let cx = 0, cy = 0, n = 0; for (const t of touches.values()) { cx += t.x; cy += t.y; n++; } const pts = [...touches.values()]; const dist = pts.length >= 2 ? Math.hypot(pts[0].x - pts[1].x, pts[0].y - pts[1].y) : 0; return { n, cx: cx / n, cy: cy / n, dist }; };
    canvas.addEventListener('pointerdown', e => {
      canvas.focus(); this.pointerDown = true;
      if (e.pointerType === 'touch') {
        touches.set(e.pointerId, { x: e.clientX, y: e.clientY }); try { canvas.setPointerCapture(e.pointerId); } catch { /* synthetic */ }
        if (touches.size >= 2) { gesture = centroid(); if (touches.size === 2) this.tool.onKey?.(new KeyboardEvent('keydown', { key: 'Escape' }), this.ctx); return; }
      }
      if (this.camCtl?.onPointerDown?.(e)) { try { canvas.setPointerCapture(e.pointerId); } catch { /* synthetic */ } e.preventDefault(); return; }
      if (e.pointerType === 'touch') { this.tool.onMove?.(e, this.ctx); }
      if (e.button === 1 || e.button === 2) { mid = { x: e.clientX, y: e.clientY, pan: e.shiftKey }; canvas.setPointerCapture(e.pointerId); e.preventDefault(); return; }
      this.tool.onDown?.(e, this.ctx);
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
      }
      if (this.camCtl?.onPointerMove?.(e)) return;
      if (mid) { mid.pan || e.shiftKey ? this.scene.cam.pan(e.clientX - mid.x, e.clientY - mid.y, this.scene.height) : this.scene.cam.orbit(e.clientX - mid.x, e.clientY - mid.y); mid.x = e.clientX; mid.y = e.clientY; return; }
      this.snapAsked = false;
      this.tool.onMove?.(e, this.ctx);
      // every tool that asked for a snap gets a visible marker, also before its first click
      if (this.snapAsked && this.lastSnap) { const sn = this.lastSnap; this.scene.setSnapMarker(sn, snapColor(sn), snapLabel(sn)); } else this.scene.setSnapMarker(null);
    });
    canvas.addEventListener('pointerleave', () => this.scene.setSnapMarker(null));
    canvas.addEventListener('pointerup', e => { this.pointerDown = false;
      if (e.pointerType === 'touch') { touches.delete(e.pointerId); if (gesture) { if (touches.size < 2) { gesture = null; this.sendState(); } return; } }
      if (this.camCtl?.onPointerUp?.(e)) return; if (mid && (e.button === 1 || e.button === 2)) { mid = null; this.sendState(); this.rememberViewSoon(); return; } this.tool.onUp?.(e, this.ctx); });
    canvas.addEventListener('pointercancel', e => { this.pointerDown = false; touches.delete(e.pointerId); if (touches.size < 2) gesture = null; });
    // narrow screens: the tray becomes an overlay toggled from the menu bar
    const narrow = () => document.getElementById('app')!.classList.toggle('narrow', window.innerWidth < 980);
    window.addEventListener('resize', narrow); narrow();
    canvas.addEventListener('dblclick', e => this.tool.onDblClick?.(e, this.ctx));
    canvas.addEventListener('wheel', e => { e.preventDefault(); if (this.camCtl?.onWheel?.(e)) return; this.rememberViewSoon(); const towards = this.scene.pick(e, { facesOnly: true })?.point ?? this.scene.groundPoint(e) ?? undefined; this.scene.cam.dolly(Math.pow(1.0015, e.deltaY), towards); }, { passive: false });
    const vcb = this.els.statusbar.querySelector<HTMLInputElement>('.vcb input')!;
    vcb.addEventListener('keydown', e => { e.stopPropagation(); if (e.key === 'Enter') { this.tool.onVCB?.(vcb.value.trim(), this.ctx); vcb.blur(); canvas.focus(); } if (e.key === 'Escape') { vcb.blur(); canvas.focus(); } });
    window.addEventListener('keydown', e => {
      const t = e.target as HTMLElement; if (t && (t.tagName === 'INPUT' || t.tagName === 'SELECT' || t.tagName === 'TEXTAREA')) return;
      if (this.camCtl?.onKey?.(e, true)) { e.preventDefault(); return; }
      if ((e.key === 'Control' || e.key === 'Alt' || e.key === 'Shift') && !e.repeat) {
        const others = [e.ctrlKey, e.altKey, e.shiftKey, e.metaKey].filter(Boolean).length; // only a lone modifier starts a temporary tool
        if (others === 1 && this.beginTempTool(e.key as ModKey)) { e.preventDefault(); return; }
        if (this.tool.onKey?.(e, this.ctx)) return; // e.g. Move toggles copy with Ctrl mid-operation
        return;
      }
      if (this.temp && !e.repeat) { this.endTempTool(); } // any other key cancels the temporary tool so shortcuts behave normally
      const mod = e.metaKey || e.ctrlKey; const k = e.altKey && e.code.startsWith('Key') ? e.code.slice(3).toLowerCase() : e.key.toLowerCase(); // ⌥+letter works as the letter (walking mode uses bare letters to move)
      if (mod && k === 'z') { e.preventDefault(); e.shiftKey ? this.redo() : this.undo(); return; }
      if (mod && k === 'a') { e.preventDefault(); this.selectAll(); return; }
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
      if (/^[\d.,;-]$/.test(e.key)) { vcb.focus(); vcb.value = ''; return; }
      if (e.key === ' ') { e.preventDefault(); this.setTool('select'); return; }
      const t2 = this.tools.find(t => t.shortcut === k); if (t2) this.setTool(t2.id);
    });
    window.addEventListener('keyup', e => {
      if (this.camCtl?.onKey?.(e, false)) return;
      if (e.key === 'Control' || e.key === 'Alt' || e.key === 'Shift') { if (this.temp?.key === e.key) { this.endTempTool(e.key as ModKey); return; } this.tool.onKey?.(e, this.ctx); }
    });
    window.addEventListener('blur', () => this.endTempTool());
  }

  // ---- ui -----------------------------------------------------------------------------------------------
  refreshUI() { renderMenubar(this, this.els.menubar); renderToolbar(this, this.els.toolbar); renderHud(this, this.els['view-hud']); renderToolOptions(this, this.els['tool-options']); this.tray.render(); }
  private toastTimer: any = null;
  toast(text: string, ms = 2600) { const t = this.els.toast; t.textContent = text; t.classList.add('show'); clearTimeout(this.toastTimer); this.toastTimer = setTimeout(() => t.classList.remove('show'), ms); }
  promptText(label: string, value = ''): Promise<string | null> { return promptText(label, value); }
  confirm(text: string, okLabel = 'OK', danger = false): Promise<boolean> { return confirmDialog(text, okLabel, danger); }
}

const app = new App(document.getElementById('app')!);
(window as any).plannr = { app, registerPlugin, getPlugins, THREE, version: '0.2.0' };
void V;
