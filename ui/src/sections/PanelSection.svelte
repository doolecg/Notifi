<script lang="ts">
  import Switch from '../lib/Switch.svelte';
  import { store, type PanelTab } from '../lib/settings.svelte';

  const panel = $derived(store.settings!.panel);
  const tabs = $derived(store.catalog?.panelTabs ?? []);

  function featureLabel(path: string) {
    const [group, id] = path.split('.');
    return store.catalog?.groups.find((g) => g.id === group)?.features.find((f) => f.id === id)?.label ?? path;
  }

  function note(tab: PanelTab) {
    if (!tab.ready) return 'Arrives in a later version.';
    if (tab.feature && !store.featureOn(tab.feature)) return `Shows once ${featureLabel(tab.feature)} is on.`;
    if (tab.id === 'power') return 'Shown on computers with a battery.';
    return '';
  }

  function toggle(id: string, on: boolean) {
    store.set('panel', { ...panel, tabs: { ...panel.tabs, [id]: on } });
  }
</script>

<p class="group-title">Tabs</p>
<div class="card">
  {#each tabs as tab (tab.id)}
    <div class="row">
      <span class="name">{tab.label}{#if note(tab)}<small>{note(tab)}</small>{/if}</span>
      {#if tab.ready}
        <Switch checked={panel.tabs[tab.id] ?? true} label={tab.label} onchange={(on) => toggle(tab.id, on)} />
      {/if}
    </div>
  {/each}
</div>
