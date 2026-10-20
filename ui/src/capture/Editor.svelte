<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars } from '../lib/motion';

  type Tool = 'pen' | 'arrow' | 'rect' | 'blur' | 'text';
  interface Pt {
    x: number;
    y: number;
  }
  type Op =
    | { t: 'pen'; pts: Pt[]; color: string; size: number }
    | { t: 'arrow' | 'rect' | 'blur'; a: Pt; b: Pt; color: string; size: number }
    | { t: 'text'; at: Pt; text: string; color: string; size: number };

  const TOOLS: { id: Tool; label: string }[] = [
    { id: 'pen', label: 'Pen' },
    { id: 'arrow', label: 'Arrow' },
    { id: 'rect', label: 'Rectangle' },
    { id: 'blur', label: 'Blur' },
    { id: 'text', label: 'Text' },
  ];
  const COLORS = ['#e5484d', '#f5a524', '#30a46c', '#0091ff', '#ffffff', '#111111'];

  let canvas = $state<HTMLCanvasElement>();
  let base: ImageBitmap | null = null;
  let size = $state({ w: 0, h: 0 });
  let tool = $state<Tool>('rect');
  let color = $state(COLORS[0]);
  let textSize = $state(24);
  let blur = $state(12);
  let ops = $state<Op[]>([]);
  let redo = $state<Op[]>([]);
  let draft = $state<Op | null>(null);
  let typing = $state<{ at: Pt; value: string } | null>(null);
  let status = $state('');
  let input = $state<HTMLInputElement>();

  const stroke = $derived(Math.max(2, Math.round(size.w / 400)));

  function arrow(g: CanvasRenderingContext2D, a: Pt, b: Pt, width: number) {
    const head = Math.max(12, width * 4);
    const ang = Math.atan2(b.y - a.y, b.x - a.x);
    g.beginPath();
    g.moveTo(a.x, a.y);
    g.lineTo(b.x, b.y);
    g.stroke();
    g.beginPath();
    g.moveTo(b.x, b.y);
    g.lineTo(b.x - head * Math.cos(ang - 0.45), b.y - head * Math.sin(ang - 0.45));
    g.lineTo(b.x - head * Math.cos(ang + 0.45), b.y - head * Math.sin(ang + 0.45));
    g.closePath();
    g.fill();
  }

  /** Pixelates the part of the canvas inside a box, in blocks of `block` pixels. */
  function pixelate(g: CanvasRenderingContext2D, a: Pt, b: Pt, block: number) {
    const x = Math.floor(Math.min(a.x, b.x));
    const y = Math.floor(Math.min(a.y, b.y));
    const w = Math.ceil(Math.abs(b.x - a.x));
    const h = Math.ceil(Math.abs(b.y - a.y));
    if (w < 2 || h < 2) return;
    const small = document.createElement('canvas');
    small.width = Math.max(1, Math.round(w / block));
    small.height = Math.max(1, Math.round(h / block));
    const s = small.getContext('2d')!;
    s.imageSmoothingEnabled = true;
    s.drawImage(g.canvas, x, y, w, h, 0, 0, small.width, small.height);
    g.imageSmoothingEnabled = false;
    g.drawImage(small, 0, 0, small.width, small.height, x, y, w, h);
    g.imageSmoothingEnabled = true;
  }

  function paint(g: CanvasRenderingContext2D, op: Op) {
    g.lineCap = 'round';
    g.lineJoin = 'round';
    g.lineWidth = op.size;
    g.strokeStyle = op.color;
    g.fillStyle = op.color;
    if (op.t === 'pen') {
      g.beginPath();
      op.pts.forEach((p, i) => (i ? g.lineTo(p.x, p.y) : g.moveTo(p.x, p.y)));
      g.stroke();
    } else if (op.t === 'arrow') arrow(g, op.a, op.b, op.size);
    else if (op.t === 'rect') g.strokeRect(Math.min(op.a.x, op.b.x), Math.min(op.a.y, op.b.y), Math.abs(op.b.x - op.a.x), Math.abs(op.b.y - op.a.y));
    else if (op.t === 'blur') pixelate(g, op.a, op.b, op.size);
    else if (op.t === 'text') {
      g.font = `600 ${op.size}px "Segoe UI", system-ui, sans-serif`;
      g.textBaseline = 'top';
      g.fillText(op.text, op.at.x, op.at.y);
    }
  }

  function render() {
    const g = canvas?.getContext('2d');
    if (!g || !base) return;
    g.clearRect(0, 0, size.w, size.h);
    g.drawImage(base, 0, 0);
    for (const op of ops) paint(g, op);
    if (draft) paint(g, draft);
  }

  $effect(() => {
    ops;
    draft;
    size;
    render();
  });

  async function load() {
    const [opts, bytes] = await Promise.all([invoke<{ textSize: number; blur: number }>('capture_editor_opts'), invoke<ArrayBuffer>('capture_editor_image')]);
    textSize = opts.textSize;
    blur = opts.blur;
    base = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
    ops = [];
    redo = [];
    draft = null;
    typing = null;
    status = '';
    size = { w: base.width, h: base.height };
    if (canvas) {
      canvas.width = base.width;
      canvas.height = base.height;
    }
    render();
  }

  function at(e: PointerEvent): Pt {
    const r = canvas!.getBoundingClientRect();
    return { x: ((e.clientX - r.left) * size.w) / r.width, y: ((e.clientY - r.top) * size.h) / r.height };
  }

  function down(e: PointerEvent) {
    if (e.button !== 0 || !base) return;
    const p = at(e);
    if (tool === 'text') {
      commitText();
      typing = { at: p, value: '' };
      queueMicrotask(() => input?.focus());
      return;
    }
    canvas!.setPointerCapture(e.pointerId);
    const common = { color, size: tool === 'blur' ? blur : stroke };
    draft = tool === 'pen' ? { t: 'pen', pts: [p], ...common } : { t: tool, a: p, b: p, ...common };
  }

  function move(e: PointerEvent) {
    if (!draft) return;
    const p = at(e);
    if (draft.t === 'pen') draft = { ...draft, pts: [...draft.pts, p] };
    else if (draft.t !== 'text') draft = { ...draft, b: p };
  }

  function up() {
    if (!draft) return;
    ops = [...ops, draft];
    redo = [];
    draft = null;
  }

  function commitText() {
    if (typing && typing.value.trim()) {
      ops = [...ops, { t: 'text', at: typing.at, text: typing.value, color, size: textSize }];
      redo = [];
    }
    typing = null;
  }

  function undo() {
    if (!ops.length) return;
    redo = [...redo, ops[ops.length - 1]];
    ops = ops.slice(0, -1);
  }

  function redoOp() {
    if (!redo.length) return;
    ops = [...ops, redo[redo.length - 1]];
    redo = redo.slice(0, -1);
  }

  async function send(action: 'copy' | 'save' | 'shelf') {
    commitText();
    render();
    const blob: Blob | null = await new Promise((done) => canvas!.toBlob(done, 'image/png'));
    if (!blob) return;
    try {
      status = await invoke<string>('capture_editor_export', new Uint8Array(await blob.arrayBuffer()), { headers: { action } });
    } catch (err) {
      status = String(err);
    }
  }

  const close = () => invoke('capture_close_editor').catch(() => {});

  function onKey(e: KeyboardEvent) {
    if (typing) {
      if (e.key === 'Enter') commitText();
      else if (e.key === 'Escape') typing = null;
      return;
    }
    const mod = e.ctrlKey || e.metaKey;
    if (e.key === 'Escape') close();
    else if (mod && e.key.toLowerCase() === 'z') undo();
    else if (mod && e.key.toLowerCase() === 'y') redoOp();
    else if (mod && e.key.toLowerCase() === 'c') send('copy');
    else if (mod && e.key.toLowerCase() === 's') {
      e.preventDefault();
      send('save');
    } else if (!mod) {
      const hit = TOOLS.find((t) => t.label[0].toLowerCase() === e.key.toLowerCase());
      if (hit) tool = hit.id;
    }
  }

  onMount(() => {
    applyCssVars();
    load();
    const off = listen('capture://editor', () => load());
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
      off.then((f) => f());
    };
  });
</script>

<div class="editor">
  <div class="bar">
    {#each TOOLS as t (t.id)}
      <button class:on={tool === t.id} onclick={() => (tool = t.id)}>{t.label}</button>
    {/each}
    <span class="sep"></span>
    {#each COLORS as c (c)}
      <button class="swatch" class:on={color === c} style:background={c} onclick={() => (color = c)} aria-label="Colour {c}"></button>
    {/each}
    <span class="sep"></span>
    <button onclick={undo} disabled={!ops.length}>Undo</button>
    <button onclick={redoOp} disabled={!redo.length}>Redo</button>
    <span class="grow"></span>
    <span class="status">{status}</span>
    <button onclick={() => send('copy')}>Copy</button>
    <button onclick={() => send('shelf')}>Shelf</button>
    <button class="main" onclick={() => send('save')}>Save</button>
    <button onclick={close}>Close</button>
  </div>
  <div class="stage">
    <div class="frame">
      <canvas bind:this={canvas} onpointerdown={down} onpointermove={move} onpointerup={up}></canvas>
      {#if typing}
        <input
          bind:this={input}
          bind:value={typing.value}
          class="text"
          style:left="{(typing.at.x / size.w) * 100}%"
          style:top="{(typing.at.y / size.h) * 100}%"
          style:color
          style:font-size="{(textSize / size.w) * 100}cqw"
          onblur={commitText}
        />
      {/if}
    </div>
  </div>
</div>

<style>
  :global(body.capture-page) {
    background: var(--bg);
  }
  .editor {
    display: flex;
    flex-direction: column;
    height: 100%;
    color: var(--fg);
    font: 13px/1.4 var(--sans);
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
    padding: 8px 10px;
    background: var(--sidebar);
  }
  .bar button {
    padding: 6px 10px;
    border: 1px solid transparent;
    border-radius: 8px;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .bar button:hover:not(:disabled) {
    background: var(--hover);
  }
  .bar button:disabled {
    color: var(--mut);
    cursor: default;
  }
  .bar button.on {
    background: var(--acc-soft);
    color: var(--acc);
  }
  .bar button.main {
    background: var(--acc);
    color: var(--acc-fg);
  }
  .bar button.swatch {
    width: 22px;
    height: 22px;
    padding: 0;
    border: 1px solid var(--line-strong);
    border-radius: 50%;
  }
  .bar button.swatch.on {
    outline: 2px solid var(--acc);
    outline-offset: 2px;
  }
  .sep {
    width: 1px;
    height: 18px;
    margin: 0 4px;
    background: var(--line-strong);
  }
  .grow {
    flex: 1;
  }
  .status {
    color: var(--mut);
  }
  .stage {
    display: flex;
    flex: 1;
    align-items: center;
    justify-content: center;
    min-height: 0;
    padding: 16px;
    overflow: auto;
    background: var(--inset);
  }
  .frame {
    position: relative;
    display: flex;
    max-width: 100%;
    max-height: 100%;
    container-type: inline-size;
  }
  canvas {
    display: block;
    max-width: 100%;
    max-height: calc(100vh - 110px);
    object-fit: contain;
    cursor: crosshair;
    touch-action: none;
  }
  .text {
    position: absolute;
    min-width: 80px;
    padding: 0;
    border: 1px dashed var(--acc);
    background: transparent;
    font-family: 'Segoe UI', system-ui, sans-serif;
    font-weight: 600;
    line-height: 1;
    outline: none;
  }
</style>
