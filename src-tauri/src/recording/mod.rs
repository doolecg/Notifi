//! Screen recording (spec/capture/screen-recording.md): choose an area, window or display, record
//! it with system sound and the microphone, draw pointer and click effects into the frames, zoom
//! toward the caret while typing, and save an MP4 (or GIF) with an Island notice on finish.
//!
//! The capture, sound and encoding run on Windows only. Two small windows carry the interface:
//! `recordpick` (the full-display overlay for choosing the area) and `recordbar` (timer, pause and
//! stop). The bar is hidden from the recording itself.

mod edit;
mod pure;
pub mod trim;

#[cfg(windows)]
mod audio;
#[cfg(windows)]
mod capture;
#[cfg(windows)]
mod encode;
#[cfg(windows)]
mod export;
#[cfg(windows)]
mod session;

use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Instant,
};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

use crate::settings::{Settings, Store};
use pure::{fps_from, ActiveClock, Region};

pub const BAR: &str = "recordbar";
pub const PICK: &str = "recordpick";
pub const TOGGLE: &str = "recordingToggle";
pub const PAUSE: &str = "recordingPause";
pub const ACTIONS: &[&str] = &[TOGGLE, PAUSE];
const NS: &str = "screenRecording";
const STATE_EVENT: &str = "recording://state";
const PICK_EVENT: &str = "recording://pick";
/// Logical size of the control bar.
const BAR_SIZE: (f64, f64) = (420.0, 56.0);
/// Distance of the bar from the bottom of the display, in logical pixels.
const BAR_LIFT: f64 = 96.0;
/// Smallest area worth recording, in pixels.
const MIN_AREA: i32 = 16;

pub fn available() -> bool {
    cfg!(windows)
}

#[derive(Default, Clone, Copy, PartialEq)]
pub(crate) enum Phase {
    #[default]
    Idle,
    Picking,
    Countdown,
    Recording,
    Paused,
    Saving,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Picking => "picking",
            Phase::Countdown => "countdown",
            Phase::Recording => "recording",
            Phase::Paused => "paused",
            Phase::Saving => "saving",
        }
    }
}

/// One recording from the end of the countdown to the file being closed.
pub(crate) struct Session {
    pub stop: AtomicBool,
    pub clock: Mutex<ActiveClock>,
}

#[derive(Default)]
struct Inner {
    phase: Phase,
    session: Option<Arc<Session>>,
    /// The last finished file, for the bar's Open folder button.
    saved: Option<PathBuf>,
    /// The recording the trimmer works on.
    trim: Option<PathBuf>,
}

#[derive(Default)]
pub struct RecordingState {
    inner: Mutex<Inner>,
}

#[allow(dead_code)]
pub(crate) enum Outcome {
    Saved(PathBuf),
    Cancelled,
    Failed(String),
}

/// The settings of the `screenRecording` page.
#[derive(Clone)]
pub(crate) struct Opts {
    pub source: String,
    pub fps: u32,
    pub quality: String,
    pub countdown: u32,
    pub system_audio: bool,
    pub mic: bool,
    pub mic_device: String,
    pub effects: bool,
    pub typing_zoom: bool,
    pub folder: String,
    pub name: String,
    pub format: String,
}

impl Opts {
    fn from(s: &Settings) -> Self {
        let get = |k: &str| s.option(NS, k);
        let text = |k: &str, d: &str| get(k).and_then(Value::as_str).unwrap_or(d).to_string();
        let flag = |k: &str, d: bool| get(k).and_then(Value::as_bool).unwrap_or(d);
        Self {
            source: text("source", "area"),
            fps: fps_from(&text("frameRate", "30")),
            quality: text("quality", "balanced"),
            countdown: get("countdown").and_then(Value::as_f64).unwrap_or(3.0).clamp(0.0, 10.0) as u32,
            system_audio: flag("systemAudio", true),
            mic: flag("microphone", false),
            mic_device: text("microphoneDevice", ""),
            effects: flag("cursorEffects", true),
            typing_zoom: flag("typingZoom", true),
            folder: text("folder", ""),
            name: text("nameTemplate", "Recording {date} {time}"),
            format: text("format", "mp4"),
        }
    }
}

fn opts(app: &AppHandle) -> Opts {
    Opts::from(&app.state::<Store>().get())
}

// ---- Windows ----

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let bar = WebviewWindowBuilder::new(app, BAR, WebviewUrl::App("recording.html".into()))
        .title("Notifi Recording")
        .inner_size(BAR_SIZE.0, BAR_SIZE.1)
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
    {
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE},
        };
        // The bar must not appear in the video it controls.
        if let Ok(h) = bar.hwnd() {
            unsafe {
                let _ = SetWindowDisplayAffinity(HWND(h.0 as _), WDA_EXCLUDEFROMCAPTURE);
            }
        }
    }
    #[cfg(not(windows))]
    let _ = bar;
    WebviewWindowBuilder::new(app, PICK, WebviewUrl::App("recording.html".into()))
        .title("Notifi Recording Area")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .visible(false)
        .focused(false)
        .build()?;
    Ok(())
}

fn monitor_at(app: &AppHandle, x: f64, y: f64) -> Option<tauri::Monitor> {
    app.monitor_from_point(x, y).ok().flatten().or_else(|| app.primary_monitor().ok().flatten())
}

fn show_bar(app: &AppHandle, region: Region) {
    let Some(win) = app.get_webview_window(BAR) else { return };
    let (cx, cy) = region.center();
    if let Some(m) = monitor_at(app, f64::from(cx), f64::from(cy)) {
        let s = m.scale_factor();
        let (bw, bh) = ((BAR_SIZE.0 * s).round() as i32, (BAR_SIZE.1 * s).round() as i32);
        let x = m.position().x + (m.size().width as i32 - bw) / 2;
        let y = m.position().y + m.size().height as i32 - bh - (BAR_LIFT * s).round() as i32;
        let _ = win.set_size(PhysicalSize::new(bw as u32, bh as u32));
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
    let _ = win.show();
}

fn hide(app: &AppHandle, label: &str) {
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.hide();
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StateEvent<'a> {
    phase: &'a str,
    elapsed_ms: u64,
    countdown: u32,
    path: String,
    message: &'a str,
}

pub(crate) fn emit(app: &AppHandle, phase: &str, elapsed_ms: u64, countdown: u32, path: Option<&Path>, message: &str) {
    let path = path.map(|p| p.display().to_string()).unwrap_or_default();
    let _ = app.emit_to(BAR, STATE_EVENT, StateEvent { phase, elapsed_ms, countdown, path, message });
}

pub(crate) fn set_phase(app: &AppHandle, phase: Phase) {
    app.state::<RecordingState>().inner.lock().unwrap().phase = phase;
}

pub(crate) fn phase_name(app: &AppHandle) -> &'static str {
    app.state::<RecordingState>().inner.lock().unwrap().phase.name()
}

// ---- Flow ----

pub fn run_action(app: &AppHandle, action: &str) {
    match action {
        TOGGLE => toggle(app),
        PAUSE => pause(app),
        _ => {}
    }
}

enum Act {
    Start,
    CancelPick,
    Stop(Arc<Session>),
    Nothing,
}

/// Starts a recording, or stops the running one. While the area is being chosen it cancels.
pub fn toggle(app: &AppHandle) {
    let act = {
        let rec_state = app.state::<RecordingState>();
        let inner = rec_state.inner.lock().unwrap();
        match inner.phase {
            Phase::Idle => Act::Start,
            Phase::Picking => Act::CancelPick,
            Phase::Countdown | Phase::Recording | Phase::Paused => inner.session.clone().map_or(Act::Nothing, Act::Stop),
            Phase::Saving => Act::Nothing,
        }
    };
    match act {
        Act::Start => start(app),
        Act::CancelPick => cancel_pick(app),
        Act::Stop(s) => s.stop.store(true, std::sync::atomic::Ordering::SeqCst),
        Act::Nothing => {}
    }
}

/// Pauses the recording, or resumes it. Video, sound and the effects pause together.
pub fn pause(app: &AppHandle) {
    let state = app.state::<RecordingState>();
    let mut inner = state.inner.lock().unwrap();
    let Some(session) = inner.session.clone() else { return };
    let now = Instant::now();
    match inner.phase {
        Phase::Recording => {
            session.clock.lock().unwrap().pause(now);
            inner.phase = Phase::Paused;
        }
        Phase::Paused => {
            session.clock.lock().unwrap().resume(now);
            inner.phase = Phase::Recording;
        }
        _ => {}
    }
}

/// Begins a recording from the default source: shows the area chooser, or records the display
/// under the pointer. Also the entry point for the capture selector's Recording mode.
pub fn start(app: &AppHandle) {
    if !available() {
        crate::island::notice(app, "Screen recording is not available on this system yet".into());
        return;
    }
    let source = opts(app).source;
    let pointer = app.cursor_position().ok();
    let at = pointer.map_or((0.0, 0.0), |p| (p.x, p.y));
    if source == "display" {
        let Some(m) = monitor_at(app, at.0, at.1) else { return };
        let region = Region { x: m.position().x, y: m.position().y, w: m.size().width as i32, h: m.size().height as i32 };
        start_region(app, region);
    } else {
        show_picker(app, &source, at);
    }
}

fn show_picker(app: &AppHandle, source: &str, at: (f64, f64)) {
    let Some(win) = app.get_webview_window(PICK) else { return };
    let Some(m) = monitor_at(app, at.0, at.1) else { return };
    {
        let rec_state = app.state::<RecordingState>();
        let mut inner = rec_state.inner.lock().unwrap();
        if inner.phase != Phase::Idle {
            return;
        }
        inner.phase = Phase::Picking;
    }
    let _ = win.set_position(*m.position());
    let _ = win.set_size(*m.size());
    let _ = app.emit_to(PICK, PICK_EVENT, source);
    let _ = win.show();
    if !crate::window::no_focus() {
        let _ = win.set_focus();
    }
}

fn cancel_pick(app: &AppHandle) {
    hide(app, PICK);
    let state = app.state::<RecordingState>();
    let mut inner = state.inner.lock().unwrap();
    if inner.phase == Phase::Picking {
        inner.phase = Phase::Idle;
    }
}

/// Records `region` (physical desktop pixels): counts down, records until stopped, saves the file.
pub fn start_region(app: &AppHandle, region: Region) {
    let session = Arc::new(Session { stop: AtomicBool::new(false), clock: Mutex::new(ActiveClock::new(Instant::now())) });
    {
        let rec_state = app.state::<RecordingState>();
        let mut inner = rec_state.inner.lock().unwrap();
        if !matches!(inner.phase, Phase::Idle | Phase::Picking) {
            return;
        }
        inner.phase = Phase::Countdown;
        inner.session = Some(session.clone());
    }
    hide(app, PICK);
    show_bar(app, region);
    let o = opts(app);
    let handle = app.clone();
    let started = std::thread::Builder::new().name("notifi-recording".into()).spawn(move || {
        let outcome = sys::record(&handle, region, &o, &session);
        finish(&handle, outcome);
    });
    if started.is_err() {
        finish(app, Outcome::Failed("Could not start the recording thread".into()));
    }
}

fn finish(app: &AppHandle, outcome: Outcome) {
    {
        let rec_state = app.state::<RecordingState>();
        let mut inner = rec_state.inner.lock().unwrap();
        inner.phase = Phase::Idle;
        inner.session = None;
        if let Outcome::Saved(p) = &outcome {
            inner.saved = Some(p.clone());
        }
    }
    if let Outcome::Saved(p) = &outcome {
        crate::capture::recent::remember(app, p, "video");
    }
    match outcome {
        Outcome::Saved(path) => {
            emit(app, "saved", 0, 0, Some(&path), "");
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            crate::island::notice(app, format!("Recording saved \u{b7} {name}"));
        }
        Outcome::Cancelled => hide(app, BAR),
        Outcome::Failed(msg) => {
            emit(app, "error", 0, 0, None, &msg);
            crate::island::notice(app, format!("Recording failed \u{b7} {msg}"));
        }
    }
}

/// Where recordings go: the folder in settings, else the Videos folder.
pub(crate) fn save_dir(folder: &str) -> PathBuf {
    let folder = folder.trim();
    if !folder.is_empty() {
        return PathBuf::from(folder);
    }
    sys::videos_dir()
}

// ---- Commands ----

fn check_on(app: &AppHandle) -> Result<(), String> {
    if !available() {
        return Err("Screen recording is only available on Windows for now".into());
    }
    if !app.state::<Store>().get().enabled(NS) {
        return Err("Screen recording is switched off in Settings".into());
    }
    Ok(())
}

#[tauri::command]
pub fn recording_available() -> bool {
    available()
}

#[tauri::command]
pub fn recording_toggle(app: AppHandle) -> Result<(), String> {
    check_on(&app)?;
    toggle(&app);
    Ok(())
}

#[tauri::command]
pub fn recording_pause(app: AppHandle) {
    pause(&app);
}

/// The area chosen in the overlay, in the overlay's CSS pixels.
#[tauri::command]
pub fn recording_pick(app: AppHandle, x: f64, y: f64, w: f64, h: f64) {
    let Some(win) = app.get_webview_window(PICK) else { return };
    let scale = win.scale_factor().unwrap_or(1.0);
    let origin = win.inner_position().unwrap_or_default();
    let region = Region {
        x: origin.x + (x * scale).round() as i32,
        y: origin.y + (y * scale).round() as i32,
        w: (w * scale).round() as i32,
        h: (h * scale).round() as i32,
    };
    if region.w < MIN_AREA || region.h < MIN_AREA {
        cancel_pick(&app);
        return;
    }
    start_region(&app, region);
}

#[tauri::command]
pub fn recording_pick_cancel(app: AppHandle) {
    cancel_pick(&app);
}

/// The app window under a point of the overlay, as x, y, width, height in the overlay's CSS pixels.
#[tauri::command]
pub fn recording_window_at(app: AppHandle, x: f64, y: f64) -> Option<[f64; 4]> {
    let win = app.get_webview_window(PICK)?;
    let scale = win.scale_factor().unwrap_or(1.0);
    let origin = win.inner_position().ok()?;
    let (px, py) = (origin.x + (x * scale).round() as i32, origin.y + (y * scale).round() as i32);
    let me = std::process::id();
    let platform = app.state::<notifi_platform::Platform>();
    let found = platform.windows().list().ok()?.into_iter().find(|w| {
        let b = w.bounds;
        w.pid != Some(me)
            && !w.minimized
            && px >= b.x
            && py >= b.y
            && i64::from(px) < i64::from(b.x) + i64::from(b.width)
            && i64::from(py) < i64::from(b.y) + i64::from(b.height)
    })?;
    let b = found.bounds;
    Some([
        f64::from(b.x - origin.x) / scale,
        f64::from(b.y - origin.y) / scale,
        f64::from(b.width) / scale,
        f64::from(b.height) / scale,
    ])
}

/// Opens the trimmer for the recording that was just saved.
#[tauri::command]
pub fn recording_trim_open(app: AppHandle) {
    let path = app.state::<RecordingState>().inner.lock().unwrap().saved.clone();
    if let Some(path) = path {
        trim::open(&app, &path);
    }
}

/// Opens the trimmer for a recording in the Recent captures list.
pub fn open_trimmer(app: &AppHandle, path: &Path) {
    trim::open(app, path);
}

#[tauri::command]
pub fn recording_open_folder(app: AppHandle) {
    let path = app.state::<RecordingState>().inner.lock().unwrap().saved.clone();
    if let Some(path) = path {
        crate::apps_clean::apps_clean_reveal(path.display().to_string());
    }
}

/// Hides the bar after a finished recording or an error; ignored while a recording is running.
#[tauri::command]
pub fn recording_dismiss(app: AppHandle) {
    if app.state::<RecordingState>().inner.lock().unwrap().phase == Phase::Idle {
        hide(&app, BAR);
    }
}

#[cfg(windows)]
use session as sys;

#[cfg(not(windows))]
mod sys {
    use super::*;

    pub fn record(_app: &AppHandle, _region: Region, _o: &Opts, _session: &Session) -> Outcome {
        Outcome::Failed("Screen recording is not available on Linux yet".into())
    }

    pub fn videos_dir() -> PathBuf {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Videos")).unwrap_or_default()
    }
}
