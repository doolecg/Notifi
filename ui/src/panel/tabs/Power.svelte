<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import Bar from '../Bar.svelte';
  import Icon from '../Icon.svelte';
  import Spark from '../Spark.svelte';
  import TopApps from '../TopApps.svelte';
  import { duration, pct, watts } from '../format';
  import { live } from '../live.svelte';

  const power = $derived(live.power);
  const b = $derived(power?.battery ?? null);
  const level = (p: number) => (p <= 15 ? 'var(--warn)' : p <= 35 ? '#d4a017' : 'var(--acc)');
  const stateLabel = $derived.by(() => {
    const s = (b?.state ?? '').toLowerCase();
    if (s === 'full') return 'Full';
    if (s === 'not-charging') return 'Not charging';
    if (s === 'charging' || b?.charging) return 'Charging';
    return 'Discharging';
  });
  const remaining = $derived(
    b?.seconds_remaining ? `${duration(b.seconds_remaining)} ${b.charging ? 'to full' : 'left'}` : stateLabel,
  );
  const anyRows = $derived(
    !!b && [b.temperature_celsius, b.system_watts, b.on_ac ? b.adapter_watts : null, b.power_watts, b.health_percent].some((v) => v != null),
  );
  const active = $derived(power?.profiles.find((p) => p.active)?.id ?? '');
  let error = $state('');

  async function setProfile(id: string) {
    error = '';
    try {
      await invoke('set_power_profile', { id });
    } catch (e) {
      error = String(e);
    }
  }
</script>

{#if b}
  <div class="p-card">
    <div class="p-row">
      <span class="p-glyph" style:color={level(b.percent)}><Icon name="battery" size={15} /></span>
      <span class="p-main">
        <span class="p-line"><span class="p-label">Battery</span><span class="p-value">{pct(b.percent)}</span></span>
        <Bar value={b.percent} tone={level(b.percent)} />
        <Spark values={live.history.battery} max={100} guide={100} height={26} tone={level(b.percent)} />
        <span class="p-sub">{remaining}</span>
      </span>
    </div>
  </div>

  <TopApps title="Apps using the most energy" apps={power?.top ?? []} />

  {#if anyRows}
  <div class="p-card rows">
    {#if b.temperature_celsius != null}
      <div class="p-row">
        <span class="p-glyph warm"><Icon name="thermometer" size={15} /></span>
        <span class="p-main"><span class="p-label">Battery temperature</span></span>
        <span class="p-value">{b.temperature_celsius.toFixed(1)}°C</span>
      </div>
    {/if}
    {#if b.system_watts != null}
      <div class="p-row">
        <span class="p-glyph"><Icon name="gauge" size={15} /></span>
        <span class="p-main">
          <span class="p-line"><span class="p-label">System power draw</span><span class="p-value">{watts(b.system_watts)}</span></span>
          <Spark values={live.history.watts} guide={10} height={24} />
        </span>
      </div>
    {/if}
    {#if b.on_ac && b.adapter_watts != null}
      <div class="p-row">
        <span class="p-glyph"><Icon name="plug" size={15} /></span>
        <span class="p-main">
          <span class="p-label">Adapter</span>
          {#if b.adapter_max_watts != null}<span class="p-sub">{Math.round(b.adapter_max_watts)} W max</span>{/if}
        </span>
        <span class="p-value">{watts(b.adapter_watts)}</span>
      </div>
    {/if}
    {#if b.power_watts != null}
      <div class="p-row">
        <span class="p-glyph"><Icon name="bolt" size={15} /></span>
        <span class="p-main">
          <span class="p-label">Battery</span>
          <span class="p-sub">{stateLabel}</span>
        </span>
        <span class="p-value">{b.charging ? '+' : b.power_watts > 0 ? '−' : ''}{watts(b.power_watts)}</span>
      </div>
    {/if}
    {#if b.health_percent != null}
      <div class="p-row">
        <span class="p-glyph"><Icon name="heart" size={15} /></span>
        <span class="p-main">
          <span class="p-label">Battery health</span>
          {#if b.cycle_count != null}<span class="p-sub">{b.cycle_count} cycles</span>{/if}
        </span>
        <span class="p-value">{pct(b.health_percent)}</span>
      </div>
    {/if}
  </div>
  {/if}

  {#if power?.profiles.length}
    <p class="p-head">Power plan</p>
    <div class="p-card">
      <label class="p-row plan">
        <span class="p-glyph"><Icon name="energy" size={15} /></span>
        <select value={active} onchange={(e) => setProfile(e.currentTarget.value)} disabled={power.profiles.length < 2}>
          {#each power.profiles as p (p.id)}
            <option value={p.id}>{p.name}</option>
          {/each}
        </select>
      </label>
    </div>
    {#if error}<p class="error">{error}</p>{/if}
  {/if}
{:else}
  <div class="p-card p-empty"><Icon name="battery" size={26} />{power ? 'No battery found' : 'Reading the battery...'}</div>
{/if}

<style>
  .rows {
    margin-top: 10px;
  }
  .p-head {
    margin-top: 12px;
  }
  .warm {
    background: rgb(180 83 9 / 0.12);
    color: var(--warn);
  }
  .plan select {
    flex: 1;
    height: 30px;
    padding: 0 8px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    font-size: 13px;
  }
  .error {
    margin: 8px 4px 0;
    color: var(--warn);
    font-size: 12px;
  }
</style>
