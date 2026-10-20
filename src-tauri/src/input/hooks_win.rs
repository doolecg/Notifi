//! The input hook thread: one WH_MOUSE_LL and one WH_KEYBOARD_LL hook, installed only while a
//! feature needs them. Everything the hooks inject carries `MARK` so they never see their own input.
//! Smooth scrolling is played out by a second thread so the hooks never wait.

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::{
    GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
    MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, PostThreadMessageW,
    SetWindowsHookExW, SystemParametersInfoW, TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, SPI_GETWHEELSCROLLCHARS, SPI_GETWHEELSCROLLLINES,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_XBUTTONDOWN,
    WM_XBUTTONUP,
};

use super::config::{ButtonsCfg, Config, SuperCfg};
use super::logic::{
    app_listed, drag_direction, is_extended_vk, is_modifier_vk, route_wheel, Action, Btn, ClickFilter, Glide,
    KeyDebounce, ScrollCfg, WheelOut, VK_LALT, VK_LWIN,
};

/// Marks the input we inject ourselves so the hooks let it through.
const MARK: usize = 0x4E_54_49_4E;
/// An unassigned virtual key: tapping it stops a lone Win or Alt release from opening a menu.
const MASK_KEY: u16 = 0xE8;

static WANT_KEYS: AtomicBool = AtomicBool::new(false);
static WANT_MOUSE: AtomicBool = AtomicBool::new(false);
static THREAD: AtomicU32 = AtomicU32::new(0);
static GLIDE: Mutex<Option<Sender<GlideMsg>>> = Mutex::new(None);

// ---- Injection ----

fn key_input(vk: u16, scan: u16, up: bool, extended: bool) -> INPUT {
    let mut flags = 0;
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY.0;
    }
    if up {
        flags |= KEYEVENTF_KEYUP.0;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: scan, dwFlags: KEYBD_EVENT_FLAGS(flags), time: 0, dwExtraInfo: MARK },
        },
    }
}

fn mouse_input(flags: MOUSE_EVENT_FLAGS, data: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: data, dwFlags: flags, time: 0, dwExtraInfo: MARK } },
    }
}

fn btn_input(btn: Btn, up: bool) -> INPUT {
    match (btn, up) {
        (Btn::Left, false) => mouse_input(MOUSEEVENTF_LEFTDOWN, 0),
        (Btn::Left, true) => mouse_input(MOUSEEVENTF_LEFTUP, 0),
        (Btn::Right, false) => mouse_input(MOUSEEVENTF_RIGHTDOWN, 0),
        (Btn::Right, true) => mouse_input(MOUSEEVENTF_RIGHTUP, 0),
        (Btn::Middle, false) => mouse_input(MOUSEEVENTF_MIDDLEDOWN, 0),
        (Btn::Middle, true) => mouse_input(MOUSEEVENTF_MIDDLEUP, 0),
        (Btn::Back, false) => mouse_input(MOUSEEVENTF_XDOWN, 1),
        (Btn::Back, true) => mouse_input(MOUSEEVENTF_XUP, 1),
        (Btn::Forward, false) => mouse_input(MOUSEEVENTF_XDOWN, 2),
        (Btn::Forward, true) => mouse_input(MOUSEEVENTF_XUP, 2),
    }
}

fn wheel_input(horizontal: bool, delta: i32) -> INPUT {
    mouse_input(if horizontal { MOUSEEVENTF_HWHEEL } else { MOUSEEVENTF_WHEEL }, delta as u32)
}

fn send(inputs: &[INPUT]) {
    if !inputs.is_empty() {
        unsafe {
            SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }
}

fn keys_down(keys: &[u16]) -> Vec<INPUT> {
    keys.iter().map(|k| key_input(*k, 0, false, is_extended_vk(*k))).collect()
}

fn keys_up(keys: &[u16]) -> Vec<INPUT> {
    keys.iter().rev().map(|k| key_input(*k, 0, true, is_extended_vk(*k))).collect()
}

fn run_action(action: &Action) {
    match action {
        Action::Keys(keys) => {
            let mut batch = keys_down(keys);
            batch.extend(keys_up(keys));
            send(&batch);
        }
        Action::Click(btn) => send(&[btn_input(*btn, false), btn_input(*btn, true)]),
        Action::Nothing => {}
    }
}

/// Lets go of the held combination; a Win or Alt that was pressed alone would otherwise open a menu.
fn release_combo(held: &[u16]) {
    let mut batch = Vec::new();
    if held.iter().any(|k| *k == VK_LWIN || *k == VK_LALT) {
        batch.push(key_input(MASK_KEY, 0, false, false));
        batch.push(key_input(MASK_KEY, 0, true, false));
    }
    batch.extend(keys_up(held));
    send(&batch);
}

// ---- Hook state ----

struct Fg {
    hwnd: isize,
    path: String,
    at: Option<Instant>,
}

impl Fg {
    fn path(&mut self) -> String {
        let hwnd = unsafe { GetForegroundWindow() };
        let fresh = self.at.is_some_and(|t| t.elapsed() < Duration::from_secs(1));
        if hwnd.0 as isize != self.hwnd || !fresh {
            self.hwnd = hwnd.0 as isize;
            self.path = process_path(hwnd);
            self.at = Some(Instant::now());
        }
        self.path.clone()
    }

    fn excluded(&mut self, list: &[String]) -> bool {
        !list.is_empty() && app_listed(list, &self.path())
    }
}

fn process_path(hwnd: windows::Win32::Foundation::HWND) -> String {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return String::new();
        }
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(process);
        if ok { String::from_utf16_lossy(&buf[..len as usize]) } else { String::new() }
    }
}

#[derive(Clone, Copy)]
enum Super {
    Idle,
    /// The source key is down and nothing else has happened yet: a tap is still possible.
    Pending(u32),
    /// The held combination is pressed.
    Active,
    /// The source key went down in an excluded app and passes untouched.
    Ignored,
}

struct Hold {
    btn: Btn,
    start: (i32, i32),
    fired: bool,
}

struct State {
    clicks: ClickFilter,
    keys: KeyDebounce,
    gesture: Option<Hold>,
    swallowed_up: [bool; 5],
    sup: Super,
    fg: Fg,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        clicks: ClickFilter::default(),
        keys: KeyDebounce::default(),
        gesture: None,
        swallowed_up: [false; 5],
        sup: Super::Idle,
        fg: Fg { hwnd: 0, path: String::new(), at: None },
    });
}

fn with_state(f: impl FnOnce(&mut State, &Config) -> bool) -> bool {
    let Some(cfg) = super::config() else { return false };
    STATE.with(|s| s.try_borrow_mut().map(|mut st| f(&mut st, &cfg)).unwrap_or(false))
}

// ---- Keyboard ----

/// Presses the held combination now and then the extra input, in one batch so the order holds.
fn activate(st: &mut State, sc: &SuperCfg, extra: Vec<INPUT>) {
    let mut batch = keys_down(&sc.held);
    batch.extend(extra);
    send(&batch);
    st.sup = Super::Active;
}

/// `true` swallows the event.
fn super_key(st: &mut State, sc: &SuperCfg, info: &KBDLLHOOKSTRUCT, down: bool) -> bool {
    let vk = info.vkCode;
    if vk == sc.source_vk {
        if down {
            return match st.sup {
                Super::Idle => {
                    if st.fg.excluded(&sc.excluded) {
                        st.sup = Super::Ignored;
                        return false;
                    }
                    st.sup = Super::Pending(info.time);
                    true
                }
                Super::Ignored => false,
                _ => true,
            };
        }
        return match std::mem::replace(&mut st.sup, Super::Idle) {
            Super::Idle | Super::Ignored => false,
            Super::Pending(since) => {
                if info.time.wrapping_sub(since) <= sc.tap_ms {
                    if let Some(action) = &sc.tap {
                        run_action(action);
                    }
                }
                true
            }
            Super::Active => {
                release_combo(&sc.held);
                true
            }
        };
    }
    if down && matches!(st.sup, Super::Pending(_)) {
        let extended = info.flags.0 & 1 != 0;
        activate(st, sc, vec![key_input(vk as u16, info.scanCode as u16, false, extended)]);
        return true;
    }
    false
}

fn handle_key(st: &mut State, cfg: &Config, info: &KBDLLHOOKSTRUCT, down: bool) -> bool {
    if let Some(sc) = &cfg.super_key {
        if super_key(st, sc, info, down) {
            return true;
        }
    }
    if let Some(interval) = cfg.debounce_ms {
        if !is_modifier_vk(info.vkCode as u16) && !st.fg.excluded(&cfg.global_excluded) {
            return !st.keys.event(info.vkCode, down, info.time, interval);
        }
    }
    false
}

unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if info.dwExtraInfo != MARK && info.flags.0 & LLKHF_INJECTED.0 == 0 {
            let down = wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN;
            if with_state(|st, cfg| handle_key(st, cfg, info, down)) {
                return LRESULT(1);
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

// ---- Mouse ----

fn action_for(st: &mut State, b: &ButtonsCfg, btn: Btn) -> Option<Action> {
    if !b.overrides.is_empty() {
        let path = st.fg.path();
        for (app, ob, action) in &b.overrides {
            if *ob == btn && app_listed(std::slice::from_ref(app), &path) {
                return Some(action.clone());
            }
        }
    }
    b.actions.get(&btn.index()).cloned()
}

fn handle_button(st: &mut State, cfg: &Config, info: &MSLLHOOKSTRUCT, btn: Btn, down: bool) -> bool {
    let idx = btn.index();
    if let Some(c) = &cfg.click {
        if c.buttons[idx] && !st.fg.excluded(&cfg.global_excluded) {
            let pass = if down { st.clicks.down(btn, info.time, c.threshold_ms) } else { st.clicks.up(btn, info.time) };
            if !pass {
                return true;
            }
        }
    }
    if down && matches!(st.sup, Super::Pending(_)) {
        if let Some(sc) = &cfg.super_key {
            activate(st, sc, vec![btn_input(btn, false)]);
            return true;
        }
    }
    let Some(b) = &cfg.buttons else { return false };
    if !down {
        if let Some(hold) = st.gesture.take() {
            if hold.btn == btn {
                if !hold.fired {
                    match action_for(st, b, btn) {
                        Some(action) => run_action(&action),
                        None => send(&[btn_input(btn, false), btn_input(btn, true)]),
                    }
                }
                return true;
            }
            st.gesture = Some(hold);
        }
        return std::mem::take(&mut st.swallowed_up[idx]);
    }
    if st.fg.excluded(&b.excluded) {
        return false;
    }
    if btn == b.gesture_button && !b.gestures.is_empty() && st.gesture.is_none() {
        st.gesture = Some(Hold { btn, start: (info.pt.x, info.pt.y), fired: false });
        return true;
    }
    if let Some(action) = action_for(st, b, btn) {
        st.swallowed_up[idx] = true;
        run_action(&action);
        return true;
    }
    false
}

fn handle_move(st: &mut State, cfg: &Config, info: &MSLLHOOKSTRUCT) {
    let (Some(b), Some(h)) = (&cfg.buttons, st.gesture.as_mut()) else { return };
    if h.fired {
        return;
    }
    if let Some(dir) = drag_direction(info.pt.x - h.start.0, info.pt.y - h.start.1, b.threshold_px) {
        h.fired = true;
        if let Some(action) = b.gestures.get(&dir) {
            run_action(action);
        }
    }
}

fn system_scroll(horizontal: bool) -> f64 {
    let mut v = 0u32;
    let action = if horizontal { SPI_GETWHEELSCROLLCHARS } else { SPI_GETWHEELSCROLLLINES };
    let ok = unsafe { SystemParametersInfoW(action, 0, Some(&mut v as *mut u32 as *mut c_void), Default::default()) }.is_ok();
    if ok && v > 0 && v != u32::MAX { f64::from(v) } else { 3.0 }
}

fn key_held(vks: [u16; 2]) -> bool {
    vks.iter().any(|vk| unsafe { GetAsyncKeyState(*vk as i32) } as u16 & 0x8000 != 0)
}

fn handle_wheel(st: &mut State, cfg: &Config, info: &MSLLHOOKSTRUCT, horizontal: bool) -> bool {
    if !cfg.wants_wheel() {
        return false;
    }
    let delta = (info.mouseData >> 16) as u16 as i16 as i32;
    if delta == 0 || delta % super::logic::NOTCH != 0 {
        return false;
    }
    let path = st.fg.path();
    let skip = |list: &[String]| app_listed(list, &path);
    let smooth = cfg.smooth.as_ref().filter(|s| !skip(&s.excluded));
    let scroll = ScrollCfg {
        invert_vertical: cfg.invert_vertical && !skip(&cfg.scroll_excluded),
        invert_horizontal: cfg.invert_horizontal && !skip(&cfg.scroll_excluded),
        sideways: cfg.sideways_vk.is_some_and(key_held) && !skip(&cfg.global_excluded),
        linear: cfg
            .linear
            .as_ref()
            .filter(|(_, excluded)| !skip(excluded))
            .map(|(lines, _)| (*lines, system_scroll(horizontal))),
        smooth: smooth.is_some(),
    };
    match route_wheel(&scroll, horizontal, delta) {
        WheelOut::Pass => false,
        WheelOut::Replace { horizontal, delta } => {
            send(&[wheel_input(horizontal, delta)]);
            true
        }
        WheelOut::Glide { horizontal, delta } => {
            let Some(s) = smooth else { return false };
            let sender = GLIDE.lock().unwrap_or_else(|e| e.into_inner()).clone();
            match sender {
                Some(tx) => {
                    let _ = tx.send(GlideMsg { horizontal, delta: f64::from(delta) * s.speed, tau_ms: s.tau_ms });
                    true
                }
                None => false,
            }
        }
    }
}

unsafe extern "system" fn mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        if info.dwExtraInfo != MARK {
            let msg = wparam.0 as u32;
            let injected = info.flags & LLMHF_INJECTED != 0;
            let x_button = if (info.mouseData >> 16) & 0xFFFF == 2 { Btn::Forward } else { Btn::Back };
            let button = match msg {
                WM_LBUTTONDOWN => Some((Btn::Left, true)),
                WM_LBUTTONUP => Some((Btn::Left, false)),
                WM_RBUTTONDOWN => Some((Btn::Right, true)),
                WM_RBUTTONUP => Some((Btn::Right, false)),
                WM_MBUTTONDOWN => Some((Btn::Middle, true)),
                WM_MBUTTONUP => Some((Btn::Middle, false)),
                WM_XBUTTONDOWN => Some((x_button, true)),
                WM_XBUTTONUP => Some((x_button, false)),
                _ => None,
            };
            let swallow = match (msg, button) {
                (_, Some((btn, down))) if !injected => with_state(|st, cfg| handle_button(st, cfg, info, btn, down)),
                (WM_MOUSEWHEEL, _) => with_state(|st, cfg| handle_wheel(st, cfg, info, false)),
                (WM_MOUSEHWHEEL, _) => with_state(|st, cfg| handle_wheel(st, cfg, info, true)),
                (WM_MOUSEMOVE, _) if !injected => with_state(|st, cfg| {
                    handle_move(st, cfg, info);
                    false
                }),
                _ => false,
            };
            if swallow {
                return LRESULT(1);
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

// ---- Smooth scrolling ----

struct GlideMsg {
    horizontal: bool,
    delta: f64,
    tau_ms: f64,
}

fn glide_worker(rx: mpsc::Receiver<GlideMsg>) {
    let mut axes = [Glide::default(), Glide::default()];
    let mut tau = 90.0;
    let mut last = Instant::now();
    loop {
        let idle = axes.iter().all(Glide::idle);
        let first = if idle {
            match rx.recv() {
                Ok(m) => {
                    last = Instant::now();
                    Some(m)
                }
                Err(_) => return,
            }
        } else {
            match rx.recv_timeout(Duration::from_millis(8)) {
                Ok(m) => Some(m),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        };
        for m in first.into_iter().chain(std::iter::from_fn(|| rx.try_recv().ok())) {
            tau = m.tau_ms;
            axes[usize::from(m.horizontal)].push(m.delta);
        }
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f64() * 1000.0;
        last = now;
        for (i, axis) in axes.iter_mut().enumerate() {
            let out = axis.step(dt, tau);
            if out != 0 {
                send(&[wheel_input(i == 1, out)]);
            }
        }
    }
}

fn ensure_glide_worker() {
    let mut slot = GLIDE.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_none() {
        let (tx, rx) = mpsc::channel();
        if std::thread::Builder::new().name("input-glide".into()).spawn(move || glide_worker(rx)).is_ok() {
            *slot = Some(tx);
        }
    }
}

// ---- Thread ----

/// Installs or removes the hooks to match the config; starts the hook thread on first use.
pub fn apply(cfg: &Config) {
    if cfg.smooth.is_some() {
        ensure_glide_worker();
    }
    WANT_KEYS.store(cfg.wants_key_hook(), Ordering::SeqCst);
    WANT_MOUSE.store(cfg.wants_mouse_hook(), Ordering::SeqCst);
    let tid = THREAD.load(Ordering::SeqCst);
    if tid != 0 {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_APP, WPARAM(0), LPARAM(0));
        }
        return;
    }
    if !cfg.wants_key_hook() && !cfg.wants_mouse_hook() {
        return;
    }
    let _ = std::thread::Builder::new().name("input-hooks".into()).spawn(|| unsafe {
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
