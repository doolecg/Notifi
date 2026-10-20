<script lang="ts">
  import Icon from './Icon.svelte';
  import type { Proc } from './live.svelte';

  let { title, apps }: { title: string; apps: Proc[] } = $props();
  let open = $state(false);
</script>

<div class="p-card top">
  <button class="p-row p-toggle head" aria-expanded={open} onclick={() => (open = !open)}>
    <span class="p-glyph"><Icon name="leaf" size={15} /></span>
    <span class="p-main"><span class="p-line"><span class="p-label">{title}</span><span class="p-sub">{apps[0]?.name ?? ''}</span></span></span>
    <span class="chev" class:open><Icon name="chevronDown" size={14} /></span>
  </button>
  {#if open}
    <ol class="p-more list">
      {#each apps as app (app.name)}
        <li>
          <span class="p-label">{app.name}</span>
          <span class="p-value">{app.cpu.toFixed(1)}%</span>
        </li>
      {/each}
    </ol>
  {/if}
</div>

<style>
  .top {
    margin-top: 10px;
  }
  .head {
    border-radius: 11px;
  }
  .chev {
    display: grid;
    color: var(--mut);
    transition: transform var(--m-tabSwitch-dur) var(--m-tabSwitch-ease);
  }
  .chev.open {
    transform: rotate(180deg);
  }
  .list {
    margin: 0;
    padding: 6px 12px 8px 48px;
    list-style: none;
  }
  .list li {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 3px 0;
    font-size: 12.5px;
  }
  .list .p-value {
    font-weight: 600;
  }
</style>
