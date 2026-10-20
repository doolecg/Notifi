<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { store } from '../lib/settings.svelte';
  import Confirm from './Confirm.svelte';
  import { bytes, message, type Proc } from './api';

  type Mode = 'end' | 'force' | 'tree' | 'restart';
  let rows = $state<Proc[]>([]);
  let loaded = $state(false);
  let query = $state('');
  let error = $state('');
  let status = $state('');
  let ending = $state<Proc | null>(null);

  const enabled = $derived(store.featureOn('killProcess'));
  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? rows.filter((r) => r.name.toLowerCase().includes(q) || String(r.pid) === q) : rows;
  });

  async function refresh() {
    try {
      rows = await invoke<Proc[]>('apps_proc_list');
      error = '';
    } catch (e) {
      error = message(e);
    } finally {
      loaded = true;
    }
  }

  async function end(p: Proc, mode: Mode) {
    ending = null;
    try {
      status = await invoke<string>('apps_proc_end', { pid: p.pid, startTime: p.startTime, mode, source: 'processes', confirmed: true });
      error = '';
    } catch (e) {
      error = message(e);
    }
    setTimeout(refresh, 600);
  }

  function ask(p: Proc) {
    if (store.values('killProcess').confirm === false && !p.protected) end(p, 'end');
    else ending = p;
  }

  $effect(() => {
    if (!enabled) return;
    refresh();
    const timer = setInterval(() => document.visibilityState === 'visible' && refresh(), 2500);
    return () => clearInterval(timer);
  });
</script>

{#if !enabled}
  <div class="view">
    <div class="scroll">
      <div class="empty">
        Kill process is off. It lets you end, force kill or restart any process you own, after a confirmation.
        <br /><br />
        <button class="ghost primary" onclick={() => store.setOption('killProcess', 'enabled', true)}>Turn it on</button>
      </div>
    </div>
  </div>
{:else}
  <div class="view">
    <div class="toolbar">
      <input type="search" placeholder="Search processes" aria-label="Search processes" bind:value={query} />
      <button class="ghost" onclick={refresh}>Refresh</button>
      <span class="grow"></span>
      <span class="note">{visible.length} processes</span>
    </div>
    {#if error}<div class="note err">{error}</div>{:else if status}<div class="note">{status}</div>{/if}

    <div class="scroll">
      {#if !loaded}
        <div class="empty">Reading processes...</div>
      {:else}
        <table class="t">
          <thead><tr><th>Process</th><th class="num">PID</th><th class="num">CPU</th><th class="num">Memory</th><th>User</th><th></th></tr></thead>
          <tbody>
            {#each visible as p (p.pid)}
              <tr>
                <td><span class="name" title={p.exe ?? ''}>{p.name}{#if p.exe}<small>{p.exe}</small>{/if}</span></td>
                <td class="num mono">{p.pid}</td>
                <td class="num mono">{p.cpu.toFixed(1)}%</td>
                <td class="num mono">{bytes(p.memory)}</td>
                <td>{p.user}</td>
                <td class="act">
                  {#if p.protected}<span class="badge" title={p.protected}>Protected</span>{/if}
                  <button class="ghost small" onclick={() => ask(p)}>End</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  </div>
{/if}

{#if ending}
  {@const p = ending}
  <Confirm
    title="End {p.name}?"
    body="PID {p.pid}. {p.protected ? 'This is a system process: ' + p.protected.toLowerCase() + '. Protected processes cannot be ended while protection is on. ' : ''}End asks it to exit; Force kill ends it at once and unsaved work is lost."
    oncancel={() => (ending = null)}
    choices={[
      { label: 'End', run: () => end(p, 'end') },
      ...(p.canRestart ? [{ label: 'Restart', run: () => end(p, 'restart') }] : []),
      { label: 'End tree', run: () => end(p, 'tree') },
      { label: 'Force kill', danger: true, run: () => end(p, 'force') },
    ]}
  />
{/if}
