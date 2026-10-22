//! The Shelf (spec/clipboard/shelf.md): a floating tray at a screen edge that holds files while
//! they are dragged between apps. It opens by shortcut, by shaking a dragged item, or peeks out
//! when a drag nears the chosen edge, and it never takes the keyboard focus.

use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
        Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use notifi_platform::{MouseButton, Observed, Watch};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, DragDropEvent, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewEvent, WebviewUrl, WebviewWindowBuilder};

use super::{base64, encode, ignored, opt_bool, opt_list, opt_str, settings, sys};
use crate::settings::Settings;

pub const WINDOW: &str = "shelf";
const CHANGED_EVENT: &str = "shelf://changed";
const MODE_EVENT: &str = "shelf://mode";
const HOVER_EVENT: &str = "shelf://hover";
const IMAGE_TYPES: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp"];
/// How far, in physical pixels, the pointer moves with the button down before it is a drag.
const DRAG_START: i32 = 12;
/// Logical pixels from the edge where a drag makes the shelf peek out; three times as far
/// sends it back.
const EDGE_ZONE: f64 = 48.0;
/// Depth of the strip that shows while the shelf peeks.
const PEEK: f64 = 64.0;
/// A shake is this many changes of direction, each after this much travel, within the window.
const SHAKE_TURNS: usize = 4;
const SHAKE_TRAVEL: i32 = 30;
const SHAKE_WINDOW: Duration = Duration::from_millis(800);

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    id: u64,
    path: String,
    name: String,
    folder: bool,
    image: bool,
}

#[derive(Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
enum Mode {
    Hidden,
    Peek,
    Open,
}

struct View {
    mode: Mode,
    /// Called up by a drag gesture, so it goes away again when the drag ends somewhere else.
    gesture: bool,
}

pub struct ShelfState {
    path: PathBuf,
    items: Mutex<Vec<Item>>,
    view: Mutex<View>,
}

impl ShelfState {
    fn save(&self) {
        let Ok(text) = serde_json::to_string(&*self.items.lock().unwrap()) else { return };
        let tmp = self.path.with_extension("json.tmp");
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if fs::write(&tmp, text).is_ok() {
            let _ = fs::rename(&tmp, &self.path);
        }
    }

    fn mode(&self) -> Mode {
        self.view.lock().unwrap().mode
    }
}

pub fn start(app: &AppHandle, dir: &Path) -> tauri::Result<()> {
    let path = dir.join("shelf.json");
    let s = settings(app);
    let items = if opt_bool(&s, "shelf", "clearOnQuit", false) {
        let _ = fs::remove_file(&path);
        Vec::new()
    } else {
        fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    };
    app.manage(ShelfState { path, items: Mutex::new(items), view: Mutex::new(View { mode: Mode::Hidden, gesture: false }) });
    sync(app, &s);
    Ok(())
}

pub fn on_settings_changed(app: &AppHandle, old: &Settings, new: &Settings) {
    sync(app, new);
    let mode = app.state::<ShelfState>().mode();
    if !new.enabled("shelf") {
        hide(app);
    } else if mode != Mode::Hidden && opt_str(old, "shelf", "edge", "right") != opt_str(new, "shelf", "edge", "right") {
        let gesture = app.state::<ShelfState>().view.lock().unwrap().gesture;
        show(app, mode, None, gesture);
    }
}

/// Builds the shelf window unless it already exists.
pub fn build_window(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(WINDOW).is_some() {
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("shelf.html".into()))
        .title("Notifi Shelf")
        .inner_size(260.0, 400.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .focusable(false)
        .focused(false)
        // WebView2 only takes file drops on a window that was visible when it was made, so it starts off screen and is hidden at once.
        .position(-20000.0, -20000.0)
        .visible(true)
        .build()?;
    let _ = window.hide();
    #[cfg(windows)]
    crate::switcher::prepare_window(&window);
    let handle = app.clone();
    window.on_webview_event(move |event| {
        let WebviewEvent::DragDrop(drop) = event else { return };
        match drop {
            DragDropEvent::Enter { .. } => {
                let _ = handle.emit_to(WINDOW, HOVER_EVENT, true);
                let state = handle.state::<ShelfState>();
                let gesture = state.view.lock().unwrap().gesture;
                if state.mode() == Mode::Peek {
                    show(&handle, Mode::Open, None, gesture);
                }
            }
            DragDropEvent::Drop { paths, .. } => {
                let _ = handle.emit_to(WINDOW, HOVER_EVENT, false);
                add(&handle, paths);
                // Something landed here, so the shelf stays until it is closed.
                if handle.state::<ShelfState>().mode() == Mode::Peek {
                    show(&handle, Mode::Open, None, false);
                } else {
                    handle.state::<ShelfState>().view.lock().unwrap().gesture = false;
                }
            }
            DragDropEvent::Leave => {
                let _ = handle.emit_to(WINDOW, HOVER_EVENT, false);
            }
            _ => {}
        }
    });
    Ok(())
}

#[derive(Serialize, Clone)]
struct ModePayload {
    mode: Mode,
    edge: String,
}

/// Shows the shelf against the chosen edge of the display under the pointer, or beside `at`.
fn show(app: &AppHandle, mode: Mode, at: Option<(f64, f64)>, gesture: bool) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    let edge = opt_str(&settings(app), "shelf", "edge", "right").to_string();
    let (px, py) = at.or_else(|| app.cursor_position().ok().map(|p| (p.x, p.y))).unwrap_or((0.0, 0.0));
    let Some(monitor) = app.monitor_from_point(px, py).ok().flatten().or_else(|| app.primary_monitor().ok().flatten()) else { return };
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let (wx, wy, ww, wh) = (work.position.x as f64, work.position.y as f64, work.size.width as f64, work.size.height as f64);
    let side = edge == "left" || edge == "right";
    let (w, h) = match (side, mode == Mode::Peek) {
        (true, true) => (PEEK, 400.0),
        (true, false) => (260.0, 400.0),
        (false, true) => (440.0, PEEK),
        (false, false) => (440.0, 220.0),
    };
    let (w, h) = ((w * scale).min(ww), (h * scale).min(wh));
    let (x, y) = match (at, edge.as_str()) {
        (Some(_), _) => ((px + 24.0 * scale).min(wx + ww - w).max(wx), (py - h / 2.0).clamp(wy, wy + wh - h)),
        (None, "left") => (wx, wy + (wh - h) / 2.0),
        (None, "top") => (wx + (ww - w) / 2.0, wy),
        (None, "bottom") => (wx + (ww - w) / 2.0, wy + wh - h),
        _ => (wx + ww - w, wy + (wh - h) / 2.0),
    };
    let _ = window.set_size(PhysicalSize::new(w.round() as u32, h.round() as u32));
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    *app.state::<ShelfState>().view.lock().unwrap() = View { mode, gesture };
    let _ = app.emit_to(WINDOW, MODE_EVENT, ModePayload { mode, edge });
    #[cfg(windows)]
    crate::switcher::show_inactive(&window);
    #[cfg(not(windows))]
    let _ = window.show();
}

fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        #[cfg(windows)]
        crate::switcher::hide_shown(&window);
        #[cfg(not(windows))]
        let _ = window.hide();
    }
    *app.state::<ShelfState>().view.lock().unwrap() = View { mode: Mode::Hidden, gesture: false };
}

/// The keyboard shortcut: works in every app, the excluded ones too.
pub fn toggle(app: &AppHandle) {
    if app.state::<ShelfState>().mode() == Mode::Hidden {
        show(app, Mode::Open, None, false);
    } else {
        hide(app);
    }
}

pub fn add(app: &AppHandle, paths: &[PathBuf]) {
    let state = app.state::<ShelfState>();
    {
        let mut items = state.items.lock().unwrap();
        let mut next = items.iter().map(|i| i.id).max().unwrap_or(0);
        for p in paths {
            let path = p.to_string_lossy().into_owned();
            if items.iter().any(|i| i.path == path) {
                continue;
            }
            next += 1;
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone());
            items.insert(0, Item { id: next, name, folder: p.is_dir(), image: IMAGE_TYPES.contains(&ext.as_str()), path });
        }
    }
    state.save();
    let _ = app.emit_to(WINDOW, CHANGED_EVENT, ());
}

// ---- Drag gestures ----

#[derive(Clone)]
struct Gestures {
    on: bool,
    shake: bool,
    edge_drop: bool,
    edge: String,
    excluded: Vec<String>,
}

static GESTURES: Mutex<Gestures> = Mutex::new(Gestures { on: false, shake: false, edge_drop: false, edge: String::new(), excluded: Vec::new() });
static WATCHING: AtomicBool = AtomicBool::new(false);

fn sync(app: &AppHandle, s: &Settings) {
    let (shake, edge_drop) = (opt_bool(s, "shelf", "shake", true), opt_bool(s, "shelf", "edgeDrop", true));
    let on = s.enabled("shelf") && (shake || edge_drop);
    *GESTURES.lock().unwrap() = Gestures {
        on,
        shake,
        edge_drop,
        edge: opt_str(s, "shelf", "edge", "right").to_string(),
        excluded: opt_list(s, "shelf", "excludedApps"),
    };
    if !on || WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    match notifi_platform::observe(Watch { keys: false, buttons: true, motion: true }) {
        Ok(rx) => {
            let app = app.clone();
            thread::Builder::new().name("shelf-gestures".into()).spawn(move || watch(&app, rx)).ok();
        }
        Err(e) => {
            eprintln!("shelf: drag gestures are off: {e}");
            WATCHING.store(false, Ordering::SeqCst);
        }
    }
}

/// Work area of a display, in physical pixels.
#[derive(Clone, Copy)]
struct Area {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    scale: f64,
}

impl Area {
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

struct Drag {
    start: (i32, i32),
    active: bool,
    ignored: bool,
    areas: Vec<Area>,
    last_x: i32,
    dir: i32,
    travel: i32,
    turns: VecDeque<Instant>,
}

impl Drag {
    fn new(x: i32, y: i32) -> Self {
        Self { start: (x, y), active: false, ignored: false, areas: Vec::new(), last_x: x, dir: 0, travel: 0, turns: VecDeque::new() }
    }

    fn shaken(&mut self, x: i32) -> bool {
        let dx = x - self.last_x;
        self.last_x = x;
        if dx == 0 {
            return false;
        }
        if dx.signum() == self.dir {
            self.travel += dx.abs();
            return false;
        }
        let now = Instant::now();
        if self.travel >= SHAKE_TRAVEL {
            self.turns.push_back(now);
        }
        self.dir = dx.signum();
        self.travel = dx.abs();
        while self.turns.front().is_some_and(|t| now.duration_since(*t) > SHAKE_WINDOW) {
            self.turns.pop_front();
        }
        if self.turns.len() >= SHAKE_TURNS {
            self.turns.clear();
            return true;
        }
        false
    }
}

fn watch(app: &AppHandle, rx: Receiver<Observed>) {
    let mut drag: Option<Drag> = None;
    while let Ok(event) = rx.recv() {
        match event {
            Observed::Button { button: MouseButton::Left, down: true, x, y } => drag = Some(Drag::new(x, y)),
            Observed::Button { button: MouseButton::Left, down: false, .. } => {
                if drag.take().is_some_and(|d| d.active && !d.ignored) {
                    ended(app);
                }
            }
            Observed::Motion { x, y } => {
                let Some(d) = drag.as_mut() else { continue };
                let g = GESTURES.lock().unwrap().clone();
                if !g.on {
                    break;
                }
                moved(app, d, x, y, &g);
            }
            _ => {}
        }
    }
    WATCHING.store(false, Ordering::SeqCst);
}

fn moved(app: &AppHandle, d: &mut Drag, x: i32, y: i32, g: &Gestures) {
    if !d.active {
        if (x - d.start.0).abs().max((y - d.start.1).abs()) < DRAG_START {
            return;
        }
        d.active = true;
        d.last_x = x;
        d.ignored = started_on_shelf(app, d.start) || ignored(&sys::foreground_app(), &g.excluded);
        d.areas = app
            .available_monitors()
            .unwrap_or_default()
            .iter()
            .map(|m| {
                let w = m.work_area();
                Area { x: w.position.x, y: w.position.y, w: w.size.width as i32, h: w.size.height as i32, scale: m.scale_factor() }
            })
            .collect();
    }
    if d.ignored {
        return;
    }
    let state = app.state::<ShelfState>();
    if g.shake && d.shaken(x) && state.mode() != Mode::Open {
        show(app, Mode::Open, Some((x as f64, y as f64)), true);
        return;
    }
    if !g.edge_drop {
        return;
    }
    let Some(area) = d.areas.iter().find(|a| a.contains(x, y)).copied() else { return };
    let (dist, beyond) = match g.edge.as_str() {
        "left" => (x - area.x, (area.x - 1, y)),
        "top" => (y - area.y, (x, area.y - 1)),
        "bottom" => (area.y + area.h - 1 - y, (x, area.y + area.h)),
        _ => (area.x + area.w - 1 - x, (area.x + area.w, y)),
    };
    // An edge shared with another display is crossed on the way over, not aimed at.
    if d.areas.iter().any(|a| a.contains(beyond.0, beyond.1)) {
        return;
    }
    let dist = dist as f64 / area.scale;
    let (mode, gesture) = {
        let v = state.view.lock().unwrap();
        (v.mode, v.gesture)
    };
    if dist <= EDGE_ZONE && mode == Mode::Hidden {
        show(app, Mode::Peek, None, true);
    } else if dist > EDGE_ZONE * 3.0 && mode == Mode::Peek && gesture {
        hide(app);
    }
}

/// Drags that start on the shelf are items leaving it.
fn started_on_shelf(app: &AppHandle, (x, y): (i32, i32)) -> bool {
    let Some(w) = app.get_webview_window(WINDOW).filter(|w| w.is_visible().unwrap_or(false)) else { return false };
    let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) else { return false };
    x >= pos.x && y >= pos.y && x < pos.x + size.width as i32 && y < pos.y + size.height as i32
}

/// A drag the gestures called the shelf up for has ended; unless it was dropped on the shelf,
/// the shelf goes away again.
fn ended(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        // The drop reaches us through the event loop a moment after the button goes up.
        thread::sleep(Duration::from_millis(400));
        let gesture = {
            let state = app.state::<ShelfState>();
            let v = state.view.lock().unwrap();
            v.gesture && v.mode != Mode::Hidden
        };
        if gesture {
            hide(&app);
        }
    });
}

// ---- Commands ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShelfView {
    items: Vec<Item>,
    mode: Mode,
    edge: String,
    /// Windows drags items out natively; elsewhere the page uses HTML drag and drop.
    native_drag: bool,
}

fn paths_of(app: &AppHandle, ids: &[u64]) -> Vec<PathBuf> {
    app.state::<ShelfState>().items.lock().unwrap().iter().filter(|i| ids.contains(&i.id)).map(|i| PathBuf::from(&i.path)).collect()
}

#[tauri::command]
pub fn shelf_list(app: AppHandle) -> ShelfView {
    let state = app.state::<ShelfState>();
    // Files moved away or deleted since they were added leave the shelf.
    let gone = {
        let mut items = state.items.lock().unwrap();
        let before = items.len();
        items.retain(|i| Path::new(&i.path).exists());
        before != items.len()
    };
    if gone {
        state.save();
    }
    let items = state.items.lock().unwrap().clone();
    ShelfView {
        items,
        mode: state.mode(),
        edge: opt_str(&settings(&app), "shelf", "edge", "right").to_string(),
        native_drag: cfg!(windows),
    }
}

#[tauri::command]
pub fn shelf_remove(app: AppHandle, ids: Vec<u64>) {
    let state = app.state::<ShelfState>();
    state.items.lock().unwrap().retain(|i| !ids.contains(&i.id));
    state.save();
    let _ = app.emit_to(WINDOW, CHANGED_EVENT, ());
}

#[tauri::command]
pub fn shelf_clear(app: AppHandle) {
    let state = app.state::<ShelfState>();
    state.items.lock().unwrap().clear();
    state.save();
    let _ = app.emit_to(WINDOW, CHANGED_EVENT, ());
}

/// Opens an item with its default app, or a folder in the file manager.
#[tauri::command]
pub fn shelf_open(app: AppHandle, id: u64) -> Result<(), String> {
    let path = paths_of(&app, &[id]).pop().ok_or("That item is no longer on the shelf.")?;
    let opener = if cfg!(windows) { "explorer" } else { "xdg-open" };
    Command::new(opener).arg(path).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Drags items out. On Windows this runs the drag itself; elsewhere the page ran an HTML drag
/// and calls this once it was dropped. Either way the shelf then closes.
#[tauri::command]
pub async fn shelf_drag_out(app: AppHandle, ids: Vec<u64>) -> Result<(), String> {
    let paths = paths_of(&app, &ids);
    #[cfg(windows)]
    let dropped = notifi_platform::win_drag::drag_files(&paths).map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    let dropped = !paths.is_empty();
    if dropped {
        hide(&app);
        let _ = app.emit_to(WINDOW, CHANGED_EVENT, ());
    }
    Ok(())
}

#[tauri::command]
pub async fn shelf_thumb(app: AppHandle, id: u64) -> Option<String> {
    let path = app.state::<ShelfState>().items.lock().unwrap().iter().find(|i| i.id == id && i.image).map(|i| i.path.clone())?;
    let thumb = image::open(path).ok()?.thumbnail(160, 160).to_rgba8();
    Some(format!("data:image/png;base64,{}", base64(&encode(&thumb).ok()?)))
}

#[tauri::command]
pub fn shelf_hide(app: AppHandle) {
    hide(&app);
}
