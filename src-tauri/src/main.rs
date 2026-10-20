#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod keepawake;
mod keybinds;
mod panel;
mod settings;
mod stats;
mod tray;
mod updater;
mod window;

use tauri::{Manager, RunEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| match panel_arg(&args) {
            Some(tab) => panel::show_by_tray(app, tab),
            None => window::open_settings(app),
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(keybinds::handle)
                .build(),
        )
        .manage(keybinds::Registry::default())
        .manage(updater::Updater::default())
        .manage(notifi_platform::Platform::detect())
        .manage(panel::PanelState::default())
        .manage(keepawake::KeepAwakeState::default())
        .setup(|app| {
            let path = app.path().app_config_dir()?.join("settings.json");
            let store = settings::Store::load(path);
            let current = store.get();
            app.manage(store);
            sync_autostart(app.handle(), current.general.autostart);
            keybinds::apply(app.handle(), &current.shortcuts);
            tray::create(app.handle())?;
            panel::create(app.handle())?;
            stats::start(app.handle());
            if let Some(tab) = panel_arg(&std::env::args().collect::<Vec<_>>()) {
                panel::show_by_tray(app.handle(), tab);
            }
            updater::start(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            settings::get_settings,
            settings::get_catalog,
            settings::set_section,
            settings::reset_section,
            keybinds::shortcut_status,
            keybinds::pause_shortcuts,
            updater::get_update_status,
            updater::check_update_now,
            updater::install_update,
            updater::open_release_page,
            updater::open_release_notes,
            panel::panel_status,
            window::open_settings_section,
            panel::panel_resize,
            panel::panel_hide,
            panel::panel_closed,
            panel::panel_set_tab,
            panel::quit_app,
            keepawake::keep_awake_status,
            keepawake::set_keep_awake,
            stats::eject_volume,
            stats::set_power_profile,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Notifi");

    app.run(|app, event| match event {
        RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
        RunEvent::Exit => updater::on_exit(app),
        _ => {}
    });
}

/// Called after every settings change, with the old and new values.
pub fn on_settings_changed(app: &tauri::AppHandle, old: &settings::Settings, new: &settings::Settings) {
    if old.general.autostart != new.general.autostart {
        sync_autostart(app, new.general.autostart);
    }
    if old.shortcuts != new.shortcuts {
        keybinds::apply(app, &new.shortcuts);
    }
}

/// `--panel` opens the tray panel; `--panel=<tab>` opens it on that tab.
fn panel_arg(args: &[String]) -> Option<Option<String>> {
    args.iter().find_map(|a| match a.strip_prefix("--panel") {
        Some("") => Some(None),
        Some(rest) => rest.strip_prefix('=').map(|tab| Some(tab.to_string())),
        None => None,
    })
}

fn sync_autostart(app: &tauri::AppHandle, on: bool) {
    let launcher = app.autolaunch();
    if launcher.is_enabled().is_ok_and(|enabled| enabled != on) {
        let _ = if on { launcher.enable() } else { launcher.disable() };
    }
}
