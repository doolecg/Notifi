<script lang="ts">
  import Switch from '../../lib/Switch.svelte';
  import { fadeSlide, motion } from '../../lib/motion';
  import { onPlatform, store } from '../../lib/settings.svelte';
  import PageIcon from '../PageIcon.svelte';
  import type { CustomProps } from '../customs';

  let { goto }: CustomProps = $props();

  const groups = $derived(
    (store.catalog?.groups ?? [])
      .map((g) => ({ ...g, pages: g.pages.filter((p) => p.feature && onPlatform(p)) }))
      .filter((g) => g.pages.length),
  );
  const on = $derived(groups.flatMap((g) => g.pages).filter((p) => store.featureOn(p.id)).length);
  const total = $derived(groups.reduce((n, g) => n + g.pages.length, 0));
</script>

<p class="summary">{on} of {total} features are on.</p>
{#each groups as group (group.id)}
  <h3 class="group-title">{group.label}</h3>
  <div class="grid">
    {#each group.pages as page, i (page.id)}
      <div
        class="tile"
        class:off={!store.featureOn(page.id)}
        role="button"
        tabindex="0"
        in:fadeSlide={{ token: 'listReorder', delay: i * (motion.listReorder.stagger ?? 0) }}
        onclick={() => goto(page.id)}
        onkeydown={(e) => e.key === 'Enter' && goto(page.id)}
      >
        <div class="top">
          <PageIcon {page} group={group.id} />
          <span role="presentation" onclick={(e) => e.stopPropagation()}>
            <Switch checked={store.featureOn(page.id)} label={page.label} onchange={(v) => store.setOption(page.id, 'enabled', v)} />
          </span>
        </div>
        <span class="label">{page.label}</span>
        <span class="blurb">{page.blurb}</span>
      </div>
    {/each}
  </div>
{/each}

<style>
  .summary {
    margin: 0 4px 4px;
    color: var(--mut);
    font-size: 13px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 10px;
  }
  .tile {
    display: flex;
    flex-direction: column;
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
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
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
  .off .label,
  .off .blurb {
    opacity: 0.7;
  }
</style>
