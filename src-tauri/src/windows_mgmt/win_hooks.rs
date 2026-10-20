//! Low-level keyboard and mouse hooks: quit and close protection, and modifier-drag.
//!
//! Each hook runs on its own thread with a message loop and answers from state cached elsewhere, so it
//! never blocks. Keys Notifi sends itself carry [`MARKER`] and are ignored here.

use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use notifi_platform::{Rect, WindowId};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use super::config::Config;
use super::drag::{anchor_at, drag_rect, Mode as DragMode};
use super::guard::{classify, Combo, ExtraModifier, Guard, Mode, Step, Tick};
use super::session::Tracker;
use super::{actions, app, config, overlay, platform};

/// "NOTI", set on every key Notifi injects so its own hooks let it through.
const MARKER: usize = 0x4E4F_5449;
const MIN_SIZE: (u32, u32) = (200, 100);

static KEYS: Once = Once::new();
static MOUSE: Once = Once::new();
static START: Mutex<Option<Instant>> = Mutex::new(None);

pub fn ensure(cfg: &Config) {
    if cfg.wants_key_hook() {
        KEYS.call_once(|| spawn_hook("notifi-key-hook", true));
    }
    if cfg.wants_mouse_hook() {
        MOUSE.call_once(|| spawn_hook("notifi-mouse-hook", false));
    }
}

fn now_ms() -> u64 {
    START.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Instant::now).elapsed().as_millis() as u64
}

fn spawn_hook(name: &'static str, keyboard: bool) {
    if !keyboard {
        let (tx, rx) = channel();
        *SENDER.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        std::thread::Builder::new().name("notifi-drag-worker".into()).spawn(move || worker(rx)).ok();
    }
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || unsafe {
            let module = GetModuleHandleW(None).ok().map(|m| windows::Win32::Foundation::HINSTANCE(m.0));
            let installed = if keyboard {
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_proc), module, 0)
            } else {
                SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0)
            };
            if installed.is_err() {
                eprintln!("windows_mgmt: could not install the {name}");
                return;
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .ok();
}

fn async_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000 != 0 }
}

fn key_input(vk: u32, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk as u16),
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: MARKER,
            },
        },
    }
}

fn inject(keys: &[INPUT]) {
    unsafe {
        SendInput(keys, std::mem::size_of::<INPUT>() as i32);
    }
}

// ---- quit and close protection ----

static GUARD: Mutex<Option<Guard>> = Mutex::new(None);
static HELD_VK: Mutex<u32> = Mutex::new(0);
static DOWN_VK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn with_guard<T>(f: impl FnOnce(&mut Guard) -> T) -> T {
    f(GUARD.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Guard::default))
}

/// Bounds of the display holding the focused window, for placing the overlay.
fn foreground_display() -> Option<Rect> {
    let platform = platform();
    let wm = platform.windows();
    let fg = wm.foreground().ok().flatten()?;
    let areas = wm.work_areas().ok()?;
    super::geometry::area_for(&areas, fg.bounds).map(|a| a.bounds)
}

fn extra_vk(extra: ExtraModifier) -> u32 {
    match extra {
        ExtraModifier::Shift => u32::from(VK_SHIFT.0),
        ExtraModifier::Ctrl => u32::from(VK_CONTROL.0),
        ExtraModifier::Alt => u32::from(VK_MENU.0),
    }
}

unsafe extern "system" fn key_proc(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 && swallow_key(w.0 as u32, &*(l.0 as *const KBDLLHOOKSTRUCT)) {
        return LRESULT(1);
    }
    CallNextHookEx(None, code, w, l)
}

fn swallow_key(msg: u32, kb: &KBDLLHOOKSTRUCT) -> bool {
    if kb.dwExtraInfo == MARKER {
        return false;
    }
    let cfg = config();
    if !cfg.protection {
        return false;
    }
    let vk = kb.vkCode;
    if msg == WM_KEYUP || msg == WM_SYSKEYUP {
        let _ = DOWN_VK.compare_exchange(vk, 0, std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst);
        let cancelled = with_guard(|g| g.key_up(if vk == super::guard::VK_W { Combo::Close } else { Combo::Quit }));
        if cancelled && vk == *HELD_VK.lock().unwrap_or_else(|e| e.into_inner()) {
            if let Some(app) = app() {
                overlay::hide(&app);
            }
            return true;
        }
        return false;
    }
    if msg != WM_KEYDOWN && msg != WM_SYSKEYDOWN {
        return false;
    }
    let alt = async_down(VK_MENU);
    let ctrl = async_down(VIRTUAL_KEY(VK_CONTROL.0));
    let shift = async_down(VK_SHIFT);
    let Some((combo, extra_held)) = classify(vk, alt, ctrl, shift, cfg.extra_modifier) else { return false };
    if (combo == Combo::Quit && !cfg.guard_quit) || (combo == Combo::Close && !cfg.guard_close) {
        return false;
    }
    let foreground = platform().windows().foreground().ok().flatten();
    let mode = cfg.mode_for(foreground.as_ref().and_then(|w| w.app_id.as_deref()));
    if mode != Mode::Modifier && extra_held {
        return false;
    }
    let repeat = DOWN_VK.swap(vk, std::sync::atomic::Ordering::SeqCst) == vk;
    let step = with_guard(|g| g.key_down(now_ms(), combo, mode, repeat, extra_held));
    match (step, mode) {
        (Step::Pass, _) => {
            if let Some(app) = app() {
                overlay::hide(&app);
            }
            false
        }
        (Step::Replay, _) => {
            let extra = extra_vk(cfg.extra_modifier);
            inject(&[key_input(extra, true), key_input(vk, false), key_input(vk, true), key_input(extra, false)]);
            true
        }
        (Step::Swallow, Mode::Hold) => {
            if !repeat {
                start_hold(vk, &cfg, if combo == Combo::Quit { "Keep holding to quit" } else { "Keep holding to close" });
            }
            true
        }
        (Step::Swallow, Mode::Twice) => {
            if let (Some(app), Some(display)) = (app(), foreground_display()) {
                overlay::hint(&app, display, if combo == Combo::Quit { "Press again to quit" } else { "Press again to close" });
            }
            true
        }
        (Step::Swallow, Mode::Modifier) => {
            if let (Some(app), Some(display)) = (app(), foreground_display()) {
                let name = match cfg.extra_modifier {
                    ExtraModifier::Shift => "Shift",
                    ExtraModifier::Ctrl => "Ctrl",
                    ExtraModifier::Alt => "Alt",
                };
                overlay::hint(&app, display, &format!("Add {name} to confirm"));
            }
            true
        }
    }
}

fn start_hold(vk: u32, cfg: &Config, label: &'static str) {
    *HELD_VK.lock().unwrap_or_else(|e| e.into_inner()) = vk;
    let hold_ms = cfg.hold_ms.max(100);
    std::thread::spawn(move || {
        let display = foreground_display();
        loop {
            std::thread::sleep(Duration::from_millis(25));
            match with_guard(|g| g.tick(now_ms(), hold_ms)) {
                Tick::Progress(p) => {
                    if let (Some(app), Some(d)) = (app(), display) {
                        overlay::hold(&app, d, p, label);
                    }
                }
                Tick::Fire(_) => {
                    inject(&[key_input(vk, false), key_input(vk, true)]);
                    if let Some(app) = app() {
                        overlay::hide(&app);
                    }
                    return;
                }
                Tick::Idle => return,
            }
        }
    });
}

// ---- modifier-drag ----

#[derive(Clone, Copy)]
struct Session {
    id: WindowId,
    mode: DragMode,
    start: Rect,
    start_ptr: (i32, i32),
    right: bool,
    alt: bool,
}

enum Msg {
    Begin(Session),
    Move(Session, (i32, i32)),
    End(Session, (i32, i32)),
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);
static SENDER: Mutex<Option<Sender<Msg>>> = Mutex::new(None);

fn send(m: Msg) {
    if let Some(tx) = SENDER.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = tx.send(m);
    }
}

unsafe extern "system" fn mouse_proc(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 && swallow_mouse(w.0 as u32, &*(l.0 as *const MSLLHOOKSTRUCT)) {
        return LRESULT(1);
    }
    CallNextHookEx(None, code, w, l)
}

fn modifiers_held(want: super::config::Modifiers) -> bool {
    (!want.alt || async_down(VK_MENU))
        && (!want.ctrl || async_down(VIRTUAL_KEY(VK_CONTROL.0)))
        && (!want.shift || async_down(VK_SHIFT))
        && (!want.meta || async_down(VK_LWIN) || async_down(VK_RWIN))
}

fn swallow_mouse(msg: u32, ms: &MSLLHOOKSTRUCT) -> bool {
    if ms.dwExtraInfo == MARKER {
        return false;
    }
    let pt = (ms.pt.x, ms.pt.y);
    match msg {
        WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            let cfg = config();
            let right = msg == WM_RBUTTONDOWN;
            if !cfg.modifier_drag || (right && !cfg.right_button_resize) || !modifiers_held(cfg.drag_modifiers) {
                return false;
            }
            if SESSION.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
                return false;
            }
            let platform = platform();
            let wm = platform.windows();
            let Ok(Some(win)) = wm.window_at(pt.0, pt.1) else { return false };
            if win.pid == Some(std::process::id()) || cfg.ignores(win.app_id.as_deref()) {
                return false;
            }
            if wm.window_state(win.id).is_ok_and(|s| s.fullscreen) {
                return false;
            }
            let resize = right || (!cfg.drag_modifiers.shift && async_down(VK_SHIFT));
            let mode = if resize { DragMode::Resize(anchor_at(pt, win.bounds)) } else { DragMode::Move };
            let session = Session { id: win.id, mode, start: win.bounds, start_ptr: pt, right, alt: cfg.drag_modifiers.alt };
            *SESSION.lock().unwrap_or_else(|e| e.into_inner()) = Some(session);
            send(Msg::Begin(session));
            true
        }
        WM_MOUSEMOVE => {
            let session = *SESSION.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(s) = session {
                send(Msg::Move(s, pt));
            }
            false
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
            match *guard {
                Some(s) if s.right == (msg == WM_RBUTTONUP) => {
                    *guard = None;
                    drop(guard);
                    send(Msg::End(s, pt));
                    if s.alt {
                        // Letting go of Alt on its own would open the app's menu bar; a tap of Ctrl first prevents that.
                        inject(&[key_input(u32::from(VK_CONTROL.0), false), key_input(u32::from(VK_CONTROL.0), true)]);
                    }
                    true
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn worker(rx: std::sync::mpsc::Receiver<Msg>) {
    let t0 = Instant::now();
    let mut tracker: Option<Tracker> = None;
    let mut last_rect: Option<Rect> = None;
    let mut last_target = None;
    while let Ok(first) = rx.recv() {
        let mut msg = first;
        while let Ok(next) = rx.try_recv() {
            let stop = matches!(next, Msg::End(..) | Msg::Begin(_));
            msg = next;
            if stop {
                break;
            }
        }
        let platform = platform();
        let wm = platform.windows();
        let cfg = config();
        match msg {
            Msg::Begin(s) => {
                last_rect = Some(s.start);
                last_target = None;
                tracker = (cfg.snap_edges && s.mode == DragMode::Move)
                    .then(|| wm.work_areas().ok())
                    .flatten()
                    .map(|a| Tracker::new(a, cfg.edge_rules.clone(), cfg.gap, cfg.margin));
            }
            Msg::Move(s, pt) => {
                let rect = drag_rect(s.mode, s.start, s.start_ptr, pt, MIN_SIZE);
                if last_rect != Some(rect) {
                    let _ = wm.set_bounds(s.id, rect);
                    last_rect = Some(rect);
                }
                if let (Some(t), Some(app)) = (tracker.as_mut(), app()) {
                    let target = t.update(t0.elapsed().as_millis() as u64, pt, rect);
                    if target != last_target {
                        match (&target, cfg.snap_preview) {
                            (Some(x), true) => overlay::preview(&app, x.display, x.rect),
                            _ => overlay::hide(&app),
                        }
                        last_target = target;
                    }
                }
            }
            Msg::End(s, pt) => {
                if let Some(app) = app() {
                    overlay::hide(&app);
                }
                if let Some(t) = tracker.as_mut() {
                    let current = last_rect.unwrap_or(s.start);
                    if let Some(target) = t.update(t0.elapsed().as_millis() as u64, pt, current) {
                        let _ = actions::snap_to(wm, s.id, s.start, target.rect);
                    }
                }
                tracker = None;
                last_target = None;
            }
        }
    }
}
