export interface IslandConfig {
  enabled: boolean;
  openOn: 'click' | 'hover';
  hoverDelayMs: number;
  collapseDelayMs: number;
  display: string;
  hideInFullscreen: boolean;
  size: 'compact' | 'spacious' | 'custom';
  customWidth: number;
  customHeight: number;
  translucent: boolean;
  defaultModule: string;
  modules: string[];
  orbitLeft: string[];
  orbitRight: string[];
  orbitBottom: string[];
  controlsButtons: string[];
  showLyrics: boolean;
  equaliser: boolean;
  preferredPlayer: string;
  hideTimerCountdown: boolean;
  activityTrack: boolean;
  activityTimer: boolean;
  activityAgent: boolean;
  activityMs: number;
  agentFinishMinutes: number;
  claudePlanLimitUsd: number;
  agentsPeriod: 'today' | '7d' | '30d';
  agentsShowLimits: boolean;
  agentsShowResets: boolean;
  agentsShowModels: boolean;
  agentsShowProjects: boolean;
  agentsShowActivity: boolean;
  idleClock: boolean;
  idleDate: boolean;
  idleBattery: boolean;
  idleCpuRam: boolean;
  idleAiUsage: boolean;
  idleThinBar: boolean;
  dock: 'top' | 'bottomLeft';
  agentsTaskbar: boolean;
  dynamicNotifications: boolean;
  dynamicOther: boolean;
  dynamicApps: string[];
  chatEnabled: boolean;
  activitySound: boolean;
  alertsInIsland: boolean;
  privacyMode: boolean;
  claudeUsageConnect: boolean;
  askInIsland: boolean;
}

/** Mirrors `IslandConfig::default()` in src-tauri/src/island/config.rs. */
export const DEFAULT_CONFIG: IslandConfig = {
  enabled: false,
  openOn: 'click',
  hoverDelayMs: 250,
  collapseDelayMs: 600,
  display: 'primary',
  hideInFullscreen: true,
  size: 'compact',
  customWidth: 1200,
  customHeight: 270,
  translucent: false,
  defaultModule: 'nowPlaying',
  modules: ['controls', 'mixer', 'nowPlaying', 'clipboard', 'captures', 'shelf', 'system', 'tools', 'calendar', 'notifications', 'timer', 'camera', 'downloads', 'scratchpad', 'agents'],
  orbitLeft: ['grid', 'timer'],
  orbitRight: ['settings', 'volume'],
  orbitBottom: ['nowPlaying'],
  controlsButtons: ['mixer', 'keepAwake', 'timer', 'calendar'],
  showLyrics: true,
  equaliser: true,
  preferredPlayer: '',
  hideTimerCountdown: false,
  activityTrack: true,
  activityTimer: true,
  activityAgent: true,
  activityMs: 3000,
  agentFinishMinutes: 2,
  claudePlanLimitUsd: 0,
  agentsPeriod: '7d',
  agentsShowLimits: true,
  agentsShowResets: true,
  agentsShowModels: true,
  agentsShowProjects: true,
  agentsShowActivity: true,
  idleClock: true,
  idleDate: true,
  idleBattery: true,
  idleCpuRam: true,
  idleAiUsage: true,
  idleThinBar: true,
  dock: 'top',
  agentsTaskbar: false,
  dynamicNotifications: false,
  dynamicOther: false,
  dynamicApps: ['Just Eat', 'Uber Eats', 'Uber', 'Deliveroo'],
  chatEnabled: false,
  activitySound: true,
  alertsInIsland: true,
  privacyMode: false,
  claudeUsageConnect: false,
  askInIsland: false,
};

export const MODULE_LABELS: Record<string, string> = {
  controls: 'Controls',
  mixer: 'Volume mixer',
  nowPlaying: 'Now Playing',
  clipboard: 'Clipboard',
  captures: 'Recent captures',
  shelf: 'Files',
  system: 'System',
  tools: 'Tools',
  calendar: 'Calendar',
  notifications: 'Notifications',
  timer: 'Timer',
  camera: 'Camera mirror',
  downloads: 'Downloads',
  scratchpad: 'Scratchpad',
  agents: 'AI Agents',
};

export interface Layout {
  /** Docked bottom-left: orbit points are from the island's left edge and top. */
  dock: boolean;
  window: [number, number];
  /** The size in use now: `base`, `tall` or `chat`. */
  open: [number, number];
  base: [number, number];
  tall: [number, number];
  chat: [number, number];
  radius: number;
  maxLeft: number;
  maxRight: number;
  maxBottom: number;
  left: [number, number][];
  right: [number, number][];
  bottom: [number, number][];
}

export interface ModuleInfo {
  id: string;
  label: string;
  ready: boolean;
}

export interface Media {
  title: string;
  artist: string;
  album: string;
  app: string;
  appId: string;
  playing: boolean;
  positionMs: number;
  durationMs: number | null;
  canPlayPause: boolean;
  canPrev: boolean;
  canNext: boolean;
  canSeek: boolean;
  nextTitle: string | null;
}

export interface Lyrics {
  synced: boolean;
  lines: { ms: number | null; text: string }[];
}

export interface Player {
  id: string;
  name: string;
}

export interface TimerSnap {
  phase: 'idle' | 'running' | 'paused' | 'done';
  totalMs: number;
  endsAt: number;
  remainingMs: number;
  leftMs: number;
}

export interface Limit {
  usedPercent: number;
  resetsAt: number | null;
  windowMs?: number | null;
}

export interface Agent {
  id: string;
  name: string;
  supported: boolean;
  working: boolean;
  /** Sessions of this agent working now. */
  count: number;
  subagents: number;
  terminals: number;
  elapsedMs: number;
  model: string | null;
  project: string | null;
  usage: { read: number; written: number };
  sessionLimit: Limit | null;
  weekLimit: Limit | null;
  lastActive: number;
}

export type ProviderId = 'claude' | 'codex' | 'openCode' | 'copilot';

export interface ToolInfo {
  id: ProviderId;
  name: string;
  parsed: boolean;
  plan: string | null;
  lastUsed: number | null;
}

export interface LimitView {
  usedPercent: number;
  resetsAt: number | null;
  pace: number;
}

export interface Named {
  name: string;
  provider: ProviderId;
  tokens: number;
}

export interface UsageSummary {
  days: number;
  costUsd: number;
  minimum: boolean;
  tokens: number;
  cacheShare: number;
  providers: { costUsd: number; tokens: number }[];
  trend: [number, number, number][];
  trendFirst: number;
  hourly: boolean;
  models: Named[];
  projects: Named[];
  activity: number[];
  activityFirst: number;
  activityTotal: number;
  activeDays: number;
  busiest: [number, number] | null;
  streak: number;
}

export interface AgentStats {
  ready: boolean;
  now: number;
  tools: ToolInfo[];
  claudeWindow: { start: number; resetsAt: number; costUsd: number; tokens: number; pace: number } | null;
  planLimitUsd: number;
  codexSession: LimitView | null;
  codexWeek: LimitView | null;
  summary: UsageSummary;
}

export interface Live {
  id: string;
  label: string;
  icon: string;
  color: string;
  value: string;
  status: '' | 'idle' | 'running' | 'done' | 'needs';
  count: number;
  /** [done, total] of the lead agent's to-do list. */
  todos: [number, number] | null;
  tokens: number;
  sessionPct: number | null;
  weekPct: number | null;
  project: string | null;
  /** Terminals open for the agent shown. */
  terminals: number;
  /** Subagents running for the agent shown. */
  subagents: number;
  /** 0 to 1 on an order or ride card. */
  progress: number | null;
  /** The stage of an order or ride card. */
  detail: string | null;
}

/** Whole percents for the resting pill's ends; null when off or not readable. */
export interface Idle {
  cpu: number | null;
  mem: number | null;
  /** Also null when the machine has no battery. */
  battery: number | null;
  charging: boolean;
  ai: number | null;
}

/** A session as a button on the taskbar bar. */
export interface SessionChip {
  id: string;
  agent: string;
  project: string | null;
  status: 'working' | 'needs' | 'done';
  elapsedMs: number;
  lastTs: number;
  /** The T3 Code chat this session is, with its title. */
  title: string | null;
  thread: string | null;
}

export interface Snapshot {
  enabled: boolean;
  config: IslandConfig;
  layout: Layout;
  modules: ModuleInfo[];
  media: Media | null;
  players: Player[];
  timer: TimerSnap;
  agents: Agent[];
  sessions: SessionChip[];
  live: Live[];
  idle: Idle;
  volume: number | null;
  brightness: number | null;
  mediaSupported: boolean;
  open: boolean;
  /** Text read from the watched screen area; null when nothing is watched or it is not shown on the pill. */
  watch: WatchReading | null;
}

/** `paused` says why nothing is being read (the window moved, text recognition failed); empty while reading. */
export interface WatchReading {
  text: string;
  paused: string;
}

export interface Activity {
  icon: string;
  color: string;
  text: string;
  ms: number;
  /** Held back by Do Not Disturb: goes to the list of past notifications only. */
  quiet?: boolean;
}

/** Keybind action for each module that has one (see keybinds.rs). */
export const MODULE_ACTIONS: Record<string, string> = {
  nowPlaying: 'islandModuleNowPlaying',
  controls: 'islandModuleControls',
  timer: 'islandModuleTimer',
  agents: 'islandModuleAgents',
  grid: 'islandModuleGrid',
};

/** Orbit and quick buttons: modules open inside the island, the rest run outside it. */
export const BUTTONS: Record<string, { label: string; icon: string }> = {
  grid: { label: 'Modules', icon: 'grid' },
  nowPlaying: { label: 'Now Playing', icon: 'music' },
  controls: { label: 'Controls', icon: 'controls' },
  timer: { label: 'Timer', icon: 'timer' },
  agents: { label: 'AI Agents', icon: 'agents' },
  volume: { label: 'Volume', icon: 'speaker' },
  settings: { label: 'Settings', icon: 'settings' },
  mixer: { label: 'Volume mixer', icon: 'mixer' },
  keepAwake: { label: 'Keep awake', icon: 'keepAwake' },
  calendar: { label: 'Calendar', icon: 'calendar' },
  chat: { label: 'AI chat', icon: 'chat' },
  dictate: { label: 'Dictate (Windows voice typing)', icon: 'mic' },
};

export const MODULE_ICONS: Record<string, string> = {
  controls: 'controls',
  mixer: 'mixer',
  nowPlaying: 'music',
  clipboard: 'clipboard',
  captures: 'captures',
  shelf: 'shelf',
  system: 'system',
  tools: 'tools',
  calendar: 'calendar',
  notifications: 'bell',
  timer: 'timer',
  camera: 'camera',
  downloads: 'download',
  scratchpad: 'notes',
  agents: 'agents',
};

export function clock(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor(s / 60) % 60;
  const ss = String(s % 60).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

export function tokens(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)}B`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)}k`;
  return String(n);
}
