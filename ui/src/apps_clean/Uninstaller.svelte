<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { fadeSlide } from '../lib/motion';
  import './apps-clean.css';
  import { bytes, message } from './util';

  interface App {
    id: string;
    name: string;
    version: string;
    publisher: string;
    source: string;
    size: number;
    installed: string;
    location: string | null;
    stamp: string;
  }
  interface Leftover {
    path: string;
    kind: 'file' | 'registry';
    confidence: 'verified' | 'name';
    size: number;
    why: string;
  }
  interface Report {
    removed: string[];
    failed: { item: string; reason: string }[];
    freed: number;
    needsElevation: boolean;
    appRemoved: boolean;
    note: string;
  }

  let { values }: { values: Record<string, unknown> } = $props();

  let apps = $state<App[]>([]);
  let loading = $state(true);
  let query = $state('');
  let error = $state('');
  let chosen = $state<App | null>(null);
  let leftovers = $state<Leftover[]>([]);
  let ticked = $state<Set<string>>(new Set());
  let scanning = $state(false);
  let deep = $state(false);
  let confirming = $state(false);
  let removing = $state(false);
  let report = $state<Report | null>(null);

  const shown = $derived(apps.filter((a) => `${a.name} ${a.publisher}`.toLowerCase().includes(query.trim().toLowerCase())));
  const tickedSize = $derived(leftovers.filter((l) => ticked.has(l.path)).reduce((n, l) => n + l.size, 0));

  async function load() {
    loading = true;
    error = '';
    try {
      apps = await invoke<App[]>('uninstaller_list');
    } catch (e) {
      error = message(e);
    }
    loading = false;
  }

  onMount(load);

  async function scan(app: App, wide: boolean) {
    chosen = app;
    deep = wide;
    report = null;
    confirming = false;
    scanning = true;
    error = '';
    leftovers = [];
    try {
      leftovers = await invoke<Leftover[]>('uninstaller_scan', { id: app.id, deep: wide });
      const pre = values.preTickVerified !== false;
      ticked = new Set(leftovers.filter((l) => pre && l.confidence === 'verified').map((l) => l.path));
    } catch (e) {
      error = message(e);
    }
    scanning = false;
  }

  function toggle(path: string) {
    const next = new Set(ticked);
    if (!next.delete(path)) next.add(path);
    ticked = next;
  }

  async function remove() {
    if (!chosen) return;
    if (!confirming) {
      confirming = true;
      return;
    }
    confirming = false;
    removing = true;
    error = '';
    try {
      const items = leftovers.filter((l) => ticked.has(l.path)).map((l) => ({ path: l.path, kind: l.kind }));
      report = await invoke<Report>('uninstaller_remove', { id: chosen.id, stamp: chosen.stamp, items });
      if (report.appRemoved) await load();
    } catch (e) {
      error = message(e);
    }
    removing = false;
  }

  async function retryElevated() {
    if (!chosen || !report) return;
    try {
      await invoke('uninstaller_retry_elevated', { id: chosen.id, paths: report.failed.map((f) => f.item) });
      report = { ...report, failed: [], needsElevation: false };
    } catch (e) {
      error = message(e);
    }
  }

  function back() {
    invoke('apps_clean_cancel');
    chosen = null;
    report = null;
    leftovers = [];
  }

  const reveal = (path: string) => invoke('apps_clean_reveal', { path });
</script>

<div class="ac">
  {#if !chosen}
    <div class="ac-bar">
      <input type="search" placeholder="Search installed apps" aria-label="Search installed apps" bind:value={query} />
      <button class="ghost" onclick={load} disabled={loading}>Refresh</button>
    </div>
    {#if error}<p class="ac-note warn">{error}</p>{/if}
    <div class="ac-list" aria-busy={loading}>
      {#if loading}
        <p class="ac-empty"><span class="ac-spin"></span>Reading installed apps</p>
      {:else if !shown.length}
        <p class="ac-empty">{apps.length ? 'No app matches that search.' : 'No removable apps were found.'}</p>
      {/if}
      {#each shown as app (app.id)}
        <div class="ac-item" style="padding:0">
          <button class="ac-item" style="flex:1" onclick={() => scan(app, false)}>
            <span class="ac-main">
              <b>{app.name}</b>
              <small>{[app.version, app.publisher, app.installed && `installed ${app.installed}`].filter(Boolean).join('  ·  ')}</small>
            </span>
            <span class="ac-chip">{app.source}</span>
            <span class="ac-size">{app.size ? bytes(app.size) : ''}</span>
          </button>
          {#if app.location}
            <button class="ghost" style="margin-right:10px" onclick={() => reveal(app.location!)} aria-label="Show {app.name} in the file manager">Show</button>
          {/if}
        </div>
      {/each}
    </div>
  {:else}
    <div class="ac-bar">
      <button class="ghost" onclick={back}>Back to the list</button>
      <span class="grow"></span>
      {#if scanning}
        <button class="ghost" onclick={() => invoke('apps_clean_cancel')}>Cancel scan</button>
      {:else if !report}
        <button class="ghost" onclick={() => chosen && scan(chosen, true)} disabled={deep}>{deep ? 'Deeper scan done' : 'Scan deeper'}</button>
      {/if}
    </div>
    <h3 style="margin:4px 0 2px">{chosen.name}</h3>
    <p class="ac-note" style="margin-top:0">{[chosen.version, chosen.publisher, chosen.source].filter(Boolean).join('  ·  ')}</p>
    {#if error}<p class="ac-note warn">{error}</p>{/if}

    {#if report}
      <div in:fadeSlide={{ token: 'listReorder', y: 4 }}>
        <p class="ac-note" class:warn={!report.appRemoved}>
          {report.appRemoved ? `${chosen.name} was removed. ${report.note}.` : `${chosen.name} was not removed. ${report.note}`}
        </p>
        {#if report.appRemoved}
          <p class="ac-note">{report.removed.length} related item(s) cleared, {bytes(report.freed)} recovered.</p>
        {/if}
        {#each report.failed as f (f.item)}
          <p class="ac-note warn">Could not remove {f.item}: {f.reason}</p>
        {/each}
        <div class="ac-bar">
          {#if report.needsElevation && report.failed.length}
            <button class="ghost" onclick={retryElevated}>Retry as administrator</button>
          {/if}
          <button class="primary" onclick={back}>Done</button>
        </div>
      </div>
    {:else}
      {#if scanning}
        <p class="ac-note"><span class="ac-spin"></span>Looking for configuration, caches, logs and helpers</p>
      {/if}
      <div class="ac-list">
        {#if !scanning && !leftovers.length}
          <p class="ac-empty">No leftovers were found. Only the app itself will be removed.</p>
        {/if}
        {#each leftovers as l (l.path)}
          <div class="ac-item" in:fadeSlide={{ token: 'listReorder', y: 4 }}>
            <input class="ac-check" type="checkbox" checked={ticked.has(l.path)} onchange={() => toggle(l.path)} aria-label="Remove {l.path}" />
            <span class="ac-main">
              <b title={l.path}>{l.path}</b>
              <small>{l.confidence === 'verified' ? l.why : 'Matched by name only. Check the path before ticking.'}</small>
            </span>
            <span class="ac-chip" class:ok={l.confidence === 'verified'} class:warn={l.confidence !== 'verified'}>{l.confidence === 'verified' ? 'Verified' : 'Name match'}</span>
            <span class="ac-size">{l.kind === 'registry' ? 'registry' : bytes(l.size)}</span>
            {#if l.kind === 'file'}
              <button class="ghost" onclick={() => reveal(l.path)} aria-label="Show in the file manager">Show</button>
            {/if}
          </div>
        {/each}
      </div>
      <div class="ac-bar" style="margin-top:12px">
        <span class="ac-note grow" style="margin:0">
          {#if confirming}Click again to remove {chosen.name}. Its own uninstaller runs first.{:else}{ticked.size} ticked, {bytes(tickedSize)}{/if}
        </span>
        <button class="primary" class:danger={confirming} onclick={remove} disabled={scanning || removing}>
          {removing ? 'Removing' : confirming ? 'Yes, remove it' : 'Remove app and ticked items'}
        </button>
      </div>
    {/if}
  {/if}
</div>
