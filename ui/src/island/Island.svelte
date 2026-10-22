<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, untrack } from 'svelte';
  import { fade } from 'svelte/transition';
  import { applyCssVars, duration, setReducedMotion } from '../lib/motion';
  import { store } from '../lib/settings.svelte';
  import AiButtons from './AiButtons.svelte';
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
  /** The idle info pill is sized from what it shows (rough widths of each item), between these. */
  const IDLE_MIN = 190;
  const IDLE_MAX = 820;
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
  /** With the AI Agents bar on the taskbar, the top pill leaves the agents out; docked or rising, the pill is that bar. */
  const live = $derived((snap?.live ?? []).filter((l) => !(l.id === 'agents' && cfg?.agentsTaskbar && !layout?.dock && !layout?.rise)));

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
  /** An AI app in front: on the taskbar the pill becomes that app's buttons beside the agent's limits. */
  const aiOn = $derived(!!snap?.aiApp && !open && !!(layout?.dock || layout?.rise));
  const agentLive = $derived(live.find((l) => l.id === 'agents'));

  /** One thing on the resting pill. `w` is its rough width, for sizing the pill. */
  type IdleItem = {
    key: string;
    kind: 'weather' | 'bell' | 'stat' | 'ask' | 'agents';
    text: string;
    label?: string;
    icon?: string;
    tip?: string;
    pct?: number;
    /** A meter: a ring with the value inside, a small bar, or plain text. */
    look?: 'ring' | 'bar' | 'text';
    /** The Ask AI button: a chip with a chevron, bare text, a search-style field, or just the icon. */
    ask?: 'chev' | 'plain' | 'field' | 'icon';
    dot?: string;
    w: number;
  };
  const pillStyle = $derived(cfg?.pillStyle ?? 'calm');
  /** The pill's contents in the chosen style, as one group per black pill (only "chips" has more than one). */
  const idleGroups = $derived.by((): IdleItem[][] => {
    if (!cfg) return [];
    const s = pillStyle;
    const first: IdleItem[] = [];
    const stats: IdleItem[] = [];
    const bell: IdleItem[] = [];
    const ask: IdleItem[] = [];
    const stat = (key: string, label: string | undefined, v: number, look: 'ring' | 'bar' | 'text', icon?: string, tip?: string): IdleItem => {
      const l = label ? label.length * 6.2 + 4 : 0;
      const digits = String(v).length;
      return { key, kind: 'stat', label, icon, text: `${v}%`, pct: v, look, tip, w: look === 'ring' ? 22 + (label ? l + 4 : 0) : look === 'bar' ? (label ? l : 17) + 30 + digits * 6.8 + 8 : (label ? l : 17) + (digits + 1) * 6.8 };
    };
    if (s === 'agents') {
      const n = (snap?.agents ?? []).filter((a) => a.working).reduce((t, a) => t + a.count, 0);
      const l = snap?.live.find((x) => x.id === 'agents');
      const dot = l?.status === 'needs' ? 'var(--i-agent)' : l?.status === 'done' ? '#35d6a4' : n ? 'var(--i-acc)' : 'var(--i-mut)';
      const text = n ? `${n} agent${n === 1 ? '' : 's'} working` : 'Agents idle';
      first.push({ key: 'agents', kind: 'agents', text, dot, w: 16 + text.length * 6.4 });
      const pct = l?.sessionPct ?? snap?.idle.ai ?? null;
      if (cfg.idleAiUsage && pct !== null) stats.push(stat('five', '5h', Math.round(pct), 'bar', undefined, 'Share of the 5 hour limit used'));
    }
    const wx = snap?.weather;
    if (wx) first.push({ key: 'wx', kind: 'weather', icon: wx.icon, text: `${wx.temp}°`, tip: `${wx.city}: ${wx.label}`, w: 22 + String(wx.temp).length * 8 });
    else if (cfg.weatherCity && snap?.weatherError) first.push({ key: 'wx', kind: 'weather', icon: 'cloud', text: '--', tip: snap.weatherError, w: 40 });
    const i = snap?.idle;
    if (i) {
      const look = s === 'calm' || s === 'rings' ? 'ring' : 'text';
      const label = (l: string) => (s === 'rings' ? undefined : l);
      const add = (key: string, l: string, v: number | null, tip?: string) => {
        if (v !== null) stats.push(stat(key, label(l), v, look, undefined, tip));
      };
      if (s === 'field') {
        if (cfg.idleAiUsage && i.ai !== null) stats.push(stat('ai', undefined, i.ai, 'ring', undefined, 'Share of the Claude 5 hour session used'));
      } else if (s === 'agents') {
        if (cfg.idleCpuRam) add('cpu', 'CPU', i.cpu);
      } else {
        if (cfg.idleAiUsage) add('ai', 'AI', i.ai, 'Share of the Claude 5 hour session used');
        if (cfg.idleCpuRam) {
          add('cpu', 'CPU', i.cpu);
          add('mem', 'RAM', i.mem);
        }
        if (cfg.idleBattery) {
          if (i.battery !== null) stats.push(stat('battery', undefined, i.battery, 'text', i.charging ? 'bolt' : 'battery', i.charging ? 'Charging' : 'Battery'));
          else if (!cfg.idleCpuRam) add('cpu', 'CPU', i.cpu);
        }
      }
    }
    const waiting = island.history.length;
    if (cfg.idleNotifications && waiting) bell.push({ key: 'bell', kind: 'bell', text: String(waiting), tip: 'Notifications', w: 22 + String(waiting).length * 7.5 });
    if (cfg.chatEnabled && s !== 'agents') {
      const look = { calm: 'chev', chips: 'plain', native: 'plain', field: 'field', rings: 'icon' }[s as string] as 'chev' | 'plain' | 'field' | 'icon';
      ask.push({ key: 'ask', kind: 'ask', ask: look, text: 'Ask AI', w: { chev: 100, plain: 56, field: 190, icon: 22 }[look] });
    }
    const groups = s === 'chips' ? [first, [...stats, ...bell], ask] : s === 'field' ? [[...first, ...ask, ...stats, ...bell]] : [[...first, ...stats, ...bell, ...ask]];
    return groups.filter((g) => g.length);
  });
  const idleShown = $derived(idleGroups.length > 0);
  /** Rough width of the pill in the chosen style: each style spaces its items its own way. */
  const idleTotal = $derived.by(() => {
    const sum = (g: IdleItem[]) => g.reduce((w, it) => w + it.w, 0);
    if (pillStyle === 'chips') return idleGroups.reduce((t, g) => t + sum(g) + (g.length - 1) * 12 + 24, 0) + (idleGroups.length - 1) * 8;
    const items = idleGroups.flat();
    if (pillStyle === 'native') return items.reduce((t, it) => t + it.w + 24, 0);
    return 28 + sum(items) + (items.length - 1) * 14;
  });

  /** A quick AI button: the prompt is copied and the chat opens (Notifi shows a notice if that fails). */
  const askAi = (index: number) => invoke('island_ai_prompt', { index }).catch(() => {});
  /** The prompt list under the Ask AI button; it opens above the pill on the taskbar and below it at the top. */
  let pop = $state<{ left: number; y: number; up: boolean } | null>(null);
  const POP_W = 190;
  const POP_ROW = 32;
  const popH = $derived(pop && cfg ? cfg.aiPrompts.length * POP_ROW + 12 : 0);
  function askClick(e: MouseEvent) {
    e.stopPropagation();
    if (!cfg?.aiPrompts.length) {
      invoke('island_open_chat').catch(() => {});
      return;
    }
    if (pop) {
      pop = null;
      return;
    }
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    // Clear of the whole pill, not just of the button.
    const pr = (e.currentTarget as HTMLElement).closest('.pill')?.getBoundingClientRect() ?? r;
    const up = !!(layout?.rise || layout?.dock);
    pop = { left: Math.max(8, Math.min(window.innerWidth - POP_W - 8, r.left + r.width / 2 - POP_W / 2)), y: up ? window.innerHeight - pr.top + 8 : pr.bottom + 8, up };
  }
  function pickPrompt(index: number) {
    pop = null;
    askAi(index);
  }
  // The list closes soon after the pointer leaves, and in any case after a while.
  $effect(() => {
    if (!pop) return;
    const t = setTimeout(() => (pop = null), near ? 8000 : 700);
    return () => clearTimeout(t);
  });
  $effect(() => {
    if (open) pop = null;
  });
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
      ? Math.round(Math.min(IDLE_MAX, Math.max(IDLE_MIN, idleTotal)))
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
  /** The thin bar belongs to the top dock; on the taskbar (docked or rising from the bottom) the pill stays visible. */
  const thinOn = $derived(!!cfg?.idleThinBar && !layout?.dock && !layout?.rise);
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
    if (aiOn) return Math.min(AGENTS_MAX, Math.max(LIVE.w, agentsNeed + 12));
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
    : aiOn ? { w: liveW, h: pillBar }
    : infoChosen ? { w: idleW, h: pillBar }
    : showLive ? { w: Math.max(liveW, idleW), h: pillBar }
    : thin ? THIN
    : { w: idleW, h: pillBar },
  );
  const box = $derived(
    thin
      ? THIN_HIT
      : { w: barCap(rest.w), h: rest.h + (pop ? popH + 8 : 0) },
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

  $effect(() => setReducedMotion(store.settings?.general.reducedMotion ?? false));

  function clearTimers() {
    clearTimeout(hoverTimer);
    clearTimeout(collapseTimer);
  }

  /** What the open Island shows first: a waiting question, otherwise the Nook. */
  function autoModule() {
    return island.asks.length ? 'ask' : 'nook';
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
    // Two frames: the docked menu is created closed in the first, so it has a size to grow from.
    requestAnimationFrame(() => requestAnimationFrame(() => (expanded = true)));
  }

  function collapse() {
    clearTimers();
    if (!open || collapsing) return;
    collapsing = true;
    // The chat (a separate page over the menu) sees `collapsing` and shrinks into the pill alongside it.
    expanded = false;
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
      // Notifi keeps the old window size while the menu shrinks, so this goes at once.
      chatMode = false;
      invoke('island_chat', { on: false }).catch(() => {});
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
    // On the taskbar (docked, or rising from it) the pill lifts out from under the pointer, so leaving never closes it.
    if (open && !layout?.dock && !layout?.rise && module !== 'chat') collapseTimer = setTimeout(collapse, cfg?.collapseDelayMs ?? 600);
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

  async function clickPill(e: MouseEvent) {
    // Clicks inside the open menu (its tabs and buttons) are theirs: they must not reach here and reopen a menu they just closed.
    if ((e.target as Element).closest('.tabs, .content')) return;
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

  /** A notification that belongs to a feature: its module opens in the Island, its app opens outside it. */
  function go(target: string) {
    if (target.startsWith('module:')) openIsland(target.slice(7));
    else if (target.startsWith('apps:')) {
      invoke('apps_open', { view: target.slice(5) }).catch(() => {});
      collapse();
    }
  }

  function command(cmd: string) {
    if (cmd === 'toggle') {
      if (open && !collapsing) collapse();
      else openIsland();
    } else if (cmd === 'close') {
      collapse();
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
      if (e.key === 'Escape') {
        pop = null;
        collapse();
      }
    };
    // Opened by hovering, the window can't take keys yet: a click anywhere in it does, so Esc works.
    const focusOnClick = () => { if (open) invoke('island_focus').catch(() => {}); };
    window.addEventListener('keydown', key);
    window.addEventListener('pointerdown', focusOnClick, true);
    return () => {
      window.removeEventListener('keydown', key);
      window.removeEventListener('pointerdown', focusOnClick, true);
    };
  });

  /** Docked, the bar stays in the taskbar and the open menu is a separate flyout above it. */
  const dock = $derived(!!layout?.dock);
  /** At the bottom centre (Start at the left): like the dock, the bar stays in the taskbar and the open menu is a flyout above it. */
  const rise = $derived(!!layout?.rise);
  /** Docked, the bar keeps clear of the Start button. */
  function barCap(w: number) {
    return dock && layout?.barMax ? Math.min(w, layout.barMax) : w;
  }
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
  const grown = $derived(expanded && !dock && !rise);
  // Docked, the bar in the taskbar widens to the menu above it, so the two read as one block, but never past the Start button.
  const pillW = $derived(barCap(expanded && layout && !rise ? openW : rest.w));
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
    const shown = selLive?.id === 'agents' ? selLive : aiOn ? agentLive : undefined;
    const kind = shown ? ({ claude: 0, codex: 1, opencode: 2 } as Record<string, number>)[shown.icon] : undefined;
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
      <section><Notifications onGo={go} /></section>
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
    <Chat closing={collapsing} pill={dock || rise ? null : { openW, openH: flyH, restW: rest.w, restH: rest.h }} onAsk={() => setModule('ask')} />
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
    <Notifications onGo={go} />
  {:else}
    <div class="later">
      <Glyph name={MODULE_ICONS[module] ?? 'utilities'} size={30} />
      <strong>{moduleLabel}</strong>
      <span>Coming in a later version of Notifi.</span>
    </div>
  {/if}
{/snippet}

{#snippet idleRow(group: IdleItem[])}
  {#each group as it (it.key)}
    {#if it.kind === 'weather'}
      <span class="iwx" title={it.tip}><Glyph name={it.icon ?? 'cloud'} size={15} />{it.text}</span>
    {:else if it.kind === 'bell'}
      <span class="ibell" title={it.tip}><Glyph name="bell" size={14} />{it.text}</span>
    {:else if it.kind === 'agents'}
      <span class="iag"><i class="idot" style:background={it.dot}></i>{it.text}</span>
    {:else if it.kind === 'ask'}
      {#if it.ask === 'field'}
        <button class="ifield" onclick={askClick}><Glyph name="sparkle" size={13} />Ask AI...</button>
      {:else}
        <button class="iask" class:plain={pillStyle === 'chips' || pillStyle === 'native'} class:icon={it.ask === 'icon'} aria-label="Ask AI" title="Ask AI" onclick={askClick}>
          <Glyph name="sparkle" size={13} />{#if it.ask !== 'icon'}Ask AI{/if}{#if it.ask === 'chev'}<Glyph name={layout?.rise || layout?.dock ? 'expand' : 'shrink'} size={13} />{/if}
        </button>
      {/if}
    {:else}
      <span class="istat" class:warn={(it.pct ?? 0) >= 70} class:hot={(it.pct ?? 0) >= 90} title={it.tip ?? it.label}>
        {#if it.look === 'ring'}
          <span class="iring"><svg viewBox="0 0 22 22" aria-hidden="true"><circle class="t" cx="11" cy="11" r="8.5" /><circle class="v" cx="11" cy="11" r="8.5" stroke-dasharray={`${((it.pct ?? 0) / 100) * 53.4} 53.4`} /></svg>{it.pct}</span>
          {#if it.label}<small>{it.label}</small>{/if}
        {:else}
          {#if it.icon}<Glyph name={it.icon} size={13} />{/if}{#if it.label}<small>{it.label}</small>{/if}{#if it.look === 'bar'}<i class="ibar"><u style:width={`${it.pct}%`}></u></i>{/if}{it.text}
        {/if}
      </span>
    {/if}
  {/each}
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
    <button class="act" aria-label="Close" title="Close (Esc)" onclick={(e) => { e.stopPropagation(); pop = null; collapse(); }}><Glyph name="close" size={16} /></button>
  </nav>
{/snippet}

{#if snap?.enabled && cfg && layout}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="root" class:translucent={cfg.translucent} class:dock={layout.dock} class:rise={layout.rise} onpointerenter={() => (near = true)} onpointerleave={() => (near = false)}>
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
      style:bottom={layout.dock ? `${DOCK_GAP}px` : rise ? "0px" : undefined}
      style:border-radius={layout.dock ? `0 ${radius}px ${radius}px 0` : rise ? `${radius}px` : `0 0 ${radius}px ${radius}px`}
      onpointerenter={enter}
      onpointerleave={leave}
      onclick={clickPill}
      onwheel={wheel}
    >
      {#if open && !dock && !rise}
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
      {:else if aiOn}
        <div class="ends live-in" out:fade={swapOut()} use:fit={needs}>
          <AiButtons />
          {#if agentLive}
            <span class="limits">
              {@render limit('5h', agentLive.sessionPct, '5 hour limit used')}
              {@render limit('7d', agentLive.weekPct, 'Weekly limit used')}
              {#if dayTokens !== null}<span class="tok" title="Tokens used today"><small>today</small>{compact(dayTokens)}</span>{/if}
            </span>
          {/if}
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
        <div class="ends idle live-in {pillStyle}" class:bare={pillStyle === 'chips' || pillStyle === 'native'} out:fade={swapOut()}>
          {#each idleGroups as group, g (g)}
            {#if pillStyle === 'chips'}
              <div class="ichip">{@render idleRow(group)}</div>
            {:else}
              {@render idleRow(group)}
            {/if}
          {/each}
        </div>
      {/if}
    </div>

    {#if pop && !open}
      <div class="askpop" style:left={`${pop.left}px`} style:bottom={pop.up ? `${pop.y}px` : undefined} style:top={pop.up ? undefined : `${pop.y}px`} in:fade={{ duration: 120 }}>
        {#each cfg.aiPrompts as p, i (i)}
          <button onclick={() => pickPrompt(i)}>{p}</button>
        {/each}
      </div>
    {/if}

    {#if open && (dock || rise)}
      <div
        class="fly"
        class:shown={expanded}
        style:bottom={`${bar + FLY_GAP}px`}
        style:width={`${openW}px`}
        style:height={`${expanded ? flyH : 0}px`}
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
  /* Docked, the bar and its menu use the top Island's black, not the taskbar's colour. */
  .dock .pill {
    top: auto;
    left: 0;
    transform: none;
    box-shadow:
      inset 0 0 0 1px rgb(255 255 255 / 0.06),
      inset 0 1px 0 rgb(255 255 255 / 0.12);
    transition:
      width var(--m-pillRise-dur) var(--m-pillRise-ease),
      height var(--m-pillRise-dur) var(--m-pillRise-ease),
      bottom var(--m-pillRise-dur) var(--m-pillRise-ease),
      border-radius var(--m-pillRise-dur) var(--m-pillRise-ease);
  }
  /* The top Island upside down: rests on the bottom edge and grows up. */
  .rise .pill {
    top: auto;
    box-shadow:
      0 -6px 18px rgb(0 0 0 / 0.35),
      inset 0 0 0 1px rgb(255 255 255 / 0.06),
      inset 0 1px 0 rgb(255 255 255 / 0.16);
    transition:
      width var(--m-pillRise-dur) var(--m-pillRise-ease),
      height var(--m-pillRise-dur) var(--m-pillRise-ease),
      bottom var(--m-pillRise-dur) var(--m-pillRise-ease),
      border-radius var(--m-pillRise-dur) var(--m-pillRise-ease);
  }
  /* Rising, the menu flyout is centred over the bar. */
  .rise .fly {
    left: 50%;
    transform: translateX(-50%);
  }
  /* The menu grows up out of the bar on the same spring the top Island drops on, already at the bar's width. */
  .fly {
    position: absolute;
    left: 0;
    overflow: hidden;
    background: var(--i-bg);
    box-shadow:
      0 -6px 18px rgb(0 0 0 / 0.35),
      inset 0 0 0 1px rgb(255 255 255 / 0.06),
      inset 0 1px 0 rgb(255 255 255 / 0.12);
    opacity: 0;
    transition:
      opacity var(--m-pillRise-dur) var(--m-pillRise-ease),
      height var(--m-pillRise-dur) var(--m-pillRise-ease);
  }
  .fly.shown {
    opacity: 1;
    transition:
      opacity var(--m-glassIn-dur) var(--m-glassIn-ease),
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
  /* A clicked tab keeps the keyboard, which drew a heavy ring; a soft one is enough. */
  .tab:focus-visible,
  .act:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--i-fg), transparent 70%);
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
    /* At least this wide, sliding sideways when the menu is narrow; sharing the spare room when it is wide. */
    flex: 1 0 250px;
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
    flex: 1.4 0 360px;
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
    justify-content: center;
    gap: 14px;
    padding: 0 14px;
    font-size: 12px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .idle small {
    color: var(--i-mut);
    font-size: 10.5px;
    font-weight: 600;
  }
  .iwx,
  .ibell,
  .istat,
  .iag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .iag {
    gap: 8px;
  }
  .idot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
  }
  .ibell {
    color: var(--i-agent);
  }
  .istat {
    --c: var(--i-acc);
  }
  .istat.warn {
    --c: var(--i-agent);
  }
  .istat.hot {
    --c: #ff6b5e;
  }
  .ibar {
    width: 26px;
    height: 4px;
    border-radius: 99px;
    background: color-mix(in srgb, var(--i-fg), transparent 86%);
    overflow: hidden;
  }
  .ibar u {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--c);
    transition: width var(--m-glassIn-dur) var(--m-glassIn-ease);
  }
  .iring {
    position: relative;
    display: inline-grid;
    place-items: center;
    width: 22px;
    height: 22px;
    margin-right: 4px;
    font-size: 9px;
    font-weight: 700;
  }
  .iring svg {
    position: absolute;
    inset: 0;
    transform: rotate(-90deg);
  }
  .iring circle {
    fill: none;
    stroke-width: 2.6;
    stroke-linecap: round;
  }
  .iring .t {
    stroke: rgb(255 255 255 / 0.14);
  }
  .iring .v {
    stroke: var(--c);
    transition: stroke-dasharray var(--m-glassIn-dur) var(--m-glassIn-ease);
  }
  /* Ask AI: a chip, bare text, a search-style field or just the icon, by style. */
  .iask,
  .ifield {
    display: inline-flex;
    align-items: center;
    height: 22px;
    border: 0;
    color: var(--i-fg);
    font: inherit;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .iask {
    gap: 6px;
    padding: 0 10px 0 8px;
    border-radius: 99px;
    background: rgb(255 255 255 / 0.1);
  }
  .iask:hover {
    background: color-mix(in srgb, var(--i-acc), transparent 78%);
  }
  .iask.plain {
    padding: 0;
    border-radius: 0;
    background: none;
  }
  .iask.plain:hover {
    color: var(--i-acc);
  }
  .iask.icon {
    padding: 0 7px;
  }
  .ifield {
    gap: 8px;
    width: 190px;
    padding: 0 10px;
    border-radius: 99px;
    background: var(--i-raised);
    color: var(--i-mut);
    text-align: left;
  }
  .ifield:hover {
    background: color-mix(in srgb, var(--i-raised), var(--i-fg) 8%);
  }
  .ifield :global(.glyph) {
    color: var(--i-fg);
  }
  /* Three chips: separate small black pills; the pill behind them is not drawn. */
  .idle.chips {
    gap: 8px;
  }
  .ichip {
    display: inline-flex;
    align-items: center;
    gap: 12px;
    height: 100%;
    padding: 0 12px;
    border-radius: 99px;
    background: var(--i-bg);
    box-shadow:
      inset 0 0 0 1px rgb(255 255 255 / 0.06),
      inset 0 1px 0 rgb(255 255 255 / 0.12);
  }
  /* Native text: no black at all, thin dividers between the parts. */
  .idle.bare {
    padding: 0;
  }
  .idle.native {
    gap: 0;
  }
  .idle.native > :global(*) {
    display: inline-flex;
    align-items: center;
    height: 100%;
    padding: 0 12px;
  }
  .idle.native > :global(* + *) {
    border-left: 1px solid rgb(255 255 255 / 0.16);
  }
  .pill:has(> .idle.bare) {
    background: transparent;
    box-shadow: none;
    overflow: visible;
  }
  /* The prompt list under the Ask AI button. */
  .askpop {
    position: absolute;
    z-index: 10;
    display: flex;
    flex-direction: column;
    width: 190px;
    padding: 6px;
    border-radius: 14px;
    background: var(--i-bg);
    box-shadow:
      inset 0 0 0 1px rgb(255 255 255 / 0.07),
      0 6px 18px rgb(0 0 0 / 0.35);
    font-size: 12.5px;
  }
  .askpop button {
    height: 32px;
    padding: 0 10px;
    border: 0;
    border-radius: 9px;
    background: none;
    color: var(--i-fg);
    font: inherit;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }
  .askpop button:hover {
    background: rgb(255 255 255 / 0.08);
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
