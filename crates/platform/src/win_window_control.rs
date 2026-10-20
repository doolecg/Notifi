//! Moving, sizing and organising windows on Windows: SetWindowPos with the invisible frame
//! compensated, SetWinEventHook for events, virtual desktops through the registry and the public
//! IVirtualDesktopManager.
//!
//! Windows has no public call to switch desktops or to move another process's window between
//! them. Switching sends the documented Win+Ctrl+Left/Right shortcut, and moving another app's
//! window is reported as unsupported; IVirtualDesktopManager::MoveWindowToDesktop only works for
//! windows of the calling process.

use std::collections::HashSet;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, Once};
use std::time::Duration;

use windows::core::GUID;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_BINARY};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_D, VK_LBUTTON, VK_LEFT, VK_LWIN, VK_MENU, VK_RBUTTON, VK_RIGHT, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetCursorPos, GetMessageW, GetWindowLongPtrW, IsWindow, PostMessageW, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, EVENT_OBJECT_DESTROY, EVENT_OBJECT_LOCATIONCHANGE,
    EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MOVESIZEEND,
    EVENT_SYSTEM_MOVESIZESTART, MSG, OBJID_WINDOW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_CLOSE,
    WS_BORDER, WS_THICKFRAME,
};

use super::*;
use crate::caps::Feature;
use crate::common::EventStream;
use crate::error::PlatformError;
use crate::window_control::{DesktopState, PointerState, WindowState, WorkArea};

fn err(what: &str, e: impl std::fmt::Display) -> PlatformError {
    PlatformError::backend("windows", format!("{what}: {e}"))
}

fn hwnd(id: WindowId) -> HWND {
    HWND(id.0 as usize as *mut _)
}

fn live(id: WindowId) -> Result<HWND> {
    let h = hwnd(id);
    if unsafe { IsWindow(Some(h)) }.as_bool() { Ok(h) } else { Err(err("window", "no longer exists")) }
}

/// How far the window's real outer rectangle reaches past its visible bounds (left, top, right, bottom).
unsafe fn frame_insets(h: HWND) -> (i32, i32, i32, i32) {
    let mut outer = RECT::default();
    let _ = GetWindowRect(h, &mut outer);
    let vis = bounds(h);
    (
        vis.x - outer.left,
        vis.y - outer.top,
        outer.right - (vis.x + vis.width as i32),
        outer.bottom - (vis.y + vis.height as i32),
    )
}

pub(super) fn set_bounds(id: WindowId, target: Rect) -> Result<()> {
    let h = live(id)?;
    unsafe {
        if IsZoomed(h).as_bool() || IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        let before = monitor_name(h);
        let apply = || {
            let (l, t, r, b) = frame_insets(h);
            SetWindowPos(
                h,
                None,
                target.x - l,
                target.y - t,
                target.width as i32 + l + r,
                target.height as i32 + t + b,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        apply().map_err(|e| err("SetWindowPos", e))?;
        if monitor_name(h) != before {
            // Moving across displays of different scale makes the window rescale itself; place it again.
            apply().map_err(|e| err("SetWindowPos", e))?;
        }
    }
    Ok(())
}

fn key(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

fn send_keys(keys: &[INPUT]) {
    unsafe {
        SendInput(keys, std::mem::size_of::<INPUT>() as i32);
    }
}

pub(super) fn focus(id: WindowId) -> Result<()> {
    let h = live(id)?;
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        // A tap on Alt lets a background process take the foreground.
        send_keys(&[key(VK_MENU, KEYBD_EVENT_FLAGS(0)), key(VK_MENU, KEYEVENTF_KEYUP)]);
        if SetForegroundWindow(h).as_bool() { Ok(()) } else { Err(err("SetForegroundWindow", "refused")) }
    }
}

pub(super) fn show(id: WindowId, cmd: windows::Win32::UI::WindowsAndMessaging::SHOW_WINDOW_CMD) -> Result<()> {
    let h = live(id)?;
    unsafe {
        let _ = ShowWindow(h, cmd);
    }
    Ok(())
}

pub(super) fn minimize(id: WindowId) -> Result<()> {
    show(id, SW_MINIMIZE)
}

pub(super) fn maximize(id: WindowId) -> Result<()> {
    show(id, SW_MAXIMIZE)
}

pub(super) fn restore(id: WindowId) -> Result<()> {
    show(id, SW_RESTORE)
}

pub(super) fn close(id: WindowId) -> Result<()> {
    let h = live(id)?;
    unsafe { PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)) }.map_err(|e| err("PostMessage", e))
}

static ORIGINAL_STYLE: Mutex<Vec<(u64, isize)>> = Mutex::new(Vec::new());
const FRAME_BITS: u32 = WS_CAPTION.0 | WS_THICKFRAME.0 | WS_BORDER.0;

pub(super) fn set_decorations(id: WindowId, on: bool) -> Result<()> {
    let h = live(id)?;
    let mut saved = ORIGINAL_STYLE.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let style = GetWindowLongPtrW(h, GWL_STYLE);
        let next = if on {
            match saved.iter().position(|(k, _)| *k == id.0) {
                Some(i) => saved.swap_remove(i).1,
                None => style | FRAME_BITS as isize,
            }
        } else {
            if !saved.iter().any(|(k, _)| *k == id.0) {
                saved.push((id.0, style));
            }
            style & !(FRAME_BITS as isize)
        };
        SetWindowLongPtrW(h, GWL_STYLE, next);
        SetWindowPos(h, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE)
            .map_err(|e| err("SetWindowPos", e))
    }
}

pub(super) fn window_state(id: WindowId) -> Result<WindowState> {
    let h = live(id)?;
    unsafe {
        let style = GetWindowLongW(h, GWL_STYLE) as u32;
        let mon = work_areas()?.into_iter().find(|a| Some(&a.display_id) == monitor_name(h).as_ref()).map(|a| a.bounds);
        let maximized = IsZoomed(h).as_bool();
        let caption_max = maximized && style & WS_CAPTION.0 == WS_CAPTION.0;
        Ok(WindowState {
            maximized,
            minimized: IsIconic(h).as_bool(),
            fullscreen: mon.is_some_and(|m| is_fullscreen(&class_name(h), bounds(h), m, caption_max)),
            resizable: style & WS_THICKFRAME.0 != 0,
        })
    }
}

unsafe extern "system" fn collect_monitor(mon: HMONITOR, _: HDC, _: *mut RECT, lparam: LPARAM) -> BOOL {
    (*(lparam.0 as *mut Vec<HMONITOR>)).push(mon);
    BOOL(1)
}

pub(super) fn work_areas() -> Result<Vec<WorkArea>> {
    let mut mons: Vec<HMONITOR> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect_monitor), LPARAM(&mut mons as *mut Vec<HMONITOR> as isize));
        Ok(mons
            .into_iter()
            .filter_map(|m| {
                let mut info = MONITORINFOEXW {
                    monitorInfo: MONITORINFO { cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32, ..Default::default() },
                    ..Default::default()
                };
                GetMonitorInfoW(m, &mut info.monitorInfo as *mut MONITORINFO).as_bool().then_some(())?;
                let end = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
                let (mut dx, mut dy) = (96u32, 96u32);
                let _ = GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
                Some(WorkArea {
                    display_id: String::from_utf16_lossy(&info.szDevice[..end]),
                    bounds: rect(info.monitorInfo.rcMonitor),
                    work: rect(info.monitorInfo.rcWork),
                    scale: dx as f32 / 96.0,
                })
            })
            .collect())
    }
}

pub(super) fn pointer() -> Result<PointerState> {
    let down = |vk: VIRTUAL_KEY| unsafe { GetAsyncKeyState(vk.0 as i32) } as u16 & 0x8000 != 0;
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p) }.map_err(|e| err("GetCursorPos", e))?;
    Ok(PointerState {
        x: p.x,
        y: p.y,
        left: down(VK_LBUTTON),
        right: down(VK_RBUTTON),
        shift: down(VK_SHIFT),
        ctrl: down(VK_CONTROL),
        alt: down(VK_MENU),
        meta: down(VK_LWIN) || down(VK_RWIN),
    })
}

unsafe extern "system" fn collect_all(hwnd: HWND, lparam: LPARAM) -> BOOL {
    (*(lparam.0 as *mut Vec<HWND>)).push(hwnd);
    BOOL(1)
}

pub(super) fn windows_of_pid(pid: u32) -> Result<Vec<WindowInfo>> {
    let mut all: Vec<HWND> = Vec::new();
    unsafe {
        EnumWindows(Some(collect_all), LPARAM(&mut all as *mut Vec<HWND> as isize)).map_err(|e| err("EnumWindows", e))?;
        let fg = GetForegroundWindow();
        Ok(all
            .into_iter()
            .filter(|&h| {
                let mut p = 0u32;
                GetWindowThreadProcessId(h, Some(&mut p));
                let b = bounds(h);
                p == pid && IsWindowVisible(h).as_bool() && !cloaked(h) && b.width > 1 && b.height > 1
            })
            .map(|h| info(h, fg))
            .collect())
    }
}

const NEVER_QUIT: &[&str] = &["explorer.exe", "csrss.exe", "winlogon.exe", "dwm.exe", "services.exe", "svchost.exe", "lsass.exe"];

pub(super) fn quit_process(pid: u32) -> Result<()> {
    if pid == 0 || pid == unsafe { GetCurrentProcessId() } {
        return Err(err("quit", "refusing to end Notifi itself"));
    }
    if exe_name(pid).is_some_and(|n| NEVER_QUIT.contains(&n.to_ascii_lowercase().as_str())) {
        return Err(err("quit", "refusing to end a system process"));
    }
    let mut all: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect_all), LPARAM(&mut all as *mut Vec<HWND> as isize));
        for h in all {
            let mut p = 0u32;
            GetWindowThreadProcessId(h, Some(&mut p));
            if p == pid {
                let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        let proc = OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, false, pid).map_err(|e| err("OpenProcess", e))?;
        if WaitForSingleObject(proc, 3000).0 != 0 {
            let _ = TerminateProcess(proc, 0);
        }
        let _ = CloseHandle(proc);
    }
    Ok(())
}

// ---- virtual desktops ----

const EXPLORER_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer";

fn reg_binary(subkey: &str, value: &str) -> Option<Vec<u8>> {
    let sub: Vec<u16> = subkey.encode_utf16().chain([0]).collect();
    let val: Vec<u16> = value.encode_utf16().chain([0]).collect();
    let (sub, val) = (windows::core::PCWSTR(sub.as_ptr()), windows::core::PCWSTR(val.as_ptr()));
    let mut size = 0u32;
    unsafe {
        let probe = RegGetValueW(HKEY_CURRENT_USER, sub, val, RRF_RT_REG_BINARY, None, None, Some(&mut size));
        if probe.is_err() || size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let read =
            RegGetValueW(HKEY_CURRENT_USER, sub, val, RRF_RT_REG_BINARY, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size));
        read.is_ok().then(|| {
            buf.truncate(size as usize);
            buf
        })
    }
}

fn guids(bytes: &[u8]) -> Vec<[u8; 16]> {
    bytes.chunks_exact(16).map(|c| c.try_into().unwrap()).collect()
}

fn desktop_ids() -> Vec<[u8; 16]> {
    reg_binary(&format!(r"{EXPLORER_KEY}\VirtualDesktops"), "VirtualDesktopIDs").map(|b| guids(&b)).unwrap_or_default()
}

/// The desktop count and the 1-based current one from Explorer's saved state; one desktop when it has none yet.
pub(super) fn desktop_state() -> Result<DesktopState> {
    let mut session = 0u32;
    unsafe {
        let _ = ProcessIdToSessionId(GetCurrentProcessId(), &mut session);
    }
    let ids = desktop_ids();
    let current = reg_binary(&format!(r"{EXPLORER_KEY}\SessionInfo\{session}\VirtualDesktops"), "CurrentVirtualDesktop")
        .or_else(|| reg_binary(&format!(r"{EXPLORER_KEY}\VirtualDesktops"), "CurrentVirtualDesktop"))
        .and_then(|b| guids(&b).into_iter().next());
    if ids.is_empty() {
        return Ok(DesktopState { count: 1, current: 1 });
    }
    let pos = current.and_then(|c| ids.iter().position(|g| *g == c)).unwrap_or(0);
    Ok(DesktopState { count: ids.len() as u32, current: pos as u32 + 1 })
}

/// Win+Ctrl plus one key, the documented desktop shortcuts.
fn desktop_chord(vk: VIRTUAL_KEY) {
    let none = KEYBD_EVENT_FLAGS(0);
    let up = KEYEVENTF_KEYUP;
    send_keys(&[key(VK_LWIN, none), key(VK_CONTROL, none), key(vk, none), key(vk, up), key(VK_CONTROL, up), key(VK_LWIN, up)]);
    std::thread::sleep(Duration::from_millis(220));
}

pub(super) fn switch_desktop(n: u32) -> Result<()> {
    let state = desktop_state()?;
    if n == 0 || n > state.count {
        return Err(err("desktop", format!("there is no desktop {n}")));
    }
    let (vk, hops) = if n > state.current { (VK_RIGHT, n - state.current) } else { (VK_LEFT, state.current - n) };
    for _ in 0..hops {
        desktop_chord(vk);
    }
    Ok(())
}

pub(super) fn create_desktop() -> Result<()> {
    desktop_chord(VK_D);
    Ok(())
}

pub(super) fn move_to_desktop(id: WindowId, n: u32) -> Result<()> {
    let h = live(id)?;
    let ids = desktop_ids();
    let raw = n.checked_sub(1).and_then(|i| ids.get(i as usize)).ok_or_else(|| err("desktop", format!("there is no desktop {n}")))?;
    let guid = GUID::from_values(
        u32::from_le_bytes(raw[0..4].try_into().unwrap()),
        u16::from_le_bytes(raw[4..6].try_into().unwrap()),
        u16::from_le_bytes(raw[6..8].try_into().unwrap()),
        raw[8..16].try_into().unwrap(),
    );
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let mgr: IVirtualDesktopManager =
            CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL).map_err(|e| err("virtual desktops", e))?;
        mgr.MoveWindowToDesktop(h, &guid).map_err(|_| {
            PlatformError::unsupported(
                Feature::WindowControl,
                "Windows only lets a program move its own windows to another desktop; other apps' windows need an undocumented interface",
            )
        })
    }
}

// ---- events ----

static SINKS: Mutex<Vec<Sender<WindowEvent>>> = Mutex::new(Vec::new());
static KNOWN: Mutex<Option<HashSet<u64>>> = Mutex::new(None);
static HOOKS: Once = Once::new();

fn dispatch(ev: WindowEvent) {
    SINKS.lock().unwrap_or_else(|e| e.into_inner()).retain(|s| s.send(ev.clone()).is_ok());
}

fn known<T>(f: impl FnOnce(&mut HashSet<u64>) -> T) -> T {
    f(KNOWN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashSet::new))
}

fn appears(h: HWND) -> bool {
    unsafe {
        let owned = GetWindow(h, GW_OWNER).map(|o| !o.is_invalid()).unwrap_or(false);
        let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
        is_app_window(IsWindowVisible(h).as_bool(), cloaked(h), owned, ex, GetWindowTextLengthW(h) > 0)
            && !SHELL_CLASSES.contains(&class_name(h).as_str())
    }
}

unsafe extern "system" fn on_event(_: HWINEVENTHOOK, event: u32, h: HWND, id_object: i32, id_child: i32, _: u32, _: u32) {
    if id_object != OBJID_WINDOW.0 || id_child != 0 || h.is_invalid() {
        return;
    }
    let id = WindowId(h.0 as usize as u64);
    match event {
        EVENT_SYSTEM_MOVESIZESTART => dispatch(WindowEvent::MoveSizeStarted(id)),
        EVENT_SYSTEM_MOVESIZEEND => dispatch(WindowEvent::MoveSizeEnded(id)),
        EVENT_SYSTEM_FOREGROUND => {
            if appears(h) {
                if known(|k| k.insert(id.0)) {
                    dispatch(WindowEvent::Created(id));
                }
                dispatch(WindowEvent::Focused(id));
            }
        }
        EVENT_OBJECT_SHOW | EVENT_OBJECT_NAMECHANGE => {
            if appears(h) && known(|k| k.insert(id.0)) {
                dispatch(WindowEvent::Created(id));
            } else if event == EVENT_OBJECT_NAMECHANGE && known(|k| k.contains(&id.0)) {
                dispatch(WindowEvent::TitleChanged(id));
            }
        }
        EVENT_OBJECT_DESTROY => {
            if known(|k| k.remove(&id.0)) {
                dispatch(WindowEvent::Destroyed(id));
            }
        }
        EVENT_OBJECT_LOCATIONCHANGE => {
            if known(|k| k.contains(&id.0)) {
                dispatch(WindowEvent::Moved(id, bounds(h)));
            }
        }
        _ => {}
    }
}

fn start_hooks() {
    HOOKS.call_once(|| {
        if let Ok(list) = WindowManager::list(&Win) {
            known(|k| k.extend(list.iter().map(|w| w.id.0)));
        }
        std::thread::Builder::new()
            .name("notifi-win-events".into())
            .spawn(|| unsafe {
                let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
                for (lo, hi) in [
                    (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
                    (EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND),
                    (EVENT_OBJECT_DESTROY, EVENT_OBJECT_NAMECHANGE),
                ] {
                    let _ = SetWinEventHook(lo, hi, None, Some(on_event), 0, 0, flags);
                }
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            })
            .ok();
    });
}

pub(super) fn events() -> Result<EventStream<WindowEvent>> {
    start_hooks();
    let (tx, rx) = channel();
    SINKS.lock().unwrap_or_else(|e| e.into_inner()).push(tx);
    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_chunks() {
        assert_eq!(guids(&[1u8; 33]).len(), 2);
    }

    #[test]
    fn work_areas_are_inside_their_displays() {
        let areas = work_areas().unwrap();
        assert!(!areas.is_empty());
        for a in areas {
            assert!(covers(a.bounds, a.work), "{} work area leaves the display", a.display_id);
            assert!(a.scale >= 1.0);
        }
    }

    #[test]
    fn desktops_have_a_current_one() {
        let s = desktop_state().unwrap();
        assert!(s.current >= 1 && s.current <= s.count);
    }

    #[test]
    fn pointer_is_readable() {
        pointer().unwrap();
    }

    #[test]
    fn refuses_to_end_itself() {
        assert!(quit_process(std::process::id()).is_err());
    }
}
