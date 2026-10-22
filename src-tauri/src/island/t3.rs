//! T3 Code records every chat's runs in its own database, so it knows whether a chat is running, waiting on
//! you or finished, where Claude's log can only be guessed from. The taskbar bar reads it for those chats.
//! Read only; nothing is written, and nothing leaves this computer.

use std::{fs, path::Path, time::Duration};

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use super::agents::{parse_ts, AgentSnap, Kind, SessionChip, CHIP_DONE_MS, DONE_MS};

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
