//! The floating camera preview (spec/capture/camera-preview.md): a small always-on-top mirror of the
//! chosen camera. The picture comes from the page (`getUserMedia`), so nothing is recorded or sent
//! anywhere; this side owns the window, remembers where it was and keeps it on a connected screen.
//! The window is built when it opens and destroyed when it closes, so the camera is released at once.

mod place;

use std::{fs, path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

use crate::{settings::Store, window};
use place::{clamp_onto, corner, Rect, MARGIN};

pub const WINDOW: &str = "camerapreview";
pub const ACTION: &str = "cameraPreviewToggle";
const NS: &str = "cameraPreview";
const FILE: &str = "camera-preview.json";
/// Logical size of the window for each choice of the Window size option (4:3).
const SIZES: [(&str, f64, f64); 3] = [("small", 240.0, 180.0), ("medium", 320.0, 240.0), ("large", 480.0, 360.0)];
const MIN_SIZE: (f64, f64) = (160.0, 120.0);

/// Where the window was when it closed, in physical pixels, and the size choice it was made for.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct Saved {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    size: String,
}

#[derive(Default)]
pub struct CameraState {
    /// The window's latest place while it is open.
    current: Mutex<Option<Saved>>,
}

struct Opts {
    camera: String,
    mirror: bool,
    size: String,
    remember: bool,
}

impl Opts {
    fn read(app: &AppHandle) -> Self {
        let s = app.state::<Store>().get();
        let text = |k: &str| s.option(NS, k).and_then(Value::as_str).unwrap_or("").trim().to_string();
        let size = text("size");
        Self {
            camera: text("camera"),
            mirror: s.option(NS, "mirror").and_then(Value::as_bool).unwrap_or(true),
            size: if SIZES.iter().any(|(n, ..)| *n == size) { size } else { "small".into() },
            remember: s.option(NS, "rememberPosition").and_then(Value::as_bool).unwrap_or(true),
        }
    }

    fn logical(&self) -> (f64, f64) {
        SIZES.iter().find(|(n, ..)| *n == self.size).map_or((SIZES[0].1, SIZES[0].2), |(_, w, h)| (*w, *h))
    }
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(FILE))
}

fn load(app: &AppHandle) -> Option<Saved> {
    serde_json::from_str(&fs::read_to_string(file(app)?).ok()?).ok()
}

fn save(app: &AppHandle, saved: &Saved) {
    let Some(path) = file(app) else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(saved) {
        let _ = fs::write(path, text);
    }
}

fn monitors(app: &AppHandle) -> Vec<Rect> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| Rect { x: m.position().x, y: m.position().y, w: m.size().width as i32, h: m.size().height as i32 })
        .collect()
}

/// Where a new window goes, in physical pixels: the remembered place moved onto a connected screen,
/// else the bottom-right corner of the screen under the pointer.
fn placement(app: &AppHandle, o: &Opts) -> Option<Rect> {
    let (lw, lh) = o.logical();
    let default = || {
        let at = app.cursor_position().ok();
        let under = at.and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten()).or_else(|| app.primary_monitor().ok().flatten())?;
        let scale = under.scale_factor();
        let screen = Rect { x: under.position().x, y: under.position().y, w: under.size().width as i32, h: under.size().height as i32 };
        Some(corner(((lw * scale).round() as i32, (lh * scale).round() as i32), screen, (f64::from(MARGIN) * scale).round() as i32))
    };
    let saved = o.remember.then(|| load(app)).flatten().filter(|s| s.size == o.size && s.w > 0 && s.h > 0);
    match saved {
        Some(s) => clamp_onto(Rect { x: s.x, y: s.y, w: s.w as i32, h: s.h as i32 }, &monitors(app)).or_else(default),
        None => default(),
    }
}

/// Opens the preview, or brings the open one forward. Goes to the settings page when the tool is off.
pub fn open(app: &AppHandle) {
    if !app.state::<Store>().get().enabled(NS) {
        window::open_settings_at(app, Some(NS));
        return;
    }
    if let Some(win) = app.get_webview_window(WINDOW) {
        let _ = win.show();
        if !window::no_focus() {
            let _ = win.set_focus();
        }
        return;
    }
    // Building a webview from a command or shortcut handler can deadlock on Windows, so build off-thread.
    let app = app.clone();
    std::thread::spawn(move || build(&app));
}

fn build(app: &AppHandle) {
    let o = Opts::read(app);
    let (lw, lh) = o.logical();
    let built = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("camera.html".into()))
        .title("Camera preview")
        .inner_size(lw, lh)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(true)
        .visible(false)
        .focused(false)
        .build();
    let Ok(win) = built else { return };
    if let Some(r) = placement(app, &o) {
        let _ = win.set_size(PhysicalSize::new(r.w as u32, r.h as u32));
        let _ = win.set_position(PhysicalPosition::new(r.x, r.y));
        *app.state::<CameraState>().current.lock().unwrap() = Some(Saved { x: r.x, y: r.y, w: r.w as u32, h: r.h as u32, size: o.size.clone() });
    }
    let handle = app.clone();
    win.on_window_event(move |event| track(&handle, event));
    let _ = win.show();
    if !window::no_focus() {
        let _ = win.set_focus();
    }
}

/// Keeps the latest place of the window, and saves it when the window goes (if it should be remembered).
fn track(app: &AppHandle, event: &WindowEvent) {
    let state = app.state::<CameraState>();
    match event {
        // A minimised window reports a position far off screen; that is not a place to come back to.
        WindowEvent::Moved(p) if p.x > -10_000 && p.y > -10_000 => {
            if let Some(c) = state.current.lock().unwrap().as_mut() {
                (c.x, c.y) = (p.x, p.y);
            }
        }
        WindowEvent::Resized(s) if s.width > 0 && s.height > 0 => {
            if let Some(c) = state.current.lock().unwrap().as_mut() {
                (c.w, c.h) = (s.width, s.height);
            }
        }
        WindowEvent::Destroyed => {
            let saved = state.current.lock().unwrap().take();
            if let Some(saved) = saved.filter(|_| Opts::read(app).remember) {
                save(app, &saved);
            }
        }
        _ => {}
    }
}

pub fn toggle(app: &AppHandle) {
    match app.get_webview_window(WINDOW) {
        Some(win) => {
            let _ = win.close();
        }
        None => open(app),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageOptions {
    camera: String,
    mirror: bool,
}

#[tauri::command]
pub fn camera_preview_options(app: AppHandle) -> PageOptions {
    let o = Opts::read(&app);
    PageOptions { camera: o.camera, mirror: o.mirror }
}

#[tauri::command]
pub fn camera_preview_toggle(app: AppHandle) {
    toggle(&app);
}

#[tauri::command]
pub fn camera_preview_close(window: WebviewWindow) {
    if window.label() == WINDOW {
        let _ = window.close();
    }
}

/// The camera went away while the preview was open: say so and close, so no frozen frame is left.
#[tauri::command]
pub fn camera_preview_lost(window: WebviewWindow, why: String) {
    if window.label() == WINDOW {
        crate::island::notice(window.app_handle(), why);
        let _ = window.close();
    }
}

/// Keeps the camera picked in the window as the Preferred camera option.
#[tauri::command]
pub fn camera_preview_remember(app: AppHandle, store: tauri::State<Store>, label: String) -> Result<(), String> {
    let label = label.trim().to_string();
    store
        .update(&app, |s| {
            s.features.get_mut(NS).ok_or("Camera preview has no settings")?.insert("camera".into(), json!(label));
            Ok(())
        })
        .map(|_| ())
}
