use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const SETTINGS: &str = "settings";
pub const GOTO_EVENT: &str = "settings://goto";

/// `NOTIFI_NO_FOCUS=1` shows windows without taking focus (for test runs).
pub fn no_focus() -> bool {
    std::env::var("NOTIFI_NO_FOCUS").is_ok_and(|v| v == "1")
}

pub fn open_settings(app: &AppHandle) {
    open_settings_at(app, None);
}

/// Opens the settings window, jumping to `section` when given.
pub fn open_settings_at(app: &AppHandle, section: Option<&str>) {
    if let Some(window) = app.get_webview_window(SETTINGS) {
        let _ = window.unminimize();
        let _ = window.show();
        if !no_focus() {
            let _ = window.set_focus();
        }
        if let Some(section) = section {
            let _ = app.emit_to(SETTINGS, GOTO_EVENT, section);
        }
        return;
    }
    let url = match section {
        Some(section) => format!("index.html#{section}"),
        None => "index.html".into(),
    };
    // Building a webview from an event handler can deadlock on Windows, so build off-thread.
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = WebviewWindowBuilder::new(&app, SETTINGS, WebviewUrl::App(url.into()))
            .title("Notifi Settings")
            .inner_size(980.0, 680.0)
            .min_inner_size(760.0, 520.0)
            .center()
            .focused(!no_focus())
            .build();
    });
}

#[tauri::command]
pub fn open_settings_section(app: AppHandle, section: Option<String>) {
    open_settings_at(&app, section.as_deref());
}
