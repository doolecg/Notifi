//! One thread with a message loop for everything in the energy features that needs a window or a
//! hook: lock, sleep and lid notifications, the dim and blackout covers, and the cleaning-mode input hooks.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc::{self, Receiver, Sender},
        Mutex, OnceLock,
    },
    time::Instant,
};

use notifi_platform::Rect;
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{COLORREF, HANDLE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, DrawTextW, EndPaint, FillRect, GetStockObject, InvalidateRect, SelectObject, SetBkMode, SetTextColor,
            BLACK_BRUSH, DEFAULT_GUI_FONT, DT_CENTER, DT_VCENTER, DT_WORDBREAK, HBRUSH, PAINTSTRUCT, TRANSPARENT,
        },
        System::{
            LibraryLoader::GetModuleHandleW,
            Power::{RegisterPowerSettingNotification, RegisterSuspendResumeNotification, POWERBROADCAST_SETTING},
            RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION},
            SystemServices::GUID_LIDSWITCH_STATE_CHANGE,
            Threading::GetCurrentThreadId,
        },
        UI::{
            Input::KeyboardAndMouse::{
                SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEINPUT, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU,
                VK_RWIN, VK_SHIFT,
            },
            WindowsAndMessaging::{
                CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
                GetWindowLongPtrW, PostThreadMessageW, RegisterClassExW, SetCursor, SetLayeredWindowAttributes, SetTimer,
                SetWindowLongPtrW, SetWindowPos, SetWindowsHookExW, ShowWindow, TranslateMessage, UnhookWindowsHookEx,
                DEVICE_NOTIFY_WINDOW_HANDLE, GWLP_USERDATA, HHOOK, HWND_TOPMOST, KBDLLHOOKSTRUCT, LWA_ALPHA, MSG,
                PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, PBT_APMSUSPEND, PBT_POWERSETTINGCHANGE, SWP_NOACTIVATE, SW_SHOWNOACTIVATE,
                WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_ERASEBKGND, WM_KEYDOWN, WM_MOUSEMOVE, WM_PAINT, WM_POWERBROADCAST,
                WM_SETCURSOR, WM_SYSKEYDOWN, WM_TIMER, WM_WTSSESSION_CHANGE, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
                WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_OVERLAPPED, WS_POPUP, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
            },
        },
    },
};

use super::logic::UnlockCounter;

pub const SUPPORTED: bool = true;

/// Tags the pointer moves we inject, so the cleaning-mode hook lets them through.
const NUDGE_TAG: usize = 0x4E_54_46_45;
const WM_CMD: u32 = WM_APP;
const WM_UNLOCKED: u32 = WM_APP + 1;
const KIND_DIM: isize = 0;
const KIND_COVER: isize = 1;
const KIND_MARKER: isize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sys {
    Locked,
    Unlocked,
    Suspending,
    Resumed,
    /// The lid switch changed; true when closed.
    Lid(bool),
    /// The unlock key sequence was typed.
    CleaningUnlocked,
}

pub struct CleanSpec {
    /// One rectangle per display for the blackout, or the single corner for the marker.
    pub rects: Vec<Rect>,
    pub blackout: bool,
    pub allow_pointer: bool,
    pub show_timer: bool,
    pub presses: u32,
    pub window_ms: u64,
}

enum Cmd {
    Dim { key: String, rect: Rect, alpha: u8 },
    RemoveDim(String),
    Clean(CleanSpec),
    CleanStop,
}

struct Clean {
    counter: UnlockCounter,
    allow_pointer: bool,
    show_timer: bool,
    presses: u32,
    started: Instant,
}

static SENDER: Mutex<Option<Sender<Cmd>>> = Mutex::new(None);
static THREAD: AtomicU32 = AtomicU32::new(0);
static EVENTS: OnceLock<Box<dyn Fn(Sys) + Send + Sync>> = OnceLock::new();
static CLEAN: Mutex<Option<Clean>> = Mutex::new(None);

#[derive(Default)]
struct Local {
    dims: HashMap<String, HWND>,
    covers: Vec<HWND>,
    hooks: Vec<HHOOK>,
}

fn emit(event: Sys) {
    if let Some(f) = EVENTS.get() {
        f(event);
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Starts the thread. Events are delivered on it, so handlers must return quickly or hand off.
pub fn start(on_event: impl Fn(Sys) + Send + Sync + 'static) {
    if EVENTS.set(Box::new(on_event)).is_err() {
        return;
    }
    let (tx, rx) = mpsc::channel();
    *SENDER.lock().unwrap() = Some(tx);
    let _ = std::thread::Builder::new().name("notifi-energy".into()).spawn(move || unsafe { run(rx) });
}

fn send(cmd: Cmd) {
    if let Some(tx) = SENDER.lock().unwrap().as_ref() {
        let _ = tx.send(cmd);
    }
    let thread = THREAD.load(Ordering::SeqCst);
    if thread != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread, WM_CMD, WPARAM(0), LPARAM(0));
        }
    }
}

/// Covers `rect` with black at `alpha` (0 removes it). Clicks pass straight through.
pub fn dim(key: &str, rect: Rect, alpha: u8) {
    if alpha == 0 {
        send(Cmd::RemoveDim(key.to_string()));
    } else {
        send(Cmd::Dim { key: key.to_string(), rect, alpha });
    }
}

pub fn remove_dim(key: &str) {
    send(Cmd::RemoveDim(key.to_string()));
}

pub fn clean_start(spec: CleanSpec) {
    send(Cmd::Clean(spec));
}

pub fn clean_stop() {
    send(Cmd::CleanStop);
}

/// Moves the pointer one pixel and back; the events carry [`NUDGE_TAG`].
pub fn nudge() -> bool {
    let step = |dx: i32| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy: 0, mouseData: 0, dwFlags: MOUSEEVENTF_MOVE, time: 0, dwExtraInfo: NUDGE_TAG } },
    };
    unsafe { SendInput(&[step(1), step(-1)], std::mem::size_of::<INPUT>() as i32) == 2 }
}

unsafe fn run(rx: Receiver<Cmd>) {
    let Ok(module) = GetModuleHandleW(None) else { return };
    let events_class = wide("NotifiEnergyEvents");
    let cover_class = wide("NotifiEnergyCover");
    let black = HBRUSH(GetStockObject(BLACK_BRUSH).0);
    RegisterClassExW(&WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(events_proc),
        hInstance: module.into(),
        lpszClassName: PCWSTR(events_class.as_ptr()),
        ..Default::default()
    });
    RegisterClassExW(&WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(cover_proc),
        hInstance: module.into(),
        hbrBackground: black,
        lpszClassName: PCWSTR(cover_class.as_ptr()),
        ..Default::default()
    });
    // A hidden top-level window: message-only windows do not get the sleep broadcasts.
    if let Ok(hwnd) = CreateWindowExW(
        Default::default(),
        PCWSTR(events_class.as_ptr()),
        PCWSTR(events_class.as_ptr()),
        WS_OVERLAPPED,
        0,
        0,
        0,
        0,
        None,
        None,
        Some(module.into()),
        None,
    ) {
        let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
        let _ = RegisterSuspendResumeNotification(HANDLE(hwnd.0), DEVICE_NOTIFY_WINDOW_HANDLE);
        let _ = RegisterPowerSettingNotification(HANDLE(hwnd.0), &GUID_LIDSWITCH_STATE_CHANGE, DEVICE_NOTIFY_WINDOW_HANDLE);
    }
    THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);

    let mut local = Local::default();
    drain(&rx, &mut local, module.into(), &cover_class);
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
        match msg.message {
            WM_CMD => drain(&rx, &mut local, module.into(), &cover_class),
            WM_UNLOCKED => {
                stop_clean(&mut local);
                emit(Sys::CleaningUnlocked);
            }
            _ => {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

unsafe fn drain(rx: &Receiver<Cmd>, local: &mut Local, module: windows::Win32::Foundation::HINSTANCE, class: &[u16]) {
    while let Ok(cmd) = rx.try_recv() {
        match cmd {
            Cmd::Dim { key, rect, alpha } => {
                let hwnd = match local.dims.get(&key) {
                    Some(h) => *h,
                    None => match make_cover(module, class, rect, KIND_DIM, true) {
                        Some(h) => {
                            local.dims.insert(key, h);
                            h
                        }
                        None => continue,
                    },
                };
                let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), rect.x, rect.y, rect.width as i32, rect.height as i32, SWP_NOACTIVATE);
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
            }
            Cmd::RemoveDim(key) => {
                if let Some(h) = local.dims.remove(&key) {
                    let _ = DestroyWindow(h);
                }
            }
            Cmd::Clean(spec) => start_clean(local, module, class, spec),
            Cmd::CleanStop => stop_clean(local),
        }
    }
}

unsafe fn make_cover(module: windows::Win32::Foundation::HINSTANCE, class: &[u16], rect: Rect, kind: isize, layered: bool) -> Option<HWND> {
    let mut ex = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
    if layered {
        ex |= WS_EX_LAYERED | WS_EX_TRANSPARENT;
    }
    let hwnd = CreateWindowExW(
        ex,
        PCWSTR(class.as_ptr()),
        PCWSTR(class.as_ptr()),
        WS_POPUP,
        rect.x,
        rect.y,
        rect.width as i32,
        rect.height as i32,
        None,
        None,
        Some(module),
        None,
    )
    .ok()?;
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, kind);
    if layered {
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA);
    }
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    Some(hwnd)
}

unsafe fn start_clean(local: &mut Local, module: windows::Win32::Foundation::HINSTANCE, class: &[u16], spec: CleanSpec) {
    stop_clean(local);
    let kind = if spec.blackout { KIND_COVER } else { KIND_MARKER };
    for rect in &spec.rects {
        if let Some(h) = make_cover(module, class, *rect, kind, false) {
            let _ = SetWindowPos(h, Some(HWND_TOPMOST), rect.x, rect.y, rect.width as i32, rect.height as i32, SWP_NOACTIVATE);
            SetTimer(Some(h), 1, 1000, None);
            local.covers.push(h);
        }
    }
    *CLEAN.lock().unwrap() = Some(Clean {
        counter: UnlockCounter::new(spec.presses, spec.window_ms),
        allow_pointer: spec.allow_pointer,
        show_timer: spec.show_timer,
        presses: spec.presses,
        started: Instant::now(),
    });
    for (id, proc) in [(WH_KEYBOARD_LL, keyboard_hook as unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT), (WH_MOUSE_LL, mouse_hook as unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT)] {
        if let Ok(h) = SetWindowsHookExW(id, Some(proc), None, 0) {
            local.hooks.push(h);
        }
    }
}

unsafe fn stop_clean(local: &mut Local) {
    for h in local.hooks.drain(..) {
        let _ = UnhookWindowsHookEx(h);
    }
    *CLEAN.lock().unwrap() = None;
    for h in local.covers.drain(..) {
        let _ = DestroyWindow(h);
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    let down = wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN;
    if down {
        if let Ok(mut guard) = CLEAN.try_lock() {
            if let Some(clean) = guard.as_mut() {
                let vk = info.vkCode as u16;
                if vk == VK_ESCAPE.0 {
                    let now = clean.started.elapsed().as_millis() as u64;
                    if clean.counter.press(now) {
                        let _ = PostThreadMessageW(THREAD.load(Ordering::SeqCst), WM_UNLOCKED, WPARAM(0), LPARAM(0));
                    }
                } else if is_modifier(vk) {
                    clean.counter.reset();
                }
            }
        }
    }
    LRESULT(1)
}

fn is_modifier(vk: u16) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
        || [VK_SHIFT.0, VK_CONTROL.0, VK_MENU.0, VK_LWIN.0, VK_RWIN.0].contains(&vk)
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    let info = &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::MSLLHOOKSTRUCT);
    let allow_move = CLEAN.try_lock().ok().and_then(|g| g.as_ref().map(|c| c.allow_pointer)).unwrap_or(false);
    if info.dwExtraInfo == NUDGE_TAG || (allow_move && wparam.0 as u32 == WM_MOUSEMOVE) {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    LRESULT(1)
}

unsafe extern "system" fn events_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_WTSSESSION_CHANGE if wp.0 as u32 == WTS_SESSION_LOCK => emit(Sys::Locked),
        WM_WTSSESSION_CHANGE if wp.0 as u32 == WTS_SESSION_UNLOCK => emit(Sys::Unlocked),
        WM_POWERBROADCAST => match wp.0 as u32 {
            PBT_APMSUSPEND => emit(Sys::Suspending),
            PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => emit(Sys::Resumed),
            PBT_POWERSETTINGCHANGE if lp.0 != 0 => {
                let setting = &*(lp.0 as *const POWERBROADCAST_SETTING);
                if setting.PowerSetting == GUID_LIDSWITCH_STATE_CHANGE && setting.DataLength >= 4 {
                    let state = std::ptr::read_unaligned(setting.Data.as_ptr() as *const u32);
                    emit(Sys::Lid(state == 0));
                }
            }
            _ => {}
        },
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

unsafe extern "system" fn cover_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let kind = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    match msg {
        WM_ERASEBKGND => return LRESULT(1),
        WM_SETCURSOR if kind == KIND_COVER => {
            SetCursor(None);
            return LRESULT(1);
        }
        WM_TIMER => {
            let _ = InvalidateRect(Some(hwnd), None, false);
            return LRESULT(0);
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            FillRect(hdc, &rect, HBRUSH(GetStockObject(BLACK_BRUSH).0));
            if kind != KIND_DIM {
                let text = clean_text(kind == KIND_MARKER);
                if !text.is_empty() {
                    SelectObject(hdc, GetStockObject(DEFAULT_GUI_FONT));
                    SetBkMode(hdc, TRANSPARENT);
                    SetTextColor(hdc, COLORREF(0x00D0D0D0));
                    let mut text = wide(&text);
                    text.pop();
                    DrawTextW(hdc, &mut text, &mut rect, DT_CENTER | DT_VCENTER | DT_WORDBREAK);
                }
            }
            let _ = EndPaint(hwnd, &ps);
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

fn clean_text(marker: bool) -> String {
    let guard = CLEAN.lock().unwrap();
    let Some(clean) = guard.as_ref() else { return String::new() };
    let mut text = if marker { format!("Cleaning mode\nEsc x{} to unlock", clean.presses) } else { format!("Cleaning mode\nPress Esc {} times to unlock", clean.presses) };
    if clean.show_timer {
        let s = clean.started.elapsed().as_secs();
        text.push_str(&format!("\n{}:{:02}", s / 60, s % 60));
    }
    text
}
