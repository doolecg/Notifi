import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type Theme = 'system' | 'light' | 'dark';

export interface General {
  autostart: boolean;
  theme: Theme;
  translucent: boolean;
  reducedMotion: boolean;
  language: string;
  autoUpdate: boolean;
  updateCheckHours: number;
  installOnQuit: boolean;
  betaChannel: boolean;
  previewNotes: boolean;
  whatsNew: boolean;
  showTour: boolean;
  haptics: boolean;
}

export interface Panel {
  lastTab: string;
  tabs: Record<string, boolean>;
  order: string[];
  keepAwakeMinutes: number;
  trayIcon: string;
  hideIconWithReadouts: boolean;
  leftClick: string;
  compact: boolean;
}

export type Values = Record<string, unknown>;

export interface Settings {
  general: General;
  shortcuts: Record<string, string>;
  /** Option values per catalog namespace (a page id, or a card's sub-feature id). */
  features: Record<string, Values>;
  panel: Panel;
  /** The Island's options (spec/everyday/island.md); see IslandConfig in ui/src/island/types.ts. */
  island: Values;
}

export interface Choice {
  value: string;
  label: string;
  icon: string;
}

export type Control =
  | { kind: 'switch'; default: boolean }
  | { kind: 'picker'; choices: Choice[]; default: string }
  | { kind: 'tiles'; choices: Choice[]; default: string }
  | { kind: 'slider'; min: number; max: number; step: number; unit: string; default: number }
  | { kind: 'number'; min: number; max: number; step: number; unit: string; default: number }
  | { kind: 'text'; placeholder: string; default: string }
  | { kind: 'list'; placeholder: string; default: string[] }
  | { kind: 'shortcut'; default: string }
  | { kind: 'custom' };

export interface Opt {
  id: string;
  label: string;
  hint: string;
  icon: string;
  control: Control;
}

export interface Card {
  title: string;
  tab: string;
  feature: string;
  options: Opt[];
}

export interface Page {
  id: string;
  label: string;
  icon: string;
  blurb: string;
  feature: boolean;
  tool: boolean;
  platform: string;
  tabs: string[];
  custom: boolean;
  cards: Card[];
  specs: string[];
}

export interface Group {
  id: string;
  label: string;
  pages: Page[];
}

export interface PanelTab {
  id: string;
  label: string;
  /** Feature namespace (or `namespace.key`) that must be on for the tab to show. */
  feature: string | null;
  /** Settings page the customise button opens. */
  section: string | null;
  ready: boolean;
}

export interface Catalog {
  groups: Group[];
  actions: { id: string; label: string; defaultAccel: string }[];
  reserved: { accel: string; label: string }[];
  panelTabs: PanelTab[];
  keepAwakeChoices: number[];
}

/** Sections stored as typed structs rather than in `features`. */
const TYPED = ['general', 'panel', 'island'];

const IS_WINDOWS = typeof navigator !== 'undefined' && navigator.userAgent.includes('Windows');

/** Whether a page exists on this platform. */
export function onPlatform(page: Page) {
  return !page.platform || page.platform === (IS_WINDOWS ? 'windows' : 'linux');
}

/** The namespace a card's options are stored under. */
export function namespaceOf(page: Page, card: Card) {
  return card.feature || page.id;
}

class SettingsStore {
  settings = $state<Settings | null>(null);
  catalog = $state<Catalog | null>(null);
  /** Actions whose shortcut the OS refused, with the reason. */
  shortcutErrors = $state<Record<string, string>>({});

  async init() {
    await listen<Settings>('settings://changed', (e) => (this.settings = e.payload));
    await listen<Record<string, string>>('shortcuts://status', (e) => (this.shortcutErrors = e.payload));
    [this.settings, this.catalog, this.shortcutErrors] = await Promise.all([
      invoke<Settings>('get_settings'),
      invoke<Catalog>('get_catalog'),
      invoke<Record<string, string>>('shortcut_status'),
    ]);
  }

  /** Merges `value` (some or all of a section's keys) into a section; saved and applied at once. */
  async set(section: string, value: unknown) {
    this.settings = await invoke<Settings>('set_section', { section, value });
  }

  async reset(section: string) {
    this.settings = await invoke<Settings>('reset_section', { section });
  }

  /** The values of a namespace, typed sections included. */
  values(namespace: string): Values {
    const s = this.settings;
    if (!s) return {};
    if (TYPED.includes(namespace)) return s[namespace as 'general' | 'panel' | 'island'] as unknown as Values;
    return s.features[namespace] ?? {};
  }

  setOption(namespace: string, key: string, value: unknown) {
    return this.set(namespace, { [key]: value });
  }

  /** Whether a feature namespace is on; `namespace.key` reads that switch instead of `enabled`. */
  featureOn(path: string) {
    const [namespace, key = 'enabled'] = path.split('.');
    return this.values(namespace)[key] === true;
  }

  /** Every page on this platform. */
  pages(): Page[] {
    return (this.catalog?.groups ?? []).flatMap((g) => g.pages.filter(onPlatform));
  }

  page(id: string) {
    return this.pages().find((p) => p.id === id);
  }

  pauseShortcuts(paused: boolean) {
    return invoke('pause_shortcuts', { paused });
  }
}

export const store = new SettingsStore();
