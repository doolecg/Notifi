<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { store } from '../lib/settings.svelte';
  import Confirm from './Confirm.svelte';
  import { message, type Socket } from './api';

  type SortKey = 'port' | 'proto' | 'address' | 'process' | 'pid' | 'user';
  let rows = $state<Socket[]>([]);
  let loaded = $state(false);
  let query = $state('');
  let sort = $state<{ key: SortKey; dir: 1 | -1 }>({ key: 'port', dir: 1 });
  let error = $state('');
  let status = $state('');
  let stopping = $state<Socket | null>(null);
  let menu = $state<{ x: number; y: number; row: Socket } | null>(null);

  const settings = $derived(store.values('portManager'));
  const seconds = $derived(Math.max(1, Number(settings.refresh ?? 3)));
  const killOn = $derived(store.featureOn('killProcess'));

  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = q ? rows.filter((r) => String(r.port) === q || String(r.port).startsWith(q) || r.process.toLowerCase().includes(q) || String(r.pid) === q) : rows;
    const { key, dir } = sort;
    return [...list].sort((a, b) => {
      const x = a[key] ?? -1;
      const y = b[key] ?? -1;
      return (typeof x === 'number' && typeof y === 'number' ? x - y : String(x).localeCompare(String(y))) * dir || a.port - b.port;
    });
  });

  async function refresh() {
    try {
      rows = await invoke<Socket[]>('apps_ports');
      error = '';
    } catch (e) {
      error = message(e);
    } finally {
      loaded = true;
    }
  }

  function sortBy(key: SortKey) {
    sort = { key, dir: sort.key === key ? (sort.dir === 1 ? -1 : 1) : 1 };
  }

  const arrow = (key: SortKey) => (sort.key === key ? (sort.dir === 1 ? ' ▲' : ' ▼') : '');
  const hostPort = (r: Socket) => (r.address.includes(':') ? `[${r.address}]:${r.port}` : `${r.address}:${r.port}`);

  async function copy(text: string) {
    menu = null;
    await navigator.clipboard.writeText(text).catch(() => {});
  }

  async function openInBrowser(r: Socket) {
    menu = null;
    const host = r.allInterfaces ? 'localhost' : r.address.includes(':') ? `[${r.address}]` : r.address;
    await invoke('apps_open_url', { url: `http://${host}:${r.port}` }).catch((e) => (error = message(e)));
  }

  async function stop(r: Socket, mode: 'end' | 'force' | 'tree') {
    stopping = null;
    if (r.pid == null) return;
    try {
      status = await invoke<string>('apps_proc_end', { pid: r.pid, startTime: r.startTime, mode, source: 'ports', confirmed: true });
      error = '';
    } catch (e) {
      error = message(e);
    }
    setTimeout(refresh, 600);
  }

  function ask(r: Socket) {
    menu = null;
    if (!(settings.confirmStop ?? true) && !r.protected) stop(r, 'end');
    else stopping = r;
  }

  onMount(() => {
    refresh();
    let timer: ReturnType<typeof setTimeout>;
    const tick = async () => {
      if (document.visibilityState === 'visible') await refresh();
      timer = setTimeout(tick, seconds * 1000);
    };
    timer = setTimeout(tick, seconds * 1000);
    return () => clearTimeout(timer);
  });
</script>

<svelte:window onclick={() => (menu = null)} />

<div class="view">
  <div class="toolbar">
    <input type="search" placeholder="Search by port or process" aria-label="Search ports" bind:value={query} />
    <button class="ghost" onclick={refresh}>Refresh</button>
    <span class="grow"></span>
    <span class="note">{visible.length} listening, refreshed every {seconds} s</span>
  </div>
  {#if error}<div class="note err">{error}</div>{:else if status}<div class="note">{status}</div>{/if}

  <div class="scroll">
    {#if !loaded}
      <div class="empty">Reading sockets...</div>
    {:else if !visible.length}
      <div class="empty">No matching listeners.</div>
    {:else}
      <table class="t">
        <thead>
          <tr>
            <th class="sort num" onclick={() => sortBy('port')}>Port{arrow('port')}</th>
            <th class="sort" onclick={() => sortBy('proto')}>Proto{arrow('proto')}</th>
            <th class="sort" onclick={() => sortBy('address')}>Address{arrow('address')}</th>
            <th class="sort" onclick={() => sortBy('process')}>Process{arrow('process')}</th>
            <th class="sort num" onclick={() => sortBy('pid')}>PID{arrow('pid')}</th>
            <th class="sort" onclick={() => sortBy('user')}>User{arrow('user')}</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {#each visible as r (r.proto + r.address + r.port + r.pid)}
            <tr oncontextmenu={(e) => { e.preventDefault(); menu = { x: e.clientX, y: e.clientY, row: r }; }}>
              <td class="num mono">{r.port}</td>
              <td>{r.proto}</td>
              <td class="mono">
                {r.address}
                {#if r.allInterfaces}<span class="badge warn" title="Reachable from other machines">All interfaces</span>{/if}
              </td>
              <td>{r.process || 'unknown'}</td>
              <td class="num mono">{r.pid ?? ''}</td>
              <td>{r.user}</td>
              <td class="act">
                <button class="ghost small" onclick={(e) => { e.stopPropagation(); menu = { x: e.clientX - 170, y: e.clientY + 8, row: r }; }}>...</button>
                <button class="ghost small" disabled={r.pid == null} title={r.pid == null ? 'No process found for this socket' : ''} onclick={() => ask(r)}>Stop</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
</div>

{#if menu}
  {@const row = menu.row}
  <div class="menu" role="menu" style="left:{Math.max(8, menu.x)}px;top:{menu.y}px">
    <button role="menuitem" onclick={() => copy(String(row.port))}>Copy port</button>
    <button role="menuitem" disabled={row.pid == null} onclick={() => copy(String(row.pid))}>Copy PID</button>
    <button role="menuitem" onclick={() => copy(hostPort(row))}>Copy address</button>
    {#if row.proto === 'TCP'}<button role="menuitem" onclick={() => openInBrowser(row)}>Open in browser</button>{/if}
  </div>
{/if}

{#if stopping}
  {@const r = stopping}
  <Confirm
    title="Stop {r.process || 'this process'}?"
    body="{r.process} (PID {r.pid}) holds port {r.port}. {r.protected ? 'It is a system process: ' + r.protected.toLowerCase() + '. ' : ''}Stop asks it to exit first; Force kill ends it at once and unsaved work is lost."
    oncancel={() => (stopping = null)}
    choices={[
      { label: 'Stop', run: () => stop(r, 'end') },
      ...(killOn ? [{ label: 'End process tree', run: () => stop(r, 'tree') }] : []),
      { label: 'Force kill', danger: true, run: () => stop(r, 'force') },
    ]}
  />
{/if}
