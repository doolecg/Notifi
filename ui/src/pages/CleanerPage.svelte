<script lang="ts">
  // Custom option of the Cleaner page (spec/apps/cleaner.md): `scan`.
  import Cleaner from '../apps_clean/Cleaner.svelte';
  import type { CustomProps } from '../sections/customs';

  let { option }: CustomProps = $props();
</script>

{#if option.id === 'scan'}
  <Cleaner />
{/if}
