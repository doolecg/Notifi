import { normalize } from './accel';
import { namespaceOf, store, type Page } from './settings.svelte';

export interface Binding {
  /** `shortcuts.<action>` for app actions, `<namespace>.<key>` for feature options. */
  id: string;
  label: string;
  page: Page | null;
  namespace: string;
  key: string;
  value: string;
  default: string;
  /** False when the option's feature (or sub-feature) is switched off. */
  active: boolean;
}

/** Every shortcut the app has: global actions plus each feature's shortcut options. */
export function bindings(): Binding[] {
  const s = store.settings;
  const c = store.catalog;
  if (!s || !c) return [];
  const out: Binding[] = c.actions.map((a) => ({
    id: `shortcuts.${a.id}`,
    label: a.label,
    page: null,
    namespace: 'shortcuts',
    key: a.id,
    value: s.shortcuts[a.id] ?? '',
    default: a.defaultAccel,
    active: true,
  }));
  for (const page of store.pages()) {
    for (const card of page.cards) {
      const ns = namespaceOf(page, card);
      const active = (!page.feature || store.featureOn(page.id)) && (!card.feature || store.featureOn(card.feature));
      for (const o of card.options) {
        if (o.control.kind !== 'shortcut') continue;
        out.push({
          id: `${ns}.${o.id}`,
          label: o.label,
          page,
          namespace: ns,
          key: o.id,
          value: String(store.values(ns)[o.id] ?? ''),
          default: o.control.default,
          active,
        });
      }
    }
  }
  return out;
}

/** Why `accel` is a problem for binding `id`, or null. Only shortcuts that are switched on clash. */
export function clash(id: string, accel: string, all = bindings()): string | null {
  if (!accel) return null;
  const key = normalize(accel);
  const other = all.find((b) => b.id !== id && b.active && b.value && normalize(b.value) === key);
  if (other) return `Also used by ${other.label}${other.page ? ` (${other.page.label})` : ''}.`;
  const os = store.catalog?.reserved.find((r) => normalize(r.accel) === key);
  if (os) return `Your system uses this for ${os.label}.`;
  return null;
}

export function assign(b: Binding, accel: string) {
  return b.namespace === 'shortcuts' ? store.set('shortcuts', { [b.key]: accel }) : store.setOption(b.namespace, b.key, accel);
}
