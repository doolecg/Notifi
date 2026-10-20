<script lang="ts">
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars } from '../lib/motion';

  interface Target {
    path: string;
    name: string;
    kind: 'gif' | 'mp4';
    durationMs: number | null;
  }

  let target = $state<Target | null>(null);
  let video = $state<HTMLVideoElement>();
  let src = $state('');
  let duration = $state(0);
  let start = $state(0);
  let end = $state(0);
  let busy = $state(false);
  let message = $state('');
  let saved = $state('');
  let previewing = false;

  const fmt = (ms: number) => {
    const total = Math.max(0, Math.round(ms));
    const m = Math.floor(total / 60000);
    const s = Math.floor((total % 60000) / 1000);
    return `${m}:${String(s).padStart(2, '0')}.${String(total % 1000).padStart(3, '0')}`;
  };

  function setDuration(ms: number) {
    duration = ms;
    start = 0;
    end = ms;
  }

  async function load() {
    message = '';
    saved = '';
    try {
      target = await invoke<Target>('recording_trim_target');
    } catch (err) {
      target = null;
      message = String(err);
      return;
    }
    duration = 0;
    start = 0;
    end = 0;
    if (target.kind === 'gif') {
      src = '';
      setDuration(target.durationMs ?? 0);
    } else {
      src = convertFileSrc(`video-${Date.now()}`, 'capturemedia');
    }
  }

  function loaded() {
    if (video && Number.isFinite(video.duration)) setDuration(Math.round(video.duration * 1000));
  }

  function setStart(ms: number) {
    start = Math.min(Math.max(0, ms), end - 100);
    if (video) video.currentTime = start / 1000;
  }

  function setEnd(ms: number) {
    end = Math.max(Math.min(duration, ms), start + 100);
  }

  function preview() {
    if (!video) return;
    previewing = true;
    video.currentTime = start / 1000;
    video.play().catch(() => {});
  }

  function tick() {
    if (video && previewing && video.currentTime * 1000 >= end) {
      video.pause();
      previewing = false;
    }
  }

  async function save() {
    if (!target || busy) return;
    busy = true;
    message = 'Saving the trimmed copy. This can take a while for long videos.';
    try {
      saved = await invoke<string>('recording_trim_save', { startMs: start, endMs: end, durationMs: duration });
      message = '';
    } catch (err) {
      message = String(err);
    }
    busy = false;
  }

  const reveal = () => invoke('recording_trim_reveal', { path: saved }).catch(() => {});
  const close = () => invoke('recording_trim_close').catch(() => {});

  onMount(() => {
    applyCssVars();
    load();
    const off = listen('recording://trim', () => load());
    return () => off.then((f) => f());
  });
</script>

<div class="trim">
  <header>
    <h1>Trim recording</h1>
    <span class="name" title={target?.path}>{target?.name ?? ''}</span>
  </header>

  {#if target}
    <div class="stage">
      {#if target.kind === 'mp4'}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video bind:this={video} {src} controls preload="metadata" onloadedmetadata={loaded} ontimeupdate={tick}></video>
      {:else}
        <p class="note">GIF files have no preview here. Choose the start and end with the sliders.</p>
      {/if}
    </div>

    <div class="range">
      <label>
        <span>Start</span>
        <input type="range" min="0" max={duration} step="10" value={start} oninput={(e) => setStart(+e.currentTarget.value)} disabled={!duration || busy} />
        <b>{fmt(start)}</b>
        {#if target.kind === 'mp4'}<button onclick={() => setStart(Math.round((video?.currentTime ?? 0) * 1000))} disabled={busy}>Use current time</button>{/if}
      </label>
      <label>
        <span>End</span>
        <input type="range" min="0" max={duration} step="10" value={end} oninput={(e) => setEnd(+e.currentTarget.value)} disabled={!duration || busy} />
        <b>{fmt(end)}</b>
        {#if target.kind === 'mp4'}<button onclick={() => setEnd(Math.round((video?.currentTime ?? 0) * 1000))} disabled={busy}>Use current time</button>{/if}
      </label>
      <p class="keep">Keeps {fmt(end - start)} of {fmt(duration)}</p>
    </div>

    <div class="actions">
      {#if target.kind === 'mp4'}<button onclick={preview} disabled={!duration || busy}>Play the part you keep</button>{/if}
      <span class="grow"></span>
      <button onclick={close}>Close</button>
      <button class="main" onclick={save} disabled={!duration || busy || end - start < 100}>{busy ? 'Saving...' : 'Save trimmed copy'}</button>
    </div>
  {/if}

  {#if saved}
    <p class="done">Saved <span title={saved}>{saved.split(/[\\/]/).pop()}</span>. The original is unchanged. <button onclick={reveal}>Show in folder</button></p>
  {/if}
  {#if message}<p class="msg">{message}</p>{/if}
</div>

<style>
  :global(html),
  :global(body.recording-page) {
    background: var(--bg);
    overflow: auto;
    user-select: none;
  }
  .trim {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 100%;
    padding: 14px;
    box-sizing: border-box;
    color: var(--fg);
    font: 13px/1.4 var(--sans);
  }
  header {
    display: flex;
    gap: 10px;
    align-items: baseline;
  }
  h1 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .name {
    min-width: 0;
    overflow: hidden;
    color: var(--mut);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .stage {
    display: grid;
    place-items: center;
    min-height: 120px;
    overflow: hidden;
    border-radius: 10px;
    background: var(--inset);
  }
  video {
    display: block;
    width: 100%;
    max-height: 240px;
  }
  .note,
  .msg {
    margin: 0;
    padding: 10px;
    color: var(--mut);
  }
  .range {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  label {
    display: grid;
    grid-template-columns: 44px 1fr 76px auto;
    gap: 8px;
    align-items: center;
  }
  label span {
    color: var(--mut);
  }
  input[type='range'] {
    min-width: 0;
    accent-color: var(--acc);
  }
  b {
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .keep {
    margin: 0;
    color: var(--mut);
  }
  .actions {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-top: auto;
  }
  .grow {
    flex: 1;
  }
  button {
    padding: 5px 10px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  button:hover:not(:disabled) {
    background: var(--hover);
  }
  button:disabled {
    color: var(--mut);
    cursor: default;
  }
  button.main {
    border-color: transparent;
    background: var(--acc);
    color: var(--acc-fg);
  }
  .done {
    margin: 0;
  }
</style>
