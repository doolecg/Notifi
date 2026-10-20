// Hit-testing, moving and canvas-expansion maths for the screenshot editor. Pure, so it can be tested
// with `node --test ui/src/capture/geometry.test.mjs`. Coordinates are in image pixels.

export interface Pt {
  x: number;
  y: number;
}
export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
/** Extra canvas around the capture: top, right, bottom, left. */
export interface Pad {
  t: number;
  r: number;
  b: number;
  l: number;
}
/** The fields of a drawn item that the maths needs. */
export interface Item {
  id: number;
  t: string;
  size: number;
  pts?: Pt[];
  a?: Pt;
  b?: Pt;
  at?: Pt;
  text?: string;
  /** Image layers: `a` is the top-left corner. */
  w?: number;
  h?: number;
}

/** Average glyph width of the editor's text font, as a share of its size. */
const GLYPH = 0.58;

export const hasPad = (p: Pad): boolean => p.t > 0 || p.r > 0 || p.b > 0 || p.l > 0;

export const noPad = (): Pad => ({ t: 0, r: 0, b: 0, l: 0 });

/** The canvas size once the padding is added. */
export const padded = (size: { w: number; h: number }, p: Pad): { w: number; h: number } => ({
  w: size.w + p.l + p.r,
  h: size.h + p.t + p.b,
});

/** Where the capture sits inside the padded canvas. */
export const innerBox = (size: { w: number; h: number }, p: Pad): Box => ({ x: p.l, y: p.t, w: size.w, h: size.h });

/** A point of the padded canvas as a point of the capture. */
export const toImage = (c: Pt, p: Pad): Pt => ({ x: c.x - p.l, y: c.y - p.t });

export function dist(p: Pt, a: Pt, b: Pt): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len = dx * dx + dy * dy;
  const t = len === 0 ? 0 : Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / len));
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}

const around = (a: Pt, b: Pt): Box => ({ x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w: Math.abs(b.x - a.x), h: Math.abs(b.y - a.y) });

const inside = (bx: Box, p: Pt, grow: number): boolean =>
  p.x >= bx.x - grow && p.x <= bx.x + bx.w + grow && p.y >= bx.y - grow && p.y <= bx.y + bx.h + grow;

export function bounds(s: Item): Box {
  if (s.pts && s.pts.length) {
    const xs = s.pts.map((q) => q.x);
    const ys = s.pts.map((q) => q.y);
    const x = Math.min(...xs);
    const y = Math.min(...ys);
    return { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y };
  }
  if (s.t === 'image' && s.a) return { x: s.a.x, y: s.a.y, w: s.w ?? 0, h: s.h ?? 0 };
  if (s.t === 'step' && s.at) return { x: s.at.x - s.size, y: s.at.y - s.size, w: s.size * 2, h: s.size * 2 };
  if (s.t === 'text' && s.at) return { x: s.at.x, y: s.at.y, w: (s.text ?? '').length * s.size * GLYPH, h: s.size * 1.2 };
  if (s.a && s.b) return around(s.a, s.b);
  return { x: 0, y: 0, w: 0, h: 0 };
}

/** Whether a point is on a drawn item, with `tol` pixels of slack. Strokes and outlines are hit on the line. */
export function hit(s: Item, p: Pt, tol: number): boolean {
  const half = s.size / 2 + tol;
  switch (s.t) {
    case 'crop':
      return false;
    case 'pen':
    case 'highlight': {
      const pts = s.pts ?? [];
      const reach = s.t === 'highlight' ? (s.size * 5) / 2 + tol : half;
      if (pts.length === 1) return Math.hypot(p.x - pts[0].x, p.y - pts[0].y) <= reach;
      return pts.some((q, i) => i > 0 && dist(p, pts[i - 1], q) <= reach);
    }
    case 'line':
    case 'arrow':
      return !!s.a && !!s.b && dist(p, s.a, s.b) <= half;
    case 'rect': {
      if (!s.a || !s.b) return false;
      const bx = around(s.a, s.b);
      const core = { x: bx.x + half, y: bx.y + half, w: bx.w - half * 2, h: bx.h - half * 2 };
      return inside(bx, p, half) && !(core.w > 0 && core.h > 0 && inside(core, p, 0));
    }
    case 'ellipse': {
      if (!s.a || !s.b) return false;
      const bx = around(s.a, s.b);
      const rx = bx.w / 2;
      const ry = bx.h / 2;
      if (rx <= 0 || ry <= 0) return inside(bx, p, half);
      const k = Math.hypot((p.x - bx.x - rx) / rx, (p.y - bx.y - ry) / ry);
      return Math.abs(k - 1) * Math.min(rx, ry) <= half;
    }
    case 'step':
      return !!s.at && Math.hypot(p.x - s.at.x, p.y - s.at.y) <= s.size + tol;
    default:
      return inside(bounds(s), p, tol);
  }
}

/** Index of the topmost item under the point, or -1. */
export function topHit(list: Item[], p: Pt, tol: number): number {
  for (let i = list.length - 1; i >= 0; i--) if (hit(list[i], p, tol)) return i;
  return -1;
}

/** Ids of every item under the eraser. */
export const eraseHits = (list: Item[], p: Pt, tol: number): number[] => list.filter((s) => hit(s, p, tol)).map((s) => s.id);

/** The item moved by (dx, dy). */
export function moved<T extends Item>(s: T, dx: number, dy: number): T {
  const shift = (q: Pt): Pt => ({ x: q.x + dx, y: q.y + dy });
  const out: Item = { ...s };
  if (s.pts) out.pts = s.pts.map(shift);
  if (s.a) out.a = shift(s.a);
  if (s.b) out.b = shift(s.b);
  if (s.at) out.at = shift(s.at);
  return out as T;
}

/** Whether the point is on the resize corner of an image layer. */
export function onHandle(s: Item, p: Pt, tol: number): boolean {
  if (s.t !== 'image' || !s.a) return false;
  return Math.hypot(p.x - (s.a.x + (s.w ?? 0)), p.y - (s.a.y + (s.h ?? 0))) <= tol;
}

/** An image layer resized from its corner to the point, keeping its shape. */
export function resizedImage<T extends Item>(s: T, p: Pt): T {
  if (!s.a || !s.w || !s.h) return s;
  const w = Math.max(8, p.x - s.a.x);
  return { ...s, w, h: (w * s.h) / s.w };
}

/** Size for a dropped or pasted image: its own size, shrunk to at most half of the capture each way. */
export function fitImage(iw: number, ih: number, size: { w: number; h: number }): { w: number; h: number } {
  const f = Math.min(1, (size.w * 0.5) / Math.max(1, iw), (size.h * 0.5) / Math.max(1, ih));
  return { w: Math.max(1, Math.round(iw * f)), h: Math.max(1, Math.round(ih * f)) };
}
