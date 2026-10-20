//! Input enhancements: click filter, key debounce, mouse buttons and gestures, scroll direction,
//! smooth and linear scrolling, sideways scrolling, super key, pointer acceleration and the
//! touchpad middle click page. `logic` and `config` are pure and tested; `hooks_win` and
//! `system_win` connect them to Windows. Nothing here runs on Linux yet.
#![cfg_attr(not(windows), allow(dead_code))]

pub mod config;
#[cfg(windows)]
mod hooks_win;
pub mod logic;
#[cfg(windows)]
pub mod system_win;

use std::sync::{Arc, RwLock};

use tauri::{AppHandle, Manager};

use crate::settings::Store;
use config::Config;

static CONFIG: RwLock<Option<Arc<Config>>> = RwLock::new(None);

/// The options as of the last settings change; `None` before `start`.
pub fn config() -> Option<Arc<Config>> {
    CONFIG.read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn start(app: &AppHandle) {
    #[cfg(windows)]
    if let Some(file) = restore_file(app) {
        system_win::recover(&file);
    }
    reload(app);
}

pub fn on_settings_changed(app: &AppHandle) {
    reload(app);
}

pub fn on_exit(app: &AppHandle) {
    #[cfg(windows)]
    if let Some(file) = restore_file(app) {
        system_win::set_flat_pointer(false, &file);
    }
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
fn restore_file(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("pointer-acceleration.restore"))
}

/// Opens the system page where the touchpad's three finger tap is set.
#[tauri::command]
pub fn input_open_touchpad_settings() -> Result<(), String> {
    #[cfg(windows)]
    return system_win::open_touchpad_settings();
    #[cfg(not(windows))]
    Err("Touchpad middle click is not available on this system yet".into())
}

fn reload(app: &AppHandle) {
    let Some(store) = app.try_state::<Store>() else { return };
    let new = Arc::new(Config::from_settings(&store.get()));
    let old = CONFIG.write().unwrap_or_else(|e| e.into_inner()).replace(new.clone());
    if old.as_deref() == Some(&*new) {
        return;
    }
    #[cfg(windows)]
    {
        hooks_win::apply(&new);
        if let Some(file) = restore_file(app) {
            system_win::set_flat_pointer(new.flat_pointer, &file);
        }
        if new.touchpad_page && old.as_deref().is_some_and(|o| !o.touchpad_page) {
            let _ = system_win::open_touchpad_settings();
        }
    }
}
