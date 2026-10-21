<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import { store } from '../lib/settings.svelte';
  import { message } from './api';
  import { duration, reducedMotion } from '../lib/motion';
  import { layout, type SpaceNode } from './space-layout';

  interface Drive { name: string; mount: string; total: number; free: number }
  interface Status { scanning: boolean; root: string; files: number; dirs: number; bytes: number; unreadable: number; current: string }

  const TAGS = ['red', 'yellow', 'green', 'blue'];
  const SAVED = 'spaceMap.root';

  let drives = $state<Drive[]>([]);
  let root = $state('');
  let status = $state<Status | null>(null);
  let zoom = $state('');
  let data = $state<SpaceNode | null>(null);
  let filter = $state('');
  let selected = $state<SpaceNode | null>(null);
  let hovered = $state<SpaceNode | null>(null);
  let back = $state<string[]>([]);
  let forward = $state<string[]>([]);
  let heading = $state<'in' | 'out'>('in');
  let menu = $state<{ x: number; y: number; node: SpaceNode } | null>(null);
  let error = $state('');
  let note = $state('');
  let exported = $state('');
  let width = $state(0);
  let height = $state(0);

  const boxes = $derived(layout(data, width, height));
  const sep = $derived(status?.root.includes('\\') ? '\\' : '/');
  const crumbs = $derived.by(() => {
    if (!status) return [];
    const base = status.root.replace(/[\\/]+$/, '');
    const out = [{ label: status.root, path: status.root }];
    const rel = zoom.slice(base.length).split(/[\\/]/).filter(Boolean);
    let acc = base;
    for (const part of rel) {
      acc += sep + part;
      out.push({ label: part, path: acc });
    }
    return out;
  });

  const KIND: Record<string, string> = {
    archive: '#e2c58d',
    video: '#e49fae',
    image: '#9fd3b2',
    audio: '#94cdd6',
    document: '#a2c0ea',
    code: '#b3c0d6',
    app: '#e4b496',
    other: '#b9c0c9',
  };
  const SOFT = ['#9fc3e8', '#a5d8c0', '#eab99a', '#e8cf9a', '#e5a9b8', '#98d0d6', '#b7cf9a', '#d9b3a6'];
  const tint = (n: SpaceNode) => {
    if (n.kind !== 'other' && n.kind !== 'folder') return KIND[n.kind];
    let h = 0;
    for (const c of n.name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
    return n.other ? KIND.other : SOFT[h % SOFT.length];
  };
  const TAG_COLOR: Record<string, string> = { red: '#e5484d', yellow: '#f2c230', green: '#3fb36b', blue: '#3b8eea' };

  function size(b: number): string {
    const u = ['B', 'KB', 'MB', 'GB', 'TB'];
    let v = b;
    let i = 0;
    while (v >= 1024 && i < 4) {
      v /= 1024;
      i++;
    }
    return i === 0 ? `${b} B` : `${v.toFixed(v >= 100 ? 0 : 1)} ${u[i]}`;
  }

  async function load() {
    if (!status) return;
    try {
      data = await invoke<SpaceNode | null>('space_view', { path: zoom || status.root, filter, depth: 4, limit: 60 });
      error = '';
    } catch (e) {
      error = message(e);
    }
  }

  async function poll() {
    status = await invoke<Status | null>('space_status');
    if (status) {
      if (!zoom) zoom = status.root;
      await load();
    }
  }

  async function start() {
    const path = root.trim();
    if (!path) return;
    try {
      await invoke('space_start', { path });
      localStorage.setItem(SAVED, path);
      zoom = path;
      back = [];
      forward = [];
      selected = null;
      exported = '';
      error = '';
      await poll();
    } catch (e) {
      error = message(e);
    }
  }

  async function cancel() {
    await invoke('space_cancel');
    setTimeout(poll, 300);
  }

  const parent = $derived(crumbs.length > 1 ? crumbs[crumbs.length - 2].path : '');

  function zoomTo(path: string, record = true) {
    if (path === zoom) return;
    if (record) {
      back = [...back, zoom];
      forward = [];
    }
    heading = path.length > zoom.length ? 'in' : 'out';
    zoom = path;
    selected = null;
    hovered = null;
    load();
  }

  function goUp() {
    if (parent) zoomTo(parent);
  }

  function goBack() {
    const to = back[back.length - 1];
    if (!to) return;
    back = back.slice(0, -1);
    forward = [...forward, zoom];
    zoomTo(to, false);
  }

  function goForward() {
    const to = forward[forward.length - 1];
    if (!to) return;
    forward = forward.slice(0, -1);
    back = [...back, zoom];
    zoomTo(to, false);
  }

  function enter(n: SpaceNode | null) {
    if (n?.dir && !n.other) zoomTo(n.path);
  }

  function onKey(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement;
    if (e.key === 'Escape') {
      if (menu) menu = null;
      else if (typing) (e.target as HTMLElement).blur();
      else goUp();
      return;
    }
    if (typing) return;
    if (e.altKey && e.key === 'ArrowLeft') goBack();
    else if (e.altKey && e.key === 'ArrowRight') goForward();
    else if (e.key === 'Backspace' || e.key === 'ArrowLeft' || e.key === 'ArrowUp') goUp();
    else if (e.key === 'Enter' || e.key === 'ArrowRight' || e.key === 'ArrowDown') enter(selected ?? data?.children.find((c) => c.dir && !c.other) ?? null);
    else return;
    e.preventDefault();
  }

  function onMouseUp(e: MouseEvent) {
    if (e.button === 3) goBack();
    else if (e.button === 4) goForward();
    else return;
    e.preventDefault();
  }

  /** Content melts in from a soft blur: slightly larger when stepping out, smaller when stepping in. */
  function melt(_node: Element) {
    const reduced = reducedMotion();
    const from = heading === 'in' ? 0.94 : 1.05;
    return {
      duration: duration('glassIn'),
      easing: (t: number) => 1 - Math.pow(1 - t, 4),
      css: (t: number, u: number) => (reduced ? `opacity:${t}` : `opacity:${t};transform:scale(${1 - (1 - from) * u});filter:blur(${u * 8}px)`),
    };
  }

  const radius = (w: number, h: number) => Math.max(3, Math.min(11, w / 7, h / 7));

  function pick(n: SpaceNode) {
    menu = null;
    if (n.other) return;
    if (n.dir) enter(n);
    else selected = n;
  }

  function open(n: SpaceNode) {
    if (n.dir && !n.other) zoomTo(n.path);
    else if (!n.other) invoke('space_open', { path: n.path }).catch((e) => (error = message(e)));
  }

  async function tag(n: SpaceNode, color: string | null) {
    menu = null;
    await invoke('space_tag', { path: n.path, color }).catch((e) => (error = message(e)));
    await load();
  }

  async function copy(text: string) {
    menu = null;
    await navigator.clipboard.writeText(text).catch(() => {});
  }

  async function exportReport() {
    try {
      exported = await invoke<string>('space_export', { path: zoom || root, filter, depth: Number(store.values('spaceMap').reportDepth ?? 3) });
      error = '';
    } catch (e) {
      error = message(e);
    }
  }

  let debounce: ReturnType<typeof setTimeout>;
  function onFilter() {
    clearTimeout(debounce);
    debounce = setTimeout(load, 200);
  }

  onMount(() => {
    (async () => {
      drives = await invoke<Drive[]>('space_drives').catch(() => []);
      await poll();
      const start = String(store.values('spaceMap').startRoot ?? '').trim();
      root = status?.root ?? (start || localStorage.getItem(SAVED) || drives[0]?.mount || '');
    })();
    const timer = setInterval(() => {
      if (status?.scanning && document.visibilityState === 'visible') poll();
    }, 800);
    return () => {
      clearInterval(timer);
      clearTimeout(debounce);
    };
  });
</script>

<svelte:window onclick={() => (menu = null)} onkeydown={onKey} onmouseup={onMouseUp} />

<div class="view">
  <div class="toolbar">
    <div class="nav" role="group" aria-label="Navigate">
      <button class="ghost icon" aria-label="Back" title="Back (Alt+Left)" disabled={!back.length} onclick={goBack}>&#8249;</button>
      <button class="ghost icon" aria-label="Forward" title="Forward (Alt+Right)" disabled={!forward.length} onclick={goForward}>&#8250;</button>
      <button class="ghost icon" aria-label="Up one level" title="Up one level (Esc)" disabled={!parent} onclick={goUp}>&#8593;</button>
    </div>
    <select aria-label="Drive" value={drives.some((d) => d.mount === root) ? root : ''} onchange={(e) => (root = e.currentTarget.value)}>
      <option value="" disabled>Drive</option>
      {#each drives as d}
        <option value={d.mount}>{d.mount} {d.name ? `(${d.name})` : ''} - {size(d.total - d.free)} used, {size(d.free)} free</option>
      {/each}
    </select>
    <input type="text" class="path" placeholder="Folder to scan" aria-label="Folder to scan" bind:value={root} onkeydown={(e) => e.key === 'Enter' && start()} />
    {#if status?.scanning}
      <button class="ghost" onclick={cancel}>Cancel</button>
    {:else}
      <button class="ghost primary" onclick={start} disabled={!root.trim()}>{status ? 'Rescan' : 'Scan'}</button>
    {/if}
    <input type="text" class="filter" placeholder="Filter, e.g. *.zip;>500mb;>1y;:yellow" aria-label="Filter" bind:value={filter} oninput={onFilter} />
    <button class="ghost" onclick={exportReport} disabled={!status}>Export report</button>
  </div>

  {#if crumbs.length}
    <div class="crumbs" aria-label="Location">
      {#each crumbs as c, i}
        {#if i}<span class="sepr">&rsaquo;</span>{/if}
        <button class="crumb" class:last={i === crumbs.length - 1} onclick={() => zoomTo(c.path)}>{c.label}</button>
      {/each}
    </div>
  {/if}
  {#if error}<div class="note err">{error}</div>{/if}
  {#if exported}
    <div class="note">Saved {exported} <button class="ghost small" onclick={() => invoke('apps_clean_reveal', { path: exported })}>Show</button></div>
  {/if}

  <div class="map">
    {#if status && data && data.size}
      <button class="top" disabled={!parent} onclick={goUp} title={parent ? 'Click to go up a level (Esc)' : ''}>
        {#if parent}<span class="chev">&#8249;</span>{/if}
        <span class="tname">{data.name}</span>
        <small>{size(data.size)}</small>
      </button>
    {/if}
    <div class="stage" bind:clientWidth={width} bind:clientHeight={height}>
      {#if !status}
        <div class="empty">Choose a drive or folder and press Scan.</div>
      {:else if !data || !data.size}
        <div class="empty">{status.scanning ? 'Scanning...' : 'Nothing matches here.'}</div>
      {:else}
        {#key zoom}
          <div class="layer" in:melt out:fade={{ duration: 120 }}>
            {#each boxes as b (b.node.path + b.node.name + b.level + b.node.other)}
              {@const n = b.node}
              <div
                class="box"
                class:frame={b.frame}
                class:dir={n.dir && !b.frame}
                class:sel={selected?.path === n.path && !n.other}
                role="button"
                tabindex="-1"
                style="left:{b.x}px;top:{b.y}px;width:{b.w}px;height:{b.h}px;--r:{radius(b.w, b.h)}px;--k:{tint(n)}"
                onclick={(e) => { e.stopPropagation(); pick(n); }}
                ondblclick={(e) => { e.stopPropagation(); if (!n.dir) open(n); }}
                onmouseenter={() => (hovered = n)}
                onmouseleave={() => (hovered = null)}
                oncontextmenu={(e) => { e.preventDefault(); e.stopPropagation(); if (!n.other) { selected = n; menu = { x: e.clientX, y: e.clientY, node: n }; } }}
                onkeydown={() => {}}
              >
                <div class="fill">
                  {#if n.tag}<span class="dot" style="background:{TAG_COLOR[n.tag]}"></span>{/if}
                  {#if b.frame}
                    <span class="head">{n.name} <small>{size(n.size)}</small></span>
                  {:else if b.w > 52 && b.h > 28}
                    <span class="lbl">{n.name}<small>{size(n.size)}</small></span>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {/key}
      {/if}
    </div>
  </div>

  <div class="strip">
    {#if status?.scanning}
      <span>Scanning {status.files.toLocaleString()} files, {status.dirs.toLocaleString()} folders, {size(status.bytes)}</span>
      <span class="cur">{status.current}</span>
    {:else if (hovered ?? selected) && data}
      {@const n = (hovered ?? selected)!}
      <span class="mono">{n.other ? n.name : n.path}</span>
      <span>{size(n.size)}{data.size ? `, ${Math.round((n.size / data.size) * 100)}% of this view` : ''}{n.dir && !n.other ? `, ${n.files.toLocaleString()} files` : ''}</span>
    {:else if status}
      <span>{size(data?.size ?? status.bytes)} in {(data?.files ?? status.files).toLocaleString()} files{status.unreadable ? `, ${status.unreadable} folders could not be read` : ''}. Click a folder to go in; Esc or the arrow keys go up and down a level.</span>
    {/if}
  </div>
</div>

{#if menu}
  {@const n = menu.node}
  <div class="menu" role="menu" style="left:{Math.max(8, menu.x)}px;top:{menu.y}px">
    <button role="menuitem" onclick={() => { menu = null; open(n); }}>{n.dir ? 'Zoom in' : 'Open'}</button>
    <button role="menuitem" onclick={() => { menu = null; invoke('apps_clean_reveal', { path: n.path }); }}>Show in file manager</button>
    <button role="menuitem" onclick={() => copy(n.path)}>Copy path</button>
    <div class="tags">
      {#each TAGS as c}
        <button role="menuitem" class="swatch" class:on={n.tag === c} aria-label="Tag {c}" title="Tag {c}" style="background:{TAG_COLOR[c]}" onclick={() => tag(n, c)}></button>
      {/each}
      <button role="menuitem" class="ghost small" disabled={!n.tag} onclick={() => tag(n, null)}>Clear</button>
    </div>
  </div>
{/if}

<style>
  .toolbar select {
    max-width: 240px;
    padding: 6px 8px;
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    font-size: 13px;
  }
  .toolbar .path {
    flex: 1;
    min-width: 140px;
  }
  .toolbar .filter {
    flex: 1.4;
    min-width: 200px;
  }
  .nav {
    display: flex;
    gap: 4px;
  }
  button.icon {
    width: 30px;
    padding: 3px 0;
    font-size: 17px;
    line-height: 1;
    border-radius: 10px;
  }
  .crumbs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px;
    font-size: 13px;
  }
  .crumb {
    padding: 2px 8px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--fg-soft);
    font: inherit;
    cursor: pointer;
    transition: background var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .crumb:hover {
    background: var(--hover);
  }
  .crumb.last {
    color: var(--fg);
    font-weight: 600;
  }
  .sepr {
    color: var(--mut);
  }
  .map {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 6px;
    min-height: 220px;
    padding: 8px;
    overflow: hidden;
    border: 1px solid var(--line);
    border-radius: 18px;
    background: color-mix(in srgb, var(--surface) 92%, var(--fg) 3%);
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.06);
  }
  .top {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 12px;
    border: 0;
    border-radius: 12px;
    background: color-mix(in srgb, var(--fg) 8%, transparent);
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.09), inset 0 0 0 1px rgb(255 255 255 / 0.05);
    color: var(--fg);
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    text-align: left;
    cursor: pointer;
    transition:
      background var(--m-hoverLift-dur) var(--m-hoverLift-ease),
      transform var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .top:not(:disabled):hover {
    background: color-mix(in srgb, var(--fg) 14%, transparent);
  }
  .top:not(:disabled):active {
    transform: scale(0.994);
  }
  .top:disabled {
    cursor: default;
  }
  .top .chev {
    font-size: 18px;
    line-height: 1;
    color: var(--mut);
  }
  .top .tname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .top small {
    margin-left: auto;
    color: var(--mut);
    font-weight: 500;
  }
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .layer {
    position: absolute;
    inset: 0;
    transform-origin: center;
  }
  .box {
    position: absolute;
    box-sizing: border-box;
    padding: 1.5px;
    cursor: pointer;
    transition:
      left var(--m-tabSwitch-dur) var(--m-tabSwitch-ease),
      top var(--m-tabSwitch-dur) var(--m-tabSwitch-ease),
      width var(--m-tabSwitch-dur) var(--m-tabSwitch-ease),
      height var(--m-tabSwitch-dur) var(--m-tabSwitch-ease);
  }
  .fill {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    border-radius: var(--r);
    color: var(--fg);
    font-size: 11.5px;
    line-height: 1.25;
    background-color: color-mix(in srgb, var(--k) 52%, var(--surface));
    background-image: linear-gradient(160deg, rgb(255 255 255 / 0.16), rgb(255 255 255 / 0) 60%);
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.2), inset 0 0 0 1px rgb(255 255 255 / 0.07);
    transition:
      filter var(--m-hoverLift-dur) var(--m-hoverLift-ease),
      box-shadow var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .box.dir .fill {
    background-color: color-mix(in srgb, var(--k) 42%, var(--surface));
  }
  .box.frame .fill {
    background-color: color-mix(in srgb, var(--fg) 7%, transparent);
    background-image: none;
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.08), inset 0 0 0 1px rgb(255 255 255 / 0.05);
  }
  .box:hover > .fill {
    filter: brightness(1.14);
  }
  .box.sel > .fill {
    box-shadow: inset 0 0 0 2px var(--acc), inset 0 1px 0 rgb(255 255 255 / 0.2);
  }
  .head {
    display: block;
    height: 18px;
    padding: 1px 8px 0;
    overflow: hidden;
    font-weight: 600;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .head small,
  .lbl small {
    margin-left: 4px;
    color: var(--mut);
    font-weight: 400;
  }
  .lbl {
    display: block;
    padding: 4px 7px;
    overflow: hidden;
    word-break: break-all;
  }
  .lbl small {
    display: block;
    margin: 0;
  }
  .dot {
    position: absolute;
    top: 4px;
    right: 4px;
    z-index: 1;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    box-shadow: 0 0 0 2px rgb(0 0 0 / 0.35);
  }
  .strip {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    min-height: 18px;
    color: var(--mut);
    font-size: 12.5px;
  }
  .strip .mono,
  .strip .cur {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tags {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
  }
  .swatch {
    width: 18px;
    height: 18px;
    padding: 0;
    border: 2px solid transparent;
    border-radius: 50%;
    cursor: pointer;
  }
  .swatch.on {
    border-color: var(--fg);
  }
  @media (prefers-reduced-motion: reduce) {
    .box {
      transition: none;
    }
  }
</style>
