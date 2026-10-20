<script lang="ts">
  import { fadeSlide } from '../lib/motion';

  export interface Choice {
    label: string;
    danger?: boolean;
    run: () => void;
  }

  let { title, body, choices, oncancel }: { title: string; body: string; choices: Choice[]; oncancel: () => void } = $props();
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && oncancel()} />

<div class="scrim" role="presentation" onclick={oncancel}>
  <div class="dialog" role="alertdialog" aria-label={title} tabindex="-1" in:fadeSlide={{ token: 'toastIn', y: 8 }} onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
    <h3>{title}</h3>
    <p>{body}</p>
    <div class="actions">
      <button class="ghost" onclick={oncancel}>Cancel</button>
      {#each choices as c}
        <button class="ghost" class:danger={c.danger} onclick={c.run}>{c.label}</button>
      {/each}
    </div>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: grid;
    place-items: center;
    background: rgb(0 0 0 / 0.35);
  }
  .dialog {
    width: min(440px, calc(100vw - 48px));
    padding: 18px 20px;
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    background: var(--surface);
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.25);
  }
  h3 {
    margin: 0 0 6px;
    font-size: 15px;
  }
  p {
    margin: 0 0 16px;
    color: var(--fg-soft);
    font-size: 13.5px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .danger {
    border-color: var(--warn);
    color: var(--warn);
  }
</style>
