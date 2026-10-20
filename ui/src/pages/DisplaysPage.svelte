<script lang="ts">
  // Custom options of the Displays page. Wallpaper gallery (spec/apps/wallpaper-gallery.md): `gallery`.
  import Gallery from '../apps_clean/Gallery.svelte';
  import type { CustomProps } from '../sections/customs';

  let { option }: CustomProps = $props();
</script>

{#if option.id === 'gallery'}
  <Gallery />
{/if}
