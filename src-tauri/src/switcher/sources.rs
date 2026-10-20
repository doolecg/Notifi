//! Where the switcher and the taskbar preview get their windows. Windows reports virtual desktops,
//! windows hidden to the tray and background apps; other systems report plain window lists.

use notifi_platform::{Platform, WindowInfo};

use super::filter::Facts;

/// An app with no window of its own.
pub struct Idle {
    pub pid: u32,
    pub app_id: String,
    pub path: String,
}

/// Every window with its facts, front to back. `tray_hidden` also reports windows hidden to the tray.
#[cfg(windows)]
pub fn windows(_platform: &Platform, tray_hidden: bool) -> Vec<(WindowInfo, Facts)> {
    notifi_platform::win_switcher::list(tray_hidden)
        .unwrap_or_default()
        .into_iter()
        .map(|d| {
            let facts = Facts { minimized: d.info.minimized, other_desktop: d.other_desktop, fullscreen: d.fullscreen, tray_hidden: d.tray_hidden };
            (d.info, facts)
        })
        .collect()
}

#[cfg(not(windows))]
pub fn windows(platform: &Platform, _tray_hidden: bool) -> Vec<(WindowInfo, Facts)> {
    platform
        .windows()
        .list()
        .unwrap_or_default()
        .into_iter()
        .map(|w| {
            let facts = Facts { minimized: w.minimized, ..Facts::default() };
            (w, facts)
        })
        .collect()
}

#[cfg(windows)]
pub fn idle_apps() -> Vec<Idle> {
    notifi_platform::win_switcher::background_apps()
        .into_iter()
        .map(|b| Idle { pid: b.pid, app_id: b.app_id, path: b.path })
        .collect()
}

#[cfg(not(windows))]
pub fn idle_apps() -> Vec<Idle> {
    Vec::new()
}
