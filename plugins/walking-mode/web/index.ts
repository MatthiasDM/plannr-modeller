// Walking mode — first-person walk-through with every editing tool still available.
//
// Desktop: click the view to lock the mouse (game style): the mouse looks and steers, W A S D move, Shift runs,
// the wheel steps, a click acts at the crosshair with the active tool. Esc unlocks: the cursor is back and the
// left button interacts normally; right-click (or the HUD button) locks again. ⌥+letter picks a tool.
// Touch: a joystick moves, one-finger drag looks, a tap acts with the tool, on-screen buttons change storey.
import type { Tool, ToolContext } from '../../../web/src/tools/base';
import type { Plugin, PluginHost, CameraController } from '../../../web/src/plugins';
import type { Document } from '../../../web/src/model';
import type * as THREE_NS from 'three';

const P = (window as any).plannr as { registerPlugin(p: Plugin): void; THREE: typeof THREE_NS };
const THREE = P.THREE;
const ACCENT = '#0f9d8a';
const TOUCH = 'ontouchstart' in window || navigator.maxTouchPoints > 0;

const storeys = (doc: Document): { name: string; height: number }[] => ((doc.extensions?.construction as any)?.storeys ?? [{ name: 'Ground', height: 2.6 }]);
const ground = (doc: Document) => ((doc.extensions?.construction as any)?.groundLevel ?? 0) as number;
const floorY = (doc: Document, level: number) => ground(doc) + storeys(doc).slice(0, level).reduce((s, f) => s + f.height, 0);

let host: PluginHost;
let active: Walk | null = null;

class Walk implements CameraController {
  pos = new THREE.Vector3(); yaw = 0; pitch = 0; level = 0; eye = 1.65; fly = false;
  keys = new Set<string>(); raf = 0; last = performance.now();
  locked = false; wantLock = true;                       // game mode vs interact mode
  rightDown: { x: number; y: number; moved: boolean } | null = null;
  touchLook: { id: number; x: number; y: number; moved: boolean; sx: number; sy: number } | null = null;
  stick = { el: document.createElement('div'), knob: document.createElement('div'), id: -1, dx: 0, dy: 0 };
  hud = document.createElement('div'); cross = document.createElement('div'); touchBar = document.createElement('div');
  private hoverPending = false;
  constructor(private ctx: ToolContext) {
    const cam = ctx.scene.cam; const doc = ctx.doc();
    this.level = this.activeLevel(); const dir = cam.target.clone().sub(cam.camera.position).setY(0); if (dir.lengthSq() < 1e-6) dir.set(0, 0, -1); dir.normalize();
    this.yaw = Math.atan2(-dir.x, -dir.z); this.pos.set(cam.target.x, floorY(doc, this.level) + this.eye, cam.target.z);
    this.hud.className = 'walk-hud'; this.hud.style.setProperty('--plugin-accent', ACCENT); this.cross.className = 'walk-cross';
    host.mount(this.hud); host.mount(this.cross);
    if (TOUCH) this.mountTouch();
    document.addEventListener('pointerlockchange', this.onLockChange); document.addEventListener('pointerlockerror', this.onLockError); document.addEventListener('mousemove', this.onMouseMove);
    document.addEventListener('mousedown', this.onLockedMouse); document.addEventListener('mouseup', this.onLockedMouse);
    this.renderHud(); this.apply(); this.raf = requestAnimationFrame(this.tick);
  }
  private activeLevel(): number { const fb = document.querySelector<HTMLElement>('.floor-bar button.active[data-lvl]'); return fb ? parseInt(fb.dataset.lvl!) : 0; }
  private forward() { return new THREE.Vector3(-Math.sin(this.yaw), 0, -Math.cos(this.yaw)); }
  private apply() {
    const f = this.forward(); const look = new THREE.Vector3(f.x * Math.cos(this.pitch), Math.sin(this.pitch), f.z * Math.cos(this.pitch));
    this.ctx.scene.cam.setImmediate(this.pos.clone(), this.pos.clone().add(look), new THREE.Vector3(0, 1, 0));
    if (this.locked) this.hoverAtCrosshair();
  }
  private centre() { const r = host.canvas().getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; }
  /** While locked the active tool "hovers" at the crosshair so inference, highlights and previews follow the view. */
  private hoverAtCrosshair() { if (this.hoverPending) return; this.hoverPending = true; requestAnimationFrame(() => { this.hoverPending = false; if (!this.locked) return; const c = this.centre(); host.simulatePointer('move', c.x, c.y); }); }
  private renderHud() {
    const st = storeys(this.ctx.doc())[this.level]; const eyeAbs = (this.pos.y - ground(this.ctx.doc())).toFixed(2);
    const mode = this.locked ? `<b>Walking · mouse locked</b> <span class="muted">click = use tool at crosshair · Esc = interact</span>` : TOUCH ? `<b>Walking</b> <span class="muted">drag to look · joystick to move · tap to use the tool</span>` : `<b>Walking</b> <span class="muted">click the view to lock the mouse · left button interacts · right-drag looks</span>`;
    this.hud.innerHTML = `${mode} <span class="muted">· ${st?.name ?? `storey ${this.level + 1}`} · eye ${eyeAbs} m${this.fly ? ' · fly' : ''}</span> ${TOUCH ? '' : '<kbd>W</kbd><kbd>A</kbd><kbd>S</kbd><kbd>D</kbd> <kbd>⇧</kbd> run <kbd>PgUp</kbd><kbd>PgDn</kbd> storey <kbd>⌥</kbd>+key tools'} ${!this.locked && !TOUCH ? '<button data-lock>Lock mouse</button>' : ''}<button data-fly>${this.fly ? 'Walk' : 'Fly'}</button><button data-exit>Exit</button>`;
    this.hud.querySelector<HTMLElement>('[data-exit]')!.onclick = () => host.setCameraController(null);
    this.hud.querySelector<HTMLElement>('[data-fly]')!.onclick = () => { this.fly = !this.fly; if (!this.fly) this.pos.y = floorY(this.ctx.doc(), this.level) + this.eye; this.apply(); this.renderHud(); };
    this.hud.querySelector<HTMLElement>('[data-lock]')?.addEventListener('click', () => this.lock());
    this.cross.style.display = this.locked || TOUCH ? '' : 'none';
    document.getElementById('app')!.classList.toggle('walk-locked', this.locked);
    this.ctx.setHint(this.locked ? 'Mouse locked: move the mouse to look, W A S D to walk, click to use the active tool at the crosshair, Esc to release the mouse.' : 'Walking: left button interacts with the active tool · click the view (or right-click) to lock the mouse and steer · W A S D move · Esc leaves.');
  }
  // ---- pointer lock -------------------------------------------------------------------------------------------
  private lockTimer: any = null;
  lock() {
    this.wantLock = true; const c = host.canvas();
    try { (c.requestPointerLock as any)?.({ unadjustedMovement: true })?.catch?.(() => c.requestPointerLock()); } catch { try { c.requestPointerLock(); } catch { /* unsupported */ } }
    // some embedded browsers refuse pointer lock: fall back to interact mode so clicks reach the tools
    clearTimeout(this.lockTimer); this.lockTimer = setTimeout(() => { if (!this.locked) this.lockUnavailable(); }, 450);
  }
  private lockUnavailable() { this.wantLock = false; this.renderHud(); this.ctx.toast('Mouse lock is not available in this browser: right-drag (or one-finger drag) to look around, W A S D to move.'); }
  onLockError = () => this.lockUnavailable();
  onLockChange = () => { clearTimeout(this.lockTimer); this.locked = document.pointerLockElement === host.canvas(); if (!this.locked) this.wantLock = false; this.renderHud(); if (this.locked) this.hoverAtCrosshair(); };
  onMouseMove = (e: MouseEvent) => { if (!this.locked) return; this.yaw -= e.movementX * 0.0028; this.pitch = Math.max(-1.45, Math.min(1.45, this.pitch - e.movementY * 0.0028)); this.apply(); };
  /** Locked: mouse buttons act at the crosshair with the active tool (the browser reports them to the document). */
  onLockedMouse = (e: MouseEvent) => { if (!this.locked || e.button !== 0) return; const c = this.centre(); host.simulatePointer(e.type === 'mousedown' ? 'down' : 'up', c.x, c.y); };
  // ---- camera controller -------------------------------------------------------------------------------------
  onPointerDown(ev: PointerEvent) {
    if (this.locked) return true;                                   // handled by onLockedMouse
    if (ev.pointerType === 'touch') { if (this.stick.id >= 0 && ev.pointerId === this.stick.id) return true; this.touchLook = { id: ev.pointerId, x: ev.clientX, y: ev.clientY, moved: false, sx: ev.clientX, sy: ev.clientY }; return true; }
    if (ev.button === 2) { this.rightDown = { x: ev.clientX, y: ev.clientY, moved: false }; return true; }
    if (ev.button === 0 && this.wantLock) { this.lock(); return true; }   // first click after entering: lock
    return false;                                                     // interact mode: the tool gets it
  }
  onPointerMove(ev: PointerEvent) {
    if (this.touchLook && ev.pointerId === this.touchLook.id) {
      const dx = ev.clientX - this.touchLook.x, dy = ev.clientY - this.touchLook.y; if (Math.hypot(ev.clientX - this.touchLook.sx, ev.clientY - this.touchLook.sy) > 8) this.touchLook.moved = true;
      this.yaw -= dx * 0.005; this.pitch = Math.max(-1.45, Math.min(1.45, this.pitch - dy * 0.005)); this.touchLook.x = ev.clientX; this.touchLook.y = ev.clientY; this.apply(); return true;
    }
    if (this.rightDown) { const dx = ev.clientX - this.rightDown.x, dy = ev.clientY - this.rightDown.y; if (Math.hypot(dx, dy) > 3) this.rightDown.moved = true; this.yaw -= dx * 0.0032; this.pitch = Math.max(-1.45, Math.min(1.45, this.pitch - dy * 0.0032)); this.rightDown.x = ev.clientX; this.rightDown.y = ev.clientY; this.apply(); return true; }
    return this.locked;
  }
  onPointerUp(ev: PointerEvent) {
    if (this.touchLook && ev.pointerId === this.touchLook.id) { const t = this.touchLook; this.touchLook = null; if (!t.moved) { host.simulatePointer('move', ev.clientX, ev.clientY); host.simulatePointer('down', ev.clientX, ev.clientY); host.simulatePointer('up', ev.clientX, ev.clientY); } return true; }
    if (ev.button === 2 && this.rightDown) { const moved = this.rightDown.moved; this.rightDown = null; if (!moved && !TOUCH) this.lock(); return true; }
    return this.locked;
  }
  onWheel(ev: WheelEvent) { const f = this.forward(); this.pos.addScaledVector(f, -ev.deltaY * 0.004); this.apply(); return true; }
  onKey(ev: KeyboardEvent, down: boolean) {
    if (ev.metaKey || ev.ctrlKey || ev.altKey) return false;
    const k = ev.key.toLowerCase();
    if (['w', 'a', 's', 'd', 'shift', ' ', 'c'].includes(k)) { if ((k === ' ' || k === 'c') && !this.fly) return false; down ? this.keys.add(k) : this.keys.delete(k); return true; }
    if (!down) return false;
    if (ev.key === 'PageUp' || ev.key === 'PageDown') { this.changeLevel(ev.key === 'PageUp' ? 1 : -1); return true; }
    if (ev.key === 'Escape') { if (this.locked) return false; /* the browser releases the lock; onLockChange switches to interact mode */ if (!this.ctx.selection().size && !this.ctx.scene.overlay.children.length) { host.setCameraController(null); return true; } }
    return false;
  }
  changeLevel(d: number) { const n = storeys(this.ctx.doc()).length; this.level = Math.max(0, Math.min(n - 1, this.level + d)); this.pos.y = floorY(this.ctx.doc(), this.level) + this.eye; this.apply(); this.renderHud(); document.querySelector<HTMLElement>(`.floor-bar button[data-lvl="${this.level}"]`)?.click(); }
  // ---- touch controls -------------------------------------------------------------------------------------------
  private mountTouch() {
    const s = this.stick; s.el.className = 'walk-stick'; s.el.style.setProperty('--plugin-accent', ACCENT); s.knob.className = 'knob'; s.el.appendChild(s.knob); host.mount(s.el);
    const R = 40;
    s.el.addEventListener('pointerdown', e => { s.id = e.pointerId; s.el.setPointerCapture(e.pointerId); e.preventDefault(); e.stopPropagation(); });
    s.el.addEventListener('pointermove', e => { if (e.pointerId !== s.id) return; const r = s.el.getBoundingClientRect(); let dx = e.clientX - (r.left + r.width / 2), dy = e.clientY - (r.top + r.height / 2); const L = Math.hypot(dx, dy); if (L > R) { dx *= R / L; dy *= R / L; } s.dx = dx / R; s.dy = dy / R; s.knob.style.transform = `translate(${dx}px, ${dy}px)`; e.preventDefault(); });
    const end = (e: PointerEvent) => { if (e.pointerId !== s.id) return; s.id = -1; s.dx = s.dy = 0; s.knob.style.transform = ''; };
    s.el.addEventListener('pointerup', end); s.el.addEventListener('pointercancel', end);
    this.touchBar.className = 'walk-touch'; this.touchBar.innerHTML = `<button data-up>▲ storey</button><button data-down>▼ storey</button>`;
    this.touchBar.querySelector<HTMLElement>('[data-up]')!.onclick = () => this.changeLevel(1); this.touchBar.querySelector<HTMLElement>('[data-down]')!.onclick = () => this.changeLevel(-1);
    host.mount(this.touchBar);
  }
  // ---- loop -------------------------------------------------------------------------------------------------------
  tick = () => {
    const now = performance.now(); const dt = Math.min(0.1, (now - this.last) / 1000); this.last = now;
    const speed = (this.keys.has('shift') ? 4.2 : 1.6) * dt; const f = this.forward(); const r = new THREE.Vector3(-f.z, 0, f.x);
    let moved = false;
    if (this.keys.has('w')) { this.pos.addScaledVector(f, speed); moved = true; } if (this.keys.has('s')) { this.pos.addScaledVector(f, -speed); moved = true; }
    if (this.keys.has('d')) { this.pos.addScaledVector(r, speed); moved = true; } if (this.keys.has('a')) { this.pos.addScaledVector(r, -speed); moved = true; }
    if (this.stick.dx || this.stick.dy) { this.pos.addScaledVector(f, -this.stick.dy * 2.4 * dt).addScaledVector(r, this.stick.dx * 2.4 * dt); moved = true; }
    if (this.fly) { if (this.keys.has(' ')) { this.pos.y += speed; moved = true; } if (this.keys.has('c')) { this.pos.y -= speed; moved = true; } }
    if (moved) this.apply();
    this.raf = requestAnimationFrame(this.tick);
  };
  release() {
    cancelAnimationFrame(this.raf); if (document.pointerLockElement) document.exitPointerLock();
    clearTimeout(this.lockTimer); document.removeEventListener('pointerlockchange', this.onLockChange); document.removeEventListener('pointerlockerror', this.onLockError); document.removeEventListener('mousemove', this.onMouseMove); document.removeEventListener('mousedown', this.onLockedMouse); document.removeEventListener('mouseup', this.onLockedMouse);
    this.hud.remove(); this.cross.remove(); this.stick.el.remove(); this.touchBar.remove(); document.getElementById('app')!.classList.remove('walk-locked');
    if (active === this) active = null; this.ctx.setHint('Left walking mode.'); host.refresh();
  }
}

/** The toolbar entry is a toggle: activating it enters or leaves walking mode and hands the tool back. */
const walkTool: Tool = {
  id: 'walk', label: 'Walk through', hint: 'Walk through the model at eye height; every tool keeps working.', cursor: 'default', shortcut: 'w',
  icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="13" cy="4" r="1.8" fill="#9fe0d4"/><path d="M12 7.5 9.5 12l2.5 2.5V21M9.5 12l-3 2M12 7.5l3.5 1.5 2 3M11.5 14.5 8 21"/></svg>`,
  activate(ctx) {
    const prev = (host as any)._prevTool as string | undefined;
    if (active) host.setCameraController(null); else { active = new Walk(ctx); host.setCameraController(active); }
    setTimeout(() => ctx.setTool(prev && prev !== 'walk' ? prev : 'select'), 0);
  },
};

P.registerPlugin({
  id: 'walking-mode', name: 'Walking mode', version: '0.2.0', accent: ACCENT,
  tools: [walkTool], toolbar: [['walk']],
  panels: [{ id: 'main', title: 'Walking mode', render: (_ctx, el) => {
    el.innerHTML = `<div class="muted" style="margin:4px 0 8px">First-person walk-through at 1.65 m eye height. Everything stays editable.</div>
      <div class="kv"><label>Status</label><div>${active ? `<b style="color:${ACCENT}">walking</b>` : 'off'}</div>
        <label>Mouse</label><div>click the view to <b>lock</b> it: the mouse looks and steers, a click uses the active tool at the crosshair. <kbd>Esc</kbd> releases it to interact normally; right-click locks again.</div>
        <label>Move</label><div><kbd>W</kbd> <kbd>A</kbd> <kbd>S</kbd> <kbd>D</kbd>, <kbd>⇧</kbd> run, wheel steps</div>
        <label>Touch</label><div>joystick moves, one-finger drag looks, tap uses the tool, ▲▼ change storey</div>
        <label>Storey</label><div><kbd>PgUp</kbd> / <kbd>PgDn</kbd> or the floor bar</div><label>Tools</label><div>toolbar, or <kbd>⌥</kbd> + the usual letter</div>
        <label>Fly</label><div>button in the bar; <kbd>Space</kbd> up, <kbd>C</kbd> down</div><label>Leave</label><div><kbd>Esc</kbd> again (nothing selected) or Exit</div></div>
      <div class="btn-row"><button class="btn ${active ? '' : 'primary'}" data-walk>${active ? 'Exit walking mode' : 'Start walking'}</button></div>`;
    el.querySelector<HTMLElement>('[data-walk]')!.onclick = () => host.setTool('walk');
  } }],
  activate(h) { host = h; h.on('tool', (id: string) => { if (id !== 'walk') (h as any)._prevTool = id; }); },
});
