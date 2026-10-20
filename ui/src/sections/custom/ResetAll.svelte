<script lang="ts">
  import type { CustomProps } from '../customs';
  import { store } from '../../lib/settings.svelte';

  let { option }: CustomProps = $props();
  let confirming = $state(false);
  let timer: ReturnType<typeof setTimeout>;

  async function click() {
    clearTimeout(timer);
    if (!confirming) {
      confirming = true;
      timer = setTimeout(() => (confirming = false), 4000);
      return;
    }
    confirming = false;
    await store.reset('all');
  }
</script>

<div class="row">
  <span class="name">{option.label}<small>{confirming ? 'Click again to reset every page.' : option.hint}</small></span>
  <button class="ghost" class:danger={confirming} onclick={click}>{confirming ? 'Reset everything' : 'Reset...'}</button>
</div>

<style>
  .danger {
    border-color: var(--warn);
    color: var(--warn);
  }
</style>
