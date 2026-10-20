<script lang="ts">
  import { HISTORY } from './live.svelte';

  let {
    values,
    max,
    guide,
    tone = 'var(--acc)',
    height = 26,
  }: { values: number[]; max?: number; guide?: number; tone?: string; height?: number } = $props();

  const top = $derived(max ?? Math.max(1, guide ?? 0, ...values) * 1.15);
  const y = (v: number) => height - 1 - (Math.min(v, top) / top) * (height - 2);
  const points = $derived(
    values.map((v, i) => `${(HISTORY - values.length + i) * (100 / (HISTORY - 1))},${y(v)}`).join(' '),
  );
  const firstX = $derived((HISTORY - values.length) * (100 / (HISTORY - 1)));
</script>

<svg class="spark" viewBox="0 0 100 {height}" preserveAspectRatio="none" style:height="{height}px" style:--tone={tone}>
  {#if guide !== undefined}
    <line x1="0" x2="100" y1={y(guide)} y2={y(guide)} class="guide" />
  {/if}
  {#if values.length > 1}
    <polygon points="{firstX},{height} {points} 100,{height}" class="area" />
    <polyline {points} class="line" />
  {/if}
</svg>

<style>
  .spark {
    display: block;
    width: 100%;
    overflow: visible;
  }
  .line {
    fill: none;
    stroke: var(--tone);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
    stroke-linejoin: round;
  }
  .area {
    fill: var(--tone);
    opacity: 0.12;
  }
  .guide {
    stroke: var(--mut);
    stroke-width: 1;
    stroke-dasharray: 3 3;
    vector-effect: non-scaling-stroke;
    opacity: 0.6;
  }
</style>
