<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { fadeSlide } from '../lib/motion';
  import './apps-clean.css';
  import { bytes, leaf, message } from './util';

  interface Offer {
    path: string;
    name: string;
    kind: 'msi' | 'exe' | 'zip' | 'appimage';
    size: number;
  }
  interface Inspect {
    kind: string;
    silent: string | null;
    signature: string;
  }
  interface Installed {
    ok: boolean;
    code: number | null;
    message: string;
    program: string | null;
    choices: string[];
  }
  interface AppImageDone {
    name: string;
    path: string;
    desktop: string;
  }

  let { values }: { values: Record<string, unknown> } = $props();

  const linux = $derived(/linux/i.test(navigator.userAgent));
  let offers = $state<Offer[]>([]);
  let busy = $state<string | null>(null);
  let warn = $state<{ offer: Offer; signature: string } | null>(null);
  let result = $state<{ offer: Offer; text: string; ok: boolean; program: string | null; choices: string[] } | null>(null);
  let error = $state('');
  let desktopShortcut = $state(false);

  const remember = $derived(values.rememberAnswers === true);
  const ask = (key: string) => String(values[key] ?? 'ask');

  const KEY = 'notifi.appimage.answers';
  function saved(): { remove?: boolean; reveal?: boolean } {
    try {
      return JSON.parse(localStorage.getItem(KEY) ?? '{}');
    } catch {
      return {};
    }
  }
  function save(patch: { remove?: boolean; reveal?: boolean }) {
    try {
      if (remember) localStorage.setItem(KEY, JSON.stringify({ ...saved(), ...patch }));
    } catch {
      /* storage can be unavailable */
    }
  }

  async function refresh() {
    offers = await invoke<Offer[]>('installer_offers').catch(() => []);
  }

  onMount(() => {
    refresh();
    const off = listen('apps-clean://offers', refresh);
    const timer = setInterval(refresh, 8000);
    return () => {
      off.then((f) => f());
      clearInterval(timer);
    };
  });

  async function start(offer: Offer, trusted = false) {
    error = '';
    result = null;
    busy = offer.path;
    try {
      if (offer.kind === 'appimage') {
        const done = await invoke<AppImageDone>('appimage_install', { path: offer.path });
        result = { offer, text: `${done.name} was installed to ${done.path} and added to the menu.`, ok: true, program: done.path, choices: [] };
        await refresh();
      } else {
        if (!trusted && offer.kind !== 'zip') {
          const info = await invoke<Inspect>('installer_inspect', { path: offer.path });
          if (info.signature && info.signature !== 'Valid') {
            warn = { offer, signature: info.signature };
            busy = null;
            return;
          }
        }
        warn = null;
        const done = await invoke<Installed>('installer_install', { path: offer.path });
        result = { offer, text: done.message, ok: done.ok, program: done.program, choices: done.choices };
        if (done.ok && ask('removeDownload') === 'always') await trash(offer);
      }
    } catch (e) {
      error = message(e);
    }
    busy = null;
  }

  async function trash(offer: Offer) {
    try {
      await invoke('installer_trash_download', { path: offer.path });
      if (result) result = { ...result, text: `${result.text} The download was moved to the Recycle Bin.` };
      await refresh();
    } catch (e) {
      error = message(e);
    }
  }

  async function addShortcut(program: string) {
    try {
      await invoke('installer_shortcut', { program, desktop: desktopShortcut });
      if (result) result = { ...result, program, choices: [], text: `${result.text}. A Start menu shortcut was added.` };
    } catch (e) {
      error = message(e);
    }
  }
</script>

<div class="ac">
  <p class="ac-note" style="margin-top:0">
    {linux ? 'AppImages that land in Downloads appear here.' : 'Installers (.msi, .exe) and portable archives (.zip) that land in Downloads appear here.'}
  </p>
  {#if error}<p class="ac-note warn">{error}</p>{/if}
  <div class="ac-list">
    {#if !offers.length}<p class="ac-empty">Nothing to install right now.</p>{/if}
    {#each offers as o (o.path)}
      <div class="ac-item" in:fadeSlide={{ token: 'listReorder', y: 4 }}>
        <span class="ac-main"><b>{o.name}</b><small title={o.path}>{leaf(o.path)} · {bytes(o.size)}</small></span>
        <span class="ac-chip">{o.kind}</span>
        <button class="primary" disabled={busy !== null} onclick={() => start(o)}>
          {#if busy === o.path}<span class="ac-spin"></span>Installing{:else}Install{/if}
        </button>
      </div>
    {/each}
  </div>

  {#if warn}
    <p class="ac-note warn">
      {warn.offer.name} is not signed by a trusted publisher ({warn.signature}). Only run it if you trust where it came from.
    </p>
    <div class="ac-bar">
      <button class="ghost" onclick={() => (warn = null)}>Cancel</button>
      <button class="primary danger" onclick={() => warn && start(warn.offer, true)}>Run it anyway</button>
    </div>
  {/if}

  {#if result}
    <div in:fadeSlide={{ token: 'listReorder', y: 4 }}>
      <p class="ac-note" class:warn={!result.ok}>{result.text}</p>
      {#if result.ok && result.choices.length}
        <p class="ac-note">Several programs were found. Choose the one to add to the Start menu.</p>
        <label class="ac-note"><input type="checkbox" bind:checked={desktopShortcut} /> Also put one on the desktop</label>
        {#each result.choices as p (p)}
          <div class="ac-item"><span class="ac-main"><b>{leaf(p)}</b><small>{p}</small></span><button class="ghost" onclick={() => addShortcut(p)}>Add</button></div>
        {/each}
      {:else if result.ok && result.program && !linux && result.offer.kind === 'zip'}
        <div class="ac-bar"><button class="ghost" onclick={() => result?.program && addShortcut(result.program)}>Add to the Start menu</button></div>
      {/if}
      {#if result.ok}
        <div class="ac-bar">
          {#if result.offer.kind !== 'appimage' && ask('removeDownload') === 'ask'}
            <button class="ghost" onclick={() => trash(result!.offer)}>Delete the download</button>
          {/if}
          {#if result.program && (ask('revealProgram') === 'ask' || (linux && saved().reveal !== false))}
            <button class="ghost" onclick={() => { save({ reveal: true }); invoke('apps_clean_reveal', { path: result!.program }); }}>Show the program</button>
          {/if}
          {#if result.program && result.offer.kind !== 'zip' || linux}
            <button class="ghost" onclick={() => result?.program && invoke('installer_launch', { program: result.program })}>Open it</button>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>
