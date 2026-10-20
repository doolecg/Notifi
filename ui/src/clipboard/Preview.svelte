<script lang="ts">
  import { LOOK_ICON, LOOK_LABEL, fullTime, lookOf, prettyJson, splitPath, title, type Entry } from './entries';
  import Glyph from './Glyph.svelte';
  import Highlight from './Highlight.svelte';
  import { image, want } from './images.svelte';

  let {
    entry,
    stack,
    words,
    autoPaste,
    onpaste,
    oncopy,
    onpin,
    ondelete,
  }: {
    entry: Entry | null;
    stack: Entry[];
    words: string[];
    autoPaste: boolean;
    onpaste: () => void;
    oncopy: () => void;
    onpin: () => void;
    ondelete: () => void;
  } = $props();

  const look = $derived(entry ? lookOf(entry) : 'text');
  const full = $derived(entry?.kind === 'image' ? (image(entry.id, true) ?? image(entry.id)) : undefined);
  const paths = $derived(entry ? (entry.files.length ? entry.files : entry.text.split('\n').filter(Boolean)) : []);

  $effect(() => {
    if (entry?.kind === 'image') want(entry.id, true);
  });
</script>

<div class="preview">
  {#if stack.length > 1}
    <div class="head">
      <span class="tag"><Glyph name="stack" size={13} />Stack</span>
      <span class="meta">{stack.length} items, pasted top to bottom</span>
    </div>
    <ol class="stack">
      {#each stack as s (s.id)}
        <li><Glyph name={LOOK_ICON[lookOf(s)]} size={14} /><span>{title(s) || ' '}</span></li>
      {/each}
    </ol>
    <div class="actions">
      <button class="act primary" onclick={onpaste}><Glyph name="paste" size={15} />{autoPaste ? 'Paste' : 'Copy'} {stack.length}</button>
      <button class="act" onclick={oncopy}><Glyph name="copy" size={15} />Copy</button>
      <span class="grow"></span>
      <button class="act icon" title="Delete {stack.length} items" aria-label="Delete {stack.length} items" onclick={ondelete}><Glyph name="trash" size={15} /></button>
    </div>
  {:else if entry}
    <div class="head">
      <span class="tag"><Glyph name={LOOK_ICON[look]} size={13} />{LOOK_LABEL[look]}</span>
      {#if entry.kind === 'rich' && look !== 'rich'}<span class="tag">Formatted</span>{/if}
      <span class="meta" title={fullTime(entry.created)}>
        {entry.app ? `${entry.app} · ${fullTime(entry.created)}` : fullTime(entry.created)}
      </span>
    </div>

    <div class="body" class:center={look === 'image' || look === 'color'}>
      {#if look === 'image'}
        {#if full}
          <img class="full" src={full} alt="Copied image, {entry.width} by {entry.height} pixels" draggable="false" />
        {:else if full === null}
          <span class="muted">No preview for this image.</span>
        {:else}
          <span class="loading"></span>
        {/if}
      {:else if look === 'color'}
        <span class="swatch" style:background={entry.text.trim()}></span>
        <code class="color-code"><Highlight text={entry.text.trim()} {words} /></code>
      {:else if look === 'files'}
        <ul class="files">
          {#each paths as p, i (i)}
            {@const parts = splitPath(p)}
            <li title={p}>
              <Glyph name="files" size={15} />
              <span class="path"><span class="file"><Highlight text={parts.name} {words} /></span><span class="dir"><Highlight text={parts.dir} {words} /></span></span>
            </li>
          {/each}
        </ul>
      {:else if look === 'url'}
        <span class="link"><Highlight text={entry.text.trim()} {words} /></span>
      {:else if look === 'json'}
        <pre class="code"><Highlight text={prettyJson(entry)} {words} /></pre>
      {:else}
        <pre class="plain"><Highlight text={entry.text} {words} /></pre>
        {#if entry.length > entry.text.length}
          <span class="muted more">Showing {entry.text.length.toLocaleString()} of {entry.length.toLocaleString()} characters.</span>
        {/if}
      {/if}
    </div>

    <div class="foot">
      <span class="muted">
        {#if entry.kind === 'image'}{entry.width} x {entry.height} px
        {:else if entry.kind === 'files'}{paths.length} {paths.length === 1 ? 'item' : 'items'}
        {:else}{entry.length.toLocaleString()} {entry.length === 1 ? 'character' : 'characters'}{/if}
      </span>
    </div>
    <div class="actions">
      <button class="act primary" onclick={onpaste}><Glyph name="paste" size={15} />{autoPaste ? 'Paste' : 'Copy'}</button>
      {#if autoPaste}<button class="act" onclick={oncopy}><Glyph name="copy" size={15} />Copy</button>{/if}
      <span class="grow"></span>
      <button class="act icon" class:on={entry.pinned} aria-pressed={entry.pinned} title={entry.pinned ? 'Unpin' : 'Pin'} aria-label={entry.pinned ? 'Unpin' : 'Pin'} onclick={onpin}><Glyph name="pin" size={15} /></button>
      <button class="act icon" title="Delete" aria-label="Delete" onclick={ondelete}><Glyph name="trash" size={15} /></button>
    </div>
  {/if}
</div>

<style>
  .preview {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    padding: 12px 14px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    flex: none;
  }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
    padding: 2px 8px;
    border-radius: 99px;
    background: var(--inset);
    color: var(--fg-soft);
    font-size: 11.5px;
    font-weight: 600;
  }
  .meta {
    overflow: hidden;
    margin-left: 4px;
    color: var(--mut);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .body {
    flex: 1;
    min-height: 0;
    margin: 10px 0 6px;
    overflow: auto;
    user-select: text;
  }
  .body.center {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
  }
  pre {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
    font-size: 13px;
    line-height: 1.5;
  }
  .plain {
    font-family: var(--sans);
  }
  .code {
    font: 12.5px/1.55 var(--mono);
  }
  .full {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: 6px;
    background:
      repeating-conic-gradient(var(--inset) 0 25%, var(--surface) 0 50%) 0 0 / 14px 14px;
    box-shadow: 0 0 0 1px var(--line);
  }
  .swatch {
    width: min(160px, 70%);
    aspect-ratio: 1.6;
    border-radius: 12px;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.12), 0 4px 14px rgb(0 0 0 / 0.12);
  }
  .color-code {
    font: 600 13px var(--mono);
  }
  .link {
    color: var(--acc);
    text-decoration: underline;
    text-underline-offset: 3px;
    word-break: break-all;
    font-size: 13px;
  }
  .files,
  .stack {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .files li,
  .stack li {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 5px 4px;
    color: var(--mut);
    font-size: 13px;
  }
  .stack {
    flex: 1;
    min-height: 0;
    margin: 10px 0 6px;
    overflow: auto;
  }
  .stack li span {
    overflow: hidden;
    color: var(--fg);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .path {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .file {
    color: var(--acc);
    font-weight: 600;
    word-break: break-all;
  }
  .dir {
    font-size: 11.5px;
    word-break: break-all;
  }
  .muted {
    color: var(--mut);
    font-size: 12px;
  }
  .more {
    display: block;
    margin-top: 8px;
  }
  .loading {
    width: 22px;
    height: 22px;
    border: 2px solid var(--line-strong);
    border-top-color: var(--acc);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(1turn);
    }
  }
  .foot {
    flex: none;
    margin-bottom: 8px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }
  .grow {
    flex: 1;
  }
  .act {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 12px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg-soft);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease),
      color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .act:hover {
    background: var(--hover);
    color: var(--fg);
  }
  .act.primary {
    border-color: var(--acc);
    background: var(--acc);
    color: var(--acc-fg);
    font-weight: 600;
  }
  .act.primary:hover {
    filter: brightness(1.06);
  }
  .act.icon {
    width: 30px;
    padding: 0;
    justify-content: center;
  }
  .act.on {
    color: var(--acc);
    border-color: var(--acc);
    background: var(--acc-soft);
  }
  .act:focus-visible {
    outline: 2px solid var(--acc);
    outline-offset: 1px;
  }
</style>
