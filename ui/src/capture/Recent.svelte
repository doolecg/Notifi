<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars } from '../lib/motion';

  interface Item {
    path: string;
    name: string;
    kind: 'shot' | 'video';
    time: number;
  }

  let items = $state<Item[]>([]);
  let thumbs = $state<Record<string, string>>({});
  let status = $state('');
  let sure = $state('');

  const isGif = (i: Item) => i.name.toLowerCase().endsWith('.gif');

  function when(secs: number): string {
    const d = new Date(secs * 1000);
    const days = Math.floor((Date.now() - d.getTime()) / 86_400_000);
    const time = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    if (days < 1 && d.getDate() === new Date().getDate()) return `Today ${time}`;
    return `${d.toLocaleDateString([], { day: 'numeric', month: 'short' })} ${time}`;
  }

  async function load() {
    items = await invoke<Item[]>('capture_recent_list');
    for (const i of items) {
      if (i.kind !== 'shot' || thumbs[i.path]) continue;
      invoke<ArrayBuffer>('capture_recent_thumb', { path: i.path })
        .then((bytes) => (thumbs[i.path] = URL.createObjectURL(new Blob([bytes], { type: 'image/png' }))))
        .catch(() => {});
    }
  }

  async function act(item: Item, action: string) {
    sure = '';
    try {
      status = await invoke<string>('capture_recent_action', { path: item.path, action });
    } catch (err) {
      status = String(err);
    }
    if (action === 'delete') await load();
  }

  async function clear() {
    await invoke('capture_recent_clear');
    status = 'List cleared. Your files are still there.';
    await load();
  }

  const close = () => invoke('capture_recent_close').catch(() => {});

  onMount(() => {
    applyCssVars();
    load();
    const off = listen('capture://recent', () => load());
    const key = (e: KeyboardEvent) => e.key === 'Escape' && close();
    window.addEventListener('keydown', key);
    return () => {
      window.removeEventListener('keydown', key);
      off.then((f) => f());
      Object.values(thumbs).forEach((u) => URL.revokeObjectURL(u));
    };
  });
</script>

<div class="recent">
  <header>
    <h1>Recent captures</h1>
    <span class="status">{status}</span>
    <button onclick={clear} disabled={!items.length}>Clear list</button>
    <button onclick={close}>Close</button>
  </header>
  <div class="grid">
    {#each items as item (item.path)}
      <article>
        <div class="thumb">
          {#if thumbs[item.path]}
            <img src={thumbs[item.path]} alt={item.name} draggable="false" />
          {:else}
            <span class="badge">{item.kind === 'video' ? (isGif(item) ? 'GIF' : 'Video') : 'Picture'}</span>
          {/if}
        </div>
        <div class="name" title={item.path}>{item.name}</div>
        <div class="when">{when(item.time)}</div>
        <div class="actions">
          <button onclick={() => act(item, 'open')}>Open</button>
          <button onclick={() => act(item, 'copy')}>{item.kind === 'shot' ? 'Copy' : 'Copy path'}</button>
          <button onclick={() => act(item, 'reveal')}>Show in folder</button>
          {#if item.kind === 'shot'}
            <button onclick={() => act(item, 'edit')}>Edit</button>
            <button onclick={() => act(item, 'pin')}>Pin</button>
          {:else}
            <button onclick={() => act(item, 'trim')}>Trim</button>
          {/if}
          {#if sure === item.path}
            <button class="danger" onclick={() => act(item, 'delete')}>Move to Recycle Bin</button>
            <button onclick={() => (sure = '')}>Keep</button>
          {:else}
            <button onclick={() => (sure = item.path)}>Delete</button>
          {/if}
        </div>
      </article>
    {:else}
      <p class="empty">Nothing here yet. Screenshots and recordings you save show up in this list.</p>
    {/each}
  </div>
</div>

<style>
  :global(body.capture-page) {
    background: var(--bg);
  }
  .recent {
    display: flex;
    flex-direction: column;
    height: 100%;
    color: var(--fg);
    font: 13px/1.4 var(--sans);
  }
  header {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 10px 14px;
    background: var(--sidebar);
  }
  h1 {
    flex: none;
    margin: 0 8px 0 0;
    font-size: 15px;
    font-weight: 600;
  }
  .status {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--mut);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .grid {
    display: grid;
    flex: 1;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    align-content: start;
    gap: 10px;
    min-height: 0;
    padding: 12px 14px;
    overflow-y: auto;
  }
  article {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px;
    border-radius: 10px;
    background: var(--surface);
  }
  .thumb {
    display: grid;
    place-items: center;
    height: 130px;
    overflow: hidden;
    border-radius: 7px;
    background: var(--inset);
  }
  .thumb img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .badge {
    color: var(--mut);
  }
  .name {
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .when {
    color: var(--mut);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 2px;
  }
  button {
    padding: 4px 8px;
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
  button.danger {
    border-color: var(--warn);
    color: var(--warn);
  }
  .empty {
    grid-column: 1 / -1;
    color: var(--mut);
  }
</style>
