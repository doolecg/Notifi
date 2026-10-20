<script lang="ts">
  import Switch from '../lib/Switch.svelte';
  import { store, type General, type Theme } from '../lib/settings.svelte';
  import { fadeSlide } from '../lib/motion';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';

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

  const general = $derived(store.settings!.general);
  const themes: { id: Theme; label: string }[] = [
    { id: 'system', label: 'System' },
    { id: 'light', label: 'Light' },
    { id: 'dark', label: 'Dark' },
  ];
  const hours = [0, 1, 3, 6, 12, 24];
  const loginLabel = navigator.userAgent.includes('Windows') ? 'Start with Windows' : 'Start at login';

  function patch(change: Partial<General>) {
    store.set('general', { ...general, ...change });
  }
</script>

<h3 class="group-title">Startup</h3>
<div class="card">
  <div class="row">
    <span class="name">{loginLabel}<small>Notifi waits in the tray until you need it.</small></span>
    <Switch checked={general.autostart} label={loginLabel} onchange={(autostart) => patch({ autostart })} />
  </div>
</div>

<h3 class="group-title">Appearance</h3>
<div class="card">
  <div class="row">
    <span class="name">Theme</span>
    <div class="segmented" role="radiogroup" aria-label="Theme" style:--index={themes.findIndex((t) => t.id === general.theme)}>
      <span class="thumb"></span>
      {#each themes as theme}
        <button role="radio" aria-checked={general.theme === theme.id} onclick={() => patch({ theme: theme.id })}>{theme.label}</button>
      {/each}
    </div>
  </div>
  <div class="row">
    <span class="name">Reduce motion<small>Also follows your system's animation setting.</small></span>
    <Switch checked={general.reducedMotion} label="Reduce motion" onchange={(reducedMotion) => patch({ reducedMotion })} />
  </div>
</div>

<h3 class="group-title">Updates</h3>
<div class="card">
  <div class="row">
    <span class="name">Update automatically<small>Downloads new versions in the background.</small></span>
    <Switch checked={general.autoUpdate} label="Update automatically" onchange={(autoUpdate) => patch({ autoUpdate })} />
  </div>
  <div class="row">
    <span class="name">Check every</span>
    <select
      value={general.updateCheckHours}
      onchange={(e) => patch({ updateCheckHours: Number(e.currentTarget.value) })}
      disabled={!general.autoUpdate}
    >
      {#each hours as h}
        <option value={h}>{h === 0 ? 'Only at startup' : `${h} ${h === 1 ? 'hour' : 'hours'}`}</option>
      {/each}
    </select>
  </div>
  <div class="row">
    <span class="name">Install on quit<small>Installs a downloaded update when you quit Notifi.</small></span>
    <Switch checked={general.installOnQuit} label="Install on quit" onchange={(installOnQuit) => patch({ installOnQuit })} />
  </div>
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
        <button class="primary" onclick={() => invoke('install_update')}>Install now</button>
      {:else if update.state === 'manual'}
        <button class="primary" onclick={() => invoke('open_release_page')}>Open release page</button>
      {/if}
      <button disabled={busy} onclick={() => invoke('check_update_now')}>Check now</button>
    </span>
  </div>
</div>

<style>
  .segmented {
    position: relative;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    padding: 3px;
    border-radius: 9px;
    background: var(--inset);
  }
  .thumb {
    position: absolute;
    inset: 3px auto 3px 3px;
    width: calc((100% - 6px) / 3);
    border-radius: 7px;
    background: var(--surface);
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.12);
    transform: translateX(calc(var(--index) * 100%));
    transition: transform var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
  .segmented button {
    position: relative;
    min-width: 72px;
    padding: 5px 12px;
    border: 0;
    background: none;
    color: var(--mut);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .segmented button[aria-checked='true'] {
    color: var(--fg);
    font-weight: 600;
  }
  select {
    padding: 6px 10px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    font-size: 13px;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .actions button {
    padding: 6px 12px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .actions button.primary {
    border-color: var(--acc);
    background: var(--acc);
    color: #fff;
  }
  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .error {
    color: #d14343;
  }
  .bar {
    display: block;
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
  select:disabled {
    opacity: 0.5;
  }
</style>
