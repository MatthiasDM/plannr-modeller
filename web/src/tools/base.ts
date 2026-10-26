import * as THREE from 'three';
import type { Viewport } from '../scene';
import type { Document, Op, OpResult } from '../model';

export type SnapKind = 'vertex' | 'midpoint' | 'edge' | 'face' | 'guide' | 'axis' | 'grid' | 'plane' | 'free';
export interface Snap { point: THREE.Vector3; kind: SnapKind; id?: string; normal?: THREE.Vector3; axis?: 'x' | 'y' | 'z'; dir?: THREE.Vector3 }
export interface Plane { origin: THREE.Vector3; normal: THREE.Vector3 }
export type AxisLock = 'x' | 'y' | 'z' | null;

export interface SnapOptions {
  /** Work plane for free points (default: ground). */
  plane?: Plane;
  /** Previous point: enables axis inference and axis locking from it. */
  from?: THREE.Vector3;
  lock?: AxisLock;
  ignore?: Set<string>;
  /** Snap onto faces (the "on face" inference). */
  faces?: boolean;
}

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
  /** Called when the axis lock changes (arrow keys). */
  onLock?(ctx: ToolContext): void;
  /** True while the tool is in the middle of an operation (first point placed, dragging…). Modifier-hold shortcuts stay out of the way then. */
  busy?(): boolean;
}

/** Parse "5;4", "5,4", "5x4", "5 4" into numbers (metres; "250cm"/"2500mm" accepted). */
export function parseNumbers(v: string): number[] {
  return v.split(/[;,x×\s]+/).map(s => s.trim()).filter(Boolean).map(s => {
    const m = s.match(/^(-?\d*\.?\d+)\s*(mm|cm|m|°|deg|s)?$/i); if (!m) return NaN;
    const n = parseFloat(m[1]); const u = (m[2] ?? 'm').toLowerCase();
    return u === 'mm' ? n / 1000 : u === 'cm' ? n / 100 : n;
  }).filter(n => !isNaN(n));
}

export const AXIS = { x: new THREE.Vector3(1, 0, 0), y: new THREE.Vector3(0, 1, 0), z: new THREE.Vector3(0, 0, 1) };
export const AXIS_COLOR = { x: 0xd93025, y: 0x1a73e8, z: 0x1e8e3e };
export const INK = 0x1b1b1b, MAGENTA = 0xc8169a, SELECT = 0x2f6fde;

/** Colour a rubber-band line like SketchUp: red/green/blue when parallel to an axis, magenta when snapped. */
export function lineColor(a: THREE.Vector3, b: THREE.Vector3, snap?: Snap): number {
  if (snap?.axis) return AXIS_COLOR[snap.axis];
  const d = b.clone().sub(a); const L = d.length(); if (L < 1e-6) return INK;
  const ax = Math.abs(d.x) / L, ay = Math.abs(d.y) / L, az = Math.abs(d.z) / L;
  if (ax > 0.9999) return AXIS_COLOR.x; if (ay > 0.9999) return AXIS_COLOR.y; if (az > 0.9999) return AXIS_COLOR.z;
  return INK;
}
export const snapColor = (s: Snap) => s.kind === 'vertex' ? 0x1e8e3e : s.kind === 'midpoint' ? 0x18a0b8 : s.kind === 'edge' ? 0xd93025 : s.kind === 'face' ? 0x1a73e8 : s.kind === 'guide' ? 0x18a0b8 : s.kind === 'grid' ? 0x6b7280 : s.axis ? AXIS_COLOR[s.axis] : 0x888888;
export const snapLabel = (s: Snap) => ({ vertex: 'Endpoint', midpoint: 'Midpoint', edge: 'On Edge', face: 'On Face', guide: 'On Guide', grid: 'On Grid', axis: s.axis ? `On ${({ x: 'Red', y: 'Blue', z: 'Green' })[s.axis]} Axis` : '', plane: '', free: '' })[s.kind];

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
