import * as THREE from 'three';
import type { CompiledScene } from './api';
import { fmtM, fmtDeg, type Vec3 } from './model';
import { textureUrl } from './textures';

export const V = (p: Vec3) => new THREE.Vector3(p[0], p[1], p[2]);
/** The arc of an angle: points at distance `r` from the corner `c`, from the direction `u` round to `w` (unit vectors). */
export function angleArc(c: THREE.Vector3, u: THREE.Vector3, w: THREE.Vector3, r: number): THREE.Vector3[] {
  const n = u.clone().cross(w); if (n.lengthSq() < 1e-12) return [c.clone().addScaledVector(u, r)];
  n.normalize(); const ang = u.angleTo(w); const steps = Math.max(2, Math.ceil(ang / (Math.PI / 36)));
  return Array.from({ length: steps + 1 }, (_, i) => c.clone().add(u.clone().applyAxisAngle(n, ang * i / steps).multiplyScalar(r)));
}
type Dim = { a: Vec3; b: Vec3; offset: Vec3; value: number; style?: string | null };
/** Where a dimension's label sits, and what it says. */
function dimLabel(d: Dim): { pos: THREE.Vector3; text: string } {
  const a = V(d.a), b = V(d.b), o = V(d.offset);
  if (d.style === 'angle') { const arc = angleArc(a, b.clone().sub(a).normalize(), o.clone().normalize(), a.distanceTo(b)); return { pos: arc[arc.length >> 1], text: fmtDeg(d.value) }; }
  if (d.style) return { pos: b.add(o), text: `${d.style === 'radius' ? 'R ' : '⌀ '}${fmtM(d.value)}` };
  return { pos: a.add(o).lerp(b.add(o), 0.5), text: fmtM(d.value) };
}
export const A = (p: THREE.Vector3): Vec3 => [p.x, p.y, p.z];

/** How faces are drawn, as in SketchUp's Face Style. */
export type FaceStyle = 'shaded' | 'monochrome' | 'hiddenline' | 'wireframe' | 'xray';
export const FACE_STYLES: { id: FaceStyle; label: string }[] = [{ id: 'xray', label: 'X-ray' }, { id: 'wireframe', label: 'Wireframe' }, { id: 'hiddenline', label: 'Hidden Line' }, { id: 'shaded', label: 'Shaded' }, { id: 'monochrome', label: 'Monochrome' }];
export interface Pick { id: string; kind: 'face' | 'edge' | 'guide' | 'text' | 'dimension' | 'section'; point: THREE.Vector3; normal: THREE.Vector3; material?: string; back?: boolean }
export interface Bounds { min: Vec3; max: Vec3 }
export interface SnapTarget { point: THREE.Vector3; kind: 'vertex' | 'midpoint' | 'edge' | 'face' | 'guide'; id?: string; dir?: THREE.Vector3; normal?: THREE.Vector3 }

export const COLOR = { select: 0x2f6fde, hover: 0x5a93f0, ink: 0x1b1b1b, red: 0xd93025, green: 0x1e8e3e, blue: 0x1a73e8, magenta: 0xc8169a, cyan: 0x18a0b8 };
export type ViewName = 'iso' | 'top' | 'bottom' | 'front' | 'back' | 'left' | 'right';

/** SketchUp-style camera: middle-drag orbits, shift+middle pans, wheel zooms toward the cursor. */
export class ViewCamera {
  camera: THREE.PerspectiveCamera;
  target = new THREE.Vector3(0, 0.5, 0);
  up = new THREE.Vector3(0, 1, 0);
  private sph = new THREE.Spherical(16, Math.PI / 3.1, Math.PI / 4);
  private anim: { from: [THREE.Vector3, THREE.Vector3, THREE.Vector3]; to: [THREE.Vector3, THREE.Vector3, THREE.Vector3]; t: number } | null = null;
  constructor(aspect: number) { this.camera = new THREE.PerspectiveCamera(35, aspect, 0.05, 4000); this.apply(); }
  private apply() {
    this.camera.position.setFromSpherical(this.sph).add(this.target);
    this.camera.up.copy(this.up); this.camera.lookAt(this.target);
  }
  orbit(dx: number, dy: number) {
    this.anim = null; this.sph.theta -= dx * 0.006; this.sph.phi -= dy * 0.006;
    this.sph.phi = Math.max(0.02, Math.min(Math.PI - 0.02, this.sph.phi)); this.up.set(0, 1, 0); this.apply();
  }
  pan(dx: number, dy: number, viewportH: number) {
    this.anim = null;
    const scale = (2 * Math.tan((this.camera.fov * Math.PI) / 360) * this.sph.radius) / viewportH;
    const right = new THREE.Vector3().setFromMatrixColumn(this.camera.matrix, 0), upv = new THREE.Vector3().setFromMatrixColumn(this.camera.matrix, 1);
    this.target.addScaledVector(right, -dx * scale).addScaledVector(upv, dy * scale); this.apply();
  }
  dolly(factor: number, towards?: THREE.Vector3) {
    this.anim = null;
    if (towards) this.target.addScaledVector(towards.clone().sub(this.target), 1 - factor);
    this.sph.radius = Math.max(0.2 * this.reach, Math.min(1500 * this.reach, this.sph.radius * factor)); this.apply();
  }
  /** How much further away than the standard 35° lens this lens stands for the same picture. */
  private get reach() { return Math.tan((35 * Math.PI) / 360) / Math.tan((this.camera.fov * Math.PI) / 360); }
  /** Change the lens, and stand back (or come closer) so that what is at the target stays the same size. A 1° lens
   *  from far away is the parallel projection: lines that are parallel stay parallel on screen. */
  setFov(fov: number) {
    const t = (f: number) => Math.tan((f * Math.PI) / 360); const k = t(this.camera.fov) / t(fov);
    if (this.anim) this.anim.to[0] = this.anim.to[1].clone().add(this.anim.to[0].clone().sub(this.anim.to[1]).multiplyScalar(k));
    this.sph.radius *= k; this.camera.fov = fov; this.camera.updateProjectionMatrix(); this.apply();
  }
  get distance() { return this.sph.radius; }
  /** The distance a standard 35° lens would show this picture from. */
  get range() { return this.sph.radius / this.reach; }
  fitDistance(b: Bounds): number {
    const r = Math.max(Math.hypot(b.max[0] - b.min[0], b.max[1] - b.min[1], b.max[2] - b.min[2]) / 2, 1.2);
    const vfov = (this.camera.fov * Math.PI) / 180, hfov = 2 * Math.atan(Math.tan(vfov / 2) * this.camera.aspect);
    return (r / Math.sin(Math.min(vfov, hfov) / 2)) * 1.05;
  }
  private centerOf(b?: Bounds) { return b ? new THREE.Vector3((b.min[0] + b.max[0]) / 2, (b.min[1] + b.max[1]) / 2, (b.min[2] + b.max[2]) / 2) : new THREE.Vector3(0, 0.5, 0); }
  setView(kind: ViewName, bounds?: Bounds) {
    const c = this.centerOf(bounds); const r = bounds ? this.fitDistance(bounds) : 18 * this.reach;
    const views: Record<ViewName, [number, number]> = { iso: [Math.PI / 3.2, Math.PI / 4], top: [0.0001, 0], bottom: [Math.PI - 0.0001, 0], front: [Math.PI / 2, 0], back: [Math.PI / 2, Math.PI], left: [Math.PI / 2, -Math.PI / 2], right: [Math.PI / 2, Math.PI / 2] };
    const [phi, theta] = views[kind];
    const pos = new THREE.Vector3().setFromSpherical(new THREE.Spherical(r, phi, theta)).add(c);
    const up = new THREE.Vector3(0, 1, 0); if (kind === 'top') up.set(0, 0, -1); if (kind === 'bottom') up.set(0, 0, 1);
    this.animateTo(pos, c, up);
  }
  frame(bounds?: Bounds) {
    if (!bounds) return this.setView('iso');
    const c = this.centerOf(bounds); const dir = this.camera.position.clone().sub(this.target).normalize();
    this.animateTo(c.clone().addScaledVector(dir, this.fitDistance(bounds)), c, this.up.clone());
  }
  animateTo(pos: THREE.Vector3, tgt: THREE.Vector3, up = new THREE.Vector3(0, 1, 0)) {
    this.anim = { from: [this.camera.position.clone(), this.target.clone(), this.up.clone()], to: [pos, tgt, up], t: 0 };
  }
  setImmediate(pos: THREE.Vector3, tgt: THREE.Vector3, up = new THREE.Vector3(0, 1, 0)) {
    this.anim = null; this.target.copy(tgt); this.up.copy(up); this.sph.setFromVector3(pos.clone().sub(tgt)); if (this.sph.phi < 0.0001) this.sph.phi = 0.0001; this.apply();
  }
  tick(dt: number) {
    if (!this.anim) return false;
    this.anim.t = Math.min(1, this.anim.t + dt * 3.2); const e = 1 - Math.pow(1 - this.anim.t, 3);
    const pos = this.anim.from[0].clone().lerp(this.anim.to[0], e); this.target.copy(this.anim.from[1]).lerp(this.anim.to[1], e); this.up.copy(this.anim.from[2]).lerp(this.anim.to[2], e).normalize();
    this.sph.setFromVector3(pos.clone().sub(this.target)); if (this.sph.phi < 0.0001) this.sph.phi = 0.0001; this.apply();
    if (this.anim.t >= 1) this.anim = null; return true;
  }
  serialize(): number[] { const p = this.camera.position, t = this.target, u = this.up; return [p.x, p.y, p.z, t.x, t.y, t.z, u.x, u.y, u.z]; }
  restore(c: number[], animate = true) {
    if (c.length < 6) return;
    const up = c.length >= 9 ? new THREE.Vector3(c[6], c[7], c[8]) : new THREE.Vector3(0, 1, 0);
    animate ? this.animateTo(new THREE.Vector3(c[0], c[1], c[2]), new THREE.Vector3(c[3], c[4], c[5]), up) : this.setImmediate(new THREE.Vector3(c[0], c[1], c[2]), new THREE.Vector3(c[3], c[4], c[5]), up);
  }
}

export class Viewport {
  renderer: THREE.WebGLRenderer;
  scene = new THREE.Scene();
  cam: ViewCamera;
  container: HTMLElement; labels: HTMLElement;
  modelGroup = new THREE.Group(); overlay = new THREE.Group(); snapGroup = new THREE.Group(); private snapEl: HTMLDivElement | null = null; selectionGroup = new THREE.Group(); hoverGroup = new THREE.Group();
  ground: THREE.Mesh; grid: THREE.GridHelper; axes: THREE.Group; figure: THREE.Group;
  private raycaster = new THREE.Raycaster();
  private meshes: THREE.Mesh[] = [];
  private groupData = new Map<THREE.Mesh, { ids: string[]; ranges: [number, number][]; material: string; back: boolean }>();
  private edgeLines: THREE.LineSegments | null = null; private edgeIds: string[] = [];
  private guideObjects: THREE.Object3D[] = []; private guideIds = new Map<THREE.Object3D, string>();
  private textObjects: { id: string; pos: THREE.Vector3 }[] = [];
  private sectionMeshes: THREE.Mesh[] = []; private sectionGroups: THREE.Object3D[] = [];
  /** Whether the section planes themselves are drawn (and pickable). Off by default, like SketchUp's
   *  "Display Section Planes": the cut still applies, only the sheets are hidden. A selected plane still shows its highlight. */
  sectionPlanesVisible = false;
  setSectionPlanesVisible(on: boolean) { this.sectionPlanesVisible = on; for (const g of this.sectionGroups) g.visible = on; }
  private faceGeometries = new Map<string, { pos: THREE.Vector3[]; normal: THREE.Vector3 }>(); // outline + normal per face id for planes
  vertices: THREE.Vector3[] = []; segments: { a: THREE.Vector3; b: THREE.Vector3; id: string }[] = [];
  // snap index: flat arrays + a screen-space projection cache that is rebuilt only when the camera or viewport changes
  private vp = new Float32Array(0); private sa = new Float32Array(0); private sb = new Float32Array(0);
  private scr = { key: '', verts: new Float32Array(0), segA: new Float32Array(0), segB: new Float32Array(0) };
  bounds?: Bounds; compiled?: CompiledScene;
  /** The active section planes as GPU clipping planes, shared by every model material (one array, updated in
   *  place): switching a plane is instant. Picking and snapping skip what they cut away (`kept`). */
  private clips: THREE.Plane[] = [];
  private clip<M extends THREE.Material>(m: M): M { m.clippingPlanes = this.clips; m.clipShadows = true; return m; }
  /** Whether a point survives the active section planes. */
  kept(p: THREE.Vector3): boolean { for (const pl of this.clips) if (pl.distanceToPoint(p) < -1e-6) return false; return true; }
  private setClips(cuts: CompiledScene['cuts']) {
    this.clips.length = 0;
    for (const c of cuts ?? []) { const n = V(c.normal).normalize(); this.clips.push(new THREE.Plane(n.clone().negate(), n.dot(V(c.origin)))); }
  }
  edgesVisible = true; edgeColor = COLOR.ink;
  /** The colour of lines no face draws: loose edges, dimensions, the tools' own lines. Lighter in the dark theme. */
  ink = COLOR.ink; private sun: THREE.DirectionalLight;
  private clock = new THREE.Clock();

  constructor(container: HTMLElement, labels: HTMLElement) {
    this.container = container; this.labels = labels;
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.shadowMap.enabled = true; this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.localClippingEnabled = true; // section planes cut the model on the GPU (see `clips`)
    container.appendChild(this.renderer.domElement);
    this.cam = new ViewCamera(1);
    this.scene.add(new THREE.HemisphereLight(0xdfe9f3, 0xbfb8a8, 1.15));
    this.sun = new THREE.DirectionalLight(0xfff2dd, 1.4); this.sun.position.set(20, 32, 14); this.sun.castShadow = true;
    this.sun.shadow.mapSize.set(2048, 2048); Object.assign(this.sun.shadow.camera, { left: -40, right: 40, top: 40, bottom: -40, near: 1, far: 160 }); this.sun.shadow.bias = -0.0005;
    this.scene.add(this.sun);
    const fill = new THREE.DirectionalLight(0xcfd8e6, 0.35); fill.position.set(-18, 12, -22); this.scene.add(fill);
    // the ground is subdivided: two 6 km triangles interpolate depth so coarsely near the camera that
    // the ground (12 mm down) could win over faces lying on y = 0 (slabs, floors) in wedges
    this.ground = new THREE.Mesh(new THREE.PlaneGeometry(4000, 4000, 400, 400), new THREE.MeshLambertMaterial({ color: 0xe9e5da }));
    // the ground and the grid sit clearly below the drawing plane (y = 0): faces drawn on the ground must win the depth test
    this.ground.rotation.x = -Math.PI / 2; this.ground.position.y = -0.012; this.ground.receiveShadow = true; this.scene.add(this.ground);
    this.grid = new THREE.GridHelper(200, 200, 0xc9c4b8, 0xdcd8cd); this.grid.position.y = -0.006;
    const gm = this.grid.material as THREE.Material; gm.transparent = true; gm.opacity = 0.7; gm.depthWrite = false; this.scene.add(this.grid);
    this.axes = this.makeAxes(); this.scene.add(this.axes);
    this.figure = this.makeFigure(); this.scene.add(this.figure);
    this.scene.add(this.modelGroup, this.previewGroup, this.overlay, this.snapGroup, this.selectionGroup, this.hoverGroup);
    this.resize(); new ResizeObserver(() => this.resize()).observe(container);
    this.applyTheme(); matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => this.applyTheme());
  }

  /** Match the ground and grid to the CSS theme (light / dark). */
  applyTheme() {
    const css = getComputedStyle(document.documentElement); const dark = css.getPropertyValue('--dark').trim() === '1';
    (this.ground.material as THREE.MeshLambertMaterial).color.set(css.getPropertyValue('--ground').trim() || '#e9e5da');
    const g = this.grid.material as THREE.LineBasicMaterial; g.color.set(css.getPropertyValue('--grid-3d').trim() || (dark ? '#27405f' : '#c3d2e6')); g.opacity = dark ? 0.75 : 0.8;
    // the borders of faces are dark in both themes: they read on the lit faces. A line no face draws takes the
    // theme's ink, or it would vanish against the dark ground.
    this.edgeColor = dark ? 0x101216 : COLOR.ink;
    this.ink = new THREE.Color(css.getPropertyValue('--ink-3d').trim() || (dark ? '#a6b6d0' : '#1b1b1b')).getHex();
    this.paintEdges();
    if (this.backLines) (this.backLines.material as THREE.LineDashedMaterial).color.set(this.edgeColor);
    for (const o of this.guideObjects) if (o.userData.ink) ((o as THREE.Line).material as THREE.LineBasicMaterial).color.set(this.ink);
  }
  /** A colour for both ends of every edge segment: the edge colour for the borders of faces and for lines lying on
   *  a face, the theme's ink for the other lines, and for everything in wireframe, where no face is behind them. */
  private edgeColors(c: Drawn): THREE.Float32BufferAttribute {
    const ids = c.edges.ids, P = c.edges.positions as ArrayLike<number>, n = ids.length, light = new Uint8Array(n);
    if (this.ink !== this.edgeColor && this.faceStyle === 'wireframe') light.fill(1);
    else if (this.ink !== this.edgeColor) {
      const faces = new Set<string>(); for (const g of c.groups) for (const id of g.ids) faces.add(id);
      const q = (v: number) => Math.round(v * 1e4), h = (o: number) => Math.imul(q(P[o]), 73856093) ^ Math.imul(q(P[o + 1]), 19349663) ^ Math.imul(q(P[o + 2]), 83492791);
      const key = (i: number) => (h(i * 6) + h(i * 6 + 3)) | 0;
      const loose = new Map<number, number[]>(); let count = 0;
      for (let i = 0; i < n; i++) if (!faces.has(ids[i])) { light[i] = 1; count++; const k = key(i), l = loose.get(k); if (l) l.push(i); else loose.set(k, [i]); }
      if (count) {
        // a loose line that runs along the border of a face (the line that split it) is drawn as that border
        const near = (a: number, b: number) => Math.abs(P[a] - P[b]) < 1e-4 && Math.abs(P[a + 1] - P[b + 1]) < 1e-4 && Math.abs(P[a + 2] - P[b + 2]) < 1e-4;
        for (let i = 0; i < n; i++) {
          if (light[i]) continue;
          for (const j of loose.get(key(i)) ?? []) if ((near(i * 6, j * 6) && near(i * 6 + 3, j * 6 + 3)) || (near(i * 6, j * 6 + 3) && near(i * 6 + 3, j * 6))) light[j] = 0;
        }
        // and so is one that lies on a face, as long as looking for the face stays cheap enough to do on every change
        if (count * triangles(c) <= 2e6) for (let i = 0; i < n; i++) if (light[i] && onFace(c, (P[i * 6] + P[i * 6 + 3]) / 2, (P[i * 6 + 1] + P[i * 6 + 4]) / 2, (P[i * 6 + 2] + P[i * 6 + 5]) / 2)) light[i] = 0;
      }
    }
    const col = new Float32Array(n * 6), edge = new THREE.Color(this.edgeColor), ink = new THREE.Color(this.ink);
    for (let i = 0; i < n; i++) { const k = light[i] ? ink : edge; for (let o = i * 6; o < i * 6 + 6; o += 3) { col[o] = k.r; col[o + 1] = k.g; col[o + 2] = k.b; } }
    return new THREE.Float32BufferAttribute(col, 3);
  }
  private paintEdges() { if (this.edgeLines && this.compiled) this.edgeLines.geometry.setAttribute('color', this.edgeColors(this.compiled)); }

  /** Put the drawn axes and the grid where the drawing axes are. */
  setAxes(origin: THREE.Vector3, x: THREE.Vector3, y: THREE.Vector3, z: THREE.Vector3) {
    const q = new THREE.Quaternion().setFromRotationMatrix(new THREE.Matrix4().makeBasis(x, y, z));
    this.axes.position.copy(origin); this.axes.quaternion.copy(q);
    this.grid.quaternion.copy(q); this.grid.position.copy(origin).addScaledVector(y, -0.006);
  }
  private makeAxes(): THREE.Group {
    const g = new THREE.Group(); const L = 800;
    const mk = (dir: THREE.Vector3, color: number) => {
      const solid = new THREE.Line(new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(), dir.clone().multiplyScalar(L)]), new THREE.LineBasicMaterial({ color }));
      const dashed = new THREE.Line(new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(), dir.clone().multiplyScalar(-L)]), new THREE.LineDashedMaterial({ color, dashSize: 0.4, gapSize: 0.25 }));
      dashed.computeLineDistances(); g.add(solid, dashed);
    };
    mk(new THREE.Vector3(1, 0, 0), COLOR.red); mk(new THREE.Vector3(0, 0, 1), COLOR.green); mk(new THREE.Vector3(0, 1, 0), COLOR.blue);
    return g;
  }
  private makeFigure(): THREE.Group {
    const g = new THREE.Group(); const mat = new THREE.MeshLambertMaterial({ color: 0x6d7a8a }); const skin = new THREE.MeshLambertMaterial({ color: 0xd9b89a });
    const add = (geo: THREE.BufferGeometry, m: THREE.Material, x: number, y: number, z: number) => { const mesh = new THREE.Mesh(geo, m); mesh.position.set(x, y, z); mesh.castShadow = true; g.add(mesh); };
    add(new THREE.CapsuleGeometry(0.075, 0.72, 4, 8), mat, -0.1, 0.44, 0); add(new THREE.CapsuleGeometry(0.075, 0.72, 4, 8), mat, 0.1, 0.44, 0);
    add(new THREE.CapsuleGeometry(0.19, 0.42, 4, 10), new THREE.MeshLambertMaterial({ color: 0x3f5f8a }), 0, 1.13, 0);
    add(new THREE.CapsuleGeometry(0.05, 0.56, 4, 8), skin, -0.27, 1.1, 0); add(new THREE.CapsuleGeometry(0.05, 0.56, 4, 8), skin, 0.27, 1.1, 0);
    add(new THREE.SphereGeometry(0.11, 12, 10), skin, 0, 1.63, 0);
    g.position.set(-1.2, 0, 1.2); return g;
  }
  resize() {
    const w = this.container.clientWidth || 1, h = this.container.clientHeight || 1;
    this.renderer.setSize(w, h, false); this.cam.camera.aspect = w / h; this.cam.camera.updateProjectionMatrix();
  }
  get width() { return this.container.clientWidth; } get height() { return this.container.clientHeight; }

  /** Replace the model with a freshly compiled scene. */
  /** Materials drawn through everything in front of them (conduits inside walls, …). */
  private xray = new Set<string>();
  get xrayMaterials() { return [...this.xray]; }
  setXray(materials: string[]) { this.xray = new Set(materials); for (const m of this.meshes) this.applyXray(m); }
  private applyXray(mesh: THREE.Mesh) {
    const d = this.groupData.get(mesh); const on = !!d && this.xray.has(d.material);
    const mat = mesh.material as THREE.MeshLambertMaterial;
    if (mat.depthTest === !on) return;
    mat.depthTest = !on; mat.depthWrite = !on && !mat.transparent; mesh.renderOrder = on ? 4 : 0; mesh.castShadow = !on && !mat.transparent; mat.needsUpdate = true;
  }
  /** The face style of the whole model; `backEdges` adds dashed lines for edges behind faces. */
  faceStyle: FaceStyle = 'shaded'; backEdges = false; private backLines: THREE.LineSegments | null = null;
  setFaceStyle(s: FaceStyle) { this.faceStyle = s; this.applyFaceStyles(); this.paintEdges(); }
  setBackEdges(on: boolean) { this.backEdges = on; this.applyFaceStyles(); }
  private applyFaceStyles() {
    for (const m of this.meshes) this.applyFaceStyle(m);
    const style = this.faceStyle;
    if (this.edgeLines) { const em = this.edgeLines.material as THREE.LineBasicMaterial; const t = style === 'xray'; if (em.transparent !== t) { em.transparent = t; em.needsUpdate = true; } } // drawn after the see-through faces, not dimmed by them
    const want = this.backEdges && this.edgesVisible && !!this.edgeLines && (style === 'shaded' || style === 'monochrome' || style === 'hiddenline');
    if (want && !this.backLines) {
      const l = new THREE.LineSegments(this.edgeLines!.geometry, this.clip(new THREE.LineDashedMaterial({ color: this.edgeColor, dashSize: 0.06, gapSize: 0.05, transparent: true, opacity: 0.55, depthWrite: false, depthFunc: THREE.GreaterDepth })));
      l.computeLineDistances(); l.renderOrder = 2; this.modelGroup.add(l); this.backLines = l;
    }
    if (this.backLines) this.backLines.visible = want;
  }
  /** The texture image of each material that has one, and the GPU texture made of each image (shared by
   *  every mesh that shows it). */
  private imageOf = new Map<string, string>(); private textures = new Map<string, THREE.Texture>();
  /** The document's materials: call before the scene compiled from that document is set. */
  setMaterials(materials: { id: string; texture?: { image: string } | null }[]) {
    this.imageOf = new Map(materials.filter(m => m.texture?.image).map(m => [m.id, m.texture!.image]));
    const used = new Set(this.imageOf.values());
    for (const [image, t] of this.textures) if (!used.has(image)) { t.dispose(); this.textures.delete(image); }
  }
  private textureOf(material: string): THREE.Texture | null {
    const image = this.imageOf.get(material); if (!image) return null;
    let t = this.textures.get(image);
    if (!t) {
      t = new THREE.TextureLoader().load(textureUrl(image)); t.wrapS = t.wrapT = THREE.RepeatWrapping; t.colorSpace = THREE.SRGBColorSpace;
      t.anisotropy = Math.min(8, this.renderer.capabilities.getMaxAnisotropy()); this.textures.set(image, t);
    }
    return t;
  }

  private applyFaceStyle(mesh: THREE.Mesh) {
    const d = this.groupData.get(mesh); const mat = mesh.material as THREE.MeshLambertMaterial; const base = mesh.userData.base as { color: string; opacity: number; map?: THREE.Texture | null } | undefined; if (!d || !base) return;
    const ghost = d.material === '__hidden'; const style = ghost ? 'shaded' : this.faceStyle; const glass = base.opacity < 1;
    mesh.visible = ghost || style !== 'wireframe';
    mat.emissive.set(style === 'hiddenline' ? 0xffffff : 0x000000);
    // a texture shows as it is (not tinted by the colour) wherever the materials show
    const map = !ghost && (style === 'shaded' || style === 'xray') ? base.map ?? null : null;
    if (mat.map !== map) { mat.map = map; mat.needsUpdate = true; }
    mat.color.set(style === 'hiddenline' ? 0x000000 : style === 'monochrome' ? (d.back ? '#b7c1cc' : '#f4f4f2') : map ? '#ffffff' : base.color);
    const see = style === 'xray' || glass;
    mat.opacity = style === 'xray' ? Math.min(base.opacity, 0.45) : base.opacity;
    if (mat.transparent !== see) { mat.transparent = see; mat.needsUpdate = true; }
    const drawnThrough = this.xray.has(d.material);
    mat.depthWrite = !see && !drawnThrough;
    mesh.castShadow = !see && !drawnThrough && !ghost && (style === 'shaded' || style === 'monochrome');
    mesh.receiveShadow = style !== 'hiddenline' && !ghost;
  }

  // ---- partial previews: only what an op changes is redrawn; the rest of the model stays as it is ----
  previewGroup = new THREE.Group();
  /** Base meshes / edges hidden while a preview shows (a copy without the changed objects stands in). */
  private swapped = new Set<THREE.Object3D>(); private previewKey = ''; private previewHit: ((id: string) => boolean) | null = null;
  private previewOwn: THREE.Object3D[] = []; private lastSelection = new Set<string>();
  /** Show `c` (the changed objects as the ops leave them) in place of the current geometry of `dirty`
   *  (top-level ids). Nothing else is rebuilt, so this keeps up with the mouse on a big model. `null` clears it. */
  setPreview(c: CompiledScene | null, dirty: string[] = []) {
    const key = c ? [...dirty].sort().join(',') : '';
    if (key !== this.previewKey) this.dropPreview();
    else this.dropPreviewOwn();
    if (!c) return;
    if (!this.previewKey) {
      this.previewKey = key;
      const ids = this.expandIds(new Set(dirty)); const bases = new Set([...ids].filter(i => !i.includes('#')));
      const gone = (fid: string) => ids.has(fid) || bases.has(fid.split('#')[0]); this.previewHit = gone;
      for (const mesh of this.meshes) {
        if (!mesh.visible) continue; const d = this.groupData.get(mesh)!; if (!d.ids.some(gone)) continue;
        const src = mesh.geometry.getIndex()!.array; const sub: number[] = [];
        d.ranges.forEach(([s, n], i) => { if (!gone(d.ids[i])) for (let k = s; k < s + n; k++) sub.push(src[k]); });
        mesh.visible = false; this.swapped.add(mesh);
        if (!sub.length) continue;
        const geo = new THREE.BufferGeometry(); geo.setAttribute('position', mesh.geometry.getAttribute('position')); const nrm = mesh.geometry.getAttribute('normal'); if (nrm) geo.setAttribute('normal', nrm); const uv = mesh.geometry.getAttribute('uv'); if (uv) geo.setAttribute('uv', uv); geo.setIndex(sub);
        const m = new THREE.Mesh(geo, mesh.material); m.castShadow = mesh.castShadow; m.receiveShadow = mesh.receiveShadow; m.renderOrder = mesh.renderOrder; this.previewGroup.add(m);
      }
      if (this.edgeLines && this.edgeLines.visible && this.edgeIds.some(gone)) {
        const P = this.edgeLines.geometry.getAttribute('position').array, C = this.edgeLines.geometry.getAttribute('color').array; const keep: number[] = [], cols: number[] = [];
        this.edgeIds.forEach((id, i) => { if (!gone(id)) for (let k = i * 6; k < i * 6 + 6; k++) { keep.push(P[k]); cols.push(C[k]); } });
        this.edgeLines.visible = false; this.swapped.add(this.edgeLines); if (this.backLines?.visible) { this.backLines.visible = false; this.swapped.add(this.backLines); }
        const geo = new THREE.BufferGeometry(); geo.setAttribute('position', new THREE.Float32BufferAttribute(keep, 3)); geo.setAttribute('color', new THREE.Float32BufferAttribute(cols, 3));
        const l = new THREE.LineSegments(geo, this.edgeLines.material); l.renderOrder = 1; this.previewGroup.add(l);
      }
      this.highlight(this.lastSelection);
    }
    // the changed objects as the ops leave them
    for (const g of c.groups) {
      if (!g.indices.length) continue;
      const mesh = this.makeMesh(g); this.groupData.set(mesh, { ids: g.ids, ranges: g.idRanges, material: g.material, back: g.back });
      this.applyFaceStyle(mesh); this.applyXray(mesh); this.previewGroup.add(mesh); this.previewOwn.push(mesh);
    }
    if (c.edges.positions.length && this.edgesVisible) {
      const geo = new THREE.BufferGeometry(); geo.setAttribute('position', new THREE.Float32BufferAttribute(c.edges.positions, 3)); geo.setAttribute('color', this.edgeColors(c));
      const l = new THREE.LineSegments(geo, this.clip(new THREE.LineBasicMaterial({ vertexColors: true }))); l.renderOrder = 1; this.previewGroup.add(l); this.previewOwn.push(l);
    }
  }
  private dropPreviewOwn() {
    for (const o of this.previewOwn) { this.previewGroup.remove(o); const m = o as THREE.Mesh; m.geometry.dispose(); (m.material as THREE.Material).dispose(); if ((o as any).isMesh) this.groupData.delete(m); }
    this.previewOwn = [];
  }
  private dropPreview() {
    this.dropPreviewOwn();
    for (const o of [...this.previewGroup.children]) this.previewGroup.remove(o); // stand-ins share the model's buffers: nothing to dispose
    const had = this.swapped.size > 0;
    for (const o of this.swapped) { if (o === this.edgeLines) o.visible = this.edgesVisible; else if ((o as any).isMesh) this.applyFaceStyle(o as THREE.Mesh); }
    this.swapped.clear(); this.previewKey = ''; this.previewHit = null;
    if (had) { this.applyFaceStyles(); this.highlight(this.lastSelection); }
  }
  private makeMesh(g: CompiledScene['groups'][number]) {
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.Float32BufferAttribute(g.positions, 3));
    if (g.normals?.length === g.positions.length) geo.setAttribute('normal', new THREE.Float32BufferAttribute(g.normals, 3)); else geo.computeVertexNormals();
    geo.setIndex(Array.isArray(g.indices) ? g.indices : new THREE.BufferAttribute(g.indices as Uint32Array, 1)); // a patched group holds typed arrays
    const map = g.uvs && g.uvs.length / 2 === g.positions.length / 3 ? this.textureOf(g.material) : null;
    if (map) geo.setAttribute('uv', new THREE.Float32BufferAttribute(g.uvs!, 2));
    const transparent = g.opacity < 1;
    const mesh = new THREE.Mesh(geo, this.clip(new THREE.MeshLambertMaterial({ color: new THREE.Color(g.color), transparent, opacity: g.opacity, depthWrite: !transparent, polygonOffset: true, polygonOffsetFactor: 0, polygonOffsetUnits: 4 }))); // constant offset: edges win, faces never sink behind nearby surfaces at grazing angles
    mesh.castShadow = !transparent; mesh.receiveShadow = true; mesh.userData.base = { color: g.color, opacity: g.opacity, map };
    return mesh;
  }

  /** Swap what a commit changed into the model: `c` holds the top-level entities `dirty` as they are now
   *  (those that still exist). Material groups the change doesn't touch keep their meshes and GPU buffers. */
  patch(c: CompiledScene, dirty: string[]) {
    const base = this.compiled; if (!base) { this.setCompiled(c); return; }
    const ids = this.expandIds(new Set(dirty)); const bases = new Set([...ids].filter(i => !i.includes('#')));
    const gone = (fid: string) => ids.has(fid) || bases.has(fid.split('#')[0]);
    const key = (g: CompiledScene['groups'][number]) => `${g.material}\u0000${g.back}`;
    const add = new Map(c.groups.map(g => [key(g), g]));
    const groups: CompiledScene['groups'] = [];
    for (const g of base.groups) {
      const a = add.get(key(g)); add.delete(key(g)); const touched = g.ids.some(gone);
      const m = touched || a ? mergeGroup(g, touched ? gone : null, a) : g;
      if (m.indices.length) groups.push(m);
    }
    for (const a of add.values()) if (a.indices.length) groups.push(a);
    // edges: drop the changed ones, append the new
    const E = base.edges; let edges = E;
    if (E.ids.some(gone) || c.edges.ids.length) {
      const keep: number[] = []; E.ids.forEach((id, i) => { if (!gone(id)) keep.push(i); });
      const pos = new Float32Array((keep.length + c.edges.ids.length) * 6); const eids: string[] = new Array(keep.length + c.edges.ids.length);
      const EP = E.positions; keep.forEach((i, j) => { for (let k = 0; k < 6; k++) pos[j * 6 + k] = EP[i * 6 + k]; eids[j] = E.ids[i]; });
      pos.set(c.edges.positions as ArrayLike<number>, keep.length * 6); c.edges.ids.forEach((id, j) => { eids[keep.length + j] = id; });
      edges = { positions: pos as unknown as number[], ids: eids };
    }
    const keepOld = <T extends { id: string }>(old: T[] | undefined, now: T[] | undefined) => [...(old ?? []).filter(x => !gone(x.id)), ...(now ?? [])];
    const finite = (b?: Bounds) => !!b && isFinite(b.min[0]) && b.max[0] >= b.min[0] && (b.max[0] > b.min[0] || b.max[1] > b.min[1] || b.max[2] > b.min[2]);
    const bounds = !finite(c.bounds) ? base.bounds : !finite(base.bounds) ? c.bounds
      : { min: [0, 1, 2].map(i => Math.min(base.bounds.min[i], c.bounds.min[i])) as Vec3, max: [0, 1, 2].map(i => Math.max(base.bounds.max[i], c.bounds.max[i])) as Vec3 };
    this.setCompiled({ ...base, version: c.version, groups, edges, guides: keepOld(base.guides, c.guides), dimensions: keepOld(base.dimensions, c.dimensions), texts: keepOld(base.texts, c.texts), sections: keepOld(base.sections, c.sections), centers: keepOld(base.centers, c.centers), cuts: c.cuts, bounds });
  }

  /** The mesh built for each compiled group, so a patched scene reuses the meshes of the groups it kept. */
  private meshOf = new Map<CompiledScene['groups'][number], THREE.Mesh>();
  setCompiled(c: CompiledScene) {
    this.dropPreview();
    this.compiled = c; this.setClips(c.cuts);
    const old = this.meshOf; this.meshOf = new Map();
    const models = new Set<THREE.Object3D>(this.meshes);
    for (const o of this.modelGroup.children) if (!models.has(o)) disposeObject(o, o === this.backLines); // edges, guides, dimensions, sections… (the back edges share the edges' geometry)
    this.modelGroup.clear(); this.meshes = []; this.groupData.clear(); this.guideObjects = []; this.guideIds.clear(); this.textObjects = []; this.sectionMeshes = []; this.sectionGroups = []; this.faceGeometries.clear();
    for (const g of c.groups) {
      if (!g.indices.length) continue;
      let mesh = old.get(g); old.delete(g);
      if (!mesh) mesh = this.makeMesh(g);
      this.meshOf.set(g, mesh);
      this.modelGroup.add(mesh); this.meshes.push(mesh);
      this.groupData.set(mesh, { ids: g.ids, ranges: g.idRanges, material: g.material, back: g.back });
      this.applyFaceStyle(mesh); this.applyXray(mesh);
    }
    for (const m of old.values()) disposeObject(m); // groups that went: free their GPU buffers
    // edges: one segment list with ids, coloured end by end
    this.edgeIds = c.edges.ids;
    const eg = new THREE.BufferGeometry(); eg.setAttribute('position', new THREE.Float32BufferAttribute(c.edges.positions, 3)); eg.setAttribute('color', this.edgeColors(c));
    this.edgeLines = new THREE.LineSegments(eg, this.clip(new THREE.LineBasicMaterial({ vertexColors: true }))); this.edgeLines.renderOrder = 1; this.edgeLines.visible = this.edgesVisible;
    this.modelGroup.add(this.edgeLines);
    this.backLines = null; this.applyFaceStyles();
    // snapping data
    const seen = new Map<string, THREE.Vector3>(); this.segments = [];
    const P = c.edges.positions;
    const q = (v: number) => Math.round(v * 1e4); // 0.1 mm: points closer than that are one vertex
    for (let i = 0; i + 5 < P.length; i += 6) {
      let a = new THREE.Vector3(P[i], P[i + 1], P[i + 2]), b = new THREE.Vector3(P[i + 3], P[i + 4], P[i + 5]);
      if (this.clips.length) { const s = clipSegment(a, b, this.clips); if (!s) continue; [a, b] = s; } // what the section planes cut away can't be snapped to
      const ka = `${q(a.x)},${q(a.y)},${q(a.z)}`; if (!seen.has(ka)) seen.set(ka, a);
      const kb = `${q(b.x)},${q(b.y)},${q(b.z)}`; if (!seen.has(kb)) seen.set(kb, b);
      this.segments.push({ a, b, id: c.edges.ids[i / 6] });
    }
    this.vertices = [...seen.values()];
    this.vp = new Float32Array(this.vertices.length * 3); this.vertices.forEach((v, i) => { this.vp[i * 3] = v.x; this.vp[i * 3 + 1] = v.y; this.vp[i * 3 + 2] = v.z; });
    this.sa = new Float32Array(this.segments.length * 3); this.sb = new Float32Array(this.segments.length * 3);
    this.segments.forEach((sg, i) => { this.sa[i * 3] = sg.a.x; this.sa[i * 3 + 1] = sg.a.y; this.sa[i * 3 + 2] = sg.a.z; this.sb[i * 3] = sg.b.x; this.sb[i * 3 + 1] = sg.b.y; this.sb[i * 3 + 2] = sg.b.z; });
    this.scr.key = '';
    // guides
    for (const g of c.guides) {
      if (g.direction) {
        const d = V(g.direction).normalize(); const p = V(g.point);
        const line = new THREE.Line(new THREE.BufferGeometry().setFromPoints([p.clone().addScaledVector(d, -500), p.clone().addScaledVector(d, 500)]), new THREE.LineDashedMaterial({ color: COLOR.cyan, dashSize: 0.3, gapSize: 0.2 }));
        line.computeLineDistances(); this.modelGroup.add(line); this.guideObjects.push(line); this.guideIds.set(line, g.id);
      } else {
        const m = new THREE.Mesh(new THREE.SphereGeometry(0.04, 8, 6), new THREE.MeshBasicMaterial({ color: COLOR.cyan })); m.position.copy(V(g.point)); this.modelGroup.add(m); this.guideObjects.push(m); this.guideIds.set(m, g.id);
      }
    }
    // dimensions
    for (const d of c.dimensions) {
      const a = V(d.a), b = V(d.b), o = V(d.offset); const a2 = a.clone().add(o), b2 = b.clone().add(o);
      const mat = new THREE.LineBasicMaterial({ color: this.ink });
      // a radius or diameter: the line across (or out from the centre), then the leader from the curve to the label
      let pts = d.style ? [a, b, b, b2] : [a, a2, b, b2, a2, b2];
      if (d.style === 'angle') { // both legs out to the arc, and the arc between them
        const u = b.clone().sub(a).normalize(), w = o.clone().normalize(), r = a.distanceTo(b); const arc = angleArc(a, u, w, r);
        pts = [a, b, a, a.clone().addScaledVector(w, r)]; for (let i = 0; i + 1 < arc.length; i++) pts.push(arc[i], arc[i + 1]);
      }
      const geo = new THREE.BufferGeometry().setFromPoints(pts);
      const ls = new THREE.LineSegments(geo, mat); ls.userData.ink = true; this.modelGroup.add(ls); this.guideObjects.push(ls); this.guideIds.set(ls, d.id);
      this.textObjects.push({ id: d.id, pos: dimLabel(d).pos });
    }
    for (const t of c.texts) {
      if (t.screen) continue; // screen text: a label only, at a fixed place of the view
      this.textObjects.push({ id: t.id, pos: V(t.position) });
      if (t.anchor) { const l = new THREE.Line(new THREE.BufferGeometry().setFromPoints([V(t.anchor), V(t.position)]), new THREE.LineBasicMaterial({ color: this.ink })); l.userData.ink = true; this.modelGroup.add(l); this.guideObjects.push(l); this.guideIds.set(l, t.id); }
    }
    this.bounds = c.bounds && isFinite(c.bounds.min[0]) && c.bounds.max[0] >= c.bounds.min[0] ? c.bounds : undefined;
    for (const s of c.sections) {
      const { group, sheet } = this.sectionShape(V(s.origin), V(s.normal), s.active ? COLOR.blue : 0x888888, s.active ? 0.07 : 0.04);
      group.visible = this.sectionPlanesVisible; this.modelGroup.add(group); this.sectionMeshes.push(sheet); this.sectionGroups.push(group); this.guideIds.set(sheet, s.id);
    }
    if (this.bounds) this.fitShadows();
    this.renderStaticLabels();
  }
  /** Fit the sun's shadow frustum tightly around the model (plus the figure), so a shadow texel stays
   *  a few millimetres on a house-sized model instead of 4 cm over a fixed 80 m square: no jagged
   *  shadow edges or acne on floors and slabs. */
  private fitShadows() {
    const b = this.bounds!; const c = new THREE.Vector3((b.min[0] + b.max[0]) / 2, (b.min[1] + b.max[1]) / 2, (b.min[2] + b.max[2]) / 2);
    const r = Math.max(3, new THREE.Vector3(b.max[0] - b.min[0], b.max[1] - b.min[1], b.max[2] - b.min[2]).length() / 2 + 1.5);
    const dir = this.sunDir;
    this.sun.target.position.copy(c); this.sun.target.updateMatrixWorld();
    this.sun.position.copy(c).addScaledVector(dir, r + 10);
    const sc = this.sun.shadow.camera; sc.left = -r; sc.right = r; sc.top = r; sc.bottom = -r; sc.near = 0.5; sc.far = 2 * r + 20; sc.updateProjectionMatrix();
    const size = r < 25 ? 4096 : 2048;
    if (this.sun.shadow.mapSize.x !== size) { this.sun.shadow.mapSize.set(size, size); this.sun.shadow.map?.dispose(); (this.sun.shadow as any).map = null; }
    // a texel-scaled normal offset removes acne without detaching shadows from their casters
    this.sun.shadow.normalBias = Math.min(0.05, (2 * r / size) * 1.5); this.sun.shadow.bias = -0.0002;
  }
  /** Where the sun stands (the direction towards it) and how bright it is (1 = full daylight). */
  sunDir = new THREE.Vector3(20, 32, 14).normalize();
  setSun(dir: THREE.Vector3, strength = 1) {
    this.sunDir.copy(dir).normalize(); this.sun.intensity = 1.4 * strength;
    if (this.bounds) this.fitShadows(); else this.sun.position.copy(this.sunDir).multiplyScalar(50);
  }
  /** Write the dimension and text labels again (the unit of length changed). */
  relabel() { this.renderStaticLabels(); }
  private renderStaticLabels() {
    for (const el of Array.from(this.labels.querySelectorAll('.vlabel.static'))) el.remove();
    for (const d of this.compiled?.dimensions ?? []) { const l = dimLabel(d); this.addLabel(l.pos, l.text, 'static dim', d.id); }
    for (const t of this.compiled?.texts ?? []) { const el = this.addLabel(V(t.position), t.text, t.screen ? 'static text screen' : 'static text', t.id); if (t.screen) (el as any).__screen = t.screen; }
  }

  ndc(ev: { clientX: number; clientY: number }): THREE.Vector2 {
    const r = this.renderer.domElement.getBoundingClientRect();
    return new THREE.Vector2(((ev.clientX - r.left) / r.width) * 2 - 1, -((ev.clientY - r.top) / r.height) * 2 + 1);
  }
  private ray(ev: { clientX: number; clientY: number }) { this.raycaster.setFromCamera(this.ndc(ev), this.cam.camera); return this.raycaster; }

  /** The face under the cursor, remembered for the same position, view and model (a raycast costs milliseconds on a big model). */
  private faceCache: { x: number; y: number; cam: string; model: unknown; style: FaceStyle; hit: Pick | null } | null = null;
  pickFace(ev: { clientX: number; clientY: number }): Pick | null {
    const cam = this.cam.camera.matrixWorld.elements.join(','); const fc = this.faceCache;
    if (fc && fc.x === ev.clientX && fc.y === ev.clientY && fc.cam === cam && fc.model === this.compiled && fc.style === this.faceStyle) return fc.hit;
    const hit = this.pick(ev, { facesOnly: true }); this.faceCache = { x: ev.clientX, y: ev.clientY, cam, model: this.compiled, style: this.faceStyle, hit }; return hit;
  }
  private sightRay = new THREE.Raycaster();
  /** The first opaque, visible face on the line of sight through a world point, and how far along that line the point is. */
  private sightHit(p: THREE.Vector3, ignore?: (id: string) => boolean): { t: number; hit: { distance: number; point: THREE.Vector3; normal: THREE.Vector3; id: string } | null } {
    const ndc = p.clone().project(this.cam.camera); this.sightRay.setFromCamera(new THREE.Vector2(ndc.x, ndc.y), this.cam.camera);
    const r = this.sightRay.ray; const t = p.clone().sub(r.origin).dot(r.direction);
    for (const h of this.sightRay.intersectObjects(this.meshes, false)) {
      const m = h.object as THREE.Mesh; if (!m.visible || (m.material as THREE.Material).transparent || !this.kept(h.point)) continue;
      const d = this.groupData.get(m); if (!d) continue; const first = (h.faceIndex ?? 0) * 3; let id = '';
      for (let i = 0; i < d.ranges.length; i++) { const [s0, n] = d.ranges[i]; if (first >= s0 && first < s0 + n) { id = d.ids[i]; break; } }
      if (ignore?.(id)) continue;
      const normal = h.face ? h.face.normal.clone().transformDirection(m.matrixWorld) : new THREE.Vector3(0, 1, 0); if (d.back) normal.negate();
      return { t, hit: { distance: h.distance, point: h.point.clone(), normal, id } };
    }
    return { t, hit: null };
  }
  /** A picture of how far away the visible faces are, one value per CSS pixel, redrawn when the view or the model changes:
   *  telling whether a point is hidden is then a lookup instead of a raycast (milliseconds each on a big model). */
  private depth: { cam: number[]; proj: number[]; w: number; h: number; model: unknown; vis: string; data: Uint8Array } | null = null;
  private depthTarget: THREE.WebGLRenderTarget | null = null;
  private depthMat = this.clip(new THREE.ShaderMaterial({
    side: THREE.DoubleSide, clipping: true, // what the section planes cut away hides nothing
    vertexShader: '#include <clipping_planes_pars_vertex>\nvarying float vDist; void main() { vec4 mvPosition = modelViewMatrix * vec4(position, 1.0); vDist = -mvPosition.z; gl_Position = projectionMatrix * mvPosition;\n#include <clipping_planes_vertex>\n}',
    // the distance in tenths of a millimetre over three bytes (up to 1.6 km)
    fragmentShader: '#include <clipping_planes_pars_fragment>\nvarying float vDist; void main() {\n#include <clipping_planes_fragment>\nfloat u = clamp(floor(vDist * 10000.0 + 0.5), 0.0, 16777214.0); float r = floor(u / 65536.0); float g = floor((u - r * 65536.0) / 256.0); float b = u - r * 65536.0 - g * 256.0; gl_FragColor = vec4(r, g, b, 255.0) / 255.0; }',
  }));
  private depthMap() {
    const c = this.cam.camera; c.updateMatrixWorld(); const w = Math.max(1, Math.round(this.width)), h = Math.max(1, Math.round(this.height));
    const solid = (m: THREE.Mesh) => (m.visible || this.swapped.has(m)) && !(m.material as THREE.Material).transparent; // a mesh a preview stands in for still hides what is behind it
    const vis = this.meshes.map(m => (solid(m) ? '1' : '0')).join('');
    const same = (a: number[], b: ArrayLike<number>) => { for (let i = 0; i < 16; i++) if (a[i] !== b[i]) return false; return true; };
    const d = this.depth;
    if (d && d.w === w && d.h === h && d.model === this.compiled && d.vis === vis && same(d.cam, c.matrixWorld.elements) && same(d.proj, c.projectionMatrix.elements)) return d;
    if (!this.depthTarget || this.depthTarget.width !== w || this.depthTarget.height !== h) { this.depthTarget?.dispose(); this.depthTarget = new THREE.WebGLRenderTarget(w, h); }
    const r = this.renderer; const prevTarget = r.getRenderTarget(); const prevColor = r.getClearColor(new THREE.Color()); const prevAlpha = r.getClearAlpha();
    const meshes = new Set<THREE.Object3D>(this.meshes); const kids = this.modelGroup.children.map(k => [k, k.visible] as const);
    const mats = this.meshes.map(m => [m, m.material, solid(m)] as const);
    for (const [k] of kids) if (!meshes.has(k)) k.visible = false;
    for (const [m, , sol] of mats) { m.material = this.depthMat; m.visible = sol; }
    try {
      r.setRenderTarget(this.depthTarget); r.setClearColor(0xffffff, 1); r.clear(true, true, true); r.render(this.modelGroup, c);
      const data = new Uint8Array(w * h * 4); r.readRenderTargetPixels(this.depthTarget, 0, 0, w, h, data);
      this.depth = { cam: [...c.matrixWorld.elements], proj: [...c.projectionMatrix.elements], w, h, model: this.compiled, vis, data };
    } finally {
      for (const [m, mat] of mats) m.material = mat; for (const [k, v] of kids) k.visible = v;
      r.setRenderTarget(prevTarget); r.setClearColor(prevColor, prevAlpha);
    }
    return this.depth!;
  }
  /** Whether an opaque face hides the point from the camera (nothing hides in X-ray or wireframe). A point is seen when
   *  it is no farther than the nearest face at its pixel or at one next to it (so the edges of a face stay seen). */
  hidden(p: THREE.Vector3): boolean {
    if (this.faceStyle === 'xray' || this.faceStyle === 'wireframe' || !this.meshes.length) return false;
    const c = this.cam.camera; const ndc = p.clone().project(c); if (ndc.z < -1 || ndc.z > 1 || Math.abs(ndc.x) > 1 || Math.abs(ndc.y) > 1) return false;
    const m = this.depthMap(); const x = Math.floor(((ndc.x + 1) / 2) * m.w), y = Math.floor(((ndc.y + 1) / 2) * m.h);
    const dist = -p.clone().applyMatrix4(c.matrixWorldInverse).z; const tol = 1e-3 + Math.abs(dist) * 2e-4;
    for (let dy = -1; dy <= 1; dy++) for (let dx = -1; dx <= 1; dx++) {
      const X = x + dx, Y = y + dy; if (X < 0 || Y < 0 || X >= m.w || Y >= m.h) return false;
      const i = (Y * m.w + X) * 4; const at = (m.data[i] * 65536 + m.data[i + 1] * 256 + m.data[i + 2]) / 10000;
      if (at >= dist - tol) return false;
    }
    return true;
  }
  /** The normal of the visible face that holds this point (seen from the camera), if any. */
  faceAt(p: THREE.Vector3): THREE.Vector3 | null {
    const { t, hit } = this.sightHit(p); if (!hit) return null;
    return Math.abs(hit.distance - t) < 1e-3 + Math.abs(t) * 2e-4 ? hit.normal : null;
  }

  /** What is under the cursor: an edge within a few pixels wins over a face. */
  pick(ev: { clientX: number; clientY: number }, opts?: { ignore?: Set<string>; facesOnly?: boolean; only?: (id: string) => boolean }): Pick | null {
    const rc = this.ray(ev);
    const skip = (id: string) => !!opts?.ignore?.has(id) || (!!opts?.only && !opts.only(id)); // `only`: the rest is see-through
    rc.params.Line = { threshold: this.pixelSize() * 8 }; // (in pixels: the parallel projection looks from very far away)
    const faceHits = this.faceStyle === 'wireframe' ? rc.intersectObjects(this.meshes.filter(m => m.visible), false) : rc.intersectObjects(this.meshes, false); // wireframe: only edges can be picked
    let face: Pick | null = null;
    for (const h of faceHits) {
      if (!this.kept(h.point)) continue; // cut away by a section plane
      const d = this.groupData.get(h.object as THREE.Mesh)!; const first = (h.faceIndex ?? 0) * 3; let id = '';
      for (let i = 0; i < d.ranges.length; i++) { const [s, n] = d.ranges[i]; if (first >= s && first < s + n) { id = d.ids[i]; break; } }
      if (skip(id)) continue;
      const normal = h.face ? h.face.normal.clone().transformDirection(h.object.matrixWorld) : new THREE.Vector3(0, 1, 0);
      face = { id, kind: 'face', point: h.point, normal, material: d.material, back: d.back }; break;
    }
    if (opts?.facesOnly) return face;
    if (this.edgeLines) {
      const eh = rc.intersectObject(this.edgeLines, false);
      for (const h of eh) {
        const id = this.edgeIds[(h.index ?? 0) / 2 | 0]; if (!id || skip(id) || !this.kept(h.point)) continue;
        if (!face || h.distance <= face.point.distanceTo(this.cam.camera.position) + 0.02) return { id, kind: 'edge', point: h.point, normal: new THREE.Vector3(0, 1, 0) };
        break;
      }
    }
    if (!opts?.facesOnly) {
      const sh = this.sectionPlanesVisible ? rc.intersectObjects(this.sectionMeshes, false)[0] : undefined; // hidden planes can't be clicked
      if (sh) { const id = this.guideIds.get(sh.object); if (id && !skip(id) && (!face || sh.distance < face.point.distanceTo(this.cam.camera.position) - 0.05)) return { id, kind: 'section', point: sh.point, normal: new THREE.Vector3(0, 0, 1).applyQuaternion(sh.object.quaternion) }; }
    }
    const gh = rc.intersectObjects(this.guideObjects, false);
    for (const h of gh) { const id = this.guideIds.get(h.object); if (id && !skip(id)) { const kind = this.compiled?.guides.some(g => g.id === id) ? 'guide' : this.compiled?.dimensions.some(d => d.id === id) ? 'dimension' : 'text'; if (!face || h.distance < face.point.distanceTo(this.cam.camera.position)) return { id, kind, point: h.point, normal: new THREE.Vector3(0, 1, 0) }; } }
    return face;
  }
  pickSection(ev: { clientX: number; clientY: number }): string | null {
    if (!this.sectionPlanesVisible) return null; const h = this.ray(ev).intersectObjects(this.sectionMeshes, false)[0]; return h ? this.guideIds.get(h.object) ?? null : null;
  }
  /** Ray ∩ arbitrary plane. */
  planePoint(ev: { clientX: number; clientY: number }, origin: THREE.Vector3, normal: THREE.Vector3): THREE.Vector3 | null {
    const rc = this.ray(ev); const plane = new THREE.Plane().setFromNormalAndCoplanarPoint(normal.clone().normalize(), origin);
    const out = new THREE.Vector3(); return rc.ray.intersectPlane(plane, out) ? out : null;
  }
  groundPoint(ev: { clientX: number; clientY: number }, y = 0) { return this.planePoint(ev, new THREE.Vector3(0, y, 0), new THREE.Vector3(0, 1, 0)); }
  /** Closest point on a 3D line (through origin along dir) to the pointer ray — for axis-locked moves. */
  linePoint(ev: { clientX: number; clientY: number }, origin: THREE.Vector3, dir: THREE.Vector3): THREE.Vector3 {
    const rc = this.ray(ev); const r0 = rc.ray.origin, r1 = rc.ray.direction; const d = dir.clone().normalize();
    const w0 = origin.clone().sub(r0); const a = d.dot(d), b = d.dot(r1), c = r1.dot(r1), dd = d.dot(w0), e = r1.dot(w0);
    const den = a * c - b * b; const s = den < 1e-9 ? 0 : (b * e - c * dd) / den;
    return origin.clone().addScaledVector(d, s);
  }
  /** World-space size of one pixel at the given depth point (for pixel-sized snap radii). */
  pixelSize(at?: THREE.Vector3): number {
    const dist = at ? at.distanceTo(this.cam.camera.position) : this.cam.distance;
    return (2 * Math.tan((this.cam.camera.fov * Math.PI) / 360) * dist) / this.height;
  }
  screenPoint(p: THREE.Vector3): THREE.Vector2 { const v = p.clone().project(this.cam.camera); return new THREE.Vector2(((v.x + 1) / 2) * this.width, ((1 - v.y) / 2) * this.height); }

  // ---- snap index ---------------------------------------------------------------------------
  private static project(src: Float32Array, dst: Float32Array, m: THREE.Matrix4, w: number, h: number) {
    const e = m.elements;
    for (let i = 0; i < src.length; i += 3) {
      const x = src[i], y = src[i + 1], z = src[i + 2];
      const cw = e[3] * x + e[7] * y + e[11] * z + e[15];
      if (cw <= 1e-6) { dst[i] = NaN; dst[i + 1] = NaN; dst[i + 2] = -1; continue; } // behind the camera
      dst[i] = ((e[0] * x + e[4] * y + e[8] * z + e[12]) / cw + 1) * 0.5 * w;
      dst[i + 1] = (1 - (e[1] * x + e[5] * y + e[9] * z + e[13]) / cw) * 0.5 * h;
      dst[i + 2] = cw;
    }
  }
  /** Re-project vertices and segment endpoints to pixels when the camera or the viewport changed. */
  private screenCache() {
    const c = this.cam.camera; const key = `${c.matrixWorld.elements.join(',')}|${c.fov}|${this.width}x${this.height}`;
    if (this.scr.key === key) return this.scr;
    c.updateMatrixWorld(); const vpm = new THREE.Matrix4().multiplyMatrices(c.projectionMatrix, c.matrixWorldInverse);
    if (this.scr.verts.length !== this.vp.length) this.scr.verts = new Float32Array(this.vp.length);
    if (this.scr.segA.length !== this.sa.length) { this.scr.segA = new Float32Array(this.sa.length); this.scr.segB = new Float32Array(this.sb.length); }
    Viewport.project(this.vp, this.scr.verts, vpm, this.width, this.height);
    Viewport.project(this.sa, this.scr.segA, vpm, this.width, this.height);
    Viewport.project(this.sb, this.scr.segB, vpm, this.width, this.height);
    this.scr.key = key; return this.scr;
  }
  /** Closest model vertex within `r` pixels of the cursor. */
  nearestVertex(cx: number, cy: number, r: number, ok?: (p: THREE.Vector3) => boolean): { point: THREE.Vector3; d: number } | null {
    const s = this.screenCache(); const cand: { i: number; d: number }[] = [];
    for (let i = 0; i < s.verts.length; i += 3) { const dx = s.verts[i] - cx; if (dx > r || dx < -r) continue; const dy = s.verts[i + 1] - cy; const d = Math.hypot(dx, dy); if (d < r) cand.push({ i, d }); }
    cand.sort((a, b) => a.d - b.d);
    for (const { i, d } of cand) { const p = new THREE.Vector3(this.vp[i], this.vp[i + 1], this.vp[i + 2]); if (!ok || ok(p)) return { point: p, d }; }
    return null;
  }
  /** Closest segment midpoint within `r` pixels. */
  nearestMidpoint(cx: number, cy: number, r: number, ignore?: (id: string) => boolean, ok?: (p: THREE.Vector3) => boolean): { point: THREE.Vector3; id: string; dir: THREE.Vector3; d: number } | null {
    const s = this.screenCache(); const cand: { i: number; d: number }[] = [];
    for (let i = 0; i < s.segA.length; i += 3) {
      if (s.segA[i + 2] <= 0 || s.segB[i + 2] <= 0) continue;
      const mx = (s.segA[i] + s.segB[i]) / 2, my = (s.segA[i + 1] + s.segB[i + 1]) / 2; const dx = mx - cx; if (dx > r || dx < -r) continue;
      const d = Math.hypot(dx, my - cy); if (d < r && !ignore?.(this.segments[i / 3].id)) cand.push({ i, d });
    }
    cand.sort((a, b) => a.d - b.d);
    for (const { i, d } of cand) { const sg = this.segments[i / 3]; const p = sg.a.clone().lerp(sg.b, 0.5); if (!ok || ok(p)) return { point: p, id: sg.id, dir: sg.b.clone().sub(sg.a).normalize(), d }; }
    return null;
  }
  /** Closest point on any segment within `r` pixels: 2D prefilter on the projected segment, exact 3D closest-point only for candidates. */
  nearestEdge(ev: { clientX: number; clientY: number }, cx: number, cy: number, r: number, ignore?: (id: string) => boolean, ok?: (p: THREE.Vector3) => boolean): { point: THREE.Vector3; id: string; dir: THREE.Vector3; d: number } | null {
    const s = this.screenCache(); const rc = this.ray(ev); const ro = rc.ray.origin.clone(), rd = rc.ray.direction.clone();
    const cand: { point: THREE.Vector3; id: string; dir: THREE.Vector3; d: number }[] = []; const bd = r;
    const u = new THREE.Vector3(), w0 = new THREE.Vector3(), p = new THREE.Vector3();
    for (let i = 0; i < s.segA.length; i += 3) {
      if (s.segA[i + 2] <= 0 || s.segB[i + 2] <= 0) continue;
      // 2D distance from the cursor to the projected segment
      const ax = s.segA[i], ay = s.segA[i + 1], bx = s.segB[i], by = s.segB[i + 1];
      if (Math.min(ax, bx) > cx + r || Math.max(ax, bx) < cx - r || Math.min(ay, by) > cy + r || Math.max(ay, by) < cy - r) continue;
      const vx = bx - ax, vy = by - ay; const L2 = vx * vx + vy * vy || 1e-9; let t2 = ((cx - ax) * vx + (cy - ay) * vy) / L2; t2 = Math.max(0, Math.min(1, t2));
      const d2 = Math.hypot(ax + vx * t2 - cx, ay + vy * t2 - cy); if (d2 >= bd) continue;
      const sg = this.segments[i / 3]; if (ignore?.(sg.id)) continue;
      // exact: closest point on the 3D segment to the pointer ray
      u.subVectors(sg.b, sg.a); w0.subVectors(sg.a, ro); const a = u.dot(u), b = u.dot(rd), d = u.dot(w0), e = rd.dot(w0);
      const den = a - b * b; if (den < 1e-9) continue; let t = (b * e - d) / den; t = Math.max(0, Math.min(1, t));
      p.copy(sg.a).addScaledVector(u, t);
      cand.push({ point: p.clone(), id: sg.id, dir: u.clone().normalize(), d: d2 });
    }
    cand.sort((a, b) => a.d - b.d);
    return cand.find(c => !ok || ok(c.point)) ?? null;
  }

  /** Where the infinite line (p, d) crosses a model edge within `r` pixels of the cursor: the "Intersection" inference. */
  lineEdgeHit(p: THREE.Vector3, d: THREE.Vector3, cx: number, cy: number, r: number, ignore?: (id: string) => boolean, ok?: (p: THREE.Vector3) => boolean): { point: THREE.Vector3; id: string; d: number } | null {
    const s = this.screenCache(); let best: { point: THREE.Vector3; id: string; d: number } | null = null; let bd = r;
    const u = new THREE.Vector3(), w0 = new THREE.Vector3(), P = new THREE.Vector3(), Q = new THREE.Vector3();
    for (let i = 0; i < s.segA.length; i += 3) {
      if (s.segA[i + 2] <= 0 || s.segB[i + 2] <= 0) continue;
      const ax = s.segA[i], ay = s.segA[i + 1], bx = s.segB[i], by = s.segB[i + 1];
      if (Math.min(ax, bx) > cx + r || Math.max(ax, bx) < cx - r || Math.min(ay, by) > cy + r || Math.max(ay, by) < cy - r) continue;
      const sg = this.segments[i / 3]; if (ignore?.(sg.id)) continue;
      u.subVectors(sg.b, sg.a); const L = u.length(); if (L < 1e-9) continue; u.divideScalar(L);
      // closest points of the two lines; a crossing when they (nearly) meet inside the segment
      w0.subVectors(p, sg.a); const b = d.dot(u); const den = 1 - b * b; if (den < 1e-6) continue;
      const dd = d.dot(w0), e = u.dot(w0); const sp = (b * e - dd) / den, t = (e - b * dd) / den;
      if (t < -1e-6 || t > L + 1e-6) continue;
      P.copy(p).addScaledVector(d, sp); Q.copy(sg.a).addScaledVector(u, t); if (P.distanceTo(Q) > 1e-4) continue;
      const dpx = this.screenPoint(P).distanceTo(new THREE.Vector2(cx, cy));
      if (dpx < bd && (!ok || ok(P))) { bd = dpx; best = { point: P.clone(), id: sg.id, d: dpx }; }
    }
    return best;
  }

  /** Blue highlight for faces and edges. */
  /** Set by the app: a group id stands for everything inside it (the compiled faces carry their own ids). */
  expandIds: (ids: Set<string>) => Set<string> = ids => ids;
  highlight(ids: Set<string>, hover = false) {
    const target = hover ? this.hoverGroup : this.selectionGroup; target.clear();
    if (!hover) this.lastSelection = new Set(ids);
    if (!ids.size) return;
    ids = this.expandIds(ids);
    // plugin geometry is compiled as `<entityId>#<faceKey>`: an entity id matches all its faces, a full id just that face
    const bases = new Set<string>(); for (const id of ids) if (!id.includes('#')) bases.add(id);
    const gone = this.previewHit; // under a preview: not where the old geometry was
    const hit = (fid: string) => (ids.has(fid) || (bases.size > 0 && bases.has(fid.split('#')[0]))) && !gone?.(fid);
    for (const mesh of this.meshes) {
      if (!mesh.visible && !this.swapped.has(mesh)) continue; // wireframe: only the edges light up
      const d = this.groupData.get(mesh)!; const src = mesh.geometry.getIndex()!.array; const sub: number[] = [];
      d.ranges.forEach(([s, n], i) => { if (hit(d.ids[i])) for (let k = s; k < s + n; k++) sub.push(src[k]); });
      if (!sub.length) continue;
      const geo = new THREE.BufferGeometry(); geo.setAttribute('position', mesh.geometry.getAttribute('position')); geo.setIndex(sub);
      const fill = new THREE.Mesh(geo, this.clip(new THREE.MeshBasicMaterial({ color: hover ? COLOR.hover : COLOR.select, transparent: true, opacity: hover ? 0.16 : 0.3, depthWrite: false, polygonOffset: true, polygonOffsetFactor: -2, polygonOffsetUnits: -2, side: THREE.DoubleSide })));
      fill.renderOrder = 3; target.add(fill);
    }
    const pts: THREE.Vector3[] = [];
    for (const s of this.segments) if (hit(s.id)) pts.push(s.a, s.b);
    if (pts.length) { const l = new THREE.LineSegments(new THREE.BufferGeometry().setFromPoints(pts), new THREE.LineBasicMaterial({ color: hover ? COLOR.hover : COLOR.select, linewidth: 2 })); l.renderOrder = 4; target.add(l); }
    for (const [obj, id] of this.guideIds) if (hit(id) && (obj as any).isMesh && this.sectionMeshes.includes(obj as THREE.Mesh)) { const m = obj as THREE.Mesh; const c = new THREE.Mesh(m.geometry, new THREE.MeshBasicMaterial({ color: hover ? COLOR.hover : COLOR.select, transparent: true, opacity: hover ? 0.12 : 0.22, side: THREE.DoubleSide, depthWrite: false })); c.position.copy(m.position); c.quaternion.copy(m.quaternion); c.scale.copy(m.scale); c.renderOrder = 3; target.add(c); }
    for (const [obj, id] of this.guideIds) if (hit(id) && (obj as any).isLine) { const l = new THREE.Line((obj as THREE.Line).geometry, new THREE.LineBasicMaterial({ color: COLOR.select })); l.renderOrder = 4; target.add(l); }
  }

  /** The inference marker under the cursor, drawn by the app for every tool on its own layer
   *  (tools clearing their overlay don't touch it): a ringed dot / square by kind, a cross on the
   *  bare working plane, and its label. `null` hides it. */
  setSnapMarker(s: { point: THREE.Vector3; kind: string; axis?: string; refs?: { from: THREE.Vector3; color: number }[] } | null, color = 0x888888, label = '') {
    this.snapGroup.clear();
    if (!s) { if (this.snapEl) this.snapEl.style.display = 'none'; return; }
    const p = s.point; const px = this.pixelSize(p);
    // dotted inference lines from the reference points (SketchUp's "from point" lines)
    for (const r of s.refs ?? []) {
      if (r.from.distanceTo(p) < 1e-6) continue;
      const l = new THREE.Line(new THREE.BufferGeometry().setFromPoints([r.from, p]), new THREE.LineDashedMaterial({ color: r.color, dashSize: px * 5, gapSize: px * 4, depthTest: false, transparent: true }));
      l.computeLineDistances(); l.renderOrder = 6; this.snapGroup.add(l);
    }
    const flat = (geo: THREE.BufferGeometry, c: number, order: number) => { const m = new THREE.Mesh(geo, new THREE.MeshBasicMaterial({ color: c, depthTest: false, transparent: true })); m.position.copy(p); m.quaternion.copy(this.cam.camera.quaternion); m.renderOrder = order; this.snapGroup.add(m); };
    if (s.kind === 'plane' || s.kind === 'free') {
      // a small cross on the working plane: where the click lands when nothing else catches it
      const r = px * 7; const cross = this.ink === COLOR.ink ? 0x3c4148 : this.ink; const pts = [p.clone().add(new THREE.Vector3(-r, 0, 0)), p.clone().add(new THREE.Vector3(r, 0, 0)), p.clone().add(new THREE.Vector3(0, 0, -r)), p.clone().add(new THREE.Vector3(0, 0, r))];
      const l = new THREE.LineSegments(new THREE.BufferGeometry().setFromPoints(pts), new THREE.LineBasicMaterial({ color: cross, depthTest: false })); l.renderOrder = 7; this.snapGroup.add(l);
      flat(new THREE.CircleGeometry(px * 2, 12), cross, 8);
    } else if (s.kind === 'intersection') {
      // a cross, like SketchUp's
      const r = px * 6; const q = this.cam.camera.quaternion; const pts = [new THREE.Vector3(-r, -r, 0), new THREE.Vector3(r, r, 0), new THREE.Vector3(-r, r, 0), new THREE.Vector3(r, -r, 0)].map(v => v.applyQuaternion(q).add(p));
      flat(new THREE.CircleGeometry(px * 9, 24), 0xffffff, 7);
      const l = new THREE.LineSegments(new THREE.BufferGeometry().setFromPoints(pts), new THREE.LineBasicMaterial({ color, depthTest: false, transparent: true })); l.renderOrder = 8; this.snapGroup.add(l); // transparent: drawn in the same pass as the white disc, so it stays on top
    } else if (['edge', 'axis', 'grid', 'from', 'parallel', 'perpendicular', 'square'].includes(s.kind) || (s.kind === 'guide' && (s as any).dir)) {
      flat(new THREE.PlaneGeometry(px * 16, px * 16), 0xffffff, 7); flat(new THREE.PlaneGeometry(px * 11, px * 11), color, 8);
    } else {
      flat(new THREE.CircleGeometry(px * 9, 24), 0xffffff, 7); flat(new THREE.CircleGeometry(px * 6.5, 24), color, 8);
    }
    if (!this.snapEl) { this.snapEl = document.createElement('div'); this.snapEl.className = 'vlabel infer static snap-label'; this.labels.appendChild(this.snapEl); }
    this.snapEl.textContent = label; this.snapEl.style.display = label ? '' : 'none'; (this.snapEl as any).__pos = p.clone();
  }

  /** What a section plane looks like: a translucent sheet sized to the model, its frame, and arrows at
   *  the corners pointing to the side that is cut away (the way SketchUp draws them). */
  sectionShape(origin: THREE.Vector3, normal: THREE.Vector3, color: number, opacity: number, centerArrow = false): { group: THREE.Group; sheet: THREE.Mesh } {
    const b = this.bounds; const size = b ? Math.max(b.max[0] - b.min[0], b.max[2] - b.min[2], b.max[1] - b.min[1]) + 2 : 6;
    const group = new THREE.Group();
    const sheet = new THREE.Mesh(new THREE.PlaneGeometry(size, size), new THREE.MeshBasicMaterial({ color, transparent: true, opacity, side: THREE.DoubleSide, depthWrite: false }));
    sheet.position.copy(origin); sheet.lookAt(origin.clone().add(normal));
    const frame = new THREE.LineSegments(new THREE.EdgesGeometry(sheet.geometry), new THREE.LineBasicMaterial({ color })); frame.position.copy(sheet.position); frame.quaternion.copy(sheet.quaternion);
    group.add(sheet, frame);
    const n = normal.clone().normalize(); const len = size / 12;
    for (const [sx, sy] of [[-1, -1], [1, -1], [1, 1], [-1, 1]]) {
      const corner = new THREE.Vector3(sx * size * 0.46, sy * size * 0.46, 0).applyQuaternion(sheet.quaternion).add(origin);
      group.add(new THREE.ArrowHelper(n, corner, len, color, len * 0.4, len * 0.22));
    }
    if (centerArrow) { const l = Math.min(len, this.pixelSize(origin) * 60); group.add(new THREE.ArrowHelper(n, origin, l, color, l * 0.4, l * 0.22)); }
    return { group, sheet };
  }
  /** Preview of a section plane about to be placed (cleared with the overlay). */
  addSectionPreview(origin: THREE.Vector3, normal: THREE.Vector3) {
    const { group } = this.sectionShape(origin, normal, COLOR.blue, 0.12, true); group.renderOrder = 4; this.overlay.add(group);
  }
  // overlay helpers ------------------------------------------------------------
  clearOverlay() { this.overlay.clear(); for (const el of Array.from(this.labels.querySelectorAll('.vlabel:not(.static)'))) el.remove(); }
  /** Ink asked for by a tool is the theme's ink, or the dark of the edges for something drawn on a face. */
  private themed(color: number, at?: THREE.Vector3) {
    if (color !== COLOR.ink || this.ink === COLOR.ink) return color;
    return at && this.compiled && this.faceStyle !== 'wireframe' && onFace(this.compiled, at.x, at.y, at.z) ? this.edgeColor : this.ink;
  }
  addLine(points: THREE.Vector3[], color = COLOR.ink, dashed = false, width = 1): THREE.Line {
    color = this.themed(color, points.length > 1 ? points[0].clone().lerp(points[1], 0.5) : points[0]);
    const mat = dashed ? new THREE.LineDashedMaterial({ color, dashSize: 0.15, gapSize: 0.1, linewidth: width }) : new THREE.LineBasicMaterial({ color, linewidth: width });
    const line = new THREE.Line(new THREE.BufferGeometry().setFromPoints(points), mat); if (dashed) line.computeLineDistances();
    line.renderOrder = 5; this.overlay.add(line); return line;
  }
  addFace(points: THREE.Vector3[], color = COLOR.select, opacity = 0.18) {
    if (points.length < 3) return;
    // triangulate in the polygon's own plane
    const n = new THREE.Vector3(); for (let i = 0; i < points.length; i++) { const a = points[i], b = points[(i + 1) % points.length]; n.x += (a.y - b.y) * (a.z + b.z); n.y += (a.z - b.z) * (a.x + b.x); n.z += (a.x - b.x) * (a.y + b.y); }
    if (n.lengthSq() < 1e-12) return; n.normalize();
    const u = Math.abs(n.y) < 0.9 ? new THREE.Vector3(0, 1, 0).cross(n).normalize() : new THREE.Vector3(1, 0, 0).cross(n).normalize(); const w = n.clone().cross(u);
    const tris = THREE.ShapeUtils.triangulateShape(points.map(p => new THREE.Vector2(p.dot(u), p.dot(w))), []);
    const pos: number[] = []; for (const t of tris) for (const i of t) pos.push(points[i].x, points[i].y, points[i].z);
    const geo = new THREE.BufferGeometry(); geo.setAttribute('position', new THREE.Float32BufferAttribute(pos, 3));
    const mesh = new THREE.Mesh(geo, new THREE.MeshBasicMaterial({ color, transparent: true, opacity, side: THREE.DoubleSide, depthWrite: false, polygonOffset: true, polygonOffsetFactor: -3, polygonOffsetUnits: -3 }));
    mesh.renderOrder = 4; this.overlay.add(mesh);
  }
  addDot(p: THREE.Vector3, color = COLOR.green, px = 7) {
    const s = this.pixelSize(p) * px; color = this.themed(color, p);
    const mesh = new THREE.Mesh(new THREE.SphereGeometry(s * 0.5, 10, 8), new THREE.MeshBasicMaterial({ color, depthTest: false })); mesh.position.copy(p); mesh.renderOrder = 6; this.overlay.add(mesh);
  }
  addSquare(p: THREE.Vector3, color = COLOR.red, px = 7) {
    const s = this.pixelSize(p) * px; color = this.themed(color, p); const m = new THREE.Mesh(new THREE.BoxGeometry(s, s, s), new THREE.MeshBasicMaterial({ color, depthTest: false })); m.position.copy(p); m.renderOrder = 6; this.overlay.add(m);
  }
  addLabel(p: THREE.Vector3, text: string, cls = '', id?: string) {
    const el = document.createElement('div'); el.className = `vlabel ${cls}`; el.textContent = text; if (id) el.dataset.id = id;
    (el as any).__pos = p.clone(); this.labels.appendChild(el); return el;
  }
  private updateLabels() {
    const w = this.width, h = this.height;
    for (const el of Array.from(this.labels.children) as HTMLElement[]) {
      const at = (el as any).__screen as [number, number] | undefined; // screen text keeps its place in the view
      if (at) { el.style.display = ''; el.style.transform = `translate(${at[0] * w}px, ${at[1] * h}px) translate(-50%, -50%)`; continue; }
      const p = ((el as any).__pos as THREE.Vector3).clone().project(this.cam.camera); const vis = p.z < 1;
      el.style.display = vis ? '' : 'none'; el.style.transform = `translate(${((p.x + 1) / 2) * w}px, ${((1 - p.y) / 2) * h}px) translate(-50%, -140%)`;
    }
  }
  render() {
    const dt = Math.min(this.clock.getDelta(), 0.1); this.cam.tick(dt); this.updateLabels();
    // depth precision follows the viewing distance: near ≈ 0.2 % of it, far generous enough for the ground and site
    const c = this.cam.camera; const d = Math.max(0.5, this.cam.distance); const tele = c.fov < 8; // the parallel projection looks from far away: its depth range lies around the target, not at the lens
    const near = tele ? d * 0.5 : Math.min(0.5, Math.max(0.005, d * 0.002)), far = tele ? d * 2 + 3000 : Math.max(3000, d * 400);
    if (Math.abs(c.near - near) > near * 0.2 || Math.abs(c.far - far) > far * 0.2) { c.near = near; c.far = far; c.updateProjectionMatrix(); }
    this.renderer.render(this.scene, this.cam.camera);
  }
  start() { const loop = () => { this.render(); requestAnimationFrame(loop); }; loop(); }
  /** PNG of the viewport (optionally at another size). */
  screenshot(width?: number, height?: number): { png: string; width: number; height: number } {
    const w0 = this.renderer.domElement.width, h0 = this.renderer.domElement.height;
    if (width && height) { this.renderer.setSize(width, height, false); this.cam.camera.aspect = width / height; this.cam.camera.updateProjectionMatrix(); }
    this.renderer.render(this.scene, this.cam.camera);
    // composite the CSS background (sky/ground gradient) under the transparent canvas
    const c = document.createElement('canvas'); c.width = this.renderer.domElement.width; c.height = this.renderer.domElement.height;
    const ctx = c.getContext('2d')!; const g = ctx.createLinearGradient(0, 0, 0, c.height);
    const css = getComputedStyle(document.documentElement); const cv = (k: string, d: string) => css.getPropertyValue(k).trim() || d;
    g.addColorStop(0, cv('--sky-top', '#a9c7e8')); g.addColorStop(0.42, cv('--sky-mid', '#d7e6f5')); g.addColorStop(0.55, cv('--horizon', '#f4f6f8')); g.addColorStop(0.56, cv('--ground', '#e9e5da')); g.addColorStop(1, cv('--ground', '#e9e5da'));
    ctx.fillStyle = g; ctx.fillRect(0, 0, c.width, c.height); ctx.drawImage(this.renderer.domElement, 0, 0);
    // burn the HTML labels in
    ctx.font = '12px -apple-system, Helvetica, Arial'; ctx.textAlign = 'center';
    const sx = c.width / this.width, sy = c.height / this.height;
    for (const el of Array.from(this.labels.children) as HTMLElement[]) {
      if (el.style.display === 'none') continue; const at = (el as any).__screen as [number, number] | undefined;
      const p = at ? new THREE.Vector3(at[0] * 2 - 1, 1 - at[1] * 2, 0) : ((el as any).__pos as THREE.Vector3).clone().project(this.cam.camera); if (p.z >= 1) continue;
      const x = ((p.x + 1) / 2) * this.width * sx, y = ((1 - p.y) / 2) * this.height * sy - (at ? -4 : 14) * sy; const t = el.textContent ?? ''; const tw = ctx.measureText(t).width + 8;
      ctx.fillStyle = 'rgba(255,255,255,.92)'; ctx.fillRect(x - tw / 2, y - 12, tw, 16); ctx.strokeStyle = '#333'; ctx.strokeRect(x - tw / 2, y - 12, tw, 16); ctx.fillStyle = '#111'; ctx.fillText(t, x, y);
    }
    const png = c.toDataURL('image/png').split(',')[1];
    if (width && height) { this.renderer.setSize(w0, h0, false); this.cam.camera.aspect = w0 / h0; this.cam.camera.updateProjectionMatrix(); }
    return { png, width: c.width, height: c.height };
  }
}

/** Free an object's GPU buffers (and its material unless shared). */
/** The faces and edges of a compiled scene, whole or the part a preview redraws. */
type Drawn = { groups: CompiledScene['groups']; edges: CompiledScene['edges'] };
/** The faces a line can lie on: drawn solid, front side. */
const solid = (c: Drawn) => c.groups.filter(g => !g.back && g.material !== '__hidden' && g.opacity >= 1);
const triangles = (c: Drawn) => solid(c).reduce((t, g) => t + g.indices.length / 3, 0);
/** Whether a point lies on a face of the scene (within a millimetre of its plane). */
function onFace(c: Drawn, x: number, y: number, z: number): boolean {
  for (const g of solid(c)) {
    const V = g.positions, I = g.indices;
    for (let t = 0; t < I.length; t += 3) {
      const a = I[t] * 3, b = I[t + 1] * 3, d = I[t + 2] * 3;
      const ux = V[b] - V[a], uy = V[b + 1] - V[a + 1], uz = V[b + 2] - V[a + 2], vx = V[d] - V[a], vy = V[d + 1] - V[a + 1], vz = V[d + 2] - V[a + 2];
      const nx = uy * vz - uz * vy, ny = uz * vx - ux * vz, nz = ux * vy - uy * vx, nn = nx * nx + ny * ny + nz * nz; if (nn < 1e-18) continue;
      const wx = x - V[a], wy = y - V[a + 1], wz = z - V[a + 2], off = wx * nx + wy * ny + wz * nz; if (off * off > 1e-6 * nn) continue;
      const uu = ux * ux + uy * uy + uz * uz, uv = ux * vx + uy * vy + uz * vz, vv = vx * vx + vy * vy + vz * vz, wu = wx * ux + wy * uy + wz * uz, wv = wx * vx + wy * vy + wz * vz;
      const s = (vv * wu - uv * wv) / nn, r = (uu * wv - uv * wu) / nn;
      if (s >= -1e-3 && r >= -1e-3 && s + r <= 1.001) return true;
    }
  }
  return false;
}
function disposeObject(o: THREE.Object3D, keepGeometry = false) {
  o.traverse(x => {
    const m = x as THREE.Mesh; if (!keepGeometry) m.geometry?.dispose();
    const mat = m.material as THREE.Material | THREE.Material[] | undefined; if (Array.isArray(mat)) mat.forEach(t => t.dispose()); else mat?.dispose?.();
  });
}

type Group = CompiledScene['groups'][number];
/** `g` without the triangles of the ids `drop` matches (null: keep all), with `add` appended: compacted typed arrays. */
function mergeGroup(g: Group, drop: ((id: string) => boolean) | null, add: Group | undefined): Group {
  const P = g.positions, N = g.normals, I = g.indices; const hasN = N.length === P.length;
  const U = g.uvs ?? [], aU = add?.uvs ?? [], hasU = U.length / 2 === P.length / 3 && (!add || aU.length / 2 === add.positions.length / 3); // textured: two coordinates per corner, in what stays and in what comes
  const remap = new Int32Array(P.length / 3).fill(-1); let nv = 0;
  const keep: number[] = []; let ni = 0;
  for (let r = 0; r < g.ids.length; r++) { if (drop?.(g.ids[r])) continue; keep.push(r); const [st, n] = g.idRanges[r]; ni += n; for (let k = st; k < st + n; k++) if (remap[I[k]] < 0) remap[I[k]] = nv++; }
  const aP = add?.positions ?? [], aN = add?.normals ?? [], aI = add?.indices ?? [], aV = aP.length / 3;
  const pos = new Float32Array((nv + aV) * 3), nrm = new Float32Array((nv + aV) * 3), idx = new Uint32Array(ni + aI.length), uvs = new Float32Array(hasU ? (nv + aV) * 2 : 0);
  for (let v = 0; v < remap.length; v++) { const t = remap[v]; if (t < 0) continue; pos[t * 3] = P[v * 3]; pos[t * 3 + 1] = P[v * 3 + 1]; pos[t * 3 + 2] = P[v * 3 + 2]; if (hasN) { nrm[t * 3] = N[v * 3]; nrm[t * 3 + 1] = N[v * 3 + 1]; nrm[t * 3 + 2] = N[v * 3 + 2]; } if (hasU) { uvs[t * 2] = U[v * 2]; uvs[t * 2 + 1] = U[v * 2 + 1]; } }
  const ids: string[] = []; const ranges: [number, number][] = []; let at = 0;
  for (const r of keep) { const [st, n] = g.idRanges[r]; for (let k = 0; k < n; k++) idx[at + k] = remap[I[st + k]]; ids.push(g.ids[r]); ranges.push([at, n]); at += n; }
  if (add) {
    pos.set(aP as ArrayLike<number>, nv * 3);
    if (aN.length === aP.length) nrm.set(aN as ArrayLike<number>, nv * 3);
    if (hasU) uvs.set(aU as ArrayLike<number>, nv * 2);
    for (let k = 0; k < aI.length; k++) idx[at + k] = aI[k] + nv;
    add.ids.forEach((id, j) => { const [st, n] = add.idRanges[j]; ids.push(id); ranges.push([at + st, n]); });
  }
  return { ...g, positions: pos as unknown as number[], normals: nrm as unknown as number[], uvs: hasU ? uvs as unknown as number[] : undefined, indices: idx as unknown as number[], ids, idRanges: ranges };
}

/** The part of segment a–b that every plane keeps (three.js: distance >= 0), or null. */
function clipSegment(a: THREE.Vector3, b: THREE.Vector3, planes: THREE.Plane[]): [THREE.Vector3, THREE.Vector3] | null {
  for (const pl of planes) {
    const da = pl.distanceToPoint(a), db = pl.distanceToPoint(b);
    if (da >= 0 && db >= 0) continue;
    if (da < 0 && db < 0) return null;
    const m = a.clone().lerp(b, da / (da - db));
    if (da < 0) a = m; else b = m;
  }
  return [a, b];
}
