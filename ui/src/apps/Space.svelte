<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { store } from '../lib/settings.svelte';
  import { message } from './api';
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
    archive: '#b08430',
    video: '#a9525f',
    image: '#4b8f60',
    audio: '#3c8a94',
    document: '#4876ae',
    code: '#6b82a0',
    app: '#a8603c',
    other: '#69737d',
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

  function zoomTo(path: string) {
    zoom = path;
    selected = null;
    load();
  }

  function pick(n: SpaceNode) {
    selected = n;
    menu = null;
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

<svelte:window onclick={() => (menu = null)} />

<div class="view">
  <div class="toolbar">
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

  <div class="map" bind:clientWidth={width} bind:clientHeight={height}>
    {#if !status}
      <div class="empty">Choose a drive or folder and press Scan.</div>
    {:else if !data || !data.size}
      <div class="empty">{status.scanning ? 'Scanning...' : 'Nothing matches here.'}</div>
    {:else}
      {#each boxes as b (b.node.path + b.node.name + b.level + b.node.other)}
        {@const n = b.node}
        <div
          class="box"
          class:frame={b.frame}
          class:sel={selected?.path === n.path && !n.other}
          role="button"
          tabindex="-1"
          title="{n.name}\n{size(n.size)}"
          style="left:{b.x}px;top:{b.y}px;width:{b.w}px;height:{b.h}px;{b.frame ? '' : `background:${n.dir ? 'var(--inset)' : KIND[n.kind]}`}"
          onclick={(e) => { e.stopPropagation(); pick(n); }}
          ondblclick={(e) => { e.stopPropagation(); open(n); }}
          oncontextmenu={(e) => { e.preventDefault(); e.stopPropagation(); if (!n.other) { selected = n; menu = { x: e.clientX, y: e.clientY, node: n }; } }}
          onkeydown={() => {}}
        >
          {#if n.tag}<span class="dot" style="background:{TAG_COLOR[n.tag]}"></span>{/if}
          {#if b.frame}
            <span class="head">{n.name} <small>{size(n.size)}</small></span>
          {:else if b.w > 52 && b.h > 28}
            <span class="lbl">{n.name}<small>{size(n.size)}</small></span>
          {/if}
        </div>
      {/each}
    {/if}
  </div>

  <div class="strip">
    {#if status?.scanning}
      <span>Scanning {status.files.toLocaleString()} files, {status.dirs.toLocaleString()} folders, {size(status.bytes)}</span>
      <span class="cur">{status.current}</span>
    {:else if selected && data}
      <span class="mono">{selected.path}</span>
      <span>{size(selected.size)}{data.size ? `, ${Math.round((selected.size / data.size) * 100)}% of this view` : ''}{selected.dir ? `, ${selected.files.toLocaleString()} files` : ''}</span>
    {:else if status}
      <span>{size(data?.size ?? status.bytes)} in {(data?.files ?? status.files).toLocaleString()} files{status.unreadable ? `, ${status.unreadable} folders could not be read` : ''}</span>
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
    max-width: 280px;
    padding: 6px 8px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    font-size: 13px;
  }
  .toolbar .path {
    flex: 1;
    min-width: 160px;
  }
  .toolbar .filter {
    flex: 1.4;
    min-width: 220px;
  }
  .crumbs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px;
    font-size: 13px;
  }
  .crumb {
    padding: 2px 6px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--fg-soft);
    font: inherit;
    cursor: pointer;
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
    position: relative;
    flex: 1;
    min-height: 200px;
    overflow: hidden;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--surface);
  }
  .box {
    position: absolute;
    box-sizing: border-box;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--bg) 70%, transparent);
    color: #fff;
    font-size: 11.5px;
    line-height: 1.25;
    cursor: default;
  }
  .box.frame {
    background: var(--inset);
    color: var(--fg);
  }
  .box:hover {
    filter: brightness(1.12);
  }
  .box.sel {
    outline: 2px solid var(--acc);
    outline-offset: -2px;
    z-index: 2;
  }
  .head {
    display: block;
    height: 16px;
    padding: 0 5px;
    overflow: hidden;
    font-weight: 600;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .head small,
  .lbl small {
    margin-left: 4px;
    font-weight: 400;
    opacity: 0.75;
  }
  .lbl {
    display: block;
    padding: 3px 5px;
    overflow: hidden;
    word-break: break-all;
  }
  .lbl small {
    display: block;
    margin: 0;
  }
  .dot {
    position: absolute;
    top: 3px;
    right: 3px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    box-shadow: 0 0 0 2px rgb(0 0 0 / 0.45);
    z-index: 1;
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
</style>
