<script lang="ts">
  // Custom option of the Installer helper page (spec/clipboard/installer-helper.md): `offers`.
  import Offers from '../apps_clean/Offers.svelte';
  import type { CustomProps } from '../sections/customs';

  let { option, values }: CustomProps = $props();
</script>

{#if option.id === 'offers'}
  <Offers {values} />
{/if}
