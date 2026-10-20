<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import Bar from '../Bar.svelte';
  import Icon from '../Icon.svelte';
  import { bytes } from '../format';
  import { live } from '../live.svelte';

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

{#if live.storage?.length}
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
  .error {
    margin: 8px 4px 0;
    color: var(--warn);
    font-size: 12px;
  }
</style>
