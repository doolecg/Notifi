<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';

  type Frame = {
    kind: 'preview' | 'hold' | 'hint';
    x: number;
    y: number;
    width: number;
    height: number;
    progress: number;
    label: string;
    displayWidth: number;
    displayHeight: number;
  };

  let frame = $state<Frame | null>(null);
  let shown = $state(false);
  let scale = $state(1);

  const RING = 2 * Math.PI * 34;

  onMount(() => {
    const offs: Array<() => void> = [];
    listen<Frame>('snap://show', (e) => {
      scale = window.devicePixelRatio || 1;
      frame = e.payload;
      shown = true;
    }).then((off) => offs.push(off));
    listen('snap://hide', () => (shown = false)).then((off) => offs.push(off));
    return () => offs.forEach((off) => off());
  });

  const css = (px: number) => `${px / scale}px`;
</script>

<div class="stage" class:shown>
  {#if frame?.kind === 'preview'}
    <div
      class="preview"
      style:transform={`translate(${css(frame.x)}, ${css(frame.y)})`}
      style:width={css(frame.width)}
      style:height={css(frame.height)}
    ></div>
  {:else if frame?.kind === 'hold'}
    <div class="badge">
      <svg viewBox="0 0 80 80" width="80" height="80" aria-hidden="true">
        <circle class="track" cx="40" cy="40" r="34" />
        <circle class="fill" cx="40" cy="40" r="34" stroke-dasharray={RING} stroke-dashoffset={RING * (1 - frame.progress)} />
      </svg>
      <span>{frame.label}</span>
    </div>
  {:else if frame?.kind === 'hint'}
    <div class="badge"><span>{frame.label}</span></div>
  {/if}
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    background: transparent !important;
    overflow: hidden;
    user-select: none;
    pointer-events: none;
    font-family: 'Segoe UI Variable Text', 'Segoe UI', system-ui, 'Cantarell', 'Ubuntu', sans-serif;
  }

  .stage {
    position: fixed;
    inset: 0;
    opacity: 0;
    transition: opacity 300ms cubic-bezier(0.4, 0, 0.2, 1);
    --acc: #2cc9b2;
  }
  .stage.shown {
    opacity: 1;
    transition-duration: 40ms;
  }

  .preview {
    position: absolute;
    left: 0;
    top: 0;
    box-sizing: border-box;
    border: 2px solid var(--acc);
    border-radius: 8px;
    background: color-mix(in srgb, var(--acc) 18%, transparent);
    transition:
      transform 160ms cubic-bezier(0.22, 1, 0.36, 1),
      width 160ms cubic-bezier(0.22, 1, 0.36, 1),
      height 160ms cubic-bezier(0.22, 1, 0.36, 1);
  }

  .badge {
    position: absolute;
    left: 50%;
    bottom: 12%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 14px 18px;
    border-radius: 16px;
    background: rgb(29 33 34 / 0.92);
    color: #f2f4f3;
    font-size: 14px;
  }
  svg {
    transform: rotate(-90deg);
  }
  .track {
    fill: none;
    stroke: rgb(255 255 255 / 0.18);
    stroke-width: 6;
  }
  .fill {
    fill: none;
    stroke: var(--acc);
    stroke-width: 6;
    stroke-linecap: round;
    transition: stroke-dashoffset 30ms linear;
  }

  @media (prefers-reduced-motion: reduce) {
    .stage {
      transition-duration: 80ms;
    }
    .preview {
      transition: none;
    }
  }
</style>
