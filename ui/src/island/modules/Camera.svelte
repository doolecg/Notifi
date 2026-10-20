<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import Glyph from '../Glyph.svelte';

  let video = $state<HTMLVideoElement>();
  let stream: MediaStream | null = null;
  let status = $state<'starting' | 'live' | 'denied' | 'none'>('starting');
  let mirrored = $state(true);

  async function start() {
    status = 'starting';
    try {
      stream = await navigator.mediaDevices.getUserMedia({ video: { width: 640, height: 400 }, audio: false });
      if (video) video.srcObject = stream;
      status = 'live';
    } catch (e) {
      status = (e as DOMException)?.name === 'NotFoundError' ? 'none' : 'denied';
    }
  }
  function stop() {
    stream?.getTracks().forEach((t) => t.stop());
    stream = null;
  }

  onMount(start);
  onDestroy(stop);
</script>

<div class="cam">
  <header>
    <h2>Camera mirror</h2>
    {#if status === 'live'}
      <button class="icon" aria-label="Flip" title="Flip" onclick={() => (mirrored = !mirrored)}><Glyph name="camera" size={15} /></button>
    {/if}
  </header>
  <div class="frame" class:live={status === 'live'}>
    <video bind:this={video} autoplay muted playsinline class:flip={mirrored}></video>
    {#if status !== 'live'}
      <p>
        {#if status === 'starting'}Starting the camera…{:else if status === 'none'}No camera found.{:else}Camera access was blocked. Allow it for Notifi in Windows Settings, Privacy, Camera.{/if}
      </p>
      {#if status !== 'starting'}<button class="again" onclick={start}>Try again</button>{/if}
    {/if}
  </div>
</div>

<style>
  .cam {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    gap: 8px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--i-mut);
  }
  .frame {
    position: relative;
    flex: 1;
    min-height: 0;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 14px;
    background: var(--i-raised);
    text-align: center;
    color: var(--i-mut);
  }
  video {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0;
    filter: blur(8px);
    transition:
      opacity var(--m-glassIn-dur) var(--m-glassIn-ease),
      filter var(--m-glassIn-dur) var(--m-glassIn-ease);
  }
  .live video {
    opacity: 1;
    filter: none;
  }
  video.flip {
    transform: scaleX(-1);
  }
  p {
    z-index: 1;
    max-width: 280px;
    margin: 0;
  }
  .again {
    z-index: 1;
    margin-top: 8px;
    padding: 5px 14px;
    border: 0;
    border-radius: 99px;
    background: var(--i-fg);
    color: var(--i-bg);
    font: inherit;
    cursor: pointer;
  }
</style>
