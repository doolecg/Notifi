//! The colour picker (spec/capture/color-picker.md): a magnifier that follows the pointer, samples
//! the screen pixel under it and copies it in the chosen format, plus a list of recent colours.

use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindowBuilder};

use crate::settings::{Settings, Store};

pub const WINDOW: &str = "picker";
pub const ACTION: &str = "colorPicker";
const SAMPLE_EVENT: &str = "picker://sample";
const MODE_EVENT: &str = "picker://mode";
const COPIED_EVENT: &str = "picker://copied";
const FILE: &str = "color-history.json";

/// Width and height of the sampled square, in pixels. The middle one is copied.
pub const GRID: i32 = 11;
/// Logical sizes of the window as a magnifier and as the history list.
const MAGNIFIER: (f64, f64) = (156.0, 200.0);
const LIST: (f64, f64) = (280.0, 360.0);
/// Gap between the pointer and the magnifier, so the magnifier never covers sampled pixels.
const GAP: f64 = 24.0;

#[derive(Default)]
pub struct PickerState {
    /// Stop flag of the running pick, if one is running.
    session: Mutex<Option<Arc<AtomicBool>>>,
    /// Serialises history reads and writes.
    history: Mutex<()>,
}

#[derive(Clone)]
struct Opts {
    format: String,
    no_hash: bool,
    upper: bool,
    close_after: bool,
    keep_history: bool,
    history_size: usize,
}

impl Opts {
    fn from(s: &Settings) -> Self {
        let get = |k: &str| s.option("colorPicker", k);
        Self {
            format: get("format").and_then(|v| v.as_str()).unwrap_or("hex").to_string(),
            no_hash: get("hexNoHash").and_then(|v| v.as_bool()).unwrap_or(false),
            upper: get("hexUppercase").and_then(|v| v.as_bool()).unwrap_or(true),
            close_after: get("closeAfterCopy").and_then(|v| v.as_bool()).unwrap_or(true),
            keep_history: get("keepHistory").and_then(|v| v.as_bool()).unwrap_or(true),
            history_size: get("historySize").and_then(|v| v.as_f64()).unwrap_or(20.0).clamp(5.0, 100.0) as usize,
        }
    }

    fn text(&self, rgb: [u8; 3]) -> String {
        format_color(rgb, &self.format, self.no_hash, self.upper)
    }
}

fn opts(app: &AppHandle) -> Opts {
    Opts::from(&app.state::<Store>().get())
}

// ---- Pure logic ----

/// A colour as text in one of the page's formats. `no_hash` and `upper` apply to the hex digits.
pub fn format_color(rgb: [u8; 3], format: &str, no_hash: bool, upper: bool) -> String {
    let [r, g, b] = rgb;
    let case = |s: String| if upper { s.to_uppercase() } else { s };
    let hex = case(format!("{r:02x}{g:02x}{b:02x}"));
    match format {
        "rgb" => format!("rgb({r}, {g}, {b})"),
        "hsl" => {
            let (h, s, l) = to_hsl(rgb);
            format!("hsl({h}, {s}%, {l}%)")
        }
        "css" => format!("color: #{hex};"),
        "qt" => format!("QColor({r}, {g}, {b})"),
        "xaml" => format!("<SolidColorBrush Color=\"#{}\"/>", case(format!("ff{r:02x}{g:02x}{b:02x}"))),
        _ if no_hash => hex,
        _ => format!("#{hex}"),
    }
}

/// Hue in degrees, saturation and lightness in percent, each rounded.
pub fn to_hsl([r, g, b]: [u8; 3]) -> (u32, u32, u32) {
    let (r, g, b) = (f64::from(r) / 255.0, f64::from(g) / 255.0, f64::from(b) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0, 0, (l * 100.0).round() as u32);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let sector = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    let h = sector * 60.0;
    ((h.round() as u32) % 360, (s * 100.0).round() as u32, (l * 100.0).round() as u32)
}

/// The history's own spelling of a colour: `#rrggbb`, lower case.
pub fn canonical(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let hex = text.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let part = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([part(0)?, part(2)?, part(4)?])
}

/// Puts `hex` first (moving it up if it is already there) and keeps at most `cap` colours.
pub fn push_history(list: &mut Vec<String>, hex: &str, cap: usize) {
    list.retain(|h| h != hex);
    list.insert(0, hex.to_string());
    list.truncate(cap.max(1));
}

/// Top-left of a `size` box beside the pointer inside `area` (x, y, width, height), flipping to the
/// other side of the pointer when it would run off the area.
pub fn place(pointer: (i32, i32), area: (i32, i32, i32, i32), size: (i32, i32), gap: i32) -> (i32, i32) {
    let (ax, ay, aw, ah) = area;
    let mut x = pointer.0 + gap;
    if x + size.0 > ax + aw {
        x = pointer.0 - gap - size.0;
    }
    let mut y = pointer.1 + gap;
    if y + size.1 > ay + ah {
        y = pointer.1 - gap - size.1;
    }
    (x.max(ax), y.max(ay))
}

/// Which of a set of keys went from up to down since the last call.
pub struct Edges<const N: usize> {
    prev: [bool; N],
}

impl<const N: usize> Edges<N> {
    /// Starts from the keys already held, so a key still down from the shortcut doesn't count.
    pub fn new(held: [bool; N]) -> Self {
        Self { prev: held }
    }

    pub fn rising(&mut self, now: [bool; N]) -> [bool; N] {
        let out = std::array::from_fn(|i| now[i] && !self.prev[i]);
        self.prev = now;
        out
    }
}

fn history_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(FILE))
}

fn read_history(app: &AppHandle) -> Vec<String> {
    history_path(app)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
        .unwrap_or_default()
}

fn remember(app: &AppHandle, o: &Opts, rgb: [u8; 3]) {
    if !o.keep_history {
        return;
    }
    let state = app.state::<PickerState>();
    let _guard = state.history.lock().unwrap();
    let mut list = read_history(app);
    push_history(&mut list, &canonical(rgb), o.history_size);
    if let Some(path) = history_path(app) {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(&list) {
            let _ = fs::write(path, text);
        }
    }
}

fn copy(app: &AppHandle, o: &Opts, rgb: [u8; 3]) {
    let text = o.text(rgb);
    if crate::clipboard::set_text(&text).is_ok() {
        remember(app, o, rgb);
        let _ = app.emit_to(WINDOW, COPIED_EVENT, text);
    }
}

// ---- Window ----

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("picker.html".into()))
        .title("Notifi Colour Picker")
        .inner_size(MAGNIFIER.0, MAGNIFIER.1)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .focusable(false)
        .visible(false)
        .focused(false)
        .build()?;
    let _ = window.set_ignore_cursor_events(true);
    Ok(())
}

fn area_at(app: &AppHandle, pointer: (i32, i32)) -> ((i32, i32, i32, i32), f64) {
    let monitor = app
        .monitor_from_point(f64::from(pointer.0), f64::from(pointer.1))
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    match monitor {
        Some(m) => ((m.position().x, m.position().y, m.size().width as i32, m.size().height as i32), m.scale_factor()),
        None => ((0, 0, 1920, 1080), 1.0),
    }
}

fn move_to(app: &AppHandle, pointer: (i32, i32), logical: (f64, f64)) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    let (area, scale) = area_at(app, pointer);
    let size = ((logical.0 * scale).round() as i32, (logical.1 * scale).round() as i32);
    let (x, y) = place(pointer, area, size, (GAP * scale).round() as i32);
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn show_magnifier(app: &AppHandle, pointer: (i32, i32)) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    let (_, scale) = area_at(app, pointer);
    let _ = window.set_focusable(false);
    let _ = window.set_ignore_cursor_events(true);
    let _ = window.set_size(PhysicalSize::new((MAGNIFIER.0 * scale).round() as u32, (MAGNIFIER.1 * scale).round() as u32));
    move_to(app, pointer, MAGNIFIER);
    let _ = app.emit_to(WINDOW, MODE_EVENT, "pick");
    let _ = window.show();
}

fn show_list(app: &AppHandle, pointer: (i32, i32)) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    let (area, scale) = area_at(app, pointer);
    let size = ((LIST.0 * scale).round() as i32, (LIST.1 * scale).round() as i32);
    let x = (pointer.0 - size.0 / 2).clamp(area.0, (area.0 + area.2 - size.0).max(area.0));
    let y = (pointer.1 - size.1 / 2).clamp(area.1, (area.1 + area.3 - size.1).max(area.1));
    let _ = window.set_size(PhysicalSize::new(size.0 as u32, size.1 as u32));
    let _ = window.set_position(PhysicalPosition::new(x, y));
    let _ = window.set_ignore_cursor_events(false);
    let _ = window.set_focusable(true);
    let _ = app.emit_to(WINDOW, MODE_EVENT, "list");
    let _ = window.show();
    if !crate::window::no_focus() {
        let _ = window.set_focus();
    }
}

fn hide(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    let _ = window.hide();
    let _ = window.set_focusable(false);
    let _ = window.set_ignore_cursor_events(true);
}

enum Outcome {
    Cancel,
    Copied,
    List,
}

/// Starts picking, or stops it when it is already running.
pub fn toggle(app: &AppHandle) {
    let state = app.state::<PickerState>();
    let mut session = state.session.lock().unwrap();
    if let Some(stop) = session.as_ref() {
        stop.store(true, Ordering::SeqCst);
        return;
    }
    if !available() {
        return;
    }
    let stop = Arc::new(AtomicBool::new(false));
    *session = Some(stop.clone());
    drop(session);
    let app = app.clone();
    let _ = std::thread::Builder::new().name("notifi-picker".into()).spawn(move || {
        let o = opts(&app);
        let (outcome, pointer) = sys::run(&app, &o, &stop);
        *app.state::<PickerState>().session.lock().unwrap() = None;
        match outcome {
            Outcome::List => show_list(&app, pointer),
            Outcome::Cancel | Outcome::Copied => hide(&app),
        }
    });
}

pub fn available() -> bool {
    cfg!(windows)
}

#[tauri::command]
pub fn picker_toggle(app: AppHandle) {
    toggle(&app);
}

#[tauri::command]
pub fn picker_available() -> bool {
    available()
}

#[derive(Serialize)]
pub struct Recent {
    hex: String,
    text: String,
}

#[tauri::command]
pub fn picker_history(app: AppHandle, state: State<PickerState>) -> Vec<Recent> {
    let o = opts(&app);
    if !o.keep_history {
        return Vec::new();
    }
    let _guard = state.history.lock().unwrap();
    read_history(&app)
        .into_iter()
        .filter_map(|hex| parse_hex(&hex).map(|rgb| Recent { text: o.text(rgb), hex }))
        .collect()
}

/// Copies a colour chosen in the list, then closes it.
#[tauri::command]
pub fn picker_copy(app: AppHandle, hex: String) {
    if let Some(rgb) = parse_hex(&hex) {
        copy(&app, &opts(&app), rgb);
    }
    hide(&app);
}

#[tauri::command]
pub fn picker_close(app: AppHandle) {
    hide(&app);
}

#[derive(Serialize, Clone)]
struct Sample {
    x: i32,
    y: i32,
    /// RGB of the sampled square, row by row.
    cells: Vec<u8>,
    hex: String,
    text: String,
}

// ---- Screen sampling (Windows) ----

#[cfg(windows)]
mod sys {
    use std::time::Duration;

    use windows::Win32::{
        Foundation::{LPARAM, LRESULT, POINT, WPARAM},
        Graphics::Gdi::{
            BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject,
            BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, ROP_CODE, SRCCOPY,
        },
        UI::{
            Input::KeyboardAndMouse::GetAsyncKeyState,
            WindowsAndMessaging::{
                CallNextHookEx, DispatchMessageW, GetCursorPos, PeekMessageW, SetCursorPos, SetWindowsHookExW, TranslateMessage,
                UnhookWindowsHookEx, MSG, PM_REMOVE, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP,
            },
        },
    };

    use super::*;

    static ACTIVE: AtomicBool = AtomicBool::new(false);
    static CLICK: AtomicBool = AtomicBool::new(false);

    /// Esc, Tab, Enter, C, then the arrows left, up, right, down.
    const KEYS: [i32; 8] = [0x1B, 0x09, 0x0D, 0x43, 0x25, 0x26, 0x27, 0x28];

    /// Swallows the left click that picks, so it doesn't also press whatever is under the pointer.
    unsafe extern "system" fn mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && ACTIVE.load(Ordering::SeqCst) {
            match wparam.0 as u32 {
                WM_LBUTTONDOWN => {
                    CLICK.store(true, Ordering::SeqCst);
                    return LRESULT(1);
                }
                WM_LBUTTONUP => return LRESULT(1),
                _ => {}
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    fn held() -> [bool; 8] {
        std::array::from_fn(|i| unsafe { GetAsyncKeyState(KEYS[i]) as u16 & 0x8000 != 0 })
    }

    /// RGB of the `GRID` x `GRID` square of screen centred on (cx, cy), layered windows included.
    fn grab(cx: i32, cy: i32) -> Option<Vec<u8>> {
        unsafe {
            let screen = GetDC(None);
            if screen.is_invalid() {
                return None;
            }
            let mem = CreateCompatibleDC(Some(screen));
            let bitmap = CreateCompatibleBitmap(screen, GRID, GRID);
            let old = SelectObject(mem, bitmap.into());
            let half = GRID / 2;
            let copied = BitBlt(mem, 0, 0, GRID, GRID, Some(screen), cx - half, cy - half, ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0)).is_ok();
            // A bitmap can't be read while it is selected into a device context.
            SelectObject(mem, old);
            let mut out = None;
            if copied {
                let mut info = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: GRID,
                        biHeight: -GRID,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let mut pixels = vec![0u8; (GRID * GRID * 4) as usize];
                let rows = GetDIBits(screen, bitmap, 0, GRID as u32, Some(pixels.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS);
                if rows != 0 {
                    out = Some(pixels.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect());
                }
            }
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            out
        }
    }

    /// Runs one pick on the calling thread; returns how it ended and where the pointer was.
    pub fn run(app: &AppHandle, o: &Opts, stop: &AtomicBool) -> (Outcome, (i32, i32)) {
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
        }
        show_magnifier(app, (pt.x, pt.y));
        CLICK.store(false, Ordering::SeqCst);
        ACTIVE.store(true, Ordering::SeqCst);
        let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), None, 0).ok() };
        let mut edges = Edges::new(held());
        let mut last: Option<((i32, i32), Vec<u8>)> = None;
        let outcome = loop {
            unsafe {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                let _ = GetCursorPos(&mut pt);
            }
            if stop.load(Ordering::SeqCst) {
                break Outcome::Cancel;
            }
            let rising = edges.rising(held());
            if rising[0] {
                break Outcome::Cancel;
            }
            if rising[1] {
                break Outcome::List;
            }
            let (dx, dy) = (i32::from(rising[6]) - i32::from(rising[4]), i32::from(rising[7]) - i32::from(rising[5]));
            if dx != 0 || dy != 0 {
                unsafe {
                    let _ = SetCursorPos(pt.x + dx, pt.y + dy);
                    let _ = GetCursorPos(&mut pt);
                }
            }
            let click = CLICK.swap(false, Ordering::SeqCst);
            if let Some(cells) = grab(pt.x, pt.y) {
                let mid = (GRID * GRID / 2) as usize * 3;
                let rgb = [cells[mid], cells[mid + 1], cells[mid + 2]];
                if last.as_ref().is_none_or(|(p, c)| *p != (pt.x, pt.y) || *c != cells) {
                    let _ = app.emit_to(
                        WINDOW,
                        SAMPLE_EVENT,
                        Sample { x: pt.x, y: pt.y, cells: cells.clone(), hex: canonical(rgb), text: o.text(rgb) },
                    );
                    move_to(app, (pt.x, pt.y), MAGNIFIER);
                    last = Some(((pt.x, pt.y), cells));
                }
                if click || rising[2] || rising[3] {
                    copy(app, o, rgb);
                    if rising[2] || o.close_after {
                        break Outcome::Copied;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(16));
        };
        ACTIVE.store(false, Ordering::SeqCst);
        if let Some(h) = hook {
            unsafe {
                let _ = UnhookWindowsHookEx(h);
            }
        }
        (outcome, (pt.x, pt.y))
    }
}

#[cfg(not(windows))]
mod sys {
    use super::*;

    pub fn run(_app: &AppHandle, _o: &Opts, _stop: &AtomicBool) -> (Outcome, (i32, i32)) {
        (Outcome::Cancel, (0, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        let c = [255, 128, 0];
        assert_eq!(format_color(c, "hex", false, true), "#FF8000");
        assert_eq!(format_color(c, "hex", true, true), "FF8000");
        assert_eq!(format_color(c, "hex", false, false), "#ff8000");
        assert_eq!(format_color(c, "rgb", false, true), "rgb(255, 128, 0)");
        assert_eq!(format_color(c, "hsl", false, true), "hsl(30, 100%, 50%)");
        assert_eq!(format_color(c, "css", true, false), "color: #ff8000;");
        assert_eq!(format_color(c, "qt", false, true), "QColor(255, 128, 0)");
        assert_eq!(format_color(c, "xaml", false, true), "<SolidColorBrush Color=\"#FFFF8000\"/>");
    }

    #[test]
    fn hsl_edges() {
        assert_eq!(to_hsl([0, 0, 0]), (0, 0, 0));
        assert_eq!(to_hsl([255, 255, 255]), (0, 0, 100));
        assert_eq!(to_hsl([255, 0, 0]), (0, 100, 50));
        assert_eq!(to_hsl([0, 0, 255]), (240, 100, 50));
        assert_eq!(to_hsl([255, 0, 128]), (330, 100, 50));
    }

    #[test]
    fn hex_round_trip() {
        assert_eq!(parse_hex(&canonical([1, 171, 255])), Some([1, 171, 255]));
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("123456"), None);
        assert_eq!(parse_hex("#12345g"), None);
    }

    #[test]
    fn history_moves_dedupes_and_caps() {
        let mut list = Vec::new();
        for c in ["#000001", "#000002", "#000003"] {
            push_history(&mut list, c, 2);
        }
        assert_eq!(list, ["#000003", "#000002"]);
        push_history(&mut list, "#000002", 2);
        assert_eq!(list, ["#000002", "#000003"]);
    }

    #[test]
    fn magnifier_flips_at_the_edges() {
        let area = (0, 0, 1000, 800);
        assert_eq!(place((100, 100), area, (150, 200), 24), (124, 124));
        assert_eq!(place((900, 100), area, (150, 200), 24), (726, 124));
        assert_eq!(place((100, 700), area, (150, 200), 24), (124, 476));
    }

    #[test]
    fn only_new_presses_count() {
        let mut e = Edges::new([true, false]);
        assert_eq!(e.rising([true, false]), [false, false]);
        assert_eq!(e.rising([true, true]), [false, true]);
        assert_eq!(e.rising([false, true]), [false, false]);
        assert_eq!(e.rising([true, true]), [true, false]);
    }
}
