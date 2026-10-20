import type { Component } from 'svelte';
import type { Card, Opt, Page, Values } from '../lib/settings.svelte';
import About from './custom/About.svelte';
import Backup from './custom/Backup.svelte';
import FeaturesGrid from './custom/FeaturesGrid.svelte';
import PanelTabs from './custom/PanelTabs.svelte';
import Permissions from './custom/Permissions.svelte';
import ResetAll from './custom/ResetAll.svelte';
import Shortcuts from './custom/Shortcuts.svelte';
import UpdateStatus from './custom/UpdateStatus.svelte';

/** What every custom control receives. */
export interface CustomProps {
  page: Page;
  card: Card;
  option: Opt;
  /** Current values of the card's namespace. */
  values: Values;
  /** Saves one key in the card's namespace (applied live). */
  set: (key: string, value: unknown) => void;
  /** Opens another settings page, optionally at an option. */
  goto: (page: string, option?: string) => void;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any -- some take no props
const BUILT_IN: Record<string, Component<any>> = {
  'general.updateStatus': UpdateStatus,
  'general.exportBackup': Backup,
  'general.importBackup': Backup,
  'general.resetAll': ResetAll,
  'panel.tabs': PanelTabs,
  'features.grid': FeaturesGrid,
  'shortcuts.all': Shortcuts,
  'permissions.list': Permissions,
  'about.info': About,
};

// Hook for feature owners: `ui/src/pages/<PageId>Page.svelte` (page id with a capital first
// letter, e.g. IslandPage.svelte) draws every custom option on that page that has no built-in
// component. It gets CustomProps and can switch on `option.id`.
const PAGES = import.meta.glob<Component<CustomProps>>('../pages/*Page.svelte', { eager: true, import: 'default' });

export function customFor(page: Page, option: Opt): Component<CustomProps> | undefined {
  const name = page.id.charAt(0).toUpperCase() + page.id.slice(1);
  return BUILT_IN[`${page.id}.${option.id}`] ?? PAGES[`../pages/${name}Page.svelte`];
}
