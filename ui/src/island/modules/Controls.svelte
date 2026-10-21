<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import Glyph from '../Glyph.svelte';
  import { island, media } from '../state.svelte';
  import { BUTTONS } from '../types';

  let { onButton }: { onButton: (id: string) => void } = $props();

  const snap = $derived(island.snap!);
  const m = $derived(snap.media);

  const pct = (v: number | null) => (v === null ? null : Math.round(v));
  const vol = $derived(pct(snap.volume === null ? null : snap.volume * 100));
  const bright = $derived(pct(snap.brightness));

  const volumeTo = (n: number) => invoke('island_set_volume', { volume: n / 100 }).catch(() => {});
  const brightnessTo = (n: number) => invoke('island_set_brightness', { percent: n }).catch(() => {});
  function setVolume(e: Event) {
    volumeTo(Number((e.target as HTMLInputElement).value));
  }
  function setBrightness(e: Event) {
    brightnessTo(Number((e.target as HTMLInputElement).value));
  }
  /** Scrolling over a slider nudges it by 2 per notch; reads the slider so quick notches add up. */
  function wheel(e: WheelEvent, to: (n: number) => void) {
    const input = (e.currentTarget as HTMLElement).querySelector('input')!;
    if (input.disabled || !e.deltaY) return;
    input.value = String(Math.min(100, Math.max(0, Number(input.value) + (e.deltaY < 0 ? 2 : -2))));
    to(Number(input.value));
  }
</script>

<div class="controls">
  <div class="mini">
    {#if m}
      {#if island.cover}<img src={island.cover} alt="" />{:else}<span class="ph"><Glyph name="music" /></span>{/if}
      <div class="t">
        <div class="title">{m.title || 'Unknown title'}</div>
        <div class="artist">{m.artist || m.app}</div>
      </div>
      <button class="icon" aria-label="Previous" disabled={!m.canPrev} onclick={() => media('previous')}><Glyph name="prev" filled size={16} /></button>
      <button class="icon" aria-label={m.playing ? 'Pause' : 'Play'} onclick={() => media('toggle')}><Glyph name={m.playing ? 'pause' : 'play'} filled={!m.playing} /></button>
      <button class="icon" aria-label="Next" disabled={!m.canNext} onclick={() => media('next')}><Glyph name="next" filled size={16} /></button>
    {:else}
      <span class="ph"><Glyph name="music" /></span>
      <div class="t"><div class="title">Nothing playing</div></div>
    {/if}
  </div>

  <label class="row" onwheel={(e) => wheel(e, volumeTo)}>
    <Glyph name="speaker" size={16} />
    <input type="range" min="0" max="100" value={vol ?? 0} disabled={vol === null} oninput={setVolume} aria-label="Volume" />
    <span class="pct">{vol === null ? 'n/a' : `${vol}%`}</span>
  </label>
  <label class="row" onwheel={(e) => wheel(e, brightnessTo)}>
    <Glyph name="sun" size={16} />
    <input type="range" min="0" max="100" value={bright ?? 0} disabled={bright === null} oninput={setBrightness} aria-label="Brightness" />
    <span class="pct">{bright === null ? 'n/a' : `${bright}%`}</span>
  </label>

  <div class="quick">
    {#each snap.config.controlsButtons as id (id)}
      {@const b = BUTTONS[id]}
      {#if b}
        <button onclick={() => onButton(id)}><Glyph name={b.icon} size={18} /><span>{b.label}</span></button>
      {/if}
    {/each}
  </div>
</div>

<style>
  .controls {
    display: flex;
    flex-direction: column;
    gap: 10px;
    height: 100%;
  }
  .mini {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    border-radius: 12px;
    background: var(--i-raised);
  }
  .mini img,
  .ph {
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: 8px;
    object-fit: cover;
  }
  .ph {
    display: grid;
    place-items: center;
    background: var(--i-hover);
    color: var(--i-mut);
  }
  .t {
    flex: 1;
    min-width: 0;
  }
  .title,
  .artist {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .title {
    font-weight: 600;
  }
  .artist {
    font-size: 11.5px;
    color: var(--i-mut);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--i-mut);
  }
  .row input {
    flex: 1;
  }
  .pct {
    width: 34px;
    text-align: right;
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
  }
  .quick {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
    margin-top: auto;
  }
  .quick button {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    padding: 7px 2px;
    border: 0;
    border-radius: 10px;
    background: var(--i-raised);
    color: var(--i-fg);
    font: inherit;
    font-size: 10.5px;
    cursor: pointer;
  }
  .quick button:hover {
    background: var(--i-hover);
  }
</style>
