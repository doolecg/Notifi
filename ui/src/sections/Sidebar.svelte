<script lang="ts">
  import { tick } from 'svelte';
  import { Spring } from 'svelte/motion';
  import { reducedMotion, springOptions } from '../lib/motion';
  import { namespaceOf, onPlatform, store, type Page } from '../lib/settings.svelte';
  import Icon from '../panel/Icon.svelte';
  import PageIcon from './PageIcon.svelte';

  let { active, goto }: { active: string; goto: (page: string, option?: string) => void } = $props();

  interface Match {
    page: Page;
    group: string;
    /** `namespace.key` and label of the option that matched, when it wasn't the page name. */
    option?: { target: string; label: string };
  }

  const COLLAPSED_KEY = 'notifi.sidebar.collapsed';
  let query = $state('');
  let collapsed = $state<Record<string, boolean>>(load());
  let nav: HTMLElement;
  const rows: Record<string, HTMLElement> = $state({});
  const indicator = new Spring({ y: 0, h: 0, show: 0 }, springOptions('selectionMove'));

  const groups = $derived(
    (store.catalog?.groups ?? []).map((g) => ({ ...g, pages: g.pages.filter(onPlatform) })).filter((g) => g.pages.length),
  );
  /** The group (and so the colour) each page belongs to. */
  const groupOf = $derived(Object.fromEntries(groups.flatMap((g) => g.pages.map((p) => [p.id, g.id]))));
  /** Installed and Install features sit above everything, then the core pages, then only what you have installed. */
  const pinned = $derived(['installed', 'features'].map((id) => store.page(id)).filter((p): p is Page => !!p));
  const installedCount = $derived(store.featureGroups().reduce((n, g) => n + g.pages.filter((p) => store.installed(p.id)).length, 0));
  const sections = $derived([{ id: 'core', label: 'Core', pages: store.corePages() }]);
  /** The sidebar row that is lit: a feature page opened from the lists lights Installed (or Install features). */
  const lit = $derived.by(() => {
    const page = store.page(active);
    if (!page?.feature || page.id === 'island') return active;
    return store.installed(page.id) ? 'installed' : 'features';
  });

  const matches = $derived.by((): Match[] => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    const out: Match[] = [];
    for (const g of groups) {
      for (const page of g.pages) {
        if (page.label.toLowerCase().includes(q)) {
          out.push({ page, group: g.id });
          continue;
        }
        for (const card of page.cards) {
          const hit = card.options.find((o) => o.label.toLowerCase().includes(q)) ?? (card.title.toLowerCase().includes(q) ? card.options[0] : undefined);
          if (hit) {
            out.push({ page, group: g.id, option: { target: `${namespaceOf(page, card)}.${hit.id}`, label: hit.label } });
            break;
          }
        }
      }
    }
    return out;
  });

  function load(): Record<string, boolean> {
    try {
      return JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? '{}');
    } catch {
      return {};
    }
  }

  function toggle(id: string) {
    collapsed = { ...collapsed, [id]: !collapsed[id] };
    try {
      localStorage.setItem(COLLAPSED_KEY, JSON.stringify(collapsed));
    } catch {}
  }

  function open(m: Match) {
    goto(m.page.id, m.option?.target);
    query = '';
  }

  $effect(() => {
    void lit;
    void query;
    void collapsed;
    void groups.length;
    tick().then(() => {
      const el = rows[lit];
      if (!el || !nav?.contains(el)) {
        indicator.set({ ...indicator.target, show: 0 }, { instant: true });
        return;
      }
      const instant = indicator.target.show === 0 || reducedMotion();
      indicator.set({ y: el.offsetTop, h: el.offsetHeight, show: 1 }, { instant });
    });
  });
</script>

<aside class="sidebar">
  <div class="search">
    <input
      type="search"
      placeholder="Search settings"
      aria-label="Search settings"
      bind:value={query}
      onkeydown={(e) => {
        if (e.key === 'Enter' && matches[0]) open(matches[0]);
        if (e.key === 'Escape') query = '';
      }}
    />
  </div>

  <nav bind:this={nav}>
    {#if query.trim()}
      {#each matches as m (m.page.id)}
        <button class="item match" onclick={() => open(m)}>
          <PageIcon page={m.page} group={m.group} size={22} />
          <span class="text">
            <span>{m.page.label}</span>
            {#if m.option}<small>{m.option.label}</small>{:else if m.page.feature && !store.installed(m.page.id)}<small>Not installed</small>{/if}
          </span>
        </button>
      {:else}
        <p class="none">Nothing matches "{query}".</p>
      {/each}
    {:else}
      <span
        class="indicator"
        style:transform="translateY({indicator.current.y}px)"
        style:height="{indicator.current.h}px"
        style:opacity={indicator.current.show}
      ></span>
      {#each pinned as page (page.id)}
        <button class="item pinned" class:active={lit === page.id} bind:this={rows[page.id]} onclick={() => goto(page.id)}>
          <PageIcon {page} group="essentials" size={22} />
          <span class="text"><span>{page.label}</span></span>
          {#if page.id === 'installed'}<span class="badge">{installedCount}</span>{/if}
        </button>
      {/each}
      {#each sections as g (g.id)}
        <button class="group" aria-expanded={!collapsed[g.id]} onclick={() => toggle(g.id)}>
          <span class="chev" class:open={!collapsed[g.id]}><Icon name="chevron" size={11} /></span>
          {g.label}
        </button>
        {#if !collapsed[g.id]}
          {#each g.pages as page (page.id)}
            <button class="item" class:active={lit === page.id} bind:this={rows[page.id]} onclick={() => goto(page.id)}>
              <PageIcon {page} group={groupOf[page.id] ?? g.id} size={22} />
              <span class="text"><span>{page.label}</span></span>
            </button>
          {/each}
        {/if}
      {/each}
    {/if}
  </nav>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--line);
    background: var(--sidebar);
  }
  .search {
    padding: 14px 12px 8px;
  }
  .search input {
    width: 100%;
    padding: 7px 10px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    font-size: 13px;
    user-select: text;
  }
  .search input:focus {
    outline: 2px solid var(--acc-soft);
    border-color: var(--acc);
  }
  nav {
    position: relative;
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 1px;
    padding: 4px 10px 16px;
    overflow-y: auto;
  }
  .indicator {
    position: absolute;
    top: 0;
    left: 10px;
    right: 10px;
    border-radius: 7px;
    background: var(--acc);
    pointer-events: none;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 5px;
    margin: 12px 0 3px;
    padding: 2px 6px;
    border: 0;
    background: none;
    color: var(--mut);
    font: inherit;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    font-variant: small-caps;
    cursor: pointer;
  }
  .group:first-of-type {
    margin-top: 2px;
  }
  .chev {
    display: inline-flex;
    transition: transform var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 5px 8px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--fg-soft);
    font: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
    transition:
      background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease),
      color var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
  .item:hover:not(.active) {
    background: var(--hover);
  }
  .item.active {
    color: var(--acc-fg);
    font-weight: 600;
  }
  .item.pinned {
    margin-bottom: 1px;
  }
  .badge {
    margin-left: auto;
    padding: 0 7px;
    border-radius: 99px;
    background: var(--hover);
    color: var(--mut);
    font-size: 11px;
    font-weight: 600;
  }
  .item.active .badge {
    background: rgb(255 255 255 / 0.22);
    color: var(--acc-fg);
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .text small {
    color: var(--mut);
    font-size: 11.5px;
  }
  .none {
    margin: 12px 8px;
    color: var(--mut);
    font-size: 12.5px;
  }
</style>
