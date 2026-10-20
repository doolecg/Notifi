<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount } from 'svelte';
  import { applyCssVars } from '../lib/motion';

  let url = $state('');

  const close = () => invoke('capture_pin_close').catch(() => {});

  function wheel(e: WheelEvent) {
    e.preventDefault();
    invoke('capture_pin_zoom', { notches: -Math.sign(e.deltaY) }).catch(() => {});
  }

  function drag(e: PointerEvent) {
    if (e.button === 0) getCurrentWindow().startDragging().catch(() => {});
  }

  onMount(() => {
    applyCssVars();
    invoke<ArrayBuffer>('capture_pin_image')
      .then((bytes) => (url = URL.createObjectURL(new Blob([bytes], { type: 'image/png' }))))
      .catch(() => {});
    return () => {
      if (url) URL.revokeObjectURL(url);
    };
  });
</script>

<div class="pin" role="presentation" onpointerdown={drag} onwheel={wheel} ondblclick={close}>
  {#if url}<img src={url} alt="Pinned screenshot" draggable="false" />{/if}
  <button class="x" onpointerdown={(e) => e.stopPropagation()} onclick={close} aria-label="Close pin" title="Close (or double-click)">
    <svg viewBox="0 0 16 16" width="12" height="12"><path d="M3 3l10 10M13 3L3 13" stroke="currentColor" stroke-width="2" stroke-linecap="round" fill="none" /></svg>
  </button>
</div>

<style>
  :global(html),
  :global(body.capture-page) {
    background: var(--bg);
    overflow: hidden;
    user-select: none;
  }
  .pin {
    position: relative;
    width: 100%;
    height: 100%;
    cursor: move;
  }
  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: fill;
  }
  .x {
    position: absolute;
    top: 6px;
    right: 6px;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--toast);
    color: var(--toast-fg);
    opacity: 0;
    cursor: pointer;
    transition: opacity var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .pin:hover .x {
    opacity: 0.9;
  }
</style>
