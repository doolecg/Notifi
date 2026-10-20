<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { fadeSlide } from '../../lib/motion';
  import Glyph from '../Glyph.svelte';

  let { kind }: { kind: 'downloads' | 'captures' } = $props();

  type Row = { name: string; path: string; modifiedMs: number; size: number; image: boolean };
  let rows = $state<Row[]>([]);
  let loaded = $state(false);
  let thumbs = $state<Record<string, string>>({});
  const grid = $derived(kind === 'captures');
  const title = $derived(kind === 'captures' ? 'Recent captures' : 'Downloads');

  const ago = (ms: number) => {
    const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
    if (s < 60) return 'just now';
    if (s < 3600) return `${Math.floor(s / 60)} min ago`;
    if (s < 86400) return `${Math.floor(s / 3600)} hr ago`;
    return `${Math.floor(s / 86400)} d ago`;
  };
  const size = (b: number) => (b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : b >= 1e6 ? `${(b / 1e6).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1e3))} KB`);

  async function load() {
    const next = await invoke<Row[]>('island_recent_files', { kind }).catch(() => []);
    rows = next;
    loaded = true;
    for (const r of next) {
      if (r.image && !(r.path in thumbs)) {
        invoke<string | null>('island_file_thumb', { path: r.path })
          .then((t) => {
            if (t) thumbs[r.path] = t;
          })
          .catch(() => {});
      }
    }
  }

  const open = (r: Row, reveal = false) => invoke('island_open_path', { path: r.path, reveal }).catch(() => {});

  onMount(() => {
    load();
    const id = setInterval(load, 3000);
    return () => clearInterval(id);
  });
</script>

<div class="rf">
  <header><h2>{title}</h2></header>
  {#if loaded && !rows.length}
    <p class="empty">{kind === 'captures' ? 'No screenshots in Pictures\\Screenshots yet.' : 'Nothing in Downloads yet.'}</p>
  {:else if grid}
    <div class="tiles">
      {#each rows as r (r.path)}
        <button
          class="tile"
          title={`${r.name} · ${ago(r.modifiedMs)}`}
          onclick={() => open(r)}
          oncontextmenu={(e) => {
            e.preventDefault();
            open(r, true);
          }}
          in:fadeSlide={{ token: 'tabSwitch' }}
        >
          {#if thumbs[r.path]}<img src={thumbs[r.path]} alt="" />{:else}<span class="ph"><Glyph name="captures" size={18} /></span>{/if}
          <small>{ago(r.modifiedMs)}</small>
        </button>
      {/each}
    </div>
  {:else}
    <div class="list">
      {#each rows as r (r.path)}
        <button
          class="row"
          onclick={() => open(r)}
          oncontextmenu={(e) => {
            e.preventDefault();
            open(r, true);
          }}
          in:fadeSlide={{ token: 'tabSwitch' }}
        >
          <span class="ico"><Glyph name={r.image ? 'captures' : 'download'} size={15} /></span>
          <span class="nm" title={r.name}>{r.name}</span>
          <span class="meta">{size(r.size)} · {ago(r.modifiedMs)}</span>
        </button>
      {/each}
    </div>
  {/if}
  {#if rows.length}<p class="hint">Click to open. Right-click to show in its folder.</p>{/if}
</div>

<style>
  .rf {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    gap: 8px;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--i-mut);
  }
  .empty {
    margin: auto;
    color: var(--i-mut);
  }
  .list,
  .tiles {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row {
    display: grid;
    grid-template-columns: 22px 1fr auto;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--i-fg);
    font: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  .row:hover,
  .tile:hover {
    background: var(--i-hover);
  }
  .ico {
    display: grid;
    place-items: center;
    color: var(--i-mut);
  }
  .nm {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .meta {
    color: var(--i-mut);
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
    grid-auto-rows: min-content;
    gap: 8px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 4px;
    border: 0;
    border-radius: 10px;
    background: none;
    color: var(--i-mut);
    font: inherit;
    cursor: pointer;
  }
  .tile img,
  .ph {
    width: 100%;
    aspect-ratio: 16 / 10;
    border-radius: 8px;
    object-fit: cover;
    background: var(--i-raised);
  }
  .ph {
    display: grid;
    place-items: center;
  }
  .tile small {
    font-size: 11px;
  }
  .hint {
    margin: 0;
    font-size: 11.5px;
    color: var(--i-mut);
  }
</style>
