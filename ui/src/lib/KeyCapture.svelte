<script lang="ts">
  import { fromEvent, keycaps } from './accel';
  import { store } from './settings.svelte';

  let { value, label, onchange }: { value: string; label: string; onchange: (accel: string) => void } = $props();

  let recording = $state(false);
  let held = $state<string[]>([]);
  let button: HTMLButtonElement;

  async function start() {
    if (recording) return;
    recording = true;
    held = [];
    await store.pauseShortcuts(true);
    button.focus();
  }

  async function stop(accel?: string) {
    if (!recording) return;
    recording = false;
    held = [];
    if (accel) onchange(accel);
    await store.pauseShortcuts(false);
  }

  function modifiers(e: KeyboardEvent) {
    return [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Super'].filter(Boolean) as string[];
  }

  function onkeydown(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.code === 'Escape' && !modifiers(e).length) return stop();
    const accel = fromEvent(e);
    if (accel) stop(accel);
    else held = modifiers(e);
  }

  function onkeyup(e: KeyboardEvent) {
    if (recording) held = modifiers(e);
  }
</script>

<button
  bind:this={button}
  class="capture"
  class:recording
  aria-label="Shortcut for {label}"
  onclick={start}
  {onkeydown}
  {onkeyup}
  onblur={() => stop()}
>
  {#if recording}
    {#if held.length}
      {#each keycaps(held.join('+')) as cap}<kbd>{cap}</kbd>{/each}<span class="hint">…</span>
    {:else}
      <span class="hint">Press a shortcut · Esc to cancel</span>
    {/if}
  {:else if value}
    {#each keycaps(value) as cap}<kbd>{cap}</kbd>{/each}
  {:else}
    <span class="hint">Not set</span>
  {/if}
</button>

<style>
  .capture {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-width: 168px;
    min-height: 34px;
    padding: 4px 8px;
    border: 1px solid var(--line-strong);
    border-radius: 9px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    cursor: pointer;
    transition: border-color var(--m-hoverLift-dur) var(--m-hoverLift-ease),
      box-shadow var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .capture:hover {
    border-color: var(--mut);
  }
  .capture.recording {
    border-color: var(--acc);
    box-shadow: 0 0 0 3px var(--acc-soft);
  }
  .capture:focus-visible {
    outline: none;
    border-color: var(--acc);
  }
  kbd {
    padding: 2px 7px;
    border: 1px solid var(--line-strong);
    border-bottom-width: 2px;
    border-radius: 6px;
    background: var(--inset);
    font: 600 12px/1.4 var(--mono);
  }
  .hint {
    color: var(--mut);
    font-size: 12.5px;
  }
</style>
