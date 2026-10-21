<script lang="ts">
  import { store } from '../../lib/settings.svelte';
  import Glyph from '../Glyph.svelte';
  import { island } from '../state.svelte';
  import { MODULE_ACTIONS, MODULE_ICONS } from '../types';

  let { onOpen, onBack, onSetDefault }: { onOpen: (id: string) => void; onBack: () => void; onSetDefault: (id: string) => void } =
    $props();

  /** One tile's room; the page holds as many as fit, so a wide, short menu shows every module at once. */
  const TILE_W = 124;
  const TILE_H = 64;
  let pagerW = $state(0);
  let pagerH = $state(0);
  const cols = $derived(Math.max(1, Math.floor((pagerW + 4) / TILE_W)));
  const rows = $derived(Math.max(1, Math.floor((pagerH + 4) / TILE_H)));
  const PER_PAGE = $derived(cols * rows);
  const snap = $derived(island.snap!);
  let query = $state('');
  let searching = $state(false);
  let menu = $state(false);
  let page = $state(0);

  const listed = $derived(
    snap.config.modules
      .map((id) => snap.modules.find((m) => m.id === id))
      .filter((m) => m !== undefined)
      .filter((m) => m.label.toLowerCase().includes(query.trim().toLowerCase())),
  );
  const pages = $derived(Math.max(1, Math.ceil(listed.length / PER_PAGE)));
  /** When everything fits on one page, spread the tiles evenly over the rows instead of leaving a short last row. */
  const spread = $derived(pages > 1 ? cols : Math.min(cols, Math.max(1, Math.ceil(listed.length / rows))));
  const visible = $derived(listed.slice(page * PER_PAGE, page * PER_PAGE + PER_PAGE));

  $effect(() => {
    if (page >= pages) page = pages - 1;
  });

  const shortcut = (id: string) => {
    const action = MODULE_ACTIONS[id];
    return action ? (store.settings?.shortcuts[action] ?? '') : '';
  };

  function wheel(e: WheelEvent) {
    if (pages < 2) return;
    page = Math.max(0, Math.min(pages - 1, page + Math.sign(e.deltaY)));
  }
</script>

<div class="grid-mod">
  <header>
    <button class="icon" aria-label="Back" onclick={onBack}><Glyph name="back" /></button>
    {#if searching}
      <!-- svelte-ignore a11y_autofocus -->
      <input class="search" placeholder="Search modules" bind:value={query} autofocus
        onkeydown={(e) => { if (e.key === 'Escape') { e.stopPropagation(); searching = false; query = ''; } }} />
    {:else}
      <span class="title">Modules</span>
    {/if}
    <button class="icon" aria-label="Search" onclick={() => (searching = !searching)}><Glyph name="search" /></button>
    <button class="icon" aria-label="More" onclick={() => (menu = !menu)}><Glyph name="more" /></button>
    {#if menu}
      <div class="menu" role="menu">
        <span class="hint">Choose the module that opens first:</span>
        {#each snap.modules.filter((m) => snap.config.modules.includes(m.id)) as m (m.id)}
          <button role="menuitem" class:sel={snap.config.defaultModule === m.id} onclick={() => { onSetDefault(m.id); menu = false; }}>
            {m.label}
          </button>
        {/each}
      </div>
    {/if}
  </header>

  <div class="pager" onwheel={wheel}>
    <div class="tiles" bind:clientWidth={pagerW} bind:clientHeight={pagerH} style:grid-template-columns={`repeat(${spread}, minmax(0, 1fr))`} style:grid-template-rows={`repeat(${rows}, minmax(0, 1fr))`}>
      {#each visible as m (m.id)}
        <button class="tile" onclick={() => onOpen(m.id)} oncontextmenu={(e) => { e.preventDefault(); onSetDefault(m.id); }}
          title={m.ready ? m.label : `${m.label} (coming in a later version)`}>
          {#if snap.config.defaultModule === m.id}<span class="tick"><Glyph name="check" size={11} /></span>{/if}
          <span class="ti" class:later={!m.ready}><Glyph name={MODULE_ICONS[m.id] ?? 'utilities'} size={20} /></span>
          <span class="name">{m.label}</span>
          <span class="key">{shortcut(m.id)}</span>
        </button>
      {/each}
    </div>
    {#if pages > 1}
      <div class="dots">
        {#each Array(pages) as _, i (i)}
          <button class="dot" class:on={i === page} aria-label={`Page ${i + 1}`} onclick={() => (page = i)}></button>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .grid-mod {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: 8px;
  }
  header {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .title {
    flex: 1;
    font-size: 13px;
    font-weight: 600;
    color: var(--i-mut);
  }
  .search {
    flex: 1;
    min-width: 0;
    padding: 4px 10px;
    border: 1px solid var(--i-line);
    border-radius: 99px;
    background: var(--i-raised);
    color: var(--i-fg);
    font: inherit;
    outline: none;
  }
  .menu {
    position: absolute;
    top: 30px;
    right: 0;
    z-index: 5;
    display: flex;
    flex-direction: column;
    max-height: 170px;
    overflow: auto;
    padding: 4px;
    border-radius: 10px;
    background: var(--i-raised);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.5);
  }
  .hint {
    padding: 4px 8px;
    font-size: 11px;
    color: var(--i-mut);
  }
  .menu button {
    padding: 6px 10px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--i-fg);
    font: inherit;
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
  }
  .menu button:hover,
  .menu .sel {
    background: var(--i-hover);
  }
  .pager {
    display: flex;
    flex: 1;
    min-height: 0;
    gap: 6px;
  }
  .tiles {
    display: grid;
    flex: 1;
    min-height: 0;
    gap: 4px;
  }
  .tile {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    min-width: 0;
    padding: 2px;
    border: 0;
    border-radius: 10px;
    background: none;
    color: var(--i-fg);
    font: inherit;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .tile:hover {
    background: var(--i-hover);
  }
  .ti {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 9px;
    background: var(--i-raised);
  }
  .ti.later {
    opacity: 0.55;
  }
  .name {
    max-width: 100%;
    overflow: hidden;
    font-size: 10.5px;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .key {
    min-height: 11px;
    font-size: 9px;
    color: var(--i-mut);
  }
  .tick {
    position: absolute;
    top: 3px;
    right: 6px;
    display: grid;
    place-items: center;
    width: 15px;
    height: 15px;
    border-radius: 50%;
    background: var(--i-acc);
    color: #000;
  }
  .dots {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 6px;
  }
  .dot {
    width: 6px;
    height: 6px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--i-line);
    cursor: pointer;
  }
  .dot.on {
    background: var(--i-fg);
  }
</style>
