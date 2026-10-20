use std::{
    collections::{BTreeMap, HashMap},
    sync::Mutex,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::{settings::Store, window};

pub const STATUS_EVENT: &str = "shortcuts://status";

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: &'static str,
    pub label: &'static str,
    pub default_accel: &'static str,
}

pub const ACTIONS: &[Action] = &[Action {
    id: "openSettings",
    label: "Open Settings",
    default_accel: "Ctrl+Alt+N",
}];

/// Shortcuts the OS already uses, shown as clashes in the capture widget.
#[derive(Serialize, Clone, Copy)]
pub struct Reserved {
    pub accel: &'static str,
    pub label: &'static str,
}

#[cfg(windows)]
pub const RESERVED: &[Reserved] = &[
    Reserved { accel: "Alt+Tab", label: "Switch windows" },
    Reserved { accel: "Alt+F4", label: "Close window" },
    Reserved { accel: "Ctrl+Shift+Escape", label: "Task Manager" },
    Reserved { accel: "Super+D", label: "Show desktop" },
    Reserved { accel: "Super+E", label: "File Explorer" },
    Reserved { accel: "Super+L", label: "Lock" },
    Reserved { accel: "Super+R", label: "Run" },
    Reserved { accel: "Super+V", label: "Clipboard history" },
    Reserved { accel: "Super+Shift+S", label: "Snipping Tool" },
    Reserved { accel: "Super+Period", label: "Emoji panel" },
];

#[cfg(not(windows))]
pub const RESERVED: &[Reserved] = &[
    Reserved { accel: "Alt+Tab", label: "Switch windows" },
    Reserved { accel: "Alt+F4", label: "Close window" },
    Reserved { accel: "Ctrl+Alt+T", label: "Terminal" },
    Reserved { accel: "Ctrl+Alt+Delete", label: "Log out" },
    Reserved { accel: "Super+L", label: "Lock" },
    Reserved { accel: "Super+D", label: "Show desktop" },
];

pub fn defaults() -> BTreeMap<String, String> {
    ACTIONS
        .iter()
        .map(|a| (a.id.to_string(), a.default_accel.to_string()))
        .collect()
}

pub fn parse(accel: &str) -> Result<Shortcut, String> {
    accel.parse::<Shortcut>().map_err(|e| e.to_string())
}

#[derive(Default)]
pub struct Registry {
    bound: Mutex<HashMap<u32, &'static str>>,
    failed: Mutex<BTreeMap<String, String>>,
}

/// Re-registers every global shortcut from the settings map.
pub fn apply(app: &AppHandle, shortcuts: &BTreeMap<String, String>) {
    let global = app.global_shortcut();
    let _ = global.unregister_all();
    let registry = app.state::<Registry>();
    let mut bound = registry.bound.lock().unwrap();
    let mut failed = registry.failed.lock().unwrap();
    bound.clear();
    failed.clear();

    for action in ACTIONS {
        let accel = shortcuts.get(action.id).map(String::as_str).unwrap_or_default();
        if accel.is_empty() {
            continue;
        }
        match parse(accel).and_then(|s| global.register(s).map(|_| s).map_err(|e| e.to_string())) {
            Ok(shortcut) => {
                bound.insert(shortcut.id(), action.id);
            }
            Err(err) => {
                failed.insert(action.id.to_string(), err);
            }
        }
    }
    let _ = app.emit(STATUS_EVENT, failed.clone());
}

pub fn handle(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state() != ShortcutState::Pressed {
        return;
    }
    let action = app.state::<Registry>().bound.lock().unwrap().get(&shortcut.id()).copied();
    if let Some(action) = action {
        run(app, action);
    }
}

fn run(app: &AppHandle, action: &str) {
    if action == "openSettings" {
        window::open_settings(app);
    }
}

/// Actions whose shortcut could not be registered, with the reason.
#[tauri::command]
pub fn shortcut_status(registry: State<Registry>) -> BTreeMap<String, String> {
    registry.failed.lock().unwrap().clone()
}

/// Lets the capture widget receive a combo that is currently bound.
#[tauri::command]
pub fn pause_shortcuts(app: AppHandle, store: State<Store>, paused: bool) {
    if paused {
        let _ = app.global_shortcut().unregister_all();
        app.state::<Registry>().bound.lock().unwrap().clear();
    } else {
        apply(&app, &store.get().shortcuts);
    }
}
