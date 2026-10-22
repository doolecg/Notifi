<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { fadeSlide } from '../../lib/motion';
  import Glyph from '../Glyph.svelte';

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

  const notes = $derived(
    (saved?.notes ?? [])
      .filter((n) => n.text.trim())
      .slice()
      .reverse(),
  );
  let editing = $state<number | null>(null);
  let undo = $state<{ note: Note; at: number } | null>(null);
  let undoTimer: ReturnType<typeof setTimeout> | undefined;

  async function load() {
    try {
      const r = await invoke<{ state: Saved; warning: string | null; pinned: boolean }>('scratchpad_load');
      saved = r.state;
      warning = r.warning ?? '';
    } catch (err) {
      warning = String(err);
    }
  }

  /** Reads the file fresh, lets `change` build the new state from it, and saves that. */
  async function commit(change: (s: Saved) => Saved) {
    if (busy) return false;
    busy = true;
    try {
      await load();
      if (!saved) return false;
      const value = change(saved);
      await invoke('scratchpad_save', { value });
      saved = value;
      warning = '';
      return true;
    } catch (err) {
      warning = `Could not save: ${String(err)}`;
      return false;
    } finally {
      busy = false;
    }
  }

  async function add() {
    const text = draft.trim();
    if (!text) return;
    const id = editing;
    const ok = await commit((s) => {
      if (id !== null && s.notes.some((n) => n.id === id)) {
        return { ...s, notes: s.notes.map((n) => (n.id === id ? { ...n, text } : n)) };
      }
      const used = new Set(s.notes.map((n) => n.title));
      let n = 1;
      while (used.has(String(n))) n++;
      const next = s.next ?? Math.max(0, ...s.notes.map((x) => x.id)) + 1;
      return { notes: [...s.notes, { id: next, title: String(n), text }], active: s.active, next: next + 1 };
    });
    if (ok) cancel();
  }

  function edit(n: Note) {
    editing = n.id;
    draft = n.text;
  }

  function cancel() {
    editing = null;
    draft = '';
  }

  async function remove(id: number) {
    let gone: { note: Note; at: number } | null = null;
    const ok = await commit((s) => {
      const i = s.notes.findIndex((n) => n.id === id);
      if (i < 0) return s;
      gone = { note: s.notes[i], at: i };
      let list = s.notes.filter((n) => n.id !== id);
      let next = s.next;
      if (!list.length) {
        list = [{ id: next, title: '1', text: '' }];
        next += 1;
      }
      const active = list.some((n) => n.id === s.active) ? s.active : list[Math.min(i, list.length - 1)].id;
      return { notes: list, active, next };
    });
    if (!ok || !gone) return;
    if (editing === id) cancel();
    undo = gone;
    clearTimeout(undoTimer);
    undoTimer = setTimeout(() => (undo = null), 6000);
  }

  async function restore() {
    const back = undo;
    if (!back) return;
    undo = null;
    clearTimeout(undoTimer);
    await commit((s) => {
      if (s.notes.some((n) => n.id === back.note.id)) return s;
      // The placeholder an emptied pad was given makes way for the note.
      const list = s.notes.length === 1 && !s.notes[0].text.trim() ? [] : s.notes.slice();
      list.splice(Math.min(back.at, list.length), 0, back.note);
      return { ...s, notes: list, active: list.some((n) => n.id === s.active) ? s.active : back.note.id };
    });
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
    <span class="acts">
      {#if editing !== null}<button class="ghost" onclick={cancel}>Cancel</button>{/if}
      <button class="add" disabled={!draft.trim()} onclick={add}>{editing !== null ? 'Save' : 'Add note'}</button>
    </span>
  </header>

  <textarea placeholder={editing !== null ? 'Edit note (Ctrl+Enter to save)' : 'Quick note (Ctrl+Enter to add)'} aria-label="Quick note" bind:value={draft} onkeydown={key}></textarea>

  {#if warning}<p class="warn">{warning}</p>{/if}

  {#if notes.length}
    <ul>
      {#each notes as n (n.id)}
        <li class:on={editing === n.id} in:fadeSlide={{ token: 'tabSwitch', y: 6 }}>
          <button class="row" title={n.text} onclick={() => edit(n)}>{n.text.trim().split('\n')[0]}</button>
          <button class="del" aria-label="Delete note" title="Delete" onclick={() => remove(n.id)}><Glyph name="close" size={13} /></button>
        </li>
      {/each}
    </ul>
  {:else if !warning}
    <p class="empty">No notes yet</p>
  {/if}

  {#if undo}
    <div class="undo" in:fadeSlide={{ token: 'tabSwitch', y: 6 }}>Note deleted <button onclick={restore}>Undo</button></div>
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
    display: flex;
    align-items: center;
    border-radius: 6px;
    font-size: 12.5px;
  }
  li:hover,
  li.on {
    background: var(--i-hover);
  }
  .row {
    flex: 1;
    min-width: 0;
    padding: 4px 8px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    cursor: pointer;
  }
  .del {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    margin-right: 2px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--i-mut);
    opacity: 0;
    cursor: pointer;
  }
  li:hover .del,
  .del:focus-visible {
    opacity: 1;
  }
  .del:hover {
    color: #ff6b5e;
  }
  .acts {
    display: flex;
    gap: 6px;
  }
  .ghost {
    padding: 3px 10px;
    border: 0;
    border-radius: 6px;
    background: var(--i-raised);
    color: var(--i-fg);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .undo {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 5px 10px;
    border-radius: 8px;
    background: var(--i-raised);
    color: var(--i-mut);
    font-size: 12px;
  }
  .undo button {
    border: 0;
    background: none;
    color: var(--i-acc);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
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
