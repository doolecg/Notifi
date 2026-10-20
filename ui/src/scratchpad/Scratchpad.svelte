<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, tick } from 'svelte';
  import { applyCssVars, setReducedMotion } from '../lib/motion';
  import { store } from '../lib/settings.svelte';
  import { continuation, render, toggleTask } from './markdown';

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

  const LARGE = 5 * 1024 * 1024;

  let notes = $state<Note[]>([{ id: 1, title: '1', text: '' }]);
  let active = $state(1);
  let next = $state(2);
  let pinned = $state(false);
  let preview = $state(false);
  let showFormat = $state(false);
  let showFind = $state(false);
  let showMenu = $state(false);
  let find = $state('');
  let findFrom = 0;
  let closing = $state<number | null>(null);
  let warning = $state('');
  let info = $state('');
  let editor = $state<HTMLTextAreaElement>();
  let findInput = $state<HTMLInputElement>();
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let loaded = false;
  let lastSaveFailed = false;

  const values = $derived(store.values('scratchpad'));
  const textSize = $derived(typeof values.textSize === 'number' ? values.textSize : 14);
  const opacity = $derived(typeof values.opacity === 'number' ? values.opacity : 90);
  const spell = $derived(values.spellCheck !== false);
  const current = $derived(notes.find((n) => n.id === active) ?? notes[0]);
  const large = $derived((current?.text.length ?? 0) > LARGE);
  const showPreview = $derived(preview && !large);
  const html = $derived(showPreview ? render(current.text) : '');
  const words = $derived(current.text.trim() ? current.text.trim().split(/\s+/).length : 0);

  function snapshot(): Saved {
    return { notes: $state.snapshot(notes) as Note[], active, next };
  }

  async function saveNow() {
    clearTimeout(saveTimer);
    if (!loaded) return;
    try {
      await invoke('scratchpad_save', { value: snapshot() });
      lastSaveFailed = false;
      if (warning.startsWith('Could not save')) warning = '';
    } catch (err) {
      lastSaveFailed = true;
      warning = `Could not save your notes: ${String(err)}`;
    }
  }

  function changed() {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(saveNow, 350);
  }

  async function load() {
    try {
      const r = await invoke<{ state: Saved; warning: string | null; pinned: boolean }>('scratchpad_load');
      notes = r.state.notes;
      active = notes.some((n) => n.id === r.state.active) ? r.state.active : notes[0].id;
      next = r.state.next ?? Math.max(...notes.map((n) => n.id)) + 1;
      pinned = r.pinned;
      if (r.warning) warning = r.warning;
    } catch (err) {
      warning = String(err);
    }
    loaded = true;
    await tick();
    editor?.focus();
  }

  function addNote() {
    const used = new Set(notes.map((n) => n.title));
    let n = 1;
    while (used.has(String(n))) n++;
    notes.push({ id: next, title: String(n), text: '' });
    active = next;
    next += 1;
    changed();
    tick().then(() => editor?.focus());
  }

  function askClose(id: number) {
    const note = notes.find((n) => n.id === id);
    if (!note) return;
    if (note.text.trim() && closing !== id) {
      closing = id;
      return;
    }
    removeNote(id);
  }

  function removeNote(id: number) {
    closing = null;
    const i = notes.findIndex((n) => n.id === id);
    if (i < 0) return;
    notes.splice(i, 1);
    if (!notes.length) {
      notes.push({ id: next, title: '1', text: '' });
      next += 1;
    }
    if (active === id) active = notes[Math.min(i, notes.length - 1)].id;
    changed();
    tick().then(() => editor?.focus());
  }

  function setText(text: string, from: number, to = from) {
    current.text = text;
    changed();
    tick().then(() => {
      editor?.focus();
      editor?.setSelectionRange(from, to);
    });
  }

  function wrap(before: string, after = before) {
    if (!editor) return;
    const { selectionStart: a, selectionEnd: b, value } = editor;
    const chosen = value.slice(a, b);
    setText(value.slice(0, a) + before + chosen + after + value.slice(b), a + before.length, b + before.length);
  }

  function prefix(marker: string) {
    if (!editor) return;
    const { selectionStart: a, selectionEnd: b, value } = editor;
    const start = value.lastIndexOf('\n', a - 1) + 1;
    const endIdx = value.indexOf('\n', b);
    const end = endIdx < 0 ? value.length : endIdx;
    const block = value.slice(start, end).split('\n').map((l) => marker + l).join('\n');
    setText(value.slice(0, start) + block + value.slice(end), a + marker.length, b + marker.length * (block.split('\n').length));
  }

  function link() {
    if (!editor) return;
    const { selectionStart: a, selectionEnd: b, value } = editor;
    const chosen = value.slice(a, b) || 'text';
    const text = `[${chosen}](https://)`;
    setText(value.slice(0, a) + text + value.slice(b), a + chosen.length + 3, a + text.length - 1);
  }

  function moveLine(dir: -1 | 1) {
    if (!editor) return;
    const { selectionStart: a, value } = editor;
    const lines = value.split('\n');
    let pos = 0;
    let idx = 0;
    for (; idx < lines.length; idx++) {
      if (a <= pos + lines[idx].length) break;
      pos += lines[idx].length + 1;
    }
    const to = idx + dir;
    if (to < 0 || to >= lines.length) return;
    const col = a - pos;
    [lines[idx], lines[to]] = [lines[to], lines[idx]];
    const newPos = lines.slice(0, to).reduce((n, l) => n + l.length + 1, 0) + Math.min(col, lines[to].length);
    setText(lines.join('\n'), newPos);
  }

  function findNext(dir: 1 | -1 = 1) {
    if (!editor || !find) return;
    const hay = current.text.toLowerCase();
    const needle = find.toLowerCase();
    let at = dir === 1 ? hay.indexOf(needle, findFrom) : hay.lastIndexOf(needle, Math.max(findFrom - needle.length - 1, 0));
    if (at < 0) at = dir === 1 ? hay.indexOf(needle) : hay.lastIndexOf(needle);
    if (at < 0) return;
    findFrom = dir === 1 ? at + needle.length : at;
    if (preview) preview = false;
    tick().then(() => {
      editor?.setSelectionRange(at, at + needle.length);
      const line = current.text.slice(0, at).split('\n').length - 1;
      editor?.scrollTo({ top: Math.max(line * textSize * 1.55 - 60, 0) });
    });
  }

  async function openFind() {
    showFind = true;
    const sel = editor && editor.selectionStart !== editor.selectionEnd ? editor.value.slice(editor.selectionStart, editor.selectionEnd) : '';
    if (sel && !sel.includes('\n')) find = sel;
    await tick();
    findInput?.focus();
    findInput?.select();
  }

  function closeFind() {
    showFind = false;
    editor?.focus();
  }

  function onEditorKey(e: KeyboardEvent) {
    if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      e.preventDefault();
      moveLine(e.key === 'ArrowUp' ? -1 : 1);
    } else if (e.key === 'Enter' && e.shiftKey && editor) {
      const { selectionStart: a, value } = editor;
      const start = value.lastIndexOf('\n', a - 1) + 1;
      const more = continuation(value.slice(start, a));
      if (more) {
        e.preventDefault();
        setText(value.slice(0, a) + '\n' + more + value.slice(editor.selectionEnd), a + 1 + more.length);
      }
    } else if ((e.ctrlKey || e.metaKey) && (e.key === 'b' || e.key === 'i')) {
      e.preventDefault();
      wrap(e.key === 'b' ? '**' : '*');
    }
  }

  function onKey(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === 't') {
      e.preventDefault();
      addNote();
    } else if (mod && e.key.toLowerCase() === 'w') {
      e.preventDefault();
      askClose(current.id);
    } else if (mod && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      openFind();
    } else if (e.key === 'Escape' && !e.isComposing) {
      e.preventDefault();
      if (showMenu) showMenu = false;
      else if (closing !== null) closing = null;
      else if (showFind) closeFind();
      else hide();
    }
  }

  function hide() {
    saveNow();
    invoke('scratchpad_hide').catch(() => {});
  }

  async function togglePin() {
    pinned = !pinned;
    await invoke('scratchpad_pin', { pinned });
  }

  function togglePreview() {
    preview = !preview;
    store.setOption('scratchpad', 'preview', preview).catch(() => {});
  }

  function onPreviewClick(e: MouseEvent) {
    const box = e.target as HTMLInputElement;
    if (box.tagName !== 'INPUT' || box.dataset.line === undefined) return;
    current.text = toggleTask(current.text, Number(box.dataset.line));
    changed();
  }

  async function exportNote(extension: 'md' | 'txt') {
    showMenu = false;
    try {
      const path = await invoke<string>('scratchpad_export', { name: `Scratchpad ${current.title}`, text: current.text, extension });
      flash(`Saved to ${path}`);
    } catch (err) {
      warning = `Could not export: ${String(err)}`;
    }
  }

  async function copyAll() {
    showMenu = false;
    try {
      await navigator.clipboard.writeText(current.text);
      flash('Copied');
    } catch {
      editor?.select();
      document.execCommand('copy');
      flash('Copied');
    }
  }

  let infoTimer: ReturnType<typeof setTimeout> | undefined;
  function flash(message: string) {
    info = message;
    clearTimeout(infoTimer);
    infoTimer = setTimeout(() => (info = ''), 4000);
  }

  onMount(() => {
    applyCssVars();
    store.init().then(() => (preview = values.preview === true)).catch(() => {});
    load();
    const offs = [
      listen('scratchpad://open', () => {
        closing = null;
        showMenu = false;
        tick().then(() => editor?.focus());
        if (lastSaveFailed) warning = 'The last save failed. Your changes are not stored yet.';
      }),
      listen('scratchpad://closing', () => {
        if (lastSaveFailed) warning = 'The last save failed. Your changes may not be stored.';
        saveNow();
      }),
    ];
    window.addEventListener('keydown', onKey);
    window.addEventListener('blur', saveNow);
    return () => {
      offs.forEach((off) => off.then((f) => f()));
      clearTimeout(saveTimer);
      clearTimeout(infoTimer);
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('blur', saveNow);
    };
  });

  $effect(() => {
    const theme = store.settings?.general.theme;
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
    setReducedMotion(store.settings?.general.reducedMotion ?? false);
  });
</script>

<div class="pad" style:--bgmix="{opacity}%" style:--size="{textSize}px">
  <header data-tauri-drag-region>
    <div class="tabs" data-tauri-drag-region role="tablist">
      {#each notes as note (note.id)}
        <div class="tab" class:on={note.id === active} role="presentation">
          <button class="name" role="tab" aria-selected={note.id === active} onclick={() => (active = note.id)}>{note.title}</button>
          <button class="x" aria-label="Close note {note.title}" onclick={() => askClose(note.id)}>
            <svg width="10" height="10" viewBox="0 0 10 10"><path d="M2 2l6 6M8 2l-6 6" /></svg>
          </button>
        </div>
      {/each}
      <button class="add" aria-label="New note" title="New note (Ctrl+T)" onclick={addNote}>
        <svg width="12" height="12" viewBox="0 0 12 12"><path d="M6 2v8M2 6h8" /></svg>
      </button>
    </div>
    <div class="tools">
      <button class="tool" class:on={showFormat} title="Formatting" aria-label="Formatting" onclick={() => (showFormat = !showFormat)}>Aa</button>
      <button class="tool" class:on={showPreview} disabled={large} title={large ? 'Preview is off for very large notes' : 'Markdown preview'} aria-label="Preview" onclick={togglePreview}>
        <svg width="14" height="14" viewBox="0 0 24 24"><path d="M3 12s3.5-6 9-6 9 6 9 6-3.5 6-9 6-9-6-9-6Z M12 9.5a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5Z" /></svg>
      </button>
      <button class="tool" class:on={pinned} title={pinned ? 'Unpin' : 'Keep open'} aria-label="Pin" aria-pressed={pinned} onclick={togglePin}>
        <svg width="14" height="14" viewBox="0 0 24 24"><path d="M9 4h6l-1 6 3 3H7l3-3-1-6Z M12 13v7" /></svg>
      </button>
      <div class="menu-wrap">
        <button class="tool" title="More" aria-label="More" aria-expanded={showMenu} onclick={() => (showMenu = !showMenu)}>
          <svg width="14" height="14" viewBox="0 0 24 24"><path d="M5 12h.01M12 12h.01M19 12h.01" /></svg>
        </button>
        {#if showMenu}
          <div class="menu" role="menu">
            <button role="menuitem" onclick={() => exportNote('md')}>Save as Markdown</button>
            <button role="menuitem" onclick={() => exportNote('txt')}>Save as text</button>
            <button role="menuitem" onclick={copyAll}>Copy all</button>
            <button role="menuitem" onclick={() => { showMenu = false; openFind(); }}>Find (Ctrl+F)</button>
          </div>
        {/if}
      </div>
      <button class="tool" title="Close" aria-label="Close" onclick={hide}>
        <svg width="12" height="12" viewBox="0 0 10 10"><path d="M2 2l6 6M8 2l-6 6" /></svg>
      </button>
    </div>
  </header>

  {#if showFormat}
    <div class="format" role="toolbar" aria-label="Formatting">
      <button title="Bold (Ctrl+B)" onclick={() => wrap('**')}><b>B</b></button>
      <button title="Italic (Ctrl+I)" onclick={() => wrap('*')}><i>I</i></button>
      <button title="Heading" onclick={() => prefix('# ')}>H</button>
      <button title="List" onclick={() => prefix('- ')}>List</button>
      <button title="Task" onclick={() => prefix('- [ ] ')}>Task</button>
      <button title="Link" onclick={link}>Link</button>
      <button title="Code" onclick={() => wrap('`')}>Code</button>
    </div>
  {/if}

  {#if showFind}
    <div class="find">
      <input
        bind:this={findInput}
        bind:value={find}
        placeholder="Find in this note"
        spellcheck="false"
        oninput={() => (findFrom = 0)}
        onkeydown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            findNext(e.shiftKey ? -1 : 1);
          }
        }}
      />
      <button onclick={() => findNext(-1)} aria-label="Previous">Prev</button>
      <button onclick={() => findNext(1)} aria-label="Next">Next</button>
      <button onclick={closeFind} aria-label="Close find">Done</button>
    </div>
  {/if}

  {#if closing !== null}
    <div class="ask" role="alertdialog">
      <span>Close this note? Its text will be lost.</span>
      <button onclick={() => removeNote(closing!)}>Close note</button>
      <button onclick={() => (closing = null)}>Keep</button>
    </div>
  {/if}

  <main>
    {#if showPreview}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="preview" onclick={onPreviewClick}>{@html html || '<p class="hint">Nothing to preview yet.</p>'}</div>
    {:else}
      <textarea
        bind:this={editor}
        bind:value={current.text}
        oninput={changed}
        onkeydown={onEditorKey}
        spellcheck={spell}
        placeholder="Start typing. Notes save themselves."
      ></textarea>
    {/if}
  </main>

  {#if warning || info}
    <footer class:warn={!!warning}>
      <span>{warning || info}</span>
      {#if warning}<button onclick={() => (warning = '')} aria-label="Dismiss">Dismiss</button>{/if}
    </footer>
  {:else}
    <footer class="quiet"><span>{words} {words === 1 ? 'word' : 'words'}</span></footer>
  {/if}
</div>

<style>
  :global(html),
  :global(body.scratchpad-page) {
    background: transparent;
  }
  .pad {
    display: flex;
    flex-direction: column;
    height: 100%;
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    background: color-mix(in srgb, var(--bg) var(--bgmix), transparent);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.2);
    overflow: hidden;
  }
  header {
    display: flex;
    flex: none;
    align-items: center;
    gap: 6px;
    min-height: 36px;
    padding: 4px 6px 4px 8px;
    border-bottom: 1px solid var(--line);
  }
  .tabs {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 2px;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tab {
    display: flex;
    flex: none;
    align-items: center;
    border-radius: 7px;
    color: var(--mut);
  }
  .tab.on {
    background: var(--inset);
    color: var(--fg);
  }
  .tab .name {
    padding: 4px 4px 4px 10px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .x,
  .add,
  .tool {
    display: grid;
    place-items: center;
    min-width: 22px;
    height: 22px;
    padding: 0 4px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--mut);
    font: inherit;
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
  }
  .x:hover,
  .add:hover,
  .tool:hover:not(:disabled) {
    background: var(--hover);
    color: var(--fg);
  }
  .tool:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .tool.on {
    background: var(--acc-soft);
    color: var(--acc);
  }
  svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.7;
    stroke-linecap: round;
    stroke-linejoin: round;
    pointer-events: none;
  }
  .tools {
    display: flex;
    flex: none;
    align-items: center;
    gap: 2px;
  }
  .menu-wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    top: 26px;
    right: 0;
    z-index: 5;
    display: flex;
    min-width: 150px;
    flex-direction: column;
    padding: 4px;
    border: 1px solid var(--line-strong);
    border-radius: 9px;
    background: var(--surface);
    box-shadow: 0 6px 18px rgb(0 0 0 / 0.22);
  }
  .menu button {
    padding: 6px 10px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--fg);
    font: inherit;
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
  }
  .menu button:hover {
    background: var(--hover);
  }
  .format,
  .find,
  .ask {
    display: flex;
    flex: none;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    padding: 5px 8px;
    border-bottom: 1px solid var(--line);
    font-size: 12.5px;
  }
  .format button,
  .find button,
  .ask button {
    padding: 2px 9px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: var(--surface);
    color: var(--fg-soft);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .format button:hover,
  .find button:hover,
  .ask button:hover {
    background: var(--hover);
  }
  .find input {
    flex: 1;
    min-width: 80px;
    padding: 3px 8px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    font-size: 12.5px;
    user-select: text;
  }
  .ask span {
    flex: 1;
    color: var(--warn);
  }
  main {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  textarea,
  .preview {
    width: 100%;
    height: 100%;
    margin: 0;
    padding: 10px 12px;
    border: 0;
    background: none;
    color: var(--fg);
    font: inherit;
    font-size: var(--size);
    line-height: 1.55;
    user-select: text;
  }
  textarea {
    display: block;
    resize: none;
    outline: none;
  }
  .preview {
    overflow: auto;
  }
  .preview :global(h1),
  .preview :global(h2),
  .preview :global(h3) {
    margin: 0.6em 0 0.3em;
    line-height: 1.25;
  }
  .preview :global(p),
  .preview :global(ul),
  .preview :global(ol) {
    margin: 0.4em 0;
  }
  .preview :global(code) {
    padding: 0 4px;
    border-radius: 4px;
    background: var(--inset);
    font-family: var(--mono);
    font-size: 0.92em;
  }
  .preview :global(pre) {
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--inset);
    overflow-x: auto;
  }
  .preview :global(blockquote) {
    margin: 0.4em 0;
    padding-left: 10px;
    border-left: 3px solid var(--line-strong);
    color: var(--fg-soft);
  }
  .preview :global(a) {
    color: var(--acc);
  }
  .preview :global(ul.tasks) {
    padding-left: 4px;
    list-style: none;
  }
  .preview :global(li.task) {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .preview :global(li.done span) {
    color: var(--mut);
    text-decoration: line-through;
  }
  .preview :global(.hint) {
    color: var(--mut);
  }
  footer {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 4px 10px;
    border-top: 1px solid var(--line);
    font-size: 11.5px;
  }
  footer.quiet {
    color: var(--mut);
  }
  footer.warn {
    color: var(--warn);
  }
  footer button {
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-decoration: underline;
    cursor: pointer;
  }
</style>
