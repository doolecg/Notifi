<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, untrack } from 'svelte';
  import { fade } from 'svelte/transition';
  import { applyCssVars, duration, setReducedMotion } from '../lib/motion';
  import { store } from '../lib/settings.svelte';
  import Glyph from './Glyph.svelte';
  import Logo, { LOGOS } from './Logo.svelte';
  import { fit } from './fit';
  import PixelBar from './PixelBar.svelte';
  import Agents from './modules/Agents.svelte';
  import Controls from './modules/Controls.svelte';
  import Grid from './modules/Grid.svelte';
  import NowPlaying from './modules/NowPlaying.svelte';
  import Ask from './modules/Ask.svelte';
  import Calendar from './modules/Calendar.svelte';
  import Camera from './modules/Camera.svelte';
  import Chat from './modules/Chat.svelte';
  import Captures from './modules/Captures.svelte';
  import Downloads from './modules/Downloads.svelte';
  import Notifications from './modules/Notifications.svelte';
  import Clipboard from './modules/Clipboard.svelte';
  import Files from './modules/Files.svelte';
  import Mixer from './modules/Mixer.svelte';
  import Scratchpad from './modules/Scratchpad.svelte';
  import System from './modules/System.svelte';
  import Timer from './modules/Timer.svelte';
  import Tools from './modules/Tools.svelte';
  import { island, media, runOutside } from './state.svelte';
  import { BUTTONS, MODULE_ICONS } from './types';

  /** Resting pill sizes in logical px (Notifi design). */
  const REST = { w: 190, h: 32 };
  const LIVE = { w: 310, h: 34 };
  const ACTIVITY = { w: 390, h: 40 };
  /** The idle info pill is sized from what it shows (rough text widths), between these. */
  const IDLE_MIN = 190;
  const IDLE_MAX = 420;
  const IDLE_CH = 6.5;
  const IDLE_ICON = 17;
  const IDLE_GAP = 8;
  const IDLE_PAD = 28;
  const IDLE_MID = 24;
  /** The thin bar shown when idle, and the taller area around it that catches the pointer. */
  const THIN = { w: 120, h: 4 };
  const THIN_HIT = { w: 160, h: 12 };
  /** The agents pill carries status, to-do count and three rings, so it is wider. */
  const AGENTS_W = 600;
  /** The widest the agents pill grows to when its items need more room. */
  const AGENTS_MAX = 840;
  const CELLS = Array.from({ length: 10 }, (_, n) => n);
  const compact = (n: number) => (n >= 1e9 ? `${(n / 1e9).toFixed(1)}B` : n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}K` : `${n}`);
  /** Gap under a docked island; matches DOCK_GAP in island/layout.rs. */
  const DOCK_GAP = 0;
  /** Space between the taskbar and the open island; matches FLY_GAP in island/layout.rs. */
  const FLY_GAP = 12;
  /** How long the pill stays dropped after the last news. */
  const QUIET_MS = 6000;

  const STATUS_TEXT: Record<string, string> = { idle: 'Idle', running: 'Running', done: 'Done', needs: 'Needs you' };

  const OUTSIDE =new Set(['settings', 'mixer', 'keepAwake']);

  const snap = $derived(island.snap);
  const cfg = $derived(snap?.config);
  const layout = $derived(snap?.layout);
  /** With the AI Agents bar on the taskbar, the top pill leaves the agents out. */
  const live = $derived((snap?.live ?? []).filter((l) => !(l.id === 'agents' && cfg?.agentsTaskbar && !layout?.dock)));

  /** The window is at its open size. */
  let open = $state(false);
  /** The island is drawn open (follows `open` by a frame, leads it when collapsing). */
  let expanded = $state(false);
  let collapsing = $state(false);
  let hovering = $state(false);
  /** The docked menu is stretched up to its tallest size. */
  let tall = $state(false);
  /** Height of the open menu before it is stretched; taken when it opens. */
  let baseH = $state(236);
  /** Open on the AI chat: the menu has the chat's size. */
  let chatMode = $state(false);
  let module = $state('nowPlaying');
  let lastModule = $state<string | null>(null);
  let chosenLive = $state<string | null>(null);

  let tallTimer: ReturnType<typeof setTimeout> | undefined;
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  let collapseTimer: ReturnType<typeof setTimeout> | undefined;
  let shrinkTimer: ReturnType<typeof setTimeout> | undefined;

  const workingCount = $derived((snap?.agents ?? []).filter((a) => a.working).length);
  const selLive = $derived(live.find((l) => l.id === chosenLive) ?? live[0]);

  type IdleItem = { key: string; label?: string; icon?: string; text: string };
  const minute = $derived(Math.floor(island.now / 60_000));
  const idleLeft = $derived.by(() => {
    if (!cfg) return [];
    const d = new Date(minute * 60_000);
    const out: IdleItem[] = [];
    if (cfg.idleClock) out.push({ key: 'clock', text: d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) });
    if (cfg.idleDate) out.push({ key: 'date', text: d.toLocaleDateString('en-GB', { weekday: 'short', day: 'numeric', month: 'short' }) });
    return out;
  });
  const idleRight = $derived.by(() => {
    const i = snap?.idle;
    if (!cfg || !i) return [];
    const out: IdleItem[] = [];
    const pct = (key: string, label: string, v: number | null) => {
      if (v !== null) out.push({ key, label, text: `${v}%` });
    };
    if (cfg.idleAiUsage) pct('ai', 'AI', i.ai);
    if (cfg.idleCpuRam) {
      pct('cpu', 'CPU', i.cpu);
      pct('mem', 'RAM', i.mem);
    }
    if (cfg.idleBattery) {
      if (i.battery !== null) out.push({ key: 'battery', icon: i.charging ? 'bolt' : 'battery', text: `${i.battery}%` });
      else if (!cfg.idleCpuRam) pct('cpu', 'CPU', i.cpu);
    }
    return out;
  });
  const sideW = (items: IdleItem[]) =>
    items.reduce((w, it) => w + (it.text.length + (it.label ? it.label.length + 1 : 0)) * IDLE_CH + (it.icon ? IDLE_ICON : 0), 0) +
    Math.max(0, items.length - 1) * IDLE_GAP;
  const idleShown = $derived(idleLeft.length + idleRight.length > 0);
  /** The clock/CPU page is one more page to scroll to when something is live. */
  const watchChosen = $derived(chosenLive === 'watch' && !!snap?.watch);
  const infoChosen = $derived(!watchChosen && chosenLive === 'info' && idleShown && live.length > 0);
  const pages = $derived([
    ...live.map((l) => ({ id: l.id, label: l.label, icon: l.icon })),
    ...(idleShown && live.length ? [{ id: 'info', label: 'Info', icon: 'calendar' }] : []),
    ...(snap?.watch ? [{ id: 'watch', label: 'Watch', icon: 'search' }] : []),
  ]);
  const selPage = $derived(watchChosen ? 'watch' : infoChosen ? 'info' : (selLive?.id ?? ''));
  const idleW = $derived(
    idleShown
      ? Math.round(Math.min(IDLE_MAX, Math.max(IDLE_MIN,
          IDLE_PAD + sideW(idleLeft) + sideW(idleRight) + (idleLeft.length && idleRight.length ? IDLE_MID : 0))))
      : REST.w,
  );

  /** The pointer is over the window, which grows the thin bar into the info pill. */
  let near = $state(false);
  /** Dropped down for news; goes back up after a quiet spell unless the pointer is on it. */
  let awake = $state(false);
  let quietTimer: ReturnType<typeof setTimeout> | undefined;
  function wake() {
    awake = true;
    clearTimeout(quietTimer);
    quietTimer = setTimeout(() => (awake = false), QUIET_MS);
  }
  const news = $derived(live.map((l) => l.id + (l.detail ?? '')).join() + '|' + (snap?.media?.title ?? ''));
  $effect(() => {
    news;
    wake();
  });
  $effect(() => {
    if (island.activity) wake();
  });
  $effect(() => {
    if (near) {
      clearTimeout(quietTimer);
      awake = true;
    } else wake();
  });
  /** The thin bar belongs to the top dock; on the taskbar the pill stays visible. */
  const thinOn = $derived(!!cfg?.idleThinBar && !layout?.dock);
  const showLive = $derived(!!live.length && (awake || !thinOn));
  const thin = $derived(thinOn && !island.activity && !showLive && !near && !open);
  /** Height of a normal window title bar, so the pill never hangs below it. */
  let bar = $state(REST.h);
  /** Docked, the pill is inset from the taskbar's edges. */
  const pillBar = $derived(layout?.dock ? Math.max(16, bar - 2 * DOCK_GAP) : bar);
  /** The live pill is as wide as what it shows. */
  /** How wide the agents pill's items need it to be; grows at once, shrinks only for a real drop, so a clock ticking over doesn't jiggle it. */
  let agentsNeed = $state(0);
  const needs = (w: number) => {
    if (w > agentsNeed || w < agentsNeed - 40) agentsNeed = w;
  };
  const liveW = $derived.by(() => {
    if (selLive?.id === 'agents') return Math.min(AGENTS_MAX, Math.max(AGENTS_W, agentsNeed + 12));
    if (selLive?.id === 'music' && snap?.media) {
      const text = (snap.media.title || '').length + (snap.media.artist || '').length;
      return Math.round(Math.min(480, Math.max(LIVE.w, 190 + text * 7.2)));
    }
    return LIVE.w;
  });
  const rest = $derived(
    island.asks.length && !open ? { w: Math.max(ACTIVITY.w + 80, idleW), h: pillBar }
    : island.activity ? { w: Math.max(ACTIVITY.w, idleW), h: pillBar }
    : watchChosen ? { w: Math.max(ACTIVITY.w, idleW), h: pillBar }
    : infoChosen ? { w: idleW, h: pillBar }
    : showLive ? { w: Math.max(liveW, idleW), h: pillBar }
    : thin ? THIN
    : { w: idleW, h: pillBar },
  );
  const box = $derived(
    thin
      ? THIN_HIT
      : { w: rest.w, h: rest.h },
  );

  /** What the window was last told, so a shrinking pill can keep its room until it has finished shrinking. */
  let sentShape = { w: 0, h: 0 };
  let shapeTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const { w, h } = box;
    clearTimeout(shapeTimer);
    if (open) {
      sentShape = { w: 0, h: 0 };
      return;
    }
    const send = () => {
      sentShape = { w, h };
      invoke('island_shape', { open: false, width: w, height: h }).catch(() => {});
    };
    if (w < sentShape.w - 0.5 || h < sentShape.h - 0.5) shapeTimer = setTimeout(send, duration('pillDrop') + 80);
    else send();
  });
  /** The old text leaves quickly, so it never sits on top of the new text. */
  const swapOut = () => ({ duration: Math.min(70, duration('hoverLift')) });

  /** Height of the title bar, or of the taskbar when docked; refetched when the dock changes. */
  $effect(() => {
    void layout?.dock;
    invoke<number>('island_bar_height').then((h) => (bar = h)).catch(() => {});
  });

  /** The taskbar's colour, sampled again now and then so wallpaper and theme changes follow. */
  let taskbar = $state({ bg: '#1c1c1c', light: false });
  $effect(() => {
    if (!layout?.dock) return;
    const read = () => invoke<{ bg: string; light: boolean }>('island_taskbar_style').then((t) => (taskbar = t)).catch(() => {});
    read();
    const id = setInterval(read, 15000);
    return () => clearInterval(id);
  });

  $effect(() => setReducedMotion(store.settings?.general.reducedMotion ?? false));

  function clearTimers() {
    clearTimeout(hoverTimer);
    clearTimeout(collapseTimer);
  }

  /** What the open Island shows first: the thing that is live, or the grid when nothing is going on. */
  function autoModule() {
    if (island.asks.length) return 'ask';
    const lead = live[0]?.id;
    return lead === 'agents' ? 'agents' : lead === 'timer' ? 'timer' : lead === 'music' ? 'nowPlaying' : dock ? 'nook' : 'grid';
  }

  async function openIsland(id?: string) {
    clearTimers();
    if (id) module = id;
    else if (!open || collapsing) module = autoModule();
    lastModule = module;
    if (layout && !tall && !chatMode && !open) baseH = layout.base[1];
    // The chat gets its own size; questions get the tall menu so options and a reply box fit.
    if (module === 'chat' && !chatMode) {
      await invoke('island_chat', { on: true }).catch(() => {});
      chatMode = true;
    } else if (module === 'ask' && !tall) {
      await invoke('island_tall', { tall: true }).catch(() => {});
      tall = true;
    }
    if (open && !collapsing) return;
    clearTimeout(shrinkTimer);
    collapsing = false;
    if (layout && !tall && !chatMode) baseH = layout.base[1];
    if (!open) {
      open = true;
      await invoke('island_shape', { open: true, width: box.w, height: box.h }).catch(() => {});
    }
    requestAnimationFrame(() => (expanded = true));
  }

  function collapse() {
    clearTimers();
    if (!open || collapsing) return;
    collapsing = true;
    expanded = false;
    // The chat is a separate page over the menu, so it goes first.
    if (module === 'chat') invoke('chat_hide').catch(() => {});
    shrinkTimer = setTimeout(() => {
      collapsing = false;
      open = false;
      tall = false;
      chatMode = false;
    }, duration('pillCollapse'));
  }

  /** The window grows first and shrinks last, so the menu never outruns its window. */
  async function toggleTall() {
    clearTimeout(tallTimer);
    if (!tall) {
      await invoke('island_tall', { tall: true }).catch(() => {});
      tall = true;
    } else {
      tall = false;
      tallTimer = setTimeout(() => invoke('island_tall', { tall: false }).catch(() => {}), duration('pillDrop') + 80);
    }
  }

  /** The chat has its own, larger size; the window grows first and shrinks last, like the stretched menu. */
  async function setChat(on: boolean) {
    clearTimeout(tallTimer);
    if (on) {
      await invoke('island_chat', { on: true }).catch(() => {});
      chatMode = true;
    } else {
      chatMode = false;
      tallTimer = setTimeout(() => invoke('island_chat', { on: false }).catch(() => {}), duration('pillDrop') + 80);
    }
  }

  function setModule(id: string) {
    module = id;
    lastModule = id;
    if (id === 'chat' && !chatMode) setChat(true);
    else if (id !== 'chat' && chatMode) setChat(false);
  }

  function press(id: string) {
    if (OUTSIDE.has(id)) {
      runOutside(id);
      collapse();
    } else if (id === 'dictate') {
      invoke('chat_dictate').catch(() => {});
    } else if (id === 'volume') {
      setModule('controls');
    } else {
      setModule(id);
    }
  }

  /** The Nook's widgets sit side by side and run past the edge; scrolling over it slides them, except inside a list that scrolls itself. */
  function nookWheel(e: WheelEvent) {
    const el = e.currentTarget as HTMLElement;
    if (el.scrollWidth <= el.clientWidth || (e.target as Element).closest('.list, .scroll, textarea, input')) return;
    el.scrollLeft += e.deltaY || e.deltaX;
  }

  /** Scrolling over the tab strip steps through the tabs. */
  function stripWheel(e: WheelEvent) {
    const d = e.deltaY || e.deltaX;
    const now = performance.now();
    if (Math.abs(d) < 1 || now - lastWheel < 200 || !strip.tabs.length) return;
    lastWheel = now;
    const at = Math.max(0, strip.tabs.findIndex(activeButton));
    press(strip.tabs[(at + (d > 0 ? 1 : strip.tabs.length - 1)) % strip.tabs.length]);
  }

  function setDefault(id: string) {
    invoke('set_section', { section: 'island', value: { defaultModule: id } }).catch(() => {});
  }

  let hoverOff: ReturnType<typeof setTimeout> | undefined;

  function enter() {
    clearTimeout(hoverOff);
    hovering = true;
    clearTimeout(collapseTimer);
    if (!open && cfg?.openOn === 'hover') {
      clearTimeout(hoverTimer);
      hoverTimer = setTimeout(() => openIsland(), cfg.hoverDelayMs);
    }
  }

  function leave() {
    // A short grace so a brief pointer slip off the pill doesn't count as leaving.
    clearTimeout(hoverOff);
    hoverOff = setTimeout(() => (hovering = false), 200);
    clearTimeout(hoverTimer);
    // The chat is its own page over the menu, so the pointer is always "off" the menu while using it.
    if (open && !layout?.dock && module !== 'chat') collapseTimer = setTimeout(collapse, cfg?.collapseDelayMs ?? 600);
  }

  let lastWheel = 0;

  /** Scrolling on the resting pill steps through its pages (agents, timer, music, info). */
  function wheel(e: WheelEvent) {
    if (open || pages.length < 2 || Math.abs(e.deltaY) < 1) return;
    const now = performance.now();
    if (now - lastWheel < 260) return;
    lastWheel = now;
    const at = Math.max(0, pages.findIndex((p) => p.id === selPage));
    chosenLive = pages[(at + (e.deltaY > 0 ? 1 : pages.length - 1)) % pages.length].id;
  }

  /** Done or needs you: the click jumps to that session's window instead of opening the menu. */
  async function gotoSession() {
    const l = selLive;
    if (!l || l.id !== 'agents' || (l.status !== 'done' && l.status !== 'needs') || infoChosen) return false;
    return invoke<boolean>('island_goto_agent', { agent: l.icon, project: l.project }).catch(() => false);
  }

  async function clickPill() {
    if (open && !collapsing) {
      if (dock) collapse();
      return;
    }
    if (island.asks.length) {
      invoke('island_focus').catch(() => {});
      openIsland('ask');
      return;
    }
    if (await gotoSession()) return;
    invoke('island_focus').catch(() => {});
    openIsland();
  }

  function command(cmd: string) {
    if (cmd === 'toggle') {
      if (open && !collapsing) collapse();
      else openIsland();
    } else if (cmd === 'open') {
      openIsland();
    } else if (cmd.startsWith('module:')) {
      openIsland(cmd.slice(7));
    }
  }

  onMount(() => {
    applyCssVars();
    store.init();
    island.init(command);
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') collapse();
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  });

  /** Docked, the bar stays in the taskbar and the open menu is a separate flyout above it. */
  const dock = $derived(!!layout?.dock);
  /** The buttons are a tab strip along the top of the open menu: modules as tabs, the rest as icons at the right. */
  const strip = $derived.by(() => {
    const ids = [...new Set(['nook', ...(cfg?.chatEnabled ? ['chat'] : []), ...(cfg?.orbitLeft ?? []), ...(cfg?.orbitRight ?? []), ...(cfg?.orbitBottom ?? [])])];
    return { tabs: ids.filter((id) => !OUTSIDE.has(id)), actions: [...(cfg?.chatEnabled && module === 'chat' ? ['dictate'] : []), ...ids.filter((id) => OUTSIDE.has(id))] };
  });
  const buttonInfo = (id: string) =>
    id === 'nook' ? { label: 'Nook', icon: 'home' } : (BUTTONS[id] ?? { label: snap?.modules.find((m) => m.id === id)?.label ?? id, icon: MODULE_ICONS[id] ?? 'utilities' });
  const activeButton = (id: string) => module === id || (id === 'volume' && module === 'controls');
  /** The open menu's width: the chat's own, else the normal one (the stretched menu keeps it). */
  const openW = $derived(chatMode ? (layout?.chat[0] ?? 0) : (layout?.base[0] ?? 0));
  /** The tab names would not fit across the menu; the estimate is a name's letters at about 7 px plus the icon and padding. */
  const stripTight = $derived(
    strip.tabs.reduce((w, id) => w + 46 + buttonInfo(id).label.length * 7.3, 0) + strip.actions.length * 36 + 40 > openW - 36,
  );
  /** Height of the tab strip. */
  const STRIP = 46;
  /** The menu's height: stretched, or as opened. */
  const flyH = $derived(chatMode ? (layout?.chat[1] ?? baseH) : tall ? (layout?.tall[1] ?? baseH) : baseH);
  const grown = $derived(expanded && !dock);
  const pillW = $derived(grown && layout ? openW : rest.w);
  const pillH = $derived(grown && layout ? flyH : rest.h);
  const radius = $derived(grown && layout ? layout.radius : rest.h / 2);
  /** What the pill is telling you, so a change in it can flash the edge. */
  const changeKey = $derived(
    `${selPage}|${selLive?.status ?? ''}|${selLive?.count ?? ''}|${snap?.media?.title ?? ''}|${island.activity?.text ?? ''}|${island.asks.length}`,
  );
  let flashing = $state(false);
  let burst = $state(0);
  let flashTimer: ReturnType<typeof setTimeout> | undefined;
  let firstKey = true;
  $effect(() => {
    void changeKey;
    if (firstKey) {
      firstKey = false;
      return;
    }
    // Written without reading, so this effect only depends on the change key.
    untrack(() => {
      burst += 1;
      clearTimeout(flashTimer);
      flashing = false;
      requestAnimationFrame(() => {
        flashing = true;
        flashTimer = setTimeout(() => (flashing = false), 1300);
      });
    });
  });
  /** The dot field changes colour with what matters: nearly out of limit, waiting on you, done, idle, or the agent's own colour. */
  const pixelColor = $derived.by(() => {
    const l = selLive;
    if (!l) return '#f0a24a';
    if ((l.sessionPct ?? 0) >= 90 || (l.weekPct ?? 0) >= 90) return '#ff6b5e';
    if (l.status === 'needs') return '#ffcf4a';
    if (l.status === 'done') return '#35d6a4';
    if (l.status === 'idle') return '#8d979a';
    return l.color;
  });
  /** Tokens the shown agent has used today, asked for every so often while the agents pill is up. */
  let dayTokens = $state<number | null>(null);
  $effect(() => {
    const kind = selLive?.id === 'agents' ? ({ claude: 0, codex: 1, opencode: 2 } as Record<string, number>)[selLive.icon] : undefined;
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
  /** Agents that have a session working right now, shown as logos on the agents pill. */
  const workingAgents = $derived((snap?.agents ?? []).filter((a) => a.working));
  /** How far through the song, for the fill behind the resting media pill. */
  const songPct = $derived.by(() => {
    const d = snap?.media?.durationMs;
    return d ? Math.min(100, Math.max(0, (island.position / d) * 100)) : 0;
  });
  const moduleLabel = $derived(snap?.modules.find((m) => m.id === module)?.label ?? module);
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

{#snippet body()}
  {#if module === 'nook'}
    <div class="nook" onwheel={nookWheel}>
      <section class="wide"><NowPlaying onSetDefault={setDefault} /></section>
      <section><Calendar /></section>
      <section><Notifications /></section>
      <section><Scratchpad /></section>
    </div>
  {:else if module === 'nowPlaying'}
    <NowPlaying onSetDefault={setDefault} />
  {:else if module === 'grid'}
    <Grid onOpen={setModule} onBack={collapse} onSetDefault={setDefault} />
  {:else if module === 'controls'}
    <Controls onButton={press} />
  {:else if module === 'timer'}
    <Timer />
  {:else if module === 'agents'}
    <Agents />
  {:else if module === 'chat'}
    <Chat onAsk={() => setModule('ask')} />
  {:else if module === 'ask'}
    <Ask onDone={collapse} onChat={cfg?.chatEnabled ? () => setModule('chat') : undefined} />
  {:else if module === 'mixer'}
    <Mixer />
  {:else if module === 'system'}
    <System />
  {:else if module === 'tools'}
    <Tools />
  {:else if module === 'calendar'}
    <Calendar />
  {:else if module === 'clipboard'}
    <Clipboard />
  {:else if module === 'scratchpad'}
    <Scratchpad />
  {:else if module === 'shelf'}
    <Files />
  {:else if module === 'captures'}
    <Captures />
  {:else if module === 'downloads'}
    <Downloads />
  {:else if module === 'camera'}
    <Camera />
  {:else if module === 'notifications'}
    <Notifications />
  {:else}
    <div class="later">
      <Glyph name={MODULE_ICONS[module] ?? 'utilities'} size={30} />
      <strong>{moduleLabel}</strong>
      <span>Coming in a later version of Notifi.</span>
    </div>
  {/if}
{/snippet}

{#snippet tabstrip()}
  <nav class="strip" class:tight={stripTight} style:height={`${STRIP}px`} onwheel={stripWheel}>
    {#each strip.tabs as id (id)}
      {@const b = buttonInfo(id)}
      <button class="tab" class:active={activeButton(id)} title={b.label} onclick={() => press(id)}>
        <Glyph name={b.icon} size={16} /><span>{b.label}</span>
      </button>
    {/each}
    <span class="spacer"></span>
    {#each strip.actions as id (id)}
      {@const b = buttonInfo(id)}
      <button class="act" title={b.label} aria-label={b.label} onclick={() => press(id)}><Glyph name={b.icon} size={16} /></button>
    {/each}
    {#if !chatMode}
      <button class="act" aria-label={tall ? 'Make the menu shorter' : 'Make the menu taller'} title={tall ? 'Shorter' : 'Taller'} onclick={toggleTall}>
        <Glyph name={tall ? 'shrink' : 'expand'} size={16} />
      </button>
    {/if}
  </nav>
{/snippet}

{#if snap?.enabled && cfg && layout}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="root" class:translucent={cfg.translucent} class:dock={layout.dock}
    class:light={layout.dock && taskbar.light}
    style:--tb-bg={layout.dock ? taskbar.bg : undefined}
    style:--dock-dark={`${cfg.dockDarkness}%`} onpointerenter={() => (near = true)} onpointerleave={() => (near = false)}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div
      class="pill"
      class:expanded={grown}
      class:collapsing
      class:awake={awake && !thin}
      class:flash={flashing}
      style:--fc={pixelColor}
      style:width={`${pillW}px`}
      style:height={`${pillH}px`}
      style:bottom={layout.dock ? `${DOCK_GAP}px` : undefined}
      style:border-radius={layout.dock ? `0 ${radius}px ${radius}px 0` : `0 0 ${radius}px ${radius}px`}
      onpointerenter={enter}
      onpointerleave={leave}
      onclick={clickPill}
      onwheel={wheel}
    >
      {#if open && !dock}
        <div class="tabs" class:shown={expanded}>{@render tabstrip()}</div>
        <div class="content" class:shown={expanded} style:top={`${STRIP}px`} style:width={`${openW}px`} style:height={`${flyH - STRIP}px`}>
          {@render body()}
        </div>
      {:else if island.asks.length}
        {@const a = island.asks[0]}
        <div class="ends activity asking" out:fade={swapOut()}>
          <Glyph name="bell" size={16} />
          <span class="atext">{a.kind === 'question' ? (a.questions[0]?.question ?? 'A question') : `Allow ${a.tool}?`}</span>
          {#if island.asks.length > 1}<small class="more">+{island.asks.length - 1}</small>{/if}
        </div>
      {:else if island.activity}
        <div class="ends activity" out:fade={swapOut()}>
          <span style:color={island.activity.color}><Glyph name={island.activity.icon} size={16} /></span>
          <span class="atext">{island.activity.text}</span>
        </div>
      {:else if watchChosen && snap.watch}
        <div class="ends activity live-in" out:fade={swapOut()}>
          <Glyph name="search" size={16} />
          <span class="atext" title={snap.watch.paused || snap.watch.text}>{snap.watch.paused ? `Watching paused: ${snap.watch.paused}` : snap.watch.text || 'Watching…'}</span>
        </div>
      {:else if showLive && selLive && !infoChosen}
        {#key selLive.id}
          <div class="ends live-in" out:fade={swapOut()} use:fit={selLive.id === 'agents' ? needs : null}>
            {#if selLive.id === 'agents'}<PixelBar color={pixelColor} calm={selLive.status === 'done' || selLive.status === 'idle'} period={selLive.status === 'needs' ? 900 : selLive.status === 'running' ? 2000 : 4200} {burst} />{/if}
            {#if selLive.id === 'music' && snap.media}
              <span class="prog" class:still={!snap.media.playing} style:--c={selLive.color} style:width={`${songPct}%`}></span>
            {:else if selLive.id === 'order'}
              <span class="prog" style:--c={selLive.color} style:width={`${(selLive.progress ?? 0) * 100}%`}></span>
            {/if}
            <span class="end" style:color={selLive.color}>
              {#if selLive.id === 'music' && island.cover}
                <img class="tiny" src={island.cover} alt="" />
              {:else if selLive.id === 'music' && snap.media?.app}
                <span class="tiny mono" title={snap.media.app}>{snap.media.app.trim().charAt(0).toUpperCase()}</span>
              {:else if LOGOS.has(selLive.icon)}
                <Logo id={selLive.icon} size={18} />
              {:else}
                <Glyph name={selLive.icon} size={16} />
              {/if}
            </span>
            {#if selLive.id === 'music' && snap.media}
              <span class="mid track">
                <b>{snap.media.title || 'Unknown title'}</b>{#if snap.media.artist}<em>{snap.media.artist}</em>{/if}
              </span>
            {:else if selLive.id === 'order'}
              <span class="mid track"><b>{selLive.label}</b><em>{selLive.detail}</em></span>
            {:else if selLive.id === 'timer'}
              <span class="mid">Timer{snap.timer.phase === 'paused' ? ' paused' : ''}</span>
            {:else}
              <span class="mid {selLive.status}"><span class="nat">
                {STATUS_TEXT[selLive.status] ?? ''}
                {#if selLive.id === 'agents' && selLive.value}<small class="todo" title="Running for">{selLive.value}</small>{/if}
                {#if selLive.todos}<small class="todo">Todo {selLive.todos[0]}/{selLive.todos[1]}</small>{/if}
              </span></span>
            {/if}
            {#if selLive.id === 'agents'}
              <span class="limits">
                {@render limit('5h', selLive.sessionPct, '5 hour limit used')}
                {@render limit('7d', selLive.weekPct, 'Weekly limit used')}
                {#if dayTokens !== null}<span class="tok" title="Tokens used today"><small>today</small>{compact(dayTokens)}</span>{/if}
              </span>
            {/if}
            <span class="end value" style:color={selLive.color}>
              {#if selLive.id === 'agents'}
                {#each workingAgents as a (a.id)}
                  <span class="ag" title={`${a.name}: ${a.count} working`}>{#if a.id !== selLive.icon}<Logo id={a.id} size={15} />{/if}{#if a.count > 1}<small>{a.count}{a.id === selLive.icon ? ' running' : ''}</small>{/if}</span>
                {/each}
                {#if selLive.subagents > 0}<span class="count sub">{selLive.subagents} {selLive.subagents === 1 ? 'subagent' : 'subagents'}</span>{/if}
                <span class="count">{selLive.terminals ? `${selLive.terminals} ${selLive.terminals === 1 ? 'session' : 'sessions'}` : 'No sessions'}</span>
              {/if}
              {#if selLive.id === 'agents'}
                <!-- the running time sits beside the status -->
              {:else if selLive.value}
                {selLive.value}
              {:else if selLive.id === 'music' && snap.media}
                <span class="transport" role="group" aria-label="Playback">
                  <button class="mini" aria-label="Previous" disabled={!snap.media.canPrev} onclick={(e) => { e.stopPropagation(); media('previous'); }}><Glyph name="prev" size={14} filled /></button>
                  <button class="mini play" aria-label={snap.media.playing ? 'Pause' : 'Play'} disabled={!snap.media.canPlayPause} onclick={(e) => { e.stopPropagation(); media('toggle'); }}><Glyph name={snap.media.playing ? 'pause' : 'play'} size={14} filled={!snap.media.playing} /></button>
                  <button class="mini" aria-label="Next" disabled={!snap.media.canNext} onclick={(e) => { e.stopPropagation(); media('next'); }}><Glyph name="next" size={14} filled /></button>
                </span>
              {/if}
            </span>
          </div>
        {/key}
      {:else if !thin && idleShown}
        <div class="ends idle live-in" out:fade={swapOut()} class:one={!idleLeft.length || !idleRight.length}>
          {#each [idleLeft, idleRight] as side, s (s)}
            {#if side.length}
              <span class="iside">
                {#each side as it (it.key)}
                  <span class="iitem">
                    {#if it.icon}<Glyph name={it.icon} size={14} />{/if}{#if it.label}<small>{it.label}</small>{/if}{it.text}
                  </span>
                {/each}
              </span>
            {/if}
          {/each}
        </div>
      {/if}
    </div>

    {#if open && dock}
      <div
        class="fly"
        class:shown={expanded}
        style:bottom={`${bar + FLY_GAP}px`}
        style:width={`${openW}px`}
        style:height={`${flyH}px`}
        style:border-radius={`${layout.radius}px`}
        onpointerenter={enter}
        onpointerleave={leave}
      >
        {@render tabstrip()}
        <div class="content shown" style:top={`${STRIP}px`} style:width={`${openW}px`} style:height={`${flyH - STRIP}px`}>
          {@render body()}
        </div>
      </div>
    {/if}

  </div>
{/if}

<style>
  :global(html),
  :global(body.island-page) {
    margin: 0;
    overflow: hidden;
    background: transparent !important;
  }
  .root {
    --i-bg: #000;
    --i-raised: #1b1e1f;
    --i-hover: rgb(255 255 255 / 0.08);
    --i-line: #34393b;
    --i-fg: #f2f4f3;
    --i-mut: #9aa1a3;
    --i-acc: #2cc9b2;
    --i-agent: #f0a24a;
    --i-timer: #8fb3ff;
    position: fixed;
    inset: 0;
    color: var(--i-fg);
    font-family: var(--sans);
    font-size: 13px;
    user-select: none;
  }
  .pill {
    position: absolute;
    top: 0;
    left: 50%;
    overflow: hidden;
    transform: translateX(-50%);
    background: var(--i-bg);
    box-shadow:
      0 6px 18px rgb(0 0 0 / 0.35),
      inset 0 0 0 1px rgb(255 255 255 / 0.06),
      inset 0 -1px 0 rgb(255 255 255 / 0.16);
    cursor: default;
    /* The window is only as big as the resting pill, so the resting pill must not overshoot it; the bouncy drop is for the open Island. */
    transition:
      width var(--m-pillRise-dur) var(--m-pillRise-ease),
      height var(--m-pillRise-dur) var(--m-pillRise-ease),
      border-radius var(--m-pillRise-dur) var(--m-pillRise-ease);
  }
  .pill.expanded {
    transition:
      width var(--m-pillDrop-dur) var(--m-pillDrop-ease),
      height var(--m-pillDrop-dur) var(--m-pillDrop-ease),
      border-radius var(--m-pillDrop-dur) var(--m-pillDrop-ease);
  }
  .dock {
    --i-bg: var(--tb-bg);
    --i-face: color-mix(in srgb, var(--tb-bg), #000 var(--dock-dark, 25%));
    --i-fg: #f3f3f3;
    --i-mut: #a8a8a8;
  }
  .dock.light {
    --i-fg: #1a1a1a;
    --i-mut: #5d5d5d;
    --i-hover: rgb(0 0 0 / 0.07);
  }
  .dock .pill {
    top: auto;
    left: 0;
    transform: none;
    background: var(--i-face);
    box-shadow: none;
    transition:
      width var(--m-pillRise-dur) var(--m-pillRise-ease),
      height var(--m-pillRise-dur) var(--m-pillRise-ease),
      bottom var(--m-pillRise-dur) var(--m-pillRise-ease),
      border-radius var(--m-pillRise-dur) var(--m-pillRise-ease);
  }
  /* The Start-menu style flyout: rises out of the taskbar. */
  .fly {
    position: absolute;
    left: 0;
    overflow: hidden;
    background: var(--tb-bg);
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--i-fg), transparent 88%),
      0 12px 36px rgb(0 0 0 / 0.4);
    opacity: 0;
    filter: blur(6px);
    transform: translateY(28px) scale(0.96);
    transform-origin: bottom left;
    transition:
      opacity var(--m-pillRise-dur) var(--m-pillRise-ease),
      filter var(--m-pillRise-dur) var(--m-pillRise-ease),
      transform var(--m-pillRise-dur) var(--m-pillRise-ease);
  }
  .fly.shown {
    opacity: 1;
    filter: blur(0);
    transform: none;
    transition:
      opacity var(--m-glassIn-dur) var(--m-glassIn-ease),
      filter var(--m-glassIn-dur) var(--m-glassIn-ease),
      transform var(--m-pillDrop-dur) var(--m-pillDrop-ease),
      height var(--m-pillDrop-dur) var(--m-pillDrop-ease);
  }
  .tabs {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    z-index: 3;
    opacity: 0;
    filter: blur(10px);
    transition:
      opacity 80ms linear,
      filter 80ms linear;
  }
  .tabs.shown {
    opacity: 1;
    filter: blur(0);
    transition:
      opacity var(--m-glassIn-dur) var(--m-glassIn-ease) 80ms,
      filter var(--m-glassIn-dur) var(--m-glassIn-ease) 80ms;
  }
  .strip {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    z-index: 3;
    display: flex;
    align-items: center;
    gap: 6px;
    box-sizing: border-box;
    padding: 0 18px;
  }
  /* When the names would not fit, only the open tab keeps its name, so the strip never crowds or clips. */
  .strip.tight .tab:not(.active) span {
    display: none;
  }
  .strip.tight .tab:not(.active) {
    padding: 0 11px;
  }
  .spacer {
    flex: 1;
  }
  .tab,
  .act {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    height: 30px;
    border: 0;
    border-radius: 99px;
    background: none;
    color: var(--i-mut);
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition:
      background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease),
      color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .tab {
    padding: 0 15px 0 13px;
  }
  .act {
    width: 30px;
    padding: 0;
  }
  .tab:hover,
  .act:hover {
    background: var(--i-hover);
    color: var(--i-fg);
  }
  .tab.active {
    background: color-mix(in srgb, var(--i-fg), transparent 88%);
    color: var(--i-fg);
  }
  .nook {
    display: flex;
    height: 100%;
    min-height: 0;
    overflow-x: auto;
    scrollbar-width: none;
    scroll-snap-type: x proximity;
  }
  .nook section {
    flex: 0 0 250px;
    scroll-snap-align: start;
    min-width: 0;
    padding: 0 16px;
    border-left: 1px solid color-mix(in srgb, var(--i-fg), transparent 90%);
  }
  .nook section:first-child {
    padding-left: 0;
    border-left: 0;
  }
  .nook section:last-child {
    padding-right: 0;
  }
  .nook section {
    overflow: hidden;
  }
  .nook .wide {
    flex-basis: 360px;
  }

  .dock .pill.collapsing {
    transition:
      width var(--m-pillRise-dur) var(--m-pillRise-ease),
      height var(--m-pillRise-dur) var(--m-pillRise-ease),
      bottom var(--m-pillRise-dur) var(--m-pillRise-ease),
      border-radius var(--m-pillRise-dur) var(--m-pillRise-ease);
  }
  .pill.collapsing {
    transition:
      width var(--m-pillRise-dur) var(--m-pillRise-ease),
      height var(--m-pillRise-dur) var(--m-pillRise-ease),
      border-radius var(--m-pillRise-dur) var(--m-pillRise-ease);
  }
  /* A glint that sweeps across the glass as it drops. */
  .pill::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: linear-gradient(110deg, transparent 35%, rgb(255 255 255 / 0.16) 50%, transparent 65%) no-repeat;
    background-size: 300% 100%;
    background-position: 130% 0;
    opacity: 0;
    pointer-events: none;
  }
  .pill.awake::after,
  .pill.expanded::after {
    animation: glint calc(var(--m-pillDrop-dur) * 1.4) var(--m-glassIn-ease);
  }
  @keyframes glint {
    0% {
      opacity: 1;
      background-position: 130% 0;
    }
    100% {
      opacity: 0;
      background-position: -30% 0;
    }
  }
  .translucent .pill.expanded {
    background: rgb(0 0 0 / 0.78);
    backdrop-filter: blur(24px);
  }
  .content {
    position: absolute;
    top: 0;
    left: 0;
    box-sizing: border-box;
    padding: 14px 18px;
    opacity: 0;
    filter: blur(10px);
    transform: scale(0.96);
    transform-origin: top center;
    transition:
      opacity 80ms linear,
      filter 80ms linear,
      transform 80ms linear;
  }
  .content.shown {
    opacity: 1;
    filter: blur(0);
    transform: none;
    transition:
      opacity var(--m-glassIn-dur) var(--m-glassIn-ease) 80ms,
      filter var(--m-glassIn-dur) var(--m-glassIn-ease) 80ms,
      transform var(--m-glassIn-dur) var(--m-glassIn-ease) 80ms;
  }
  .ends {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 100%;
    padding: 0 16px;
    box-sizing: border-box;
  }
  .ends > :global(*:not(.px):not(.prog)) {
    position: relative;
  }
  .end {
    display: flex;
    align-items: center;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .tiny {
    width: 20px;
    height: 20px;
    border-radius: 5px;
    object-fit: cover;
  }
  .mono {
    display: grid;
    place-items: center;
    background: #eef0f0;
    color: #111;
    font-size: 11px;
    font-weight: 800;
  }
  .live-in {
    animation: live-in var(--m-glassIn-dur) var(--m-glassIn-ease) 80ms backwards;
  }
  /* No blur or scale: text that is resampled while it animates looks heavier, then snaps crisp. */
  @keyframes live-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
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
  .todo {
    margin-left: 8px;
    font-size: 11px;
    font-weight: 500;
    color: var(--i-mut);
    font-variant-numeric: tabular-nums;
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
  .pill.flash::before {
    animation: edge-flash 1250ms var(--m-glassIn-ease) both;
  }
  @keyframes edge-flash {
    0% {
      opacity: 0;
    }
    14% {
      opacity: 1;
    }
    100% {
      opacity: 0;
    }
  }
  :global([data-reduced-motion]) .pill.flash,
  :global([data-reduced-motion]) .prog,
  :global([data-reduced-motion]) .prog::after {
    animation: none;
  }
  .prog {
    animation: breathe 3.4s ease-in-out infinite;
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    background: linear-gradient(90deg, color-mix(in srgb, var(--c) 6%, transparent), color-mix(in srgb, var(--c) 46%, transparent));
    transition: width 600ms linear;
    pointer-events: none;
  }
  .prog::after {
    content: '';
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: 30px;
    background: linear-gradient(90deg, transparent, color-mix(in srgb, var(--c), white 35%));
    filter: blur(5px);
    opacity: 0.75;
    animation: edge-glow 1.8s ease-in-out infinite;
  }
  .prog.still,
  .prog.still::after {
    animation-play-state: paused;
  }
  @keyframes breathe {
    0%,
    100% {
      opacity: 0.78;
    }
    50% {
      opacity: 1;
    }
  }
  @keyframes edge-glow {
    0%,
    100% {
      opacity: 0.35;
    }
    50% {
      opacity: 0.95;
    }
  }
  .transport {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .mini {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--i-fg);
    cursor: pointer;
  }
  .mini:hover:not(:disabled) {
    background: var(--i-hover);
  }
  .mini.play {
    background: var(--i-fg);
    color: var(--i-bg);
  }
  .mini.play:hover:not(:disabled) {
    background: color-mix(in srgb, var(--i-fg), transparent 15%);
  }
  .mid.track {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mid.track b {
    color: var(--i-fg);
    font-weight: 700;
  }
  .mid.track em {
    margin-left: 8px;
    font-style: normal;
    font-weight: 600;
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
  .root :global(.eq.level) {
    gap: 2px;
    height: 13px;
  }
  .root :global(.eq.level i) {
    width: 3px;
  }
  .root :global(.eq.level i:nth-child(1)) {
    height: 40%;
  }
  .root :global(.eq.level i:nth-child(2)) {
    height: 60%;
  }
  .root :global(.eq.level i:nth-child(3)) {
    height: 80%;
  }
  .root :global(.eq.level i:nth-child(4)) {
    height: 100%;
    animation-delay: -0.45s;
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
  .asking {
    color: var(--i-fg);
    font-weight: 700;
  }
  .asking :global(svg) {
    color: var(--i-agent);
    animation: ask-ring 1.6s ease-in-out infinite;
  }
  .asking .more {
    margin-left: 8px;
    color: var(--i-mut);
  }
  @keyframes ask-ring {
    0%,
    100% {
      transform: rotate(0);
    }
    10%,
    30% {
      transform: rotate(14deg);
    }
    20%,
    40% {
      transform: rotate(-14deg);
    }
    50% {
      transform: rotate(0);
    }
  }
  :global([data-reduced-motion]) .asking :global(svg) {
    animation: none;
  }
  .count.sub {
    margin-right: 10px;
    color: var(--i-mut);
  }
  .count {
    font-size: 13px;
    font-weight: 700;
  }
  .idle {
    padding: 0 14px;
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .idle.one {
    justify-content: center;
  }
  .iside {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .iitem {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-weight: 500;
  }
  .iitem small {
    color: var(--i-mut);
    font-size: 10px;
    font-weight: 600;
  }
  .activity {
    justify-content: center;
    gap: 8px;
  }
  .atext {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .ends {
    transition:
      opacity var(--m-pillRise-dur) var(--m-pillRise-ease),
      filter var(--m-pillRise-dur) var(--m-pillRise-ease),
      transform var(--m-pillRise-dur) var(--m-pillRise-ease);
  }
  .later {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 100%;
    color: var(--i-mut);
  }
  .later strong {
    color: var(--i-fg);
    font-size: 14px;
  }

  /* Shared by the modules. */
  .root :global(button.icon) {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--i-fg);
    cursor: pointer;
  }
  .root :global(button.icon:hover) {
    background: var(--i-hover);
  }
  .root :global(button:disabled) {
    opacity: 0.4;
    cursor: default;
  }
  .root :global(input[type='range']) {
    height: 4px;
    margin: 8px 0;
    accent-color: var(--i-fg);
  }
  .root :global(.eq) {
    display: inline-flex;
    align-items: flex-end;
    gap: 2px;
    height: 12px;
  }
  .root :global(.eq i) {
    width: 3px;
    height: 100%;
    border-radius: 1px;
    background: currentColor;
    transform-origin: bottom;
    animation: eq 0.9s ease-in-out infinite alternate;
  }
  .root :global(.eq i:nth-child(2)) {
    animation-delay: -0.3s;
  }
  .root :global(.eq i:nth-child(3)) {
    animation-delay: -0.6s;
  }
  .root :global(.eq.still i) {
    height: 30%;
    animation: none;
  }
  @keyframes -global-eq {
    from {
      transform: scaleY(0.25);
    }
    to {
      transform: scaleY(1);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .root :global(.eq i) {
      animation: none;
      height: 60%;
    }
  }
</style>
