//! The Command Bar (spec/everyday/command-bar.md): one field that searches apps, windows,
//! files, clipboard history, a calculator, units, emoji and user scripts.

mod apps;
mod calc;
mod colors;
mod emoji_data;
mod index;
mod rank;
mod search;

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use notifi_platform::{Platform, WindowId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::{
    settings::{Settings, Store},
    window,
};
use apps::{App, Launch};
use index::FileEntry;
use search::{ClipRow, Input, Options, Row, SnippetRow, SystemRow, Target, WindowRow};

pub const WINDOW: &str = "commandbar";
pub const ACTION: &str = "commandBar";
/// Prefixes of the keybind actions made from the script and app shortcut lists; the rest is the name.
const SCRIPT_PREFIX: &str = "commandBar.script:";
const APP_PREFIX: &str = "commandBar.app:";
const MAX_APP_SHORTCUTS: usize = 64;
const OPEN_EVENT: &str = "commandbar://open";
const WIDTH: f64 = 720.0;
const START_HEIGHT: f64 = 96.0;
const MAX_HEIGHT: f64 = 600.0;
const APPS_STALE: Duration = Duration::from_secs(300);
const FILES_STALE: Duration = Duration::from_secs(600);

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    usage: HashMap<String, u32>,
    position: Option<(i32, i32)>,
}

#[derive(Default)]
pub struct CommandBarState {
    apps: Mutex<(Vec<App>, Option<Instant>)>,
    files: Mutex<(Vec<FileEntry>, Option<Instant>)>,
    scanning_apps: AtomicBool,
    scanning_files: AtomicBool,
    targets: Mutex<HashMap<String, Target>>,
    previous: Mutex<Option<WindowId>>,
    saved: Mutex<Saved>,
    learned: Mutex<HashMap<String, String>>,
    path: Mutex<Option<PathBuf>>,
}

impl CommandBarState {
    fn persist(&self) {
        let Some(path) = self.path.lock().unwrap().clone() else { return };
        if let Ok(text) = serde_json::to_string(&*self.saved.lock().unwrap()) {
            let _ = fs::write(path, text);
        }
    }
}

fn settings(app: &AppHandle) -> Settings {
    app.state::<Store>().get()
}

fn opt<'a>(s: &'a Settings, key: &str) -> Option<&'a Value> {
    s.option("commandBar", key)
}

fn opt_bool(s: &Settings, key: &str, default: bool) -> bool {
    opt(s, key).and_then(Value::as_bool).unwrap_or(default)
}

fn opt_str<'a>(s: &'a Settings, key: &str, default: &'a str) -> &'a str {
    opt(s, key).and_then(Value::as_str).unwrap_or(default)
}

fn opt_list(s: &Settings, key: &str) -> Vec<String> {
    opt(s, key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default()
}

fn options(s: &Settings) -> Options {
    Options {
        apps: opt_bool(s, "searchApps", true),
        windows: opt_bool(s, "searchWindows", true),
        files: opt_bool(s, "searchFiles", true),
        clipboard: opt_bool(s, "searchClipboard", true),
        emoji: opt_bool(s, "searchEmoji", true),
        calculator: opt_bool(s, "calculator", true),
        colors: opt_bool(s, "colors", true),
        snippets: opt_bool(s, "searchSnippets", true),
        system: opt_bool(s, "searchSystem", true),
        scripts: !opt_str(s, "scriptsFolder", "").is_empty(),
        aliases: search::parse_pairs(&opt_list(s, "aliases")),
        pins: opt_list(s, "pins"),
        hidden: opt_list(s, "hidden"),
        bare_scripts: opt_list(s, "scriptBareNames"),
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let state = CommandBarState::default();
    if let Ok(dir) = app.path().app_config_dir() {
        let path = dir.join("commandbar.json");
        if let Some(saved) = fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Saved>(&t).ok()) {
            *state.saved.lock().unwrap() = saved;
        }
        *state.path.lock().unwrap() = Some(path);
    }
    app.manage(state);

    let window = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("commandbar.html".into()))
        .title("Notifi Command Bar")
        .inner_size(WIDTH, START_HEIGHT)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .visible(false)
        .focused(false)
        .build()?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Focused(false)) && !window::no_focus() {
            hide(&handle, false);
        }
    });
    refresh_apps(app);
    Ok(())
}

fn refresh_apps(app: &AppHandle) {
    let state = app.state::<CommandBarState>();
    if state.scanning_apps.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    thread::spawn(move || {
        let found = apps::scan();
        let state = app.state::<CommandBarState>();
        *state.apps.lock().unwrap() = (found, Some(Instant::now()));
        state.scanning_apps.store(false, Ordering::SeqCst);
    });
}

fn refresh_files(app: &AppHandle) {
    let state = app.state::<CommandBarState>();
    if state.scanning_files.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut roots: Vec<PathBuf> = opt_list(&settings(app), "fileFolders").into_iter().map(PathBuf::from).filter(|p| p.is_dir()).collect();
    if roots.is_empty() {
        roots = index::default_folders();
    }
    let app = app.clone();
    thread::spawn(move || {
        let found = index::scan_files(&roots);
        let state = app.state::<CommandBarState>();
        *state.files.lock().unwrap() = (found, Some(Instant::now()));
        state.scanning_files.store(false, Ordering::SeqCst);
    });
}

fn stale<T>(slot: &Mutex<(Vec<T>, Option<Instant>)>, limit: Duration) -> bool {
    slot.lock().unwrap().1.is_none_or(|t| t.elapsed() > limit)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OpenPayload {
    compact: bool,
    no_suggestions: bool,
    query: String,
}

/// Opens the bar, or closes it when it is already showing (the shortcut toggles).
pub fn toggle(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    if window.is_visible().unwrap_or(false) {
        hide(app, true);
    } else {
        open(app, None);
    }
}

/// `--commandbar` or `--commandbar=<query>` on the command line opens the bar once the page has loaded.
pub fn open_from_args(app: &AppHandle) {
    let Some(query) = std::env::args().find_map(|a| a.strip_prefix("--commandbar").map(|r| r.strip_prefix('=').unwrap_or("").to_string())) else { return };
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(2500));
        open(&app, Some(query).filter(|q| !q.is_empty()));
    });
}

/// Opens the bar, optionally with a query already typed (`--commandbar=<query>`).
pub fn open(app: &AppHandle, query: Option<String>) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    let s = settings(app);
    let state = app.state::<CommandBarState>();
    *state.previous.lock().unwrap() = app.state::<Platform>().windows().foreground().ok().flatten().map(|w| w.id);
    if stale(&state.apps, APPS_STALE) {
        refresh_apps(app);
    }
    if opt_bool(&s, "searchFiles", true) && stale(&state.files, FILES_STALE) {
        refresh_files(app);
    }
    state.learned.lock().unwrap().clear();
    place(app, &s, false);
    let _ = window.show();
    if !window::no_focus() {
        let _ = window.set_focus();
    }
    let _ = app.emit_to(
        WINDOW,
        OPEN_EVENT,
        OpenPayload { compact: opt_bool(&s, "compact", false), no_suggestions: opt_bool(&s, "noSuggestions", false), query: query.unwrap_or_default() },
    );
}

fn hide(app: &AppHandle, refocus: bool) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let refocus = refocus && window.is_focused().unwrap_or(false);
    let _ = window.hide();
    let previous = *app.state::<CommandBarState>().previous.lock().unwrap();
    if let (true, Some(id)) = (refocus, previous) {
        let _ = app.state::<Platform>().windows().focus(id);
    }
}

/// Puts the bar on the display under the pointer; `recentre` ignores a remembered spot.
fn place(app: &AppHandle, s: &Settings, recentre: bool) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    let (px, py) = app.cursor_position().map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
    let Some(monitor) = app.monitor_from_point(px, py).ok().flatten().or_else(|| app.primary_monitor().ok().flatten()) else { return };
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let (wx, wy, ww, wh) = (work.position.x as f64, work.position.y as f64, work.size.width as f64, work.size.height as f64);
    let width = (WIDTH * scale).min(ww);
    let mode = opt_str(s, "position", "center");
    let saved = app.state::<CommandBarState>().saved.lock().unwrap().position;
    let (x, y) = match (mode, saved) {
        ("remember", Some(p)) if !recentre => (p.0 as f64, p.1 as f64),
        ("edge", _) if !recentre => (wx + (ww - width) / 2.0, wy + 8.0 * scale),
        _ => (wx + (ww - width) / 2.0, wy + wh * 0.18),
    };
    let (x, y) = match under_pill(app, s) {
        Some(bottom) => (wx + (ww - width) / 2.0, wy + (bottom + 8.0) * scale),
        None => (x, y),
    };
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// With "Command Bar comes out of the Island", the bar opens just under the resting pill.
fn under_pill(app: &AppHandle, s: &Settings) -> Option<f64> {
    let on = s.enabled("companion") && s.option("companion", "commandBarInPill").and_then(Value::as_bool).unwrap_or(false);
    on.then(|| crate::island::pill_bottom(app)).flatten()
}

// ---- Commands ----

#[derive(Serialize)]
pub struct Results {
    rows: Vec<Row>,
}

#[tauri::command]
pub async fn commandbar_search(app: AppHandle, query: String) -> Results {
    let s = settings(&app);
    let opts = options(&s);
    let state = app.state::<CommandBarState>();

    let windows: Vec<WindowRow> = if opts.windows && !query.trim().is_empty() {
        let own = std::process::id();
        app.state::<Platform>()
            .windows()
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|w| w.pid != Some(own) && !w.title.trim().is_empty())
            .map(|w| WindowRow { id: w.id.0, title: w.title, app: w.app_id.unwrap_or_default() })
            .collect()
    } else {
        Vec::new()
    };
    let clips: Vec<ClipRow> = if opts.clipboard && query.trim().chars().count() >= 2 { clipboard_rows(&app) } else { Vec::new() };
    let scripts = match opt_str(&s, "scriptsFolder", "") {
        "" => Vec::new(),
        folder => index::list_scripts(&PathBuf::from(folder)),
    };

    let snippets: Vec<SnippetRow> = if opts.snippets && !query.trim().is_empty() {
        crate::clipboard::snippets_get(app.clone())
            .snippets
            .into_iter()
            .map(|s| SnippetRow { id: s.id, name: s.name, trigger: s.trigger })
            .collect()
    } else {
        Vec::new()
    };
    let system = if opts.system && query.trim().chars().count() >= 2 { system_rows() } else { Vec::new() };

    let usage = state.saved.lock().unwrap().usage.clone();
    let learned = state.learned.lock().unwrap().get(&query.trim().to_lowercase()).cloned();
    let apps = state.apps.lock().unwrap().0.clone();
    let files = if opts.files { state.files.lock().unwrap().0.clone() } else { Vec::new() };
    let found = search::run(
        &query,
        &Input { apps: &apps, windows: &windows, files: &files, clips: &clips, scripts: &scripts, snippets: &snippets, system: &system, usage: &usage, learned: learned.as_deref(), options: &opts },
    );
    let mut targets = state.targets.lock().unwrap();
    targets.clear();
    let rows = found
        .into_iter()
        .map(|(row, target)| {
            targets.insert(row.id.clone(), target);
            row
        })
        .collect();
    Results { rows }
}

/// Notifi's own actions and settings pages, listed under "System actions and features".
fn system_rows() -> Vec<SystemRow> {
    let actions = crate::keybinds::ACTIONS.iter().map(|a| SystemRow {
        id: format!("system:{}", a.id),
        title: a.label.to_string(),
        subtitle: "Notifi action".into(),
        target: Target::Action(a.id.to_string()),
    });
    let pages = crate::catalog::pages().map(|p| SystemRow {
        id: format!("page:{}", p.id),
        title: format!("{} settings", p.label),
        subtitle: "Open in Notifi Settings".into(),
        target: Target::Page(p.id.to_string()),
    });
    actions.chain(pages).collect()
}

/// Clipboard history rows, read through the clipboard module's public list command.
fn clipboard_rows(app: &AppHandle) -> Vec<ClipRow> {
    let Ok(Value::Object(list)) = serde_json::to_value(crate::clipboard::clipboard_list(app.clone())) else { return Vec::new() };
    list.get("entries")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter(|e| e.get("kind").and_then(Value::as_str) != Some("image"))
                .filter_map(|e| Some(ClipRow { id: e.get("id")?.as_u64()?, text: e.get("text")?.as_str()?.to_string() }))
                .filter(|c| !c.text.trim().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// Text the page should put on the clipboard (calculator, emoji, script output).
    copy: Option<String>,
    /// New text for the field instead of running (a script name waiting for arguments).
    complete: Option<String>,
    /// The bar already closed.
    closed: bool,
}

fn remember(app: &AppHandle, query: &str, id: &str) {
    let state = app.state::<CommandBarState>();
    {
        let mut saved = state.saved.lock().unwrap();
        let count = saved.usage.entry(id.to_string()).or_insert(0);
        *count = count.saturating_add(1);
    }
    if !query.trim().is_empty() && (id.starts_with("app:") || id.starts_with("emoji:")) {
        state.learned.lock().unwrap().insert(query.trim().to_lowercase(), id.to_string());
    }
    state.persist();
}

#[tauri::command]
pub async fn commandbar_run(app: AppHandle, id: String, query: String, alt: bool) -> Result<Outcome, String> {
    let target = app.state::<CommandBarState>().targets.lock().unwrap().get(&id).cloned().ok_or("That result is no longer available.")?;
    remember(&app, &query, &id);
    match target {
        Target::App(launch) => {
            hide(&app, false);
            match launch {
                Launch::Shell(path) => index::open_path(&path)?,
                Launch::Appx(package) => index::open_appx(&package)?,
                Launch::Exec(exec) => index::spawn_exec(&exec)?,
            }
            Ok(Outcome { closed: true, ..Default::default() })
        }
        Target::Window(raw) => {
            hide(&app, false);
            let platform = app.state::<Platform>();
            let result = if alt { platform.windows().close(WindowId(raw)) } else { platform.windows().focus(WindowId(raw)) };
            result.map_err(|e| e.to_string())?;
            Ok(Outcome { closed: true, ..Default::default() })
        }
        Target::File(path) => {
            hide(&app, false);
            if alt { index::reveal(&path)? } else { index::open_path(&path)? }
            Ok(Outcome { closed: true, ..Default::default() })
        }
        Target::Clip(entry) => {
            if alt {
                crate::clipboard::clipboard_copy(app.clone(), vec![entry]).await?;
            } else {
                hide(&app, true);
                pause(120).await;
                crate::clipboard::clipboard_paste(app.clone(), vec![entry], false).await?;
            }
            hide(&app, true);
            Ok(Outcome { closed: true, ..Default::default() })
        }
        Target::Snippet(snippet) => {
            if alt {
                let text = crate::clipboard::snippets_get(app.clone()).snippets.into_iter().find(|s| s.id == snippet).map(|s| s.text).ok_or("That snippet no longer exists.")?;
                let expanded = crate::clipboard::snippet_preview(text, HashMap::new()).await;
                return Ok(Outcome { copy: Some(expanded), ..Default::default() });
            }
            hide(&app, true);
            pause(120).await;
            crate::clipboard::snippet_insert(app.clone(), snippet, HashMap::new()).await?;
            Ok(Outcome { closed: true, ..Default::default() })
        }
        Target::Action(action) => {
            hide(&app, false);
            let handle = app.clone();
            thread::spawn(move || crate::keybinds::run(&handle, &action));
            Ok(Outcome { closed: true, ..Default::default() })
        }
        Target::Page(page) => {
            hide(&app, false);
            window::open_settings_at(&app, Some(page.as_str()));
            Ok(Outcome { closed: true, ..Default::default() })
        }
        Target::Copy(text) => Ok(Outcome { copy: Some(text), ..Default::default() }),
        Target::Script { complete: Some(text), .. } => Ok(Outcome { complete: Some(text), ..Default::default() }),
        Target::Script { path, args, .. } => {
            let out = tauri::async_runtime::spawn_blocking(move || index::run_script(&path, &args)).await.map_err(|e| e.to_string())??;
            Ok(Outcome { copy: (!out.is_empty()).then_some(out), ..Default::default() })
        }
    }
}

// ---- Script and app shortcuts ----

/// (accelerator, keybind action id, key shown when it cannot be registered) for every line of the
/// script and app shortcut lists; the keybinds module registers them with the other shortcuts.
pub fn shortcut_bindings(s: &Settings) -> Vec<(String, String, String)> {
    if !s.enabled("commandBar") {
        return Vec::new();
    }
    bindings(&opt_list(s, "scriptShortcuts"), &opt_list(s, "appShortcuts"))
}

fn bindings(scripts: &[String], apps: &[String]) -> Vec<(String, String, String)> {
    let script = search::parse_pairs(scripts)
        .into_iter()
        .map(|(name, accel)| (accel, format!("{SCRIPT_PREFIX}{name}"), format!("commandBar.scriptShortcuts:{name}")));
    let app = search::parse_pairs(apps)
        .into_iter()
        .take(MAX_APP_SHORTCUTS)
        .map(|(name, accel)| (accel, format!("{APP_PREFIX}{name}"), format!("commandBar.appShortcuts:{name}")));
    script.chain(app).collect()
}

/// Whether the keybind action belongs to this module.
pub fn is_shortcut_action(action: &str) -> bool {
    action.starts_with(SCRIPT_PREFIX) || action.starts_with(APP_PREFIX)
}

/// Runs a script or app shortcut: a script runs and its output is copied; an app is opened or, when
/// it is in front, hidden.
pub fn run_shortcut(app: &AppHandle, action: &str) {
    let (app, action) = (app.clone(), action.to_string());
    thread::spawn(move || {
        if let Some(name) = action.strip_prefix(SCRIPT_PREFIX) {
            run_script_shortcut(&app, name);
        } else if let Some(name) = action.strip_prefix(APP_PREFIX) {
            run_app_shortcut(&app, name);
        }
    });
}

fn run_script_shortcut(app: &AppHandle, name: &str) {
    let folder = opt_str(&settings(app), "scriptsFolder", "").to_string();
    if folder.is_empty() {
        return;
    }
    let Some(script) = index::list_scripts(&PathBuf::from(folder)).into_iter().find(|s| s.name.eq_ignore_ascii_case(name)) else { return };
    match index::run_script(&script.path, &[]) {
        Ok(out) if !out.is_empty() => {
            let _ = crate::clipboard::set_text(&out);
        }
        Ok(_) => {}
        Err(message) => crate::monitor::notify::send(format!("{name} failed"), message, false),
    }
}

/// Whether a window's executable name (or app id) is the app called `app_name`.
fn belongs(app_name: &str, exe: Option<&str>) -> bool {
    let Some(exe) = exe else { return false };
    let norm = |s: &str| {
        let lower = s.to_ascii_lowercase();
        lower.trim_end_matches(".exe").chars().filter(|c| c.is_alphanumeric()).collect::<String>()
    };
    let (a, e) = (norm(app_name), norm(exe));
    a.len() >= 3 && e.len() >= 3 && (a == e || a.contains(&e) || e.contains(&a))
}

fn launch_app(launch: &Launch) -> Result<(), String> {
    match launch {
        Launch::Shell(path) => index::open_path(path),
        Launch::Appx(package) => index::open_appx(package),
        Launch::Exec(exec) => index::spawn_exec(exec),
    }
}

fn run_app_shortcut(app: &AppHandle, name: &str) {
    let platform = app.state::<Platform>();
    let windows = platform.windows().list().unwrap_or_default();
    let mine: Vec<_> = windows.iter().filter(|w| belongs(name, w.app_id.as_deref())).collect();
    if mine.iter().any(|w| w.focused) {
        for w in &mine {
            let _ = platform.windows().minimize(w.id);
        }
        return;
    }
    if let Some(w) = mine.first() {
        let _ = platform.windows().focus(w.id);
        return;
    }
    let state = app.state::<CommandBarState>();
    if state.apps.lock().unwrap().0.is_empty() {
        let found = apps::scan();
        *state.apps.lock().unwrap() = (found, Some(Instant::now()));
    }
    let launch = state.apps.lock().unwrap().0.iter().find(|a| a.name.eq_ignore_ascii_case(name)).map(|a| a.launch.clone());
    if let Some(launch) = launch {
        let _ = launch_app(&launch);
    }
}

async fn pause(ms: u64) {
    let _ = tauri::async_runtime::spawn_blocking(move || thread::sleep(Duration::from_millis(ms))).await;
}

/// Closes the bar and gives focus back to the app that had it.
#[tauri::command]
pub fn commandbar_hide(app: AppHandle, refocus: bool) {
    hide(&app, refocus);
}

/// Sets the window height (logical pixels) so it fits the field and the rows; the top stays put.
#[tauri::command]
pub fn commandbar_resize(app: AppHandle, height: f64) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.set_size(LogicalSize::new(WIDTH, height.clamp(START_HEIGHT, MAX_HEIGHT)));
    }
}

/// The user finished dragging the bar by its handle.
#[tauri::command]
pub fn commandbar_dragged(app: AppHandle, state: State<CommandBarState>) {
    if let Some(pos) = app.get_webview_window(WINDOW).and_then(|w| w.outer_position().ok()) {
        state.saved.lock().unwrap().position = Some((pos.x, pos.y));
        state.persist();
    }
}

/// Double-clicking the handle: forget the remembered spot and centre the bar.
#[tauri::command]
pub fn commandbar_recenter(app: AppHandle, state: State<CommandBarState>) {
    state.saved.lock().unwrap().position = None;
    state.persist();
    place(&app, &settings(&app), true);
}

/// Clears the usage ranking and the per-query choices (the "forget learned ranking" button).
#[tauri::command]
pub fn commandbar_forget(state: State<CommandBarState>) {
    state.saved.lock().unwrap().usage.clear();
    state.learned.lock().unwrap().clear();
    state.persist();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn shortcut_lines_become_bindings() {
        let b = bindings(&lines(&["backup = Ctrl+Alt+B", "no equals sign"]), &lines(&["Firefox = Ctrl+Alt+F"]));
        assert_eq!(b.len(), 2);
        assert_eq!(b[0], ("Ctrl+Alt+B".into(), "commandBar.script:backup".into(), "commandBar.scriptShortcuts:backup".into()));
        assert_eq!(b[1].1, "commandBar.app:Firefox");
        assert!(is_shortcut_action(&b[0].1) && is_shortcut_action(&b[1].1) && !is_shortcut_action(ACTION));
    }

    #[test]
    fn app_shortcuts_stop_at_the_limit() {
        let many: Vec<String> = (0..80).map(|i| format!("App{i} = Ctrl+Alt+F{}", i % 12 + 1)).collect();
        assert_eq!(bindings(&[], &many).len(), MAX_APP_SHORTCUTS);
    }

    #[test]
    fn bindings_need_the_feature_on() {
        let mut s = Settings::default();
        let ns = s.features.entry("commandBar".into()).or_default();
        ns.insert("appShortcuts".into(), serde_json::json!(["Firefox = Ctrl+Alt+F"]));
        assert!(shortcut_bindings(&s).is_empty());
        s.features.get_mut("commandBar").unwrap().insert("enabled".into(), true.into());
        assert_eq!(shortcut_bindings(&s).len(), 1);
    }

    #[test]
    fn windows_are_matched_to_apps_by_name() {
        assert!(belongs("Firefox", Some("firefox.exe")));
        assert!(belongs("Visual Studio Code", Some("Code.exe")));
        assert!(belongs("Google Chrome", Some("chrome.exe")));
        assert!(belongs("Notepad++", Some("notepad++.exe")));
        assert!(!belongs("Firefox", None));
        assert!(!belongs("Firefox", Some("explorer.exe")));
        assert!(!belongs("ab", Some("ab.exe")), "names under three characters are too loose to match");
    }
}
