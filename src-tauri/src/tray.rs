use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle,
};

use crate::{
    panel::{self, Anchor},
    updater, window,
};

pub const TRAY_ID: &str = "main";

/// The update version shown in the menu, so readout items get the same menu.
static UPDATE: Mutex<Option<String>> = Mutex::new(None);
/// Tray items currently showing readouts.
static READOUTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Keep awake's "toggle with a right click": the right click then has no menu, so it can toggle.
static RIGHT_CLICK_TOGGLE: AtomicBool = AtomicBool::new(false);

pub fn set_right_click_toggle(app: &AppHandle, on: bool) {
    if RIGHT_CLICK_TOGGLE.swap(on, Ordering::SeqCst) == on {
        return;
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let update = UPDATE.lock().unwrap().clone();
        let readouts = READOUTS.lock().unwrap().clone();
        for id in std::iter::once(TRAY_ID.to_string()).chain(readouts) {
            if let Some(tray) = app2.tray_by_id(&id) {
                let menu = if on { None } else { build_menu(&app2, update.as_deref()).ok() };
                let _ = tray.set_menu(menu);
            }
        }
    });
}

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

fn builder(app: &AppHandle, id: &str) -> tauri::Result<TrayIconBuilder<tauri::Wry>> {
    let update = UPDATE.lock().unwrap().clone();
    let mut builder = TrayIconBuilder::with_id(id).tooltip("Notifi");
    if !RIGHT_CLICK_TOGGLE.load(Ordering::SeqCst) {
        builder = builder.menu(&build_menu(app, update.as_deref())?);
    }
    Ok(builder
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } => {
                panel::toggle(tray.app_handle(), Anchor::from_rect(rect));
            }
            TrayIconEvent::Click { button: MouseButton::Right, button_state: MouseButtonState::Up, .. }
                if RIGHT_CLICK_TOGGLE.load(Ordering::SeqCst) =>
            {
                crate::keepawake::toggle(tray.app_handle());
            }
            _ => {}
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open_panel" => panel::show(app, Anchor::Pointer, None),
            "open_settings" => window::open_settings(app),
            "install_update" => updater::install_and_exit(app),
            "quit" => app.exit(0),
            _ => {}
        }))
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let mut tray = builder(app, TRAY_ID)?;
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Shows `image` in the readout item `id`, creating the item (with the app's menu and click
/// behaviour) the first time. `label` is the text label where the tray supports one.
pub fn set_readout(app: &AppHandle, id: &str, image: Image<'static>, tooltip: &str, label: Option<&str>) {
    let (app2, id, tooltip, label) = (app.clone(), id.to_string(), tooltip.to_string(), label.map(str::to_string));
    let _ = app.run_on_main_thread(move || {
        let tray: TrayIcon = match app2.tray_by_id(&id) {
            Some(tray) => tray,
            None => match builder(&app2, &id).and_then(|b| b.icon(image.clone()).build(&app2)) {
                Ok(tray) => {
                    READOUTS.lock().unwrap().push(id.clone());
                    tray
                }
                Err(_) => return,
            },
        };
        let _ = tray.set_icon(Some(image));
        let _ = tray.set_tooltip(Some(&tooltip));
        if label.is_some() {
            let _ = tray.set_title(label.as_deref());
        }
    });
}

pub fn remove_readout(app: &AppHandle, id: &str) {
    let (app2, id) = (app.clone(), id.to_string());
    let _ = app.run_on_main_thread(move || {
        READOUTS.lock().unwrap().retain(|r| *r != id);
        let _ = app2.remove_tray_by_id(&id);
    });
}

/// Hides or shows the plain app icon; readout items keep the menu reachable while it's hidden.
pub fn set_app_icon_visible(app: &AppHandle, visible: bool) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_visible(visible);
    }
}

/// Shows the "Install update" item while an update is ready.
pub fn set_update_ready(app: &AppHandle, version: Option<&str>) {
    *UPDATE.lock().unwrap() = version.map(str::to_string);
    if RIGHT_CLICK_TOGGLE.load(Ordering::SeqCst) {
        return;
    }
    let readouts = READOUTS.lock().unwrap().clone();
    for id in std::iter::once(TRAY_ID.to_string()).chain(readouts) {
        if let (Some(tray), Ok(menu)) = (app.tray_by_id(&id), build_menu(app, version)) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}
