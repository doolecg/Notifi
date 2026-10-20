<script lang="ts">
  // Custom option of the Uninstaller page (spec/apps/uninstaller.md): `list`.
  import Uninstaller from '../apps_clean/Uninstaller.svelte';
  import type { CustomProps } from '../sections/customs';

  let { option, values }: CustomProps = $props();
</script>

{#if option.id === 'list'}
  <Uninstaller {values} />
{/if}
