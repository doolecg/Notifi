<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, tick } from 'svelte';
  import { flip } from 'svelte/animate';
  import { cubicOut } from 'svelte/easing';
  import { applyCssVars, duration, fadeSlide, motion, reducedMotion, setReducedMotion } from '../lib/motion';
  import { store } from '../lib/settings.svelte';
  import { LOOK_LABEL, ago, forget, lookOf, search, title, words, type Entry, type Layout, type ListResult } from './entries';
  import Glyph from './Glyph.svelte';
  import Highlight from './Highlight.svelte';
  import { dropImages } from './images.svelte';
  import Preview from './Preview.svelte';
  import Thumb from './Thumb.svelte';

  const ROW = 52;
  const CARD = 196;
  const GAP = 10;
  const OVERSCAN = 6;

  let card: HTMLElement;
  let input: HTMLInputElement;
  let scroller = $state<HTMLElement>();

  let entries = $state.raw<Entry[]>([]);
  let layout = $state<Layout>('list');
  let autoPaste = $state(true);
  let loaded = $state(false);

  let draft = $state('');
  let composing = false;
  let query = $state('');
  let cursor = $state<number | null>(null);
  let marked = $state.raw(new Set<number>());
  let anchor: number | null = null;

  let focused = $state(typeof document !== 'undefined' && document.hasFocus());
  let warning = $state('');
  let warnTimer: ReturnType<typeof setTimeout> | undefined;
  let confirm = $state<{ count: number; before: number } | null>(null);
  let now = $state(Date.now());
  let reduced = $state(false);
  let hiding = false;

  let scrollPos = $state(0);
  let viewSize = $state(600);

  const shelf = $derived(layout === 'shelf');
  const ws = $derived(words(query));
  const filtered = $derived(search(entries, query));
  const index = $derived(cursor === null ? -1 : filtered.findIndex((e) => e.id === cursor));
  const current = $derived(index >= 0 ? filtered[index] : null);
  const stack = $derived(marked.size > 1 ? filtered.filter((e) => marked.has(e.id)) : []);

  const stride = $derived(shelf ? CARD + GAP : ROW);
  const first = $derived(Math.max(0, Math.floor(scrollPos / stride) - OVERSCAN));
  const last = $derived(Math.min(filtered.length, Math.ceil((scrollPos + viewSize) / stride) + OVERSCAN));
  const visible = $derived(filtered.slice(first, last));

  function warn(message: string) {
    warning = message;
    clearTimeout(warnTimer);
    warnTimer = setTimeout(() => (warning = ''), 5000);
  }

  async function load(keep: boolean) {
    const prevIndex = index;
    let r: ListResult;
    try {
      r = await invoke<ListResult>('clipboard_list');
    } catch (err) {
      warn(String(err));
      return;
    }
    const ids = new Set(r.entries.map((e) => e.id));
    forget(ids);
    dropImages(ids);
    entries = r.entries;
    layout = r.layout;
    autoPaste = r.autoPaste;
    loaded = true;
    if (marked.size) marked = new Set([...marked].filter((id) => ids.has(id)));
    if (!keep || cursor === null || !ids.has(cursor)) {
      const list = search(r.entries, query);
      cursor = (keep ? list[Math.min(Math.max(prevIndex, 0), list.length - 1)] : list[0])?.id ?? null;
    }
  }

  function setQuery(value: string) {
    query = value;
    marked = new Set();
    const list = search(entries, value);
    cursor = list[0]?.id ?? null;
    anchor = cursor;
    if (scroller) scroller.scrollTo({ top: 0, left: 0 });
  }

  function range(a: number | null, b: number): Set<number> {
    const i = Math.max(0, filtered.findIndex((e) => e.id === a));
    const j = filtered.findIndex((e) => e.id === b);
    const [lo, hi] = i < j ? [i, j] : [j, i];
    return new Set(filtered.slice(lo, hi + 1).map((e) => e.id));
  }

  function move(by: number, extend = false) {
    if (!filtered.length) return;
    const to = filtered[Math.min(filtered.length - 1, Math.max(0, (index < 0 ? -1 : index) + by))];
    if (extend) {
      if (!marked.size) anchor = cursor ?? to.id;
      marked = range(anchor, to.id);
      if (marked.size < 2) marked = new Set();
    } else {
      marked = new Set();
      anchor = to.id;
    }
    cursor = to.id;
  }

  function choose(e: MouseEvent, entry: Entry) {
    if (e.ctrlKey || e.metaKey) {
      const next = new Set(marked);
      if (!next.size && cursor !== null && cursor !== entry.id) next.add(cursor);
      if (next.has(entry.id)) next.delete(entry.id);
      else next.add(entry.id);
      cursor = entry.id;
      if (next.size === 1) cursor = [...next][0];
      marked = next.size > 1 ? next : new Set();
      anchor = cursor;
    } else if (e.shiftKey) {
      marked = range(anchor ?? cursor, entry.id);
      if (marked.size < 2) marked = new Set();
      cursor = entry.id;
    } else {
      marked = new Set();
      cursor = entry.id;
      anchor = entry.id;
    }
    input?.focus({ preventScroll: true });
  }

  const chosenIds = () => (stack.length > 1 ? stack.map((e) => e.id) : current ? [current.id] : []);

  function paste(ids = chosenIds(), plain = false) {
    if (!ids.length || hiding) return;
    hiding = true;
    invoke('clipboard_paste', { ids, plain }).catch((err) => {
      hiding = false;
      warn(String(err) || 'Could not copy that. Nothing was pasted.');
    });
  }

  function copy(ids = chosenIds()) {
    if (!ids.length || hiding) return;
    hiding = true;
    invoke('clipboard_copy', { ids }).catch((err) => {
      hiding = false;
      warn(String(err) || 'Could not copy that.');
    });
  }

  function pin(entry: Entry) {
    const pinned = !entry.pinned;
    entries = entries.map((e) => (e.id === entry.id ? { ...e, pinned } : e));
    invoke('clipboard_pin', { id: entry.id, pinned }).catch((err) => {
      warn(String(err));
      load(true);
    });
  }

  function remove(ids = chosenIds()) {
    if (!ids.length) return;
    const gone = new Set(ids);
    const after = filtered.slice(index + 1).find((e) => !gone.has(e.id)) ?? filtered.slice(0, index).reverse().find((e) => !gone.has(e.id));
    entries = entries.filter((e) => !gone.has(e.id));
    marked = new Set();
    cursor = after?.id ?? null;
    anchor = cursor;
    invoke('clipboard_delete', { ids }).catch((err) => {
      warn(String(err));
      load(true);
    });
  }

  async function askClear() {
    try {
      const c = await invoke<{ count: number; before: number }>('clipboard_clear_count');
      if (c.count === 0) warn('There are no unpinned items to clear.');
      else confirm = c;
    } catch (err) {
      warn(String(err));
    }
  }

  async function clear() {
    if (!confirm) return;
    const { before } = confirm;
    confirm = null;
    try {
      await invoke<number>('clipboard_clear_unpinned', { before });
    } catch (err) {
      warn(String(err));
    }
    await load(true);
    input?.focus({ preventScroll: true });
  }

  function close() {
    if (hiding) return;
    hiding = true;
    const r = reducedMotion();
    card.getAnimations().forEach((a) => a.cancel());
    const anim = card.animate([{ opacity: 1, transform: 'none' }, r ? { opacity: 0 } : { opacity: 0, transform: 'scale(0.98)' }], {
      duration: duration('commandBarClose'),
      easing: motion.commandBarClose.ease,
    });
    card.style.opacity = '0';
    anim.finished.then(() => invoke('clipboard_hide')).catch(() => invoke('clipboard_hide'));
  }

  function animateOpen() {
    const r = reducedMotion();
    reduced = r;
    const from = r ? { opacity: 0 } : { opacity: 0, transform: `translateY(${shelf ? 12 : -12}px) scale(0.98)` };
    card.getAnimations().forEach((a) => a.cancel());
    card.style.opacity = '1';
    card.animate([from, { opacity: 1, transform: 'none' }], {
      duration: duration('commandBarOpen'),
      easing: r ? 'linear' : motion.commandBarOpen.ease,
    });
  }

  async function opened(p?: { layout: Layout; autoPaste: boolean }) {
    hiding = false;
    if (p) {
      layout = p.layout;
      autoPaste = p.autoPaste;
    }
    draft = '';
    query = '';
    marked = new Set();
    confirm = null;
    warning = '';
    now = Date.now();
    animateOpen();
    input?.focus();
    await load(false);
    await tick();
    scroller?.scrollTo({ top: 0, left: 0 });
    input?.focus();
  }

  function reorder(node: Element, rects: { from: DOMRect; to: DOMRect }) {
    return flip(node, rects, { duration: duration('listReorder'), easing: cubicOut });
  }

  function onKey(e: KeyboardEvent) {
    if (e.isComposing || e.keyCode === 229) return;
    const ctrl = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();

    if (confirm) {
      if (e.key === 'Escape') {
        e.preventDefault();
        confirm = null;
      } else if (e.key === 'Enter') {
        e.preventDefault();
        clear();
      }
      return;
    }

    const digit = /^(?:Digit|Numpad)([1-9])$/.exec(e.code);
    if (ctrl && !e.altKey && digit) {
      e.preventDefault();
      const entry = filtered[Number(digit[1]) - 1];
      if (entry) paste([entry.id], e.shiftKey);
      return;
    }

    const inSearch = document.activeElement === input;
    const caretInside = inSearch && (input.selectionStart !== input.selectionEnd || (input.selectionStart ?? 0) < input.value.length);

    if (e.key === 'Escape') {
      e.preventDefault();
      close();
    } else if (e.key === 'ArrowDown' || (ctrl && key === 'n')) {
      e.preventDefault();
      move(1, e.shiftKey);
    } else if (e.key === 'ArrowUp' || (ctrl && key === 'p')) {
      e.preventDefault();
      move(-1, e.shiftKey);
    } else if (shelf && (e.key === 'ArrowRight' || e.key === 'ArrowLeft') && (!inSearch || !input.value)) {
      e.preventDefault();
      move(e.key === 'ArrowRight' ? 1 : -1, e.shiftKey);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      paste(chosenIds(), e.shiftKey);
    } else if (e.key === 'Delete' && !caretInside) {
      e.preventDefault();
      remove();
    } else if (ctrl && key === 'c' && !(inSearch && input.selectionStart !== input.selectionEnd)) {
      e.preventDefault();
      copy();
    } else if (!inSearch && e.key.length === 1 && !ctrl && !e.altKey) {
      input.focus();
    }
  }

  $effect(() => {
    const theme = store.settings?.general.theme;
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
  });

  $effect(() => {
    setReducedMotion(store.settings?.general.reducedMotion ?? false);
    reduced = reducedMotion();
  });

  $effect(() => {
    const el = scroller;
    if (!el) return;
    const measure = () => (viewSize = shelf ? el.clientWidth : el.clientHeight);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    const wheel = (e: WheelEvent) => {
      if (!shelf || Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
      e.preventDefault();
      el.scrollLeft += e.deltaY;
    };
    el.addEventListener('wheel', wheel, { passive: false });
    return () => {
      observer.disconnect();
      el.removeEventListener('wheel', wheel);
    };
  });

  $effect(() => {
    const el = scroller;
    if (!el || index < 0) return;
    const start = index * stride;
    const end = start + (shelf ? CARD : ROW);
    const pos = shelf ? el.scrollLeft : el.scrollTop;
    const size = shelf ? el.clientWidth : el.clientHeight;
    const pad = shelf ? 12 : 6;
    let to = pos;
    if (start < pos + pad) to = Math.max(0, start - pad);
    else if (end > pos + size - pad) to = end - size + pad;
    if (to !== pos) el.scrollTo(shelf ? { left: to } : { top: to });
  });

  onMount(() => {
    applyCssVars();
    store.init().catch(() => {});
    load(false).then(() => input?.focus());
    const offs = [
      listen<{ layout: Layout; autoPaste: boolean }>('clipboard://open', (e) => opened(e.payload)),
      listen('clipboard://changed', () => load(true)),
    ];
    const onFocus = () => {
      focused = true;
      if (card.style.opacity === '0' && !hiding) animateOpen();
    };
    const onBlur = () => {
      focused = false;
      if (!hiding && !document.hasFocus()) close();
    };
    const clock = setInterval(() => (now = Date.now()), 30_000);
    window.addEventListener('keydown', onKey);
    window.addEventListener('focus', onFocus);
    window.addEventListener('blur', onBlur);
    return () => {
      offs.forEach((off) => off.then((f) => f()));
      clearInterval(clock);
      clearTimeout(warnTimer);
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('focus', onFocus);
      window.removeEventListener('blur', onBlur);
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" onmousedown={(e) => e.target === e.currentTarget && close()}>
  <div class="card" class:shelf class:reduced bind:this={card} role="dialog" aria-label="Clipboard history">
    <header>
      <label class="search">
        <Glyph name="search" size={16} />
        <input
          bind:this={input}
          value={draft}
          type="text"
          placeholder="Search clipboard"
          aria-label="Search clipboard"
          aria-controls="clip-list"
          aria-activedescendant={current ? `clip-${current.id}` : undefined}
          spellcheck="false"
          autocomplete="off"
          oninput={(e) => {
            draft = e.currentTarget.value;
            if (!composing) setQuery(draft);
          }}
          oncompositionstart={() => (composing = true)}
          oncompositionend={(e) => {
            composing = false;
            draft = e.currentTarget.value;
            setQuery(draft);
          }}
        />
        {#if draft}
          <button class="clear-search" title="Clear search" aria-label="Clear search" onclick={() => { draft = ''; setQuery(''); input.focus(); }}>
            <Glyph name="close" size={13} />
          </button>
        {/if}
      </label>
      {#if shelf}
        <span class="count">{query ? `${filtered.length} of ${entries.length}` : `${entries.length} ${entries.length === 1 ? 'item' : 'items'}`}</span>
        <span class="hints">
          <span><kbd>Enter</kbd>{autoPaste ? 'Paste' : 'Copy'}</span>
          <span><kbd>Ctrl</kbd><kbd>1-9</kbd>Quick</span>
          <span><kbd>Del</kbd>Delete</span>
        </span>
        <button class="foot-btn" onclick={askClear} disabled={!entries.some((e) => !e.pinned)}>Clear unpinned</button>
      {/if}
    </header>

    <div class="main">
      <div
        class="scroller"
        id="clip-list"
        role="listbox"
        aria-label="Copied items"
        aria-multiselectable="true"
        bind:this={scroller}
        onscroll={(e) => (scrollPos = shelf ? e.currentTarget.scrollLeft : e.currentTarget.scrollTop)}
      >
        {#if loaded && !filtered.length}
          <div class="empty">
            <span class="empty-mark"><Glyph name={query ? 'search' : 'clipboard'} size={22} /></span>
            {#if query}
              <strong>No matches for “{query}”</strong>
              <span>Try fewer or different words.</span>
            {:else}
              <strong>Nothing copied yet</strong>
              <span>Copy some text, an image or files and they appear here.</span>
            {/if}
          </div>
        {:else}
          <div
            class="track"
            style:height={shelf ? '100%' : `${filtered.length * ROW}px`}
            style:width={shelf ? `${filtered.length * stride - GAP}px` : '100%'}
          >
            <div class="window" style:transform={shelf ? `translateX(${first * stride}px)` : `translateY(${first * ROW}px)`}>
              {#each visible as entry, i (entry.id)}
                {@const n = first + i}
                {@const look = lookOf(entry)}
                {@const isCursor = entry.id === cursor}
                {@const isMarked = marked.has(entry.id)}
                <div
                  class="item"
                  class:cursor={isCursor}
                  class:marked={isMarked}
                  class:multi={marked.size > 1}
                  id="clip-{entry.id}"
                  role="option"
                  tabindex="-1"
                  aria-selected={isCursor || isMarked}
                  onclick={(e) => choose(e, entry)}
                  ondblclick={() => paste([entry.id])}
                  onkeydown={() => {}}
                  animate:reorder
                >
                  {#if shelf}
                    <div class="c-top">
                      <span class="c-kind">{LOOK_LABEL[look]}{#if entry.kind === 'rich' && look !== 'rich'}<span class="fmt">Formatted</span>{/if}</span>
                      <button class="pin" class:on={entry.pinned} aria-pressed={entry.pinned} title={entry.pinned ? 'Unpin' : 'Pin'} aria-label={entry.pinned ? 'Unpin' : 'Pin'} onclick={(e) => { e.stopPropagation(); pin(entry); }}>
                        <Glyph name="pin" size={14} />
                      </button>
                    </div>
                    <div class="c-body" class:media={look === 'image' || look === 'color'}>
                      {#if look === 'image' || look === 'color'}
                        <Thumb {entry} size={200} />
                        {#if look === 'color'}<code class="c-code">{entry.text.trim()}</code>{/if}
                      {:else}
                        <p class:mono={look === 'json'}><Highlight text={look === 'files' ? entry.text : title(entry)} words={ws} around /></p>
                      {/if}
                    </div>
                    <div class="c-meta">
                      {#if entry.app}<span class="app"><Highlight text={entry.app} words={ws} /></span><span class="dot">·</span>{/if}
                      <span class="time">{ago(entry.created, now)}</span>
                    </div>
                  {:else}
                    <Thumb {entry} size={34} />
                    <div class="r-main">
                      <span class="r-title"><Highlight text={title(entry) || ' '} words={ws} around /></span>
                      <span class="r-meta">
                        {#if entry.kind === 'rich'}<span class="fmt">Formatted</span>{/if}
                        {#if entry.app}<span class="app"><Highlight text={entry.app} words={ws} /></span><span class="dot">·</span>{/if}
                        <span class="time">{ago(entry.created, now)}</span>
                      </span>
                    </div>
                    <button class="pin" class:on={entry.pinned} aria-pressed={entry.pinned} title={entry.pinned ? 'Unpin' : 'Pin'} aria-label={entry.pinned ? 'Unpin' : 'Pin'} onclick={(e) => { e.stopPropagation(); pin(entry); }}>
                      <Glyph name="pin" size={14} />
                    </button>
                  {/if}
                  {#if isMarked}
                    <span class="badge check"><svg viewBox="0 0 24 24" width="11" height="11" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg></span>
                  {:else if focused && n < 9}
                    <span class="badge" aria-hidden="true">{n + 1}</span>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      {#if !shelf}
        <aside class="pane">
          {#if current || stack.length}
            <Preview
              entry={current}
              {stack}
              words={ws}
              {autoPaste}
              onpaste={() => paste()}
              oncopy={() => copy()}
              onpin={() => current && pin(current)}
              ondelete={() => remove()}
            />
          {/if}
        </aside>
      {/if}
    </div>

    {#if !shelf}
      <footer>
        <span class="count">{query ? `${filtered.length} of ${entries.length}` : `${entries.length} ${entries.length === 1 ? 'item' : 'items'}`}</span>
        <span class="hints">
          <span><kbd>Enter</kbd>{autoPaste ? 'Paste' : 'Copy'}</span>
          <span><kbd>Shift</kbd><kbd>Enter</kbd>Plain</span>
          <span><kbd>Ctrl</kbd><kbd>1-9</kbd>Quick</span>
          <span><kbd>Del</kbd>Delete</span>
        </span>
        <button class="foot-btn" onclick={askClear} disabled={!entries.some((e) => !e.pinned)}>Clear unpinned</button>
      </footer>
    {/if}

    {#if confirm}
      <div class="sheet" role="alertdialog" aria-label="Clear unpinned" transition:fadeSlide={{ token: 'toastIn', y: 8 }}>
        <Glyph name="trash" size={16} />
        <span class="sheet-text">
          Remove {confirm.count.toLocaleString()} unpinned {confirm.count === 1 ? 'item' : 'items'}?
          <small>Pinned items stay, and anything copied now is kept.</small>
        </span>
        <button class="foot-btn" onclick={() => (confirm = null)}>Cancel</button>
        <button class="foot-btn danger" onclick={clear}>Remove {confirm.count.toLocaleString()}</button>
      </div>
    {:else if warning}
      <button class="sheet warn" onclick={() => (warning = '')} transition:fadeSlide={{ token: 'toastIn', y: 8 }}>
        <Glyph name="warning" size={16} />
        <span class="sheet-text">{warning}</span>
      </button>
    {/if}
  </div>
</div>

<style>
  :global(html),
  :global(body.clipboard-page) {
    background: transparent;
  }
  .backdrop {
    position: fixed;
    inset: 0;
  }
  .card {
    position: absolute;
    inset: 18px;
    display: flex;
    flex-direction: column;
    min-width: 0;
    border: 1px solid var(--line-strong);
    border-radius: 14px;
    background: var(--bg);
    box-shadow:
      0 16px 40px rgb(0 0 0 / 0.24),
      0 2px 6px rgb(0 0 0 / 0.12);
    overflow: hidden;
    container-type: size;
    transform-origin: 50% 0;
  }
  .card.shelf {
    transform-origin: 50% 100%;
  }

  header {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: none;
    padding: 10px 10px 8px;
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    height: 38px;
    padding: 0 8px 0 12px;
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    background: var(--surface);
    color: var(--mut);
    transition: border-color var(--m-hoverLift-dur) var(--m-hoverLift-ease),
      box-shadow var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .search:focus-within {
    border-color: var(--acc);
    box-shadow: 0 0 0 3px var(--acc-soft);
  }
  .search input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    outline: 0;
    background: none;
    color: var(--fg);
    font: inherit;
    font-size: 14.5px;
    user-select: text;
  }
  .search input::placeholder {
    color: var(--mut);
  }
  .clear-search {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--inset);
    color: var(--mut);
    cursor: pointer;
  }
  .clear-search:hover {
    color: var(--fg);
  }
  .shelf .search {
    flex: 0 1 340px;
  }

  .main {
    display: flex;
    flex: 1;
    min-height: 0;
    border-top: 1px solid var(--line);
  }
  .scroller {
    position: relative;
    flex: 0 0 min(400px, 46%);
    min-width: 0;
    overflow-y: auto;
    overflow-x: hidden;
    overscroll-behavior: contain;
    outline: none;
  }
  .shelf .scroller {
    flex: 1;
    overflow-x: auto;
    overflow-y: hidden;
    padding: 12px;
    border-top: 0;
  }
  .shelf .main {
    border-top: 0;
  }
  .track {
    position: relative;
  }
  .window {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
  }
  .shelf .window {
    right: auto;
    bottom: 0;
    display: flex;
    gap: 10px;
  }

  .item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 52px;
    padding: 0 10px 0 12px;
    cursor: default;
    transition: background-color var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
  .item:hover {
    background: var(--hover);
  }
  .item.cursor,
  .item.marked {
    background: var(--acc-soft);
  }
  .item.cursor::before {
    content: '';
    position: absolute;
    top: 8px;
    bottom: 8px;
    left: 3px;
    width: 3px;
    border-radius: 3px;
    background: var(--acc);
  }
  .r-main {
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
    gap: 1px;
  }
  .r-title {
    overflow: hidden;
    font-size: 13.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .r-meta,
  .c-meta {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    color: var(--mut);
    font-size: 11.5px;
    white-space: nowrap;
  }
  .app {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .time {
    flex: none;
  }
  .dot {
    flex: none;
    opacity: 0.6;
  }
  .fmt {
    flex: none;
    padding: 0 5px;
    border-radius: 4px;
    background: var(--inset);
    color: var(--fg-soft);
    font-size: 10.5px;
    font-weight: 600;
  }

  .pin {
    display: grid;
    place-items: center;
    flex: none;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--mut);
    opacity: 0;
    cursor: pointer;
    transition:
      opacity var(--m-hoverLift-dur) var(--m-hoverLift-ease),
      background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .item:hover .pin,
  .item.cursor .pin,
  .pin:focus-visible {
    opacity: 1;
  }
  .pin:hover {
    background: var(--hover);
    color: var(--fg);
  }
  .pin.on {
    color: var(--acc);
    opacity: 1;
  }
  .pin.on :global(path) {
    fill: currentColor;
  }

  .badge {
    display: grid;
    place-items: center;
    flex: none;
    min-width: 20px;
    height: 20px;
    padding: 0 5px;
    border: 1px solid var(--line-strong);
    border-bottom-width: 2px;
    border-radius: 6px;
    background: var(--surface);
    color: var(--mut);
    font: 600 11px/1 var(--mono);
  }
  .badge.check {
    border: 0;
    border-radius: 50%;
    background: var(--acc);
  }
  .badge.check path {
    fill: none;
    stroke: var(--acc-fg);
    stroke-width: 3;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .shelf .item {
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    flex: none;
    width: 196px;
    height: 100%;
    padding: 8px 10px 9px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--surface);
    transition:
      border-color var(--m-selectionMove-dur) var(--m-selectionMove-ease),
      background-color var(--m-selectionMove-dur) var(--m-selectionMove-ease),
      transform var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .shelf .item:hover {
    transform: scale(1.03);
  }
  .shelf.reduced .item:hover {
    transform: none;
  }
  .shelf .item.cursor {
    border-color: var(--acc);
    box-shadow: 0 0 0 2px var(--acc-soft);
  }
  .shelf .item.cursor::before {
    content: none;
  }
  .shelf .item.marked {
    border-color: var(--acc);
    background: var(--acc-soft);
  }
  .shelf .badge {
    position: absolute;
    right: 8px;
    bottom: 8px;
  }
  .c-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 24px;
  }
  .c-kind {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--mut);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }
  .c-kind .fmt {
    letter-spacing: 0;
    text-transform: none;
  }
  .c-body {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    mask-image: linear-gradient(#000 calc(100% - 18px), transparent);
  }
  .c-body.media {
    mask-image: none;
  }
  .c-body.media {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .c-body.media > :global(.thumb) {
    width: 100% !important;
    height: auto !important;
    flex: 1;
    min-height: 0;
  }
  .c-body p {
    display: -webkit-box;
    margin: 0;
    overflow: hidden;
    font-size: 13px;
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 8;
    line-clamp: 8;
  }
  .c-body p.mono {
    font: 12px/1.45 var(--mono);
  }
  .c-code {
    font: 600 12px var(--mono);
  }
  .c-meta {
    padding-right: 26px;
  }

  .pane {
    flex: 1;
    min-width: 0;
    border-left: 1px solid var(--line);
    background: var(--surface);
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    height: 100%;
    min-height: 160px;
    padding: 24px;
    color: var(--mut);
    font-size: 12.5px;
    text-align: center;
  }
  .empty strong {
    color: var(--fg);
    font-size: 14px;
  }
  .empty-mark {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    margin-bottom: 6px;
    border-radius: 12px;
    background: var(--acc-soft);
    color: var(--acc);
  }

  footer {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: none;
    height: 42px;
    padding: 0 10px 0 14px;
    border-top: 1px solid var(--line);
  }
  .count {
    flex: none;
    color: var(--mut);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .hints {
    display: flex;
    gap: 12px;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--mut);
    font-size: 11.5px;
    white-space: nowrap;
  }
  .shelf .hints {
    justify-content: flex-end;
  }
  .hints > span {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  kbd {
    padding: 0 5px;
    border: 1px solid var(--line-strong);
    border-bottom-width: 2px;
    border-radius: 5px;
    background: var(--surface);
    color: var(--fg-soft);
    font: 600 10.5px/1.5 var(--mono);
  }
  .foot-btn {
    flex: none;
    height: 28px;
    padding: 0 11px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg-soft);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .foot-btn:hover:not(:disabled) {
    background: var(--hover);
    color: var(--fg);
  }
  .foot-btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .foot-btn.danger {
    border-color: var(--warn);
    background: var(--warn);
    color: #fff;
    font-weight: 600;
  }
  .foot-btn.danger:hover {
    background: var(--warn);
    filter: brightness(1.08);
  }
  .foot-btn:focus-visible,
  .clear-search:focus-visible {
    outline: 2px solid var(--acc);
    outline-offset: 1px;
  }

  .sheet {
    position: absolute;
    left: 10px;
    right: 10px;
    bottom: 10px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--line-strong);
    border-radius: 11px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    font-size: 13px;
    text-align: left;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.18);
  }
  .sheet.warn {
    border-color: var(--warn);
    color: var(--warn);
    cursor: pointer;
  }
  .sheet-text {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .sheet-text small {
    color: var(--mut);
    font-size: 11.5px;
  }
  .shelf .sheet {
    left: auto;
    top: 10px;
    bottom: auto;
    max-width: 520px;
  }

  @container (max-width: 560px) {
    .pane {
      display: none;
    }
    .scroller {
      flex: 1;
    }
  }
  @container (max-width: 470px) {
    .hints {
      visibility: hidden;
    }
  }
  @container (max-width: 640px) {
    .shelf .hints {
      display: none;
    }
  }
  @container (max-height: 150px) {
    .shelf .c-body p {
      -webkit-line-clamp: 2;
      line-clamp: 2;
    }
  }
</style>
