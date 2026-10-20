<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars } from '../lib/motion';
  import { store } from '../lib/settings.svelte';

  interface Notice {
    kind: 'volume' | 'text';
    icon: string;
    label: string;
    level: number;
    muted: boolean;
  }

  const PATHS: Record<string, string> = {
    speakers: 'M4 9.5h3.5L12 5.5v13l-4.5-4H4z M15.5 9a4 4 0 0 1 0 6 M18 6.5a7.5 7.5 0 0 1 0 11',
    muted: 'M4 9.5h3.5L12 5.5v13l-4.5-4H4z M16 9.5l5 5 M21 9.5l-5 5',
    headphones: 'M4.5 14v-2a7.5 7.5 0 0 1 15 0v2 M4.5 14h3v5h-1.5a1.5 1.5 0 0 1-1.5-1.5V14Z M19.5 14h-3v5h1.5a1.5 1.5 0 0 0 1.5-1.5V14Z',
    display: 'M3.5 5.5h17v10h-17z M9 19.5h6 M12 15.5v4',
    mic: 'M12 4a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V7a3 3 0 0 1 3-3Z M6.5 11.5a5.5 5.5 0 0 0 11 0 M12 17v3',
    'mic-off': 'M12 4a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V7a3 3 0 0 1 3-3Z M6.5 11.5a5.5 5.5 0 0 0 11 0 M12 17v3 M4 4l16 16',
  };

  $effect(() => {
    const theme = store.settings?.general.theme;
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
  });

  const indicator = location.hash === '#mic';
  let notice = $state<Notice | null>(null);
  let leaving = $state(false);

  const path = $derived(notice ? (notice.kind === 'volume' && notice.muted ? PATHS.muted : (PATHS[notice.icon] ?? PATHS.speakers)) : '');

  onMount(() => {
    store.init();
    applyCssVars();
    const offs = [
      listen<Notice>('osd://show', (e) => {
        notice = e.payload;
        leaving = false;
      }),
      listen('osd://hide', () => (leaving = true)),
    ];
    return () => offs.forEach((o) => o.then((f) => f()));
  });
</script>

{#if indicator}
  <div class="pill dot" role="status" aria-label="Microphones are muted">
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d={PATHS['mic-off']} /></svg>
  </div>
{:else if notice}
  <div class="pill" class:leaving role="status">
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={path} /></svg>
    {#if notice.kind === 'volume'}
      <span class="bar"><span class="fill" style:width="{notice.level * 100}%"></span></span>
      <span class="num">{notice.muted ? 'Muted' : notice.label}</span>
    {:else}
      <span class="text">{notice.label}</span>
    {/if}
  </div>
{/if}

<style>
  :global(html, body) {
    margin: 0;
    background: transparent !important;
    overflow: hidden;
  }
  .pill {
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100vw;
    height: 100vh;
    padding: 0 18px;
    border-radius: 99px;
    background: var(--toast);
    color: var(--toast-fg);
    font: 600 14px var(--sans);
    box-shadow: 0 6px 22px rgb(0 0 0 / 0.3);
    animation: in var(--m-toastIn-dur, 220ms) var(--m-toastIn-ease, ease-out);
    transition: opacity var(--m-toastOut-dur, 180ms) ease-out;
  }
  .pill.leaving {
    opacity: 0;
  }
  .dot {
    justify-content: center;
    padding: 0;
    border-radius: 50%;
    background: #d6403a;
    color: #fff;
  }
  .bar {
    flex: 1;
    height: 6px;
    border-radius: 99px;
    background: rgb(128 128 128 / 0.35);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: currentColor;
  }
  .num {
    min-width: 44px;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  @keyframes in {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.97);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .pill {
      animation: none;
    }
  }
</style>
