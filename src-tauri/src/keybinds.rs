use std::{
    collections::{BTreeMap, HashMap},
    sync::Mutex,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::{
    settings::{Settings, Store},
    window,
};

pub const STATUS_EVENT: &str = "shortcuts://status";

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: &'static str,
    pub label: &'static str,
    pub default_accel: &'static str,
}

pub const ACTIONS: &[Action] = &[
    Action { id: "openSettings", label: "Open Settings", default_accel: "Ctrl+Alt+N" },
    Action { id: "islandToggle", label: "Island: open or close", default_accel: "Ctrl+Alt+P" },
    Action { id: "islandModuleNowPlaying", label: "Island: Now Playing", default_accel: "" },
    Action { id: "islandModuleControls", label: "Island: Controls", default_accel: "" },
    Action { id: "islandModuleTimer", label: "Island: Timer", default_accel: "" },
    Action { id: "islandModuleAgents", label: "Island: AI Agents", default_accel: "" },
    Action { id: "islandModuleGrid", label: "Island: module grid", default_accel: "" },
    Action { id: "mediaPlayPause", label: "Media: play or pause", default_accel: "MediaPlayPause" },
    Action { id: "mediaNext", label: "Media: next track", default_accel: "MediaTrackNext" },
    Action { id: "mediaPrevious", label: "Media: previous track", default_accel: "MediaTrackPrevious" },
    Action { id: "quickToggleDarkMode", label: "Toggle: Dark mode", default_accel: "" },
    Action { id: "quickToggleHiddenFiles", label: "Toggle: Show hidden files", default_accel: "" },
    Action { id: "quickToggleFileExtensions", label: "Toggle: Show file extensions", default_accel: "" },
    Action { id: "quickToggleDesktopIcons", label: "Toggle: Show desktop icons", default_accel: "" },
    Action { id: "quickToggleNightLight", label: "Toggle: Night light", default_accel: "" },
    Action { id: "quickToggleDnd", label: "Toggle: Do not disturb", default_accel: "" },
    Action { id: "quickToggleMuteMic", label: "Toggle: Mute microphone", default_accel: "" },
    Action { id: "quickToggleKeepAwake", label: "Toggle: Keep awake", default_accel: "" },
    Action { id: "quickToggleEjectAll", label: "Toggle: Eject all disks", default_accel: "" },
    Action { id: "quickToggleEmptyTrash", label: "Toggle: Empty the bin", default_accel: "" },
    Action { id: "quickToggleLockScreen", label: "Toggle: Lock screen", default_accel: "" },
    Action { id: "switcherOpen", label: "App switcher: open (click to choose)", default_accel: "" },
    Action { id: "switcherSameApp", label: "App switcher: this app's windows", default_accel: "" },
];

/// A shortcut option on a feature's settings page that runs `action` while the feature is on.
pub struct FeatureShortcut {
    pub namespace: &'static str,
    pub key: &'static str,
    pub action: &'static str,
}

/// Opens the Media tools window. Not named "media..." because those actions are play, pause and track keys.
const MEDIA_TOOLS: &str = "openMediaTools";

pub const FEATURE_SHORTCUTS: &[FeatureShortcut] = &[
    FeatureShortcut { namespace: "commandBar", key: "shortcut", action: crate::commandbar::ACTION },
    FeatureShortcut { namespace: "clipboardHistory", key: "openShortcut", action: "clipboardHistory" },
    FeatureShortcut { namespace: "plainTextPaste", key: "shortcut", action: "plainTextPaste" },
    FeatureShortcut { namespace: "textSnippets", key: "menuShortcut", action: "snippetMenu" },
    FeatureShortcut { namespace: "outputSwitcher", key: "cycleOutput", action: "cycleOutput" },
    FeatureShortcut { namespace: "microphone", key: "muteAll", action: "muteMicrophones" },
    FeatureShortcut { namespace: "volumeMixer", key: "openMixer", action: "openMixer" },
    FeatureShortcut { namespace: "cleanUrl", key: "cleanShortcut", action: "cleanUrl" },
    FeatureShortcut { namespace: "pasteImageAsFile", key: "shortcut", action: "pasteImageAsFile" },
    FeatureShortcut { namespace: "shelf", key: "shortcut", action: "shelfToggle" },
    FeatureShortcut { namespace: "quickPanel", key: "shortcut", action: "quickPanel" },
    FeatureShortcut { namespace: "scratchpad", key: "shortcut", action: "scratchpadToggle" },
    FeatureShortcut { namespace: "colorPicker", key: "shortcut", action: crate::picker::ACTION },
    FeatureShortcut { namespace: "screenRecording", key: "shortcut", action: crate::recording::TOGGLE },
    FeatureShortcut { namespace: "screenRecording", key: "pauseShortcut", action: crate::recording::PAUSE },
    FeatureShortcut { namespace: "screenshot", key: "shortcut", action: "captureScreenshot" },
    FeatureShortcut { namespace: "textFromScreen", key: "shortcut", action: "captureText" },
    FeatureShortcut { namespace: "recentCaptures", key: "shortcut", action: "captureRecent" },
    FeatureShortcut { namespace: "radialMenu", key: "shortcut", action: crate::radial::ACTION },
    FeatureShortcut { namespace: "mediaTools", key: "shortcut", action: MEDIA_TOOLS },
];

/// Whether a settings change touches a feature shortcut or its feature switch.
pub fn feature_shortcuts_changed(old: &Settings, new: &Settings) -> bool {
    FEATURE_SHORTCUTS
        .iter()
        .chain(crate::windows_mgmt::SHORTCUTS)
        .chain(crate::energy::SHORTCUTS)
        .any(|f| old.enabled(f.namespace) != new.enabled(f.namespace) || old.option(f.namespace, f.key) != new.option(f.namespace, f.key))
        || crate::commandbar::shortcut_bindings(old) != crate::commandbar::shortcut_bindings(new)
}

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
    bound: Mutex<HashMap<u32, String>>,
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
                bound.insert(shortcut.id(), action.id.to_string());
            }
            Err(err) => {
                failed.insert(action.id.to_string(), err);
            }
        }
    }
    if let Some(settings) = app.try_state::<Store>().map(|s| s.get()) {
        for f in FEATURE_SHORTCUTS
            .iter()
            .chain(crate::windows_mgmt::SHORTCUTS)
            .chain(crate::energy::SHORTCUTS)
            .filter(|f| settings.enabled(f.namespace))
        {
            let accel = settings.option(f.namespace, f.key).and_then(|v| v.as_str()).unwrap_or_default();
            if accel.is_empty() {
                continue;
            }
            match parse(accel).and_then(|s| global.register(s).map(|_| s).map_err(|e| e.to_string())) {
                Ok(shortcut) => {
                    bound.insert(shortcut.id(), f.action.to_string());
                }
                Err(err) => {
                    failed.insert(format!("{}.{}", f.namespace, f.key), err);
                }
            }
        }
    }
    if let Some(settings) = app.try_state::<Store>().map(|s| s.get()) {
        for (accel, action, key) in crate::commandbar::shortcut_bindings(&settings) {
            match parse(&accel).and_then(|s| global.register(s).map(|_| s).map_err(|e| e.to_string())) {
                Ok(shortcut) => {
                    bound.insert(shortcut.id(), action);
                }
                Err(err) => {
                    failed.insert(key, err);
                }
            }
        }
    }
    let _ = app.emit(STATUS_EVENT, failed.clone());
}

pub fn handle(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state() != ShortcutState::Pressed {
        return;
    }
    let action = app.state::<Registry>().bound.lock().unwrap().get(&shortcut.id()).cloned();
    if let Some(action) = action {
        run(app, &action);
    }
}

pub fn run(app: &AppHandle, action: &str) {
    match action {
        "openSettings" => window::open_settings(app),
        MEDIA_TOOLS => crate::media_tools::open(app),
        media if media.starts_with("media") => crate::island::run_media_key(app, media),
        island if island.starts_with("island") => crate::island::run_action(app, island),
        sound if crate::sound::ACTIONS.contains(&sound) => crate::sound::run_action(app, sound),
        crate::commandbar::ACTION => crate::commandbar::toggle(app),
        cb if crate::commandbar::is_shortcut_action(cb) => crate::commandbar::run_shortcut(app, cb),
        clip if crate::clipboard::ACTIONS.contains(&clip) => crate::clipboard::run_action(app, clip),
        "scratchpadToggle" => crate::scratchpad::toggle(app),
        crate::picker::ACTION => crate::picker::toggle(app),
        rec if crate::recording::ACTIONS.contains(&rec) => crate::recording::run_action(app, rec),
        capture if crate::capture::ACTIONS.contains(&capture) => crate::capture::run_action(app, capture),
        crate::radial::ACTION => crate::radial::open_from_shortcut(app),
        "quickToggleLockScreen" => {
            let handle = app.clone();
            if !crate::island::lock::notice_then(app, move || crate::quick::run_action(&handle, "quickToggleLockScreen")) {
                crate::quick::run_action(app, "quickToggleLockScreen");
            }
        }
        quick if quick.starts_with("quick") => crate::quick::run_action(app, quick),
        sw if crate::switcher::ACTIONS.contains(&sw) => crate::switcher::run_action(app, sw),
        wm if wm.starts_with("wm.") => crate::windows_mgmt::run_action(app, wm),
        energy if energy.starts_with("energy.") => crate::energy::run_action(app, energy),
        _ => {}
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

#[cfg(test)]
mod media_tools_shortcut_tests {
    use super::*;

    #[test]
    fn media_tools_action_is_not_taken_by_the_media_keys() {
        assert!(!MEDIA_TOOLS.starts_with("media") && !MEDIA_TOOLS.starts_with("island"));
        assert!(FEATURE_SHORTCUTS.iter().any(|f| f.namespace == "mediaTools" && f.key == "shortcut" && f.action == MEDIA_TOOLS));
    }

    #[test]
    fn accelerators_parse() {
        assert!(parse("Ctrl+Alt+M").is_ok());
    }
}
