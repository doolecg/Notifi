import { invoke } from '@tauri-apps/api/core';

/** How previews are drawn: live (the OS composites over empty slots), snapshot images, or none. */
export type ThumbKind = 'live' | 'snapshot' | 'none';
export type Layout = 'previews' | 'icons' | 'simple';

export interface Entry {
  id: number;
  title: string;
  app: string;
  pid: number | null;
  minimized: boolean;
  monitor: string | null;
  icon: string | null;
  noThumb: boolean;
  /** On another virtual desktop. */
  otherDesktop: boolean;
  /** An app with no window; choosing it starts or reopens the program. */
  noWindow: boolean;
}

export interface Grid {
  cols: number;
  rows: number;
  cardW: number;
  cardH: number;
  thumbH: number;
  width: number;
  height: number;
}

export interface Model {
  layout: Layout;
  entries: Entry[];
  query: string;
  visible: number[];
  selected: number;
  group: number;
  groupWindow: number;
}

export interface OpenPayload {
  model: Model;
  grid: Grid;
  kind: ThumbKind;
  size: string;
  hoverButtons: boolean;
  skipAnimations: boolean;
  wheel: boolean;
}

export interface UpdatePayload {
  query: string;
  visible: number[];
  selected: number;
  group: number;
  groupWindow: number;
}

/** Sends the rectangles of every `[data-slot]` element so the backend can place previews there. */
export function reportSlots(root: HTMLElement | undefined, command: string) {
  if (!root) return;
  const slots = [...root.querySelectorAll<HTMLElement>('[data-slot]')].map((el) => {
    const r = el.getBoundingClientRect();
    return { id: Number(el.dataset.slot), x: r.left, y: r.top, w: r.width, h: r.height };
  });
  invoke(command, { slots }).catch(() => {});
}

/** Groups visible entries by app, in order of each app's most recent window. */
export function groupsOf(entries: Entry[], visible: number[]): { app: string; items: number[] }[] {
  const out: { app: string; key: string; items: number[] }[] = [];
  for (const i of visible) {
    const key = entries[i].app.toLowerCase();
    const g = out.find((x) => x.key === key);
    if (g) g.items.push(i);
    else out.push({ app: entries[i].app, key, items: [i] });
  }
  return out;
}
