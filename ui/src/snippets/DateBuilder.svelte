<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import Switch from '../lib/Switch.svelte';
  import { allZones, buildFormat, dateToken, DEFAULT_PARTS, offsetOf, validZone, zonesFor, type Parts } from './vars';

  let { oninsert }: { oninsert: (token: string) => void } = $props();

  let parts = $state<Parts>({ ...DEFAULT_PARTS });
  let format = $state(buildFormat(DEFAULT_PARTS));
  let zone = $state('local');
  let zoneQuery = $state('');
  let picking = $state(false);
  let preview = $state('');
  let previewFailed = $state(false);

  const token = $derived(dateToken(format, zone));
  const zoneOk = $derived(validZone(zone));

  const now = new Date();
  const ex = {
    ddd: now.toLocaleDateString('en-GB', { weekday: 'short' }),
    dddd: now.toLocaleDateString('en-GB', { weekday: 'long' }),
    D: String(now.getDate()),
    DD: String(now.getDate()).padStart(2, '0'),
    M: String(now.getMonth() + 1),
    MM: String(now.getMonth() + 1).padStart(2, '0'),
    MMM: now.toLocaleDateString('en-GB', { month: 'short' }),
    MMMM: now.toLocaleDateString('en-GB', { month: 'long' }),
    YY: String(now.getFullYear()).slice(2),
    YYYY: String(now.getFullYear()),
  };

  function setPart<K extends keyof Parts>(key: K, value: Parts[K]) {
    parts[key] = value;
    format = buildFormat(parts);
  }

  const offsetLabel = (mins: number) => {
    const sign = mins < 0 ? '-' : '+';
    const a = Math.abs(mins);
    return `UTC${sign}${String(Math.floor(a / 60)).padStart(2, '0')}:${String(a % 60).padStart(2, '0')}`;
  };

  const matches = $derived.by(() => {
    const q = zoneQuery.trim().toLowerCase().replace(/\s+/g, '_');
    if (!q) return [];
    const out = allZones().filter((z) => z.toLowerCase().includes(q)).slice(0, 8);
    if (/^[+-]\d{2}:\d{2}$/.test(zoneQuery.trim())) out.unshift(zoneQuery.trim());
    return out;
  });

  function zoneHint(z: string) {
    if (z === 'local' || z === 'UTC' || /^[+-]/.test(z)) return '';
    try {
      return offsetLabel(offsetOf(z));
    } catch {
      return '';
    }
  }

  function pickZone(z: string) {
    zone = z;
    zoneQuery = '';
    picking = false;
  }

  $effect(() => {
    const t = token;
    if (!zoneOk || !format.trim()) {
      preview = '';
      return;
    }
    const timer = setTimeout(() => {
      invoke<string>('snippet_preview', { text: t, zones: zonesFor(t) })
        .then((s) => {
          preview = s;
          previewFailed = false;
        })
        .catch(() => (previewFailed = true));
    }, 150);
    return () => clearTimeout(timer);
  });
</script>

<div class="builder">
  <div class="grid">
    <label>
      <span>Weekday</span>
      <select value={parts.weekday} onchange={(e) => setPart('weekday', e.currentTarget.value as Parts['weekday'])}>
        <option value="">None</option>
        <option value="ddd">Short ({ex.ddd})</option>
        <option value="dddd">Long ({ex.dddd})</option>
      </select>
    </label>
    <label>
      <span>Day</span>
      <select value={parts.day} onchange={(e) => setPart('day', e.currentTarget.value as Parts['day'])}>
        <option value="">None</option>
        <option value="D">{ex.D}</option>
        <option value="DD">{ex.DD} (two digits)</option>
      </select>
    </label>
    <label>
      <span>Month</span>
      <select value={parts.month} onchange={(e) => setPart('month', e.currentTarget.value as Parts['month'])}>
        <option value="">None</option>
        <option value="M">Number ({ex.M})</option>
        <option value="MM">Number ({ex.MM})</option>
        <option value="MMM">Short ({ex.MMM})</option>
        <option value="MMMM">Long ({ex.MMMM})</option>
      </select>
    </label>
    <label>
      <span>Year</span>
      <select value={parts.year} onchange={(e) => setPart('year', e.currentTarget.value as Parts['year'])}>
        <option value="">None</option>
        <option value="YY">{ex.YY}</option>
        <option value="YYYY">{ex.YYYY}</option>
      </select>
    </label>
    <label>
      <span>Order</span>
      <select value={parts.order} onchange={(e) => setPart('order', e.currentTarget.value as Parts['order'])}>
        <option value="dmy">Day, month, year</option>
        <option value="mdy">Month, day, year</option>
        <option value="ymd">Year, month, day</option>
      </select>
    </label>
    <label>
      <span>Between parts</span>
      <select value={parts.sep} onchange={(e) => setPart('sep', e.currentTarget.value)}>
        <option value="-">Dash  -</option>
        <option value="/">Slash  /</option>
        <option value=".">Dot  .</option>
        <option value=" ">Space</option>
      </select>
    </label>
    <label>
      <span>Time</span>
      <select value={parts.time} onchange={(e) => setPart('time', e.currentTarget.value as Parts['time'])}>
        <option value="">None</option>
        <option value="24">24-hour (14:05)</option>
        <option value="12">12-hour (2:05 PM)</option>
      </select>
    </label>
    <div class="toggle">
      <span>Seconds</span>
      <Switch checked={parts.seconds} label="Seconds" onchange={(on) => setPart('seconds', on)} />
    </div>
    <div class="toggle">
      <span>UTC offset</span>
      <Switch checked={parts.offset} label="UTC offset" onchange={(on) => setPart('offset', on)} />
    </div>
  </div>

  <label class="wide">
    <span>Format</span>
    <input class="mono" type="text" bind:value={format} spellcheck="false" aria-describedby="fmt-help" />
    <small id="fmt-help">YYYY YY MMMM MMM MM M DD D dddd ddd HH H hh h mm ss A a Z. Put other words in [square brackets].</small>
  </label>

  <div class="wide">
    <span class="label">Time zone</span>
    <div class="zones" role="radiogroup" aria-label="Time zone">
      <button role="radio" class="chip" aria-checked={zone === 'local'} onclick={() => pickZone('local')}>This computer</button>
      <button role="radio" class="chip" aria-checked={zone === 'UTC'} onclick={() => pickZone('UTC')}>UTC</button>
      {#if zone !== 'local' && zone !== 'UTC'}
        <button role="radio" class="chip" aria-checked="true" class:bad={!zoneOk} onclick={() => (picking = true)}>
          {zone.replaceAll('_', ' ')}{#if zoneHint(zone)}<small>{zoneHint(zone)}</small>{/if}
        </button>
      {/if}
      <button class="chip ghosted" onclick={() => (picking = !picking)}>Other zone…</button>
    </div>
    {#if picking}
      <div class="zone-search">
        <input
          type="text"
          placeholder="Search cities or type an offset like +05:30"
          bind:value={zoneQuery}
          spellcheck="false"
          onkeydown={(e) => {
            if (e.key === 'Enter' && matches[0]) {
              e.preventDefault();
              pickZone(matches[0]);
            } else if (e.key === 'Escape') {
              e.stopPropagation();
              picking = false;
            }
          }}
        />
        {#if matches.length}
          <ul class="matches">
            {#each matches as z (z)}
              <li><button onclick={() => pickZone(z)}><span>{z.replaceAll('_', ' ')}</span><small>{zoneHint(z)}</small></button></li>
            {/each}
          </ul>
        {:else if zoneQuery.trim()}
          <p class="none">No zone matches “{zoneQuery.trim()}”.</p>
        {/if}
      </div>
    {/if}
  </div>

  <div class="result">
    <div class="out">
      <span class="label">Preview</span>
      <strong class:muted={previewFailed || !preview}>
        {#if !format.trim()}Choose some parts or type a format.{:else if !zoneOk}Unknown time zone.{:else if previewFailed}Preview unavailable.{:else}{preview || ' '}{/if}
      </strong>
      <code>{token}</code>
    </div>
    <button class="ghost primary" disabled={!format.trim() || !zoneOk} onclick={() => oninsert(token)}>Insert</button>
  </div>
</div>

<style>
  .builder {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--bg);
    container-type: inline-size;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
  }
  @container (max-width: 420px) {
    .grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  label,
  .toggle,
  .wide {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .toggle {
    justify-content: space-between;
  }
  label > span,
  .toggle > span,
  .label {
    color: var(--mut);
    font-size: 11.5px;
    font-weight: 600;
  }
  select,
  input {
    width: 100%;
    min-width: 0;
    padding: 5px 8px;
    border: 1px solid var(--line-strong);
    border-radius: 7px;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    font-size: 12.5px;
    user-select: text;
  }
  input:focus,
  select:focus {
    outline: none;
    border-color: var(--acc);
    box-shadow: 0 0 0 3px var(--acc-soft);
  }
  .mono {
    font-family: var(--mono);
  }
  small {
    color: var(--mut);
    font-size: 11.5px;
  }
  .zones {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border: 1px solid var(--line-strong);
    border-radius: 99px;
    background: var(--surface);
    color: var(--fg-soft);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
    transition:
      border-color var(--m-selectionMove-dur) var(--m-selectionMove-ease),
      background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .chip:hover {
    background: var(--hover);
  }
  .chip[aria-checked='true'] {
    border-color: var(--acc);
    background: var(--acc-soft);
    color: var(--fg);
    font-weight: 600;
  }
  .chip.bad {
    border-color: var(--warn);
    color: var(--warn);
  }
  .chip.ghosted {
    border-style: dashed;
  }
  .zone-search {
    margin-top: 6px;
  }
  .matches {
    margin: 4px 0 0;
    padding: 4px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    list-style: none;
  }
  .matches button {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    width: 100%;
    padding: 5px 8px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--fg);
    font: inherit;
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
  }
  .matches button:hover,
  .matches li:first-child button {
    background: var(--hover);
  }
  .none {
    margin: 6px 2px 0;
    color: var(--mut);
    font-size: 12px;
  }
  .result {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    padding-top: 10px;
    border-top: 1px solid var(--line);
  }
  .out {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .out strong {
    font-size: 14px;
    font-weight: 600;
    word-break: break-word;
    user-select: text;
  }
  .out strong.muted {
    color: var(--mut);
    font-weight: 400;
  }
  .out code {
    color: var(--mut);
    font: 11.5px var(--mono);
    word-break: break-all;
    user-select: text;
  }
  .ghost.primary {
    border-color: var(--acc);
    background: var(--acc);
    color: var(--acc-fg);
    font-weight: 600;
  }
  .ghost.primary:hover:not(:disabled) {
    background: var(--acc);
    filter: brightness(1.06);
  }
</style>
