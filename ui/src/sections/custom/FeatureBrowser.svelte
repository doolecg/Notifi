<script lang="ts">
  import { fadeSlide, motion } from '../../lib/motion';
  import { store, type Page } from '../../lib/settings.svelte';
  import Icon from '../../panel/Icon.svelte';
  import PageIcon from '../PageIcon.svelte';
  import type { CustomProps } from '../customs';

  /** The Installed page lists what you have; the Install features page lists everything. Both open a feature's settings on click. */
  let { page, goto }: CustomProps = $props();

  const mode = $derived(page.id === 'installed' ? 'installed' : 'store');
  const VIEW_KEY = 'notifi.features.view';
  let view = $state<'list' | 'grid'>(loadView());
  let query = $state('');
  let kind = $state('all');
  let undo = $state<{ id: string; label: string } | null>(null);
  let undoTimer: ReturnType<typeof setTimeout> | undefined;

  const groups = $derived(store.featureGroups());
  const all = $derived(groups.flatMap((g) => g.pages));
  const installedCount = $derived(all.filter((p) => store.installed(p.id)).length);
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return groups
      .map((g) => ({
        ...g,
        pages: g.pages.filter(
          (p) =>
            (mode === 'store' || store.installed(p.id)) &&
            (kind === 'all' || g.id === kind) &&
            (!q || p.label.toLowerCase().includes(q) || p.blurb.toLowerCase().includes(q)),
        ),
      }))
      .filter((g) => g.pages.length);
  });

  function loadView(): 'list' | 'grid' {
    try {
      return localStorage.getItem(VIEW_KEY) === 'grid' ? 'grid' : 'list';
    } catch {
      return 'list';
    }
  }

  function setView(v: 'list' | 'grid') {
    view = v;
    try {
      localStorage.setItem(VIEW_KEY, v);
    } catch {}
  }

  function uninstall(p: Page) {
    store.uninstall(p.id);
    clearTimeout(undoTimer);
    undo = { id: p.id, label: p.label };
    undoTimer = setTimeout(() => (undo = null), 7000);
  }

  function restore() {
    if (undo) store.install(undo.id);
    undo = null;
  }
</script>

<div class="tools">
  <input type="search" placeholder={mode === 'store' ? 'Search features' : 'Search installed features'} aria-label="Search features" bind:value={query} />
  <div class="seg" role="group" aria-label="View">
    <button class:on={view === 'list'} aria-pressed={view === 'list'} onclick={() => setView('list')}>List</button>
    <button class:on={view === 'grid'} aria-pressed={view === 'grid'} onclick={() => setView('grid')}>Grid</button>
  </div>
</div>

{#if mode === 'store'}
  <div class="chips" role="group" aria-label="Kind of feature">
    <button class:on={kind === 'all'} onclick={() => (kind = 'all')}>All</button>
    {#each groups as g (g.id)}
      <button class:on={kind === g.id} onclick={() => (kind = g.id)}>{g.label}</button>
    {/each}
  </div>
{/if}

<p class="summary">{installedCount} of {all.length} features installed.{mode === 'store' ? ' Only what you install runs.' : ''}</p>

{#snippet action(p: Page)}
  {#if mode === 'installed'}
    <button class="ghost" onclick={(e) => { e.stopPropagation(); uninstall(p); }}>Uninstall</button>
  {:else if store.installed(p.id)}
    <span class="done"><Icon name="check" size={13} />Installed</span>
  {:else}
    <button class="install" onclick={(e) => { e.stopPropagation(); store.install(p.id); }}>Install</button>
  {/if}
{/snippet}

{#each shown as g (g.id)}
  <h3 class="group-title">{g.label}</h3>
  {#if view === 'grid'}
    <div class="grid">
      {#each g.pages as p, i (p.id)}
        <div class="tile" role="button" tabindex="0" in:fadeSlide={{ token: 'listReorder', delay: i * (motion.listReorder.stagger ?? 0) }} onclick={() => goto(p.id)} onkeydown={(e) => e.key === 'Enter' && goto(p.id)}>
          <PageIcon page={p} group={g.id} />
          <span class="label">{p.label}</span>
          <span class="blurb">{p.blurb}</span>
          <span class="act">{@render action(p)}</span>
        </div>
      {/each}
    </div>
  {:else}
    <div class="list">
      {#each g.pages as p, i (p.id)}
        <div class="row" role="button" tabindex="0" in:fadeSlide={{ token: 'listReorder', delay: i * (motion.listReorder.stagger ?? 0) }} onclick={() => goto(p.id)} onkeydown={(e) => e.key === 'Enter' && goto(p.id)}>
          <PageIcon page={p} group={g.id} size={30} />
          <span class="text"><span class="label">{p.label}</span><span class="blurb">{p.blurb}</span></span>
          {@render action(p)}
        </div>
      {/each}
    </div>
  {/if}
{:else}
  <div class="empty">
    {#if mode === 'installed' && !query.trim()}
      <strong>Nothing installed yet.</strong>
      <span>Notifi only runs what you install, so it stays light.</span>
      <button class="install" onclick={() => goto('features')}>Install features</button>
    {:else}
      <span>Nothing matches "{query}".</span>
    {/if}
  </div>
{/each}

{#if undo}
  <div class="undo" role="status" in:fadeSlide={{ token: 'toastIn', y: 12 }}>
    Uninstalled {undo.label}. Its options are kept.
    <button onclick={restore}>Undo</button>
  </div>
{/if}

<style>
  .tools {
    display: flex;
    gap: 10px;
    margin-bottom: 10px;
  }
  .tools input {
    flex: 1;
    min-width: 0;
    padding: 7px 10px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    font-size: 13px;
    user-select: text;
  }
  .tools input:focus {
    outline: 2px solid var(--acc-soft);
    border-color: var(--acc);
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
  }
  .seg button {
    padding: 4px 12px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--mut);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }
  .seg button.on {
    background: var(--acc);
    color: var(--acc-fg);
    font-weight: 600;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 8px;
  }
  .chips button {
    padding: 4px 11px;
    border: 1px solid var(--line);
    border-radius: 99px;
    background: none;
    color: var(--fg-soft);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .chips button.on {
    border-color: var(--acc);
    background: var(--acc-soft);
    color: var(--fg);
  }
  .summary {
    margin: 0 4px 2px;
    color: var(--mut);
    font-size: 13px;
  }
  .list {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--surface);
    overflow: hidden;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .row + .row {
    border-top: 1px solid var(--line);
  }
  .row:hover {
    background: var(--hover);
  }
  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }
  .label {
    font-weight: 600;
    font-size: 13.5px;
  }
  .blurb {
    color: var(--mut);
    font-size: 12px;
    line-height: 1.4;
  }
  .row .blurb {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    gap: 10px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 12px 14px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--surface);
    cursor: pointer;
    transition:
      background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease),
      border-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .tile:hover {
    border-color: var(--line-strong);
    background: color-mix(in srgb, var(--surface), var(--fg) 3%);
  }
  .tile .act {
    margin-top: auto;
    padding-top: 6px;
  }
  .install {
    padding: 5px 14px;
    border: 1px solid var(--acc);
    border-radius: 8px;
    background: var(--acc);
    color: var(--acc-fg);
    font: inherit;
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .install:hover {
    filter: brightness(1.08);
  }
  .done {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--acc);
    font-size: 12.5px;
    font-weight: 600;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 48px 0;
    color: var(--mut);
    font-size: 13px;
  }
  .empty strong {
    color: var(--fg);
    font-size: 15px;
  }
  .undo {
    position: fixed;
    left: 50%;
    bottom: 24px;
    translate: -50% 0;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 16px;
    border-radius: 10px;
    background: var(--toast);
    color: var(--toast-fg);
    font-size: 13px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.18);
  }
  .undo button {
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-weight: 700;
    text-decoration: underline;
    cursor: pointer;
  }
</style>
