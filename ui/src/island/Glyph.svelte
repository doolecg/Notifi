<script lang="ts" module>
  import { icons as panelIcons } from '../panel/icons';

  // Island-only line icons on the same 24 x 24 grid as the panel's.
  const own: Record<string, string> = {
    music: 'M9 17.5V6l10-2v11.5 M9 17.5a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0Z M19 15.5a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0Z',
    timer: 'M12 21a8 8 0 1 0 0-16 8 8 0 0 0 0 16Z M12 9v4l2.5 2 M10 2.5h4',
    agents: 'M7 8.5h10a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-6.5a2 2 0 0 1 2-2Z M12 4.5v4 M9.5 13h.01 M14.5 13h.01 M10 16h4',
    grid: 'M4.5 4.5h6v6h-6z M13.5 4.5h6v6h-6z M4.5 13.5h6v6h-6z M13.5 13.5h6v6h-6z',
    play: 'M8 5.5v13l10-6.5Z',
    pause: 'M8.5 5.5v13 M15.5 5.5v13',
    next: 'M6 6.5l8 5.5-8 5.5Z M18 6.5v11',
    prev: 'M18 6.5l-8 5.5 8 5.5Z M6 6.5v11',
    back: 'M14.5 5.5 8 12l6.5 6.5',
    search: 'M11 17.5a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13Z M20 20l-4.3-4.3',
    more: 'M6 12h.01 M12 12h.01 M18 12h.01',
    sun: 'M12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8Z M12 2.5v2 M12 19.5v2 M2.5 12h2 M19.5 12h2 M5.3 5.3l1.4 1.4 M17.3 17.3l1.4 1.4 M5.3 18.7l1.4-1.4 M17.3 6.7l1.4-1.4',
    calendar: 'M4.5 6.5h15v13h-15z M4.5 10.5h15 M8.5 4v4 M15.5 4v4',
    bell: 'M6.5 16v-5a5.5 5.5 0 0 1 11 0v5l1.5 2H5Z M10 20.5h4',
    camera: 'M4 7.5h11v9H4z M15 10.5l5-3v9l-5-3',
    download: 'M12 4v11 M7.5 10.5 12 15l4.5-4.5 M5 19.5h14',
    tools: 'M14 4.5a4.5 4.5 0 0 0 5.5 5.5L11 18.5a2.1 2.1 0 0 1-3-3L16.5 7A4.5 4.5 0 0 1 14 4.5Z',
    captures: 'M4 8V5.5A1.5 1.5 0 0 1 5.5 4H8 M16 4h2.5A1.5 1.5 0 0 1 20 5.5V8 M20 16v2.5a1.5 1.5 0 0 1-1.5 1.5H16 M8 20H5.5A1.5 1.5 0 0 1 4 18.5V16',
    shelf: 'M4 7h6l2 2h8v9.5a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18.5Z',
    close: 'M6.5 6.5l11 11 M17.5 6.5l-11 11',
    minus: 'M6 12h12',
    expand: 'M6 14.5l6-6 6 6',
    shrink: 'M6 9.5l6 6 6-6',
  };

  export const glyphs: Record<string, string> = { ...panelIcons, ...own };
</script>

<script lang="ts">
  let { name, size = 18, filled = false }: { name: string; size?: number; filled?: boolean } = $props();
</script>

<svg class="glyph" class:filled width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
  <path d={glyphs[name] ?? glyphs.utilities} />
</svg>

<style>
  .glyph {
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.7;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .filled {
    fill: currentColor;
  }
</style>
