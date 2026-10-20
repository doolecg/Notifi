<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, tick } from 'svelte';
  import { flip } from 'svelte/animate';
  import { cubicOut } from 'svelte/easing';
  import { SvelteSet } from 'svelte/reactivity';
  import { duration, fadeSlide } from '../lib/motion';
  import { words } from '../clipboard/entries';
  import Glyph from '../clipboard/Glyph.svelte';
  import Highlight from '../clipboard/Highlight.svelte';
  import DateBuilder from './DateBuilder.svelte';
  import { zonesFor, type Snippet } from './vars';

  let snippets = $state<Snippet[]>([]);
  let loaded = $state(false);
  let loadError = $state('');
  let saveError = $state('');
  let selectedId = $state<string | null>(null);
  let query = $state('');
  let building = $state(false);
  let confirmDelete = $state(false);
  let preview = $state('');
  let previewFailed = $state(false);
  const collapsed = new SvelteSet<string>();

  let textarea = $state<HTMLTextAreaElement>();
  let nameInput = $state<HTMLInputElement>();
  let saveTimer: ReturnType<typeof setTimeout> | undefined;

  const sel = $derived(snippets.find((s) => s.id === selectedId) ?? null);
  const ws = $derived(words(query));
  const groupNames = $derived([...new Set(snippets.map((s) => s.group.trim()).filter(Boolean))]);

  const sections = $derived.by(() => {
    const hits = ws.length
      ? snippets.filter((s) => {
          const hay = `${s.name}\n${s.trigger}\n${s.text}\n${s.group}`.toLowerCase();
          return ws.every((w) => hay.includes(w));
        })
      : snippets;
    const groups = new Map<string, Snippet[]>();
    for (const s of hits) {
      const g = s.group.trim();
      if (!groups.has(g)) groups.set(g, []);
      groups.get(g)!.push(s);
    }
    const order = [...(groups.has('') ? [''] : []), ...[...groups.keys()].filter(Boolean)];
    return order.map((group) => ({ group, items: groups.get(group)! }));
  });
  const grouped = $derived(sections.length > 1 || (sections[0]?.group ?? '') !== '');
  const shown = $derived(sections.flatMap((s) => (grouped && collapsed.has(s.group) ? [] : s.items)));

  const clash = $derived(
    sel && sel.trigger.trim() ? snippets.some((s) => s.id !== sel.id && s.trigger.trim() === sel.trigger.trim()) : false,
  );

  function schedule() {
    saveError = '';
    clearTimeout(saveTimer);
    saveTimer = setTimeout(save, 400);
  }

  async function save() {
    saveTimer = undefined;
    try {
      await invoke('snippets_save', { snippets: $state.snapshot(snippets) });
    } catch (err) {
      saveError = String(err) || 'Could not save the snippets.';
    }
  }

  async function select(id: string, focusName = false) {
    selectedId = id;
    confirmDelete = false;
    const s = snippets.find((x) => x.id === id);
    if (s) collapsed.delete(s.group.trim());
    if (focusName) {
      await tick();
      nameInput?.focus();
      nameInput?.select();
    }
  }

  function add() {
    const group = sel?.group.trim() ?? '';
    const s: Snippet = { id: crypto.randomUUID(), name: 'New snippet', trigger: '', text: '', group };
    let at = snippets.length;
    for (let i = snippets.length - 1; i >= 0; i--) {
      if (snippets[i].group.trim() === group) {
        at = i + 1;
        break;
      }
    }
    snippets.splice(at, 0, s);
    query = '';
    schedule();
    select(s.id, true);
  }

  function duplicate() {
    if (!sel) return;
    const copy: Snippet = { ...$state.snapshot(sel), id: crypto.randomUUID(), name: `${sel.name} copy` };
    snippets.splice(snippets.indexOf(sel) + 1, 0, copy);
    schedule();
    select(copy.id, true);
  }

  function remove() {
    if (!sel) return;
    const order = shown.length ? shown : snippets;
    const i = order.findIndex((s) => s.id === sel.id);
    const next = order[i + 1] ?? order[i - 1] ?? null;
    snippets.splice(snippets.indexOf(sel), 1);
    confirmDelete = false;
    selectedId = next?.id ?? null;
    schedule();
  }

  /** Moves the selected snippet past its neighbour in the same group. */
  function moveSnippet(by: -1 | 1) {
    if (!sel) return;
    const group = sel.group.trim();
    const siblings = snippets.map((s, i) => [s, i] as const).filter(([s]) => s.group.trim() === group);
    const k = siblings.findIndex(([s]) => s.id === sel.id);
    const other = siblings[k + by];
    if (!other) return;
    const i = siblings[k][1];
    const j = other[1];
    [snippets[i], snippets[j]] = [snippets[j], snippets[i]];
    schedule();
  }

  /** Alt+Up / Alt+Down inside the text: moves the lines the selection touches. */
  function moveLines(ta: HTMLTextAreaElement, by: -1 | 1) {
    if (!sel) return;
    const v = ta.value;
    const s = ta.selectionStart;
    const e = ta.selectionEnd;
    const start = v.lastIndexOf('\n', s - 1) + 1;
    let end = v.indexOf('\n', e > s && v[e - 1] === '\n' ? e - 1 : e);
    if (end < 0) end = v.length;
    const block = v.slice(start, end);
    let next: string;
    let shift: number;
    if (by < 0) {
      if (start === 0) return;
      const prevStart = v.lastIndexOf('\n', start - 2) + 1;
      const prev = v.slice(prevStart, start - 1);
      next = v.slice(0, prevStart) + block + '\n' + prev + v.slice(end);
      shift = -(prev.length + 1);
    } else {
      if (end >= v.length) return;
      let after = v.indexOf('\n', end + 1);
      if (after < 0) after = v.length;
      const below = v.slice(end + 1, after);
      next = v.slice(0, start) + below + '\n' + block + v.slice(after);
      shift = below.length + 1;
    }
    ta.value = next;
    sel.text = next;
    ta.setSelectionRange(s + shift, e + shift);
    schedule();
  }

  function insertAtCursor(token: string) {
    const ta = textarea;
    if (!sel || !ta) return;
    const s = ta.selectionStart ?? ta.value.length;
    const e = ta.selectionEnd ?? s;
    ta.focus();
    ta.setRangeText(token, s, e, 'end');
    sel.text = ta.value;
    schedule();
  }

  function listKey(e: KeyboardEvent) {
    if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
    e.preventDefault();
    const by = e.key === 'ArrowUp' ? -1 : 1;
    if (e.altKey) {
      moveSnippet(by);
      return;
    }
    const i = shown.findIndex((s) => s.id === selectedId);
    const to = shown[Math.min(shown.length - 1, Math.max(0, i + by))];
    if (to) {
      select(to.id);
      tick().then(() => (document.querySelector(`[data-snippet="${CSS.escape(to.id)}"]`) as HTMLElement | null)?.focus());
    }
  }

  function detailKey(e: KeyboardEvent) {
    if (!e.altKey || (e.key !== 'ArrowUp' && e.key !== 'ArrowDown')) return;
    e.preventDefault();
    const by = e.key === 'ArrowUp' ? -1 : 1;
    if (e.target === textarea && textarea) moveLines(textarea, by);
    else moveSnippet(by);
  }

  function reorder(node: Element, rects: { from: DOMRect; to: DOMRect }) {
    return flip(node, rects, { duration: duration('listReorder'), easing: cubicOut });
  }

  $effect(() => {
    const text = sel?.text;
    if (text === undefined) return;
    if (!text) {
      preview = '';
      previewFailed = false;
      return;
    }
    const timer = setTimeout(() => {
      invoke<string>('snippet_preview', { text, zones: zonesFor(text) })
        .then((s) => {
          preview = s;
          previewFailed = false;
        })
        .catch(() => (previewFailed = true));
    }, 150);
    return () => clearTimeout(timer);
  });

  onMount(() => {
    invoke<{ snippets: Snippet[] }>('snippets_get')
      .then((r) => {
        snippets = r.snippets;
        selectedId = r.snippets[0]?.id ?? null;
        loaded = true;
      })
      .catch((err) => (loadError = String(err) || 'Could not load the snippets.'));
    return () => {
      if (saveTimer) {
        clearTimeout(saveTimer);
        save();
      }
    };
  });
</script>

<div class="editor">
  {#if loadError}
    <p class="warn">{loadError}</p>
  {:else}
    <div class="split">
      <div class="side">
        <div class="side-head">
          <label class="search">
            <Glyph name="search" size={14} />
            <input type="text" placeholder="Search snippets" aria-label="Search snippets" bind:value={query} spellcheck="false" />
          </label>
          <button class="ghost add" onclick={add}><Glyph name="plus" size={14} />New</button>
        </div>
        <div class="list" role="listbox" aria-label="Snippets" tabindex="-1" onkeydown={listKey}>
          {#if loaded && !snippets.length}
            <p class="empty">No snippets yet. Add one to get started.</p>
          {:else if loaded && !sections.length}
            <p class="empty">Nothing matches “{query}”.</p>
          {/if}
          {#each sections as section (section.group)}
            {#if grouped}
              <button class="group" aria-expanded={!collapsed.has(section.group)} onclick={() => (collapsed.has(section.group) ? collapsed.delete(section.group) : collapsed.add(section.group))}>
                <span class="chev" class:open={!collapsed.has(section.group)}><Glyph name="chevron" size={12} /></span>
                <span class="g-name">{section.group || 'No group'}</span>
                <span class="g-count">{section.items.length}</span>
              </button>
            {/if}
            {#if !grouped || !collapsed.has(section.group)}
              {#each section.items as s (s.id)}
                <button
                  class="item"
                  class:on={s.id === selectedId}
                  role="option"
                  aria-selected={s.id === selectedId}
                  data-snippet={s.id}
                  onclick={() => select(s.id)}
                  animate:reorder
                >
                  <span class="i-name"><Highlight text={s.name || 'Untitled'} words={ws} /></span>
                  {#if s.trigger}<span class="i-trigger"><Highlight text={s.trigger} words={ws} /></span>{/if}
                </button>
              {/each}
            {/if}
          {/each}
        </div>
        <p class="tip"><kbd>Alt</kbd><kbd>↑</kbd><kbd>↓</kbd> moves the selected snippet.</p>
      </div>

      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div class="detail" role="group" aria-label="Edit snippet" onkeydown={detailKey}>
        {#if sel}
          <div class="fields">
            <label class="field">
              <span>Name</span>
              <input type="text" bind:this={nameInput} bind:value={sel.name} oninput={schedule} placeholder="Signature" />
            </label>
            <label class="field">
              <span>Trigger</span>
              <input class="mono" type="text" bind:value={sel.trigger} oninput={schedule} placeholder=";sig" spellcheck="false" />
              {#if clash}<small class="warn">Another snippet uses this trigger.</small>{/if}
            </label>
            <label class="field">
              <span>Group</span>
              <input type="text" bind:value={sel.group} oninput={schedule} placeholder="No group" />
            </label>
          </div>
          {#if groupNames.some((g) => g !== sel.group.trim())}
            <div class="suggest" aria-label="Existing groups">
              {#each groupNames.filter((g) => g !== sel.group.trim()) as g (g)}
                <button class="chip" onclick={() => { sel.group = g; schedule(); }}>{g}</button>
              {/each}
              {#if sel.group.trim()}<button class="chip" onclick={() => { sel.group = ''; schedule(); }}>No group</button>{/if}
            </div>
          {/if}

          <div class="field">
            <div class="text-head">
              <span>Text</span>
              <span class="inserts">
                <button class="ghost small" onclick={() => insertAtCursor('{clipboard}')}>Clipboard</button>
                <button class="ghost small" onclick={() => insertAtCursor('{date}')}>Date</button>
                <button class="ghost small" onclick={() => insertAtCursor('{time}')}>Time</button>
                <button class="ghost small" class:pressed={building} aria-expanded={building} onclick={() => (building = !building)}>
                  <Glyph name="calendar" size={13} />Date and time…
                </button>
              </span>
            </div>
            <textarea
              bind:this={textarea}
              bind:value={sel.text}
              oninput={schedule}
              rows="6"
              spellcheck="true"
              aria-label="Snippet text"
              placeholder="Type the text this snippet inserts."
            ></textarea>
          </div>

          {#if building}
            <div transition:fadeSlide={{ token: 'listReorder', y: -6 }}>
              <DateBuilder oninsert={(t) => insertAtCursor(t)} />
            </div>
          {/if}

          <div class="preview">
            <span class="p-label">Preview</span>
            {#if !sel.text}
              <span class="muted">The expanded text shows here.</span>
            {:else if previewFailed}
              <span class="muted">Preview unavailable.</span>
            {:else}
              <pre>{preview}</pre>
            {/if}
          </div>

          <div class="actions">
            {#if confirmDelete}
              <span class="ask">Delete “{sel.name || 'Untitled'}”?</span>
              <button class="ghost" onclick={() => (confirmDelete = false)}>Cancel</button>
              <button class="ghost danger" onclick={remove}>Delete</button>
            {:else}
              <button class="ghost" onclick={duplicate}><Glyph name="duplicate" size={14} />Duplicate</button>
              <button class="ghost" onclick={() => (confirmDelete = true)}><Glyph name="trash" size={14} />Delete</button>
            {/if}
            <span class="grow"></span>
            {#if saveError}<small class="warn" transition:fadeSlide={{ token: 'toastIn', y: -4 }}>{saveError}</small>{/if}
          </div>
        {:else if loaded}
          <p class="empty">Select a snippet to edit it, or add a new one.</p>
        {/if}
      </div>
    </div>
    <p class="note">Variables: <code>{'{clipboard}'}</code> <code>{'{date}'}</code> <code>{'{time}'}</code> and <code>{'{date:FORMAT@ZONE}'}</code>; type <code>{'{{'}</code> or <code>{'}}'}</code> for a plain brace. Typed triggers arrive in a later update; the snippet menu inserts snippets now.</p>
  {/if}
</div>

<style>
  .editor {
    padding: 14px 16px 12px;
    container-type: inline-size;
  }
  .split {
    display: grid;
    grid-template-columns: minmax(180px, 230px) minmax(0, 1fr);
    gap: 14px;
  }
  @container (max-width: 600px) {
    .split {
      grid-template-columns: minmax(0, 1fr);
    }
    .list {
      max-height: 220px;
    }
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .side-head {
    display: flex;
    gap: 6px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
    padding: 0 8px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--mut);
  }
  .search:focus-within {
    border-color: var(--acc);
    box-shadow: 0 0 0 3px var(--acc-soft);
  }
  .search input {
    flex: 1;
    min-width: 0;
    height: 30px;
    padding: 0;
    border: 0;
    outline: 0;
    background: none;
    color: var(--fg);
    font: inherit;
    font-size: 12.5px;
    user-select: text;
  }
  .add {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-height: 120px;
    max-height: 420px;
    padding: 4px;
    overflow-y: auto;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--bg);
    outline: none;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    padding: 4px 6px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--mut);
    font: inherit;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-align: left;
    text-transform: uppercase;
    cursor: pointer;
  }
  .group:first-child {
    margin-top: 0;
  }
  .group:hover {
    background: var(--hover);
  }
  .chev {
    display: grid;
    transition: transform var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .g-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .g-count {
    font-variant-numeric: tabular-nums;
    letter-spacing: 0;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    padding: 5px 8px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--fg);
    font: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
    transition: background-color var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
  .item:hover {
    background: var(--hover);
  }
  .item.on {
    background: var(--acc-soft);
    font-weight: 600;
  }
  .item:focus-visible,
  .group:focus-visible {
    outline: 2px solid var(--acc);
    outline-offset: -2px;
  }
  .i-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .i-trigger {
    flex: none;
    max-width: 45%;
    overflow: hidden;
    color: var(--mut);
    font: 500 11.5px var(--mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tip {
    margin: 0;
    color: var(--mut);
    font-size: 11.5px;
  }
  kbd {
    padding: 0 4px;
    margin-right: 2px;
    border: 1px solid var(--line-strong);
    border-bottom-width: 2px;
    border-radius: 4px;
    background: var(--surface);
    font: 600 10.5px/1.4 var(--mono);
  }
  .detail {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .fields {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
  }
  @container (max-width: 760px) {
    .fields {
      grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    }
    .fields .field:first-child {
      grid-column: 1 / -1;
    }
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .field > span,
  .text-head > span:first-child,
  .p-label {
    color: var(--mut);
    font-size: 11.5px;
    font-weight: 600;
  }
  .field input,
  textarea {
    width: 100%;
    padding: 6px 9px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    font-size: 13px;
    user-select: text;
  }
  .field input:focus,
  textarea:focus {
    outline: none;
    border-color: var(--acc);
    box-shadow: 0 0 0 3px var(--acc-soft);
  }
  .mono {
    font-family: var(--mono) !important;
  }
  textarea {
    min-height: 120px;
    resize: vertical;
    line-height: 1.5;
  }
  .suggest {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin-top: -4px;
  }
  .chip {
    padding: 2px 9px;
    border: 1px dashed var(--line-strong);
    border-radius: 99px;
    background: none;
    color: var(--fg-soft);
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .chip:hover {
    background: var(--hover);
  }
  .text-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 6px;
  }
  .inserts {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .ghost.small {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 9px;
    font-size: 12px;
  }
  .ghost.pressed {
    border-color: var(--acc);
    background: var(--acc-soft);
    color: var(--fg);
  }
  .preview {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    border-radius: 9px;
    background: var(--inset);
  }
  .preview pre {
    margin: 0;
    max-height: 200px;
    overflow: auto;
    font: inherit;
    font-size: 13px;
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }
  .muted {
    color: var(--mut);
    font-size: 12.5px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .actions .ghost {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .ghost.danger {
    border-color: var(--warn);
    color: var(--warn);
    font-weight: 600;
  }
  .ask {
    font-size: 12.5px;
  }
  .grow {
    flex: 1;
  }
  .warn {
    margin: 0;
    color: var(--warn);
    font-size: 12px;
  }
  .empty {
    margin: 0;
    padding: 18px 8px;
    color: var(--mut);
    font-size: 12.5px;
    text-align: center;
  }
  .note {
    margin: 12px 0 0;
    color: var(--mut);
    font-size: 12px;
  }
  .note code {
    padding: 0 4px;
    border-radius: 4px;
    background: var(--inset);
    color: var(--fg-soft);
    font: 11.5px var(--mono);
  }
</style>
