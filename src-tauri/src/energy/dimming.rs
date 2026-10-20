//! Display brightness steps, extra dimming below the hardware minimum, and per-display off.
//! Hardware brightness goes through the platform `Display` trait; the software part is a black
//! click-through cover per monitor, so it needs no special rights and steps aside for system
//! prompts, which Windows draws on a separate desktop.

use std::collections::HashSet;

use notifi_platform::{DisplayInfo, Platform};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::{block, flag, logic, number, saved, settings, text, update_saved, win, EnergyState};
use crate::{settings::Settings, sound::osd};

#[derive(Default)]
pub struct Runtime {
    /// The display the brightness shortcuts act on after "previous/next display".
    selected: Option<String>,
    off: HashSet<String>,
    /// What the monitors looked like at the last check, to notice plugging and unplugging.
    layout: Vec<String>,
}

fn dim_key(id: &str) -> String {
    format!("dim:{id}")
}

fn off_key(id: &str) -> String {
    format!("off:{id}")
}

fn list(app: &AppHandle) -> Vec<DisplayInfo> {
    let platform = app.state::<Platform>().inner().clone();
    block(platform.display().list()).unwrap_or_default()
}

/// Extra dimming is on for this display: the feature is on, it is picked, and brightness can be read.
fn extra_on(app: &AppHandle, s: &Settings, info: &DisplayInfo) -> bool {
    win::SUPPORTED
        && s.enabled("extraDimming")
        && (info.ddc || info.internal)
        && saved(app).dim_displays.as_ref().is_none_or(|picked| picked.contains(&info.id))
}

fn share_and_floor(s: &Settings) -> (f32, f32) {
    (number(s, "extraDimming", "softwareShare", 25.0) as f32, number(s, "extraDimming", "minimumFloor", 5.0) as f32)
}

fn hardware(app: &AppHandle, id: &str) -> Option<f32> {
    let platform = app.state::<Platform>().inner().clone();
    block(platform.display().brightness(id)).ok().map(f32::from)
}

/// The slider position (0..=100) for a display, counting extra dimming when it is on.
fn current_level(app: &AppHandle, s: &Settings, info: &DisplayInfo) -> Option<f32> {
    if extra_on(app, s, info) {
        if let Some(level) = saved(app).levels.get(&info.id) {
            return Some(*level);
        }
        let (share, _) = share_and_floor(s);
        return hardware(app, &info.id).map(|hw| logic::combined_level(hw, 0.0, share));
    }
    hardware(app, &info.id)
}

fn set_cover(info: &DisplayInfo, alpha: f32) {
    let value = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    win::dim(&dim_key(&info.id), info.bounds, value);
}

/// Moves a display to `level` (0..=100). With extra dimming the low end is software dimming.
fn set_level(app: &AppHandle, s: &Settings, info: &DisplayInfo, level: f32) -> Result<f32, String> {
    let platform = app.state::<Platform>().inner().clone();
    let state = app.state::<EnergyState>();
    if state.display.lock().unwrap().off.remove(&info.id) {
        win::remove_dim(&off_key(&info.id));
    }
    if extra_on(app, s, info) {
        let (share, floor) = share_and_floor(s);
        let level = level.clamp(floor, 100.0);
        let (hw, alpha) = logic::split_level(level, share, floor);
        block(platform.display().set_brightness(&info.id, hw.round() as u8)).map_err(|e| e.to_string())?;
        set_cover(info, alpha);
        update_saved(app, |f| {
            f.levels.insert(info.id.clone(), level);
        });
        Ok(level)
    } else {
        // Never fully dark by accident: the lowest step keeps the picture visible.
        let level = level.clamp(1.0, 100.0);
        block(platform.display().set_brightness(&info.id, level.round() as u8)).map_err(|e| e.to_string())?;
        win::remove_dim(&dim_key(&info.id));
        update_saved(app, |f| {
            f.levels.remove(&info.id);
        });
        Ok(level)
    }
}

fn target(app: &AppHandle, s: &Settings, infos: &[DisplayInfo]) -> Option<DisplayInfo> {
    let primary = || infos.iter().find(|d| d.primary).or(infos.first()).cloned();
    let selected = app.state::<EnergyState>().display.lock().unwrap().selected.clone();
    if let Some(d) = selected.and_then(|id| infos.iter().find(|d| d.id == id)) {
        return Some(d.clone());
    }
    if text(s, "displays", "keyTarget", "pointer") == "primary" {
        return primary();
    }
    app.cursor_position()
        .ok()
        .and_then(|p| {
            infos.iter().find(|d| {
                let b = d.bounds;
                p.x >= f64::from(b.x)
                    && p.y >= f64::from(b.y)
                    && p.x < f64::from(b.x) + f64::from(b.width)
                    && p.y < f64::from(b.y) + f64::from(b.height)
            })
        })
        .cloned()
        .or_else(primary)
}

fn show_level(app: &AppHandle, s: &Settings, level: f32) {
    if !s.enabled("osdOverlays") || flag(s, "osdOverlays", "brightnessOverlay", true) {
        osd::show(app, osd::volume("display", format!("{}%", level.round()), level / 100.0, false));
    }
}

fn show_text(app: &AppHandle, label: String) {
    osd::show(app, osd::text("display", label));
}

/// A brightness shortcut: one step up or down on the target display.
pub fn step(app: &AppHandle, up: bool) {
    let s = settings(app);
    let infos = list(app);
    let Some(info) = target(app, &s, &infos) else { return };
    let Some(current) = current_level(app, &s, &info) else {
        show_text(app, format!("{} does not allow brightness control", info.name));
        return;
    };
    let delta = logic::key_step(text(&s, "displays", "keyStep", "full"));
    let next = if up { current + delta } else { current - delta };
    match set_level(app, &s, &info, next) {
        Ok(level) => show_level(app, &s, level),
        Err(e) => show_text(app, e),
    }
}

/// Makes the next or previous display the one the shortcuts act on.
pub fn cycle(app: &AppHandle, forward: bool) {
    let s = settings(app);
    let infos = list(app);
    if infos.is_empty() {
        return;
    }
    let current = target(app, &s, &infos).and_then(|t| infos.iter().position(|d| d.id == t.id)).unwrap_or(0);
    let next = &infos[logic::cycle_index(infos.len(), current, forward)];
    app.state::<EnergyState>().display.lock().unwrap().selected = Some(next.id.clone());
    show_text(app, format!("Display: {}", next.name));
}

/// Switches the target display to black, or back. The next brightness change also brings it back.
pub fn toggle_off(app: &AppHandle) {
    if !win::SUPPORTED {
        show_text(app, "Switching a display off is not available here".into());
        return;
    }
    let s = settings(app);
    let infos = list(app);
    let Some(info) = target(app, &s, &infos) else { return };
    let state = app.state::<EnergyState>();
    let mut rt = state.display.lock().unwrap();
    if rt.off.remove(&info.id) {
        win::remove_dim(&off_key(&info.id));
    } else {
        rt.off.insert(info.id.clone());
        win::dim(&off_key(&info.id), info.bounds, 255);
    }
}

pub fn backlight(app: &AppHandle) {
    show_text(app, "No keyboard backlight control on this system".into());
}

/// Re-applies the software dimming to every display (at start-up, on a settings change or a plug event).
pub fn refresh(app: &AppHandle) {
    let s = settings(app);
    let infos = list(app);
    let levels = saved(app).levels;
    for info in &infos {
        match levels.get(&info.id) {
            Some(level) if extra_on(app, &s, info) => {
                let (share, floor) = share_and_floor(&s);
                set_cover(info, logic::split_level(*level, share, floor).1);
            }
            _ => win::remove_dim(&dim_key(&info.id)),
        }
    }
    app.state::<EnergyState>().display.lock().unwrap().layout = layout(&infos);
}

fn layout(infos: &[DisplayInfo]) -> Vec<String> {
    infos.iter().map(|d| format!("{}@{},{},{}x{}", d.id, d.bounds.x, d.bounds.y, d.bounds.width, d.bounds.height)).collect()
}

/// Called every few seconds: redraws the covers when monitors were plugged, unplugged or resized.
pub fn poll(app: &AppHandle) {
    if saved(app).levels.is_empty() {
        return;
    }
    let now = layout(&list(app));
    if app.state::<EnergyState>().display.lock().unwrap().layout != now {
        refresh(app);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayRow {
    id: String,
    name: String,
    internal: bool,
    ddc: bool,
    primary: bool,
    /// Slider position 0..=100, `None` when the display does not answer.
    level: Option<f32>,
    /// Extra dimming can be used on this display.
    extra_supported: bool,
    /// Extra dimming is picked for this display.
    extra: bool,
    off: bool,
}

#[tauri::command]
pub async fn energy_displays(app: AppHandle) -> Vec<DisplayRow> {
    tauri::async_runtime::spawn_blocking(move || {
        let s = settings(&app);
        let picked = saved(&app).dim_displays;
        let off = app.state::<EnergyState>().display.lock().unwrap().off.clone();
        list(&app)
            .into_iter()
            .map(|d| DisplayRow {
                level: current_level(&app, &s, &d),
                extra_supported: win::SUPPORTED && (d.ddc || d.internal),
                extra: picked.as_ref().is_none_or(|p| p.contains(&d.id)),
                off: off.contains(&d.id),
                id: d.id,
                name: d.name,
                internal: d.internal,
                ddc: d.ddc,
                primary: d.primary,
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// Moves one display's slider; returns the level that was applied.
#[tauri::command]
pub async fn energy_set_level(app: AppHandle, id: String, level: f32) -> Result<f32, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let s = settings(&app);
        let info = list(&app).into_iter().find(|d| d.id == id).ok_or_else(|| format!("No display {id}"))?;
        set_level(&app, &s, &info, level)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Picks or unpicks a display for extra dimming (the Displays list on the Extra dimming page).
#[tauri::command]
pub async fn energy_set_extra_display(app: AppHandle, id: String, on: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let all: Vec<String> = list(&app).into_iter().map(|d| d.id).collect();
        update_saved(&app, |f| {
            let mut picked = f.dim_displays.take().unwrap_or_else(|| all.clone());
            picked.retain(|p| *p != id);
            if on {
                picked.push(id.clone());
            }
            f.dim_displays = Some(picked);
        });
        refresh(&app);
    })
    .await
    .map_err(|e| e.to_string())
}
