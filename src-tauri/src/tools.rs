//! The "Open" button on a tool's settings page: runs the tool's own action, the same one its shortcut runs.

use tauri::{AppHandle, Listener};

use crate::keybinds::{self, FEATURE_SHORTCUTS};

/// Pages whose own settings page is the tool (nothing separate to open), or that apps::start already opens.
const NO_WINDOW: &[&str] = &["uninstaller", "cleaner", "displays", "appUpdates", "packageManager", "portManager", "killProcess"];

/// Whether a tool page has something to open.
pub fn openable(page: &str) -> bool {
    !NO_WINDOW.contains(&page) && action_of(page).is_some()
}

fn action_of(page: &str) -> Option<&'static str> {
    match page {
        "aiAgents" => Some("islandModuleAgents"),
        "cleaningMode" => Some("energy.cleaning"),
        _ => FEATURE_SHORTCUTS.iter().find(|s| s.namespace == page).map(|s| s.action),
    }
}

pub fn start(app: &AppHandle) {
    let handle = app.clone();
    app.listen("settings://open-feature", move |event| {
        let id: String = serde_json::from_str(event.payload()).unwrap_or_default();
        if NO_WINDOW.contains(&id.as_str()) {
            return;
        }
        if id == "cleaningMode" {
            let app = handle.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::energy::cleaning::cleaning_mode_start(app).await;
            });
        } else if let Some(action) = action_of(&id) {
            keybinds::run(&handle, action);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_with_a_shortcut_action_can_be_opened() {
        for page in ["screenshot", "screenRecording", "colorPicker", "scratchpad", "commandBar", "mediaTools", "cameraPreview", "radialMenu", "quickPanel", "textFromScreen", "shelf", "clipboardHistory", "textSnippets", "aiAgents", "cleaningMode"] {
            assert!(openable(page), "{page}");
        }
        assert!(!openable("cleaner"));
        assert!(!openable("island"));
    }
}
