<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onDestroy, onMount } from 'svelte';
  import Glyph from '../Glyph.svelte';
  import { island } from '../state.svelte';

  let { onAsk }: { onAsk: () => void } = $props();

  let slot = $state<HTMLDivElement>();
  let error = $state('');
  let last = '';
  let busy = false;
  let stopped = false;
  let raf = 0;

  async function place(r: DOMRect) {
    busy = true;
    try {
      await invoke('chat_show', { x: r.left, y: r.top, w: r.width, h: r.height });
      error = '';
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  /** Keeps the chat on the empty area below, which moves and grows while the Island opens. */
  function follow() {
    if (stopped) return;
    const r = slot?.getBoundingClientRect();
    if (r && !busy && r.width > 40 && r.height > 40) {
      const key = [r.left, r.top, r.width, r.height].map(Math.round).join();
      if (key !== last) {
        last = key;
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
