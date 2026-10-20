<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars } from '../lib/motion';

  type Tool = 'pen' | 'line' | 'arrow' | 'rect' | 'ellipse' | 'highlight' | 'spotlight' | 'step' | 'text' | 'blur' | 'crop';
  type BlurMode = 'pixelate' | 'blur' | 'erase';
  interface Pt {
    x: number;
    y: number;
  }
  interface Box {
    x: number;
    y: number;
    w: number;
    h: number;
  }
  interface Base {
    id: number;
    color: string;
    size: number;
  }
  type Op =
    | (Base & { t: 'pen' | 'highlight'; pts: Pt[] })
    | (Base & { t: 'line' | 'arrow' | 'rect' | 'ellipse' | 'spotlight' | 'crop'; a: Pt; b: Pt })
    | (Base & { t: 'blur'; a: Pt; b: Pt; mode: BlurMode; textOnly: boolean; boxes: Box[] | null })
    | (Base & { t: 'step'; at: Pt; n: number })
    | (Base & { t: 'text'; at: Pt; text: string });

  const TOOLS: { id: Tool; label: string; key: string }[] = [
    { id: 'pen', label: 'Pen', key: 'p' },
    { id: 'line', label: 'Line', key: 'l' },
    { id: 'arrow', label: 'Arrow', key: 'a' },
    { id: 'rect', label: 'Rectangle', key: 'r' },
    { id: 'ellipse', label: 'Ellipse', key: 'e' },
    { id: 'highlight', label: 'Highlighter', key: 'h' },
    { id: 'spotlight', label: 'Spotlight', key: 's' },
    { id: 'step', label: 'Step', key: 'n' },
    { id: 'text', label: 'Text', key: 't' },
    { id: 'blur', label: 'Blur', key: 'b' },
    { id: 'crop', label: 'Crop', key: 'c' },
  ];
  const WEIGHTS = [
    { id: 0.6, label: 'Thin' },
    { id: 1, label: 'Medium' },
    { id: 2, label: 'Thick' },
  ];
  const BLUR_MODES: { id: BlurMode; label: string }[] = [
    { id: 'pixelate', label: 'Pixelate' },
    { id: 'blur', label: 'Blur' },
    { id: 'erase', label: 'Cover' },
  ];
  const COLORS = ['#e5484d', '#f5a524', '#ffd60a', '#30a46c', '#0091ff', '#ffffff', '#111111'];
  const REMEMBER = 'notifi.editor.v1';

  let canvas = $state<HTMLCanvasElement>();
  let base: ImageBitmap | null = null;
  let size = $state({ w: 0, h: 0 });
  let tool = $state<Tool>('rect');
  let color = $state(COLORS[0]);
  let weight = $state(1);
  let blurMode = $state<BlurMode>('pixelate');
  let textOnly = $state(false);
  let textSize = $state(24);
  let blur = $state(12);
  let ops = $state<Op[]>([]);
  let redo = $state<Op[]>([]);
  let draft = $state<Op | null>(null);
  let typing = $state<{ at: Pt; value: string } | null>(null);
  let status = $state('');
  let input = $state<HTMLInputElement>();
  let nextId = 1;

  const stroke = $derived(Math.max(2, Math.round((size.w / 400) * weight)));
  const badges = $derived(ops.filter((o) => o.t === 'step').length);

  function restore() {
    try {
      const v = JSON.parse(localStorage.getItem(REMEMBER) ?? '{}');
      if (TOOLS.some((t) => t.id === v.tool)) tool = v.tool;
      if (typeof v.color === 'string' && /^#[0-9a-f]{6}$/i.test(v.color)) color = v.color;
      if (WEIGHTS.some((w) => w.id === v.weight)) weight = v.weight;
      if (BLUR_MODES.some((m) => m.id === v.blurMode)) blurMode = v.blurMode;
      if (typeof v.textOnly === 'boolean') textOnly = v.textOnly;
    } catch {
      /* nothing remembered */
    }
  }

  $effect(() => {
    const v = { tool, color, weight, blurMode, textOnly };
    try {
      localStorage.setItem(REMEMBER, JSON.stringify(v));
    } catch {
      /* the choices just are not kept */
    }
  });

  /** Whole-pixel box between two points, kept inside the picture. */
  function snap(a: Pt, b: Pt): Box {
    const x0 = Math.min(Math.max(Math.round(Math.min(a.x, b.x)), 0), size.w);
    const y0 = Math.min(Math.max(Math.round(Math.min(a.y, b.y)), 0), size.h);
    const x1 = Math.min(Math.max(Math.round(Math.max(a.x, b.x)), 0), size.w);
    const y1 = Math.min(Math.max(Math.round(Math.max(a.y, b.y)), 0), size.h);
    return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
  }

  function lastCrop(list: Op[]): Op | undefined {
    for (let i = list.length - 1; i >= 0; i--) if (list[i].t === 'crop') return list[i];
    return undefined;
  }

  /** Black or white, whichever reads on `bg`. */
  function ink(bg: string): string {
    const n = parseInt(bg.slice(1), 16);
    const lum = (0.299 * ((n >> 16) & 255) + 0.587 * ((n >> 8) & 255) + 0.114 * (n & 255)) / 255;
    return lum > 0.6 ? '#111111' : '#ffffff';
  }

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
  function pixelate(g: CanvasRenderingContext2D, r: Box, block: number) {
    const x = Math.max(0, Math.floor(r.x));
    const y = Math.max(0, Math.floor(r.y));
    const w = Math.min(size.w - x, Math.ceil(r.w));
    const h = Math.min(size.h - y, Math.ceil(r.h));
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

  function obscure(g: CanvasRenderingContext2D, r: Box, mode: BlurMode, strength: number, cover: string) {
    if (r.w < 2 || r.h < 2) return;
    if (mode === 'pixelate') pixelate(g, r, strength);
    else if (mode === 'erase') {
      g.fillStyle = cover;
      g.fillRect(r.x, r.y, r.w, r.h);
    } else {
      g.save();
      g.beginPath();
      g.rect(r.x, r.y, r.w, r.h);
      g.clip();
      g.filter = `blur(${Math.max(1, strength * 0.6)}px)`;
      g.drawImage(g.canvas, 0, 0);
      g.restore();
    }
  }

  function boxesOf(op: Op & { t: 'blur' }): Box[] {
    if (!op.textOnly) return [snap(op.a, op.b)];
    const pad = Math.max(2, op.size / 3);
    return (op.boxes ?? []).map((b) => ({ x: b.x - pad, y: b.y - pad, w: b.w + pad * 2, h: b.h + pad * 2 }));
  }

  /** Dims everything outside the given boxes. */
  function spotlight(g: CanvasRenderingContext2D, holes: Box[]) {
    const dim = document.createElement('canvas');
    dim.width = size.w;
    dim.height = size.h;
    const d = dim.getContext('2d')!;
    d.fillStyle = 'rgba(0, 0, 0, 0.55)';
    d.fillRect(0, 0, size.w, size.h);
    d.globalCompositeOperation = 'destination-out';
    for (const h of holes) d.fillRect(h.x, h.y, h.w, h.h);
    g.drawImage(dim, 0, 0);
  }

  function paint(g: CanvasRenderingContext2D, op: Op, exporting: boolean) {
    g.lineCap = 'round';
    g.lineJoin = 'round';
    g.lineWidth = op.size;
    g.strokeStyle = op.color;
    g.fillStyle = op.color;
    if (op.t === 'pen' || op.t === 'highlight') {
      if (op.t === 'highlight') {
        g.globalAlpha = 0.4;
        g.lineWidth = op.size * 5;
        g.lineCap = 'butt';
      }
      g.beginPath();
      op.pts.forEach((p, i) => (i ? g.lineTo(p.x, p.y) : g.moveTo(p.x, p.y)));
      g.stroke();
      g.globalAlpha = 1;
    } else if (op.t === 'line') {
      g.beginPath();
      g.moveTo(op.a.x, op.a.y);
      g.lineTo(op.b.x, op.b.y);
      g.stroke();
    } else if (op.t === 'arrow') arrow(g, op.a, op.b, op.size);
    else if (op.t === 'rect') g.strokeRect(Math.min(op.a.x, op.b.x), Math.min(op.a.y, op.b.y), Math.abs(op.b.x - op.a.x), Math.abs(op.b.y - op.a.y));
    else if (op.t === 'ellipse') {
      g.beginPath();
      g.ellipse((op.a.x + op.b.x) / 2, (op.a.y + op.b.y) / 2, Math.abs(op.b.x - op.a.x) / 2, Math.abs(op.b.y - op.a.y) / 2, 0, 0, Math.PI * 2);
      g.stroke();
    } else if (op.t === 'blur') for (const r of boxesOf(op)) obscure(g, r, op.mode, op.size, op.color);
    else if (op.t === 'step') {
      g.beginPath();
      g.arc(op.at.x, op.at.y, op.size, 0, Math.PI * 2);
      g.fill();
      g.fillStyle = ink(op.color);
      g.font = `700 ${Math.round(op.size * 1.15)}px "Segoe UI", system-ui, sans-serif`;
      g.textAlign = 'center';
      g.textBaseline = 'middle';
      g.fillText(String(op.n), op.at.x, op.at.y + op.size * 0.06);
      g.textAlign = 'start';
    } else if (op.t === 'text') {
      g.font = `600 ${op.size}px "Segoe UI", system-ui, sans-serif`;
      g.textBaseline = 'top';
      g.fillText(op.text, op.at.x, op.at.y);
    } else if (op.t === 'crop' && !exporting) {
      const c = snap(op.a, op.b);
      g.fillStyle = 'rgba(0, 0, 0, 0.6)';
      g.beginPath();
      g.rect(0, 0, size.w, size.h);
      g.rect(c.x, c.y, c.w, c.h);
      g.fill('evenodd');
      g.strokeStyle = '#ffffff';
      g.lineWidth = Math.max(1, size.w / 800);
      g.setLineDash([8, 6]);
      g.strokeRect(c.x, c.y, c.w, c.h);
      g.setLineDash([]);
    }
  }

  function render(exporting = false) {
    const g = canvas?.getContext('2d');
    if (!g || !base) return;
    g.clearRect(0, 0, size.w, size.h);
    g.drawImage(base, 0, 0);
    const all = draft ? [...ops, draft] : ops;
    const crop = lastCrop(all);
    for (const op of all) if (op.t !== 'spotlight' && (op.t !== 'crop' || op === crop)) paint(g, op, exporting);
    const holes = all.flatMap((o) => (o.t === 'spotlight' && 'a' in o ? [snap(o.a, o.b)] : []));
    if (holes.length) spotlight(g, holes);
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
    if (tool === 'step') {
      ops = [...ops, { id: nextId++, t: 'step', at: p, n: badges + 1, color, size: Math.max(12, stroke * 5) }];
      redo = [];
      return;
    }
    canvas!.setPointerCapture(e.pointerId);
    const id = nextId++;
    if (tool === 'pen' || tool === 'highlight') draft = { id, t: tool, pts: [p], color, size: stroke };
    else if (tool === 'blur') draft = { id, t: 'blur', a: p, b: p, color, size: blur, mode: blurMode, textOnly, boxes: null };
    else draft = { id, t: tool, a: p, b: p, color, size: stroke };
  }

  function move(e: PointerEvent) {
    if (!draft) return;
    const p = at(e);
    if (draft.t === 'pen' || draft.t === 'highlight') draft = { ...draft, pts: [...draft.pts, p] };
    else if ('a' in draft) draft = { ...draft, b: p } as Op;
  }

  /** Words inside a text-only blur are found after the box is drawn. */
  async function findText(op: Op & { t: 'blur' }) {
    const box = snap(op.a, op.b);
    const drop = () => (ops = ops.filter((o) => o.id !== op.id));
    try {
      const found = await invoke<number[][]>('capture_editor_text_boxes', { x: box.x, y: box.y, w: box.w, h: box.h });
      if (!found.length) {
        status = 'No text found in that area';
        drop();
        return;
      }
      const boxes = found.map(([x, y, w, h]) => ({ x, y, w, h }));
      ops = ops.map((o) => (o.id === op.id && o.t === 'blur' ? { ...o, boxes } : o));
    } catch (err) {
      status = String(err);
      drop();
    }
  }

  function up() {
    if (!draft) return;
    const done = draft;
    draft = null;
    if (done.t === 'crop' || done.t === 'spotlight' || done.t === 'blur') {
      const box = snap(done.a, done.b);
      if (box.w < 2 || box.h < 2) return;
    }
    ops = [...ops, done];
    redo = [];
    if (done.t === 'blur' && done.textOnly) findText(done);
  }

  function commitText() {
    if (typing && typing.value.trim()) {
      ops = [...ops, { id: nextId++, t: 'text', at: typing.at, text: typing.value, color, size: textSize }];
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

  async function send(action: 'copy' | 'save' | 'shelf' | 'pin') {
    commitText();
    render(true);
    const blob: Blob | null = await new Promise((done) => canvas!.toBlob(done, 'image/png'));
    render();
    if (!blob) return;
    const headers: Record<string, string> = { action };
    const crop = lastCrop(ops);
    if (crop && crop.t === 'crop') {
      const c = snap(crop.a, crop.b);
      headers.crop = `${c.x},${c.y},${c.w},${c.h}`;
    }
    try {
      status = await invoke<string>('capture_editor_export', new Uint8Array(await blob.arrayBuffer()), { headers });
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
    if ((e.target as HTMLElement | null)?.closest('select, input, textarea')) return;
    const mod = e.ctrlKey || e.metaKey;
    if (e.key === 'Escape') close();
    else if (mod && e.key.toLowerCase() === 'z') undo();
    else if (mod && e.key.toLowerCase() === 'y') redoOp();
    else if (mod && e.key.toLowerCase() === 'c') send('copy');
    else if (mod && e.key.toLowerCase() === 's') {
      e.preventDefault();
      send('save');
    } else if (!mod) {
      const hit = TOOLS.find((t) => t.key === e.key.toLowerCase());
      if (hit) tool = hit.id;
    }
  }

  onMount(() => {
    applyCssVars();
    restore();
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
      <button class:on={tool === t.id} onclick={() => (tool = t.id)} title="{t.label} ({t.key.toUpperCase()})">{t.label}</button>
    {/each}
    <span class="sep"></span>
    <button onclick={undo} disabled={!ops.length}>Undo</button>
    <button onclick={redoOp} disabled={!redo.length}>Redo</button>
    <span class="grow"></span>
    <span class="status">{status}</span>
    <button onclick={() => send('copy')}>Copy</button>
    <button onclick={() => send('shelf')}>Shelf</button>
    <button onclick={() => send('pin')}>Pin</button>
    <button class="main" onclick={() => send('save')}>Save</button>
    <button onclick={close}>Close</button>
  </div>
  <div class="bar tool-options">
    {#each COLORS as c (c)}
      <button class="swatch" class:on={color === c} style:background={c} onclick={() => (color = c)} aria-label="Colour {c}"></button>
    {/each}
    <input class="picker" type="color" bind:value={color} aria-label="Custom colour" />
    {#if tool !== 'blur' && tool !== 'crop' && tool !== 'spotlight' && tool !== 'text'}
      <span class="sep"></span>
      {#each WEIGHTS as w (w.id)}
        <button class:on={weight === w.id} onclick={() => (weight = w.id)}>{w.label}</button>
      {/each}
    {/if}
    {#if tool === 'text'}
      <span class="sep"></span>
      <label class="field">Size <input type="number" min="8" max="96" bind:value={textSize} /></label>
    {/if}
    {#if tool === 'blur'}
      <span class="sep"></span>
      {#each BLUR_MODES as m (m.id)}
        <button class:on={blurMode === m.id} onclick={() => (blurMode = m.id)}>{m.label}</button>
      {/each}
      <label class="field"><input type="checkbox" bind:checked={textOnly} /> Text only</label>
      <label class="field">Strength <input type="range" min="1" max="50" bind:value={blur} /></label>
    {/if}
    {#if tool === 'crop'}
      <span class="hint">Drag to choose the part to keep. It is cut when you copy, save or pin.</span>
    {:else if tool === 'step'}
      <span class="hint">Click to place number {badges + 1}.</span>
    {/if}
  </div>
  <div class="stage">
    <div class="frame" style:--ar={size.h ? size.w / size.h : 1}>
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
    width: min(100%, calc((100vh - 150px) * var(--ar, 1)));
    max-height: 100%;
    container-type: inline-size;
  }
  canvas {
    display: block;
    max-width: 100%;
    max-height: calc(100vh - 150px);
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
  .tool-options {
    padding-top: 0;
  }
  .tool-options .picker {
    width: 26px;
    height: 24px;
    padding: 0;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: transparent;
    cursor: pointer;
  }
  .field {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    padding: 0 6px;
    color: var(--mut);
  }
  .field input[type='number'] {
    width: 56px;
    padding: 3px 6px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: var(--inset);
    color: var(--fg);
    font: inherit;
  }
  .hint {
    color: var(--mut);
  }
</style>
