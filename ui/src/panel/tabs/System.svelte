<script lang="ts">
  import { store } from '../../lib/settings.svelte';
  import Bar from '../Bar.svelte';
  import Icon from '../Icon.svelte';
  import Spark from '../Spark.svelte';
  import TopApps from '../TopApps.svelte';
  import { bytes, duration, pct } from '../format';
  import { live } from '../live.svelte';

  let coresOpen = $state(false);
  const sys = $derived(live.system);
  const temps = $derived.by(() => {
    if (!sys || !store.featureOn('system.temperatures')) return [];
    const t = sys.temps;
    return [
      { label: 'CPU', value: t.cpu },
      { label: 'GPU', value: t.gpu },
      { label: 'Battery', value: t.battery },
    ].filter((x): x is { label: string; value: number } => x.value != null);
  });
  const memShare = $derived(sys ? sys.memUsed / Math.max(1, sys.memTotal) : 0);
  const pressure = $derived(memShare >= 0.85 ? 'High' : memShare >= 0.7 ? 'Elevated' : 'Normal');
  const heat = (c: number) => (c >= 85 ? 'var(--warn)' : 'var(--fg)');
  const batteryTone = (p: number) => (p <= 15 ? 'var(--warn)' : 'var(--acc)');
</script>

{#if sys}
  {#if temps.length}
    <div class="p-tiles">
      {#each temps as t}
        <div class="p-tile">
          <span class="p-sub">{t.label}</span>
          <span class="p-value" style:color={heat(t.value)}>{Math.round(t.value)}°C</span>
        </div>
      {/each}
    </div>
  {/if}

  <p class="p-head">Hardware usage</p>
  <div class="p-card">
    <button class="p-row p-toggle" aria-expanded={coresOpen} onclick={() => (coresOpen = !coresOpen)}>
      <span class="p-glyph"><Icon name="cpu" size={15} /></span>
      <span class="p-main">
        <span class="p-line"><span class="p-label">CPU</span><span class="p-value">{pct(sys.cpu)}</span></span>
        <Bar value={sys.cpu} />
        <Spark values={live.history.cpu} max={100} height={22} />
      </span>
      <span class="chev" class:open={coresOpen}><Icon name="chevronDown" size={14} /></span>
    </button>
    {#if coresOpen}
      <div class="p-more cores">
        {#each sys.cores as core, i}
          <span class="core" title="Core {i + 1}: {pct(core)}">
            <span class="core-fill" style:height="{Math.max(3, core)}%"></span>
          </span>
        {/each}
      </div>
    {/if}
    {#if sys.battery}
      <div class="p-row">
        <span class="p-glyph"><Icon name="battery" size={15} /></span>
        <span class="p-main">
          <span class="p-line">
            <span class="p-label">Battery{sys.battery.charging ? ', charging' : ''}</span>
            <span class="p-value">{pct(sys.battery.percent)}</span>
          </span>
          <Bar value={sys.battery.percent} tone={batteryTone(sys.battery.percent)} />
          <Spark values={live.history.battery} max={100} guide={100} height={22} />
        </span>
      </div>
    {/if}
  </div>

  <TopApps title="Apps using the most CPU" apps={sys.top} />

  <p class="p-head">
    <span>Memory</span>
    <span class="pill {pressure.toLowerCase()}">{pressure}</span>
  </p>
  <div class="p-card">
    <div class="p-row">
      <span class="p-glyph"><Icon name="memory" size={15} /></span>
      <span class="p-main">
        <span class="p-line"><span class="p-label">{bytes(sys.memUsed)} of {bytes(sys.memTotal)}</span><span class="p-value">{pct(memShare * 100)}</span></span>
        <Spark values={live.history.mem} max={100} height={22} tone={memShare >= 0.85 ? 'var(--warn)' : 'var(--acc)'} />
      </span>
    </div>
  </div>

  <p class="uptime"><Icon name="clock" size={13} />Up for {duration(sys.uptime)}</p>
{:else}
  <div class="p-card p-empty">Reading the system...</div>
{/if}

<style>
  .p-head {
    margin-top: 12px;
  }
  .p-card :global(.spark) {
    margin-top: 2px;
  }
  .chev {
    display: grid;
    color: var(--mut);
    transition: transform var(--m-tabSwitch-dur) var(--m-tabSwitch-ease);
  }
  .chev.open {
    transform: rotate(180deg);
  }
  .cores {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(14px, 1fr));
    gap: 4px;
    padding: 10px 12px 10px 48px;
  }
  .core {
    position: relative;
    height: 30px;
    border-radius: 4px;
    background: var(--track);
    overflow: hidden;
  }
  .core-fill {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    background: var(--acc);
    transition: height var(--m-tabSwitch-dur) var(--m-tabSwitch-ease);
  }
  .pill {
    padding: 1px 8px;
    border-radius: 99px;
    font-size: 11px;
    font-weight: 700;
  }
  .pill.normal {
    background: var(--acc-soft);
    color: var(--acc);
  }
  .pill.elevated {
    background: rgb(180 83 9 / 0.12);
    color: var(--warn);
  }
  .pill.high {
    background: var(--warn);
    color: var(--bg);
  }
  .uptime {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    margin: 12px 0 0;
    color: var(--mut);
    font-size: 12px;
  }
</style>
