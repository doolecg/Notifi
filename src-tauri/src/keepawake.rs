use std::{
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use notifi_platform::{Feature, Guard, KeepAwake, Platform, Support};
use serde::Serialize;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::{image::Image, AppHandle, Emitter, Manager, State};

use crate::{
    energy::{self, logic, win},
    settings::{Settings, Store},
};

const CHANGED_EVENT: &str = "keepawake://changed";
const NS: &str = "keepAwake";

#[derive(Default)]
pub struct KeepAwakeState {
    inner: Mutex<Inner>,
    /// Applies changes one at a time, in the order they were asked for.
    apply: Mutex<()>,
    processes: Mutex<Option<System>>,
}

#[derive(Default)]
struct Inner {
    held: Option<Held>,
    manual: bool,
    ends_at: Option<u64>,
    trigger: bool,
    locked: bool,
    blocked: Option<String>,
    last_nudge: Option<u64>,
}

struct Held {
    _guard: Guard,
    display: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    supported: bool,
    active: bool,
    /// Unix time in ms when it turns itself off; `None` while it runs until turned off.
    ends_at: Option<u64>,
    /// A session is on but waiting for the screen to be unlocked.
    paused: bool,
    /// Held awake by a trigger (apps, mains power, external display) rather than a manual start.
    trigger: bool,
    /// Why a session did not start or was ended, such as a low battery.
    blocked: Option<String>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn settings(app: &AppHandle) -> Settings {
    app.state::<Store>().get()
}

fn status(app: &AppHandle) -> Status {
    let supported = *app.state::<Platform>().capabilities().get(Feature::KeepAwake) == Support::Supported;
    let state = app.state::<KeepAwakeState>();
    let inner = state.inner.lock().unwrap();
    let pause = energy::flag(&settings(app), NS, "pauseOnLock", false);
    Status {
        supported,
        active: inner.held.is_some(),
        ends_at: inner.ends_at,
        paused: inner.held.is_none() && (inner.manual || inner.trigger) && inner.locked && pause,
        trigger: inner.trigger,
        blocked: inner.blocked.clone(),
    }
}

fn changed(app: &AppHandle) -> Status {
    let status = status(app);
    let _ = app.emit(CHANGED_EVENT, &status);
    status
}

/// Whether a session is holding the machine awake right now.
pub fn is_active(app: &AppHandle) -> bool {
    app.state::<KeepAwakeState>().inner.lock().unwrap().held.is_some()
}

#[tauri::command]
pub fn keep_awake_status(app: AppHandle) -> Status {
    status(&app)
}

/// Turns keep awake on for `minutes` (0 = until turned off) or off.
#[tauri::command]
pub async fn set_keep_awake(app: AppHandle, on: bool, minutes: u32) -> Result<Status, String> {
    tauri::async_runtime::spawn_blocking(move || start_stop(&app, on, minutes)).await.map_err(|e| e.to_string())?
}

fn start_stop(app: &AppHandle, on: bool, minutes: u32) -> Result<Status, String> {
    let state: State<KeepAwakeState> = app.state();
    if on {
        if let Some(reason) = battery_reason(app) {
            state.inner.lock().unwrap().blocked = Some(reason.clone());
            changed(app);
            return Err(reason);
        }
    }
    {
        let mut inner = state.inner.lock().unwrap();
        inner.manual = on;
        inner.ends_at = (on && minutes > 0).then(|| now_ms() + u64::from(minutes) * 60_000);
        if on {
            inner.blocked = None;
            inner.last_nudge = None;
        }
    }
    reconcile(app);
    let status = status(app);
    match (on, status.active, status.paused) {
        (true, false, false) => Err(status.blocked.unwrap_or_else(|| "Could not keep the computer awake".into())),
        _ => Ok(status),
    }
}

/// Brings the OS in line with what is wanted: holds or releases the wake lock, and updates the tray.
fn reconcile(app: &AppHandle) {
    let state = app.state::<KeepAwakeState>();
    let _serial = state.apply.lock().unwrap();
    let s = settings(app);
    let want_display = !energy::flag(&s, NS, "allowDisplaySleep", false);
    let pause = energy::flag(&s, NS, "pauseOnLock", false);
    let (wanted, held_display) = {
        let inner = state.inner.lock().unwrap();
        (logic::session_wanted(inner.manual, inner.trigger, inner.locked, pause), inner.held.as_ref().map(|h| h.display))
    };
    match (wanted, held_display) {
        (true, Some(display)) if display == want_display => {}
        (true, _) => {
            let old = state.inner.lock().unwrap().held.take();
            drop(old);
            let platform = app.state::<Platform>().inner().clone();
            let request = KeepAwake { keep_display_on: want_display, reason: "Keep awake is on".into() };
            let result = energy::block(platform.power().keep_awake(request));
            let mut inner = state.inner.lock().unwrap();
            match result {
                Ok(guard) => inner.held = Some(Held { _guard: guard, display: want_display }),
                Err(e) => {
                    inner.blocked = Some(e.to_string());
                    inner.manual = false;
                    inner.ends_at = None;
                }
            }
        }
        (false, Some(_)) => {
            let old = state.inner.lock().unwrap().held.take();
            drop(old);
        }
        (false, None) => {}
    }
    refresh_tray(app, &s);
    energy::lid::sync(app);
    changed(app);
}

fn reconcile_later(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || reconcile(&app));
}

fn battery_reason(app: &AppHandle) -> Option<String> {
    let s = settings(app);
    let cutoff = energy::number(&s, NS, "cutoffLevel", 20.0);
    let on = energy::flag(&s, NS, "batteryCutoff", true);
    let battery = app.state::<Platform>().sensors().battery().ok().flatten()?;
    logic::battery_blocks(on, battery.on_ac, battery.percent, cutoff as f32)
        .then(|| format!("Battery is below {cutoff:.0}%, so Keep awake is off until you plug in."))
}

fn apps_running(app: &AppHandle, wanted: &[String]) -> bool {
    let state = app.state::<KeepAwakeState>();
    let mut guard = state.processes.lock().unwrap();
    let sys = guard.get_or_insert_with(System::new);
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    sys.processes().values().any(|p| {
        let name = p.name().to_string_lossy();
        wanted.iter().any(|w| logic::app_matches(&name, w))
    })
}

fn triggers_hold(app: &AppHandle, s: &Settings) -> bool {
    if !s.enabled(NS) {
        return false;
    }
    let platform = app.state::<Platform>().inner().clone();
    let mut results = Vec::new();
    let apps = energy::strings(s, NS, "triggerApps");
    if !apps.is_empty() {
        results.push(apps_running(app, &apps));
    }
    if energy::flag(s, NS, "triggerPower", false) {
        results.push(match platform.sensors().battery() {
            Ok(Some(b)) => b.on_ac,
            Ok(None) => true,
            Err(_) => false,
        });
    }
    if energy::flag(s, NS, "triggerDisplay", false) {
        results.push(
            energy::block(platform.display().list()).is_ok_and(|list| list.iter().any(|d| !d.internal)),
        );
    }
    logic::triggers_hold(logic::Combine::parse(energy::text(s, NS, "combine", "any")), &results)
}

/// Starts the watcher and, if asked, a session at launch.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    let _ = std::thread::Builder::new().name("notifi-keepawake".into()).spawn(move || {
        let s = settings(&app);
        set_right_click(&app, &s);
        if s.enabled(NS) && energy::flag(&s, NS, "startAtLaunch", false) {
            let _ = start_stop(&app, true, s.panel.keep_awake_minutes);
        }
        let mut n: u64 = 0;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            tick(&app, n);
            n += 1;
        }
    });
}

fn tick(app: &AppHandle, n: u64) {
    let s = settings(app);
    let state = app.state::<KeepAwakeState>();
    let now = now_ms();
    let mut dirty = false;
    {
        let mut inner = state.inner.lock().unwrap();
        if inner.manual && inner.ends_at.is_some_and(|end| end <= now) {
            inner.manual = false;
            inner.ends_at = None;
            dirty = true;
        }
    }
    if n % 10 == 0 {
        let low = battery_reason(app);
        let mut inner = state.inner.lock().unwrap();
        match low {
            Some(reason) => {
                if inner.manual || inner.trigger {
                    inner.manual = false;
                    inner.ends_at = None;
                    inner.trigger = false;
                    dirty = true;
                }
                if inner.blocked.as_ref() != Some(&reason) {
                    inner.blocked = Some(reason);
                    dirty = true;
                }
            }
            None => {
                if inner.blocked.take().is_some() {
                    dirty = true;
                }
            }
        }
    }
    if n % 5 == 0 {
        let blocked = state.inner.lock().unwrap().blocked.is_some();
        let hold = !blocked && triggers_hold(app, &s);
        let mut inner = state.inner.lock().unwrap();
        if inner.trigger != hold {
            inner.trigger = hold;
            dirty = true;
        }
    }
    if dirty {
        reconcile(app);
    }
    if energy::flag(&s, NS, "pointerNudge", false) {
        let due = {
            let inner = state.inner.lock().unwrap();
            inner.held.is_some()
                && logic::nudge_due(inner.last_nudge, now, energy::number(&s, NS, "nudgeInterval", 60.0) as u64)
        };
        if due {
            win::nudge();
            state.inner.lock().unwrap().last_nudge = Some(now);
        }
    }
    if energy::flag(&s, NS, "showTimeLeft", true) && state.inner.lock().unwrap().ends_at.is_some() {
        set_tooltip(app, tooltip(app, &s));
    }
}

fn tooltip(app: &AppHandle, s: &Settings) -> String {
    let state = app.state::<KeepAwakeState>();
    let inner = state.inner.lock().unwrap();
    match (inner.held.is_some(), inner.ends_at) {
        (true, Some(end)) if energy::flag(s, NS, "showTimeLeft", true) => {
            format!("Notifi - awake, {}", logic::time_left_text(end.saturating_sub(now_ms())))
        }
        (true, _) => "Notifi - keeping awake".into(),
        _ => "Notifi".into(),
    }
}

fn set_tooltip(app: &AppHandle, text: String) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = app2.tray_by_id(crate::tray::TRAY_ID) {
            let _ = tray.set_tooltip(Some(&text));
        }
    });
}

/// Tints the tray icon with the chosen colour while a session is active.
fn refresh_tray(app: &AppHandle, s: &Settings) {
    let active = is_active(app);
    let color = logic::parse_hex(energy::text(s, NS, "iconColor", "#4C8DFF"));
    let tip = tooltip(app, s);
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(tray) = app2.tray_by_id(crate::tray::TRAY_ID) else { return };
        let Some(base) = app2.default_window_icon() else { return };
        let icon: Image<'static> = match (active, color) {
            (true, Some(rgb)) => {
                let mut px = base.rgba().to_vec();
                logic::tint(&mut px, rgb);
                Image::new_owned(px, base.width(), base.height())
            }
            _ => Image::new_owned(base.rgba().to_vec(), base.width(), base.height()),
        };
        let _ = tray.set_icon(Some(icon));
        let _ = tray.set_tooltip(Some(&tip));
    });
}

fn set_right_click(app: &AppHandle, s: &Settings) {
    crate::tray::set_right_click_toggle(app, s.enabled(NS) && energy::flag(s, NS, "rightClickToggle", false));
}

/// Starts a session with the panel's last duration, or stops the running one.
pub fn toggle(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let on = !is_active(&app);
        let minutes = settings(&app).panel.keep_awake_minutes;
        let _ = start_stop(&app, on, minutes);
    });
}

pub fn set_locked(app: &AppHandle, locked: bool) {
    app.state::<KeepAwakeState>().inner.lock().unwrap().locked = locked;
    reconcile(app);
}

pub fn settings_changed(app: &AppHandle, s: &Settings) {
    set_right_click(app, s);
    reconcile_later(app);
}

/// Releases the wake lock when the app quits.
pub fn on_exit(app: &AppHandle) {
    let old = {
        let state = app.state::<KeepAwakeState>();
        let mut inner = state.inner.lock().unwrap();
        inner.manual = false;
        inner.held.take()
    };
    drop(old);
}
