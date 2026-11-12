// Texture images for materials: a few ready-made patterns drawn here, and the user's own image files.
// A texture travels in the document as a data: URL (PNG or JPEG) with the size in metres one copy covers.

export interface TextureInput { image: string; width: number; height?: number; color: string }

type Rnd = () => number;
const seeded = (seed: number): Rnd => () => { seed = (seed + 0x6d2b79f5) | 0; let t = Math.imul(seed ^ (seed >>> 15), 1 | seed); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
/** `hex` made lighter or darker by `d` (-1…1). */
const shade = (hex: string, d: number) => '#' + [1, 3, 5].map(i => Math.max(0, Math.min(255, Math.round(parseInt(hex.slice(i, i + 2), 16) + d * 255))).toString(16).padStart(2, '0')).join('');
/** Fine grain over the whole image; every pixel on its own, so the image still repeats without a seam. */
function grain(ctx: CanvasRenderingContext2D, rnd: Rnd, amount: number) {
  const { width: w, height: h } = ctx.canvas; const img = ctx.getImageData(0, 0, w, h); const d = img.data;
  for (let i = 0; i < d.length; i += 4) { const n = (rnd() - 0.5) * amount; d[i] += n; d[i + 1] += n; d[i + 2] += n; }
  ctx.putImageData(img, 0, 0);
}
/** Soft light and dark patches, drawn again across each border so they carry over to the next copy. */
function patches(ctx: CanvasRenderingContext2D, rnd: Rnd, count: number, size: number, strength: number) {
  const { width: w, height: h } = ctx.canvas;
  for (let i = 0; i < count; i++) {
    const x = rnd() * w, y = rnd() * h, r = size * (0.4 + rnd()), dark = rnd() < 0.5, a = strength * (0.4 + rnd() * 0.6);
    for (const dx of [-w, 0, w]) for (const dy of [-h, 0, h]) {
      const g = ctx.createRadialGradient(x + dx, y + dy, 0, x + dx, y + dy, r);
      g.addColorStop(0, dark ? `rgba(0,0,0,${a})` : `rgba(255,255,255,${a})`); g.addColorStop(1, 'rgba(0,0,0,0)');
      ctx.fillStyle = g; ctx.fillRect(x + dx - r, y + dy - r, r * 2, r * 2);
    }
  }
}

interface Preset { id: string; label: string; /** Metres one copy covers. */ width: number; height: number; pixels: number; draw: (ctx: CanvasRenderingContext2D, s: number, rnd: Rnd) => void }
export const TEXTURE_PRESETS: Preset[] = [
  { // four bricks across (21 cm and a 1 cm joint), eight courses up (6.5 cm and a joint), every other course shifted half a brick
    id: 'brick', label: 'Brick', width: 0.88, height: 0.6, pixels: 512, draw(ctx, s, rnd) {
      ctx.fillStyle = '#c8c0b2'; ctx.fillRect(0, 0, s, s);
      const bw = s / 4, bh = s / 8, jx = bw * (0.01 / 0.22), jy = bh * (0.01 / 0.075);
      for (let r = 0; r < 8; r++) {
        const tones = [0, 1, 2, 3].map(() => shade('#a9573a', (rnd() - 0.5) * 0.16));
        for (let k = -1; k <= 4; k++) { ctx.fillStyle = tones[(k + 4) % 4]; ctx.fillRect((r % 2) * bw / 2 + k * bw + jx / 2, r * bh + jy / 2, bw - jx, bh - jy); }
      }
      grain(ctx, rnd, 26);
    },
  },
  { // four tiles across (30 cm), four rows up (25 cm showing), each row shifted half a tile and shaded under the row above
    id: 'roof', label: 'Roof tiles', width: 1.2, height: 1, pixels: 512, draw(ctx, s, rnd) {
      const tw = s / 4, th = s / 4;
      for (let r = 0; r < 4; r++) {
        const tones = [0, 1, 2, 3].map(() => shade('#8a4a3a', (rnd() - 0.5) * 0.1));
        for (let k = -1; k <= 4; k++) {
          const x = (r % 2) * tw / 2 + k * tw; ctx.fillStyle = tones[(k + 4) % 4]; ctx.fillRect(x, r * th, tw, th);
          ctx.fillStyle = 'rgba(0,0,0,.28)'; ctx.fillRect(x, r * th, 2, th);
        }
        const g = ctx.createLinearGradient(0, r * th, 0, (r + 1) * th); g.addColorStop(0, 'rgba(0,0,0,.5)'); g.addColorStop(0.18, 'rgba(0,0,0,.08)'); g.addColorStop(0.9, 'rgba(255,255,255,.06)'); g.addColorStop(1, 'rgba(255,255,255,.14)');
        ctx.fillStyle = g; ctx.fillRect(0, r * th, s, th);
      }
      grain(ctx, rnd, 18);
    },
  },
  { // eight boards up (12 cm), two lengths to a row, with the grain running along them
    id: 'wood', label: 'Wood boards', width: 1.8, height: 0.96, pixels: 512, draw(ctx, s, rnd) {
      const bh = s / 8;
      for (let r = 0; r < 8; r++) {
        const a = rnd() * s, b = (a + s * (0.35 + rnd() * 0.3)) % s, tones = [shade('#b8895a', (rnd() - 0.5) * 0.12), shade('#b8895a', (rnd() - 0.5) * 0.12)];
        const [x0, x1] = a < b ? [a, b] : [b, a];
        ctx.fillStyle = tones[0]; ctx.fillRect(0, r * bh, s, bh); ctx.fillStyle = tones[1]; ctx.fillRect(x0, r * bh, x1 - x0, bh);
        ctx.save(); ctx.beginPath(); ctx.rect(0, r * bh, s, bh); ctx.clip();
        for (let i = 0; i < 7; i++) { // grain: wavy lines that close on themselves across the width
          const y = r * bh + rnd() * bh, amp = 1 + rnd() * 3, waves = 1 + Math.floor(rnd() * 3), ph = rnd() * 6.28;
          ctx.strokeStyle = `rgba(70,40,15,${0.08 + rnd() * 0.14})`; ctx.lineWidth = 1; ctx.beginPath();
          for (let x = 0; x <= s; x += 8) ctx.lineTo(x, y + amp * Math.sin(ph + waves * 6.2832 * x / s));
          ctx.stroke();
        }
        ctx.restore();
        ctx.fillStyle = 'rgba(40,22,8,.55)'; ctx.fillRect(0, r * bh, s, 1.5); ctx.fillRect(x0, r * bh, 1.5, bh); ctx.fillRect(x1, r * bh, 1.5, bh);
      }
      grain(ctx, rnd, 12);
    },
  },
  { // two by two tiles of 30 cm with a 4 mm joint
    id: 'tiles', label: 'Floor tiles', width: 0.6, height: 0.6, pixels: 512, draw(ctx, s, rnd) {
      ctx.fillStyle = '#9a968e'; ctx.fillRect(0, 0, s, s); const t = s / 2, j = s * (0.004 / 0.6);
      for (let r = 0; r < 2; r++) for (let k = 0; k < 2; k++) { ctx.fillStyle = shade('#d9d4c8', (rnd() - 0.5) * 0.05); ctx.fillRect(k * t + j / 2, r * t + j / 2, t - j, t - j); }
      grain(ctx, rnd, 10);
    },
  },
  { id: 'concrete', label: 'Concrete', width: 2, height: 2, pixels: 256, draw(ctx, s, rnd) { ctx.fillStyle = '#a9a8a3'; ctx.fillRect(0, 0, s, s); patches(ctx, rnd, 40, s / 5, 0.07); grain(ctx, rnd, 22); } },
  { id: 'asphalt', label: 'Asphalt', width: 1, height: 1, pixels: 256, draw(ctx, s, rnd) { ctx.fillStyle = '#4a4b4d'; ctx.fillRect(0, 0, s, s); patches(ctx, rnd, 30, s / 6, 0.06); grain(ctx, rnd, 60); } },
  {
    id: 'grass', label: 'Grass', width: 2, height: 2, pixels: 256, draw(ctx, s, rnd) {
      ctx.fillStyle = '#5f8a3c'; ctx.fillRect(0, 0, s, s); patches(ctx, rnd, 60, s / 6, 0.1);
      for (let i = 0; i < 2600; i++) { // blades: short strokes, kept clear of the border
        const x = 3 + rnd() * (s - 6), y = 4 + rnd() * (s - 8); ctx.strokeStyle = shade('#5f8a3c', (rnd() - 0.5) * 0.28); ctx.lineWidth = 1;
        ctx.beginPath(); ctx.moveTo(x, y); ctx.lineTo(x + (rnd() - 0.5) * 3, y - 2 - rnd() * 3); ctx.stroke();
      }
      grain(ctx, rnd, 24);
    },
  },
];

/** The colour a texture reads as from a distance: what shows where textures are not drawn. */
function average(src: CanvasImageSource): string {
  const c = document.createElement('canvas'); c.width = c.height = 8; const ctx = c.getContext('2d')!; ctx.drawImage(src, 0, 0, 8, 8);
  const d = ctx.getImageData(0, 0, 8, 8).data; const sum = [0, 0, 0];
  for (let i = 0; i < d.length; i += 4) { sum[0] += d[i]; sum[1] += d[i + 1]; sum[2] += d[i + 2]; }
  return '#' + sum.map(v => Math.round(v / 64).toString(16).padStart(2, '0')).join('');
}

/** One of the ready-made patterns as a texture (the same picture every time). */
export function presetTexture(id: string): TextureInput | null {
  const p = TEXTURE_PRESETS.find(p => p.id === id); if (!p) return null;
  const c = document.createElement('canvas'); c.width = c.height = p.pixels; const ctx = c.getContext('2d')!;
  p.draw(ctx, p.pixels, seeded(p.id.length * 7919 + p.id.charCodeAt(0)));
  return { image: c.toDataURL('image/jpeg', 0.85), width: p.width, height: p.height, color: average(c) };
}

/** The longest image the engine takes, with a little room to spare. */
const MAX_CHARS = 1_300_000;
/** An image file as a texture: scaled down to 1024 pixels at most, one metre across until the user says otherwise. */
export async function fileTexture(file: File): Promise<TextureInput> {
  const bmp = await createImageBitmap(file).catch(() => null);
  if (!bmp || !bmp.width || !bmp.height) throw new Error('this file cannot be read as an image');
  const png = file.type === 'image/png';
  for (const longest of [1024, 512, 256]) {
    const k = Math.min(1, longest / Math.max(bmp.width, bmp.height));
    const c = document.createElement('canvas'); c.width = Math.max(1, Math.round(bmp.width * k)); c.height = Math.max(1, Math.round(bmp.height * k));
    const ctx = c.getContext('2d')!; if (!png) { ctx.fillStyle = '#fff'; ctx.fillRect(0, 0, c.width, c.height); } ctx.drawImage(bmp, 0, 0, c.width, c.height);
    // a PNG stays a PNG while it is small (it may be see-through), anything else becomes a JPEG
    let image = png ? c.toDataURL('image/png') : ''; if (!image || image.length > 400_000) image = c.toDataURL('image/jpeg', 0.85);
    if (image.length <= MAX_CHARS) return { image, width: 1, color: average(c) };
  }
  throw new Error('this image is too large, even scaled down');
}

/** Let the user choose an image file. */
export function pickImage(): Promise<File | null> {
  return new Promise(resolve => {
    const inp = document.createElement('input'); inp.type = 'file'; inp.accept = 'image/png,image/jpeg,image/webp,image/gif,.png,.jpg,.jpeg,.webp';
    inp.onchange = () => resolve(inp.files?.[0] ?? null); inp.oncancel = () => resolve(null); inp.click();
  });
}

const urls = new Map<string, string>();
/** A short address for a texture image (the data: URL itself is up to a megabyte of text). */
export function textureUrl(image: string): string {
  let u = urls.get(image);
  if (!u) {
    try {
      const [head, body] = image.split(','); const bin = atob(body); const bytes = new Uint8Array(bin.length); for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
      u = URL.createObjectURL(new Blob([bytes], { type: head.slice(5).split(';')[0] }));
    } catch { u = image; }
    urls.set(image, u);
  }
  return u;
}
/** Let go of the addresses of images that are no longer in the document. */
export function keepTextureUrls(images: Iterable<string>) {
  const keep = new Set(images);
  for (const [image, u] of urls) if (!keep.has(image)) { if (u.startsWith('blob:')) URL.revokeObjectURL(u); urls.delete(image); }
}
/** The CSS background of a material's swatch. */
export function swatch(m: { color: string; texture?: { image: string } | null } | undefined): string {
  return m?.texture ? `${m.color} url(${textureUrl(m.texture.image)}) center/cover` : m?.color ?? '#ccc';
}
