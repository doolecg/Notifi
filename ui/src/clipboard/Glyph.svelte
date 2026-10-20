<script lang="ts">
  import { glyphs, type GlyphName } from './glyphs';

  let { name, size = 16 }: { name: GlyphName; size?: number } = $props();
</script>

<svg class="glyph" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
  <path d={glyphs[name]} />
</svg>

<style>
  .glyph {
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.7;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
