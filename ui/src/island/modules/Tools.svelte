<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import Glyph from '../Glyph.svelte';
  import { fadeSlide } from '../../lib/motion';
  import { onPlatform, type Catalog, type Settings } from '../../lib/settings.svelte';

  interface Awake {
    supported: boolean;
    active: boolean;
    endsAt: number | null;
  }

  let catalog = $state<Catalog | null>(null);
  let settings = $state<Settings | null>(null);
  let awake = $state<Awake | null>(null);

  const tools = $derived(
    (catalog?.groups ?? []).flatMap((g) =>
      g.pages.filter((p) => p.tool && onPlatform(p) && settings?.features[p.id]?.enabled === true).map((p) => ({ ...p, group: g.id })),
    ),
  );

  const loadAwake = () =>
    invoke<Awake>('keep_awake_status')
      .then((a) => (awake = a))
      .catch(() => {});

  async function toggleAwake() {
    if (!awake) return;
    try {
      awake = await invoke<Awake>('set_keep_awake', { on: !awake.active, minutes: settings?.panel.keepAwakeMinutes ?? 0 });
    } catch {
      loadAwake();
    }
  }

  /** Tools that can run straight from the menu; the rest open their settings page. */
  const RUN: Record<string, string> = { colorPicker: 'picker_toggle', mediaTools: 'media_open', cleaningMode: 'cleaning_mode_start', cameraPreview: 'camera_preview_toggle' };
  const open = (section: string) => invoke(RUN[section] ?? 'open_settings_section', RUN[section] ? {} : { section }).catch(() => {});

  onMount(() => {
    let stop: (() => void) | undefined;
    listen<Settings>('settings://changed', (e) => (settings = e.payload)).then((f) => (stop = f));
    Promise.all([invoke<Catalog>('get_catalog'), invoke<Settings>('get_settings')])
      .then(([c, s]) => {
        catalog = c;
        settings = s;
      })
      .catch(() => {});
    loadAwake();
    const timer = setInterval(loadAwake, 2000);
    return () => {
      clearInterval(timer);
      stop?.();
    };
  });
</script>

<div class="tools">
  <header><h2>Tools</h2></header>

  <div class="grid">
    {#if awake?.supported}
      <button class="tile" class:on={awake.active} aria-pressed={awake.active} onclick={toggleAwake}>
        <Glyph name="keepAwake" size={20} />
        <span class="label">Keep awake</span>
        <span class="sub">{awake.active ? 'On' : 'Off'}</span>
      </button>
    {/if}
    {#each tools as t (t.id)}
      <button class="tile" title={t.blurb} onclick={() => open(t.id)} in:fadeSlide={{ token: 'listReorder', y: 6 }}>
        <Glyph name={t.group} size={20} />
        <span class="label">{t.label}</span>
      </button>
    {/each}
    <button class="tile add" onclick={() => open('features')}>
      <Glyph name="plus" size={20} />
      <span class="label">Add tools</span>
    </button>
  </div>
</div>

<style>
  .tools {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: 8px;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--i-mut);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(116px, 1fr));
    grid-auto-rows: 64px;
    gap: 8px;
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    align-content: start;
    scrollbar-width: none;
  }
  .tile {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    justify-content: space-between;
    min-width: 0;
    padding: 9px 10px;
    border: 0;
    border-radius: 10px;
    background: var(--i-raised);
    color: var(--i-fg);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease), transform var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .tile:hover {
    background: var(--i-hover);
  }
  .tile:active {
    transform: scale(0.97);
  }
  .tile.on {
    color: var(--i-acc);
  }
  .tile.add {
    background: none;
    color: var(--i-mut);
    outline: 1px dashed var(--i-line);
    outline-offset: -1px;
  }
  .label {
    max-width: 100%;
    overflow: hidden;
    font-size: 12.5px;
    font-weight: 600;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .sub {
    font-size: 11px;
    color: var(--i-mut);
  }
</style>
