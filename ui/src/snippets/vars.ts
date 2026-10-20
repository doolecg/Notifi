// Snippet variables: {clipboard}, {date}, {time}, {date:FORMAT} and {date:FORMAT@ZONE}; {{ and }} are literal braces.
// Rust expands them; the UI only needs to find zones, show variables and build date tokens.

export interface Snippet {
  id: string;
  name: string;
  trigger: string;
  text: string;
  group: string;
}

export type Piece = { kind: 'text'; value: string } | { kind: 'var'; raw: string; name: string; format: string; zone: string };

const VAR = /^(clipboard|date|time)(?::(.*))?$/s;
const OFFSET = /^[+-]\d{2}:\d{2}$/;

export function parse(text: string): Piece[] {
  const out: Piece[] = [];
  let buf = '';
  let i = 0;
  while (i < text.length) {
    const two = text.slice(i, i + 2);
    if (two === '{{' || two === '}}') {
      buf += two[0];
      i += 2;
      continue;
    }
    if (text[i] === '{') {
      const end = text.indexOf('}', i + 1);
      const m = end > 0 ? VAR.exec(text.slice(i + 1, end)) : null;
      if (m) {
        if (buf) out.push({ kind: 'text', value: buf });
        buf = '';
        let format = m[2] ?? '';
        let zone = '';
        const at = format.lastIndexOf('@');
        if (at >= 0 && !format.slice(at).includes(']')) {
          zone = format.slice(at + 1);
          format = format.slice(0, at);
        }
        out.push({ kind: 'var', raw: text.slice(i, end + 1), name: m[1], format, zone });
        i = end + 1;
        continue;
      }
    }
    buf += text[i];
    i += 1;
  }
  if (buf) out.push({ kind: 'text', value: buf });
  return out;
}

export function validZone(zone: string): boolean {
  if (!zone || zone === 'local' || zone === 'UTC' || OFFSET.test(zone)) return true;
  try {
    new Intl.DateTimeFormat('en-US', { timeZone: zone });
    return true;
  } catch {
    return false;
  }
}

/** UTC offset of an IANA zone in minutes, east positive. */
export function offsetOf(zone: string, at = new Date()): number {
  const f = new Intl.DateTimeFormat('en-US', {
    timeZone: zone,
    hourCycle: 'h23',
    year: 'numeric',
    month: 'numeric',
    day: 'numeric',
    hour: 'numeric',
    minute: 'numeric',
    second: 'numeric',
  });
  const p: Record<string, number> = {};
  for (const part of f.formatToParts(at)) p[part.type] = Number(part.value);
  const wall = Date.UTC(p.year, p.month - 1, p.day, p.hour % 24, p.minute, p.second);
  return Math.round((wall - Math.floor(at.getTime() / 1000) * 1000) / 60000);
}

/** Offsets for 'local' and every IANA zone the text names, as `snippet_insert` and `snippet_preview` expect. */
export function zonesFor(text: string, at = new Date()): Record<string, number> {
  const zones: Record<string, number> = { local: -at.getTimezoneOffset() };
  for (const p of parse(text)) {
    if (p.kind !== 'var' || !p.zone || p.zone in zones || p.zone === 'UTC' || OFFSET.test(p.zone)) continue;
    try {
      zones[p.zone] = offsetOf(p.zone, at);
    } catch {
      /* unknown zone: Rust reports it */
    }
  }
  return zones;
}

let zoneList: string[] | null = null;

export function allZones(): string[] {
  if (!zoneList) {
    try {
      zoneList = Intl.supportedValuesOf('timeZone');
    } catch {
      zoneList = [];
    }
  }
  return zoneList;
}

export const oneLine = (text: string) => text.replace(/\s+/g, ' ').trim();

export interface Parts {
  weekday: '' | 'ddd' | 'dddd';
  day: '' | 'D' | 'DD';
  month: '' | 'M' | 'MM' | 'MMM' | 'MMMM';
  year: '' | 'YY' | 'YYYY';
  order: 'dmy' | 'mdy' | 'ymd';
  sep: string;
  time: '' | '24' | '12';
  seconds: boolean;
  offset: boolean;
}

export const DEFAULT_PARTS: Parts = {
  weekday: '',
  day: 'DD',
  month: 'MM',
  year: 'YYYY',
  order: 'ymd',
  sep: '-',
  time: '',
  seconds: false,
  offset: false,
};

export function buildFormat(p: Parts): string {
  const byKey = { d: p.day, m: p.month, y: p.year };
  const date = [...p.order].map((k) => byKey[k as 'd' | 'm' | 'y']).filter(Boolean).join(p.sep);
  const head = p.weekday ? (date ? `${p.weekday}, ${date}` : p.weekday) : date;
  let time = '';
  if (p.time) {
    time = p.time === '24' ? 'HH:mm' : 'h:mm';
    if (p.seconds) time += ':ss';
    if (p.time === '12') time += ' A';
  }
  if (p.offset) time = time ? `${time} Z` : 'Z';
  return [head, time].filter(Boolean).join(' ');
}

export function dateToken(format: string, zone: string): string {
  return `{date:${format}${zone && zone !== 'local' ? `@${zone}` : ''}}`;
}
