<script lang="ts">
  import { untrack } from 'svelte';
  import { reducedMotion } from '../lib/motion';

  let { color = '#f0a24a', calm = false, period = 2200, burst = 0 }: { color?: string; calm?: boolean; period?: number; burst?: number } = $props();

  const PITCH = 5;
  let cv = $state<HTMLCanvasElement>();
  let w = $state(0);
  let h = $state(0);

  /** Dots that flicker at random, brightest at the left edge and fading out toward the middle. */
  $effect(() => {
    if (!cv || !w || !h) return;
    const dpr = window.devicePixelRatio || 1;
    cv.width = Math.round(w * dpr);
    cv.height = Math.round(h * dpr);
    const ctx = cv.getContext('2d');
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    const cols = Math.floor(w / PITCH);
    const rows = Math.max(1, Math.floor(h / PITCH));
    const top = (h - rows * PITCH) / 2;
    const val = new Float32Array(cols * rows).map(() => Math.random());
    const tgt = val.slice();
    const chance = calm ? 0.015 : 0.09;
    const gain = calm ? 0.3 : 0.5;
    let raf = 0;
    let last = 0;
    let seenBurst = untrack(() => burst);
    let burstAt = -1e9;
    const draw = (t: number) => {
      raf = requestAnimationFrame(draw);
      if (t - last < 33) return;
      last = t;
      const still = reducedMotion();
      if (burst !== seenBurst) {
        seenBurst = burst;
        burstAt = t;
      }
      const energy = Math.max(0, 1 - (t - burstAt) / 1000);
      const front = (1 - energy) * cols * 1.3;
      ctx.clearRect(0, 0, w, h);
      ctx.fillStyle = color;
      for (let c = 0; c < cols; c++) {
        const mask = Math.pow(1 - c / cols, 1.6);
        for (let r = 0; r < rows; r++) {
          const i = c * rows + r;
          if (!still && Math.random() < chance) tgt[i] = Math.random();
          val[i] += (tgt[i] - val[i]) * 0.35;
          // A soft wave runs left to right all the time; a burst sends a bright front across.
          const wave = still ? 1 : 0.62 + 0.38 * Math.sin((t / period - (c / cols) * 0.9) * 6.2832);
          const glow = energy > 0 ? Math.max(0, 1 - Math.abs(c - front) / 7) * energy : 0;
          ctx.globalAlpha = Math.min(1, (0.05 + val[i] * gain) * mask * wave + glow * 0.85 * Math.pow(1 - c / cols, 0.6));
          ctx.beginPath();
          ctx.arc(c * PITCH + PITCH / 2, top + r * PITCH + PITCH / 2, 1.2 + val[i] * 0.7 + glow * 0.6, 0, 6.2832);
          ctx.fill();
        }
      }
    };
    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  });
</script>

<div class="px" bind:clientWidth={w} bind:clientHeight={h} aria-hidden="true">
  <canvas bind:this={cv} style:width={`${w}px`} style:height={`${h}px`}></canvas>
</div>

<style>
  .px {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    width: 55%;
    pointer-events: none;
  }
  canvas {
    display: block;
  }
</style>
