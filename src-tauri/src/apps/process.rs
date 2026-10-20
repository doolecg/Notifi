//! Kill process (spec/apps/kill-process.md): list, end, force kill, end the tree, restart.
//! Every action re-finds the process by PID and start time first, so a recycled PID is never hit.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Serialize;
use sysinfo::{Pid, Process, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users};
use tauri::{AppHandle, Manager};

use super::AppsState;

#[derive(Default)]
pub struct ProcState {
    sys: Mutex<System>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub memory: u64,
    pub start_time: u64,
    pub parent: Option<u32>,
    pub exe: Option<String>,
    pub user: String,
    pub protected: Option<&'static str>,
    pub can_restart: bool,
}

const WINDOWS_CRITICAL: &[&str] = &[
    "system", "registry", "smss.exe", "csrss.exe", "wininit.exe", "winlogon.exe", "services.exe", "lsass.exe", "lsm.exe", "svchost.exe", "dwm.exe",
    "fontdrvhost.exe", "memory compression", "secure system", "system idle process", "sihost.exe", "ctfmon.exe",
];
const LINUX_CRITICAL: &[&str] = &[
    "systemd", "init", "kthreadd", "dbus-daemon", "dbus-broker", "xorg", "xwayland", "gnome-shell", "kwin_x11", "kwin_wayland", "plasmashell", "sddm", "gdm", "lightdm", "login", "polkitd",
];

/// Why a process is off limits, or `None`.
pub fn protected_reason(name: &str, pid: u32, parent: Option<u32>) -> Option<&'static str> {
    if pid == std::process::id() {
        return Some("Notifi itself");
    }
    if pid == 0 || (cfg!(windows) && pid == 4) || (cfg!(unix) && pid == 1) {
        return Some("A core system process");
    }
    if cfg!(unix) && parent == Some(2) {
        return Some("A kernel thread");
    }
    let lower = name.to_lowercase();
    let list = if cfg!(windows) { WINDOWS_CRITICAL } else { LINUX_CRITICAL };
    list.contains(&lower.as_str()).then_some("A system process the desktop depends on")
}

pub fn snapshot_into(sys: &mut System) {
    let kind = ProcessRefreshKind::nothing()
        .with_cpu()
        .with_memory()
        .with_exe(UpdateKind::OnlyIfNotSet)
        .with_cmd(UpdateKind::OnlyIfNotSet)
        .with_user(UpdateKind::OnlyIfNotSet);
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
}

pub fn snapshot() -> System {
    let mut sys = System::new();
    snapshot_into(&mut sys);
    sys
}

pub fn user_of(users: &Users, p: &Process) -> String {
    p.user_id().and_then(|id| users.get_user_by_id(id)).map(|u| u.name().to_string()).unwrap_or_default()
}

fn info(users: &Users, p: &Process) -> ProcInfo {
    let name = p.name().to_string_lossy().into_owned();
    let pid = p.pid().as_u32();
    let parent = p.parent().map(Pid::as_u32);
    ProcInfo {
        pid,
        protected: protected_reason(&name, pid, parent),
        name,
        cpu: p.cpu_usage(),
        memory: p.memory(),
        start_time: p.start_time(),
        parent,
        can_restart: p.exe().is_some(),
        exe: p.exe().map(|e| e.to_string_lossy().into_owned()),
        user: user_of(users, p),
    }
}

#[tauri::command]
pub async fn apps_proc_list(app: AppHandle) -> Result<Vec<ProcInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppsState>();
        let mut sys = state.procs.sys.lock().unwrap();
        snapshot_into(&mut sys);
        let users = Users::new_with_refreshed_list();
        let mut list: Vec<ProcInfo> = sys.processes().values().map(|p| info(&users, p)).collect();
        list.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.memory.cmp(&a.memory)));
        Ok(list)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn descendants(sys: &System, root: Pid) -> Vec<Pid> {
    let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
    for (pid, p) in sys.processes() {
        if let Some(parent) = p.parent() {
            children.entry(parent).or_default().push(*pid);
        }
    }
    let (mut out, mut stack) = (Vec::new(), vec![root]);
    while let Some(pid) = stack.pop() {
        for &c in children.get(&pid).into_iter().flatten() {
            if c != root && !out.contains(&c) {
                out.push(c);
                stack.push(c);
            }
        }
    }
    out
}

/// Ends a process and everything below it, deepest first (used to cancel a running job).
pub fn kill_tree_by_pid(pid: u32) {
    let sys = snapshot();
    let root = Pid::from_u32(pid);
    for p in descendants(&sys, root).iter().rev() {
        if let Some(proc) = sys.process(*p) {
            proc.kill();
        }
    }
    if let Some(proc) = sys.process(root) {
        proc.kill();
    }
}

#[cfg(windows)]
fn ask_to_close(pid: u32) -> bool {
    use windows::core::BOOL;
    use windows::Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE},
    };
    struct Ctx {
        pid: u32,
        posted: bool,
    }
    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };
        let mut owner = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner)) };
        if owner == ctx.pid && unsafe { IsWindowVisible(hwnd) }.as_bool() && unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_ok() {
            ctx.posted = true;
        }
        BOOL(1)
    }
    let mut ctx = Ctx { pid, posted: false };
    let _ = unsafe { EnumWindows(Some(each), LPARAM(&mut ctx as *mut Ctx as isize)) };
    ctx.posted
}

fn alive(pid: Pid, start: u64) -> bool {
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing());
    sys.process(pid).is_some_and(|p| p.start_time() == start)
}

fn wait_gone(pid: Pid, start: u64, limit: Duration) -> bool {
    let began = Instant::now();
    while alive(pid, start) {
        if began.elapsed() > limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    true
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    End,
    Force,
    Tree,
    Restart,
}

impl Mode {
    fn parse(s: &str) -> Result<Self, String> {
        Ok(match s {
            "end" => Self::End,
            "force" => Self::Force,
            "tree" => Self::Tree,
            "restart" => Self::Restart,
            _ => return Err("Unknown action".into()),
        })
    }
}

/// `source` is `ports` or `processes`; it picks which "ask before ending" setting applies.
#[tauri::command]
pub async fn apps_proc_end(app: AppHandle, pid: u32, start_time: u64, mode: String, source: String, confirmed: bool) -> Result<String, String> {
    let mode = Mode::parse(&mode)?;
    tauri::async_runtime::spawn_blocking(move || end(&app, pid, start_time, mode, &source, confirmed)).await.map_err(|e| e.to_string())?
}

fn end(app: &AppHandle, pid: u32, start_time: u64, mode: Mode, source: &str, confirmed: bool) -> Result<String, String> {
    let sys = snapshot();
    let target = Pid::from_u32(pid);
    let Some(proc) = sys.process(target).filter(|p| p.start_time() == start_time) else {
        return Ok("It has already ended.".into());
    };
    let name = proc.name().to_string_lossy().into_owned();
    let reason = protected_reason(&name, pid, proc.parent().map(Pid::as_u32));
    if reason.is_some() && super::flag(app, "killProcess", "protectSystem", true) {
        return Err(format!("{name} is protected: {}.", reason.unwrap().to_lowercase()));
    }
    let ask = if source == "ports" { super::flag(app, "portManager", "confirmStop", true) } else { super::flag(app, "killProcess", "confirm", true) };
    if (ask || reason.is_some()) && !confirmed {
        return Err("Confirm before ending this process.".into());
    }
    match mode {
        Mode::Force => {
            if !proc.kill() {
                return Err(why_failed(&name));
            }
            Ok(format!("{name} was force killed."))
        }
        Mode::Tree => {
            for p in descendants(&sys, target).iter().rev() {
                if let Some(child) = sys.process(*p) {
                    child.kill();
                }
            }
            if !proc.kill() {
                return Err(why_failed(&name));
            }
            Ok(format!("{name} and its child processes were ended."))
        }
        Mode::End => {
            graceful(&sys, proc, &name)?;
            Ok(format!("{name} was asked to exit."))
        }
        Mode::Restart => {
            let exe = proc.exe().map(|e| e.to_path_buf()).ok_or("This app cannot be relaunched: its location is unknown.")?;
            let args: Vec<_> = proc.cmd().iter().skip(1).cloned().collect();
            if !proc.kill() {
                return Err(why_failed(&name));
            }
            if !wait_gone(target, start_time, Duration::from_secs(4)) {
                return Err(format!("{name} did not exit in time, so it was not relaunched."));
            }
            let mut cmd = super::exec::command(&exe.to_string_lossy());
            cmd.args(args);
            if let Some(dir) = exe.parent() {
                cmd.current_dir(dir);
            }
            cmd.spawn().map_err(|e| format!("{name} was ended but could not be relaunched: {e}"))?;
            Ok(format!("{name} was restarted."))
        }
    }
}

fn why_failed(name: &str) -> String {
    format!("{name} could not be ended. It may belong to another user or be elevated; run Notifi as administrator to end it.")
}

#[cfg(windows)]
fn graceful(_sys: &System, proc: &Process, name: &str) -> Result<(), String> {
    if ask_to_close(proc.pid().as_u32()) {
        Ok(())
    } else if proc.kill() {
        Ok(())
    } else {
        Err(why_failed(name))
    }
}

#[cfg(not(windows))]
fn graceful(_sys: &System, proc: &Process, name: &str) -> Result<(), String> {
    use sysinfo::Signal;
    if proc.kill_with(Signal::Term) != Some(true) {
        return Err(why_failed(name));
    }
    let (pid, start) = (proc.pid(), proc.start_time());
    std::thread::spawn(move || {
        if !wait_gone(pid, start, Duration::from_secs(5)) {
            let sys = snapshot();
            if let Some(p) = sys.process(pid).filter(|p| p.start_time() == start) {
                p.kill();
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critical_processes_are_protected() {
        if cfg!(windows) {
            assert!(protected_reason("lsass.exe", 700, Some(500)).is_some());
            assert!(protected_reason("LSASS.EXE", 700, None).is_some());
            assert!(protected_reason("notepad.exe", 4242, Some(1)).is_none());
        } else {
            assert!(protected_reason("systemd", 1, None).is_some());
            assert!(protected_reason("kworker/0:1", 99, Some(2)).is_some());
            assert!(protected_reason("firefox", 4242, Some(1000)).is_none());
        }
        assert!(protected_reason("anything", std::process::id(), None).is_some());
    }

    #[test]
    fn modes_parse() {
        assert_eq!(Mode::parse("tree"), Ok(Mode::Tree));
        assert!(Mode::parse("nope").is_err());
    }

    #[test]
    fn a_started_child_can_be_force_killed_and_a_stale_start_time_is_refused() {
        let mut child = if cfg!(windows) {
            super::super::exec::command("ping").args(["-n", "30", "127.0.0.1"]).spawn().unwrap()
        } else {
            super::super::exec::command("sleep").arg("30").spawn().unwrap()
        };
        let pid = Pid::from_u32(child.id());
        let sys = snapshot();
        let start = sys.process(pid).map(|p| p.start_time()).expect("child visible");
        assert!(!alive(pid, start + 1000));
        assert!(alive(pid, start));
        sys.process(pid).unwrap().kill();
        let _ = child.wait();
        assert!(wait_gone(pid, start, Duration::from_secs(5)));
    }
}
