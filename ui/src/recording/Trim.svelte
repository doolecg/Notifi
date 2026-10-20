<script lang="ts">
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars } from '../lib/motion';
  import {
    type Arrow,
    type Blur,
    type Effect,
    type EffectKind,
    type Segment,
    type Text,
    type Zoom,
    MIN_PART_MS,
    SPEEDS,
    arrowShape,
    clamp01,
    cutRange,
    fmt,
    hexToRgb,
    outputMs,
    partAt,
    splitAt,
    textPng,
    whole,
    zoomAt,
    zoomStyle,
  } from './edit';

  interface Target {
    path: string;
    name: string;
    kind: 'gif' | 'mp4';
    durationMs: number | null;
    audioTracks: number;
  }

  type Tool = 'cut' | 'speed' | 'zoom' | 'blur' | 'arrow' | 'text' | 'sound';
  const TOOLS: { id: Tool; label: string }[] = [
    { id: 'cut', label: 'Cut' },
    { id: 'speed', label: 'Speed' },
    { id: 'zoom', label: 'Zoom' },
    { id: 'blur', label: 'Blur' },
    { id: 'arrow', label: 'Arrow' },
    { id: 'text', label: 'Text' },
    { id: 'sound', label: 'Sound' },
  ];
  const COLORS = ['#ffffff', '#ffd640', '#ff5a4f', '#40a0ff', '#38d9a9', '#000000'];
  const STRIP = 14;

  let target = $state<Target | null>(null);
  let video = $state<HTMLVideoElement>();
  let src = $state('');
  let duration = $state(0);
  let vw = $state(16);
  let vh = $state(9);
  let now = $state(0);
  let playing = $state(false);
  let busy = $state(false);
  let message = $state('');
  let saved = $state('');
  let tool = $state<Tool>('cut');

  let segments = $state<Segment[]>([]);
  let zooms = $state<Zoom[]>([]);
  let blurs = $state<Blur[]>([]);
  let arrows = $state<Arrow[]>([]);
  let texts = $state<Text[]>([]);
  let volumes = $state<number[]>([]);
  let selected = $state<{ kind: EffectKind; id: number } | null>(null);
  let selA = $state<number | null>(null);
  let selB = $state<number | null>(null);
  let previewZoom = $state(false);
  let filmstrip = $state<string[]>([]);
  let nextId = 1;
  let raf = 0;

  // GIF files only get a start and an end.
  let gifStart = $state(0);
  let gifEnd = $state(0);

  // Settings of the next thing drawn.
  let color = $state('#ff5a4f');
  let pixelate = $state(false);
  let words = $state('Text');
  let size = $state(0.06);
  let draft = $state<{ kind: 'blur' | 'arrow'; x1: number; y1: number; x2: number; y2: number } | null>(null);

  const pct = (ms: number) => (duration ? (ms / duration) * 100 : 0);
  const kept = $derived(outputMs(segments));
  const zoomNow = $derived(zoomAt(zooms, now));
  const zoomApplied = $derived((playing || previewZoom) && zoomNow !== null);
  const hasEffects = $derived(zooms.length + blurs.length + arrows.length + texts.length > 0);
  const edited = $derived(
    segments.length !== 1 || segments[0].startMs !== 0 || segments[0].endMs !== duration || segments.some((s) => s.speed !== 1) || hasEffects || volumes.some((v) => v !== 1),
  );
  const all = $derived<{ kind: EffectKind; item: Effect }[]>([
    ...zooms.map((item) => ({ kind: 'zoom' as const, item })),
    ...blurs.map((item) => ({ kind: 'blur' as const, item })),
    ...arrows.map((item) => ({ kind: 'arrow' as const, item })),
    ...texts.map((item) => ({ kind: 'text' as const, item })),
  ]);
  const current = $derived(all.find((e) => e.kind === selected?.kind && e.item.id === selected.id)?.item ?? null);
  const selection = $derived(selA !== null && selB !== null && Math.abs(selB - selA) >= MIN_PART_MS ? ([Math.min(selA, selB), Math.max(selA, selB)] as const) : null);

  function reset(ms: number) {
    duration = ms;
    segments = whole(ms);
    zooms = [];
    blurs = [];
    arrows = [];
    texts = [];
    selected = null;
    selA = selB = null;
    now = 0;
    gifStart = 0;
    gifEnd = ms;
  }

  async function load() {
    message = '';
    saved = '';
    filmstrip = [];
    try {
      target = await invoke<Target>('recording_trim_target');
    } catch (err) {
      target = null;
      message = String(err);
      return;
    }
    volumes = Array.from({ length: target.audioTracks }, () => 1);
    reset(0);
    if (target.kind === 'gif') {
      src = '';
      reset(target.durationMs ?? 0);
    } else {
      src = convertFileSrc(`video-${Date.now()}`, 'capturemedia');
    }
  }

  function loaded() {
    if (!video || !Number.isFinite(video.duration)) return;
    vw = video.videoWidth || 16;
    vh = video.videoHeight || 9;
    reset(Math.round(video.duration * 1000));
    makeFilmstrip(src);
  }

  // A row of small pictures along the timeline, taken from a second video element.
  async function makeFilmstrip(url: string) {
    const clip = document.createElement('video');
    clip.muted = true;
    clip.preload = 'auto';
    clip.src = url;
    const canvas = document.createElement('canvas');
    canvas.width = 96;
    canvas.height = Math.max(1, Math.round((96 * vh) / vw));
    const ctx = canvas.getContext('2d');
    const out: string[] = [];
    try {
      await new Promise<void>((ok, bad) => {
        clip.onloadeddata = () => ok();
        clip.onerror = () => bad();
      });
      for (let i = 0; i < STRIP && ctx; i++) {
        if (url !== src) return;
        await new Promise<void>((ok) => {
          clip.onseeked = () => ok();
          clip.currentTime = ((i + 0.5) / STRIP) * (duration / 1000);
        });
        ctx.drawImage(clip, 0, 0, canvas.width, canvas.height);
        out.push(canvas.toDataURL('image/jpeg', 0.6));
      }
      filmstrip = out;
    } catch {
      filmstrip = [];
    } finally {
      clip.removeAttribute('src');
      clip.load();
    }
  }

  // ---- Playback: skips what is cut and plays each part at its speed ----

  function follow() {
    if (!video) return;
    const t = video.currentTime * 1000;
    now = t;
    const i = partAt(segments, t);
    if (i >= 0) {
      if (video.playbackRate !== segments[i].speed) video.playbackRate = segments[i].speed;
    } else {
      const next = segments.find((s) => s.startMs > t);
      if (next) video.currentTime = next.startMs / 1000;
      else video.pause();
    }
    if (!video.paused) raf = requestAnimationFrame(follow);
  }

  function onplay() {
    playing = true;
    cancelAnimationFrame(raf);
    raf = requestAnimationFrame(follow);
  }

  function onpause() {
    playing = false;
    cancelAnimationFrame(raf);
    if (video) now = video.currentTime * 1000;
  }

  const sync = () => {
    if (video) now = video.currentTime * 1000;
  };

  function seek(ms: number) {
    if (!video) return;
    video.currentTime = Math.min(Math.max(ms, 0), duration) / 1000;
    now = video.currentTime * 1000;
  }

  function playKept() {
    if (!video || !segments.length) return;
    seek(segments[0].startMs);
    video.play().catch(() => {});
  }

  const toggle = () => (video?.paused ? video.play().catch(() => {}) : video?.pause());

  // ---- Timeline ----

  let track = $state<HTMLDivElement>();
  let dragging = false;
  let moved = false;

  const timeAt = (e: PointerEvent) => {
    const r = track!.getBoundingClientRect();
    return Math.min(Math.max(((e.clientX - r.left) / r.width) * duration, 0), duration);
  };

  function down(e: PointerEvent) {
    if (!duration || busy) return;
    track?.setPointerCapture(e.pointerId);
    dragging = true;
    moved = false;
    selA = timeAt(e);
    selB = selA;
  }

  function drag(e: PointerEvent) {
    if (!dragging) return;
    const t = timeAt(e);
    if (Math.abs(t - (selA ?? t)) > duration / 300) moved = true;
    selB = t;
  }

  function up(e: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    if (!moved) {
      seek(timeAt(e));
      selA = selB = null;
    }
  }

  // ---- Cuts and speed ----

  const apply = (next: Segment[]) => {
    if (next.length) segments = next;
  };

  function cutSelection() {
    if (!selection) return;
    apply(cutRange(segments, selection[0], selection[1]));
    selA = selB = null;
  }

  const startHere = () => apply(cutRange(segments, 0, now));
  const endHere = () => apply(cutRange(segments, now, duration));
  const split = () => (segments = splitAt(segments, now));
  const removePart = (i: number) => apply(segments.filter((_, k) => k !== i));
  const setSpeed = (i: number, speed: number) => (segments = segments.map((s, k) => (k === i ? { ...s, speed } : s)));
  const setAllSpeed = (speed: number) => (segments = segments.map((s) => ({ ...s, speed })));
  const startOver = () => {
    segments = whole(duration);
    selA = selB = null;
  };

  // ---- Effects ----

  const span = (ms = 3000) => ({ startMs: Math.round(now), endMs: Math.round(Math.min(duration, now + ms)) });
  const room = () => duration - now >= MIN_PART_MS;

  function pick(kind: EffectKind, id: number) {
    selected = { kind, id };
    tool = kind;
    seek(Math.round(all.find((e) => e.kind === kind && e.item.id === id)!.item.startMs));
  }

  function addZoom() {
    if (!room()) return;
    const z: Zoom = { id: nextId++, ...span(2500), x: 0.5, y: 0.5, level: 2 };
    zooms = [...zooms, z];
    selected = { kind: 'zoom', id: z.id };
  }

  function change<T extends Effect>(list: T[], id: number, patch: Partial<T>): T[] {
    return list.map((e) => (e.id === id ? { ...e, ...patch } : e));
  }

  function patch(p: Partial<Zoom & Blur & Arrow & Text>) {
    if (!selected) return;
    const id = selected.id;
    if (selected.kind === 'zoom') zooms = change(zooms, id, p as Partial<Zoom>);
    else if (selected.kind === 'blur') blurs = change(blurs, id, p as Partial<Blur>);
    else if (selected.kind === 'arrow') arrows = change(arrows, id, p as Partial<Arrow>);
    else texts = change(texts, id, p as Partial<Text>);
  }

  function setTime(which: 'startMs' | 'endMs', ms: number) {
    if (!current || !Number.isFinite(ms)) return;
    const v = Math.round(Math.min(Math.max(ms, 0), duration));
    if (which === 'startMs') patch({ startMs: Math.min(v, current.endMs - MIN_PART_MS / 2) });
    else patch({ endMs: Math.max(v, current.startMs + MIN_PART_MS / 2) });
  }

  function removeEffect() {
    if (!selected) return;
    const id = selected.id;
    zooms = zooms.filter((e) => e.id !== id || selected?.kind !== 'zoom');
    blurs = blurs.filter((e) => e.id !== id || selected?.kind !== 'blur');
    arrows = arrows.filter((e) => e.id !== id || selected?.kind !== 'arrow');
    texts = texts.filter((e) => e.id !== id || selected?.kind !== 'text');
    selected = null;
  }

  // ---- Drawing on the picture ----

  let frame = $state<HTMLDivElement>();

  const point = (e: PointerEvent) => {
    const r = frame!.getBoundingClientRect();
    return { x: clamp01((e.clientX - r.left) / r.width), y: clamp01((e.clientY - r.top) / r.height) };
  };

  function stageDown(e: PointerEvent) {
    if (!frame || busy || zoomApplied) return;
    const p = point(e);
    if (tool === 'zoom' && selected?.kind === 'zoom') {
      patch({ x: p.x, y: p.y });
    } else if (!room()) {
      return;
    } else if (tool === 'text') {
      const t: Text = { id: nextId++, ...span(), x: p.x, y: p.y, text: words.trim() || 'Text', size, color };
      texts = [...texts, t];
      selected = { kind: 'text', id: t.id };
    } else if (tool === 'blur' || tool === 'arrow') {
      frame.setPointerCapture(e.pointerId);
      draft = { kind: tool, x1: p.x, y1: p.y, x2: p.x, y2: p.y };
    }
  }

  function stageMove(e: PointerEvent) {
    if (!draft) return;
    const p = point(e);
    draft = { ...draft, x2: p.x, y2: p.y };
  }

  function stageUp() {
    const d = draft;
    draft = null;
    if (!d) return;
    if (d.kind === 'blur') {
      const [x, y, w, h] = [Math.min(d.x1, d.x2), Math.min(d.y1, d.y2), Math.abs(d.x2 - d.x1), Math.abs(d.y2 - d.y1)];
      if (w < 0.02 || h < 0.02) return;
      const b: Blur = { id: nextId++, ...span(), x, y, w, h, pixelate };
      blurs = [...blurs, b];
      selected = { kind: 'blur', id: b.id };
    } else {
      if (Math.hypot((d.x2 - d.x1) * vw, (d.y2 - d.y1) * vh) < 12) return;
      const a: Arrow = { id: nextId++, ...span(), x1: d.x1, y1: d.y1, x2: d.x2, y2: d.y2, color, width: 0.008 };
      arrows = [...arrows, a];
      selected = { kind: 'arrow', id: a.id };
    }
  }

  const active = (e: { startMs: number; endMs: number }) => now >= e.startMs && now < e.endMs;

  // ---- Saving ----

  async function save() {
    if (!target || busy) return;
    busy = true;
    saved = '';
    message = 'Saving the edited copy. This can take a while for long videos.';
    try {
      if (target.kind === 'gif') {
        saved = await invoke<string>('recording_trim_save', { startMs: gifStart, endMs: gifEnd, durationMs: duration });
      } else {
        const labels = await Promise.all(
          texts.map(async (t) => ({ startMs: t.startMs, endMs: t.endMs, x: t.x, y: t.y, png: await textPng(t, vh) })),
        );
        saved = await invoke<string>('recording_edit_save', {
          spec: {
            durationMs: duration,
            segments,
            zooms: zooms.map(({ id: _, ...z }) => z),
            blurs: blurs.map(({ id: _, ...b }) => b),
            arrows: arrows.map(({ id: _, color: c, ...a }) => ({ ...a, color: hexToRgb(c) })),
            labels,
            volumes,
          },
        });
      }
      message = '';
    } catch (err) {
      message = String(err);
    }
    busy = false;
  }

  const reveal = () => invoke('recording_trim_reveal', { path: saved }).catch(() => {});
  const close = () => invoke('recording_trim_close').catch(() => {});

  onMount(() => {
    applyCssVars();
    load();
    const off = listen('recording://trim', () => load());
    return () => {
      cancelAnimationFrame(raf);
      off.then((f) => f());
    };
  });
</script>

<div class="edit">
  <header>
    <h1>Edit recording</h1>
    <span class="name" title={target?.path}>{target?.name ?? ''}</span>
  </header>

  {#if target?.kind === 'gif'}
    <div class="stage"><p class="note">GIF files have no preview here. Choose the start and end with the sliders.</p></div>
    <div class="range">
      <label>
        <span>Start</span>
        <input type="range" min="0" max={duration} step="10" value={gifStart} oninput={(e) => (gifStart = Math.min(+e.currentTarget.value, gifEnd - 100))} disabled={!duration || busy} />
        <b>{fmt(gifStart)}</b>
      </label>
      <label>
        <span>End</span>
        <input type="range" min="0" max={duration} step="10" value={gifEnd} oninput={(e) => (gifEnd = Math.max(+e.currentTarget.value, gifStart + 100))} disabled={!duration || busy} />
        <b>{fmt(gifEnd)}</b>
      </label>
      <p class="keep">Keeps {fmt(gifEnd - gifStart)} of {fmt(duration)}</p>
    </div>
  {:else if target}
    <div class="stage">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="frame"
        class:draw={tool === 'blur' || tool === 'arrow' || tool === 'text' || (tool === 'zoom' && selected?.kind === 'zoom')}
        style:aspect-ratio="{vw} / {vh}"
        style:--ratio={vw / vh}
        bind:this={frame}
        onpointerdown={stageDown}
        onpointermove={stageMove}
        onpointerup={stageUp}
      >
        <div class="inner" style={zoomApplied && zoomNow ? zoomStyle(zoomNow.level, zoomNow.x, zoomNow.y) : ''}>
          <!-- svelte-ignore a11y_media_has_caption -->
          <video bind:this={video} {src} preload="auto" onloadedmetadata={loaded} {onplay} {onpause} onseeked={sync} onended={onpause}></video>
          {#each blurs as b (b.id)}
            {#if active(b)}
              <div class="blur" class:chosen={selected?.id === b.id} style:left="{b.x * 100}%" style:top="{b.y * 100}%" style:width="{b.w * 100}%" style:height="{b.h * 100}%"></div>
            {/if}
          {/each}
          <svg viewBox="0 0 {vw} {vh}" preserveAspectRatio="none">
            {#each arrows as a (a.id)}
              {#if active(a)}
                {@const g = arrowShape(a, vw, vh)}
                <line x1={a.x1 * vw} y1={a.y1 * vh} x2={g.shaftEnd[0]} y2={g.shaftEnd[1]} stroke={a.color} stroke-width={a.width * vh} stroke-linecap="round" />
                <polygon points={g.head} fill={a.color} />
              {/if}
            {/each}
            {#if draft?.kind === 'arrow'}
              <line x1={draft.x1 * vw} y1={draft.y1 * vh} x2={draft.x2 * vw} y2={draft.y2 * vh} stroke={color} stroke-width={0.008 * vh} stroke-linecap="round" />
            {/if}
          </svg>
          {#each texts as t (t.id)}
            {#if active(t)}
              <span class="label" class:chosen={selected?.id === t.id} style:left="{t.x * 100}%" style:top="{t.y * 100}%" style:font-size="{t.size * 100}cqh" style:color={t.color}>{t.text}</span>
            {/if}
          {/each}
          {#if draft?.kind === 'blur'}
            <div class="blur draft" style:left="{Math.min(draft.x1, draft.x2) * 100}%" style:top="{Math.min(draft.y1, draft.y2) * 100}%" style:width="{Math.abs(draft.x2 - draft.x1) * 100}%" style:height="{Math.abs(draft.y2 - draft.y1) * 100}%"></div>
          {/if}
          {#if tool === 'zoom' && selected?.kind === 'zoom' && current && !zoomApplied}
            {@const z = current as Zoom}
            <span class="focus" style:left="{z.x * 100}%" style:top="{z.y * 100}%"></span>
          {/if}
        </div>
      </div>
    </div>

    <div class="transport">
      <button onclick={toggle} disabled={!duration}>{playing ? 'Pause' : 'Play'}</button>
      <button onclick={playKept} disabled={!duration}>Play from the start</button>
      <span class="clock">{fmt(now)} / {fmt(duration)}</span>
      <label class="check"><input type="checkbox" bind:checked={previewZoom} /> Show zoom while paused</label>
    </div>

    <div class="timeline" bind:this={track} onpointerdown={down} onpointermove={drag} onpointerup={up} role="slider" aria-label="Timeline" aria-valuenow={now} tabindex="0">
      <div class="strip">
        {#each filmstrip as shot}<img src={shot} alt="" draggable="false" />{/each}
      </div>
      {#each segments as s}
        <div class="part" style:left="{pct(s.startMs)}%" style:width="{pct(s.endMs - s.startMs)}%">{#if s.speed !== 1}{s.speed}x{/if}</div>
      {/each}
      {#if selection}<div class="select" style:left="{pct(selection[0])}%" style:width="{pct(selection[1] - selection[0])}%"></div>{/if}
      <div class="head" style:left="{pct(now)}%"></div>
    </div>
    <div class="lanes">
      {#each all as e (e.kind + e.item.id)}
        <button class="lane {e.kind}" class:chosen={selected?.id === e.item.id && selected.kind === e.kind} style:left="{pct(e.item.startMs)}%" style:width="{Math.max(pct(e.item.endMs - e.item.startMs), 0.8)}%" title={e.kind} aria-label={e.kind} onclick={() => pick(e.kind, e.item.id)}></button>
      {/each}
    </div>

    <nav class="tools">
      {#each TOOLS as t}
        {#if t.id !== 'sound' || target.audioTracks > 0}
          <button class:on={tool === t.id} onclick={() => (tool = t.id)}>{t.label}</button>
        {/if}
      {/each}
    </nav>

    <section class="panel">
      {#if tool === 'cut'}
        <div class="row">
          <button onclick={cutSelection} disabled={!selection || busy}>Cut selection{selection ? ` (${fmt(selection[1] - selection[0])})` : ''}</button>
          <button onclick={split} disabled={busy}>Split here</button>
          <button onclick={startHere} disabled={busy}>Start here</button>
          <button onclick={endHere} disabled={busy}>End here</button>
          <button onclick={startOver} disabled={busy || !edited}>Keep everything</button>
        </div>
        <p class="hint">Drag across the timeline to select a part, then cut it out. Click the timeline to move the playhead.</p>
        <ul class="parts">
          {#each segments as s, i}
            <li>
              <span>{fmt(s.startMs)} to {fmt(s.endMs)}</span>
              <button onclick={() => removePart(i)} disabled={segments.length < 2 || busy}>Remove</button>
            </li>
          {/each}
        </ul>
      {:else if tool === 'speed'}
        {@const i = partAt(segments, now)}
        <div class="row">
          <label>Part at the playhead
            <select value={i >= 0 ? segments[i].speed : 1} disabled={i < 0 || busy} onchange={(e) => setSpeed(i, +e.currentTarget.value)}>
              {#each SPEEDS as v}<option value={v}>{v}x</option>{/each}
            </select>
          </label>
          <label>Every part
            <select value="" onchange={(e) => e.currentTarget.value && setAllSpeed(+e.currentTarget.value)} disabled={busy}>
              <option value="">Set all to...</option>
              {#each SPEEDS as v}<option value={v}>{v}x</option>{/each}
            </select>
          </label>
        </div>
        <p class="hint">0.25x to 4x. The sound keeps its pitch. Split the video first to speed up only part of it.</p>
      {:else if tool === 'zoom'}
        <div class="row"><button onclick={addZoom} disabled={busy || !room()}>Add zoom here</button></div>
        {#if selected?.kind === 'zoom' && current}
          {@const z = current as Zoom}
          <label class="slide">Zoom <input type="range" min="1.2" max="5" step="0.1" value={z.level} oninput={(e) => patch({ level: +e.currentTarget.value })} /><b>{z.level.toFixed(1)}x</b></label>
          <p class="hint">Click the picture to choose what the zoom centres on. Turn on Show zoom while paused to see the result.</p>
        {:else}<p class="hint">Add a zoom, then pick its centre on the picture.</p>{/if}
      {:else if tool === 'blur'}
        <div class="row">
          <label class="check"><input type="checkbox" bind:checked={pixelate} /> Pixelate instead of blur</label>
        </div>
        <p class="hint">Drag on the picture to draw the area. It lasts three seconds from the playhead; change that below.</p>
      {:else if tool === 'arrow'}
        <div class="row colors">
          {#each COLORS as c}<button class="swatch" class:on={color === c} style:background={c} aria-label={c} onclick={() => { color = c; if (selected?.kind === 'arrow') patch({ color: c }); }}></button>{/each}
        </div>
        <p class="hint">Drag on the picture from the tail to the point.</p>
      {:else if tool === 'text'}
        <div class="row">
          <input class="words" type="text" bind:value={words} maxlength="80" oninput={() => selected?.kind === 'text' && patch({ text: words })} />
          <select bind:value={size} onchange={() => selected?.kind === 'text' && patch({ size })}>
            <option value={0.04}>Small</option>
            <option value={0.06}>Medium</option>
            <option value={0.09}>Large</option>
          </select>
        </div>
        <div class="row colors">
          {#each COLORS as c}<button class="swatch" class:on={color === c} style:background={c} aria-label={c} onclick={() => { color = c; if (selected?.kind === 'text') patch({ color: c }); }}></button>{/each}
        </div>
        <p class="hint">Click the picture to put the text there.</p>
      {:else if tool === 'sound'}
        {#each volumes as v, i}
          <label class="slide">
            {target.audioTracks === 2 ? (i === 0 ? 'System sound' : 'Microphone') : 'Sound'}
            <input type="range" min="0" max="2" step="0.05" value={v} oninput={(e) => (volumes[i] = +e.currentTarget.value)} />
            <b>{v === 0 ? 'Removed' : `${Math.round(v * 100)}%`}</b>
          </label>
        {/each}
        <p class="hint">Applies to the saved copy. A track set to 0 is left out of it.</p>
      {/if}

      {#if current && selected && selected.kind === tool}
        <div class="row when">
          <label>From <input type="number" min="0" step="0.1" value={(current.startMs / 1000).toFixed(2)} onchange={(e) => setTime('startMs', +e.currentTarget.value * 1000)} /> s</label>
          <button onclick={() => setTime('startMs', now)}>Playhead</button>
          <label>To <input type="number" min="0" step="0.1" value={(current.endMs / 1000).toFixed(2)} onchange={(e) => setTime('endMs', +e.currentTarget.value * 1000)} /> s</label>
          <button onclick={() => setTime('endMs', now)}>Playhead</button>
          <span class="grow"></span>
          <button onclick={removeEffect}>Delete</button>
        </div>
      {/if}
    </section>
  {/if}

  {#if target}
    <div class="actions">
      {#if target.kind === 'mp4'}<span class="keep">Saved copy: {fmt(kept)}</span>{/if}
      <span class="grow"></span>
      <button onclick={close}>Close</button>
      <button class="main" onclick={save} disabled={!duration || busy || (target.kind === 'gif' ? gifEnd - gifStart < 100 : !edited)}>
        {busy ? 'Saving...' : target.kind === 'gif' ? 'Save trimmed copy' : 'Save edited copy'}
      </button>
    </div>
  {/if}

  {#if saved}
    <p class="done">Saved <span title={saved}>{saved.split(/[\\/]/).pop()}</span>. The original is unchanged. <button onclick={reveal}>Show in folder</button></p>
  {/if}
  {#if message}<p class="msg">{message}</p>{/if}
</div>

<style>
  :global(html),
  :global(body.recording-page) {
    background: var(--bg);
    overflow: auto;
    user-select: none;
  }
  .edit {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 100%;
    padding: 14px;
    box-sizing: border-box;
    color: var(--fg);
    font: 13px/1.4 var(--sans);
  }
  header {
    display: flex;
    gap: 10px;
    align-items: baseline;
  }
  h1 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .name {
    min-width: 0;
    overflow: hidden;
    color: var(--mut);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .stage {
    display: grid;
    place-items: center;
    min-height: 120px;
    overflow: hidden;
    border-radius: 10px;
    background: var(--inset);
  }
  .frame {
    position: relative;
    container-type: size;
    width: 100%;
    max-width: calc(46vh * var(--ratio, 1.7778));
    max-height: 46vh;
    overflow: hidden;
    background: #000;
  }
  .frame.draw {
    cursor: crosshair;
  }
  .inner {
    position: absolute;
    inset: 0;
    transform-origin: 0 0;
  }
  video {
    display: block;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }
  svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }
  .blur {
    position: absolute;
    backdrop-filter: blur(12px);
    outline: 1px dashed rgb(255 255 255 / 0.6);
    pointer-events: none;
  }
  .blur.chosen,
  .label.chosen {
    outline: 1px solid var(--acc);
  }
  .blur.draft {
    outline-color: var(--acc);
  }
  .label {
    position: absolute;
    padding: 0.15em 0.15em 0.45em;
    font-weight: 600;
    line-height: 1;
    white-space: nowrap;
    paint-order: stroke fill;
    -webkit-text-stroke: 0.12em rgb(0 0 0 / 0.6);
    pointer-events: none;
  }
  .focus {
    position: absolute;
    width: 14px;
    height: 14px;
    margin: -7px 0 0 -7px;
    border: 2px solid var(--acc);
    border-radius: 50%;
    pointer-events: none;
  }
  .note,
  .msg {
    margin: 0;
    padding: 10px;
    color: var(--mut);
  }
  .transport,
  .row,
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .clock {
    color: var(--mut);
    font-variant-numeric: tabular-nums;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-left: auto;
    color: var(--mut);
  }
  .row .check {
    margin-left: 0;
  }
  .timeline {
    position: relative;
    height: 54px;
    border-radius: 8px;
    background: var(--inset);
    overflow: hidden;
    cursor: pointer;
    touch-action: none;
  }
  .strip {
    position: absolute;
    inset: 0;
    display: flex;
    opacity: 0.55;
  }
  .strip img {
    flex: 1;
    min-width: 0;
    height: 100%;
    object-fit: cover;
    pointer-events: none;
  }
  .part {
    position: absolute;
    top: 0;
    bottom: 0;
    box-sizing: border-box;
    padding: 2px 4px;
    border: 2px solid var(--acc);
    border-radius: 6px;
    background: var(--acc-soft);
    color: var(--fg);
    font-size: 11px;
    pointer-events: none;
  }
  .select {
    position: absolute;
    top: 0;
    bottom: 0;
    background: rgb(255 90 79 / 0.35);
    pointer-events: none;
  }
  .head {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--fg);
    pointer-events: none;
  }
  .lanes {
    position: relative;
    height: 12px;
  }
  .lane {
    position: absolute;
    top: 2px;
    height: 8px;
    min-width: 6px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: var(--mut);
  }
  .lane.zoom {
    background: #40a0ff;
  }
  .lane.blur {
    background: #38d9a9;
  }
  .lane.arrow {
    background: #ff5a4f;
  }
  .lane.text {
    background: #ffd640;
  }
  .lane.chosen {
    outline: 2px solid var(--fg);
  }
  .tools {
    display: flex;
    gap: 4px;
  }
  .tools button.on {
    border-color: transparent;
    background: var(--acc-soft);
    color: var(--fg);
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 96px;
    padding: 10px;
    border-radius: 10px;
    background: var(--surface);
  }
  .hint,
  .keep {
    margin: 0;
    color: var(--mut);
  }
  .parts {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 90px;
    margin: 0;
    padding: 0;
    overflow: auto;
    list-style: none;
  }
  .parts li {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: space-between;
    font-variant-numeric: tabular-nums;
  }
  .slide {
    display: grid;
    grid-template-columns: 110px 1fr 70px;
    gap: 8px;
    align-items: center;
  }
  .words {
    flex: 1;
    min-width: 120px;
  }
  .swatch {
    width: 22px;
    height: 22px;
    padding: 0;
    border-radius: 50%;
  }
  .swatch.on {
    outline: 2px solid var(--acc);
    outline-offset: 2px;
  }
  .when input {
    width: 64px;
  }
  .when {
    padding-top: 8px;
    border-top: 1px solid var(--line);
  }
  .range {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .range label {
    display: grid;
    grid-template-columns: 44px 1fr 76px;
    gap: 8px;
    align-items: center;
  }
  .range label span {
    color: var(--mut);
  }
  input[type='range'] {
    min-width: 0;
    accent-color: var(--acc);
  }
  input[type='text'],
  input[type='number'],
  select {
    padding: 4px 6px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: transparent;
    color: inherit;
    font: inherit;
  }
  b {
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .actions {
    margin-top: auto;
  }
  .grow {
    flex: 1;
  }
  button {
    padding: 5px 10px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  button:hover:not(:disabled) {
    background: var(--hover);
  }
  button:disabled {
    color: var(--mut);
    cursor: default;
  }
  button.main {
    border-color: transparent;
    background: var(--acc);
    color: var(--acc-fg);
  }
  .lane,
  .swatch {
    min-width: 0;
  }
  .done {
    margin: 0;
  }
</style>
