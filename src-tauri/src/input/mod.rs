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

use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock, RwLock};

use tauri::{AppHandle, Manager};

use crate::energy::EnergyState;
use crate::settings::Store;
use config::Config;

static CONFIG: RwLock<Option<Arc<Config>>> = RwLock::new(None);
/// The same options with the click filter and key debounce off, used while Cleaning Mode is on.
static CLEANING_CONFIG: RwLock<Option<Arc<Config>>> = RwLock::new(None);
static APP: OnceLock<AppHandle> = OnceLock::new();

/// The options as of the last settings change; `None` before `start`. The click filter and key
/// debounce are suspended while Cleaning Mode is on.
pub fn config() -> Option<Arc<Config>> {
    let cleaning = APP.get().is_some_and(|app| app.state::<EnergyState>().cleaning.load(Ordering::SeqCst));
    let slot = if cleaning { &CLEANING_CONFIG } else { &CONFIG };
    slot.read().unwrap_or_else(|e| e.into_inner()).clone()
}

fn without_filters(cfg: &Config) -> Config {
    Config { click: None, debounce_ms: None, ..cfg.clone() }
}

pub fn start(app: &AppHandle) {
    let _ = APP.set(app.clone());
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
    *CLEANING_CONFIG.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(without_filters(&new)));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn cleaning_config_drops_click_filter_and_debounce_only() {
        let mut settings = Settings::default();
        let ns = settings.features.entry("clickFilter".into()).or_default();
        ns.insert("enabled".into(), true.into());
        let key = settings.features.entry("keyDebounce".into()).or_default();
        key.insert("enabled".into(), true.into());
        let cfg = Config::from_settings(&settings);
        assert!(cfg.click.is_some() && cfg.debounce_ms.is_some());
        let quiet = without_filters(&cfg);
        assert!(quiet.click.is_none() && quiet.debounce_ms.is_none());
        assert_eq!(quiet.flat_pointer, cfg.flat_pointer);
    }
}
