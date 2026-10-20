//! Which windows and apps the switcher lists: pure decisions over facts the platform layer reports.

use std::collections::HashSet;

use super::listed;

/// Marks entries that are apps with no window. Real window ids never reach this bit.
const IDLE_BIT: u64 = 1 << 63;

pub fn idle_id(pid: u32) -> u64 {
    IDLE_BIT | pid as u64
}

pub fn is_idle(id: u64) -> bool {
    id & IDLE_BIT != 0
}

/// What the platform knows about one window beyond its title and bounds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Facts {
    pub minimized: bool,
    /// On a virtual desktop other than the current one.
    pub other_desktop: bool,
    /// Fills its whole display.
    pub fullscreen: bool,
    /// Hidden to the tray: not visible, but the app is running.
    pub tray_hidden: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Rules {
    pub show_minimised: bool,
    pub current_desktop_only: bool,
    pub show_hidden_apps: bool,
    pub other_desktop_fullscreen: bool,
}

pub fn keep(f: Facts, r: Rules) -> bool {
    if f.minimized && !r.show_minimised {
        return false;
    }
    if f.tray_hidden && !r.show_hidden_apps {
        return false;
    }
    !(f.other_desktop && (r.current_desktop_only || (f.fullscreen && !r.other_desktop_fullscreen)))
}

fn is_file_manager(app_id: &str) -> bool {
    matches!(app_id.to_lowercase().trim_end_matches(".exe"), "explorer" | "nautilus" | "dolphin" | "thunar" | "nemo" | "pcmanfm")
}

/// Whether an app with no window gets an entry. `with_windows` holds the lower-case ids of apps that
/// have a window anywhere, `window_only` the apps that must never be listed without one.
pub fn keep_windowless(app_id: &str, with_windows: &HashSet<String>, hide_file_manager: bool, window_only: &[String]) -> bool {
    !with_windows.contains(&app_id.to_lowercase()) && !(hide_file_manager && is_file_manager(app_id)) && !listed(window_only, app_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Rules = Rules { show_minimised: true, current_desktop_only: false, show_hidden_apps: true, other_desktop_fullscreen: false };

    #[test]
    fn minimised_and_hidden_windows_follow_their_switches() {
        let min = Facts { minimized: true, ..Default::default() };
        let hid = Facts { tray_hidden: true, ..Default::default() };
        assert!(keep(min, ALL) && keep(hid, ALL));
        assert!(!keep(min, Rules { show_minimised: false, ..ALL }));
        assert!(!keep(hid, Rules { show_hidden_apps: false, ..ALL }));
        assert!(keep(Facts::default(), Rules { show_minimised: false, show_hidden_apps: false, ..ALL }));
    }

    #[test]
    fn other_desktops() {
        let other = Facts { other_desktop: true, ..Default::default() };
        let game = Facts { other_desktop: true, fullscreen: true, ..Default::default() };
        assert!(keep(other, ALL));
        assert!(!keep(other, Rules { current_desktop_only: true, ..ALL }));
        assert!(!keep(game, ALL));
        assert!(keep(game, Rules { other_desktop_fullscreen: true, ..ALL }));
        assert!(!keep(game, Rules { other_desktop_fullscreen: true, current_desktop_only: true, ..ALL }));
        let here = Facts { fullscreen: true, ..Default::default() };
        assert!(keep(here, ALL));
    }

    #[test]
    fn windowless_apps() {
        let have: HashSet<String> = ["chrome.exe".to_string()].into_iter().collect();
        assert!(keep_windowless("Discord.exe", &have, false, &[]));
        assert!(!keep_windowless("chrome.exe", &have, false, &[]));
        assert!(!keep_windowless("Chrome.exe", &have, false, &[]));
        assert!(keep_windowless("explorer.exe", &have, false, &[]));
        assert!(!keep_windowless("explorer.exe", &have, true, &[]));
        assert!(!keep_windowless("Discord.exe", &have, false, &["discord".to_string()]));
    }

    #[test]
    fn idle_ids_never_clash_with_windows() {
        assert!(is_idle(idle_id(4321)));
        assert!(!is_idle(0x0001_0A2C));
        assert_ne!(idle_id(1), idle_id(2));
    }
}
