<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, tick } from 'svelte';
  import { applyCssVars } from '../lib/motion';
  import { reportSlots, type Grid, type ThumbKind } from '../switcher/types';

  interface Card {
    id: number;
    title: string;
    app: string;
    minimized: boolean;
    icon: string | null;
    noThumb: boolean;
  }

  interface OpenPayload {
    app: string;
    cards: Card[];
    grid: Grid;
    kind: ThumbKind;
    minimal: boolean;
    closeQuits: boolean;
    opacity: number;
  }

  const MENU: [string, string][] = [
    ['minimise', 'Minimise'],
    ['maximise', 'Maximise'],
    ['snapLeft', 'Snap to left half'],
    ['snapRight', 'Snap to right half'],
    ['nextDisplay', 'Move to next display'],
  ];

  let root = $state<HTMLElement>();
  let app = $state('');
  let cards = $state.raw<Card[]>([]);
  let grid = $state<Grid>({ cols: 1, rows: 1, cardW: 220, cardH: 168, thumbH: 138, width: 480, height: 220 });
  let kind = $state<ThumbKind>('live');
  let minimal = $state(false);
  let closeQuits = $state(false);
  let opacity = $state(100);
  let images = $state<Record<number, string>>({});
  let menuFor = $state<number | null>(null);
  let shown = $state(false);
  let refresh: ReturnType<typeof setInterval> | undefined;

  const wantsSlot = (c: Card) => kind !== 'none' && !c.noThumb && !minimal;

  async function slots() {
    await tick();
    requestAnimationFrame(() => reportSlots(root, 'taskbar_slots'));
  }

  onMount(() => {
    applyCssVars();
    const offs = [
      listen<OpenPayload>('taskbar://open', ({ payload }) => {
        app = payload.app;
        cards = payload.cards;
        grid = payload.grid;
        kind = payload.kind;
        minimal = payload.minimal;
        closeQuits = payload.closeQuits;
        opacity = payload.opacity;
        images = {};
        menuFor = null;
        shown = true;
        clearInterval(refresh);
        if (kind === 'snapshot') refresh = setInterval(() => reportSlots(root, 'taskbar_slots'), 1000);
        slots();
      }),
      listen('taskbar://hide', () => {
        shown = false;
        cards = [];
        menuFor = null;
        clearInterval(refresh);
      }),
      listen<{ id: number; url: string }>('taskbar://image', ({ payload }) => {
        images = { ...images, [payload.id]: payload.url };
      }),
    ];
    return () => {
      clearInterval(refresh);
      offs.forEach((p) => p.then((off) => off()));
    };
  });

  $effect(() => {
    void menuFor;
    if (shown) slots();
  });

  function act(id: number, action: string, e?: Event) {
    e?.stopPropagation();
    menuFor = null;
    invoke('taskbar_action', { id, action });
  }

  function onAux(e: MouseEvent, id: number) {
    if (e.button === 1) {
      e.preventDefault();
      act(id, 'close');
    }
  }
</script>

<div class="panel" class:shown bind:this={root} style:--alpha="{opacity}%">
  {#if cards.length}
    <div class="head">
      {#if cards[0].icon}<img class="ico" src={cards[0].icon} alt="" />{/if}
      <span class="app">{app}</span>
      <span class="count">{cards.length} {cards.length === 1 ? 'window' : 'windows'}</span>
    </div>
    <div class="grid" style:grid-template-columns="repeat({grid.cols}, {grid.cardW}px)">
      {#each cards as c (c.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="cardw"
          class:dim={c.minimized}
          role="button"
          tabindex="-1"
          style:width="{grid.cardW}px"
          style:height="{grid.cardH}px"
          onclick={() => act(c.id, 'focus')}
          onauxclick={(e) => onAux(e, c.id)}
          oncontextmenu={(e) => {
            e.preventDefault();
            menuFor = c.id;
          }}
        >
          <div class="label">
            {#if c.icon}<img class="ico" src={c.icon} alt="" />{/if}
            <span class="title">{c.title || c.app}</span>
            <button class="more" aria-label="More actions" onclick={(e) => { e.stopPropagation(); menuFor = menuFor === c.id ? null : c.id; }}>⋯</button>
            <button class="x" aria-label={closeQuits ? 'Quit app' : 'Close window'} onclick={(e) => act(c.id, 'close', e)}>×</button>
          </div>
          {#if !minimal}
            <div class="slot" data-slot={wantsSlot(c) && menuFor !== c.id ? c.id : undefined} style:height="{grid.thumbH - 6}px">
              {#if !wantsSlot(c)}
                <div class="placeholder">
                  {#if c.icon}<img src={c.icon} alt="" />{/if}
                  {#if c.minimized}<span class="tag">Minimised</span>{/if}
                </div>
              {:else if kind === 'snapshot' && images[c.id]}
                <img class="snap" src={images[c.id]} alt="" />
              {/if}
            </div>
          {/if}
          {#if menuFor === c.id}
            <div class="menu" role="menu">
              {#each MENU as [action, label] (action)}
                <button role="menuitem" onclick={(e) => act(c.id, action, e)}>{label}</button>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  :global(body.taskbar-page) {
    background: transparent;
  }

  .panel {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    padding: 12px 18px 18px;
    border: 1px solid var(--line-strong);
    border-radius: 16px;
    background: color-mix(in srgb, var(--surface) var(--alpha), transparent);
    opacity: 0;
    overflow: hidden;
  }

  .panel.shown {
    opacity: 1;
    transition: opacity var(--m-switcherShow-dur) var(--m-switcherShow-ease);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 4px 6px;
  }

  .app {
    color: var(--fg);
    font-size: 13px;
    font-weight: 600;
  }

  .count {
    margin-left: auto;
    color: var(--mut);
    font-size: 12px;
  }

  .grid {
    display: grid;
    gap: 12px;
    justify-content: center;
  }

  .cardw {
    position: relative;
    display: flex;
    flex-direction: column;
    padding: 3px;
    border: 2px solid transparent;
    border-radius: 12px;
    transition:
      border-color var(--m-hoverLift-dur) var(--m-hoverLift-ease),
      background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }

  .cardw:hover {
    border-color: var(--acc);
    background: var(--acc-soft);
  }

  .cardw.dim .placeholder {
    opacity: 0.6;
  }

  .label {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 27px;
    padding: 0 2px 0 4px;
    min-width: 0;
  }

  .ico {
    flex: none;
    width: 18px;
    height: 18px;
  }

  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--fg-soft);
    font-size: 12.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .slot {
    flex: none;
    border-radius: 8px;
    overflow: hidden;
  }

  .placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 100%;
    border-radius: 8px;
    background: var(--inset);
  }

  .placeholder img {
    width: 44px;
    height: 44px;
  }

  .snap {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .tag {
    padding: 1px 7px;
    border-radius: 99px;
    background: var(--track);
    color: var(--mut);
    font-size: 11px;
  }

  .x,
  .more {
    flex: none;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 99px;
    background: transparent;
    color: var(--mut);
    font: 15px/1 var(--sans);
    opacity: 0;
    cursor: pointer;
    transition: opacity var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }

  .cardw:hover .x,
  .cardw:hover .more {
    opacity: 1;
  }

  .x:hover {
    background: #d6453d;
    color: #fff;
  }

  .more:hover {
    background: var(--hover);
  }

  .menu {
    position: absolute;
    top: 30px;
    right: 6px;
    z-index: 2;
    display: flex;
    flex-direction: column;
    min-width: 170px;
    padding: 4px;
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    background: var(--surface);
  }

  .menu button {
    padding: 6px 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--fg);
    font: inherit;
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
  }

  .menu button:hover {
    background: var(--hover);
  }
</style>
