use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle,
};

use crate::{
    panel::{self, Anchor},
    updater, window,
};

pub const TRAY_ID: &str = "main";

fn build_menu(app: &AppHandle, update: Option<&str>) -> tauri::Result<Menu<tauri::Wry>> {
    let open = MenuItem::with_id(app, "open_settings", "Open Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Notifi", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = match update {
        Some(version) => {
            let label = format!("Install update ({version})");
            let install = MenuItem::with_id(app, "install_update", label, true, None::<&str>)?;
            Menu::with_items(app, &[&install, &open, &separator, &quit])?
        }
        None => Menu::with_items(app, &[&open, &separator, &quit])?,
    };
    // Linux trays often don't report left clicks, so the panel needs a menu entry.
    if cfg!(target_os = "linux") {
        let panel = MenuItem::with_id(app, "open_panel", "Open panel", true, None::<&str>)?;
        menu.prepend(&panel)?;
    }
    Ok(menu)
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Notifi")
        .menu(&build_menu(app, None)?)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = event {
                panel::toggle(tray.app_handle(), Anchor::from_rect(rect));
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open_panel" => panel::show(app, Anchor::Pointer, None),
            "open_settings" => window::open_settings(app),
            "install_update" => updater::install_and_exit(app),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Shows the "Install update" item while an update is ready.
pub fn set_update_ready(app: &AppHandle, version: Option<&str>) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(app, version)) {
        let _ = tray.set_menu(Some(menu));
    }
}
