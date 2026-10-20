<script lang="ts">
  import Icon from '../Icon.svelte';
  import Spark from '../Spark.svelte';
  import { bytes, rate } from '../format';
  import { live } from '../live.svelte';

  let copied = $state(false);
  let timer: ReturnType<typeof setTimeout>;
  const net = $derived(live.network);

  async function copyIp() {
    if (!net?.ip) return;
    await navigator.clipboard.writeText(net.ip);
    copied = true;
    clearTimeout(timer);
    timer = setTimeout(() => (copied = false), 1400);
  }
</script>

{#if net}
  <div class="p-tiles">
    <div class="p-tile">
      <span class="p-sub dir"><Icon name="arrowDown" size={13} />Download</span>
      <span class="p-value">{rate(net.down)}</span>
      <Spark values={live.history.down} />
    </div>
    <div class="p-tile">
      <span class="p-sub dir"><Icon name="arrowUp" size={13} />Upload</span>
      <span class="p-value">{rate(net.up)}</span>
      <Spark values={live.history.up} tone="var(--warn)" />
    </div>
  </div>

  <p class="p-head">Since Notifi started</p>
  <div class="p-card">
    <div class="p-row">
      <span class="p-glyph"><Icon name="arrowDown" size={15} /></span>
      <span class="p-main"><span class="p-line"><span class="p-label">Downloaded</span><span class="p-value">{bytes(net.downTotal)}</span></span></span>
    </div>
    <div class="p-row">
      <span class="p-glyph"><Icon name="arrowUp" size={15} /></span>
      <span class="p-main"><span class="p-line"><span class="p-label">Uploaded</span><span class="p-value">{bytes(net.upTotal)}</span></span></span>
    </div>
  </div>

  {#if net.ip}
    <div class="p-card">
      <button class="p-row p-toggle ip" title="Copy" onclick={copyIp}>
        <span class="p-glyph"><Icon name="globe" size={15} /></span>
        <span class="p-main"><span class="p-line"><span class="p-label">Local IP</span><span class="p-value mono">{net.ip}</span></span></span>
        <span class="copy" class:done={copied}><Icon name={copied ? 'check' : 'copy'} size={15} /></span>
      </button>
    </div>
  {/if}
{:else}
  <div class="p-card p-empty">Reading network...</div>
{/if}

<style>
  .dir {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .p-tile :global(.spark) {
    margin-top: 6px;
  }
  .ip {
    border-radius: 11px;
  }
  .mono {
    font-family: var(--mono);
    font-size: 12.5px;
  }
  .copy {
    display: grid;
    color: var(--mut);
  }
  .copy.done {
    color: var(--acc);
  }
</style>
