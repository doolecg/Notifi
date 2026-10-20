import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type Theme = 'system' | 'light' | 'dark';

export interface General {
  autostart: boolean;
  theme: Theme;
  reducedMotion: boolean;
  autoUpdate: boolean;
  updateCheckHours: number;
  installOnQuit: boolean;
}

export interface Panel {
  lastTab: string;
  tabs: Record<string, boolean>;
  keepAwakeMinutes: number;
}

export interface Settings {
  general: General;
  shortcuts: Record<string, string>;
  features: Record<string, Record<string, boolean>>;
  panel: Panel;
}

export interface Feature {
  id: string;
  label: string;
  blurb: string;
  tool: boolean;
}

export interface FeatureGroup {
  id: string;
  label: string;
  features: Feature[];
}

export interface PanelTab {
  id: string;
  label: string;
  /** `group.feature` that must be on for the tab to show. */
  feature: string | null;
  section: string | null;
  ready: boolean;
}

export interface Catalog {
  groups: FeatureGroup[];
  actions: { id: string; label: string; defaultAccel: string }[];
  reserved: { accel: string; label: string }[];
  panelTabs: PanelTab[];
  keepAwakeChoices: number[];
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

  async set(section: string, value: unknown) {
    this.settings = await invoke<Settings>('set_section', { section, value });
  }

  async reset(section: string) {
    this.settings = await invoke<Settings>('reset_section', { section });
  }

  /** Whether `group.feature` is switched on. */
  featureOn(path: string) {
    const [group, id] = path.split('.');
    return this.settings?.features[group]?.[id] ?? false;
  }

  pauseShortcuts(paused: boolean) {
    return invoke('pause_shortcuts', { paused });
  }
}

export const store = new SettingsStore();
