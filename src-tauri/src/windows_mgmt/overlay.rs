//! The snap overlay: one transparent, click-through window that never takes focus. It is moved over
//! whichever display needs it and tells its page what to draw (snap preview, hold ring, hint).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use notifi_platform::Rect;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "snap";
const SHOW_EVENT: &str = "snap://show";
const HIDE_EVENT: &str = "snap://hide";
/// Long enough for the page's fade-out to finish.
const FADE_MS: u64 = 320;

static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Everything the page draws, in physical pixels relative to the display it sits on.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Frame {
    kind: &'static str,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    progress: f32,
    label: String,
    display_width: u32,
    display_height: u32,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("snap.html".into()))
        .title("Notifi snap areas")
        .inner_size(400.0, 300.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .resizable(false)
        .focused(false)
        .focusable(false)
        .visible(false)
        .build()?;
    window.set_ignore_cursor_events(true)?;
    Ok(())
}

fn present(app: &AppHandle, display: Rect, frame: Frame) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    let Some(window) = app.get_webview_window(LABEL) else { return };
    let _ = window.set_position(PhysicalPosition::new(display.x, display.y));
    let _ = window.set_size(PhysicalSize::new(display.width, display.height));
    if !window.is_visible().unwrap_or(false) {
        let _ = window.show();
        let _ = window.set_ignore_cursor_events(true);
    }
    let _ = app.emit_to(LABEL, SHOW_EVENT, frame);
}

/// The rectangle a window would snap to.
pub fn preview(app: &AppHandle, display: Rect, target: Rect) {
    present(
        app,
        display,
        Frame {
            kind: "preview",
            x: target.x - display.x,
            y: target.y - display.y,
            width: target.width,
            height: target.height,
            progress: 0.0,
            label: String::new(),
            display_width: display.width,
            display_height: display.height,
        },
    );
}

/// The ring that fills while a guarded shortcut is held. `progress` is 0 to 1.
pub fn hold(app: &AppHandle, display: Rect, progress: f32, label: &str) {
    present(app, display, Frame { kind: "hold", x: 0, y: 0, width: 0, height: 0, progress, label: label.into(), display_width: display.width, display_height: display.height });
}

/// A short message, such as "Press again to quit".
pub fn hint(app: &AppHandle, display: Rect, label: &str) {
    present(app, display, Frame { kind: "hint", x: 0, y: 0, width: 0, height: 0, progress: 0.0, label: label.into(), display_width: display.width, display_height: display.height });
    let generation = GENERATION.load(Ordering::SeqCst);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1400));
        if GENERATION.load(Ordering::SeqCst) == generation {
            hide(&app);
        }
    });
}

pub fn hide(app: &AppHandle) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = app.emit_to(LABEL, HIDE_EVENT, ());
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(FADE_MS));
        if GENERATION.load(Ordering::SeqCst) == generation {
            if let Some(window) = app.get_webview_window(LABEL) {
                let _ = window.hide();
            }
        }
    });
}
