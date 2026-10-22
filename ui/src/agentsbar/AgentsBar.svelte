<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, untrack } from 'svelte';
  import { applyCssVars, fadeSlide, setReducedMotion } from '../lib/motion';
  import { store } from '../lib/settings.svelte';
  import AiButtons from '../island/AiButtons.svelte';
  import { fit } from '../island/fit';
  import Glyph from '../island/Glyph.svelte';
  import Logo from '../island/Logo.svelte';
  import PixelBar from '../island/PixelBar.svelte';
  import { island } from '../island/state.svelte';
  import { clock, type SessionChip } from '../island/types';

  const CELLS = Array.from({ length: 10 }, (_, n) => n);
  const STATUS_TEXT: Record<string, string> = { idle: 'Idle', running: 'Running', done: 'Done', needs: 'Needs you' };
  const compact = (n: number) => (n >= 1e9 ? `${(n / 1e9).toFixed(1)}B` : n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}K` : `${n}`);

  /** The window's fixed size (Notifi never resizes it): the widest bar, and room above it for the session buttons. Matches BAR_MAX_W and BAR_ROOM in island/mod.rs. */
  const BAR_MAX = 1000;
  const ROOM = 120;
  const BAR_MIN = 520;

  /** How wide the bar's items need it; it grows at once and shrinks only for a real drop, so a clock ticking over doesn't jiggle it. */
  let barNeed = $state(BAR_MIN);
  function needs(width: number) {
    const target = Math.min(BAR_MAX, Math.max(BAR_MIN, width + 16));
    if (target > barNeed || target < barNeed - 60) barNeed = target;
  }

  const snap = $derived(island.snap);
  const cfg = $derived(snap?.config);
  const live = $derived(snap?.live.find((l) => l.id === 'agents'));
  const workingAgents = $derived((snap?.agents ?? []).filter((a) => a.working));

  /** Sessions as buttons above the bar while the pointer is over it, coloured by what they are doing. */
  const chips = $derived(snap?.sessions ?? []);
  const CHIP_TEXT = { working: 'Working', needs: 'Needs you', done: 'Done' } as const;
  let hover = $state(false);
  let leaveTimer: ReturnType<typeof setTimeout> | undefined;
  let chipsEl = $state<HTMLDivElement>();
  let chipsH = $state(0);
  /** The buttons' width in one row, so the bar and the row above it can line up. */
  let chipsRow = $state(0);
  let winH = $state(32 + ROOM);
  /** The bar's own height: the window's, less the room kept above it. */
  const barH = $derived(Math.max(16, winH - ROOM));
  const showChips = $derived(hover && chips.length > 0);
  /** While the buttons show, the bar and their row share one width. */
  /** Never past the taskbar's Start button. */
  const cap = $derived(Math.min(BAR_MAX, snap?.layout.barMax ?? BAR_MAX));
  const width = $derived(Math.min(cap, showChips ? Math.max(barNeed, chipsRow) : barNeed));
  $effect(() => {
    if (!chipsEl) return;
    const el = chipsEl;
    const measure = () => {
      chipsH = el.offsetHeight;
      const kids = [...el.children] as HTMLElement[];
      chipsRow = Math.ceil(kids.reduce((w, k) => w + k.offsetWidth, 0) + Math.max(0, kids.length - 1) * 6 + 16);
    };
    const watcher = new ResizeObserver(measure);
    watcher.observe(el);
    for (const k of el.children) watcher.observe(k);
    measure();
    return () => watcher.disconnect();
  });
  // Notifi lets clicks through outside these, so the page says what it covers.
  $effect(() => {
    invoke('island_bar_width', { width }).catch(() => {});
  });
  $effect(() => {
    invoke('island_bar_extra', { extra: showChips ? Math.min(ROOM, chipsH + 16) : 0 }).catch(() => {});
  });
  function enter() {
    clearTimeout(leaveTimer);
    hover = true;
  }
  function leave() {
    clearTimeout(leaveTimer);
    leaveTimer = setTimeout(() => (hover = false), 250);
  }
  async function openChip(c: SessionChip) {
    // A T3 Code chat opens in the Island's own chat.
    if (c.thread && cfg?.chatEnabled) {
      invoke('chat_open_thread', { thread: c.thread }).catch(() => {});
      return;
    }
    const found = await invoke<boolean>('island_goto_agent', { agent: c.agent, project: c.project }).catch(() => false);
    if (!found) invoke('island_open_agents').catch(() => {});
  }

  $effect(() => setReducedMotion(store.settings?.general.reducedMotion ?? false));

  /** The dot field changes colour with what matters: nearly out of limit, waiting on you, done, idle, or the agent's own colour. */
  const pixelColor = $derived.by(() => {
    const l = live;
    if (!l) return '#8d979a';
    if ((l.sessionPct ?? 0) >= 90 || (l.weekPct ?? 0) >= 90) return '#ff6b5e';
    if (l.status === 'needs') return '#ffcf4a';
    if (l.status === 'done') return '#35d6a4';
    if (l.status === 'idle') return '#8d979a';
    return l.color;
  });

  let burst = $state(0);
  let firstKey = true;
  $effect(() => {
    void `${live?.status ?? ''}|${live?.count ?? ''}`;
    if (firstKey) {
      firstKey = false;
      return;
    }
    untrack(() => (burst += 1));
  });

  /** Tokens the shown agent has used today. */
  let dayTokens = $state<number | null>(null);
  $effect(() => {
    const kind = live ? ({ claude: 0, codex: 1, opencode: 2 } as Record<string, number>)[live.icon] : undefined;
    if (kind === undefined) {
      dayTokens = null;
      return;
    }
    const read = () =>
      invoke<{ summary: { providers: { tokens: number }[] } }>('island_agent_stats', { days: 1, tzOffsetMin: -new Date().getTimezoneOffset() })
        .then((s) => (dayTokens = s.summary.providers[kind]?.tokens ?? null))
        .catch(() => {});
    read();
    const id = setInterval(read, 20000);
    return () => clearInterval(id);
  });

  /** Done or needs you: jump to that session's window; otherwise open the Island on its AI Agents module. */
  async function click() {
    const l = live;
    if (l && (l.status === 'done' || l.status === 'needs')) {
      if (await invoke<boolean>('island_goto_agent', { agent: l.icon, project: l.project }).catch(() => false)) return;
    }
    invoke('island_open_agents').catch(() => {});
  }

  onMount(() => {
    applyCssVars();
    store.init().catch(() => {});
    island.init(() => {});
    // Notifi saw the pointer leave even if this page missed it.
    const off = listen('agentsbar://leave', () => (hover = false));
    return () => off.then((f) => f());
  });
</script>

{#snippet limit(label: string, pct: number | null, tip: string)}
  <span class="limit" class:off={pct === null} title={`${tip}${pct === null ? ' not reported' : ` ${Math.round(pct)}%`}`}>
    <small>{label}</small>
    <span class="cells" class:warn={(pct ?? 0) >= 70} class:hot={(pct ?? 0) >= 90}>
      {#each CELLS as n (n)}
        <i style:--n={n} class:on={pct !== null && n < Math.max(pct > 0 ? 1 : 0, Math.round(pct / 10))}></i>
      {/each}
    </span>
  </span>
{/snippet}

<svelte:window bind:innerHeight={winH} />

{#if snap?.enabled && cfg}
  <div class="root" role="group" onpointerenter={enter} onpointerleave={leave}>
    <div class="chips" class:show={showChips} bind:this={chipsEl} style:bottom={`${barH + 8}px`} style:width={`${width}px`}>
      {#each chips as c, i (c.id)}
        <button class="chip {c.status}" style:--i={i} title={`${c.title ?? c.project ?? c.agent}: ${CHIP_TEXT[c.status]}`} onclick={() => openChip(c)}>
          <span class="dot"></span><Logo id={c.agent} size={13} />
          <span class="nm">{c.title ?? c.project ?? 'Session'}</span>
          <span class="st">{CHIP_TEXT[c.status]}{#if c.status === 'working' && c.elapsedMs}<span class="el">{clock(c.elapsedMs)}</span>{/if}</span>
        </button>
      {/each}
    </div>
    <div class="pill" role="button" tabindex="0" class:with-chat={cfg.chatEnabled} style:height={`${barH}px`} style:width={`${width}px`} style:--fc={pixelColor} onclick={click} onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && click()} use:fit={needs}>
      {#if live}
        <PixelBar color={pixelColor} calm={live.status === 'done' || live.status === 'idle'} period={live.status === 'needs' ? 900 : live.status === 'running' ? 2000 : 4200} {burst} />
        <span class="end" style:color={live.color}><Logo id={live.icon} size={18} /></span>
        <span class="mid {live.status}"><span class="nat">
          {#key live.status}<span class="st-text" in:fadeSlide={{ token: 'tabSwitch', y: 4 }}>{STATUS_TEXT[live.status] ?? ''}</span>{/key}
          {#if live.value}<small class="time" title="Running for">{live.value}</small>{/if}
          {#if live.todos}<small class="todo">Todo {live.todos[0]}/{live.todos[1]}</small>{/if}
        </span></span>
        {#if snap.aiApp}<AiButtons />{/if}
        <span class="limits">
          {@render limit('5h', live.sessionPct, '5 hour limit used')}
          {@render limit('7d', live.weekPct, 'Weekly limit used')}
          {#if dayTokens !== null}<span class="tok" title="Tokens used today"><small>today</small>{compact(dayTokens)}</span>{/if}
        </span>
        <span class="end value" style:color={live.color}>
          {#each workingAgents as a (a.id)}
            <span class="ag" title={`${a.name}: ${a.count} working`}>{#if a.id !== live.icon}<Logo id={a.id} size={15} />{/if}{#if a.count > 1}<small>{a.count}{a.id === live.icon ? ' running' : ''}</small>{/if}</span>
          {/each}
          {#if live.subagents > 0}<span class="count sub">{live.subagents} {live.subagents === 1 ? 'subagent' : 'subagents'}</span>{/if}
          <span class="count">{live.terminals ? `${live.terminals} ${live.terminals === 1 ? 'session' : 'sessions'}` : 'No sessions'}</span>
        </span>
      {:else}
        <span class="mid"><span class="nat">AI Agents</span></span>
        {#if snap.aiApp}<AiButtons />{/if}
        <span class="count">No sessions</span>
      {/if}
    </div>
    {#if cfg.chatEnabled}
      <button class="chat" style:bottom={`${(barH - 26) / 2}px`} style:left={`${width - 36}px`} title="AI chat" aria-label="AI chat" onclick={() => invoke('island_open_chat').catch(() => {})}><Glyph name="chat" size={16} /></button>
    {/if}
  </div>
{/if}

<style>
  :global(html),
  :global(body.agentsbar-page) {
    margin: 0;
    overflow: hidden;
    background: transparent !important;
  }
  /* The top Island's palette, so the two read as one thing. */
  .root {
    --i-face: #000;
    --i-raised: #1b1e1f;
    --i-hover: rgb(255 255 255 / 0.08);
    --i-fg: #f2f4f3;
    --i-mut: #9aa1a3;
    --i-acc: #2cc9b2;
    --i-agent: #f0a24a;
    position: fixed;
    inset: 0;
    color: var(--i-fg);
    font-family: var(--sans);
    font-size: 13px;
    user-select: none;
  }
  .pill {
    position: absolute;
    left: 0;
    bottom: 0;
    box-shadow:
      inset 0 0 0 1px rgb(255 255 255 / 0.06),
      inset 0 1px 0 rgb(255 255 255 / 0.12);
    transition: width var(--m-pillRise-dur) var(--m-pillRise-ease);
    display: flex;
    align-items: center;
    justify-content: space-between;
    box-sizing: border-box;
    padding: 0 16px;
    border: 0;
    border-radius: 0 999px 999px 0;
    background: var(--i-face);
    color: inherit;
    font: inherit;
    overflow: hidden;
    cursor: pointer;
  }
  .chips {
    position: absolute;
    left: 0;
    box-sizing: border-box;
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 6px;
    padding: 0 8px;
    pointer-events: none;
    transition: width var(--m-pillRise-dur) var(--m-pillRise-ease);
  }
  .chips.show {
    pointer-events: auto;
  }
  .chip {
    --c: #6ea8ff;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 30px;
    padding: 0 12px 0 10px;
    border: 0;
    border-radius: 99px;
    background: color-mix(in srgb, var(--c) 16%, var(--i-face));
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--c) 40%, transparent),
      inset 0 1px 0 rgb(255 255 255 / 0.1);
    color: var(--i-fg);
    font: inherit;
    font-size: 12.5px;
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
    /* Leaving: all at once and quick. */
    opacity: 0;
    transform: translateY(10px) scale(0.96);
    transition:
      opacity var(--m-hoverLift-dur) var(--m-hoverLift-ease),
      transform var(--m-hoverLift-dur) var(--m-hoverLift-ease),
      background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  /* Arriving: one after another, rising on the Island's spring. */
  .chips.show .chip {
    opacity: 1;
    transform: none;
    transition:
      opacity var(--m-glassIn-dur) var(--m-glassIn-ease) calc(var(--i) * 35ms),
      transform var(--m-pillDrop-dur) var(--m-pillDrop-ease) calc(var(--i) * 35ms),
      background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .chip.needs {
    --c: #f0a24a;
  }
  .chip.done {
    --c: #35d6a4;
  }
  .chip:hover {
    background: color-mix(in srgb, var(--c) 34%, var(--i-face));
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c);
  }
  .chip.working .dot,
  .chip.needs .dot {
    animation: pulse 1.6s ease-in-out infinite;
  }
  .nm {
    max-width: 170px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .el {
    margin-left: 6px;
  }
  .st {
    color: var(--c);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
  :global([data-reduced-motion]) .dot {
    animation: none;
  }
  .pill.with-chat {
    padding-right: 44px;
  }
  .chat {
    position: absolute;
    z-index: 5;
    transition:
      left var(--m-pillRise-dur) var(--m-pillRise-ease),
      background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease),
      color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--i-mut);
    cursor: pointer;
  }
  .chat:hover {
    background: var(--i-hover);
    color: var(--i-fg);
  }
  .pill::before {
    content: '';
    position: absolute;
    inset: 0;
    z-index: 4;
    border-radius: inherit;
    box-shadow:
      inset 0 0 0 1.5px var(--fc, var(--i-acc)),
      inset 0 0 16px 1px color-mix(in srgb, var(--fc, var(--i-acc)), transparent 55%);
    opacity: 0;
    pointer-events: none;
  }
  .pill > :global(*:not(.px)) {
    position: relative;
  }
  .end {
    display: flex;
    align-items: center;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .end.value {
    gap: 6px;
    white-space: nowrap;
  }
  .mid {
    flex: 1;
    margin: 0 12px 0 10px;
    text-align: left;
    white-space: nowrap;
    font-size: 13px;
    font-weight: 700;
    color: var(--i-mut);
  }
  .mid.running {
    color: var(--i-fg);
  }
  .mid.done {
    color: var(--i-acc);
  }
  .mid.needs {
    color: var(--i-agent);
  }
  .time,
  .todo {
    margin-left: 8px;
    font-size: 11px;
    font-weight: 500;
    color: var(--i-mut);
    font-variant-numeric: tabular-nums;
  }
  .limits {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-right: 14px;
  }
  .limit {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .limit small,
  .tok small {
    color: color-mix(in srgb, var(--i-fg), transparent 25%);
    font-size: 11px;
    font-weight: 700;
  }
  .limit.off {
    opacity: 0.55;
  }
  .cells {
    --c: var(--i-acc);
    display: flex;
    gap: 2px;
    padding: 3px 5px;
    border-radius: 99px;
    background: color-mix(in srgb, var(--i-fg), transparent 91%);
  }
  .cells.warn {
    --c: var(--i-agent);
  }
  .cells.hot {
    --c: #ff6b5e;
  }
  .cells i {
    width: 4px;
    height: 4px;
    border-radius: 1px;
    background: color-mix(in srgb, var(--i-fg), transparent 80%);
    transition: background-color var(--m-glassIn-dur) var(--m-glassIn-ease) calc(var(--n) * 30ms);
  }
  .cells i.on {
    background: var(--c);
  }
  .tok {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .ag {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    margin-right: 4px;
    color: var(--i-fg);
  }
  .ag small {
    font-size: 11px;
    font-weight: 700;
    color: var(--i-mut);
  }
  .count {
    font-size: 13px;
    font-weight: 700;
  }
  .count.sub {
    margin-right: 10px;
    color: var(--i-mut);
  }
</style>
