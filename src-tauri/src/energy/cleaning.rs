//! Cleaning mode: keyboard and pointer input is blocked while you wipe them, with a black screen
//! or a small corner marker. Pressing Escape the chosen number of times inside the time window unlocks.

use std::sync::atomic::Ordering;

use notifi_platform::{Platform, Rect};
use tauri::{AppHandle, Manager};

use super::{block, flag, number, settings, text, win, EnergyState};

const MARKER: (u32, u32) = (240, 72);
const MARGIN: i32 = 16;

pub fn is_active(app: &AppHandle) -> bool {
    app.state::<EnergyState>().cleaning.load(Ordering::SeqCst)
}

pub fn start(app: &AppHandle) -> Result<(), String> {
    if !win::SUPPORTED {
        return Err("Cleaning mode is only built for Windows so far".into());
    }
    if is_active(app) {
        return Ok(());
    }
    let s = settings(app);
    let platform = app.state::<Platform>().inner().clone();
    let displays = block(platform.display().list()).map_err(|e| e.to_string())?;
    let blackout = text(&s, "cleaningMode", "displayMode", "blackout") != "indicator";
    let rects: Vec<Rect> = if blackout {
        displays.iter().map(|d| d.bounds).collect()
    } else {
        let main = displays.iter().find(|d| d.primary).or(displays.first()).ok_or("No display found")?;
        let b = main.bounds;
        vec![Rect { x: b.x + b.width as i32 - MARKER.0 as i32 - MARGIN, y: b.y + MARGIN, width: MARKER.0, height: MARKER.1 }]
    };
    if rects.is_empty() {
        return Err("No display found".into());
    }
    win::clean_start(win::CleanSpec {
        rects,
        blackout,
        allow_pointer: !blackout && flag(&s, "cleaningMode", "allowPointer", false),
        show_timer: flag(&s, "cleaningMode", "showTimer", false),
        presses: number(&s, "cleaningMode", "unlockPresses", 5.0) as u32,
        window_ms: (number(&s, "cleaningMode", "unlockWindow", 6.0) * 1000.0) as u64,
    });
    app.state::<EnergyState>().cleaning.store(true, Ordering::SeqCst);
    Ok(())
}

pub fn stop(app: &AppHandle) {
    if app.state::<EnergyState>().cleaning.swap(false, Ordering::SeqCst) {
        win::clean_stop();
    }
}

/// The unlock presses were typed; the hook thread has already let go of the input.
pub fn ended(app: &AppHandle) {
    app.state::<EnergyState>().cleaning.store(false, Ordering::SeqCst);
}

pub fn toggle(app: &AppHandle) {
    if is_active(app) {
        stop(app);
    } else if let Err(e) = start(app) {
        super::say(app, e);
    }
}

#[tauri::command]
pub async fn cleaning_mode_start(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || start(&app)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn cleaning_mode_stop(app: AppHandle) {
    stop(&app);
}
