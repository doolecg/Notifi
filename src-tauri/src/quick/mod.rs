//! Quick toggles and the Quick panel (spec/everyday/quick-toggles.md, quick-panel.md).

mod toggles;

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::{settings::Store, window};
use toggles::Kind;

pub const WINDOW: &str = "quick";
pub const CHANGED_EVENT: &str = "quick://changed";
const OPEN_EVENT: &str = "quick://open";

/// Keybind action ids that flip or run a toggle: `quickToggle` + the toggle id with a capital.
pub const TOGGLE_PREFIX: &str = "quickToggle";
pub const PANEL_ACTION: &str = "quickPanel";

#[derive(Default)]
pub struct QuickState {
    anchor: Mutex<Option<(f64, f64)>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    id: String,
    label: String,
    hint: String,
    /// "switch", "action" or "tool".
    kind: &'static str,
    on: bool,
    /// Greyed out; `note` says why and clicking opens the settings page.
    disabled: bool,
    note: String,
    confirm: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OpenPayload {
    close_after_run: bool,
}

struct Tool {
    id: &'static str,
    label: &'static str,
    hint: &'static str,
    feature: &'static str,
}

const TOOLS: &[Tool] = &[
    Tool { id: "scratchpad", label: "Scratchpad", hint: "Notes that save themselves.", feature: "scratchpad" },
    Tool { id: "clipboardHistory", label: "Clipboard history", hint: "Paste something you copied earlier.", feature: "clipboardHistory" },
    Tool { id: "textSnippets", label: "Text snippets", hint: "Insert a saved snippet.", feature: "textSnippets" },
    Tool { id: "settings", label: "Notifi settings", hint: "Open the settings window.", feature: "" },
];

fn matches(entry: &str, id: &str, label: &str) -> bool {
    let entry = entry.trim();
    entry.eq_ignore_ascii_case(id) || entry.eq_ignore_ascii_case(label)
}

/// Every row the panel can show, in the user's order, with live state.
async fn rows(app: &AppHandle, panel: bool) -> Vec<Row> {
    let settings = app.state::<Store>().get();
    let mut all = Vec::new();
    if panel {
        for tool in TOOLS {
            let off = !tool.feature.is_empty() && !settings.enabled(tool.feature);
            all.push(Row {
                id: format!("tool:{}", tool.id),
                label: tool.label.into(),
                hint: tool.hint.into(),
                kind: "tool",
                on: false,
                disabled: off,
                note: if off { "Turn on in Features".into() } else { String::new() },
                confirm: false,
            });
        }
    }
    for def in toggles::ALL.iter().filter(|d| toggles::available(app, d.id)) {
        let on = if def.kind == Kind::Switch { toggles::state(app, def.id).await } else { false };
        let confirm = def.id == "emptyTrash"
            && settings.option("quickToggles", "confirmEmptyTrash").and_then(|v| v.as_bool()).unwrap_or(true);
        all.push(Row {
            id: def.id.into(),
            label: def.label.into(),
            hint: def.hint.into(),
            kind: if def.kind == Kind::Switch { "switch" } else { "action" },
            on,
            disabled: false,
            note: String::new(),
            confirm,
        });
    }
    let order = if panel {
        settings.option("quickPanel", "tools").and_then(|v| v.as_array()).cloned().unwrap_or_default()
    } else {
        toggles::shown(app).into_iter().map(serde_json::Value::from).collect()
    };
    let order: Vec<String> = order.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
    if order.is_empty() {
        return all;
    }
    order
        .iter()
        .filter_map(|entry| {
            all.iter().find(|r| matches(entry, r.id.trim_start_matches("tool:"), &r.label)).cloned()
        })
        .collect()
}

/// The toggles for the tray panel's Controls tab and the quick panel's toggle rows.
#[tauri::command]
pub async fn quick_toggles(app: AppHandle) -> Vec<Row> {
    rows(&app, false).await
}

#[tauri::command]
pub async fn quick_rows(app: AppHandle) -> Vec<Row> {
    rows(&app, true).await
}

/// Runs a toggle or tool row. `on` sets a switch; leave it out to flip it.
#[tauri::command]
pub async fn quick_run(app: AppHandle, id: String, on: Option<bool>) -> Result<(), String> {
    if let Some(tool) = id.strip_prefix("tool:") {
        run_tool(&app, tool);
        return Ok(());
    }
    let def = toggles::find(&id).ok_or_else(|| format!("Unknown toggle {id}"))?;
    let target = match (def.kind, on) {
        (Kind::Action, _) => true,
        (Kind::Switch, Some(on)) => on,
        (Kind::Switch, None) => !toggles::state(&app, &id).await,
    };
    let result = toggles::apply(&app, &id, target).await;
    let _ = app.emit(CHANGED_EVENT, ());
    result
}

fn run_tool(app: &AppHandle, id: &str) {
    let settings = app.state::<Store>().get();
    let Some(tool) = TOOLS.iter().find(|t| t.id == id) else { return };
    hide(app);
    if !tool.feature.is_empty() && !settings.enabled(tool.feature) {
        window::open_settings_at(app, Some(tool.feature));
        return;
    }
    match id {
        "scratchpad" => crate::scratchpad::show(app),
        "clipboardHistory" => crate::clipboard::run_action(app, "clipboardHistory"),
        "textSnippets" => crate::clipboard::run_action(app, "snippetMenu"),
        "cameraPreview" => crate::camera::toggle(app),
        _ => window::open_settings(app),
    }
}

/// Keybind entry point for the quick panel and the toggle actions.
pub fn run_action(app: &AppHandle, action: &str) {
    if action == PANEL_ACTION {
        toggle_panel(app);
        return;
    }
    let Some(rest) = action.strip_prefix(TOGGLE_PREFIX) else { return };
    let mut chars = rest.chars();
    let Some(first) = chars.next() else { return };
    let id = format!("{}{}", first.to_ascii_lowercase(), chars.as_str());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if toggles::available(&app, &id) {
            let _ = quick_run(app, id, None).await;
        }
    });
}

// ---- The panel window ----

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("quick.html".into()))
        .title("Notifi Quick panel")
        .inner_size(360.0, 320.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .visible(false)
        .focused(false)
        .build()?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            if !window::no_focus() {
                hide(&handle);
            }
        }
    });
    Ok(())
}

fn width(app: &AppHandle) -> f64 {
    match app.state::<Store>().get().option("quickPanel", "size").and_then(|v| v.as_str()) {
        Some("small") => 300.0,
        Some("large") => 440.0,
        _ => 360.0,
    }
}

fn toggle_panel(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    if window.is_visible().unwrap_or(false) {
        hide(app);
        return;
    }
    let settings = app.state::<Store>().get();
    if !settings.enabled("quickPanel") {
        return;
    }
    let centered = settings.option("quickPanel", "position").and_then(|v| v.as_str()) == Some("center");
    let cursor = app.cursor_position().map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
    *app.state::<QuickState>().anchor.lock().unwrap() = if centered { None } else { Some(cursor) };
    let close_after_run = settings.option("quickPanel", "closeAfterRun").and_then(|v| v.as_bool()).unwrap_or(true);
    let scale = window.scale_factor().unwrap_or(1.0);
    let current = window.inner_size().map(|s| s.height as f64 / scale).unwrap_or(320.0);
    let _ = window.set_size(PhysicalSize::new((width(app) * scale).round() as u32, (current * scale).round() as u32));
    place(app, current * scale, cursor);
    let _ = window.show();
    if !window::no_focus() {
        let _ = window.set_focus();
    }
    let _ = app.emit_to(WINDOW, OPEN_EVENT, OpenPayload { close_after_run });
}

/// Puts the window under the pointer (or centred) and keeps it inside the display's work area.
fn place(app: &AppHandle, height: f64, cursor: (f64, f64)) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    let anchor = *app.state::<QuickState>().anchor.lock().unwrap();
    let probe = anchor.unwrap_or(cursor);
    let Some(monitor) = app.monitor_from_point(probe.0, probe.1).ok().flatten().or_else(|| app.primary_monitor().ok().flatten()) else {
        return;
    };
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let (wx, wy, ww, wh) = (work.position.x as f64, work.position.y as f64, work.size.width as f64, work.size.height as f64);
    let w = (width(app) * scale).min(ww);
    let h = height.min(wh);
    let (x, y) = match anchor {
        Some((ax, ay)) => {
            let below = ay + 6.0 * scale;
            let y = if below + h > wy + wh { (ay - 6.0 * scale - h).max(wy) } else { below };
            (ax - w / 2.0, y)
        }
        None => (wx + (ww - w) / 2.0, wy + (wh - h) / 3.0),
    };
    let x = x.clamp(wx, (wx + ww - w).max(wx));
    let y = y.clamp(wy, (wy + wh - h).max(wy));
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// The page reports its content height so the window fits its rows.
#[tauri::command]
pub fn quick_resize(app: AppHandle, height: f64) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    let scale = window.scale_factor().unwrap_or(1.0);
    let h = (height.clamp(80.0, 640.0) * scale).round();
    let _ = window.set_size(PhysicalSize::new((width(&app) * scale).round() as u32, h as u32));
    if window.is_visible().unwrap_or(false) {
        let cursor = app.cursor_position().map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
        place(&app, h, cursor);
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn quick_hide(app: AppHandle) {
    hide(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_toggle_has_a_keybind_action() {
        for def in toggles::ALL {
            let mut id = def.id.to_string();
            id[..1].make_ascii_uppercase();
            let action = format!("{TOGGLE_PREFIX}{id}");
            assert!(crate::keybinds::ACTIONS.iter().any(|a| a.id == action), "{action}");
        }
    }

    #[test]
    fn order_entries_match_ids_and_labels() {
        assert!(matches("dark mode", "darkMode", "Dark mode"));
        assert!(matches("darkmode", "darkMode", "Dark mode"));
        assert!(!matches("night", "darkMode", "Dark mode"));
    }
}
