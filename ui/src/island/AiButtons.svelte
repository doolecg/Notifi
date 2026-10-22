<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import Glyph from './Glyph.svelte';

  /** What each button types into the AI app in front (island_ai_send in island/mod.rs). */
  const BUTTONS = [
    { id: 'stop', label: 'Stop', icon: 'stop', tip: 'Press Esc in the app to stop the agent' },
    { id: 'continue', label: 'Continue', icon: 'play', tip: 'Type "Continue" and send' },
    { id: 'clear', label: 'Clear', icon: 'clear', tip: 'Type /clear and send: start a fresh chat' },
    { id: 'compact', label: 'Compact', icon: 'compact', tip: 'Type /compact and send: shrink the chat to save context' },
  ];

  function send(e: MouseEvent, action: string) {
    e.stopPropagation();
    invoke('island_ai_send', { action }).catch(() => {});
  }
</script>

<span class="ai" role="group" aria-label="AI app">
  {#each BUTTONS as b (b.id)}
    <button class="aib" title={b.tip} onclick={(e) => send(e, b.id)}><Glyph name={b.icon} size={14} filled={b.id === 'stop' || b.id === 'continue'} />{b.label}</button>
  {/each}
</span>

<style>
  .ai {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .aib {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 10px 0 8px;
    border: 0;
    border-radius: 99px;
    background: var(--i-raised, #1b1e1f);
    color: var(--i-fg, #f2f4f3);
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .aib:hover {
    background: var(--i-hover, rgb(255 255 255 / 0.08));
  }
</style>
