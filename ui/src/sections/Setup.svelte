<script lang="ts">
  import { fadeSlide } from '../lib/motion';
  import { store } from '../lib/settings.svelte';
  import PageIcon from './PageIcon.svelte';

  /** The first launch: pick what to install out of the box. Skipping installs nothing. */
  let { onDone }: { onDone: () => void } = $props();

  const groups = $derived(store.featureGroups());
  const all = $derived(groups.flatMap((g) => g.pages.map((p) => p.id)));
  const recommended = $derived((store.catalog?.recommended ?? []).filter((id) => all.includes(id)));
  let picked = $state<Record<string, boolean>>(Object.fromEntries((store.catalog?.recommended ?? []).map((id) => [id, true])));
  let busy = $state(false);
  let error = $state('');

  const ids = $derived(all.filter((id) => picked[id]));
  const same = (a: string[], b: string[]) => a.length === b.length && a.every((id) => b.includes(id));
  const preset = $derived(!ids.length ? 'none' : same(ids, all) ? 'all' : same(ids, recommended) ? 'recommended' : '');

  function choose(list: string[]) {
    picked = Object.fromEntries(list.map((id) => [id, true]));
  }

  async function finish(install: string[]) {
    busy = true;
    error = '';
    try {
      await store.finishSetup(install);
      onDone();
    } catch (e) {
      error = String(e);
      busy = false;
    }
  }
</script>

<div class="setup" role="dialog" aria-modal="true" aria-labelledby="setup-title" in:fadeSlide={{ token: 'tabSwitch' }}>
  <div class="inner">
    <h1 id="setup-title">Choose what to install</h1>
    <p class="lead">Notifi only runs what you install, so it stays light. Anything you skip now is one click away on the Install features page.</p>

    <div class="presets" role="group" aria-label="Starting points">
      <button class:on={preset === 'none'} onclick={() => choose([])}>Just the basics</button>
      <button class:on={preset === 'recommended'} onclick={() => choose(recommended)}>Recommended</button>
      <button class:on={preset === 'all'} onclick={() => choose(all)}>Everything</button>
    </div>

    {#each groups as g (g.id)}
      <h3 class="group-title">{g.label}</h3>
      <div class="grid">
        {#each g.pages as p (p.id)}
          <label class="item" class:on={picked[p.id]}>
            <input type="checkbox" bind:checked={picked[p.id]} />
            <PageIcon page={p} group={g.id} size={26} />
            <span class="text"><span class="label">{p.label}</span><span class="blurb">{p.blurb}</span></span>
          </label>
        {/each}
      </div>
    {/each}
  </div>

  <footer>
    {#if error}<span class="err">{error}</span>{/if}
    <span class="count">{ids.length} selected</span>
    <button class="ghost" disabled={busy} onclick={() => finish([])}>Skip for now</button>
    <button class="go" disabled={busy} onclick={() => finish(ids)}>{ids.length ? `Install ${ids.length} ${ids.length === 1 ? 'feature' : 'features'}` : 'Continue with the basics'}</button>
  </footer>
</div>

<style>
  .setup {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .inner {
    flex: 1;
    min-height: 0;
    width: 100%;
    max-width: 820px;
    margin: 0 auto;
    padding: 34px 32px 16px;
    overflow-y: auto;
  }
  h1 {
    margin: 0 0 6px;
    font-size: 24px;
  }
  .lead {
    margin: 0 0 16px;
    color: var(--mut);
    font-size: 14px;
  }
  .presets {
    display: inline-flex;
    padding: 2px;
    margin-bottom: 6px;
    border: 1px solid var(--line);
    border-radius: 9px;
    background: var(--surface);
  }
  .presets button {
    padding: 6px 16px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--fg-soft);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .presets button.on {
    background: var(--acc);
    color: var(--acc-fg);
    font-weight: 600;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 8px;
  }
  .item {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--surface);
    cursor: pointer;
    transition: border-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .item:hover {
    border-color: var(--line-strong);
  }
  .item.on {
    border-color: var(--acc);
    background: var(--acc-soft);
  }
  .item input {
    margin: 6px 0 0;
    accent-color: var(--acc);
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .label {
    font-weight: 600;
    font-size: 13.5px;
  }
  .blurb {
    color: var(--mut);
    font-size: 12px;
    line-height: 1.4;
  }
  footer {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: flex-end;
    gap: 12px;
    padding: 12px 32px;
    border-top: 1px solid var(--line);
    background: var(--sidebar);
  }
  .count {
    margin-right: auto;
    color: var(--mut);
    font-size: 13px;
  }
  .err {
    color: var(--warn);
    font-size: 13px;
  }
  .go {
    padding: 8px 18px;
    border: 1px solid var(--acc);
    border-radius: 9px;
    background: var(--acc);
    color: var(--acc-fg);
    font: inherit;
    font-size: 13.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .go:disabled {
    opacity: 0.55;
    cursor: default;
  }
</style>
