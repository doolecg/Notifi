<script lang="ts">
  import Icon from '../panel/Icon.svelte';
  import { icons, type IconName } from '../panel/icons';
  import type { Page } from '../lib/settings.svelte';

  let { page, group, size = 26 }: { page: Page; group: string; size?: number } = $props();

  const COLORS: Record<string, string> = {
    essentials: '#0f9483',
    utilities: '#d9772b',
    sound: '#d6455d',
    energy: '#c9a227',
    windows: '#3b7dd8',
    input: '#7b5cd6',
    clipboard: '#2a9d5c',
    apps: '#5b6b7a',
    about: '#5b6b7a',
  };
  const ALIAS: Record<string, IconName> = { mic: 'sound', music: 'sound', headphones: 'speaker', output: 'speaker', window: 'windows', layout: 'windows', taskbar: 'windows', mouse: 'input', clipboard: 'clipboard', notes: 'notes', island: 'controls' };
  const name = $derived((page.icon in icons ? page.icon : ALIAS[page.icon]) as IconName | undefined);
</script>

<span class="glyph" style:--c={COLORS[group] ?? COLORS.apps} style:--s="{size}px">
  {#if name}<Icon {name} size={Math.round(size * 0.62)} />{:else}{page.label.charAt(0)}{/if}
</span>

<style>
  .glyph {
    display: inline-grid;
    flex: none;
    place-items: center;
    width: var(--s);
    height: var(--s);
    border-radius: calc(var(--s) * 0.28);
    background: var(--c);
    color: #fff;
    font-size: calc(var(--s) * 0.5);
    font-weight: 700;
  }
</style>
