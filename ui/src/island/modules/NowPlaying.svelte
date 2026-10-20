<script lang="ts">
  import { untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import Glyph from '../Glyph.svelte';
  import { island, media } from '../state.svelte';
  import { clock, type Lyrics } from '../types';

  let { onSetDefault }: { onSetDefault: (id: string) => void } = $props();

  const snap = $derived(island.snap!);
  const m = $derived(snap.media);
  const pos = $derived(island.position);
  let menu = $state(false);
  let dragging = $state<number | null>(null);

  const shown = $derived(dragging ?? pos);
  const dur = $derived(m?.durationMs ?? 0);

  let lyrics = $state<Lyrics | null>(null);
  let showing = $state(false);
  let box = $state<HTMLElement>();
  const track = $derived(m && snap.config.showLyrics ? `${m.appId}${m.title}${m.artist}` : '');

  $effect(() => {
    const key = track;
    lyrics = null;
    showing = false;
    const cur = untrack(() => m);
    if (!key || !cur) return;
    const { title, artist, album, durationMs } = cur;
    invoke<Lyrics | null>('island_lyrics', { title, artist, album, durationMs })
      .then((l) => { if (key === track) lyrics = l; })
      .catch(() => {});
  });

  const current = $derived.by(() => {
    if (!lyrics?.synced) return -1;
    let at = -1;
    lyrics.lines.forEach((l, i) => { if (l.ms !== null && l.ms <= pos + 300) at = i; });
    return at;
  });

  $effect(() => {
    if (showing && current >= 0) box?.children[current]?.scrollIntoView({ block: 'center', behavior: 'smooth' });
  });

  function seek(e: Event) {
    dragging = Number((e.target as HTMLInputElement).value);
  }
  function commit() {
    if (dragging !== null) media('seek', { value: dragging });
    dragging = null;
  }
  function setVolume(e: Event) {
    invoke('island_set_volume', { volume: Number((e.target as HTMLInputElement).value) / 100 }).catch(() => {});
  }
</script>

<div class="np">
  <header>
    <h2>Now Playing</h2>
    <button class="icon" aria-label="More" onclick={() => (menu = !menu)}><Glyph name="more" /></button>
    {#if menu}
      <div class="menu" role="menu">
        <button role="menuitem" onclick={() => { onSetDefault('nowPlaying'); menu = false; }}>Open this module by default</button>
        {#each snap.players as p (p.id)}
          <button role="menuitem" class:sel={m?.appId === p.id} onclick={() => { media('player', { player: p.id }); menu = false; }}>
            Play from {p.name}
          </button>
        {/each}
      </div>
    {/if}
  </header>

  {#if !snap.mediaSupported}
    <p class="empty">Media controls aren't available on this system.</p>
  {:else if !m}
    <p class="empty">Nothing playing</p>
  {:else}
    {#if showing && lyrics}
      <button class="lyr back" onclick={() => (showing = false)}>Back to player</button>
      <div class="lyrics" bind:this={box}>
        {#each lyrics.lines as l, i}
          <p class:now={i === current} class:past={i < current}>{l.text || '♪'}</p>
        {/each}
      </div>
    {/if}
    <div class="body" hidden={showing && !!lyrics}>
      <div class="art">
        {#if island.cover}
          <img class="glow" src={island.cover} alt="" />
          <img class="cover" src={island.cover} alt="" />
        {:else}
          <div class="cover none"><Glyph name="music" size={36} /></div>
        {/if}
      </div>
      <div class="info">
        <div class="title" title={m.title}>{m.title || 'Unknown title'}</div>
        <div class="artist" title={m.artist}>{m.artist}</div>
        {#if m.nextTitle}<div class="next" title={m.nextTitle}>Up next: {m.nextTitle}</div>{/if}
        <div class="source">
          {#if snap.players.length > 1}
            <select aria-label="Player" value={m.appId} onchange={(e) => media('player', { player: (e.target as HTMLSelectElement).value })}>
              {#each snap.players as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
            </select>
          {:else}
            <span>{m.app}</span>
          {/if}
          {#if lyrics}
            <button class="lyr" class:on={showing} onclick={() => (showing = !showing)}>Lyrics</button>
          {/if}
          {#if snap.config.equaliser}
            <span class="eq" class:still={!m.playing} aria-hidden="true"><i></i><i></i><i></i></span>
          {/if}
        </div>

        {#if dur > 0}
          <div class="seek">
            <input type="range" min="0" max={dur} step="1000" value={shown} disabled={!m.canSeek}
              oninput={seek} onchange={commit} aria-label="Position" />
            <div class="times"><span>{clock(shown)}</span><span>-{clock(dur - shown)}</span></div>
          </div>
        {/if}

        <div class="transport">
          <span></span>
          <span class="tcenter">
          <button class="icon" aria-label="Previous" disabled={!m.canPrev} onclick={() => media('previous')}><Glyph name="prev" filled /></button>
          <button class="play" aria-label={m.playing ? 'Pause' : 'Play'} disabled={!m.canPlayPause} onclick={() => media('toggle')}>
            <Glyph name={m.playing ? 'pause' : 'play'} size={22} filled={!m.playing} />
          </button>
          <button class="icon" aria-label="Next" disabled={!m.canNext} onclick={() => media('next')}><Glyph name="next" filled /></button>
          </span>
          {#if snap.volume !== null}
            <span class="vol">
              <Glyph name="speaker" size={14} />
              <input type="range" min="0" max="100" value={Math.round(snap.volume * 100)} oninput={setVolume} aria-label="Volume" />
            </span>
          {:else}<span></span>{/if}
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .np {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: 8px;
  }
  header {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--i-mut);
  }
  .menu {
    position: absolute;
    top: 30px;
    right: 0;
    z-index: 5;
    display: flex;
    flex-direction: column;
    min-width: 200px;
    padding: 4px;
    border-radius: 10px;
    background: var(--i-raised);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.5);
  }
  .menu button {
    padding: 7px 10px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--i-fg);
    font: inherit;
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
  }
  .menu button:hover,
  .menu .sel {
    background: var(--i-hover);
  }
  .empty {
    margin: auto;
    color: var(--i-mut);
  }
  .body {
    display: flex;
    gap: 16px;
    min-height: 0;
    flex: 1;
  }
  .art {
    position: relative;
    flex: none;
    width: 132px;
    height: 132px;
  }
  .cover {
    position: relative;
    width: 100%;
    height: 100%;
    border-radius: 14px;
    object-fit: cover;
  }
  .none {
    display: grid;
    place-items: center;
    background: var(--i-raised);
    color: var(--i-mut);
  }
  .glow {
    position: absolute;
    inset: 6px;
    width: calc(100% - 12px);
    height: calc(100% - 12px);
    filter: blur(22px) saturate(1.6);
    opacity: 0.75;
    transform: scale(1.15);
  }
  .info {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .title {
    overflow: hidden;
    font-size: 18px;
    font-weight: 600;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .artist {
    overflow: hidden;
    font-size: 14px;
    color: var(--i-mut);
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .source {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 6px;
    font-size: 11.5px;
    color: var(--i-mut);
  }
  .next {
    overflow: hidden;
    font-size: 11.5px;
    color: var(--i-mut);
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .lyr {
    padding: 1px 8px;
    border: 1px solid var(--i-line);
    border-radius: 6px;
    background: none;
    color: var(--i-fg);
    font: inherit;
    cursor: pointer;
  }
  .lyr.on {
    background: var(--i-hover);
  }
  .back {
    align-self: center;
  }
  .lyrics {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 24px 0;
    text-align: center;
    scrollbar-width: none;
  }
  .lyrics p {
    margin: 0 0 8px;
    font-size: 15px;
    font-weight: 600;
    color: var(--i-mut);
    opacity: 0.55;
    transition: opacity 0.2s, color 0.2s;
  }
  .lyrics p.now {
    color: var(--i-fg);
    opacity: 1;
  }
  .body[hidden] {
    display: none;
  }
  select {
    max-width: 140px;
    padding: 1px 4px;
    border: 1px solid var(--i-line);
    border-radius: 6px;
    background: var(--i-raised);
    color: var(--i-fg);
    font: inherit;
  }
  .seek {
    margin-top: auto;
  }
  .seek input {
    display: block;
    width: 100%;
    margin: 6px 0 2px;
  }
  .times {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--i-mut);
    font-variant-numeric: tabular-nums;
  }
  .transport {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    margin-top: 6px;
  }
  .tcenter {
    display: flex;
    align-items: center;
    gap: 18px;
  }
  .vol {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    color: var(--i-mut);
  }
  .vol input {
    width: 76px;
    margin: 0;
  }
  .play {
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    border: 0;
    border-radius: 50%;
    background: var(--i-fg);
    color: #000;
    cursor: pointer;
  }
</style>
