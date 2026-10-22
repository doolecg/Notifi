<script lang="ts">
  // Custom options of the Island settings page (spec/everyday/island.md, "Settings page").
  // Switches on `option.id`: layout, modules, activity, behaviour, shortcuts, preview, pickArea.
  import { invoke } from '@tauri-apps/api/core';
  import KeyCapture from '../lib/KeyCapture.svelte';
  import Switch from '../lib/Switch.svelte';
  import { store } from '../lib/settings.svelte';
  import Glyph from '../island/Glyph.svelte';
  import WatchArea from './WatchArea.svelte';
  import { maxBottom, maxSide, openSize, slots, windowSize, MAX_H, MAX_W, MIN_H, MIN_W, type Side } from '../island/layout';
  import { BUTTONS, DEFAULT_CONFIG, MODULE_ACTIONS, MODULE_ICONS, MODULE_LABELS, type IslandConfig } from '../island/types';
  import type { CustomProps } from '../sections/customs';

  let { option, values, set }: CustomProps = $props();

  const cfg = $derived({ ...DEFAULT_CONFIG, ...(values as Partial<IslandConfig>) } as IslandConfig);

  // ---- AI questions ----
  type HooksStatus = { installed: boolean; port: number; settingsPath: string; otherHooksKept: boolean };
  let hooksStatus = $state<HooksStatus | null>(null);
  let hooksBusy = $state(false);
  let hooksError = $state('');
  const readHooks = () => invoke<HooksStatus>('ask_hooks_status').then((s) => (hooksStatus = s)).catch((e) => (hooksError = String(e)));
  async function hooks(command: string) {
    hooksBusy = true;
    hooksError = '';
    try {
      hooksStatus = await invoke<HooksStatus>(command);
    } catch (e) {
      hooksError = String(e);
    } finally {
      hooksBusy = false;
    }
  }
  $effect(() => {
    if (option.id === 'askHooks') readHooks();
  });
  const hooksText = $derived(
    !hooksStatus ? 'Checking…' : hooksStatus.installed ? `Added to ${hooksStatus.settingsPath}.` : 'Not added yet. Questions will stay in the terminal until you add them.',
  );

  // ---- Claude usage ----
  type ClaudeUsage = { status: string; session: { usedPercent: number } | null; week: { usedPercent: number } | null; checked: number };
  let claude = $state<ClaudeUsage | null>(null);
  const readClaude = () => invoke<ClaudeUsage>('island_claude_usage').then((u) => (claude = u)).catch(() => {});
  $effect(() => {
    if (option.id !== 'claudeUsage' || !cfg.claudeUsageConnect) return;
    readClaude();
    const id = setInterval(readClaude, 5000);
    return () => clearInterval(id);
  });
  const claudeText = $derived.by(() => {
    const c = claude;
    if (!c || c.status === 'off' || c.status === 'waiting') return 'Checking…';
    if (c.status === 'noLogin') return 'No Claude Code login found. Sign in to Claude Code once, then check again.';
    if (c.status === 'expired') return 'The saved login has expired. Open Claude Code once so it renews, then check again.';
    if (c.status === 'failed') return 'Could not read your limits just now. Notifi tries again in a few minutes.';
    const p = (l: { usedPercent: number } | null) => (l ? `${Math.round(l.usedPercent)}%` : 'not reported');
    return `Connected. 5 hour ${p(c.session)}, weekly ${p(c.week)}.`;
  });

  // ---- Layout ----
  const PREVIEW_W = 520;
  const KEYS: Record<Side, 'orbitLeft' | 'orbitRight' | 'orbitBottom'> = { left: 'orbitLeft', right: 'orbitRight', bottom: 'orbitBottom' };
  let resizing = $state<[number, number] | null>(null);
  const open = $derived(resizing ?? openSize(cfg.size, cfg.customWidth, cfg.customHeight));
  const win = $derived(windowSize(open));
  const scale = $derived(Math.min(1, PREVIEW_W / win[0]));
  const cap = $derived({ left: maxSide(open[1]), right: maxSide(open[1]), bottom: maxBottom(open[0]) });

  /** Filled slots plus one "+" slot per side while there's room. */
  const slotList = $derived.by(() => {
    const out: { side: Side; index: number; id: string | null; x: number; y: number }[] = [];
    for (const side of ['left', 'right', 'bottom'] as Side[]) {
      const ids = cfg[KEYS[side]].slice(0, cap[side]);
      const count = Math.min(ids.length + 1, cap[side]);
      slots(side, count, open).forEach(([x, y], i) => out.push({ side, index: i, id: ids[i] ?? null, x, y }));
    }
    return out;
  });

  let editing = $state<{ side: Side; index: number } | null>(null);
  let dragFrom = $state<{ side: Side; index: number } | null>(null);

  function putButton(side: Side, index: number, id: string | null) {
    const list = [...cfg[KEYS[side]]];
    if (id === null) list.splice(index, 1);
    else if (index >= list.length) list.push(id);
    else list[index] = id;
    set(KEYS[side], list);
    editing = null;
  }

  function drop(side: Side, index: number) {
    const from = dragFrom;
    dragFrom = null;
    if (!from || (from.side === side && from.index === index)) return;
    const lists = { left: [...cfg.orbitLeft], right: [...cfg.orbitRight], bottom: [...cfg.orbitBottom] };
    const [id] = lists[from.side].splice(from.index, 1);
    if (!id) return;
    const target = lists[side];
    target.splice(Math.min(index, target.length), 0, id);
    if (from.side !== side) set(KEYS[from.side], lists[from.side]);
    set(KEYS[side], target);
  }

  function startResize(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const start = { x: e.clientX, y: e.clientY, w: open[0], h: open[1] };
    const move = (ev: PointerEvent) => {
      const w = Math.round(Math.min(MAX_W, Math.max(MIN_W, start.w + (2 * (ev.clientX - start.x)) / scale)));
      const h = Math.round(Math.min(MAX_H, Math.max(MIN_H, start.h + (ev.clientY - start.y) / scale)));
      resizing = [w, h];
    };
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      if (resizing) {
        set('customWidth', resizing[0]);
        set('customHeight', resizing[1]);
      }
      resizing = null;
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }

  // ---- Content ----
  const ordered = $derived([...cfg.modules, ...Object.keys(MODULE_LABELS).filter((id) => !cfg.modules.includes(id))]);
  function toggleModule(id: string, on: boolean) {
    set('modules', on ? ordered.filter((m) => m === id || cfg.modules.includes(m)) : cfg.modules.filter((m) => m !== id));
  }
  function moveModule(id: string, by: number) {
    const list = [...cfg.modules];
    const i = list.indexOf(id);
    const j = i + by;
    if (i < 0 || j < 0 || j >= list.length) return;
    [list[i], list[j]] = [list[j], list[i]];
    set('modules', list);
  }

  // ---- Shortcuts ----
  const islandActions = $derived((store.catalog?.actions ?? []).filter((a) => a.id.startsWith('island')));
  function setShortcut(id: string, accel: string) {
    store.set('shortcuts', { ...store.settings?.shortcuts, [id]: accel }).catch(() => {});
  }
</script>

{#if option.id === 'layout'}
  <div class="layout">
    <div class="preview" style:width={`${win[0] * scale}px`} style:height={`${win[1] * scale}px`}>
      <div class="island" class:translucent={cfg.translucent}
        style:width={`${open[0] * scale}px`} style:height={`${open[1] * scale}px`}
        style:border-radius={`0 0 ${24 * scale}px ${24 * scale}px`}>
        <span class="size">{open[0]} x {open[1]}</span>
        {#if cfg.size === 'custom'}
          <span class="handle" role="slider" aria-label="Resize" aria-valuenow={open[0]} tabindex="0" onpointerdown={startResize}></span>
        {/if}
      </div>
      {#each slotList as s (`${s.side}${s.index}`)}
        {@const b = s.id ? (BUTTONS[s.id] ?? { label: s.id, icon: MODULE_ICONS[s.id] ?? 'utilities' }) : null}
        <button
          class="slot"
          class:empty={!b}
          style:left={`${(win[0] / 2 + s.x - 20) * scale}px`}
          style:top={`${(s.y - 20) * scale}px`}
          style:width={`${40 * scale}px`}
          style:height={`${40 * scale}px`}
          draggable={!!b}
          title={b ? b.label : 'Add a button'}
          ondragstart={() => (dragFrom = { side: s.side, index: s.index })}
          ondragover={(e) => e.preventDefault()}
          ondrop={(e) => { e.preventDefault(); drop(s.side, s.index); }}
          onclick={() => (editing = { side: s.side, index: s.index })}
        >
          {#if b}<Glyph name={b.icon} size={Math.round(18 * scale)} />{:else}<Glyph name="plus" size={Math.round(16 * scale)} />{/if}
        </button>
      {/each}
    </div>

    {#if editing}
      {@const current = cfg[KEYS[editing.side]][editing.index] ?? null}
      <div class="editor">
        <span>{current ? 'This button opens' : 'Add a button that opens'}</span>
        <select value={current ?? ''} onchange={(e) => putButton(editing!.side, editing!.index, (e.target as HTMLSelectElement).value)}>
          {#if !current}<option value="" disabled>Choose...</option>{/if}
          {#each Object.entries(BUTTONS) as [id, b] (id)}<option value={id}>{b.label}</option>{/each}
        </select>
        {#if current}<button class="ghost" onclick={() => putButton(editing!.side, editing!.index, null)}>Remove</button>{/if}
        <button class="ghost" onclick={() => (editing = null)}>Done</button>
      </div>
    {/if}

    <p class="hint">Click + to add a button. Drag buttons around the Island. Click a button to edit it.</p>

    <div class="sizes" role="radiogroup" aria-label="Size">
      {#each [['compact', 'Compact'], ['spacious', 'Spacious'], ['custom', 'Custom']] as [id, label] (id)}
        <button class="size-card" role="radio" aria-checked={cfg.size === id} onclick={() => set('size', id)}>
          <span class="mini" class:big={id === 'spacious'} class:custom={id === 'custom'}></span>{label}
        </button>
      {/each}
    </div>

    <div class="row">
      <div><strong>Translucent background</strong><span class="sub">Blurs what's behind the open Island where the system allows it.</span></div>
      <Switch checked={cfg.translucent} label="Translucent background" onchange={(on) => set('translucent', on)} />
    </div>
  </div>
{:else if option.id === 'modules'}
  <div class="list">
    {#each ordered as id (id)}
      {@const on = cfg.modules.includes(id)}
      <div class="mrow">
        <Glyph name={MODULE_ICONS[id] ?? 'utilities'} />
        <span class="mname">{MODULE_LABELS[id] ?? id}</span>
        {#if cfg.defaultModule === id}<span class="badge">Default</span>{:else if on}<button class="ghost" onclick={() => set('defaultModule', id)}>Make default</button>{/if}
        <button class="ghost" aria-label="Move up" disabled={!on} onclick={() => moveModule(id, -1)}>Up</button>
        <button class="ghost" aria-label="Move down" disabled={!on} onclick={() => moveModule(id, 1)}>Down</button>
        <Switch checked={on} label={`Show ${MODULE_LABELS[id]}`} onchange={(v) => toggleModule(id, v)} />
      </div>
    {/each}
  </div>
  <h4>Now Playing</h4>
  <div class="row"><strong>Lyrics</strong><Switch checked={cfg.showLyrics} label="Lyrics" onchange={(on) => set('showLyrics', on)} /></div>
  <div class="row"><strong>Equaliser</strong><Switch checked={cfg.equaliser} label="Equaliser" onchange={(on) => set('equaliser', on)} /></div>
  <label class="row"><strong>Player to prefer</strong>
    <input type="text" placeholder="Any player" value={cfg.preferredPlayer} onchange={(e) => set('preferredPlayer', (e.target as HTMLInputElement).value.trim())} />
  </label>
{:else if option.id === 'activity'}
  <div class="row"><strong>Track changes</strong><Switch checked={cfg.activityTrack} label="Track changes" onchange={(on) => set('activityTrack', on)} /></div>
  <div class="row"><strong>Timer done</strong><Switch checked={cfg.activityTimer} label="Timer done" onchange={(on) => set('activityTimer', on)} /></div>
  <div class="row"><strong>AI agent task finished</strong><Switch checked={cfg.activityAgent} label="AI agent task finished" onchange={(on) => set('activityAgent', on)} /></div>
  <label class="row"><strong>Only for agent tasks longer than</strong>
    <span><input type="number" min="0" max="120" value={cfg.agentFinishMinutes} onchange={(e) => set('agentFinishMinutes', Number((e.target as HTMLInputElement).value))} /> min</span>
  </label>
  <div class="row"><strong>Hide the timer countdown on the resting pill</strong><Switch checked={cfg.hideTimerCountdown} label="Hide timer countdown" onchange={(on) => set('hideTimerCountdown', on)} /></div>
  <label class="row"><strong>Show events for</strong>
    <span><input type="range" min="1000" max="15000" step="500" value={cfg.activityMs} onchange={(e) => set('activityMs', Number((e.target as HTMLInputElement).value))} /> {(cfg.activityMs / 1000).toFixed(1)} s</span>
  </label>
{:else if option.id === 'behaviour'}
  <label class="row"><strong>Open</strong>
    <select value={cfg.openOn} onchange={(e) => set('openOn', (e.target as HTMLSelectElement).value)}>
      <option value="click">On click only</option>
      <option value="hover">On hover</option>
    </select>
  </label>
  <label class="row"><strong>Position</strong>
    <select value={cfg.dock} onchange={(e) => set('dock', (e.target as HTMLSelectElement).value)}>
      <option value="top">Top centre</option>
      <option value="bottomLeft">Bottom left, on the taskbar</option>
    </select>
  </label>
  <label class="row"><strong>Hover delay</strong>
    <span><input type="range" min="0" max="1500" step="50" value={cfg.hoverDelayMs} disabled={cfg.openOn !== 'hover'} onchange={(e) => set('hoverDelayMs', Number((e.target as HTMLInputElement).value))} /> {cfg.hoverDelayMs} ms</span>
  </label>
  <label class="row"><strong>Collapse delay</strong>
    <span><input type="range" min="0" max="3000" step="100" value={cfg.collapseDelayMs} onchange={(e) => set('collapseDelayMs', Number((e.target as HTMLInputElement).value))} /> {cfg.collapseDelayMs} ms</span>
  </label>
  <label class="row"><strong>Display</strong>
    <select value={cfg.display} onchange={(e) => set('display', (e.target as HTMLSelectElement).value)}>
      <option value="primary">Main display</option>
      <option value="pointer">Display with the pointer</option>
    </select>
  </label>
  <div class="row"><strong>Hide when an app is full screen</strong><Switch checked={cfg.hideInFullscreen} label="Hide in full screen" onchange={(on) => set('hideInFullscreen', on)} /></div>
{:else if option.id === 'shortcuts'}
  {#each islandActions as a (a.id)}
    <div class="row">
      <strong>{a.label.replace(/^Island: /, '')}</strong>
      <KeyCapture value={store.settings?.shortcuts[a.id] ?? ''} label={a.label} onchange={(accel) => setShortcut(a.id, accel)} />
    </div>
  {/each}
  {#if !islandActions.length}<p class="hint">Island shortcuts appear here once Notifi has loaded them.</p>{/if}
  <p class="hint">Modules without a shortcut here open from the module grid. {Object.keys(MODULE_ACTIONS).length} modules have their own.</p>
{:else if option.id === 'askHooks'}
  <div class="row">
    <div>
      <strong>Claude Code hooks</strong>
      <span class="sub">{hooksText}</span>
    </div>
    <span class="acts">
      <button class="ghost" disabled={hooksBusy} onclick={() => hooks(hooksStatus?.installed ? 'ask_hooks_remove' : 'ask_hooks_install')}>{hooksStatus?.installed ? 'Remove hooks' : 'Add hooks'}</button>
    </span>
  </div>
  {#if hooksError}<p class="hint">{hooksError}</p>{/if}
  <p class="hint">Works with Claude Code only: it is the one agent that lets another app answer its questions. Codex and OpenCode questions stay in their terminals.</p>
{:else if option.id === 'claudeUsage'}
  <div class="row">
    <div><strong>Connect Claude usage</strong><span class="sub">Uses the login Claude Code saved on this PC to ask Anthropic for your 5 hour and weekly limits. Nothing is copied or stored, and your login is never refreshed by Notifi.</span></div>
    <Switch checked={cfg.claudeUsageConnect} label="Connect Claude usage" onchange={(on) => { set('claudeUsageConnect', on); if (on) setTimeout(readClaude, 2500); }} />
  </div>
  {#if cfg.claudeUsageConnect}
    <div class="row">
      <span class="sub">{claudeText}</span>
      <button class="ghost" onclick={() => { invoke('island_claude_usage_refresh').catch(() => {}); setTimeout(readClaude, 2500); }}>Check now</button>
    </div>
  {/if}
{:else if option.id === 'preview'}
  <button class="ghost" disabled={!cfg.enabled} onclick={() => invoke('island_preview').catch(() => {})}>Open the Island</button>
{:else if option.id === 'pickArea'}
  <WatchArea {option} />
{/if}

<style>
  .layout {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .preview {
    position: relative;
    align-self: center;
    margin: 6px 0;
    border-radius: 10px;
    background: repeating-linear-gradient(45deg, var(--inset), var(--inset) 8px, var(--bg) 8px, var(--bg) 16px);
  }
  .island {
    position: absolute;
    top: 0;
    left: 50%;
    display: grid;
    place-items: center;
    transform: translateX(-50%);
    background: #000;
    color: #9aa1a3;
  }
  .island.translucent {
    background: rgb(0 0 0 / 0.72);
  }
  .size {
    font-size: 11px;
  }
  .handle {
    position: absolute;
    right: 4px;
    bottom: 4px;
    width: 12px;
    height: 12px;
    border-right: 2px solid #9aa1a3;
    border-bottom: 2px solid #9aa1a3;
    cursor: nwse-resize;
    touch-action: none;
  }
  .slot {
    position: absolute;
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: #000;
    color: #f2f4f3;
    cursor: grab;
  }
  .slot.empty {
    border: 1.5px dashed var(--line-strong);
    background: transparent;
    color: var(--mut);
    cursor: pointer;
  }
  .editor {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--mut);
  }
  .sizes {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .size-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 10px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    cursor: pointer;
  }
  .size-card[aria-checked='true'] {
    border-color: var(--acc);
    background: var(--acc-soft);
  }
  .mini {
    width: 44px;
    height: 22px;
    border-radius: 0 0 8px 8px;
    background: #000;
  }
  .mini.big {
    width: 56px;
    height: 30px;
  }
  .mini.custom {
    border: 1.5px dashed var(--mut);
    background: transparent;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 0;
  }
  .row strong {
    font-weight: 500;
  }
  .row .sub {
    display: block;
    font-size: 12px;
    color: var(--mut);
  }
  .list {
    display: flex;
    flex-direction: column;
  }
  .mrow {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 0;
    border-bottom: 1px solid var(--line);
  }
  .mname {
    flex: 1;
  }
  .badge {
    padding: 1px 8px;
    border-radius: 99px;
    background: var(--acc-soft);
    color: var(--acc);
    font-size: 11.5px;
  }
  h4 {
    margin: 14px 0 4px;
    font-size: 13px;
  }
  .ghost {
    padding: 4px 10px;
    border: 1px solid var(--line-strong);
    border-radius: 7px;
    background: none;
    color: var(--fg);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .ghost:disabled {
    opacity: 0.45;
    cursor: default;
  }
  select,
  input[type='text'],
  input[type='number'] {
    padding: 4px 8px;
    border: 1px solid var(--line-strong);
    border-radius: 7px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
  }
  input[type='number'] {
    width: 64px;
  }
</style>
