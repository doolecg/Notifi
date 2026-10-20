<script lang="ts">
  // Custom options of the Extra dimming page (spec/energy/extra-dimming.md): `perDisplay`, the monitor list.
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import Switch from '../lib/Switch.svelte';
  import type { CustomProps } from '../sections/customs';

  interface Row {
    id: string;
    name: string;
    internal: boolean;
    ddc: boolean;
    primary: boolean;
    level: number | null;
    extraSupported: boolean;
    extra: boolean;
    off: boolean;
  }

  let { option }: CustomProps = $props();
  let rows = $state<Row[] | null>(null);
  let error = $state('');

  async function load() {
    try {
      rows = await invoke<Row[]>('energy_displays');
    } catch (e) {
      error = String(e);
    }
  }

  async function pick(row: Row, on: boolean) {
    error = '';
    try {
      await invoke('energy_set_extra_display', { id: row.id, on });
      row.extra = on;
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  async function move(row: Row, level: number) {
    error = '';
    try {
      row.level = await invoke<number>('energy_set_level', { id: row.id, level });
    } catch (e) {
      error = String(e);
    }
  }

  const kind = (r: Row) => (r.internal ? 'Built-in display' : r.ddc ? 'External monitor' : 'Monitor');

  onMount(load);
</script>

{#if option.id === 'perDisplay'}
  <div class="intro"><span class="name">{option.label}</span><small>{error || option.hint}</small></div>
  {#if rows === null}
    <p class="empty">Looking for displays...</p>
  {:else if rows.length === 0}
    <p class="empty">No displays found.</p>
  {:else}
    {#each rows as row (row.id)}
      <div class="row">
        <span class="name">
          {row.name}{row.primary ? ' (main)' : ''}
          <small>{row.extraSupported ? kind(row) : `${kind(row)}: brightness cannot be read, so extra dimming is not available`}</small>
        </span>
        {#if row.level !== null}
          <span class="slider">
            <input
              type="range"
              min="1"
              max="100"
              step="1"
              aria-label="Brightness of {row.name}"
              value={row.level}
              onchange={(e) => move(row, Number(e.currentTarget.value))}
            />
            <output>{Math.round(row.level)}%</output>
          </span>
        {/if}
        {#if row.extraSupported}
          <Switch checked={row.extra} label="Extra dimming on {row.name}" onchange={(on) => pick(row, on)} />
        {/if}
      </div>
    {/each}
  {/if}
{/if}

<style>
  .intro,
  .row {
    display: flex;
    gap: 12px;
    padding: 10px 14px;
    font-size: 13px;
  }
  .intro {
    flex-direction: column;
    gap: 2px;
    padding-bottom: 0;
  }
  .row {
    align-items: center;
    justify-content: space-between;
  }
  .name {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  small,
  .empty {
    color: var(--mut);
    font-size: 11.5px;
  }
  .empty {
    margin: 0;
    padding: 10px 14px;
  }
  .slider {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .slider input {
    width: 140px;
    accent-color: var(--acc);
  }
  output {
    min-width: 40px;
    color: var(--mut);
    font-size: 12.5px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
