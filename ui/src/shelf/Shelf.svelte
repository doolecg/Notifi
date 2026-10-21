<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars, setReducedMotion } from '../lib/motion';
  import { store } from '../lib/settings.svelte';
  import Glyph from '../clipboard/Glyph.svelte';

  type Item = { id: number; path: string; name: string; folder: boolean; image: boolean };
  type Mode = 'hidden' | 'peek' | 'open';
  type View = { items: Item[]; mode: Mode; edge: string; nativeDrag: boolean };

  /** How far the pointer moves on an item before it is dragged out. */
  const DRAG_START = 5;

  let items = $state.raw<Item[]>([]);
  let mode = $state<Mode>('open');
  let edge = $state('right');
  let nativeDrag = $state(false);
  let hover = $state(false);
  let warning = $state('');
  let thumbs = $state<Record<number, string>>({});
  let press: { id: number; x: number; y: number; sent: boolean } | null = null;

  const across = $derived(edge === 'top' || edge === 'bottom');

  async function load() {
    try {
      const v = await invoke<View>('shelf_list');
      items = v.items;
      mode = v.mode === 'hidden' ? mode : v.mode;
      edge = v.edge;
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
    if (e.button !== 0 || !nativeDrag || (e.target as Element).closest('button')) return;
    press = { id: item.id, x: e.clientX, y: e.clientY, sent: false };
  }

  function move(e: PointerEvent) {
    if (!press || press.sent || !(e.buttons & 1)) return;
    if (Math.max(Math.abs(e.clientX - press.x), Math.abs(e.clientY - press.y)) < DRAG_START) return;
    press.sent = true;
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

  $effect(() => {
    const theme = store.settings?.general.theme;
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
  });

  $effect(() => setReducedMotion(store.settings?.general.reducedMotion ?? false));

  onMount(() => {
    applyCssVars();
    store.init().catch(() => {});
    load();
    const offs = [
      listen('shelf://changed', load),
      listen<{ mode: Mode; edge: string }>('shelf://mode', (e) => {
        mode = e.payload.mode;
        edge = e.payload.edge;
        load();
      }),
      listen<boolean>('shelf://hover', (e) => (hover = e.payload)),
    ];
    const up = () => (press = null);
    window.addEventListener('pointerup', up);
    return () => {
      offs.forEach((off) => off.then((f) => f()));
      window.removeEventListener('pointerup', up);
    };
  });
</script>

<div class="card" class:hover class:across class:peek={mode === 'peek'} role="region" aria-label="Shelf">
  {#if mode === 'peek'}
    <button class="icon corner" title="Close" aria-label="Close" onclick={() => run('shelf_hide')}>
      <Glyph name="close" size={14} />
    </button>
    <div class="peek-mark">
      <Glyph name="folder" size={22} />
      <span>Drop here</span>
    </div>
  {:else}
    <header>
      <strong>Shelf</strong>
      {#if items.length}<span class="count">{items.length}</span>{/if}
      <span class="grow"></span>
      {#if items.length}
        <button class="ghost" title="Remove everything" onclick={() => run('shelf_clear')}>Clear</button>
      {/if}
      <button class="icon" title="Close" aria-label="Close" onclick={() => run('shelf_hide')}>
        <Glyph name="close" size={14} />
      </button>
    </header>

    {#if items.length}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <ul class="items" onpointermove={move}>
        {#each items as item (item.id)}
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <li
            class="item"
            title={item.path}
            draggable={!nativeDrag}
            onpointerdown={(e) => down(e, item)}
            ondragstart={(e) => dragStart(e, item)}
            ondragend={(e) => dragEnd(e, item)}
            ondblclick={() => run('shelf_open', { id: item.id })}
          >
            <span class="thumb">
              {#if thumbs[item.id]}
                <img src={thumbs[item.id]} alt="" draggable="false" />
              {:else}
                <Glyph name={item.folder ? 'folder' : item.image ? 'image' : 'files'} size={20} />
              {/if}
            </span>
            <span class="name">{item.name}</span>
            <span class="actions">
              <button class="icon" title="Open" aria-label="Open {item.name}" onclick={() => run('shelf_open', { id: item.id })}>
                <Glyph name="open" size={14} />
              </button>
              <button class="icon" title="Remove" aria-label="Remove {item.name}" onclick={() => run('shelf_remove', { ids: [item.id] })}>
                <Glyph name="close" size={14} />
              </button>
            </span>
          </li>
        {/each}
      </ul>
    {:else}
      <div class="empty">
        <span class="empty-mark"><Glyph name="folder" size={22} /></span>
        <strong>Nothing here yet</strong>
        <span>Drop files here to keep them while you find where they go. Drag them out again when you are ready.</span>
      </div>
    {/if}

    {#if warning}
      <button class="warning" onclick={() => (warning = '')}>
        <Glyph name="warning" size={14} />
        <span>{warning}</span>
      </button>
    {/if}
  {/if}
</div>

<style>
  :global(html),
  :global(body.shelf-page) {
    background: transparent;
    overflow: hidden;
  }
  .card {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line-strong);
    border-radius: 14px;
    background: var(--bg);
    color: var(--fg);
    overflow: hidden;
    user-select: none;
    transition:
      border-color var(--m-shelfDropHighlight-dur) var(--m-shelfDropHighlight-ease),
      background var(--m-shelfDropHighlight-dur) var(--m-shelfDropHighlight-ease);
  }
  .card.hover {
    border-color: var(--acc);
    background: color-mix(in srgb, var(--acc-soft) 50%, var(--bg));
  }
  .peek-mark {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--acc);
    font-size: 12px;
    text-align: center;
  }
  .across .peek-mark {
    flex-direction: row;
  }
  .corner {
    position: absolute;
    top: 6px;
    right: 6px;
  }
  header {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 10px 8px 14px;
    border-bottom: 1px solid var(--line);
  }
  .count {
    min-width: 20px;
    padding: 1px 6px;
    border-radius: 10px;
    background: var(--surface);
    color: var(--mut);
    font-size: 12px;
    text-align: center;
  }
  .grow {
    flex: 1;
  }
  button {
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .ghost {
    padding: 3px 8px;
    border: 1px solid var(--line);
    border-radius: 7px;
    background: transparent;
    font-size: 12.5px;
  }
  .ghost:hover,
  .icon:hover {
    background: var(--hover);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--mut);
  }
  .items {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 6px;
    list-style: none;
    overflow-y: auto;
  }
  .across .items {
    flex-direction: row;
    overflow-x: auto;
    overflow-y: hidden;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 6px 6px 8px;
    border-radius: 9px;
    cursor: grab;
  }
  .across .item {
    flex: none;
    flex-direction: column;
    width: 112px;
    gap: 6px;
    padding: 8px 6px;
    text-align: center;
  }
  .item:hover {
    background: var(--hover);
  }
  .thumb {
    flex: none;
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 7px;
    background: var(--surface);
    color: var(--mut);
    overflow: hidden;
  }
  .across .thumb {
    width: 64px;
    height: 64px;
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
  }
  .across .name {
    width: 100%;
    font-size: 12px;
  }
  .item {
    position: relative;
  }
  .actions {
    position: absolute;
    top: 50%;
    right: 6px;
    display: flex;
    border-radius: 7px;
    background: var(--bg);
    opacity: 0;
    transform: translateY(-50%);
  }
  .across .actions {
    top: 4px;
    right: 4px;
    transform: none;
  }
  .item:hover .actions,
  .item:focus-within .actions {
    opacity: 1;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 16px;
    color: var(--mut);
    font-size: 12.5px;
    text-align: center;
  }
  .empty strong {
    color: var(--fg);
    font-size: 13.5px;
  }
  .empty-mark {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    margin-bottom: 4px;
    border-radius: 12px;
    background: var(--surface);
  }
  .warning {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 8px 8px;
    padding: 8px 10px;
    border: 0;
    border-radius: 9px;
    background: var(--toast);
    color: var(--toast-fg);
    font-size: 12.5px;
    text-align: left;
  }
</style>
