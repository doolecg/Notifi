<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { fadeSlide } from '../lib/motion';
  import './apps-clean.css';
  import { bytes, days, message } from './util';

  interface File {
    path: string;
    name: string;
    kind: string;
    size: number;
    ageDays: number;
    eligible: boolean;
    reason: string;
    duplicateOf: string | null;
  }
  interface Group {
    app: string;
    size: number;
    eligibleSize: number;
    files: File[];
  }
  interface Outcome {
    moved: number;
    freed: number;
    failed: [string, string][];
  }

  let { values }: { values: Record<string, unknown> } = $props();

  let groups = $state<Group[]>([]);
  let picked = $state<Set<string>>(new Set());
  let scanning = $state(false);
  let applying = $state(false);
  let scanned = $state(false);
  let showAll = $state(false);
  let error = $state('');
  let outcome = $state<Outcome | null>(null);

  const organise = $derived(values.action === 'organise');
  const kinds: Record<string, string> = { images: 'Images', video: 'Video', audio: 'Audio', documents: 'Documents', voice: 'Voice notes', stickers: 'Stickers and GIFs' };
  const total = $derived(groups.flatMap((g) => g.files).filter((f) => picked.has(f.path)).reduce((n, f) => n + f.size, 0));

  async function scan() {
    scanning = true;
    error = '';
    outcome = null;
    try {
      groups = await invoke<Group[]>('messaging_scan');
      picked = new Set(groups.flatMap((g) => g.files).filter((f) => f.eligible).map((f) => f.path));
      scanned = true;
    } catch (e) {
      error = message(e);
    }
    scanning = false;
  }

  function flip(path: string) {
    const next = new Set(picked);
    if (!next.delete(path)) next.add(path);
    picked = next;
  }

  function byKind(files: File[]): [string, File[]][] {
    const map = new Map<string, File[]>();
    for (const f of files.filter((f) => showAll || f.eligible)) map.set(f.kind, [...(map.get(f.kind) ?? []), f]);
    return [...map.entries()];
  }

  async function apply() {
    applying = true;
    error = '';
    try {
      const done = await invoke<Outcome>('messaging_apply', { paths: [...picked] });
      await scan();
      outcome = done;
    } catch (e) {
      error = message(e);
    }
    applying = false;
  }
</script>

<div class="ac">
  <div class="ac-bar">
    <span class="ac-note grow" style="margin:0">
      {#if scanning}<span class="ac-spin"></span>Reading file names and dates only{:else if scanned}This is what the rules would {organise ? 'sort into folders' : 'move to the bin'}. Nothing has been touched.{:else}Preview what the rules would do before anything is touched.{/if}
    </span>
    <label class="ac-note" style="margin:0"><input type="checkbox" bind:checked={showAll} /> Show files that stay</label>
    {#if scanning}
      <button class="ghost" onclick={() => invoke('apps_clean_cancel')}>Cancel</button>
    {:else}
      <button class="ghost" onclick={scan}>{scanned ? 'Scan again' : 'Preview'}</button>
    {/if}
  </div>
  {#if error}<p class="ac-note warn">{error}</p>{/if}

  {#if scanned}
    <div class="ac-list">
      {#if !groups.length}<p class="ac-empty">No messaging downloads were found.</p>{/if}
      {#each groups as g (g.app)}
        <div in:fadeSlide={{ token: 'listReorder', y: 4 }}>
          <div class="ac-item" style="background:var(--inset)">
            <span class="ac-main"><b>{g.app}</b><small>{g.files.length} files, {bytes(g.size)} in all</small></span>
            <span class="ac-size">{bytes(g.eligibleSize)} eligible</span>
          </div>
          {#each byKind(g.files) as [kind, files] (kind)}
            <div class="ac-item" style="min-height:30px"><span class="ac-main"><small>{kinds[kind] ?? kind}</small></span></div>
            {#each files as f (f.path)}
              <div class="ac-item">
                <input class="ac-check" type="checkbox" checked={picked.has(f.path)} disabled={!f.eligible} onchange={() => flip(f.path)} aria-label="Include {f.name}" />
                <span class="ac-main">
                  <b title={f.path}>{f.name}</b>
                  <small>{f.reason} · {days(f.ageDays)} old{f.duplicateOf ? ' · duplicate' : ''}</small>
                </span>
                <span class="ac-size">{bytes(f.size)}</span>
              </div>
            {/each}
          {/each}
        </div>
      {/each}
    </div>
    <div class="ac-bar" style="margin-top:12px">
      <span class="ac-note grow" style="margin:0">{picked.size} files, {bytes(total)}. Files moved to the bin can be restored from it.</span>
      <button class="primary" onclick={apply} disabled={applying || scanning || !picked.size}>{applying ? 'Working' : organise ? 'Sort into folders' : 'Move to the bin'}</button>
    </div>
  {/if}

  {#if outcome}
    <div in:fadeSlide={{ token: 'listReorder', y: 4 }}>
      <p class="ac-note" class:warn={outcome.failed.length > 0}>{outcome.moved} file(s) {organise ? 'sorted' : 'moved'}, {bytes(outcome.freed)}{outcome.failed.length ? `; ${outcome.failed.length} failed` : ''}.</p>
      {#each outcome.failed as [path, why] (path)}
        <p class="ac-note warn">{path}: {why}</p>
      {/each}
    </div>
  {/if}
</div>
