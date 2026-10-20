<script lang="ts">
  import KeyCapture from '../lib/KeyCapture.svelte';
  import ListEditor from '../lib/ListEditor.svelte';
  import Switch from '../lib/Switch.svelte';
  import { fadeSlide } from '../lib/motion';
  import { store, type Card, type Opt, type Page } from '../lib/settings.svelte';
  import { clash } from '../lib/shortcuts';
  import Icon from '../panel/Icon.svelte';
  import { icons, type IconName } from '../panel/icons';
  import { customFor } from './customs';

  let {
    page,
    card,
    option,
    namespace,
    goto,
  }: { page: Page; card: Card; option: Opt; namespace: string; goto: (page: string, option?: string) => void } = $props();

  const values = $derived(store.values(namespace));
  const control = $derived(option.control);
  const value = $derived(values[option.id]);
  const Custom = $derived(control.kind === 'custom' ? customFor(page, option) : undefined);
  let error = $state('');
  /** Slider position while dragging, before it is saved. */
  let dragging = $state<number | null>(null);

  async function set(key: string, v: unknown) {
    error = '';
    try {
      await store.setOption(namespace, key, v);
    } catch (err) {
      error = String(err);
    }
  }

  const note = $derived(control.kind === 'shortcut' ? clash(`${namespace}.${option.id}`, String(value ?? '')) : null);
  const iconOf = (name: string) => (name in icons ? (name as IconName) : null);
</script>

{#if control.kind === 'custom'}
  {#if Custom}
    <Custom {page} {card} {option} {values} set={(k: string, v: unknown) => set(k, v)} {goto} />
  {:else}
    <div class="row">
      <span class="name">{option.label}<small>{option.hint || 'Arrives with this feature.'}</small></span>
      <span class="soon">Coming later</span>
    </div>
  {/if}
{:else if control.kind === 'tiles'}
  <div class="row stacked">
    <span class="name">{option.label}{#if option.hint}<small>{option.hint}</small>{/if}</span>
    <div class="tiles" role="radiogroup" aria-label={option.label} style:--n={control.choices.length}>
      {#each control.choices as choice (choice.value)}
        {@const icon = iconOf(choice.icon)}
        <button role="radio" class="tile" aria-checked={value === choice.value} onclick={() => set(option.id, choice.value)}>
          {#if icon}<Icon name={icon} size={20} />{:else if control.choices.some((c) => iconOf(c.icon))}<span class="no-icon"></span>{/if}
          <span>{choice.label}</span>
        </button>
      {/each}
    </div>
  </div>
{:else if control.kind === 'list'}
  <div class="row stacked">
    <span class="name">{option.label}{#if option.hint}<small>{option.hint}</small>{/if}</span>
  </div>
  <ListEditor items={(value as string[]) ?? []} placeholder={control.placeholder} label={option.label} onchange={(items) => set(option.id, items)} />
{:else}
  <div class="row">
    <span class="name">
      {option.label}
      {#if error || note}
        <small class="warn" transition:fadeSlide={{ token: 'toastIn', y: -4 }}>{error || note}</small>
      {:else if option.hint}
        <small>{option.hint}</small>
      {/if}
    </span>
    {#if control.kind === 'switch'}
      <Switch checked={value === true} label={option.label} onchange={(on) => set(option.id, on)} />
    {:else if control.kind === 'picker'}
      <select aria-label={option.label} value={value} onchange={(e) => set(option.id, e.currentTarget.value)}>
        {#each control.choices as choice (choice.value)}
          <option value={choice.value}>{choice.label}</option>
        {/each}
      </select>
    {:else if control.kind === 'slider'}
      <span class="slider">
        <input
          type="range"
          aria-label={option.label}
          min={control.min}
          max={control.max}
          step={control.step}
          value={Number(value)}
          oninput={(e) => (dragging = Number(e.currentTarget.value))}
          onchange={(e) => set(option.id, Number(e.currentTarget.value)).then(() => (dragging = null))}
        />
        <output>{dragging ?? value}{control.unit}</output>
      </span>
    {:else if control.kind === 'number'}
      <span class="number">
        <input
          type="number"
          aria-label={option.label}
          min={control.min}
          max={control.max}
          step={control.step}
          value={Number(value)}
          onchange={(e) => {
            const v = Number(e.currentTarget.value);
            if (Number.isFinite(v)) set(option.id, v);
          }}
        />
        {#if control.unit}<span class="unit">{control.unit}</span>{/if}
      </span>
    {:else if control.kind === 'text'}
      <input
        class="text"
        type="text"
        aria-label={option.label}
        placeholder={control.placeholder}
        value={String(value ?? '')}
        onchange={(e) => set(option.id, e.currentTarget.value)}
      />
    {:else if control.kind === 'shortcut'}
      <span class="keys">
        <KeyCapture value={String(value ?? '')} label={option.label} onchange={(accel) => set(option.id, accel)} />
        {#if value}<button class="ghost" title="Remove" onclick={() => set(option.id, '')}>Clear</button>{/if}
      </span>
    {/if}
  </div>
{/if}

<style>
  .stacked {
    flex-direction: column;
    align-items: stretch;
    gap: 10px;
  }
  .warn {
    color: var(--warn) !important;
  }
  .soon {
    flex: none;
    padding: 3px 10px;
    border-radius: 99px;
    background: var(--inset);
    color: var(--mut);
    font-size: 12px;
    font-weight: 600;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(var(--n), 1fr);
    gap: 8px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 12px 8px;
    border: 1.5px solid var(--line-strong);
    border-radius: 10px;
    background: var(--surface);
    color: var(--fg-soft);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
    transition:
      border-color var(--m-selectionMove-dur) var(--m-selectionMove-ease),
      background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .no-icon {
    height: 20px;
  }
  .tile:hover {
    background: var(--hover);
  }
  .tile[aria-checked='true'] {
    border-color: var(--acc);
    background: var(--acc-soft);
    color: var(--fg);
    font-weight: 600;
  }
  select,
  input.text,
  .number input {
    padding: 6px 10px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    font-size: 13px;
    user-select: text;
  }
  select {
    max-width: 260px;
  }
  input.text {
    width: 220px;
  }
  .number {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .number input {
    width: 90px;
    text-align: right;
  }
  .unit,
  output {
    color: var(--mut);
    font-size: 12.5px;
  }
  .slider {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .slider input {
    width: 160px;
    accent-color: var(--acc);
  }
  output {
    min-width: 44px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .keys {
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
