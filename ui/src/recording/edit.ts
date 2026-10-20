// The editor's model: the kept parts of a recording and the effects placed on it. Times are on the
// original's timeline, so a cut or a speed change never moves an effect off the moment it was placed
// on. The saved file is made by the backend from exactly this (see recording/edit.rs).

export interface Segment {
  startMs: number;
  endMs: number;
  speed: number;
}

interface Timed {
  id: number;
  startMs: number;
  endMs: number;
}

export interface Zoom extends Timed {
  x: number;
  y: number;
  level: number;
}

export interface Blur extends Timed {
  x: number;
  y: number;
  w: number;
  h: number;
  pixelate: boolean;
}

export interface Arrow extends Timed {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  color: string;
  /** A fraction of the picture height. */
  width: number;
}

export interface Text extends Timed {
  x: number;
  y: number;
  text: string;
  /** A fraction of the picture height. */
  size: number;
  color: string;
}

export type Effect = Zoom | Blur | Arrow | Text;
export type EffectKind = 'zoom' | 'blur' | 'arrow' | 'text';

/** A kept part, and a timed effect, must be at least this long. */
export const MIN_PART_MS = 100;
export const SPEEDS = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4];

export function fmt(ms: number): string {
  const total = Math.max(0, Math.round(ms));
  const m = Math.floor(total / 60000);
  const s = Math.floor((total % 60000) / 1000);
  return `${m}:${String(s).padStart(2, '0')}.${String(total % 1000).padStart(3, '0')}`;
}

export const whole = (duration: number): Segment[] => [{ startMs: 0, endMs: duration, speed: 1 }];

/** How long the edited video lasts. */
export const outputMs = (segs: Segment[]): number => segs.reduce((sum, s) => sum + (s.endMs - s.startMs) / s.speed, 0);

/** The parts left after taking `a..b` out. Pieces shorter than the minimum are dropped with it. */
export function cutRange(segs: Segment[], a: number, b: number): Segment[] {
  const [from, to] = a < b ? [a, b] : [b, a];
  const out: Segment[] = [];
  for (const s of segs) {
    if (to <= s.startMs || from >= s.endMs) {
      out.push(s);
      continue;
    }
    if (from - s.startMs >= MIN_PART_MS) out.push({ ...s, endMs: from });
    if (s.endMs - to >= MIN_PART_MS) out.push({ ...s, startMs: to });
  }
  return out;
}

/** Splits the part holding `t` in two, so each half can be sped up or deleted on its own. */
export function splitAt(segs: Segment[], t: number): Segment[] {
  const i = segs.findIndex((s) => t - s.startMs >= MIN_PART_MS && s.endMs - t >= MIN_PART_MS);
  if (i < 0) return segs;
  return [...segs.slice(0, i), { ...segs[i], endMs: t }, { ...segs[i], startMs: t }, ...segs.slice(i + 1)];
}

export const partAt = (segs: Segment[], t: number): number => segs.findIndex((s) => t >= s.startMs && t < s.endMs);

/** The same zoom curve the backend draws: ease in over 300 ms, hold, ease out. */
export function zoomAt(zooms: Zoom[], t: number): { level: number; x: number; y: number } | null {
  const z = zooms.find((z) => t >= z.startMs && t < z.endMs);
  if (!z) return null;
  const ease = Math.min(300, (z.endMs - z.startMs) / 2);
  const p = Math.min(Math.max(Math.min((t - z.startMs) / ease, (z.endMs - t) / ease), 0), 1);
  return { level: 1 + (z.level - 1) * p * p * (3 - 2 * p), x: z.x, y: z.y };
}

/** CSS that shows the picture zoomed toward the focus the way the saved video will be. */
export function zoomStyle(level: number, x: number, y: number): string {
  const left = Math.min(Math.max(x - 0.5 / level, 0), 1 - 1 / level);
  const top = Math.min(Math.max(y - 0.5 / level, 0), 1 - 1 / level);
  return `transform: scale(${level}) translate(${-left * 100}%, ${-top * 100}%)`;
}

export const clamp01 = (v: number): number => Math.min(Math.max(v, 0), 1);

export function hexToRgb(hex: string): [number, number, number] {
  const n = parseInt(hex.replace('#', ''), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** The corner points of the arrowhead for an arrow drawn on a picture `w` by `h` pixels, and where the shaft ends. */
export function arrowShape(a: Arrow, w: number, h: number): { shaftEnd: [number, number]; head: string } {
  const [x1, y1, x2, y2] = [a.x1 * w, a.y1 * h, a.x2 * w, a.y2 * h];
  const len = Math.hypot(x2 - x1, y2 - y1) || 1;
  const [ux, uy] = [(x2 - x1) / len, (y2 - y1) / len];
  const width = a.width * h;
  const headLen = Math.min(Math.max(width * 4, 12), len);
  const half = headLen * 0.55;
  const [bx, by] = [x2 - ux * headLen, y2 - uy * headLen];
  return {
    shaftEnd: [bx, by],
    head: `${x2},${y2} ${bx - uy * half},${by + ux * half} ${bx + uy * half},${by - ux * half}`,
  };
}

export const FONT = '600 {px}px "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif';

/** Draws a text label as a PNG at the video's own size, the way the backend lays it onto the frames. */
export async function textPng(t: Text, videoHeight: number): Promise<number[]> {
  const px = Math.max(8, Math.round(t.size * videoHeight));
  const pad = Math.round(px * 0.15);
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('Text could not be drawn.');
  ctx.font = FONT.replace('{px}', String(px));
  canvas.width = Math.ceil(ctx.measureText(t.text).width) + pad * 2;
  canvas.height = Math.ceil(px * 1.3) + pad * 2;
  // Resizing a canvas resets its state.
  ctx.font = FONT.replace('{px}', String(px));
  ctx.textBaseline = 'top';
  ctx.lineJoin = 'round';
  ctx.lineWidth = px * 0.12;
  ctx.strokeStyle = 'rgba(0, 0, 0, 0.6)';
  ctx.strokeText(t.text, pad, pad);
  ctx.fillStyle = t.color;
  ctx.fillText(t.text, pad, pad);
  const blob = await new Promise<Blob | null>((done) => canvas.toBlob(done, 'image/png'));
  if (!blob) throw new Error('Text could not be drawn.');
  return Array.from(new Uint8Array(await blob.arrayBuffer()));
}
