<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars } from '../lib/motion';

  interface State {
    phase: 'countdown' | 'recording' | 'paused' | 'saving' | 'saved' | 'error';
    elapsedMs: number;
    countdown: number;
    path: string;
    message: string;
  }

  let s = $state<State>({ phase: 'recording', elapsedMs: 0, countdown: 0, path: '', message: '' });
  let dismissTimer: ReturnType<typeof setTimeout> | undefined;

  const clock = $derived.by(() => {
    const total = Math.floor(s.elapsedMs / 1000);
    const pad = (n: number) => String(n).padStart(2, '0');
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    return `${h > 0 ? `${h}:${pad(m)}` : pad(m)}:${pad(total % 60)}`;
  });
  const fileName = $derived(s.path.split(/[\\/]/).pop() ?? '');

  const stop = () => invoke('recording_toggle').catch(() => {});
  const pause = () => invoke('recording_pause').catch(() => {});
  const openFolder = () => invoke('recording_open_folder').catch(() => {});
  const dismiss = () => invoke('recording_dismiss').catch(() => {});

  onMount(() => {
    applyCssVars();
    const off = listen<State>('recording://state', (e) => {
      s = e.payload;
      clearTimeout(dismissTimer);
      if (s.phase === 'saved' || s.phase === 'error') dismissTimer = setTimeout(dismiss, 10000);
    });
    return () => {
      clearTimeout(dismissTimer);
      off.then((f) => f());
    };
  });
</script>

<div class="bar">
  {#if s.phase === 'countdown'}
    <span class="count">{s.countdown}</span>
    <span class="text">Recording starts soon</span>
    <button class="plain" onclick={stop}>Cancel</button>
  {:else if s.phase === 'recording' || s.phase === 'paused'}
    <i class="dot" class:paused={s.phase === 'paused'}></i>
    <span class="time">{clock}</span>
    <span class="text">{s.phase === 'paused' ? 'Paused' : 'Recording'}</span>
    <button class="icon" onclick={pause} aria-label={s.phase === 'paused' ? 'Resume' : 'Pause'} title={s.phase === 'paused' ? 'Resume' : 'Pause'}>
      {#if s.phase === 'paused'}
        <svg viewBox="0 0 16 16" width="14" height="14"><path d="M4 2.5v11l9-5.5z" fill="currentColor" /></svg>
      {:else}
        <svg viewBox="0 0 16 16" width="14" height="14"><path d="M3.5 2h3v12h-3zM9.5 2h3v12h-3z" fill="currentColor" /></svg>
      {/if}
    </button>
    <button class="icon stop" onclick={stop} aria-label="Stop" title="Stop">
      <svg viewBox="0 0 16 16" width="14" height="14"><rect x="3" y="3" width="10" height="10" rx="1.5" fill="currentColor" /></svg>
    </button>
  {:else if s.phase === 'saving'}
    <span class="text">Saving the recording</span>
  {:else if s.phase === 'saved'}
    <span class="text name" title={s.path}>Saved {fileName}</span>
    <button class="plain" onclick={openFolder}>Open folder</button>
    <button class="plain" onclick={dismiss}>Close</button>
  {:else}
    <span class="text name" title={s.message}>{s.message}</span>
    <button class="plain" onclick={dismiss}>Close</button>
  {/if}
</div>

<style>
  .bar {
    --rec: #e5484d;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 100%;
    padding: 0 10px 0 16px;
    border: 1px solid var(--line-strong);
    border-radius: 14px;
    background: var(--bg);
    color: var(--fg);
    font-family: var(--sans);
    font-size: 13px;
  }
  .dot {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--rec);
  }
  .dot.paused {
    background: var(--mut);
  }
  .time,
  .count {
    flex: none;
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }
  .count {
    min-width: 18px;
    font-size: 20px;
  }
  .text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--mut);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .text.name {
    color: var(--fg);
  }
  button {
    flex: none;
    height: 32px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: var(--fg);
    font: inherit;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  button:hover {
    background: var(--hover);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 32px;
    padding: 0;
  }
  .icon.stop {
    color: var(--rec);
  }
  .plain {
    padding: 0 10px;
  }
</style>
