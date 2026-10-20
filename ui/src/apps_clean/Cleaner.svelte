<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { fadeSlide } from '../lib/motion';
  import './apps-clean.css';
  import { bytes, days, message } from './util';

  interface Item {
    path: string;
    size: number;
    ageDays: number;
    heuristic: boolean;
  }
  interface Category {
    id: string;
    label: string;
    why: string;
    size: number;
    count: number;
    checked: boolean;
    itemised: boolean;
    items: Item[];
    needsElevation: boolean;
  }
  interface Report {
    removed: string[];
    failed: { item: string; reason: string }[];
    freed: number;
    cancelled: boolean;
    elevatedRoots: string[];
  }

  let categories = $state<Category[]>([]);
  let on = $state<Set<string>>(new Set());
  let items = $state<Record<string, Set<string>>>({});
  let open = $state<string | null>(null);
  let scanning = $state(false);
  let cleaning = $state(false);
  let scanned = $state(false);
  let error = $state('');
  let report = $state<Report | null>(null);

  const total = $derived(
    categories
      .filter((c) => on.has(c.id))
      .reduce((n, c) => n + (c.itemised ? c.items.filter((i) => items[c.id]?.has(i.path)).reduce((m, i) => m + i.size, 0) : c.size), 0),
  );

  async function scan() {
    scanning = true;
    error = '';
    try {
      const found = await invoke<Category[]>('cleaner_scan');
      categories = found;
      on = new Set(found.filter((c) => c.checked && (c.size > 0 || c.count > 0)).map((c) => c.id));
      items = Object.fromEntries(found.filter((c) => c.itemised).map((c) => [c.id, new Set(c.items.filter((i) => !i.heuristic).map((i) => i.path))]));
      scanned = true;
    } catch (e) {
      error = message(e);
    }
    scanning = false;
  }

  onMount(scan);

  function flip<T>(set: Set<T>, v: T): Set<T> {
    const next = new Set(set);
    if (!next.delete(v)) next.add(v);
    return next;
  }

  async function clean() {
    cleaning = true;
    error = '';
    try {
      const request = {
        categories: [...on],
        items: Object.fromEntries([...on].filter((id) => items[id]).map((id) => [id, [...items[id]]])),
      };
      const done = await invoke<Report>('cleaner_clean', { request });
      await scan();
      report = done;
    } catch (e) {
      error = message(e);
    }
    cleaning = false;
  }

  async function deeper() {
    scanning = true;
    error = '';
    try {
      await invoke('cleaner_scan_elevated');
    } catch (e) {
      error = message(e);
    }
    await scan();
  }

  async function cleanSystem() {
    if (!report) return;
    try {
      await invoke('cleaner_clean_system', { roots: report.elevatedRoots });
      report = { ...report, elevatedRoots: [] };
      await scan();
    } catch (e) {
      error = message(e);
    }
  }
</script>

<div class="ac">
  <div class="ac-bar">
    <span class="ac-note grow" style="margin:0">
      {#if scanning}<span class="ac-spin"></span>Measuring each category{:else if scanned}Nothing is deleted until you press Clean.{/if}
    </span>
    {#if scanning}
      <button class="ghost" onclick={() => invoke('apps_clean_cancel')}>Cancel scan</button>
    {:else}
      <button class="ghost" onclick={deeper}>Deeper scan (administrator)</button>
      <button class="ghost" onclick={scan}>Scan again</button>
    {/if}
  </div>
  {#if error}<p class="ac-note warn">{error}</p>{/if}

  <div class="ac-list" aria-busy={scanning}>
    {#if scanned && !categories.length}<p class="ac-empty">Nothing to show.</p>{/if}
    {#each categories as c (c.id)}
      <div in:fadeSlide={{ token: 'listReorder', y: 4 }}>
        <div class="ac-item">
          <input class="ac-check" type="checkbox" checked={on.has(c.id)} onchange={() => (on = flip(on, c.id))} aria-label="Clean {c.label}" />
          <span class="ac-main">
            <b>{c.label}</b>
            <small>{c.why}</small>
          </span>
          {#if c.needsElevation}<span class="ac-chip" title="Part of this category needs administrator rights">+ system</span>{/if}
          {#if c.itemised && c.items.length}
            <button class="ghost" onclick={() => (open = open === c.id ? null : c.id)} aria-expanded={open === c.id}>{open === c.id ? 'Hide' : `${c.items.length} items`}</button>
          {/if}
          <span class="ac-size">{bytes(c.size)}</span>
        </div>
        {#if open === c.id}
          {#each c.items as i (i.path)}
            <div class="ac-item" style="padding-left:40px" in:fadeSlide={{ token: 'listReorder', y: 4 }}>
              <input
                class="ac-check"
                type="checkbox"
                checked={items[c.id]?.has(i.path)}
                onchange={() => (items[c.id] = flip(items[c.id] ?? new Set(), i.path))}
                aria-label="Include {i.path}"
              />
              <span class="ac-main">
                <b title={i.path}>{i.path}</b>
                <small>{i.heuristic ? 'Found by analysis. Check the path before ticking.' : 'Untouched'} · {days(i.ageDays)} old</small>
              </span>
              <span class="ac-size">{bytes(i.size)}</span>
              <button class="ghost" onclick={() => invoke('apps_clean_reveal', { path: i.path })}>Show</button>
            </div>
          {/each}
        {/if}
      </div>
    {/each}
  </div>

  <div class="ac-bar" style="margin-top:12px">
    <span class="ac-note grow" style="margin:0">{on.size} ticked, about {bytes(total)}. Screenshots, installers and leftovers go to the bin.</span>
    <button class="primary" onclick={clean} disabled={scanning || cleaning || !on.size}>{cleaning ? 'Cleaning' : 'Clean'}</button>
  </div>

  {#if report}
    <div in:fadeSlide={{ token: 'listReorder', y: 4 }}>
      <p class="ac-note" class:warn={report.failed.length > 0 || report.cancelled}>
        {report.cancelled ? 'Stopped early. ' : ''}Recovered {bytes(report.freed)} from {report.removed.length} item(s){report.failed.length ? `; ${report.failed.length} could not be removed` : ''}.
      </p>
      {#each report.failed.slice(0, 8) as f (f.item)}
        <p class="ac-note warn">{f.item}: {f.reason}</p>
      {/each}
      {#if report.failed.length > 8}<p class="ac-note">and {report.failed.length - 8} more.</p>{/if}
      {#if report.elevatedRoots.length}
        <button class="ghost" onclick={cleanSystem}>Clean system folders (administrator)</button>
      {/if}
    </div>
  {/if}
</div>
