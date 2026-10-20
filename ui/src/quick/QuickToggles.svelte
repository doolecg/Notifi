<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import Switch from '../lib/Switch.svelte';
  import { rowGlyph, runRow, type Row } from './rows';

  let rows = $state.raw<Row[]>([]);
  let message = $state('');
  let confirming = $state('');
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function load() {
    try {
      rows = await invoke<Row[]>('quick_toggles');
    } catch {
      rows = [];
    }
  }

  async function press(row: Row) {
    message = '';
    try {
      const result = await runRow(row, confirming === row.id);
      if (result === 'confirm') {
        confirming = row.id;
        clearTimeout(timer);
        timer = setTimeout(() => (confirming = ''), 3000);
        return;
      }
      confirming = '';
    } catch (err) {
      message = `${row.label}: ${String(err)}`;
    }
    await load();
  }

  onMount(() => {
    load();
    const poll = setInterval(load, 2500);
    const off = listen('quick://changed', load);
    return () => {
      clearInterval(poll);
      clearTimeout(timer);
      off.then((f) => f());
    };
  });
</script>

<div class="p-card list">
  {#each rows as row (row.id)}
    <div class="row" title={row.hint}>
      <span class="p-glyph"><svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true"><path d={rowGlyph(row.id)} /></svg></span>
      <span class="p-main">
        <span class="p-label name">{row.label}</span>
        {#if confirming === row.id}<span class="p-sub warn">Press again to confirm</span>{/if}
      </span>
      {#if row.kind === 'switch'}
        <Switch checked={row.on} label={row.label} onchange={() => press(row)} />
      {:else}
        <button class="ghost run" onclick={() => press(row)}>{confirming === row.id ? 'Confirm' : 'Run'}</button>
      {/if}
    </div>
  {:else}
    <p class="p-sub empty">No toggles are available on this system.</p>
  {/each}
</div>
{#if message}<p class="p-sub warn msg" role="alert">{message}</p>{/if}

<style>
  .list {
    overflow: hidden;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
  }
  .row + .row {
    border-top: 1px solid var(--line);
  }
  .row svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.7;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .name {
    font-size: 13px;
    font-weight: 600;
  }
  .warn {
    color: var(--warn);
  }
  .msg {
    margin: 6px 4px 0;
  }
  .empty {
    margin: 0;
    padding: 12px;
  }
  .run {
    padding: 4px 10px;
  }
</style>
