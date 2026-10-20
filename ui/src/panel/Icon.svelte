<script lang="ts">
  import { icons, type IconName } from './icons';

  let { name, size = 18 }: { name: IconName; size?: number } = $props();
</script>

<svg class="icon" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
  <path d={icons[name]} />
</svg>

<style>
  .icon {
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.7;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
