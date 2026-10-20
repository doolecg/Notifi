<script lang="ts">
  // Custom option of the Messaging downloads page (spec/apps/messaging-downloads.md): `review`.
  import Messaging from '../apps_clean/Messaging.svelte';
  import type { CustomProps } from '../sections/customs';

  let { option, values }: CustomProps = $props();
</script>

{#if option.id === 'review'}
  <Messaging {values} />
{/if}
