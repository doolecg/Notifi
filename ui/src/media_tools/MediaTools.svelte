<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { onMount } from 'svelte';
  import { applyCssVars } from '../lib/motion';

  interface Status {
    ffmpeg: boolean;
    format: string;
    quality: number;
    maxSide: number;
    targetMb: number;
  }
  interface Row {
    path: string;
    status: 'waiting' | 'running' | 'done' | 'exists' | 'error' | 'cancelled';
    percent: number | null;
    output: string | null;
    bytes: number | null;
    message: string | null;
  }
  type Action = 'image' | 'video' | 'gif';

  const IMAGE = /\.(png|jpe?g|bmp|webp)$/i;
  const VIDEO = /\.(mp4|mov|mkv|avi|webm|m4v|wmv|flv|gif)$/i;
  const FORMATS = [
    ['png', 'PNG'],
    ['jpeg', 'JPEG'],
    ['webp', 'WebP'],
    ['avif', 'AVIF'],
  ];

  let status = $state<Status | null>(null);
  let rows = $state<Row[]>([]);
  let action = $state<Action>('image');
  let format = $state('webp');
  let quality = $state(85);
  let maxSide = $state(0);
  let busy = $state(false);
  let over = $state(false);
  let error = $state('');

  const name = (p: string) => p.split(/[\\/]/).pop() ?? p;
  const mb = (n: number) => `${(n / 1_048_576).toFixed(n < 10_485_760 ? 2 : 1)} MB`;
  const usable = (p: string) => (action === 'image' ? IMAGE.test(p) : VIDEO.test(p));
  const needsFfmpeg = $derived(action !== 'image' && status !== null && !status.ffmpeg);
  const ready = $derived(rows.filter((r) => usable(r.path)));

  function add(paths: string[]) {
    const have = new Set(rows.map((r) => r.path));
    const fresh = paths.filter((p) => !have.has(p) && (IMAGE.test(p) || VIDEO.test(p)));
    if (!fresh.length) return;
    rows = [...rows, ...fresh.map((path) => ({ path, status: 'waiting' as const, percent: null, output: null, bytes: null, message: null }))];
    if (rows.every((r) => !IMAGE.test(r.path)) && action === 'image') action = 'video';
    else if (rows.some((r) => IMAGE.test(r.path)) && rows.every((r) => IMAGE.test(r.path))) action = 'image';
  }

  async function run(only?: number[], overwrite?: string) {
    error = '';
    const picks = (only ?? rows.map((_, i) => i)).filter((i) => usable(rows[i].path));
    if (!picks.length) {
      error = 'None of these files fit that action.';
      return;
    }
    // Progress events carry the position inside this run's file list.
    map = picks;
    for (const i of picks) rows[i] = { ...rows[i], status: 'waiting', percent: null, message: null, output: null };
    busy = true;
    try {
      await invoke('media_run', {
        job: { files: picks.map((i) => rows[i].path), action, format, quality, maxSide, overwrite },
      });
    } catch (e) {
      busy = false;
      error = String(e);
    }
  }

  let map: number[] = [];

  const clear = () => (rows = rows.filter((r) => r.status === 'running'));
  const cancel = () => invoke('media_cancel').catch(() => {});
  const reveal = (path: string) => invoke('apps_clean_reveal', { path }).catch(() => {});

  onMount(() => {
    applyCssVars();
    invoke<Status>('media_status').then((s) => {
      status = s;
      format = s.format;
      quality = s.quality;
      maxSide = s.maxSide;
    });
    const offs = [
      getCurrentWebview().onDragDropEvent((e) => {
        const t = e.payload.type;
        over = t === 'enter' || t === 'over';
        if (t === 'drop') add(e.payload.paths);
      }),
      listen<{ index: number; status: Row['status']; percent: number | null; output: string | null; bytes: number | null; message: string | null }>(
        'media://progress',
        (e) => {
          const i = map[e.payload.index];
          if (i === undefined || !rows[i]) return;
          const { status: s, percent, output, bytes, message } = e.payload;
          rows[i] = { ...rows[i], status: s, percent: percent ?? rows[i].percent, output: output ?? rows[i].output, bytes: bytes ?? rows[i].bytes, message };
        },
      ),
      listen('media://finished', () => (busy = false)),
    ];
    return () => offs.forEach((p) => p.then((off) => off()));
  });
</script>

<main class:over>
  <header>
    <h1>Media tools</h1>
    <p>Drop images or videos here. Everything runs on this computer.</p>
  </header>

  <div class="options">
    <label>
      <span>Action</span>
      <select bind:value={action} disabled={busy}>
        <option value="image">Convert or shrink images</option>
        <option value="video">Compress video</option>
        <option value="gif">Make a GIF from video</option>
      </select>
    </label>
    {#if action === 'image'}
      <label>
        <span>Format</span>
        <select bind:value={format} disabled={busy}>
          {#each FORMATS as [v, l] (v)}<option value={v}>{l}</option>{/each}
        </select>
      </label>
      <label>
        <span>Quality {quality}%</span>
        <input type="range" min="10" max="100" step="5" bind:value={quality} disabled={busy} />
      </label>
      <label>
        <span>Longest side (px, 0 = original)</span>
        <input type="number" min="0" max="20000" step="100" bind:value={maxSide} disabled={busy} />
      </label>
    {:else}
      <p class="note">
        {#if needsFfmpeg}
          Needs ffmpeg. Install it and make sure <code>ffmpeg</code> runs from a terminal, then reopen this window.
        {:else}
          Preset, target size, and GIF options come from the Media tools settings{status && status.targetMb > 0 ? ` (target ${status.targetMb} MB)` : ''}.
        {/if}
      </p>
    {/if}
  </div>

  <section class="list" aria-live="polite">
    {#if rows.length === 0}
      <div class="empty">Drop files here</div>
    {/if}
    {#each rows as r, i (r.path)}
      <div class="row" class:skip={!usable(r.path)}>
        <span class="nm" title={r.path}>{name(r.path)}</span>
        <span class="st">
          {#if r.status === 'running'}
            {r.percent ?? 0}%
          {:else if r.status === 'done'}
            Saved{r.bytes ? ` (${mb(r.bytes)})` : ''}
          {:else if r.status === 'exists'}
            Already exists
          {:else if r.status === 'error'}
            Failed
          {:else if r.status === 'cancelled'}
            Cancelled
          {:else if !usable(r.path)}
            Not for this action
          {/if}
        </span>
        {#if r.status === 'done' && r.output}
          <button class="small" onclick={() => reveal(r.output!)}>Show in folder</button>
        {:else if r.status === 'exists'}
          <button class="small" onclick={() => run([i], 'replace')} disabled={busy}>Replace</button>
        {/if}
        {#if r.message}<span class="msg">{r.message}</span>{/if}
      </div>
    {/each}
  </section>

  {#if error}<p class="error">{error}</p>{/if}

  <footer>
    <button onclick={clear} disabled={busy || rows.length === 0}>Clear list</button>
    <span class="grow"></span>
    {#if busy}
      <button onclick={cancel}>Cancel</button>
    {:else}
      <button class="primary" onclick={() => run()} disabled={ready.length === 0 || needsFfmpeg}>
        Start{ready.length ? ` (${ready.length})` : ''}
      </button>
    {/if}
  </footer>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    gap: 14px;
    height: 100vh;
    box-sizing: border-box;
    padding: 18px;
    background: var(--bg);
    color: var(--fg);
    font-family: var(--sans);
    font-size: 13px;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  main.over {
    background: var(--acc-soft);
  }
  h1 {
    margin: 0 0 4px;
    font-size: 18px;
  }
  header p,
  .note {
    margin: 0;
    color: var(--mut);
  }
  .options {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 12px;
    align-items: end;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--mut);
  }
  select,
  input[type='number'] {
    padding: 6px 8px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
  }
  code {
    font-family: var(--mono);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    border-radius: 10px;
    background: var(--surface);
  }
  .empty {
    display: grid;
    place-items: center;
    height: 100%;
    min-height: 140px;
    color: var(--mut);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    padding: 9px 12px;
  }
  .row + .row {
    border-top: 1px solid var(--line);
  }
  .row.skip {
    opacity: 0.55;
  }
  .nm {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .st {
    color: var(--mut);
  }
  .msg {
    flex-basis: 100%;
    color: var(--warn);
  }
  .error {
    margin: 0;
    color: var(--warn);
  }
  footer {
    display: flex;
    gap: 8px;
  }
  .grow {
    flex: 1;
  }
  button {
    padding: 7px 14px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  button:hover:not(:disabled) {
    background: var(--hover);
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  button.primary {
    border-color: transparent;
    background: var(--acc);
    color: var(--acc-fg);
  }
  button.small {
    padding: 3px 9px;
  }
</style>
