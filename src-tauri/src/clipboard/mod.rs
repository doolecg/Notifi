//! Clipboard features (spec/clipboard/*.md): history with its popup, auto clear, paste as plain
//! text, Clean URL, paste image as file, the Shelf, and text snippets with their menu and typed
//! triggers (spec/input/text-snippets.md).

mod cleanurl;
mod history;
pub mod shelf;
mod snippets;
mod typing;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as sys;
#[cfg(windows)]
mod win;
#[cfg(windows)]
use win as sys;

use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::{
    keybinds,
    settings::{Settings, Store},
    window,
};
use history::{Capture, Entry, History, Image, Inserted, Kind};
use snippets::{Snippet, SnippetFile};

pub const HISTORY_WINDOW: &str = "clipboard";
pub const SNIPPETS_WINDOW: &str = "snippets";
const CHANGED_EVENT: &str = "clipboard://changed";
const OPEN_EVENT: &str = "clipboard://open";
const CLEANED_EVENT: &str = "clipboard://cleaned";
const SNIPPETS_OPEN_EVENT: &str = "snippets://open";
const SNIPPETS_CHANGED_EVENT: &str = "snippets://changed";

/// Actions the keybind registry runs here (see keybinds::FEATURE_SHORTCUTS).
pub const ACTIONS: &[&str] = &["clipboardHistory", "plainTextPaste", "snippetMenu", "cleanUrl", "pasteImageAsFile", "shelfToggle"];

/// Longest preview text sent to the popup per entry.
const PREVIEW_CHARS: usize = 4000;
const THUMB: u32 = 320;
/// How long to wait after the paste keystroke before putting the original clipboard back.
const RESTORE_AFTER: Duration = Duration::from_millis(450);
const READ_TIMEOUT: Duration = Duration::from_secs(5);

pub enum Event {
    Changed,
    Lock,
    Sleep,
}

/// Content to put on the clipboard.
#[derive(Default, Clone, Debug)]
pub struct Payload {
    pub text: Option<String>,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub files: Vec<String>,
    pub png: Option<Vec<u8>>,
}

impl Payload {
    fn text(text: impl Into<String>) -> Self {
        Self { text: Some(text.into()), ..Default::default() }
    }
}

/// Puts plain text on the system clipboard (the history picks it up like any other copy).
pub fn set_text(text: &str) -> Result<(), String> {
    sys::write(&Payload::text(text)).map(|_| ())
}

/// The plain text on the system clipboard, if there is any.
pub fn read_text() -> Option<String> {
    sys::read_text()
}

/// Puts a PNG image on the system clipboard.
pub fn set_png(png: &[u8]) -> Result<(), String> {
    sys::write(&Payload { png: Some(png.to_vec()), ..Default::default() }).map(|_| ())
}

pub struct ClipState {
    dir: PathBuf,
    history: Mutex<History>,
    snippets: Mutex<SnippetFile>,
    dirty: AtomicBool,
    /// Sequence numbers of our own writes, which the watcher skips.
    own: Mutex<VecDeque<u32>>,
    /// When auto clear fires, and the copy it may clear.
    clear_at: Mutex<Option<(Instant, u32)>>,
    /// The window that had focus before a popup opened.
    previous: Mutex<Option<isize>>,
    busy: AtomicBool,
    /// Zone offsets the snippet window last worked out, for triggers typed outside it.
    zones: Mutex<HashMap<String, i32>>,
}

impl ClipState {
    fn history_path(&self) -> PathBuf {
        self.dir.join("history.json")
    }

    fn snippets_path(&self) -> PathBuf {
        self.dir.join("snippets.json")
    }

    fn image_path(&self, stem: &str, thumb: bool) -> PathBuf {
        self.dir.join("images").join(if thumb { format!("{stem}.thumb.png") } else { format!("{stem}.png") })
    }

    fn mark_own(&self, seq: u32) {
        let mut own = self.own.lock().unwrap();
        own.push_back(seq);
        while own.len() > 16 {
            own.pop_front();
        }
    }

    fn is_own(&self, seq: u32) -> bool {
        let mut own = self.own.lock().unwrap();
        match own.iter().position(|s| *s == seq) {
            Some(i) => {
                own.remove(i);
                true
            }
            None => false,
        }
    }

    fn forget_files(&self, removed: &[Entry]) {
        for e in removed {
            if let Some(stem) = &e.image {
                let _ = fs::remove_file(self.image_path(stem, false));
                let _ = fs::remove_file(self.image_path(stem, true));
            }
        }
    }

    fn save_history(&self) {
        if !self.dirty.swap(false, Ordering::SeqCst) {
            return;
        }
        let text = match serde_json::to_string(&*self.history.lock().unwrap()) {
            Ok(t) => t,
            Err(_) => return,
        };
        let path = self.history_path();
        let tmp = path.with_extension("json.tmp");
        if fs::create_dir_all(&self.dir).is_err() || fs::write(&tmp, text).is_err() || fs::rename(&tmp, &path).is_err() {
            self.dirty.store(true, Ordering::SeqCst);
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

// ---- Settings ----

fn opt<'a>(s: &'a Settings, ns: &str, key: &str) -> Option<&'a Value> {
    s.option(ns, key)
}

fn opt_bool(s: &Settings, ns: &str, key: &str, default: bool) -> bool {
    opt(s, ns, key).and_then(Value::as_bool).unwrap_or(default)
}

fn opt_num(s: &Settings, ns: &str, key: &str, default: f64) -> f64 {
    opt(s, ns, key).and_then(Value::as_f64).unwrap_or(default)
}

fn opt_str<'a>(s: &'a Settings, ns: &str, key: &str, default: &'a str) -> &'a str {
    opt(s, ns, key).and_then(Value::as_str).unwrap_or(default)
}

fn opt_list(s: &Settings, ns: &str, key: &str) -> Vec<String> {
    opt(s, ns, key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
        .unwrap_or_default()
}

fn settings(app: &AppHandle) -> Settings {
    app.state::<Store>().get()
}

fn retention(s: &Settings) -> (usize, u32) {
    (opt_num(s, "clipboardHistory", "maxItems", 1000.0) as usize, opt_num(s, "clipboardHistory", "keepDays", 0.0) as u32)
}

fn ignored(app_name: &str, list: &[String]) -> bool {
    if app_name.is_empty() {
        return false;
    }
    let norm = |s: &str| {
        let s = s.trim().to_lowercase();
        let s = s.rsplit(['\\', '/']).next().unwrap_or(&s).to_string();
        s.strip_suffix(".exe").map(String::from).unwrap_or(s)
    };
    let name = norm(app_name);
    list.iter().any(|i| norm(i) == name)
}

// ---- Start-up ----

pub fn start(app: &AppHandle) -> tauri::Result<()> {
    let dir = app.path().app_data_dir()?.join("clipboard");
    let history: History = fs::read_to_string(dir.join("history.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let snippets = snippets::load(&dir.join("snippets.json"));
    shelf::start(app, &dir)?;
    app.manage(ClipState {
        dir,
        history: Mutex::new(history),
        snippets: Mutex::new(snippets),
        dirty: AtomicBool::new(false),
        own: Mutex::new(VecDeque::new()),
        clear_at: Mutex::new(None),
        previous: Mutex::new(None),
        busy: AtomicBool::new(false),
        zones: Mutex::new(HashMap::new()),
    });
    prune(app);
    typing::sync(app, &settings(app));

    let (tx, rx) = mpsc::channel();
    sys::watch(tx);
    let handle = app.clone();
    thread::Builder::new()
        .name("clipboard-events".into())
        .spawn(move || {
            for event in rx {
                match event {
                    Event::Changed => on_change(&handle),
                    Event::Lock | Event::Sleep => on_lock(&handle),
                }
            }
        })
        .ok();
    let handle = app.clone();
    thread::Builder::new().name("clipboard-timer".into()).spawn(move || tick(&handle)).ok();
    Ok(())
}

pub fn on_settings_changed(app: &AppHandle, old: &Settings, new: &Settings) {
    if retention(old) != retention(new) {
        prune(app);
    }
    if !new.enabled("clipboardHistory") {
        hide_popup(app, HISTORY_WINDOW, false);
    }
    if !new.enabled("textSnippets") {
        hide_popup(app, SNIPPETS_WINDOW, false);
    }
    if !new.enabled("autoClear") || opt_num(new, "autoClear", "delaySeconds", 60.0) <= 0.0 {
        *app.state::<ClipState>().clear_at.lock().unwrap() = None;
    }
    if opt_bool(old, "clipboardHistory", "showTrayLatest", false) && !opt_bool(new, "clipboardHistory", "showTrayLatest", false) {
        set_tray_tooltip(app, "Notifi".into());
    }
    typing::sync(app, new);
    shelf::on_settings_changed(app, old, new);
}

fn prune(app: &AppHandle) {
    let (max, days) = retention(&settings(app));
    let state = app.state::<ClipState>();
    let removed = state.history.lock().unwrap().prune(now_ms(), max, days);
    if !removed.is_empty() {
        state.forget_files(&removed);
        state.dirty.store(true, Ordering::SeqCst);
        let _ = app.emit_to(HISTORY_WINDOW, CHANGED_EVENT, ());
    }
}

fn tick(app: &AppHandle) {
    let mut last_prune = Instant::now();
    loop {
        thread::sleep(Duration::from_millis(250));
        let state = app.state::<ClipState>();
        let due = {
            let mut at = state.clear_at.lock().unwrap();
            match *at {
                Some((when, seq)) if Instant::now() >= when => {
                    *at = None;
                    Some(seq)
                }
                _ => None,
            }
        };
        if let Some(seq) = due {
            if settings(app).enabled("autoClear") && sys::sequence() == seq {
                if let Ok(n) = sys::clear() {
                    state.mark_own(n);
                }
            }
        }
        if last_prune.elapsed() > Duration::from_secs(600) {
            last_prune = Instant::now();
            prune(app);
        }
        state.save_history();
    }
}

fn on_lock(app: &AppHandle) {
    let s = settings(app);
    if s.enabled("autoClear") && opt_bool(&s, "autoClear", "clearOnLockSleep", true) {
        let state = app.state::<ClipState>();
        *state.clear_at.lock().unwrap() = None;
        if let Ok(n) = sys::clear() {
            state.mark_own(n);
        }
    }
    app.state::<ClipState>().save_history();
}

/// Reads the clipboard on a helper thread, so a source app that never answers can't stall us.
fn read_with_timeout() -> Option<Capture> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(sys::read());
    });
    rx.recv_timeout(READ_TIMEOUT).ok()?.ok()
}

fn on_change(app: &AppHandle) {
    let state = app.state::<ClipState>();
    let mut seq = sys::sequence();
    if state.is_own(seq) {
        return;
    }
    let s = settings(app);
    let auto_clear = s.enabled("autoClear").then(|| opt_num(&s, "autoClear", "delaySeconds", 60.0)).filter(|d| *d > 0.0);
    let auto_clean = s.enabled("cleanUrl") && opt_bool(&s, "cleanUrl", "automatic", false);
    let record = s.enabled("clipboardHistory");
    if auto_clear.is_some() || auto_clean || record {
        let Some(mut cap) = read_with_timeout() else { return };
        if auto_clean && !cap.concealed {
            let rules = opt_list(&s, "cleanUrl", "siteRules");
            if let Some(cleaned) = cap.text.as_deref().and_then(|t| cleanurl::clean(t, &rules)) {
                if let Ok(Some(n)) = sys::write_if(seq, &Payload::text(cleaned.url.clone())) {
                    state.mark_own(n);
                    seq = n;
                    let _ = app.emit(CLEANED_EVENT, &cleaned.removed);
                    cap.text = Some(cleaned.url);
                    cap.html = None;
                    cap.rtf = None;
                }
            }
        }
        if let Some(delay) = auto_clear {
            *state.clear_at.lock().unwrap() = Some((Instant::now() + Duration::from_secs_f64(delay), seq));
        }
        if record {
            record_copy(app, &s, cap);
        }
    }
}

fn record_copy(app: &AppHandle, s: &Settings, cap: Capture) {
    if cap.concealed || ignored(&cap.app, &opt_list(s, "clipboardHistory", "ignoredApps")) {
        return;
    }
    if opt_bool(s, "clipboardHistory", "sensitiveFilter", false) && cap.text.as_deref().is_some_and(history::looks_sensitive) {
        return;
    }
    let keep_media = opt_bool(s, "clipboardHistory", "saveImagesAndFiles", true);
    let Some(entry) = cap.to_entry(keep_media, now_ms()) else { return };
    let state = app.state::<ClipState>();
    if let (Kind::Image, Some(img), Some(stem)) = (entry.kind, &cap.image, &entry.image) {
        if !state.image_path(stem, false).exists() && save_image(&state, stem, img).is_err() {
            return;
        }
    }
    let (max, days) = retention(s);
    let removed = {
        let mut h = state.history.lock().unwrap();
        if let Inserted::Pinned(_) = h.insert(entry) {
            return;
        }
        h.prune(now_ms(), max, days)
    };
    state.forget_files(&removed);
    state.dirty.store(true, Ordering::SeqCst);
    let _ = app.emit_to(HISTORY_WINDOW, CHANGED_EVENT, ());
    if opt_bool(s, "clipboardHistory", "showTrayLatest", false) {
        if let Some(text) = cap.text.as_deref() {
            set_tray_tooltip(app, tray_tooltip(text));
        }
    }
}

/// The tray tooltip for the latest copy: its first line, cut short.
fn tray_tooltip(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    let short: String = line.chars().take(80).collect();
    if short.is_empty() {
        "Notifi".into()
    } else if short.chars().count() < line.chars().count() {
        format!("Notifi: {short}...")
    } else {
        format!("Notifi: {short}")
    }
}

fn set_tray_tooltip(app: &AppHandle, tooltip: String) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = handle.tray_by_id(crate::tray::TRAY_ID) {
            let _ = tray.set_tooltip(Some(&tooltip));
        }
    });
}

pub fn encode_png(img: &Image) -> Result<Vec<u8>, String> {
    let buf = image::RgbaImage::from_raw(img.width, img.height, img.rgba.clone()).ok_or("bad image")?;
    encode(&buf)
}

fn encode(buf: &image::RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = std::io::Cursor::new(Vec::new());
    buf.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

fn save_image(state: &ClipState, stem: &str, img: &Image) -> Result<(), String> {
    let buf = image::RgbaImage::from_raw(img.width, img.height, img.rgba.clone()).ok_or("bad image")?;
    fs::create_dir_all(state.dir.join("images")).map_err(|e| e.to_string())?;
    fs::write(state.image_path(stem, false), encode(&buf)?).map_err(|e| e.to_string())?;
    let scale = (THUMB as f64 / img.width.max(img.height) as f64).min(1.0);
    let (tw, th) = (((img.width as f64 * scale).round() as u32).max(1), ((img.height as f64 * scale).round() as u32).max(1));
    let thumb = image::imageops::thumbnail(&buf, tw, th);
    fs::write(state.image_path(stem, true), encode(&thumb)?).map_err(|e| e.to_string())
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

// ---- Popups ----

pub fn build_history_window(app: &AppHandle) -> tauri::Result<()> {
    create_window(app, HISTORY_WINDOW, "clipboard.html", (760.0, 500.0))
}

pub fn build_snippets_window(app: &AppHandle) -> tauri::Result<()> {
    create_window(app, SNIPPETS_WINDOW, "snippets.html", (520.0, 460.0))
}

/// Builds one popup window unless it already exists.
fn create_window(app: &AppHandle, label: &'static str, url: &str, size: (f64, f64)) -> tauri::Result<()> {
    if app.get_webview_window(label).is_some() {
        return Ok(());
    }
    {
        let window = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
            .title("Notifi")
            .inner_size(size.0, size.1)
            .min_inner_size(360.0, 220.0)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .resizable(label == HISTORY_WINDOW)
            .visible(false)
            .focused(false)
            .build()?;
        let handle = app.clone();
        let w = window.clone();
        window.on_window_event(move |event| match event {
            WindowEvent::Focused(false) if !window::no_focus() => hide_popup(&handle, label, false),
            WindowEvent::Resized(size) if label == HISTORY_WINDOW && w.is_visible().unwrap_or(false) => {
                if opt_str(&settings(&handle), "clipboardHistory", "layout", "list") == "list" {
                    let scale = w.scale_factor().unwrap_or(1.0);
                    let state = handle.state::<ClipState>();
                    state.history.lock().unwrap().window = Some((size.width as f64 / scale, size.height as f64 / scale));
                    state.dirty.store(true, Ordering::SeqCst);
                }
            }
            _ => {}
        });
    }
    Ok(())
}

fn place(app: &AppHandle, label: &str, shelf: bool) {
    let Some(window) = app.get_webview_window(label) else { return };
    let (px, py) = app.cursor_position().map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
    let Some(monitor) = app.monitor_from_point(px, py).ok().flatten().or_else(|| app.primary_monitor().ok().flatten()) else { return };
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let (wx, wy, ww, wh) = (work.position.x as f64, work.position.y as f64, work.size.width as f64, work.size.height as f64);
    let (w, h, x, y) = if shelf {
        let (w, h) = (ww - 32.0 * scale, 300.0 * scale);
        (w, h, wx + 16.0 * scale, wy + wh - h - 8.0 * scale)
    } else {
        let (lw, lh) = if label == HISTORY_WINDOW {
            app.state::<ClipState>().history.lock().unwrap().window.unwrap_or((760.0, 500.0))
        } else {
            (520.0, 460.0)
        };
        let (w, h) = ((lw * scale).min(ww), (lh * scale).min(wh));
        (w, h, wx + (ww - w) / 2.0, wy + (wh - h) / 3.0)
    };
    let _ = window.set_size(PhysicalSize::new(w.round() as u32, h.round() as u32));
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OpenPayload {
    layout: String,
    auto_paste: bool,
}

fn show_popup(app: &AppHandle, label: &str) {
    let Some(window) = app.get_webview_window(label) else { return };
    if window.is_visible().unwrap_or(false) {
        hide_popup(app, label, true);
        return;
    }
    let s = settings(app);
    let layout = opt_str(&s, "clipboardHistory", "layout", "list").to_string();
    *app.state::<ClipState>().previous.lock().unwrap() = sys::foreground();
    place(app, label, label == HISTORY_WINDOW && layout == "shelf");
    let _ = window.show();
    if !window::no_focus() {
        let _ = window.set_focus();
    }
    if label == HISTORY_WINDOW {
        let auto_paste = opt_bool(&s, "clipboardHistory", "autoPaste", true);
        let _ = app.emit_to(label, OPEN_EVENT, OpenPayload { layout, auto_paste });
    } else {
        let _ = app.emit_to(label, SNIPPETS_OPEN_EVENT, ());
    }
}

/// Hides a popup; `refocus` hands focus back to the app that had it before.
fn hide_popup(app: &AppHandle, label: &str, refocus: bool) {
    let Some(window) = app.get_webview_window(label) else { return };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    // When focus already left for another app (a click outside), leave it there.
    let refocus = refocus && window.is_focused().unwrap_or(false);
    let _ = window.hide();
    let previous = *app.state::<ClipState>().previous.lock().unwrap();
    if let (true, Some(prev)) = (refocus, previous) {
        sys::focus(prev);
    }
}

/// Runs a keybind action from keybinds::FEATURE_SHORTCUTS.
pub fn run_action(app: &AppHandle, action: &str) {
    match action {
        "clipboardHistory" => show_popup(app, HISTORY_WINDOW),
        "snippetMenu" => show_popup(app, SNIPPETS_WINDOW),
        "plainTextPaste" => {
            let app = app.clone();
            thread::spawn(move || paste_plain(&app));
        }
        "cleanUrl" => {
            let app = app.clone();
            thread::spawn(move || clean_now(&app));
        }
        "pasteImageAsFile" => {
            let app = app.clone();
            thread::spawn(move || paste_image_file(&app));
        }
        "shelfToggle" => shelf::toggle(app),
        _ => {}
    }
}

// ---- Pasting ----

/// Sends the paste keystroke. When the user bound plain-text paste to Ctrl+V itself, that
/// shortcut is lifted for the moment so the keystroke reaches the app.
fn send_paste(app: &AppHandle) {
    sys::release_modifiers();
    let accel = opt_str(&settings(app), "plainTextPaste", "shortcut", "").to_string();
    let is_paste_key = keybinds::parse(&accel).ok().zip(keybinds::parse("Ctrl+V").ok()).is_some_and(|(a, b)| a.id() == b.id());
    let paused = is_paste_key && settings(app).enabled("plainTextPaste");
    if paused {
        if let Ok(sc) = keybinds::parse(&accel) {
            let _ = app.global_shortcut().unregister(sc);
        }
    }
    sys::send_paste();
    if paused {
        thread::sleep(Duration::from_millis(60));
        keybinds::apply(app, &settings(app).shortcuts);
    }
}

/// Puts `text` on the clipboard, pastes it, then restores what was there unless a newer copy
/// arrived meanwhile.
fn paste_temporarily(app: &AppHandle, payload: &Payload) -> Result<(), String> {
    let state = app.state::<ClipState>();
    let snapshot = sys::snapshot().ok();
    let seq = sys::write(payload)?;
    state.mark_own(seq);
    if sys::sequence() != seq {
        return Ok(());
    }
    send_paste(app);
    thread::sleep(RESTORE_AFTER);
    if let (Some(snapshot), true) = (snapshot, sys::sequence() == seq) {
        if let Ok(n) = sys::restore(&snapshot) {
            state.mark_own(n);
        }
    }
    Ok(())
}

fn paste_plain(app: &AppHandle) {
    let state = app.state::<ClipState>();
    if state.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    match sys::read_text().filter(|t| !t.is_empty()) {
        Some(text) => {
            let _ = paste_temporarily(app, &Payload::text(text));
        }
        None => send_paste(app),
    }
    state.busy.store(false, Ordering::SeqCst);
}

fn clean_now(app: &AppHandle) {
    let seq = sys::sequence();
    let rules = opt_list(&settings(app), "cleanUrl", "siteRules");
    let Some(cleaned) = sys::read_text().and_then(|t| cleanurl::clean(&t, &rules)) else { return };
    if let Ok(Some(n)) = sys::write_if(seq, &Payload::text(cleaned.url)) {
        app.state::<ClipState>().mark_own(n);
        let _ = app.emit(CLEANED_EVENT, &cleaned.removed);
    }
}

/// Saves a copied image as a timestamped PNG and pastes it into the file manager in front as a
/// file, so the file manager puts it in the folder it has open
/// (spec/clipboard/file-manager-shortcuts.md).
fn paste_image_file(app: &AppHandle) {
    const FILE_MANAGERS: [&str; 8] = ["explorer", "dolphin", "nautilus", "nemo", "caja", "thunar", "pcmanfm", "pcmanfm-qt"];
    // Wayland doesn't say which app is in front, so there the shortcut is trusted.
    let front = sys::foreground_app();
    if !front.is_empty() && !ignored(&front, &FILE_MANAGERS.map(String::from)) {
        return;
    }
    let state = app.state::<ClipState>();
    if state.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let saved = (|| -> Result<PathBuf, String> {
        let png = encode_png(&sys::read_image().ok_or("There is no image on the clipboard.")?)?;
        // The previous one has been copied by now; only the newest is kept here.
        let dir = state.dir.join("paste-image");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let local = state.zones.lock().unwrap().get("local").copied().unwrap_or(0);
        let path = dir.join(format!("Image {}.png", snippets::format(now_ms(), local, "YYYY-MM-DD HH.mm.ss")));
        fs::write(&path, png).map_err(|e| e.to_string())?;
        Ok(path)
    })();
    if let Ok(path) = saved {
        let _ = paste_temporarily(app, &Payload { files: vec![path.to_string_lossy().into_owned()], ..Default::default() });
    }
    state.busy.store(false, Ordering::SeqCst);
}

fn payload_for(state: &ClipState, ids: &[u64], plain: bool) -> Result<Payload, String> {
    let h = state.history.lock().unwrap();
    let entries: Vec<&Entry> = ids.iter().filter_map(|id| h.get(*id)).collect();
    match entries.as_slice() {
        [] => Err("That entry is no longer in the history.".into()),
        [e] => Ok(match e.kind {
            Kind::Image => {
                let stem = e.image.as_deref().unwrap_or_default();
                let png = fs::read(state.image_path(stem, false)).map_err(|_| "The saved image is missing.".to_string())?;
                Payload { png: Some(png), ..Default::default() }
            }
            Kind::Files if !plain => Payload { files: e.files.clone(), ..Default::default() },
            _ if plain => Payload::text(e.text.clone()),
            _ => Payload { text: Some(e.text.clone()), html: e.html.clone(), rtf: e.rtf.clone(), ..Default::default() },
        }),
        many => {
            let text: Vec<&str> = many.iter().filter(|e| e.kind != Kind::Image).map(|e| e.text.as_str()).collect();
            if text.is_empty() {
                return Err("Images can only be pasted one at a time.".into());
            }
            Ok(Payload::text(text.join("\n")))
        }
    }
}

fn copy_entries(app: &AppHandle, ids: &[u64], plain: bool) -> Result<(), String> {
    let state = app.state::<ClipState>();
    let payload = payload_for(&state, ids, plain)?;
    let seq = sys::write(&payload)?;
    state.mark_own(seq);
    {
        let mut h = state.history.lock().unwrap();
        let now = now_ms();
        for id in ids.iter().rev() {
            h.touch(*id, now);
        }
    }
    state.dirty.store(true, Ordering::SeqCst);
    let _ = app.emit_to(HISTORY_WINDOW, CHANGED_EVENT, ());
    Ok(())
}

// ---- Commands: history popup ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryView {
    id: u64,
    kind: Kind,
    text: String,
    length: usize,
    app: String,
    created: i64,
    pinned: bool,
    files: Vec<String>,
    width: u32,
    height: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListView {
    entries: Vec<EntryView>,
    layout: String,
    auto_paste: bool,
}

#[tauri::command]
pub fn clipboard_list(app: AppHandle) -> ListView {
    let s = settings(&app);
    let state = app.state::<ClipState>();
    let entries = state
        .history
        .lock()
        .unwrap()
        .entries
        .iter()
        .map(|e| EntryView {
            id: e.id,
            kind: e.kind,
            text: e.text.chars().take(PREVIEW_CHARS).collect(),
            length: e.text.chars().count(),
            app: e.app.clone(),
            created: e.created,
            pinned: e.pinned,
            files: e.files.clone(),
            width: e.width,
            height: e.height,
        })
        .collect();
    ListView {
        entries,
        layout: opt_str(&s, "clipboardHistory", "layout", "list").to_string(),
        auto_paste: opt_bool(&s, "clipboardHistory", "autoPaste", true),
    }
}

#[tauri::command]
pub fn clipboard_image(app: AppHandle, id: u64, full: bool) -> Option<String> {
    let state = app.state::<ClipState>();
    let stem = state.history.lock().unwrap().get(id)?.image.clone()?;
    let bytes = fs::read(state.image_path(&stem, !full)).or_else(|_| fs::read(state.image_path(&stem, false))).ok()?;
    Some(format!("data:image/png;base64,{}", base64(&bytes)))
}

#[tauri::command]
pub async fn clipboard_paste(app: AppHandle, ids: Vec<u64>, plain: bool) -> Result<(), String> {
    copy_entries(&app, &ids, plain)?;
    hide_popup(&app, HISTORY_WINDOW, true);
    if opt_bool(&settings(&app), "clipboardHistory", "autoPaste", true) {
        thread::sleep(Duration::from_millis(80));
        send_paste(&app);
    }
    Ok(())
}

#[tauri::command]
pub async fn clipboard_copy(app: AppHandle, ids: Vec<u64>) -> Result<(), String> {
    copy_entries(&app, &ids, false)?;
    hide_popup(&app, HISTORY_WINDOW, true);
    Ok(())
}

#[tauri::command]
pub fn clipboard_pin(app: AppHandle, id: u64, pinned: bool) {
    let state = app.state::<ClipState>();
    if state.history.lock().unwrap().set_pinned(id, pinned) {
        state.dirty.store(true, Ordering::SeqCst);
        let _ = app.emit_to(HISTORY_WINDOW, CHANGED_EVENT, ());
    }
}

#[tauri::command]
pub fn clipboard_delete(app: AppHandle, ids: Vec<u64>) {
    let state = app.state::<ClipState>();
    let removed = state.history.lock().unwrap().delete(&ids);
    state.forget_files(&removed);
    state.dirty.store(true, Ordering::SeqCst);
    let _ = app.emit_to(HISTORY_WINDOW, CHANGED_EVENT, ());
}

#[derive(Serialize)]
pub struct ClearCount {
    count: usize,
    before: i64,
}

#[tauri::command]
pub fn clipboard_clear_count(app: AppHandle) -> ClearCount {
    let before = now_ms();
    ClearCount { count: app.state::<ClipState>().history.lock().unwrap().clear_count(before), before }
}

#[tauri::command]
pub fn clipboard_clear_unpinned(app: AppHandle, before: i64) -> usize {
    let state = app.state::<ClipState>();
    let removed = state.history.lock().unwrap().clear_unpinned(before);
    state.forget_files(&removed);
    state.dirty.store(true, Ordering::SeqCst);
    let _ = app.emit_to(HISTORY_WINDOW, CHANGED_EVENT, ());
    removed.len()
}

#[tauri::command]
pub fn clipboard_hide(app: AppHandle) {
    hide_popup(&app, HISTORY_WINDOW, true);
}

// ---- Commands: snippets ----

#[tauri::command]
pub fn snippets_get(app: AppHandle) -> SnippetFile {
    app.state::<ClipState>().snippets.lock().unwrap().clone()
}

#[tauri::command]
pub fn snippets_save(app: AppHandle, snippets: Vec<Snippet>) -> Result<(), String> {
    snippets::validate(&snippets)?;
    let state = app.state::<ClipState>();
    let file = SnippetFile { snippets };
    snippets::save(&state.snippets_path(), &file)?;
    *state.snippets.lock().unwrap() = file;
    let _ = app.emit(SNIPPETS_CHANGED_EVENT, ());
    Ok(())
}

fn expand(text: &str, zones: &HashMap<String, i32>) -> String {
    let clipboard = sys::read_text().unwrap_or_default();
    snippets::expand(text, &snippets::Context { now_ms: now_ms(), clipboard: &clipboard, zones })
}

/// Expands a snippet for a typed trigger, with the zones the snippet window last reported.
fn expand_typed(app: &AppHandle, text: &str) -> String {
    let zones = app.state::<ClipState>().zones.lock().unwrap().clone();
    expand(text, &zones)
}

#[tauri::command]
pub fn snippets_zones(app: AppHandle, zones: HashMap<String, i32>) {
    *app.state::<ClipState>().zones.lock().unwrap() = zones;
}

#[tauri::command]
pub async fn snippet_preview(text: String, zones: HashMap<String, i32>) -> String {
    expand(&text, &zones)
}

#[tauri::command]
pub async fn snippet_insert(app: AppHandle, id: String, zones: HashMap<String, i32>) -> Result<(), String> {
    let snippet = app
        .state::<ClipState>()
        .snippets
        .lock()
        .unwrap()
        .snippets
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or("That snippet no longer exists.")?;
    let text = expand(&snippet.text, &zones);
    hide_popup(&app, SNIPPETS_WINDOW, true);
    thread::sleep(Duration::from_millis(80));
    if opt_str(&settings(&app), "textSnippets", "insertMethod", "paste") == "type" {
        sys::release_modifiers();
        sys::type_text(&text);
        return Ok(());
    }
    paste_temporarily(&app, &Payload::text(text))
}

#[tauri::command]
pub fn snippets_hide(app: AppHandle) {
    hide_popup(&app, SNIPPETS_WINDOW, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_tooltip_is_the_first_line_cut_short() {
        assert_eq!(tray_tooltip("\n  hello  \nworld"), "Notifi: hello");
        assert_eq!(tray_tooltip("   "), "Notifi");
        assert!(tray_tooltip(&"x".repeat(200)).ends_with("..."));
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn ignored_apps_match_names_and_paths() {
        let list = vec!["KeePassXC".to_string(), "C:\\Program Files\\1Password\\1Password.exe".to_string()];
        assert!(ignored("keepassxc", &list));
        assert!(ignored("1Password", &list));
        assert!(!ignored("notepad", &list));
        assert!(!ignored("", &list));
    }
}
