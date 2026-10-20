<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onDestroy, onMount } from 'svelte';
  import { applyCssVars } from '../lib/motion';

  interface Options {
    camera: string;
    mirror: boolean;
  }

  type Status = 'starting' | 'live' | 'blocked' | 'busy' | 'none';

  let video = $state<HTMLVideoElement>();
  let status = $state<Status>('starting');
  let mirrored = $state(true);
  let cameras = $state<MediaDeviceInfo[]>([]);
  let choosing = $state(false);
  let preferred = '';
  let stream: MediaStream | null = null;

  const messages: Record<Exclude<Status, 'live'>, string> = {
    starting: 'Starting the camera...',
    blocked: 'The camera is switched off or blocked. Allow desktop apps to use it in Windows Settings, Privacy, Camera.',
    busy: 'Another app is using the camera. Close it there and try again.',
    none: 'No camera was found.',
  };

  function release() {
    stream?.getTracks().forEach((t) => t.stop());
    stream = null;
    if (video) video.srcObject = null;
  }

  function failure(e: unknown): Status {
    const name = (e as DOMException)?.name;
    if (name === 'NotFoundError' || name === 'OverconstrainedError') return 'none';
    if (name === 'NotReadableError' || name === 'AbortError') return 'busy';
    return 'blocked';
  }

  async function open(deviceId?: string) {
    release();
    status = 'starting';
    try {
      stream = await navigator.mediaDevices.getUserMedia({
        video: deviceId ? { deviceId: { exact: deviceId }, width: 640, height: 480 } : { width: 640, height: 480 },
        audio: false,
      });
    } catch (e) {
      status = failure(e);
      return;
    }
    // A camera that is unplugged or switched off mid-use closes the window instead of freezing the last frame.
    stream.getVideoTracks()[0]?.addEventListener('ended', () => {
      release();
      invoke('camera_preview_lost', { why: 'The camera was disconnected, so the preview closed' }).catch(() => {});
    });
    if (video) video.srcObject = stream;
    status = 'live';
  }

  const same = (d: MediaDeviceInfo) => !!preferred && d.label.toLowerCase().includes(preferred.toLowerCase());

  async function start() {
    await open();
    if (status !== 'live') return;
    // Camera names are only given out once a camera has been opened.
    cameras = (await navigator.mediaDevices.enumerateDevices()).filter((d) => d.kind === 'videoinput');
    const wanted = cameras.find(same);
    const current = stream?.getVideoTracks()[0]?.getSettings().deviceId;
    if (wanted && wanted.deviceId !== current) await open(wanted.deviceId);
    else if (!wanted && cameras.length > 1) choosing = true;
  }

  async function choose(d: MediaDeviceInfo) {
    choosing = false;
    preferred = d.label;
    invoke('camera_preview_remember', { label: d.label }).catch(() => {});
    await open(d.deviceId);
  }

  function stop() {
    release();
    invoke('camera_preview_close').catch(() => {});
  }

  const drag = (e: PointerEvent) => {
    if (e.button === 0 && e.target === e.currentTarget) getCurrentWindow().startDragging().catch(() => {});
  };

  onMount(() => {
    applyCssVars();
    invoke<Options>('camera_preview_options')
      .then((o) => {
        mirrored = o.mirror;
        preferred = o.camera;
      })
      .catch(() => {})
      .finally(start);
    const keys = (e: KeyboardEvent) => e.key === 'Escape' && stop();
    window.addEventListener('keydown', keys);
    window.addEventListener('pagehide', release);
    return () => {
      window.removeEventListener('keydown', keys);
      window.removeEventListener('pagehide', release);
    };
  });
  onDestroy(release);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="cam" onpointerdown={drag}>
  <video bind:this={video} autoplay muted playsinline class:flip={mirrored} class:live={status === 'live'}></video>

  {#if status !== 'live'}
    <p class="note">{messages[status]}</p>
    {#if status === 'busy' || status === 'blocked'}<button class="again" onclick={() => open()}>Try again</button>{/if}
  {/if}

  {#if choosing}
    <div class="choose">
      <span>Which camera?</span>
      {#each cameras as d (d.deviceId)}
        <button onclick={() => choose(d)}>{d.label || 'Camera'}</button>
      {/each}
    </div>
  {/if}

  <div class="bar">
    {#if status === 'live'}
      <button class="small" onclick={() => (mirrored = !mirrored)} aria-pressed={mirrored}>Mirror</button>
    {/if}
    <button class="stop" onclick={stop}>Stop camera</button>
  </div>
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    overflow: hidden;
    background: #000;
    user-select: none;
  }
  .cam {
    position: relative;
    display: grid;
    place-items: center;
    height: 100%;
    overflow: hidden;
    background: var(--toast);
    color: var(--toast-fg);
    font: 13px/1.4 var(--sans);
  }
  video {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    pointer-events: none;
    opacity: 0;
    transition: opacity var(--m-glassIn-dur) var(--m-glassIn-ease);
  }
  video.live {
    opacity: 1;
  }
  video.flip {
    transform: scaleX(-1);
  }
  .note {
    z-index: 1;
    max-width: 85%;
    margin: 0 0 28px;
    text-align: center;
    pointer-events: none;
  }
  .again {
    z-index: 1;
    margin-top: -16px;
  }
  .choose {
    position: absolute;
    inset: 0;
    z-index: 2;
    display: flex;
    flex-direction: column;
    gap: 6px;
    justify-content: center;
    padding: 12px;
    background: rgb(0 0 0 / 0.6);
    color: #fff;
  }
  .bar {
    position: absolute;
    right: 0;
    bottom: 8px;
    left: 0;
    z-index: 3;
    display: flex;
    gap: 6px;
    justify-content: center;
    opacity: 0;
    transition: opacity var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .cam:hover .bar,
  .bar:focus-within {
    opacity: 1;
  }
  button {
    padding: 5px 12px;
    border: 0;
    border-radius: 99px;
    background: rgb(0 0 0 / 0.65);
    color: #fff;
    font: inherit;
    cursor: pointer;
  }
  button:hover {
    background: rgb(0 0 0 / 0.85);
  }
  button.small[aria-pressed='true'] {
    background: var(--acc);
    color: var(--acc-fg);
  }
  button.stop {
    background: var(--acc);
    color: var(--acc-fg);
  }
</style>
