//! Windows side of capture: screen grabs, the window list, wheel input, text recognition and the
//! watermark text.

use std::{ffi::c_void, os::windows::process::CommandExt};

use image::RgbaImage;
use windows::{
    core::{w, BOOL, HSTRING, PCWSTR},
    Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Storage::Streams::DataWriter,
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, POINT, RECT, SIZE},
        Graphics::{
            Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS},
            Gdi::{
                BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, DeleteDC, DeleteObject, GetDC, GetDIBits,
                GetTextExtentPoint32W, PatBlt, ReleaseDC, SelectObject, SetBkMode, SetTextColor, TextOutW, ANTIALIASED_QUALITY,
                BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLACKNESS, CAPTUREBLT, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS,
                FW_SEMIBOLD, OUT_DEFAULT_PRECIS, ROP_CODE, SRCCOPY, TRANSPARENT,
            },
        },
        Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS},
        System::{
            Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
            SystemInformation::GetLocalTime,
        },
        UI::{
            Shell::{SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW},
            Input::KeyboardAndMouse::{GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_WHEEL, MOUSEINPUT},
            WindowsAndMessaging::{
                EnumWindows, GetCursorPos, GetSystemMetrics, GetWindowLongW, GetWindowRect, GetWindowTextLengthW, IsIconic,
                IsWindowVisible, MessageBoxW, SetCursorPos, SetWindowDisplayAffinity, GWL_EXSTYLE, IDYES, MB_ICONQUESTION, MB_SETFOREGROUND,
                MB_TOPMOST, MB_YESNO, PW_RENDERFULLCONTENT, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, WDA_EXCLUDEFROMCAPTURE,
                WDA_NONE, WS_EX_TOOLWINDOW,
            },
        },
    },
};

use super::logic::{blend_mask, normalise_contrast, ocr_scale, DateParts, Frame, Rect};

/// Marks the wheel events Notifi sends, so other hooks can tell them from the person's.
const WHEEL_TAG: usize = 0x4E_54_46_43;

pub fn available() -> bool {
    true
}

pub fn virtual_screen() -> Rect {
    unsafe {
        Rect::new(
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

/// The screen inside `r`, layered windows included, as RGBA.
pub fn grab(r: &Rect) -> Option<Frame> {
    if r.is_empty() {
        return None;
    }
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, r.w, r.h);
        let old = SelectObject(mem, bitmap.into());
        let copied = BitBlt(mem, 0, 0, r.w, r.h, Some(screen), r.x, r.y, ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0)).is_ok();
        // A bitmap can't be read while it is selected into a device context.
        SelectObject(mem, old);
        let mut out = None;
        if copied {
            let mut pixels = vec![0u8; r.w as usize * r.h as usize * 4];
            if read_bits(screen, bitmap, r.w, r.h, &mut pixels) {
                for p in pixels.chunks_exact_mut(4) {
                    p.swap(0, 2);
                    p[3] = 255;
                }
                out = Some(Frame { origin: (r.x, r.y), w: r.w as u32, h: r.h as u32, rgba: pixels });
            }
        }
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        out
    }
}

unsafe fn read_bits(dc: windows::Win32::Graphics::Gdi::HDC, bitmap: windows::Win32::Graphics::Gdi::HBITMAP, w: i32, h: i32, out: &mut [u8]) -> bool {
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    GetDIBits(dc, bitmap, 0, h as u32, Some(out.as_mut_ptr() as *mut c_void), &mut info, DIB_RGB_COLORS) != 0
}

struct Walk {
    skip: Vec<isize>,
    found: Vec<(isize, Rect)>,
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let walk = &mut *(lparam.0 as *mut Walk);
    if !walk.skip.contains(&(hwnd.0 as isize)) {
        if let Some(r) = visible_rect(hwnd) {
            walk.found.push((hwnd.0 as isize, r));
        }
    }
    true.into()
}

unsafe fn visible_rect(hwnd: HWND) -> Option<Rect> {
    if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
        return None;
    }
    if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 || GetWindowTextLengthW(hwnd) == 0 {
        return None;
    }
    let mut cloaked = 0u32;
    if DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, 4).is_ok() && cloaked != 0 {
        return None;
    }
    // The frame bounds leave out the invisible resize borders and the shadow.
    let mut r = RECT::default();
    let size = std::mem::size_of::<RECT>() as u32;
    if DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut RECT as *mut c_void, size).is_err() {
        GetWindowRect(hwnd, &mut r).ok()?;
    }
    let rect = Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top);
    (rect.w >= 16 && rect.h >= 16).then_some(rect)
}

/// Visible top-level windows, topmost first, leaving out the given handles.
pub fn top_windows(skip: &[isize]) -> Vec<(isize, Rect)> {
    let mut walk = Walk { skip: skip.to_vec(), found: Vec::new() };
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut walk as *mut Walk as isize));
    }
    walk.found
}

/// The window's own picture, drawn by the window itself, so windows on top of it stay out.
/// `frame` is the visible bounds on screen; None when the window cannot draw itself or came out blank.
pub fn grab_window(hwnd: isize, frame: &Rect) -> Option<RgbaImage> {
    unsafe {
        let hwnd = HWND(hwnd as *mut c_void);
        let mut outer = RECT::default();
        GetWindowRect(hwnd, &mut outer).ok()?;
        let (w, h) = (outer.right - outer.left, outer.bottom - outer.top);
        if w <= 0 || h <= 0 {
            return None;
        }
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bitmap.into());
        let drawn = PrintWindow(hwnd, mem, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool();
        SelectObject(mem, old);
        let mut out = None;
        if drawn {
            let mut pixels = vec![0u8; w as usize * h as usize * 4];
            if read_bits(screen, bitmap, w, h, &mut pixels) {
                for p in pixels.chunks_exact_mut(4) {
                    p.swap(0, 2);
                    p[3] = 255;
                }
                if pixels.chunks_exact(4).any(|p| p[0] != 0 || p[1] != 0 || p[2] != 0) {
                    let whole = Frame { origin: (outer.left, outer.top), w: w as u32, h: h as u32, rgba: pixels };
                    out = whole.crop(frame);
                }
            }
        }
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        out
    }
}

/// Keeps a window out of screen grabs (or lets it back in). False when Windows refuses.
pub fn set_excluded(hwnd: isize, on: bool) -> bool {
    unsafe { SetWindowDisplayAffinity(HWND(hwnd as *mut c_void), if on { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE }).is_ok() }
}

pub fn cursor() -> (i32, i32) {
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    (pt.x, pt.y)
}

pub fn set_cursor(x: i32, y: i32) {
    unsafe {
        let _ = SetCursorPos(x, y);
    }
}

/// One wheel notch per call; negative scrolls down.
pub fn wheel(notches: i32) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: (notches * 120) as u32, dwFlags: MOUSEEVENTF_WHEEL, time: 0, dwExtraInfo: WHEEL_TAG },
        },
    };
    unsafe {
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn key_down(vk: i32) -> bool {
    unsafe { GetAsyncKeyState(vk) as u16 & 0x8000 != 0 }
}

pub fn local_time() -> DateParts {
    let t = unsafe { GetLocalTime() };
    DateParts { year: i32::from(t.wYear), month: u32::from(t.wMonth), day: u32::from(t.wDay), hour: u32::from(t.wHour), minute: u32::from(t.wMinute), second: u32::from(t.wSecond) }
}

pub fn confirm_open(url: &str) -> bool {
    let text = HSTRING::from(format!("Open this link?\n\n{url}"));
    unsafe { MessageBoxW(None, &text, w!("Notifi"), MB_YESNO | MB_ICONQUESTION | MB_TOPMOST | MB_SETFOREGROUND) == IDYES }
}

pub fn open_url(url: &str) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).creation_flags(CREATE_NO_WINDOW).spawn();
}

// ---- Text recognition ----

/// WinRT calls run on a thread of their own that is set up for them.
fn on_mta<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    std::thread::spawn(move || {
        let initialised = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        let out = f();
        if initialised {
            unsafe { CoUninitialize() };
        }
        out
    })
    .join()
    .unwrap_or_else(|_| Err("Text recognition stopped unexpectedly.".into()))
}

/// The engines to try, in order: the chosen languages that are installed, then the profile languages.
fn engines(languages: &[String]) -> Result<Vec<OcrEngine>, String> {
    let wanted: Vec<String> = languages.iter().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty()).collect();
    let mut out = Vec::new();
    if !wanted.is_empty() {
        if let Ok(available) = OcrEngine::AvailableRecognizerLanguages() {
            for lang in available {
                let tag = lang.LanguageTag().map(|t| t.to_string().to_lowercase()).unwrap_or_default();
                let name = lang.DisplayName().map(|t| t.to_string().to_lowercase()).unwrap_or_default();
                let primary = tag.split('-').next().unwrap_or_default().to_string();
                if wanted.iter().any(|w| name.starts_with(w.as_str()) || tag == *w || primary == *w) {
                    if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&lang) {
                        out.push(engine);
                    }
                }
            }
        }
    }
    if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
        out.push(engine);
    }
    if out.is_empty() {
        Err("No text recognition language is installed. Add one in Windows Settings under Time & language.".into())
    } else {
        Ok(out)
    }
}

fn recognise(engine: &OcrEngine, img: &RgbaImage) -> Result<Vec<String>, String> {
    let err = |e: windows::core::Error| e.message();
    let bgra: Vec<u8> = img.pixels().flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
    let writer = DataWriter::new().map_err(err)?;
    writer.WriteBytes(&bgra).map_err(err)?;
    let buffer = writer.DetachBuffer().map_err(err)?;
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, img.width() as i32, img.height() as i32).map_err(err)?;
    let result = engine.RecognizeAsync(&bitmap).and_then(|op| op.join()).map_err(err)?;
    let mut lines = Vec::new();
    for line in result.Lines().map_err(err)? {
        lines.push(line.Text().map_err(err)?.to_string());
    }
    Ok(lines)
}

/// The text lines Windows finds in the image. A first pass runs on a contrast-normalised, rescaled
/// copy; when that finds nothing the unaltered copy is tried before giving up.
pub fn ocr_lines(img: &RgbaImage, languages: &[String]) -> Result<Vec<String>, String> {
    let img = img.clone();
    let languages = languages.to_vec();
    on_mta(move || {
        let engines = engines(&languages)?;
        let max_side = OcrEngine::MaxImageDimension().unwrap_or(2600);
        let scale = ocr_scale(img.width(), img.height(), max_side);
        let scaled = if (scale - 1.0).abs() < 0.01 {
            img.clone()
        } else {
            let filter = if scale > 1.0 { image::imageops::FilterType::CatmullRom } else { image::imageops::FilterType::Triangle };
            image::imageops::resize(&img, ((f64::from(img.width()) * scale).round() as u32).max(1), ((f64::from(img.height()) * scale).round() as u32).max(1), filter)
        };
        let mut normalised = scaled.clone();
        normalise_contrast(&mut normalised);
        let mut last_error = None;
        for variant in [&normalised, &scaled] {
            for engine in &engines {
                match recognise(engine, variant) {
                    Ok(lines) if lines.iter().any(|l| !l.trim().is_empty()) => return Ok(lines),
                    Ok(_) => {}
                    Err(e) => last_error = Some(e),
                }
            }
        }
        match last_error {
            Some(e) => Err(e),
            None => Ok(Vec::new()),
        }
    })
}

/// Boxes (x, y, width, height in image pixels) around every word Windows finds in the image.
pub fn ocr_boxes(img: &RgbaImage, languages: &[String]) -> Result<Vec<[f64; 4]>, String> {
    let img = img.clone();
    let languages = languages.to_vec();
    on_mta(move || {
        let engines = engines(&languages)?;
        let max_side = OcrEngine::MaxImageDimension().unwrap_or(2600);
        let scale = ocr_scale(img.width(), img.height(), max_side);
        let scaled = if (scale - 1.0).abs() < 0.01 {
            img.clone()
        } else {
            let filter = if scale > 1.0 { image::imageops::FilterType::CatmullRom } else { image::imageops::FilterType::Triangle };
            image::imageops::resize(&img, ((f64::from(img.width()) * scale).round() as u32).max(1), ((f64::from(img.height()) * scale).round() as u32).max(1), filter)
        };
        let mut normalised = scaled.clone();
        normalise_contrast(&mut normalised);
        let mut last_error = None;
        for variant in [&normalised, &scaled] {
            for engine in &engines {
                match word_boxes(engine, variant) {
                    Ok(boxes) if !boxes.is_empty() => return Ok(boxes.into_iter().map(|b| b.map(|v| v / scale)).collect()),
                    Ok(_) => {}
                    Err(e) => last_error = Some(e),
                }
            }
        }
        match last_error {
            Some(e) => Err(e),
            None => Ok(Vec::new()),
        }
    })
}

fn word_boxes(engine: &OcrEngine, img: &RgbaImage) -> Result<Vec<[f64; 4]>, String> {
    let err = |e: windows::core::Error| e.message();
    let bgra: Vec<u8> = img.pixels().flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
    let writer = DataWriter::new().map_err(err)?;
    writer.WriteBytes(&bgra).map_err(err)?;
    let buffer = writer.DetachBuffer().map_err(err)?;
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, img.width() as i32, img.height() as i32).map_err(err)?;
    let result = engine.RecognizeAsync(&bitmap).and_then(|op| op.join()).map_err(err)?;
    let mut boxes = Vec::new();
    for line in result.Lines().map_err(err)? {
        for word in line.Words().map_err(err)? {
            let r = word.BoundingRect().map_err(err)?;
            boxes.push([f64::from(r.X), f64::from(r.Y), f64::from(r.Width), f64::from(r.Height)]);
        }
    }
    Ok(boxes)
}

/// Moves a file to the Recycle Bin.
pub fn trash(path: &std::path::Path) -> Result<(), String> {
    // The list must end with two nulls.
    let mut from: Vec<u16> = path.to_string_lossy().encode_utf16().collect();
    from.extend([0, 0]);
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO.0 | FOF_NOCONFIRMATION.0 | FOF_NOERRORUI.0 | FOF_SILENT.0) as u16,
        ..Default::default()
    };
    let code = unsafe { SHFileOperationW(&mut op) };
    if code != 0 || op.fAnyOperationsAborted.as_bool() {
        return Err(format!("Windows could not move {} to the Recycle Bin.", path.display()));
    }
    Ok(())
}

// ---- Watermark ----

/// Draws `text` small in the bottom right corner of the image.
pub fn watermark(img: &mut RgbaImage, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return;
    }
    let px = (img.width() / 45).clamp(14, 40) as i32;
    let Some((mw, mh, mask)) = text_mask(&wide, px) else { return };
    let margin = px / 2 + 8;
    let x = (img.width() as i32 - mw - margin).max(0);
    let y = (img.height() as i32 - mh - margin).max(0);
    blend_mask(img, &mask, mw as u32, mh as u32, x + 1, y + 1, 0, 0.5);
    blend_mask(img, &mask, mw as u32, mh as u32, x, y, 255, 0.85);
}

/// Coverage (0 to 255) of the text drawn in white on black, with its width and height.
fn text_mask(wide: &[u16], px: i32) -> Option<(i32, i32, Vec<u8>)> {
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(screen));
        let font = CreateFontW(-px, 0, 0, 0, FW_SEMIBOLD.0 as i32, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, ANTIALIASED_QUALITY, 0, w!("Segoe UI"));
        let old_font = SelectObject(mem, font.into());
        let mut size = SIZE::default();
        let measured = GetTextExtentPoint32W(mem, wide, &mut size).as_bool();
        let (tw, th) = (size.cx.max(1), size.cy.max(1));
        let bitmap = CreateCompatibleBitmap(screen, tw, th);
        let old_bitmap = SelectObject(mem, bitmap.into());
        let _ = PatBlt(mem, 0, 0, tw, th, BLACKNESS);
        SetBkMode(mem, TRANSPARENT);
        SetTextColor(mem, COLORREF(0x00FF_FFFF));
        let _ = TextOutW(mem, 0, 0, wide);
        SelectObject(mem, old_bitmap);
        let mut pixels = vec![0u8; tw as usize * th as usize * 4];
        let read = measured && read_bits(screen, bitmap, tw, th, &mut pixels);
        SelectObject(mem, old_font);
        let _ = DeleteObject(font.into());
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        read.then(|| (tw, th, pixels.chunks_exact(4).map(|p| p[1]).collect()))
    }
}
