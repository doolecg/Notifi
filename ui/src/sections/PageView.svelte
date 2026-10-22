<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { emit } from '@tauri-apps/api/event';
  import { tick } from 'svelte';
  import Switch from '../lib/Switch.svelte';
  import { fadeSlide, motion, reducedMotion } from '../lib/motion';
  import { namespaceOf, store, type Card, type Page } from '../lib/settings.svelte';
  import OptionRow from './OptionRow.svelte';
  import { customFor } from './customs';

  /** `focus` is `namespace.key` of an option to scroll to and highlight. */
  let {
    page,
    focus = null,
    goto,
    onreset,
  }: { page: Page; focus?: string | null; goto: (page: string, option?: string) => void; onreset: () => void } = $props();

  let tab = $state('');
  let highlighted = $state<string | null>(null);
  let content: HTMLElement;
  const tabIndex = $derived(Math.max(0, page.tabs.indexOf(tab)));
  const pageOn = $derived(!page.feature || store.featureOn(page.id));
  const cards = $derived(page.tabs.length ? page.cards.filter((c) => c.tab === (tab || page.tabs[0])) : page.cards);
  const namespaces = $derived([...new Set(page.cards.map((c) => namespaceOf(page, c)))]);
  const resettable = $derived(namespaces.filter((ns) => ['general', 'panel', 'island'].includes(ns) || ns in (store.settings?.features ?? {})));
  /** These pages are the tool itself, so there is no separate window to open. */
  const NO_OPEN = new Set(['uninstaller', 'cleaner', 'displays']);
  const openable = $derived(page.tool && !NO_OPEN.has(page.id) && pageOn);

  $effect(() => {
    void page.id;
    tab = page.tabs[0] ?? '';
  });

  $effect(() => {
    if (focus) show(focus);
  });

  async function show(target: string) {
    const card = page.cards.find((c) => c.options.some((o) => `${namespaceOf(page, c)}.${o.id}` === target));
    if (card?.tab) tab = card.tab;
    await tick();
    const q = CSS.escape(target);
    const el = content?.querySelector<HTMLElement>(`[data-opt="${q}"], [data-opts~="${q}"]`);
    if (!el) return;
    el.scrollIntoView({ block: 'center', behavior: reducedMotion() ? 'auto' : 'smooth' });
    highlighted = target;
    setTimeout(() => highlighted === target && (highlighted = null), 1600);
  }

  async function reset() {
    for (const ns of resettable) await store.reset(ns);
    onreset();
  }

  const cardOn = (card: Card) => !card.feature || store.featureOn(card.feature);
  /** On a custom page, a card with a custom option is drawn by that option's component. */
  const drawnByCustom = (card: Card) => page.custom && card.options.some((o) => o.control.kind === 'custom');
  const optIds = (card: Card) => card.options.map((o) => `${namespaceOf(page, card)}.${o.id}`).join(' ');

  /** The Island has its own preview command; other tools are opened by whoever listens for the event. */
  function openFeature() {
    if (page.id === 'island') invoke('island_preview').catch(() => {});
    else emit('settings://open-feature', page.id);
  }
</script>

<header class="head">
  <div>
    <h1>{page.label}</h1>
    <p>{page.blurb}</p>
  </div>
  {#if page.feature}
    {#if pageOn}
      <button class="ghost" onclick={() => store.uninstall(page.id)}>Uninstall</button>
    {:else}
      <button class="install" onclick={() => store.install(page.id)}>Install</button>
    {/if}
  {/if}
</header>

{#if page.feature && !pageOn}
  <p class="notice">Not installed, so it is not running. Install it to use it. The options below are saved and stay if you uninstall it later.</p>
{/if}

{#if page.tabs.length}
  <div class="tabbar">
    <div class="segmented" role="tablist" style:--n={page.tabs.length} style:--i={tabIndex}>
      <span class="thumb"></span>
      {#each page.tabs as t (t)}
        <button role="tab" aria-selected={t === (tab || page.tabs[0])} onclick={() => (tab = t)}>{t}</button>
      {/each}
    </div>
    {#if openable}
      <button class="ghost" onclick={openFeature}>Open</button>
    {/if}
  </div>
{:else if openable}
  <div class="tabbar end"><button class="ghost" onclick={openFeature}>Open {page.label}</button></div>
{/if}

<div class="content" class:off={!pageOn} inert={!pageOn} bind:this={content}>
  {#key tab}
    <div in:fadeSlide={{ token: 'tabSwitch' }}>
      {#if !cards.length}
        <p class="empty">The switch above is all there is to set.</p>
      {/if}
      {#each cards as card, i (card.title)}
        {@const ns = namespaceOf(page, card)}
        {@const hasStored = card.options.some((o) => o.control.kind !== 'custom')}
        <section in:fadeSlide={{ token: 'listReorder', delay: i * (motion.listReorder.stagger ?? 0) }}>
          {#if drawnByCustom(card) && !hasStored}
            {#each card.options as option (option.id)}
              {@const Custom = customFor(page, option)}
              {#if Custom}
                <Custom {page} {card} {option} values={store.values(ns)} set={(k: string, v: unknown) => store.setOption(ns, k, v)} {goto} />
              {/if}
            {/each}
          {:else}
            <div class="card-head">
              <h3 class="group-title">{card.title}</h3>
              {#if card.feature}
                <Switch checked={cardOn(card)} label="{card.title} on or off" onchange={(on) => store.setOption(card.feature, 'enabled', on)} />
              {/if}
            </div>
            <div class="card" class:off={!cardOn(card)} inert={!cardOn(card)} class:flash={drawnByCustom(card) && !!highlighted && optIds(card).split(' ').includes(highlighted)} data-opts={optIds(card)}>
              {#each card.options as option (option.id)}
                {#if drawnByCustom(card)}
                  {#if option.control.kind === 'custom'}
                    {@const Custom = customFor(page, option)}
                    {#if Custom}
                      <div class="opt" data-opt="{ns}.{option.id}">
                        <Custom {page} {card} {option} values={store.values(ns)} set={(k: string, v: unknown) => store.setOption(ns, k, v)} {goto} />
                      </div>
                    {/if}
                  {/if}
                {:else}
                  <div class="opt" class:flash={highlighted === `${ns}.${option.id}`} data-opt="{ns}.{option.id}">
                    <OptionRow {page} {card} {option} namespace={ns} {goto} />
                  </div>
                {/if}
              {/each}
            </div>
          {/if}
        </section>
      {/each}
      {#if resettable.length}
        <button class="reset" onclick={reset}>Reset this page</button>
      {/if}
    </div>
  {/key}
</div>

<style>
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 18px;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 26px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }
  .head p {
    max-width: 560px;
    margin: 0;
    color: var(--mut);
    font-size: 13.5px;
  }
  .tabbar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 18px;
  }
  .tabbar.end {
    justify-content: flex-end;
  }
  .segmented {
    position: relative;
    display: grid;
    flex: 1;
    grid-template-columns: repeat(var(--n), 1fr);
    padding: 3px;
    border-radius: 9px;
    background: var(--inset);
  }
  .thumb {
    position: absolute;
    inset: 3px auto 3px 3px;
    width: calc((100% - 6px) / var(--n));
    border-radius: 7px;
    background: var(--surface);
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.12);
    transform: translateX(calc(var(--i) * 100%));
    transition: transform var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
  .segmented button {
    position: relative;
    padding: 5px 10px;
    border: 0;
    background: none;
    color: var(--mut);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .segmented button[aria-selected='true'] {
    color: var(--fg);
    font-weight: 600;
  }
  .content {
    transition: opacity var(--m-tabSwitch-dur) var(--m-tabSwitch-ease);
  }
  .off {
    opacity: 0.45;
  }
  .card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 22px 4px 8px;
  }
  .card-head .group-title {
    margin: 0;
  }
  section:first-child .card-head {
    margin-top: 0;
  }
  .opt + .opt {
    border-top: 1px solid var(--line);
  }
  .opt {
    transition: background-color var(--m-tooltipIn-dur) var(--m-tooltipIn-ease);
  }
  .card {
    transition: background-color var(--m-tooltipIn-dur) var(--m-tooltipIn-ease);
  }
  .opt.flash,
  .card.flash {
    background: var(--acc-soft);
  }
  .empty {
    color: var(--mut);
    font-size: 13px;
  }
  .reset {
    display: block;
    margin: 22px auto 0;
    padding: 4px 8px;
    border: 0;
    background: none;
    color: var(--mut);
    font: inherit;
    font-size: 12.5px;
    text-decoration: underline;
    cursor: pointer;
  }
  .reset:hover {
    color: var(--fg);
  }
  .install {
    flex: none;
    padding: 6px 16px;
    border: 1px solid var(--acc);
    border-radius: 8px;
    background: var(--acc);
    color: var(--acc-fg);
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
  .install:hover {
    filter: brightness(1.08);
  }
  .notice {
    margin: 0 0 12px;
    padding: 9px 12px;
    border-radius: 9px;
    background: var(--acc-soft);
    color: var(--fg);
    font-size: 13px;
  }
</style>
