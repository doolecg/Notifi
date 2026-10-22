<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars, fadeSlide, setReducedMotion } from './lib/motion';
  import { store } from './lib/settings.svelte';
  import Icon from './panel/Icon.svelte';
  import Breadcrumb from './sections/Breadcrumb.svelte';
  import PageView from './sections/PageView.svelte';
  import Setup from './sections/Setup.svelte';
  import Sidebar from './sections/Sidebar.svelte';

  const LAST_KEY = 'notifi.settings.lastPage';

  let history = $state<string[]>([]);
  let index = $state(-1);
  /** Option to scroll to, with a counter so the same option can be focused twice. */
  let focus = $state<{ target: string; n: number } | null>(null);
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout>;

  const current = $derived(history[index] ?? 'general');
  const page = $derived(store.page(current) ?? store.page('general'));
  /** Settings > Installed > Island for a feature you have, Settings > Install features > Island for one you don't. */
  const crumbs = $derived.by(() => {
    if (!page) return [];
    const trail: { label: string; id?: string }[] = [{ label: 'Settings', id: 'general' }];
    if (page.feature) trail.push(store.installed(page.id) ? { label: 'Installed', id: 'installed' } : { label: 'Install features', id: 'features' });
    trail.push({ label: page.label });
    return trail;
  });

  $effect(() => {
    const theme = store.settings?.general.theme;
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
  });

  $effect(() => setReducedMotion(store.settings?.general.reducedMotion ?? false));

  /** Opens a page (and option, as `namespace.key`), adding it to the history. */
  function goto(id: string, option?: string) {
    if (!id) return;
    if (id !== current || index < 0) {
      history = [...history.slice(0, index + 1), id];
      index = history.length - 1;
      remember(id);
    }
    focus = option ? { target: option, n: (focus?.n ?? 0) + 1 } : null;
  }

  function step(by: number) {
    const next = index + by;
    if (next < 0 || next >= history.length) return;
    index = next;
    focus = null;
    remember(history[index]);
  }

  function remember(id: string) {
    try {
      localStorage.setItem(LAST_KEY, id);
    } catch {}
  }

  /** `page`, `page/option` or `page/namespace.option`; old group names fall back to their first page. */
  function gotoLink(link: string) {
    const [id, option] = decodeURIComponent(link).split('/');
    const target = store.page(id) ?? store.catalog?.groups.find((g) => g.id === id)?.pages[0];
    if (!target) return;
    goto(target.id, option && (option.includes('.') ? option : `${target.id}.${option}`));
  }

  function showToast(text: string) {
    clearTimeout(toastTimer);
    toast = text;
    toastTimer = setTimeout(() => (toast = null), 2400);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
    if (e.key === 'ArrowLeft') step(-1);
    else if (e.key === 'ArrowRight') step(1);
    else return;
    e.preventDefault();
  }

  function onmouseup(e: MouseEvent) {
    if (e.button === 3) step(-1);
    else if (e.button === 4) step(1);
  }

  onMount(() => {
    applyCssVars();
    let saved: string | null = null;
    try {
      saved = localStorage.getItem(LAST_KEY);
    } catch {}
    store.init().then(() => {
      if (location.hash.length > 1) gotoLink(location.hash.slice(1));
      else goto(saved && store.page(saved) ? saved : 'general');
    });
    const off = listen<string>('settings://goto', (e) => gotoLink(e.payload));
    return () => off.then((f) => f());
  });
</script>

<svelte:window {onkeydown} {onmouseup} />

<div class="app">
  <Sidebar active={current} {goto} />

  <main>
    <div class="topbar">
      <button class="nav" aria-label="Back" title="Back (Alt+Left)" disabled={index <= 0} onclick={() => step(-1)}>
        <span class="flip"><Icon name="chevron" size={16} /></span>
      </button>
      <button class="nav" aria-label="Forward" title="Forward (Alt+Right)" disabled={index >= history.length - 1} onclick={() => step(1)}>
        <Icon name="chevron" size={16} />
      </button>
      <Breadcrumb {crumbs} {goto} />
    </div>
    <div class="scroll">
      {#if store.settings && store.catalog && page}
        {#key page.id}
          <div class="page" in:fadeSlide={{ token: 'tabSwitch' }}>
            {#key focus?.n}
              <PageView {page} focus={focus?.target ?? null} {goto} onreset={() => showToast(`${page.label} is back to its defaults`)} />
            {/key}
          </div>
        {/key}
      {/if}
    </div>
  </main>

  {#if store.settings && store.catalog && !store.settings.general.setupDone}
    <Setup onDone={() => goto('installed')} />
  {/if}

  {#if toast}
    <div class="toast" role="status" in:fadeSlide={{ token: 'toastIn', y: 12 }} out:fadeSlide={{ token: 'toastOut', y: 12 }}>{toast}</div>
  {/if}
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 230px 1fr;
    height: 100vh;
  }
  main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .topbar {
    display: flex;
    flex: none;
    align-items: center;
    gap: 4px;
    height: 46px;
    padding: 0 14px;
    border-bottom: 1px solid var(--line);
  }
  .nav {
    display: grid;
    place-items: center;
    width: 30px;
    height: 28px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--fg-soft);
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .nav:hover:not(:disabled) {
    background: var(--hover);
  }
  .nav:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .flip {
    display: inline-flex;
    transform: scaleX(-1);
  }
  .scroll {
    flex: 1;
    overflow-y: auto;
  }
  .page {
    max-width: 760px;
    margin: 0 auto;
    padding: 26px 32px 48px;
  }
  .toast {
    position: fixed;
    left: 50%;
    bottom: 24px;
    translate: -50% 0;
    padding: 9px 16px;
    border-radius: 10px;
    background: var(--toast);
    color: var(--toast-fg);
    font-size: 13px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.18);
  }
</style>
