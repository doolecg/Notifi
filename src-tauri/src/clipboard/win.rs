//! Windows clipboard: a message-only window with AddClipboardFormatListener, session-lock and
//! suspend notifications, reading and writing formats, and synthetic paste through SendInput.

use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicIsize, Ordering},
        mpsc::Sender,
        OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM},
        System::{
            DataExchange::{
                AddClipboardFormatListener, CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
                GetClipboardOwner, GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
            },
            LibraryLoader::GetModuleHandleW,
            Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE},
            Power::RegisterSuspendResumeNotification,
            RemoteDesktop::WTSRegisterSessionNotification,
            Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
        },
        UI::{
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
                KEYEVENTF_UNICODE, VIRTUAL_KEY,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
                IsWindow, RegisterClassW, SetForegroundWindow, HWND_MESSAGE, MSG, REGISTER_NOTIFICATION_FLAGS, WINDOW_EX_STYLE,
                WINDOW_STYLE, WNDCLASSW,
            },
        },
    },
};

use super::{history::{Capture, Image}, Event, Payload};

const CF_UNICODETEXT: u32 = 13;
const CF_DIB: u32 = 8;
const CF_HDROP: u32 = 15;
/// Formats backed by GDI or other handles that can't be copied as bytes.
const HANDLE_FORMATS: &[u32] = &[2, 3, 9, 14, 0x80, 0x82, 0x83, 0x8E];
/// Formats Windows synthesises from others; restoring the source is enough.
const SYNTHESISED: &[u32] = &[1, 7, 16, 17];
const SNAPSHOT_LIMIT: usize = 256 * 1024 * 1024;

const WM_CLIPBOARDUPDATE: u32 = 0x031D;
const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
const WM_POWERBROADCAST: u32 = 0x0218;
const WTS_SESSION_LOCK: usize = 0x7;
const PBT_APMSUSPEND: usize = 0x4;

/// The clipboard as it was, for putting back after a temporary change.
pub struct Snapshot {
    formats: Vec<(u32, Vec<u8>)>,
}

static WINDOW: AtomicIsize = AtomicIsize::new(0);
static EVENTS: OnceLock<Sender<Event>> = OnceLock::new();

struct Formats {
    html: u32,
    rtf: u32,
    png: u32,
    exclude: u32,
    can_include: u32,
    viewer_ignore: u32,
    drop_effect: u32,
}

fn formats() -> &'static Formats {
    static F: OnceLock<Formats> = OnceLock::new();
    F.get_or_init(|| {
        let reg = |name: &str| {
            let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
            unsafe { RegisterClipboardFormatW(PCWSTR(wide.as_ptr())) }
        };
        Formats {
            html: reg("HTML Format"),
            rtf: reg("Rich Text Format"),
            png: reg("PNG"),
            exclude: reg("ExcludeClipboardContentFromMonitorProcessing"),
            can_include: reg("CanIncludeInClipboardHistory"),
            viewer_ignore: reg("Clipboard Viewer Ignore"),
            drop_effect: reg("Preferred DropEffect"),
        }
    })
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let event = match msg {
        WM_CLIPBOARDUPDATE => Some(Event::Changed),
        WM_WTSSESSION_CHANGE if wp.0 == WTS_SESSION_LOCK => Some(Event::Lock),
        WM_POWERBROADCAST if wp.0 == PBT_APMSUSPEND => Some(Event::Sleep),
        _ => None,
    };
    if let (Some(event), Some(tx)) = (event, EVENTS.get()) {
        let _ = tx.send(event);
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

/// Starts the listener window on its own thread.
pub fn watch(tx: Sender<Event>) {
    if EVENTS.set(tx).is_err() {
        return;
    }
    thread::Builder::new()
        .name("clipboard-listener".into())
        .spawn(|| unsafe {
            let _ = formats();
            let Ok(module) = GetModuleHandleW(None) else { return };
            let class: Vec<u16> = "NotifiClipboardListener\0".encode_utf16().collect();
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: module.into(),
                lpszClassName: PCWSTR(class.as_ptr()),
                ..Default::default()
            };
            RegisterClassW(&wc);
            let Ok(hwnd) = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(class.as_ptr()),
                PCWSTR(class.as_ptr()),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(module.into()),
                None,
            ) else {
                return;
            };
            WINDOW.store(hwnd.0 as isize, Ordering::SeqCst);
            let _ = AddClipboardFormatListener(hwnd);
            let _ = WTSRegisterSessionNotification(hwnd, 0);
            let _ = RegisterSuspendResumeNotification(HANDLE(hwnd.0), REGISTER_NOTIFICATION_FLAGS(0));
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                DispatchMessageW(&msg);
            }
        })
        .ok();
}

pub fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

/// Holds the clipboard open; closes it when dropped.
struct Open;

impl Open {
    /// Other apps hold the clipboard for a moment while they write, so retry for a while.
    fn new() -> Result<Self, String> {
        let hwnd = HWND(WINDOW.load(Ordering::SeqCst) as *mut c_void);
        let owner = (!hwnd.0.is_null()).then_some(hwnd);
        let start = Instant::now();
        loop {
            if unsafe { OpenClipboard(owner) }.is_ok() {
                return Ok(Open);
            }
            if start.elapsed() > Duration::from_millis(800) {
                return Err("Another app is holding the clipboard.".into());
            }
            thread::sleep(Duration::from_millis(15));
        }
    }

    fn bytes(&self, format: u32) -> Option<Vec<u8>> {
        unsafe {
            let handle = GetClipboardData(format).ok()?;
            let global = HGLOBAL(handle.0);
            let size = GlobalSize(global);
            let ptr = GlobalLock(global) as *const u8;
            if ptr.is_null() {
                return None;
            }
            let out = std::slice::from_raw_parts(ptr, size).to_vec();
            let _ = GlobalUnlock(global);
            Some(out)
        }
    }

    fn has(&self, format: u32) -> bool {
        self.list().contains(&format)
    }

    fn list(&self) -> Vec<u32> {
        let mut out = Vec::new();
        let mut f = 0;
        loop {
            f = unsafe { EnumClipboardFormats(f) };
            if f == 0 {
                return out;
            }
            out.push(f);
        }
    }

    fn put(&self, format: u32, data: &[u8]) -> Result<(), String> {
        unsafe {
            let global = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1)).map_err(|e| e.to_string())?;
            let ptr = GlobalLock(global) as *mut u8;
            if ptr.is_null() {
                return Err("Out of memory for the clipboard.".into());
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            let _ = GlobalUnlock(global);
            SetClipboardData(format, Some(HANDLE(global.0))).map(|_| ()).map_err(|e| e.to_string())
        }
    }

    fn empty(&self) -> Result<(), String> {
        unsafe { EmptyClipboard() }.map_err(|e| e.to_string())
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

fn utf16_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
}

fn from_utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|u| *u != 0).collect();
    String::from_utf16_lossy(&units)
}

fn ansi(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// DROPFILES header: pFiles, pt.x, pt.y, fNC, fWide.
fn parse_hdrop(bytes: &[u8]) -> Vec<String> {
    if bytes.len() < 20 {
        return Vec::new();
    }
    let offset = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let wide = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) != 0;
    let Some(list) = bytes.get(offset..) else { return Vec::new() };
    let joined = if wide {
        let units: Vec<u16> = list.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(list).into_owned()
    };
    joined.split('\0').take_while(|s| !s.is_empty()).map(String::from).collect()
}

fn build_hdrop(files: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&20u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 12]);
    out.extend_from_slice(&1u32.to_le_bytes());
    for f in files {
        out.extend(f.encode_utf16().chain([0]).flat_map(u16::to_le_bytes));
    }
    out.extend_from_slice(&[0, 0]);
    out
}

/// A packed DIB (BITMAPINFOHEADER + pixels) to RGBA. 24 and 32 bit, uncompressed or bitfields.
fn dib_to_rgba(dib: &[u8]) -> Option<Image> {
    let u32_at = |o: usize| dib.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let header = u32_at(0)? as usize;
    let width = u32_at(4)? as i32;
    let height = u32_at(8)? as i32;
    let bits = u16::from_le_bytes(dib.get(14..16)?.try_into().ok()?);
    let compression = u32_at(16)?;
    let colors_used = u32_at(32)? as usize;
    if width <= 0 || height == 0 || !(bits == 24 || bits == 32) || !(compression == 0 || compression == 3) {
        return None;
    }
    let masks = if compression == 3 && header == 40 { 12 } else { 0 };
    let palette = if bits <= 8 { colors_used.max(1 << bits) * 4 } else { colors_used * 4 };
    let start = header + masks + palette;
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    let bpp = bits as usize / 8;
    let stride = (w * bpp + 3) & !3;
    let pixels = dib.get(start..start + stride * h)?;
    let mut rgba = vec![0u8; w * h * 4];
    let mut any_alpha = false;
    for y in 0..h {
        let src_row = if height > 0 { h - 1 - y } else { y };
        let row = &pixels[src_row * stride..src_row * stride + w * bpp];
        for x in 0..w {
            let p = &row[x * bpp..x * bpp + bpp];
            let o = (y * w + x) * 4;
            rgba[o] = p[2];
            rgba[o + 1] = p[1];
            rgba[o + 2] = p[0];
            rgba[o + 3] = if bpp == 4 { p[3] } else { 255 };
            any_alpha |= bpp == 4 && p[3] != 0;
        }
    }
    if bpp == 4 && !any_alpha {
        rgba.chunks_exact_mut(4).for_each(|px| px[3] = 255);
    }
    Some(Image { width: w as u32, height: h as u32, rgba })
}

fn rgba_to_dib(img: &Image) -> Vec<u8> {
    let (w, h) = (img.width as usize, img.height as usize);
    let mut out = Vec::with_capacity(40 + w * h * 4);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&((w * h * 4) as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    for y in (0..h).rev() {
        for px in img.rgba[y * w * 4..(y + 1) * w * 4].chunks_exact(4) {
            out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    out
}

fn process_name(hwnd: HWND) -> String {
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
        if !ok {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    }
}

/// Name of the program in front.
pub fn foreground_app() -> String {
    process_name(unsafe { GetForegroundWindow() })
}

pub fn read() -> Result<Capture, String> {
    let f = formats();
    let open = Open::new()?;
    let list = open.list();
    let mut cap = Capture::default();
    let concealed_flag = list.contains(&f.can_include) && open.bytes(f.can_include).is_some_and(|b| b.iter().all(|x| *x == 0));
    cap.concealed = list.contains(&f.exclude) || list.contains(&f.viewer_ignore) || concealed_flag;
    if cap.concealed {
        return Ok(cap);
    }
    if list.contains(&CF_HDROP) {
        cap.files = open.bytes(CF_HDROP).map(|b| parse_hdrop(&b)).unwrap_or_default();
    }
    if list.contains(&CF_UNICODETEXT) {
        cap.text = open.bytes(CF_UNICODETEXT).map(|b| from_utf16(&b));
    }
    if list.contains(&f.html) {
        cap.html = open.bytes(f.html).map(|b| ansi(&b));
    }
    if list.contains(&f.rtf) {
        cap.rtf = open.bytes(f.rtf).map(|b| ansi(&b));
    }
    if cap.files.is_empty() && cap.text.is_none() {
        cap.image = image_of(&open, &list);
    }
    let owner = unsafe { GetClipboardOwner() }.ok().filter(|h| !h.0.is_null());
    drop(open);
    cap.app = owner.map(process_name).filter(|n| !n.is_empty()).unwrap_or_else(foreground_app);
    Ok(cap)
}

fn image_of(open: &Open, list: &[u32]) -> Option<Image> {
    let f = formats();
    list.contains(&f.png)
        .then(|| open.bytes(f.png))
        .flatten()
        .and_then(|png| image::load_from_memory_with_format(&png, image::ImageFormat::Png).ok())
        .map(|img| {
            let rgba = img.to_rgba8();
            Image { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() }
        })
        .or_else(|| list.contains(&CF_DIB).then(|| open.bytes(CF_DIB)).flatten().and_then(|d| dib_to_rgba(&d)))
}

/// The copied image, even when text or HTML came with it.
pub fn read_image() -> Option<Image> {
    let open = Open::new().ok()?;
    let list = open.list();
    image_of(&open, &list)
}

/// Text only, for plain-text paste and snippet variables.
pub fn read_text() -> Option<String> {
    let open = Open::new().ok()?;
    open.has(CF_UNICODETEXT).then(|| open.bytes(CF_UNICODETEXT)).flatten().map(|b| from_utf16(&b))
}

/// Replaces the clipboard and returns the sequence number it ends on.
pub fn write(p: &Payload) -> Result<u32, String> {
    let f = formats();
    let image = match &p.png {
        Some(png) => Some(image::load_from_memory_with_format(png, image::ImageFormat::Png).map_err(|e| e.to_string())?.to_rgba8()),
        None => None,
    };
    let open = Open::new()?;
    open.empty()?;
    if !p.files.is_empty() {
        open.put(CF_HDROP, &build_hdrop(&p.files))?;
        open.put(f.drop_effect, &1u32.to_le_bytes())?;
    }
    if let Some(text) = &p.text {
        open.put(CF_UNICODETEXT, &utf16_bytes(text))?;
    }
    if let Some(html) = &p.html {
        open.put(f.html, &[html.as_bytes(), &[0]].concat())?;
    }
    if let Some(rtf) = &p.rtf {
        open.put(f.rtf, &[rtf.as_bytes(), &[0]].concat())?;
    }
    if let (Some(png), Some(rgba)) = (&p.png, image) {
        open.put(f.png, png)?;
        let img = Image { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() };
        open.put(CF_DIB, &rgba_to_dib(&img))?;
    }
    drop(open);
    Ok(sequence())
}

/// Like `write`, but only when the clipboard still holds the copy numbered `seq`.
pub fn write_if(seq: u32, p: &Payload) -> Result<Option<u32>, String> {
    if sequence() != seq {
        return Ok(None);
    }
    write(p).map(Some)
}

pub fn clear() -> Result<u32, String> {
    let open = Open::new()?;
    open.empty()?;
    drop(open);
    Ok(sequence())
}

/// Every byte-backed format on the clipboard, to put back after a temporary change.
pub fn snapshot() -> Result<Snapshot, String> {
    let open = Open::new()?;
    let list = open.list();
    let mut total = 0;
    let mut formats = Vec::new();
    for format in list {
        if HANDLE_FORMATS.contains(&format) || SYNTHESISED.contains(&format) {
            continue;
        }
        if let Some(bytes) = open.bytes(format) {
            total += bytes.len();
            if total > SNAPSHOT_LIMIT {
                return Err("The clipboard is too large to put back.".into());
            }
            formats.push((format, bytes));
        }
    }
    Ok(Snapshot { formats })
}

pub fn restore(s: &Snapshot) -> Result<u32, String> {
    let open = Open::new()?;
    open.empty()?;
    for (format, bytes) in &s.formats {
        open.put(*format, bytes)?;
    }
    drop(open);
    Ok(sequence())
}

pub fn foreground() -> Option<isize> {
    let h = unsafe { GetForegroundWindow() };
    (!h.0.is_null()).then_some(h.0 as isize)
}

pub fn focus(window: isize) {
    let hwnd = HWND(window as *mut c_void);
    unsafe {
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_MENU: u16 = 0x12;
const VK_LWIN: u16 = 0x5B;
const VK_RWIN: u16 = 0x5C;
const VK_RETURN: u16 = 0x0D;
const VK_BACK: u16 = 0x08;
const VK_ESCAPE: u16 = 0x1B;
const VK_V: u16 = 0x56;

fn key(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

fn send(inputs: &[INPUT]) {
    unsafe {
        SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

fn held(vk: u16) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

/// Waits for the shortcut's modifiers to come up, then lifts any still down, so the paste
/// keystroke isn't read as a different shortcut.
pub fn release_modifiers() {
    const MODS: [u16; 5] = [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN];
    let start = Instant::now();
    while MODS.iter().any(|m| held(*m)) && start.elapsed() < Duration::from_millis(600) {
        thread::sleep(Duration::from_millis(15));
    }
    let up: Vec<INPUT> = MODS.iter().filter(|m| held(**m)).map(|m| key(*m, 0, KEYEVENTF_KEYUP)).collect();
    if !up.is_empty() {
        send(&up);
    }
}

pub fn send_paste() {
    send(&[
        key(VK_CONTROL, 0, KEYBD_EVENT_FLAGS(0)),
        key(VK_V, 0, KEYBD_EVENT_FLAGS(0)),
        key(VK_V, 0, KEYEVENTF_KEYUP),
        key(VK_CONTROL, 0, KEYEVENTF_KEYUP),
    ]);
}

pub fn press_escape() {
    send(&[key(VK_ESCAPE, 0, KEYBD_EVENT_FLAGS(0)), key(VK_ESCAPE, 0, KEYEVENTF_KEYUP)]);
}

/// Presses Backspace `count` times.
pub fn erase(count: usize) {
    let inputs: Vec<INPUT> = (0..count).flat_map(|_| [key(VK_BACK, 0, KEYBD_EVENT_FLAGS(0)), key(VK_BACK, 0, KEYEVENTF_KEYUP)]).collect();
    send(&inputs);
}

pub fn type_text(text: &str) {
    let mut inputs = Vec::new();
    for line in text.replace("\r\n", "\n").split_inclusive('\n') {
        let (body, newline) = match line.strip_suffix('\n') {
            Some(b) => (b, true),
            None => (line, false),
        };
        for unit in body.encode_utf16() {
            inputs.push(key(0, unit, KEYEVENTF_UNICODE));
            inputs.push(key(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
        }
        if newline {
            inputs.push(key(VK_RETURN, 0, KEYBD_EVENT_FLAGS(0)));
            inputs.push(key(VK_RETURN, 0, KEYEVENTF_KEYUP));
        }
    }
    for chunk in inputs.chunks(200) {
        send(chunk);
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dib_round_trip() {
        let img = Image { width: 3, height: 2, rgba: (0..24).map(|i| (i * 10) as u8).collect() };
        let back = dib_to_rgba(&rgba_to_dib(&img)).unwrap();
        assert_eq!((back.width, back.height), (3, 2));
        assert_eq!(back.rgba, img.rgba);
    }

    #[test]
    fn hdrop_round_trip() {
        let files = vec!["C:\\a b\\one.txt".to_string(), "D:\\two.png".to_string()];
        assert_eq!(parse_hdrop(&build_hdrop(&files)), files);
    }
}
