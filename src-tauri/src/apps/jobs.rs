//! Install, update and remove jobs: queued per manager, with progress events and cancel.

use std::{
    collections::{HashMap, HashSet},
    io::Read,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use super::{
    exec,
    packages::{self, Action},
    process, AppsState,
};

pub const JOB_EVENT: &str = "apps://job";
pub const CHANGED_EVENT: &str = "apps://changed";

#[derive(Default)]
pub struct Jobs {
    next: AtomicU64,
    running: Mutex<HashMap<u64, u32>>,
    cancelled: Mutex<HashSet<u64>>,
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub manager: String,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub origin: String,
    /// For removals: the version the user confirmed. A different installed version refuses the job.
    pub expected_version: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct JobEvent {
    id: u64,
    name: String,
    manager: String,
    action: Action,
    state: &'static str,
    percent: Option<f32>,
    line: String,
}

/// The last `NN%` or `NN.N%` in a progress line.
pub fn percent_in(line: &str) -> Option<f32> {
    let bytes = line.as_bytes();
    let mut best = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'%' {
            continue;
        }
        let start = bytes[..i].iter().rposition(|c| !(c.is_ascii_digit() || *c == b'.')).map_or(0, |p| p + 1);
        if let Ok(v) = line[start..i].parse::<f32>() {
            if (0.0..=100.0).contains(&v) {
                best = Some(v);
            }
        }
    }
    best
}

#[tauri::command]
pub fn apps_job_run(app: AppHandle, action: Action, targets: Vec<Target>) -> Result<Vec<u64>, String> {
    let state = app.state::<AppsState>();
    let mut ids = Vec::new();
    for target in targets {
        // An id or origin that starts with `-` would be read as an option, and a path or address would make scoop or flatpak install from there.
        if [&target.id, &target.origin].iter().any(|s| s.starts_with('-')) || target.id.contains([':', '\\']) {
            return Err(format!("{} is not a package name", target.id));
        }
        let Some((program, args)) = packages::command_for(&target.manager, action, &target.id, &target.origin) else {
            return Err(format!("{} is not available on this system", target.manager));
        };
        let id = state.jobs.next.fetch_add(1, Ordering::Relaxed) + 1;
        let lock = state.jobs.locks.lock().unwrap().entry(target.manager.clone()).or_default().clone();
        emit(&app, id, &target, action, "queued", None, String::new());
        let handle = app.clone();
        std::thread::spawn(move || {
            let _queue = lock.lock().unwrap_or_else(|e| e.into_inner());
            run_job(&handle, id, &target, action, program, args);
        });
        ids.push(id);
    }
    Ok(ids)
}

#[tauri::command]
pub fn apps_job_cancel(app: AppHandle, id: u64) {
    let state = app.state::<AppsState>();
    state.jobs.cancelled.lock().unwrap().insert(id);
    let pid = state.jobs.running.lock().unwrap().get(&id).copied();
    if let Some(pid) = pid {
        process::kill_tree_by_pid(pid);
    }
}

fn emit(app: &AppHandle, id: u64, t: &Target, action: Action, state: &'static str, percent: Option<f32>, line: String) {
    let _ = app.emit(JOB_EVENT, JobEvent { id, name: t.name.clone(), manager: t.manager.clone(), action, state, percent, line });
}

fn run_job(app: &AppHandle, id: u64, t: &Target, action: Action, program: &str, args: Vec<String>) {
    let jobs = &app.state::<AppsState>().jobs;
    if jobs.cancelled.lock().unwrap().remove(&id) {
        return emit(app, id, t, action, "cancelled", None, String::new());
    }
    if action == Action::Remove {
        if let Some(expected) = &t.expected_version {
            match packages::list_one(&t.manager) {
                Ok(list) => match list.iter().find(|p| p.id == t.id) {
                    None => return emit(app, id, t, action, "failed", None, "It is already gone.".into()),
                    Some(p) if &p.version != expected => {
                        return emit(app, id, t, action, "failed", None, "The package changed since you confirmed. Check it and try again.".into())
                    }
                    Some(_) => {}
                },
                Err(e) => return emit(app, id, t, action, "failed", None, e),
            }
        }
    }
    emit(app, id, t, action, "running", None, String::new());
    let mut cmd = exec::command(&exec::program(program));
    cmd.args(&args).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return emit(app, id, t, action, "failed", None, e.to_string()),
    };
    jobs.running.lock().unwrap().insert(id, child.id());
    let (tx, rx) = mpsc::channel::<String>();
    for stream in [child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>)].into_iter().flatten() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut stream = stream;
            let (mut buf, mut line) = ([0u8; 4096], Vec::new());
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                for &b in &buf[..n] {
                    if b == b'\n' || b == b'\r' {
                        if !line.is_empty() {
                            let _ = tx.send(String::from_utf8_lossy(&line).trim().to_string());
                            line.clear();
                        }
                    } else {
                        line.push(b);
                    }
                }
            }
            if !line.is_empty() {
                let _ = tx.send(String::from_utf8_lossy(&line).trim().to_string());
            }
        });
    }
    drop(tx);
    let (mut last, mut percent) = (String::new(), None);
    for line in rx {
        if line.chars().all(|c| !c.is_alphanumeric() && c != '%') {
            continue;
        }
        percent = percent_in(&line).or(percent);
        last = line;
        emit(app, id, t, action, "running", percent, last.clone());
    }
    let status = child.wait();
    jobs.running.lock().unwrap().remove(&id);
    let was_cancelled = jobs.cancelled.lock().unwrap().remove(&id);
    let ok = status.map(|s| s.success()).unwrap_or(false);
    let state = if was_cancelled { "cancelled" } else if ok { "done" } else { "failed" };
    emit(app, id, t, action, state, ok.then_some(100.0), last);
    if ok {
        if let Some(found) = app.state::<AppsState>().updates.lock().unwrap().as_mut() {
            found.items.retain(|u| !(u.manager == t.manager && u.id == t.id));
        }
    }
    let _ = app.emit(CHANGED_EVENT, ());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_last_percentage() {
        assert_eq!(percent_in("  ████▒▒▒  45%"), Some(45.0));
        assert_eq!(percent_in("Installing 12.5% of 100%"), Some(100.0));
        assert_eq!(percent_in("Downloading 1.2 MB"), None);
        assert_eq!(percent_in("Percentage: 7%"), Some(7.0));
    }
}
