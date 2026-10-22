<script lang="ts">
  import Icon from '../panel/Icon.svelte';

  /** A trail like Settings > Installed > Island; every part but the last is a link when it has a page `id`. */
  let { crumbs, goto }: { crumbs: { label: string; id?: string }[]; goto: (page: string) => void } = $props();
</script>

<nav class="crumbs" aria-label="Breadcrumb">
  {#each crumbs as c, i (i)}
    {#if i > 0}<span class="sep"><Icon name="chevron" size={12} /></span>{/if}
    {#if c.id && i < crumbs.length - 1}
      <button class="link" onclick={() => goto(c.id!)}>{c.label}</button>
    {:else}
      <span class="here" aria-current={i === crumbs.length - 1 ? 'page' : undefined}>{c.label}</span>
    {/if}
  {/each}
</nav>

<style>
  .crumbs {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-left: 8px;
    font-size: 13.5px;
    min-width: 0;
  }
  .link {
    padding: 3px 7px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--mut);
    font: inherit;
    cursor: pointer;
    transition:
      background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease),
      color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .link:hover {
    background: var(--hover);
    color: var(--fg);
  }
  .here {
    padding: 3px 7px;
    font-weight: 600;
    white-space: nowrap;
  }
  .sep {
    display: inline-flex;
    color: var(--mut);
    opacity: 0.7;
  }
</style>
