import type { GlyphName } from './glyphs';

export type Kind = 'text' | 'rich' | 'image' | 'files' | 'url' | 'color';
export type Layout = 'list' | 'shelf';

export interface Entry {
  id: number;
  kind: Kind;
  /** Preview, at most 4000 chars; files are the paths joined by \n. */
  text: string;
  /** Full text length in chars. */
  length: number;
  app: string;
  /** Unix ms. */
  created: number;
  pinned: boolean;
  files: string[];
  width: number;
  height: number;
}

export interface ListResult {
  entries: Entry[];
  layout: Layout;
  autoPaste: boolean;
}

/** How an entry is drawn; differs from `kind` when text turns out to be a colour or JSON. */
export type Look = 'text' | 'rich' | 'image' | 'files' | 'url' | 'color' | 'json';

const COLOR = /^(#(?:[0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})|(?:rgba?|hsla?)\(\s*[-\d.%\s,/deg]+\))$/i;
const URL_RE = /^https?:\/\/\S+$/i;

const looks = new Map<number, { look: Look; json?: string }>();

function inspect(e: Entry): { look: Look; json?: string } {
  const hit = looks.get(e.id);
  if (hit) return hit;
  let out: { look: Look; json?: string } = { look: e.kind };
  const t = e.text.trim();
  if (e.kind === 'text' || e.kind === 'color') {
    if (e.kind === 'color' || (t.length <= 64 && COLOR.test(t) && CSS.supports('color', t))) out = { look: 'color' };
    else if (URL_RE.test(t)) out = { look: 'url' };
    else if ((t[0] === '{' || t[0] === '[') && e.length === e.text.length) {
      try {
        const v = JSON.parse(t);
        if (v && typeof v === 'object') out = { look: 'json', json: JSON.stringify(v, null, 2) };
      } catch {
        /* not JSON */
      }
    }
  }
  looks.set(e.id, out);
  return out;
}

export const lookOf = (e: Entry) => inspect(e).look;
export const prettyJson = (e: Entry) => inspect(e).json ?? e.text;

export const LOOK_ICON: Record<Look, GlyphName> = {
  text: 'text',
  rich: 'rich',
  image: 'image',
  files: 'files',
  url: 'url',
  color: 'color',
  json: 'json',
};

export const LOOK_LABEL: Record<Look, string> = {
  text: 'Text',
  rich: 'Formatted',
  image: 'Image',
  files: 'Files',
  url: 'Link',
  color: 'Colour',
  json: 'JSON',
};

/** One-line summary for rows and cards. */
export function title(e: Entry): string {
  if (e.kind === 'image') return `Image ${e.width} x ${e.height}`;
  if (e.kind === 'files') {
    const n = e.files.length || e.text.split('\n').filter(Boolean).length;
    const first = (e.files[0] ?? e.text.split('\n')[0] ?? '').split(/[\\/]/).pop() ?? '';
    return n > 1 ? `${first} and ${n - 1} more` : first;
  }
  return e.text.replace(/\s+/g, ' ').trim();
}

/** Lower-cased text searched per entry, cached by id (entry text never changes). */
const hays = new Map<number, string>();

function hay(e: Entry) {
  let h = hays.get(e.id);
  if (h === undefined) {
    h = `${e.text}\n${e.app}`.toLowerCase();
    hays.set(e.id, h);
  }
  return h;
}

export function words(query: string): string[] {
  return query.toLowerCase().split(/\s+/).filter(Boolean);
}

export function search(entries: Entry[], query: string): Entry[] {
  const ws = words(query);
  if (!ws.length) return entries;
  return entries.filter((e) => {
    const h = hay(e);
    for (const w of ws) if (!h.includes(w)) return false;
    return true;
  });
}

/** Drops cached work for ids that are gone. */
export function forget(keep: Set<number>) {
  for (const id of hays.keys()) if (!keep.has(id)) hays.delete(id);
  for (const id of looks.keys()) if (!keep.has(id)) looks.delete(id);
}

export interface Piece {
  text: string;
  hit: boolean;
}

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** Splits `text` into matched and unmatched pieces; with `around`, starts shortly before the first match. */
export function pieces(text: string, ws: string[], around = false): Piece[] {
  if (!ws.length) return [{ text, hit: false }];
  const re = new RegExp(ws.map(escape).sort((a, b) => b.length - a.length).join('|'), 'gi');
  let body = text;
  if (around) {
    const first = text.search(re);
    if (first > 24) body = '…' + text.slice(first - 12);
  }
  const out: Piece[] = [];
  let last = 0;
  re.lastIndex = 0;
  for (let m = re.exec(body); m; m = re.exec(body)) {
    if (m.index > last) out.push({ text: body.slice(last, m.index), hit: false });
    out.push({ text: m[0], hit: true });
    last = m.index + m[0].length;
  }
  if (last < body.length) out.push({ text: body.slice(last), hit: false });
  return out;
}

export function ago(ms: number, now: number): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 45) return 'Now';
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h`;
  const d = Math.round(h / 24);
  if (d === 1) return 'Yesterday';
  if (d < 7) return `${d} days`;
  return new Date(ms).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

export function fullTime(ms: number): string {
  return new Date(ms).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

export function splitPath(p: string): { dir: string; name: string } {
  const i = Math.max(p.lastIndexOf('/'), p.lastIndexOf('\\'));
  return i < 0 ? { dir: '', name: p } : { dir: p.slice(0, i + 1), name: p.slice(i + 1) };
}
