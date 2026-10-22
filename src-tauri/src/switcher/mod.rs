//! App switcher: a hold-to-switch window grid with previews, search and filters. The window is
//! created hidden at startup and shown without taking focus, so the app being switched from
//! keeps the keyboard until a window is chosen. Keys arrive from a low-level hook (Windows) or a
//! key grab (X11) and are handled on one controller thread, in order.

mod filter;
mod keys;
mod model;
mod sources;
pub mod taskbar;
mod thumbs;

#[cfg(windows)]
mod hook_win;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notifi_platform::{Platform, WindowId, WindowInfo};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::settings::{Settings, Store};
use filter::{Facts, Rules};
use keys::{Bindings, Command};
use model::{Entry, Grid, Layout, Model};
use thumbs::{Kind, Slot, Thumbs};

pub const WINDOW: &str = "switcher";
const NS: &str = "appSwitcher";
const OPEN_EVENT: &str = "switcher://open";
const UPDATE_EVENT: &str = "switcher://update";
const HIDE_EVENT: &str = "switcher://hide";
const IMAGE_EVENT: &str = "switcher://image";

/// Keybind actions run through keybinds.rs.
pub const ACTIONS: &[&str] = &["switcherOpen", "switcherSameApp"];

pub enum Msg {
    Key(Command),
    Show(u64),
    Pick(u64),
    Hover(u64),
    Close(u64),
    Wheel(isize),
}

#[derive(Default)]
struct Inner {
    model: Option<Model>,
    grid: Option<Grid>,
    mru: Vec<u64>,
    shown: bool,
    generation: u64,
    thumbs: Thumbs,
    icons: HashMap<String, Option<String>>,
    kind: Option<Kind>,
    /// Programs behind the apps-without-windows entries, by entry id.
    idle: HashMap<u64, String>,
}

#[derive(Default)]
pub struct SwitcherState {
    open: Arc<AtomicBool>,
    inner: Mutex<Inner>,
    tx: Mutex<Option<Sender<Msg>>>,
    #[cfg(target_os = "linux")]
    grab: Mutex<Option<notifi_platform::Guard>>,
}

pub(crate) fn opt_bool(s: &Settings, ns: &str, key: &str, default: bool) -> bool {
    s.option(ns, key).and_then(Value::as_bool).unwrap_or(default)
}
pub(crate) fn opt_num(s: &Settings, ns: &str, key: &str, default: f64) -> f64 {
    s.option(ns, key).and_then(Value::as_f64).unwrap_or(default)
}
pub(crate) fn opt_str(s: &Settings, ns: &str, key: &str, default: &str) -> String {
    s.option(ns, key).and_then(Value::as_str).unwrap_or(default).to_string()
}
pub(crate) fn opt_list(s: &Settings, ns: &str, key: &str) -> Vec<String> {
    s.option(ns, key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_lowercase).collect())
        .unwrap_or_default()
}

fn settings(app: &AppHandle) -> Settings {
    app.state::<Store>().get()
}

/// The display name of an app: executable stem on Windows, WM_CLASS on Linux.
pub(crate) fn app_name(w: &WindowInfo) -> String {
    let name = app_name_of(w.app_id.as_deref().unwrap_or_default());
    if name.is_empty() { w.title.clone() } else { name }
}

fn app_name_of(app_id: &str) -> String {
    let stem = app_id.strip_suffix(".exe").or_else(|| app_id.strip_suffix(".EXE")).unwrap_or(app_id);
    let mut c = stem.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

pub(crate) fn listed(list: &[String], app: &str) -> bool {
    let app = app.to_lowercase();
    list.iter().any(|a| {
        let a = a.trim().trim_end_matches(".exe");
        !a.is_empty() && (app == a || app.trim_end_matches(".exe") == a)
    })
}

/// "Merge tabs": windows of one app at exactly the same place and size are tabs of one window, so only
/// the front-most is kept. Minimised windows have no real position and windows on other desktops share
/// no screen with these, so neither is merged.
fn merge_tabs(list: Vec<WindowInfo>, other_desktop: &HashSet<u64>) -> Vec<WindowInfo> {
    let mut kept: Vec<WindowInfo> = Vec::with_capacity(list.len());
    for w in list {
        let stacked = |k: &WindowInfo| {
            !k.minimized && !other_desktop.contains(&k.id.0) && k.app_id == w.app_id && k.bounds == w.bounds
        };
        let twin = !w.minimized && !other_desktop.contains(&w.id.0) && w.app_id.is_some() && kept.iter().any(stacked);
        if !twin {
            kept.push(w);
        }
    }
    kept
}

/// The effective hold triggers: the system Alt+Tab only when replacing it.
fn bindings(s: &Settings) -> Bindings {
    let replace = opt_bool(s, NS, "replaceSystem", true);
    let trigger = opt_str(s, NS, "trigger", "Alt+Tab");
    let alt_tab = keys::parse("Alt+Tab");
    let mut switcher = keys::parse(&trigger);
    if replace && switcher.is_none() {
        switcher = alt_tab;
    }
    if !replace && switcher == alt_tab {
        switcher = None;
    }
    Bindings { switcher, same_app: keys::parse(&opt_str(s, NS, "sameAppKey", "Alt+`")) }
}

/// Builds the switcher window unless it already exists.
pub fn build_window(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(WINDOW).is_some() {
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("switcher.html".into()))
        .title("Notifi switcher")
        .inner_size(640.0, 260.0)
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
    #[cfg(windows)]
    os::prepare(&window);
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::ScaleFactorChanged { .. } = event {
            let _ = handle.emit_to(WINDOW, UPDATE_EVENT, ());
        }
    });
    Ok(())
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    app.manage(SwitcherState::default());
    let (tx, rx) = channel::<Msg>();
    *app.state::<SwitcherState>().tx.lock().unwrap() = Some(tx);
    let handle = app.clone();
    std::thread::Builder::new()
        .name("switcher".into())
        .spawn(move || {
            for msg in rx {
                handle_msg(&handle, msg);
            }
        })
        .map_err(|e| tauri::Error::Anyhow(e.into()))?;
    on_settings_changed(app);
    taskbar::create(app)?;
    Ok(())
}

fn send(app: &AppHandle, msg: Msg) {
    if let Some(tx) = app.state::<SwitcherState>().tx.lock().unwrap().as_ref() {
        let _ = tx.send(msg);
    }
}

/// Re-reads the trigger keys and turns the hook or grab on or off.
pub fn on_settings_changed(app: &AppHandle) {
    let s = settings(app);
    let on = s.enabled(NS);
    let b = bindings(&s);
    let state = app.state::<SwitcherState>();
    #[cfg(windows)]
    {
        let tx = state.tx.lock().unwrap().clone();
        let mut keys = hook_win::KEYS.lock().unwrap();
        let ctx = keys.get_or_insert_with(|| hook_win::KeyCtx {
            bindings: b,
            hook: keys::Hook::default(),
            held: Vec::new(),
            open: state.open.clone(),
            tx: None,
        });
        ctx.bindings = b;
        ctx.tx = tx;
        drop(keys);
        hook_win::apply(on, taskbar::clicks_wanted(&s));
    }
    #[cfg(target_os = "linux")]
    {
        let mut grab = state.grab.lock().unwrap();
        *grab = None;
        if on && app.state::<Platform>().backend() == notifi_platform::Backend::X11 {
            *grab = linux::grab(state.open.clone(), state.tx.lock().unwrap().clone(), b);
        }
    }
    if !on && state.open.load(Ordering::SeqCst) {
        send(app, Msg::Key(Command::Cancel));
    }
}

/// Runs a keybind action from keybinds.rs: opens the switcher in click-to-choose mode.
pub fn run_action(app: &AppHandle, action: &str) {
    let same_app = action == "switcherSameApp";
    if !app.state::<SwitcherState>().open.swap(true, Ordering::SeqCst) {
        send(app, Msg::Key(Command::Open { reverse: false, same_app }));
    }
}

fn cursor_monitor(app: &AppHandle) -> Option<tauri::Monitor> {
    let p = app.cursor_position().ok()?;
    app.monitor_from_point(p.x, p.y).ok().flatten().or_else(|| app.primary_monitor().ok().flatten())
}

fn collect(app: &AppHandle, s: &Settings, same_app: bool, mru: &[u64], kind: Kind) -> Vec<Entry> {
    let platform = app.state::<Platform>();
    let hidden_apps = opt_bool(s, NS, "showHiddenApps", true);
    let found = sources::windows(&platform, hidden_apps);
    let me = std::process::id();
    let rules = Rules {
        show_minimised: opt_bool(s, NS, "showMinimised", true),
        current_desktop_only: opt_bool(s, NS, "currentDesktopOnly", false),
        show_hidden_apps: hidden_apps,
        other_desktop_fullscreen: opt_bool(s, NS, "showOtherDesktopFullscreen", false),
    };
    let display = opt_bool(s, NS, "currentDisplayOnly", false).then(|| cursor_monitor(app).and_then(|m| m.name().cloned())).flatten();
    let paused = opt_list(s, NS, "pauseApps");
    let fg_app = same_app.then(|| found.iter().find(|(w, _)| w.focused).and_then(|(w, _)| w.app_id.clone())).flatten();
    let with_windows: HashSet<String> = found.iter().filter_map(|(w, _)| w.app_id.as_ref().map(|a| a.to_lowercase())).collect();
    let facts: HashMap<u64, Facts> = found.iter().map(|(w, f)| (w.id.0, *f)).collect();
    let list: Vec<WindowInfo> = found
        .into_iter()
        .filter(|(w, _)| w.pid != Some(me))
        .filter(|(_, f)| filter::keep(*f, rules))
        .filter(|(w, _)| display.as_ref().is_none_or(|d| w.monitor.as_ref() == Some(d)))
        .filter(|(w, _)| !same_app || (fg_app.is_some() && w.app_id == fg_app))
        .map(|(w, _)| w)
        .collect();
    let other_desktop: HashSet<u64> = facts.iter().filter(|(_, f)| f.other_desktop).map(|(id, _)| *id).collect();
    let list = if opt_bool(s, NS, "mergeTabs", false) { merge_tabs(list, &other_desktop) } else { list };
    let zorder: Vec<u64> = list.iter().map(|w| w.id.0).collect();
    let order = model::mru_order(&zorder, mru);
    let state = app.state::<SwitcherState>();
    let mut entries: Vec<Entry> = order
        .into_iter()
        .filter_map(|id| list.iter().find(|w| w.id.0 == id))
        .map(|w| {
            let app_key = w.app_id.clone().unwrap_or_else(|| w.title.clone());
            let icon = {
                let cached = state.inner.lock().unwrap().icons.get(&app_key).cloned();
                cached.unwrap_or_else(|| {
                    let url = thumbs::icon_url(w.id.0);
                    state.inner.lock().unwrap().icons.insert(app_key.clone(), url.clone());
                    url
                })
            };
            let name = app_name(w);
            let f = facts.get(&w.id.0).copied().unwrap_or_default();
            Entry {
                id: w.id.0,
                title: w.title.clone(),
                no_thumb: w.minimized || f.tray_hidden || kind == Kind::None || listed(&paused, &app_key) || listed(&paused, &name),
                app: name,
                pid: w.pid,
                minimized: w.minimized,
                monitor: w.monitor.clone(),
                icon,
                other_desktop: f.other_desktop,
                no_window: false,
            }
        })
        .collect();

    let mut idle_paths = HashMap::new();
    if opt_bool(s, NS, "showAppsWithoutWindows", false) && !same_app && display.is_none() {
        let hide_file_manager = opt_bool(s, NS, "hideFileManager", false);
        let window_only = opt_list(s, NS, "windowOnlyApps");
        for a in sources::idle_apps() {
            if a.pid == me || !filter::keep_windowless(&a.app_id, &with_windows, hide_file_manager, &window_only) {
                continue;
            }
            let id = filter::idle_id(a.pid);
            let name = app_name_of(&a.app_id);
            let icon_key = format!("file:{}", a.path.to_lowercase());
            let cached = state.inner.lock().unwrap().icons.get(&icon_key).cloned();
            let icon = cached.unwrap_or_else(|| {
                let url = thumbs::file_icon_url(&a.path);
                state.inner.lock().unwrap().icons.insert(icon_key, url.clone());
                url
            });
            entries.push(Entry {
                id,
                title: name.clone(),
                app: name,
                pid: Some(a.pid),
                minimized: false,
                monitor: None,
                icon,
                no_thumb: true,
                other_desktop: false,
                no_window: true,
            });
            idle_paths.insert(id, a.path);
        }
    }
    state.inner.lock().unwrap().idle = idle_paths;
    entries
}

const ICON_ROW: f64 = 112.0;
const ICON_CELL: f64 = 92.0;
const ROW_H: f64 = 44.0;

fn layout_grid(m: &Model, size: &str, max_cols: usize, avail: (f64, f64)) -> Grid {
    match m.layout {
        Layout::Previews => model::grid(m.entries.len(), model::thumb_size(size), max_cols, avail),
        Layout::Icons => {
            let groups = m.groups();
            let widest = groups.iter().map(|g| g.1.len()).max().unwrap_or(1);
            let mut g = model::grid(widest, model::thumb_size(size), max_cols, (avail.0, avail.1 - ICON_ROW));
            let icons_w = groups.len() as f64 * ICON_CELL + 2.0 * model::PAD;
            g.width = g.width.max(icons_w).min(avail.0);
            g.height += ICON_ROW;
            g
        }
        Layout::Simple => {
            let n = m.entries.len() as f64;
            let height = (n * ROW_H + 2.0 * model::PAD + model::SEARCH).min(avail.1);
            Grid { cols: 1, rows: m.entries.len(), card_w: 520.0, card_h: ROW_H, thumb_h: 0.0, width: 560.0_f64.min(avail.0), height }
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OpenPayload<'a> {
    model: &'a Model,
    grid: Grid,
    kind: Kind,
    size: String,
    hover_buttons: bool,
    skip_animations: bool,
    wheel: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UpdatePayload<'a> {
    query: &'a str,
    visible: &'a [usize],
    selected: usize,
    group: usize,
    group_window: usize,
}

fn emit_full(app: &AppHandle, inner: &Inner, s: &Settings) {
    let (Some(m), Some(grid)) = (&inner.model, inner.grid) else { return };
    let payload = OpenPayload {
        model: m,
        grid,
        kind: inner.kind.unwrap_or(Kind::None),
        size: opt_str(s, NS, "thumbnailSize", "normal"),
        hover_buttons: opt_bool(s, NS, "hoverButtons", true),
        skip_animations: opt_bool(s, NS, "skipAnimations", false),
        wheel: opt_bool(s, NS, "wheelBrowse", true),
    };
    let _ = app.emit_to(WINDOW, OPEN_EVENT, payload);
}

fn emit_update(app: &AppHandle, m: &Model) {
    let payload = UpdatePayload { query: &m.query, visible: &m.visible, selected: m.selected, group: m.group, group_window: m.group_window };
    let _ = app.emit_to(WINDOW, UPDATE_EVENT, payload);
}

fn open(app: &AppHandle, reverse: bool, same_app: bool) {
    let state = app.state::<SwitcherState>();
    let s = settings(app);
    let kind = thumbs::kind(app.state::<Platform>().backend());
    let mut layout = Layout::parse(&opt_str(&s, NS, "layout", "previews"));
    if kind == Kind::None && layout == Layout::Previews {
        layout = Layout::Simple;
    }
    let mru = state.inner.lock().unwrap().mru.clone();
    let entries = collect(app, &s, same_app, &mru, kind);
    if entries.is_empty() {
        state.open.store(false, Ordering::SeqCst);
        return;
    }
    let monitor = cursor_monitor(app);
    let avail = monitor
        .as_ref()
        .map(|m| {
            let sc = m.scale_factor();
            (m.work_area().size.width as f64 / sc, m.work_area().size.height as f64 / sc)
        })
        .unwrap_or((1280.0, 720.0));
    let max_cols = opt_num(&s, NS, "maxColumns", 0.0).max(0.0) as usize;
    let size = opt_str(&s, NS, "thumbnailSize", "normal");
    let mut m = Model::new(layout, entries, 1);
    let grid = layout_grid(&m, &size, max_cols, (avail.0 * 0.92, avail.1 * 0.86));
    m.cols = grid.cols;
    m.select_initial(reverse);
    let generation = {
        let mut inner = state.inner.lock().unwrap();
        inner.model = Some(m);
        inner.grid = Some(grid);
        inner.kind = Some(kind);
        inner.shown = false;
        inner.generation += 1;
        emit_full(app, &inner, &s);
        inner.generation
    };
    if let (Some(window), Some(mon)) = (app.get_webview_window(WINDOW), monitor) {
        let sc = mon.scale_factor();
        let work = mon.work_area();
        let _ = window.set_size(LogicalSize::new(grid.width, grid.height));
        let (w, h) = (grid.width * sc, grid.height * sc);
        let x = work.position.x as f64 + (work.size.width as f64 - w) / 2.0;
        let y = work.position.y as f64 + (work.size.height as f64 - h) / 2.0;
        let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    }
    let delay = opt_num(&s, NS, "appearDelay", 0.0).max(0.0) as u64;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(delay.max(16)));
        send(&app, Msg::Show(generation));
    });
}

fn show(app: &AppHandle, generation: u64) {
    let state = app.state::<SwitcherState>();
    let mut inner = state.inner.lock().unwrap();
    if !state.open.load(Ordering::SeqCst) || inner.generation != generation || inner.shown {
        return;
    }
    inner.shown = true;
    drop(inner);
    if let Some(window) = app.get_webview_window(WINDOW) {
        #[cfg(windows)]
        os::show_inactive(&window);
        #[cfg(not(windows))]
        let _ = window.show();
    }
}

/// Hides the panel with its fade; `focus` raises the chosen window first.
fn close(app: &AppHandle, focus: Option<u64>) {
    let state = app.state::<SwitcherState>();
    state.open.store(false, Ordering::SeqCst);
    #[cfg(windows)]
    if let Ok(mut keys) = hook_win::KEYS.lock() {
        if let Some(ctx) = keys.as_mut() {
            ctx.hook.reset();
        }
    }
    let launch = focus.filter(|id| filter::is_idle(*id)).and_then(|id| state.inner.lock().unwrap().idle.get(&id).cloned());
    if let Some(id) = focus.filter(|id| !filter::is_idle(*id)) {
        let _ = app.state::<Platform>().windows().focus(WindowId(id));
    }
    if let Some(path) = launch {
        let _ = std::process::Command::new(path).spawn();
    }
    let (shown, generation) = {
        let mut inner = state.inner.lock().unwrap();
        if let Some(id) = focus.filter(|id| !filter::is_idle(*id)) {
            model::touch(&mut inner.mru, id);
        }
        inner.thumbs.clear();
        inner.model = None;
        let shown = inner.shown;
        inner.shown = false;
        (shown, inner.generation)
    };
    let s = settings(app);
    let fade = if opt_bool(&s, NS, "skipAnimations", false) { 80 } else { 300 };
    let _ = app.emit_to(WINDOW, HIDE_EVENT, fade);
    if !shown {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(fade + 20));
        let state = app.state::<SwitcherState>();
        let same = state.inner.lock().unwrap().generation == generation;
        if same && !state.open.load(Ordering::SeqCst) {
            if let Some(w) = app.get_webview_window(WINDOW) {
                let _ = w.hide();
            }
        }
    });
}

fn close_windows(app: &AppHandle, ids: &[u64], quit_pid: Option<u32>) {
    let platform = app.state::<Platform>();
    match quit_pid {
        Some(pid) => {
            let _ = platform.windows().quit_process(pid);
        }
        None => {
            for id in ids.iter().filter(|id| !filter::is_idle(**id)) {
                let _ = platform.windows().close(WindowId(*id));
            }
        }
    }
    let state = app.state::<SwitcherState>();
    let mut inner = state.inner.lock().unwrap();
    let Some(m) = inner.model.as_mut() else { return };
    m.remove(ids);
    if m.entries.is_empty() {
        drop(inner);
        close(app, None);
        return;
    }
    inner.thumbs.clear();
    emit_full(app, &inner, &settings(app));
}

fn handle_msg(app: &AppHandle, msg: Msg) {
    let state = app.state::<SwitcherState>();
    let with_model = |f: &mut dyn FnMut(&mut Model)| {
        let mut inner = state.inner.lock().unwrap();
        if let Some(m) = inner.model.as_mut() {
            f(m);
            emit_update(app, m);
        }
    };
    match msg {
        Msg::Key(Command::Open { reverse, same_app }) => open(app, reverse, same_app),
        Msg::Show(generation) => show(app, generation),
        Msg::Key(Command::Step(d)) => with_model(&mut |m| m.step(d)),
        Msg::Wheel(d) => with_model(&mut |m| m.step(d)),
        Msg::Key(Command::StepWindow(d)) => with_model(&mut |m| m.step_window(d)),
        Msg::Key(Command::Arrow(dx, dy)) => with_model(&mut |m| m.arrow(dx, dy)),
        Msg::Key(Command::Type(c)) => with_model(&mut |m| {
            let mut q = m.query.clone();
            q.push(c);
            m.set_query(q);
        }),
        Msg::Key(Command::Backspace) => with_model(&mut |m| {
            let mut q = m.query.clone();
            q.pop();
            m.set_query(q);
        }),
        Msg::Hover(id) => with_model(&mut |m| {
            m.select_id(id);
        }),
        Msg::Key(Command::Commit) => {
            let id = state.inner.lock().unwrap().model.as_ref().and_then(|m| m.current().map(|e| e.id));
            close(app, id);
        }
        Msg::Pick(id) => close(app, Some(id)),
        Msg::Key(Command::Cancel) => close(app, None),
        Msg::Key(Command::CloseWindow) => {
            let id = state.inner.lock().unwrap().model.as_ref().and_then(|m| m.current().map(|e| e.id));
            if let Some(id) = id {
                close_windows(app, &[id], None);
            }
        }
        Msg::Close(id) => close_windows(app, &[id], None),
        Msg::Key(Command::QuitApp) => {
            let target = state.inner.lock().unwrap().model.as_ref().and_then(|m| {
                let pid = m.current()?.pid?;
                Some((pid, m.entries.iter().filter(|e| e.pid == Some(pid)).map(|e| e.id).collect::<Vec<_>>()))
            });
            if let Some((pid, ids)) = target {
                close_windows(app, &ids, Some(pid));
            }
        }
    }
}

// ---- Commands from the switcher WebView ----

/// The WebView reports where each preview slot is; live thumbnails are placed there.
#[tauri::command]
pub fn switcher_slots(app: AppHandle, slots: Vec<Slot>) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    let state = app.state::<SwitcherState>();
    if !state.open.load(Ordering::SeqCst) {
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    let host = os::host_id(&window);
    let due = state.inner.lock().unwrap().thumbs.update(host, scale, &slots);
    send_snapshots(&app, WINDOW, IMAGE_EVENT, &slots, &due, scale);
}

pub(crate) fn send_snapshots(app: &AppHandle, label: &'static str, event: &'static str, slots: &[Slot], due: &[u64], scale: f64) {
    if due.is_empty() {
        return;
    }
    let jobs: Vec<(u64, u32, u32)> = slots
        .iter()
        .filter(|s| due.contains(&s.id))
        .map(|s| (s.id, (s.w * scale) as u32, (s.h * scale) as u32))
        .collect();
    let app = app.clone();
    std::thread::spawn(move || {
        for (id, w, h) in jobs {
            if let Some(url) = thumbs::snapshot_url(id, w.max(16), h.max(16)) {
                let _ = app.emit_to(label, event, serde_json::json!({ "id": id, "url": url }));
            }
        }
    });
}

#[tauri::command]
pub fn switcher_pick(app: AppHandle, id: u64) {
    send(&app, Msg::Pick(id));
}

#[tauri::command]
pub fn switcher_hover(app: AppHandle, id: u64) {
    send(&app, Msg::Hover(id));
}

#[tauri::command]
pub fn switcher_close_window(app: AppHandle, id: u64) {
    send(&app, Msg::Close(id));
}

#[tauri::command]
pub fn switcher_step(app: AppHandle, delta: isize) {
    send(&app, Msg::Wheel(delta.signum()));
}

#[tauri::command]
pub fn switcher_cancel(app: AppHandle) {
    send(&app, Msg::Key(Command::Cancel));
}

// ---- OS glue ----

mod os {
    /// The id the platform layer uses for one of our windows (HWND on Windows, XID on X11).
    pub fn host_id(window: &tauri::WebviewWindow) -> u64 {
        #[cfg(windows)]
        {
            window.hwnd().map(|h| h.0 as usize as u64).unwrap_or(0)
        }
        #[cfg(not(windows))]
        {
            let _ = window;
            0
        }
    }

    #[cfg(windows)]
    use windows::Win32::Foundation::HWND;
    #[cfg(windows)]
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_TOPMOST, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    #[cfg(windows)]
    fn hwnd(window: &tauri::WebviewWindow) -> HWND {
        HWND(host_id(window) as usize as *mut std::ffi::c_void)
    }

    /// Clicks on the window never activate it, and it never appears in Alt+Tab itself.
    #[cfg(windows)]
    pub fn prepare(window: &tauri::WebviewWindow) {
        let h = hwnd(window);
        unsafe {
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize);
            // A changed style only takes effect once the frame is recalculated; without it Windows draws a title bar on the window.
            let _ = SetWindowPos(h, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        }
    }

    #[cfg(windows)]
    pub fn show_inactive(window: &tauri::WebviewWindow) {
        let h = hwnd(window);
        unsafe {
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    /// Hides a window `show_inactive` showed. Tao never saw that show, so its own `hide` thinks the window is already hidden and does nothing.
    #[cfg(windows)]
    pub fn hide_shown(window: &tauri::WebviewWindow) {
        let _ = window.hide();
        unsafe {
            let _ = ShowWindow(hwnd(window), SW_HIDE);
        }
    }
}

pub(crate) use os::host_id;
#[cfg(windows)]
pub(crate) use os::{hide_shown, prepare as prepare_window, show_inactive};

#[cfg(target_os = "linux")]
mod linux {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::Sender;
    use std::sync::{Arc, Mutex};

    use notifi_platform::x11_switcher::{self, GrabbedKey, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_SUPER};

    use super::keys::{Accel, Bindings, Command, Hook, Key, Mods};
    use super::Msg;

    fn keysym(key: Key) -> Option<u32> {
        Some(match key {
            Key::Tab => 0xff09,
            Key::Backquote => 0x60,
            Key::Space => 0x20,
            Key::Escape => 0xff1b,
            Key::Enter => 0xff0d,
            Key::Backspace => 0xff08,
            Key::Delete => 0xffff,
            Key::Left => 0xff51,
            Key::Up => 0xff52,
            Key::Right => 0xff53,
            Key::Down => 0xff54,
            Key::Char(c) if c.is_ascii() => c.to_ascii_lowercase() as u32,
            Key::F(n) if (1..=24).contains(&n) => 0xffbd + n as u32,
            _ => return None,
        })
    }

    fn key_of(sym: u32) -> Key {
        match sym {
            0xff09 | 0xfe20 => Key::Tab,
            0x60 => Key::Backquote,
            0x20 => Key::Space,
            0xff1b => Key::Escape,
            0xff0d | 0xff8d => Key::Enter,
            0xff08 => Key::Backspace,
            0xffff => Key::Delete,
            0xff51 => Key::Left,
            0xff52 => Key::Up,
            0xff53 => Key::Right,
            0xff54 => Key::Down,
            0xffbe..=0xffd5 => Key::F((sym - 0xffbd) as u8),
            0x21..=0x7e => Key::Char((sym as u8 as char).to_ascii_lowercase()),
            _ => Key::Other,
        }
    }

    fn is_modifier(sym: u32) -> bool {
        (0xffe1..=0xffee).contains(&sym)
    }

    fn mask(m: Mods) -> u16 {
        (if m.shift { MOD_SHIFT } else { 0 })
            | (if m.ctrl { MOD_CONTROL } else { 0 })
            | (if m.alt { MOD_ALT } else { 0 })
            | (if m.meta { MOD_SUPER } else { 0 })
    }

    fn mods(mask: u16) -> Mods {
        Mods { shift: mask & MOD_SHIFT != 0, ctrl: mask & MOD_CONTROL != 0, alt: mask & MOD_ALT != 0, meta: mask & MOD_SUPER != 0 }
    }

    pub fn grab(open: Arc<AtomicBool>, tx: Option<Sender<Msg>>, b: Bindings) -> Option<notifi_platform::Guard> {
        let tx = tx?;
        let combos: Vec<(u16, u32)> = [b.switcher, b.same_app]
            .into_iter()
            .flatten()
            .filter_map(|a: Accel| Some((mask(a.mods), keysym(a.key)?)))
            .flat_map(|(m, k)| [(m, k), (m | MOD_SHIFT, k)])
            .collect();
        let hook = Mutex::new(Hook::default());
        let flag = open.clone();
        x11_switcher::grab_keys(combos, open, move |k: GrabbedKey| {
            let is_open = flag.load(Ordering::SeqCst);
            let mut hook = hook.lock().unwrap();
            let cmd = if is_modifier(k.keysym) {
                if k.pressed { None } else { hook.modifiers(is_open, mods(k.mods)) }
            } else {
                hook.key(&b, is_open, key_of(k.keysym), k.pressed, mods(k.mods)).1
            };
            if let Some(cmd) = cmd {
                match cmd {
                    Command::Open { .. } => flag.store(true, Ordering::SeqCst),
                    Command::Commit | Command::Cancel => flag.store(false, Ordering::SeqCst),
                    _ => {}
                }
                let _ = tx.send(Msg::Key(cmd));
            }
        })
        .ok()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn keysyms_round_trip() {
            for k in [Key::Tab, Key::Backquote, Key::Escape, Key::Char('w'), Key::F(4)] {
                assert_eq!(key_of(keysym(k).unwrap()), k);
            }
            assert_eq!(mods(mask(Mods { alt: true, shift: true, ..Default::default() })), Mods { alt: true, shift: true, ..Default::default() });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(id: u64, app: &str, x: i32, minimized: bool) -> WindowInfo {
        WindowInfo {
            id: WindowId(id),
            title: "t".into(),
            app_id: Some(app.into()),
            pid: None,
            bounds: notifi_platform::Rect { x, y: 0, width: 800, height: 600 },
            monitor: None,
            minimized,
            focused: false,
        }
    }

    #[test]
    fn merging_tabs_keeps_the_front_window_of_each_stack() {
        let list = vec![win(1, "a.exe", 0, false), win(2, "a.exe", 0, false), win(3, "a.exe", 50, false), win(4, "b.exe", 0, false), win(5, "a.exe", 0, true), win(6, "a.exe", 0, true)];
        let ids: Vec<u64> = merge_tabs(list, &HashSet::new()).iter().map(|w| w.id.0).collect();
        assert_eq!(ids, vec![1, 3, 4, 5, 6]);
    }

    #[test]
    fn windows_on_other_desktops_are_not_merged() {
        let list = vec![win(1, "a.exe", 0, false), win(2, "a.exe", 0, false), win(3, "a.exe", 0, false)];
        let other: HashSet<u64> = [2].into_iter().collect();
        let ids: Vec<u64> = merge_tabs(list, &other).iter().map(|w| w.id.0).collect();
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn app_names_and_lists() {
        let w = WindowInfo {
            id: WindowId(1),
            title: "t".into(),
            app_id: Some("notepad.exe".into()),
            pid: None,
            bounds: Default::default(),
            monitor: None,
            minimized: false,
            focused: false,
        };
        assert_eq!(app_name(&w), "Notepad");
        assert!(listed(&["notepad".into()], "Notepad.exe"));
        assert!(listed(&["notepad.exe".into()], "notepad"));
        assert!(!listed(&["".into()], "x"));
    }
}
