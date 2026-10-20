<script lang="ts">
  // Custom option of the Touchpad middle click page (spec/input/middle-click-touchpad.md): `systemSettings`.
  import { invoke } from '@tauri-apps/api/core';
  import type { CustomProps } from '../sections/customs';

  let { option }: CustomProps = $props();
  let error = $state('');

  async function open() {
    error = '';
    try {
      await invoke('input_open_touchpad_settings');
    } catch (e) {
      error = String(e);
    }
  }
</script>

{#if option.id === 'systemSettings'}
  <div class="row">
    <span class="name">{option.label}<small>{error || option.hint}</small></span>
    <button onclick={open}>Open</button>
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
