<script lang="ts">
  import { getVersion } from '@tauri-apps/api/app';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import Icon from '../Icon.svelte';

  type UpdateStatus =
    | { state: 'idle' | 'checking' | 'upToDate' }
    | { state: 'downloading'; version: string; percent: number }
    | { state: 'ready'; version: string }
    | { state: 'manual'; version: string; url: string }
    | { state: 'error'; message: string };

  let version = $state('');
  let update = $state<UpdateStatus>({ state: 'idle' });
  const busy = $derived(update.state === 'checking' || update.state === 'downloading');
  const text = $derived.by(() => {
    switch (update.state) {
      case 'idle': return 'Not checked yet';
      case 'checking': return 'Checking for updates...';
      case 'upToDate': return 'Up to date';
      case 'downloading': return `Downloading ${update.version}: ${update.percent}%`;
      case 'ready': return `Version ${update.version} is ready to install`;
      case 'manual': return `Version ${update.version} is available`;
      case 'error': return update.message;
    }
  });

  onMount(() => {
    getVersion().then((v) => (version = v));
    const off = listen<UpdateStatus>('updater://status', (e) => (update = e.payload));
    invoke<UpdateStatus>('get_update_status').then((s) => (update = s));
    return () => off.then((f) => f());
  });
</script>

<div class="p-card about">
  <img src="/icon.png" alt="" />
  <strong>Notifi</strong>
  <span class="p-sub">Version {version}</span>
</div>

<div class="p-card">
  <div class="p-row">
    <span class="p-glyph"><Icon name="arrowDown" size={15} /></span>
    <span class="p-main">
      <span class="p-label">Updates</span>
      <span class="p-sub" class:error={update.state === 'error'}>{text}</span>
    </span>
    {#if update.state === 'ready'}
      <button class="btn primary" onclick={() => invoke('install_update')}>Install</button>
    {:else if update.state === 'manual'}
      <button class="btn" onclick={() => invoke('open_release_page')}>Get it</button>
    {:else}
      <button class="btn" disabled={busy} onclick={() => invoke('check_update_now')}>Check</button>
    {/if}
  </div>
  <button class="p-row p-toggle notes" onclick={() => invoke('open_release_notes')}>
    <span class="p-glyph"><Icon name="notes" size={15} /></span>
    <span class="p-main"><span class="p-label">Release notes</span></span>
    <span class="chev"><Icon name="chevron" size={14} /></span>
  </button>
</div>

<style>
  .about {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 18px;
    margin-bottom: 10px;
  }
  .about img {
    width: 44px;
    height: 44px;
    margin-bottom: 6px;
  }
  .about strong {
    font-size: 16px;
  }
  .notes {
    border-radius: 0 0 11px 11px;
  }
  .chev {
    display: grid;
    color: var(--mut);
  }
  .error {
    color: var(--warn);
  }
  .btn {
    flex: none;
    padding: 5px 12px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg-soft);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }
  .btn:hover:not(:disabled) {
    background: var(--hover);
  }
  .btn:disabled {
    opacity: 0.45;
  }
  .btn.primary {
    border-color: var(--acc);
    background: var(--acc);
    color: var(--acc-fg);
  }
</style>
