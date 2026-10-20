<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { store } from '../../lib/settings.svelte';
  import Bar from '../Bar.svelte';
  import Icon from '../Icon.svelte';
  import { bytes } from '../format';
  import { live } from '../live.svelte';

  interface Drive {
    name: string;
    media: string;
    health: string;
    temperature: number | null;
    wear: number | null;
    powerOnHours: number | null;
  }
  interface Devices {
    devices: { name: string; kind: string }[];
    drives: Drive[];
  }

  const showDisks = $derived(store.values('monitor').showDisks !== false);
  const showSmart = $derived(store.values('monitor').showSmart !== false);
  const showDevices = $derived(store.featureOn('monitor.connectedDevices'));
  let extra = $state<Devices>({ devices: [], drives: [] });
  $effect(() => {
    if (!showSmart && !showDevices) {
      extra = { devices: [], drives: [] };
      return;
    }
    const read = async () => {
      try {
        extra = await invoke<Devices>('monitor_devices');
      } catch {
        extra = { devices: [], drives: [] };
      }
    };
    read();
    const timer = setInterval(read, 30000);
    return () => clearInterval(timer);
  });
  const driveLine = (d: Drive) =>
    [d.media, d.temperature != null ? `${Math.round(d.temperature)}°C` : '', d.wear != null ? `${Math.round(d.wear)}% worn` : '', d.powerOnHours != null ? `${Math.round(d.powerOnHours)} h on` : '']
      .filter(Boolean)
      .join(', ');

  let ejecting = $state<string | null>(null);
  let error = $state('');

  const tone = (share: number) => (share >= 0.9 ? 'var(--warn)' : 'var(--acc)');
  const title = (name: string, mount: string) => {
    const clean = mount.replace(/[\/]$/, '') || mount;
    return name && name !== mount && !name.startsWith('/dev/') ? `${name} (${clean})` : clean;
  };

  async function eject(mount: string) {
    ejecting = mount;
    error = '';
    try {
      await invoke('eject_volume', { mount });
    } catch (e) {
      error = String(e);
    } finally {
      ejecting = null;
    }
  }
</script>

{#if !showDisks}
  <!-- Disks are hidden by the "Show disks" setting. -->
{:else if live.storage?.length}
  <div class="p-card">
    {#each live.storage as disk (disk.mount)}
      {@const share = disk.used / disk.total}
      <div class="p-row">
        <span class="p-glyph"><Icon name="storage" size={15} /></span>
        <span class="p-main">
          <span class="p-line"><span class="p-label">{title(disk.name, disk.mount)}</span><span class="p-value">{Math.round(share * 100)}%</span></span>
          <Bar value={share * 100} tone={tone(share)} />
          <span class="p-sub">{bytes(disk.used)} of {bytes(disk.total)} used, {bytes(disk.total - disk.used)} free</span>
        </span>
        {#if disk.removable}
          <button class="eject" title="Eject" aria-label="Eject {disk.mount}" disabled={ejecting === disk.mount} onclick={() => eject(disk.mount)}>
            <Icon name="eject" size={15} />
          </button>
        {/if}
      </div>
    {/each}
  </div>
  {#if error}<p class="error">{error}</p>{/if}
{:else}
  <div class="p-card p-empty">{live.storage ? 'No disks found' : 'Reading disks...'}</div>
{/if}

{#if showSmart && extra.drives.length}
  <p class="p-head">Drive health</p>
  <div class="p-card">
    {#each extra.drives as drive (drive.name)}
      <div class="p-row">
        <span class="p-glyph"><Icon name="storage" size={15} /></span>
        <span class="p-main">
          <span class="p-line"><span class="p-label">{drive.name}</span><span class="p-value" class:bad={drive.health && drive.health !== 'Healthy'}>{drive.health || 'Unknown'}</span></span>
          {#if driveLine(drive)}<span class="p-sub">{driveLine(drive)}</span>{/if}
        </span>
      </div>
    {/each}
  </div>
{/if}

{#if showDevices && extra.devices.length}
  <p class="p-head">Connected devices</p>
  <div class="p-card">
    {#each extra.devices as device (device.name)}
      <div class="p-row">
        <span class="p-glyph"><Icon name="plug" size={15} /></span>
        <span class="p-main">
          <span class="p-line"><span class="p-label">{device.name}</span>{#if device.kind}<span class="p-sub">{device.kind}</span>{/if}</span>
        </span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .eject {
    display: grid;
    place-items: center;
    flex: none;
    width: 30px;
    height: 30px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg-soft);
    cursor: pointer;
  }
  .eject:hover:not(:disabled) {
    background: var(--hover);
  }
  .eject:disabled {
    opacity: 0.45;
  }
  .bad {
    color: var(--warn);
  }
  .error {
    margin: 8px 4px 0;
    color: var(--warn);
    font-size: 12px;
  }
</style>
