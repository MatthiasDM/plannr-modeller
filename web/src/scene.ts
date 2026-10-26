import * as THREE from 'three';
import type { CompiledScene } from './api';
import type { Vec3 } from './model';

export const V = (p: Vec3) => new THREE.Vector3(p[0], p[1], p[2]);
export const A = (p: THREE.Vector3): Vec3 => [p.x, p.y, p.z];

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
    this.sph.radius = Math.max(0.2, Math.min(1500, this.sph.radius * factor)); this.apply();
  }
  get distance() { return this.sph.radius; }
  fitDistance(b: Bounds): number {
    const r = Math.max(Math.hypot(b.max[0] - b.min[0], b.max[1] - b.min[1], b.max[2] - b.min[2]) / 2, 1.2);
    const vfov = (this.camera.fov * Math.PI) / 180, hfov = 2 * Math.atan(Math.tan(vfov / 2) * this.camera.aspect);
    return (r / Math.sin(Math.min(vfov, hfov) / 2)) * 1.05;
  }
  private centerOf(b?: Bounds) { return b ? new THREE.Vector3((b.min[0] + b.max[0]) / 2, (b.min[1] + b.max[1]) / 2, (b.min[2] + b.max[2]) / 2) : new THREE.Vector3(0, 0.5, 0); }
  setView(kind: ViewName, bounds?: Bounds) {
    const c = this.centerOf(bounds); const r = bounds ? this.fitDistance(bounds) : 18;
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
  private sectionMeshes: THREE.Mesh[] = [];
  private faceGeometries = new Map<string, { pos: THREE.Vector3[]; normal: THREE.Vector3 }>(); // outline + normal per face id for planes
  vertices: THREE.Vector3[] = []; segments: { a: THREE.Vector3; b: THREE.Vector3; id: string }[] = [];
  // snap index: flat arrays + a screen-space projection cache that is rebuilt only when the camera or viewport changes
  private vp = new Float32Array(0); private sa = new Float32Array(0); private sb = new Float32Array(0);
  private scr = { key: '', verts: new Float32Array(0), segA: new Float32Array(0), segB: new Float32Array(0) };
  bounds?: Bounds; compiled?: CompiledScene;
  edgesVisible = true; edgeColor = COLOR.ink; private sun: THREE.DirectionalLight;
  private clock = new THREE.Clock();

  constructor(container: HTMLElement, labels: HTMLElement) {
    this.container = container; this.labels = labels;
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.shadowMap.enabled = true; this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
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
    this.scene.add(this.modelGroup, this.overlay, this.snapGroup, this.selectionGroup, this.hoverGroup);
    this.resize(); new ResizeObserver(() => this.resize()).observe(container);
    this.applyTheme(); matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => this.applyTheme());
  }

  /** Match the ground and grid to the CSS theme (light / dark). */
  applyTheme() {
    const css = getComputedStyle(document.documentElement); const dark = css.getPropertyValue('--dark').trim() === '1';
    (this.ground.material as THREE.MeshLambertMaterial).color.set(css.getPropertyValue('--ground').trim() || '#e9e5da');
    const g = this.grid.material as THREE.LineBasicMaterial; g.color.set(dark ? '#4a4e56' : '#cfcbbf'); g.opacity = dark ? 0.55 : 0.7;
    // dark edges in both themes: they read on the lit faces and still show against the dark sky
    this.edgeColor = dark ? 0x101216 : COLOR.ink;
    if (this.edgeLines) (this.edgeLines.material as THREE.LineBasicMaterial).color.set(this.edgeColor);
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

  setCompiled(c: CompiledScene) {
    this.compiled = c;
    this.modelGroup.clear(); this.meshes = []; this.groupData.clear(); this.guideObjects = []; this.guideIds.clear(); this.textObjects = []; this.sectionMeshes = []; this.faceGeometries.clear();
    for (const g of c.groups) {
      if (!g.indices.length) continue;
      const geo = new THREE.BufferGeometry();
      geo.setAttribute('position', new THREE.Float32BufferAttribute(g.positions, 3));
      if (g.normals?.length === g.positions.length) geo.setAttribute('normal', new THREE.Float32BufferAttribute(g.normals, 3)); else geo.computeVertexNormals();
      geo.setIndex(g.indices);
      const transparent = g.opacity < 1;
      const mesh = new THREE.Mesh(geo, new THREE.MeshLambertMaterial({ color: new THREE.Color(g.color), transparent, opacity: g.opacity, depthWrite: !transparent, polygonOffset: true, polygonOffsetFactor: 0, polygonOffsetUnits: 4 })); // constant offset: edges win, faces never sink behind nearby surfaces at grazing angles
      mesh.castShadow = !transparent; mesh.receiveShadow = true;
      this.modelGroup.add(mesh); this.meshes.push(mesh);
      this.groupData.set(mesh, { ids: g.ids, ranges: g.idRanges, material: g.material, back: g.back });
      this.applyXray(mesh);
    }
    // edges: one black segment list with ids
    this.edgeIds = c.edges.ids;
    const eg = new THREE.BufferGeometry(); eg.setAttribute('position', new THREE.Float32BufferAttribute(c.edges.positions, 3));
    this.edgeLines = new THREE.LineSegments(eg, new THREE.LineBasicMaterial({ color: this.edgeColor })); this.edgeLines.renderOrder = 1; this.edgeLines.visible = this.edgesVisible;
    this.modelGroup.add(this.edgeLines);
    // snapping data
    const seen = new Map<string, THREE.Vector3>(); this.segments = [];
    const P = c.edges.positions;
    for (let i = 0; i + 5 < P.length; i += 6) {
      const a = new THREE.Vector3(P[i], P[i + 1], P[i + 2]), b = new THREE.Vector3(P[i + 3], P[i + 4], P[i + 5]);
      for (const p of [a, b]) { const k = `${p.x.toFixed(4)},${p.y.toFixed(4)},${p.z.toFixed(4)}`; if (!seen.has(k)) seen.set(k, p); }
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
      const mat = new THREE.LineBasicMaterial({ color: COLOR.ink });
      const geo = new THREE.BufferGeometry().setFromPoints([a, a2, b, b2, a2, b2]);
      const ls = new THREE.LineSegments(geo, mat); this.modelGroup.add(ls); this.guideObjects.push(ls); this.guideIds.set(ls, d.id);
      this.textObjects.push({ id: d.id, pos: a2.clone().lerp(b2, 0.5) });
    }
    for (const t of c.texts) {
      this.textObjects.push({ id: t.id, pos: V(t.position) });
      if (t.anchor) { const l = new THREE.Line(new THREE.BufferGeometry().setFromPoints([V(t.anchor), V(t.position)]), new THREE.LineBasicMaterial({ color: COLOR.ink })); this.modelGroup.add(l); this.guideObjects.push(l); this.guideIds.set(l, t.id); }
    }
    this.bounds = c.bounds && isFinite(c.bounds.min[0]) && c.bounds.max[0] >= c.bounds.min[0] ? c.bounds : undefined;
    for (const s of c.sections) {
      const size = this.bounds ? Math.max(this.bounds.max[0] - this.bounds.min[0], this.bounds.max[2] - this.bounds.min[2], this.bounds.max[1] - this.bounds.min[1]) + 2 : 6;
      const m = new THREE.Mesh(new THREE.PlaneGeometry(size, size), new THREE.MeshBasicMaterial({ color: s.active ? COLOR.blue : 0x888888, transparent: true, opacity: s.active ? 0.07 : 0.04, side: THREE.DoubleSide, depthWrite: false }));
      m.position.copy(V(s.origin)); m.lookAt(V(s.origin).add(V(s.normal))); this.modelGroup.add(m); this.sectionMeshes.push(m); this.guideIds.set(m, s.id);
      const frame = new THREE.LineSegments(new THREE.EdgesGeometry(m.geometry), new THREE.LineBasicMaterial({ color: s.active ? COLOR.blue : 0x888888 })); frame.position.copy(m.position); frame.quaternion.copy(m.quaternion); this.modelGroup.add(frame);
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
    const dir = new THREE.Vector3(20, 32, 14).normalize();
    this.sun.target.position.copy(c); this.sun.target.updateMatrixWorld();
    this.sun.position.copy(c).addScaledVector(dir, r + 10);
    const sc = this.sun.shadow.camera; sc.left = -r; sc.right = r; sc.top = r; sc.bottom = -r; sc.near = 0.5; sc.far = 2 * r + 20; sc.updateProjectionMatrix();
    const size = r < 25 ? 4096 : 2048;
    if (this.sun.shadow.mapSize.x !== size) { this.sun.shadow.mapSize.set(size, size); this.sun.shadow.map?.dispose(); (this.sun.shadow as any).map = null; }
    // a texel-scaled normal offset removes acne without detaching shadows from their casters
    this.sun.shadow.normalBias = Math.min(0.05, (2 * r / size) * 1.5); this.sun.shadow.bias = -0.0002;
  }
  private renderStaticLabels() {
    for (const el of Array.from(this.labels.querySelectorAll('.vlabel.static'))) el.remove();
    for (const d of this.compiled?.dimensions ?? []) this.addLabel(V(d.a).add(V(d.offset)).lerp(V(d.b).add(V(d.offset)), 0.5), `${d.value.toFixed(3).replace(/\.?0+$/, '')} m`, 'static dim', d.id);
    for (const t of this.compiled?.texts ?? []) this.addLabel(V(t.position), t.text, 'static text', t.id);
  }

  ndc(ev: { clientX: number; clientY: number }): THREE.Vector2 {
    const r = this.renderer.domElement.getBoundingClientRect();
    return new THREE.Vector2(((ev.clientX - r.left) / r.width) * 2 - 1, -((ev.clientY - r.top) / r.height) * 2 + 1);
  }
  private ray(ev: { clientX: number; clientY: number }) { this.raycaster.setFromCamera(this.ndc(ev), this.cam.camera); return this.raycaster; }

  /** What is under the cursor: an edge within a few pixels wins over a face. */
  pick(ev: { clientX: number; clientY: number }, opts?: { ignore?: Set<string>; facesOnly?: boolean }): Pick | null {
    const rc = this.ray(ev);
    rc.params.Line = { threshold: this.cam.distance * 0.008 };
    const faceHits = rc.intersectObjects(this.meshes, false);
    let face: Pick | null = null;
    for (const h of faceHits) {
      const d = this.groupData.get(h.object as THREE.Mesh)!; const first = (h.faceIndex ?? 0) * 3; let id = '';
      for (let i = 0; i < d.ranges.length; i++) { const [s, n] = d.ranges[i]; if (first >= s && first < s + n) { id = d.ids[i]; break; } }
      if (opts?.ignore?.has(id)) continue;
      const normal = h.face ? h.face.normal.clone().transformDirection(h.object.matrixWorld) : new THREE.Vector3(0, 1, 0);
      face = { id, kind: 'face', point: h.point, normal, material: d.material, back: d.back }; break;
    }
    if (opts?.facesOnly) return face;
    if (this.edgeLines) {
      const eh = rc.intersectObject(this.edgeLines, false);
      for (const h of eh) {
        const id = this.edgeIds[(h.index ?? 0) / 2 | 0]; if (!id || opts?.ignore?.has(id)) continue;
        if (!face || h.distance <= face.point.distanceTo(this.cam.camera.position) + 0.02) return { id, kind: 'edge', point: h.point, normal: new THREE.Vector3(0, 1, 0) };
        break;
      }
    }
    if (!opts?.facesOnly) {
      const sh = rc.intersectObjects(this.sectionMeshes, false)[0];
      if (sh) { const id = this.guideIds.get(sh.object); if (id && !opts?.ignore?.has(id) && (!face || sh.distance < face.point.distanceTo(this.cam.camera.position) - 0.05)) return { id, kind: 'section', point: sh.point, normal: new THREE.Vector3(0, 0, 1).applyQuaternion(sh.object.quaternion) }; }
    }
    const gh = rc.intersectObjects(this.guideObjects, false);
    for (const h of gh) { const id = this.guideIds.get(h.object); if (id && !opts?.ignore?.has(id)) { const kind = this.compiled?.guides.some(g => g.id === id) ? 'guide' : this.compiled?.dimensions.some(d => d.id === id) ? 'dimension' : 'text'; if (!face || h.distance < face.point.distanceTo(this.cam.camera.position)) return { id, kind, point: h.point, normal: new THREE.Vector3(0, 1, 0) }; } }
    return face;
  }
  pickSection(ev: { clientX: number; clientY: number }): string | null {
    const h = this.ray(ev).intersectObjects(this.sectionMeshes, false)[0]; return h ? this.guideIds.get(h.object) ?? null : null;
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
  nearestVertex(cx: number, cy: number, r: number): { point: THREE.Vector3; d: number } | null {
    const s = this.screenCache(); let best = -1, bd = r;
    for (let i = 0; i < s.verts.length; i += 3) { const dx = s.verts[i] - cx; if (dx > r || dx < -r) continue; const dy = s.verts[i + 1] - cy; const d = Math.hypot(dx, dy); if (d < bd) { bd = d; best = i; } }
    return best < 0 ? null : { point: new THREE.Vector3(this.vp[best], this.vp[best + 1], this.vp[best + 2]), d: bd };
  }
  /** Closest segment midpoint within `r` pixels. */
  nearestMidpoint(cx: number, cy: number, r: number, ignore?: (id: string) => boolean): { point: THREE.Vector3; id: string; dir: THREE.Vector3; d: number } | null {
    const s = this.screenCache(); let best = -1, bd = r;
    for (let i = 0; i < s.segA.length; i += 3) {
      if (s.segA[i + 2] <= 0 || s.segB[i + 2] <= 0) continue;
      const mx = (s.segA[i] + s.segB[i]) / 2, my = (s.segA[i + 1] + s.segB[i + 1]) / 2; const dx = mx - cx; if (dx > r || dx < -r) continue;
      const d = Math.hypot(dx, my - cy); if (d < bd && !ignore?.(this.segments[i / 3].id)) { bd = d; best = i; }
    }
    if (best < 0) return null; const sg = this.segments[best / 3];
    return { point: sg.a.clone().lerp(sg.b, 0.5), id: sg.id, dir: sg.b.clone().sub(sg.a).normalize(), d: bd };
  }
  /** Closest point on any segment within `r` pixels: 2D prefilter on the projected segment, exact 3D closest-point only for candidates. */
  nearestEdge(ev: { clientX: number; clientY: number }, cx: number, cy: number, r: number, ignore?: (id: string) => boolean): { point: THREE.Vector3; id: string; dir: THREE.Vector3; d: number } | null {
    const s = this.screenCache(); const rc = this.ray(ev); const ro = rc.ray.origin, rd = rc.ray.direction;
    let best: { point: THREE.Vector3; id: string; dir: THREE.Vector3; d: number } | null = null; let bd = r;
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
      bd = d2; best = { point: p.clone(), id: sg.id, dir: u.clone().normalize(), d: d2 };
    }
    return best;
  }

  /** Blue highlight for faces and edges. */
  highlight(ids: Set<string>, hover = false) {
    const target = hover ? this.hoverGroup : this.selectionGroup; target.clear();
    if (!ids.size) return;
    // plugin geometry is compiled as `<entityId>#<faceKey>`: an entity id matches all its faces, a full id just that face
    const bases = new Set<string>(); for (const id of ids) if (!id.includes('#')) bases.add(id);
    const hit = (fid: string) => ids.has(fid) || (bases.size > 0 && bases.has(fid.split('#')[0]));
    for (const mesh of this.meshes) {
      const d = this.groupData.get(mesh)!; const src = mesh.geometry.getIndex()!.array; const sub: number[] = [];
      d.ranges.forEach(([s, n], i) => { if (hit(d.ids[i])) for (let k = s; k < s + n; k++) sub.push(src[k]); });
      if (!sub.length) continue;
      const geo = new THREE.BufferGeometry(); geo.setAttribute('position', mesh.geometry.getAttribute('position')); geo.setIndex(sub);
      const fill = new THREE.Mesh(geo, new THREE.MeshBasicMaterial({ color: hover ? COLOR.hover : COLOR.select, transparent: true, opacity: hover ? 0.16 : 0.3, depthWrite: false, polygonOffset: true, polygonOffsetFactor: -2, polygonOffsetUnits: -2, side: THREE.DoubleSide }));
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
  setSnapMarker(s: { point: THREE.Vector3; kind: string; axis?: string } | null, color = 0x888888, label = '') {
    this.snapGroup.clear();
    if (!s) { if (this.snapEl) this.snapEl.style.display = 'none'; return; }
    const p = s.point; const px = this.pixelSize(p);
    const flat = (geo: THREE.BufferGeometry, c: number, order: number) => { const m = new THREE.Mesh(geo, new THREE.MeshBasicMaterial({ color: c, depthTest: false, transparent: true })); m.position.copy(p); m.quaternion.copy(this.cam.camera.quaternion); m.renderOrder = order; this.snapGroup.add(m); };
    if (s.kind === 'plane' || s.kind === 'free') {
      // a small cross on the working plane: where the click lands when nothing else catches it
      const r = px * 7; const pts = [p.clone().add(new THREE.Vector3(-r, 0, 0)), p.clone().add(new THREE.Vector3(r, 0, 0)), p.clone().add(new THREE.Vector3(0, 0, -r)), p.clone().add(new THREE.Vector3(0, 0, r))];
      const l = new THREE.LineSegments(new THREE.BufferGeometry().setFromPoints(pts), new THREE.LineBasicMaterial({ color: 0x3c4148, depthTest: false })); l.renderOrder = 7; this.snapGroup.add(l);
      flat(new THREE.CircleGeometry(px * 2, 12), 0x3c4148, 8);
    } else if (s.kind === 'edge' || s.kind === 'axis' || s.kind === 'grid') {
      flat(new THREE.PlaneGeometry(px * 16, px * 16), 0xffffff, 7); flat(new THREE.PlaneGeometry(px * 11, px * 11), color, 8);
    } else {
      flat(new THREE.CircleGeometry(px * 9, 24), 0xffffff, 7); flat(new THREE.CircleGeometry(px * 6.5, 24), color, 8);
    }
    if (!this.snapEl) { this.snapEl = document.createElement('div'); this.snapEl.className = 'vlabel infer static snap-label'; this.labels.appendChild(this.snapEl); }
    this.snapEl.textContent = label; this.snapEl.style.display = label ? '' : 'none'; (this.snapEl as any).__pos = p.clone();
  }

  // overlay helpers ------------------------------------------------------------
  clearOverlay() { this.overlay.clear(); for (const el of Array.from(this.labels.querySelectorAll('.vlabel:not(.static)'))) el.remove(); }
  addLine(points: THREE.Vector3[], color = COLOR.ink, dashed = false, width = 1): THREE.Line {
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
    const s = this.pixelSize(p) * px;
    const mesh = new THREE.Mesh(new THREE.SphereGeometry(s * 0.5, 10, 8), new THREE.MeshBasicMaterial({ color, depthTest: false })); mesh.position.copy(p); mesh.renderOrder = 6; this.overlay.add(mesh);
  }
  addSquare(p: THREE.Vector3, color = COLOR.red, px = 7) {
    const s = this.pixelSize(p) * px; const m = new THREE.Mesh(new THREE.BoxGeometry(s, s, s), new THREE.MeshBasicMaterial({ color, depthTest: false })); m.position.copy(p); m.renderOrder = 6; this.overlay.add(m);
  }
  addLabel(p: THREE.Vector3, text: string, cls = '', id?: string) {
    const el = document.createElement('div'); el.className = `vlabel ${cls}`; el.textContent = text; if (id) el.dataset.id = id;
    (el as any).__pos = p.clone(); this.labels.appendChild(el);
  }
  private updateLabels() {
    const w = this.width, h = this.height;
    for (const el of Array.from(this.labels.children) as HTMLElement[]) {
      const p = ((el as any).__pos as THREE.Vector3).clone().project(this.cam.camera); const vis = p.z < 1;
      el.style.display = vis ? '' : 'none'; el.style.transform = `translate(${((p.x + 1) / 2) * w}px, ${((1 - p.y) / 2) * h}px) translate(-50%, -140%)`;
    }
  }
  render() {
    const dt = Math.min(this.clock.getDelta(), 0.1); this.cam.tick(dt); this.updateLabels();
    // depth precision follows the viewing distance: near ≈ 0.2 % of it, far generous enough for the ground and site
    const c = this.cam.camera; const d = Math.max(0.5, this.cam.distance); const near = Math.min(0.5, Math.max(0.005, d * 0.002)), far = Math.max(3000, d * 400);
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
      if (el.style.display === 'none') continue; const p = ((el as any).__pos as THREE.Vector3).clone().project(this.cam.camera); if (p.z >= 1) continue;
      const x = ((p.x + 1) / 2) * this.width * sx, y = ((1 - p.y) / 2) * this.height * sy - 14 * sy; const t = el.textContent ?? ''; const tw = ctx.measureText(t).width + 8;
      ctx.fillStyle = 'rgba(255,255,255,.92)'; ctx.fillRect(x - tw / 2, y - 12, tw, 16); ctx.strokeStyle = '#333'; ctx.strokeRect(x - tw / 2, y - 12, tw, 16); ctx.fillStyle = '#111'; ctx.fillText(t, x, y);
    }
    const png = c.toDataURL('image/png').split(',')[1];
    if (width && height) { this.renderer.setSize(w0, h0, false); this.cam.camera.aspect = w0 / h0; this.cam.camera.updateProjectionMatrix(); }
    return { png, width: c.width, height: c.height };
  }
}
