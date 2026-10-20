<script lang="ts">
  import { flip } from 'svelte/animate';
  import { duration, fadeSlide } from './motion';

  let { items, placeholder, label, onchange }: { items: string[]; placeholder: string; label: string; onchange: (items: string[]) => void } = $props();
  let draft = $state('');

  function add() {
    const v = draft.trim();
    if (!v || items.includes(v)) return;
    onchange([...items, v]);
    draft = '';
  }
</script>

<div class="list">
  {#each items as item (item)}
    <span class="chip" animate:flip={{ duration: duration('listReorder') }} in:fadeSlide={{ token: 'listReorder', y: 4 }}>
      {item}
      <button aria-label="Remove {item}" onclick={() => onchange(items.filter((i) => i !== item))}>×</button>
    </span>
  {/each}
  <input
    aria-label="Add to {label}"
    {placeholder}
    bind:value={draft}
    onkeydown={(e) => e.key === 'Enter' && add()}
    onblur={add}
  />
</div>

<style>
  .list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 0 16px 12px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 4px 3px 10px;
    border-radius: 99px;
    background: var(--inset);
    font-size: 12.5px;
  }
  .chip button {
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--mut);
    font-size: 14px;
    line-height: 1;
    cursor: pointer;
  }
  .chip button:hover {
    background: var(--hover);
    color: var(--fg);
  }
  input {
    flex: 1;
    min-width: 160px;
    padding: 5px 10px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    font-size: 12.5px;
    user-select: text;
  }
</style>
