<script lang="ts">
  import Glyph from '../Glyph.svelte';

  const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
  const sameDay = (a: Date, b: Date) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();

  const today = new Date();
  let year = $state(today.getFullYear());
  let month = $state(today.getMonth());
  let selected = $state<Date>(new Date(today.getFullYear(), today.getMonth(), today.getDate()));

  const title = $derived(new Date(year, month, 1).toLocaleDateString(undefined, { month: 'long', year: 'numeric' }));
  const chosen = $derived(selected.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' }));

  /** Six weeks starting on the Monday on or before the 1st. */
  const cells = $derived.by(() => {
    const lead = (new Date(year, month, 1).getDay() + 6) % 7;
    return Array.from({ length: 42 }, (_, i) => new Date(year, month, 1 - lead + i));
  });

  function step(by: number) {
    const d = new Date(year, month + by, 1);
    year = d.getFullYear();
    month = d.getMonth();
  }
  function goToday() {
    const t = new Date();
    year = t.getFullYear();
    month = t.getMonth();
    selected = new Date(t.getFullYear(), t.getMonth(), t.getDate());
  }
  function pick(d: Date) {
    selected = d;
    if (d.getMonth() !== month) {
      year = d.getFullYear();
      month = d.getMonth();
    }
  }
</script>

<div class="cal">
  <header>
    <h2>{title}</h2>
    <span class="nav">
      <button class="today" onclick={goToday}>Today</button>
      <button class="icon prev" aria-label="Previous month" onclick={() => step(-1)}><Glyph name="back" size={16} /></button>
      <button class="icon next" aria-label="Next month" onclick={() => step(1)}><Glyph name="back" size={16} /></button>
    </span>
  </header>

  <div class="grid">
    {#each WEEKDAYS as w (w)}<span class="wd">{w}</span>{/each}
    {#each cells as d (d.getTime())}
      <button
        class="day"
        class:out={d.getMonth() !== month}
        class:today={sameDay(d, today)}
        class:sel={sameDay(d, selected)}
        aria-label={d.toDateString()}
        aria-pressed={sameDay(d, selected)}
        onclick={() => pick(d)}
      >{d.getDate()}</button>
    {/each}
  </div>

  <p class="chosen">{chosen}</p>
</div>

<style>
  .cal {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: 6px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--i-mut);
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  button {
    border: 0;
    background: none;
    color: var(--i-fg);
    font: inherit;
    cursor: pointer;
    transition: background-color var(--m-hoverLift-dur) var(--m-hoverLift-ease), transform var(--m-hoverLift-dur) var(--m-hoverLift-ease);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border-radius: 7px;
  }
  .next :global(svg) {
    transform: scaleX(-1);
  }
  .today {
    padding: 3px 9px;
    border-radius: 7px;
    font-size: 12px;
    color: var(--i-mut);
  }
  .icon:hover,
  .today:hover,
  .day:hover {
    background: var(--i-hover);
  }
  .icon:active,
  .today:active,
  .day:active {
    transform: scale(0.94);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    grid-template-rows: auto repeat(6, minmax(0, 1fr));
    flex: 1;
    min-height: 0;
    gap: 2px;
  }
  .wd {
    text-align: center;
    font-size: 11px;
    color: var(--i-mut);
  }
  .day {
    min-height: 0;
    padding: 0;
    border-radius: 7px;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .day.out {
    color: var(--i-mut);
    opacity: 0.45;
  }
  .day.today {
    color: var(--i-acc);
    font-weight: 700;
  }
  .day.sel {
    background: var(--i-acc);
    color: var(--i-bg);
    font-weight: 700;
  }
  .chosen {
    margin: 0;
    font-size: 11.5px;
    color: var(--i-mut);
    text-align: center;
  }
</style>
