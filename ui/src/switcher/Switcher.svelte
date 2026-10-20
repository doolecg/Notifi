<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, tick } from 'svelte';
  import { applyCssVars, duration } from '../lib/motion';
  import { groupsOf, reportSlots, type Entry, type Grid, type Layout, type OpenPayload, type ThumbKind, type UpdatePayload } from './types';

  const PAD = 18;
  const GAP = 12;
  const ICON_CELL = 92;

  let root = $state<HTMLElement>();
  let entries = $state.raw<Entry[]>([]);
  let visible = $state.raw<number[]>([]);
  let layout = $state<Layout>('previews');
  let grid = $state<Grid>({ cols: 1, rows: 1, cardW: 220, cardH: 168, thumbH: 138, width: 640, height: 260 });
  let kind = $state<ThumbKind>('live');
  let query = $state('');
  let selected = $state(0);
  let group = $state(0);
  let groupWindow = $state(0);
  let hoverButtons = $state(true);
  let skip = $state(false);
  let wheel = $state(true);
  let images = $state<Record<number, string>>({});
  let phase = $state<'hidden' | 'shown' | 'leaving'>('hidden');
  let fade = $state(300);
  let refresh: ReturnType<typeof setInterval> | undefined;

  const groups = $derived(groupsOf(entries, visible));
  const current = $derived.by(() => {
    if (layout === 'icons') return groups[group]?.items[groupWindow];
    return visible[selected];
  });
  const previewItems = $derived(layout === 'icons' ? (groups[group]?.items ?? []) : visible);
  const minimal = $derived(grid.thumbH === 0);
  const showMs = $derived(skip ? 0 : duration('switcherShow'));
  const selMs = $derived(skip ? 0 : duration('selectionMove'));

  async function slots() {
    await tick();
    requestAnimationFrame(() => reportSlots(root, 'switcher_slots'));
  }

  function wantsSlot(e: Entry) {
    return kind !== 'none' && !e.noThumb && !minimal && layout !== 'simple';
  }

  onMount(() => {
    applyCssVars();
    const offs = [
      listen<OpenPayload>('switcher://open', ({ payload }) => {
        entries = payload.model.entries;
        visible = payload.model.visible;
        layout = payload.model.layout;
        query = payload.model.query;
        selected = payload.model.selected;
        group = payload.model.group;
        groupWindow = payload.model.groupWindow;
        grid = payload.grid;
        kind = payload.kind;
        hoverButtons = payload.hoverButtons;
        skip = payload.skipAnimations;
        wheel = payload.wheel;
        images = {};
        phase = 'shown';
        clearInterval(refresh);
        if (kind === 'snapshot') refresh = setInterval(() => reportSlots(root, 'switcher_slots'), 1000);
        slots();
      }),
      listen<UpdatePayload>('switcher://update', ({ payload }) => {
        if (!payload) return slots();
        const reflow = payload.visible.join() !== visible.join() || payload.group !== group;
        query = payload.query;
        visible = payload.visible;
        selected = payload.selected;
        group = payload.group;
        groupWindow = payload.groupWindow;
        if (reflow) slots();
      }),
      listen<number>('switcher://hide', ({ payload }) => {
        fade = payload;
        phase = 'leaving';
        clearInterval(refresh);
        setTimeout(() => {
          if (phase === 'leaving') {
            phase = 'hidden';
            entries = [];
            visible = [];
            images = {};
          }
        }, payload);
      }),
      listen<{ id: number; url: string }>('switcher://image', ({ payload }) => {
        images = { ...images, [payload.id]: payload.url };
      }),
    ];
    return () => {
      clearInterval(refresh);
      offs.forEach((p) => p.then((off) => off()));
    };
  });

  function onWheel(e: WheelEvent) {
    if (!wheel || phase !== 'shown') return;
    const d = Math.abs(e.deltaY) >= Math.abs(e.deltaX) ? e.deltaY : e.deltaX;
    if (d) invoke('switcher_step', { delta: d > 0 ? 1 : -1 });
  }

  function onAux(e: MouseEvent, id: number) {
    if (e.button === 1) {
      e.preventDefault();
      invoke('switcher_close_window', { id });
    }
  }

  function closeWin(e: MouseEvent, id: number) {
    e.stopPropagation();
    invoke('switcher_close_window', { id });
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="panel"
  class:shown={phase === 'shown'}
  class:leaving={phase === 'leaving'}
  bind:this={root}
  onwheel={onWheel}
  style:--show="{showMs}ms"
  style:--fade="{fade}ms"
  style:--sel="{selMs}ms"
  style:--pad="{PAD}px"
  style:--gap="{GAP}px"
>
  {#if entries.length}
    <div class="search" class:empty={!query}>
      <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="6.5" /><path d="m16 16 4.5 4.5" /></svg>
      <span class="q">{query || 'Type to search'}</span>
      {#if query}<span class="caret"></span>{/if}
      <span class="count">{visible.length} {visible.length === 1 ? 'window' : 'windows'}</span>
    </div>

    {#if layout === 'simple'}
      <div class="list">
        {#each visible as i, pos (entries[i].id)}
          {@const e = entries[i]}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            class="row"
            class:sel={pos === selected}
            role="option"
            tabindex="-1"
            aria-selected={pos === selected}
            onmouseenter={() => invoke('switcher_hover', { id: e.id })}
            onclick={() => invoke('switcher_pick', { id: e.id })}
            onauxclick={(ev) => onAux(ev, e.id)}
          >
            {#if e.icon}<img class="ico" src={e.icon} alt="" />{:else}<span class="ico blank"></span>{/if}
            <span class="app">{e.app}</span>
            <span class="title">{e.title}</span>
            {#if e.minimized}<span class="tag">Minimised</span>{/if}
            {#if hoverButtons}<button class="x" aria-label="Close window" onclick={(ev) => closeWin(ev, e.id)}>×</button>{/if}
          </div>
        {/each}
      </div>
    {:else}
      <div class="grid" style:grid-template-columns="repeat({Math.min(grid.cols, Math.max(previewItems.length, 1))}, {grid.cardW}px)">
        {#each previewItems as i (entries[i].id)}
          {@const e = entries[i]}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            class="cardw"
            class:sel={i === current}
            role="option"
            tabindex="-1"
            aria-selected={i === current}
            style:width="{grid.cardW}px"
            style:height="{grid.cardH}px"
            onmouseenter={() => invoke('switcher_hover', { id: e.id })}
            onclick={() => invoke('switcher_pick', { id: e.id })}
            onauxclick={(ev) => onAux(ev, e.id)}
          >
            {#if !minimal}
              <div class="slot" data-slot={wantsSlot(e) ? e.id : undefined} style:height="{grid.thumbH - 6}px">
                {#if !wantsSlot(e)}
                  <div class="placeholder">
                    {#if e.icon}<img src={e.icon} alt="" />{/if}
                    {#if e.minimized}<span class="tag">Minimised</span>{/if}
                  </div>
                {:else if kind === 'snapshot'}
                  {#if images[e.id]}<img class="snap" src={images[e.id]} alt="" />{:else if e.icon}<div class="placeholder"><img src={e.icon} alt="" /></div>{/if}
                {/if}
              </div>
            {/if}
            <div class="label">
              {#if e.icon}<img class="ico" src={e.icon} alt="" />{:else}<span class="ico blank"></span>{/if}
              <span class="title">{e.title || e.app}</span>
              {#if hoverButtons}<button class="x" aria-label="Close window" onclick={(ev) => closeWin(ev, e.id)}>×</button>{/if}
            </div>
          </div>
        {/each}
      </div>
      {#if layout === 'icons'}
        <div class="icons" style:--cell="{ICON_CELL}px">
          {#each groups as g, gi (g.app)}
            {@const first = entries[g.items[0]]}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <div
              class="appicon"
              class:sel={gi === group}
              role="option"
              tabindex="-1"
              aria-selected={gi === group}
              onmouseenter={() => invoke('switcher_hover', { id: first.id })}
              onclick={() => invoke('switcher_pick', { id: first.id })}
            >
              {#if first.icon}<img src={first.icon} alt="" />{:else}<span class="ico blank big"></span>{/if}
              <span class="name">{g.app}</span>
              {#if g.items.length > 1}<span class="badge">{g.items.length}</span>{/if}
            </div>
          {/each}
        </div>
      {/if}
    {/if}
  {/if}
</div>

<style>
  :global(body.switcher-page) {
    background: transparent;
  }

  .panel {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    padding: var(--pad);
    border: 1px solid var(--line-strong);
    border-radius: 18px;
    background: color-mix(in srgb, var(--surface) 94%, transparent);
    box-shadow: 0 1px 0 rgb(255 255 255 / 0.04) inset;
    opacity: 0;
    overflow: hidden;
  }

  .panel.shown {
    opacity: 1;
    transition: opacity var(--show) var(--m-switcherShow-ease);
  }

  .panel.leaving {
    opacity: 0;
    transition: opacity var(--fade) var(--m-switcherHide-ease);
  }

  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    margin-top: -6px;
    padding: 0 4px 8px;
    color: var(--fg);
    font-size: 13.5px;
  }

  .search svg {
    width: 16px;
    height: 16px;
    fill: none;
    stroke: var(--mut);
    stroke-width: 2;
    stroke-linecap: round;
  }

  .search.empty .q {
    color: var(--mut);
  }

  .caret {
    width: 1.5px;
    height: 16px;
    margin-left: -6px;
    background: var(--acc);
  }

  .count {
    margin-left: auto;
    color: var(--mut);
    font-size: 12px;
  }

  .grid {
    display: grid;
    gap: var(--gap);
    justify-content: center;
    align-content: start;
    overflow: hidden;
  }

  .cardw {
    display: flex;
    flex-direction: column;
    padding: 3px;
    border: 2px solid transparent;
    border-radius: 12px;
    cursor: default;
    transition:
      border-color var(--sel) var(--m-selectionMove-ease),
      background-color var(--sel) var(--m-selectionMove-ease);
  }

  .cardw.sel {
    border-color: var(--acc);
    background: var(--acc-soft);
  }

  .slot {
    position: relative;
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
    width: 48px;
    height: 48px;
  }

  .snap {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .label {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 27px;
    padding: 0 4px;
    min-width: 0;
  }

  .ico {
    flex: none;
    width: 18px;
    height: 18px;
  }

  .ico.blank {
    border-radius: 5px;
    background: var(--track);
  }

  .ico.blank.big {
    width: 48px;
    height: 48px;
    border-radius: 12px;
  }

  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--fg-soft);
    font-size: 12.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
    transition: color var(--sel) var(--m-selectionMove-ease);
  }

  .sel .title {
    color: var(--fg);
    font-weight: 600;
  }

  .tag {
    padding: 1px 7px;
    border-radius: 99px;
    background: var(--track);
    color: var(--mut);
    font-size: 11px;
  }

  .x {
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
  .row:hover .x {
    opacity: 1;
  }

  .x:hover {
    background: #d6453d;
    color: #fff;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow: hidden;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 42px;
    padding: 0 10px;
    border: 2px solid transparent;
    border-radius: 10px;
    transition:
      border-color var(--sel) var(--m-selectionMove-ease),
      background-color var(--sel) var(--m-selectionMove-ease);
  }

  .list > .row {
    min-height: 0;
    border-top: 2px solid transparent;
  }

  .row.sel {
    border-color: var(--acc);
    background: var(--acc-soft);
  }

  .row .ico {
    width: 22px;
    height: 22px;
  }

  .row .app {
    flex: none;
    max-width: 30%;
    overflow: hidden;
    color: var(--fg);
    font-weight: 600;
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .icons {
    display: flex;
    justify-content: center;
    gap: 0;
    margin-top: auto;
    padding-top: 10px;
  }

  .appicon {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    width: var(--cell);
    padding: 10px 4px 6px;
    border: 2px solid transparent;
    border-radius: 14px;
    transition:
      border-color var(--sel) var(--m-selectionMove-ease),
      background-color var(--sel) var(--m-selectionMove-ease);
  }

  .appicon.sel {
    border-color: var(--acc);
    background: var(--acc-soft);
  }

  .appicon img {
    width: 48px;
    height: 48px;
  }

  .appicon .name {
    max-width: 100%;
    overflow: hidden;
    color: var(--fg-soft);
    font-size: 11.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge {
    position: absolute;
    top: 4px;
    right: 12px;
    min-width: 18px;
    padding: 0 5px;
    border-radius: 99px;
    background: var(--acc);
    color: var(--acc-fg);
    font-size: 11px;
    font-weight: 700;
    text-align: center;
  }
</style>
