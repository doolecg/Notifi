//! The system switches behind Quick toggles (spec/everyday/quick-toggles.md). Each one reads its
//! state live from the OS; a switch with no equivalent on this platform is not offered.

use std::process::Command;

use notifi_platform::{DeviceDirection, Feature, Platform, Support};
use tauri::{AppHandle, Manager};

use crate::settings::Store;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Switch,
    Action,
}

pub struct Def {
    pub id: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub kind: Kind,
}

const fn switch(id: &'static str, label: &'static str, hint: &'static str) -> Def {
    Def { id, label, hint, kind: Kind::Switch }
}

const fn action(id: &'static str, label: &'static str, hint: &'static str) -> Def {
    Def { id, label, hint, kind: Kind::Action }
}

pub const ALL: &[Def] = &[
    switch("darkMode", "Dark mode", "Switches apps and the system between light and dark."),
    switch("hiddenFiles", "Show hidden files", "Shows hidden files in the file manager."),
    switch("fileExtensions", "Show file extensions", "Shows the extension on every file name."),
    switch("desktopIcons", "Show desktop icons", "Shows or hides everything on the desktop."),
    switch("nightLight", "Night light", "Warms the screen colours."),
    switch("dnd", "Do not disturb", "Stops notification banners."),
    switch("privacy", "Privacy mode", "Stops Notifi's own notifications."),
    switch("muteMic", "Mute microphone", "Mutes every microphone."),
    switch("keepAwake", "Keep awake", "Keeps the computer and screen on."),
    action("ejectAll", "Eject all disks", "Ejects removable disks, except the ones you keep mounted."),
    action("emptyTrash", "Empty the bin", "Deletes everything in the recycle bin or trash."),
    action("lockScreen", "Lock screen", "Locks the computer."),
];

pub fn find(id: &str) -> Option<&'static Def> {
    ALL.iter().find(|d| d.id == id)
}

/// Whether this platform offers the toggle at all.
pub fn available(app: &AppHandle, id: &str) -> bool {
    let caps = app.state::<Platform>().capabilities().clone();
    let supported = |f: Feature| *caps.get(f) == Support::Supported;
    match id {
        "darkMode" | "hiddenFiles" | "dnd" => cfg!(windows) || gsettings_present(),
        "fileExtensions" | "desktopIcons" => cfg!(windows),
        "nightLight" => !cfg!(windows) && gsettings_present(),
        "muteMic" => supported(Feature::AudioMicrophone),
        "keepAwake" => supported(Feature::KeepAwake),
        "ejectAll" | "emptyTrash" | "lockScreen" | "privacy" => true,
        _ => false,
    }
}

/// Current state of a switch; `false` for actions and for anything unreadable.
pub async fn state(app: &AppHandle, id: &str) -> bool {
    match id {
        "darkMode" => sys::dark_mode(),
        "hiddenFiles" => sys::hidden_files(),
        "fileExtensions" => sys::file_extensions(),
        "desktopIcons" => sys::desktop_icons(),
        "nightLight" => sys::night_light(),
        "dnd" => sys::dnd(),
        "privacy" => crate::island::config(app).privacy_mode,
        "muteMic" => mic_muted(app).await,
        "keepAwake" => serde_json::to_value(crate::keepawake::keep_awake_status(app.clone()))
            .ok()
            .and_then(|v| v.get("active").and_then(|a| a.as_bool()))
            .unwrap_or(false),
        _ => false,
    }
}

/// Flips a switch to `on`, or runs an action.
pub async fn apply(app: &AppHandle, id: &str, on: bool) -> Result<(), String> {
    match id {
        "darkMode" => sys::set_dark_mode(on),
        "hiddenFiles" => sys::set_hidden_files(on),
        "fileExtensions" => sys::set_file_extensions(on),
        "desktopIcons" => sys::set_desktop_icons(on),
        "nightLight" => sys::set_night_light(on),
        "dnd" => sys::set_dnd(on),
        "privacy" => crate::settings::set_section(app.clone(), app.state(), "island".into(), serde_json::json!({ "privacyMode": on })).map(|_| ()),
        "muteMic" => {
            let platform = app.state::<Platform>().inner().clone();
            platform.audio().mute_all_microphones(on).map_err(|e| e.to_string())
        }
        "keepAwake" => {
            let minutes = option(app, "awakeDuration").and_then(|v| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(60);
            crate::keepawake::set_keep_awake(app.clone(), on, minutes).await.map(|_| ())
        }
        "emptyTrash" => sys::empty_trash(),
        "lockScreen" => lock(app).await,
        "ejectAll" => eject_all(app).await,
        _ => Err(format!("Unknown toggle {id}")),
    }
}

fn option(app: &AppHandle, key: &str) -> Option<serde_json::Value> {
    app.state::<Store>().get().option("quickToggles", key).cloned()
}

fn list_option(app: &AppHandle, key: &str) -> Vec<String> {
    option(app, key)
        .and_then(|v| v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()))
        .unwrap_or_default()
}

/// Labels or ids the user picked and their order; empty means the whole available set.
pub fn shown(app: &AppHandle) -> Vec<String> {
    list_option(app, "shown")
}

async fn mic_muted(app: &AppHandle) -> bool {
    let platform = app.state::<Platform>().inner().clone();
    match platform.audio().devices(DeviceDirection::Input) {
        Ok(list) => !list.is_empty() && list.iter().all(|d| d.muted),
        Err(_) => false,
    }
}

async fn lock(app: &AppHandle) -> Result<(), String> {
    let platform = app.state::<Platform>().inner().clone();
    if platform.power().lock_screen().await.is_ok() {
        return Ok(());
    }
    sys::lock()
}

async fn eject_all(app: &AppHandle) -> Result<(), String> {
    let platform = app.state::<Platform>().inner().clone();
    let keep: Vec<String> = list_option(app, "ejectExceptions").iter().map(|s| s.trim().to_lowercase()).collect();
    let volumes = platform.session().removable_volumes().await.map_err(|e| e.to_string())?;
    let mounts: Vec<String> = volumes
        .into_iter()
        .filter_map(|v| {
            let mount = v.mount_point?.to_string_lossy().into_owned();
            let kept = keep.iter().any(|k| !k.is_empty() && (v.label.to_lowercase() == *k || mount.to_lowercase().starts_with(k)));
            (!kept).then_some(mount)
        })
        .collect();
    if mounts.is_empty() {
        return Err("No removable disks to eject.".into());
    }
    crate::stats::eject_all(app.clone(), mounts).await
}

fn gsettings_present() -> bool {
    use std::sync::OnceLock;
    static PRESENT: OnceLock<bool> = OnceLock::new();
    *PRESENT.get_or_init(|| Command::new("gsettings").arg("help").output().is_ok())
}

pub fn run(program: &str, args: &[&str]) -> Result<String, String> {
    #[cfg(windows)]
    let output = {
        use std::os::windows::process::CommandExt;
        Command::new(program).args(args).creation_flags(0x0800_0000).output()
    };
    #[cfg(not(windows))]
    let output = Command::new(program).args(args).output();
    let output = output.map_err(|e| format!("{program}: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(windows)]
mod sys {
    use super::run;
    use windows::{
        core::{w, HSTRING, PCWSTR},
        Win32::{
            Foundation::{ERROR_SUCCESS, HWND, LPARAM, WPARAM},
            System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD},
            UI::WindowsAndMessaging::{
                FindWindowExW, FindWindowW, SendMessageTimeoutW, SendMessageW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_COMMAND,
                WM_SETTINGCHANGE,
            },
        },
    };

    const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
    const ADVANCED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
    const TOASTS: &str = r"Software\Microsoft\Windows\CurrentVersion\Notifications\Settings";

    fn get(subkey: &str, name: &str) -> Option<u32> {
        let (subkey, name) = (HSTRING::from(subkey), HSTRING::from(name));
        let mut value = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_DWORD,
                None,
                Some(&mut value as *mut u32 as *mut _),
                Some(&mut size),
            )
        };
        (status == ERROR_SUCCESS).then_some(value)
    }

    fn set(subkey: &str, name: &str, value: u32) -> Result<(), String> {
        let key = format!(r"HKCU\{subkey}");
        run("reg", &["add", &key, "/v", name, "/t", "REG_DWORD", "/d", &value.to_string(), "/f"]).map(|_| ())
    }

    fn broadcast(area: &str) {
        let area = HSTRING::from(area);
        unsafe {
            SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(area.as_ptr() as isize), SMTO_ABORTIFHUNG, 300, None);
        }
    }

    pub fn dark_mode() -> bool {
        get(PERSONALIZE, "AppsUseLightTheme") == Some(0)
    }
    pub fn set_dark_mode(on: bool) -> Result<(), String> {
        let value = if on { 0 } else { 1 };
        set(PERSONALIZE, "AppsUseLightTheme", value)?;
        set(PERSONALIZE, "SystemUsesLightTheme", value)?;
        broadcast("ImmersiveColorSet");
        Ok(())
    }

    pub fn hidden_files() -> bool {
        get(ADVANCED, "Hidden") == Some(1)
    }
    pub fn set_hidden_files(on: bool) -> Result<(), String> {
        set(ADVANCED, "Hidden", if on { 1 } else { 2 })?;
        broadcast("ShellState");
        Ok(())
    }

    pub fn file_extensions() -> bool {
        get(ADVANCED, "HideFileExt") == Some(0)
    }
    pub fn set_file_extensions(on: bool) -> Result<(), String> {
        set(ADVANCED, "HideFileExt", if on { 0 } else { 1 })?;
        broadcast("ShellState");
        Ok(())
    }

    pub fn desktop_icons() -> bool {
        get(ADVANCED, "HideIcons") != Some(1)
    }
    pub fn set_desktop_icons(on: bool) -> Result<(), String> {
        if desktop_icons() == on {
            return Ok(());
        }
        let view = desktop_view().ok_or("Could not find the desktop.")?;
        // The shell's own "Show desktop icons" command; it flips the setting and redraws.
        unsafe { SendMessageW(view, WM_COMMAND, Some(WPARAM(0x7402)), Some(LPARAM(0))) };
        Ok(())
    }

    fn desktop_view() -> Option<HWND> {
        unsafe {
            if let Ok(progman) = FindWindowW(w!("Progman"), None) {
                if let Ok(view) = FindWindowExW(Some(progman), None, w!("SHELLDLL_DefView"), None) {
                    return Some(view);
                }
            }
            let mut after: Option<HWND> = None;
            while let Ok(worker) = FindWindowExW(None, after, w!("WorkerW"), None) {
                if let Ok(view) = FindWindowExW(Some(worker), None, w!("SHELLDLL_DefView"), None) {
                    return Some(view);
                }
                after = Some(worker);
            }
        }
        None
    }

    pub fn night_light() -> bool {
        false
    }
    pub fn set_night_light(_on: bool) -> Result<(), String> {
        Err("Night light is not available here.".into())
    }

    pub fn dnd() -> bool {
        get(TOASTS, "NOC_GLOBAL_SETTING_TOASTS_ENABLED") == Some(0)
    }
    pub fn set_dnd(on: bool) -> Result<(), String> {
        set(TOASTS, "NOC_GLOBAL_SETTING_TOASTS_ENABLED", if on { 0 } else { 1 })
    }

    pub fn empty_trash() -> Result<(), String> {
        run("powershell", &["-NoProfile", "-NonInteractive", "-Command", "Clear-RecycleBin -Force -ErrorAction Stop"]).map(|_| ())
    }

    pub fn lock() -> Result<(), String> {
        run("rundll32.exe", &["user32.dll,LockWorkStation"]).map(|_| ())
    }
}

#[cfg(not(windows))]
mod sys {
    use super::run;

    fn get(schema: &str, key: &str) -> Option<String> {
        run("gsettings", &["get", schema, key]).ok()
    }
    fn set(schema: &str, key: &str, value: &str) -> Result<(), String> {
        run("gsettings", &["set", schema, key, value]).map(|_| ())
    }

    pub fn dark_mode() -> bool {
        get("org.gnome.desktop.interface", "color-scheme").is_some_and(|v| v.contains("prefer-dark"))
    }
    pub fn set_dark_mode(on: bool) -> Result<(), String> {
        set("org.gnome.desktop.interface", "color-scheme", if on { "prefer-dark" } else { "default" })
    }

    pub fn hidden_files() -> bool {
        get("org.gtk.gtk4.Settings.FileChooser", "show-hidden").is_some_and(|v| v == "true")
    }
    pub fn set_hidden_files(on: bool) -> Result<(), String> {
        let v = if on { "true" } else { "false" };
        let gtk4 = set("org.gtk.gtk4.Settings.FileChooser", "show-hidden", v);
        let gtk3 = set("org.gtk.Settings.FileChooser", "show-hidden", v);
        gtk4.or(gtk3)
    }

    pub fn file_extensions() -> bool {
        false
    }
    pub fn set_file_extensions(_on: bool) -> Result<(), String> {
        Err("Not available here.".into())
    }
    pub fn desktop_icons() -> bool {
        false
    }
    pub fn set_desktop_icons(_on: bool) -> Result<(), String> {
        Err("Not available here.".into())
    }

    pub fn night_light() -> bool {
        get("org.gnome.settings-daemon.plugins.color", "night-light-enabled").is_some_and(|v| v == "true")
    }
    pub fn set_night_light(on: bool) -> Result<(), String> {
        set("org.gnome.settings-daemon.plugins.color", "night-light-enabled", if on { "true" } else { "false" })
    }

    pub fn dnd() -> bool {
        get("org.gnome.desktop.notifications", "show-banners").is_some_and(|v| v == "false")
    }
    pub fn set_dnd(on: bool) -> Result<(), String> {
        set("org.gnome.desktop.notifications", "show-banners", if on { "false" } else { "true" })
    }

    pub fn empty_trash() -> Result<(), String> {
        run("gio", &["trash", "--empty"]).map(|_| ())
    }

    pub fn lock() -> Result<(), String> {
        run("loginctl", &["lock-session"]).map(|_| ())
    }
}
