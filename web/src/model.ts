// Plannr document model — mirrors docs/CONTRACT.md. Generic geometry only; domain entities come from plugins.
export type Vec3 = [number, number, number];

export interface Layer { id: string; name: string; visible: boolean; color: string }
/** The image of a material: a data: URL (PNG or JPEG), and the metres one copy of it covers. */
export interface Texture { image: string; width: number; height: number }
export interface Material { id: string; name: string; color: string; opacity: number; texture?: Texture | null }

interface Base { id: string; layer: string; [extra: string]: unknown }
export interface FaceEntity extends Base { kind: 'face'; outer: Vec3[]; holes: Vec3[][]; material?: string | null; backMaterial?: string | null; /** Softened: degrees up to which it runs on smoothly into a neighbour. */ smooth?: number }
export interface EdgeEntity extends Base { kind: 'edge'; a: Vec3; b: Vec3 }
export interface GroupEntity extends Base { kind: 'group'; name: string; transform: number[]; entities: Entity[] }
export interface GuideEntity extends Base { kind: 'guide'; point: Vec3; direction?: Vec3 | null }
export interface DimensionEntity extends Base { kind: 'dimension'; a: Vec3; b: Vec3; offset: Vec3; style?: 'radius' | 'diameter' | 'angle' | null }
export interface TextEntity extends Base { kind: 'text'; position: Vec3; text: string; anchor?: Vec3 | null; screen?: [number, number] | null }
export interface SectionEntity extends Base { kind: 'section'; origin: Vec3; normal: Vec3; active: boolean }
export interface CustomEntity extends Base { kind: 'custom'; type: string; data: unknown }
export interface InstanceEntity extends Base { kind: 'instance'; definition: string; transform: number[]; name?: string }
export type Entity = FaceEntity | EdgeEntity | GroupEntity | GuideEntity | DimensionEntity | TextEntity | SectionEntity | CustomEntity | InstanceEntity;
export interface ComponentDefinition { id: string; name: string; description?: string; entities: Entity[] }
export type EntityKind = Entity['kind'];

/** Where the sun stands: by the hour of a day of the year at a latitude, or the fixed studio light. */
export interface Sun { byTime: boolean; hour: number; month: number; latitude: number }
export interface Scene {
  id: string; name: string; camera: number[]; section?: string | null; sections?: string[];
  /** What else the scene brings back: which tags show, the face style, the lens (1 = parallel projection) and the sun. */
  layers?: Record<string, boolean>; faceStyle?: string; fov?: number; sun?: Sun;
}
export interface Document {
  version: number; units: string; layers: Layer[]; materials: Material[]; entities: Entity[]; scenes: Scene[];
  components?: ComponentDefinition[];
  extensions: Record<string, unknown>; [extra: string]: unknown;
}

export type Op =
  | { op: 'draw_line'; points: Vec3[]; close?: boolean; layer?: string; group?: string }
  | { op: 'draw_rectangle'; corner: Vec3; opposite: Vec3; normal: Vec3; u?: Vec3; layer?: string; group?: string }
  | { op: 'draw_polygon'; points: Vec3[]; layer?: string; group?: string }
  | { op: 'draw_circle'; center: Vec3; normal: Vec3; radius: number; segments?: number; layer?: string; group?: string }
  | { op: 'push_pull'; face: string; distance: number; newFace?: boolean }
  | { op: 'offset'; face: string; distance: number }
  | { op: 'move'; ids: string[]; delta: Vec3 }
  | { op: 'stretch'; ids?: string[]; points?: Vec3[]; delta?: Vec3; rotate?: { origin: Vec3; axis: Vec3; angle: number }; scale?: { origin: Vec3; factors: Vec3 }; group?: string }
  | { op: 'rotate'; ids: string[]; origin: Vec3; axis: Vec3; angle: number }
  | { op: 'scale'; ids: string[]; origin: Vec3; factors: Vec3 }
  | { op: 'copy'; ids: string[]; delta?: Vec3; rotate?: { origin: Vec3; axis: Vec3; angle: number }; count?: number }
  | { op: 'erase'; ids: string[] }
  | { op: 'intersect_faces'; ids: string[] }
  | { op: 'paint'; ids: string[]; material: string; side?: 'front' | 'back' | 'both' }
  | { op: 'set_layer'; ids: string[]; layer: string }
  | { op: 'make_group'; ids: string[]; name?: string }
  | { op: 'explode'; id: string }
  | { op: 'add_layer'; name: string; color?: string; visible?: boolean }
  | { op: 'update_layer'; id: string; name?: string; visible?: boolean; color?: string }
  | { op: 'remove_layer'; id: string }
  | { op: 'add_material'; name: string; color: string; opacity?: number; texture?: Partial<Texture> }
  | { op: 'update_material'; id: string; name?: string; color?: string; opacity?: number; /** null takes the texture away; a width alone keeps the proportions */ texture?: Partial<Texture> | null }
  | { op: 'add_guide'; point: Vec3; direction?: Vec3 }
  | { op: 'add_dimension'; a: Vec3; b: Vec3; offset: Vec3; style?: 'radius' | 'diameter' | 'angle' }
  | { op: 'add_text'; position?: Vec3; text: string; anchor?: Vec3; screen?: [number, number] }
  | { op: 'add_section'; origin: Vec3; normal: Vec3; active?: boolean }
  | { op: 'update_section'; id: string; origin?: Vec3; normal?: Vec3; active?: boolean }
  | { op: 'slice_section'; id: string; layer?: string; name?: string }
  | { op: 'update_entity'; id: string; patch: Record<string, unknown> }
  | { op: 'set_hidden'; ids: string[]; hidden: boolean }
  | { op: 'set_locked'; ids: string[]; locked: boolean }
  | { op: 'soften'; ids?: string[]; edges?: [Vec3, Vec3][]; angle?: number; group?: string }
  | { op: 'text3d'; text: string; position?: Vec3; height?: number; depth?: number; font?: 'sans' | 'serif'; bold?: boolean; normal?: Vec3; u?: Vec3; align?: 'left' | 'center' | 'right'; layer?: string; material?: string; group?: string }
  | { op: 'insert'; entities: Entity[]; delta?: Vec3; group?: string; merge?: boolean; local?: boolean }
  | { op: 'boolean'; mode: 'union' | 'subtract' | 'intersect' | 'split'; target: string; tools: string[]; keepTools?: boolean; material?: string }
  | { op: 'measure'; a: Vec3; b: Vec3 }
  | { op: 'set_extension'; plugin: string; data: unknown }
  | { op: 'custom'; type: string; data: unknown }
  | { op: 'draw_arc'; center?: Vec3; start?: Vec3; end?: Vec3; through?: Vec3[]; from?: Vec3; to?: Vec3; bulge?: number; normal?: Vec3; segments?: number; pie?: boolean; closeChord?: boolean; layer?: string; group?: string }
  | { op: 'follow_me'; face: string; path?: Vec3[]; pathEdges?: string[] }
  | { op: 'mirror'; ids: string[]; origin: Vec3; normal: Vec3; copy?: boolean }
  | { op: 'make_component'; ids: string[]; name: string; description?: string }
  | { op: 'place_component'; definition: string; position: Vec3; rotation?: number; scale?: number | Vec3; layer?: string }
  | { op: 'make_unique'; instance: string }
  | { op: 'edit_component'; definition: string; ops: Op[] }
  | { op: 'rename_component'; definition: string; name: string }
  | { op: 'remove_component'; definition: string; force?: boolean };

export interface OpResult { created: string[]; modified: string[]; removed: string[]; measurement?: number; entities?: Entity[] }

/** Walk every entity, including those inside groups, with the group path. */
export function* walk(entities: Entity[], path: string[] = []): Generator<{ entity: Entity; path: string[] }> {
  for (const e of entities) {
    yield { entity: e, path };
    if (e.kind === 'group') yield* walk(e.entities, [...path, e.id]);
  }
}
export function findEntity(doc: Document, id: string): Entity | undefined {
  for (const { entity } of walk(doc.entities)) if (entity.id === id) return entity;
  return undefined;
}
/** Top-level ancestor id of an entity (what Select picks when not inside a group). */
export function topLevelId(doc: Document, id: string): string {
  for (const { entity, path } of walk(doc.entities)) if (entity.id === id) return path[0] ?? id;
  return id;
}
export function materialOf(doc: Document, id?: string | null): Material | undefined {
  return doc.materials.find(m => m.id === id);
}
export function layerOf(doc: Document, id: string): Layer | undefined { return doc.layers.find(l => l.id === id); }

export const KIND_LABEL: Record<EntityKind, string> = {
  face: 'Face', edge: 'Edge', group: 'Group', guide: 'Guide', dimension: 'Dimension', text: 'Text', section: 'Section plane', custom: 'Plugin entity', instance: 'Component',
};

export function fmt(n: number, digits = 2): string {
  return n.toLocaleString('en-GB', { minimumFractionDigits: 0, maximumFractionDigits: digits });
}
/** How lengths are shown and typed: the document's unit and decimals. (The model itself is always in metres.) */
export const lengthUnit = { name: 'm', factor: 1, digits: 3 };
export const UNITS: Record<string, { factor: number; digits: number }> = { m: { factor: 1, digits: 3 }, cm: { factor: 100, digits: 1 }, mm: { factor: 1000, digits: 0 } };
export function setLengthUnit(name: string, digits?: number | null) {
  const u = UNITS[name] ? name : 'm'; lengthUnit.name = u; lengthUnit.factor = UNITS[u].factor;
  lengthUnit.digits = typeof digits === 'number' && digits >= 0 && digits <= 6 ? Math.round(digits) : UNITS[u].digits;
}
/** A length (metres) in the document's unit. */
export const fmtM = (n: number) => `${fmt(n * lengthUnit.factor, lengthUnit.digits)} ${lengthUnit.name}`;
export const fmtM2 = (n: number) => lengthUnit.name === 'm' ? `${fmt(n)} m²` : `${fmt(n * lengthUnit.factor ** 2, lengthUnit.digits)} ${lengthUnit.name}²`;
export const fmtDeg = (n: number) => `${fmt(n, 1)}°`;

export const v = {
  add: (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]],
  sub: (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]],
  scale: (a: Vec3, s: number): Vec3 => [a[0] * s, a[1] * s, a[2] * s],
  dot: (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2],
  cross: (a: Vec3, b: Vec3): Vec3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]],
  len: (a: Vec3) => Math.hypot(a[0], a[1], a[2]),
  norm: (a: Vec3): Vec3 => { const l = Math.hypot(a[0], a[1], a[2]) || 1; return [a[0] / l, a[1] / l, a[2] / l]; },
  dist: (a: Vec3, b: Vec3) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]),
  round: (a: Vec3, d = 4): Vec3 => a.map(x => +x.toFixed(d)) as Vec3,
};

/** Newell normal of a polygon. */
export function polygonNormal(pts: Vec3[]): Vec3 {
  let n: Vec3 = [0, 0, 0];
  for (let i = 0; i < pts.length; i++) {
    const a = pts[i], b = pts[(i + 1) % pts.length];
    n[0] += (a[1] - b[1]) * (a[2] + b[2]); n[1] += (a[2] - b[2]) * (a[0] + b[0]); n[2] += (a[0] - b[0]) * (a[1] + b[1]);
  }
  return v.norm(n);
}
export function polygonCenter(pts: Vec3[]): Vec3 {
  const c: Vec3 = [0, 0, 0]; for (const p of pts) { c[0] += p[0]; c[1] += p[1]; c[2] += p[2]; }
  return v.scale(c, 1 / Math.max(1, pts.length));
}
