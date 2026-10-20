<script lang="ts">
  // Custom option of the Screenshot page: `list`, in the Recent captures card.
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import type { CustomProps } from '../sections/customs';

  let { option }: CustomProps = $props();
  let count = $state<number | null>(null);

  onMount(() => {
    invoke<unknown[]>('capture_recent_list')
      .then((items) => (count = items.length))
      .catch(() => (count = null));
  });
</script>

{#if option.id === 'list'}
  <div class="row">
    <span class="name">
      Recent captures
      <small>{count === null ? 'Turn the feature on to keep a list.' : `${count} kept. Open, copy, edit, pin, trim or delete them.`}</small>
    </span>
    <button class="ghost" onclick={() => invoke('capture_recent_open').catch(() => {})}>Open the list</button>
  </div>
{/if}
