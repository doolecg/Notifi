<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount, tick } from 'svelte';
  import { applyCssVars, duration, motion, reducedMotion, setReducedMotion } from '../lib/motion';
  import { store } from '../lib/settings.svelte';
  import { glyphPath, isGlyph, type OpenPayload, type Outcome, type Row } from './rows';

  const MARGIN = 20;

  let card: HTMLElement;
  let input: HTMLInputElement;
  let list = $state<HTMLElement>();

  let query = $state('');
  let rows = $state.raw<Row[]>([]);
  let cursor = $state(0);
  let compact = $state(false);
  let noSuggestions = $state(false);
  let revealed = $state(false);
  let warning = $state('');
  let warnTimer: ReturnType<typeof setTimeout> | undefined;
  let hiding = false;
  let composing = $state(false);
  let seq = 0;

  const showList = $derived(rows.length > 0 && (query !== '' ? true : !compact || revealed));

  function warn(message: string) {
    warning = message;
    clearTimeout(warnTimer);
    warnTimer = setTimeout(() => (warning = ''), 5000);
  }

  async function refresh() {
    const mine = ++seq;
    try {
      const r = await invoke<{ rows: Row[] }>('commandbar_search', { query });
      if (mine !== seq) return;
      rows = query === '' && noSuggestions ? [] : r.rows;
      cursor = 0;
      if (list) list.scrollTop = 0;
    } catch (err) {
      warn(String(err));
    }
  }

  function setQuery(value: string) {
    query = value;
    revealed = false;
    refresh();
  }

  function close(refocus = true) {
    if (hiding) return;
    hiding = true;
    const r = reducedMotion();
    card.getAnimations().forEach((a) => a.cancel());
    const anim = card.animate([{ opacity: 1, transform: 'none' }, r ? { opacity: 0 } : { opacity: 0, transform: 'scale(0.98)' }], {
      duration: duration('commandBarClose'),
      easing: motion.commandBarClose.ease,
    });
    card.style.opacity = '0';
    const done = () => invoke('commandbar_hide', { refocus });
    anim.finished.then(done).catch(done);
  }

  function animateOpen() {
    const r = reducedMotion();
    card.getAnimations().forEach((a) => a.cancel());
    card.style.opacity = '1';
    card.animate([r ? { opacity: 0 } : { opacity: 0, transform: 'translateY(-12px) scale(0.98)' }, { opacity: 1, transform: 'none' }], {
      duration: duration('commandBarOpen'),
      easing: r ? 'linear' : motion.commandBarOpen.ease,
    });
  }

  async function opened(p: OpenPayload) {
    hiding = false;
    compact = p.compact;
    noSuggestions = p.noSuggestions;
    query = p.query;
    revealed = false;
    warning = '';
    rows = [];
    animateOpen();
    input?.focus();
    input?.select();
    await refresh();
    await tick();
    input?.focus();
  }

  async function run(row: Row | undefined, alt = false) {
    if (!row || hiding) return;
    let out: Outcome;
    try {
      out = await invoke<Outcome>('commandbar_run', { id: row.id, query, alt });
    } catch (err) {
      warn(String(err));
      return;
    }
    if (out.complete !== null) {
      query = out.complete;
      input.setSelectionRange(query.length, query.length);
      refresh();
      return;
    }
    if (out.copy !== null) {
      try {
        await navigator.clipboard.writeText(out.copy);
      } catch {
        warn('Could not copy that to the clipboard.');
        return;
      }
      close(true);
      return;
    }
    hiding = true;
    card.style.opacity = '0';
  }

  function move(by: number) {
    if (!rows.length) return;
    cursor = (cursor + by + rows.length) % rows.length;
  }

  function complete() {
    const row = rows[cursor];
    if (!row) return;
    if (row.kind === 'app' || row.kind === 'script' || row.kind === 'file') {
      query = row.title;
      input.setSelectionRange(query.length, query.length);
      refresh();
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.isComposing || e.keyCode === 229) return;
    const ctrl = e.ctrlKey || e.metaKey;
    const digit = /^(?:Digit|Numpad)([1-9])$/.exec(e.code);
    if (ctrl && !e.altKey && !e.shiftKey && digit) {
      e.preventDefault();
      run(rows[Number(digit[1]) - 1]);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      close(true);
    } else if (e.key === 'ArrowDown' || (ctrl && e.key.toLowerCase() === 'n')) {
      e.preventDefault();
      if (compact && !revealed && query === '' && !rows.length) {
        revealed = true;
        invoke<{ rows: Row[] }>('commandbar_search', { query: '' }).then((r) => (rows = r.rows));
        return;
      }
      if (compact && !revealed && query === '') revealed = true;
      else move(1);
    } else if (e.key === 'ArrowUp' || (ctrl && e.key.toLowerCase() === 'p')) {
      e.preventDefault();
      move(-1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      run(rows[cursor], ctrl);
    } else if (e.key === 'Tab') {
      e.preventDefault();
      complete();
    }
  }

  async function startDrag(e: MouseEvent) {
    if (e.button !== 0 || e.detail > 1) return;
    try {
      await getCurrentWindow().startDragging();
    } finally {
      invoke('commandbar_dragged');
      input?.focus();
    }
  }

  function recenter() {
    invoke('commandbar_recenter');
    input?.focus();
  }

  $effect(() => {
    const theme = store.settings?.general.theme;
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
  });

  $effect(() => {
    setReducedMotion(store.settings?.general.reducedMotion ?? false);
  });

  $effect(() => {
    const el = list;
    if (!el) return;
    const active = el.children[cursor] as HTMLElement | undefined;
    active?.scrollIntoView({ block: 'nearest' });
  });

  $effect(() => {
    const observer = new ResizeObserver(() => {
      invoke('commandbar_resize', { height: Math.ceil(card.offsetHeight) + MARGIN * 2 });
    });
    observer.observe(card);
    return () => observer.disconnect();
  });

  onMount(() => {
    applyCssVars();
    store.init().catch(() => {});
    const off = listen<OpenPayload>('commandbar://open', (e) => opened(e.payload));
    window.addEventListener('keydown', onKey);
    return () => {
      off.then((f) => f());
      clearTimeout(warnTimer);
      window.removeEventListener('keydown', onKey);
    };
  });
</script>

<div class="stage">
  <div class="card" bind:this={card} role="dialog" aria-label="Command Bar">
    <header>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="handle" title="Drag to move. Double-click to centre." onmousedown={startDrag} ondblclick={recenter}>
        <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6h0M15 6h0M9 12h0M15 12h0M9 18h0M15 18h0" /></svg>
      </div>
      <svg class="lens" width="18" height="18" viewBox="0 0 24 24" aria-hidden="true"><path d="M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM20 20l-4.2-4.2" /></svg>
      <input
        bind:this={input}
        value={query}
        type="text"
        placeholder="Search apps, files, windows, or type a sum"
        aria-label="Command Bar search"
        spellcheck="false"
        autocomplete="off"
        oninput={(e) => {
          if (!composing) setQuery(e.currentTarget.value);
          else query = e.currentTarget.value;
        }}
        oncompositionstart={() => (composing = true)}
        oncompositionend={(e) => {
          composing = false;
          setQuery(e.currentTarget.value);
        }}
      />
    </header>

    {#if showList}
      <ul class="list" bind:this={list} role="listbox">
        {#each rows as row, i (row.id)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <li class="row" class:cursor={i === cursor} role="option" aria-selected={i === cursor} onmousemove={() => (cursor = i)} onclick={() => run(row)}>
            <span class="icon k-{row.kind}">
              {#if isGlyph(row.icon)}
                <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden="true"><path d={glyphPath(row.icon)} /></svg>
              {:else if row.kind === 'color'}
                <span class="swatch" style:background={row.icon}></span>
              {:else if row.kind === 'emoji'}
                <span class="emoji">{row.icon}</span>
              {:else}
                {row.title.charAt(0).toUpperCase()}
              {/if}
            </span>
            <span class="text">
              <span class="title">{row.title}</span>
              <span class="sub">{row.subtitle}</span>
            </span>
            {#if i < 9}<kbd>Ctrl {i + 1}</kbd>{/if}
          </li>
        {/each}
      </ul>
      <footer>
        <span><kbd>Enter</kbd> Open</span>
        <span><kbd>Ctrl Enter</kbd> Alternate</span>
        <span><kbd>Tab</kbd> Complete</span>
        <span><kbd>Esc</kbd> Close</span>
      </footer>
    {:else if query !== '' && !composing}
      <div class="empty">Nothing found for "{query}"</div>
    {/if}
    {#if warning}<div class="warn" role="alert">{warning}</div>{/if}
  </div>
</div>

<style>
  :global(html),
  :global(body.commandbar-page) {
    background: transparent;
  }
  .stage {
    position: fixed;
    inset: 0;
    padding: 20px;
  }
  .card {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line-strong);
    border-radius: 14px;
    background: var(--bg);
    box-shadow:
      0 16px 40px rgb(0 0 0 / 0.24),
      0 2px 6px rgb(0 0 0 / 0.12);
    overflow: hidden;
    transform-origin: 50% 0;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 56px;
    padding: 0 16px 0 6px;
  }
  .handle {
    display: grid;
    place-items: center;
    flex: none;
    width: 26px;
    height: 36px;
    border-radius: 8px;
    color: var(--mut);
    cursor: grab;
  }
  .handle:hover {
    background: var(--hover);
  }
  .handle svg,
  .lens,
  .icon svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 2.2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .lens {
    flex: none;
    color: var(--mut);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    outline: 0;
    background: none;
    color: var(--fg);
    font: inherit;
    font-size: 18px;
    user-select: text;
  }
  input::placeholder {
    color: var(--mut);
  }
  .list {
    margin: 0;
    padding: 6px;
    list-style: none;
    max-height: 408px;
    overflow-y: auto;
    border-top: 1px solid var(--line);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 48px;
    padding: 0 10px;
    border-radius: 9px;
    cursor: default;
    transition: background-color var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
  .row.cursor {
    background: var(--acc-soft);
  }
  .icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: var(--inset);
    color: var(--fg-soft);
    font-size: 14px;
    font-weight: 600;
  }
  .icon.k-calc,
  .icon.k-unit {
    background: var(--acc);
    color: var(--acc-fg);
  }
  .icon.k-emoji {
    background: none;
  }
  .swatch {
    width: 22px;
    height: 22px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
  }
  .emoji {
    font-size: 22px;
    line-height: 1;
  }
  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }
  .title {
    overflow: hidden;
    font-size: 14px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    overflow: hidden;
    color: var(--mut);
    font-size: 11.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  kbd {
    flex: none;
    padding: 1px 6px;
    border: 1px solid var(--line-strong);
    border-radius: 5px;
    color: var(--mut);
    font: inherit;
    font-size: 10.5px;
  }
  footer {
    display: flex;
    gap: 14px;
    padding: 8px 14px;
    border-top: 1px solid var(--line);
    color: var(--mut);
    font-size: 11.5px;
  }
  .empty {
    padding: 14px 18px 16px;
    border-top: 1px solid var(--line);
    color: var(--mut);
    font-size: 13px;
  }
  .warn {
    padding: 8px 16px;
    border-top: 1px solid var(--line);
    color: var(--warn);
    font-size: 12px;
  }
</style>
