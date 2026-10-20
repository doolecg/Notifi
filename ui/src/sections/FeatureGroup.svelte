<script lang="ts">
  import Switch from '../lib/Switch.svelte';
  import { fadeSlide, motion } from '../lib/motion';
  import { store, type FeatureGroup } from '../lib/settings.svelte';

  let { group }: { group: FeatureGroup } = $props();
  const values = $derived(store.settings?.features[group.id] ?? {});

  function toggle(id: string, on: boolean) {
    store.set(group.id, { ...values, [id]: on });
  }
</script>

<div class="card">
  {#each group.features as feature, i (feature.id)}
    <div class="row" in:fadeSlide={{ token: 'listReorder', delay: i * (motion.listReorder.stagger ?? 0) }}>
      <span class="name">{feature.label}</span>
      <Switch checked={values[feature.id] ?? false} label={feature.label} onchange={(on) => toggle(feature.id, on)} />
    </div>
  {/each}
</div>
