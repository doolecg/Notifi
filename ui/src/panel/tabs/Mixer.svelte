<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { store } from '../../lib/settings.svelte';
  import Icon from '../Icon.svelte';

  interface Row {
    key: string;
    name: string;
    icon: string | null;
    level: number;
    muted: boolean;
    peak: number;
    deviceId: string | null;
    routeMissing: boolean;
    pinned: boolean;
  }
  interface Level {
    id: string;
    name: string;
    kind: string;
    volume: number;
    muted: boolean;
  }
  interface Output {
    id: string;
    name: string;
    kind: string;
    isDefault: boolean;
  }
  interface State {
    supported: boolean;
    perApp: boolean;
    boost: boolean;
    maxLevel: number;
    apps: Row[];
    outputs: Output[];
    master: Level | null;
    mic: Level | null;
    micMuted: boolean;
  }

  const ICONS = {
    speaker: 'M4 9.5h3.5L12 5.5v13l-4.5-4H4z M15.5 9a4 4 0 0 1 0 6 M18 6.5a7.5 7.5 0 0 1 0 11',
    speakerOff: 'M4 9.5h3.5L12 5.5v13l-4.5-4H4z M16 9.5l5 5 M21 9.5l-5 5',
    mic: 'M12 4a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V7a3 3 0 0 1 3-3Z M6.5 11.5a5.5 5.5 0 0 0 11 0 M12 17v3',
    micOff: 'M12 4a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V7a3 3 0 0 1 3-3Z M6.5 11.5a5.5 5.5 0 0 0 11 0 M12 17v3 M4 4l16 16',
    pin: 'M9 4.5h6l-1 5 3 3.5H7l3-3.5-1-5Z M12 13v6.5',
    up: 'M6.5 14l5.5-5.5 5.5 5.5',
    down: 'M6.5 10l5.5 5.5 5.5-5.5',
    reset: 'M5 12a7 7 0 1 0 2.2-5.1 M5 5v4h4',
  };

  let st = $state<State | null>(null);
  let error = $state('');
  /** Values being dragged or just set, so the 500 ms refresh doesn't snap sliders back. */
  const held = $state<Record<string, { value: number; until: number }>>({});
  let waiting: Record<string, number> = {};

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

  const levelOf = (r: Row) => (r.muted ? 0 : shown(r.key, r.level));
  const boosted = (v: number) => v > 100;

  function setApp(r: Row, value: number) {
    drag(r.key, value, () => run('mixer_set_app', { key: r.key, level: value }));
  }
  function setMaster(value: number) {
    drag('master', value, () => run('mixer_set_master', { volume: value / 100 }));
  }
  function setMic(value: number) {
    drag('mic', value, () => run('mixer_set_mic', { volume: value / 100 }));
  }
  function typed(e: Event, apply: (v: number) => void, max: number) {
    const input = e.currentTarget as HTMLInputElement;
    const v = Math.min(max, Math.max(0, Math.round(Number(input.value) || 0)));
    input.value = String(v);
    apply(v);
  }

  const keys = $derived(st?.apps.map((a) => a.key) ?? []);
  const move = (key: string, delta: number) => run('mixer_move', { keys, key, delta });
  const pinnedCount = $derived(st?.apps.filter((a) => a.pinned).length ?? 0);

  onMount(() => {
    load();
    const timer = setInterval(load, 500);
    return () => clearInterval(timer);
  });
</script>

{#snippet glyph(d: string, size = 16)}
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path {d} /></svg>
{/snippet}

{#if !st}
  {#if error}<p class="error">{error}</p>{/if}
{:else}
  {#if st.master || st.mic}
    <div class="p-card faders">
      {#if st.master}
        {@const m = st.master}
        {@const v = m.muted ? 0 : Math.round(shown('master', m.volume * 100))}
        <div class="p-row fader">
          <button class="icon-btn" class:off={m.muted} title={m.muted ? 'Unmute' : 'Mute'} aria-label={m.muted ? 'Unmute output' : 'Mute output'} onclick={() => run('mixer_mute_master', { muted: !m.muted })}>
            {@render glyph(m.muted ? ICONS.speakerOff : ICONS.speaker)}
          </button>
          <div class="p-main">
            <span class="p-line"><span class="p-label">{m.name}</span></span>
            <input type="range" min="0" max="100" value={v} aria-label="Output volume" oninput={(e) => setMaster(Number(e.currentTarget.value))} style:--fill="{v}%" />
          </div>
          <input class="pct" type="number" min="0" max="100" value={v} aria-label="Output volume percent" onchange={(e) => typed(e, setMaster, 100)} />
        </div>
      {/if}
      {#if st.mic}
        {@const m = st.mic}
        {@const muted = m.muted || st.micMuted}
        {@const v = muted ? 0 : Math.round(shown('mic', m.volume * 100))}
        <div class="p-row fader">
          <button class="icon-btn" class:off={muted} title={muted ? 'Unmute microphone' : 'Mute microphone'} aria-label={muted ? 'Unmute microphone' : 'Mute microphone'} onclick={() => run('mixer_mute_mic', { muted: !m.muted })}>
            {@render glyph(muted ? ICONS.micOff : ICONS.mic)}
          </button>
          <div class="p-main">
            <span class="p-line"><span class="p-label">{m.name}</span></span>
            <input type="range" min="0" max="100" value={v} aria-label="Microphone level" oninput={(e) => setMic(Number(e.currentTarget.value))} style:--fill="{v}%" />
          </div>
          <input class="pct" type="number" min="0" max="100" value={v} aria-label="Microphone level percent" onchange={(e) => typed(e, setMic, 100)} />
        </div>
      {/if}
    </div>
  {/if}

  <p class="p-head">
    <span>Apps</span>
    {#if st.perApp && st.apps.length}<button class="link" onclick={() => run('reset_routes', {})}>Reset outputs</button>{/if}
  </p>
  {#if !st.supported}
    <div class="p-card p-empty">
      <Icon name="mixer" size={26} />
      <span>Volume mixer arrives in a later version</span>
      <span class="p-sub">You'll set each app's volume and speaker here.</span>
    </div>
  {:else if !st.apps.length}
    <div class="p-card p-empty">
      <Icon name="mixer" size={26} />
      <span>Nothing is playing</span>
      <span class="p-sub">Apps that make sound show up here.</span>
    </div>
  {:else}
    <div class="p-card">
      {#each st.apps as r, i (r.key)}
        {@const v = levelOf(r)}
        <div class="app">
          <div class="top">
            <span class="app-icon">
              {#if r.icon}<img src={r.icon} alt="" width="20" height="20" />{:else}{@render glyph(ICONS.speaker, 14)}{/if}
            </span>
            <span class="name" title={r.name}>{r.name}</span>
            {#if st.perApp && st.outputs.length > 1}
              <select class="route" aria-label="Output for {r.name}" value={r.deviceId ?? ''} onchange={(e) => run('mixer_set_route', { key: r.key, deviceId: e.currentTarget.value })}>
                <option value="">Default</option>
                {#each st.outputs as o (o.id)}<option value={o.id}>{o.name}</option>{/each}
              </select>
            {/if}
            <span class="tools">
              <button class="mini" class:on={r.pinned} title={r.pinned ? 'Unpin' : 'Pin to top'} aria-label={r.pinned ? 'Unpin' : 'Pin'} onclick={() => run('mixer_pin', { key: r.key, name: r.name, pinned: !r.pinned })}>{@render glyph(ICONS.pin, 13)}</button>
              <button class="mini" title="Move up" aria-label="Move up" disabled={i === 0 || (i === pinnedCount && !r.pinned)} onclick={() => move(r.key, -1)}>{@render glyph(ICONS.up, 13)}</button>
              <button class="mini" title="Move down" aria-label="Move down" disabled={i === st.apps.length - 1 || (r.pinned && i === pinnedCount - 1)} onclick={() => move(r.key, 1)}>{@render glyph(ICONS.down, 13)}</button>
            </span>
          </div>
          <div class="ctl">
            <button class="icon-btn" class:off={r.muted} title={r.muted ? 'Unmute' : 'Mute'} aria-label={r.muted ? `Unmute ${r.name}` : `Mute ${r.name}`} onclick={() => run('mixer_mute_app', { key: r.key, muted: !r.muted })}>
              {@render glyph(r.muted ? ICONS.speakerOff : ICONS.speaker)}
            </button>
            <input type="range" class="slider" class:warn={boosted(v)} min="0" max={st.maxLevel} step="1" value={v} aria-label="Volume for {r.name}" oninput={(e) => setApp(r, Number(e.currentTarget.value))} style:--fill="{(v / st.maxLevel) * 100}%" style:--mark="{(100 / st.maxLevel) * 100}%" data-boost={st.boost} />
            <input class="pct" class:warn={boosted(v)} type="number" min="0" max={st.maxLevel} value={v} aria-label="Volume percent for {r.name}" onchange={(e) => typed(e, (n) => setApp(r, n), st!.maxLevel)} />
            <button class="icon-btn small" class:hide={v === 100} title="Reset to 100%" aria-label="Reset {r.name} to 100%" disabled={v === 100} onclick={() => run('mixer_reset_app', { key: r.key })}>{@render glyph(ICONS.reset, 14)}</button>
          </div>
          {#if r.routeMissing}<p class="note">Its chosen output is not connected, so it plays on the default.</p>{/if}
          {#if r.peak > 0.001 && !r.muted}<span class="meter" style:width="{Math.min(100, r.peak * 100)}%"></span>{/if}
        </div>
      {/each}
    </div>
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
{/if}

<style>
  .faders .fader + .fader {
    border-top: 1px solid var(--line);
  }
  .icon-btn {
    display: grid;
    place-items: center;
    flex: none;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--fg-soft);
    cursor: pointer;
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--hover);
  }
  .icon-btn.off {
    color: var(--warn);
  }
  .icon-btn.small {
    width: 24px;
    height: 24px;
  }
  .icon-btn:disabled {
    color: var(--mut);
    cursor: default;
  }
  .icon-btn.hide {
    visibility: hidden;
  }
  .icon-btn:focus-visible,
  .mini:focus-visible,
  .route:focus-visible,
  .link:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--acc);
    outline-offset: 1px;
  }
  input[type='range'] {
    width: 100%;
    height: 18px;
    margin: 0;
    appearance: none;
    background: none;
    cursor: pointer;
  }
  input[type='range']::-webkit-slider-runnable-track {
    height: 5px;
    border-radius: 99px;
    background: linear-gradient(to right, var(--acc) var(--fill), var(--track) var(--fill));
  }
  input[type='range'].warn::-webkit-slider-runnable-track {
    background: linear-gradient(to right, var(--acc) var(--mark), var(--warn) var(--mark) var(--fill), var(--track) var(--fill));
  }
  input[type='range']::-webkit-slider-thumb {
    appearance: none;
    width: 14px;
    height: 14px;
    margin-top: -4.5px;
    border: 1px solid var(--line-strong);
    border-radius: 50%;
    background: var(--knob);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.25);
  }
  .pct {
    flex: none;
    width: 44px;
    height: 24px;
    padding: 0 2px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: none;
    color: var(--fg);
    font: inherit;
    font-size: 12.5px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    text-align: right;
    appearance: textfield;
  }
  .pct::-webkit-inner-spin-button {
    appearance: none;
  }
  .pct:hover,
  .pct:focus {
    border-color: var(--line-strong);
    background: var(--inset);
  }
  .pct.warn {
    color: var(--warn);
  }
  .app {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px 8px 12px;
    overflow: hidden;
  }
  .app + .app {
    border-top: 1px solid var(--line);
  }
  .top,
  .ctl {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .app-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    background: var(--inset);
    color: var(--mut);
  }
  .app-icon img {
    width: 18px;
    height: 18px;
    object-fit: contain;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-size: 13px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .route {
    flex: none;
    max-width: 108px;
    height: 24px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: var(--inset);
    color: var(--fg-soft);
    font: inherit;
    font-size: 11.5px;
  }
  .tools {
    display: flex;
    flex: none;
    gap: 1px;
  }
  .mini {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--mut);
    cursor: pointer;
  }
  .mini:hover:not(:disabled) {
    background: var(--hover);
    color: var(--fg);
  }
  .mini.on {
    color: var(--acc);
  }
  .mini:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .note {
    margin: 0 0 0 30px;
    color: var(--warn);
    font-size: 11.5px;
  }
  .meter {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 2px;
    background: var(--acc);
    opacity: 0.55;
    transition: width 120ms linear;
  }
  .link {
    border: 0;
    background: none;
    color: var(--acc);
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
  }
  .error {
    margin: 8px 4px 0;
    color: var(--warn);
    font-size: 12px;
  }
</style>
