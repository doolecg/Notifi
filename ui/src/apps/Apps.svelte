<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars, fadeSlide, setReducedMotion } from '../lib/motion';
  import { store } from '../lib/settings.svelte';
  import { jobs } from './jobs.svelte';
  import Packages from './Packages.svelte';
  import Ports from './Ports.svelte';
  import Processes from './Processes.svelte';
  import Space from './Space.svelte';
  import Updates from './Updates.svelte';
  import './view.css';

  const TABS = [
    { id: 'updates', label: 'App updates' },
    { id: 'packages', label: 'Package manager' },
    { id: 'ports', label: 'Ports' },
    { id: 'processes', label: 'Processes' },
    { id: 'space', label: 'Disk space' },
  ];

  let view = $state(location.hash.slice(1) || 'updates');
  const active = $derived(TABS.some((t) => t.id === view) ? view : 'updates');

  $effect(() => {
    const theme = store.settings?.general.theme;
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
  });
  $effect(() => {
    setReducedMotion(store.settings?.general.reducedMotion ?? false);
    applyCssVars();
  });

  onMount(() => {
    applyCssVars();
    store.init().catch(() => {});
    jobs.init();
    const off = listen<string>('apps://view', (e) => (view = e.payload));
    return () => off.then((f) => f());
  });
</script>

<div class="shell">
  <nav aria-label="App tools">
    <div role="tablist" class="tabs">
      {#each TABS as t}
        <button role="tab" aria-selected={active === t.id} class:on={active === t.id} onclick={() => (view = t.id)}>{t.label}</button>
      {/each}
    </div>
    {#if jobs.running.length}
      <span class="busy">{jobs.running.length} running</span>
    {/if}
  </nav>
  <main>
    {#key active}
      <div class="pane" in:fadeSlide={{ token: 'tabSwitch', y: 0 }}>
        {#if active === 'updates'}
          <Updates />
        {:else if active === 'packages'}
          <Packages />
        {:else if active === 'ports'}
          <Ports />
        {:else if active === 'processes'}
          <Processes />
        {:else}
          <Space />
        {/if}
      </div>
    {/key}
  </main>
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg);
    color: var(--fg);
    font-family: var(--sans);
  }
  nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px 0;
    border-bottom: 1px solid var(--line);
    background: var(--sidebar);
  }
  .tabs {
    display: flex;
    gap: 4px;
  }
  .tabs button {
    padding: 9px 14px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--mut);
    font: inherit;
    font-size: 13.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .tabs button:hover {
    color: var(--fg);
  }
  .tabs button.on {
    border-bottom-color: var(--acc);
    color: var(--fg);
  }
  .busy {
    margin-bottom: 8px;
    padding: 3px 10px;
    border-radius: 99px;
    background: var(--acc-soft);
    color: var(--acc);
    font-size: 12px;
    font-weight: 600;
  }
  main {
    flex: 1;
    min-height: 0;
    padding: 16px 20px 20px;
  }
  .pane {
    height: 100%;
  }
</style>
