//! Reads OpenCode's local database (`opencode.db`, SQLite) for the AI Agents dashboard. It is opened
//! read-only and only token counts, timestamps, project folders and trimmed message text are read;
//! a missing or locked database gives no data. Nothing leaves this computer.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{params, Connection, OpenFlags};
use serde_json::Value;

use super::{
    agents::{self, Entry, Kind, LogLine, Session, SessionRow, Usage, LOG_LINES, SESSION_ROWS, TOOL_MAX_MS},
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

pub fn db_path(home: &Path) -> Option<PathBuf> {
    usage::log_roots(home, Kind::OpenCode).into_iter().map(|r| r.join("opencode.db")).find(|p| p.is_file())
}

fn open(db: &Path) -> Option<Connection> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()?;
    conn.busy_timeout(Duration::from_millis(500)).ok()?;
    Some(conn)
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

/// A finished assistant message with token use, from a `message.data` JSON.
pub fn reply_of(data: &str, directory: &str) -> Option<Reply> {
    let v: Value = serde_json::from_str(data).ok()?;
    if v.get("role")?.as_str()? != "assistant" {
        return None;
    }
    let ts = v.pointer("/time/completed")?.as_u64()?;
    let tokens = tokens_of(&v);
    if tokens.total() == 0 {
        return None;
    }
    Some(Reply { ts, model: v.get("modelID").and_then(Value::as_str).map(str::to_string), project: project_of(&v, directory), tokens })
}

fn replies(conn: &Connection, after: u64) -> rusqlite::Result<(Vec<Reply>, u64)> {
    let mut mark = after;
    let mut out = Vec::new();
    let mut sessions = conn.prepare("SELECT id, directory FROM session WHERE time_updated > ?1")?;
    let mut messages = conn.prepare("SELECT time_updated, data FROM message WHERE session_id = ?1 AND time_updated > ?2 ORDER BY time_updated")?;
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

/// Sessions active in the last day as follow-able `Session`s, by id.
fn followed_in(conn: &Connection, now: u64) -> rusqlite::Result<HashMap<String, Session>> {
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
    let mut stmt = conn.prepare(
        "SELECT id, directory, model, time_updated, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write \
         FROM session WHERE time_archived IS NULL ORDER BY time_updated DESC LIMIT ?1",
    )?;
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

fn log_in(conn: &Connection, session: &str) -> rusqlite::Result<Vec<LogLine>> {
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
