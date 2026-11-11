// Export what is in view as OBJ or STL, and read OBJ and STL files into faces.
import type { CompiledScene } from './api';
import type { Entity, Vec3 } from './model';

const solidGroups = (c: CompiledScene) => c.groups.filter(g => !g.back && g.indices.length);

/** Wavefront OBJ: one object per material, metres, y up. */
export function toOBJ(c: CompiledScene): string {
  const out: string[] = ['# Plannr export: metres, y up']; let base = 1;
  solidGroups(c).forEach((g, i) => {
    out.push(`o ${(g.name || 'material').replace(/\s+/g, '_')}_${i + 1}`);
    for (let k = 0; k < g.positions.length; k += 3) out.push(`v ${+g.positions[k].toFixed(6)} ${+g.positions[k + 1].toFixed(6)} ${+g.positions[k + 2].toFixed(6)}`);
    for (let k = 0; k < g.indices.length; k += 3) out.push(`f ${g.indices[k] + base} ${g.indices[k + 1] + base} ${g.indices[k + 2] + base}`);
    base += g.positions.length / 3;
  });
  return out.join('\n') + '\n';
}

/** Binary STL the way slicers expect it: millimetres, z up. */
export function toSTL(c: CompiledScene): ArrayBuffer {
  const groups = solidGroups(c); const n = groups.reduce((t, g) => t + g.indices.length / 3, 0);
  const buf = new ArrayBuffer(84 + n * 50); const dv = new DataView(buf); dv.setUint32(80, n, true); let o = 84;
  for (const g of groups) {
    const P = (i: number): Vec3 => [g.positions[i * 3] * 1000, -g.positions[i * 3 + 2] * 1000, g.positions[i * 3 + 1] * 1000];
    for (let k = 0; k < g.indices.length; k += 3) {
      const [a, b, d] = [P(g.indices[k]), P(g.indices[k + 1]), P(g.indices[k + 2])];
      const u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]], v = [d[0] - a[0], d[1] - a[1], d[2] - a[2]];
      const nrm = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]; const l = Math.hypot(nrm[0], nrm[1], nrm[2]) || 1;
      for (const x of [nrm[0] / l, nrm[1] / l, nrm[2] / l, ...a, ...b, ...d]) { dv.setFloat32(o, x, true); o += 4; }
      o += 2;
    }
  }
  return buf;
}

const round = (p: Vec3): Vec3 => [+p[0].toFixed(6), +p[1].toFixed(6), +p[2].toFixed(6)];
const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const cross = (a: Vec3, b: Vec3): Vec3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const len = (a: Vec3) => Math.hypot(a[0], a[1], a[2]);
/** A polygon as faces: itself when it is flat, a fan of triangles otherwise; slivers are dropped. */
export function flat(poly: Vec3[]): Vec3[][] {
  if (poly.length < 3) return [];
  const tri = (a: Vec3, b: Vec3, c: Vec3) => len(cross(sub(b, a), sub(c, a))) > 1e-10 ? [[a, b, c]] : [];
  if (poly.length === 3) return tri(poly[0], poly[1], poly[2]);
  const n = poly.reduce<Vec3>((s, a, i) => { const b = poly[(i + 1) % poly.length]; return [s[0] + (a[1] - b[1]) * (a[2] + b[2]), s[1] + (a[2] - b[2]) * (a[0] + b[0]), s[2] + (a[0] - b[0]) * (a[1] + b[1])]; }, [0, 0, 0]);
  const l = len(n); if (l < 1e-10) return [];
  const off = (p: Vec3) => Math.abs(((p[0] - poly[0][0]) * n[0] + (p[1] - poly[0][1]) * n[1] + (p[2] - poly[0][2]) * n[2]) / l);
  if (poly.every(p => off(p) < 1e-5)) return [poly];
  return poly.slice(1, -1).flatMap((_, i) => tri(poly[0], poly[i + 1], poly[i + 2]));
}

/** The faces of a Wavefront OBJ (y up), scaled to metres. */
export function parseOBJ(text: string, scale: number): Vec3[][] {
  const v: Vec3[] = []; const faces: Vec3[][] = [];
  for (const line of text.split(/\r?\n/)) {
    const t = line.trim().split(/\s+/);
    if (t[0] === 'v') v.push(round([+t[1] * scale, +t[2] * scale, +t[3] * scale]));
    else if (t[0] === 'f') { const pts = t.slice(1).map(s => { const i = parseInt(s, 10); return v[i < 0 ? v.length + i : i - 1]; }).filter(Boolean); faces.push(...flat(pts)); }
  }
  return faces;
}
/** The triangles of an STL (binary or text; z up), turned y up and scaled to metres. */
export function parseSTL(buf: ArrayBuffer, scale: number): Vec3[][] {
  const P = (x: number, y: number, z: number): Vec3 => round([x * scale, z * scale, -y * scale]); const faces: Vec3[][] = [];
  const dv = new DataView(buf); const n = buf.byteLength >= 84 ? dv.getUint32(80, true) : 0;
  if (buf.byteLength >= 84 && 84 + n * 50 === buf.byteLength) {
    for (let i = 0, o = 84; i < n; i++, o += 50) { const p = (k: number) => P(dv.getFloat32(o + 12 + k * 12, true), dv.getFloat32(o + 16 + k * 12, true), dv.getFloat32(o + 20 + k * 12, true)); faces.push(...flat([p(0), p(1), p(2)])); }
    return faces;
  }
  const pts = [...new TextDecoder().decode(buf).matchAll(/vertex\s+(\S+)\s+(\S+)\s+(\S+)/g)].map(m => P(+m[1], +m[2], +m[3]));
  for (let i = 0; i + 2 < pts.length; i += 3) faces.push(...flat([pts[i], pts[i + 1], pts[i + 2]]));
  return faces;
}
/** Faces as one group, ready for the `insert` op. */
export function meshGroup(name: string, faces: Vec3[][]): Entity {
  return { kind: 'group', id: 'import', layer: '', name, transform: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1], entities: faces.map((outer, i) => ({ kind: 'face', id: `f${i}`, layer: '', outer, holes: [] })) };
}
export function download(name: string, data: BlobPart, type: string) {
  const a = document.createElement('a'); a.href = URL.createObjectURL(new Blob([data], { type })); a.download = name; a.click(); setTimeout(() => URL.revokeObjectURL(a.href), 2000);
}
