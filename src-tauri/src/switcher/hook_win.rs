//! Low-level keyboard and mouse hooks on their own thread. A WH_KEYBOARD_LL hook is the only way
//! on Windows to see a held modifier and swallow Tab before the system switcher does; the
//! WH_MOUSE_LL hook swallows a click on the active app's taskbar button.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use notifi_platform::Rect;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WM_APP, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_SYSKEYDOWN,
};

use super::keys::{Bindings, Command, Hook, Key, Mods};

/// Marks the input we inject ourselves so the hook lets it through.
const MARK: usize = 0x4E_54_46_49;
/// An unassigned virtual key: tapping it stops a lone Alt release from opening the app's menu bar.
const MASK_KEY: u16 = 0xE8;

pub struct KeyCtx {
    pub bindings: Bindings,
    pub hook: Hook,
    pub held: Vec<u32>,
    pub open: Arc<AtomicBool>,
    pub tx: Option<Sender<super::Msg>>,
}

pub struct ClickCtx {
    /// The hovered taskbar button and whether its app is the active one.
    pub target: Option<(Rect, bool)>,
    swallow_up: bool,
    pub tx: Option<Sender<(i32, i32)>>,
}

pub static KEYS: Mutex<Option<KeyCtx>> = Mutex::new(None);
pub static CLICKS: Mutex<ClickCtx> = Mutex::new(ClickCtx { target: None, swallow_up: false, tx: None });
static WANT_KEYS: AtomicBool = AtomicBool::new(false);
static WANT_MOUSE: AtomicBool = AtomicBool::new(false);
static THREAD: AtomicU32 = AtomicU32::new(0);

fn key_of(vk: u32) -> Key {
    match vk {
        0x09 => Key::Tab,
        0xC0 => Key::Backquote,
        0x20 => Key::Space,
        0x1B => Key::Escape,
        0x0D => Key::Enter,
        0x08 => Key::Backspace,
        0x2E => Key::Delete,
        0x25 => Key::Left,
        0x26 => Key::Up,
        0x27 => Key::Right,
        0x28 => Key::Down,
        0x41..=0x5A => Key::Char((b'a' + (vk - 0x41) as u8) as char),
        0x30..=0x39 => Key::Char((b'0' + (vk - 0x30) as u8) as char),
        0x60..=0x69 => Key::Char((b'0' + (vk - 0x60) as u8) as char),
        0xBD => Key::Char('-'),
        0xBE => Key::Char('.'),
        0x70..=0x87 => Key::F((vk - 0x6F) as u8),
        _ => Key::Other,
    }
}

fn is_modifier(vk: u32) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
}

fn mods_of(held: &[u32]) -> Mods {
    let any = |list: &[u32]| held.iter().any(|k| list.contains(k));
    Mods { shift: any(&[0x10, 0xA0, 0xA1]), ctrl: any(&[0x11, 0xA2, 0xA3]), alt: any(&[0x12, 0xA4, 0xA5]), meta: any(&[0x5B, 0x5C]) }
}

fn tap_mask_key() {
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(MASK_KEY), wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: MARK } },
    };
    unsafe {
        SendInput(&[key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)], std::mem::size_of::<INPUT>() as i32);
    }
}

unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    if info.dwExtraInfo == MARK || (info.flags.0 & LLKHF_INJECTED.0 != 0 && info.vkCode == MASK_KEY as u32) {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    let down = wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN;
    let vk = info.vkCode;
    let mut swallow = false;
    if let Ok(mut guard) = KEYS.try_lock() {
        if let Some(ctx) = guard.as_mut() {
            let open = ctx.open.load(Ordering::SeqCst);
            let mut cmd = None;
            if is_modifier(vk) {
                ctx.held.retain(|k| *k != vk);
                if down {
                    ctx.held.push(vk);
                }
                if !down {
                    cmd = ctx.hook.modifiers(open, mods_of(&ctx.held));
                }
            } else {
                let (s, c) = ctx.hook.key(&ctx.bindings, open, key_of(vk), down, mods_of(&ctx.held));
                swallow = s;
                cmd = c;
            }
            if let Some(cmd) = cmd {
                match cmd {
                    Command::Open { .. } => {
                        ctx.open.store(true, Ordering::SeqCst);
                        if ctx.hook.holding() {
                            tap_mask_key();
                        }
                    }
                    Command::Commit | Command::Cancel => ctx.open.store(false, Ordering::SeqCst),
                    _ => {}
                }
                if let Some(tx) = &ctx.tx {
                    let _ = tx.send(super::Msg::Key(cmd));
                }
            }
        }
    }
    if swallow {
        LRESULT(1)
    } else {
        CallNextHookEx(None, code, wparam, lparam)
    }
}

fn inside(r: Rect, x: i32, y: i32) -> bool {
    x >= r.x && y >= r.y && x < r.x + r.width as i32 && y < r.y + r.height as i32
}

unsafe extern "system" fn mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
    let msg = wparam.0 as u32;
    if info.dwExtraInfo != MARK && (msg == WM_LBUTTONDOWN || msg == WM_LBUTTONUP) {
        if let Ok(mut ctx) = CLICKS.try_lock() {
            if msg == WM_LBUTTONUP && ctx.swallow_up {
                ctx.swallow_up = false;
                return LRESULT(1);
            }
            if msg == WM_LBUTTONDOWN {
                if let (Some((rect, true)), Some(tx)) = (ctx.target, ctx.tx.as_ref()) {
                    if inside(rect, info.pt.x, info.pt.y) {
                        let _ = tx.send((info.pt.x, info.pt.y));
                        ctx.swallow_up = true;
                        return LRESULT(1);
                    }
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// Installs or removes the hooks to match what is wanted; starts the hook thread on first use.
pub fn apply(keys: bool, clicks: bool) {
    WANT_KEYS.store(keys, Ordering::SeqCst);
    WANT_MOUSE.store(clicks, Ordering::SeqCst);
    let tid = THREAD.load(Ordering::SeqCst);
    if tid != 0 {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_APP, WPARAM(0), LPARAM(0));
        }
        return;
    }
    if !keys && !clicks {
        return;
    }
    let _ = std::thread::Builder::new().name("switcher-hooks".into()).spawn(|| unsafe {
        THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
        let mut kb: Option<HHOOK> = None;
        let mut ms: Option<HHOOK> = None;
        let sync = |kb: &mut Option<HHOOK>, ms: &mut Option<HHOOK>| {
            let (wk, wm) = (WANT_KEYS.load(Ordering::SeqCst), WANT_MOUSE.load(Ordering::SeqCst));
            if wk && kb.is_none() {
                *kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), None, 0).ok();
            } else if !wk {
                if let Some(h) = kb.take() {
                    let _ = UnhookWindowsHookEx(h);
                }
            }
            if wm && ms.is_none() {
                *ms = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), None, 0).ok();
            } else if !wm {
                if let Some(h) = ms.take() {
                    let _ = UnhookWindowsHookEx(h);
                }
            }
        };
        sync(&mut kb, &mut ms);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_APP {
                sync(&mut kb, &mut ms);
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_keys() {
        assert_eq!(key_of(0x09), Key::Tab);
        assert_eq!(key_of(0x57), Key::Char('w'));
        assert_eq!(key_of(0x74), Key::F(5));
        assert_eq!(key_of(0xFF), Key::Other);
        let m = mods_of(&[0xA4, 0xA0]);
        assert!(m.alt && m.shift && !m.ctrl && !m.meta);
    }
}
