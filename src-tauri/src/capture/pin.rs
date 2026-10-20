//! Pinned screenshots (spec/capture/screenshot-editor.md): a picture kept in a small window above
//! every other window. Drag it, change its size with the wheel, close it with the cross.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
};

use image::RgbaImage;
use tauri::{ipc::Response, AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::{
    logic::{pin_size, Rect},
    png_fast, CaptureState,
};

/// Window labels of pins start with this; the capability file matches `capturepin-*`.
const PREFIX: &str = "capturepin-";
const MIN_WIDTH: u32 = 64;

#[derive(Default)]
pub struct Pins {
    next: AtomicU32,
    /// PNG of every open pin, by window label, until the page has fetched it and the pin closes.
    images: Mutex<HashMap<String, Vec<u8>>>,
}

/// Opens a pin showing `img`. `at` is where the picture was on screen (physical pixels); without
/// it the pin opens in the middle of the main display.
pub fn pin_image(app: &AppHandle, img: &RgbaImage, at: Option<Rect>) {
    let Ok(png) = png_fast(img) else {
        super::notice(app, "Could not pin the screenshot.");
        return;
    };
    let state = app.state::<CaptureState>();
    let pins = &state.pins;
    let label = format!("{PREFIX}{}", pins.next.fetch_add(1, Ordering::SeqCst));
    pins.images.lock().unwrap().insert(label.clone(), png);

    let monitor = at
        .and_then(|r| app.monitor_from_point(f64::from(r.x + r.w / 2), f64::from(r.y + r.h / 2)).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let scale = monitor.as_ref().map_or(1.0, |m| m.scale_factor());
    let (mw, mh) = monitor.as_ref().map_or((1600.0, 900.0), |m| (f64::from(m.size().width) / scale, f64::from(m.size().height) / scale));
    let (iw, ih) = (f64::from(img.width()) / scale, f64::from(img.height()) / scale);
    let fit = ((mw * 0.7) / iw).min((mh * 0.7) / ih).min(1.0);
    let (w, h) = ((iw * fit).max(f64::from(MIN_WIDTH)).round(), (ih * fit).max(f64::from(MIN_WIDTH) * ih / iw).round());
    let spot = match (at, monitor.as_ref()) {
        (Some(r), _) => Some(PhysicalPosition::new(r.x, r.y)),
        (None, Some(m)) => Some(PhysicalPosition::new(
            m.position().x + ((mw - w) * scale / 2.0) as i32,
            m.position().y + ((mh - h) * scale / 2.0) as i32,
        )),
        (None, None) => None,
    };
    let app = app.clone();
    // Building a webview from a command can deadlock on Windows, so build off-thread.
    std::thread::spawn(move || {
        let built = WebviewWindowBuilder::new(&app, label.clone(), WebviewUrl::App("capture.html#pin".into()))
            .title("Notifi Pinned Screenshot")
            .inner_size(w, h)
            .decorations(false)
            .shadow(true)
            .skip_taskbar(true)
            .always_on_top(true)
            .resizable(false)
            .focused(false)
            .build();
        match built {
            Ok(window) => {
                if let Some(p) = spot {
                    let _ = window.set_position(p);
                }
            }
            Err(_) => {
                app.state::<CaptureState>().pins.images.lock().unwrap().remove(&label);
            }
        }
    });
}

/// Pins the latest capture.
pub fn pin_last(app: &AppHandle) {
    let Some(last) = super::last_shot(app) else { return };
    let at = *app.state::<CaptureState>().last_rect.lock().unwrap();
    pin_image(app, &last.img, at);
}

/// The PNG of the pin that asks.
#[tauri::command]
pub fn capture_pin_image(app: AppHandle, window: WebviewWindow) -> Result<Response, String> {
    let png = app.state::<CaptureState>().pins.images.lock().unwrap().get(window.label()).cloned();
    png.map(Response::new).ok_or_else(|| "This pin has no picture.".to_string())
}

/// Wheel notches over a pin: positive grows it, negative shrinks it, around its top left corner.
#[tauri::command]
pub fn capture_pin_zoom(window: WebviewWindow, notches: f64) {
    if !window.label().starts_with(PREFIX) {
        return;
    }
    let Ok(size) = window.inner_size() else { return };
    let max_w = window.current_monitor().ok().flatten().map_or(4000, |m| m.size().width);
    let (w, h) = pin_size(size.width, size.height, notches, MIN_WIDTH, max_w);
    let _ = window.set_size(PhysicalSize::new(w, h));
}

#[tauri::command]
pub fn capture_pin_close(app: AppHandle, window: WebviewWindow) {
    if !window.label().starts_with(PREFIX) {
        return;
    }
    app.state::<CaptureState>().pins.images.lock().unwrap().remove(window.label());
    let _ = window.close();
}
