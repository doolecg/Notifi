<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, tick } from 'svelte';
  import { applyCssVars, setReducedMotion } from '../lib/motion';
  import { store } from '../lib/settings.svelte';
  import { rowGlyph, runRow, type Row } from './rows';

  let card: HTMLElement;
  let rows = $state.raw<Row[]>([]);
  let closeAfterRun = $state(true);
  let message = $state('');
  let confirming = $state('');
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function load() {
    rows = await invoke<Row[]>('quick_rows').catch(() => []);
    await tick();
    fit();
  }

  function fit() {
    if (card) invoke('quick_resize', { height: Math.ceil(card.getBoundingClientRect().height) }).catch(() => {});
  }

  function close() {
    confirming = '';
    message = '';
    invoke('quick_hide').catch(() => {});
  }

  async function press(row: Row) {
    message = '';
    if (row.kind === 'tool') {
      await invoke('quick_run', { id: row.id, on: null }).catch((e) => (message = String(e)));
      return;
    }
    try {
      const result = await runRow(row, confirming === row.id);
      if (result === 'confirm') {
        confirming = row.id;
        clearTimeout(timer);
        timer = setTimeout(() => (confirming = ''), 3000);
        return;
      }
      confirming = '';
      if (closeAfterRun) return close();
    } catch (err) {
      message = `${row.label}: ${String(err)}`;
    }
    await load();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (e.isComposing) return;
      e.preventDefault();
      close();
      return;
    }
    const n = Number(e.key);
    if (Number.isInteger(n) && n >= 1 && n <= 9 && !e.ctrlKey && !e.altKey && !e.metaKey && rows[n - 1]) {
      e.preventDefault();
      press(rows[n - 1]);
    }
  }

  onMount(() => {
    applyCssVars();
    store.init().catch(() => {});
    load();
    const offs = [
      listen<{ closeAfterRun: boolean }>('quick://open', (e) => {
        closeAfterRun = e.payload.closeAfterRun;
        confirming = '';
        message = '';
        load();
      }),
      listen('quick://changed', load),
    ];
    window.addEventListener('keydown', onKey);
    return () => {
      offs.forEach((off) => off.then((f) => f()));
      clearTimeout(timer);
      window.removeEventListener('keydown', onKey);
    };
  });

  $effect(() => {
    const theme = store.settings?.general.theme;
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
    setReducedMotion(store.settings?.general.reducedMotion ?? false);
  });
</script>

<div class="card" bind:this={card}>
  <div class="rows" role="menu">
    {#each rows as row, i (row.id)}
      <button class="row" class:off={row.disabled} role="menuitem" title={row.hint} onclick={() => press(row)}>
        <span class="num">{i < 9 ? i + 1 : ''}</span>
        <span class="glyph"><svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true"><path d={rowGlyph(row.id)} /></svg></span>
        <span class="main">
          <span class="label">{row.label}</span>
          {#if confirming === row.id}
            <span class="sub warn">Press again to confirm</span>
          {:else if row.note}
            <span class="sub">{row.note}</span>
          {/if}
        </span>
        {#if row.kind === 'switch'}
          <span class="pill" class:on={row.on}>{row.on ? 'On' : 'Off'}</span>
        {/if}
      </button>
    {/each}
  </div>
  {#if message}<p class="sub warn msg" role="alert">{message}</p>{/if}
</div>

<style>
  :global(html),
  :global(body.quick-page) {
    background: transparent;
  }
  .card {
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    background: var(--bg);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.2);
    overflow: hidden;
  }
  .rows {
    display: flex;
    flex-direction: column;
    padding: 4px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 38px;
    padding: 5px 8px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--fg);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .row:hover,
  .row:focus-visible {
    background: var(--hover);
    outline: none;
  }
  .row.off {
    opacity: 0.55;
  }
  .num {
    width: 12px;
    flex: none;
    color: var(--mut);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    text-align: center;
  }
  .glyph {
    display: grid;
    flex: none;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    background: var(--acc-soft);
    color: var(--acc);
  }
  .glyph svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.7;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .main {
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
  }
  .label {
    overflow: hidden;
    font-size: 13px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    color: var(--mut);
    font-size: 11.5px;
  }
  .warn {
    color: var(--warn);
  }
  .msg {
    margin: 0;
    padding: 4px 12px 8px;
  }
  .pill {
    flex: none;
    padding: 1px 8px;
    border-radius: 99px;
    background: var(--inset);
    color: var(--mut);
    font-size: 11.5px;
    font-weight: 600;
  }
  .pill.on {
    background: var(--acc-soft);
    color: var(--acc);
  }
</style>
