<script lang="ts">
  import { store } from '../../lib/settings.svelte';
  import Switch from '../../lib/Switch.svelte';
  import QuickToggles from '../../quick/QuickToggles.svelte';
  import Brightness from './Brightness.svelte';
  import Icon from '../Icon.svelte';
  import type { IconName } from '../icons';

  const GROUPS = ['windows', 'input', 'clipboard', 'utilities'];
  const groups = $derived((store.catalog?.groups ?? []).filter((g) => GROUPS.includes(g.id)));
  let open = $state<Record<string, boolean>>({ windows: true });

  function toggle(id: string, on: boolean) {
    store.setOption(id, 'enabled', on);
  }
</script>

<div class="p-head"><span>Quick toggles</span></div>
{#if store.featureOn('quickToggles')}
  <QuickToggles />
{:else}
  <div class="p-card item">
    <span class="p-main">
      <span class="p-label name">Quick toggles</span>
      <span class="p-sub">Dark mode, desktop icons, do not disturb and more in one place.</span>
    </span>
    <Switch checked={false} label="Quick toggles" onchange={(on) => toggle('quickToggles', on)} />
  </div>
{/if}

{#if store.featureOn('displays')}
  <Brightness />
{/if}

{#each groups as group (group.id)}
  {@const count = group.pages.filter((p) => store.featureOn(p.id)).length}
  <section class="group">
    <button class="head" aria-expanded={!!open[group.id]} onclick={() => (open[group.id] = !open[group.id])}>
      <span class="chev" class:open={open[group.id]}><Icon name="chevron" size={13} /></span>
      <span class="label">{group.label}</span>
      <span class="count" class:some={count > 0}>{count}/{group.pages.length} on</span>
    </button>
    {#if open[group.id]}
      <div class="items">
        {#each group.pages as feature (feature.id)}
          <div class="p-card item">
            <span class="p-glyph" class:off={!store.featureOn(feature.id)}><Icon name={group.id as IconName} size={15} /></span>
            <span class="p-main">
              <span class="p-label name">{feature.label}</span>
              <span class="p-sub">{feature.blurb}</span>
            </span>
            <Switch checked={store.featureOn(feature.id)} label={feature.label} onchange={(on) => toggle(feature.id, on)} />
          </div>
        {/each}
      </div>
    {/if}
  </section>
{/each}

<style>
  :global(.p-card) + .group,
  .group + .group {
    margin-top: 6px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 34px;
    padding: 0 6px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--fg);
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
  .head:hover {
    background: var(--hover);
  }
  .label {
    flex: 1;
    text-align: left;
  }
  .count {
    color: var(--mut);
    font-size: 11.5px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }
  .count.some {
    color: var(--acc);
  }
  .chev {
    display: grid;
    color: var(--mut);
    transition: transform var(--m-tabSwitch-dur) var(--m-tabSwitch-ease);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .items {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 4px 0 6px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
  }
  .name {
    font-size: 13px;
    font-weight: 600;
  }
  .p-glyph.off {
    background: var(--inset);
    color: var(--mut);
  }
</style>
