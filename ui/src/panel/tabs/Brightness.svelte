<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import Icon from '../Icon.svelte';

  interface Row {
    id: string;
    name: string;
    primary: boolean;
    level: number | null;
  }

  let rows = $state<Row[]>([]);
  let error = $state('');
  const timers = new Map<string, ReturnType<typeof setTimeout>>();

  async function load() {
    try {
      rows = (await invoke<Row[]>('energy_displays')).filter((r) => r.level !== null);
    } catch (e) {
      error = String(e);
    }
  }

  async function apply(row: Row, level: number) {
    error = '';
    try {
      row.level = await invoke<number>('energy_set_level', { id: row.id, level });
    } catch (e) {
      error = String(e);
    }
  }

  // Monitors answer slowly, so a drag sends at most one change every 120 ms and always ends on the final value.
  function drag(row: Row, level: number) {
    row.level = level;
    if (timers.has(row.id)) return;
    timers.set(
      row.id,
      setTimeout(() => {
        timers.delete(row.id);
        apply(row, row.level ?? level);
      }, 120),
    );
  }

  onMount(() => {
    load();
    return () => timers.forEach(clearTimeout);
  });
</script>

{#if rows.length}
  <div class="p-head"><span>Brightness</span></div>
  <div class="p-card faders">
    {#each rows as row (row.id)}
      {@const v = Math.round(row.level ?? 0)}
      <div class="p-row">
        <span class="p-glyph"><Icon name="displays" size={15} /></span>
        <div class="p-main">
          <span class="p-line"><span class="p-label">{row.name}</span><span class="p-value">{v}%</span></span>
          <input
            type="range"
            min="1"
            max="100"
            value={v}
            aria-label="Brightness of {row.name}"
            oninput={(e) => drag(row, Number(e.currentTarget.value))}
            style:--fill="{v}%"
          />
        </div>
      </div>
    {/each}
    {#if error}<p class="p-sub error">{error}</p>{/if}
  </div>
{/if}

<style>
  input[type='range'] {
    width: 100%;
    height: 18px;
    margin: 0;
    appearance: none;
    background: none;
    cursor: pointer;
  }
  input[type='range']::-webkit-slider-runnable-track {
    height: 5px;
    border-radius: 99px;
    background: linear-gradient(to right, var(--acc) var(--fill), var(--track) var(--fill));
  }
  input[type='range']::-webkit-slider-thumb {
    appearance: none;
    width: 14px;
    height: 14px;
    margin-top: -4.5px;
    border: 1px solid var(--line-strong);
    border-radius: 50%;
    background: var(--knob);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.25);
  }
  input:focus-visible {
    outline: 2px solid var(--acc);
    outline-offset: 1px;
  }
  .error {
    margin: 4px 0 0;
    color: var(--warn);
  }
</style>
