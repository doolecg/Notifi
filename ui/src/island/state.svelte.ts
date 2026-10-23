import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Ask } from './ask';
import type { Activity, Snapshot } from './types';

class IslandLive {
  snap = $state<Snapshot | null>(null);
  cover = $state<string | null>(null);
  /** When the last snapshot arrived, for moving the media position on between snapshots. */
  received = $state(0);
  now = $state(Date.now());
  activity = $state<Activity | null>(null);
  /** Island events since the app started, newest first, for the Notifications module. */
  history = $state<{ id: number; icon: string; color: string; text: string; at: number; target: string | null }[]>([]);
  #historyId = 0;
  /** Questions and permission requests from AI agents waiting for an answer. */
  asks = $state<Ask[]>([]);
  #activityTimer: ReturnType<typeof setTimeout> | undefined;

  async init(onCommand: (cmd: string) => void) {
    await listen<Snapshot>('island://state', (e) => this.#set(e.payload));
    await listen<string | null>('island://cover', (e) => (this.cover = e.payload));
    await listen<Activity>('island://activity', (e) => this.#showActivity(e.payload));
    await listen<string>('island://command', (e) => onCommand(e.payload));
    await listen<Ask>('ask://new', (e) => (this.asks = [...this.asks.filter((a) => a.id !== e.payload.id), e.payload]));
    await listen<number>('ask://gone', (e) => this.removeAsk(e.payload));
    invoke<Ask[]>('ask_list').then((l) => (this.asks = l)).catch(() => {});
    setInterval(() => (this.now = Date.now()), 500);
    const [snap, cover] = await Promise.all([
      invoke<Snapshot | null>('island_status'),
      invoke<string | null>('island_cover'),
    ]);
    if (snap) this.#set(snap);
    this.cover = cover;
  }

  #set(snap: Snapshot) {
    this.snap = snap;
    this.received = Date.now();
  }

  clearHistory() {
    this.history = [];
  }

  removeAsk(id: number) {
    this.asks = this.asks.filter((a) => a.id !== id);
  }

  #showActivity(a: Activity) {
    // A new song isn't news to keep.
    if (a.icon !== 'music') this.history = [{ id: ++this.#historyId, icon: a.icon, color: a.color, text: a.text, at: Date.now(), target: a.target ?? null }, ...this.history].slice(0, 100);
    if (a.quiet) return;
    this.activity = a;
    clearTimeout(this.#activityTimer);
    this.#activityTimer = setTimeout(() => (this.activity = null), a.ms);
  }

  /** Media position now, moved on from the last snapshot while playing. */
  get position() {
    const m = this.snap?.media;
    if (!m) return 0;
    const moved = m.playing ? Math.max(0, this.now - this.received) : 0;
    const pos = m.positionMs + moved;
    return m.durationMs ? Math.min(pos, m.durationMs) : pos;
  }
}

export const island = new IslandLive();

export const media = (action: string, extra: Record<string, unknown> = {}) =>
  invoke('island_media', { action, value: null, player: null, ...extra }).catch(() => {});

export const timer = (action: string, ms?: number) => invoke('island_timer', { action, ms: ms ?? null }).catch(() => {});

export const runOutside = (action: string) => invoke('island_run', { action }).catch(() => {});
