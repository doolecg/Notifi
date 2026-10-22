//! The small overlays: a volume or notice pill that fades after a moment, and a muted-microphone
//! indicator that stays while the microphones are muted. Neither takes focus or catches the pointer.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

const OSD: &str = "osd";
const MIC: &str = "micmuted";
const SHOW_EVENT: &str = "osd://show";
const HIDE_EVENT: &str = "osd://hide";
const HOLD: Duration = Duration::from_millis(1600);
const FADE: Duration = Duration::from_millis(320);

static GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    /// `volume` draws a level bar, `text` just the label.
    pub kind: &'static str,
    pub icon: String,
    pub label: String,
    /// 0..=1
    pub level: f32,
    pub muted: bool,
}

pub fn volume(icon: &str, label: String, level: f32, muted: bool) -> Notice {
    Notice { kind: "volume", icon: icon.into(), label, level, muted }
}

pub fn text(icon: &str, label: String) -> Notice {
    Notice { kind: "text", icon: icon.into(), label, level: 0.0, muted: false }
}

/// The two overlay windows: the volume and message popup, and the muted-microphone dot.
pub const LABELS: &[&str] = &[OSD, MIC];

/// Builds whichever of the overlay windows are missing.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    for (label, url, size) in [(OSD, "osd.html", (260.0, 64.0)), (MIC, "osd.html#mic", (44.0, 44.0))] {
        if app.get_webview_window(label).is_some() {
            continue;
        }
        let window = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
            .title("Notifi overlay")
            .inner_size(size.0, size.1)
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
        let _ = window.set_ignore_cursor_events(true);
    }
    Ok(())
}

fn place(app: &AppHandle, label: &str, corner: bool) {
    let Some(window) = app.get_webview_window(label) else { return };
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let (w, h) = (window.outer_size().map(|s| s.width).unwrap_or((260.0 * scale) as u32), window.outer_size().map(|s| s.height).unwrap_or((64.0 * scale) as u32));
    let (x, y) = if corner {
        (area.position.x + area.size.width as i32 - w as i32 - (12.0 * scale) as i32, area.position.y + area.size.height as i32 - h as i32 - (12.0 * scale) as i32)
    } else {
        let y = if overlay_option(app, "overlayPlace").and_then(|v| v.as_str().map(|p| p == "pill")).unwrap_or(false) {
            area.position.y + (12.0 * scale) as i32
        } else {
            area.position.y + area.size.height as i32 - h as i32 - (90.0 * scale) as i32
        };
        (area.position.x + (area.size.width as i32 - w as i32) / 2, y)
    };
    let _ = window.set_size(PhysicalSize::new(w, h));
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// An option of the Level overlays card; `None` while that card is switched off, so the overlays behave as before.
fn overlay_option(app: &AppHandle, key: &str) -> Option<serde_json::Value> {
    let s = app.state::<crate::settings::Store>().get();
    s.enabled("osdOverlays").then(|| s.option("osdOverlays", key).cloned()).flatten()
}

fn wanted(app: &AppHandle, notice: &Notice) -> bool {
    let key = match (notice.kind, notice.icon.as_str()) {
        (_, "display") => "brightnessOverlay",
        ("volume", _) => "volumeOverlay",
        _ => return true,
    };
    overlay_option(app, key).and_then(|v| v.as_bool()).unwrap_or(true)
}

pub fn show(app: &AppHandle, notice: Notice) {
    if !wanted(app, &notice) {
        return;
    }
    let Some(window) = app.get_webview_window(OSD) else { return };
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    place(app, OSD, false);
    let _ = app.emit_to(OSD, SHOW_EVENT, notice);
    let _ = window.show();
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(HOLD);
        if GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let _ = app.emit_to(OSD, HIDE_EVENT, ());
        std::thread::sleep(FADE);
        if GENERATION.load(Ordering::SeqCst) == generation {
            if let Some(window) = app.get_webview_window(OSD) {
                let _ = window.hide();
            }
        }
    });
}

pub fn mic_indicator(app: &AppHandle, on: bool) {
    let Some(window) = app.get_webview_window(MIC) else { return };
    if on && overlay_option(app, "micMuteIndicator").and_then(|v| v.as_bool()).unwrap_or(true) {
        place(app, MIC, true);
        let _ = window.show();
    } else {
        let _ = window.hide();
    }
}
