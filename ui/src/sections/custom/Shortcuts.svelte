<script lang="ts">
  import KeyCapture from '../../lib/KeyCapture.svelte';
  import { fadeSlide } from '../../lib/motion';
  import { store } from '../../lib/settings.svelte';
  import { assign, bindings, clash, type Binding } from '../../lib/shortcuts';
  import type { CustomProps } from '../customs';

  let { goto }: CustomProps = $props();

  const all = $derived(store.settings && store.catalog ? bindings() : []);
  const sections = $derived.by(() => {
    const out: { title: string; page: string | null; items: Binding[] }[] = [];
    for (const b of all) {
      const title = b.page?.label ?? 'Notifi';
      let s = out.find((x) => x.title === title);
      if (!s) out.push((s = { title, page: b.page?.id ?? null, items: [] }));
      s.items.push(b);
    }
    return out;
  });
  let refused = $state<Record<string, string>>({});

  function message(b: Binding): string | null {
    if (refused[b.id]) return refused[b.id];
    if (b.namespace === 'shortcuts' && store.shortcutErrors[b.key]) return 'Another app already holds this shortcut.';
    return b.active ? clash(b.id, b.value, all) : null;
  }

  async function set(b: Binding, accel: string) {
    refused = { ...refused, [b.id]: '' };
    try {
      await assign(b, accel);
    } catch (err) {
      refused = { ...refused, [b.id]: String(err) };
    }
  }
</script>

{#each sections as section (section.title)}
  <h3 class="group-title">
    {section.title}
    {#if section.page}<button class="link" onclick={() => goto(section.page!)}>Open page</button>{/if}
  </h3>
  <div class="card">
    {#each section.items as b (b.id)}
      {@const note = message(b)}
      <div class="row" class:inactive={!b.active}>
        <span class="name">
          {b.label}
          {#if note}<small class="warn" transition:fadeSlide={{ token: 'toastIn', y: -4 }}>{note}</small>
          {:else if !b.active}<small>Feature is off</small>{/if}
        </span>
        <span class="controls">
          <KeyCapture value={b.value} label={b.label} onchange={(accel) => set(b, accel)} />
          <button class="ghost" title="Remove" disabled={!b.value} onclick={() => set(b, '')}>Clear</button>
          <button class="ghost" title="Back to {b.default || 'none'}" disabled={b.value === b.default} onclick={() => set(b, b.default)}>Reset</button>
        </span>
      </div>
    {/each}
  </div>
{/each}
<p class="foot">Click a shortcut, then press the new keys. Backspace clears it, Esc cancels. Shortcuts of features that are off don't clash.</p>

<style>
  .controls {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .warn {
    color: var(--warn) !important;
  }
  .inactive .name {
    opacity: 0.6;
  }
  .link {
    margin-left: 8px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--acc);
    font: inherit;
    text-transform: none;
    letter-spacing: 0;
    cursor: pointer;
  }
  .foot {
    margin: 12px 4px 0;
    color: var(--mut);
    font-size: 12.5px;
  }
</style>
