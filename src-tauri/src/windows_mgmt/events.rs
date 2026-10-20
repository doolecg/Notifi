//! Reacts to window events: edge snapping while the user drags a window, quit on close when an
//! app's last window goes, and taking over the maximise button for fill screen.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use notifi_platform::{Platform, Rect, WindowEvent, WindowId};
use tauri::AppHandle;

use super::actions::{self, Outcome};
use super::config::{listed, Config};
use super::session::Tracker;
use super::{config, overlay};

static START: Once = Once::new();
static SEEN: Mutex<Option<HashMap<u64, (u32, String)>>> = Mutex::new(None);
static DRAG: Mutex<Option<(WindowId, Arc<AtomicBool>)>> = Mutex::new(None);
static TAKEOVER_BUSY: AtomicBool = AtomicBool::new(false);
static LAST_TAKEOVER: Mutex<Option<Instant>> = Mutex::new(None);

pub fn ensure(app: &AppHandle, cfg: &Config) {
    if !cfg.wants_events() {
        return;
    }
    let app = app.clone();
    START.call_once(move || {
        std::thread::Builder::new().name("notifi-window-events".into()).spawn(move || run(&app)).ok();
    });
}

fn seen<T>(f: impl FnOnce(&mut HashMap<u64, (u32, String)>) -> T) -> T {
    f(SEEN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new))
}

fn run(app: &AppHandle) {
    let platform = super::platform();
    let wm = platform.windows();
    let Ok(rx) = wm.events() else {
        tracing_note("window events are not available here");
        return;
    };
    if let Ok(list) = wm.list() {
        seen(|m| {
            for w in list {
                if let (Some(pid), Some(app)) = (w.pid, w.app_id.clone()) {
                    m.insert(w.id.0, (pid, app));
                }
            }
        });
    }
    while let Ok(event) = rx.recv() {
        let cfg = config();
        match event {
            WindowEvent::Created(id) => {
                if let Some(w) = wm.list().ok().and_then(|l| l.into_iter().find(|w| w.id == id)) {
                    if let (Some(pid), Some(app)) = (w.pid, w.app_id) {
                        seen(|m| m.insert(id.0, (pid, app)));
                    }
                }
            }
            WindowEvent::Destroyed(id) => {
                actions::forget(id);
                let known = seen(|m| m.remove(&id.0));
                if let Some((pid, name)) = known {
                    quit_if_last(&platform, &cfg, pid, &name);
                }
            }
            WindowEvent::MoveSizeStarted(id) if cfg.snap_edges => begin_drag(app, &platform, id),
            WindowEvent::MoveSizeEnded(id) => end_drag(id),
            WindowEvent::Moved(id, bounds) if cfg.fill && cfg.take_over_maximize => take_over_maximize(&platform, id, bounds),
            _ => {}
        }
    }
}

fn tracing_note(msg: &str) {
    eprintln!("windows_mgmt: {msg}");
}

fn quit_if_last(platform: &Platform, cfg: &Config, pid: u32, app: &str) {
    if !cfg.quit_on_close || !listed(&cfg.quit_apps, Some(app)) || listed(&cfg.quit_exceptions, Some(app)) {
        return;
    }
    let platform = platform.clone();
    let grace = Duration::from_millis(cfg.grace_ms);
    std::thread::spawn(move || {
        std::thread::sleep(grace);
        let wm = platform.windows();
        if wm.windows_of_pid(pid).is_ok_and(|w| w.is_empty()) {
            let _ = wm.quit_process(pid);
        }
    });
}

fn begin_drag(app: &AppHandle, platform: &Platform, id: WindowId) {
    let wm = platform.windows();
    if !wm.pointer().is_ok_and(|p| p.left) {
        return;
    }
    let Some(win) = wm.list().ok().and_then(|l| l.into_iter().find(|w| w.id == id)) else { return };
    let cfg = config();
    if cfg.ignores(win.app_id.as_deref()) || win.pid == Some(std::process::id()) {
        return;
    }
    let ended = Arc::new(AtomicBool::new(false));
    *DRAG.lock().unwrap_or_else(|e| e.into_inner()) = Some((id, ended.clone()));
    let (app, platform) = (app.clone(), platform.clone());
    std::thread::spawn(move || follow_drag(&app, &platform, win.id, win.bounds, &ended));
}

fn end_drag(id: WindowId) {
    if let Some((drag_id, ended)) = DRAG.lock().unwrap_or_else(|e| e.into_inner()).take() {
        if drag_id == id {
            ended.store(true, Ordering::SeqCst);
        } else {
            *DRAG.lock().unwrap_or_else(|e| e.into_inner()) = Some((drag_id, ended));
        }
    }
}

fn follow_drag(app: &AppHandle, platform: &Platform, id: WindowId, start: Rect, ended: &AtomicBool) {
    let wm = platform.windows();
    let cfg = config();
    let Ok(areas) = wm.work_areas() else { return };
    let mut tracker = Tracker::new(areas, cfg.edge_rules.clone(), cfg.gap, cfg.margin);
    let t0 = Instant::now();
    let mut last = None;
    let mut sized_checked = false;
    let mut resizing = false;
    let mut pointer = (start.x, start.y);
    loop {
        let done = ended.load(Ordering::SeqCst);
        if let Ok(p) = wm.pointer() {
            pointer = (p.x, p.y);
        }
        let now = t0.elapsed().as_millis() as u64;
        if !sized_checked && now >= 80 {
            sized_checked = true;
            resizing = wm.list().ok().and_then(|l| l.into_iter().find(|w| w.id == id)).is_some_and(|w| (w.bounds.width, w.bounds.height) != (start.width, start.height));
        }
        if resizing {
            break;
        }
        let target = tracker.update(now, pointer, start);
        if target != last {
            match (&target, cfg.snap_preview) {
                (Some(t), true) => overlay::preview(app, t.display, t.rect),
                _ => overlay::hide(app),
            }
            last = target;
        }
        if done {
            break;
        }
        std::thread::sleep(Duration::from_millis(12));
    }
    overlay::hide(app);
    if let (Some(t), false) = (last, resizing) {
        if let Outcome::Failed(e) = actions::snap_to(wm, id, start, t.rect) {
            tracing_note(&e);
        }
    }
}

fn take_over_maximize(platform: &Platform, id: WindowId, bounds: Rect) {
    if bounds.width < 400 || actions::is_filled(id) {
        return;
    }
    {
        let mut last = LAST_TAKEOVER.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_some_and(|t| t.elapsed() < Duration::from_millis(250)) {
            return;
        }
        *last = Some(Instant::now());
    }
    if TAKEOVER_BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let platform = platform.clone();
    std::thread::spawn(move || {
        let wm = platform.windows();
        let cfg = config();
        let maximised = wm.window_state(id).is_ok_and(|s| s.maximized && !s.fullscreen);
        let win = wm.list().ok().and_then(|l| l.into_iter().find(|w| w.id == id));
        if let Some(win) = win.filter(|w| {
            maximised
                && w.pid != Some(std::process::id())
                && !cfg.ignores(w.app_id.as_deref())
                && !listed(&cfg.native_maximize_apps, w.app_id.as_deref())
        }) {
            let _ = wm.restore(id);
            if let Some(fresh) = wm.list().ok().and_then(|l| l.into_iter().find(|w| w.id == id)) {
                let _ = actions::toggle_fill(wm, &cfg, &fresh);
            } else {
                let _ = actions::toggle_fill(wm, &cfg, &win);
            }
        }
        TAKEOVER_BUSY.store(false, Ordering::SeqCst);
    });
}
