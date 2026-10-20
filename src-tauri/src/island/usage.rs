//! Token history for the AI Agents dashboard (spec/everyday/ai-agents.md). Claude Code and Codex
//! logs and OpenCode's database are folded into hourly buckets (hour x tool x model x project), kept for 13 weeks and cached
//! in the app data folder with each file's read offset, so a restart only reads what is new.

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    agents::{self, Kind, Limit},
    opencode,
};

const MINUTE: u64 = 60_000;
const HOUR: u64 = 3_600_000;
const DAY: u64 = 86_400_000;
/// 13 weeks for the activity map, plus a week so its first column is complete.
pub const KEEP_DAYS: u64 = 98;
/// Per-minute Claude use is kept this long, to place the 5-hour session window.
const RECENT_MS: u64 = 48 * HOUR;
pub const CLAUDE_WINDOW_MS: u64 = 5 * HOUR;
pub const CODEX_SESSION_MS: u64 = 5 * HOUR;
pub const CODEX_WEEK_MS: u64 = 7 * DAY;
const CHUNK: u64 = 8 * 1024 * 1024;
const CACHE_VERSION: u32 = 1;
const NONE: u16 = u16::MAX;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    /// Fresh input, not counting cache reads or writes.
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Tokens {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }

    fn add(&mut self, o: &Tokens) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
    }

    fn sub(&mut self, o: &Tokens) {
        self.input = self.input.saturating_sub(o.input);
        self.output = self.output.saturating_sub(o.output);
        self.cache_read = self.cache_read.saturating_sub(o.cache_read);
        self.cache_write = self.cache_write.saturating_sub(o.cache_write);
    }
}

/// US dollars per million tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

const fn p(input: f64, output: f64, cache_read: f64) -> Price {
    // Cache writes (5-minute cache) are billed at 1.25x input on Claude; OpenAI has no write charge,
    // and its rows below only ever see cache reads.
    Price { input, output, cache_read, cache_write: input * 1.25 }
}

/// Pay-per-token list prices in USD per million tokens, matched by the longest model-id prefix.
/// These are public list prices at the time of writing and may change; models not listed are
/// costed at zero, so totals are shown as a minimum.
const PRICES: &[(&str, Price)] = &[
    ("claude-fable-5-1", p(10.0, 50.0, 0.25)),
    ("claude-fable-5", p(10.0, 50.0, 1.0)),
    ("claude-mythos-5-1", p(10.0, 50.0, 0.25)),
    ("claude-opus-5-5", p(4.0, 20.0, 0.20)),
    ("claude-opus-5", p(5.0, 25.0, 0.50)),
    ("claude-opus-4-8", p(5.0, 25.0, 0.50)),
    ("claude-opus-4-7", p(5.0, 25.0, 0.50)),
    ("claude-opus-4-6", p(5.0, 25.0, 0.50)),
    ("claude-opus-4-5", p(5.0, 25.0, 0.50)),
    ("claude-opus-4-1", p(15.0, 75.0, 1.50)),
    ("claude-opus-4", p(15.0, 75.0, 1.50)),
    ("claude-sonnet-5-5", p(2.0, 10.0, 0.20)),
    ("claude-sonnet-5", p(2.0, 10.0, 0.20)),
    ("claude-sonnet-4-6", p(3.0, 15.0, 0.30)),
    ("claude-sonnet-4-5", p(3.0, 15.0, 0.30)),
    ("claude-sonnet-4", p(3.0, 15.0, 0.30)),
    ("claude-3-7-sonnet", p(3.0, 15.0, 0.30)),
    ("claude-haiku-5-5", p(0.10, 0.50, 0.01)),
    ("claude-haiku-4-5", p(1.0, 5.0, 0.10)),
    ("claude-3-5-haiku", p(0.80, 4.0, 0.08)),
    ("gpt-5-nano", p(0.05, 0.40, 0.005)),
    ("gpt-5-mini", p(0.25, 2.0, 0.025)),
    ("gpt-5-codex", p(1.25, 10.0, 0.125)),
    ("gpt-5", p(1.25, 10.0, 0.125)),
];

pub fn price(model: &str) -> Option<Price> {
    let m = model.to_ascii_lowercase();
    let m = m.rsplit_once('/').map_or(m.as_str(), |(_, tail)| tail);
    PRICES.iter().filter(|(key, _)| exact_prefix(m, key)).max_by_key(|(key, _)| key.len()).map(|(_, p)| *p)
}

/// `m` starts with `key` and what follows doesn't continue the version number, so `claude-opus-5`
/// prices `claude-opus-5-20260101` but not `claude-opus-5-7`. Date suffixes are 8 digits.
fn exact_prefix(m: &str, key: &str) -> bool {
    let Some(rest) = m.strip_prefix(key) else { return false };
    if rest.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        return false;
    }
    match rest.strip_prefix('-') {
        Some(tail) => {
            let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
            digits == 0 || digits >= 8
        }
        None => true,
    }
}

/// API value in USD, `None` when the model has no known price.
pub fn cost(model: &str, t: &Tokens) -> Option<f64> {
    let p = price(model)?;
    Some(
        (t.input as f64 * p.input + t.output as f64 * p.output + t.cache_read as f64 * p.cache_read + t.cache_write as f64 * p.cache_write)
            / 1_000_000.0,
    )
}

/// How far through its window a limit is: where usage would be at an even rate (0..=1).
pub fn pace(resets_at: u64, window_ms: u64, now: u64) -> f64 {
    if window_ms == 0 {
        return 0.0;
    }
    let left = resets_at.saturating_sub(now) as f64;
    (1.0 - left / window_ms as f64).clamp(0.0, 1.0)
}

/// The Claude session window: it starts at the first message after the previous one ended
/// and lasts five hours. `minutes` are unix ms, any order. `None` when no window is open at `now`.
pub fn session_window(minutes: &[u64], now: u64) -> Option<(u64, u64)> {
    let mut sorted: Vec<u64> = minutes.iter().copied().filter(|&m| m <= now).collect();
    sorted.sort_unstable();
    let mut start: Option<u64> = None;
    for t in sorted {
        if start.is_none_or(|s| t >= s + CLAUDE_WINDOW_MS) {
            start = Some(t);
        }
    }
    start.filter(|&s| now < s + CLAUDE_WINDOW_MS).map(|s| (s, s + CLAUDE_WINDOW_MS))
}

/// The last assistant message read from a Claude log, so a repeat of it (Claude Code writes
/// one line per content block, each with the usage so far) replaces it instead of adding up.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
struct LastMsg {
    id: String,
    hour: u32,
    minute: u32,
    model: u16,
    project: u16,
    tokens: Tokens,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct FileState {
    kind: Kind,
    offset: u64,
    model: Option<String>,
    project: Option<String>,
    last: Option<LastMsg>,
    /// Codex: running total of the last token report, to skip repeated reports.
    last_total: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodexLimits {
    pub session: Option<Limit>,
    pub week: Option<Limit>,
    pub at: u64,
}

/// Bucket key: hour since the epoch, tool, model index, project index.
type Key = (u32, u8, u16, u16);

#[derive(Default, Debug)]
pub struct History {
    files: HashMap<String, FileState>,
    models: Vec<String>,
    projects: Vec<String>,
    buckets: BTreeMap<Key, Tokens>,
    /// Claude per minute: (minute since the epoch, model index).
    recent: BTreeMap<(u32, u16), Tokens>,
    last_used: HashMap<u8, u64>,
    codex_plan: Option<String>,
    codex_limits: Option<CodexLimits>,
    /// The first full scan has finished.
    pub ready: bool,
    dirty: bool,
    /// Not cached: re-read on each scan.
    pub claude_plan: Option<String>,
    /// Per tool (in `Kind::ALL` order): a data folder exists, and its newest file time.
    pub found: [bool; 4],
    pub newest: [Option<u64>; 4],
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Cache {
    version: u32,
    files: HashMap<String, FileState>,
    models: Vec<String>,
    projects: Vec<String>,
    buckets: Vec<(u32, u8, u16, u16, Tokens)>,
    recent: Vec<(u32, u16, Tokens)>,
    last_used: Vec<(u8, u64)>,
    codex_plan: Option<String>,
    codex_limits: Option<CodexLimits>,
}

fn intern(list: &mut Vec<String>, name: Option<&str>) -> u16 {
    let Some(name) = name.filter(|n| !n.is_empty()) else { return NONE };
    if let Some(i) = list.iter().position(|m| m == name) {
        return i as u16;
    }
    if list.len() >= (NONE as usize) - 1 {
        return NONE;
    }
    list.push(name.to_string());
    (list.len() - 1) as u16
}

fn kind_index(k: Kind) -> u8 {
    Kind::ALL.iter().position(|&x| x == k).unwrap_or(0) as u8
}

fn ts_of(v: &Value) -> u64 {
    v.get("timestamp").and_then(Value::as_str).and_then(agents::parse_ts).unwrap_or(0)
}

fn num(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn nice_plan(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("free") || raw.eq_ignore_ascii_case("unknown") {
        return None;
    }
    let mut c = raw.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase())
}

/// Claude's plan name from `~/.claude/.credentials.json`. Only `subscriptionType` is taken; the file
/// is parsed in memory and nothing else from it is kept, logged or stored.
pub fn claude_plan(home: &Path) -> Option<String> {
    let text = fs::read_to_string(home.join(".claude").join(".credentials.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let plan = v.pointer("/claudeAiOauth/subscriptionType").or_else(|| v.get("subscriptionType"))?.as_str()?.to_string();
    drop(v);
    nice_plan(&plan)
}

/// Log folders for each parsed tool.
pub fn log_roots(home: &Path, kind: Kind) -> Vec<PathBuf> {
    let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let mut roots = match kind {
        Kind::Claude => {
            let mut out = vec![home.join(".claude").join("projects")];
            if let Some(dir) = env("CLAUDE_CONFIG_DIR") {
                out.push(dir.join("projects"));
            }
            out
        }
        Kind::Codex => {
            let mut out = vec![home.join(".codex").join("sessions"), home.join(".codex").join("archived_sessions")];
            if let Some(dir) = env("CODEX_HOME") {
                out.push(dir.join("sessions"));
                out.push(dir.join("archived_sessions"));
            }
            out
        }
        Kind::OpenCode => {
            let mut out = vec![home.join(".local").join("share").join("opencode")];
            for var in ["LOCALAPPDATA", "APPDATA", "XDG_DATA_HOME"] {
                if let Some(dir) = env(var) {
                    out.push(dir.join("opencode"));
                }
            }
            out
        }
        Kind::Copilot => vec![home.join(".copilot")],
    };
    roots.extend(super::config::extra_roots(kind));
    roots
}

/// Newest modification time (unix ms) of anything under `dir`, a few levels down.
pub fn newest_mtime(dir: &Path, depth: usize) -> Option<u64> {
    let mut best: Option<u64> = None;
    let Ok(entries) = fs::read_dir(dir) else { return None };
    for e in entries.flatten() {
        let Ok(meta) = e.metadata() else { continue };
        let t = if meta.is_dir() {
            if depth == 0 {
                continue;
            }
            newest_mtime(&e.path(), depth - 1)
        } else {
            meta.modified().ok().and_then(|m| m.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64)
        };
        best = best.max(t);
    }
    best
}

impl History {
    pub fn load(path: &Path) -> Self {
        let Some(c) = fs::read(path).ok().and_then(|b| serde_json::from_slice::<Cache>(&b).ok()) else { return Self::default() };
        if c.version != CACHE_VERSION {
            return Self::default();
        }
        Self {
            files: c.files,
            models: c.models,
            projects: c.projects,
            buckets: c.buckets.into_iter().map(|(h, k, m, p, t)| ((h, k, m, p), t)).collect(),
            recent: c.recent.into_iter().map(|(m, model, t)| ((m, model), t)).collect(),
            last_used: c.last_used.into_iter().collect(),
            codex_plan: c.codex_plan,
            codex_limits: c.codex_limits,
            ..Self::default()
        }
    }

    pub fn save(&mut self, path: &Path) -> std::io::Result<()> {
        let c = Cache {
            version: CACHE_VERSION,
            files: self.files.clone(),
            models: self.models.clone(),
            projects: self.projects.clone(),
            buckets: self.buckets.iter().map(|(&(h, k, m, p), &t)| (h, k, m, p, t)).collect(),
            recent: self.recent.iter().map(|(&(m, model), &t)| (m, model, t)).collect(),
            last_used: self.last_used.iter().map(|(&k, &v)| (k, v)).collect(),
            codex_plan: self.codex_plan.clone(),
            codex_limits: self.codex_limits,
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec(&c)?)?;
        fs::rename(&tmp, path)?;
        self.dirty = false;
        Ok(())
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    fn touch(&mut self, kind: Kind, ts: u64) {
        let slot = self.last_used.entry(kind_index(kind)).or_default();
        *slot = (*slot).max(ts);
    }

    fn put(&mut self, kind: Kind, hour: u32, model: u16, project: u16, t: &Tokens, add: bool) {
        let slot = self.buckets.entry((hour, kind_index(kind), model, project)).or_default();
        if add {
            slot.add(t)
        } else {
            slot.sub(t)
        }
    }

    fn put_recent(&mut self, minute: u32, model: u16, t: &Tokens, add: bool) {
        let slot = self.recent.entry((minute, model)).or_default();
        if add {
            slot.add(t)
        } else {
            slot.sub(t)
        }
    }

    fn claude_line(&mut self, f: &mut FileState, line: &str, now: u64) {
        if !line.contains("\"usage\"") {
            return;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return };
        if v.get("type").and_then(Value::as_str) != Some("assistant") {
            return;
        }
        let Some(msg) = v.get("message") else { return };
        let Some(u) = msg.get("usage") else { return };
        let ts = ts_of(&v);
        if ts == 0 {
            return;
        }
        if let Some(p) = agents::project_of(v.get("cwd").and_then(Value::as_str)) {
            f.project = Some(p);
        }
        let model_name = msg.get("model").and_then(Value::as_str).filter(|m| !m.starts_with('<'));
        if model_name.is_none() && u.get("output_tokens").is_none() {
            return;
        }
        let tokens = Tokens {
            input: num(u, "input_tokens"),
            output: num(u, "output_tokens"),
            cache_read: num(u, "cache_read_input_tokens"),
            cache_write: num(u, "cache_creation_input_tokens"),
        };
        let id = format!(
            "{}|{}",
            msg.get("id").and_then(Value::as_str).unwrap_or(""),
            v.get("requestId").and_then(Value::as_str).unwrap_or("")
        );
        if let Some(prev) = f.last.take().filter(|l| l.id == id && id != "|") {
            self.put(Kind::Claude, prev.hour, prev.model, prev.project, &prev.tokens, false);
            if (prev.minute as u64) * MINUTE + RECENT_MS > now {
                self.put_recent(prev.minute, prev.model, &prev.tokens, false);
            }
        }
        let model = intern(&mut self.models, model_name);
        let project = intern(&mut self.projects, f.project.as_deref());
        let (hour, minute) = ((ts / HOUR) as u32, (ts / MINUTE) as u32);
        self.put(Kind::Claude, hour, model, project, &tokens, true);
        if ts + RECENT_MS > now {
            self.put_recent(minute, model, &tokens, true);
        }
        self.touch(Kind::Claude, ts);
        f.last = Some(LastMsg { id, hour, minute, model, project, tokens });
    }

    fn codex_line(&mut self, f: &mut FileState, line: &str) {
        if !(line.contains("token_count") || line.contains("session_meta") || line.contains("turn_context")) {
            return;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return };
        let ts = ts_of(&v);
        let Some(p) = v.get("payload") else { return };
        match v.get("type").and_then(Value::as_str) {
            Some("session_meta" | "turn_context") => {
                if let Some(proj) = agents::project_of(p.get("cwd").and_then(Value::as_str)) {
                    f.project = Some(proj);
                }
                if let Some(m) = p.get("model").and_then(Value::as_str) {
                    f.model = Some(m.to_string());
                }
            }
            Some("event_msg") if p.get("type").and_then(Value::as_str) == Some("token_count") => {
                if let Some(limits) = p.get("rate_limits").filter(|l| l.is_object()) {
                    if let Some(plan) = limits.get("plan_type").and_then(Value::as_str).and_then(nice_plan) {
                        self.codex_plan = Some(plan);
                    }
                    let session = agents::codex_limit(limits.get("primary"), ts);
                    let week = agents::codex_limit(limits.get("secondary"), ts);
                    if (session.is_some() || week.is_some()) && self.codex_limits.is_none_or(|l| l.at <= ts) {
                        self.codex_limits = Some(CodexLimits { session, week, at: ts });
                    }
                }
                let Some(info) = p.get("info").filter(|i| i.is_object()) else { return };
                let total = info.pointer("/total_token_usage/total_tokens").and_then(Value::as_u64).unwrap_or(0);
                if total != 0 && total == f.last_total {
                    return;
                }
                f.last_total = total;
                let Some(u) = info.get("last_token_usage") else { return };
                let cached = num(u, "cached_input_tokens");
                let tokens = Tokens {
                    input: num(u, "input_tokens").saturating_sub(cached),
                    output: num(u, "output_tokens"),
                    cache_read: cached,
                    cache_write: 0,
                };
                if ts == 0 || tokens.total() == 0 {
                    return;
                }
                let model = intern(&mut self.models, f.model.as_deref());
                let project = intern(&mut self.projects, f.project.as_deref());
                self.put(Kind::Codex, (ts / HOUR) as u32, model, project, &tokens, true);
                self.touch(Kind::Codex, ts);
            }
            _ => {}
        }
    }

    /// Folds OpenCode replies into the buckets; `mark` is the newest update time read, kept as the
    /// database's read position.
    pub fn fold_opencode(&mut self, key: &str, replies: &[opencode::Reply], mark: u64) {
        for r in replies {
            let model = intern(&mut self.models, r.model.as_deref());
            let project = intern(&mut self.projects, r.project.as_deref());
            self.put(Kind::OpenCode, (r.ts / HOUR) as u32, model, project, &r.tokens, true);
            self.touch(Kind::OpenCode, r.ts);
        }
        let f = self.files.entry(key.to_string()).or_insert(FileState { kind: Kind::OpenCode, offset: 0, model: None, project: None, last: None, last_total: 0 });
        f.offset = f.offset.max(mark);
        self.dirty = true;
    }

    /// Feeds complete lines of `buf` for the file at `key`; returns the bytes used.
    pub fn feed(&mut self, key: &str, kind: Kind, buf: &[u8], now: u64) -> usize {
        let Some(end) = buf.iter().rposition(|&b| b == b'\n') else { return 0 };
        let mut f = self.files.remove(key).unwrap_or(FileState { kind, offset: 0, model: None, project: None, last: None, last_total: 0 });
        for line in buf[..end].split(|&b| b == b'\n') {
            let Ok(line) = std::str::from_utf8(line) else { continue };
            match kind {
                Kind::Claude => self.claude_line(&mut f, line, now),
                Kind::Codex => self.codex_line(&mut f, line),
                _ => {}
            }
        }
        f.offset += (end + 1) as u64;
        self.files.insert(key.to_string(), f);
        self.dirty = true;
        end + 1
    }

    fn offset(&self, key: &str) -> Option<u64> {
        self.files.get(key).map(|f| f.offset)
    }

    /// Drops buckets older than the kept history and files that are gone.
    pub fn prune(&mut self, now: u64, alive: &dyn Fn(&str) -> bool) {
        let first_hour = (now.saturating_sub(KEEP_DAYS * DAY) / HOUR) as u32;
        let first_minute = (now.saturating_sub(RECENT_MS) / MINUTE) as u32;
        let (b, r, f) = (self.buckets.len(), self.recent.len(), self.files.len());
        self.buckets.retain(|k, t| k.0 >= first_hour && t.total() > 0);
        self.recent.retain(|k, _| k.0 >= first_minute);
        self.files.retain(|k, _| alive(k));
        if (b, r, f) != (self.buckets.len(), self.recent.len(), self.files.len()) {
            self.dirty = true;
        }
    }

    pub fn last_used(&self, kind: Kind) -> Option<u64> {
        self.last_used.get(&kind_index(kind)).copied()
    }

    pub fn codex_plan(&self) -> Option<String> {
        self.codex_plan.clone()
    }

    pub fn codex_limits(&self) -> Option<CodexLimits> {
        self.codex_limits
    }

    /// Claude's open session window: (start, resets at, API value so far, tokens so far).
    pub fn claude_window(&self, now: u64) -> Option<(u64, u64, f64, u64)> {
        let minutes: Vec<u64> = self.recent.keys().map(|&(m, _)| m as u64 * MINUTE).collect();
        let (start, end) = session_window(&minutes, now)?;
        let (mut cost_usd, mut tokens) = (0.0, 0);
        for (&(m, model), t) in &self.recent {
            let at = m as u64 * MINUTE;
            if at >= start && at < end {
                cost_usd += self.models.get(model as usize).and_then(|name| cost(name, t)).unwrap_or(0.0);
                tokens += t.total();
            }
        }
        Some((start, end, cost_usd, tokens))
    }

    /// Everything the dashboard shows for a period of `days` (1 = today, hour by hour), in local days
    /// for a clock `tz_offset_min` minutes ahead of UTC.
    pub fn summary(&self, now: u64, tz_offset_min: i32, days: u32) -> Summary {
        let tz = tz_offset_min as i64 * MINUTE as i64;
        let local_day = |ms: i64| (ms + tz).div_euclid(DAY as i64);
        let today = local_day(now as i64);
        let days = days.clamp(1, 90) as i64;
        let first_day = today - days + 1;
        let hourly = days == 1;

        let mut s = Summary { days: days as u32, hourly, ..Summary::default() };
        let slots = if hourly { 24 } else { days as usize };
        s.trend = vec![[0, 0, 0]; slots];
        s.trend_first = if hourly { today * 24 } else { first_day };

        let map_start = today - (today + 3).rem_euclid(7) - 12 * 7;
        s.activity_first = map_start;
        let mut map = vec![0u64; (today - map_start + 1) as usize];

        let mut models: HashMap<u16, (u8, u64)> = HashMap::new();
        let mut projects: HashMap<u16, u64> = HashMap::new();
        let (mut cache_read, mut all_input) = (0u64, 0u64);
        for (&(hour, kind, model, project), t) in &self.buckets {
            let ms = hour as i64 * HOUR as i64;
            let day = local_day(ms);
            if day >= map_start && day <= today {
                map[(day - map_start) as usize] += t.total();
            }
            if day < first_day || day > today {
                continue;
            }
            let total = t.total();
            let slot = if hourly { ((ms + tz).div_euclid(HOUR as i64) - today * 24) as usize } else { (day - first_day) as usize };
            let pi = match Kind::ALL[kind as usize] {
                Kind::Codex => 1,
                Kind::OpenCode => 2,
                _ => 0,
            };
            if let Some(v) = s.trend.get_mut(slot) {
                v[pi] += total;
            }
            let c = self.models.get(model as usize).and_then(|name| cost(name, t));
            if c.is_none() && total > 0 {
                s.minimum = true;
            }
            s.providers[pi].cost_usd += c.unwrap_or(0.0);
            s.providers[pi].tokens += total;
            s.tokens += total;
            s.cost_usd += c.unwrap_or(0.0);
            cache_read += t.cache_read;
            all_input += t.input + t.cache_read + t.cache_write;
            let e = models.entry(model).or_insert((kind, 0));
            e.1 += total;
            *projects.entry(project).or_default() += total;
        }
        s.cache_share = if all_input > 0 { cache_read as f64 / all_input as f64 } else { 0.0 };
        let name = |list: &[String], i: u16| list.get(i as usize).cloned();
        let mut ms: Vec<Named> = models
            .into_iter()
            .filter(|(_, (_, t))| *t > 0)
            .map(|(i, (k, t))| Named {
                name: name(&self.models, i).map_or_else(|| "Unknown".into(), |m| pretty_model(&m)),
                provider: Kind::ALL[k as usize],
                tokens: t,
            })
            .collect();
        merge_same_names(&mut ms);
        ms.sort_by(|a, b| b.tokens.cmp(&a.tokens));
        ms.truncate(6);
        s.models = ms;
        let mut ps: Vec<Named> = projects
            .into_iter()
            .filter(|(_, t)| *t > 0)
            .map(|(i, t)| Named { name: name(&self.projects, i).unwrap_or_else(|| "Other".into()), provider: Kind::Claude, tokens: t })
            .collect();
        merge_same_names(&mut ps);
        ps.sort_by(|a, b| b.tokens.cmp(&a.tokens));
        ps.truncate(6);
        s.projects = ps;

        s.activity_total = map.iter().sum();
        s.active_days = map.iter().filter(|&&t| t > 0).count() as u32;
        s.busiest = map.iter().enumerate().filter(|(_, t)| **t > 0).max_by_key(|(_, t)| **t).map(|(i, &t)| (map_start + i as i64, t));
        s.streak = streak(&map);
        s.activity = map;
        s
    }
}

fn merge_same_names(list: &mut Vec<Named>) {
    let mut out: Vec<Named> = Vec::new();
    for n in list.drain(..) {
        match out.iter_mut().find(|o| o.name == n.name) {
            Some(o) => o.tokens += n.tokens,
            None => out.push(n),
        }
    }
    *list = out;
}

/// Days in a row with use, ending today (or yesterday when today has none yet).
pub fn streak(days: &[u64]) -> u32 {
    let mut it = days.iter().rev().peekable();
    if it.peek().is_some_and(|&&t| t == 0) {
        it.next();
    }
    it.take_while(|&&t| t > 0).count() as u32
}

/// `claude-haiku-5-5-20260101` to `Haiku 5.5`, `gpt-5.1-codex` to `GPT-5.1 Codex`.
pub fn pretty_model(id: &str) -> String {
    let id = id.rsplit_once('/').map_or(id, |(_, t)| t);
    if let Some(rest) = id.strip_prefix("claude-") {
        let parts: Vec<&str> = rest.split('-').filter(|p| !(p.len() >= 8 && p.chars().all(|c| c.is_ascii_digit()))).collect();
        let (mut family, mut version) = (Vec::new(), Vec::new());
        for p in parts {
            if p.chars().all(|c| c.is_ascii_digit()) {
                version.push(p);
            } else {
                family.push(cap(p));
            }
        }
        let v = version.join(".");
        return if v.is_empty() { family.join(" ") } else { format!("{} {v}", family.join(" ")) };
    }
    if let Some(rest) = id.strip_prefix("gpt-") {
        let mut words = rest.split('-');
        let head = words.next().unwrap_or("");
        let tail: Vec<String> = words.map(cap).collect();
        return if tail.is_empty() { format!("GPT-{head}") } else { format!("GPT-{head} {}", tail.join(" ")) };
    }
    id.to_string()
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTotal {
    pub cost_usd: f64,
    pub tokens: u64,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Named {
    pub name: String,
    pub provider: Kind,
    pub tokens: u64,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub days: u32,
    pub cost_usd: f64,
    /// Some use was by a model with no known price, so the cost is a lower bound.
    pub minimum: bool,
    pub tokens: u64,
    pub cache_share: f64,
    /// Claude, Codex, then OpenCode.
    pub providers: [ProviderTotal; 3],
    /// One bar per day (or per hour for today): [Claude, Codex, OpenCode] tokens.
    pub trend: Vec<[u64; 3]>,
    /// Local day number (or local hour number when `hourly`) of the first trend bar.
    pub trend_first: i64,
    pub hourly: bool,
    pub models: Vec<Named>,
    pub projects: Vec<Named>,
    /// Tokens per local day from `activity_first` (a Monday, 12 weeks before this week's) to today.
    pub activity: Vec<u64>,
    pub activity_first: i64,
    pub activity_total: u64,
    pub active_days: u32,
    pub busiest: Option<(i64, u64)>,
    pub streak: u32,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// One pass over every log folder; locks `history` only while folding in a file's new bytes.
pub fn scan(history: &std::sync::Mutex<History>, home: &Path) {
    let now = now_ms();
    let cutoff = SystemTime::now() - std::time::Duration::from_millis(KEEP_DAYS * DAY);
    let mut seen: Vec<String> = Vec::new();
    for kind in [Kind::Claude, Kind::Codex] {
        for root in log_roots(home, kind) {
            let mut found = Vec::new();
            agents::files_under(&root, 6, &mut found);
            for (path, modified, len) in found {
                let key = path.to_string_lossy().into_owned();
                seen.push(key.clone());
                let known = history.lock().unwrap().offset(&key);
                if known.is_none() && modified < cutoff {
                    continue;
                }
                let mut offset = known.unwrap_or(0);
                if len < offset {
                    // Replaced with something shorter: read it again from the start.
                    history.lock().unwrap().files.remove(&key);
                    offset = 0;
                }
                while offset < len {
                    let Some(buf) = read_chunk(&path, offset) else { break };
                    let used = history.lock().unwrap().feed(&key, kind, &buf, now);
                    if used == 0 {
                        break;
                    }
                    offset += used as u64;
                }
            }
        }
    }
    if let Some(db) = opencode::db_path(home) {
        let key = db.to_string_lossy().into_owned();
        seen.push(key.clone());
        let after = history.lock().unwrap().offset(&key).unwrap_or(now.saturating_sub(KEEP_DAYS * DAY));
        if let Some((replies, mark)) = opencode::replies_since(&db, after) {
            history.lock().unwrap().fold_opencode(&key, &replies, mark);
        }
    }
    let plan = claude_plan(home);
    let mut found = [false; 4];
    let mut newest = [None; 4];
    for (i, kind) in Kind::ALL.into_iter().enumerate() {
        for root in log_roots(home, kind).iter().filter(|r| r.is_dir()) {
            found[i] = true;
            if matches!(kind, Kind::OpenCode | Kind::Copilot) {
                newest[i] = newest[i].max(newest_mtime(root, 1));
            }
        }
    }
    // A tool can be installed without session logs yet (or keep them elsewhere): its home folder
    // still marks it as present, and its newest file as when it was last used.
    for (i, dir) in [(0, home.join(".claude")), (1, home.join(".codex"))] {
        if dir.is_dir() {
            found[i] = true;
            newest[i] = newest[i].max(newest_mtime(&dir, 0));
        }
    }
    let seen: std::collections::HashSet<String> = seen.into_iter().collect();
    let mut h = history.lock().unwrap();
    h.prune(now, &|k| seen.contains(k));
    h.claude_plan = plan;
    h.found = found;
    h.newest = newest;
    h.ready = true;
}

fn read_chunk(path: &Path, offset: u64) -> Option<Vec<u8>> {
    let mut f = fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = Vec::new();
    f.take(CHUNK).read_to_end(&mut buf).ok()?;
    Some(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_792_490_400_000; // 2026-10-20T10:00:00Z, a Tuesday

    fn claude(ts: &str, id: &str, model: &str, cwd: &str, inp: u64, out: u64, cr: u64, cw: u64) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","cwd":"{cwd}","requestId":"r{id}","message":{{"id":"m{id}","model":"{model}","usage":{{"input_tokens":{inp},"output_tokens":{out},"cache_read_input_tokens":{cr},"cache_creation_input_tokens":{cw}}}}}}}"#
        )
    }

    #[test]
    fn prices_match_the_longest_prefix() {
        assert_eq!(price("claude-opus-5-5").unwrap().input, 4.0);
        assert_eq!(price("claude-opus-5").unwrap().input, 5.0);
        assert_eq!(price("claude-opus-5-20260101").unwrap().input, 5.0);
        assert_eq!(price("claude-sonnet-4-5-20250929").unwrap().input, 3.0);
        assert_eq!(price("gpt-5-codex").unwrap().output, 10.0);
        assert!(price("gpt-6-luna").is_none());
        assert!(price("mystery").is_none());
    }

    #[test]
    fn cost_maths() {
        let t = Tokens { input: 1_000_000, output: 100_000, cache_read: 2_000_000, cache_write: 400_000 };
        // Opus 5.5: 4 + 0.1 * 20 + 2 * 0.2 + 0.4 * 5.
        let c = cost("claude-opus-5-5", &t).unwrap();
        assert!((c - (4.0 + 2.0 + 0.4 + 2.0)).abs() < 1e-9, "{c}");
        assert_eq!(cost("unknown-model", &t), None);
    }

    #[test]
    fn pace_marker() {
        assert_eq!(pace(T0 + CODEX_SESSION_MS, CODEX_SESSION_MS, T0), 0.0);
        assert!((pace(T0 + 67 * MINUTE, CODEX_SESSION_MS, T0) - (1.0 - 67.0 / 300.0)).abs() < 1e-9);
        assert_eq!(pace(T0 - 1, CODEX_SESSION_MS, T0), 1.0);
        assert_eq!(pace(T0, 0, T0), 0.0);
    }

    #[test]
    fn session_window_starts_after_the_previous_one_ends() {
        let m = |h: f64| T0 + (h * HOUR as f64) as u64;
        // 0h starts a window; 4h is inside it; 5.5h starts a new one, which is open at 6h.
        assert_eq!(session_window(&[m(4.0), m(0.0), m(5.5)], m(6.0)), Some((m(5.5), m(10.5))));
        assert_eq!(session_window(&[m(0.0), m(1.0)], m(4.9)), Some((m(0.0), m(5.0))));
        assert_eq!(session_window(&[m(0.0), m(1.0)], m(5.0)), None);
        assert_eq!(session_window(&[], T0), None);
    }

    #[test]
    fn streaks() {
        assert_eq!(streak(&[1, 0, 1, 1, 1]), 3);
        assert_eq!(streak(&[1, 1, 0]), 2);
        assert_eq!(streak(&[1, 0, 0]), 0);
        assert_eq!(streak(&[]), 0);
    }

    #[test]
    fn model_names() {
        assert_eq!(pretty_model("claude-haiku-5-5"), "Haiku 5.5");
        assert_eq!(pretty_model("claude-sonnet-4-5-20250929"), "Sonnet 4.5");
        assert_eq!(pretty_model("gpt-5-codex"), "GPT-5 Codex");
        assert_eq!(pretty_model("gpt-6"), "GPT-6");
    }

    #[test]
    fn claude_repeats_replace_and_days_aggregate() {
        let mut h = History::default();
        let now = T0 + HOUR;
        let lines = [
            claude("2026-10-20T10:00:00.000Z", "1", "claude-opus-5-5", "/w/notifi", 10, 5, 100, 0),
            // Same message again with the final output count.
            claude("2026-10-20T10:00:01.000Z", "1", "claude-opus-5-5", "/w/notifi", 10, 50, 100, 0),
            claude("2026-10-20T10:05:00.000Z", "2", "claude-haiku-5-5", "/w/other", 1, 1, 0, 8),
            claude("2026-10-19T09:00:00.000Z", "3", "claude-opus-5-5", "/w/notifi", 0, 1000, 0, 0),
        ];
        let text = lines.join("\n") + "\n";
        // Fed in two pieces with a split mid-line, as a growing file would be.
        let cut = text.len() / 2;
        let used = h.feed("a.jsonl", Kind::Claude, &text.as_bytes()[..cut], now);
        h.feed("a.jsonl", Kind::Claude, &text.as_bytes()[used..], now);
        assert_eq!(h.offset("a.jsonl"), Some(text.len() as u64));

        let s = h.summary(now, 0, 7);
        assert_eq!(s.tokens, 160 + 10 + 1000);
        assert_eq!(s.trend.len(), 7);
        assert_eq!(s.trend[6], [170, 0, 0]);
        assert_eq!(s.trend[5], [1000, 0, 0]);
        assert_eq!(s.models[0].name, "Opus 5.5");
        assert_eq!(s.models[0].tokens, 1160);
        assert_eq!(s.projects[0].name, "notifi");
        assert!((s.cache_share - 100.0 / 119.0).abs() < 1e-9);
        assert!(!s.minimum);
        let today = h.summary(now, 0, 1);
        assert!(today.hourly);
        assert_eq!(today.tokens, 170);
        assert_eq!(today.trend[10], [170, 0, 0]);
        // Activity map runs from a Monday 12 weeks back to today (Tuesday): 12 * 7 + 2 days.
        assert_eq!(s.activity.len(), 86);
        assert_eq!(s.active_days, 2);
        assert_eq!(s.streak, 2);
        assert_eq!(s.busiest.map(|b| b.1), Some(1000));

        let w = h.claude_window(now).unwrap();
        assert_eq!(w.0, T0);
        assert_eq!(w.1, T0 + CLAUDE_WINDOW_MS);
        assert_eq!(w.3, 170);
        // A UTC-11 clock puts this morning's use on the previous local day.
        let west = h.summary(now, -11 * 60, 7);
        assert_eq!(west.trend[6], [0, 0, 0]);
    }

    #[test]
    fn codex_skips_repeated_reports_and_keeps_limits() {
        let mut h = History::default();
        let tc = |ts: &str, total: u64, last_in: u64, cached: u64, out: u64| {
            format!(
                r#"{{"timestamp":"{ts}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"total_tokens":{total}}},"last_token_usage":{{"input_tokens":{last_in},"cached_input_tokens":{cached},"output_tokens":{out}}}}},"rate_limits":{{"plan_type":"plus","primary":{{"used_percent":80.0,"window_minutes":300,"resets_in_seconds":4020}},"secondary":{{"used_percent":97.0,"window_minutes":10080,"resets_at":1793000000}}}}}}}}"#
            )
        };
        let text = [
            r#"{"timestamp":"2026-10-20T09:59:00.000Z","type":"session_meta","payload":{"cwd":"/src/app","model":"gpt-5-codex"}}"#.to_string(),
            tc("2026-10-20T10:00:00.000Z", 120, 100, 80, 20),
            tc("2026-10-20T10:00:01.000Z", 120, 100, 80, 20),
            tc("2026-10-20T10:01:00.000Z", 200, 70, 0, 10),
        ]
        .join("\n")
            + "\n";
        h.feed("r.jsonl", Kind::Codex, text.as_bytes(), T0 + MINUTE * 2);
        let s = h.summary(T0 + MINUTE * 2, 0, 1);
        assert_eq!(s.providers[1].tokens, 200);
        assert_eq!(s.providers[0].tokens, 0);
        assert_eq!(s.projects[0].name, "app");
        assert_eq!(s.models[0].name, "GPT-5 Codex");
        assert_eq!(h.codex_plan().as_deref(), Some("Plus"));
        let l = h.codex_limits().unwrap();
        assert_eq!(l.session.unwrap().used_percent, 80.0);
        assert_eq!(l.session.unwrap().window_ms, Some(CODEX_SESSION_MS));
        assert_eq!(l.week.unwrap().resets_at, Some(1_793_000_000_000));
        assert_eq!(h.last_used(Kind::Codex), Some(T0 + MINUTE));
    }

    #[test]
    fn cache_round_trip_and_prune() {
        let dir = std::env::temp_dir().join(format!("notifi-usage-{}", std::process::id()));
        let path = dir.join("agent-usage.json");
        let mut h = History::default();
        let line = claude("2026-10-20T10:00:00.000Z", "1", "claude-opus-5-5", "/w/p", 1, 2, 3, 4) + "\n";
        h.feed("x.jsonl", Kind::Claude, line.as_bytes(), T0);
        h.save(&path).unwrap();
        let mut back = History::load(&path);
        assert_eq!(back.offset("x.jsonl"), Some(line.len() as u64));
        assert_eq!(back.summary(T0, 0, 1).tokens, 10);
        back.prune(T0 + (KEEP_DAYS + 1) * DAY, &|_| false);
        assert_eq!(back.summary(T0, 0, 1).tokens, 0);
        assert_eq!(back.offset("x.jsonl"), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
