//! Window facts the app switcher and taskbar preview need beyond [`crate::WindowManager::list`]:
//! windows on other virtual desktops, windows hidden to the tray, and background apps with no window.
//!
//! Other desktops use the documented `IVirtualDesktopManager`: windows there are still visible but
//! cloaked by the shell, and `IsWindowOnCurrentVirtualDesktop` tells them apart from shell-cloaked
//! windows of the current desktop (suspended store apps), which are skipped.
//!
//! Windows has no call that lists tray apps. Windows 11 (22H2 and later) keeps one registry key per
//! notification icon under `Control Panel\NotifyIconSettings`, with the program's path; those file
//! names are the "tray apps". On older Windows that key does not exist, so no window counts as
//! hidden to the tray and no background app is found.

use std::collections::HashSet;

use windows::core::{BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWM_CLOAKED_SHELL};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ, RRF_RT_REG_SZ,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindow, GetWindowLongW, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible,
    IsZoomed, GWL_EXSTYLE, GWL_STYLE, GW_OWNER, WS_CAPTION, WS_EX_TOOLWINDOW,
};

use crate::common::Rect;
use crate::error::{PlatformError, Result};
use crate::traits::WindowInfo;
use crate::win_windows::{bounds, class_name, exe_name, info, is_app_window, is_fullscreen, SHELL_CLASSES};

/// A window with the extra facts the switcher filters on.
#[derive(Debug, Clone, PartialEq)]
pub struct Detail {
    pub info: WindowInfo,
    /// On a virtual desktop other than the current one.
    pub other_desktop: bool,
    /// Fills its whole display (only worked out for windows on other desktops).
    pub fullscreen: bool,
    /// Not visible: the app was hidden to its tray icon.
    pub tray_hidden: bool,
}

/// A running app with a tray icon and no window of its own to list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Background {
    pub pid: u32,
    /// Executable file name, like `WindowInfo::app_id`.
    pub app_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Current,
    OtherDesktop,
    Skip,
}

/// Where a visible window lives, from its DWM cloak flags and whether the shell says it is on the current desktop.
fn place(cloak: u32, on_current_desktop: bool) -> Place {
    match (cloak, on_current_desktop) {
        (0, _) => Place::Current,
        (c, false) if c == DWM_CLOAKED_SHELL => Place::OtherDesktop,
        _ => Place::Skip,
    }
}

/// A hidden window worth listing: a real, titled, top-level app window of a program that has a tray icon.
fn hidden_candidate(owned: bool, ex_style: u32, has_title: bool, size: (u32, u32), class: &str, in_tray: bool) -> bool {
    in_tray
        && has_title
        && !owned
        && ex_style & WS_EX_TOOLWINDOW.0 == 0
        && size.0 >= 100
        && size.1 >= 50
        && !SHELL_CLASSES.contains(&class)
}

/// For each process not already showing a window, its biggest hidden window.
fn pick_hidden(candidates: &[(u32, u64, u64)], showing: &HashSet<u32>) -> Vec<u64> {
    let mut best: Vec<(u32, u64, u64)> = Vec::new();
    for &(pid, id, area) in candidates {
        if showing.contains(&pid) {
            continue;
        }
        match best.iter_mut().find(|b| b.0 == pid) {
            Some(b) if b.2 >= area => {}
            Some(b) => *b = (pid, id, area),
            None => best.push((pid, id, area)),
        }
    }
    best.into_iter().map(|b| b.1).collect()
}

/// Lower-case file name of a path with either kind of separator.
fn file_name_lower(path: &str) -> Option<String> {
    let name = path.rsplit(['\\', '/']).next()?.trim();
    (!name.is_empty()).then(|| name.to_lowercase())
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    (*(lparam.0 as *mut Vec<HWND>)).push(hwnd);
    BOOL(1)
}

fn desktop_manager() -> Option<IVirtualDesktopManager> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL).ok()
    }
}

unsafe fn cloak_flags(hwnd: HWND) -> u32 {
    let mut v = 0u32;
    if DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut v as *mut u32 as *mut _, 4).is_err() {
        return 0;
    }
    v
}

unsafe fn monitor_rect(hwnd: HWND) -> Option<Rect> {
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if !GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut mi).as_bool() {
        return None;
    }
    let r = mi.rcMonitor;
    Some(Rect { x: r.left, y: r.top, width: (r.right - r.left).max(0) as u32, height: (r.bottom - r.top).max(0) as u32 })
}

unsafe fn window_fullscreen(hwnd: HWND, class: &str) -> bool {
    let Some(monitor) = monitor_rect(hwnd) else { return false };
    let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
    let maximised_with_caption = IsZoomed(hwnd).as_bool() && style & WS_CAPTION.0 == WS_CAPTION.0;
    is_fullscreen(class, bounds(hwnd), monitor, maximised_with_caption)
}

/// Every window Alt+Tab shows, plus windows on other virtual desktops and, when `tray_hidden` is set,
/// windows of tray apps that are hidden. Front to back.
pub fn list(tray_hidden: bool) -> Result<Vec<Detail>> {
    let mut all: Vec<HWND> = Vec::new();
    unsafe {
        EnumWindows(Some(collect), LPARAM(&mut all as *mut Vec<HWND> as isize))
            .map_err(|e| PlatformError::backend("windows", format!("EnumWindows: {e}")))?;
        let fg = GetForegroundWindow();
        let desktops = desktop_manager();
        let tray = if tray_hidden { tray_exes() } else { HashSet::new() };
        let mut out: Vec<Detail> = Vec::new();
        let mut hidden: Vec<(u32, u64, u64)> = Vec::new();
        for h in all {
            let class = class_name(h);
            if SHELL_CLASSES.contains(&class.as_str()) {
                continue;
            }
            let owned = GetWindow(h, GW_OWNER).map(|o| !o.is_invalid()).unwrap_or(false);
            let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
            let has_title = GetWindowTextLengthW(h) > 0;
            if IsWindowVisible(h).as_bool() {
                let on_current = desktops
                    .as_ref()
                    .and_then(|m| m.IsWindowOnCurrentVirtualDesktop(h).ok())
                    .map(|b| b.as_bool())
                    .unwrap_or(true);
                let kind = place(cloak_flags(h), on_current);
                if kind == Place::Skip || !is_app_window(true, false, owned, ex, has_title) {
                    continue;
                }
                let other_desktop = kind == Place::OtherDesktop;
                out.push(Detail {
                    info: info(h, fg),
                    other_desktop,
                    fullscreen: other_desktop && window_fullscreen(h, &class),
                    tray_hidden: false,
                });
            } else if !tray.is_empty() {
                let mut pid = 0u32;
                GetWindowThreadProcessId(h, Some(&mut pid));
                let b = bounds(h);
                let in_tray = pid != 0 && exe_name(pid).and_then(|n| file_name_lower(&n)).is_some_and(|n| tray.contains(&n));
                if hidden_candidate(owned, ex, has_title, (b.width, b.height), &class, in_tray) {
                    hidden.push((pid, h.0 as usize as u64, b.width as u64 * b.height as u64));
                }
            }
        }
        let showing: HashSet<u32> = out.iter().filter_map(|d| d.info.pid).collect();
        for id in pick_hidden(&hidden, &showing) {
            let h = HWND(id as usize as *mut _);
            out.push(Detail { info: info(h, fg), other_desktop: false, fullscreen: false, tray_hidden: true });
        }
        Ok(out)
    }
}

fn reg_string(key: HKEY, sub: PCWSTR, value: PCWSTR) -> Option<String> {
    let mut size = 0u32;
    unsafe {
        if RegGetValueW(key, sub, value, RRF_RT_REG_SZ, None, None, Some(&mut size)).0 != 0 || size < 2 {
            return None;
        }
        let mut buf = vec![0u16; size as usize / 2 + 1];
        if RegGetValueW(key, sub, value, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)).0 != 0 {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }
}

/// Lower-case executable file names of every program that has (or had) a notification-area icon.
pub fn tray_exes() -> HashSet<String> {
    let mut out = HashSet::new();
    let root: Vec<u16> = r"Control Panel\NotifyIconSettings".encode_utf16().chain([0]).collect();
    let path: Vec<u16> = "ExecutablePath".encode_utf16().chain([0]).collect();
    unsafe {
        let mut key = HKEY(std::ptr::null_mut());
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(root.as_ptr()), None, KEY_READ, &mut key).0 != 0 {
            return out;
        }
        let mut index = 0u32;
        loop {
            let mut name = [0u16; 256];
            let mut len = name.len() as u32;
            if RegEnumKeyExW(key, index, Some(PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None).0 != 0 {
                break;
            }
            index += 1;
            if let Some(file) = reg_string(key, PCWSTR(name.as_ptr()), PCWSTR(path.as_ptr())).and_then(|p| file_name_lower(&p)) {
                out.insert(file);
            }
        }
        let _ = RegCloseKey(key);
    }
    out
}

fn exe_path(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// Running programs in this session that have a tray icon, one per executable. The file manager
/// (`explorer.exe`) is always included because it keeps running with no window open.
pub fn background_apps() -> Vec<Background> {
    let tray = tray_exes();
    let mut out: Vec<Background> = Vec::new();
    unsafe {
        let mut mine = 0u32;
        let _ = ProcessIdToSessionId(GetCurrentProcessId(), &mut mine);
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snap, &mut entry).is_ok();
        while more {
            let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let file = String::from_utf16_lossy(&entry.szExeFile[..end]);
            let lower = file.to_lowercase();
            let pid = entry.th32ProcessID;
            if (tray.contains(&lower) || lower == "explorer.exe") && pid != GetCurrentProcessId() && !out.iter().any(|b| b.app_id.to_lowercase() == lower) {
                let mut session = u32::MAX;
                let _ = ProcessIdToSessionId(pid, &mut session);
                if session == mine {
                    if let Some(path) = exe_path(pid) {
                        out.push(Background { pid, app_id: file, path });
                    }
                }
            }
            more = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_placement() {
        assert_eq!(place(0, true), Place::Current);
        assert_eq!(place(0, false), Place::Current);
        assert_eq!(place(DWM_CLOAKED_SHELL, false), Place::OtherDesktop);
        assert_eq!(place(DWM_CLOAKED_SHELL, true), Place::Skip);
        assert_eq!(place(1, false), Place::Skip);
        assert_eq!(place(4, false), Place::Skip);
    }

    #[test]
    fn hidden_windows_need_a_tray_icon_and_a_real_window() {
        assert!(hidden_candidate(false, 0, true, (800, 600), "Chrome_WidgetWin_1", true));
        assert!(!hidden_candidate(false, 0, true, (800, 600), "Chrome_WidgetWin_1", false));
        assert!(!hidden_candidate(true, 0, true, (800, 600), "x", true));
        assert!(!hidden_candidate(false, WS_EX_TOOLWINDOW.0, true, (800, 600), "x", true));
        assert!(!hidden_candidate(false, 0, false, (800, 600), "x", true));
        assert!(!hidden_candidate(false, 0, true, (1, 1), "x", true));
        assert!(!hidden_candidate(false, 0, true, (800, 600), "Progman", true));
    }

    #[test]
    fn one_hidden_window_per_process_and_none_when_it_shows_one() {
        let c = [(10, 1, 100), (10, 2, 500), (11, 3, 50), (12, 4, 900)];
        let showing: HashSet<u32> = [12].into_iter().collect();
        assert_eq!(pick_hidden(&c, &showing), vec![2, 3]);
    }

    #[test]
    fn executable_file_names() {
        assert_eq!(file_name_lower(r"C:\Program Files\Foo\Bar.EXE").as_deref(), Some("bar.exe"));
        assert_eq!(file_name_lower(r"{GUID}\Foo\bar.exe").as_deref(), Some("bar.exe"));
        assert_eq!(file_name_lower("bar.exe").as_deref(), Some("bar.exe"));
        assert_eq!(file_name_lower(r"C:\dir\"), None);
    }

    #[test]
    fn lists_windows_and_background_apps() {
        let list = list(true).unwrap();
        assert!(list.iter().all(|d| !d.info.title.is_empty()));
        let _ = background_apps();
    }
}
