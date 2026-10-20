const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

export function bytes(n: number, digits = 1): string {
  let i = 0;
  while (n >= 1024 && i < UNITS.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(i === 0 ? 0 : n >= 100 ? 0 : digits)} ${UNITS[i]}`;
}

export const rate = (n: number) => `${bytes(n)}/s`;

export const pct = (n: number) => `${Math.round(n)}%`;

export function duration(seconds: number): string {
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (d) return `${d} d ${h} h`;
  if (h) return `${h} h ${m} min`;
  return `${m} min`;
}

/** mm:ss or h:mm:ss for a countdown. */
export function clock(ms: number): string {
  const s = Math.max(0, Math.ceil(ms / 1000));
  const h = Math.floor(s / 3600);
  const mm = String(Math.floor((s % 3600) / 60)).padStart(h ? 2 : 1, '0');
  const ss = String(s % 60).padStart(2, '0');
  return h ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

export const watts = (w: number) => `${w.toFixed(w < 10 ? 1 : 0)} W`;
