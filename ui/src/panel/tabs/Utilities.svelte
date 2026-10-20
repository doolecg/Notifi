<script lang="ts">
  import { store } from '../../lib/settings.svelte';
  import Icon from '../Icon.svelte';
  import type { IconName } from '../icons';

  let { openSettings }: { openSettings: (section: string | null) => void } = $props();

  const tools = $derived(
    (store.catalog?.groups ?? []).flatMap((g) =>
      g.pages.filter((p) => p.tool && store.featureOn(p.id)).map((p) => ({ ...p, group: g.id })),
    ),
  );
</script>

<div class="list">
  {#each tools as tool (tool.id)}
    {@const accel = store.settings?.shortcuts[tool.id]}
    <button class="p-card tool" onclick={() => openSettings(tool.id)}>
      <span class="p-glyph"><Icon name={tool.group as IconName} size={15} /></span>
      <span class="p-main">
        <span class="p-label name">{tool.label}</span>
        <span class="p-sub p-label">{tool.blurb}</span>
      </span>
      {#if accel}<kbd>{accel}</kbd>{/if}
      <span class="chev"><Icon name="chevron" size={14} /></span>
    </button>
  {/each}
  <button class="p-card tool add" onclick={() => openSettings('features')}>
    <span class="p-glyph"><Icon name="plus" size={15} /></span>
    <span class="p-main">
      <span class="name">Add tools…</span>
      <span class="p-sub">{tools.length ? 'Turn more tools on in Settings' : 'No tools are on yet. Pick some in Settings.'}</span>
    </span>
    <span class="chev"><Icon name="chevron" size={14} /></span>
  </button>
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .tool {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px 9px 12px;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .tool:hover {
    background: var(--hover);
  }
  .name {
    font-size: 13px;
    font-weight: 600;
  }
  .add {
    border-style: dashed;
    background: none;
  }
  kbd {
    flex: none;
    padding: 1px 6px;
    border: 1px solid var(--line-strong);
    border-bottom-width: 2px;
    border-radius: 5px;
    background: var(--inset);
    color: var(--fg-soft);
    font: 11px var(--mono);
  }
  .chev {
    display: grid;
    color: var(--mut);
  }
</style>
