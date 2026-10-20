<script lang="ts">
  // Custom options of the Per-app output page (spec/sound/per-app-output.md): only `resetRoutes`.
  import { invoke } from '@tauri-apps/api/core';
  import type { CustomProps } from '../sections/customs';

  let { option }: CustomProps = $props();
  let done = $state(false);
  let error = $state('');

  async function reset() {
    error = '';
    try {
      await invoke('reset_routes');
      done = true;
      setTimeout(() => (done = false), 1800);
    } catch (e) {
      error = String(e);
    }
  }
</script>

{#if option.id === 'resetRoutes'}
  <div class="row">
    <span class="name">{option.label}<small>{error || option.hint}</small></span>
    <button onclick={reset}>{done ? 'Done' : 'Reset'}</button>
  </div>
{/if}

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
  }
  .name {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 13px;
  }
  small {
    color: var(--mut);
    font-size: 11.5px;
  }
  button {
    height: 28px;
    padding: 0 14px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--inset);
    color: var(--fg);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }
  button:focus-visible {
    outline: 2px solid var(--acc);
    outline-offset: 2px;
  }
</style>
