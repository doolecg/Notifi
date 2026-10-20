<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import Bar from './Bar.svelte';
  import Pick from './Pick.svelte';

  const picking = getCurrentWindow().label === 'recordpick';
</script>

{#if picking}
  <Pick />
{:else}
  <Bar />
{/if}

<style>
  :global(html),
  :global(body.recording-page) {
    background: transparent;
    overflow: hidden;
    user-select: none;
  }
</style>
