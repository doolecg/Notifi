<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars } from '../lib/motion';

  interface Rect {
    x: number;
    y: number;
    w: number;
    h: number;
  }
  interface Start {
    mode: 'screenshot' | 'text';
    width: number;
    height: number;
    windows: Rect[];
    monitors: Rect[];
    showMenu: boolean;
    freeze: boolean;
    magnifier: boolean;
    magnification: number;
    outline: Rect | null;
  }

  const MODES = [
    { id: 'screenshot', label: 'Screenshot' },
    { id: 'text', label: 'Copy text' },
  ] as const;

  let start = $state<Start | null>(null);
  let mode = $state<'screenshot' | 'text'>('screenshot');
  let scrolling = $state(false);
  let dpr = $state(1);
  let pointer = $state<{ x: number; y: number } | null>(null);
  let anchor = $state<{ x: number; y: number } | null>(null);
  let dragging = $state(false);
  let flash = $state('');
  let bg = $state<HTMLCanvasElement>();
  let mag = $state<HTMLCanvasElement>();
  let strip = $state<HTMLDivElement>();
  let pixels: Uint8ClampedArray | null = null;
  let flashTimer: ReturnType<typeof setTimeout> | undefined;

  const css = (v: number) => v / dpr;

  /** Whole-pixel box around two corners, as the backend cuts it. */
  function box(a: { x: number; y: number }, b: { x: number; y: number }): Rect {
    const x = Math.floor(Math.min(a.x, b.x));
    const y = Math.floor(Math.min(a.y, b.y));
    return { x, y, w: Math.ceil(Math.max(a.x, b.x)) - x, h: Math.ceil(Math.max(a.y, b.y)) - y };
  }

  const selection = $derived(dragging && anchor && pointer ? box(anchor, pointer) : null);
  const hover = $derived.by(() => {
    if (!start || dragging || !pointer) return null;
    return start.windows.find((w) => pointer!.x >= w.x && pointer!.x < w.x + w.w && pointer!.y >= w.y && pointer!.y < w.y + w.h) ?? null;
  });
  const monitor = $derived.by(() => {
    if (!start) return null;
    const p = pointer ?? { x: 0, y: 0 };
    return (
      start.monitors.find((m) => p.x >= m.x && p.x < m.x + m.w && p.y >= m.y && p.y < m.y + m.h) ?? start.monitors[0] ?? { x: 0, y: 0, w: start.width, h: start.height }
    );
  });

  async function load(s: Start) {
    const buf: ArrayBuffer = await invoke('capture_frame');
    const head = new DataView(buf, 0, 8);
    const w = head.getUint32(0, true);
    const h = head.getUint32(4, true);
    pixels = new Uint8ClampedArray(buf, 8);
    if (bg) {
      bg.width = w;
      bg.height = h;
      bg.getContext('2d')!.putImageData(new ImageData(pixels as never, w, h), 0, 0);
    }
    await invoke('capture_ready');
  }

  function reset(s: Start) {
    start = s;
    mode = s.mode;
    scrolling = false;
    dpr = window.devicePixelRatio || 1;
    pointer = null;
    anchor = null;
    dragging = false;
    flash = '';
    pixels = null;
    load(s).catch(() => invoke('capture_cancel').catch(() => {}));
  }

  function say(text: string) {
    flash = text;
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => (flash = ''), 1200);
  }

  const cancel = () => invoke('capture_cancel').catch(() => {});

  function finish(target: Record<string, unknown>) {
    invoke('capture_finish', { finish: { target, scrolling: mode === 'screenshot' && scrolling } }).catch(() => {});
    start = null;
  }

  function overStrip(e: { clientX: number; clientY: number }) {
    const r = strip?.getBoundingClientRect();
    return !!r && e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
  }

  function onDown(e: MouseEvent) {
    if (e.button !== 0 || overStrip(e)) return;
    anchor = { x: e.clientX * dpr, y: e.clientY * dpr };
  }

  function onMove(e: MouseEvent) {
    pointer = { x: e.clientX * dpr, y: e.clientY * dpr };
    if (anchor && !dragging && Math.hypot(pointer.x - anchor.x, pointer.y - anchor.y) >= 3) dragging = true;
    drawMagnifier(e);
  }

  function onUp(e: MouseEvent) {
    if (e.button !== 0 || !anchor) return;
    const a = anchor;
    anchor = null;
    if (dragging) {
      dragging = false;
      const p = { x: e.clientX * dpr, y: e.clientY * dpr };
      finish({ kind: 'area', ax: a.x, ay: a.y, bx: p.x, by: p.y });
    } else if (hover) {
      finish({ kind: 'window', x: a.x, y: a.y });
    }
  }

  function fullScreen() {
    const p = pointer ?? { x: (monitor?.x ?? 0) + 1, y: (monitor?.y ?? 0) + 1 };
    finish({ kind: 'full', x: p.x, y: p.y });
  }

  function hexAt(x: number, y: number) {
    if (!pixels || !bg || x < 0 || y < 0 || x >= bg.width || y >= bg.height) return '';
    const i = (y * bg.width + x) * 4;
    return '#' + [0, 1, 2].map((k) => pixels![i + k].toString(16).padStart(2, '0')).join('').toUpperCase();
  }

  const cells = $derived.by(() => {
    const m = start?.magnification ?? 8;
    const n = Math.ceil(200 / m);
    return { n: n % 2 ? n : n + 1, m };
  });

  const readout = $derived.by(() => {
    if (!pointer) return '';
    const x = Math.floor(pointer.x);
    const y = Math.floor(pointer.y);
    return `${x}, ${y}  ${hexAt(x, y)}`;
  });

  function drawMagnifier(e: MouseEvent) {
    if (!start?.magnifier || !mag || !bg || overStrip(e)) return;
    const { n, m } = cells;
    const g = mag.getContext('2d')!;
    const px = Math.floor(e.clientX * dpr);
    const py = Math.floor(e.clientY * dpr);
    const half = (n - 1) / 2;
    g.imageSmoothingEnabled = false;
    g.clearRect(0, 0, mag.width, mag.height);
    g.drawImage(bg, px - half, py - half, n, n, 0, 0, n * m, n * m);
    if (m >= 6) {
      g.strokeStyle = 'rgba(0,0,0,0.18)';
      g.lineWidth = 1;
      g.beginPath();
      for (let i = 1; i < n; i++) {
        g.moveTo(i * m + 0.5, 0);
        g.lineTo(i * m + 0.5, n * m);
        g.moveTo(0, i * m + 0.5);
        g.lineTo(n * m, i * m + 0.5);
      }
      g.stroke();
    }
    g.lineWidth = 2;
    g.strokeStyle = '#000';
    g.strokeRect(half * m + 1, half * m + 1, m - 2, m - 2);
    g.lineWidth = 1;
    g.strokeStyle = '#fff';
    g.strokeRect(half * m + 2.5, half * m + 2.5, m - 5, m - 5);
  }

  const magBox = $derived.by(() => {
    if (!start?.magnifier || !pointer) return null;
    const size = cells.n * cells.m;
    const w = css(size) + 16;
    const h = css(size) + 44;
    const px = css(pointer.x);
    const py = css(pointer.y);
    let x = px + 24;
    let y = py + 24;
    if (x + w > css(start.width)) x = px - 24 - w;
    if (y + h > css(start.height)) y = py - 24 - h;
    return { x: Math.max(0, x), y: Math.max(0, y), size };
  });

  async function copyPixel() {
    if (!pointer) return;
    try {
      const text: string = await invoke('capture_copy_pixel', { x: Math.floor(pointer.x), y: Math.floor(pointer.y) });
      say(`Copied ${text}`);
    } catch {
      say('Could not copy that colour');
    }
  }

  function nextMode() {
    if (!start?.showMenu) return;
    const i = MODES.findIndex((m) => m.id === mode);
    mode = MODES[(i + 1) % MODES.length].id;
  }

  function onKey(e: KeyboardEvent) {
    const step = e.shiftKey ? 10 : 1;
    const arrows: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
    if (e.key === 'Escape') cancel();
    else if (e.key === 'Tab') {
      e.preventDefault();
      nextMode();
    } else if (e.key in arrows) {
      e.preventDefault();
      invoke('capture_nudge', { dx: arrows[e.key][0], dy: arrows[e.key][1] }).catch(() => {});
    } else if (e.key === 'c' || e.key === 'C') copyPixel();
    else if ((e.key === 's' || e.key === 'S') && mode === 'screenshot') scrolling = !scrolling;
    else if (e.key === 'f' || e.key === 'F') fullScreen();
  }

  onMount(() => {
    applyCssVars();
    const off = listen<Start>('capture://start', (e) => reset(e.payload));
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
      off.then((f) => f());
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="root" onmousedown={onDown} onmousemove={onMove} onmouseup={onUp} oncontextmenu={(e) => (e.preventDefault(), cancel())}>
  <canvas bind:this={bg} class:hidden={!start?.freeze} style:width="{css(start?.width ?? 0)}px" style:height="{css(start?.height ?? 0)}px"></canvas>
  {#if start}
    {#if !selection}
      <div class="dim"></div>
    {/if}
    {#if start.outline && !selection}
      <div class="outline" style:left="{css(start.outline.x)}px" style:top="{css(start.outline.y)}px" style:width="{css(start.outline.w)}px" style:height="{css(start.outline.h)}px"></div>
    {/if}
    {#if hover}
      <div class="hover" style:left="{css(hover.x)}px" style:top="{css(hover.y)}px" style:width="{css(hover.w)}px" style:height="{css(hover.h)}px"></div>
    {/if}
    {#if selection}
      <div class="sel" style:left="{css(selection.x)}px" style:top="{css(selection.y)}px" style:width="{css(selection.w)}px" style:height="{css(selection.h)}px"></div>
      <div class="size" style:left="{css(selection.x)}px" style:top="{Math.max(0, css(selection.y) - 26)}px">{selection.w} x {selection.h}</div>
    {/if}
    {#if monitor}
      <div class="strip" bind:this={strip} style:left="{css(monitor.x + monitor.w / 2)}px" style:top="{css(monitor.y) + 16}px">
        {#if start.showMenu}
          {#each MODES as m (m.id)}
            <button class:on={mode === m.id} onclick={() => (mode = m.id)}>{m.label}</button>
          {/each}
          <button onclick={() => invoke('capture_color').catch(() => {})}>Colour</button>
          <span class="sep"></span>
        {/if}
        {#if mode === 'screenshot'}
          <button class:on={scrolling} onclick={() => (scrolling = !scrolling)} title="S">Scrolling</button>
        {/if}
        <button onclick={fullScreen} title="F">Full screen</button>
        <button onclick={cancel} title="Esc">Cancel</button>
      </div>
    {/if}
    {#if magBox}
      <div class="mag" style:left="{magBox.x}px" style:top="{magBox.y}px">
        <canvas bind:this={mag} width={magBox.size} height={magBox.size} style:width="{css(magBox.size)}px" style:height="{css(magBox.size)}px"></canvas>
        <span>{flash || readout}</span>
        {#if !flash && !selection}
          <small>C copies the colour. Arrows nudge.</small>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  :global(html),
  :global(body.capture-page) {
    background: transparent;
    overflow: hidden;
    user-select: none;
  }
  .root {
    position: fixed;
    inset: 0;
    cursor: crosshair;
    overflow: hidden;
  }
  canvas {
    position: absolute;
    left: 0;
    top: 0;
    display: block;
  }
  canvas.hidden {
    visibility: hidden;
  }
  .dim {
    position: absolute;
    inset: 0;
    background: rgb(0 0 0 / 0.38);
  }
  .sel {
    position: absolute;
    border: 1px solid var(--acc);
    box-shadow: 0 0 0 100vmax rgb(0 0 0 / 0.38);
  }
  .hover {
    position: absolute;
    border: 2px solid var(--acc);
    background: var(--acc-soft);
  }
  .outline {
    position: absolute;
    border: 1px dashed rgb(255 255 255 / 0.8);
  }
  .size {
    position: absolute;
    padding: 2px 7px;
    border-radius: 6px;
    background: var(--toast);
    color: var(--toast-fg);
    font: 12px/1.4 var(--sans);
  }
  .strip {
    position: absolute;
    display: flex;
    gap: 4px;
    align-items: center;
    padding: 5px;
    border-radius: 12px;
    background: var(--surface);
    color: var(--fg);
    border: 1px solid var(--line-strong);
    width: max-content;
    transform: translateX(-50%);
    font: 13px/1 var(--sans);
    cursor: default;
  }
  .strip button {
    padding: 7px 11px;
    white-space: nowrap;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .strip button:hover {
    background: var(--hover);
  }
  .strip button.on {
    background: var(--acc-soft);
    color: var(--acc);
  }
  .sep {
    width: 1px;
    height: 18px;
    background: var(--line-strong);
  }
  .mag {
    position: absolute;
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 8px;
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    background: var(--surface);
    color: var(--fg);
    pointer-events: none;
    font: 12px/1.3 var(--sans);
  }
  .mag canvas {
    position: static;
    border-radius: 4px;
    image-rendering: pixelated;
  }
  .mag small {
    color: var(--mut);
    font-size: 11px;
  }
</style>
