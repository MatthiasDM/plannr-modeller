// DXF (the text form of AutoCAD's drawing exchange format): read a drawing's lines, curves, faces, blocks and layers
// into one group, and write what is in view as faces and edges. DXF is z up; Plannr is y up.
import type { CompiledScene } from './api';
import type { Document, Entity, Vec3 } from './model';
import { flat } from './io';

type Pair = [number, string];
interface Rec { type: string; pairs: Pair[]; verts?: Rec[] }
interface Block { base: Vec3; recs: Rec[] }
export interface DxfLayer { name: string; color: string; visible: boolean }
export interface Drawing { unit?: string; layers: Map<string, DxfLayer>; blocks: Map<string, Block>; recs: Rec[] }

const num = (r: Rec, code: number, def = 0) => { for (const [c, v] of r.pairs) if (c === code) { const n = parseFloat(v); return Number.isFinite(n) ? n : def; } return def; };
const int = (r: Rec, code: number, def = 0) => Math.round(num(r, code, def));
const str = (r: Rec, code: number) => { for (const [c, v] of r.pairs) if (c === code) return v; return ''; };
const pt = (r: Rec, i = 0): Vec3 => [num(r, 10 + i), num(r, 20 + i), num(r, 30 + i)];
const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const cross = (a: Vec3, b: Vec3): Vec3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const len = (a: Vec3) => Math.hypot(a[0], a[1], a[2]);
const unit = (a: Vec3): Vec3 => { const l = len(a) || 1; return [a[0] / l, a[1] / l, a[2] / l]; };

/** AutoCAD's colour index as #rrggbb: the nine named colours, then 24 hues in five shades, full and half saturated. */
function aci(i: number): string {
  const named = ['#000000', '#ff0000', '#ffff00', '#00ff00', '#00ffff', '#0000ff', '#ff00ff', '#7a8aa0', '#808080', '#c0c0c0'];
  if (i < 10) return named[Math.max(0, i)];
  if (i >= 250) { const g = Math.round(51 + (i - 250) * 40.8); return rgb(g, g, g); }
  const h = Math.floor((i - 10) / 10) * 15, d = i % 10, v = [1, 0.8, 0.6, 0.5, 0.3][d >> 1], s = d & 1 ? 0.5 : 1;
  const f = (n: number) => { const k = (n + h / 60) % 6; return Math.round(255 * v * (1 - s * Math.max(0, Math.min(k, 4 - k, 1)))); };
  return rgb(f(5), f(3), f(1));
}
const rgb = (r: number, g: number, b: number) => '#' + [r, g, b].map(x => x.toString(16).padStart(2, '0')).join('');
/** `$INSUNITS` and the units an import can ask for (in metres). */
const INSUNITS: Record<number, string> = { 1: 'in', 2: 'ft', 4: 'mm', 5: 'cm', 6: 'm' };
export const DXF_UNITS: Record<string, number> = { mm: 0.001, cm: 0.01, m: 1, in: 0.0254, ft: 0.3048 };

/** Split a DXF text into its unit, layers, blocks and entities. */
export function readDXF(text: string): Drawing {
  if (text.startsWith('AutoCAD Binary DXF')) throw new Error('this is a binary DXF; save it as a text (ASCII) DXF');
  const lines = text.split(/\r\n|\r|\n/); const pairs: Pair[] = [];
  for (let i = 0; i + 1 < lines.length; i += 2) {
    const c = parseInt(lines[i], 10); if (Number.isNaN(c)) { if (!lines[i].trim()) continue; throw new Error(`line ${i + 1} is not a DXF group code`); }
    if (c !== 999) pairs.push([c, lines[i + 1].trim()]);
  }
  const d: Drawing = { layers: new Map(), blocks: new Map(), recs: [] };
  let section = '', block: Block | null = null, poly: Rec | null = null, found = false;
  for (let i = 0; i < pairs.length; i++) {
    const [c, v] = pairs[i];
    if (c !== 0) { if (section === 'HEADER' && c === 9 && v === '$INSUNITS' && pairs[i + 1]?.[0] === 70) d.unit = INSUNITS[parseInt(pairs[i + 1][1], 10)]; continue; }
    if (v === 'SECTION') { section = pairs[i + 1]?.[0] === 2 ? pairs[i + 1][1] : ''; found = true; continue; }
    if (v === 'ENDSEC') { section = ''; continue; }
    if (v === 'EOF') break;
    const r: Rec = { type: v, pairs: [] };
    for (let k = i + 1; k < pairs.length && pairs[k][0] !== 0; k++) r.pairs.push(pairs[k]);
    if (section === 'TABLES') {
      if (v !== 'LAYER') continue;
      const col = int(r, 62, 7), tc = str(r, 420), name = str(r, 2); if (!name) continue;
      d.layers.set(name, { name, color: tc ? '#' + (parseInt(tc, 10) & 0xffffff).toString(16).padStart(6, '0') : aci(Math.abs(col)), visible: col >= 0 && !(int(r, 70) & 1) });
    } else if (section === 'BLOCKS' || section === 'ENTITIES') {
      if (v === 'BLOCK') { block = { base: pt(r), recs: [] }; d.blocks.set(str(r, 2), block); continue; }
      if (v === 'ENDBLK') { block = null; continue; }
      if (v === 'SEQEND') { poly = null; continue; }
      if (v === 'VERTEX') { poly?.verts!.push(r); continue; }
      if (v === 'ATTRIB') continue;
      poly = v === 'POLYLINE' ? r : null; if (poly) poly.verts = [];
      (section === 'BLOCKS' ? block?.recs : d.recs)?.push(r);
    }
  }
  if (!found) throw new Error('no DXF sections found');
  return d;
}

interface Edge { a: Vec3; b: Vec3; layer: string; arc?: { center: Vec3; radius: number; normal: Vec3 } }
interface Node { name: string; layer: string; edges: Edge[]; faces: { outer: Vec3[]; layer: string }[]; kids: Node[]; seen: Set<string> }
type Xf = (p: Vec3) => Vec3;
export interface DxfResult { group: Entity; layers: DxfLayer[]; edges: number; faces: number; skipped: Record<string, number> }

/** An entity's own coordinate system from its extrusion direction (the "arbitrary axis algorithm"). */
function ocs(r: Rec): Xf {
  const n = unit([num(r, 210), num(r, 220), num(r, 230, 1)]);
  if (Math.abs(n[0]) < 1e-12 && Math.abs(n[1]) < 1e-12 && n[2] > 0) return p => p;
  const ax = unit(Math.abs(n[0]) < 1 / 64 && Math.abs(n[1]) < 1 / 64 ? cross([0, 1, 0], n) : cross([0, 0, 1], n)), ay = cross(n, ax);
  return p => [p[0] * ax[0] + p[1] * ay[0] + p[2] * n[0], p[0] * ax[1] + p[1] * ay[1] + p[2] * n[1], p[0] * ax[2] + p[1] * ay[2] + p[2] * n[2]];
}
/** A point of a B-spline (de Boor), on control points in homogeneous coordinates. */
function deBoor(deg: number, knots: number[], ctrl: number[][], u: number): Vec3 {
  let k = deg; while (k < ctrl.length - 1 && knots[k + 1] <= u) k++;
  const d = Array.from({ length: deg + 1 }, (_, j) => ctrl[j + k - deg].slice());
  for (let r = 1; r <= deg; r++) for (let j = deg; j >= r; j--) {
    const i = j + k - deg, den = knots[i + deg - r + 1] - knots[i], a = den > 0 ? (u - knots[i]) / den : 0;
    d[j] = d[j].map((x, m) => d[j - 1][m] * (1 - a) + x * a);
  }
  const w = d[deg][3] || 1; return [d[deg][0] / w, d[deg][1] / w, d[deg][2] / w];
}

/** The drawing as one group named `name`, scaled to metres: lines and curves as edges, 3D faces and solids as faces,
 *  each block reference as a group of its own; layers keep their names. Text, hatches and dimensions are left out. */
export function dxfGroup(d: Drawing, scale: number, name: string): DxfResult {
  const r6 = (x: number) => { const v = +(x * scale).toFixed(6); return v === 0 ? 0 : v; };
  const out = (p: Vec3): Vec3 => [r6(p[0]), r6(p[2]), r6(-p[1])];
  const key = (p: Vec3) => `${p[0]},${p[1]},${p[2]}`;
  const skipped: Record<string, number> = {}; const used = new Set<string>(); let edges = 0, faces = 0;
  const node = (name: string, layer: string): Node => ({ name, layer, edges: [], faces: [], kids: [], seen: new Set() });

  const edge = (n: Node, a: Vec3, b: Vec3, layer: string, arc?: Edge['arc']) => {
    const [ka, kb] = [key(a), key(b)]; if (ka === kb) return;
    const k = ka < kb ? ka + '|' + kb : kb + '|' + ka; if (n.seen.has(k)) return; n.seen.add(k);
    n.edges.push(arc ? { a, b, layer, arc } : { a, b, layer }); used.add(layer); edges++;
  };
  /** A run of points (in the drawing's coordinates) as edges; `circ` when they lie on a circle. */
  const curve = (n: Node, xf: Xf, pts: Vec3[], closed: boolean, layer: string, circ?: { c: Vec3; n: Vec3 }) => {
    const q = pts.map(p => out(xf(p))); let arc: Edge['arc'];
    if (circ && q.length > 1) {
      const c = out(xf(circ.c)), far = out(xf([circ.c[0] + circ.n[0], circ.c[1] + circ.n[1], circ.c[2] + circ.n[2]])), r = len(sub(q[0], c));
      // still a circle after the block's scaling?
      if (r > 1e-6 && q.every(p => Math.abs(len(sub(p, c)) - r) < 1e-5 + r * 1e-6)) arc = { center: c, radius: +r.toFixed(9), normal: unit(sub(far, c)) };
    }
    for (let i = 0; i + 1 < q.length; i++) edge(n, q[i], q[i + 1], layer, arc);
    if (closed && q.length > 2) edge(n, q[q.length - 1], q[0], layer, arc);
  };
  const face = (n: Node, xf: Xf, pts: Vec3[], layer: string) => {
    const q = pts.map(p => out(xf(p))).filter((p, i, a) => key(p) !== key(a[(i + 1) % a.length]));
    for (const outer of flat(q)) { n.faces.push({ outer, layer }); used.add(layer); faces++; }
  };
  /** A circular arc from angle `a0` over `sweep` (radians) in the plane `o`, about every 15°. */
  const arcPts = (c: Vec3, r: number, a0: number, sweep: number, o: Xf): Vec3[] => {
    const full = Math.abs(Math.abs(sweep) - 2 * Math.PI) < 1e-9, n = Math.max(full ? 24 : 2, Math.ceil(Math.abs(sweep) / (Math.PI / 12) - 1e-9));
    return Array.from({ length: full ? n : n + 1 }, (_, i) => { const t = a0 + sweep * i / n; return o([c[0] + r * Math.cos(t), c[1] + r * Math.sin(t), c[2]]); });
  };
  /** A 2D polyline whose vertices may bulge into arcs. */
  const poly2d = (n: Node, xf: Xf, v: { x: number; y: number; b: number }[], closed: boolean, z: number, o: Xf, layer: string) => {
    for (let i = 0; i < v.length - (closed ? 0 : 1); i++) {
      const p = v[i], q = v[(i + 1) % v.length], dx = q.x - p.x, dy = q.y - p.y, d = Math.hypot(dx, dy); if (d < 1e-12) continue;
      if (Math.abs(p.b) < 1e-9) { curve(n, xf, [o([p.x, p.y, z]), o([q.x, q.y, z])], false, layer); continue; }
      const k = (1 - p.b * p.b) / (4 * p.b), c: Vec3 = [(p.x + q.x) / 2 - dy * k, (p.y + q.y) / 2 + dx * k, z];
      const pts = arcPts(c, Math.hypot(p.x - c[0], p.y - c[1]), Math.atan2(p.y - c[1], p.x - c[0]), 4 * Math.atan(p.b), o);
      pts[0] = o([p.x, p.y, z]); pts[pts.length - 1] = o([q.x, q.y, z]); // the ends exactly, so neighbours meet
      curve(n, xf, pts, false, layer, { c: o(c), n: o([0, 0, 1]) });
    }
  };

  const draw = (recs: Rec[], xf: Xf, n: Node, insLayer: string | null, depth: number) => {
    for (const r of recs) {
      if (int(r, 67) === 1 || int(r, 60) === 1) continue; // paper space, invisible
      let layer = str(r, 8) || '0'; if (layer === '0' && insLayer !== null) layer = insLayer;
      const o = ocs(r);
      switch (r.type) {
        case 'LINE': curve(n, xf, [pt(r), pt(r, 1)], false, layer); break;
        case 'CIRCLE': case 'ARC': {
          const c = pt(r), rad = num(r, 40); if (!(rad > 0)) break;
          const a0 = r.type === 'ARC' ? num(r, 50) : 0, sweep = r.type === 'ARC' ? (((num(r, 51) - a0) % 360) + 360) % 360 || 360 : 360;
          curve(n, xf, arcPts(c, rad, a0 * Math.PI / 180, sweep * Math.PI / 180, o), sweep === 360, layer, { c: o(c), n: o([0, 0, 1]) });
          break;
        }
        case 'ELLIPSE': {
          const c = pt(r), major = pt(r, 1), ratio = num(r, 40, 1), nrm = unit([num(r, 210), num(r, 220), num(r, 230, 1)]);
          const minor = cross(nrm, major).map(x => x * ratio) as Vec3; let sweep = num(r, 42, 2 * Math.PI) - num(r, 41); if (sweep <= 1e-9) sweep += 2 * Math.PI;
          const full = Math.abs(sweep - 2 * Math.PI) < 1e-9, k = Math.max(full ? 24 : 2, Math.ceil(sweep / (Math.PI / 12) - 1e-9)), t0 = num(r, 41);
          const pts = Array.from({ length: full ? k : k + 1 }, (_, i): Vec3 => { const t = t0 + sweep * i / k, cs = Math.cos(t), sn = Math.sin(t); return [c[0] + cs * major[0] + sn * minor[0], c[1] + cs * major[1] + sn * minor[1], c[2] + cs * major[2] + sn * minor[2]]; });
          curve(n, xf, pts, full, layer, Math.abs(ratio - 1) < 1e-9 ? { c, n: nrm } : undefined);
          break;
        }
        case 'LWPOLYLINE': {
          const v: { x: number; y: number; b: number }[] = [];
          for (const [c, s] of r.pairs) { if (c === 10) v.push({ x: +s, y: 0, b: 0 }); else if (c === 20 && v.length) v[v.length - 1].y = +s; else if (c === 42 && v.length) v[v.length - 1].b = +s; }
          poly2d(n, xf, v, (int(r, 70) & 1) === 1, num(r, 38), o, layer);
          break;
        }
        case 'POLYLINE': {
          const fl = int(r, 70), vs = r.verts ?? [];
          if (fl & 64) { // polyface mesh: corner vertices, then faces that index them
            const corners = vs.filter(v => int(v, 70) & 64).map(v => pt(v));
            for (const v of vs) if (!(int(v, 70) & 64)) face(n, xf, [71, 72, 73, 74].map(c => corners[Math.abs(int(v, c)) - 1]).filter(Boolean), layer);
          } else if (fl & 16) { // polygon mesh: a grid of M by N vertices
            const M = int(r, 71), N = int(r, 72), g = vs.map(v => pt(v)); if (M < 2 || N < 2 || g.length < M * N) break;
            for (let i = 0; i < M - (fl & 1 ? 0 : 1); i++) for (let j = 0; j < N - (fl & 32 ? 0 : 1); j++) {
              const at = (a: number, b: number) => g[(a % M) * N + (b % N)]; face(n, xf, [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)], layer);
            }
          } else if (fl & 8) curve(n, xf, vs.map(v => pt(v)), (fl & 1) === 1, layer);
          else poly2d(n, xf, vs.filter(v => !(int(v, 70) & 16)).map(v => ({ x: num(v, 10), y: num(v, 20), b: num(v, 42) })), (fl & 1) === 1, num(r, 30), o, layer);
          break;
        }
        case '3DFACE': face(n, xf, [pt(r), pt(r, 1), pt(r, 2), pt(r, 3)], layer); break;
        case 'SOLID': case 'TRACE': face(n, xf, [pt(r), pt(r, 1), pt(r, 3), pt(r, 2)].map(o), layer); break;
        case 'SPLINE': {
          const deg = int(r, 71, 3), knots: number[] = [], ctrl: number[][] = [], fit: Vec3[] = [], w: number[] = [];
          for (const [c, s] of r.pairs) {
            if (c === 40) knots.push(+s); else if (c === 41) w.push(+s);
            else if (c === 10) ctrl.push([+s, 0, 0, 1]); else if (c === 20 && ctrl.length) ctrl[ctrl.length - 1][1] = +s; else if (c === 30 && ctrl.length) ctrl[ctrl.length - 1][2] = +s;
            else if (c === 11) fit.push([+s, 0, 0]); else if (c === 21 && fit.length) fit[fit.length - 1][1] = +s; else if (c === 31 && fit.length) fit[fit.length - 1][2] = +s;
          }
          let pts: Vec3[];
          if (ctrl.length > deg && deg >= 1 && knots.length === ctrl.length + deg + 1) {
            if (w.length === ctrl.length) ctrl.forEach((c, i) => { c[0] *= w[i]; c[1] *= w[i]; c[2] *= w[i]; c[3] = w[i]; });
            const u0 = knots[deg], u1 = knots[ctrl.length], k = Math.min(600, (ctrl.length - deg) * 8);
            pts = Array.from({ length: k + 1 }, (_, i) => deBoor(deg, knots, ctrl, u0 + (u1 - u0) * i / k));
          } else pts = fit.length > 1 ? fit : ctrl.map(c => [c[0], c[1], c[2]] as Vec3);
          curve(n, xf, pts, false, layer);
          break;
        }
        case 'INSERT': {
          const b = d.blocks.get(str(r, 2)); if (!b || depth > 16) { skipped.INSERT = (skipped.INSERT ?? 0) + 1; break; }
          const ins = pt(r), sx = num(r, 41, 1), sy = num(r, 42, 1), sz = num(r, 43, 1), rot = num(r, 50) * Math.PI / 180, cs = Math.cos(rot), sn = Math.sin(rot);
          const bname = str(r, 2), into = bname.startsWith('*') ? n : node(bname, layer);
          for (let ci = 0; ci < Math.max(1, int(r, 70, 1)); ci++) for (let ri = 0; ri < Math.max(1, int(r, 71, 1)); ri++) {
            const ox = ci * num(r, 44), oy = ri * num(r, 45);
            draw(b.recs, p => { const x = (p[0] - b.base[0]) * sx + ox, y = (p[1] - b.base[1]) * sy + oy, z = (p[2] - b.base[2]) * sz; return xf(o([ins[0] + x * cs - y * sn, ins[1] + x * sn + y * cs, ins[2] + z])); }, into, layer, depth + 1);
          }
          if (into !== n && (into.edges.length || into.faces.length || into.kids.length)) { n.kids.push(into); used.add(layer); }
          break;
        }
        default: skipped[r.type] = (skipped[r.type] ?? 0) + 1;
      }
    }
  };
  const root = node(name, '0');
  draw(d.recs, p => p, root, null, 0);

  let id = 0; const ID = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
  const entity = (n: Node): Entity => ({
    kind: 'group', id: `g${id++}`, layer: n.layer, name: n.name, transform: ID,
    entities: [
      ...n.edges.map((e): Entity => ({ kind: 'edge', id: `e${id++}`, layer: e.layer, a: e.a, b: e.b, ...(e.arc ? { arc: e.arc } : {}) })),
      ...n.faces.map((f): Entity => ({ kind: 'face', id: `f${id++}`, layer: f.layer, outer: f.outer, holes: [] })),
      ...n.kids.map(entity),
    ],
  });
  const layers = [...used].filter(l => l !== '0').sort().map(l => d.layers.get(l) ?? { name: l, color: '#7a8aa0', visible: true });
  return { group: entity(root), layers, edges, faces, skipped };
}

/** What is in view as a DXF (R12, z up) in the document's unit: every triangle a 3DFACE with its edges switched
 *  off, every edge a LINE, each on a layer named after its tag. */
export function toDXF(c: CompiledScene, doc: Document, unitName: string, factor: number): string {
  // the tag an entity is shown with: its own, or else the nearest group's
  const first = doc.layers[0]?.id, tag = new Map<string, string>();
  const walk = (list: Entity[], inherited: string, prefix: string) => {
    for (const e of list) {
      const l = e.layer && e.layer !== first ? e.layer : inherited; tag.set(prefix + e.id, l);
      if (e.kind === 'group') walk(e.entities, l, prefix);
      else if (e.kind === 'instance') { const def = (doc.components ?? []).find(d => d.id === e.definition); if (def) walk(def.entities, l, `${e.id}#`); } // the ids compile gives what an instance shows
    }
  };
  walk(doc.entities, '', '');
  const names = new Map<string, string>([['', '0']]); const taken = new Set(['0']);
  const layerName = (id: string): string => {
    const l = tag.get(id) ?? ''; let n = names.get(l);
    if (!n) {
      const base = (doc.layers.find(x => x.id === l)?.name ?? 'LAYER').normalize('NFD').replace(/[̀-ͯ]/g, '').replace(/[^A-Za-z0-9_$-]+/g, '_').slice(0, 28) || 'LAYER';
      n = base; for (let i = 2; taken.has(n.toUpperCase()); i++) n = `${base}_${i}`;
      taken.add(n.toUpperCase()); names.set(l, n);
    }
    return n;
  };
  const f = (x: number) => { const v = +(x * factor).toFixed(6); return String(v === 0 ? 0 : v); };
  const P = (a: ArrayLike<number>, i: number, k: number) => [`${10 + k}`, f(a[i]), `${20 + k}`, f(-a[i + 2]), `${30 + k}`, f(a[i + 1])];
  const ents: string[] = [];
  for (const g of c.groups) {
    if (g.back || !g.indices.length) continue;
    g.ids.forEach((id, r) => {
      const [st, n] = g.idRanges[r], layer = layerName(id);
      for (let k = st; k + 2 < st + n; k += 3) ents.push('0', '3DFACE', '8', layer, ...P(g.positions, g.indices[k] * 3, 0), ...P(g.positions, g.indices[k + 1] * 3, 1), ...P(g.positions, g.indices[k + 2] * 3, 2), ...P(g.positions, g.indices[k + 2] * 3, 3), '70', '15');
    });
  }
  const seen = new Set<string>(), p = c.edges.positions;
  for (let i = 0; i * 6 < p.length; i++) {
    const a = P(p, i * 6, 0), b = P(p, i * 6 + 3, 1), ka = `${a[1]},${a[3]},${a[5]}`, kb = `${b[1]},${b[3]},${b[5]}`; if (ka === kb) continue;
    const k = ka < kb ? ka + '|' + kb : kb + '|' + ka; if (seen.has(k)) continue; seen.add(k);
    ents.push('0', 'LINE', '8', layerName(c.edges.ids[i]), ...a, ...b);
  }
  const used = [...new Set(names.values())];
  const insunits = ({ mm: 4, cm: 5, m: 6 } as Record<string, number>)[unitName] ?? 6;
  return [
    '999', `Plannr export: ${unitName}, z up`,
    '0', 'SECTION', '2', 'HEADER', '9', '$ACADVER', '1', 'AC1009', '9', '$INSUNITS', '70', String(insunits), '0', 'ENDSEC',
    '0', 'SECTION', '2', 'TABLES',
    '0', 'TABLE', '2', 'LTYPE', '70', '1', '0', 'LTYPE', '2', 'CONTINUOUS', '70', '0', '3', 'Solid line', '72', '65', '73', '0', '40', '0.0', '0', 'ENDTAB',
    '0', 'TABLE', '2', 'LAYER', '70', String(used.length), ...used.flatMap(n => ['0', 'LAYER', '2', n, '70', '0', '62', '7', '6', 'CONTINUOUS']), '0', 'ENDTAB',
    '0', 'ENDSEC',
    '0', 'SECTION', '2', 'ENTITIES', ...ents, '0', 'ENDSEC', '0', 'EOF', '',
  ].join('\n');
}
