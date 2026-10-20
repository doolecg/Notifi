//! The radial menu's own low-level hooks. The mouse hook is installed while a button trigger is set
//! or the wheel is open; the keyboard hook only while the wheel is open (to catch Esc).

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Mutex;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_XBUTTONDOWN,
    WM_XBUTTONUP,
};

use super::Event;

const VK_ESCAPE: u32 = 0x1B;

/// 0 none, 1 Back (X1), 2 Forward (X2).
static BUTTON: AtomicU32 = AtomicU32::new(0);
static OPEN: AtomicBool = AtomicBool::new(false);
static CENTER_X: AtomicI32 = AtomicI32::new(0);
static CENTER_Y: AtomicI32 = AtomicI32::new(0);
static RADIUS: AtomicI32 = AtomicI32::new(0);
/// Left, right and middle button-ups still owed a swallow because their button-down was swallowed.
static OWED_UP: AtomicU32 = AtomicU32::new(0);
static THREAD: AtomicU32 = AtomicU32::new(0);
static TX: Mutex<Option<Sender<Event>>> = Mutex::new(None);

fn send(event: Event) {
    if let Some(tx) = TX.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = tx.send(event);
    }
}

fn inside(x: i32, y: i32) -> bool {
    let (dx, dy, r) = (
        i64::from(x - CENTER_X.load(Ordering::SeqCst)),
        i64::from(y - CENTER_Y.load(Ordering::SeqCst)),
        i64::from(RADIUS.load(Ordering::SeqCst)),
    );
    dx * dx + dy * dy <= r * r
}

/// `true` swallows the event.
fn handle_mouse(msg: u32, info: &MSLLHOOKSTRUCT) -> bool {
    match msg {
        WM_XBUTTONDOWN | WM_XBUTTONUP => {
            let which = (info.mouseData >> 16) & 0xFFFF;
            if which == 0 || which != BUTTON.load(Ordering::SeqCst) {
                return false;
            }
            send(if msg == WM_XBUTTONDOWN { Event::ButtonDown } else { Event::ButtonUp });
            true
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            if !OPEN.load(Ordering::SeqCst) {
                return false;
            }
            let in_ring = inside(info.pt.x, info.pt.y);
            send(Event::Click { inside: in_ring });
            if in_ring {
                OWED_UP.fetch_or(bit(msg), Ordering::SeqCst);
            }
            in_ring
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => OWED_UP.fetch_and(!bit(msg), Ordering::SeqCst) & bit(msg) != 0,
        _ => false,
    }
}

fn bit(msg: u32) -> u32 {
    match msg {
        WM_LBUTTONDOWN | WM_LBUTTONUP => 1,
        WM_RBUTTONDOWN | WM_RBUTTONUP => 2,
        _ => 4,
    }
}

unsafe extern "system" fn mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        if info.flags & LLMHF_INJECTED == 0 && handle_mouse(wparam.0 as u32, info) {
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && OPEN.load(Ordering::SeqCst) && wparam.0 as u32 == WM_KEYDOWN {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if info.vkCode == VK_ESCAPE {
            send(Event::Escape);
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

pub fn start(tx: Sender<Event>) {
    *TX.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
}

/// Sets which extra button opens the wheel (0 none, 1 Back, 2 Forward).
pub fn set_button(button: u32) {
    BUTTON.store(button, Ordering::SeqCst);
    sync();
}

/// Marks the wheel open (centre and radius in screen pixels) or closed.
pub fn set_open(ring: Option<(i32, i32, i32)>) {
    if let Some((x, y, r)) = ring {
        CENTER_X.store(x, Ordering::SeqCst);
        CENTER_Y.store(y, Ordering::SeqCst);
        RADIUS.store(r, Ordering::SeqCst);
    }
    OPEN.store(ring.is_some(), Ordering::SeqCst);
    sync();
}

fn sync() {
    let tid = THREAD.load(Ordering::SeqCst);
    if tid != 0 {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_APP, WPARAM(0), LPARAM(0));
        }
        return;
    }
    if BUTTON.load(Ordering::SeqCst) == 0 && !OPEN.load(Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new().name("radial-hooks".into()).spawn(|| unsafe {
        THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
        let mut ms: Option<HHOOK> = None;
        let mut kb: Option<HHOOK> = None;
        let update = |ms: &mut Option<HHOOK>, kb: &mut Option<HHOOK>| {
            let open = OPEN.load(Ordering::SeqCst);
            let want_mouse = open || BUTTON.load(Ordering::SeqCst) != 0;
            if want_mouse && ms.is_none() {
                *ms = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), None, 0).ok();
            } else if !want_mouse {
                if let Some(h) = ms.take() {
                    let _ = UnhookWindowsHookEx(h);
                }
            }
            if open && kb.is_none() {
                *kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), None, 0).ok();
            } else if !open {
                if let Some(h) = kb.take() {
                    let _ = UnhookWindowsHookEx(h);
                }
            }
        };
        update(&mut ms, &mut kb);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_APP {
                update(&mut ms, &mut kb);
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}
