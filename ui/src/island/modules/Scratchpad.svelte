<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { fadeSlide } from '../../lib/motion';

  interface Note {
    id: number;
    title: string;
    text: string;
  }
  interface Saved {
    notes: Note[];
    active: number;
    next: number;
  }

  let saved = $state.raw<Saved | null>(null);
  let draft = $state('');
  let warning = $state('');
  let busy = false;

  const recent = $derived(
    (saved?.notes ?? [])
      .filter((n) => n.text.trim())
      .slice()
      .reverse()
      .slice(0, 6),
  );

  async function load() {
    try {
      const r = await invoke<{ state: Saved; warning: string | null; pinned: boolean }>('scratchpad_load');
      saved = r.state;
      warning = r.warning ?? '';
    } catch (err) {
      warning = String(err);
    }
  }

  async function add() {
    const text = draft.trim();
    if (!text || busy) return;
    busy = true;
    try {
      await load();
      if (!saved) return;
      const used = new Set(saved.notes.map((n) => n.title));
      let n = 1;
      while (used.has(String(n))) n++;
      const id = saved.next ?? Math.max(0, ...saved.notes.map((x) => x.id)) + 1;
      const value: Saved = {
        notes: [...saved.notes, { id, title: String(n), text }],
        active: saved.active,
        next: id + 1,
      };
      await invoke('scratchpad_save', { value });
      saved = value;
      draft = '';
      warning = '';
    } catch (err) {
      warning = `Could not save: ${String(err)}`;
    } finally {
      busy = false;
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      add();
    }
  }

  onMount(load);
</script>

<div class="sp">
  <header>
    <h2>Scratchpad</h2>
    <button class="add" disabled={!draft.trim()} onclick={add}>Add note</button>
  </header>

  <textarea placeholder="Quick note (Ctrl+Enter to add)" aria-label="Quick note" bind:value={draft} onkeydown={key}></textarea>

  {#if warning}<p class="warn">{warning}</p>{/if}

  {#if recent.length}
    <ul>
      {#each recent as n (n.id)}
        <li in:fadeSlide={{ token: 'tabSwitch', y: 6 }} title={n.text}>{n.text.trim().split('\n')[0]}</li>
      {/each}
    </ul>
  {:else if !warning}
    <p class="empty">No notes yet</p>
  {/if}
</div>

<style>
  .sp {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: 8px;
    min-height: 0;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--i-mut);
  }
  .add {
    padding: 3px 10px;
    border: 0;
    border-radius: 6px;
    background: var(--i-acc);
    color: #000;
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    transition: opacity var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
  .add:disabled {
    opacity: 0.35;
    cursor: default;
  }
  textarea {
    flex: none;
    height: 56px;
    padding: 6px 8px;
    border: 1px solid var(--i-line);
    border-radius: 8px;
    background: var(--i-raised);
    color: var(--i-fg);
    font: inherit;
    font-size: 12.5px;
    resize: none;
  }
  ul {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
    scrollbar-width: none;
  }
  li {
    padding: 4px 8px;
    overflow: hidden;
    border-radius: 6px;
    font-size: 12.5px;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  li:hover {
    background: var(--i-hover);
  }
  .empty,
  .warn {
    margin: auto;
    color: var(--i-mut);
    text-align: center;
  }
  .warn {
    margin: 0;
    font-size: 12px;
  }
</style>
