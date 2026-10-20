<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { applyCssVars, reducedMotion } from '../lib/motion';
  import { store } from '../lib/settings.svelte';
  import Icon from '../panel/Icon.svelte';
  import { icons, type IconName } from '../panel/icons';

  interface SliceView {
    label: string;
    icon: string;
    submenu: boolean;
    empty: boolean;
  }
  interface Show {
    slices: SliceView[];
    back: boolean;
    labels: boolean;
    animate: boolean;
    theme: string;
    size: number;
    outer: number;
    inner: number;
  }

  const GAP = 0.025;

  let view = $state<Show | null>(null);
  let hovered = $state<number | null>(null);
  let epoch = $state(0);

  $effect(() => {
    const theme = view?.theme === 'accent' || !view ? store.settings?.general.theme : view.theme;
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
  });

  const animate = $derived(!!view?.animate && !reducedMotion());
  const centre = $derived(view ? view.size / 2 : 0);
  const mid = $derived(view ? (view.inner + view.outer) / 2 : 0);

  const point = (r: number, a: number) => [centre + r * Math.sin(a), centre - r * Math.cos(a)];

  function sector(i: number, n: number, inner: number, outer: number) {
    const step = (Math.PI * 2) / n;
    const a0 = i * step - step / 2 + GAP;
    const a1 = i * step + step / 2 - GAP;
    const [ox0, oy0] = point(outer, a0);
    const [ox1, oy1] = point(outer, a1);
    const [ix1, iy1] = point(inner, a1);
    const [ix0, iy0] = point(inner, a0);
    return `M${ox0} ${oy0}A${outer} ${outer} 0 0 1 ${ox1} ${oy1}L${ix1} ${iy1}A${inner} ${inner} 0 0 0 ${ix0} ${iy0}Z`;
  }

  const shown = (s: string) => (s.length > 11 ? `${s.slice(0, 10)}…` : s);
  const centreText = $derived.by(() => {
    if (!view) return '';
    const s = hovered === null ? null : view.slices[hovered];
    if (s && !s.empty) return s.label;
    return view.back ? 'Back' : '';
  });

  onMount(() => {
    store.init();
    applyCssVars();
    const offs = [
      listen<Show>('radial://show', (e) => {
        view = e.payload;
        hovered = null;
        epoch += 1;
      }),
      listen<number | null>('radial://hover', (e) => (hovered = e.payload)),
      listen('radial://hide', () => {
        view = null;
        hovered = null;
      }),
    ];
    return () => offs.forEach((o) => o.then((f) => f()));
  });
</script>

{#if view}
  {#key epoch}
    <div class="wheel" class:animate style:width="{view.size}px" style:height="{view.size}px">
      <svg width={view.size} height={view.size} viewBox="0 0 {view.size} {view.size}" aria-hidden="true">
        {#each view.slices as slice, i (i)}
          <path class="slice" class:on={hovered === i && !slice.empty} class:empty={slice.empty} d={sector(i, view.slices.length, view.inner, view.outer)} />
        {/each}
        <circle class="hub" class:on={hovered === null && view.back} cx={centre} cy={centre} r={view.inner - 6} />
      </svg>
      {#each view.slices as slice, i (i)}
        {#if !slice.empty}
          {@const [x, y] = point(mid, (i * Math.PI * 2) / view.slices.length)}
          <div class="item" class:on={hovered === i} style:left="{x}px" style:top="{y}px">
            {#if slice.icon in icons}
              <Icon name={slice.icon as IconName} size={20} />
            {:else}
              <span class="initial">{slice.label.slice(0, 1).toUpperCase()}</span>
            {/if}
            {#if view.labels}<span class="name">{shown(slice.label)}</span>{/if}
            {#if slice.submenu}<span class="more" aria-hidden="true"></span>{/if}
          </div>
        {/if}
      {/each}
      <div class="centre" style:left="{centre}px" style:top="{centre}px" style:width="{(view.inner - 8) * 2}px">{centreText}</div>
    </div>
  {/key}
{/if}

<style>
  :global(html),
  :global(body) {
    background: transparent !important;
  }
  .wheel {
    position: relative;
  }
  .wheel.animate {
    animation: pop var(--m-switcherShow-dur) var(--m-switcherShow-ease) both;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: scale(0.88);
    }
  }
  svg {
    position: absolute;
    inset: 0;
  }
  .slice {
    fill: var(--surface);
    stroke: var(--line-strong);
    stroke-width: 1;
  }
  .slice.on {
    fill: var(--acc);
    stroke: var(--acc);
  }
  .slice.empty {
    fill: var(--inset);
    opacity: 0.6;
  }
  .hub {
    fill: var(--surface);
    stroke: var(--line-strong);
  }
  .hub.on {
    fill: var(--hover);
  }
  .item {
    position: absolute;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    width: 76px;
    transform: translate(-50%, -50%);
    color: var(--fg);
    font-size: 11px;
    line-height: 1.2;
    text-align: center;
    pointer-events: none;
  }
  .item.on {
    color: var(--acc-fg);
  }
  .initial {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    font-size: 15px;
    font-weight: 600;
  }
  .more {
    position: absolute;
    top: -4px;
    right: 14px;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: currentColor;
  }
  .centre {
    position: absolute;
    transform: translate(-50%, -50%);
    color: var(--fg-soft);
    font-size: 12px;
    font-weight: 600;
    text-align: center;
    pointer-events: none;
  }
</style>
