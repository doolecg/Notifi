<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { fadeSlide } from '../../lib/motion';

  type UpdateStatus =
    | { state: 'idle' | 'checking' | 'upToDate' }
    | { state: 'downloading'; version: string; percent: number }
    | { state: 'ready'; version: string }
    | { state: 'manual'; version: string; url: string }
    | { state: 'error'; message: string };

  let update = $state<UpdateStatus>({ state: 'idle' });
  const busy = $derived(update.state === 'checking' || update.state === 'downloading');
  const statusText = $derived.by(() => {
    switch (update.state) {
      case 'idle': return 'Not checked yet';
      case 'checking': return 'Checking for updates...';
      case 'upToDate': return 'Notifi is up to date';
      case 'downloading': return `Downloading ${update.version}: ${update.percent}%`;
      case 'ready': return `Version ${update.version} is ready to install`;
      case 'manual': return `Version ${update.version} is available`;
      case 'error': return update.message;
    }
  });

  onMount(() => {
    let stop: (() => void) | undefined;
    listen<UpdateStatus>('updater://status', (e) => (update = e.payload)).then((off) => (stop = off));
    invoke<UpdateStatus>('get_update_status').then((s) => (update = s));
    return () => stop?.();
  });
</script>

<div class="row">
  <span class="name">
    Status
    {#key statusText}
      <small class:error={update.state === 'error'} in:fadeSlide={{ token: 'tabSwitch', y: 4 }}>{statusText}</small>
    {/key}
    {#if update.state === 'downloading'}
      <span class="bar"><span style:width="{update.percent}%"></span></span>
    {/if}
  </span>
  <span class="actions">
    {#if update.state === 'ready'}
      <button class="ghost primary" onclick={() => invoke('install_update')}>Install now</button>
    {:else if update.state === 'manual'}
      <button class="ghost primary" onclick={() => invoke('open_release_page')}>Open release page</button>
    {/if}
    <button class="ghost" disabled={busy} onclick={() => invoke('check_update_now')}>Check now</button>
  </span>
</div>

<style>
  .actions {
    display: flex;
    gap: 8px;
  }
  .primary {
    border-color: var(--acc);
    background: var(--acc);
    color: var(--acc-fg);
  }
  .error {
    color: var(--warn) !important;
  }
  .bar {
    display: block;
    width: 220px;
    height: 4px;
    margin-top: 6px;
    border-radius: 99px;
    background: var(--inset);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--acc);
    transition: width var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
</style>
