<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import './apps-clean.css';
  import { message } from './util';

  interface Wall {
    path: string;
    name: string;
  }

  let walls = $state<Wall[]>([]);
  let thumbs = $state<Record<string, string>>({});
  let loading = $state(true);
  let error = $state('');
  let applied = $state('');
  let active = $state(0);

  const queue: Wall[] = [];

  async function pump() {
    if (active >= 3) return;
    const next = queue.shift();
    if (!next) return;
    active++;
    try {
      thumbs[next.path] = await invoke<string>('wallpaper_thumb', { path: next.path });
    } catch {
      thumbs[next.path] = '';
    }
    active--;
    pump();
    pump();
  }

  /** Thumbnails are prepared only when a tile scrolls into view. */
  function lazy(node: HTMLElement, wall: Wall) {
    const watcher = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        watcher.disconnect();
        if (!(wall.path in thumbs)) {
          queue.push(wall);
          pump();
        }
      }
    });
    watcher.observe(node);
    return { destroy: () => watcher.disconnect() };
  }

  onMount(async () => {
    try {
      walls = await invoke<Wall[]>('wallpaper_list');
    } catch (e) {
      error = message(e);
    }
    loading = false;
  });

  async function apply(w: Wall) {
    error = '';
    applied = '';
    try {
      await invoke('wallpaper_apply', { path: w.path });
      applied = w.name;
    } catch (e) {
      error = message(e);
    }
  }
</script>

<div class="ac">
  {#if error}<p class="ac-note warn">{error}</p>{/if}
  {#if applied}<p class="ac-note" role="status">Wallpaper set to {applied}.</p>{/if}
  {#if loading}
    <p class="ac-empty"><span class="ac-spin"></span>Looking for pictures</p>
  {:else if !walls.length}
    <p class="ac-empty">No pictures were found in the gallery folders.</p>
  {:else}
    <div class="grid">
      {#each walls as w (w.path)}
        <button class="tile" use:lazy={w} onclick={() => apply(w)} title={w.path} aria-label="Set {w.name} as the wallpaper">
          {#if thumbs[w.path]}
            <img src={thumbs[w.path]} alt="" />
          {:else if w.path in thumbs}
            <span class="miss">Cannot preview</span>
          {:else}
            <span class="miss"></span>
          {/if}
          <span class="cap">{w.name}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 10px;
    max-height: 420px;
    overflow: auto;
    padding: 2px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--surface);
    color: inherit;
    font: inherit;
    overflow: hidden;
    cursor: pointer;
    transition: border-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .tile:hover,
  .tile:focus-visible {
    border-color: var(--acc);
  }
  img,
  .miss {
    display: block;
    width: 100%;
    aspect-ratio: 16 / 10;
    object-fit: cover;
    background: var(--inset);
  }
  .miss {
    display: grid;
    place-items: center;
    color: var(--mut);
    font-size: 12px;
  }
  .cap {
    padding: 0 8px 6px;
    overflow: hidden;
    font-size: 12px;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
