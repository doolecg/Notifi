<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, tick } from 'svelte';
  import { applyCssVars, duration, fadeSlide, motion, reducedMotion, setReducedMotion } from '../lib/motion';
  import { store } from '../lib/settings.svelte';
  import { words } from '../clipboard/entries';
  import Glyph from '../clipboard/Glyph.svelte';
  import Highlight from '../clipboard/Highlight.svelte';
  import { oneLine, parse, zonesFor, type Snippet } from './vars';

  let card: HTMLElement;
  let input: HTMLInputElement;
  let scroller = $state<HTMLElement>();

  let snippets = $state.raw<Snippet[]>([]);
  let loaded = $state(false);
  let draft = $state('');
  let composing = false;
  let query = $state('');
  let cursor = $state<string | null>(null);
  let warning = $state('');
  let warnTimer: ReturnType<typeof setTimeout> | undefined;
  let hiding = false;

  const ws = $derived(words(query));

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
    const named = [...groups.keys()].filter(Boolean);
    return [...(groups.has('') ? [''] : []), ...named].map((group) => ({ group, items: groups.get(group)! }));
  });
  const ordered = $derived(sections.flatMap((s) => s.items));
  const index = $derived(ordered.findIndex((s) => s.id === cursor));
  const numberOf = $derived(new Map(ordered.slice(0, 9).map((s, i) => [s.id, i + 1])));

  function warn(message: string) {
    warning = message;
    clearTimeout(warnTimer);
    warnTimer = setTimeout(() => (warning = ''), 5000);
  }

  async function load(keep: boolean) {
    try {
      const r = await invoke<{ snippets: Snippet[] }>('snippets_get');
      snippets = r.snippets;
      loaded = true;
      reportZones();
    } catch (err) {
      warn(String(err));
      return;
    }
    await tick();
    if (!keep || !ordered.some((s) => s.id === cursor)) cursor = ordered[0]?.id ?? null;
  }

  /** Typed triggers expand in Rust, which can't resolve named zones, so it keeps these. */
  function reportZones() {
    invoke('snippets_zones', { zones: zonesFor(snippets.map((s) => s.text).join('\n')) }).catch(() => {});
  }

  async function setQuery(value: string) {
    query = value;
    await tick();
    cursor = ordered[0]?.id ?? null;
    scroller?.scrollTo({ top: 0 });
  }

  function move(by: number) {
    if (!ordered.length) return;
    cursor = ordered[Math.min(ordered.length - 1, Math.max(0, (index < 0 ? -1 : index) + by))].id;
  }

  function insert(s: Snippet | undefined) {
    if (!s || hiding) return;
    hiding = true;
    invoke('snippet_insert', { id: s.id, zones: zonesFor(s.text) }).catch((err) => {
      hiding = false;
      warn(String(err) || 'Could not insert that snippet.');
    });
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
    anim.finished.then(() => invoke('snippets_hide')).catch(() => invoke('snippets_hide'));
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

  async function opened() {
    hiding = false;
    draft = '';
    query = '';
    warning = '';
    animateOpen();
    input?.focus();
    await load(false);
    scroller?.scrollTo({ top: 0 });
    input?.focus();
  }

  function editSnippets() {
    invoke('open_settings_section', { section: 'textSnippets' }).catch(() => {});
    close();
  }

  function onKey(e: KeyboardEvent) {
    if (e.isComposing || e.keyCode === 229) return;
    const ctrl = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();
    const digit = /^(?:Digit|Numpad)([1-9])$/.exec(e.code);
    if (digit && !e.altKey && !e.shiftKey && (ctrl || !input.value)) {
      e.preventDefault();
      insert(ordered[Number(digit[1]) - 1]);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      close();
    } else if (e.key === 'ArrowDown' || (ctrl && key === 'n')) {
      e.preventDefault();
      move(1);
    } else if (e.key === 'ArrowUp' || (ctrl && key === 'p')) {
      e.preventDefault();
      move(-1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      insert(ordered[index]);
    } else if (document.activeElement !== input && e.key.length === 1 && !ctrl && !e.altKey) {
      input.focus();
    }
  }

  $effect(() => {
    const theme = store.settings?.general.theme;
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
  });

  $effect(() => setReducedMotion(store.settings?.general.reducedMotion ?? false));

  $effect(() => {
    const id = cursor;
    if (!id || !scroller) return;
    scroller.querySelector(`[data-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: 'nearest' });
  });

  onMount(() => {
    applyCssVars();
    store.init().catch(() => {});
    load(false).then(() => input?.focus());
    const offs = [listen('snippets://open', opened), listen('snippets://changed', () => load(true))];
    const onFocus = () => {
      if (card.style.opacity === '0' && !hiding) animateOpen();
    };
    const onBlur = () => {
      if (!hiding && !document.hasFocus()) close();
    };
    window.addEventListener('keydown', onKey);
    window.addEventListener('focus', onFocus);
    window.addEventListener('blur', onBlur);
    // Offsets move with daylight saving.
    const zoneTimer = setInterval(reportZones, 30 * 60 * 1000);
    return () => {
      offs.forEach((off) => off.then((f) => f()));
      clearTimeout(warnTimer);
      clearInterval(zoneTimer);
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('focus', onFocus);
      window.removeEventListener('blur', onBlur);
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" onmousedown={(e) => e.target === e.currentTarget && close()}>
  <div class="card" bind:this={card} role="dialog" aria-label="Insert a snippet">
    <header>
      <label class="search">
        <Glyph name="search" size={16} />
        <input
          bind:this={input}
          value={draft}
          type="text"
          placeholder="Search snippets"
          aria-label="Search snippets"
          aria-controls="snip-list"
          aria-activedescendant={cursor ? `snip-${cursor}` : undefined}
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
      </label>
    </header>

    <div class="scroller" id="snip-list" role="listbox" aria-label="Snippets" bind:this={scroller}>
      {#if loaded && !ordered.length}
        <div class="empty">
          <span class="empty-mark"><Glyph name={query ? 'search' : 'snippet'} size={22} /></span>
          {#if query}
            <strong>No snippets match “{query}”</strong>
            <span>Search looks at names, triggers, text and groups.</span>
          {:else}
            <strong>No snippets yet</strong>
            <span>Add phrases you type often, then insert them from here.</span>
            <button class="foot-btn" onclick={editSnippets}>Add snippets</button>
          {/if}
        </div>
      {:else}
        {#each sections as section (section.group)}
          {#if section.group}
            <div class="group" role="presentation">{section.group}</div>
          {/if}
          {#each section.items as s (s.id)}
            {@const n = numberOf.get(s.id)}
            <div
              class="item"
              class:cursor={s.id === cursor}
              id="snip-{s.id}"
              data-id={s.id}
              role="option"
              tabindex="-1"
              aria-selected={s.id === cursor}
              onclick={() => {
                cursor = s.id;
                input.focus({ preventScroll: true });
              }}
              ondblclick={() => insert(s)}
              onkeydown={() => {}}
            >
              <span class="badge" class:none={!n} aria-hidden="true">{n ?? ''}</span>
              <span class="main">
                <span class="name"><Highlight text={s.name || 'Untitled'} words={ws} /></span>
                <span class="line">
                  {#each parse(oneLine(s.text)) as p, i (i)}
                    {#if p.kind === 'var'}<span class="var">{p.raw}</span>{:else}<Highlight text={p.value} words={ws} />{/if}
                  {/each}
                </span>
              </span>
              {#if s.trigger}<span class="trigger"><Highlight text={s.trigger} words={ws} /></span>{/if}
            </div>
          {/each}
        {/each}
      {/if}
    </div>

    <footer>
      <span class="hints">
        <span><kbd>Enter</kbd>Insert</span>
        <span><kbd>{draft ? 'Ctrl 1-9' : '1-9'}</kbd>Quick insert</span>
        <span><kbd>Esc</kbd>Close</span>
      </span>
      <button class="foot-btn" onclick={editSnippets}>Edit snippets</button>
    </footer>

    {#if warning}
      <button class="sheet" onclick={() => (warning = '')} transition:fadeSlide={{ token: 'toastIn', y: 8 }}>
        <Glyph name="warning" size={16} />
        <span>{warning}</span>
      </button>
    {/if}
  </div>
</div>

<style>
  :global(html),
  :global(body.snippets-page) {
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
    border: 1px solid var(--line-strong);
    border-radius: 14px;
    background: var(--bg);
    box-shadow:
      0 16px 40px rgb(0 0 0 / 0.24),
      0 2px 6px rgb(0 0 0 / 0.12);
    overflow: hidden;
    container-type: inline-size;
    transform-origin: 50% 0;
  }
  header {
    flex: none;
    padding: 10px 10px 8px;
    border-bottom: 1px solid var(--line);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 38px;
    padding: 0 12px;
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
  .scroller {
    flex: 1;
    min-height: 0;
    padding: 4px 0 6px;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .group {
    position: sticky;
    top: -4px;
    z-index: 1;
    padding: 9px 14px 4px;
    background: var(--bg);
    color: var(--mut);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 48px;
    padding: 6px 12px;
    cursor: default;
    transition: background-color var(--m-selectionMove-dur) var(--m-selectionMove-ease);
  }
  .item:hover {
    background: var(--hover);
  }
  .item.cursor {
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
  .badge {
    display: grid;
    place-items: center;
    flex: none;
    width: 20px;
    height: 20px;
    border: 1px solid var(--line-strong);
    border-bottom-width: 2px;
    border-radius: 6px;
    background: var(--surface);
    color: var(--mut);
    font: 600 11px/1 var(--mono);
  }
  .badge.none {
    visibility: hidden;
  }
  .main {
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
    gap: 1px;
  }
  .name {
    overflow: hidden;
    font-size: 13.5px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .line {
    overflow: hidden;
    color: var(--mut);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .var {
    padding: 0 4px;
    border-radius: 4px;
    background: var(--inset);
    color: var(--acc);
    font: 600 11px var(--mono);
  }
  .trigger {
    flex: none;
    max-width: 40%;
    overflow: hidden;
    padding: 1px 7px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: var(--surface);
    color: var(--fg-soft);
    font: 600 11.5px/1.5 var(--mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    height: 100%;
    min-height: 150px;
    padding: 20px;
    color: var(--mut);
    font-size: 12.5px;
    text-align: center;
  }
  .empty strong {
    color: var(--fg);
    font-size: 14px;
  }
  .empty .foot-btn {
    margin-top: 10px;
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
    height: 40px;
    padding: 0 10px 0 14px;
    border-top: 1px solid var(--line);
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
  .hints > span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
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
  .foot-btn:hover {
    background: var(--hover);
    color: var(--fg);
  }
  .foot-btn:focus-visible {
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
    border: 1px solid var(--warn);
    border-radius: 11px;
    background: var(--surface);
    color: var(--warn);
    font: inherit;
    font-size: 13px;
    text-align: left;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.18);
    cursor: pointer;
  }
  @container (max-width: 380px) {
    .hints > span:nth-child(3) {
      display: none;
    }
    .trigger {
      display: none;
    }
  }
</style>
