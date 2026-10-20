<script lang="ts">
  import { flip } from 'svelte/animate';
  import Switch from '../../lib/Switch.svelte';
  import { duration } from '../../lib/motion';
  import { store, type PanelTab } from '../../lib/settings.svelte';
  import type { CustomProps } from '../customs';

  let { goto }: CustomProps = $props();

  const panel = $derived(store.settings!.panel);
  const tabs = $derived.by(() => {
    const all = store.catalog?.panelTabs ?? [];
    return panel.order.map((id) => all.find((t) => t.id === id)).filter((t): t is PanelTab => !!t);
  });

  function note(tab: PanelTab) {
    if (!tab.ready) return 'Arrives in a later version.';
    if (tab.feature && !store.featureOn(tab.feature)) {
      const page = store.page(tab.feature);
      return `Shows once ${page?.label ?? tab.feature} is on.`;
    }
    if (tab.id === 'power') return 'Shown on computers with a battery.';
    return '';
  }

  function move(id: string, by: number) {
    const order = [...panel.order];
    const i = order.indexOf(id);
    const j = i + by;
    if (i < 0 || j < 0 || j >= order.length) return;
    [order[i], order[j]] = [order[j], order[i]];
    store.set('panel', { order });
  }
</script>

{#each tabs as tab, i (tab.id)}
  <div class="row" animate:flip={{ duration: duration('listReorder') }}>
    <span class="name">
      {tab.label}
      {#if note(tab)}
        <small>
          {note(tab)}
          {#if tab.feature && !store.featureOn(tab.feature) && store.page(tab.feature)}
            <button class="link" onclick={() => goto(tab.feature!)}>Open</button>
          {/if}
        </small>
      {/if}
    </span>
    <span class="controls">
      <button class="ghost arrow" aria-label="Move {tab.label} up" disabled={i === 0} onclick={() => move(tab.id, -1)}>↑</button>
      <button class="ghost arrow" aria-label="Move {tab.label} down" disabled={i === tabs.length - 1} onclick={() => move(tab.id, 1)}>↓</button>
      {#if tab.ready}
        <Switch checked={panel.tabs[tab.id] ?? true} label={tab.label} onchange={(on) => store.set('panel', { tabs: { ...panel.tabs, [tab.id]: on } })} />
      {/if}
    </span>
  </div>
{/each}

<style>
  .row + .row {
    border-top: 1px solid var(--line);
  }
  .controls {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .arrow {
    padding: 4px 9px;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--acc);
    font: inherit;
    cursor: pointer;
  }
</style>
