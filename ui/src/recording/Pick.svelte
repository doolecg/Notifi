<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars } from '../lib/motion';

  interface Box {
    x: number;
    y: number;
    w: number;
    h: number;
  }

  const MIN = 16;

  let mode = $state<'area' | 'window'>('area');
  let start = $state<{ x: number; y: number } | null>(null);
  let drag = $state<Box | null>(null);
  let hover = $state<Box | null>(null);
  let asking = false;

  const box = $derived(mode === 'area' ? drag : hover);
  const size = $derived(box ? `${Math.round(box.w * devicePixelRatio)} x ${Math.round(box.h * devicePixelRatio)}` : '');
  const hint = $derived(mode === 'area' ? 'Drag to choose the area to record. Esc cancels.' : 'Click a window to record it. Esc cancels.');

  const cancel = () => invoke('recording_pick_cancel').catch(() => {});

  function reset() {
    start = null;
    drag = null;
    hover = null;
  }

  function span(a: { x: number; y: number }, x: number, y: number): Box {
    return { x: Math.min(a.x, x), y: Math.min(a.y, y), w: Math.abs(x - a.x), h: Math.abs(y - a.y) };
  }

  async function lookUp(x: number, y: number) {
    if (asking) return;
    asking = true;
    const r = await invoke<[number, number, number, number] | null>('recording_window_at', { x, y }).catch(() => null);
    asking = false;
    hover = r ? { x: r[0], y: r[1], w: r[2], h: r[3] } : null;
  }

  function onDown(e: MouseEvent) {
    if (e.button !== 0) return;
    if (mode === 'area') {
      start = { x: e.clientX, y: e.clientY };
      drag = span(start, e.clientX, e.clientY);
    } else if (hover) {
      invoke('recording_pick', { ...hover }).catch(() => {});
    }
  }

  function onMove(e: MouseEvent) {
    if (mode === 'area') {
      if (start) drag = span(start, e.clientX, e.clientY);
    } else {
      lookUp(e.clientX, e.clientY);
    }
  }

  function onUp() {
    if (mode !== 'area' || !start || !drag) return;
    const chosen = drag;
    reset();
    if (chosen.w >= MIN && chosen.h >= MIN) invoke('recording_pick', { ...chosen }).catch(() => {});
    else cancel();
  }

  function onMenu(e: MouseEvent) {
    e.preventDefault();
    cancel();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') cancel();
  }

  onMount(() => {
    applyCssVars();
    const off = listen<string>('recording://pick', (e) => {
      mode = e.payload === 'window' ? 'window' : 'area';
      reset();
    });
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
      off.then((f) => f());
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" class:dim={!box} onmousedown={onDown} onmousemove={onMove} onmouseup={onUp} oncontextmenu={onMenu}>
  {#if box}
    <div class="frame" style:left="{box.x}px" style:top="{box.y}px" style:width="{box.w}px" style:height="{box.h}px">
      <span class="size">{size}</span>
    </div>
  {/if}
  <p class="hint">{hint}</p>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    cursor: crosshair;
  }
  .dim {
    background: rgb(0 0 0 / 0.35);
  }
  .frame {
    position: absolute;
    box-sizing: border-box;
    border: 2px solid var(--acc);
    box-shadow: 0 0 0 100vmax rgb(0 0 0 / 0.45);
    pointer-events: none;
  }
  .size {
    position: absolute;
    left: 0;
    bottom: calc(100% + 6px);
    padding: 3px 8px;
    border-radius: 6px;
    background: var(--toast);
    color: var(--toast-fg);
    font: 600 12px var(--sans);
    white-space: nowrap;
  }
  .hint {
    position: absolute;
    top: 24px;
    left: 50%;
    margin: 0;
    padding: 8px 14px;
    border-radius: 10px;
    background: var(--toast);
    color: var(--toast-fg);
    font: 500 13px var(--sans);
    transform: translateX(-50%);
    pointer-events: none;
  }
</style>
