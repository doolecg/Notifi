<script lang="ts">
  import { fadeSlide } from '../../lib/motion';
  import Glyph from '../Glyph.svelte';
  import { island } from '../state.svelte';

  const ago = (ms: number) => {
    const s = Math.max(0, Math.round((island.now - ms) / 1000));
    if (s < 60) return 'just now';
    if (s < 3600) return `${Math.floor(s / 60)} min ago`;
    return `${Math.floor(s / 3600)} hr ago`;
  };
</script>

<div class="nt">
  <header>
    <h2>Notifications</h2>
    {#if island.history.length}<button class="clear" onclick={() => island.clearHistory()}>Clear</button>{/if}
  </header>
  {#if !island.history.length}
    <p class="empty">Nothing yet. Track changes, finished timers and finished agent tasks collect here.</p>
  {:else}
    <div class="list">
      {#each island.history as n (n.id)}
        <div class="item" in:fadeSlide={{ token: 'tabSwitch' }}>
          <span class="ic" style:color={n.color}><Glyph name={n.icon} size={15} /></span>
          <span class="tx">{n.text}</span>
          <span class="when">{ago(n.at)}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .nt {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    gap: 8px;
  }
  header {
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
  .clear {
    border: 0;
    background: none;
    color: var(--i-mut);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .clear:hover {
    color: var(--i-fg);
  }
  .empty {
    margin: auto;
    max-width: 300px;
    text-align: center;
    color: var(--i-mut);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    scrollbar-width: none;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .item {
    display: grid;
    grid-template-columns: 24px 1fr auto;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--i-fg), transparent 94%);
    font-size: 13px;
  }
  .ic {
    display: grid;
    place-items: center;
  }
  .tx {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    overflow: hidden;
    line-height: 1.25;
  }
  .when {
    color: var(--i-mut);
    font-size: 11.5px;
  }
</style>
