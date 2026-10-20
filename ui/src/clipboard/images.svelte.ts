import { invoke } from '@tauri-apps/api/core';
import { SvelteMap } from 'svelte/reactivity';

/** Image data URLs by `id:full`, fetched on first request. `null` means none could be made. */
const cache = new SvelteMap<string, string | null>();
const pending = new Set<string>();

const key = (id: number, full: boolean) => `${id}:${full ? 1 : 0}`;

/** Starts a fetch if needed. Call from an effect, not while rendering. */
export function want(id: number, full = false) {
  const k = key(id, full);
  if (cache.has(k) || pending.has(k)) return;
  pending.add(k);
  invoke<string | null>('clipboard_image', { id, full })
    .then((url) => cache.set(k, url))
    .catch(() => cache.set(k, null))
    .finally(() => pending.delete(k));
}

export function image(id: number, full = false): string | null | undefined {
  return cache.get(key(id, full));
}

export function dropImages(keep: Set<number>) {
  for (const k of [...cache.keys()]) if (!keep.has(Number(k.split(':')[0]))) cache.delete(k);
}
