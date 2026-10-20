<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { fadeSlide } from '../../lib/motion';

  type Item = { id: number; path: string; name: string; folder: boolean; image: boolean };
  type View = { items: Item[]; nativeDrag: boolean };

  const DRAG_START = 5;

  let items = $state.raw<Item[]>([]);
  let nativeDrag = $state(false);
  let warning = $state('');
  let thumbs = $state<Record<number, string>>({});
  let press: { id: number; x: number; y: number; sent: boolean } | null = null;
  let dragged = false;

  async function load() {
    try {
      const v = await invoke<View>('shelf_list');
      items = v.items;
      nativeDrag = v.nativeDrag;
      warning = '';
    } catch (err) {
      warning = String(err);
    }
    for (const item of items) {
      if (item.image && !(item.id in thumbs)) {
        invoke<string | null>('shelf_thumb', { id: item.id })
          .then((url) => url && (thumbs = { ...thumbs, [item.id]: url }))
          .catch(() => {});
      }
    }
  }

  function run(cmd: string, args: Record<string, unknown> = {}) {
    invoke(cmd, args).catch((err) => (warning = String(err)));
  }

  function down(e: PointerEvent, item: Item) {
    dragged = false;
    if (e.button !== 0 || !nativeDrag) return;
    press = { id: item.id, x: e.clientX, y: e.clientY, sent: false };
  }

  function move(e: PointerEvent) {
    if (!press || press.sent || !(e.buttons & 1)) return;
    if (Math.max(Math.abs(e.clientX - press.x), Math.abs(e.clientY - press.y)) < DRAG_START) return;
    press.sent = true;
    dragged = true;
    run('shelf_drag_out', { ids: [press.id] });
  }

  function dragStart(e: DragEvent, item: Item) {
    if (nativeDrag || !e.dataTransfer) return;
    const uri = 'file://' + item.path.split('/').map(encodeURIComponent).join('/');
    e.dataTransfer.setData('text/uri-list', uri);
    e.dataTransfer.setData('text/plain', item.path);
    e.dataTransfer.effectAllowed = 'copyMove';
  }

  function dragEnd(e: DragEvent, item: Item) {
    if (!nativeDrag && e.dataTransfer?.dropEffect !== 'none') run('shelf_drag_out', { ids: [item.id] });
  }

  function open(item: Item) {
    if (dragged) return;
    run('shelf_open', { id: item.id });
  }

  onMount(() => {
    load();
    const t = setInterval(load, 2000);
    const up = () => (press = null);
    window.addEventListener('pointerup', up);
    return () => {
      clearInterval(t);
      window.removeEventListener('pointerup', up);
    };
  });
</script>

<div class="fl">
  <header>
    <h2>Files</h2>
    {#if items.length}<button class="clear" onclick={() => run('shelf_clear')}>Clear shelf</button>{/if}
  </header>

  {#if warning}
    <p class="empty">{warning}</p>
  {:else if !items.length}
    <p class="empty">Drop files on the shelf and they show up here.</p>
  {:else}
    <ul>
      {#each items as item (item.id)}
        <li in:fadeSlide={{ token: 'tabSwitch', y: 6 }}>
          <button
            title={item.path}
            draggable={!nativeDrag}
            onpointerdown={(e) => down(e, item)}
            onpointermove={move}
            ondragstart={(e) => dragStart(e, item)}
            ondragend={(e) => dragEnd(e, item)}
            onclick={() => open(item)}
          >
            {#if thumbs[item.id]}
              <img src={thumbs[item.id]} alt="" />
            {:else}
              <span class="kind">{item.folder ? 'DIR' : (item.name.split('.').pop() ?? '').slice(0, 4).toUpperCase()}</span>
            {/if}
            <span class="name">{item.name}</span>
          </button>
          <button class="x" aria-label="Remove {item.name} from the shelf" onclick={() => run('shelf_remove', { ids: [item.id] })}>x</button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .fl {
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
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--i-mut);
  }
  .clear {
    padding: 2px 8px;
    border: 1px solid var(--i-line);
    border-radius: 6px;
    background: none;
    color: var(--i-fg);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    transition: background var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
  .clear:hover {
    background: var(--i-hover);
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
  li {
    display: flex;
    align-items: center;
    border-radius: 8px;
    transition: background var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
  li:hover {
    background: var(--i-hover);
  }
  li button {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    padding: 5px 8px;
    border: 0;
    background: none;
    color: var(--i-fg);
    font: inherit;
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
  }
  img,
  .kind {
    flex: none;
    display: grid;
    place-items: center;
    width: 28px;
    height: 22px;
    border-radius: 4px;
    object-fit: cover;
    background: var(--i-raised);
    color: var(--i-mut);
    font-size: 9px;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  li .x {
    flex: none;
    padding: 5px 10px;
    color: var(--i-mut);
    opacity: 0;
    transition: opacity var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
  li:hover .x {
    opacity: 1;
  }
</style>
