// Same geometry as src-tauri/src/island/layout.rs, for the settings preview.

export const ORBIT_D = 40;
export const ORBIT_GAP = 10;
export const SIDE_GAP = 12;
export const TOP_PAD = 8;
export const EDGE = 10;
export const MIN_W = 380;
export const MAX_W = 900;
export const MIN_H = 200;
export const MAX_H = 600;

export function openSize(size: string, w: number, h: number): [number, number] {
  if (size === 'spacious') return [560, 300];
  if (size === 'custom') {
    const c = (v: number, lo: number, hi: number) => (Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : lo);
    return [c(w, MIN_W, MAX_W), c(h, MIN_H, MAX_H)];
  }
  return [440, 236];
}

export const maxSide = (h: number) => Math.max(0, Math.floor((h - TOP_PAD + ORBIT_GAP) / (ORBIT_D + ORBIT_GAP)));
export const maxBottom = (w: number) => Math.max(0, Math.floor((w + ORBIT_GAP) / (ORBIT_D + ORBIT_GAP)));

export type Side = 'left' | 'right' | 'bottom';

/** Slot centres relative to the island's top centre. */
export function slots(side: Side, count: number, [w, h]: [number, number]): [number, number][] {
  const step = ORBIT_D + ORBIT_GAP;
  const r = ORBIT_D / 2;
  if (side === 'bottom') {
    const n = Math.min(count, maxBottom(w));
    const total = n * ORBIT_D + Math.max(0, n - 1) * ORBIT_GAP;
    return Array.from({ length: n }, (_, i) => [-total / 2 + r + i * step, h + SIDE_GAP + r]);
  }
  const x = (w / 2 + SIDE_GAP + r) * (side === 'left' ? -1 : 1);
  return Array.from({ length: Math.min(count, maxSide(h)) }, (_, i) => [x, TOP_PAD + r + i * step]);
}

export function windowSize([w, h]: [number, number]): [number, number] {
  const side = SIDE_GAP + ORBIT_D + EDGE;
  return [w + 2 * side, h + SIDE_GAP + ORBIT_D + EDGE];
}
