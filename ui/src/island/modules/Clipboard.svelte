<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { fadeSlide } from '../../lib/motion';
  import { ago, search, title, type Entry, type ListResult } from '../../clipboard/entries';

  const SHOWN = 8;

  let entries = $state.raw<Entry[]>([]);
  let query = $state('');
  let copied = $state<number | null>(null);
  let warning = $state('');
  let thumbs = $state<Record<number, string>>({});
  let now = $state(Date.now());

  const list = $derived(search(entries, query).slice(0, SHOWN));

  async function load() {
    try {
      const r = await invoke<ListResult>('clipboard_list');
      entries = r.entries;
      warning = '';
    } catch (err) {
      warning = String(err);
    }
    now = Date.now();
    for (const e of entries.slice(0, 40)) {
      if (e.kind === 'image' && !(e.id in thumbs)) {
        invoke<string | null>('clipboard_image', { id: e.id, full: false })
          .then((url) => url && (thumbs = { ...thumbs, [e.id]: url }))
          .catch(() => {});
      }
    }
  }

  function copy(e: Entry) {
    invoke('clipboard_copy', { ids: [e.id] })
      .then(() => {
        copied = e.id;
        setTimeout(() => copied === e.id && (copied = null), 1200);
        load();
      })
      .catch((err) => (warning = String(err)));
  }

  onMount(() => {
    load();
    const t = setInterval(load, 2000);
    return () => clearInterval(t);
  });
</script>

<div class="cb">
  <header>
    <h2>Clipboard</h2>
    <input type="search" placeholder="Search" aria-label="Search clipboard history" bind:value={query} />
  </header>

  {#if warning}
    <p class="empty">{warning}</p>
  {:else if !list.length}
    <p class="empty">{query ? 'No matches' : 'Nothing copied yet'}</p>
  {:else}
    <ul>
      {#each list as e (e.id)}
        <li in:fadeSlide={{ token: 'tabSwitch', y: 6 }}>
          <button onclick={() => copy(e)} title="Copy back">
            {#if e.kind === 'image'}
              {#if thumbs[e.id]}<img src={thumbs[e.id]} alt="" />{:else}<span class="ph"></span>{/if}
            {:else if e.kind === 'color'}
              <span class="sw" style:background={e.text.trim()}></span>
            {/if}
            <span class="txt">{title(e) || '(blank)'}</span>
            <span class="meta">{copied === e.id ? 'Copied' : ago(e.created, now)}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .cb {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: 8px;
    min-height: 0;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--i-mut);
  }
  input {
    width: 150px;
    padding: 3px 8px;
    border: 1px solid var(--i-line);
    border-radius: 6px;
    background: var(--i-raised);
    color: var(--i-fg);
    font: inherit;
    font-size: 12px;
  }
  .empty {
    margin: auto;
    color: var(--i-mut);
    text-align: center;
  }
  ul {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
    scrollbar-width: none;
  }
  li button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 5px 8px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--i-fg);
    font: inherit;
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
    transition: background var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
  li button:hover {
    background: var(--i-hover);
  }
  li button:active {
    background: var(--i-raised);
  }
  img,
  .ph,
  .sw {
    flex: none;
    width: 28px;
    height: 20px;
    border-radius: 4px;
    object-fit: cover;
    background: var(--i-raised);
  }
  .txt {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .meta {
    flex: none;
    font-size: 11px;
    color: var(--i-mut);
  }
</style>
