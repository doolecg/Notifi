#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod apps;
mod apps_clean;
mod ask;
mod camera;
mod capture;
mod chat;
mod catalog;
mod clipboard;
mod commandbar;
mod energy;
mod island;
mod input;
mod keepawake;
mod keybinds;
mod lifecycle;
mod monitor;
mod media_tools;
mod panel;
mod picker;
mod quick;
mod recording;
mod radial;
mod scratchpad;
mod settings;
mod sound;
mod stats;
mod switcher;
mod tools;
mod tray;
mod updater;
mod window;
mod windows_mgmt;

use tauri::{Manager, RunEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

fn main() {
    if apps_clean::elevated_main() {
        return;
    }
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
        .register_uri_scheme_protocol("capturemedia", |ctx, request| recording::trim::media(ctx.app_handle(), &request))
        .manage(apps::AppsState::default())
        .manage(ask::AskState::default())
        .manage(apps_clean::AppsCleanState::default())
        .manage(keybinds::Registry::default())
        .manage(updater::Updater::default())
        .manage(notifi_platform::Platform::detect())
        .manage(panel::PanelState::default())
        .manage(keepawake::KeepAwakeState::default())
        .manage(energy::EnergyState::default())
        .manage(monitor::speedtest::SpeedTestState::default())
        .manage(island::IslandState::default())
        .manage(sound::SoundState::default())
        .manage(quick::QuickState::default())
        .manage(radial::RadialState::default())
        .manage(scratchpad::ScratchpadState::default())
        .manage(picker::PickerState::default())
        .manage(recording::RecordingState::default())
        .manage(camera::CameraState::default())
        .manage(capture::CaptureState::default())
        .manage(media_tools::MediaState::default())
        .setup(|app| {
            let path = app.path().app_config_dir()?.join("settings.json");
            let store = settings::Store::load(path);
            let current = store.get();
            app.manage(store);
            sync_autostart(app.handle(), current.general.autostart);
            keybinds::apply(app.handle(), &current.shortcuts);
            tray::create(app.handle())?;
            panel::create(app.handle())?;
            island::create(app.handle())?;
            commandbar::create(app.handle())?;
            sound::create(app.handle())?;
            quick::create(app.handle())?;
            scratchpad::create(app.handle())?;
            picker::create(app.handle())?;
            recording::create(app.handle())?;
            capture::create(app.handle())?;
            energy::start(app.handle())?;
            clipboard::start(app.handle())?;
            windows_mgmt::start(app.handle())?;
            input::start(app.handle());
            apps_clean::start(app.handle());
            switcher::create(app.handle())?;
            radial::create(app.handle())?;
            stats::start(app.handle());
            lifecycle::sync(app.handle());
            commandbar::open_from_args(app.handle());
            if let Some(tab) = panel_arg(&std::env::args().collect::<Vec<_>>()) {
                panel::show_by_tray(app.handle(), tab);
            }
            apps::start(app.handle());
            tools::start(app.handle());
            ask::start(app.handle());
            updater::start(app.handle());
            // The first launch opens settings on the chooser for which features to install.
            if !current.general.setup_done {
                window::open_settings(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            apps_clean::uninstaller_list,
            apps_clean::uninstaller_scan,
            apps_clean::uninstaller_remove,
            apps_clean::uninstaller_retry_elevated,
            apps_clean::apps_clean_cancel,
            apps_clean::apps_clean_reveal,
            apps_clean::cleaner_scan,
            apps_clean::cleaner_scan_elevated,
            apps_clean::cleaner_clean,
            apps_clean::cleaner_clean_system,
            apps_clean::messaging_scan,
            apps_clean::messaging_apply,
            apps_clean::installer_offers,
            apps_clean::installer_inspect,
            apps_clean::installer_install,
            apps_clean::installer_shortcut,
            apps_clean::installer_trash_download,
            apps_clean::installer_launch,
            apps_clean::appimage_install,
            apps_clean::wallpaper_list,
            apps_clean::wallpaper_thumb,
            apps_clean::wallpaper_apply,
            apps::apps_open,
            apps::apps_open_url,
            apps::packages::apps_managers,
            apps::packages::apps_pkg_installed,
            apps::packages::apps_pkg_search,
            apps::scoop_deps::apps_pkg_dependencies,
            apps::jobs::apps_job_run,
            apps::jobs::apps_job_cancel,
            apps::updates::apps_updates_check,
            apps::updates::apps_updates_last,
            apps::rules::apps_rules,
            apps::rules::apps_rule_add,
            apps::rules::apps_rule_remove,
            apps::ports::apps_ports,
            apps::space::space_drives,
            apps::space::space_start,
            apps::space::space_cancel,
            apps::space::space_status,
            apps::space::space_view,
            apps::space::space_tag,
            apps::space::space_open,
            apps::space::space_export,
            apps::process::apps_proc_list,
            apps::process::apps_proc_end,
            settings::get_settings,
            windows_mgmt::desktop_info,
            settings::get_catalog,
            settings::set_section,
            settings::reset_section,
            settings::finish_setup,
            settings::export_settings,
            settings::import_settings,
            keybinds::shortcut_status,
            keybinds::pause_shortcuts,
            updater::get_update_status,
            updater::check_update_now,
            updater::install_update,
            updater::open_release_page,
            updater::open_release_notes,
            updater::update_notes,
            panel::panel_status,
            window::open_settings_section,
            panel::panel_resize,
            panel::panel_hide,
            panel::panel_closed,
            panel::panel_set_tab,
            panel::quit_app,
            keepawake::keep_awake_status,
            keepawake::set_keep_awake,
            energy::dimming::energy_displays,
            energy::dimming::energy_set_level,
            energy::dimming::energy_set_extra_display,
            energy::cleaning::cleaning_mode_start,
            energy::cleaning::cleaning_mode_stop,
            stats::eject_volume,
            stats::set_power_profile,
            stats::eject_all,
            monitor::speedtest::speed_test_run,
            monitor::devices::monitor_devices,
            monitor::speedtest::speed_test_history,
            commandbar::commandbar_search,
            commandbar::commandbar_run,
            commandbar::commandbar_hide,
            commandbar::commandbar_resize,
            commandbar::commandbar_dragged,
            commandbar::commandbar_recenter,
            commandbar::commandbar_forget,
            ask::ask_list,
            ask::ask_answer,
            ask::ask_decide,
            ask::ask_skip,
            ask::ask_hooks_status,
            ask::ask_hooks_install,
            ask::ask_hooks_remove,
            island::island_status,
            island::island_cover,
            island::island_shape,
            island::island_bar_height,
            island::island_tall,
            island::island_goto_agent,
            island::files::island_recent_files,
            island::files::island_file_thumb,
            island::files::island_open_path,
            island::island_claude_usage,
            island::island_claude_usage_refresh,
            island::watch::watch_area,
            island::watch::watch_screen,
            island::watch::watch_set_area,
            island::watch::watch_clear_area,
            island::watch::watch_read,
            island::island_focus,
            island::island_open_agents,
            island::island_open_chat,
            island::island_ai_prompt,
            island::island_bar_width,
            island::island_bar_extra,
            island::island_chat,
            chat::chat_show,
            chat::chat_hide,
            chat::chat_open_thread,
            chat::chat_dictate,
            island::island_media,
            island::lyrics::island_lyrics,
            island::island_timer,
            island::island_set_volume,
            island::island_set_brightness,
            island::island_run,
            quick::quick_toggles,
            quick::quick_rows,
            quick::quick_run,
            quick::quick_resize,
            quick::quick_hide,
            scratchpad::scratchpad_load,
            scratchpad::scratchpad_save,
            scratchpad::scratchpad_export,
            scratchpad::scratchpad_pin,
            scratchpad::scratchpad_hide,
            picker::picker_toggle,
            picker::picker_available,
            picker::picker_history,
            picker::picker_copy,
            picker::picker_close,
            recording::recording_available,
            recording::recording_toggle,
            recording::recording_pause,
            recording::recording_pick,
            recording::recording_pick_cancel,
            recording::recording_window_at,
            recording::recording_open_folder,
            recording::recording_dismiss,
            recording::recording_trim_open,
            recording::trim::recording_trim_target,
            recording::trim::recording_trim_save,
            recording::trim::recording_edit_save,
            camera::camera_preview_options,
            camera::camera_preview_toggle,
            camera::camera_preview_close,
            camera::camera_preview_lost,
            camera::camera_preview_remember,
            recording::trim::recording_trim_reveal,
            recording::trim::recording_trim_close,
            capture::capture_frame,
            capture::capture_ready,
            capture::capture_cancel,
            capture::capture_available,
            capture::capture_start,
            capture::capture_color,
            capture::capture_nudge,
            capture::capture_copy_pixel,
            capture::capture_finish,
            capture::capture_thumb,
            capture::capture_action,
            capture::capture_editor_opts,
            capture::capture_editor_image,
            capture::capture_editor_export,
            capture::capture_close_editor,
            capture::capture_editor_text_boxes,
            capture::share::capture_share_file,
            capture::pin::capture_pin_image,
            capture::pin::capture_pin_zoom,
            capture::pin::capture_pin_close,
            capture::recent::capture_recent_list,
            capture::recent::capture_recent_thumb,
            capture::recent::capture_recent_action,
            capture::recent::capture_recent_clear,
            capture::recent::capture_recent_close,
            capture::recent::capture_recent_open,
            media_tools::media_status,
            media_tools::media_run,
            media_tools::media_cancel,
            media_tools::media_open,
            island::island_preview,
            island::island_agent_stats,
            island::agents::island_agent_sessions,
            island::agents::island_agent_log,
            sound::mixer_state,
            sound::mixer_set_app,
            sound::mixer_mute_app,
            sound::mixer_reset_app,
            sound::mixer_set_route,
            sound::reset_routes,
            sound::mixer_pin,
            sound::mixer_move,
            sound::mixer_set_master,
            sound::mixer_mute_master,
            sound::mixer_set_mic,
            sound::mixer_mute_mic,
            sound::mixer_mute_all_mics,
            clipboard::clipboard_list,
            clipboard::clipboard_image,
            clipboard::clipboard_paste,
            clipboard::clipboard_copy,
            clipboard::clipboard_pin,
            clipboard::clipboard_delete,
            clipboard::clipboard_clear_count,
            clipboard::clipboard_clear_unpinned,
            clipboard::clipboard_hide,
            clipboard::snippets_get,
            clipboard::snippets_save,
            clipboard::snippet_preview,
            clipboard::snippet_insert,
            clipboard::snippets_hide,
            clipboard::snippets_zones,
            clipboard::shelf::shelf_list,
            clipboard::shelf::shelf_remove,
            clipboard::shelf::shelf_clear,
            clipboard::shelf::shelf_open,
            clipboard::shelf::shelf_drag_out,
            clipboard::shelf::shelf_thumb,
            clipboard::shelf::shelf_hide,
            switcher::switcher_slots,
            switcher::switcher_pick,
            switcher::switcher_hover,
            switcher::switcher_close_window,
            switcher::switcher_step,
            switcher::switcher_cancel,
            switcher::taskbar::taskbar_slots,
            switcher::taskbar::taskbar_action,
            input::input_open_touchpad_settings,
            radial::radial_wheel,
            radial::radial_wheel_save,
            radial::radial_wheel_reset,
            radial::radial_actions,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Notifi");

    app.run(|app, event| match event {
        RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
        RunEvent::Exit => {
            windows_mgmt::on_exit();
            energy::on_exit(app);
            input::on_exit(app);
            updater::on_exit(app)
        }
        _ => {}
    });
}

/// Called after every settings change, with the old and new values.
pub fn on_settings_changed(app: &tauri::AppHandle, old: &settings::Settings, new: &settings::Settings) {
    if old.general.autostart != new.general.autostart {
        sync_autostart(app, new.general.autostart);
    }
    if old.shortcuts != new.shortcuts || keybinds::feature_shortcuts_changed(old, new) {
        keybinds::apply(app, &new.shortcuts);
    }
    windows_mgmt::on_settings_changed(app);
    island::on_settings_changed(app);
    stats::settings_changed(app);
    clipboard::on_settings_changed(app, old, new);
    sound::on_settings_changed(app, old, new);
    energy::on_settings_changed(app, old, new);
    switcher::on_settings_changed(app);
    input::on_settings_changed(app);
    radial::on_settings_changed(app);
    lifecycle::sync_later(app);
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
