<script lang="ts">
  // `pickArea` of the Island's Watch card (spec/everyday/watch.md): shows a picture of the screen
  // and lets the person drag a rectangle on it. The area is kept by Rust (watch-area.json).
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import type { Opt } from '../lib/settings.svelte';

  interface Region {
    x: number;
    y: number;
    w: number;
    h: number;
  }
  interface Area {
    region: Region;
    anchor: { id: number; bounds: Region } | null;
  }
  interface Screen {
    image: string;
    region: Region;
  }

  let { option }: { option: Opt } = $props();
  let area = $state<Area | null>(null);
  let screen = $state<Screen | null>(null);
  let message = $state('');
  let busy = $state(false);
  let picture = $state<HTMLImageElement>();
  let drag = $state<{ x0: number; y0: number; x1: number; y1: number } | null>(null);

  const load = () => invoke<Area | null>('watch_area').then((a) => (area = a)).catch(() => {});
  onMount(load);

  const say = (e: unknown) => (message = typeof e === 'string' ? e : String(e));

  async function start() {
    message = '';
    busy = true;
    try {
      screen = await invoke<Screen>('watch_screen', { maxWidth: 1600 });
    } catch (e) {
      say(e);
    }
    busy = false;
  }

  function point(e: PointerEvent) {
    const box = picture!.getBoundingClientRect();
    return { x: Math.min(Math.max(e.clientX - box.left, 0), box.width), y: Math.min(Math.max(e.clientY - box.top, 0), box.height) };
  }

  function down(e: PointerEvent) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const p = point(e);
    drag = { x0: p.x, y0: p.y, x1: p.x, y1: p.y };
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const p = point(e);
    drag = { ...drag, x1: p.x, y1: p.y };
  }

  async function up() {
    const d = drag;
    const s = screen;
    drag = null;
    if (!d || !s || !picture) return;
    const box = picture.getBoundingClientRect();
    const kx = s.region.w / box.width;
    const ky = s.region.h / box.height;
    const region = {
      x: Math.round(s.region.x + Math.min(d.x0, d.x1) * kx),
      y: Math.round(s.region.y + Math.min(d.y0, d.y1) * ky),
      w: Math.round(Math.abs(d.x1 - d.x0) * kx),
      h: Math.round(Math.abs(d.y1 - d.y0) * ky),
    };
    try {
      area = await invoke<Area>('watch_set_area', { region });
      screen = null;
      message = '';
    } catch (e) {
      say(e);
    }
  }

  async function clear() {
    await invoke('watch_clear_area');
    area = null;
    message = '';
  }

  async function read() {
    message = 'Reading...';
    try {
      const text = await invoke<string>('watch_read');
      message = text.trim() ? `Reads: ${text}` : 'No text found in that area.';
    } catch (e) {
      say(e);
    }
  }

  const box = $derived(drag ? { left: Math.min(drag.x0, drag.x1), top: Math.min(drag.y0, drag.y1), width: Math.abs(drag.x1 - drag.x0), height: Math.abs(drag.y1 - drag.y0) } : null);
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && screen && (screen = null)} />

<div class="row">
  <span class="name">{option.label}<small>{message || (area ? `${area.region.w} x ${area.region.h} px at ${area.region.x}, ${area.region.y}` : option.hint)}</small></span>
  <span class="buttons">
    <button onclick={start} disabled={busy}>{area ? 'Pick again' : 'Pick area'}</button>
    {#if area}
      <button onclick={read}>Read now</button>
      <button onclick={clear}>Clear</button>
    {/if}
  </span>
</div>

{#if screen}
  <div class="scrim" role="dialog" aria-label="Pick the area to watch">
    <p>Drag a rectangle over the part to watch. Esc cancels.</p>
    <div class="stage" role="presentation" onpointerdown={down} onpointermove={move} onpointerup={up}>
      <img bind:this={picture} src={screen.image} alt="The screen" draggable="false" />
      {#if box}<span class="pick" style:left="{box.left}px" style:top="{box.top}px" style:width="{box.width}px" style:height="{box.height}px"></span>{/if}
    </div>
    <button onclick={() => (screen = null)}>Cancel</button>
  </div>
{/if}

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
  }
  .name {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    font-size: 13px;
  }
  small {
    color: var(--mut);
    font-size: 11.5px;
    overflow-wrap: anywhere;
  }
  .buttons {
    display: flex;
    gap: 8px;
    flex: none;
  }
  button {
    height: 28px;
    padding: 0 14px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--inset);
    color: var(--fg);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }
  button:disabled {
    color: var(--mut);
    cursor: default;
  }
  button:focus-visible {
    outline: 2px solid var(--acc);
    outline-offset: 2px;
  }
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 16px;
    background: rgb(0 0 0 / 0.85);
    color: #fff;
  }
  p {
    margin: 0;
    font-size: 13px;
  }
  .stage {
    position: relative;
    max-width: 100%;
    max-height: calc(100% - 90px);
    cursor: crosshair;
    touch-action: none;
    user-select: none;
  }
  img {
    display: block;
    max-width: 100%;
    max-height: calc(100vh - 120px);
  }
  .pick {
    position: absolute;
    border: 2px solid var(--acc);
    background: rgb(255 255 255 / 0.12);
    pointer-events: none;
  }
</style>
