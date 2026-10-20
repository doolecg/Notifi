<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import Glyph from '../Glyph.svelte';
  import { fadeSlide } from '../../lib/motion';

  interface Row {
    key: string;
    name: string;
    icon: string | null;
    level: number;
    muted: boolean;
  }
  interface Level {
    id: string;
    name: string;
    volume: number;
    muted: boolean;
  }
  interface State {
    supported: boolean;
    perApp: boolean;
    maxLevel: number;
    apps: Row[];
    master: Level | null;
  }

  let st = $state<State | null>(null);
  let error = $state('');
  /** Values being dragged, so the refresh doesn't snap sliders back. */
  const held = $state<Record<string, { value: number; until: number }>>({});
  const waiting: Record<string, number> = {};

  async function load() {
    try {
      st = await invoke<State>('mixer_state');
    } catch (e) {
      error = String(e);
    }
  }

  async function run(cmd: string, args: Record<string, unknown>) {
    error = '';
    try {
      await invoke(cmd, args);
    } catch (e) {
      error = String(e);
    }
    load();
  }

  const shown = (id: string, real: number) => (held[id] && held[id].until > Date.now() ? held[id].value : real);

  function drag(id: string, value: number, send: () => void) {
    held[id] = { value, until: Date.now() + 700 };
    clearTimeout(waiting[id]);
    waiting[id] = window.setTimeout(send, 40);
  }

  const setApp = (r: Row, value: number) => drag(r.key, value, () => run('mixer_set_app', { key: r.key, level: value }));
  const setMaster = (value: number) => drag('master', value, () => run('mixer_set_master', { volume: value / 100 }));

  onMount(() => {
    load();
    const timer = setInterval(load, 600);
    return () => clearInterval(timer);
  });
</script>

<div class="mixer">
  <header><h2>Volume mixer</h2></header>

  {#if !st}
    <p class="empty">{error || 'Loading'}</p>
  {:else if !st.supported}
    <p class="empty">The volume mixer isn't available on this system.</p>
  {:else}
    <div class="list">
      {#if st.master}
        {@const m = st.master}
        {@const v = m.muted ? 0 : Math.round(shown('master', m.volume * 100))}
        <div class="row master">
          <button class="mute" class:off={m.muted} aria-label={m.muted ? 'Unmute output' : 'Mute output'} onclick={() => run('mixer_mute_master', { muted: !m.muted })}>
            <Glyph name="speaker" size={16} />
          </button>
          <span class="name" title={m.name}>{m.name}</span>
          <input type="range" min="0" max="100" value={v} aria-label="Output volume" oninput={(e) => setMaster(Number(e.currentTarget.value))} />
          <span class="pct">{v}%</span>
        </div>
      {/if}

      {#if st.perApp}
        {#each st.apps as r (r.key)}
          {@const v = r.muted ? 0 : Math.round(shown(r.key, r.level))}
          <div class="row" in:fadeSlide={{ token: 'listReorder', y: 6 }}>
            <button class="mute" class:off={r.muted} aria-label={r.muted ? `Unmute ${r.name}` : `Mute ${r.name}`} onclick={() => run('mixer_mute_app', { key: r.key, muted: !r.muted })}>
              {#if r.icon}<img src={r.icon} alt="" />{:else}<Glyph name="speaker" size={16} />{/if}
            </button>
            <span class="name" title={r.name}>{r.name}</span>
            <input type="range" min="0" max={st.maxLevel} value={v} aria-label={`${r.name} volume`} oninput={(e) => setApp(r, Number(e.currentTarget.value))} />
            <span class="pct">{v}%</span>
          </div>
        {:else}
          <p class="empty">No app is playing sound.</p>
        {/each}
      {:else}
        <p class="empty">Per-app volume isn't available on this system.</p>
      {/if}
    </div>
    {#if error}<p class="error">{error}</p>{/if}
  {/if}
</div>

<style>
  .mixer {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: 8px;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--i-mut);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .row {
    display: grid;
    grid-template-columns: 30px minmax(0, 110px) 1fr 38px;
    align-items: center;
    gap: 10px;
    padding: 4px 6px;
    border-radius: 8px;
  }
  .master {
    margin-bottom: 4px;
    background: var(--i-raised);
  }
  .name {
    overflow: hidden;
    font-size: 12.5px;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .mute {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--i-fg);
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease), transform var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .mute:hover {
    background: var(--i-hover);
  }
  .mute:active {
    transform: scale(0.92);
  }
  .mute.off {
    color: var(--i-mut);
    opacity: 0.6;
  }
  .mute img {
    width: 18px;
    height: 18px;
    object-fit: contain;
  }
  input[type='range'] {
    width: 100%;
    margin: 0;
    accent-color: var(--i-acc);
  }
  .pct {
    font-size: 11.5px;
    text-align: right;
    color: var(--i-mut);
    font-variant-numeric: tabular-nums;
  }
  .empty,
  .error {
    margin: auto;
    color: var(--i-mut);
    text-align: center;
  }
  .error {
    margin: 0;
    font-size: 11.5px;
  }
</style>
