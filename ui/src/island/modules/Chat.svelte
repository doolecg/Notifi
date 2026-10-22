<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onDestroy, onMount } from 'svelte';
  import Glyph from '../Glyph.svelte';
  import { island } from '../state.svelte';
  import { duration, motion, reducedMotion } from '../../lib/motion';

  type Box = { left: number; top: number; width: number; height: number };
  /** The open and resting sizes of the pill at the top centre (or bottom centre with `rise`); null when docked. */
  type Pill = { openW: number; openH: number; restW: number; restH: number; rise: boolean; lift: number } | null;
  let { closing, pill, onAsk }: { closing: boolean; pill: Pill; onAsk: () => void } = $props();

  let slot = $state<HTMLDivElement>();
  let error = $state('');
  let last = '';
  let seen = '';
  let stillFor = 0;
  let busy = false;
  let stopped = false;
  let raf = 0;
  /** Where this page put the chat, while it is shown. */
  let placed: Box | null = null;

  const key = (r: Box) => [r.left, r.top, r.width, r.height].map(Math.round).join();

  /** The chat moving with the pill: scaled about the pill's fixed edge (top, or bottom when it grows up) to the pill's size (w, h), on the pill's own spring. */
  function morph(r: Box, w: number, h: number, name: 'pillDrop' | 'pillRise') {
    if (!pill || reducedMotion()) return null;
    // The open pill's bottom edge, which floats above the taskbar when rising.
    const oy = pill.rise ? innerHeight - pill.lift - r.top : -r.top;
    return { ox: innerWidth / 2 - r.left, oy, sx: w / pill.openW, sy: h / pill.openH, ms: duration(name), ease: motion[name].ease };
  }

  $effect(() => {
    if (!closing || !placed || !slot) return;
    // Where the chat is now: the window may have moved since it was placed.
    const r = slot.getBoundingClientRect();
    const m = pill ? morph(r, pill.restW, pill.restH, 'pillRise') : null;
    placed = null;
    last = '';
    invoke('chat_hide', { fade: true, morph: m }).catch(() => {});
  });

  async function place(r: Box, m: ReturnType<typeof morph> = null) {
    busy = true;
    placed = r;
    try {
      await invoke('chat_show', { x: r.left, y: r.top, w: r.width, h: r.height, morph: m });
      error = '';
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  /** Keeps the chat on the empty area below. A native view dragged along each frame lags behind the menu, so it
   *  goes straight to where the area will end up and grows there with the pill; later moves wait until the area is still. */
  function follow() {
    if (stopped) return;
    const r = slot?.getBoundingClientRect();
    const content = slot?.closest<HTMLElement>('.content');
    const box = content?.getBoundingClientRect();
    const now = slot?.closest('.pill')?.getBoundingClientRect();
    if (closing || !r || busy || r.width <= 40 || r.height <= 40) {
      // Nothing to do this frame.
    } else if (!placed && pill && content && box && now && slot) {
      // Where the area ends up once the menu has settled. The content comes in slightly scaled down, so the
      // on-screen box is unscaled first (a later resize would make the chat page lay itself out again).
      if (innerWidth >= pill.openW) {
        const k = content.offsetWidth / box.width;
        // The settled pill's top, plus the content's place in it (the strip above it), plus the area's place in the content.
        const pillTop = pill.rise ? innerHeight - pill.lift - pill.openH : 0;
        const end = { left: innerWidth / 2 - pill.openW / 2 + (r.left - box.left) * k, top: pillTop + pill.openH - content.offsetHeight + (r.top - box.top) * k, width: slot.offsetWidth, height: slot.offsetHeight };
        last = seen = key(end);
        place(end, morph(end, now.width, now.height, 'pillDrop'));
      }
    } else {
      const k = key(r);
      // Still for 4 frames, so a spring's turning point does not count as stopped.
      stillFor = k === seen ? stillFor + 1 : 0;
      seen = k;
      if (stillFor >= 4 && k !== last) {
        last = k;
        place(r);
      }
    }
    raf = requestAnimationFrame(follow);
  }

  onMount(() => {
    // The page behind needs the keyboard to type into.
    invoke('island_focus').catch(() => {});
    follow();
  });
  onDestroy(() => {
    stopped = true;
    cancelAnimationFrame(raf);
    invoke('chat_hide').catch(() => {});
  });
</script>

<div class="chat">
  <header>
    <strong>AI chat</strong>
    <span class="host">T3 Code</span>
    <span class="grow"></span>
    {#if island.asks.length}
      <button class="ask" onclick={onAsk}><Glyph name="bell" size={14} />Questions ({island.asks.length})</button>
    {/if}
  </header>
  <div class="slot" bind:this={slot}>
    {#if error}
      <div class="err">
        <strong>{error}</strong>
        <span>The chat is T3 Code's own chat, so T3 Code must be installed and running. Dictation uses Windows voice typing and needs online speech recognition turned on.</span>
        <button onclick={() => (last = '')}>Try again</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .chat {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    gap: 8px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }
  .host {
    color: var(--i-mut);
  }
  .grow {
    flex: 1;
  }
  .ask {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border: 0;
    border-radius: 99px;
    background: color-mix(in srgb, var(--i-agent), transparent 80%);
    color: var(--i-agent);
    font: inherit;
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .slot {
    flex: 1;
    min-height: 0;
    margin-bottom: 8px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--i-fg), transparent 95%);
  }
  .err {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 100%;
    padding: 16px;
    color: var(--i-mut);
    font-size: 12.5px;
    text-align: center;
  }
  .err strong {
    color: var(--i-fg);
    font-size: 13.5px;
  }
  .err button {
    padding: 5px 14px;
    border: 0;
    border-radius: 99px;
    background: color-mix(in srgb, var(--i-fg), transparent 88%);
    color: var(--i-fg);
    font: inherit;
    cursor: pointer;
  }
</style>
