//! Top-level windows on Windows: EnumWindows plus DWM for real bounds and cloaking.

use std::collections::HashMap;
use std::sync::Mutex;

use windows::core::BOOL;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindow, GetWindowLongW, GetWindowRect, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed, GWL_EXSTYLE, GWL_STYLE, GW_OWNER,
    WS_CAPTION, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

use crate::common::{covers, Rect};
use crate::error::Result;
use crate::traits::*;
use crate::win::Win;

#[path = "win_window_control.rs"]
mod control;

/// Shell windows that fill the screen without being an app (desktop, taskbars).
pub(crate) const SHELL_CLASSES: &[&str] = &["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"];

fn rect(r: RECT) -> Rect {
    Rect {
        x: r.left,
        y: r.top,
        width: (r.right - r.left).max(0) as u32,
        height: (r.bottom - r.top).max(0) as u32,
    }
}

pub(crate) unsafe fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = GetClassNameW(hwnd, &mut buf);
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

unsafe fn title(hwnd: HWND) -> String {
    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let n = GetWindowTextW(hwnd, &mut buf);
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Visible bounds without the invisible resize border, in physical pixels.
pub(crate) unsafe fn bounds(hwnd: HWND) -> Rect {
    let mut r = RECT::default();
    let size = std::mem::size_of::<RECT>() as u32;
    if DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut RECT as *mut _, size).is_err() {
        let _ = GetWindowRect(hwnd, &mut r);
    }
    rect(r)
}

unsafe fn cloaked(hwnd: HWND) -> bool {
    let mut v = 0u32;
    DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut v as *mut u32 as *mut _, 4).is_ok() && v != 0
}

/// The GDI device name ("\\.\DISPLAY1") of the monitor holding most of the window; matches `DisplayInfo::id`.
pub(crate) unsafe fn monitor_name(hwnd: HWND) -> Option<String> {
    let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFOEXW { monitorInfo: MONITORINFO { cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32, ..Default::default() }, ..Default::default() };
    if !GetMonitorInfoW(mon, &mut info.monitorInfo).as_bool() {
        return None;
    }
    let end = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
    Some(String::from_utf16_lossy(&info.szDevice[..end]))
}

static EXE_NAMES: Mutex<Option<HashMap<u32, Option<String>>>> = Mutex::new(None);

pub(crate) fn exe_name(pid: u32) -> Option<String> {
    let mut guard = EXE_NAMES.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some(n) = cache.get(&pid) {
        return n.clone();
    }
    let name = unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok().and_then(|h| {
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len);
            let _ = CloseHandle(h);
            r.ok()?;
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            std::path::Path::new(&path).file_name()?.to_str().map(str::to_string)
        })
    };
    if cache.len() > 1024 {
        cache.clear();
    }
    cache.insert(pid, name.clone());
    name
}

pub(crate) unsafe fn info(hwnd: HWND, foreground: HWND) -> WindowInfo {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    WindowInfo {
        id: WindowId(hwnd.0 as usize as u64),
        title: title(hwnd),
        app_id: (pid != 0).then(|| exe_name(pid)).flatten(),
        pid: (pid != 0).then_some(pid),
        bounds: bounds(hwnd),
        monitor: monitor_name(hwnd),
        minimized: IsIconic(hwnd).as_bool(),
        focused: hwnd == foreground,
    }
}

/// The windows Alt+Tab would show: visible, uncloaked, unowned (or app-styled), titled, not tool windows.
pub(crate) fn is_app_window(visible: bool, cloaked: bool, owned: bool, ex_style: u32, has_title: bool) -> bool {
    let app = ex_style & WS_EX_APPWINDOW.0 != 0;
    let tool = ex_style & WS_EX_TOOLWINDOW.0 != 0;
    visible && !cloaked && has_title && (app || (!owned && !tool))
}

/// Fullscreen means filling the monitor, not just being maximised (which with an auto-hide taskbar fills it too).
pub(crate) fn is_fullscreen(class: &str, bounds: Rect, monitor: Rect, maximised_with_caption: bool) -> bool {
    !SHELL_CLASSES.contains(&class) && !maximised_with_caption && covers(bounds, monitor)
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let out = &mut *(lparam.0 as *mut Vec<HWND>);
    out.push(hwnd);
    BOOL(1)
}

impl WindowManager for Win {
    fn list(&self) -> Result<Vec<WindowInfo>> {
        let mut all: Vec<HWND> = Vec::new();
        unsafe {
            EnumWindows(Some(collect), LPARAM(&mut all as *mut Vec<HWND> as isize))
                .map_err(crate::win_audio::werr("EnumWindows"))?;
            let fg = GetForegroundWindow();
            Ok(all
                .into_iter()
                .filter(|&h| {
                    let owned = GetWindow(h, GW_OWNER).map(|o| !o.is_invalid()).unwrap_or(false);
                    let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
                    is_app_window(IsWindowVisible(h).as_bool(), cloaked(h), owned, ex, GetWindowTextLengthW(h) > 0)
                        && !SHELL_CLASSES.contains(&class_name(h).as_str())
                })
                .map(|h| info(h, fg))
                .collect())
        }
    }

    fn focus(&self, id: WindowId) -> Result<()> {
        control::focus(id)
    }
    fn set_bounds(&self, id: WindowId, bounds: Rect) -> Result<()> {
        control::set_bounds(id, bounds)
    }
    fn minimize(&self, id: WindowId) -> Result<()> {
        control::minimize(id)
    }
    fn close(&self, id: WindowId) -> Result<()> {
        control::close(id)
    }
    fn maximize(&self, id: WindowId) -> Result<()> {
        control::maximize(id)
    }
    fn restore(&self, id: WindowId) -> Result<()> {
        control::restore(id)
    }
    fn set_decorations(&self, id: WindowId, on: bool) -> Result<()> {
        control::set_decorations(id, on)
    }
    fn window_state(&self, id: WindowId) -> Result<crate::window_control::WindowState> {
        control::window_state(id)
    }
    fn work_areas(&self) -> Result<Vec<crate::window_control::WorkArea>> {
        control::work_areas()
    }
    fn pointer(&self) -> Result<crate::window_control::PointerState> {
        control::pointer()
    }
    fn windows_of_pid(&self, pid: u32) -> Result<Vec<WindowInfo>> {
        control::windows_of_pid(pid)
    }
    fn quit_process(&self, pid: u32) -> Result<()> {
        control::quit_process(pid)
    }
    fn desktop_state(&self) -> Result<crate::window_control::DesktopState> {
        control::desktop_state()
    }
    fn switch_desktop(&self, n: u32) -> Result<()> {
        control::switch_desktop(n)
    }
    fn create_desktop(&self) -> Result<()> {
        control::create_desktop()
    }
    fn move_to_desktop(&self, id: WindowId, n: u32) -> Result<()> {
        control::move_to_desktop(id, n)
    }
    fn events(&self) -> Result<crate::common::EventStream<WindowEvent>> {
        control::events()
    }

    fn foreground(&self) -> Result<Option<WindowInfo>> {
        unsafe {
            let fg = GetForegroundWindow();
            if fg.is_invalid() || SHELL_CLASSES.contains(&class_name(fg).as_str()) {
                return Ok(None);
            }
            Ok(Some(info(fg, fg)))
        }
    }

    fn foreground_fullscreen(&self, monitor: Rect) -> Result<bool> {
        unsafe {
            let fg = GetForegroundWindow();
            if fg.is_invalid() || IsIconic(fg).as_bool() {
                return Ok(false);
            }
            let style = GetWindowLongW(fg, GWL_STYLE) as u32;
            let maximised_with_caption = IsZoomed(fg).as_bool() && style & WS_CAPTION.0 == WS_CAPTION.0;
            Ok(is_fullscreen(&class_name(fg), bounds(fg), monitor, maximised_with_caption))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_window_rules() {
        assert!(is_app_window(true, false, false, 0, true));
        assert!(!is_app_window(true, true, false, 0, true));
        assert!(!is_app_window(true, false, true, 0, true));
        assert!(is_app_window(true, false, true, WS_EX_APPWINDOW.0, true));
        assert!(!is_app_window(true, false, false, WS_EX_TOOLWINDOW.0, true));
        assert!(!is_app_window(false, false, false, 0, true));
        assert!(!is_app_window(true, false, false, 0, false));
    }

    #[test]
    fn fullscreen_rules() {
        let mon = Rect { x: 0, y: 0, width: 1920, height: 1080 };
        assert!(is_fullscreen("Chrome_WidgetWin_1", mon, mon, false));
        assert!(!is_fullscreen("Chrome_WidgetWin_1", mon, mon, true));
        assert!(!is_fullscreen("WorkerW", mon, mon, false));
        assert!(!is_fullscreen("Notepad", Rect { x: 0, y: 0, width: 1920, height: 1040 }, mon, false));
    }

    #[test]
    fn lists_windows() {
        let list = WindowManager::list(&Win).unwrap();
        assert!(list.iter().all(|w| !w.title.is_empty()));
    }
}
