<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import Logo, { PROVIDER_COLORS } from '../Logo.svelte';
  import { island, runOutside } from '../state.svelte';
  import { clock, type AgentStats, type LimitView, type ProviderId, type ToolInfo } from '../types';

  const POLL_MS = 5000;
  const SPLIT_IDS = ['claude', 'codex', 'openCode'] as const;
  const PERIODS = [
    { id: 'today', label: 'Today', days: 1 },
    { id: '7d', label: '7 days', days: 7 },
    { id: '30d', label: '30 days', days: 30 },
  ] as const;
  const WEEKDAY = ['M', 'T', 'W', 'T', 'F', 'S', 'S'];
  const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  const API_VALUE_HINT =
    'What the same work would cost at pay-per-token list prices. Subscription plans charge a fixed amount instead.';

  let stats = $state<AgentStats | null>(null);
  let periodId = $state<string>(island.snap?.config.agentsPeriod ?? '7d');
  let picking = $state(false);

  const cfg = $derived(island.snap?.config);
  const showLimits = $derived(cfg?.agentsShowLimits ?? true);
  const showResets = $derived(cfg?.agentsShowResets ?? true);
  const showModels = $derived(cfg?.agentsShowModels ?? true);
  const showProjects = $derived(cfg?.agentsShowProjects ?? true);
  const showActivity = $derived(cfg?.agentsShowActivity ?? true);

  const period = $derived(PERIODS.find((p) => p.id === periodId) ?? PERIODS[1]);
  const live = $derived(island.snap?.agents ?? []);
  const tools = $derived(stats?.tools ?? []);
  const sum = $derived(stats?.summary);
  const now = $derived(island.now);

  async function load() {
    try {
      stats = await invoke<AgentStats>('island_agent_stats', {
        days: period.days,
        tzOffsetMin: -new Date().getTimezoneOffset(),
      });
    } catch {
      /* the next poll tries again */
    }
  }

  type SessionRow = {
    id: string;
    agent: ProviderId;
    project: string | null;
    model: string | null;
    lastTs: number;
    usage: { read: number; written: number };
    working: boolean;
  };
  type LogLine = { ts: number; role: 'user' | 'assistant' | 'tool'; text: string };

  let filter = $state<ProviderId | null>(null);
  let sessions = $state<SessionRow[] | null>(null);
  let openId = $state<string | null>(null);
  let log = $state<LogLine[] | null>(null);

  async function loadSessions() {
    try {
      sessions = await invoke<SessionRow[]>('island_agent_sessions', { agent: filter });
    } catch {
      sessions = [];
    }
  }

  function pickFilter(f: ProviderId | null) {
    filter = f;
    openId = null;
    loadSessions();
  }

  async function toggleLog(id: string) {
    openId = openId === id ? null : id;
    log = null;
    if (!openId) return;
    try {
      const lines = await invoke<LogLine[]>('island_agent_log', { id });
      if (openId === id) log = lines;
    } catch {
      if (openId === id) log = [];
    }
  }

  onMount(() => {
    load();
    loadSessions();
    const t = setInterval(load, POLL_MS);
    return () => clearInterval(t);
  });

  function choose(id: string) {
    picking = false;
    if (id === periodId) return;
    periodId = id;
    load();
    invoke('set_section', { section: 'island', value: { agentsPeriod: id } }).catch(() => {});
  }

  function working(id: ProviderId) {
    return live.find((a) => a.id === id && a.working);
  }

  function lastActive(t: ToolInfo) {
    const a = live.find((x) => x.id === t.id);
    return Math.max(t.lastUsed ?? 0, a?.lastActive ?? 0) || null;
  }

  function countdown(ms: number) {
    const m = Math.max(0, Math.round(ms / 60_000));
    if (m >= 1440) return `${Math.floor(m / 1440)}d ${Math.floor((m % 1440) / 60)}h`;
    if (m >= 60) return `${Math.floor(m / 60)}h ${m % 60}m`;
    return `${m}m`;
  }

  function ago(t: number | null) {
    if (!t) return 'Not used yet';
    const s = Math.max(0, Math.round((now - t) / 1000));
    if (s < 60) return `${s} sec. ago`;
    if (s < 3600) return `${Math.floor(s / 60)} min. ago`;
    if (s < 86_400) return `${Math.floor(s / 3600)} hr. ago`;
    if (s < 14 * 86_400) return `${Math.floor(s / 86_400)} ${s < 2 * 86_400 ? 'day' : 'days'} ago`;
    if (s < 60 * 86_400) return `${Math.floor(s / (7 * 86_400))} wk. ago`;
    return `${Math.floor(s / (30 * 86_400))} mo. ago`;
  }

  function compact(n: number) {
    const f = (v: number, unit: string) => `${v >= 10 ? Math.round(v) : Math.round(v * 10) / 10}${unit}`;
    if (n >= 1e9) return f(n / 1e9, 'B');
    if (n >= 1e6) return f(n / 1e6, 'M');
    if (n >= 1e3) return f(n / 1e3, 'k');
    return String(n);
  }

  function money(v: number) {
    return v >= 100 ? `$${Math.round(v).toLocaleString('en-US')}` : `$${v.toFixed(2)}`;
  }

  /** Keeps both ends of a long name, like `Kingdoms…end.Client`. */
  function middle(s: string, max = 20) {
    if (s.length <= max) return s;
    const keep = Math.floor((max - 1) / 2);
    return `${s.slice(0, keep)}…${s.slice(-keep)}`;
  }

  /** Local day number to a date in UTC fields, so labels don't shift with the time zone again. */
  const dayDate = (d: number) => new Date(d * 86_400_000);
  const weekday = (d: number) => WEEKDAY[(dayDate(d).getUTCDay() + 6) % 7];
  const dayLabel = (d: number) => `${dayDate(d).getUTCDate()} ${MONTHS[dayDate(d).getUTCMonth()]}`;

  function trendLabel(i: number) {
    if (!sum) return '';
    if (sum.hourly) return i % 6 === 0 ? String(i) : '';
    const d = sum.trendFirst + i;
    if (sum.days <= 7) return weekday(d);
    return (sum.trend.length - 1 - i) % 7 === 0 ? String(dayDate(d).getUTCDate()) : '';
  }

  const trendMax = $derived(Math.max(1, ...(sum?.trend.map((t) => t[0] + t[1] + t[2]) ?? [1])));

  const heat = $derived.by(() => {
    if (!sum) return [];
    const max = Math.max(1, ...sum.activity);
    return sum.activity.map((t, i) => ({
      col: Math.floor(i / 7),
      row: i % 7,
      level: t === 0 ? 0 : Math.min(4, Math.ceil((t / max) * 4)),
      title: `${dayLabel(sum.activityFirst + i)} · ${compact(t)} tokens`,
    }));
  });

  function limitRow(l: LimitView) {
    return { pct: Math.min(100, Math.max(0, l.usedPercent)), left: l.resetsAt ? countdown(l.resetsAt - now) : null };
  }

  /** The plan limits an agent reports (its session and its week), from the live snapshot, with where use would be at an even rate. */
  function planLimits(id: ProviderId) {
    const a = live.find((x) => x.id === id);
    return (
      [
        ['Session', a?.sessionLimit],
        ['Week', a?.weekLimit],
      ] as const
    ).flatMap(([label, l]) => {
      if (!l) return [];
      const pace = l.resetsAt && l.windowMs ? Math.min(1, Math.max(0, 1 - (l.resetsAt - now) / l.windowMs)) : null;
      return [{ label, pct: Math.min(100, Math.max(0, l.usedPercent)), left: l.resetsAt ? countdown(l.resetsAt - now) : null, pace }];
    });
  }
</script>

{#snippet logoHead(t: ToolInfo)}
  {@const w = working(t.id)}
  <div class="head">
    <Logo id={t.id} size={17} />
    <span class="title">{t.id === 'claude' ? 'Claude' : t.name}</span>
    {#if w}<span class="busy" style:color={PROVIDER_COLORS[t.id]}><i></i>{clock(w.elapsedMs)}</span>{/if}
    {#if t.plan}<span class="badge {t.id}">{t.plan}</span>{/if}
  </div>
{/snippet}

{#snippet idle(t: ToolInfo)}
  <p class="line">No session running</p>
  <p class="dim">{ago(lastActive(t))}</p>
{/snippet}

{#snippet planBars(t: ToolInfo)}
  {#each planLimits(t.id) as r (r.label)}
    <div class="lrow">
      <span class="label">{r.label}</span>
      {#if r.left && showResets}<span class="reset"><span class="cyc">↻</span>{r.left}</span>{/if}
      <span class="value">{Math.round(r.pct)}%</span>
    </div>
    {@render bar(r.pct / 100, PROVIDER_COLORS[t.id], r.pace)}
  {/each}
{/snippet}

{#snippet bar(fill: number, color: string, pace: number | null = null)}
  <span class="track">
    <i style:width={`${Math.min(100, Math.max(0, fill * 100))}%`} style:background={color}></i>
    {#if pace !== null}<b class="pace" style:left={`calc(${pace * 100}% - 1px)`}></b>{/if}
  </span>
{/snippet}

<div class="agents">
  <h2>AI Agents</h2>
  {#if !stats}
    <p class="empty">Reading agent logs…</p>
  {:else if tools.length === 0}
    <p class="empty">No coding agents found on this computer.</p>
  {:else}
    <div class="scroll">
    <div class="grid">
      {#each tools as t (t.id)}
        <section class="tile">
          {@render logoHead(t)}
          {#if !showLimits}
            <p class="dim">{working(t.id) ? 'Working' : ago(lastActive(t))}</p>
          {:else if t.id === 'claude' && planLimits(t.id).length}
            {@render planBars(t)}
          {:else if t.id === 'claude'}
            {@const win = stats.claudeWindow}
            {#if win}
              {@const limit = stats.planLimitUsd}
              <div class="lrow">
                <span class="label">Session</span>
                {#if showResets}<span class="reset"><span class="cyc">↻</span>{countdown(win.resetsAt - now)}</span>{/if}
                <span class="value">{money(win.costUsd)}{#if limit > 0}<small> / {money(limit)}</small>{/if}</span>
              </div>
              {#if limit > 0}
                {@render bar(win.costUsd / limit, PROVIDER_COLORS.claude, win.pace)}
              {:else}
                {@render bar(win.pace, '#7c5041')}
                <button class="link" onclick={() => runOutside('agentLimits')}>↗ Set up plan limits…</button>
              {/if}
            {:else if !stats.ready}
              <p class="dim">Reading history…</p>
            {:else}
              {@render idle(t)}
            {/if}
          {:else if t.id === 'codex' && (stats.codexSession || stats.codexWeek)}
            {#each [['Session', stats.codexSession], ['Week', stats.codexWeek]] as const as [label, l] (label)}
              {#if l}
                {@const r = limitRow(l)}
                <div class="lrow">
                  <span class="label">{label}</span>
                  {#if r.left && showResets}<span class="reset"><span class="cyc">↻</span>{r.left}</span>{/if}
                  <span class="value">{Math.round(r.pct)}%</span>
                </div>
                {@render bar(r.pct / 100, PROVIDER_COLORS.codex, l.resetsAt ? l.pace : null)}
              {/if}
            {/each}
          {:else if t.id === 'codex' && working('codex')}
            <p class="dim">Limits appear after Codex's next reply.</p>
          {:else if t.id === 'antigravity' && (planLimits(t.id).length || t.used?.split.length)}
            <!-- Antigravity keeps Claude and GPT on one quota, so that has one meter; the tokens are counted for each kind of model on its own. -->
            {#if planLimits(t.id).length}
              {@render planBars(t)}
              <p class="dim">Session and Week are the quota Claude and GPT share</p>
            {/if}
            {#each t.used?.split ?? [] as s (s.name)}
              <div class="lrow">
                <span class="label">{s.name}</span>
                <span class="reset">{compact(s.fiveHour)} · 5h</span>
                <span class="value">{compact(s.week)}<small> week</small></span>
              </div>
            {/each}
          {:else if planLimits(t.id).length}
            {@render planBars(t)}
          {:else if t.used && (t.used.fiveHour > 0 || t.used.week > 0)}
            <!-- No plan limit is reported (OpenCode): the same windows, in tokens used. -->
            <div class="lrow"><span class="label">5 hours</span><span class="value">{compact(t.used.fiveHour)}<small> tokens</small></span></div>
            <div class="lrow"><span class="label">Week</span><span class="value">{compact(t.used.week)}<small> tokens</small></span></div>
            <p class="dim">No plan limit reported</p>
          {:else}
            {@render idle(t)}
          {/if}
        </section>
      {/each}

      {#if sum}
        <section class="tile spend" class:wide={tools.length % 2 === 0}>
          <div class="head">
            <svg class="ico" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="8.5" /><path d="M14.6 9.2c-.5-.8-1.5-1.2-2.6-1.2-1.5 0-2.6.8-2.6 1.9 0 2.6 5.3 1.3 5.3 4.1 0 1.1-1.2 2-2.7 2-1.2 0-2.2-.5-2.7-1.3M12 6.5V8m0 8v1.5" /></svg>
            <span class="title">Spending</span>
            <div class="picker">
              <button class="pick" onclick={() => (picking = !picking)} aria-haspopup="listbox" aria-expanded={picking}>
                {period.label}<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 10l5 5 5-5" /></svg>
              </button>
              {#if picking}
                <div class="menu" role="listbox">
                  {#each PERIODS as p (p.id)}
                    <button role="option" aria-selected={p.id === periodId} class:sel={p.id === periodId} onclick={() => choose(p.id)}>{p.label}</button>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
          <div class="big" title={API_VALUE_HINT}>
            {#if sum.minimum}<span class="ge">≥</span>{/if}{money(sum.costUsd)}<span class="unit">API value</span>
          </div>
          <span class="split">
            {#each sum.providers as pv, i (i)}
              {#if pv.costUsd > 0 || (sum.costUsd === 0 && pv.tokens > 0)}
                <i style:flex-grow={sum.costUsd > 0 ? pv.costUsd : pv.tokens} style:background={PROVIDER_COLORS[SPLIT_IDS[i]]}></i>
              {/if}
            {/each}
          </span>
          <p class="dim">{compact(sum.tokens)} tokens · {Math.round(sum.cacheShare * 100)}% from cache</p>
        </section>

        <section class="tile wide">
          <div class="head">
            <svg class="ico" viewBox="0 0 24 24" aria-hidden="true"><path d="M3 12h4l2.5-6 4 12 2.5-6h5" /></svg>
            <span class="title">Now</span>
          </div>
          {#each tools as t (t.id)}
            {@const w = working(t.id)}
            <div class="now">
              <Logo id={t.id} size={13} />
              <span class="nm">{t.id === 'claude' ? 'Claude' : t.name}</span>
              {#if w}
                <span class="when on" style:color={PROVIDER_COLORS[t.id]}>Working · {clock(w.elapsedMs)}</span>
              {:else}
                <span class="when">{ago(lastActive(t))}</span>
              {/if}
            </div>
          {/each}
        </section>

        <section class="tile wide">
          <div class="head">
            <svg class="ico" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 6h16M4 12h16M4 18h10" /></svg>
            <span class="title">Sessions</span>
            {#each [null, 'claude', 'codex', 'openCode', 'antigravity'] as f (f)}
              <button class="chip" class:sel={filter === f} onclick={() => pickFilter(f as ProviderId | null)}>{f === null ? 'All' : f === 'claude' ? 'Claude' : f === 'codex' ? 'Codex' : f === 'openCode' ? 'OpenCode' : 'Antigravity'}</button>
            {/each}
            <button class="chip" title="Refresh" onclick={loadSessions}>↻</button>
          </div>
          {#if sessions === null}
            <p class="dim">Reading sessions…</p>
          {:else if sessions.length === 0}
            <p class="dim">No session logs found.</p>
          {/if}
          {#each sessions ?? [] as s (s.id)}
            <button class="srow" class:open={openId === s.id} onclick={() => toggleLog(s.id)}>
              <Logo id={s.agent} size={13} />
              <span class="nm" title={s.model ?? ''}>{s.project ?? 'Unknown project'}</span>
              <span class="when" class:on={s.working} style:color={s.working ? PROVIDER_COLORS[s.agent] : null}>{s.working ? 'Working' : ago(s.lastTs || null)}</span>
              <span class="num wide-num">{compact(s.usage.read + s.usage.written)}</span>
            </button>
            {#if openId === s.id}
              <div class="log">
                {#if log === null}
                  <p class="dim">Reading log…</p>
                {:else if log.length === 0}
                  <p class="dim">Nothing readable in this log.</p>
                {/if}
                {#each log ?? [] as l, i (i)}
                  <p class="ll {l.role}"><b>{l.role === 'user' ? 'You' : l.role === 'tool' ? 'Tool' : 'Agent'}</b>{l.text}</p>
                {/each}
              </div>
            {/if}
          {/each}
        </section>

        <section class="tile wide">
          <div class="head">
            <svg class="ico" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20h16M6 17V9m4 8V5m4 12v-6m4 6V8" /></svg>
            <span class="title">Trend</span>
            <span class="aside">{period.label} · {compact(sum.tokens)} tokens</span>
          </div>
          <div class="bars" class:thin={sum.trend.length > 7}>
            {#each sum.trend as [c, x, o], i (i)}
              <div class="col">
                <div class="stack" title={`${compact(c + x + o)} tokens`}>
                  {#if c + x + o === 0}
                    <i class="zero"></i>
                  {:else}
                    {#if o > 0}<i style:height={`max(2px, ${(o / trendMax) * 100}%)`} style:background={PROVIDER_COLORS.openCode}></i>{/if}
                    {#if x > 0}<i style:height={`max(2px, ${(x / trendMax) * 100}%)`} style:background={PROVIDER_COLORS.codex}></i>{/if}
                    {#if c > 0}<i style:height={`max(2px, ${(c / trendMax) * 100}%)`} style:background={PROVIDER_COLORS.claude}></i>{/if}
                  {/if}
                </div>
                <span class="tick">{trendLabel(i)}</span>
              </div>
            {/each}
          </div>
        </section>

        {#each [['Models', sum.models], ['Projects', sum.projects]] as const as [label, list] (label)}
          {#if label === 'Models' ? showModels : showProjects}
          {@const max = Math.max(1, ...list.map((m) => m.tokens))}
          <section class="tile">
            <div class="head">
              {#if label === 'Models'}
                <svg class="ico" viewBox="0 0 24 24" aria-hidden="true"><rect x="7" y="7" width="10" height="10" rx="1.5" /><path d="M10 4v3m4-3v3m-4 10v3m4-3v3M4 10h3m-3 4h3m10-4h3m-3 4h3" /></svg>
              {:else}
                <svg class="ico" viewBox="0 0 24 24" aria-hidden="true"><path d="M3.5 7.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2Z" /></svg>
              {/if}
              <span class="title">{label}</span>
            </div>
            {#if list.length === 0}
              <p class="dim">No use in this period</p>
            {/if}
            {#each list as m (m.name)}
              <div class="rank">
                <span class="nm" title={m.name}>{middle(m.name)}</span>
                <span class="rbar"><i style:width={`max(3px, ${(m.tokens / max) * 100}%)`}
                  style:background={label === 'Models' ? PROVIDER_COLORS[m.provider] : '#e2e4e4'}></i></span>
                <span class="num">{compact(m.tokens)}</span>
              </div>
            {/each}
          </section>
          {/if}
        {/each}

        {#if showActivity}
        <section class="tile wide">
          <div class="head">
            <svg class="ico" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 5h3v3H5zM10.5 5h3v3h-3zM16 5h3v3h-3zM5 10.5h3v3H5zM10.5 10.5h3v3h-3zM16 10.5h3v3h-3zM5 16h3v3H5zM10.5 16h3v3h-3zM16 16h3v3h-3z" /></svg>
            <span class="title">Activity</span>
            {#if sum.streak > 0}
              <span class="streak" title={`${sum.streak} days in a row`}>
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 21c-3.9 0-6.5-2.6-6.5-6.1 0-3.3 2.4-5.4 3.6-8.4.3 1.8 1.2 3 2.4 3.6-.2-2.6.8-5.2 3-7.1-.2 3.3 4 5.6 4 10.6 0 4.6-2.8 7.4-6.5 7.4Z" /></svg>{sum.streak}
              </span>
            {/if}
          </div>
          <div class="act">
            <div class="heat" style:grid-template-columns={`repeat(${Math.max(1, Math.ceil(sum.activity.length / 7))}, 1fr)`}>
              {#each heat as c, i (i)}
                <i class={`l${c.level}`} style:grid-column={c.col + 1} style:grid-row={c.row + 1} title={c.title}></i>
              {/each}
            </div>
            <div class="facts">
              <strong>{compact(sum.activityTotal)}</strong>
              <span class="dim">13 weeks</span>
              <div class="fact"><span>Active days</span><b>{sum.activeDays}</b></div>
              <div class="fact"><span>Busiest day</span><b>{sum.busiest ? `${dayLabel(sum.busiest[0])} · ${compact(sum.busiest[1])}` : '—'}</b></div>
            </div>
          </div>
        </section>
        {/if}
      {/if}
    </div>
    </div>
  {/if}
</div>

<style>
  .agents {
    display: flex;
    flex-direction: column;
    gap: 8px;
    height: 100%;
    min-height: 0;
  }
  h2 {
    margin: 0 0 2px;
    font-size: 15px;
    font-weight: 700;
    color: var(--i-fg);
  }
  .empty {
    margin: auto;
    color: var(--i-mut);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    padding-right: 6px;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    grid-auto-rows: max-content;
    gap: 10px;
    padding-bottom: 4px;
  }
  .scroll::-webkit-scrollbar {
    width: 5px;
  }
  .scroll::-webkit-scrollbar-thumb {
    border-radius: 3px;
    background: #4a4d4f;
  }
  .tile {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    padding: 11px 12px 12px;
    border-radius: 14px;
    background: #111;
  }
  .wide {
    grid-column: 1 / -1;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 7px;
    min-height: 18px;
    margin-bottom: 2px;
  }
  .title {
    flex: 1;
    overflow: hidden;
    font-size: 12.5px;
    font-weight: 600;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .ico {
    flex: none;
    width: 15px;
    height: 15px;
    fill: none;
    stroke: var(--i-mut);
    stroke-width: 1.7;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .badge {
    padding: 1px 7px;
    border-radius: 7px;
    font-size: 10.5px;
    font-weight: 600;
    background: #262a2b;
    color: var(--i-mut);
  }
  .badge.claude {
    background: #2d1f1b;
    color: #d0805f;
  }
  .badge.codex {
    background: #22263d;
    color: #8798e8;
  }
  .busy {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .busy i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
    animation: pulse 1.2s ease-in-out infinite;
  }
  .lrow {
    display: flex;
    align-items: baseline;
    gap: 6px;
    font-size: 11.5px;
  }
  .label {
    font-weight: 600;
  }
  .reset {
    flex: 1;
    color: var(--i-mut);
    font-variant-numeric: tabular-nums;
  }
  .cyc {
    margin-right: 3px;
  }
  .value {
    font-size: 12.5px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .value small {
    color: var(--i-mut);
    font-size: 11px;
    font-weight: 500;
  }
  .track {
    position: relative;
    display: block;
    height: 8px;
    margin-bottom: 2px;
    border-radius: 4px;
    background: #303030;
  }
  .track i {
    display: block;
    height: 100%;
    border-radius: 4px;
  }
  .pace {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    border-radius: 1px;
    background: #ebeffc;
  }
  .link {
    align-self: flex-start;
    padding: 0;
    border: 0;
    background: none;
    color: #d0805f;
    font: inherit;
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .link:hover {
    text-decoration: underline;
  }
  .line {
    margin: 0;
    font-size: 11.5px;
    color: #c3c7c8;
  }
  .dim {
    margin: 0;
    font-size: 11px;
    color: var(--i-mut);
  }
  .picker {
    position: relative;
  }
  .pick {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 2px 4px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--i-mut);
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
  }
  .pick:hover {
    background: var(--i-hover);
  }
  .pick svg {
    width: 13px;
    height: 13px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .menu {
    position: absolute;
    top: 100%;
    right: 0;
    z-index: 2;
    display: flex;
    flex-direction: column;
    min-width: 92px;
    padding: 4px;
    border: 1px solid var(--i-line);
    border-radius: 9px;
    background: #1b1e1f;
    box-shadow: 0 8px 20px rgb(0 0 0 / 0.5);
  }
  .menu button {
    padding: 5px 8px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--i-fg);
    font: inherit;
    font-size: 11.5px;
    text-align: left;
    cursor: pointer;
  }
  .menu button:hover,
  .menu button.sel {
    background: var(--i-hover);
  }
  .big {
    display: flex;
    align-items: baseline;
    gap: 2px;
    font-size: 23px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    line-height: 1.1;
  }
  .ge {
    margin-right: 4px;
  }
  .unit {
    margin-left: 6px;
    color: var(--i-mut);
    font-size: 11px;
    font-weight: 500;
  }
  .split {
    display: flex;
    gap: 3px;
    height: 5px;
    overflow: hidden;
    border-radius: 3px;
    background: #303030;
  }
  .split i {
    flex-basis: 0;
    min-width: 6px;
    border-radius: 3px;
  }
  .now {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
  }
  .now .nm {
    flex: 1;
    font-weight: 600;
  }
  .when {
    color: var(--i-mut);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .when.on {
    font-weight: 600;
  }
  .aside {
    color: var(--i-mut);
    font-size: 11px;
  }
  .bars {
    display: flex;
    gap: 6px;
    height: 74px;
  }
  .bars.thin {
    gap: 2px;
  }
  .col {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .stack {
    display: flex;
    flex: 1;
    flex-direction: column;
    justify-content: flex-end;
    gap: 1px;
  }
  .stack i {
    display: block;
    border-radius: 3px;
  }
  .stack i:first-child {
    border-radius: 3px 3px 1px 1px;
  }
  .stack i.zero {
    height: 2px;
    background: #2a2a2a;
  }
  .tick {
    height: 11px;
    color: var(--i-mut);
    font-size: 9px;
    font-weight: 600;
    line-height: 11px;
    text-align: center;
  }
  .rank {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11.5px;
  }
  .rank .nm {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-weight: 600;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .rbar {
    flex: none;
    width: 44px;
  }
  .rbar i {
    display: block;
    height: 4px;
    border-radius: 2px;
  }
  .num {
    flex: none;
    width: 34px;
    color: var(--i-mut);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .streak {
    display: flex;
    align-items: center;
    gap: 2px;
    color: #f08a3c;
    font-size: 12px;
    font-weight: 700;
  }
  .streak svg {
    width: 13px;
    height: 13px;
    fill: currentColor;
  }
  .act {
    display: flex;
    gap: 16px;
    align-items: flex-start;
  }
  .heat {
    display: grid;
    flex: none;
    grid-template-rows: repeat(7, 8px);
    grid-auto-columns: 8px;
    gap: 3px;
    width: max-content;
  }
  .heat i {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: #222;
  }
  .heat i.l1 {
    background: #4a4c4d;
  }
  .heat i.l2 {
    background: #7d8081;
  }
  .heat i.l3 {
    background: #b4b7b8;
  }
  .heat i.l4 {
    background: #ececec;
  }
  .facts {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .facts strong {
    font-size: 18px;
    font-weight: 700;
    line-height: 1.1;
  }
  .facts .dim {
    margin-bottom: 8px;
  }
  .fact {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 11.5px;
  }
  .fact span {
    color: var(--i-mut);
  }
  .fact b {
    font-weight: 600;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .chip {
    padding: 2px 8px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--i-mut);
    font: inherit;
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
  }
  .chip:hover,
  .chip.sel {
    background: var(--i-hover);
    color: var(--i-fg);
  }
  .srow {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 4px 6px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--i-fg);
    font: inherit;
    font-size: 11.5px;
    text-align: left;
    cursor: pointer;
  }
  .srow:hover,
  .srow.open {
    background: var(--i-hover);
  }
  .srow .nm {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-weight: 600;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .wide-num {
    width: 40px;
  }
  .log {
    display: flex;
    flex-direction: column;
    gap: 3px;
    max-height: 220px;
    padding: 6px 8px;
    overflow-y: auto;
    overscroll-behavior: contain;
    border-radius: 8px;
    background: #181a1b;
  }
  .ll {
    margin: 0;
    font-size: 11px;
    line-height: 1.35;
    color: #c3c7c8;
    overflow-wrap: anywhere;
  }
  .ll b {
    margin-right: 6px;
    color: var(--i-mut);
    font-weight: 600;
  }
  .ll.tool {
    color: var(--i-mut);
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .busy i {
      animation: none;
    }
  }
</style>
