//! T3 Code records every chat's runs in its own database, so it knows whether a chat is running, waiting on
//! you or finished, where Claude's log can only be guessed from. The taskbar bar reads it for those chats.
//! Read only; nothing is written, and nothing leaves this computer.

use std::{collections::HashMap, fs, path::Path, time::Duration};

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use super::agents::{parse_ts, AgentSnap, Kind, Limit, SessionChip, CHIP_DONE_MS, DONE_MS};

/// T3 Code's name for each provider, which is also the name of its file in `caches/`.
const PROVIDERS: [(&str, Kind); 4] = [("claudeAgent", Kind::Claude), ("codex", Kind::Codex), ("opencode", Kind::OpenCode), ("antigravity", Kind::Antigravity)];

/// One usage window from a provider's cache: how much of it is used and when it renews. A window that has already renewed
/// reads as empty until T3 Code looks again.
fn window(w: &Value, now: u64) -> Option<Limit> {
    let used = w.get("usedPercent")?.as_f64()?.clamp(0.0, 100.0);
    let resets_at = w.get("resetsAt").and_then(Value::as_str).and_then(parse_ts);
    let renewed = resets_at.is_some_and(|r| r <= now);
    Some(Limit {
        used_percent: if renewed { 0.0 } else { used },
        resets_at: resets_at.filter(|r| *r > now),
        window_ms: w.get("windowDurationMins").and_then(Value::as_u64).map(|m| m * 60_000),
    })
}

/// (session, weekly) from one provider's cache file. A provider can report several of each (Claude has a weekly limit for one model
/// as well as the overall one); the one closest to running out is the one shown.
fn limits_in(text: &str, now: u64) -> (Option<Limit>, Option<Limit>) {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return (None, None) };
    let windows = v.pointer("/usageLimits/windows").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    let fullest = |kind: &str| {
        windows
            .iter()
            .filter(|w| w.get("kind").and_then(Value::as_str) == Some(kind))
            .filter_map(|w| window(w, now))
            .max_by(|a, b| a.used_percent.total_cmp(&b.used_percent))
    };
    (fullest("session"), fullest("weekly"))
}

/// The plan usage T3 Code keeps for each agent (`caches/<provider>.json`, `usageLimits`), the same shape for every provider that
/// reports it. A provider that doesn't (OpenCode) or is turned off in T3 Code has none, and is left out.
pub fn limits(now: u64) -> HashMap<Kind, (Option<Limit>, Option<Limit>)> {
    let Some(dir) = crate::chat::t3_home().map(|h| h.join("caches")) else { return HashMap::new() };
    PROVIDERS
        .into_iter()
        .filter_map(|(name, kind)| {
            let text = fs::read_to_string(dir.join(format!("{name}.json"))).ok()?;
            let found = limits_in(&text, now);
            (found.0.is_some() || found.1.is_some()).then_some((kind, found))
        })
        .collect()
}

/// A run still marked running after this long is left over from a T3 Code that was closed, not a job.
const STALE_MS: u64 = 6 * 3_600_000;
const MAX_CHATS: u32 = 40;

/// Whether T3 Code's server is up; its "running" rows are stale when it was closed in the middle of a run.
fn server_up(dir: &Path) -> bool {
    let pid = fs::read_to_string(dir.join("server-runtime.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("pid")?.as_u64())
        .and_then(|p| u32::try_from(p).ok());
    let Some(pid) = pid.map(Pid::from_u32) else { return false };
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing());
    sys.process(pid).is_some()
}

/// The chats that are running, waiting on you, or finished in the last minute and a half, newest first.
pub fn chats(now: u64) -> Vec<SessionChip> {
    let Some(dir) = crate::chat::t3_home().map(|h| h.join("userdata")) else { return Vec::new() };
    if !server_up(&dir) {
        return Vec::new();
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let Ok(conn) = Connection::open_with_flags(dir.join("statev2.sqlite"), flags) else { return Vec::new() };
    let _ = conn.busy_timeout(Duration::from_millis(300));
    read(&conn, now).unwrap_or_default()
}

/// Adds T3 Code's chats to the agent summaries: they are left out of the log-based count, so nothing is counted twice.
pub fn apply(snaps: &mut [AgentSnap], chats: &[SessionChip], now: u64) {
    for a in snaps {
        let mine = || chats.iter().filter(|c| c.agent == a.id);
        let busy: Vec<&SessionChip> = mine().filter(|c| c.status != "done").collect();
        let just_done = mine().any(|c| c.status == "done" && now.saturating_sub(c.last_ts) < DONE_MS);
        if let Some(longest) = busy.iter().max_by_key(|c| c.elapsed_ms) {
            if !a.working || longest.elapsed_ms > a.elapsed_ms {
                a.elapsed_ms = longest.elapsed_ms;
                a.project = longest.project.clone().or(a.project.take());
            }
            a.working = true;
            a.count += busy.len() as u32;
            a.needs_you |= busy.iter().any(|c| c.status == "needs");
        }
        a.done = a.count == 0 && (a.done || just_done);
    }
}

fn read(conn: &Connection, now: u64) -> rusqlite::Result<Vec<SessionChip>> {
    // Each chat's current run: the one running (messages you queue behind it are runs of their own, still waiting), else its latest
    // finished one. A chat waits on you while one of that run's requests (a question, an approval) is open.
    let mut q = conn.prepare(
        "select t.thread_id, t.title, p.title, r.provider, r.status, r.requested_at, r.completed_at,
            exists(select 1 from orchestration_v2_projection_runtime_requests q
                   where q.thread_id = t.thread_id and q.resolved_at is null and q.status not in ('resolved', 'cancelled', 'expired') and q.created_at >= r.requested_at)
         from orchestration_v2_projection_threads t
         join orchestration_v2_projection_runs r on r.thread_id = t.thread_id
              and r.ordinal = coalesce(
                    (select max(ordinal) from orchestration_v2_projection_runs where thread_id = t.thread_id and status = 'running'),
                    (select max(ordinal) from orchestration_v2_projection_runs where thread_id = t.thread_id and status in ('completed', 'interrupted', 'failed')))
         left join projection_projects p on p.project_id = t.project_id
         where t.deleted_at is null and t.archived_at is null and r.status in ('running', 'completed')
         order by coalesce(r.completed_at, r.requested_at) desc
         limit ?1",
    )?;
    let rows = q.query_map([MAX_CHATS], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?, r.get::<_, Option<String>>(6)?, r.get::<_, bool>(7)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (thread, title, project, provider, status, requested, completed, waiting) = row?;
        let agent = match provider.as_str() {
            "claudeAgent" => Kind::Claude,
            "codex" => Kind::Codex,
            "opencode" => Kind::OpenCode,
            "antigravity" => Kind::Antigravity,
            _ => continue,
        };
        let start = parse_ts(&requested).unwrap_or(now);
        let end = completed.as_deref().and_then(parse_ts);
        let (status, last_ts) = match (status.as_str(), end) {
            ("running", _) if now.saturating_sub(start) < STALE_MS => (if waiting { "needs" } else { "working" }, now),
            ("completed", Some(done)) if now.saturating_sub(done) < CHIP_DONE_MS => ("done", done),
            _ => continue,
        };
        out.push(SessionChip {
            id: format!("t3:{thread}"),
            agent,
            project: project.filter(|p| !p.is_empty() && p != "No project"),
            status,
            elapsed_ms: end.unwrap_or(now).saturating_sub(start),
            last_ts,
            title: Some(title),
            thread: Some(thread),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "create table orchestration_v2_projection_threads (thread_id text primary key, project_id text, title text, deleted_at text, archived_at text);
             create table orchestration_v2_projection_runs (thread_id text, ordinal integer, provider text, status text, requested_at text, completed_at text);
             create table orchestration_v2_projection_runtime_requests (thread_id text, status text, created_at text, resolved_at text);
             create table projection_projects (project_id text, title text);
             insert into projection_projects values ('p1', 'Notifi');",
        )
        .unwrap();
        c
    }

    fn chat(c: &Connection, id: &str, ordinal: u32, status: &str, requested: &str, completed: Option<&str>) {
        c.execute("insert or ignore into orchestration_v2_projection_threads values (?1, 'p1', ?1, null, null)", [id]).unwrap();
        c.execute("insert into orchestration_v2_projection_runs values (?1, ?2, 'claudeAgent', ?3, ?4, ?5)", rusqlite::params![id, ordinal, status, requested, completed]).unwrap();
    }

    const NOW: &str = "2026-10-22T12:00:00.000Z";

    fn now() -> u64 {
        parse_ts(NOW).unwrap()
    }

    /// The shape of `caches/claudeAgent.json` and `caches/antigravity.json`, trimmed.
    fn cache(windows: &str) -> String {
        format!(r#"{{"displayName":"X","models":[{{"slug":"m"}}],"usageLimits":{{"checkedAt":"2026-10-22T11:59:00.000Z","windows":[{windows}]}}}}"#)
    }

    #[test]
    fn plan_usage_is_read_from_a_providers_cache() {
        let text = cache(
            r#"{"id":"five_hour","kind":"session","label":"Session","windowDurationMins":300,"usedPercent":10,"resetsAt":"2026-10-22T15:00:00.000Z"},
               {"id":"seven_day","kind":"weekly","label":"Weekly","windowDurationMins":10080,"usedPercent":64,"resetsAt":"2026-10-24T12:00:00.000Z"},
               {"id":"seven_day_x","kind":"weekly","label":"Weekly, one model","windowDurationMins":10080,"usedPercent":2,"resetsAt":"2026-10-24T12:00:00.000Z"}"#,
        );
        let (session, week) = limits_in(&text, now());
        let (session, week) = (session.unwrap(), week.unwrap());
        assert_eq!((session.used_percent, session.window_ms), (10.0, Some(300 * 60_000)));
        assert_eq!(session.resets_at, parse_ts("2026-10-22T15:00:00.000Z"));
        // Of several weekly limits, the one closest to running out.
        assert_eq!((week.used_percent, week.window_ms), (64.0, Some(10_080 * 60_000)));
    }

    #[test]
    fn a_window_that_has_renewed_reads_as_empty_and_odd_caches_give_nothing() {
        let text = cache(r#"{"kind":"session","usedPercent":80,"resetsAt":"2026-10-22T11:00:00.000Z","windowDurationMins":300}"#);
        let (session, week) = limits_in(&text, now());
        assert_eq!((session.unwrap().used_percent, session.unwrap().resets_at, week), (0.0, None, None));
        // A provider that reports no usage, one that is switched off, and a file that is not JSON.
        assert_eq!(limits_in(r#"{"usageLimits":{"windows":[],"unavailable":{"reason":"unsupported"}}}"#, now()), (None, None));
        assert_eq!(limits_in(r#"{"displayName":"Codex","status":"disabled"}"#, now()), (None, None));
        assert_eq!(limits_in("not json", now()), (None, None));
        // A window with no figure is skipped, and one with no reset time still counts.
        let text = cache(r#"{"kind":"weekly","label":"no figure"},{"kind":"weekly","usedPercent":33}"#);
        assert_eq!(limits_in(&text, now()).1.map(|l| (l.used_percent, l.resets_at)), Some((33.0, None)));
    }

    #[test]
    fn running_waiting_and_just_finished_chats_are_told_apart() {
        let c = db();
        chat(&c, "busy", 1, "running", "2026-10-22T11:50:00.000Z", None);
        chat(&c, "asks", 1, "running", "2026-10-22T11:55:00.000Z", None);
        c.execute("insert into orchestration_v2_projection_runtime_requests values ('asks', 'pending', '2026-10-22T11:58:00.000Z', null)", []).unwrap();
        chat(&c, "fresh", 1, "completed", "2026-10-22T11:58:00.000Z", Some("2026-10-22T11:59:30.000Z"));
        chat(&c, "old", 1, "completed", "2026-10-22T11:00:00.000Z", Some("2026-10-22T11:30:00.000Z"));
        chat(&c, "stopped", 1, "cancelled", "2026-10-22T11:59:00.000Z", Some("2026-10-22T11:59:30.000Z"));
        let got: Vec<(String, &str)> = read(&c, now()).unwrap().into_iter().map(|s| (s.thread.unwrap(), s.status)).collect();
        assert!(got.contains(&("busy".into(), "working")));
        assert!(got.contains(&("asks".into(), "needs")));
        assert!(got.contains(&("fresh".into(), "done")));
        assert_eq!(got.len(), 3, "a chat finished long ago, or stopped by you, is not shown: {got:?}");
    }

    #[test]
    fn a_running_chat_stays_running_behind_queued_and_cancelled_messages() {
        let c = db();
        chat(&c, "q", 1, "running", "2026-10-22T11:50:00.000Z", None);
        chat(&c, "q", 2, "cancelled", "2026-10-22T11:55:00.000Z", Some("2026-10-22T11:55:02.000Z"));
        chat(&c, "q", 3, "queued", "2026-10-22T11:56:00.000Z", None);
        chat(&c, "done", 1, "completed", "2026-10-22T11:58:00.000Z", Some("2026-10-22T11:59:30.000Z"));
        chat(&c, "done", 2, "cancelled", "2026-10-22T11:59:40.000Z", Some("2026-10-22T11:59:42.000Z"));
        let got: Vec<(String, &str)> = read(&c, now()).unwrap().into_iter().map(|s| (s.thread.unwrap(), s.status)).collect();
        assert!(got.contains(&("q".into(), "working")), "{got:?}");
        assert!(got.contains(&("done".into(), "done")), "{got:?}");
    }

    #[test]
    fn only_the_latest_run_counts_and_old_requests_do_not() {
        let c = db();
        chat(&c, "t", 1, "running", "2026-10-22T11:00:00.000Z", None);
        c.execute("insert into orchestration_v2_projection_runtime_requests values ('t', 'pending', '2026-10-22T11:01:00.000Z', null)", []).unwrap();
        chat(&c, "t", 2, "running", "2026-10-22T11:50:00.000Z", None);
        let got = read(&c, now()).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].status, got[0].project.as_deref(), got[0].elapsed_ms), ("working", Some("Notifi"), 10 * 60_000));
    }

    #[test]
    fn a_run_left_running_for_hours_is_stale() {
        let c = db();
        chat(&c, "ghost", 1, "running", "2026-10-22T04:00:00.000Z", None);
        assert!(read(&c, now()).unwrap().is_empty());
    }
}
