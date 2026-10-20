<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { MANAGER_LABEL, message, runJobs, type UpdateItem, type UpdatesResult } from './api';
  import { jobs } from './jobs.svelte';

  let result = $state<UpdatesResult | null>(null);
  let checking = $state(false);
  let error = $state('');
  let picked = $state<Record<string, boolean>>({});
  let menu = $state<{ x: number; y: number; item: UpdateItem } | null>(null);

  const key = (i: UpdateItem) => `${i.manager}|${i.id}`;
  const chosen = $derived((result?.items ?? []).filter((i) => picked[key(i)]));
  const busy = $derived(jobs.running.some((j) => j.action === 'update'));

  async function check() {
    checking = true;
    error = '';
    try {
      result = await invoke<UpdatesResult>('apps_updates_check');
    } catch (e) {
      error = message(e);
    } finally {
      checking = false;
    }
  }

  async function load() {
    result = (await invoke<UpdatesResult | null>('apps_updates_last')) ?? result;
  }

  async function updateSelected(items: UpdateItem[]) {
    error = '';
    try {
      await runJobs('update', items.map((i) => ({ manager: i.manager, id: i.id, name: i.name, origin: i.origin })));
    } catch (e) {
      error = message(e);
    }
  }

  async function rule(kind: 'skip' | 'ignore', item: UpdateItem) {
    menu = null;
    try {
      await invoke('apps_rule_add', { kind, manager: item.manager, id: item.id, name: item.name, version: item.available });
      if (result) result = { ...result, items: result.items.filter((i) => key(i) !== key(item)), notCompared: result.notCompared.filter((i) => key(i) !== key(item)) };
    } catch (e) {
      error = message(e);
    }
  }

  function ago(secs: number) {
    if (!secs) return '';
    const m = Math.round((Date.now() / 1000 - secs) / 60);
    return m < 1 ? 'just now' : m < 60 ? `${m} min ago` : `${Math.round(m / 60)} h ago`;
  }

  onMount(() => {
    load().then(() => {
      if (!result) check();
    });
    const offs = [listen<UpdatesResult>('apps://updates', (e) => (result = e.payload)), listen('apps://changed', load)];
    return () => offs.forEach((o) => o.then((f) => f()));
  });
</script>

<svelte:window onclick={() => (menu = null)} />

<div class="view">
  <div class="toolbar">
    <button class="ghost" disabled={checking} onclick={check}>{checking ? 'Checking...' : 'Check now'}</button>
    <button class="ghost primary" disabled={!chosen.length || busy} onclick={() => updateSelected(chosen)}>Update selected{chosen.length ? ` (${chosen.length})` : ''}</button>
    <span class="grow"></span>
    {#if result?.checkedAt}<span class="note">Checked {ago(result.checkedAt)}</span>{/if}
  </div>

  {#if error}<div class="note err">{error}</div>{/if}
  {#each result?.errors ?? [] as e}
    <div class="note err">{MANAGER_LABEL[e.source] ?? e.source} could not be checked: {e.message}</div>
  {/each}

  <div class="scroll">
    {#if !result && checking}
      <div class="empty">Looking for updates...</div>
    {:else if result && !result.items.length && !result.notCompared.length}
      <div class="empty">Everything is up to date.</div>
    {:else if result}
      <table class="t">
        <thead>
          <tr><th></th><th>App</th><th>Installed</th><th>Available</th><th>Source</th><th></th></tr>
        </thead>
        <tbody>
          {#each result.items as item (key(item))}
            {@const job = jobs.latest(item.manager, item.name)}
            <tr oncontextmenu={(e) => { e.preventDefault(); menu = { x: e.clientX, y: e.clientY, item }; }}>
              <td><input type="checkbox" aria-label="Select {item.name}" bind:checked={picked[key(item)]} /></td>
              <td><span class="name" title={item.id}>{item.name}<small>{item.id}</small></span></td>
              <td class="mono">{item.installed}</td>
              <td class="mono">{item.available}</td>
              <td><span class="badge">{MANAGER_LABEL[item.manager] ?? item.manager}</span></td>
              <td class="act">
                {#if jobs.active(job)}
                  <span class="progress" title={job?.line}><i style="width:{job?.percent ?? 4}%"></i></span>
                  <button class="ghost small" onclick={() => job && jobs.cancel(job.id)}>Cancel</button>
                {:else if job?.state === 'failed'}
                  <span class="badge warn" title={job.line}>Failed</span>
                  <button class="ghost small" onclick={() => updateSelected([item])}>Retry</button>
                {:else if job?.state === 'done'}
                  <span class="badge acc">Updated</span>
                {:else}
                  <button class="ghost small" onclick={() => updateSelected([item])}>Update</button>
                  <button class="ghost small" aria-label="Rules for {item.name}" onclick={(e) => { e.stopPropagation(); menu = { x: e.clientX - 160, y: e.clientY + 8, item }; }}>...</button>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>

  {#if result?.notCompared.length}
    <details class="scroll" style="flex:none;max-height:30%">
      <summary class="note">Versions not compared ({result.notCompared.length})</summary>
      <table class="t">
        <tbody>
          {#each result.notCompared as item (key(item))}
            <tr oncontextmenu={(e) => { e.preventDefault(); menu = { x: e.clientX, y: e.clientY, item }; }}>
              <td><span class="name">{item.name}<small>{item.id}</small></span></td>
              <td class="mono">{item.installed || 'unknown'}</td>
              <td class="mono">{item.available}</td>
              <td><span class="badge">{MANAGER_LABEL[item.manager] ?? item.manager}</span></td>
              <td class="act"><button class="ghost small" onclick={() => updateSelected([item])}>Update</button></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </details>
  {/if}

  {#if result?.pending.length}
    <div class="note">Not available yet: {result.pending.join(', ')}. Only package managers are checked for now.</div>
  {/if}
</div>

{#if menu}
  <div class="menu" role="menu" style="left:{Math.max(8, menu.x)}px;top:{menu.y}px">
    <button role="menuitem" onclick={() => menu && rule('skip', menu.item)}>Skip version {menu.item.available}</button>
    <button role="menuitem" onclick={() => menu && rule('ignore', menu.item)}>Stop checking this app</button>
  </div>
{/if}
