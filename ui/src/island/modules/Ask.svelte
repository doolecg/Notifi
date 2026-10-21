<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { fadeSlide } from '../../lib/motion';
  import { answerFor, type AskQuestion } from '../ask';
  import Glyph from '../Glyph.svelte';
  import Logo from '../Logo.svelte';
  import { island } from '../state.svelte';

  let { onDone, onChat }: { onDone: () => void; onChat?: () => void } = $props();

  let at = $state(0);
  const ask = $derived(island.asks[Math.min(at, island.asks.length - 1)]);

  /** Picked option labels and typed replies per request id and question text. */
  let picked = $state<Record<string, string[]>>({});
  let typed = $state<Record<string, string>>({});
  let reason = $state('');
  let busy = $state(false);
  let error = $state('');

  const key = (id: number, q: AskQuestion) => `${id}|${q.question}`;

  function pick(id: number, q: AskQuestion, label: string) {
    const k = key(id, q);
    const now = picked[k] ?? [];
    if (q.multiSelect) picked[k] = now.includes(label) ? now.filter((l) => l !== label) : [...now, label];
    else {
      picked[k] = now[0] === label ? [] : [label];
      typed[k] = '';
    }
  }

  const complete = $derived(
    !!ask && ask.kind === 'question' && ask.questions.every((q) => answerFor(q, picked[key(ask.id, q)] ?? [], typed[key(ask.id, q)] ?? '') !== ''),
  );

  async function run(id: number, call: Promise<unknown>) {
    busy = true;
    error = '';
    try {
      await call;
      const last = island.asks.length <= 1;
      island.removeAsk(id);
      if (last) onDone();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  const submit = () => {
    const a = ask;
    if (!a || !complete) return;
    const answers: Record<string, string> = {};
    for (const q of a.questions) answers[q.question] = answerFor(q, picked[key(a.id, q)] ?? [], typed[key(a.id, q)] ?? '');
    return run(a.id, invoke('ask_answer', { id: a.id, answers }));
  };
  const decide = (allow: boolean) => ask && run(ask.id, invoke('ask_decide', { id: ask.id, allow, message: allow ? null : reason.trim() || null }));
  const skip = () => ask && run(ask.id, invoke('ask_skip', { id: ask.id }));

  function keydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && complete) submit();
  }
</script>

<svelte:window onkeydown={keydown} />

{#if ask}
  <div class="ask">
    <header>
      <span class="who">
        <Logo id={ask.agent} size={16} />
        <b>{ask.agent === 'claude' ? 'Claude Code' : ask.agent}</b>
        {#if ask.project}<span class="proj">{ask.project}</span>{/if}
      </span>
      {#if island.asks.length > 1}
        <span class="pager">
          <button class="icon" aria-label="Previous" disabled={at === 0} onclick={() => (at = Math.max(0, at - 1))}><Glyph name="back" size={14} /></button>
          <small>{Math.min(at, island.asks.length - 1) + 1} / {island.asks.length}</small>
          <button class="icon next" aria-label="Next" disabled={at >= island.asks.length - 1} onclick={() => (at = Math.min(island.asks.length - 1, at + 1))}><Glyph name="back" size={14} /></button>
        </span>
      {/if}
      {#if onChat}
        <button class="chat-switch" onclick={onChat}><Glyph name="chat" size={14} />Prompt chat</button>
      {/if}
    </header>

    {#key ask.id}
      <div class="body" in:fadeSlide={{ token: 'tabSwitch' }}>
        {#if ask.kind === 'question'}
          {#each ask.questions as q (q.question)}
            {@const k = key(ask.id, q)}
            <section class="q">
              {#if q.header}<span class="chip">{q.header}</span>{/if}
              <h3>{q.question}</h3>
              <div class="opts">
                {#each q.options as o (o.label)}
                  <button class="opt" class:on={(picked[k] ?? []).includes(o.label)} aria-pressed={(picked[k] ?? []).includes(o.label)} onclick={() => pick(ask.id, q, o.label)}>
                    <span class="mark" class:box={q.multiSelect}></span>
                    <span class="txt"><b>{o.label}</b>{#if o.description}<small>{o.description}</small>{/if}</span>
                  </button>
                {/each}
              </div>
              <input class="reply" type="text" placeholder={q.multiSelect ? 'Add your own answer' : 'Or type your own answer'} bind:value={typed[k]} oninput={() => { if (!q.multiSelect && typed[k]) picked[k] = []; }} />
            </section>
          {/each}
        {:else}
          <section class="q">
            <span class="chip">Permission</span>
            <h3>Allow {ask.tool}?</h3>
            <pre class="sum">{ask.summary}</pre>
            <input class="reply" type="text" placeholder="Reason if you say no (optional)" bind:value={reason} />
          </section>
        {/if}
      </div>
    {/key}

    {#if error}<p class="err">{error}</p>{/if}

    <footer>
      <button class="link" onclick={skip} disabled={busy}>Answer in the terminal</button>
      {#if ask.kind === 'question'}
        <button class="go" disabled={!complete || busy} onclick={submit}>Submit</button>
      {:else}
        <span class="pair">
          <button class="no" disabled={busy} onclick={() => decide(false)}>Deny</button>
          <button class="go" disabled={busy} onclick={() => decide(true)}>Allow</button>
        </span>
      {/if}
    </footer>
  </div>
{:else}
  <p class="none">No questions waiting.</p>
{/if}

<style>
  .ask {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    gap: 10px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 13px;
    color: var(--i-mut);
  }
  .who {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .who b {
    color: var(--i-fg);
  }
  .proj {
    padding: 1px 8px;
    border-radius: 99px;
    background: color-mix(in srgb, var(--i-fg), transparent 90%);
    font-size: 11.5px;
  }
  .pager {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .pager .next :global(svg) {
    transform: scaleX(-1);
  }
  .chat-switch {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border: 0;
    border-radius: 99px;
    background: color-mix(in srgb, var(--i-fg), transparent 90%);
    color: var(--i-fg);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }
  .chat-switch:hover {
    background: color-mix(in srgb, var(--i-fg), transparent 82%);
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    scrollbar-width: none;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .q {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .chip {
    align-self: flex-start;
    padding: 2px 9px;
    border-radius: 99px;
    background: color-mix(in srgb, var(--i-acc), transparent 78%);
    color: var(--i-acc);
    font-size: 11.5px;
    font-weight: 700;
  }
  h3 {
    margin: 0;
    color: var(--i-fg);
    font-size: 16px;
    font-weight: 700;
    line-height: 1.3;
  }
  .opts {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 12px;
    border: 1px solid color-mix(in srgb, var(--i-fg), transparent 86%);
    border-radius: 12px;
    background: color-mix(in srgb, var(--i-fg), transparent 95%);
    color: var(--i-fg);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .opt:hover {
    background: color-mix(in srgb, var(--i-fg), transparent 90%);
  }
  .opt.on {
    border-color: var(--i-acc);
    background: color-mix(in srgb, var(--i-acc), transparent 84%);
  }
  .mark {
    flex: none;
    width: 14px;
    height: 14px;
    border: 2px solid color-mix(in srgb, var(--i-fg), transparent 55%);
    border-radius: 50%;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease), border-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .mark.box {
    border-radius: 4px;
  }
  .on .mark {
    border-color: var(--i-acc);
    background: var(--i-acc);
  }
  .txt {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .txt b {
    font-weight: 600;
  }
  .txt small {
    color: var(--i-mut);
    font-size: 12px;
  }
  .reply {
    padding: 7px 12px;
    border: 1px solid color-mix(in srgb, var(--i-fg), transparent 86%);
    border-radius: 12px;
    background: transparent;
    color: var(--i-fg);
    font: inherit;
  }
  .reply:focus {
    outline: none;
    border-color: var(--i-acc);
  }
  .sum {
    max-height: 150px;
    margin: 0;
    padding: 10px 12px;
    overflow: auto;
    border-radius: 12px;
    background: color-mix(in srgb, var(--i-fg), transparent 93%);
    color: var(--i-fg);
    font: 12px/1.45 var(--mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .pair {
    display: inline-flex;
    gap: 8px;
  }
  .go,
  .no {
    padding: 8px 20px;
    border: 0;
    border-radius: 99px;
    font: inherit;
    font-weight: 700;
    cursor: pointer;
  }
  .go {
    background: var(--i-fg);
    color: var(--i-bg);
  }
  .no {
    background: color-mix(in srgb, var(--i-fg), transparent 88%);
    color: var(--i-fg);
  }
  .go:disabled,
  .no:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .link {
    border: 0;
    background: none;
    color: var(--i-mut);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }
  .link:hover:not(:disabled) {
    color: var(--i-fg);
  }
  .err {
    margin: 0;
    color: #ff6b5e;
    font-size: 12px;
  }
  .none {
    margin: auto;
    color: var(--i-mut);
  }
</style>
