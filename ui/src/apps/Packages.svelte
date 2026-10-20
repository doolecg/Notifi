<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import Confirm from './Confirm.svelte';
  import { MANAGER_LABEL, message, runJobs, type Listing, type ManagerInfo, type Pkg, type UpdatesResult } from './api';
  import { jobs } from './jobs.svelte';

  type Tab = 'search' | 'installed' | 'updates';
  let tab = $state<Tab>('installed');
  let query = $state('');
  let managers = $state<ManagerInfo[] | null>(null);
  let installed = $state<Listing | null>(null);
  let found = $state<Listing | null>(null);
  let updates = $state<UpdatesResult | null>(null);
  let loading = $state(false);
  let error = $state('');
  let filter = $state('');
  let confirmRemove = $state<Pkg | null>(null);
  let timer: ReturnType<typeof setTimeout>;

  const anyManager = $derived(managers?.some((m) => m.found) ?? true);
  const installedKeys = $derived(new Set((installed?.items ?? []).map((p) => `${p.manager}|${p.id}`)));
  const shown = $derived(
    (installed?.items ?? []).filter((p) => !filter || `${p.name} ${p.id}`.toLowerCase().includes(filter.toLowerCase())),
  );
  const target = (p: Pkg) => ({ manager: p.manager, id: p.id, name: p.name, origin: p.origin });

  async function loadInstalled() {
    loading = true;
    try {
      installed = await invoke<Listing>('apps_pkg_installed');
    } catch (e) {
      error = message(e);
    } finally {
      loading = false;
    }
  }

  async function loadUpdates() {
    updates = await invoke<UpdatesResult>('apps_updates_check').catch(() => updates);
  }

  function onQuery() {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      if (query.trim().length < 2) return (found = null);
      loading = true;
      try {
        found = await invoke<Listing>('apps_pkg_search', { query });
      } catch (e) {
        error = message(e);
      } finally {
        loading = false;
      }
    }, 350);
  }

  async function run(action: 'install' | 'update' | 'remove', pkgs: Pkg[]) {
    error = '';
    try {
      await runJobs(action, pkgs.map((p) => ({ ...target(p), expectedVersion: action === 'remove' ? p.version : undefined })));
    } catch (e) {
      error = message(e);
    }
  }

  $effect(() => {
    if (tab === 'updates' && !updates) loadUpdates();
  });

  onMount(() => {
    invoke<ManagerInfo[]>('apps_managers').then((m) => {
      managers = m;
      if (m.some((x) => x.found)) loadInstalled();
    });
    const off = listen('apps://changed', () => {
      loadInstalled();
      invoke<UpdatesResult | null>('apps_updates_last').then((u) => u && (updates = u));
    });
    return () => off.then((f) => f());
  });
</script>

<div class="view">
  {#if managers && !anyManager}
    <div class="scroll">
      <div class="empty">
        No package manager was found. {#if navigator.userAgent.includes('Windows')}Install winget (App Installer from the Microsoft Store) or Scoop (scoop.sh).{:else}Install PackageKit (pkcon) or Flatpak with your distribution's package manager.{/if}
      </div>
    </div>
  {:else}
    <div class="toolbar">
      <div class="seg" role="tablist">
        {#each ['search', 'installed', 'updates'] as const as t}
          <button role="tab" aria-selected={tab === t} class:on={tab === t} onclick={() => (tab = t)}>
            {t === 'search' ? 'Search' : t === 'installed' ? 'Installed' : 'Updates'}{t === 'updates' && updates ? ` (${updates.items.length})` : ''}
          </button>
        {/each}
      </div>
      {#if tab === 'search'}
        <input type="search" placeholder="Search apps" aria-label="Search apps" bind:value={query} oninput={onQuery} />
      {:else if tab === 'installed'}
        <input type="search" placeholder="Filter installed" aria-label="Filter installed" bind:value={filter} />
        <button class="ghost" disabled={loading} onclick={loadInstalled}>Refresh</button>
      {:else}
        <button class="ghost" onclick={loadUpdates}>Refresh sources</button>
        <button class="ghost primary" disabled={!updates?.items.length} onclick={() => updates && run('update', updates.items.map((u) => ({ manager: u.manager, id: u.id, name: u.name, version: u.installed, available: u.available, origin: u.origin })))}>Update all</button>
      {/if}
      <span class="grow"></span>
      {#if managers}<span class="note">{managers.filter((m) => m.found).map((m) => m.label).join(', ')}</span>{/if}
    </div>

    {#if error}<div class="note err">{error}</div>{/if}
    {#each (tab === 'search' ? found : installed)?.errors ?? [] as e}
      <div class="note err">{MANAGER_LABEL[e.source] ?? e.source}: {e.message}</div>
    {/each}

    <div class="scroll">
      {#if tab === 'updates'}
        {#if !updates}<div class="empty">Checking...</div>
        {:else if !updates.items.length}<div class="empty">Nothing to update.</div>
        {:else}
          <table class="t">
            <thead><tr><th>Package</th><th>Installed</th><th>Available</th><th>Source</th><th></th></tr></thead>
            <tbody>
              {#each updates.items as u (u.manager + u.id)}
                {@const job = jobs.latest(u.manager, u.name)}
                <tr>
                  <td><span class="name" title={u.id}>{u.name}<small>{u.id}</small></span></td>
                  <td class="mono">{u.installed}</td>
                  <td class="mono">{u.available}</td>
                  <td><span class="badge">{MANAGER_LABEL[u.manager]}</span></td>
                  <td class="act">
                    {#if jobs.active(job)}<span class="progress"><i style="width:{job?.percent ?? 4}%"></i></span>
                    {:else}<button class="ghost small" onclick={() => run('update', [{ manager: u.manager, id: u.id, name: u.name, version: u.installed, available: u.available, origin: u.origin }])}>Update</button>{/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      {:else}
        {@const list = tab === 'search' ? (found?.items ?? []) : shown}
        {#if loading && !list.length}
          <div class="empty">Loading...</div>
        {:else if !list.length}
          <div class="empty">{tab === 'search' ? (query.trim().length < 2 ? 'Type at least two letters to search.' : 'No packages found.') : 'No packages.'}</div>
        {:else}
          <table class="t">
            <thead><tr><th>Package</th><th>{tab === 'search' ? 'Version' : 'Installed'}</th>{#if tab === 'installed'}<th>Update</th>{/if}<th>Source</th><th></th></tr></thead>
            <tbody>
              {#each list as p (p.manager + p.id)}
                {@const job = jobs.latest(p.manager, p.name)}
                {@const has = installedKeys.has(`${p.manager}|${p.id}`)}
                <tr>
                  <td><span class="name" title={p.id}>{p.name}<small>{p.id}</small></span></td>
                  <td class="mono">{p.version}</td>
                  {#if tab === 'installed'}<td class="mono">{p.available ?? ''}</td>{/if}
                  <td><span class="badge">{MANAGER_LABEL[p.manager]}</span>{#if p.origin && p.origin !== p.manager} <small class="note">{p.origin}</small>{/if}</td>
                  <td class="act">
                    {#if jobs.active(job)}
                      <span class="progress" title={job?.line}><i style="width:{job?.percent ?? 4}%"></i></span>
                      <button class="ghost small" onclick={() => job && jobs.cancel(job.id)}>Cancel</button>
                    {:else if job?.state === 'failed'}
                      <span class="badge warn" title={job.line}>Failed</span>
                    {:else if tab === 'search'}
                      {#if has || (job?.state === 'done' && job.action === 'install')}<span class="badge acc">Installed</span>
                      {:else}<button class="ghost small" onclick={() => run('install', [p])}>Install</button>{/if}
                    {:else}
                      {#if p.available}<button class="ghost small" onclick={() => run('update', [p])}>Update</button>{/if}
                      <button class="ghost small" onclick={() => (confirmRemove = p)}>Uninstall</button>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      {/if}
    </div>
  {/if}
</div>

{#if confirmRemove}
  {@const p = confirmRemove}
  <Confirm
    title="Uninstall {p.name}?"
    body="{p.name} {p.version} will be removed with {MANAGER_LABEL[p.manager]}. If it changed since this list was loaded, the removal is refused."
    oncancel={() => (confirmRemove = null)}
    choices={[{ label: 'Uninstall', danger: true, run: () => { run('remove', [p]); confirmRemove = null; } }]}
  />
{/if}

<style>
  .seg {
    display: inline-flex;
    padding: 2px;
    border-radius: 9px;
    background: var(--inset);
  }
  .seg button {
    padding: 5px 12px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--mut);
    font: inherit;
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .seg button.on {
    background: var(--surface);
    color: var(--fg);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.12);
  }
</style>
