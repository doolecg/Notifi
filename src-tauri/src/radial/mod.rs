//! The radial menu: a ring of actions that opens around the pointer. One controller thread owns the
//! open wheel; the pointer hook (`hook`) and the shortcut feed it events, and it polls the pointer to
//! pick the slice under it. The window is a click-through overlay that only draws.

mod logic;

#[cfg(windows)]
#[path = "hook_win.rs"]
mod hook;
#[cfg(not(windows))]
#[path = "hook_other.rs"]
mod hook;

use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::{
        mpsc::{channel, Receiver, RecvTimeoutError, Sender},
        Mutex,
    },
    time::Duration,
};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindowBuilder};

use crate::settings::{Settings, Store};
use logic::{choose, clamp_center, level, sanitize, slice_at, starter_wheel, Outcome, Slice, Wheel, ACTIONS, MAX_SLICES, MIN_SLICES};

pub const WINDOW: &str = "radial";
/// The keybind action behind the starter wheel's shortcut.
pub const ACTION: &str = "radial.open";
const NS: &str = "radialMenu";
const SHOW_EVENT: &str = "radial://show";
const HOVER_EVENT: &str = "radial://hover";
const HIDE_EVENT: &str = "radial://hide";
const FILE: &str = "radial-wheel.json";
const POLL: Duration = Duration::from_millis(16);

/// Side of the overlay window and the wheel's outer radius, in logical pixels.
const WINDOW_SIZE: f64 = 360.0;
const OUTER: f64 = 170.0;

pub enum Event {
    ButtonDown,
    ButtonUp,
    Click { inside: bool },
    Escape,
    Shortcut,
    Reload,
}

#[derive(Default)]
pub struct RadialState {
    tx: Mutex<Option<Sender<Event>>>,
    wheel: Mutex<Wheel>,
    path: Mutex<Option<PathBuf>>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SliceView {
    label: String,
    icon: String,
    submenu: bool,
    empty: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Show {
    slices: Vec<SliceView>,
    back: bool,
    labels: bool,
    animate: bool,
    theme: String,
    size: f64,
    outer: f64,
    inner: f64,
}

struct Look {
    count: usize,
    dead_zone: f64,
    labels: bool,
    animate: bool,
    theme: String,
}

struct Session {
    path: Vec<usize>,
    hovered: Option<usize>,
    /// Opened by holding the mouse button, so releasing it picks.
    from_button: bool,
    center: (i32, i32),
    scale: f64,
    wheel: Vec<Slice>,
    look: Look,
}

fn num(s: &Settings, key: &str, default: f64) -> f64 {
    s.option(NS, key).and_then(Value::as_f64).unwrap_or(default)
}

fn flag(s: &Settings, key: &str, default: bool) -> bool {
    s.option(NS, key).and_then(Value::as_bool).unwrap_or(default)
}

fn text<'a>(s: &'a Settings, key: &str, default: &'a str) -> &'a str {
    s.option(NS, key).and_then(Value::as_str).unwrap_or(default)
}

fn button_code(s: &Settings) -> u32 {
    if !s.enabled(NS) {
        return 0;
    }
    match text(s, "mouseButton", "none") {
        "back" => 1,
        "forward" => 2,
        _ => 0,
    }
}

fn views(ring: &[Slice], count: usize) -> Vec<SliceView> {
    (0..count)
        .map(|i| match ring.get(i) {
            Some(s) if !s.kind.is_empty() => SliceView {
                label: s.label.clone(),
                icon: s.icon.clone(),
                submenu: s.kind == "submenu",
                empty: false,
            },
            _ => SliceView { label: String::new(), icon: String::new(), submenu: false, empty: true },
        })
        .collect()
}

// ---- Setup ----

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("radial.html".into()))
        .title("Notifi radial menu")
        .inner_size(WINDOW_SIZE, WINDOW_SIZE)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .focusable(false)
        .visible(false)
        .focused(false)
        .build()?;
    let _ = window.set_ignore_cursor_events(true);
    #[cfg(windows)]
    crate::switcher::prepare_window(&window);

    let state = app.state::<RadialState>();
    let path = app.path().app_config_dir()?.join(FILE);
    *state.wheel.lock().unwrap() = load(&path);
    *state.path.lock().unwrap() = Some(path);

    let (tx, rx) = channel();
    hook::start(tx.clone());
    *state.tx.lock().unwrap() = Some(tx);
    let handle = app.clone();
    let _ = std::thread::Builder::new().name("radial".into()).spawn(move || controller(handle, rx));
    on_settings_changed(app);
    Ok(())
}

fn load(path: &PathBuf) -> Wheel {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Wheel>(&text).ok())
        .map(sanitize)
        .unwrap_or_else(starter_wheel)
}

pub fn on_settings_changed(app: &AppHandle) {
    let Some(store) = app.try_state::<Store>() else { return };
    hook::set_button(button_code(&store.get()));
    send(app, Event::Reload);
}

/// The starter wheel's shortcut: opens the wheel, or closes it when it is open.
pub fn open_from_shortcut(app: &AppHandle) {
    send(app, Event::Shortcut);
}

fn send(app: &AppHandle, event: Event) {
    if let Some(tx) = app.state::<RadialState>().tx.lock().unwrap().as_ref() {
        let _ = tx.send(event);
    }
}

// ---- Commands ----

#[tauri::command]
pub fn radial_wheel(state: State<RadialState>) -> Wheel {
    state.wheel.lock().unwrap().clone()
}

#[tauri::command]
pub fn radial_wheel_save(state: State<RadialState>, wheel: Wheel) -> Result<Wheel, String> {
    let wheel = sanitize(wheel);
    let path = state.path.lock().unwrap().clone().ok_or("The radial menu has not started yet")?;
    let text = serde_json::to_string_pretty(&wheel).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    fs::write(&path, text).map_err(|e| e.to_string())?;
    *state.wheel.lock().unwrap() = wheel.clone();
    Ok(wheel)
}

#[tauri::command]
pub fn radial_wheel_reset(state: State<RadialState>) -> Result<Wheel, String> {
    radial_wheel_save(state, starter_wheel())
}

/// Shortcut actions a slice can run, as (id, name).
#[tauri::command]
pub fn radial_actions() -> Vec<(&'static str, &'static str)> {
    ACTIONS.to_vec()
}

// ---- Controller ----

fn controller(app: AppHandle, rx: Receiver<Event>) {
    let mut session: Option<Session> = None;
    loop {
        let event = if session.is_some() { rx.recv_timeout(POLL) } else { rx.recv().map_err(|_| RecvTimeoutError::Disconnected) };
        match event {
            Ok(event) => step(&app, &mut session, event),
            Err(RecvTimeoutError::Timeout) => {
                if let Some(s) = session.as_mut() {
                    poll(&app, s);
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn step(app: &AppHandle, session: &mut Option<Session>, event: Event) {
    let Some(store) = app.try_state::<Store>() else { return };
    let settings = store.get();
    let hold = text(&settings, "openMode", "hold") == "hold";
    match event {
        Event::ButtonDown if hold && session.is_none() => *session = open(app, &settings, true),
        Event::ButtonDown => {}
        Event::ButtonUp => match session.as_ref().map(|s| s.from_button) {
            Some(true) => finish(app, session, false),
            Some(false) => close(app, session),
            None if !hold => *session = open(app, &settings, false),
            None => {}
        },
        Event::Shortcut => {
            if session.is_some() {
                close(app, session);
            } else {
                *session = open(app, &settings, false);
            }
        }
        Event::Click { inside } => {
            if session.is_some() {
                if inside { finish(app, session, true) } else { close(app, session) }
            }
        }
        Event::Escape => close(app, session),
        Event::Reload => {
            if !settings.enabled(NS) {
                close(app, session);
            }
        }
    }
}

fn open(app: &AppHandle, settings: &Settings, from_button: bool) -> Option<Session> {
    if !settings.enabled(NS) {
        return None;
    }
    let pos = app.cursor_position().ok()?;
    let monitor = app.monitor_from_point(pos.x, pos.y).ok().flatten().or_else(|| app.primary_monitor().ok().flatten())?;
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let area = (work.position.x, work.position.y, work.position.x + work.size.width as i32, work.position.y + work.size.height as i32);
    let size = (WINDOW_SIZE * scale).round() as i32;
    let center = clamp_center((pos.x.round() as i32, pos.y.round() as i32), size / 2, area);

    let window = app.get_webview_window(WINDOW)?;
    let _ = window.set_size(PhysicalSize::new(size as u32, size as u32));
    let _ = window.set_position(PhysicalPosition::new(center.0 - size / 2, center.1 - size / 2));

    let look = Look {
        count: num(settings, "slices", 8.0).clamp(MIN_SLICES as f64, MAX_SLICES as f64) as usize,
        dead_zone: num(settings, "deadZone", 20.0).clamp(0.0, 100.0),
        labels: flag(settings, "showLabels", true),
        animate: flag(settings, "animations", true),
        theme: text(settings, "theme", "accent").to_string(),
    };
    let wheel = app.state::<RadialState>().wheel.lock().unwrap().slices.clone();
    let session = Session { path: Vec::new(), hovered: None, from_button, center, scale, wheel, look };
    hook::set_open(Some((center.0, center.1, (OUTER * scale).round() as i32)));
    show_level(app, &session);
    #[cfg(windows)]
    crate::switcher::show_inactive(&window);
    #[cfg(not(windows))]
    let _ = window.show();
    Some(session)
}

fn show_level(app: &AppHandle, s: &Session) {
    let ring = level(&s.wheel, &s.path).unwrap_or(&[]);
    let payload = Show {
        slices: views(ring, s.look.count),
        back: !s.path.is_empty(),
        labels: s.look.labels,
        animate: s.look.animate,
        theme: s.look.theme.clone(),
        size: WINDOW_SIZE,
        outer: OUTER,
        inner: s.look.dead_zone.max(24.0),
    };
    let _ = app.emit_to(WINDOW, SHOW_EVENT, payload);
}

fn close(app: &AppHandle, session: &mut Option<Session>) {
    if session.take().is_none() {
        return;
    }
    hook::set_open(None);
    let _ = app.emit_to(WINDOW, HIDE_EVENT, ());
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.hide();
    }
}

fn poll(app: &AppHandle, s: &mut Session) {
    let Ok(pos) = app.cursor_position() else { return };
    let hovered = slice_at(pos.x - f64::from(s.center.0), pos.y - f64::from(s.center.1), s.look.dead_zone * s.scale, s.look.count);
    if hovered != s.hovered {
        s.hovered = hovered;
        let _ = app.emit_to(WINDOW, HOVER_EVENT, hovered);
    }
}

/// Picks the hovered slice: a click (or a tap-mode choice) or the release of the held button.
fn finish(app: &AppHandle, session: &mut Option<Session>, click: bool) {
    let Some(s) = session.as_mut() else { return };
    let outcome = {
        let ring = level(&s.wheel, &s.path).unwrap_or(&[]);
        choose(ring, s.hovered, click, !s.path.is_empty())
    };
    match outcome {
        Outcome::Run(slice) => {
            close(app, session);
            run_slice(app, slice);
        }
        Outcome::Enter(i) => {
            s.path.push(i);
            s.from_button = false;
            s.hovered = None;
            show_level(app, s);
        }
        Outcome::Back => {
            s.path.pop();
            s.hovered = None;
            show_level(app, s);
        }
        Outcome::Close => close(app, session),
        Outcome::Stay => {}
    }
}

// ---- Running a slice ----

fn run_slice(app: &AppHandle, slice: Slice) {
    let app = app.clone();
    std::thread::spawn(move || match slice.kind.as_str() {
        "action" => crate::keybinds::run(&app, &slice.arg),
        "url" if slice.arg.starts_with("https://") || slice.arg.starts_with("http://") => open_target(&slice.arg),
        "open" => open_target(&slice.arg),
        _ => {}
    });
}

/// Opens a file, folder, app, script or address the way a double click would.
fn open_target(target: &str) {
    let program = if cfg!(windows) { "explorer" } else { "xdg-open" };
    let _ = Command::new(program).arg(target).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(label: &str) -> Slice {
        Slice { label: label.into(), icon: "settings".into(), kind: "action".into(), arg: "openSettings".into(), children: Vec::new() }
    }

    #[test]
    fn views_pad_to_the_ring_size_and_cut_extras() {
        let ring = vec![action("a"), Slice::default(), Slice { kind: "submenu".into(), label: "more".into(), ..Slice::default() }];
        let v = views(&ring, 4);
        assert_eq!(v.len(), 4);
        assert!(!v[0].empty && v[0].label == "a");
        assert!(v[1].empty);
        assert!(v[2].submenu);
        assert!(v[3].empty);
        assert_eq!(views(&ring, 2).len(), 2);
    }

    #[test]
    fn button_trigger_needs_the_feature_on() {
        let mut s = Settings::default();
        s.features.entry(NS.into()).or_default().insert("mouseButton".into(), "forward".into());
        assert_eq!(button_code(&s), 0);
        s.features.entry(NS.into()).or_default().insert("enabled".into(), true.into());
        assert_eq!(button_code(&s), 2);
        s.features.entry(NS.into()).or_default().insert("mouseButton".into(), "back".into());
        assert_eq!(button_code(&s), 1);
        s.features.entry(NS.into()).or_default().insert("mouseButton".into(), "none".into());
        assert_eq!(button_code(&s), 0);
    }
}
