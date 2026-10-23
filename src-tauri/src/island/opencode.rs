//! Reads OpenCode's local database (`opencode.db`, SQLite) for the AI Agents dashboard. It is opened
//! read-only and only token counts, timestamps, project folders and trimmed message text are read;
//! a missing or locked database gives no data. Nothing leaves this computer.
//!
//! OpenCode 2 writes sessions to `session_v2` and `session_message`; the older `session`, `message` and
//! `part` tables stop updating there (their rows are copied into the new ones). Older versions only have
//! the old tables, so both layouts are read.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{params, Connection, OpenFlags};
use serde_json::Value;

use super::{
    agents::{self, Entry, Kind, LogLine, Session, SessionRow, Usage, LOG_LINES, NEEDS_MS, NEEDS_SLOW_MS, SESSION_ROWS, TOOL_MAX_MS},
    usage::{self, Tokens},
};

/// Sessions idle for longer than this are not followed for working-agent detection.
const FOLLOW_MS: u64 = 24 * 3_600_000;
/// A session's `time_updated` can trail its messages, so the stats pass looks this far back.
const SLACK_MS: u64 = 10 * 60_000;
pub const ID_PREFIX: &str = "opencode:";

/// One finished assistant reply: when it ended, what it used.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub ts: u64,
    pub model: Option<String>,
    pub project: Option<String>,
    pub tokens: Tokens,
}

/// OpenCode's databases in one folder: `opencode.db`, and `opencode-<channel>.db` from a build of another release channel.
pub fn dbs_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n == "opencode.db" || (n.starts_with("opencode-") && n.ends_with(".db"))))
        .filter(|p| p.is_file())
        .collect();
    found.sort();
    found
}

/// Every OpenCode database on this computer, each once. The desktop app and the command line share one by default, but a
/// build from another release channel keeps its own, and `OPENCODE_DB` can name any file (relative to the data folder).
pub fn db_paths(home: &Path) -> Vec<PathBuf> {
    let named = std::env::var_os("OPENCODE_DB").filter(|v| !v.is_empty()).map(PathBuf::from);
    dbs_among(&usage::log_roots(home, Kind::OpenCode), named)
}

fn dbs_among(roots: &[PathBuf], named: Option<PathBuf>) -> Vec<PathBuf> {
    let candidates: Vec<PathBuf> = match named {
        Some(p) if p.is_absolute() => vec![p],
        Some(p) => roots.iter().map(|r| r.join(&p)).collect(),
        None => Vec::new(),
    };
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    for path in candidates.into_iter().chain(roots.iter().flat_map(|r| dbs_in(r))).filter(|p| p.is_file()) {
        // The same file can be reached through two folders (a data-folder variable that points at the usual one).
        let real = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !seen.contains(&real) {
            seen.push(real);
            out.push(path);
        }
    }
    out
}

fn open(db: &Path) -> Option<Connection> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()?;
    conn.busy_timeout(Duration::from_millis(500)).ok()?;
    Some(conn)
}

/// Whether the database has OpenCode 2's tables, which then hold every session, old ones included.
fn is_v2(conn: &Connection) -> bool {
    conn.query_row("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'session_v2'", [], |_| Ok(())).is_ok()
}

fn n(v: &Value, pointer: &str) -> u64 {
    v.pointer(pointer).and_then(Value::as_u64).unwrap_or(0)
}

fn tokens_of(v: &Value) -> Tokens {
    Tokens {
        input: n(v, "/tokens/input"),
        output: n(v, "/tokens/output") + n(v, "/tokens/reasoning"),
        cache_read: n(v, "/tokens/cache/read"),
        cache_write: n(v, "/tokens/cache/write"),
    }
}

fn project_of(v: &Value, directory: &str) -> Option<String> {
    agents::project_of(v.pointer("/path/cwd").and_then(Value::as_str).or(Some(directory)).filter(|d| !d.is_empty()))
}

/// A finished assistant message with token use, from a `message.data` JSON (or a `session_message.data` one, which has
/// no `role`: its kind is a column, and `replies` only asks for assistant messages).
pub fn reply_of(data: &str, directory: &str) -> Option<Reply> {
    let v: Value = serde_json::from_str(data).ok()?;
    if v.get("role").and_then(Value::as_str).is_some_and(|r| r != "assistant") {
        return None;
    }
    let ts = v.pointer("/time/completed")?.as_u64()?;
    let tokens = tokens_of(&v);
    if tokens.total() == 0 {
        return None;
    }
    let model = v.get("modelID").or_else(|| v.pointer("/model/id")).and_then(Value::as_str).map(str::to_string);
    Some(Reply { ts, model, project: project_of(&v, directory), tokens })
}

fn replies(conn: &Connection, after: u64) -> rusqlite::Result<(Vec<Reply>, u64)> {
    let mut mark = after;
    let mut out = Vec::new();
    let (sessions_sql, messages_sql) = if is_v2(conn) {
        (
            "SELECT id, directory FROM session_v2 WHERE time_updated > ?1",
            "SELECT time_updated, data FROM session_message WHERE session_id = ?1 AND type = 'assistant' AND time_updated > ?2 ORDER BY time_updated",
        )
    } else {
        (
            "SELECT id, directory FROM session WHERE time_updated > ?1",
            "SELECT time_updated, data FROM message WHERE session_id = ?1 AND time_updated > ?2 ORDER BY time_updated",
        )
    };
    let mut sessions = conn.prepare(sessions_sql)?;
    let mut messages = conn.prepare(messages_sql)?;
    let list = sessions.query_map(params![after.saturating_sub(SLACK_MS) as i64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for session in list {
        let (id, directory) = session?;
        let rows = messages.query_map(params![id, after as i64], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (updated, data) = row?;
            mark = mark.max(updated as u64);
            out.extend(reply_of(&data, &directory));
        }
    }
    Ok((out, mark))
}

/// Finished replies updated after `after` (unix ms) and the newest update time seen, `None` when the
/// database can't be read.
pub fn replies_since(db: &Path, after: u64) -> Option<(Vec<Reply>, u64)> {
    replies(&open(db)?, after).ok()
}

/// Folds one `message.data` into a session the way the log parsers do.
fn fold_message(s: &mut Session, data: &str, directory: &str) {
    let Ok(v) = serde_json::from_str::<Value>(data) else { return };
    let created = n(&v, "/time/created");
    if s.project.is_none() {
        s.apply(0, Entry::Meta { project: project_of(&v, directory), model: None });
    }
    match v.get("role").and_then(Value::as_str) {
        Some("user") => s.apply(created, Entry::Prompt { project: None }),
        Some("assistant") => {
            let completed = v.pointer("/time/completed").and_then(Value::as_u64);
            let t = tokens_of(&v);
            let finish = v.get("finish").and_then(Value::as_str);
            s.apply(
                completed.unwrap_or(created),
                Entry::Reply {
                    model: v.get("modelID").and_then(Value::as_str).map(str::to_string),
                    usage: Usage { read: t.input + t.cache_read + t.cache_write, written: t.output },
                    // A reply that ended in tool calls is followed by another step of the same turn.
                    done: completed.is_some() && finish != Some("tool-calls"),
                    tool: false,
                    todos: None,
                    needs_after: None,
                },
            );
        }
        _ => {}
    }
}

/// Folds one `session_message` row into a session. `updated` is the row's update time: a reply that is still
/// streaming has no end time, but its row keeps changing.
fn fold_v2(s: &mut Session, kind: &str, updated: u64, data: &str, directory: &str) {
    let Ok(v) = serde_json::from_str::<Value>(data) else { return };
    let created = n(&v, "/time/created");
    if s.project.is_none() {
        s.apply(0, Entry::Meta { project: agents::project_of(Some(directory).filter(|d| !d.is_empty())), model: None });
    }
    match kind {
        "user" => s.apply(created, Entry::Prompt { project: None }),
        // Written when a turn ends, whether it finished, failed or was stopped.
        "idle" => s.apply(created, Entry::TurnEnd),
        "assistant" => {
            let completed = v.pointer("/time/completed").and_then(Value::as_u64);
            // A reply that ended in tool calls is followed by another step of the same turn; one with no `finish` is still streaming.
            let done = v.get("finish").and_then(Value::as_str).is_some_and(|f| f != "tool-calls");
            let t = tokens_of(&v);
            let unfinished = |tool: &&Value| matches!(tool.pointer("/state/status").and_then(Value::as_str), Some("pending" | "running"));
            let tools: Vec<&Value> = v
                .get("content")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter(|i| i.get("type").and_then(Value::as_str) == Some("tool")).filter(unfinished).collect())
                .unwrap_or_default();
            // A question to you sits in the database as a tool call that stays open until you answer.
            let asking = tools.iter().any(|tool| tool.get("name").and_then(Value::as_str) == Some("question"));
            s.apply(
                completed.unwrap_or(updated).max(created),
                Entry::Reply {
                    model: v.pointer("/model/id").and_then(Value::as_str).map(str::to_string),
                    usage: Usage { read: t.input + t.cache_read + t.cache_write, written: t.output },
                    done,
                    tool: completed.is_none() && !tools.is_empty(),
                    todos: None,
                    needs_after: Some(if asking { NEEDS_MS } else { NEEDS_SLOW_MS }),
                },
            );
        }
        _ => {}
    }
}

fn followed_v2(conn: &Connection, now: u64) -> rusqlite::Result<HashMap<String, Session>> {
    let since = now.saturating_sub(FOLLOW_MS) as i64;
    let mut out = HashMap::new();
    let mut sessions = conn.prepare("SELECT id, directory FROM session_v2 WHERE time_updated > ?1 AND time_archived IS NULL")?;
    let mut messages = conn.prepare("SELECT type, time_updated, data FROM session_message WHERE session_id = ?1 AND time_created > ?2 ORDER BY seq")?;
    let list = sessions.query_map(params![since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for session in list {
        let (id, directory) = session?;
        let mut s = Session::default();
        for row in messages.query_map(params![id, since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))? {
            let (kind, updated, data) = row?;
            fold_v2(&mut s, &kind, updated.max(0) as u64, &data, &directory);
        }
        if s.last_ts == 0 {
            continue;
        }
        out.insert(id, s);
    }
    Ok(out)
}

/// Sessions active in the last day as follow-able `Session`s, by id.
fn followed_in(conn: &Connection, now: u64) -> rusqlite::Result<HashMap<String, Session>> {
    if is_v2(conn) {
        return followed_v2(conn, now);
    }
    let since = now.saturating_sub(FOLLOW_MS) as i64;
    let recent = now.saturating_sub(TOOL_MAX_MS) as i64;
    let mut out = HashMap::new();
    let mut sessions = conn.prepare("SELECT id, directory FROM session WHERE time_updated > ?1 AND time_archived IS NULL")?;
    let mut messages = conn.prepare("SELECT data FROM message WHERE session_id = ?1 AND time_created > ?2 ORDER BY time_created, id")?;
    let mut newest = conn.prepare("SELECT MAX(time_updated) FROM part WHERE session_id = ?1")?;
    let mut running = conn.prepare(
        "SELECT 1 FROM part WHERE session_id = ?1 AND time_updated > ?2 AND json_extract(data, '$.type') = 'tool' \
         AND json_extract(data, '$.state.status') IN ('pending', 'running') LIMIT 1",
    )?;
    let list = sessions.query_map(params![since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for session in list {
        let (id, directory) = session?;
        let mut s = Session::default();
        for data in messages.query_map(params![id, since], |r| r.get::<_, String>(0))? {
            fold_message(&mut s, &data?, &directory);
        }
        if s.last_ts == 0 {
            continue;
        }
        // Parts keep changing while a reply streams, long after its message row was written.
        if s.last_ts as i64 > recent {
            let part_ts: Option<i64> = newest.query_row(params![id], |r| r.get(0))?;
            s.last_ts = s.last_ts.max(part_ts.unwrap_or(0) as u64);
            if s.turn_start.is_some() {
                s.tool_pending = running.exists(params![id, recent])?;
                // OpenCode's ask is not in its database, so a tool still going is waited on like a shell command.
                s.needs_after = Some(agents::NEEDS_SLOW_MS);
            }
        }
        out.insert(id, s);
    }
    Ok(out)
}

/// Sessions to follow for working-agent detection, by session id. Empty when unreadable.
pub fn followed(db: &Path, now: u64) -> HashMap<String, Session> {
    open(db).and_then(|c| followed_in(&c, now).ok()).unwrap_or_default()
}

fn session_rows_in(conn: &Connection, now: u64) -> rusqlite::Result<Vec<SessionRow>> {
    let live = followed_in(conn, now)?;
    let table = if is_v2(conn) { "session_v2" } else { "session" };
    let mut stmt = conn.prepare(&format!(
        "SELECT id, directory, model, time_updated, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write \
         FROM {table} WHERE time_archived IS NULL ORDER BY time_updated DESC LIMIT ?1"
    ))?;
    let rows = stmt.query_map(params![SESSION_ROWS as i64], |r| {
        let id: String = r.get(0)?;
        let directory: String = r.get(1)?;
        let model: Option<String> = r.get(2)?;
        let updated = r.get::<_, i64>(3)? as u64;
        let t = |i: usize| r.get::<_, i64>(i).map(|v| v.max(0) as u64);
        let (input, output, reasoning, cache_read, cache_write) = (t(4)?, t(5)?, t(6)?, t(7)?, t(8)?);
        let model = model.and_then(|m| serde_json::from_str::<Value>(&m).ok()).and_then(|m| m.get("id").and_then(Value::as_str).map(str::to_string));
        let working = live.get(&id).is_some_and(|s| s.working(now));
        Ok(SessionRow {
            id: format!("{ID_PREFIX}{id}"),
            agent: Kind::OpenCode,
            project: agents::project_of(Some(directory.as_str())),
            model,
            last_ts: live.get(&id).map_or(updated, |s| s.last_ts.max(updated)),
            usage: Usage { read: input + cache_read + cache_write, written: output + reasoning },
            working,
        })
    })?;
    rows.collect()
}

/// The newest OpenCode sessions for the drill-down list.
pub fn session_rows(db: &Path, now: u64) -> Vec<SessionRow> {
    open(db).and_then(|c| session_rows_in(&c, now).ok()).unwrap_or_default()
}

/// A text or tool part as a log line; other parts (reasoning, steps, patches) have nothing to show.
fn log_row(ts: i64, role: Option<&str>, kind: Option<&str>, text: Option<&str>, tool: Option<&str>) -> Option<LogLine> {
    let (role, text) = match kind? {
        "text" => (if role == Some("user") { "user" } else { "assistant" }, agents::clip(text?)),
        "tool" => ("tool", tool.unwrap_or("tool").to_string()),
        _ => return None,
    };
    (!text.is_empty()).then_some(LogLine { ts: ts.max(0) as u64, role, text })
}

/// The same lines from `session_message`: a user's text, and an assistant's text and tool calls.
fn log_v2(conn: &Connection, session: &str) -> rusqlite::Result<Vec<LogLine>> {
    let mut stmt = conn.prepare(
        "SELECT type, time_created, data FROM session_message WHERE session_id = ?1 AND type IN ('user', 'assistant') ORDER BY seq DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![session, LOG_LINES as i64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))?;
    let mut lines = Vec::new();
    // Newest message first, so each message's own lines are reversed here and the whole list once more below.
    for row in rows {
        let (kind, created, data) = row?;
        let Ok(v) = serde_json::from_str::<Value>(&data) else { continue };
        if kind == "user" {
            lines.extend(log_row(created, Some("user"), Some("text"), v.get("text").and_then(Value::as_str), None));
            continue;
        }
        let items = v.get("content").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
        for item in items.iter().rev() {
            let at = item.pointer("/time/created").and_then(Value::as_i64).unwrap_or(created);
            let kind = item.get("type").and_then(Value::as_str);
            lines.extend(log_row(at, Some("assistant"), kind, item.get("text").and_then(Value::as_str), item.get("name").and_then(Value::as_str)));
        }
    }
    lines.reverse();
    if lines.len() > LOG_LINES {
        lines.drain(..lines.len() - LOG_LINES);
    }
    Ok(lines)
}

fn log_in(conn: &Connection, session: &str) -> rusqlite::Result<Vec<LogLine>> {
    if is_v2(conn) {
        return log_v2(conn, session);
    }
    let mut stmt = conn.prepare(
        "SELECT p.time_created, json_extract(m.data, '$.role'), json_extract(p.data, '$.type'), \
         json_extract(p.data, '$.text'), json_extract(p.data, '$.tool') \
         FROM part p JOIN message m ON m.id = p.message_id \
         WHERE p.session_id = ?1 AND json_extract(p.data, '$.type') IN ('text', 'tool') \
         ORDER BY p.time_created DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![session, LOG_LINES as i64], |r| {
        Ok(log_row(
            r.get(0)?,
            r.get::<_, Option<String>>(1)?.as_deref(),
            r.get::<_, Option<String>>(2)?.as_deref(),
            r.get::<_, Option<String>>(3)?.as_deref(),
            r.get::<_, Option<String>>(4)?.as_deref(),
        ))
    })?;
    let mut lines = Vec::new();
    for row in rows {
        lines.extend(row?);
    }
    lines.reverse();
    Ok(lines)
}

/// The last messages of one session (`id` is the session id without `ID_PREFIX`), oldest first.
pub fn log(db: &Path, session: &str) -> Vec<LogLine> {
    open(db).and_then(|c| log_in(&c, session).ok()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000_000;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, directory TEXT NOT NULL, model TEXT, time_updated INTEGER NOT NULL, time_archived INTEGER,
               tokens_input INTEGER NOT NULL DEFAULT 0, tokens_output INTEGER NOT NULL DEFAULT 0, tokens_reasoning INTEGER NOT NULL DEFAULT 0,
               tokens_cache_read INTEGER NOT NULL DEFAULT 0, tokens_cache_write INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT NOT NULL, session_id TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL);",
        )
        .unwrap();
        c
    }

    fn message(c: &Connection, id: &str, created: u64, data: &str) {
        c.execute("INSERT INTO message VALUES (?1, 's1', ?2, ?2, ?3)", params![id, created as i64, data]).unwrap();
    }

    fn part(c: &Connection, id: &str, message: &str, at: u64, data: &str) {
        c.execute("INSERT INTO part VALUES (?1, ?2, 's1', ?3, ?3, ?4)", params![id, message, at as i64, data]).unwrap();
    }

    const DONE: &str = r#"{"role":"assistant","modelID":"big-pickle","finish":"stop","path":{"cwd":"C:\\work\\notifi"},
        "tokens":{"input":100,"output":20,"reasoning":5,"cache":{"read":50,"write":10}},"time":{"created":1799999990000,"completed":1799999995000}}"#;

    fn folder(tag: &str, files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notifi-oc-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for f in files {
            fs::write(dir.join(f), b"x").unwrap();
        }
        dir
    }

    #[test]
    fn every_opencode_database_is_found_once() {
        let usual = folder("usual", &["opencode.db", "opencode.db-wal", "opencode.db-shm", "opencode-dev.db", "opencode-dev.db-wal", "opencode-notes.txt", "other.db"]);
        let other = folder("other", &["custom.db"]);
        let names = |dbs: Vec<PathBuf>| dbs.iter().map(|p| p.file_name().unwrap().to_str().unwrap().to_string()).collect::<Vec<_>>();
        // The app's and the command line's database, and a channel build's own.
        assert_eq!(names(dbs_in(&usual)), ["opencode-dev.db", "opencode.db"]);
        // The same folder reached twice is still one set of databases.
        assert_eq!(names(dbs_among(&[usual.clone(), usual.clone()], None)), ["opencode-dev.db", "opencode.db"]);
        // OPENCODE_DB: a path of its own, or a name inside a data folder.
        assert_eq!(names(dbs_among(&[usual.clone()], Some(other.join("custom.db")))), ["custom.db", "opencode-dev.db", "opencode.db"]);
        assert_eq!(names(dbs_among(&[other.clone()], Some(PathBuf::from("custom.db")))), ["custom.db"]);
        // A named file that is not there is ignored.
        assert_eq!(names(dbs_among(&[usual.clone()], Some(PathBuf::from("missing.db")))), ["opencode-dev.db", "opencode.db"]);
        assert!(dbs_in(Path::new("/no/such/folder")).is_empty());
        let _ = (fs::remove_dir_all(usual), fs::remove_dir_all(other));
    }

    #[test]
    fn reply_mapping() {
        let r = reply_of(DONE, "/other").unwrap();
        assert_eq!(r.ts, 1_799_999_995_000);
        assert_eq!(r.model.as_deref(), Some("big-pickle"));
        assert_eq!(r.project.as_deref(), Some("notifi"));
        assert_eq!(r.tokens, Tokens { input: 100, output: 25, cache_read: 50, cache_write: 10 });
        assert!(reply_of(r#"{"role":"assistant","tokens":{"input":1},"time":{"created":1}}"#, "").is_none());
        assert!(reply_of(r#"{"role":"user","time":{"created":1}}"#, "").is_none());
        // No cwd on the message: the session's folder names the project.
        assert_eq!(reply_of(&DONE.replace(r#""path":{"cwd":"C:\\work\\notifi"},"#, ""), "F:/REPOS/BlockCompanion").unwrap().project.as_deref(), Some("BlockCompanion"));
    }

    #[test]
    fn replies_come_once() {
        let c = db();
        c.execute("INSERT INTO session (id, directory, time_updated) VALUES ('s1', '/w', ?1)", params![NOW as i64]).unwrap();
        message(&c, "m1", 1_799_999_990_000, DONE);
        message(&c, "m2", 1_799_999_999_000, r#"{"role":"assistant","time":{"created":1799999999000}}"#);
        let (rows, mark) = replies(&c, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(mark, 1_799_999_999_000);
        assert!(replies(&c, mark).unwrap().0.is_empty());
    }

    #[test]
    fn working_session_and_rows() {
        let c = db();
        c.execute("INSERT INTO session VALUES ('s1', 'C:/work/notifi', '{\"id\":\"big-pickle\"}', ?1, NULL, 100, 20, 5, 50, 10)", params![NOW as i64 - 1000]).unwrap();
        message(&c, "m1", NOW - 30_000, r#"{"role":"user","time":{"created":1799999970000}}"#);
        message(&c, "m2", NOW - 10_000, r#"{"role":"assistant","modelID":"big-pickle","time":{"created":1799999990000}}"#);
        part(&c, "p1", "m2", NOW - 2_000, r#"{"type":"tool","tool":"bash","state":{"status":"running"}}"#);
        let live = followed_in(&c, NOW).unwrap();
        let s = &live["s1"];
        assert_eq!(s.project.as_deref(), Some("notifi"));
        assert!(s.tool_pending);
        assert!(s.working(NOW));
        let rows = session_rows_in(&c, NOW).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].id.as_str(), rows[0].working, rows[0].model.as_deref()), ("opencode:s1", true, Some("big-pickle")));
        assert_eq!(rows[0].usage, Usage { read: 160, written: 25 });
    }

    /// OpenCode 2's layout: the old tables may still be there, frozen, but everything is in these.
    fn db_v2() -> Connection {
        let c = db();
        c.execute_batch(
            "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, directory TEXT NOT NULL, model TEXT, time_updated INTEGER NOT NULL, time_archived INTEGER,
               tokens_input INTEGER NOT NULL DEFAULT 0, tokens_output INTEGER NOT NULL DEFAULT 0, tokens_reasoning INTEGER NOT NULL DEFAULT 0,
               tokens_cache_read INTEGER NOT NULL DEFAULT 0, tokens_cache_write INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE session_message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, type TEXT NOT NULL, seq INTEGER NOT NULL,
               time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL);",
        )
        .unwrap();
        c.execute("INSERT INTO session_v2 VALUES ('s1', 'C:/work/notifi', '{\"id\":\"glm\"}', ?1, NULL, 100, 20, 5, 50, 10)", params![NOW as i64 - 1000]).unwrap();
        c
    }

    fn message_v2(c: &Connection, seq: i64, kind: &str, created: u64, updated: u64, data: &str) {
        c.execute("INSERT INTO session_message VALUES (?1, 's1', ?2, ?3, ?4, ?5, ?6)", params![format!("m{seq}"), kind, seq, created as i64, updated as i64, data]).unwrap();
    }

    const USER_V2: &str = r#"{"time":{"created":1799999970000},"text":"lets do a cool html page"}"#;
    const DONE_V2: &str = r#"{"time":{"created":1799999971000,"completed":1799999980000},"model":{"id":"glm","providerID":"opencode"},"finish":"stop","content":[],
        "tokens":{"input":100,"output":20,"reasoning":5,"cache":{"read":50,"write":10}}}"#;

    #[test]
    fn v2_replies_come_once() {
        let c = db_v2();
        message_v2(&c, 4, "user", 1_799_999_970_000, 1_799_999_970_000, USER_V2);
        message_v2(&c, 6, "assistant", 1_799_999_971_000, 1_799_999_980_000, DONE_V2);
        message_v2(&c, 7, "assistant", 1_799_999_990_000, 1_799_999_999_000, r#"{"time":{"created":1799999990000},"content":[]}"#);
        let (rows, mark) = replies(&c, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].ts, rows[0].model.as_deref(), rows[0].project.as_deref()), (1_799_999_980_000, Some("glm"), Some("notifi")));
        assert_eq!(rows[0].tokens, Tokens { input: 100, output: 25, cache_read: 50, cache_write: 10 });
        assert_eq!(mark, 1_799_999_999_000);
        assert!(replies(&c, mark).unwrap().0.is_empty());
    }

    #[test]
    fn v2_working_session_and_rows() {
        let c = db_v2();
        message_v2(&c, 4, "user", NOW - 30_000, NOW - 30_000, USER_V2);
        // A reply that is still streaming: no end time, but its row keeps being written.
        message_v2(&c, 6, "assistant", NOW - 20_000, NOW - 2_000, r#"{"time":{"created":1799999980000},"model":{"id":"glm"},"content":[{"type":"reasoning","text":""}]}"#);
        let live = followed_in(&c, NOW).unwrap();
        let s = &live["s1"];
        assert_eq!(s.project.as_deref(), Some("notifi"));
        assert!(s.working(NOW) && !s.tool_pending);
        let rows = session_rows_in(&c, NOW).unwrap();
        assert_eq!((rows[0].id.as_str(), rows[0].working, rows[0].model.as_deref()), ("opencode:s1", true, Some("glm")));
        assert_eq!(rows[0].usage, Usage { read: 160, written: 25 });
        // Done: the reply finished with `stop`, and the turn's idle marker came after it.
        message_v2(&c, 7, "idle", NOW - 1_000, NOW - 1_000, r#"{"time":{"created":1799999999000},"outcome":"succeeded"}"#);
        c.execute("UPDATE session_message SET data = ?1 WHERE seq = 6", params![DONE_V2.replace("1799999971000", "1799999980000")]).unwrap();
        assert!(!followed_in(&c, NOW).unwrap()["s1"].working(NOW));
    }

    #[test]
    fn v2_a_question_waits_on_you_and_a_tool_call_in_a_step_does_not() {
        let c = db_v2();
        message_v2(&c, 4, "user", NOW - 60_000, NOW - 60_000, USER_V2);
        let tool = |name: &str, status: &str| format!(r#"{{"time":{{"created":1799999940000}},"content":[{{"type":"tool","name":"{name}","state":{{"status":"{status}"}}}}]}}"#);
        message_v2(&c, 6, "assistant", NOW - 50_000, NOW - 20_000, &tool("question", "running"));
        let s = &followed_in(&c, NOW).unwrap()["s1"];
        assert!(s.tool_pending && s.needs_you(NOW));
        c.execute("UPDATE session_message SET data = ?1 WHERE seq = 6", params![tool("bash", "running")]).unwrap();
        let s = &followed_in(&c, NOW).unwrap()["s1"];
        assert!(s.tool_pending && s.working(NOW) && !s.needs_you(NOW));
    }

    #[test]
    fn v2_log_lines() {
        let c = db_v2();
        message_v2(&c, 4, "user", 1, 1, r#"{"time":{"created":1},"text":"fix the bug"}"#);
        message_v2(&c, 6, "assistant", 2, 5, r#"{"time":{"created":2},"content":[{"type":"reasoning","text":"hmm"},{"type":"tool","name":"read","state":{"output":"secret"},"time":{"created":3}},{"type":"text","text":"done","time":{"created":4}}]}"#);
        message_v2(&c, 7, "system", 6, 6, r#"{"text":"notice"}"#);
        let got: Vec<(&str, String)> = log_v2(&c, "s1").unwrap().into_iter().map(|l| (l.role, l.text)).collect();
        assert_eq!(got, [("user", "fix the bug".to_string()), ("tool", "read".to_string()), ("assistant", "done".to_string())]);
    }

    #[test]
    fn log_lines() {
        let c = db();
        message(&c, "m1", 1, r#"{"role":"user"}"#);
        message(&c, "m2", 2, r#"{"role":"assistant"}"#);
        part(&c, "p1", "m1", 1, r#"{"type":"text","text":"fix the bug"}"#);
        part(&c, "p2", "m2", 2, r#"{"type":"reasoning","text":"hmm"}"#);
        part(&c, "p3", "m2", 3, r#"{"type":"tool","tool":"read","state":{"output":"secret"}}"#);
        part(&c, "p4", "m2", 4, r#"{"type":"text","text":"done"}"#);
        let lines = log_in(&c, "s1").unwrap();
        let got: Vec<(&str, &str)> = lines.iter().map(|l| (l.role, l.text.as_str())).collect();
        assert_eq!(got, [("user", "fix the bug"), ("tool", "read"), ("assistant", "done")]);
    }
}
