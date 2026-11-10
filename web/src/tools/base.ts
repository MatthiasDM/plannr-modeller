import * as THREE from 'three';
import type { Viewport, Pick } from '../scene';
import type { Document, Op, OpResult } from '../model';

export type SnapKind = 'vertex' | 'midpoint' | 'center' | 'intersection' | 'edge' | 'face' | 'guide' | 'origin' | 'axis' | 'from' | 'parallel' | 'perpendicular' | 'square' | 'grid' | 'plane' | 'free';
export interface Snap {
  point: THREE.Vector3; kind: SnapKind; id?: string; normal?: THREE.Vector3; axis?: 'x' | 'y' | 'z';
  /** Direction of the edge, axis or inference line the point lies on. */
  dir?: THREE.Vector3;
  /** A point on that line (the reference point of a "from point" inference, the previous point of an axis one). */
  origin?: THREE.Vector3;
  /** Dotted inference lines to draw from these reference points to the snap point. */
  refs?: { from: THREE.Vector3; color: number }[];
}
export interface Plane { origin: THREE.Vector3; normal: THREE.Vector3 }
export type AxisLock = 'x' | 'y' | 'z' | null;

export interface SnapOptions {
  /** Work plane for free points (default: ground). */
  plane?: Plane;
  /** Previous point: enables axis inference and axis locking from it. */
  from?: THREE.Vector3;
  lock?: AxisLock;
  ignore?: Set<string>;
  /** Keep to the work plane: inference lines across it (along its normal) are left out. */
  inPlane?: boolean;
  /** Snap onto faces (the "on face" inference). */
  faces?: boolean;
  /** Leave out entities by compiled id, e.g. geometry that only exists in the current preview. */
  ignoreIf?: (id: string) => boolean;
  /** Scale the snap distances (1 = the usual ~10 px to a point; 0.5 for a tool that should only catch a point right under the cursor). */
  reach?: number;
}
/** Parts of a generated object (a framed wall's members) are selected: they can't be moved, turned or scaled
 *  on their own, only the whole object. Shows why and returns true. */
export function partsRefused(ctx: ToolContext): boolean {
  if (![...ctx.selection()].some(id => id.includes('#'))) return false;
  ctx.toast('Members of a generated object move with it. Press Esc twice to close it, then move the whole object.'); return true;
}
/** A snap on a definite point (not a free point on a line, face or plane). */
export const isPointSnap = (s: Snap) => ['vertex', 'midpoint', 'center', 'intersection', 'edge', 'guide', 'origin'].includes(s.kind);

export interface ToolContext {
  scene: Viewport;
  doc(): Document;
  /** Commit ops as one undo step. Resolves with the results after the scene refreshed. */
  ops(ops: Op[], label: string): Promise<OpResult[]>;
  /** Show the document with these ops applied, without committing (pass [] to clear). */
  preview(ops: Op[]): void;
  select(ids: string[], mode?: 'replace' | 'add' | 'toggle'): void;
  selection(): Set<string>;
  /** Resolve a picked id to what Select would select (its top-level group unless entered). */
  resolve(id: string): string;
  setHint(text: string): void;
  setVCB(value: string, label?: string): void;
  snap(ev: { clientX: number; clientY: number }, opts?: SnapOptions): Snap;
  setTool(id: string): void;
  options: { circleSegments: number; gridSnap: boolean; material: string; layer: string };
  toast(text: string): void;
  prompt(label: string, value?: string): Promise<string | null>;
  confirm(text: string, okLabel?: string, danger?: boolean): Promise<boolean>;
  lock: AxisLock;
  /** Normal of the face under the cursor (its visible side), or null over empty space. With `at`, a face
   *  that contains that point wins, also one just beside the cursor (a corner or an edge snapped from outside). */
  faceNormal(ev: { clientX: number; clientY: number }, at?: THREE.Vector3): THREE.Vector3 | null;
  /** The selected entities a tool works on: the whole selection, or those `accept` takes (a plugin's own type). */
  targets(accept?: (id: string) => boolean): string[];
  /** What is under the cursor for a tool that works on an object. With targets selected (see `targets`) only
   *  they can be hovered or picked, everything else is see-through; with none, anything under the cursor.
   *  `anywhere`: ignore the selection (then `accept` alone decides; what it turns down is see-through). Geometry that only exists in the current preview is never picked. */
  pickTarget(ev: { clientX: number; clientY: number }, opts?: { facesOnly?: boolean; accept?: (id: string) => boolean; anywhere?: boolean }): Pick | null;
  /** False for geometry that exists only in the current preview (a tool's own previewed result). pickTarget
   *  never returns such geometry; pass `!inDocument(id)` as a snap's `ignoreIf` to keep it out of inference. */
  inDocument(id: string): boolean;
}

export interface Tool {
  id: string; label: string; hint: string; cursor: string; shortcut?: string; icon?: string;
  activate?(ctx: ToolContext): void;
  deactivate?(ctx: ToolContext): void;
  onDown?(ev: PointerEvent, ctx: ToolContext): void;
  onMove?(ev: PointerEvent, ctx: ToolContext): void;
  onUp?(ev: PointerEvent, ctx: ToolContext): void;
  onDblClick?(ev: MouseEvent, ctx: ToolContext): void;
  /** Return true when the key was consumed. */
  onKey?(ev: KeyboardEvent, ctx: ToolContext): boolean;
  /** Text entered in the Measurements box. */
  onVCB?(value: string, ctx: ToolContext): void;
  /** The arrow keys set the drawing plane (the shape faces that axis) instead of locking a direction. */
  planeLock?: boolean;
  /** Called when the axis lock changes (arrow keys). */
  onLock?(ctx: ToolContext): void;
  /** Shift pressed: a tool that holds its own line (Push/Pull along the face normal) returns the badge text and
   *  keeps to that line while Shift is down (`ev.shiftKey`); null leaves Shift to the snapping (hold the inference). */
  onShiftHold?(ctx: ToolContext): string | null;
  /** True while the tool is in the middle of an operation (first point placed, dragging…). Modifier-hold shortcuts stay out of the way then. */
  busy?(): boolean;
  /** Modifier keys the tool uses itself (Paint: Ctrl fills, Alt samples): holding one alone does not switch to that key's temporary tool. */
  ownKeys?: ('Control' | 'Alt' | 'Shift')[];
}

/** Parse "5;4", "5,4", "5x4", "5 4" into numbers (metres; "250cm"/"2500mm" accepted). */
export function parseNumbers(v: string): number[] {
  return v.split(/[;,x×\s]+/).map(s => s.trim()).filter(Boolean).map(s => {
    const m = s.match(/^(-?\d*\.?\d+)\s*(mm|cm|m|°|deg|s)?$/i); if (!m) return NaN;
    const n = parseFloat(m[1]); const u = (m[2] ?? 'm').toLowerCase();
    return u === 'mm' ? n / 1000 : u === 'cm' ? n / 100 : n;
  }).filter(n => !isNaN(n));
}

/** An angle in degrees: "30", "30°", or a slope as rise:run ("1:6"). */
export function parseAngle(v: string): number | null {
  const m = v.trim().match(/^(-?\d*\.?\d+)\s*:\s*(\d*\.?\d+)$/);
  if (m) { const run = parseFloat(m[2]); return run > 0 ? (Math.atan2(parseFloat(m[1]), run) * 180) / Math.PI : null; }
  const n = parseNumbers(v); return n.length ? n[0] : null;
}
/** "3x", "x3", "*3": that many copies, each one step further; "/3", "3/": that many, dividing the step. */
export function parseArray(v: string): { count: number; divide: boolean } | null {
  const s = v.replace(/\s+/g, '').toLowerCase(); const n = (m: RegExpMatchArray) => Math.min(500, Math.max(1, +(m[1] ?? m[2])));
  let m = s.match(/^(?:[x*](\d+)|(\d+)[x*])$/); if (m) return { count: n(m), divide: false };
  m = s.match(/^(?:\/(\d+)|(\d+)\/)$/); return m ? { count: n(m), divide: true } : null;
}
/** "[x; y; z]": a position; "<x; y; z>": a step from the previous point. Model coordinates, as Entity Info shows them (y is up). */
export function parsePoint(v: string): { relative: boolean; p: THREE.Vector3 } | null {
  const m = v.trim().match(/^([\[<])(.*?)[\]>]?$/); if (!m) return null;
  const n = m[2].split(/[;,\s]+/).filter(Boolean).flatMap(s => parseNumbers(s)); if (n.length !== 3) return null;
  return { relative: m[1] === '<', p: new THREE.Vector3(n[0], n[1], n[2]) };
}
/** Sizes typed with a unit ("2m", "150cm; 80cm"), in metres; null when a value has none (then it is a plain factor). */
export function parseSizes(v: string): number[] | null {
  const parts = v.split(/[;,\s]+/).map(s => s.trim()).filter(Boolean);
  if (!parts.length || !parts.every(s => /^\d*\.?\d+(mm|cm|m)$/i.test(s))) return null;
  return parts.flatMap(s => parseNumbers(s));
}
/** The point lies on the segment, between its ends. */
export const onSegment = (g: { a: THREE.Vector3; b: THREE.Vector3 }, p: THREE.Vector3) => { const ab = g.b.clone().sub(g.a); const t = p.clone().sub(g.a).dot(ab) / ab.lengthSq(); return t > 0 && t < 1 && g.a.clone().addScaledVector(ab, t).distanceTo(p) < 1e-4; };

export const AXIS = { x: new THREE.Vector3(1, 0, 0), y: new THREE.Vector3(0, 1, 0), z: new THREE.Vector3(0, 0, 1) };
export const AXIS_COLOR = { x: 0xd93025, y: 0x1a73e8, z: 0x1e8e3e };
export const INK = 0x1b1b1b, MAGENTA = 0xc8169a, SELECT = 0x2f6fde;

/** Colour a rubber-band line like SketchUp: red/green/blue when parallel to an axis, magenta when snapped. */
export function lineColor(a: THREE.Vector3, b: THREE.Vector3, snap?: Snap): number {
  if (snap?.kind === 'parallel' || snap?.kind === 'perpendicular') return MAGENTA;
  if (snap?.axis && (snap.kind === 'axis' || snap.kind === 'intersection')) return AXIS_COLOR[snap.axis];
  const d = b.clone().sub(a); const L = d.length(); if (L < 1e-6) return INK;
  const ax = Math.abs(d.x) / L, ay = Math.abs(d.y) / L, az = Math.abs(d.z) / L;
  if (ax > 0.9999) return AXIS_COLOR.x; if (ay > 0.9999) return AXIS_COLOR.y; if (az > 0.9999) return AXIS_COLOR.z;
  return INK;
}
export const snapColor = (s: Snap): number => ({
  vertex: 0x1e8e3e, midpoint: 0x18a0b8, center: 0x18a0b8, intersection: INK, edge: 0xd93025, face: 0x1a73e8, guide: 0x18a0b8, origin: INK,
  axis: s.axis ? AXIS_COLOR[s.axis] : 0x888888, from: s.axis ? AXIS_COLOR[s.axis] : 0x888888, parallel: MAGENTA, perpendicular: MAGENTA, square: MAGENTA, grid: 0x6b7280, plane: 0x888888, free: 0x888888,
})[s.kind];
export const AXIS_NAME = { x: 'Red', y: 'Blue', z: 'Green' };
export const snapLabel = (s: Snap): string => ({
  vertex: 'Endpoint', midpoint: 'Midpoint', center: 'Center', intersection: 'Intersection', edge: 'On Edge', face: 'On Face', guide: s.dir ? 'On Guide' : 'Guide Point', origin: 'Origin',
  axis: s.axis ? `On ${AXIS_NAME[s.axis]} Axis` : 'On Line', from: s.axis ? `From Point (${AXIS_NAME[s.axis].toLowerCase()})` : 'From Point', parallel: 'Parallel to Edge', perpendicular: 'Perpendicular to Edge', square: 'Square', grid: 'On Grid', plane: '', free: '',
})[s.kind];

/** Draw the snap marker + label for a snap. */
export function drawSnap(_ctx: ToolContext, _s: Snap) {
  // the app draws the marker of every snap a tool asks for (App.snap → Viewport.setSnapMarker)
}
/** @deprecated kept for reference: the per-tool marker before the app drew it centrally. */
export function drawSnapLocal(ctx: ToolContext, s: Snap) {
  if (s.kind === 'free' || s.kind === 'plane') return;
  if (s.kind === 'edge' || s.kind === 'axis' || s.kind === 'grid') ctx.scene.addSquare(s.point, snapColor(s), s.kind === 'grid' ? 6 : 7); else ctx.scene.addDot(s.point, snapColor(s));
  const l = snapLabel(s); if (l) ctx.scene.addLabel(s.point, l, 'infer');
}
/** The plane a snap implies: the face's plane, or the given default. */
export function planeFromSnap(s: Snap, fallback: Plane): Plane {
  return s.kind === 'face' && s.normal ? { origin: s.point.clone(), normal: s.normal.clone() } : { origin: s.point.clone(), normal: fallback.normal.clone() };
}
export const GROUND: Plane = { origin: new THREE.Vector3(), normal: new THREE.Vector3(0, 1, 0) };
export const arr = (p: THREE.Vector3): [number, number, number] => [+p.x.toFixed(5), +p.y.toFixed(5), +p.z.toFixed(5)];

/** The plane a shape is drawn on, SketchUp-style: the face under the first point, also when that point
 *  snapped to a corner, an edge or a circle centre on it; the ground over empty space. */
export function surfacePlane(ctx: ToolContext, ev: { clientX: number; clientY: number }, s: Snap): Plane {
  const origin = s.point.clone();
  if (s.normal) return { origin, normal: s.normal.clone().normalize() };
  const n = ctx.faceNormal(ev, origin); return { origin, normal: n ? n.normalize() : GROUND.normal.clone() };
}
/** That plane, or the one facing the locked axis (arrow keys) through the same point, turned towards the camera. */
export function withLock(ctx: ToolContext, p: Plane): Plane {
  if (!ctx.lock) return p;
  const n = AXIS[ctx.lock].clone(); if (ctx.lock !== 'y' && n.dot(ctx.scene.cam.camera.position.clone().sub(p.origin)) < 0) n.negate();
  return { origin: p.origin.clone(), normal: n };
}
export const toPlane = (p: THREE.Vector3, pl: Plane) => p.clone().sub(pl.normal.clone().multiplyScalar(p.clone().sub(pl.origin).dot(pl.normal)));
/** The colour of the axis a plane faces (red, blue or green), magenta for a sloped plane. */
export function planeColor(pl: Plane): number {
  const n = pl.normal; for (const ax of ['x', 'y', 'z'] as const) if (Math.abs(n.dot(AXIS[ax])) > 0.999) return AXIS_COLOR[ax];
  return MAGENTA;
}
/** A small ring or square lying in the plane at its origin: shows which way the shape will face before the first click. */
export function drawPlaneCursor(ctx: ToolContext, pl: Plane, shape: 'ring' | 'square') {
  const n = pl.normal.clone().normalize(); let u = AXIS.x.clone().sub(n.clone().multiplyScalar(AXIS.x.dot(n)));
  if (u.lengthSq() < 1e-6) u = AXIS.z.clone().sub(n.clone().multiplyScalar(AXIS.z.dot(n)));
  u.normalize(); const w = n.clone().cross(u).normalize(); const r = ctx.scene.pixelSize(pl.origin) * 26; const c = pl.origin;
  const pts: THREE.Vector3[] = [];
  if (shape === 'ring') for (let i = 0; i < 32; i++) { const a = (i / 32) * Math.PI * 2; pts.push(c.clone().addScaledVector(u, Math.cos(a) * r).addScaledVector(w, Math.sin(a) * r)); }
  else for (const [a, b] of [[-1, -1], [1, -1], [1, 1], [-1, 1]]) pts.push(c.clone().addScaledVector(u, a * r * 0.8).addScaledVector(w, b * r * 0.8));
  const col = planeColor(pl); ctx.scene.addLine([...pts, pts[0]], col, false, 2); ctx.scene.addFace(pts, col, 0.12);
}
/** The plane-picking part shared by the shape tools: before the first point it follows the face under
 *  the cursor; once the first point is down it stays, and the arrow keys turn it to face an axis. */
export class DrawPlane {
  /** The plane found under the first point (without the arrow-key lock). */
  auto: Plane = GROUND; plane: Plane = GROUND;
  /** Before the first point: the plane under the cursor, with a cursor showing it. */
  hover(ctx: ToolContext, ev: { clientX: number; clientY: number }, s: Snap, shape: 'ring' | 'square') { const p = withLock(ctx, surfacePlane(ctx, ev, s)); drawPlaneCursor(ctx, p, shape); return p; }
  /** The first point: fix the plane there. */
  start(ctx: ToolContext, ev: { clientX: number; clientY: number }, s: Snap) { this.auto = surfacePlane(ctx, ev, s); this.plane = withLock(ctx, this.auto); return this.plane; }
  /** The arrow keys changed: turn the plane about the first point. */
  relock(ctx: ToolContext) { this.plane = withLock(ctx, this.auto); return this.plane; }
  on(p: THREE.Vector3) { return toPlane(p, this.plane); }
}
