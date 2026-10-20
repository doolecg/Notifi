<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';

  interface Slice {
    label: string;
    icon: string;
    kind: string;
    arg: string;
    children: Slice[];
  }
  interface Wheel {
    slices: Slice[];
  }

  const KINDS = [
    ['', 'Empty'],
    ['action', 'Notifi action'],
    ['open', 'Open file, folder, app or script'],
    ['url', 'Open website'],
    ['submenu', 'Submenu'],
  ];
  const ICONS = ['', 'settings', 'clipboard', 'utilities', 'controls', 'notes', 'sound', 'speaker', 'window', 'globe', 'apps', 'bolt', 'clock', 'leaf', 'power', 'plus'];
  const MAX_SLICES = 12;
  const MAX_DEPTH = 3;

  let wheel = $state<Wheel>({ slices: [] });
  let actions = $state<[string, string][]>([]);
  /** Indexes of the submenus being edited, from the top. */
  let path = $state<number[]>([]);
  let note = $state('');
  let failed = $state(false);

  const ring = $derived.by(() => {
    let slices = wheel.slices;
    for (const i of path) slices = slices[i]?.children ?? [];
    return slices;
  });
  const trail = $derived.by(() => {
    const names = ['Wheel'];
    let slices = wheel.slices;
    for (const i of path) {
      names.push(slices[i]?.label || 'Submenu');
      slices = slices[i]?.children ?? [];
    }
    return names;
  });

  onMount(async () => {
    wheel = await invoke<Wheel>('radial_wheel');
    actions = await invoke<[string, string][]>('radial_actions');
  });

  async function save() {
    try {
      wheel = await invoke<Wheel>('radial_wheel_save', { wheel });
      [note, failed] = ['Saved', false];
    } catch (err) {
      [note, failed] = [String(err), true];
    }
  }

  function blank(): Slice {
    return { label: '', icon: '', kind: '', arg: '', children: [] };
  }

  function add() {
    if (ring.length >= MAX_SLICES) return;
    ring.push({ ...blank(), kind: 'action', arg: actions[0]?.[0] ?? '', label: actions[0]?.[1] ?? '' });
    save();
  }

  function remove(i: number) {
    ring.splice(i, 1);
    save();
  }

  function move(i: number, by: number) {
    const j = i + by;
    if (j < 0 || j >= ring.length) return;
    [ring[i], ring[j]] = [ring[j], ring[i]];
    save();
  }

  function kindChanged(slice: Slice) {
    if (slice.kind === 'action') slice.arg = actions.some(([id]) => id === slice.arg) ? slice.arg : (actions[0]?.[0] ?? '');
    else if (slice.kind === 'submenu' || slice.kind === '') slice.arg = '';
    if (slice.kind === 'action' && !slice.label) slice.label = actions.find(([id]) => id === slice.arg)?.[1] ?? '';
    save();
  }

  async function restore() {
    path = [];
    wheel = await invoke<Wheel>('radial_wheel_reset');
    [note, failed] = ['Restored the starter wheel', false];
  }
</script>

<div class="editor">
  <div class="head">
    <span class="crumbs">
      {#each trail as name, depth (depth)}
        <button class="crumb" disabled={depth === path.length} onclick={() => (path = path.slice(0, depth))}>{name}</button>
        {#if depth < trail.length - 1}<span class="sep">/</span>{/if}
      {/each}
    </span>
    <small class:warn={failed}>{note || `${ring.length} of ${MAX_SLICES} slices. Only as many as "Slices per ring" are shown.`}</small>
  </div>

  {#each ring as slice, i (i)}
    <div class="slice">
      <input class="text label" placeholder="Name" bind:value={slice.label} onchange={save} aria-label="Slice name" />
      <select bind:value={slice.icon} onchange={save} aria-label="Icon">
        {#each ICONS as icon (icon)}<option value={icon}>{icon || 'Initial'}</option>{/each}
      </select>
      <select bind:value={slice.kind} onchange={() => kindChanged(slice)} aria-label="What it does">
        {#each KINDS as [value, name] (value)}<option {value}>{name}</option>{/each}
      </select>
      {#if slice.kind === 'action'}
        <select bind:value={slice.arg} onchange={save} aria-label="Action">
          {#each actions as [id, name] (id)}<option value={id}>{name}</option>{/each}
        </select>
      {:else if slice.kind === 'open' || slice.kind === 'url'}
        <input class="text" placeholder={slice.kind === 'url' ? 'https://example.com' : 'C:\\Path\\to\\it'} bind:value={slice.arg} onchange={save} aria-label="Target" />
      {:else if slice.kind === 'submenu'}
        <button class="ghost" disabled={path.length + 1 >= MAX_DEPTH} onclick={() => (path = [...path, i])}>Edit slices</button>
      {:else}
        <span></span>
      {/if}
      <span class="tools">
        <button class="ghost" disabled={i === 0} onclick={() => move(i, -1)} aria-label="Move earlier">Up</button>
        <button class="ghost" disabled={i === ring.length - 1} onclick={() => move(i, 1)} aria-label="Move later">Down</button>
        <button class="ghost" onclick={() => remove(i)} aria-label="Delete slice">Delete</button>
      </span>
    </div>
  {/each}

  <div class="foot">
    <button class="ghost" disabled={ring.length >= MAX_SLICES} onclick={add}>Add slice</button>
    <button class="ghost" onclick={restore}>Restore starter wheel</button>
  </div>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 14px;
  }
  .head {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .crumb {
    padding: 0;
    border: 0;
    background: none;
    color: var(--acc);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .crumb:disabled {
    color: var(--fg);
    cursor: default;
  }
  .sep,
  small {
    color: var(--mut);
    font-size: 11.5px;
  }
  .warn {
    color: var(--warn);
  }
  .slice {
    display: grid;
    grid-template-columns: 1.3fr 0.9fr 1.8fr;
    gap: 6px;
    align-items: center;
  }
  .slice > :nth-child(4) {
    grid-column: 1 / span 2;
  }
  .slice .tools {
    justify-self: end;
  }
  .text,
  select {
    min-width: 0;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--inset);
    color: var(--fg);
    font: inherit;
    font-size: 12.5px;
  }
  .tools,
  .foot {
    display: flex;
    gap: 6px;
  }
  button.ghost:disabled {
    opacity: 0.45;
    cursor: default;
  }
  input:focus-visible,
  select:focus-visible,
  button:focus-visible {
    outline: 2px solid var(--acc);
    outline-offset: 2px;
  }
</style>
