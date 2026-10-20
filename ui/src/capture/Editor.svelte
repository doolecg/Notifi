<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars } from '../lib/motion';
  import { bounds, eraseHits, fitImage, hasPad, innerBox, moved, noPad, onHandle, padded, resizedImage, toImage, topHit, type Pad } from './geometry';

  type Tool = 'select' | 'eraser' | 'pen' | 'line' | 'arrow' | 'rect' | 'ellipse' | 'highlight' | 'spotlight' | 'step' | 'text' | 'blur' | 'crop';
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
    | (Base & { t: 'text'; at: Pt; text: string })
    | (Base & { t: 'image'; a: Pt; w: number; h: number; opacity: number; bitmap: ImageBitmap });

  const TOOLS: { id: Tool; label: string; key: string }[] = [
    { id: 'select', label: 'Select', key: 'v' },
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
    { id: 'eraser', label: 'Eraser', key: 'x' },
  ];
  const SIDES: { k: keyof Pad; label: string }[] = [
    { k: 't', label: 'Top' },
    { k: 'r', label: 'Right' },
    { k: 'b', label: 'Bottom' },
    { k: 'l', label: 'Left' },
  ];
  const MAX_PAD = 2000;
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
  let work: HTMLCanvasElement | null = null;
  let tool = $state<Tool>('rect');
  let color = $state(COLORS[0]);
  let weight = $state(1);
  let blurMode = $state<BlurMode>('pixelate');
  let textOnly = $state(false);
  let textSize = $state(24);
  let blur = $state(12);
  let ops = $state<Op[]>([]);
  let past = $state<Op[][]>([]);
  let redo = $state<Op[][]>([]);
  let selected = $state<number | null>(null);
  let pad = $state<Pad>(noPad());
  let fill = $state<'solid' | 'gradient' | 'clear'>('solid');
  let bg = $state('#ffffff');
  let bg2 = $state('#0091ff');
  let radius = $state(0);
  let shadow = $state(false);
  let canvasOpen = $state(false);
  let shareOn = $state(false);
  let drag: { id: number; from: Pt; before: Op[]; mode: 'move' | 'resize'; changed: boolean } | null = null;
  let erasing: Op[] | null = null;
  let opacityFrom: Op[] | null = null;
  let draft = $state<Op | null>(null);
  let typing = $state<{ at: Pt; value: string } | null>(null);
  let status = $state('');
  let input = $state<HTMLInputElement>();
  let nextId = 1;

  const stroke = $derived(Math.max(2, Math.round((size.w / 400) * weight)));
  const badges = $derived(ops.filter((o) => o.t === 'step').length);
  const out = $derived(padded(size, pad));
  const selectedOp = $derived(ops.find((o) => o.id === selected));

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
    } else if (op.t === 'image') {
      g.globalAlpha = op.opacity;
      g.drawImage(op.bitmap, op.a.x, op.a.y, op.w, op.h);
      g.globalAlpha = 1;
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

  /** Dashed box round the selected item; image layers also get a square to resize from. */
  function outline(g: CanvasRenderingContext2D) {
    const s = selectedOp;
    if (!s) return;
    const b = bounds(s);
    g.save();
    g.strokeStyle = '#0091ff';
    g.fillStyle = '#0091ff';
    g.lineWidth = Math.max(1, size.w / 700);
    g.setLineDash([6, 4]);
    g.strokeRect(b.x - 4, b.y - 4, b.w + 8, b.h + 8);
    g.setLineDash([]);
    if (s.t === 'image') {
      const h = Math.max(8, size.w / 100);
      g.fillRect(b.x + b.w - h / 2, b.y + b.h - h / 2, h, h);
    }
    g.restore();
  }

  /** Puts the finished picture on the visible canvas, with the extra canvas, fill, corners and shadow. */
  function compose(exporting: boolean) {
    const g = canvas?.getContext('2d');
    if (!canvas || !g || !work) return;
    const crop = exporting ? lastCrop(ops) : undefined;
    const src = crop && crop.t === 'crop' ? snap(crop.a, crop.b) : { x: 0, y: 0, w: size.w, h: size.h };
    const dim = padded(src, pad);
    if (canvas.width !== dim.w || canvas.height !== dim.h) {
      canvas.width = dim.w;
      canvas.height = dim.h;
    }
    g.clearRect(0, 0, dim.w, dim.h);
    const dressed = hasPad(pad);
    if (dressed && fill !== 'clear') {
      if (fill === 'gradient') {
        const grad = g.createLinearGradient(0, 0, dim.w, dim.h);
        grad.addColorStop(0, bg);
        grad.addColorStop(1, bg2);
        g.fillStyle = grad;
      } else g.fillStyle = bg;
      g.fillRect(0, 0, dim.w, dim.h);
    }
    const to = innerBox(src, pad);
    const r = dressed ? Math.min(radius, src.w / 2, src.h / 2) : 0;
    if (dressed && shadow) {
      const room = Math.max(pad.t, pad.r, pad.b, pad.l);
      g.save();
      g.shadowColor = 'rgba(0, 0, 0, 0.4)';
      g.shadowBlur = room * 0.6;
      g.shadowOffsetY = room * 0.2;
      g.fillStyle = '#000000';
      g.beginPath();
      g.roundRect(to.x, to.y, to.w, to.h, r);
      g.fill();
      g.restore();
    }
    g.save();
    if (r > 0) {
      g.beginPath();
      g.roundRect(to.x, to.y, to.w, to.h, r);
      g.clip();
    }
    g.drawImage(work, src.x, src.y, src.w, src.h, to.x, to.y, to.w, to.h);
    g.restore();
  }

  /** Draws the capture and its marks on an off-screen canvas, then dresses it onto the visible one. */
  function render(exporting = false) {
    if (!canvas || !base) return;
    if (!work) work = document.createElement('canvas');
    if (work.width !== size.w || work.height !== size.h) {
      work.width = size.w;
      work.height = size.h;
    }
    const g = work.getContext('2d');
    if (!g) return;
    g.clearRect(0, 0, size.w, size.h);
    g.drawImage(base, 0, 0);
    const all = draft ? [...ops, draft] : ops;
    const crop = lastCrop(all);
    for (const op of all) if (op.t !== 'spotlight' && (op.t !== 'crop' || op === crop)) paint(g, op, exporting);
    const holes = all.flatMap((o) => (o.t === 'spotlight' && 'a' in o ? [snap(o.a, o.b)] : []));
    if (holes.length) spotlight(g, holes);
    if (!exporting) outline(g);
    compose(exporting);
  }

  $effect(() => {
    ops;
    draft;
    size;
    selected;
    pad;
    fill;
    bg;
    bg2;
    radius;
    shadow;
    render();
  });

  $effect(() => {
    if (tool !== 'select') selected = null;
  });

  async function load() {
    const [opts, bytes] = await Promise.all([invoke<{ textSize: number; blur: number; share: boolean }>('capture_editor_opts'), invoke<ArrayBuffer>('capture_editor_image')]);
    textSize = opts.textSize;
    blur = opts.blur;
    shareOn = opts.share;
    base = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
    ops = [];
    past = [];
    redo = [];
    selected = null;
    draft = null;
    typing = null;
    status = '';
    size = { w: base.width, h: base.height };
    render();
  }

  /** A pointer position as a point of the capture (the extra canvas is left of and above zero). */
  function at(e: { clientX: number; clientY: number }): Pt {
    const r = canvas!.getBoundingClientRect();
    return toImage({ x: ((e.clientX - r.left) * canvas!.width) / r.width, y: ((e.clientY - r.top) * canvas!.height) / r.height }, pad);
  }

  /** Slack for hitting a thin line, about six screen pixels. */
  const tol = () => (6 * canvas!.width) / canvas!.getBoundingClientRect().width;

  /** Records the current marks for undo and makes `next` the new marks. */
  function commit(next: Op[]) {
    past = [...past, ops];
    redo = [];
    ops = next;
  }

  function eraseAt(p: Pt) {
    const ids = eraseHits(ops, p, tol());
    if (!ids.length) return;
    ops = ops.filter((o) => !ids.includes(o.id));
  }

  function startDrag(e: PointerEvent, id: number, p: Pt, mode: 'move' | 'resize') {
    canvas!.setPointerCapture(e.pointerId);
    drag = { id, from: p, before: ops, mode, changed: false };
  }

  function pickAt(e: PointerEvent, p: Pt) {
    if (selectedOp && onHandle(selectedOp, p, tol() * 1.5)) return startDrag(e, selectedOp.id, p, 'resize');
    const i = topHit(ops, p, tol());
    selected = i < 0 ? null : ops[i].id;
    if (i >= 0) startDrag(e, ops[i].id, p, 'move');
  }

  function down(e: PointerEvent) {
    if (e.button !== 0 || !base) return;
    const p = at(e);
    if (tool === 'select') return pickAt(e, p);
    if (tool === 'eraser') {
      canvas!.setPointerCapture(e.pointerId);
      erasing = ops;
      eraseAt(p);
      return;
    }
    if (tool === 'text') {
      commitText();
      typing = { at: p, value: '' };
      queueMicrotask(() => input?.focus());
      return;
    }
    if (tool === 'step') {
      commit([...ops, { id: nextId++, t: 'step', at: p, n: badges + 1, color, size: Math.max(12, stroke * 5) }]);
      return;
    }
    canvas!.setPointerCapture(e.pointerId);
    const id = nextId++;
    if (tool === 'pen' || tool === 'highlight') draft = { id, t: tool, pts: [p], color, size: stroke };
    else if (tool === 'blur') draft = { id, t: 'blur', a: p, b: p, color, size: blur, mode: blurMode, textOnly, boxes: null };
    else draft = { id, t: tool, a: p, b: p, color, size: stroke };
  }

  function move(e: PointerEvent) {
    if (drag) {
      const d = drag;
      const p = at(e);
      if (d.mode === 'move') {
        const [dx, dy] = [p.x - d.from.x, p.y - d.from.y];
        ops = ops.map((o) => (o.id === d.id ? moved(o, dx, dy) : o));
        d.from = p;
      } else ops = ops.map((o) => (o.id === d.id ? resizedImage(o, p) : o));
      d.changed = true;
      return;
    }
    if (erasing) return eraseAt(at(e));
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
      const withBoxes = (list: Op[]) => list.map((o) => (o.id === op.id && o.t === 'blur' ? { ...o, boxes } : o));
      ops = withBoxes(ops);
      past = past.map(withBoxes);
      redo = redo.map(withBoxes);
    } catch (err) {
      status = String(err);
      drop();
    }
  }

  function up() {
    if (drag) {
      const d = drag;
      drag = null;
      if (d.changed) {
        past = [...past, d.before];
        redo = [];
      }
      return;
    }
    if (erasing) {
      const before = erasing;
      erasing = null;
      if (ops.length !== before.length) {
        past = [...past, before];
        redo = [];
      }
      return;
    }
    if (!draft) return;
    const done = draft;
    draft = null;
    if (done.t === 'crop' || done.t === 'spotlight' || done.t === 'blur') {
      const box = snap(done.a, done.b);
      if (box.w < 2 || box.h < 2) return;
    }
    commit([...ops, done]);
    if (done.t === 'blur' && done.textOnly) findText(done);
  }

  function commitText() {
    if (typing && typing.value.trim()) {
      commit([...ops, { id: nextId++, t: 'text', at: typing.at, text: typing.value, color, size: textSize }]);
    }
    typing = null;
  }

  function undo() {
    if (!past.length) return;
    redo = [...redo, ops];
    ops = past[past.length - 1];
    past = past.slice(0, -1);
    selected = null;
  }

  function redoOp() {
    if (!redo.length) return;
    past = [...past, ops];
    ops = redo[redo.length - 1];
    redo = redo.slice(0, -1);
    selected = null;
  }

  function removeSelected() {
    if (selected === null) return;
    commit(ops.filter((o) => o.id !== selected));
    selected = null;
  }

  function addImage(bitmap: ImageBitmap, centre?: Pt) {
    if (!base) return;
    const f = fitImage(bitmap.width, bitmap.height, size);
    const c = centre ?? { x: size.w / 2, y: size.h / 2 };
    const id = nextId++;
    commit([...ops, { id, t: 'image', bitmap, a: { x: Math.round(c.x - f.w / 2), y: Math.round(c.y - f.h / 2) }, w: f.w, h: f.h, opacity: 1, color: '#000000', size: 0 }]);
    tool = 'select';
    selected = id;
  }

  async function takeImage(file: File | undefined, centre?: Pt): Promise<boolean> {
    if (!file || !file.type.startsWith('image/')) return false;
    try {
      addImage(await createImageBitmap(file), centre);
    } catch {
      status = 'That image could not be opened';
    }
    return true;
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    takeImage(e.dataTransfer?.files[0], at(e));
  }

  function onPaste(e: ClipboardEvent) {
    const file = [...(e.clipboardData?.files ?? [])].find((f) => f.type.startsWith('image/'));
    if (!file) return;
    e.preventDefault();
    takeImage(file);
  }

  function setOpacity(v: number) {
    opacityFrom ??= ops;
    ops = ops.map((o) => (o.id === selected && o.t === 'image' ? { ...o, opacity: v } : o));
  }

  function endOpacity() {
    if (!opacityFrom) return;
    past = [...past, opacityFrom];
    redo = [];
    opacityFrom = null;
  }

  function setSide(k: keyof Pad, v: number) {
    pad = { ...pad, [k]: Math.max(0, Math.min(MAX_PAD, Math.round(v) || 0)) };
  }

  function setAllSides(v: number) {
    const n = Math.max(0, Math.min(MAX_PAD, Math.round(v) || 0));
    pad = { t: n, r: n, b: n, l: n };
  }

  function resetCanvas() {
    pad = noPad();
    fill = 'solid';
    radius = 0;
    shadow = false;
  }

  async function send(action: 'copy' | 'save' | 'shelf' | 'pin' | 'share') {
    commitText();
    render(true);
    const blob: Blob | null = await new Promise((done) => canvas!.toBlob(done, 'image/png'));
    render();
    if (!blob) return;
    const headers = { action };
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
    else if (e.key === 'Delete' || e.key === 'Backspace') removeSelected();
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
    window.addEventListener('paste', onPaste);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('paste', onPaste);
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
    <button onclick={undo} disabled={!past.length}>Undo</button>
    <button onclick={redoOp} disabled={!redo.length}>Redo</button>
    <button class:on={canvasOpen} onclick={() => (canvasOpen = !canvasOpen)}>Canvas</button>
    <span class="grow"></span>
    <span class="status">{status}</span>
    <button onclick={() => send('copy')}>Copy</button>
    <button onclick={() => send('shelf')}>Shelf</button>
    <button onclick={() => send('pin')}>Pin</button>
    {#if shareOn}<button onclick={() => send('share')}>Share link</button>{/if}
    <button class="main" onclick={() => send('save')}>Save</button>
    <button onclick={close}>Close</button>
  </div>
  <div class="bar tool-options">
    {#each COLORS as c (c)}
      <button class="swatch" class:on={color === c} style:background={c} onclick={() => (color = c)} aria-label="Colour {c}"></button>
    {/each}
    <input class="picker" type="color" bind:value={color} aria-label="Custom colour" />
    {#if tool !== 'blur' && tool !== 'crop' && tool !== 'spotlight' && tool !== 'text' && tool !== 'select' && tool !== 'eraser'}
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
    {#if selectedOp?.t === 'image'}
      <span class="sep"></span>
      <label class="field">Opacity <input type="range" min="10" max="100" value={Math.round(selectedOp.opacity * 100)} oninput={(e) => setOpacity(e.currentTarget.valueAsNumber / 100)} onchange={endOpacity} /></label>
    {/if}
    {#if tool === 'select'}
      <span class="hint">Click a mark to select it and drag to move it. Delete removes it. Drop or paste an image to add a layer; drag its corner square to resize.</span>
    {:else if tool === 'eraser'}
      <span class="hint">Drag over marks to remove them.</span>
    {:else if tool === 'crop'}
      <span class="hint">Drag to choose the part to keep. It is cut when you copy, save or pin.</span>
    {:else if tool === 'step'}
      <span class="hint">Click to place number {badges + 1}.</span>
    {/if}
  </div>
  {#if canvasOpen}
    <div class="bar tool-options">
      {#each SIDES as side (side.k)}
        <label class="field">{side.label} <input type="number" min="0" max={MAX_PAD} value={pad[side.k]} onchange={(e) => setSide(side.k, e.currentTarget.valueAsNumber)} /></label>
      {/each}
      <label class="field">All <input type="number" min="0" max={MAX_PAD} value={pad.t === pad.r && pad.r === pad.b && pad.b === pad.l ? pad.t : ''} onchange={(e) => setAllSides(e.currentTarget.valueAsNumber)} /></label>
      <span class="sep"></span>
      <select bind:value={fill} aria-label="Fill">
        <option value="solid">Solid</option>
        <option value="gradient">Gradient</option>
        <option value="clear">Transparent</option>
      </select>
      {#if fill !== 'clear'}<input class="picker" type="color" bind:value={bg} aria-label="Fill colour" />{/if}
      {#if fill === 'gradient'}<input class="picker" type="color" bind:value={bg2} aria-label="Second fill colour" />{/if}
      <label class="field">Corners <input type="number" min="0" max="200" bind:value={radius} /></label>
      <label class="field"><input type="checkbox" bind:checked={shadow} /> Shadow</label>
      <button onclick={resetCanvas}>Reset</button>
    </div>
  {/if}
  <div class="stage" role="presentation" ondragover={(e) => e.preventDefault()} ondrop={onDrop}>
    <div class="frame" style:--ar={out.h ? out.w / out.h : 1}>
      <canvas bind:this={canvas} class:pick={tool === 'select'} class:erase={tool === 'eraser'} onpointerdown={down} onpointermove={move} onpointerup={up}></canvas>
      {#if typing}
        <input
          bind:this={input}
          bind:value={typing.value}
          class="text"
          style:left="{((typing.at.x + pad.l) / out.w) * 100}%"
          style:top="{((typing.at.y + pad.t) / out.h) * 100}%"
          style:color
          style:font-size="{(textSize / out.w) * 100}cqw"
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
  canvas.pick {
    cursor: default;
  }
  canvas.erase {
    cursor: cell;
  }
  .bar select {
    padding: 4px 6px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: var(--inset);
    color: var(--fg);
    font: inherit;
  }
  .field input[type='range'] {
    width: 110px;
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
