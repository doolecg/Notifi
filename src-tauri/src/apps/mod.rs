//! App management tools (spec/apps/app-updates.md, package-manager.md, port-manager.md,
//! kill-process.md): one tools window with Updates, Packages, Ports, Processes and Disk space views.

mod exec;
mod flatpak;
pub mod jobs;
pub mod packages;
mod pkcon;
pub mod ports;
pub mod process;
pub mod rules;
mod scoop;
pub mod scoop_deps;
pub mod space;
mod table;
pub mod updates;
mod version;
mod winget;

use std::sync::Mutex;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::{settings::Store, window};

pub const WINDOW: &str = "apps";
const VIEW_EVENT: &str = "apps://view";

#[derive(Default)]
pub struct AppsState {
    updates: Mutex<Option<updates::UpdatesResult>>,
    jobs: jobs::Jobs,
    procs: process::ProcState,
    space: space::SpaceState,
}

/// Reads one option of a feature namespace from the settings store.
pub(crate) fn option(app: &AppHandle, namespace: &str, key: &str) -> Option<Value> {
    app.state::<Store>().get().option(namespace, key).cloned()
}

pub(crate) fn flag(app: &AppHandle, namespace: &str, key: &str, default: bool) -> bool {
    option(app, namespace, key).and_then(|v| v.as_bool()).unwrap_or(default)
}

pub(crate) fn strings(app: &AppHandle, namespace: &str, key: &str) -> Vec<String> {
    option(app, namespace, key)
        .and_then(|v| v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()))
        .unwrap_or_default()
}

const VIEWS: &[(&str, &str)] = &[
    ("appUpdates", "updates"),
    ("packageManager", "packages"),
    ("killProcess", "processes"),
    ("spaceMap", "space"),
];

/// Opens (or raises) the tools window on `view`.
pub fn open(app: &AppHandle, view: &str) {
    if let Some(win) = app.get_webview_window(WINDOW) {
        let _ = win.unminimize();
        let _ = win.show();
        if !window::no_focus() {
            let _ = win.set_focus();
        }
        let _ = app.emit_to(WINDOW, VIEW_EVENT, view);
        return;
    }
    let url = format!("apps.html#{view}");
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = WebviewWindowBuilder::new(&app, WINDOW, WebviewUrl::App(url.into()))
            .title("Notifi App tools")
            .inner_size(1000.0, 680.0)
            .min_inner_size(760.0, 480.0)
            .center()
            .focused(!window::no_focus())
            .build();
    });
}

#[tauri::command]
pub fn apps_open(app: AppHandle, view: Option<String>) {
    open(&app, view.as_deref().unwrap_or("updates"));
}

/// Opens a local `http://host:port` address in the default browser.
#[tauri::command]
pub fn apps_open_url(url: String) -> Result<(), String> {
    let ok = url.strip_prefix("http://").is_some_and(|rest| rest.chars().all(|c| c.is_ascii_alphanumeric() || ":.[]-".contains(c)));
    if !ok {
        return Err("Only plain http://host:port addresses can be opened.".into());
    }
    let program = if cfg!(windows) { "explorer" } else { "xdg-open" };
    exec::command(program).arg(&url).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Wires the settings pages' "open" button and starts the background update check.
pub fn start(app: &AppHandle) {
    let handle = app.clone();
    app.listen("settings://open-feature", move |event| {
        let id: String = serde_json::from_str(event.payload()).unwrap_or_default();
        if let Some((_, view)) = VIEWS.iter().find(|(page, _)| *page == id) {
            open(&handle, view);
        }
    });
    updates::start(app);
    #[cfg(debug_assertions)]
    if let Ok(view) = std::env::var("NOTIFI_APPS_VIEW") {
        open(app, &view);
    }
}
