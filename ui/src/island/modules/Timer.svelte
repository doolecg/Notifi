<script lang="ts">
  import Glyph from '../Glyph.svelte';
  import { island, timer } from '../state.svelte';
  import { clock } from '../types';

  const PRESETS = [1, 5, 10, 25, 45, 60];
  const t = $derived(island.snap!.timer);
  // Count down locally between snapshots.
  const left = $derived(t.phase === 'running' ? Math.max(0, t.endsAt - island.now) : t.leftMs);
  const progress = $derived(t.totalMs > 0 ? left / t.totalMs : 0);
  const active = $derived(t.phase === 'running' || t.phase === 'paused');
</script>

<div class="timer">
  <div class="face">
    <svg viewBox="0 0 100 100" aria-hidden="true">
      <circle cx="50" cy="50" r="44" class="track" />
      <circle cx="50" cy="50" r="44" class="fill" style:stroke-dashoffset={276.5 * (1 - progress)} />
    </svg>
    <div class="read">
      <span class="left">{active ? clock(Math.ceil(left / 1000) * 1000) : t.phase === 'done' ? 'Done' : '0:00'}</span>
      {#if t.phase === 'paused'}<span class="sub">Paused</span>{/if}
    </div>
  </div>
  <div class="side">
    {#if active}
      <div class="btns">
        {#if t.phase === 'running'}
          <button class="main" onclick={() => timer('pause')}><Glyph name="pause" size={16} />Pause</button>
        {:else}
          <button class="main" onclick={() => timer('resume')}><Glyph name="play" size={16} filled />Resume</button>
        {/if}
        <button onclick={() => timer('add', 60_000)}>+1 min</button>
        <button onclick={() => timer('add', 300_000)}>+5 min</button>
        <button onclick={() => timer('cancel')}><Glyph name="close" size={14} />Cancel</button>
      </div>
    {:else}
      <div class="presets">
        {#each PRESETS as m (m)}
          <button onclick={() => timer('start', m * 60_000)}>{m < 60 ? `${m} min` : '1 hour'}</button>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .timer {
    display: flex;
    align-items: center;
    gap: 18px;
    height: 100%;
  }
  .face {
    position: relative;
    flex: none;
    width: 132px;
    height: 132px;
  }
  svg {
    width: 100%;
    height: 100%;
    transform: rotate(-90deg);
  }
  circle {
    fill: none;
    stroke-width: 6;
  }
  .track {
    stroke: var(--i-raised);
  }
  .fill {
    stroke: var(--i-timer);
    stroke-linecap: round;
    stroke-dasharray: 276.5;
  }
  .read {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
  }
  .left {
    font-size: 26px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .sub {
    font-size: 11px;
    color: var(--i-mut);
  }
  .side {
    flex: 1;
  }
  .presets,
  .btns {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 6px;
  }
  button {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 9px 6px;
    border: 0;
    border-radius: 10px;
    background: var(--i-raised);
    color: var(--i-fg);
    font: inherit;
    cursor: pointer;
  }
  button:hover {
    background: var(--i-hover);
  }
  .main {
    background: var(--i-fg);
    color: #000;
  }
</style>
