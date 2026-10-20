<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars } from '../lib/motion';

  interface Sample {
    x: number;
    y: number;
    cells: number[];
    hex: string;
    text: string;
  }
  interface Recent {
    hex: string;
    text: string;
  }

  const GRID = 11;
  const CELL = 12;

  let mode = $state<'pick' | 'list'>('pick');
  let sample = $state<Sample | null>(null);
  let recent = $state<Recent[]>([]);
  let copied = $state('');
  let canvas = $state<HTMLCanvasElement>();
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  function draw(cells: number[]) {
    const g = canvas?.getContext('2d');
    if (!g) return;
    for (let i = 0; i < GRID * GRID; i++) {
      g.fillStyle = `rgb(${cells[i * 3]} ${cells[i * 3 + 1]} ${cells[i * 3 + 2]})`;
      g.fillRect((i % GRID) * CELL, Math.floor(i / GRID) * CELL, CELL, CELL);
    }
    const mid = Math.floor(GRID / 2) * CELL;
    g.lineWidth = 2;
    g.strokeStyle = '#000';
    g.strokeRect(mid + 1, mid + 1, CELL - 2, CELL - 2);
    g.lineWidth = 1;
    g.strokeStyle = '#fff';
    g.strokeRect(mid + 2.5, mid + 2.5, CELL - 5, CELL - 5);
  }

  $effect(() => {
    if (mode === 'pick' && sample) draw(sample.cells);
  });

  async function openList() {
    recent = await invoke<Recent[]>('picker_history').catch(() => []);
    mode = 'list';
  }

  const close = () => invoke('picker_close').catch(() => {});
  const pick = (hex: string) => invoke('picker_copy', { hex }).catch(() => {});

  function onKey(e: KeyboardEvent) {
    if (mode === 'list' && e.key === 'Escape') close();
  }

  onMount(() => {
    applyCssVars();
    const offs = [
      listen<Sample>('picker://sample', (e) => (sample = e.payload)),
      listen<string>('picker://mode', (e) => {
        if (e.payload === 'list') openList();
        else {
          mode = 'pick';
          sample = null;
        }
      }),
      listen<string>('picker://copied', (e) => {
        copied = e.payload;
        clearTimeout(copiedTimer);
        copiedTimer = setTimeout(() => (copied = ''), 900);
      }),
    ];
    window.addEventListener('keydown', onKey);
    window.addEventListener('blur', () => mode === 'list' && close());
    return () => {
      window.removeEventListener('keydown', onKey);
      offs.forEach((p) => p.then((off) => off()));
    };
  });
</script>

{#if mode === 'pick'}
  <div class="card pick">
    <canvas bind:this={canvas} width={GRID * CELL} height={GRID * CELL}></canvas>
    <div class="value">
      <i style:background={sample?.hex ?? 'transparent'}></i>
      <span title={sample?.text}>{copied ? 'Copied' : (sample?.text ?? '')}</span>
    </div>
    <p class="hint">Click or C copies. Arrows nudge. Tab lists recent colours. Esc cancels.</p>
  </div>
{:else}
  <div class="card list">
    <h2>Recent colours</h2>
    {#if recent.length === 0}
      <p class="empty">Nothing yet. Colours you copy show up here.</p>
    {/if}
    <div class="rows">
      {#each recent as r (r.hex)}
        <button class="row" onclick={() => pick(r.hex)} title="Copy {r.text}">
          <i style:background={r.hex}></i>
          <span>{r.text}</span>
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  :global(html),
  :global(body.picker-page) {
    background: transparent;
    overflow: hidden;
    user-select: none;
  }
  .card {
    box-sizing: border-box;
    height: 100%;
    padding: 10px;
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    background: var(--bg);
    color: var(--fg);
    font-family: var(--sans);
  }
  .pick {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
  canvas {
    width: 132px;
    height: 132px;
    border-radius: 6px;
    image-rendering: pixelated;
  }
  .value {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    font-size: 12px;
    font-weight: 600;
  }
  .value span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  i {
    flex: none;
    width: 18px;
    height: 18px;
    border: 1px solid var(--line-strong);
    border-radius: 5px;
  }
  .hint {
    margin: 0;
    color: var(--mut);
    font-size: 11px;
    line-height: 1.35;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }
  .empty {
    margin: 0;
    color: var(--mut);
    font-size: 12px;
  }
  .rows {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-height: 0;
    overflow-y: auto;
  }
  .row {
    display: flex;
    flex: none;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--fg);
    font: inherit;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .row:hover {
    background: var(--hover);
  }
  .row span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
