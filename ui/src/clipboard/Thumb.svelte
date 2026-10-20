<script lang="ts">
  import { LOOK_ICON, lookOf, type Entry } from './entries';
  import Glyph from './Glyph.svelte';
  import { image, want } from './images.svelte';

  let { entry, size = 34 }: { entry: Entry; size?: number } = $props();

  const look = $derived(lookOf(entry));
  const src = $derived(entry.kind === 'image' ? image(entry.id) : undefined);

  $effect(() => {
    if (entry.kind === 'image') want(entry.id);
  });
</script>

<span class="thumb" class:img={!!src} style:width="{size}px" style:height="{size}px">
  {#if src}
    <img {src} alt="" draggable="false" />
  {:else if look === 'color'}
    <span class="swatch" style:background={entry.text.trim()}></span>
  {:else}
    <Glyph name={LOOK_ICON[look]} size={Math.round(size * 0.5)} />
  {/if}
</span>

<style>
  .thumb {
    display: grid;
    place-items: center;
    flex: none;
    overflow: hidden;
    border-radius: 8px;
    background: var(--inset);
    color: var(--mut);
  }
  .thumb.img {
    background:
      repeating-conic-gradient(var(--inset) 0 25%, var(--surface) 0 50%) 0 0 / 10px 10px;
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .swatch {
    width: 100%;
    height: 100%;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.12);
    border-radius: inherit;
  }
</style>
