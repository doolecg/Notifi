<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, tick } from 'svelte';
  import { cubicOut } from 'svelte/easing';
  import type { TransitionConfig } from 'svelte/transition';
  import { applyCssVars, duration, motion, reducedMotion, setReducedMotion } from '../lib/motion';
  import { store, type PanelTab } from '../lib/settings.svelte';
  import Icon from './Icon.svelte';
  import type { IconName } from './icons';
  import { live } from './live.svelte';
  import About from './tabs/About.svelte';
  import Controls from './tabs/Controls.svelte';
  import KeepAwake from './tabs/KeepAwake.svelte';
  import Mixer from './tabs/Mixer.svelte';
  import Network from './tabs/Network.svelte';
  import Power from './tabs/Power.svelte';
  import Storage from './tabs/Storage.svelte';
  import System from './tabs/System.svelte';
  import Utilities from './tabs/Utilities.svelte';

  let card: HTMLElement;
  let scroller: HTMLElement;
  let inner = $state<HTMLElement>();
  const buttons: Record<string, HTMLElement> = $state({});

  let chosen = $state<string | null>(null);
  let dir = $state(1);
  let highlight = $state({ x: 0, w: 0, instant: true });

  const shown = (tab: PanelTab) => {
    const s = store.settings;
    if (!s || !tab.ready || s.panel.tabs[tab.id] === false) return false;
    if (tab.feature && !store.featureOn(tab.feature)) return false;
    return tab.id !== 'power' || live.hasBattery;
  };
  const tabs = $derived.by(() => {
    const order = store.settings?.panel.order ?? [];
    const rank = (id: string) => (order.indexOf(id) + order.length + 1) % (order.length + 1);
    return (store.catalog?.panelTabs ?? []).filter(shown).sort((a, b) => rank(a.id) - rank(b.id));
  });
  const active = $derived.by(() => {
    const valid = (id: string | null | undefined) => tabs.find((t) => t.id === id);
    return valid(chosen) ?? valid(store.settings?.panel.lastTab) ?? tabs[0];
  });

  function select(id: string) {
    if (!active || id === active.id) return;
    dir = tabs.findIndex((t) => t.id === id) > tabs.findIndex((t) => t.id === active.id) ? 1 : -1;
    chosen = id;
  }

  $effect(() => {
    const theme = store.settings?.general.theme;
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
  });

  $effect(() => setReducedMotion(store.settings?.general.reducedMotion ?? false));

  $effect(() => {
    if (active) invoke('panel_set_tab', { tab: active.id });
  });

  $effect(() => {
    const id = active?.id;
    void tabs.length;
    tick().then(() => {
      const el = id ? buttons[id] : undefined;
      if (!el) return;
      highlight = { x: el.offsetLeft, w: el.offsetWidth, instant: highlight.w === 0 || reducedMotion() };
    });
  });

  function tabIn(_node: Element): TransitionConfig {
    const reduced = reducedMotion();
    return {
      duration: duration('tabSwitch'),
      easing: cubicOut,
      css: (t, u) => (reduced ? `opacity:${t}` : `opacity:${t};transform:translateX(${u * 8 * dir}px)`),
    };
  }

  function animateOpen() {
    const reduced = reducedMotion();
    const from = reduced ? { opacity: 0 } : { opacity: 0, transform: 'translateY(-6px) scale(0.96)' };
    card.getAnimations().forEach((a) => a.cancel());
    card.style.opacity = '1';
    card.animate([from, { opacity: 1, transform: 'none' }], {
      duration: duration('panelOpen'),
      easing: reduced ? 'linear' : motion.panelOpen.ease,
    });
  }

  function animateClose() {
    const reduced = reducedMotion();
    const to = reduced ? { opacity: 0 } : { opacity: 0, transform: 'scale(0.98)' };
    card.getAnimations().forEach((a) => a.cancel());
    const anim = card.animate([{ opacity: 1, transform: 'none' }, to], {
      duration: duration('panelClose'),
      easing: motion.panelClose.ease,
    });
    card.style.opacity = '0';
    anim.finished.then(() => invoke('panel_closed')).catch(() => {});
  }

  function reportHeight() {
    if (!card || !scroller || !inner) return;
    const natural = card.offsetHeight - scroller.clientHeight + inner.offsetHeight;
    invoke('panel_resize', { height: natural });
  }

  $effect(() => {
    if (!inner) return;
    const observer = new ResizeObserver(reportHeight);
    observer.observe(inner);
    observer.observe(card);
    return () => observer.disconnect();
  });

  function openSettings(section: string | null) {
    invoke('open_settings_section', { section });
    invoke('panel_hide');
  }

  onMount(() => {
    applyCssVars();
    store.init();
    live.init();
    const offs = [
      listen<{ tab: string | null; hasBattery: boolean }>('panel://open', (e) => {
        live.hasBattery = e.payload.hasBattery;
        if (e.payload.tab) chosen = e.payload.tab;
        animateOpen();
      }),
      listen('panel://close', animateClose),
    ];
    invoke<{ visible: boolean; hasBattery: boolean }>('panel_status').then((s) => {
      live.hasBattery = s.hasBattery;
      if (s.visible) animateOpen();
    });
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') invoke('panel_hide');
    };
    window.addEventListener('keydown', onKey);
    return () => {
      offs.forEach((off) => off.then((f) => f()));
      window.removeEventListener('keydown', onKey);
    };
  });
</script>

<div class="card" bind:this={card} style="opacity: 0">
  <header>
    <img class="mark" src="/icon.png" alt="Notifi" />
    <nav class="strip" aria-label="Panel tabs">
      <span
        class="highlight"
        class:instant={highlight.instant}
        style:transform="translateX({highlight.x}px)"
        style:width="{highlight.w}px"
      ></span>
      {#each tabs as tab (tab.id)}
        <button
          class="tab"
          class:active={active?.id === tab.id}
          bind:this={buttons[tab.id]}
          title={tab.label}
          aria-label={tab.label}
          aria-pressed={active?.id === tab.id}
          onclick={() => select(tab.id)}
        >
          <Icon name={tab.id as IconName} size={17} />
        </button>
      {/each}
    </nav>
    {#if active}
      <div class="title">
        <span>{active.label}</span>
        {#if active.section}
          <button class="icon-btn" title="Customise {active.label}" aria-label="Customise {active.label}" onclick={() => openSettings(active.section)}>
            <Icon name="customise" size={16} />
          </button>
        {/if}
      </div>
    {/if}
  </header>

  <div class="scroller" bind:this={scroller}>
    <div class="inner" bind:this={inner}>
      {#if store.settings && store.catalog && active}
        {#key active.id}
          <div in:tabIn>
            {#if active.id === 'keepAwake'}
              <KeepAwake />
            {:else if active.id === 'mixer'}
              <Mixer />
            {:else if active.id === 'system'}
              <System />
            {:else if active.id === 'network'}
              <Network />
            {:else if active.id === 'storage'}
              <Storage />
            {:else if active.id === 'power'}
              <Power />
            {:else if active.id === 'utilities'}
              <Utilities {openSettings} />
            {:else if active.id === 'controls'}
              <Controls />
            {:else if active.id === 'about'}
              <About />
            {/if}
          </div>
        {/key}
      {/if}
    </div>
  </div>

  <footer>
    <button class="foot-btn" onclick={() => openSettings(null)}><Icon name="settings" size={16} />Settings</button>
    <button class="foot-btn" onclick={() => invoke('quit_app')}><Icon name="quit" size={16} />Quit</button>
  </footer>
</div>

<style>
  :global(html),
  :global(body.panel-page) {
    background: transparent;
  }
  .card {
    position: absolute;
    inset: 18px;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line-strong);
    border-radius: 14px;
    background: var(--bg);
    box-shadow:
      0 12px 32px rgb(0 0 0 / 0.22),
      0 2px 6px rgb(0 0 0 / 0.12);
    overflow: hidden;
    transform-origin: 50% 100%;
  }
  header {
    flex: none;
    padding: 12px 12px 0;
  }
  .mark {
    display: block;
    width: 20px;
    height: 20px;
    margin: 0 auto 10px;
  }
  .strip {
    position: relative;
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 11px;
    background: var(--inset);
  }
  .highlight {
    position: absolute;
    top: 3px;
    bottom: 3px;
    left: 0;
    border-radius: 8px;
    background: var(--surface);
    box-shadow:
      0 1px 2px rgb(0 0 0 / 0.12),
      inset 0 -2px 0 var(--acc);
    transition:
      transform var(--m-selectionMove-dur) var(--m-selectionMove-ease),
      width var(--m-selectionMove-dur) var(--m-selectionMove-ease);
    pointer-events: none;
  }
  .highlight.instant {
    transition: none;
  }
  .tab {
    position: relative;
    flex: 1;
    display: grid;
    place-items: center;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--mut);
    cursor: pointer;
    transition: color var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
  .tab:hover:not(.active) {
    color: var(--fg-soft);
    background: var(--hover);
  }
  .tab.active {
    color: var(--acc);
  }
  .tab:focus-visible,
  .icon-btn:focus-visible,
  .foot-btn:focus-visible {
    outline: 2px solid var(--acc);
    outline-offset: 1px;
  }
  .title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 34px;
    margin-top: 6px;
    padding: 0 2px 0 4px;
    color: var(--mut);
    font-size: 11.5px;
    font-weight: 700;
    letter-spacing: 0.07em;
    text-transform: uppercase;
  }
  .icon-btn {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--mut);
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .icon-btn:hover {
    background: var(--hover);
    color: var(--fg);
  }
  .scroller {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .inner {
    display: flow-root;
    padding: 0 12px 12px;
  }
  footer {
    flex: none;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    padding: 10px 12px 12px;
    border-top: 1px solid var(--line);
  }
  .foot-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    height: 34px;
    border: 1px solid var(--line-strong);
    border-radius: 9px;
    background: var(--surface);
    color: var(--fg-soft);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .foot-btn:hover {
    background: var(--hover);
    color: var(--fg);
  }

  /* Shared by the tabs. */
  :global(.p-card) {
    border: 1px solid var(--line);
    border-radius: 11px;
    background: var(--surface);
  }
  :global(.p-card + .p-card),
  :global(.p-card + .p-head),
  :global(.p-tiles + .p-head),
  :global(.p-tiles + .p-card) {
    margin-top: 10px;
  }
  :global(.p-head) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 0 4px 6px;
    color: var(--mut);
    font-size: 11.5px;
    font-weight: 600;
  }
  :global(.p-row) {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 8px 12px;
  }
  :global(.p-row + .p-row),
  :global(.p-row + .p-more),
  :global(.p-more + .p-row) {
    border-top: 1px solid var(--line);
  }
  :global(.p-glyph) {
    display: grid;
    place-items: center;
    flex: none;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    background: var(--acc-soft);
    color: var(--acc);
  }
  :global(.p-main) {
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
    gap: 4px;
  }
  :global(.p-line) {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    font-size: 13px;
  }
  :global(.p-label) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  :global(.p-value) {
    flex: none;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  :global(.p-sub) {
    color: var(--mut);
    font-size: 11.5px;
  }
  :global(.p-toggle) {
    width: 100%;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  :global(.p-toggle:hover) {
    background: var(--hover);
  }
  :global(.p-empty) {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 28px 16px;
    color: var(--mut);
    font-size: 13px;
    text-align: center;
  }
  :global(.p-tiles) {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(0, 1fr));
    gap: 8px;
  }
  :global(.p-tile) {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: 11px;
    background: var(--surface);
  }
  :global(.p-tile .p-value) {
    font-size: 17px;
  }
</style>
