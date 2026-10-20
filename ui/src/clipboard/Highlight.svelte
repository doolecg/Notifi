<script lang="ts">
  import { pieces } from './entries';

  let { text, words, around = false }: { text: string; words: string[]; around?: boolean } = $props();

  const parts = $derived(pieces(text, words, around));
</script>

{#each parts as p, i (i)}{#if p.hit}<mark>{p.text}</mark>{:else}{p.text}{/if}{/each}

<style>
  mark {
    border-radius: 3px;
    background: var(--acc-soft);
    color: var(--acc);
    font-weight: 600;
    box-shadow: 0 0 0 1px var(--acc-soft);
  }
</style>
