<script lang="ts">
  import { island } from '../state.svelte';
  import Glyph from '../Glyph.svelte';

  const idle = $derived(island.snap?.idle);

  type Meter = { key: string; label: string; value: number | null };
  const meters = $derived<Meter[]>([
    { key: 'cpu', label: 'Processor', value: idle?.cpu ?? null },
    { key: 'mem', label: 'Memory', value: idle?.mem ?? null },
    { key: 'battery', label: 'Battery', value: idle?.battery ?? null },
    { key: 'ai', label: 'Claude 5h', value: idle?.ai ?? null },
  ]);
  const hot = (key: string, v: number) => (key === 'battery' ? v <= 20 : v >= 85);
</script>

<div class="sys">
  <header><h2>System</h2></header>
  <div class="list">
    {#each meters as m (m.key)}
      <div class="row" class:off={m.value === null}>
        <span class="name">
          {#if m.key === 'battery' && idle?.charging}<Glyph name="bolt" size={13} />{/if}{m.label}
        </span>
        <span class="bar"><i class:hot={m.value !== null && hot(m.key, m.value)} style:width={`${m.value ?? 0}%`}></i></span>
        <span class="val">{m.value === null ? 'Not reported' : `${m.value}%`}</span>
      </div>
    {/each}
  </div>
  <p class="hint">Readings follow the resting-pill options in Settings → Island.</p>
</div>

<style>
  .sys {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: 10px;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--i-mut);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 4px;
  }
  .row {
    display: grid;
    grid-template-columns: 96px 1fr 84px;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }
  .row.off {
    opacity: 0.5;
  }
  .name {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-weight: 600;
  }
  .bar {
    height: 8px;
    overflow: hidden;
    border-radius: 99px;
    background: color-mix(in srgb, var(--i-fg), transparent 88%);
  }
  .bar i {
    display: block;
    height: 100%;
    border-radius: 99px;
    background: var(--i-acc);
    transition: width var(--m-glassIn-dur) var(--m-glassIn-ease), background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .bar i.hot {
    background: #ff6b5e;
  }
  .val {
    text-align: right;
    color: var(--i-mut);
    font-variant-numeric: tabular-nums;
  }
  .hint {
    margin: auto 0 0;
    font-size: 11.5px;
    color: var(--i-mut);
  }
</style>
