<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars } from '../lib/motion';

  interface Data {
    kind: 'shot' | 'scroll' | 'link';
    seconds: number;
    autoCopy: boolean;
    width: number;
    height: number;
    text: string;
    link: string | null;
    share: boolean;
  }

  let data = $state<Data | null>(null);
  let thumb = $state('');
  let rows = $state(0);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const act = (action: string) => invoke('capture_action', { action }).catch(() => {});

  async function show(d: Data) {
    clearTimeout(timer);
    data = d;
    rows = d.height;
    if (thumb) URL.revokeObjectURL(thumb);
    thumb = '';
    if (d.kind === 'shot') {
      try {
        const bytes: ArrayBuffer = await invoke('capture_thumb');
        thumb = URL.createObjectURL(new Blob([bytes], { type: 'image/png' }));
      } catch {
        thumb = '';
      }
    }
    if (d.seconds > 0) timer = setTimeout(() => act('dismiss'), d.seconds * 1000);
  }

  onMount(() => {
    applyCssVars();
    const offs = [listen<Data>('capture://preview', (e) => show(e.payload)), listen<number>('capture://scroll', (e) => (rows = e.payload))];
    return () => offs.forEach((p) => p.then((off) => off()));
  });
</script>

{#if data}
  <div class="card">
    {#if data.kind === 'shot'}
      <div class="shot">
        {#if thumb}<img src={thumb} alt="Latest screenshot" />{/if}
      </div>
      <p class="meta">{data.width} x {data.height} px</p>
      <div class="buttons">
        {#if data.autoCopy}<button onclick={() => act('copy')}>Copy</button>{/if}
        <button class="main" onclick={() => act('save')}>{data.autoCopy ? 'Save and copy' : 'Save'}</button>
        <button onclick={() => act('shelf')}>Shelf</button>
        <button onclick={() => act('edit')}>Edit</button>
        <button onclick={() => act('pin')}>Pin</button>
        {#if data.share}<button onclick={() => act('share')}>Share link</button>{/if}
        <button onclick={() => act('text')}>Copy text</button>
        <button class="quiet" onclick={() => act('dismiss')} title="Dismiss">Close</button>
      </div>
    {:else if data.kind === 'scroll'}
      <h2>Scrolling capture</h2>
      <p class="meta">{data.text}</p>
      <p class="meta">{rows} px so far</p>
      <div class="buttons">
        <button class="main" onclick={() => act('stop')}>Stop</button>
      </div>
    {:else}
      <h2>QR code</h2>
      <p class="link">{data.text}</p>
      <div class="buttons">
        <button class="main" onclick={() => act('open-link')}>Open link</button>
        <button onclick={() => act('dismiss')}>Close</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  :global(html),
  :global(body.capture-page) {
    background: transparent;
    overflow: hidden;
    user-select: none;
  }
  .card {
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 8px;
    height: 100%;
    padding: 10px;
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    background: var(--bg);
    color: var(--fg);
    font: 13px/1.4 var(--sans);
  }
  .shot {
    display: flex;
    flex: 1;
    min-height: 84px;
    align-items: center;
    justify-content: center;
    min-height: 0;
    border-radius: 8px;
    background: var(--inset);
    overflow: hidden;
  }
  img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }
  .meta {
    margin: 0;
    color: var(--mut);
    font-size: 12px;
  }
  .link {
    margin: 0;
    overflow-wrap: anywhere;
    font-size: 12px;
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  button {
    padding: 5px 9px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: inherit;
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  button:hover {
    background: var(--hover);
  }
  button.main {
    border-color: transparent;
    background: var(--acc);
    color: var(--acc-fg);
  }
  button.quiet {
    margin-left: auto;
    border-color: transparent;
    background: transparent;
    color: var(--mut);
  }
</style>
