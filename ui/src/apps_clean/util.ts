export function bytes(n: number): string {
  if (n <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${i === 0 ? v : v.toFixed(v >= 100 ? 0 : 1)} ${units[i]}`;
}

export function days(n: number): string {
  if (n < 1) return 'today';
  if (n < 2) return '1 day';
  if (n < 60) return `${Math.floor(n)} days`;
  if (n < 730) return `${Math.floor(n / 30)} months`;
  return `${Math.floor(n / 365)} years`;
}

export function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e);
}

/** The last part of a path, for showing a path compactly. */
export function leaf(path: string): string {
  return path.split(/[\/]/).filter(Boolean).pop() ?? path;
}
