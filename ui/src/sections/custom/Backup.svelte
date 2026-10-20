<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import type { CustomProps } from '../customs';
  import { store, type Settings } from '../../lib/settings.svelte';

  let { option }: CustomProps = $props();
  let note = $state('');
  let failed = $state(false);
  let input = $state<HTMLInputElement>();

  async function exportNow() {
    try {
      const path = await invoke<string>('export_settings');
      [note, failed] = [`Saved to ${path}`, false];
    } catch (err) {
      [note, failed] = [String(err), true];
    }
  }

  async function importFile(file: File | undefined) {
    if (!file) return;
    try {
      store.settings = await invoke<Settings>('import_settings', { text: await file.text() });
      [note, failed] = [`Loaded ${file.name}`, false];
    } catch (err) {
      [note, failed] = [String(err), true];
    }
    if (input) input.value = '';
  }
</script>

<div class="row">
  <span class="name">
    {option.label}
    <small class:warn={failed}>{note || option.hint}</small>
  </span>
  {#if option.id === 'exportBackup'}
    <button class="ghost" onclick={exportNow}>Export</button>
  {:else}
    <input bind:this={input} type="file" accept=".json,application/json" hidden onchange={(e) => importFile(e.currentTarget.files?.[0])} />
    <button class="ghost" onclick={() => input?.click()}>Import...</button>
  {/if}
</div>

<style>
  .warn {
    color: var(--warn) !important;
  }
</style>
