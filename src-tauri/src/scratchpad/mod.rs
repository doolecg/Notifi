//! The floating Scratchpad (spec/everyday/scratchpad.md): tabbed notes that save themselves.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};

use crate::{settings::Store, window};

pub const WINDOW: &str = "scratchpad";
const OPEN_EVENT: &str = "scratchpad://open";
const FILE: &str = "scratchpad.json";

#[derive(Default)]
pub struct ScratchpadState {
    pinned: Mutex<bool>,
    /// Serialises saves so two quick edits cannot interleave their temp files.
    write: Mutex<()>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Loaded {
    state: Value,
    /// Set when the saved file could not be read; the original was kept next to it.
    warning: Option<String>,
    pinned: bool,
}

fn folder(app: &AppHandle) -> Result<PathBuf, String> {
    let custom = app
        .state::<Store>()
        .get()
        .option("scratchpad", "folder")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let dir = if custom.is_empty() { app.path().app_data_dir().map_err(|e| e.to_string())? } else { PathBuf::from(custom) };
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn empty_state() -> Value {
    json!({ "notes": [{ "id": 1, "title": "1", "text": "" }], "active": 1, "next": 2 })
}

fn valid(state: &Value) -> bool {
    state.get("notes").and_then(Value::as_array).is_some_and(|n| !n.is_empty() && n.iter().all(|n| n.get("id").is_some() && n.get("text").is_some_and(Value::is_string)))
        && state.get("active").is_some()
}

fn read(dir: &Path) -> (Value, Option<String>) {
    let path = dir.join(FILE);
    let Ok(text) = fs::read_to_string(&path) else { return (empty_state(), None) };
    match serde_json::from_str::<Value>(&text) {
        Ok(state) if valid(&state) => (state, None),
        _ => {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let kept = dir.join(format!("scratchpad.unreadable-{stamp}.json"));
            let _ = fs::copy(&path, &kept);
            (empty_state(), Some(format!("Your notes file could not be read. The original was kept as {}.", kept.display())))
        }
    }
}

fn write(dir: &Path, state: &Value) -> Result<(), String> {
    let tmp = dir.join(format!("{FILE}.tmp"));
    let body = serde_json::to_string(state).map_err(|e| e.to_string())?;
    fs::write(&tmp, body).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dir.join(FILE)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scratchpad_load(app: AppHandle, state: State<ScratchpadState>) -> Loaded {
    let (value, warning) = match folder(&app) {
        Ok(dir) => read(&dir),
        Err(e) => (empty_state(), Some(e)),
    };
    Loaded { state: value, warning, pinned: *state.pinned.lock().unwrap() }
}

#[tauri::command]
pub fn scratchpad_save(app: AppHandle, state: State<ScratchpadState>, value: Value) -> Result<(), String> {
    if !valid(&value) {
        return Err("The notes were not in a form that can be saved.".into());
    }
    let _guard = state.write.lock().unwrap();
    write(&folder(&app)?, &value)
}

/// Saves a note as a file in the Downloads folder and returns where it went.
#[tauri::command]
pub fn scratchpad_export(app: AppHandle, name: String, text: String, extension: String) -> Result<String, String> {
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    let stem: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' }).collect();
    let extension = if extension == "txt" { "txt" } else { "md" };
    let mut path = dir.join(format!("{}.{extension}", stem.trim().trim_matches('.')));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{} ({n}).{extension}", stem.trim()));
        n += 1;
    }
    fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn scratchpad_pin(state: State<ScratchpadState>, pinned: bool) {
    *state.pinned.lock().unwrap() = pinned;
}

#[tauri::command]
pub fn scratchpad_hide(app: AppHandle) {
    hide(&app);
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(WINDOW).is_some() {
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("scratchpad.html".into()))
        .title("Notifi Scratchpad")
        .inner_size(440.0, 380.0)
        .min_inner_size(300.0, 220.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(true)
        .visible(false)
        .focused(false)
        .build()?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            let s = handle.state::<Store>().get();
            let outside = s.option("scratchpad", "closeOnOutsideClick").and_then(Value::as_bool).unwrap_or(true);
            let pinned = *handle.state::<ScratchpadState>().pinned.lock().unwrap();
            if outside && !pinned && !window::no_focus() {
                hide(&handle);
            }
        }
    });
    Ok(())
}

pub fn toggle(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    if window.is_visible().unwrap_or(false) {
        hide(app);
    } else {
        show(app);
    }
}

pub fn show(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    if !window.is_visible().unwrap_or(false) {
        place(app);
        let _ = window.show();
    }
    if !window::no_focus() {
        let _ = window.set_focus();
    }
    let _ = app.emit_to(WINDOW, OPEN_EVENT, ());
}

pub fn hide(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let keep = app.state::<Store>().get().option("scratchpad", "keepPinned").and_then(Value::as_bool).unwrap_or(false);
    if !keep {
        *app.state::<ScratchpadState>().pinned.lock().unwrap() = false;
    }
    let _ = app.emit_to(WINDOW, "scratchpad://closing", ());
    let _ = window.hide();
}

/// Opens where it was last left, else near the top third of the display under the pointer.
fn place(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    if SEEN.set(()).is_err() {
        return;
    }
    let (px, py) = app.cursor_position().map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
    let Some(monitor) = app.monitor_from_point(px, py).ok().flatten().or_else(|| app.primary_monitor().ok().flatten()) else { return };
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let (wx, wy, ww, wh) = (work.position.x as f64, work.position.y as f64, work.size.width as f64, work.size.height as f64);
    let (w, h) = ((440.0 * scale).min(ww), (380.0 * scale).min(wh));
    let _ = window.set_size(PhysicalSize::new(w.round() as u32, h.round() as u32));
    let _ = window.set_position(PhysicalPosition::new((wx + (ww - w) / 2.0).round() as i32, (wy + (wh - h) / 3.0).round() as i32));
}

static SEEN: std::sync::OnceLock<()> = std::sync::OnceLock::new();

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notifi-scratchpad-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saves_and_reads_back() {
        let dir = temp("roundtrip");
        let state = json!({ "notes": [{ "id": 1, "title": "1", "text": "hello" }], "active": 1, "next": 2 });
        write(&dir, &state).unwrap();
        let (read_back, warning) = read(&dir);
        assert_eq!(read_back, state);
        assert!(warning.is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unreadable_file_is_kept() {
        let dir = temp("corrupt");
        fs::write(dir.join(FILE), "{ not json").unwrap();
        let (state, warning) = read(&dir);
        assert_eq!(state, empty_state());
        assert!(warning.is_some());
        let kept = fs::read_dir(&dir).unwrap().filter_map(Result::ok).any(|e| e.file_name().to_string_lossy().contains("unreadable"));
        assert!(kept);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_file_starts_one_empty_note() {
        let dir = temp("missing");
        let (state, warning) = read(&dir);
        assert!(valid(&state));
        assert!(warning.is_none());
        let _ = fs::remove_dir_all(dir);
    }
}
