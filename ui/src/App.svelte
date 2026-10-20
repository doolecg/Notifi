<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { Spring } from 'svelte/motion';
  import { applyCssVars, fadeSlide, reducedMotion, setReducedMotion, springOptions } from './lib/motion';
  import { store } from './lib/settings.svelte';
  import { listen } from '@tauri-apps/api/event';
  import FeatureGroup from './sections/FeatureGroup.svelte';
  import General from './sections/General.svelte';
  import PanelSection from './sections/PanelSection.svelte';
  import Permissions from './sections/Permissions.svelte';
  import Shortcuts from './sections/Shortcuts.svelte';

  const FIXED = {
    general: { label: 'General', blurb: 'Startup, appearance and updates.' },
    panel: { label: 'Tray panel', blurb: 'What the panel shows when you click the tray icon.' },
    shortcuts: { label: 'Shortcuts', blurb: 'Global keys that work from any app.' },
    permissions: { label: 'Permissions', blurb: 'What Notifi needs from your system, and why.' },
  };

  let active = $state('general');
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout>;
  const items: Record<string, HTMLElement> = {};
  const indicator = new Spring({ y: 0, h: 0 }, springOptions('selectionMove'));

  const groups = $derived(store.catalog?.groups ?? []);
  const group = $derived(groups.find((g) => g.id === active));
  const title = $derived(group?.label ?? FIXED[active as keyof typeof FIXED]?.label ?? '');
  const enabled = (id: string) => Object.values(store.settings?.features[id] ?? {}).filter(Boolean).length;

  $effect(() => {
    const theme = store.settings?.general.theme;
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
  });

  $effect(() => setReducedMotion(store.settings?.general.reducedMotion ?? false));

  $effect(() => {
    void active;
    void groups.length;
    tick().then(() => {
      const el = items[active];
      if (!el) return;
      const first = indicator.target.h === 0;
      indicator.set({ y: el.offsetTop, h: el.offsetHeight }, { instant: first || reducedMotion() });
    });
  });

  async function resetSection() {
    await store.reset(active);
    clearTimeout(toastTimer);
    toast = `${title} is back to its defaults`;
    toastTimer = setTimeout(() => (toast = null), 2400);
  }

  const goto = (section: string) => {
    if (section) active = section;
  };

  onMount(() => {
    applyCssVars();
    store.init();
    goto(decodeURIComponent(location.hash.slice(1)));
    const off = listen<string>('settings://goto', (e) => goto(e.payload));
    return () => off.then((f) => f());
  });
</script>

{#snippet navItem(id: string, label: string, count = 0)}
  <button class="nav-item" class:active={active === id} bind:this={items[id]} onclick={() => (active = id)}>
    <span>{label}</span>
    {#if count}<span class="count">{count}</span>{/if}
  </button>
{/snippet}

<div class="app">
  <aside class="sidebar">
    <div class="brand">
      <img src="/icon.png" alt="" />
      <span class="brand-name">Notifi</span>
      <span class="version">0.1.0</span>
    </div>
    <nav>
      <span class="indicator" style:transform="translateY({indicator.current.y}px)" style:height="{indicator.current.h}px"></span>
      {@render navItem('general', 'General')}
      {@render navItem('panel', 'Tray panel')}
      <p class="nav-label">Features</p>
      {#each groups as g (g.id)}
        {@render navItem(g.id, g.label, enabled(g.id))}
      {/each}
      <p class="nav-label">Setup</p>
      {@render navItem('shortcuts', 'Shortcuts')}
      {@render navItem('permissions', 'Permissions')}
    </nav>
  </aside>

  <main>
    {#if store.settings && store.catalog}
      {#key active}
        <div class="page" in:fadeSlide={{ token: 'tabSwitch' }}>
          <header>
            <div>
              <h1>{title}</h1>
              <p>
                {#if group}
                  {enabled(group.id)} of {group.features.length} on. Turn on only what you use; features that are off don't run.
                {:else}
                  {FIXED[active as keyof typeof FIXED].blurb}
                {/if}
              </p>
            </div>
            {#if active !== 'permissions'}
              <button class="ghost" onclick={resetSection}>Reset section</button>
            {/if}
          </header>
          {#if group}
            <FeatureGroup {group} />
          {:else if active === 'general'}
            <General />
          {:else if active === 'panel'}
            <PanelSection />
          {:else if active === 'shortcuts'}
            <Shortcuts />
          {:else}
            <Permissions />
          {/if}
        </div>
      {/key}
    {/if}
  </main>

  {#if toast}
    <div class="toast" role="status" in:fadeSlide={{ token: 'toastIn', y: 12 }} out:fadeSlide={{ token: 'toastOut', y: 12 }}>{toast}</div>
  {/if}
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 232px 1fr;
    height: 100vh;
  }
  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 18px 12px;
    border-right: 1px solid var(--line);
    background: var(--sidebar);
    overflow-y: auto;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 8px 4px;
  }
  .brand img {
    width: 28px;
    height: 28px;
  }
  .brand-name {
    font-size: 16px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .version {
    margin-left: auto;
    color: var(--mut);
    font: 11px var(--mono);
  }
  nav {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .indicator {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    border-radius: 8px;
    background: var(--acc-soft);
    box-shadow: inset 3px 0 0 var(--acc);
    pointer-events: none;
  }
  .nav-label {
    margin: 14px 10px 4px;
    color: var(--mut);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .nav-item {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 7px 10px 7px 14px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--fg-soft);
    font: inherit;
    font-size: 13.5px;
    text-align: left;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .nav-item:hover:not(.active) {
    background: var(--hover);
  }
  .nav-item.active {
    color: var(--fg);
    font-weight: 600;
  }
  .count {
    min-width: 20px;
    padding: 0 6px;
    border-radius: 99px;
    background: var(--acc);
    color: var(--acc-fg);
    font-size: 11px;
    font-weight: 700;
    line-height: 18px;
    text-align: center;
  }
  main {
    overflow-y: auto;
  }
  .page {
    max-width: 720px;
    margin: 0 auto;
    padding: 30px 32px 48px;
  }
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 20px;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 24px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }
  header p {
    margin: 0;
    color: var(--mut);
    font-size: 13.5px;
  }
  .toast {
    position: fixed;
    left: 50%;
    bottom: 24px;
    translate: -50% 0;
    padding: 9px 16px;
    border-radius: 10px;
    background: var(--toast);
    color: var(--toast-fg);
    font-size: 13px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.18);
  }
</style>
