//! Reads Antigravity's local conversation index for the AI Agents dashboard. The desktop app and the `agy`
//! command line share one folder (`~/.gemini/antigravity`): `conversation_summaries.db` has a row per
//! conversation, with its folder and whether a turn is running, and every conversation has a database of its
//! own beside it that is written to while the agent works. The index is opened read-only and only titles,
//! folders, times and the run status are read; a missing or locked database gives no data. Nothing leaves this computer.
//!
//! Token use comes from the same conversation databases: every model call is a step whose record says when it
//! happened and how many tokens it took, and the generation record beside it says whether the model was Claude, a
//! GPT or Gemini. Both are protobuf, of which only those few numbered fields are read.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, UNIX_EPOCH},
};

use rusqlite::{Connection, OpenFlags};

use super::{
    agents::{self, Entry, Kind, Session, SessionRow, Usage, NEEDS_SLOW_MS, SESSION_ROWS},
    usage,
};

pub const ID_PREFIX: &str = "antigravity:";
const RUNNING: &str = "CASCADE_RUN_STATUS_RUNNING";
/// Conversations untouched for longer than this are not followed for working-agent detection.
const FOLLOW_MS: u64 = 24 * 3_600_000;
/// Most conversations read from the index in one pass.
const MAX_ROWS: i64 = 200;

/// One conversation from the index.
#[derive(Clone, Debug, PartialEq)]
pub struct Conversation {
    pub id: String,
    pub project: Option<String>,
    /// A conversation started by another one (a subagent): part of it, never a session of its own.
    pub sub: bool,
    pub session: Session,
}

pub fn db_path(home: &Path) -> Option<PathBuf> {
    usage::log_roots(home, Kind::Antigravity).into_iter().map(|r| r.join("conversation_summaries.db")).find(|p| p.is_file())
}

fn open(db: &Path) -> Option<Connection> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()?;
    conn.busy_timeout(Duration::from_millis(500)).ok()?;
    Some(conn)
}

/// `%20`-style escapes in a file URL back to characters.
fn unescape(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let byte = s.as_bytes()[i];
        match (byte, s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(decoded)) => {
                out.push(decoded);
                i += 3;
            }
            _ => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The folder name of a conversation's first workspace, from the index's JSON list of file URLs.
fn project_of(workspaces: &str) -> Option<String> {
    let list: Vec<String> = serde_json::from_str(workspaces).ok()?;
    let url = list.first()?;
    agents::project_of(Some(unescape(url.strip_prefix("file:///").unwrap_or(url)).as_str()))
}

/// When a file was last written, unix ms; 0 when it is not there.
fn modified(path: &Path) -> u64 {
    fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64)
}

/// When a conversation's own database was last written: it is written to as steps finish, and the changes sit in
/// its write-ahead file until the next checkpoint, so both count.
fn written(dir: &Path, id: &str) -> u64 {
    modified(&dir.join(format!("{id}.db"))).max(modified(&dir.join(format!("{id}.db-wal"))))
}

/// The follow-able state of one conversation. While a turn runs the agent is working: a conversation that goes
/// quiet for a while is more likely waiting on you (a permission, a question) than done, since the index
/// says when a turn ends.
fn session_of(project: Option<String>, running: bool, user_input: u64, modified_at: u64, written_at: u64) -> Session {
    let mut s = Session::default();
    let last = modified_at.max(written_at);
    s.apply(0, Entry::Meta { project, model: None });
    s.apply(last, Entry::Activity);
    if running {
        s.apply(if user_input > 0 { user_input.min(last) } else { last }, Entry::Prompt { project: None });
        s.apply(last, Entry::Reply { model: None, usage: Usage::default(), done: false, tool: true, todos: None, needs_after: Some(NEEDS_SLOW_MS) });
    }
    s
}

fn conversations_in(conn: &Connection, dir: &Path, now: u64) -> rusqlite::Result<Vec<Conversation>> {
    let since = now.saturating_sub(FOLLOW_MS);
    let mut stmt = conn.prepare(
        "SELECT conversation_id, workspace_uris, status, last_modified_time, last_user_input_time, \
                COALESCE(parent_conversation_id, ''), COALESCE(nesting_depth, 0), COALESCE(killed, 0) \
         FROM conversation_summaries ORDER BY last_modified_time DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([MAX_ROWS], |r| {
        let time = |i: usize| r.get::<_, Option<String>>(i).map(|t| t.as_deref().and_then(agents::parse_ts).unwrap_or(0));
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, time(3)?, time(4)?, r.get::<_, String>(5)?, r.get::<_, i64>(6)?, r.get::<_, i64>(7)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, workspaces, status, modified_at, user_input, parent, depth, killed) = row?;
        let written_at = written(dir, &id);
        if modified_at.max(written_at) < since {
            continue;
        }
        let running = status.as_deref() == Some(RUNNING) && killed == 0;
        let project = workspaces.as_deref().and_then(project_of);
        let session = session_of(project.clone(), running, user_input, modified_at, written_at);
        out.push(Conversation { id, project, sub: !parent.is_empty() || depth > 0, session });
    }
    Ok(out)
}

/// Conversations active in the last day, newest first. Empty when the index can't be read.
pub fn followed(db: &Path, now: u64) -> Vec<Conversation> {
    let Some(dir) = db.parent().map(|p| p.join("conversations")) else { return Vec::new() };
    open(db).and_then(|c| conversations_in(&c, &dir, now).ok()).unwrap_or_default()
}

/// The newest conversations for the drill-down list.
pub fn session_rows(db: &Path, now: u64) -> Vec<SessionRow> {
    followed(db, now)
        .into_iter()
        .filter(|c| !c.sub)
        .take(SESSION_ROWS)
        .map(|c| SessionRow {
            id: format!("{ID_PREFIX}{}", c.id),
            agent: Kind::Antigravity,
            project: c.project,
            model: None,
            last_ts: c.session.last_ts,
            usage: Usage::default(),
            working: c.session.working(now),
        })
        .collect()
}

// ---- Tokens used ----

/// One value in a protobuf message: a number, or a string or sub-message.
enum Wire<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

fn varint(b: &[u8]) -> Option<(u64, &[u8])> {
    let mut v = 0u64;
    for (i, &x) in b.iter().enumerate().take(10) {
        v |= u64::from(x & 0x7F) << (7 * i);
        if x & 0x80 == 0 {
            return Some((v, &b[i + 1..]));
        }
    }
    None
}

/// The fields of one message, in order. What can't be read ends the list, and a field this doesn't use is skipped.
fn fields(mut b: &[u8]) -> Vec<(u32, Wire<'_>)> {
    let mut out = Vec::new();
    while !b.is_empty() {
        let Some((key, rest)) = varint(b) else { break };
        b = rest;
        let field = (key >> 3) as u32;
        match key & 7 {
            0 => {
                let Some((v, rest)) = varint(b) else { break };
                b = rest;
                out.push((field, Wire::Varint(v)));
            }
            1 if b.len() >= 8 => b = &b[8..],
            5 if b.len() >= 4 => b = &b[4..],
            2 => {
                let Some((n, rest)) = varint(b) else { break };
                let Some(chunk) = rest.get(..n as usize) else { break };
                out.push((field, Wire::Bytes(chunk)));
                b = &rest[n as usize..];
            }
            _ => break,
        }
    }
    out
}

fn bytes_of<'a>(f: &[(u32, Wire<'a>)], n: u32) -> Option<&'a [u8]> {
    f.iter().find_map(|(k, w)| match w {
        Wire::Bytes(b) if *k == n => Some(*b),
        _ => None,
    })
}

fn num_of(f: &[(u32, Wire<'_>)], n: u32) -> u64 {
    f.iter()
        .find_map(|(k, w)| match w {
            Wire::Varint(v) if *k == n => Some(*v),
            _ => None,
        })
        .unwrap_or(0)
}

fn text(b: Option<&[u8]>) -> Option<&str> {
    std::str::from_utf8(b?).ok()
}

/// Which models a call used. Google keeps Claude and GPT on one quota, but they are different models, so they are counted apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    Gemini,
    Claude,
    Gpt,
}

impl Family {
    pub const ALL: [Family; 3] = [Family::Gemini, Family::Claude, Family::Gpt];

    pub fn name(self) -> &'static str {
        match self {
            Family::Gemini => "Gemini",
            Family::Claude => "Claude",
            Family::Gpt => "GPT",
        }
    }
}

/// A generation record (`gen_metadata`): the id of the call it describes, and the family of model that made it.
fn family_of(record: &[u8]) -> Option<(String, Family)> {
    let top = fields(record);
    let gen = fields(bytes_of(&top, 1)?);
    let id = text(bytes_of(&fields(bytes_of(&gen, 4)?), 7))?.to_string();
    let name = text(bytes_of(&gen, 19)).unwrap_or_default().to_ascii_lowercase();
    // Flags the record carries as key/value pairs: `used_claude` and `used_non_gemini_model` are "true" or "false".
    let flag = |key: &str| {
        gen.iter().filter(|(k, _)| *k == 20).any(|(_, w)| {
            let Wire::Bytes(entry) = w else { return false };
            let pair = fields(entry);
            text(bytes_of(&pair, 1)) == Some(key) && text(bytes_of(&pair, 2)) == Some("true")
        })
    };
    let family = if name.contains("claude") || flag("used_claude") {
        Family::Claude
    } else if name.contains("gpt") || name.contains("openai") || flag("used_non_gemini_model") {
        Family::Gpt
    } else {
        Family::Gemini
    };
    Some((id, family))
}

/// One model call's step record: when it happened (unix ms), the id that ties it to its generation record, and the tokens it took
/// (read, written and cache, as the other agents count them). Steps that aren't model calls have no usage and give `None`.
fn call_of(metadata: &[u8]) -> Option<(u64, String, u64)> {
    let step = fields(metadata);
    let usage = fields(bytes_of(&step, 9)?);
    let id = text(bytes_of(&usage, 7))?.to_string();
    let tokens = [2, 3, 4, 5].iter().map(|n| num_of(&usage, *n)).sum::<u64>();
    let seconds = num_of(&fields(bytes_of(&step, 1)?), 1);
    (tokens > 0 && seconds > 0).then_some((seconds * 1000, id, tokens))
}

/// One model call: when, which family, how many tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Call {
    at: u64,
    family: Family,
    tokens: u64,
}

fn calls_in(conn: &Connection) -> rusqlite::Result<Vec<Call>> {
    let mut families: HashMap<String, Family> = HashMap::new();
    for data in conn.prepare("SELECT data FROM gen_metadata")?.query_map([], |r| r.get::<_, Vec<u8>>(0))? {
        if let Some((id, family)) = family_of(&data?) {
            families.insert(id, family);
        }
    }
    let mut out = Vec::new();
    for metadata in conn.prepare("SELECT metadata FROM steps")?.query_map([], |r| r.get::<_, Option<Vec<u8>>>(0))? {
        let Some((at, id, tokens)) = metadata?.as_deref().and_then(call_of) else { continue };
        out.push(Call { at, family: families.get(&id).copied().unwrap_or(Family::Gemini), tokens });
    }
    Ok(out)
}

/// Tokens by family over the last 5 hours and the last 7 days.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FamilyUse {
    pub family: Family,
    pub five_hour: u64,
    pub week: u64,
}

const FIVE_HOURS: u64 = 5 * 3_600_000;
const WEEK: u64 = 7 * 24 * 3_600_000;

/// The use of each family that has any in the last 7 days.
fn totals(calls: &[Call], now: u64) -> Vec<FamilyUse> {
    Family::ALL
        .into_iter()
        .map(|family| {
            let of = |span: u64| calls.iter().filter(|c| c.family == family && c.at + span > now).map(|c| c.tokens).sum();
            FamilyUse { family, five_hour: of(FIVE_HOURS), week: of(WEEK) }
        })
        .filter(|u| u.week > 0)
        .collect()
}

/// When a conversation's database was last written, unix ms: the file, or its write-ahead file, whichever is newer.
fn written_at(db: &Path) -> u64 {
    let wal = db.with_extension("db-wal");
    modified(db).max(modified(&wal))
}

/// What has been read from each conversation, and when it was written then: a conversation is read again only when it changes.
type Read = HashMap<PathBuf, (u64, Vec<Call>)>;
static READ: Mutex<Option<Read>> = Mutex::new(None);

/// Tokens used in the last 5 hours and 7 days, by model family, from every conversation written to in the last 7 days.
pub fn usage_by_family(home: &Path, now: u64) -> Vec<FamilyUse> {
    let Some(dir) = db_path(home).and_then(|db| db.parent().map(|p| p.join("conversations"))) else { return Vec::new() };
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut guard = READ.lock().unwrap();
    let cache = guard.get_or_insert_with(HashMap::new);
    let mut calls: Vec<Call> = Vec::new();
    let mut live: Vec<PathBuf> = Vec::new();
    for path in entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "db")) {
        let at = written_at(&path);
        if at + WEEK < now {
            continue;
        }
        if cache.get(&path).map_or(true, |(seen, _)| *seen != at) {
            let read = open(&path).and_then(|c| calls_in(&c).ok());
            // A conversation that can't be read right now (it is being written) keeps what was read before.
            match read {
                Some(found) => {
                    cache.insert(path.clone(), (at, found));
                }
                None => {
                    cache.entry(path.clone()).or_insert((at, Vec::new()));
                }
            }
        }
        if let Some((_, found)) = cache.get(&path) {
            calls.extend_from_slice(found);
        }
        live.push(path);
    }
    cache.retain(|p, _| live.contains(p));
    totals(&calls, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000_000;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE conversation_summaries (conversation_id TEXT PRIMARY KEY, title TEXT, workspace_uris TEXT, status TEXT,
               last_modified_time datetime, last_user_input_time datetime, parent_conversation_id TEXT, nesting_depth INTEGER, killed INTEGER);",
        )
        .unwrap();
        c
    }

    fn row(c: &Connection, id: &str, status: &str, modified: &str, parent: &str, killed: i64) {
        c.execute(
            "INSERT INTO conversation_summaries VALUES (?1, 't', '[\"file:///c%3A/Users/Dayle%20Frost/Desktop/Peter%20Wilson\"]', ?2, ?3, '2027-01-15T08:00:00.123456+00:00', ?4, ?5, ?6)",
            rusqlite::params![id, status, modified, parent, i64::from(!parent.is_empty()), killed],
        )
        .unwrap();
    }

    // 2027-01-15T08:00:00Z is 1_800_000_000_000.
    const FRESH: &str = "2027-01-15 07:59:50.5+00:00";

    // A small protobuf writer, to build records with the layout Antigravity's own have.
    fn put(mut v: u64, out: &mut Vec<u8>) {
        while v >= 0x80 {
            out.push((v as u8 & 0x7F) | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    }
    fn number(field: u32, v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        put(u64::from(field) << 3, &mut out);
        put(v, &mut out);
        out
    }
    fn blob(field: u32, b: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        put(u64::from(field) << 3 | 2, &mut out);
        put(b.len() as u64, &mut out);
        out.extend_from_slice(b);
        out
    }
    fn string(field: u32, s: &str) -> Vec<u8> {
        blob(field, s.as_bytes())
    }
    fn cat(parts: &[Vec<u8>]) -> Vec<u8> {
        parts.concat()
    }

    /// A step that was a model call: its time, and the usage record with the message id.
    fn step(seconds: u64, id: &str, input: u64, output: u64, cache_read: u64) -> Vec<u8> {
        cat(&[
            blob(1, &cat(&[number(1, seconds), number(2, 361_923_500)])),
            blob(9, &cat(&[number(1, 1417), number(2, input), number(3, output), number(5, cache_read), number(6, 24), string(7, id)])),
            number(11, 1417),
        ])
    }

    /// The generation record for that call: the model's name and the flags it carries.
    fn generation(id: &str, name: &str, flags: &[(&str, &str)]) -> Vec<u8> {
        let mut inner = vec![blob(4, &cat(&[number(1, 1417), string(7, id)])), string(19, name)];
        inner.extend(flags.iter().map(|(k, v)| blob(20, &cat(&[string(1, k), string(2, v)]))));
        blob(1, &cat(&inner))
    }

    #[test]
    fn a_model_calls_time_and_tokens_are_read_from_its_step() {
        let (at, id, tokens) = call_of(&step(1_800_000_000, "bot-1", 8880, 335, 6826)).unwrap();
        assert_eq!((at, id.as_str(), tokens), (1_800_000_000_000, "bot-1", 8880 + 335 + 6826));
        // A step with no usage (a tool call), and bytes that are not a record at all, are not calls.
        assert_eq!(call_of(&blob(1, &number(1, 1_800_000_000))), None);
        assert_eq!(call_of(&[0xFF, 0xFF, 0xFF]), None);
        assert_eq!(call_of(&[]), None);
        assert_eq!(call_of(&[0x4A, 0x7F, 1, 2]), None);
    }

    #[test]
    fn claude_gpt_and_gemini_are_told_apart() {
        let family = |name: &str, flags: &[(&str, &str)]| family_of(&generation("bot-1", name, flags)).unwrap();
        assert_eq!(family("gemini-4-argon-trial", &[("used_claude", "false"), ("used_non_gemini_model", "false")]), ("bot-1".to_string(), Family::Gemini));
        assert_eq!(family("model", &[("used_claude", "true"), ("used_non_gemini_model", "true")]).1, Family::Claude);
        assert_eq!(family("claude-sonnet-4-6", &[]).1, Family::Claude);
        assert_eq!(family("model", &[("used_claude", "false"), ("used_non_gemini_model", "true")]).1, Family::Gpt);
        assert_eq!(family("gpt-oss-120b", &[]).1, Family::Gpt);
        assert_eq!(family_of(&[]), None);
    }

    #[test]
    fn use_is_counted_per_family_over_the_last_5_hours_and_7_days() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE gen_metadata (idx INTEGER, data BLOB); CREATE TABLE steps (idx INTEGER, step_type INTEGER, metadata BLOB);").unwrap();
        let now_s = NOW / 1000;
        let calls = [
            ("a", "gemini-4", &[][..], now_s - 600, 1000),
            ("b", "gemini-4", &[][..], now_s - 6 * 3600, 2000),
            ("c", "x", &[("used_claude", "true")][..], now_s - 3600, 300),
            ("d", "x", &[("used_non_gemini_model", "true")][..], now_s - 2 * 86_400, 40),
            ("e", "gemini-4", &[][..], now_s - 8 * 86_400, 99_999),
        ];
        for (i, (id, name, flags, at, tokens)) in calls.into_iter().enumerate() {
            conn.execute("INSERT INTO gen_metadata VALUES (?1, ?2)", rusqlite::params![i as i64, generation(id, name, flags)]).unwrap();
            conn.execute("INSERT INTO steps VALUES (?1, 15, ?2)", rusqlite::params![i as i64, step(at, id, tokens, 0, 0)]).unwrap();
        }
        // A tool call has a step too, but no usage.
        conn.execute("INSERT INTO steps VALUES (9, 132, ?1)", rusqlite::params![blob(1, &number(1, now_s))]).unwrap();
        let by = |u: Vec<FamilyUse>| u.into_iter().map(|u| (u.family, u.five_hour, u.week)).collect::<Vec<_>>();
        assert_eq!(by(totals(&calls_in(&conn).unwrap(), NOW)), [(Family::Gemini, 1000, 3000), (Family::Claude, 300, 300), (Family::Gpt, 0, 40)]);
        assert!(totals(&[], NOW).is_empty());
    }

    #[test]
    fn folder_names_come_from_file_urls() {
        assert_eq!(project_of(r#"["file:///c%3A/Users/Dayle%20Frost/Desktop/PeterWilson"]"#).as_deref(), Some("PeterWilson"));
        assert_eq!(project_of(r#"["file:///f%3A/PROGRAMMING/REPOS/Big%20Haul"]"#).as_deref(), Some("Big Haul"));
        assert_eq!(project_of("[]"), None);
        assert_eq!(project_of("not json"), None);
        assert_eq!(unescape("100%"), "100%");
        assert_eq!(unescape("%zz%4"), "%zz%4");
    }

    #[test]
    fn a_running_conversation_works_until_it_goes_quiet_then_waits_on_you() {
        let c = db();
        row(&c, "a", "CASCADE_RUN_STATUS_RUNNING", FRESH, "", 0);
        let dir = Path::new("/nowhere");
        let live = conversations_in(&c, dir, NOW).unwrap();
        assert_eq!(live.len(), 1);
        let s = &live[0].session;
        assert_eq!(live[0].project.as_deref(), Some("Peter Wilson"));
        assert!(s.working(NOW) && !s.needs_you(NOW));
        // Nothing written for three minutes: still its turn, but probably waiting on you.
        let later = NOW + 3 * 60_000;
        assert!(s.working(later) && s.needs_you(later));
        // Half an hour with no word: a conversation that was left running, not a job.
        assert!(!s.working(NOW + 31 * 60_000));
    }

    #[test]
    fn idle_killed_and_old_conversations_do_not_work() {
        let c = db();
        row(&c, "idle", "CASCADE_RUN_STATUS_IDLE", FRESH, "", 0);
        row(&c, "killed", "CASCADE_RUN_STATUS_RUNNING", FRESH, "", 1);
        row(&c, "old", "CASCADE_RUN_STATUS_RUNNING", "2027-01-10 07:59:50+00:00", "", 0);
        let got = conversations_in(&c, Path::new("/nowhere"), NOW).unwrap();
        let ids: Vec<&str> = got.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, ["idle", "killed"]);
        assert!(got.iter().all(|x| !x.session.working(NOW)));
    }

    #[test]
    fn a_conversation_started_by_another_is_a_subagent() {
        let c = db();
        row(&c, "main", "CASCADE_RUN_STATUS_RUNNING", FRESH, "", 0);
        row(&c, "child", "CASCADE_RUN_STATUS_RUNNING", FRESH, "main", 0);
        let got = conversations_in(&c, Path::new("/nowhere"), NOW).unwrap();
        assert_eq!(got.len(), 2);
        assert!(got.iter().find(|x| x.id == "child").unwrap().sub);
        assert!(!got.iter().find(|x| x.id == "main").unwrap().sub);
    }

    #[test]
    fn a_conversations_own_database_keeps_a_long_turn_alive() {
        let dir = std::env::temp_dir().join(format!("notifi-agy-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.db"), b"x").unwrap();
        let c = db();
        // The index row is years old, but the conversation was written to just now.
        row(&c, "a", "CASCADE_RUN_STATUS_RUNNING", "2020-01-01 04:00:00+00:00", "", 0);
        let now = modified(&dir.join("a.db"));
        let got = conversations_in(&c, &dir, now).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(got.len(), 1);
        assert!(got[0].session.working(now) && !got[0].session.needs_you(now));
    }
}
