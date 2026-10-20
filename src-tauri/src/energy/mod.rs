//! The energy and display features: Keep awake extras, display brightness and extra dimming,
//! Bluetooth off during sleep, and cleaning mode. Keep awake itself lives in `keepawake.rs`.

mod bluetooth;
pub mod cleaning;
pub mod dimming;
pub mod lid;
pub mod logic;

#[cfg(windows)]
pub mod win;
#[cfg(not(windows))]
#[path = "other.rs"]
pub mod win;

use std::{
    collections::BTreeMap,
    future::Future,
    path::PathBuf,
    sync::{atomic::AtomicBool, Mutex},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::{
    keybinds::FeatureShortcut,
    settings::{Settings, Store},
    sound::osd,
};

use win::Sys;

pub const SHORTCUTS: &[FeatureShortcut] = &[
    FeatureShortcut { namespace: "keepAwake", key: "toggle", action: "energy.keepAwake" },
    FeatureShortcut { namespace: "cleaningMode", key: "shortcut", action: "energy.cleaning" },
    FeatureShortcut { namespace: "displays", key: "brightnessUp", action: "energy.brightnessUp" },
    FeatureShortcut { namespace: "displays", key: "brightnessDown", action: "energy.brightnessDown" },
    FeatureShortcut { namespace: "displays", key: "previousDisplay", action: "energy.previousDisplay" },
    FeatureShortcut { namespace: "displays", key: "nextDisplay", action: "energy.nextDisplay" },
    FeatureShortcut { namespace: "displays", key: "displayOff", action: "energy.displayOff" },
    FeatureShortcut { namespace: "displays", key: "backlightUp", action: "energy.backlightUp" },
    FeatureShortcut { namespace: "displays", key: "backlightDown", action: "energy.backlightDown" },
];

/// What survives a restart: debts that must still be paid (Bluetooth to turn back on, the lid
/// action to put back) and the display levels and picks.
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub bt_owed: bool,
    /// Lid-close action (on mains, on battery) before Keep awake changed it.
    pub lid_prev: Option<(u32, u32)>,
    /// Slider position per display while extra dimming is on.
    pub levels: BTreeMap<String, f32>,
    /// Displays picked for extra dimming; `None` means every display that can do it.
    pub dim_displays: Option<Vec<String>>,
}

#[derive(Default)]
pub struct EnergyState {
    saved: Mutex<Saved>,
    path: Mutex<Option<PathBuf>>,
    pub(crate) lid: Mutex<lid::Lid>,
    pub(crate) display: Mutex<dimming::Runtime>,
    pub(crate) cleaning: AtomicBool,
}

pub fn saved(app: &AppHandle) -> Saved {
    app.state::<EnergyState>().saved.lock().unwrap().clone()
}

pub fn update_saved(app: &AppHandle, change: impl FnOnce(&mut Saved)) {
    let state = app.state::<EnergyState>();
    let mut saved = state.saved.lock().unwrap();
    change(&mut saved);
    let path = state.path.lock().unwrap();
    if let (Some(path), Ok(text)) = (path.as_ref(), serde_json::to_string(&*saved)) {
        let _ = std::fs::write(path, text);
    }
}

pub fn settings(app: &AppHandle) -> Settings {
    app.state::<Store>().get()
}

pub fn flag(s: &Settings, ns: &str, key: &str, default: bool) -> bool {
    s.option(ns, key).and_then(Value::as_bool).unwrap_or(default)
}

pub fn number(s: &Settings, ns: &str, key: &str, default: f64) -> f64 {
    s.option(ns, key).and_then(Value::as_f64).unwrap_or(default)
}

pub fn text<'a>(s: &'a Settings, ns: &str, key: &str, default: &'a str) -> &'a str {
    s.option(ns, key).and_then(Value::as_str).unwrap_or(default)
}

pub fn strings(s: &Settings, ns: &str, key: &str) -> Vec<String> {
    s.option(ns, key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

/// Runs a platform future to the end from a plain thread. It uses a fresh thread so it is safe
/// to call from anywhere, including inside the async runtime's own threads.
pub fn block<F: Future + Send>(future: F) -> F::Output
where
    F::Output: Send,
{
    std::thread::scope(|scope| {
        scope.spawn(|| tauri::async_runtime::block_on(future)).join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

/// A short notice for something that did not work.
pub fn say(app: &AppHandle, label: String) {
    osd::show(app, osd::text("display", label));
}

pub fn start(app: &AppHandle) -> tauri::Result<()> {
    let path = app.path().app_config_dir()?.join("energy.json");
    if let Some(loaded) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Saved>(&t).ok()) {
        *app.state::<EnergyState>().saved.lock().unwrap() = loaded;
    }
    *app.state::<EnergyState>().path.lock().unwrap() = Some(path);

    let handle = app.clone();
    win::start(move |event| on_event(&handle, event));
    crate::keepawake::start(app);

    let app = app.clone();
    let _ = std::thread::Builder::new().name("notifi-energy-watch".into()).spawn(move || {
        lid::recover(&app);
        bluetooth::after_wake(&app);
        dimming::refresh(&app);
        loop {
            std::thread::sleep(Duration::from_secs(5));
            dimming::poll(&app);
        }
    });
    Ok(())
}

fn on_event(app: &AppHandle, event: Sys) {
    let app = app.clone();
    match event {
        // Sleep waits for this to return, so it runs here and not on a new thread.
        Sys::Suspending => bluetooth::before_sleep(&app),
        Sys::Locked => {
            std::thread::spawn(move || {
                cleaning::stop(&app);
                crate::keepawake::set_locked(&app, true);
            });
        }
        Sys::Unlocked => {
            std::thread::spawn(move || crate::keepawake::set_locked(&app, false));
        }
        Sys::Resumed => {
            std::thread::spawn(move || {
                bluetooth::after_wake(&app);
                dimming::refresh(&app);
            });
        }
        Sys::Lid(closed) => {
            std::thread::spawn(move || lid::lid_changed(&app, closed));
        }
        Sys::CleaningUnlocked => cleaning::ended(&app),
    }
}

pub fn run_action(app: &AppHandle, action: &str) {
    let (app, action) = (app.clone(), action.to_string());
    std::thread::spawn(move || match action.as_str() {
        "energy.keepAwake" => crate::keepawake::toggle(&app),
        "energy.cleaning" => cleaning::toggle(&app),
        other => {
            if !flag(&settings(&app), "displays", "customShortcuts", false) {
                return;
            }
            match other {
                "energy.brightnessUp" => dimming::step(&app, true),
                "energy.brightnessDown" => dimming::step(&app, false),
                "energy.previousDisplay" => dimming::cycle(&app, false),
                "energy.nextDisplay" => dimming::cycle(&app, true),
                "energy.displayOff" => dimming::toggle_off(&app),
                "energy.backlightUp" | "energy.backlightDown" => dimming::backlight(&app),
                _ => {}
            }
        }
    });
}

pub fn on_settings_changed(app: &AppHandle, old: &Settings, new: &Settings) {
    let touched = |ns: &str| old.features.get(ns) != new.features.get(ns);
    if touched("keepAwake") {
        crate::keepawake::settings_changed(app, new);
    }
    if touched("lidClosedAwake") {
        let app = app.clone();
        std::thread::spawn(move || lid::sync(&app));
    }
    if touched("extraDimming") {
        let app = app.clone();
        std::thread::spawn(move || dimming::refresh(&app));
    }
}

/// Puts the lid action and the wake lock back when the app quits.
pub fn on_exit(app: &AppHandle) {
    cleaning::stop(app);
    crate::keepawake::on_exit(app);
    lid::on_exit(app);
}
