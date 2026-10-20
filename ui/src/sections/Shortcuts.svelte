<script lang="ts">
  import KeyCapture from '../lib/KeyCapture.svelte';
  import { normalize } from '../lib/accel';
  import { fadeSlide } from '../lib/motion';
  import { store } from '../lib/settings.svelte';

  const shortcuts = $derived(store.settings!.shortcuts);
  const actions = $derived(store.catalog!.actions);
  let refused = $state<Record<string, string>>({});

  function clash(id: string, accel: string): string | null {
    if (!accel) return null;
    const key = normalize(accel);
    const other = actions.find((a) => a.id !== id && shortcuts[a.id] && normalize(shortcuts[a.id]) === key);
    if (other) return `Already used by ${other.label}.`;
    const os = store.catalog!.reserved.find((r) => normalize(r.accel) === key);
    if (os) return `Your system uses this for ${os.label}.`;
    return null;
  }

  function message(id: string): string | null {
    if (refused[id]) return refused[id];
    if (store.shortcutErrors[id]) return 'Another app already holds this shortcut.';
    return clash(id, shortcuts[id]);
  }

  async function assign(id: string, accel: string) {
    const taken = actions.find((a) => a.id !== id && shortcuts[a.id] && normalize(shortcuts[a.id]) === normalize(accel));
    if (taken) {
      refused = { ...refused, [id]: `${accel} is already used by ${taken.label}.` };
      return;
    }
    refused = { ...refused, [id]: '' };
    try {
      await store.set('shortcuts', { ...shortcuts, [id]: accel });
    } catch (err) {
      refused = { ...refused, [id]: String(err) };
    }
  }
</script>

<div class="card">
  {#each actions as action (action.id)}
    {@const note = message(action.id)}
    <div class="row">
      <span class="name">
        {action.label}
        {#if note}<small class="warn" transition:fadeSlide={{ token: 'toastIn', y: -4 }}>{note}</small>{/if}
      </span>
      <div class="controls">
        <KeyCapture value={shortcuts[action.id] ?? ''} label={action.label} onchange={(accel) => assign(action.id, accel)} />
        <button
          class="ghost"
          title="Back to {action.defaultAccel}"
          disabled={shortcuts[action.id] === action.defaultAccel}
          onclick={() => assign(action.id, action.defaultAccel)}>Reset</button
        >
      </div>
    </div>
  {/each}
</div>
<p class="foot">Click a shortcut, then press the new keys. Esc cancels.</p>

<style>
  .controls {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .warn {
    color: var(--warn) !important;
  }
  .foot {
    margin: 12px 4px 0;
    color: var(--mut);
    font-size: 12.5px;
  }
</style>
