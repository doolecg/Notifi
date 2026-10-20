<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { store } from '../../lib/settings.svelte';
  import Icon from '../Icon.svelte';
  import { clock } from '../format';

  interface Status {
    supported: boolean;
    active: boolean;
    endsAt: number | null;
  }

  let status = $state<Status | null>(null);
  let now = $state(Date.now());
  let error = $state('');
  const minutes = $derived(store.settings!.panel.keepAwakeMinutes);
  const choices = $derived(store.catalog?.keepAwakeChoices ?? []);
  const label = (m: number) => (m === 0 ? 'Until off' : m < 60 ? `${m} min` : `${m / 60} h`);

  async function set(on: boolean, mins = minutes) {
    error = '';
    try {
      status = await invoke<Status>('set_keep_awake', { on, minutes: mins });
    } catch (e) {
      error = String(e);
    }
  }

  function choose(m: number) {
    store.set('panel', { ...store.settings!.panel, keepAwakeMinutes: m });
    if (status?.active) set(true, m);
  }

  onMount(() => {
    invoke<Status>('keep_awake_status').then((s) => (status = s));
    const off = listen<Status>('keepawake://changed', (e) => (status = e.payload));
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => {
      off.then((f) => f());
      clearInterval(timer);
    };
  });
</script>

{#if status && !status.supported}
  <div class="p-card p-empty"><Icon name="keepAwake" size={26} />Keep awake isn't available on this system yet.</div>
{:else if status}
  <div class="p-card hero">
    <button
      class="big"
      class:on={status.active}
      role="switch"
      aria-checked={status.active}
      aria-label="Keep awake"
      onclick={() => set(!status!.active)}
    >
      <Icon name="keepAwake" size={34} />
    </button>
    <div class="state">
      <strong>{status.active ? 'Staying awake' : 'Sleeping normally'}</strong>
      <span class="p-sub">
        {#if status.active && status.endsAt}
          Turns off in <span class="count">{clock(status.endsAt - now)}</span>
        {:else if status.active}
          Until you turn it off
        {:else}
          Your computer sleeps when it's idle
        {/if}
      </span>
    </div>
  </div>

  <p class="p-head">How long</p>
  <div class="choices" role="radiogroup" aria-label="How long">
    {#each choices as m}
      <button role="radio" aria-checked={minutes === m} class:picked={minutes === m} onclick={() => choose(m)}>{label(m)}</button>
    {/each}
  </div>
  {#if error}<p class="error">{error}</p>{/if}
{/if}

<style>
  .hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 22px 16px 18px;
  }
  .big {
    display: grid;
    place-items: center;
    width: 88px;
    height: 88px;
    border: 1px solid var(--line-strong);
    border-radius: 50%;
    background: var(--inset);
    color: var(--mut);
    cursor: pointer;
    transition:
      background-color var(--m-toggleSwitch-dur) var(--m-toggleSwitch-ease),
      color var(--m-toggleSwitch-dur) var(--m-toggleSwitch-ease),
      box-shadow var(--m-toggleSwitch-dur) var(--m-toggleSwitch-ease);
  }
  .big:hover {
    color: var(--fg-soft);
  }
  .big.on {
    border-color: var(--acc);
    background: var(--acc);
    color: var(--acc-fg);
    box-shadow: 0 0 0 6px var(--acc-soft);
  }
  .big:focus-visible {
    outline: 2px solid var(--acc);
    outline-offset: 3px;
  }
  .state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    font-size: 14px;
  }
  .count {
    color: var(--fg);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .p-head {
    margin-top: 14px;
  }
  .choices {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 3px;
    padding: 3px;
    border-radius: 10px;
    background: var(--inset);
  }
  .choices button {
    height: 30px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--fg-soft);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
    transition: background-color var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
  .choices button.picked {
    background: var(--surface);
    color: var(--fg);
    font-weight: 600;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.12);
  }
  .error {
    margin: 8px 4px 0;
    color: var(--warn);
    font-size: 12px;
  }
</style>
