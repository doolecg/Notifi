<script lang="ts" module>
  /** Provider colours, as each tool presents itself. */
  export const PROVIDER_COLORS: Record<string, string> = {
    claude: '#d97757',
    codex: '#7b8ceb',
    openCode: '#c9cccd',
    copilot: '#c9cccd',
    antigravity: '#6ea8fe',
  };

  export const LOGOS = new Set(['claude', 'codex', 'openCode', 'opencode', 'copilot', 'antigravity']);

  // Notifi's own simple marks, not the tools' artwork.
  const rays = Array.from({ length: 12 }, (_, i) => {
    const a = (i * Math.PI) / 6 + Math.PI / 12;
    const r = i % 2 ? 7.2 : 9.4;
    const x = (v: number) => (12 + Math.cos(a) * v).toFixed(2);
    const y = (v: number) => (12 + Math.sin(a) * v).toFixed(2);
    return `M${x(2.2)} ${y(2.2)}L${x(r)} ${y(r)}`;
  }).join(' ');
</script>

<script lang="ts">
  let { id, size = 16 }: { id: string; size?: number } = $props();
  const kind = $derived(id === 'opencode' ? 'openCode' : id);
  const color = $derived(PROVIDER_COLORS[kind] ?? '#c9cccd');
</script>

<svg class="logo" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" style:color>
  {#if kind === 'claude'}
    <path d={rays} stroke="currentColor" stroke-width="2.4" stroke-linecap="round" fill="none" />
  {:else if kind === 'codex'}
    <g fill="none" stroke="currentColor" stroke-width="1.6">
      <rect x="8" y="3" width="8" height="18" rx="4" />
      <rect x="8" y="3" width="8" height="18" rx="4" transform="rotate(60 12 12)" />
      <rect x="8" y="3" width="8" height="18" rx="4" transform="rotate(120 12 12)" />
    </g>
  {:else if kind === 'openCode'}
    <rect x="3.5" y="3.5" width="17" height="17" rx="3" fill="none" stroke="currentColor" stroke-width="1.6" />
    <rect x="8.5" y="7" width="7" height="10" rx="1" fill="currentColor" />
  {:else if kind === 'copilot'}
    <rect x="3.5" y="5.5" width="17" height="13" rx="5" fill="none" stroke="currentColor" stroke-width="1.6" />
    <circle cx="9" cy="12" r="1.6" fill="currentColor" />
    <circle cx="15" cy="12" r="1.6" fill="currentColor" />
  {:else if kind === 'antigravity'}
    <path d="M5 19L12 5l7 14M8.6 14.2h6.8" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
  {:else}
    <circle cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="1.6" />
  {/if}
</svg>

<style>
  .logo {
    flex: none;
  }
</style>
