//! Windows side of [`crate::observe`]: low-level keyboard and mouse hooks on a thread with a
//! message loop, key presses turned into text with the foreground window's keyboard layout, and
//! UI Automation for the focused field.

use std::cell::RefCell;
use std::sync::mpsc::{channel, Sender};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, GetKeyboardLayout, ToUnicodeEx, VIRTUAL_KEY, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_MENU,
    VK_RMENU, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, PostQuitMessage,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSG,
    MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEMOVE, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use crate::common::EventStream;
use crate::error::{PlatformError, Result};
use crate::observe::{Field, Observed, Watch};
use crate::traits::MouseButton;

/// Shift, Ctrl, Alt, Caps Lock, the Windows keys, Num Lock, Scroll Lock and their left/right forms.
const MODIFIERS: &[u16] = &[0x10, 0x11, 0x12, 0x14, 0x5B, 0x5C, 0x90, 0x91, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5];

thread_local! {
    /// The hook procedures run on the thread that installed them, so each watcher keeps its own sink.
    static SINK: RefCell<Option<(Watch, Sender<Observed>)>> = const { RefCell::new(None) };
    static UIA: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
}

pub fn observe(watch: Watch) -> Result<EventStream<Observed>> {
    let (tx, rx) = channel();
    let (ready, started) = channel();
    std::thread::Builder::new().name("notifi-observe".into()).spawn(move || unsafe {
        SINK.with(|s| *s.borrow_mut() = Some((watch, tx)));
        let keys = if watch.keys { SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_proc), None, 0).map(Some) } else { Ok(None) };
        let mouse =
            if watch.buttons || watch.motion { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0).map(Some) } else { Ok(None) };
        let ok = keys.is_ok() && mouse.is_ok();
        let _ = ready.send(ok);
        if ok {
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        for hook in [keys, mouse].into_iter().flatten().flatten() {
            let _ = UnhookWindowsHookEx(hook);
        }
    })?;
    match started.recv() {
        Ok(true) => Ok(rx),
        _ => Err(PlatformError::backend("input hook", "Windows refused the low-level hook")),
    }
}

fn emit(event: Observed) {
    let gone = SINK.with(|s| s.borrow().as_ref().is_some_and(|(_, tx)| tx.send(event).is_err()));
    if gone {
        unsafe { PostQuitMessage(0) };
    }
}

fn watching() -> Watch {
    SINK.with(|s| s.borrow().as_ref().map(|(w, _)| *w).unwrap_or_default())
}

unsafe extern "system" fn key_proc(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 {
        let kb = unsafe { &*(l.0 as *const KBDLLHOOKSTRUCT) };
        let msg = w.0 as u32;
        if (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN) && kb.flags.0 & LLKHF_INJECTED.0 == 0 {
            for event in translate(kb.vkCode, kb.scanCode) {
                emit(event);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, w, l) }
}

unsafe extern "system" fn mouse_proc(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 {
        let ms = unsafe { &*(l.0 as *const MSLLHOOKSTRUCT) };
        if ms.flags & LLMHF_INJECTED == 0 {
            if let Some(event) = mouse_event(w.0 as u32, ms) {
                emit(event);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, w, l) }
}

fn mouse_event(msg: u32, ms: &MSLLHOOKSTRUCT) -> Option<Observed> {
    let watch = watching();
    let (x, y) = (ms.pt.x, ms.pt.y);
    let button = |button, down| watch.buttons.then_some(Observed::Button { button, down, x, y });
    let side = if ms.mouseData >> 16 == 1 { MouseButton::Back } else { MouseButton::Forward };
    match msg {
        WM_MOUSEMOVE => watch.motion.then_some(Observed::Motion { x, y }),
        WM_LBUTTONDOWN => button(MouseButton::Left, true),
        WM_LBUTTONUP => button(MouseButton::Left, false),
        WM_RBUTTONDOWN => button(MouseButton::Right, true),
        WM_RBUTTONUP => button(MouseButton::Right, false),
        WM_MBUTTONDOWN => button(MouseButton::Middle, true),
        WM_MBUTTONUP => button(MouseButton::Middle, false),
        WM_XBUTTONDOWN => button(side, true),
        WM_XBUTTONUP => button(side, false),
        _ => None,
    }
}

fn down(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000 != 0 }
}

fn translate(vk: u32, scan: u32) -> Vec<Observed> {
    if vk == VK_BACK.0 as u32 {
        return vec![Observed::Backspace];
    }
    if MODIFIERS.contains(&(vk as u16)) {
        return Vec::new();
    }
    let (ctrl, alt) = (down(VK_CONTROL), down(VK_MENU));
    // AltGr arrives as Ctrl plus right Alt and types characters on many layouts.
    let altgr = ctrl && down(VK_RMENU);
    if (ctrl || alt) && !altgr {
        return vec![Observed::Break];
    }
    let mut state = [0u8; 256];
    if down(VK_SHIFT) {
        state[VK_SHIFT.0 as usize] = 0x80;
    }
    if unsafe { GetKeyState(VK_CAPITAL.0 as i32) } & 1 != 0 {
        state[VK_CAPITAL.0 as usize] = 1;
    }
    if altgr {
        state[VK_CONTROL.0 as usize] = 0x80;
        state[VK_MENU.0 as usize] = 0x80;
    }
    let mut buf = [0u16; 8];
    // Flag 4 leaves the keyboard's dead-key state alone, so the app still gets its accents.
    let n = unsafe {
        let layout = GetKeyboardLayout(GetWindowThreadProcessId(GetForegroundWindow(), None));
        ToUnicodeEx(vk, scan, &state, &mut buf, 4, Some(layout))
    };
    if n < 0 {
        return Vec::new();
    }
    let chars: Vec<char> = char::decode_utf16(buf[..n as usize].iter().copied()).filter_map(|c| c.ok()).collect();
    if chars.is_empty() || chars.iter().any(|c| c.is_control()) {
        return vec![Observed::Break];
    }
    chars.into_iter().map(Observed::Text).collect()
}

fn automation() -> Option<IUIAutomation> {
    UIA.with(|cell| {
        if let Some(a) = cell.borrow().as_ref() {
            return Some(a.clone());
        }
        let a: IUIAutomation = unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
        }
        .ok()?;
        *cell.borrow_mut() = Some(a.clone());
        Some(a)
    })
}

pub fn focused_field() -> Field {
    let Some(element) = automation().and_then(|a| unsafe { a.GetFocusedElement() }.ok()) else { return Field::default() };
    let password = unsafe { element.CurrentIsPassword() }.is_ok_and(|b| b.as_bool());
    let selection = unsafe { element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) }
        .and_then(|p| unsafe { p.GetSelection() })
        .and_then(|ranges| unsafe { ranges.GetElement(0) })
        .and_then(|range| unsafe { range.GetText(-1) })
        .is_ok_and(|text| !text.is_empty());
    Field { password, selection }
}
