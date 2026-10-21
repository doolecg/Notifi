//! Screenshot and copy text from screen (spec/capture/capture-selector.md, screenshot-editor.md,
//! scrolling-capture.md, text-and-qr.md): a selector over a snapshot of the whole desktop, outputs
//! (clipboard, file, Shelf, editor, preview), scrolling capture and text recognition.

mod logic;
pub mod pin;
pub mod recent;
pub mod share;
mod stitch;
#[cfg_attr(windows, path = "win.rs")]
#[cfg_attr(not(windows), path = "other.rs")]
mod sys;

use std::{
    fs,
    io::Cursor,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use image::{
    codecs::{
        jpeg::JpegEncoder,
        png::{CompressionType, FilterType, PngEncoder},
        webp::WebPEncoder,
    },
    ExtendedColorType, ImageEncoder, ImageFormat, RgbaImage,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{
    ipc::{InvokeBody, Request, Response},
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

use crate::settings::{Settings, Store};
use logic::{clamp_crop, file_stem, join_text, resolve_target, shorten, topmost_index, unique_path, web_link, Frame, Rect, Target};
use stitch::{Step, Stitcher};

pub const OVERLAY: &str = "capture";
pub const PREVIEW: &str = "capturepreview";
pub const EDITOR: &str = "captureedit";
pub const ACTIONS: &[&str] = &["captureScreenshot", "captureText", "captureRecent", "captureUpload"];

const START_EVENT: &str = "capture://start";
const PREVIEW_EVENT: &str = "capture://preview";
const SCROLL_EVENT: &str = "capture://scroll";
const EDITOR_EVENT: &str = "capture://editor";
const VK_ESCAPE: i32 = 0x1B;
const VK_RETURN: i32 = 0x0D;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Screenshot,
    Text,
}

/// The latest capture, kept in memory for the preview, the editor and copy-text-from-preview.
struct Last {
    img: RgbaImage,
    /// Display scale where it was taken, for the 1x size option.
    scale: f64,
    saved: Mutex<Option<PathBuf>>,
}

struct Session {
    mode: Mode,
    frame: Arc<Frame>,
    windows: Vec<Rect>,
    /// Window handles in the same order as `windows`.
    hwnds: Vec<isize>,
    monitors: Vec<Rect>,
    freeze: bool,
}

#[derive(Default)]
pub struct CaptureState {
    /// A selector, scrolling capture or recognition is running.
    busy: AtomicBool,
    scrolling: AtomicBool,
    stop_scroll: AtomicBool,
    /// The selector window can be kept out of screen grabs.
    hideable: AtomicBool,
    shown: AtomicBool,
    session: Mutex<Option<Session>>,
    last: Mutex<Option<Arc<Last>>>,
    last_rect: Mutex<Option<Rect>>,
    link: Mutex<Option<String>>,
    /// Moves on when the capture is replaced or edited, so a late upload does not copy its link.
    share_gen: AtomicU64,
    pins: pin::Pins,
}

// ---- Options ----

fn raw<'a>(s: &'a Settings, ns: &str, key: &str) -> Option<&'a Value> {
    s.option(ns, key)
}

fn flag(s: &Settings, ns: &str, key: &str, default: bool) -> bool {
    raw(s, ns, key).and_then(Value::as_bool).unwrap_or(default)
}

/// A number stored as a number or as the text of a pick.
fn number(s: &Settings, ns: &str, key: &str, default: f64) -> f64 {
    match raw(s, ns, key) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(default),
        Some(Value::String(t)) => t.trim().parse().unwrap_or(default),
        _ => default,
    }
}

fn text(s: &Settings, ns: &str, key: &str, default: &str) -> String {
    raw(s, ns, key).and_then(Value::as_str).unwrap_or(default).to_string()
}

fn items(s: &Settings, ns: &str, key: &str) -> Vec<String> {
    match raw(s, ns, key) {
        Some(Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        Some(Value::String(t)) => t.split([',', '\n']).map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

#[derive(Clone)]
struct Opts {
    default_mode: String,
    show_menu: bool,
    freeze: bool,
    outline: bool,
    magnifier: bool,
    magnification: u32,
    auto_copy: bool,
    auto_shelf: bool,
    auto_editor: bool,
    auto_pin: bool,
    shelf_auto: bool,
    preview_secs: i32,
    save_1x: bool,
    include_pill: bool,
    folder: String,
    name_template: String,
    format: String,
    quality: u8,
    text_size: u32,
    blur: u32,
    watermark: String,
    scroll_auto: bool,
    scroll_speed: String,
    max_height: usize,
    languages: Vec<String>,
    join_lines: bool,
    keep_breaks: bool,
    show_toast: bool,
    open_qr: bool,
}

impl Opts {
    fn read(app: &AppHandle) -> Self {
        let s = app.state::<Store>().get();
        let (p, t) = ("screenshot", "textFromScreen");
        Self {
            default_mode: text(&s, p, "defaultMode", "screenshot"),
            show_menu: flag(&s, p, "showModeMenu", true),
            freeze: flag(&s, p, "freezeScreen", true),
            outline: flag(&s, p, "showLastOutline", true),
            magnifier: flag(&s, p, "magnifier", true),
            magnification: number(&s, p, "magnification", 8.0).clamp(2.0, 16.0) as u32,
            auto_copy: flag(&s, p, "autoCopy", true),
            auto_shelf: flag(&s, p, "autoShelf", false),
            auto_editor: flag(&s, p, "autoEditor", false),
            auto_pin: flag(&s, p, "autoPin", false),
            shelf_auto: s.enabled("shelf") && flag(&s, "shelf", "autoAddCaptures", false),
            preview_secs: number(&s, p, "previewDuration", 5.0) as i32,
            save_1x: flag(&s, p, "saveAt1x", false),
            include_pill: flag(&s, p, "includePillInShots", false),
            folder: text(&s, p, "folder", ""),
            name_template: text(&s, p, "nameTemplate", "Screenshot {date} {time}"),
            format: text(&s, p, "format", "png"),
            quality: number(&s, p, "quality", 90.0).clamp(10.0, 100.0) as u8,
            text_size: number(&s, p, "textSize", 24.0).clamp(8.0, 96.0) as u32,
            blur: number(&s, p, "blurStrength", 12.0).clamp(1.0, 50.0) as u32,
            watermark: text(&s, p, "watermark", ""),
            scroll_auto: text(&s, p, "scrollMode", "auto") != "manual",
            scroll_speed: text(&s, p, "scrollSpeed", "medium"),
            max_height: number(&s, p, "maxHeight", 30000.0).max(500.0) as usize,
            languages: items(&s, t, "languages"),
            join_lines: flag(&s, t, "joinLines", false),
            keep_breaks: flag(&s, t, "keepLineBreaks", true),
            show_toast: flag(&s, t, "showToast", true),
            open_qr: flag(&s, t, "openQrLinks", false),
        }
    }
}

fn notice(app: &AppHandle, text: impl Into<String>) {
    crate::island::notice(app, text.into());
}

// ---- Entry points ----

pub fn available() -> bool {
    sys::available()
}

pub fn run_action(app: &AppHandle, action: &str) {
    match action {
        "captureText" => start(app, Mode::Text),
        "captureRecent" => recent::open(app),
        "captureUpload" => match last_shot(app) {
            Some(last) => share::share_shot(app, &last),
            None => notice(app, "There is no screenshot to share yet."),
        },
        _ => match Opts::read(app).default_mode.as_str() {
            "text" => start(app, Mode::Text),
            "color" => crate::picker::toggle(app),
            _ => start(app, Mode::Screenshot),
        },
    }
}

/// Opens the selector, or closes it when it is already open. A second press during a scrolling
/// capture ends it.
pub fn start(app: &AppHandle, mode: Mode) {
    let state = app.state::<CaptureState>();
    if state.scrolling.load(Ordering::SeqCst) {
        state.stop_scroll.store(true, Ordering::SeqCst);
        return;
    }
    if state.session.lock().unwrap().is_some() {
        cancel(app);
        return;
    }
    if !sys::available() {
        notice(app, "Screenshots and text from screen are only built for Windows so far.");
        return;
    }
    if state.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    let _ = std::thread::Builder::new().name("notifi-capture".into()).spawn(move || {
        if !open_selector(&app, mode) {
            app.state::<CaptureState>().busy.store(false, Ordering::SeqCst);
        }
    });
}

fn cancel(app: &AppHandle) {
    let state = app.state::<CaptureState>();
    if state.session.lock().unwrap().take().is_some() {
        state.busy.store(false, Ordering::SeqCst);
    }
    hide_overlay(app);
}

fn hide_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(OVERLAY) {
        let _ = window.hide();
    }
}

#[cfg(windows)]
fn hwnd_of(app: &AppHandle, label: &str) -> Option<isize> {
    app.get_window(label)?.hwnd().ok().map(|h| h.0 as isize)
}

#[cfg(not(windows))]
fn hwnd_of(_app: &AppHandle, _label: &str) -> Option<isize> {
    None
}

/// Runs `f` with the Top Pill kept out of screen grabs unless shots should include it.
fn grab_with_pill<T>(app: &AppHandle, o: &Opts, f: impl FnOnce() -> T) -> T {
    let pill = if o.include_pill { None } else { hwnd_of(app, crate::island::ISLAND) };
    let hidden = pill.is_some_and(|h| sys::set_excluded(h, true));
    let out = f();
    if let (true, Some(h)) = (hidden, pill) {
        sys::set_excluded(h, false);
    }
    out
}

fn monitor_rects(app: &AppHandle) -> Vec<Rect> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| Rect::new(m.position().x, m.position().y, m.size().width as i32, m.size().height as i32))
        .collect()
}

fn scale_at(app: &AppHandle, r: &Rect) -> f64 {
    app.monitor_from_point(f64::from(r.x + r.w / 2), f64::from(r.y + r.h / 2))
        .ok()
        .flatten()
        .map_or(1.0, |m| m.scale_factor())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Start {
    mode: Mode,
    width: i32,
    height: i32,
    windows: Vec<Rect>,
    monitors: Vec<Rect>,
    show_menu: bool,
    freeze: bool,
    magnifier: bool,
    magnification: u32,
    outline: Option<Rect>,
}

/// Takes the snapshot and brings the selector up. False when nothing could be started.
fn open_selector(app: &AppHandle, mode: Mode) -> bool {
    let o = Opts::read(app);
    let screen = sys::virtual_screen();
    let Some(frame) = grab_with_pill(app, &o, || sys::grab(&screen)) else {
        notice(app, "Notifi could not read the screen.");
        return false;
    };
    let mut skip = Vec::new();
    skip.extend(hwnd_of(app, OVERLAY));
    skip.extend(hwnd_of(app, PREVIEW));
    let local = |r: &Rect| r.offset(-screen.x, -screen.y);
    let (hwnds, windows): (Vec<isize>, Vec<Rect>) = sys::top_windows(&skip).into_iter().unzip();
    let monitors = monitor_rects(app);
    let state = app.state::<CaptureState>();
    let outline = if o.outline { *state.last_rect.lock().unwrap() } else { None };
    let payload = Start {
        mode,
        width: screen.w,
        height: screen.h,
        windows: windows.iter().map(local).collect(),
        monitors: monitors.iter().map(local).collect(),
        show_menu: o.show_menu,
        freeze: o.freeze,
        magnifier: o.magnifier,
        magnification: o.magnification,
        outline: outline.map(|r| local(&r)),
    };
    *state.session.lock().unwrap() = Some(Session { mode, frame: Arc::new(frame), windows, hwnds, monitors, freeze: o.freeze });
    state.shown.store(false, Ordering::SeqCst);
    let Some(window) = app.get_webview_window(OVERLAY) else {
        state.session.lock().unwrap().take();
        return false;
    };
    let _ = window.set_position(PhysicalPosition::new(screen.x, screen.y));
    let _ = window.set_size(PhysicalSize::new(screen.w as u32, screen.h as u32));
    let _ = window.set_position(PhysicalPosition::new(screen.x, screen.y));
    let _ = app.emit_to(OVERLAY, START_EVENT, payload);
    // The page tells us when its snapshot is drawn; show anyway if it never does.
    std::thread::sleep(Duration::from_millis(2000));
    if !state.shown.load(Ordering::SeqCst) && state.session.lock().unwrap().is_some() {
        show_overlay(app);
    }
    true
}

fn show_overlay(app: &AppHandle) {
    app.state::<CaptureState>().shown.store(true, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window(OVERLAY) {
        let _ = window.show();
        if !crate::window::no_focus() {
            let _ = window.set_focus();
        }
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    if !sys::available() {
        return Ok(());
    }
    WebviewWindowBuilder::new(app, OVERLAY, WebviewUrl::App("capture.html#select".into()))
        .title("Notifi Capture")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .visible(false)
        .focused(false)
        .build()?;
    WebviewWindowBuilder::new(app, PREVIEW, WebviewUrl::App("capture.html#preview".into()))
        .title("Notifi Capture Preview")
        .inner_size(340.0, 252.0)
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
    let hideable = hwnd_of(app, OVERLAY).is_some_and(|h| sys::set_excluded(h, true));
    app.state::<CaptureState>().hideable.store(hideable, Ordering::SeqCst);
    if let Some(h) = hwnd_of(app, PREVIEW) {
        sys::set_excluded(h, true);
    }
    Ok(())
}

// ---- Selector commands ----

/// The snapshot as 8 bytes (width, height, little endian) followed by RGBA rows.
#[tauri::command]
pub async fn capture_frame(app: AppHandle) -> Result<Response, String> {
    let frame = {
        let state = app.state::<CaptureState>();
        let session = state.session.lock().unwrap();
        session.as_ref().map(|s| s.frame.clone()).ok_or("No capture is running.")?
    };
    let mut out = Vec::with_capacity(frame.rgba.len() + 8);
    out.extend_from_slice(&frame.w.to_le_bytes());
    out.extend_from_slice(&frame.h.to_le_bytes());
    out.extend_from_slice(&frame.rgba);
    Ok(Response::new(out))
}

#[tauri::command]
pub fn capture_ready(app: AppHandle) {
    if app.state::<CaptureState>().session.lock().unwrap().is_some() {
        show_overlay(&app);
    }
}

#[tauri::command]
pub fn capture_cancel(app: AppHandle) {
    cancel(&app);
}

#[tauri::command]
pub fn capture_available() -> bool {
    sys::available()
}

/// Opens the selector in a mode: `screenshot` or `text`.
#[tauri::command]
pub fn capture_start(app: AppHandle, mode: Option<String>) {
    start(&app, if mode.as_deref() == Some("text") { Mode::Text } else { Mode::Screenshot });
}

/// Hands over to the colour picker, which takes the pointer itself.
#[tauri::command]
pub fn capture_color(app: AppHandle) {
    cancel(&app);
    crate::picker::toggle(&app);
}

/// Moves the pointer by whole pixels (arrow keys in the selector).
#[tauri::command]
pub fn capture_nudge(dx: i32, dy: i32) {
    let (x, y) = sys::cursor();
    sys::set_cursor(x + dx, y + dy);
}

/// Copies the colour of the pixel at a point of the selector window.
#[tauri::command]
pub fn capture_copy_pixel(app: AppHandle, x: i32, y: i32) -> Result<String, String> {
    let rgb = {
        let state = app.state::<CaptureState>();
        let session = state.session.lock().unwrap();
        let frame = &session.as_ref().ok_or("No capture is running.")?.frame;
        frame.pixel(x + frame.origin.0, y + frame.origin.1).ok_or("That point is off the screen.")?
    };
    let s = app.state::<Store>().get();
    let colour = crate::picker::format_color(
        rgb,
        &text(&s, "colorPicker", "format", "hex"),
        flag(&s, "colorPicker", "hexNoHash", false),
        flag(&s, "colorPicker", "hexUppercase", true),
    );
    crate::clipboard::set_text(&colour)?;
    Ok(colour)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finish {
    target: Target,
    scrolling: bool,
}

#[tauri::command]
pub fn capture_finish(app: AppHandle, finish: Finish) {
    let _ = std::thread::Builder::new().name("notifi-capture-run".into()).spawn(move || {
        run_finish(&app, finish);
        app.state::<CaptureState>().busy.store(false, Ordering::SeqCst);
    });
}

fn run_finish(app: &AppHandle, finish: Finish) {
    let state = app.state::<CaptureState>();
    let Some(session) = state.session.lock().unwrap().take() else { return };
    hide_overlay(app);
    let o = Opts::read(app);
    let Some(rect) = resolve_target(finish.target, session.frame.origin, &session.windows, &session.monitors) else { return };
    let rect = match rect.intersect(&session.frame.bounds()) {
        Some(r) => r,
        None => return,
    };
    *state.last_rect.lock().unwrap() = Some(rect);
    // A window grab asks the window to draw itself, so windows lying over it stay out of the shot.
    let own = match finish.target {
        Target::Window { x, y } if session.mode == Mode::Screenshot && !finish.scrolling => {
            let (ox, oy) = session.frame.origin;
            topmost_index(&session.windows, x.floor() as i32 + ox, y.floor() as i32 + oy)
                .and_then(|i| session.hwnds.get(i).copied())
                .and_then(|h| sys::grab_window(h, &rect))
        }
        _ => None,
    };
    let img = if own.is_some() {
        own
    } else if session.freeze {
        session.frame.crop(&rect)
    } else {
        if !state.hideable.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(160));
        }
        grab_with_pill(app, &o, || sys::grab(&rect)).and_then(|f| f.crop(&rect))
    };
    let Some(img) = img else {
        notice(app, "Notifi could not read that part of the screen.");
        return;
    };
    let scale = scale_at(app, &rect);
    match (session.mode, finish.scrolling) {
        (Mode::Text, _) => read_text(app, &o, img),
        (Mode::Screenshot, true) => {
            if let Some(tall) = scroll_capture(app, &o, rect, img) {
                deliver(app, &o, tall, scale);
            }
        }
        (Mode::Screenshot, false) => deliver(app, &o, img, scale),
    }
}

// ---- Outputs ----

fn png_fast(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::NoFilter)
        .write_image(img.as_raw(), img.width(), img.height(), ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// The image as it leaves Notifi: shrunk to 1x for files when asked, with the watermark on.
fn export(o: &Opts, img: &RgbaImage, scale: f64, for_file: bool) -> RgbaImage {
    let mut out = if for_file && o.save_1x && scale > 1.01 {
        let (w, h) = (((f64::from(img.width()) / scale).round() as u32).max(1), ((f64::from(img.height()) / scale).round() as u32).max(1));
        image::imageops::resize(img, w, h, image::imageops::FilterType::Lanczos3)
    } else {
        img.clone()
    };
    let mark = o.watermark.trim();
    if !mark.is_empty() {
        sys::watermark(&mut out, mark);
    }
    out
}

fn encode(img: &RgbaImage, format: &str, quality: u8) -> Result<(Vec<u8>, &'static str), String> {
    let (w, h) = img.dimensions();
    let mut out = Vec::new();
    match format {
        "jpeg" => {
            let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
            JpegEncoder::new_with_quality(&mut out, quality)
                .write_image(rgb.as_raw(), w, h, ExtendedColorType::Rgb8)
                .map_err(|e| e.to_string())?;
            Ok((out, "jpg"))
        }
        "webp" => {
            WebPEncoder::new_lossless(&mut out).write_image(img.as_raw(), w, h, ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
            Ok((out, "webp"))
        }
        _ => {
            img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png).map_err(|e| e.to_string())?;
            Ok((out, "png"))
        }
    }
}

fn copy_image(o: &Opts, last: &Last) -> Result<(), String> {
    let png = png_fast(&export(o, &last.img, last.scale, false))?;
    crate::clipboard::set_png(&png)
}

fn save_file(app: &AppHandle, o: &Opts, last: &Last) -> Result<PathBuf, String> {
    let dir = match o.folder.trim() {
        "" => app.path().picture_dir().map_err(|e| e.to_string())?,
        folder => PathBuf::from(folder),
    };
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let (bytes, ext) = encode(&export(o, &last.img, last.scale, true), &o.format, o.quality)?;
    let path = unique_path(&dir, &file_stem(&o.name_template, &sys::local_time()), ext, |p| p.exists());
    fs::write(&path, bytes).map_err(|e| format!("Could not save {}: {e}", path.display()))?;
    *last.saved.lock().unwrap() = Some(path.clone());
    recent::remember(app, &path, "shot");
    Ok(path)
}

fn file_label(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn last_shot(app: &AppHandle) -> Option<Arc<Last>> {
    app.state::<CaptureState>().last.lock().unwrap().clone()
}

/// What happens to a finished capture: the automatic outputs, then the editor or the preview.
fn deliver(app: &AppHandle, o: &Opts, img: RgbaImage, scale: f64) {
    let last = Arc::new(Last { img, scale, saved: Mutex::new(None) });
    share::cancel_pending(app);
    *app.state::<CaptureState>().last.lock().unwrap() = Some(last.clone());
    let mut copied = false;
    let mut notes: Vec<String> = Vec::new();
    if o.auto_copy {
        match copy_image(o, &last) {
            Ok(()) => copied = true,
            Err(e) => notes.push(format!("Could not copy the screenshot: {e}")),
        }
    }
    if o.auto_shelf || o.shelf_auto {
        match save_file(app, o, &last) {
            Ok(path) => crate::clipboard::shelf::add(app, &[path]),
            Err(e) => notes.push(e),
        }
    }
    for n in &notes {
        notice(app, n.clone());
    }
    if o.auto_pin {
        pin::pin_last(app);
    }
    if o.auto_editor {
        open_editor(app);
    } else if o.preview_secs != 0 {
        show_preview(app, o, &last);
    } else if copied {
        notice(app, "Screenshot copied");
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PreviewData {
    kind: &'static str,
    seconds: i32,
    auto_copy: bool,
    width: u32,
    height: u32,
    text: String,
    link: Option<String>,
    share: bool,
}

fn place_preview(app: &AppHandle, logical: (f64, f64)) {
    let Some(window) = app.get_webview_window(PREVIEW) else { return };
    let (cx, cy) = sys::cursor();
    let monitor = app
        .monitor_from_point(f64::from(cx), f64::from(cy))
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(m) = monitor else { return };
    let s = m.scale_factor();
    let (pw, ph) = ((logical.0 * s).round() as i32, (logical.1 * s).round() as i32);
    let x = m.position().x + m.size().width as i32 - pw - (24.0 * s) as i32;
    let y = m.position().y + m.size().height as i32 - ph - (72.0 * s) as i32;
    let _ = window.set_size(PhysicalSize::new(pw as u32, ph as u32));
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn show_card(app: &AppHandle, logical: (f64, f64), data: PreviewData) {
    place_preview(app, logical);
    let _ = app.emit_to(PREVIEW, PREVIEW_EVENT, data);
    if let Some(window) = app.get_webview_window(PREVIEW) {
        let _ = window.show();
    }
}

fn show_preview(app: &AppHandle, o: &Opts, last: &Last) {
    let data = PreviewData {
        kind: "shot",
        seconds: o.preview_secs,
        auto_copy: o.auto_copy,
        width: last.img.width(),
        height: last.img.height(),
        text: String::new(),
        link: None,
        share: share::available(app),
    };
    show_card(app, (340.0, 252.0), data);
}

fn hide_preview(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(PREVIEW) {
        let _ = window.hide();
    }
}

/// A scaled-down PNG of the latest capture for the preview.
#[tauri::command]
pub async fn capture_thumb(app: AppHandle) -> Result<Response, String> {
    let last = last_shot(&app).ok_or("There is no capture yet.")?;
    let (w, h) = last.img.dimensions();
    let fit = (640.0 / f64::from(w)).min(400.0 / f64::from(h)).min(1.0);
    let small = image::imageops::resize(
        &last.img,
        ((f64::from(w) * fit).round() as u32).max(1),
        ((f64::from(h) * fit).round() as u32).max(1),
        image::imageops::FilterType::Triangle,
    );
    Ok(Response::new(png_fast(&small)?))
}

/// A button on the preview: copy, save, shelf, edit, pin, text, open-link, stop or dismiss.
#[tauri::command]
pub fn capture_action(app: AppHandle, action: String) {
    let _ = std::thread::Builder::new().name("notifi-capture-action".into()).spawn(move || {
        let o = Opts::read(&app);
        match action.as_str() {
            "dismiss" => hide_preview(&app),
            "stop" => app.state::<CaptureState>().stop_scroll.store(true, Ordering::SeqCst),
            "open-link" => {
                if let Some(link) = app.state::<CaptureState>().link.lock().unwrap().clone() {
                    sys::open_url(&link);
                }
                hide_preview(&app);
            }
            "edit" => {
                hide_preview(&app);
                open_editor(&app);
            }
            "pin" => {
                hide_preview(&app);
                pin::pin_last(&app);
            }
            "share" => {
                hide_preview(&app);
                if let Some(last) = last_shot(&app) {
                    share::share_shot(&app, &last);
                }
            }
            "copy" | "save" | "shelf" | "text" => {
                let Some(last) = last_shot(&app) else { return };
                let done = match action.as_str() {
                    "copy" => copy_image(&o, &last).map(|_| "Screenshot copied".to_string()),
                    "save" => save_file(&app, &o, &last).and_then(|p| {
                        if o.auto_copy {
                            copy_image(&o, &last)?;
                        }
                        Ok(format!("Saved {}", file_label(&p)))
                    }),
                    "shelf" => save_file(&app, &o, &last).map(|p| {
                        crate::clipboard::shelf::add(&app, &[p]);
                        "Added to the Shelf".to_string()
                    }),
                    _ => {
                        read_text(&app, &o, last.img.clone());
                        Ok(String::new())
                    }
                };
                match done {
                    Ok(message) if !message.is_empty() => notice(&app, message),
                    Ok(_) => {}
                    Err(e) => notice(&app, e),
                }
                if action != "copy" {
                    hide_preview(&app);
                }
            }
            _ => {}
        }
    });
}

// ---- Editor ----

fn open_editor(app: &AppHandle) {
    let Some(last) = last_shot(app) else { return };
    share::cancel_pending(app);
    if let Some(window) = app.get_webview_window(EDITOR) {
        let _ = app.emit_to(EDITOR, EDITOR_EVENT, ());
        let _ = window.unminimize();
        let _ = window.show();
        if !crate::window::no_focus() {
            let _ = window.set_focus();
        }
        return;
    }
    let area = app.primary_monitor().ok().flatten().map(|m| {
        let s = m.scale_factor();
        (f64::from(m.size().width) / s, f64::from(m.size().height) / s)
    });
    let (mw, mh) = area.unwrap_or((1600.0, 900.0));
    let s = app.primary_monitor().ok().flatten().map_or(1.0, |m| m.scale_factor());
    let width = (f64::from(last.img.width()) / s + 48.0).clamp(640.0, mw * 0.9);
    let height = (f64::from(last.img.height()) / s + 120.0).clamp(480.0, mh * 0.9);
    // Building a webview from a command can deadlock on Windows, so build off-thread.
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = WebviewWindowBuilder::new(&app, EDITOR, WebviewUrl::App("capture.html#editor".into()))
            .title("Notifi Screenshot Editor")
            .inner_size(width, height)
            .min_inner_size(560.0, 420.0)
            .center()
            // Dropped image files go to the page (image layers), not to the window's drag-drop events.
            .disable_drag_drop_handler()
            .focused(!crate::window::no_focus())
            .build();
    });
}

#[tauri::command]
pub fn capture_close_editor(app: AppHandle) {
    if let Some(window) = app.get_webview_window(EDITOR) {
        let _ = window.close();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorOpts {
    text_size: u32,
    blur: u32,
    share: bool,
}

#[tauri::command]
pub fn capture_editor_opts(app: AppHandle) -> EditorOpts {
    let o = Opts::read(&app);
    EditorOpts { text_size: o.text_size, blur: o.blur, share: share::available(&app) }
}

/// Boxes around the words found in a part of the editor image (x, y, width, height in image pixels),
/// for the text-only blur.
#[tauri::command]
pub async fn capture_editor_text_boxes(app: AppHandle, x: f64, y: f64, w: f64, h: f64) -> Result<Vec<[f64; 4]>, String> {
    let last = last_shot(&app).ok_or("There is no capture to read.")?;
    let (cx, cy, cw, ch) = clamp_crop(&format!("{x},{y},{w},{h}"), last.img.width(), last.img.height()).ok_or("That area is outside the picture.")?;
    let part = image::imageops::crop_imm(&last.img, cx, cy, cw, ch).to_image();
    let languages = Opts::read(&app).languages;
    let boxes = sys::ocr_boxes(&part, &languages)?;
    Ok(boxes.into_iter().map(|b| [b[0] + f64::from(cx), b[1] + f64::from(cy), b[2], b[3]]).collect())
}

#[tauri::command]
pub async fn capture_editor_image(app: AppHandle) -> Result<Response, String> {
    let last = last_shot(&app).ok_or("There is no capture to edit.")?;
    Ok(Response::new(png_fast(&last.img)?))
}

/// The edited image as PNG bytes, with the wanted action in the `action` header: copy, save, shelf.
#[tauri::command]
pub fn capture_editor_export(app: AppHandle, request: Request<'_>) -> Result<String, String> {
    let InvokeBody::Raw(bytes) = request.body() else { return Err("The editor sent no image.".into()) };
    let action = request.headers().get("action").and_then(|v| v.to_str().ok()).unwrap_or("copy").to_string();
    let mut img = image::load_from_memory_with_format(bytes, ImageFormat::Png).map_err(|e| e.to_string())?.to_rgba8();
    if let Some((x, y, w, h)) = request.headers().get("crop").and_then(|v| v.to_str().ok()).and_then(|v| clamp_crop(v, img.width(), img.height())) {
        img = image::imageops::crop_imm(&img, x, y, w, h).to_image();
    }
    let o = Opts::read(&app);
    let scale = last_shot(&app).map_or(1.0, |l| l.scale);
    let last = Arc::new(Last { img, scale, saved: Mutex::new(None) });
    share::cancel_pending(&app);
    *app.state::<CaptureState>().last.lock().unwrap() = Some(last.clone());
    match action.as_str() {
        "pin" => {
            pin::pin_last(&app);
            Ok("Pinned".into())
        }
        "share" => {
            share::share_shot(&app, &last);
            Ok(String::new())
        }
        "save" => save_file(&app, &o, &last).map(|p| format!("Saved {}", file_label(&p))),
        "shelf" => save_file(&app, &o, &last).map(|p| {
            crate::clipboard::shelf::add(&app, &[p]);
            "Added to the Shelf".to_string()
        }),
        _ => copy_image(&o, &last).map(|_| "Screenshot copied".to_string()),
    }
}

// ---- Scrolling capture ----

fn scroll_settle(speed: &str) -> Duration {
    Duration::from_millis(match speed {
        "slow" => 380,
        "fast" => 170,
        _ => 260,
    })
}

/// Share of the region height to move per step.
fn scroll_fraction(speed: &str) -> f64 {
    match speed {
        "slow" => 0.3,
        "fast" => 0.7,
        _ => 0.5,
    }
}

/// Wheel notches per step once one notch has been measured in pixels.
fn notches_for(region_height: i32, px_per_notch: f64, speed: &str) -> i32 {
    if px_per_notch < 1.0 {
        return 1;
    }
    ((f64::from(region_height) * scroll_fraction(speed) / px_per_notch).round() as i32).max(1)
}

struct ScrollGuard<'a>(&'a AppHandle);

impl Drop for ScrollGuard<'_> {
    fn drop(&mut self) {
        let state = self.0.state::<CaptureState>();
        state.scrolling.store(false, Ordering::SeqCst);
        state.stop_scroll.store(false, Ordering::SeqCst);
        hide_preview(self.0);
    }
}

/// Follows a scrolling region, frame by frame, and joins the frames. Returns what was gathered up
/// to the end of the content, Stop, Esc or Enter, or the maximum height.
fn scroll_capture(app: &AppHandle, o: &Opts, rect: Rect, first: RgbaImage) -> Option<RgbaImage> {
    let state = app.state::<CaptureState>();
    state.scrolling.store(true, Ordering::SeqCst);
    state.stop_scroll.store(false, Ordering::SeqCst);
    let _guard = ScrollGuard(app);
    let hint = if o.scroll_auto { "Scrolling for you. Esc or Enter stops." } else { "Scroll the page yourself. Esc or Enter stops." };
    show_card(app, (340.0, 150.0), PreviewData { kind: "scroll", seconds: -1, auto_copy: o.auto_copy, width: 0, height: first.height(), text: hint.into(), link: None, share: false });
    let armed = Instant::now();
    let before = sys::cursor();
    if o.scroll_auto {
        sys::set_cursor(rect.x + rect.w / 2, rect.y + rect.h / 2);
    }
    let mut stitcher = Stitcher::new(first);
    let mut notches = 1;
    let mut capped = false;
    let mut previous_raw: Option<RgbaImage> = None;
    let stopped = |armed: Instant| {
        state.stop_scroll.load(Ordering::SeqCst)
            || (armed.elapsed() > Duration::from_millis(500) && (sys::key_down(VK_ESCAPE) || sys::key_down(VK_RETURN)))
    };
    let grab_region = || sys::grab(&rect).and_then(|f| f.crop(&rect));
    'frames: loop {
        if stopped(armed) {
            break;
        }
        if o.scroll_auto {
            for _ in 0..notches {
                sys::wheel(-1);
                std::thread::sleep(Duration::from_millis(12));
            }
            std::thread::sleep(scroll_settle(&o.scroll_speed));
        } else {
            std::thread::sleep(Duration::from_millis(150));
        }
        // Wait for the picture to stop moving before taking the frame.
        let mut frame = match grab_region() {
            Some(f) => f,
            None => break,
        };
        for _ in 0..6 {
            if !o.scroll_auto && previous_raw.as_ref().is_some_and(|p| p.as_raw() == frame.as_raw()) {
                continue 'frames;
            }
            std::thread::sleep(Duration::from_millis(90));
            match grab_region() {
                Some(again) if again.as_raw() == frame.as_raw() => break,
                Some(again) => frame = again,
                None => break 'frames,
            }
        }
        previous_raw = Some(frame.clone());
        match stitcher.push(frame) {
            Step::Same | Step::Lost => {
                if o.scroll_auto {
                    break;
                }
            }
            Step::Added(rows) => {
                if o.scroll_auto {
                    notches = notches_for(rect.h, rows as f64 / f64::from(notches), &o.scroll_speed);
                }
                let _ = app.emit_to(PREVIEW, SCROLL_EVENT, stitcher.height());
                if stitcher.height() >= o.max_height {
                    capped = true;
                    break;
                }
            }
        }
    }
    if o.scroll_auto {
        sys::set_cursor(before.0, before.1);
    }
    if capped {
        notice(app, format!("Scrolling capture stopped at {} px, the maximum height.", o.max_height));
    }
    let mut tall = stitcher.finish()?;
    if tall.height() as usize > o.max_height {
        tall = image::imageops::crop_imm(&tall, 0, 0, tall.width(), o.max_height as u32).to_image();
    }
    Some(tall)
}

// ---- Text and QR ----

fn qr_texts(img: &RgbaImage) -> Vec<String> {
    let mut grey = image::imageops::grayscale(img);
    let long = grey.width().max(grey.height());
    if long > 1800 {
        let f = 1800.0 / f64::from(long);
        grey = image::imageops::resize(
            &grey,
            ((f64::from(grey.width()) * f).round() as u32).max(1),
            ((f64::from(grey.height()) * f).round() as u32).max(1),
            image::imageops::FilterType::Triangle,
        );
    }
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(grey.width() as usize, grey.height() as usize, |x, y| grey.get_pixel(x as u32, y as u32)[0]);
    let mut found: Vec<String> = Vec::new();
    for grid in prepared.detect_grids() {
        if let Ok((_, content)) = grid.decode() {
            if !found.contains(&content) {
                found.push(content);
            }
        }
    }
    found
}

/// Decodes a QR code in the image, else recognises its text, and copies the result.
fn read_text(app: &AppHandle, o: &Opts, img: RgbaImage) {
    let state = app.state::<CaptureState>();
    let codes = qr_texts(&img);
    if let Some(first) = codes.first() {
        if let Err(e) = crate::clipboard::set_text(first) {
            notice(app, format!("Could not copy the code: {e}"));
            return;
        }
        let more = if codes.len() > 1 { format!(" (+{} more)", codes.len() - 1) } else { String::new() };
        if o.show_toast {
            notice(app, format!("QR code copied: {}{more}", shorten(first, 60)));
        }
        if let Some(link) = web_link(first) {
            if o.open_qr {
                if sys::confirm_open(&link) {
                    sys::open_url(&link);
                }
            } else if o.show_toast {
                *state.link.lock().unwrap() = Some(link.clone());
                let data = PreviewData { kind: "link", seconds: 8, auto_copy: false, width: 0, height: 0, text: link.clone(), link: Some(link), share: false };
                show_card(app, (340.0, 160.0), data);
            }
        }
        return;
    }
    match sys::ocr_lines(&img, &o.languages) {
        Ok(lines) => {
            let result = join_text(&lines, o.join_lines, o.keep_breaks);
            if result.trim().is_empty() {
                notice(app, "No text found in that area.");
            } else if let Err(e) = crate::clipboard::set_text(&result) {
                notice(app, format!("Could not copy the text: {e}"));
            } else if o.show_toast {
                notice(app, format!("Copied text: {}", shorten(&result, 70)));
            }
        }
        Err(e) => notice(app, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_steps_follow_speed_and_measured_notch() {
        assert_eq!(notches_for(800, 100.0, "medium"), 4);
        assert_eq!(notches_for(800, 100.0, "slow"), 2);
        assert_eq!(notches_for(800, 100.0, "fast"), 6);
        assert_eq!(notches_for(200, 100.0, "slow"), 1);
        assert_eq!(notches_for(800, 0.0, "fast"), 1);
    }

    #[test]
    fn slower_speeds_wait_longer() {
        assert!(scroll_settle("slow") > scroll_settle("medium"));
        assert!(scroll_settle("medium") > scroll_settle("fast"));
    }
}
