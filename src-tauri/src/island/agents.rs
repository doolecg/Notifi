//! Follows local coding agents through the logs they already write (spec/everyday/ai-agents.md).
//! Only new bytes are read on each pass, a new file is read from its last 512 KB, and nothing
//! leaves this computer. Claude Code and Codex logs and OpenCode's database are read; Copilot is only detected.

use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::opencode;

/// A turn stops counting after this long without a log line, unless a tool call is still out.
pub const OFFLINE_MS: u64 = 20_000;
/// Even a pending tool call stops counting after this.
pub const TOOL_MAX_MS: u64 = 30 * 60_000;
/// A pending tool call this quiet most likely waits on the user (a permission prompt).
pub const NEEDS_MS: u64 = 8_000;
/// A shell or agent tool that has been quiet this long may be waiting on you; shorter is just a tool still running.
pub const NEEDS_SLOW_MS: u64 = 120_000;
/// How long a finished agent reads as done.
pub const DONE_MS: u64 = 8_000;
/// How long a finished session stays as a done chip on the taskbar bar.
const CHIP_DONE_MS: u64 = 90_000;
const TAIL_BYTES: u64 = 512 * 1024;
/// Files untouched for longer than this are not followed.
const FRESH_FOR: Duration = Duration::from_secs(24 * 3600);
const MAX_DEPTH: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Claude,
    Codex,
    OpenCode,
    Copilot,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Claude, Kind::Codex, Kind::OpenCode, Kind::Copilot];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Claude => "Claude Code",
            Kind::Codex => "Codex",
            Kind::OpenCode => "OpenCode",
            Kind::Copilot => "Copilot CLI",
        }
    }

    fn root(self, home: &Path) -> PathBuf {
        match self {
            Kind::Claude => home.join(".claude").join("projects"),
            Kind::Codex => home.join(".codex").join("sessions"),
            Kind::OpenCode => home.join(".local").join("share").join("opencode"),
            Kind::Copilot => home.join(".copilot"),
        }
    }

    pub fn parsed(self) -> bool {
        matches!(self, Kind::Claude | Kind::Codex | Kind::OpenCode)
    }

    fn parse(self, line: &str) -> Option<(u64, Entry)> {
        match self {
            Kind::Claude => parse_claude(line),
            Kind::Codex => parse_codex(line),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    /// Tokens read: fresh input plus cache reads and writes.
    pub read: u64,
    pub written: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limit {
    pub used_percent: f64,
    /// Unix ms, when known.
    pub resets_at: Option<u64>,
    /// Length of the limit's window, when the tool says.
    #[serde(default)]
    pub window_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Meta { project: Option<String>, model: Option<String> },
    /// The user started a turn, in this project when the line says.
    Prompt { project: Option<String> },
    /// `todos` is (done, total) when the line rewrote the agent's to-do list.
    /// `needs_after` is how long a pending tool call must stay quiet before it reads as waiting for you.
    Reply { model: Option<String>, usage: Usage, done: bool, tool: bool, todos: Option<(u32, u32)>, needs_after: Option<u64> },
    ToolResult,
    TurnEnd,
    /// Token use, with plan limits when the tool reported them.
    Usage { usage: Usage, session: Option<Limit>, week: Option<Limit> },
}

/// One log file's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Session {
    pub project: Option<String>,
    pub model: Option<String>,
    pub last_ts: u64,
    pub turn_start: Option<u64>,
    pub tool_pending: bool,
    /// Quiet time before the pending tool call counts as needing you; the default when unset.
    pub needs_after: Option<u64>,
    pub usage: Usage,
    pub session_limit: Option<Limit>,
    pub week_limit: Option<Limit>,
    /// (done, total) of the latest to-do list.
    pub todos: Option<(u32, u32)>,
}

impl Session {
    pub fn apply(&mut self, ts: u64, entry: Entry) {
        self.last_ts = self.last_ts.max(ts);
        match entry {
            Entry::Meta { project, model } => {
                self.project = project.or(self.project.take());
                self.model = model.or(self.model.take());
            }
            Entry::Prompt { project } => {
                self.project = project.or(self.project.take());
                self.turn_start = Some(ts);
                self.tool_pending = false;
            }
            Entry::Reply { model, usage, done, tool, todos, needs_after } => {
                self.needs_after = if tool { needs_after } else { None };
                self.model = model.or(self.model.take());
                self.todos = todos.or(self.todos);
                self.add(usage);
                self.tool_pending = tool;
                if done && !tool {
                    self.turn_start = None;
                } else {
                    // The prompt can lie before the part of a long log that was read.
                    self.turn_start.get_or_insert(ts);
                }
            }
            Entry::ToolResult => {
                self.tool_pending = false;
                self.turn_start.get_or_insert(ts);
            }
            Entry::TurnEnd => {
                self.turn_start = None;
                self.tool_pending = false;
            }
            Entry::Usage { usage, session, week } => {
                self.add(usage);
                self.session_limit = session.or(self.session_limit);
                self.week_limit = week.or(self.week_limit);
            }
        }
    }

    fn add(&mut self, u: Usage) {
        self.usage.read += u.read;
        self.usage.written += u.written;
    }

    pub fn working(&self, now: u64) -> bool {
        if self.turn_start.is_none() {
            return false;
        }
        let quiet = now.saturating_sub(self.last_ts);
        quiet < OFFLINE_MS || (self.tool_pending && quiet < TOOL_MAX_MS)
    }

    pub fn needs_you(&self, now: u64) -> bool {
        self.working(now) && self.tool_pending && now.saturating_sub(self.last_ts) >= self.needs_after.unwrap_or(NEEDS_MS)
    }

    pub fn elapsed(&self, now: u64) -> u64 {
        self.turn_start.map_or(0, |t| now.saturating_sub(t))
    }
}

/// `2026-10-20T10:11:12.345Z` (or with a `+hh:mm` offset) to unix ms.
pub fn parse_ts(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b' ') {
        return None;
    }
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    let mut rest = &s[19..];
    let mut ms = 0i64;
    if let Some(frac) = rest.strip_prefix('.') {
        let digits = frac.bytes().take_while(u8::is_ascii_digit).count();
        let mut f = frac[..digits.min(3)].to_string();
        while f.len() < 3 {
            f.push('0');
        }
        ms = f.parse().ok()?;
        rest = &frac[digits..];
    }
    let offset_min = match rest {
        "" | "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes()[0] {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let r = rest[1..].replace(':', "");
            let hh: i64 = r.get(0..2)?.parse().ok()?;
            let mm: i64 = r.get(2..4).unwrap_or("00").parse().ok()?;
            sign * (hh * 60 + mm)
        }
    };
    // Days from civil (proleptic Gregorian).
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + h * 3600 + mi * 60 + sec - offset_min * 60;
    u64::try_from(secs * 1000 + ms).ok()
}

fn ts_of(v: &Value) -> u64 {
    v.get("timestamp").and_then(Value::as_str).and_then(parse_ts).unwrap_or(0)
}

pub(crate) fn project_of(cwd: Option<&str>) -> Option<String> {
    let cwd = cwd?.trim_end_matches(['/', '\\']);
    cwd.rsplit(['/', '\\']).next().filter(|s| !s.is_empty()).map(str::to_string)
}

fn n(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// How long a pending tool call must stay quiet before it reads as waiting for you. Questions to you
/// count at once; edits are the ones that usually raise a permission prompt; shells, subagents and
/// the like are usually just running, so they get a long wait.
fn needs_after_of(content: Option<&Value>) -> u64 {
    let names: Vec<&str> = content
        .and_then(Value::as_array)
        .map(|items| items.iter().filter(|i| i.get("type").and_then(Value::as_str) == Some("tool_use")).filter_map(|i| i.get("name").and_then(Value::as_str)).collect())
        .unwrap_or_default();
    if names.iter().any(|n| matches!(*n, "AskUserQuestion" | "ExitPlanMode")) {
        0
    } else if !names.is_empty() && names.iter().all(|n| matches!(*n, "Edit" | "Write" | "MultiEdit" | "NotebookEdit")) {
        NEEDS_MS
    } else {
        NEEDS_SLOW_MS
    }
}

/// (done, total) from a `TodoWrite` tool call.
fn todo_counts(item: &Value) -> Option<(u32, u32)> {
    if item.get("name").and_then(Value::as_str) != Some("TodoWrite") {
        return None;
    }
    let list = item.pointer("/input/todos")?.as_array()?;
    let done = list.iter().filter(|t| t.get("status").and_then(Value::as_str) == Some("completed")).count();
    Some((done as u32, list.len() as u32))
}

fn has_item(content: Option<&Value>, kind: &str) -> bool {
    content
        .and_then(Value::as_array)
        .is_some_and(|items| items.iter().any(|i| i.get("type").and_then(Value::as_str) == Some(kind)))
}

/// One line of a Claude Code session log (`~/.claude/projects/<project>/<session>.jsonl`).
pub fn parse_claude(line: &str) -> Option<(u64, Entry)> {
    let v: Value = serde_json::from_str(line).ok()?;
    let ts = ts_of(&v);
    let msg = v.get("message");
    match v.get("type")?.as_str()? {
        "user" => {
            if v.get("isMeta").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            let content = msg?.get("content");
            let project = project_of(v.get("cwd").and_then(Value::as_str));
            Some((ts, if has_item(content, "tool_result") { Entry::ToolResult } else { Entry::Prompt { project } }))
        }
        "assistant" => {
            let msg = msg?;
            let usage = msg.get("usage").map_or_else(Usage::default, |u| Usage {
                read: n(u, "input_tokens") + n(u, "cache_read_input_tokens") + n(u, "cache_creation_input_tokens"),
                written: n(u, "output_tokens"),
            });
            let stop = msg.get("stop_reason").and_then(Value::as_str);
            let tool = has_item(msg.get("content"), "tool_use") || stop == Some("tool_use");
            let done = matches!(stop, Some("end_turn" | "stop_sequence" | "max_tokens" | "refusal"));
            let model = msg.get("model").and_then(Value::as_str).filter(|m| !m.starts_with('<')).map(str::to_string);
            let todos = msg.get("content").and_then(Value::as_array).and_then(|items| items.iter().rev().find_map(todo_counts));
            let needs_after = tool.then(|| needs_after_of(msg.get("content")));
            Some((ts, Entry::Reply { model, usage, done, tool, todos, needs_after }))
        }
        _ => project_of(v.get("cwd").and_then(Value::as_str)).map(|p| (ts, Entry::Meta { project: Some(p), model: None })),
    }
}

pub(crate) fn codex_limit(v: Option<&Value>, ts: u64) -> Option<Limit> {
    let v = v?;
    let used_percent = v.get("used_percent").and_then(Value::as_f64)?;
    let resets_at = v
        .get("resets_at")
        .and_then(Value::as_u64)
        .map(|s| if s < 100_000_000_000 { s * 1000 } else { s })
        .or_else(|| v.get("resets_in_seconds").and_then(Value::as_u64).map(|s| ts + s * 1000));
    let window_ms = v.get("window_minutes").and_then(Value::as_u64).map(|m| m * 60_000);
    Some(Limit { used_percent, resets_at, window_ms })
}

/// One line of a Codex rollout log (`~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`).
pub fn parse_codex(line: &str) -> Option<(u64, Entry)> {
    let v: Value = serde_json::from_str(line).ok()?;
    let ts = ts_of(&v);
    let p = v.get("payload")?;
    let entry = match (v.get("type")?.as_str()?, p.get("type").and_then(Value::as_str)) {
        ("session_meta" | "turn_context", _) => Entry::Meta {
            project: project_of(p.get("cwd").and_then(Value::as_str)),
            model: p.get("model").and_then(Value::as_str).map(str::to_string),
        },
        ("event_msg", Some("user_message")) => Entry::Prompt { project: None },
        ("event_msg", Some("task_complete" | "turn_aborted")) => Entry::TurnEnd,
        ("event_msg", Some("token_count")) => {
            let limits = p.get("rate_limits");
            let session = codex_limit(limits.and_then(|l| l.get("primary")), ts);
            let week = codex_limit(limits.and_then(|l| l.get("secondary")), ts);
            let usage = p.pointer("/info/last_token_usage").map_or_else(Usage::default, |u| Usage {
                read: n(u, "input_tokens"),
                written: n(u, "output_tokens") + n(u, "reasoning_output_tokens"),
            });
            Entry::Usage { usage, session, week }
        }
        ("response_item", Some("function_call" | "custom_tool_call" | "local_shell_call")) => {
            Entry::Reply { model: None, usage: Usage::default(), done: false, tool: true, todos: None, needs_after: None }
        }
        ("response_item", Some("function_call_output" | "custom_tool_call_output")) => Entry::ToolResult,
        _ => return None,
    };
    Some((ts, entry))
}

/// Feeds complete lines from `buf`; returns how many bytes were used (up to the last newline).
pub fn feed(kind: Kind, session: &mut Session, buf: &[u8]) -> usize {
    let Some(end) = buf.iter().rposition(|&b| b == b'\n') else { return 0 };
    for line in buf[..end].split(|&b| b == b'\n') {
        let Ok(line) = std::str::from_utf8(line) else { continue };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((ts, entry)) = kind.parse(line) {
            session.apply(ts, entry);
        }
    }
    end + 1
}

struct Followed {
    offset: u64,
    session: Session,
}

/// What the Island shows for one agent.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSnap {
    pub id: Kind,
    pub name: &'static str,
    /// False when the logs are found but their format isn't read yet.
    pub supported: bool,
    pub working: bool,
    /// Sessions of this agent working right now.
    pub count: u32,
    /// A working session is waiting on the user.
    pub needs_you: bool,
    /// Subagents running right now.
    pub subagents: u32,
    /// Terminals open for this agent (filled in by the Island loop from the process list).
    pub terminals: u32,
    /// A session finished a moment ago and none is working.
    pub done: bool,
    pub elapsed_ms: u64,
    pub model: Option<String>,
    pub project: Option<String>,
    pub usage: Usage,
    pub session_limit: Option<Limit>,
    pub week_limit: Option<Limit>,
    pub last_active: u64,
    /// (done, total) of the main session's to-do list.
    pub todos: Option<(u32, u32)>,
}

/// One session as a button on the taskbar bar.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionChip {
    /// The session log's path.
    pub id: String,
    pub agent: Kind,
    pub project: Option<String>,
    /// "working", "needs" or "done".
    pub status: &'static str,
    pub elapsed_ms: u64,
    pub last_ts: u64,
    /// The T3 Code chat this session is, with its title; filled in by the Island loop.
    pub title: Option<String>,
    pub thread: Option<String>,
}

/// A turn that just ended after running at least the notice threshold.
#[derive(Clone, Debug, PartialEq)]
pub struct Finished {
    pub kind: Kind,
    pub project: Option<String>,
    pub elapsed_ms: u64,
}

#[derive(Default)]
pub struct Tracker {
    files: HashMap<PathBuf, Followed>,
    /// Working sessions from the last pass, with their turn start.
    working: HashMap<PathBuf, u64>,
    /// When sessions stopped working, kept for [`DONE_MS`].
    ended: HashMap<PathBuf, u64>,
    /// The same, kept for `CHIP_DONE_MS`, for the session chips.
    recent: HashMap<PathBuf, u64>,
    primed: bool,
}

pub(crate) fn files_under(dir: &Path, depth: usize, out: &mut Vec<(PathBuf, SystemTime, u64)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let Ok(meta) = e.metadata() else { continue };
        let path = e.path();
        if meta.is_dir() {
            if depth > 0 {
                files_under(&path, depth - 1, out);
            }
        } else if path.extension().is_some_and(|x| x == "jsonl") {
            if let Ok(modified) = meta.modified() {
                out.push((path, modified, meta.len()));
            }
        }
    }
}

fn read_from(path: &Path, offset: u64) -> Option<Vec<u8>> {
    let mut f = File::open(path).ok()?;
    f.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = Vec::new();
    f.take(8 * 1024 * 1024).read_to_end(&mut buf).ok()?;
    Some(buf)
}

impl Tracker {
    /// Reads new log lines. Returns turns that finished since the last pass and ran at least `notice_ms`
    /// (nothing on the first pass, so a restart doesn't replay old notices).
    pub fn poll(&mut self, home: &Path, now: u64, notice_ms: u64) -> Vec<Finished> {
        let cutoff = SystemTime::now() - FRESH_FOR;
        for kind in [Kind::Claude, Kind::Codex] {
            let mut found = Vec::new();
            files_under(&kind.root(home), MAX_DEPTH, &mut found);
            for (path, modified, len) in found {
                let known = self.files.contains_key(&path);
                if !known && modified < cutoff {
                    continue;
                }
                let f = self.files.entry(path.clone()).or_insert_with(|| Followed {
                    offset: len.saturating_sub(TAIL_BYTES),
                    session: Session::default(),
                });
                if len < f.offset {
                    // Truncated or replaced: start over.
                    *f = Followed { offset: len.saturating_sub(TAIL_BYTES), session: Session::default() };
                }
                if len == f.offset {
                    continue;
                }
                let skip_partial = f.offset > 0 && f.session == Session::default();
                let Some(buf) = read_from(&path, f.offset) else { continue };
                let start = if skip_partial { buf.iter().position(|&b| b == b'\n').map_or(buf.len(), |i| i + 1) } else { 0 };
                let used = feed(kind, &mut f.session, &buf[start..]);
                f.offset += (start + used) as u64;
            }
        }
        self.files.retain(|p, _| p.exists());
        // OpenCode keeps sessions in one database: each is followed under a path inside it.
        if let Some(db) = opencode::db_path(home) {
            for (id, session) in opencode::followed(&db, now) {
                self.files.insert(db.join(id), Followed { offset: 0, session });
            }
        }

        let mut finished = Vec::new();
        let mut working = HashMap::new();
        for (path, f) in &self.files {
            if is_sub(path) {
                continue;
            }
            if f.session.working(now) {
                working.insert(path.clone(), f.session.turn_start.unwrap_or(now));
            } else if let Some(&start) = self.working.get(path) {
                if self.primed {
                    self.ended.insert(path.clone(), now);
                    self.recent.insert(path.clone(), now);
                }
                let elapsed = f.session.last_ts.saturating_sub(start);
                if self.primed && elapsed >= notice_ms {
                    finished.push(Finished { kind: kind_of(home, path), project: f.session.project.clone(), elapsed_ms: elapsed });
                }
            }
        }
        self.ended.retain(|_, t| now.saturating_sub(*t) < DONE_MS);
        self.recent.retain(|p, t| now.saturating_sub(*t) < CHIP_DONE_MS && !working.contains_key(p));
        self.working = working;
        self.primed = true;
        finished
    }

    /// The sessions worth a button: working, waiting on you, or finished in the last minute and a half. Waiting first, then working, then done.
    pub fn chips(&self, home: &Path, now: u64) -> Vec<SessionChip> {
        let mut out: Vec<SessionChip> = self
            .files
            .iter()
            .filter(|(p, _)| !is_sub(p))
            .filter_map(|(p, f)| {
                let s = &f.session;
                // T3 Code starts a helper session just to title each chat; it is not one to open.
                if s.project.as_deref().is_some_and(|n| n.starts_with("t3code-claude-title")) {
                    return None;
                }
                let status = if s.working(now) {
                    if s.needs_you(now) { "needs" } else { "working" }
                } else if self.recent.contains_key(p) {
                    "done"
                } else {
                    return None;
                };
                Some(SessionChip { id: p.to_string_lossy().into_owned(), agent: kind_of(home, p), project: s.project.clone(), status, elapsed_ms: s.elapsed(now), last_ts: s.last_ts, title: None, thread: None })
            })
            .collect();
        let rank = |s: &str| match s {
            "needs" => 0,
            "working" => 1,
            _ => 2,
        };
        out.sort_by_key(|c| (rank(c.status), std::cmp::Reverse(c.last_ts)));
        out.truncate(8);
        out
    }

    /// Whether something is happening, so the caller can poll faster.
    pub fn busy(&self, now: u64) -> bool {
        self.files.values().any(|f| f.session.working(now) || now.saturating_sub(f.session.last_ts) < 60_000)
    }

    pub fn snapshot(&self, home: &Path, now: u64) -> Vec<AgentSnap> {
        Kind::ALL
            .iter()
            .filter(|k| k.root(home).exists())
            .map(|&kind| {
                let of_kind = self.files.iter().filter(|(p, _)| kind_of(home, p) == kind);
                let (subs, mains): (Vec<_>, Vec<_>) = of_kind.partition(|(p, _)| is_sub(p));
                let sessions: Vec<&Session> = mains.iter().map(|(_, f)| &f.session).collect();
                let subagents = subs.iter().filter(|(_, f)| f.session.working(now)).count() as u32;
                // The working session with the longest turn, else the most recent one.
                let main = sessions
                    .iter()
                    .filter(|s| s.working(now))
                    .max_by_key(|s| s.elapsed(now))
                    .or_else(|| sessions.iter().max_by_key(|s| s.last_ts))
                    .copied();
                let count = sessions.iter().filter(|s| s.working(now)).count() as u32;
                let usage = sessions.iter().fold(Usage::default(), |a, s| Usage { read: a.read + s.usage.read, written: a.written + s.usage.written });
                let latest_limits = sessions.iter().filter(|s| s.session_limit.is_some() || s.week_limit.is_some()).max_by_key(|s| s.last_ts);
                AgentSnap {
                    id: kind,
                    name: kind.name(),
                    supported: kind.parsed(),
                    working: main.is_some_and(|s| s.working(now)),
                    count,
                    // A turn waiting on its subagents is working, not waiting for you.
                    needs_you: sessions.iter().any(|s| s.needs_you(now)) && subagents == 0,
                    subagents,
                    terminals: 0,
                    done: count == 0 && self.ended.keys().any(|p| kind_of(home, p) == kind),
                    elapsed_ms: main.filter(|s| s.working(now)).map_or(0, |s| s.elapsed(now)),
                    model: main.and_then(|s| s.model.clone()),
                    project: main.and_then(|s| s.project.clone()),
                    usage,
                    session_limit: latest_limits.and_then(|s| s.session_limit),
                    week_limit: latest_limits.and_then(|s| s.week_limit),
                    last_active: main.map_or(0, |s| s.last_ts),
                    todos: main.and_then(|s| s.todos),
                }
            })
            .collect()
    }
}

/// A subagent's own log (`<session>/subagents/*.jsonl`): part of a session, never a session itself.
pub(crate) fn is_sub(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "subagents")
}

fn kind_of(home: &Path, path: &Path) -> Kind {
    if path.starts_with(Kind::Codex.root(home)) {
        Kind::Codex
    } else if path.starts_with(Kind::Claude.root(home)) {
        Kind::Claude
    } else {
        Kind::OpenCode
    }
}

pub fn home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

pub(crate) const SESSION_ROWS: usize = 30;
pub(crate) const LOG_LINES: usize = 200;
const LOG_TEXT_CHARS: usize = 240;
/// Largest stretch of a log read for one session row or viewer.
const MAX_READ: u64 = 8 * 1024 * 1024;

/// One session log in the drill-down list. `id` is the log's path, passed back to `island_agent_log`.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub id: String,
    pub agent: Kind,
    pub project: Option<String>,
    pub model: Option<String>,
    pub last_ts: u64,
    pub usage: Usage,
    pub working: bool,
}

/// One readable line of a session log.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct LogLine {
    pub ts: u64,
    /// `user`, `assistant` or `tool`.
    pub role: &'static str,
    pub text: String,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

pub(crate) fn clip(s: &str) -> String {
    let s = s.trim();
    match s.char_indices().nth(LOG_TEXT_CHARS) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// The readable part of one log line: what was said, or which tool ran.
pub fn log_line(kind: Kind, line: &str) -> Option<LogLine> {
    let v: Value = serde_json::from_str(line).ok()?;
    let ts = ts_of(&v);
    let (role, text) = match kind {
        Kind::Claude => {
            let role = match v.get("type")?.as_str()? {
                "user" => "user",
                "assistant" => "assistant",
                _ => return None,
            };
            let content = v.get("message")?.get("content")?;
            if let Some(s) = content.as_str() {
                (role, clip(s))
            } else {
                let mut parts = Vec::new();
                let mut tool = false;
                for item in content.as_array()? {
                    match item.get("type").and_then(Value::as_str) {
                        Some("text") => parts.push(clip(item.get("text").and_then(Value::as_str).unwrap_or(""))),
                        Some("tool_use") => {
                            tool = true;
                            parts.push(item.get("name").and_then(Value::as_str).unwrap_or("tool").to_string());
                        }
                        Some("tool_result") => {
                            tool = true;
                            parts.push("tool result".to_string());
                        }
                        _ => {}
                    }
                }
                (if tool { "tool" } else { role }, parts.join(" · "))
            }
        }
        _ => {
            let p = v.get("payload")?;
            let ptype = p.get("type").and_then(Value::as_str)?;
            match (v.get("type")?.as_str()?, ptype) {
                ("event_msg", "user_message") => ("user", clip(p.get("message")?.as_str()?)),
                ("event_msg", "agent_message") => ("assistant", clip(p.get("message")?.as_str()?)),
                ("response_item", "function_call" | "custom_tool_call") => {
                    ("tool", p.get("name").and_then(Value::as_str).unwrap_or("tool").to_string())
                }
                ("response_item", "local_shell_call") => ("tool", "shell".to_string()),
                ("response_item", "function_call_output" | "custom_tool_call_output") => ("tool", "tool result".to_string()),
                _ => return None,
            }
        }
    };
    (!text.is_empty()).then_some(LogLine { ts, role, text })
}

fn read_tail(path: &Path) -> Option<Vec<u8>> {
    let len = fs::metadata(path).ok()?.len();
    read_from(path, len.saturating_sub(MAX_READ))
}

/// The recent session logs of Claude Code and Codex (or only `agent`'s), newest first.
#[tauri::command]
pub fn island_agent_sessions(agent: Option<Kind>) -> Vec<SessionRow> {
    let Some(home) = home() else { return Vec::new() };
    let now = now_ms();
    let mut found = Vec::new();
    for kind in [Kind::Claude, Kind::Codex].into_iter().filter(|k| agent.map_or(true, |a| a == *k)) {
        files_under(&kind.root(&home), MAX_DEPTH, &mut found);
    }
    found.sort_by_key(|f| std::cmp::Reverse(f.1));
    let mut rows: Vec<SessionRow> = found
        .into_iter()
        .take(SESSION_ROWS)
        .filter_map(|(path, _, _)| {
            let kind = kind_of(&home, &path);
            let mut s = Session::default();
            feed(kind, &mut s, &read_tail(&path)?);
            Some(SessionRow {
                id: path.to_string_lossy().into_owned(),
                agent: kind,
                project: s.project.clone(),
                model: s.model.clone(),
                last_ts: s.last_ts,
                usage: s.usage,
                working: s.working(now),
            })
        })
        .collect();
    if agent.map_or(true, |a| a == Kind::OpenCode) {
        if let Some(db) = opencode::db_path(&home) {
            rows.extend(opencode::session_rows(&db, now));
        }
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.last_ts));
    rows.truncate(SESSION_ROWS);
    rows
}

/// The last lines of one session log, oldest first. Claude Code and Codex logs are opened by path,
/// OpenCode sessions by `opencode:<session id>`.
#[tauri::command]
pub fn island_agent_log(id: String) -> Vec<LogLine> {
    let Some(home) = home() else { return Vec::new() };
    if let Some(session) = id.strip_prefix(opencode::ID_PREFIX) {
        return opencode::db_path(&home).map_or_else(Vec::new, |db| opencode::log(&db, session));
    }
    let Ok(path) = fs::canonicalize(&id) else { return Vec::new() };
    let inside = |k: Kind| fs::canonicalize(k.root(&home)).is_ok_and(|r| path.starts_with(r));
    if !path.extension().is_some_and(|x| x == "jsonl") || !(inside(Kind::Claude) || inside(Kind::Codex)) {
        return Vec::new();
    }
    let kind = if inside(Kind::Codex) { Kind::Codex } else { Kind::Claude };
    let Some(buf) = read_tail(&path) else { return Vec::new() };
    let mut lines: Vec<LogLine> = String::from_utf8_lossy(&buf).lines().filter_map(|l| log_line(kind, l)).collect();
    lines.drain(..lines.len().saturating_sub(LOG_LINES));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: &str = "2026-10-20T10:00:00.000Z";

    #[test]
    fn log_lines() {
        let user = claude("user", T0, r#"{"role":"user","content":"fix the bug"}"#);
        let l = log_line(Kind::Claude, &user).unwrap();
        assert_eq!((l.role, l.text.as_str()), ("user", "fix the bug"));
        let tool = claude("assistant", T0, r#"{"content":[{"type":"text","text":"on it"},{"type":"tool_use","name":"Bash"}]}"#);
        let l = log_line(Kind::Claude, &tool).unwrap();
        assert_eq!((l.role, l.text.as_str()), ("tool", "on it · Bash"));
        let codex = r#"{"type":"event_msg","timestamp":"2026-10-20T10:00:00.000Z","payload":{"type":"agent_message","message":"done"}}"#;
        assert_eq!(log_line(Kind::Codex, codex).unwrap().role, "assistant");
        assert!(log_line(Kind::Codex, "not json").is_none());
    }

    #[test]
    fn timestamps() {
        assert_eq!(parse_ts("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_ts("2026-10-20T10:00:00.000Z"), Some(1_792_490_400_000));
        assert_eq!(parse_ts("2026-10-20T10:00:00.5Z"), Some(1_792_490_400_500));
        assert_eq!(parse_ts("2026-10-20T12:00:00+02:00"), parse_ts(T0));
        assert_eq!(parse_ts("2024-02-29T00:00:00.123456Z"), Some(1_709_164_800_123));
        assert_eq!(parse_ts("nope"), None);
        assert_eq!(parse_ts("2026-13-01T00:00:00Z"), None);
    }

    fn claude(kind: &str, ts: &str, message: &str) -> String {
        format!(r#"{{"type":"{kind}","timestamp":"{ts}","cwd":"/home/me/work/notifi","message":{message}}}"#)
    }

    #[test]
    fn claude_turn() {
        let t0 = parse_ts(T0).unwrap();
        let mut s = Session::default();
        let lines = [
            claude("user", "2026-10-20T10:00:00.000Z", r#"{"role":"user","content":"fix the bug"}"#),
            claude(
                "assistant",
                "2026-10-20T10:00:05.000Z",
                r#"{"model":"claude-x","content":[{"type":"tool_use","id":"1"}],"stop_reason":"tool_use","usage":{"input_tokens":10,"cache_read_input_tokens":100,"cache_creation_input_tokens":5,"output_tokens":20}}"#,
            ),
        ];
        let text = lines.join("\n") + "\n";
        assert_eq!(feed(Kind::Claude, &mut s, text.as_bytes()), text.len());
        assert_eq!(s.turn_start, Some(t0));
        assert!(s.tool_pending);
        assert_eq!(s.model.as_deref(), Some("claude-x"));
        assert_eq!(s.usage, Usage { read: 115, written: 20 });
        // A pending tool keeps the turn alive past the offline limit.
        assert!(s.working(t0 + 5_000 + OFFLINE_MS + 1));

        s.apply(t0 + 6_000, parse_claude(&claude("user", "2026-10-20T10:00:06.000Z", r#"{"content":[{"type":"tool_result"}]}"#)).unwrap().1);
        assert!(!s.tool_pending);
        assert!(s.working(t0 + 7_000));
        assert!(!s.working(t0 + 6_000 + OFFLINE_MS));

        let done = claude("assistant", "2026-10-20T10:00:36.000Z", r#"{"content":[{"type":"text"}],"stop_reason":"end_turn"}"#);
        let (ts, e) = parse_claude(&done).unwrap();
        s.apply(ts, e);
        assert!(!s.working(t0 + 36_500));
    }

    #[test]
    fn a_turn_seen_mid_way_still_counts() {
        let t0 = parse_ts(T0).unwrap();
        let mut s = Session::default();
        s.apply(t0, Entry::ToolResult);
        assert!(s.working(t0 + 1_000));
        assert_eq!(s.turn_start, Some(t0));
        s.apply(t0 + 2_000, Entry::Reply { model: None, usage: Usage::default(), done: true, tool: false, todos: None, needs_after: None });
        assert!(!s.working(t0 + 2_500));
    }

    #[test]
    fn claude_ignores_meta_and_partial_lines() {
        assert!(parse_claude(r#"{"type":"user","isMeta":true,"message":{"content":"x"}}"#).is_none());
        let meta = parse_claude(r#"{"type":"system","cwd":"C:\\Users\\me\\proj\\"}"#).unwrap().1;
        assert_eq!(meta, Entry::Meta { project: Some("proj".into()), model: None });
        let mut s = Session::default();
        assert_eq!(feed(Kind::Claude, &mut s, b"{\"type\":\"user\""), 0);
        assert_eq!(s, Session::default());
    }

    #[test]
    fn codex_turn_and_limits() {
        let t0 = parse_ts(T0).unwrap();
        let text = concat!(
            r#"{"timestamp":"2026-10-20T10:00:00.000Z","type":"session_meta","payload":{"cwd":"/src/app","model":"gpt-x"}}"#, "\n",
            r#"{"timestamp":"2026-10-20T10:00:01.000Z","type":"event_msg","payload":{"type":"user_message","message":"go"}}"#, "\n",
            r#"{"timestamp":"2026-10-20T10:00:02.000Z","type":"response_item","payload":{"type":"function_call","name":"shell"}}"#, "\n",
            r#"{"timestamp":"2026-10-20T10:00:03.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":50,"output_tokens":7,"reasoning_output_tokens":3}},"rate_limits":{"primary":{"used_percent":42.0,"resets_in_seconds":60},"secondary":{"used_percent":10.5,"resets_at":1792500000}}}}"#, "\n",
            r#"{"timestamp":"2026-10-20T10:00:04.000Z","type":"event_msg","payload":{"type":"agent_message"}}"#, "\n",
        );
        let mut s = Session::default();
        feed(Kind::Codex, &mut s, text.as_bytes());
        assert_eq!(s.project.as_deref(), Some("app"));
        assert_eq!(s.model.as_deref(), Some("gpt-x"));
        assert_eq!(s.turn_start, Some(t0 + 1_000));
        assert!(s.tool_pending);
        assert_eq!(s.usage, Usage { read: 50, written: 10 });
        assert_eq!(s.session_limit, Some(Limit { used_percent: 42.0, resets_at: Some(t0 + 63_000), window_ms: None }));
        assert_eq!(s.week_limit, Some(Limit { used_percent: 10.5, resets_at: Some(1_792_500_000_000), window_ms: None }));
        let end = r#"{"timestamp":"2026-10-20T10:00:09.000Z","type":"event_msg","payload":{"type":"task_complete"}}"#;
        let (ts, e) = parse_codex(end).unwrap();
        s.apply(ts, e);
        assert!(!s.working(t0 + 9_500));
        assert_eq!(s.last_ts, t0 + 9_000);
    }

    #[test]
    fn tracker_reads_incrementally_and_reports_finished_turns() {
        let home = std::env::temp_dir().join(format!("notifi-agents-{}", std::process::id()));
        let dir = home.join(".claude").join("projects").join("p");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("s.jsonl");
        let t0 = parse_ts(T0).unwrap();
        fs::write(&file, claude("user", T0, r#"{"content":"hi"}"#) + "\n").unwrap();

        let mut tr = Tracker::default();
        assert!(tr.poll(&home, t0 + 1_000, 0).is_empty());
        let snap = tr.snapshot(&home, t0 + 1_000);
        assert_eq!(snap.len(), 1);
        assert!(snap[0].working);
        assert_eq!(snap[0].elapsed_ms, 1_000);
        assert_eq!(snap[0].project.as_deref(), Some("notifi"));
        let chips = tr.chips(&home, t0 + 1_000);
        assert_eq!((chips.len(), chips[0].status, chips[0].project.as_deref()), (1, "working", Some("notifi")));

        let mut more = fs::OpenOptions::new().append(true).open(&file).unwrap();
        use std::io::Write;
        writeln!(more, "{}", claude("assistant", "2026-10-20T10:03:00.000Z", r#"{"content":[],"stop_reason":"end_turn","usage":{"output_tokens":4}}"#)).unwrap();
        drop(more);
        let done = tr.poll(&home, t0 + 180_500, 120_000);
        assert_eq!(done, vec![Finished { kind: Kind::Claude, project: Some("notifi".into()), elapsed_ms: 180_000 }]);
        assert!(tr.poll(&home, t0 + 200_000, 120_000).is_empty());
        assert_eq!(tr.snapshot(&home, t0 + 180_500)[0].usage.written, 4);
        // A finished session stays as a "done" chip for a minute and a half, then goes.
        assert_eq!(tr.chips(&home, t0 + 200_000)[0].status, "done");
        tr.poll(&home, t0 + 180_500 + CHIP_DONE_MS + 1_000, 120_000);
        assert!(tr.chips(&home, t0 + 180_500 + CHIP_DONE_MS + 1_000).is_empty());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn pending_tools_wait_according_to_what_they_are() {
        let line = |name: &str| format!(r#"{{"type":"assistant","timestamp":"2026-10-20T10:00:00Z","message":{{"stop_reason":"tool_use","content":[{{"type":"tool_use","name":"{name}"}}]}}}}"#);
        let wait = |name: &str| match parse_claude(&line(name)) {
            Some((_, Entry::Reply { needs_after, .. })) => needs_after,
            _ => None,
        };
        assert_eq!(wait("AskUserQuestion"), Some(0));
        assert_eq!(wait("Edit"), Some(NEEDS_MS));
        assert_eq!(wait("Bash"), Some(NEEDS_SLOW_MS));
        assert_eq!(wait("Task"), Some(NEEDS_SLOW_MS));
    }
}
