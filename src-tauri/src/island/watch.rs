//! Watch a window (spec/everyday/watch.md): the person marks a rectangle on the screen, a thread
//! reads it with on-device text recognition every few seconds, the text goes to the resting pill
//! and the rules ("changes", "above 50", "contains Done") raise notices.
//!
//! The area lives in `watch-area.json` in the app data folder: it is picked by a custom control, and
//! custom options are not stored in the settings map. Text recognition and the screen grab are
//! Windows only for now, like the capture module they were copied from.

use std::{
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

use notifi_platform::Platform;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::settings::{Settings, Store};

const NS: &str = "watch";
const FILE: &str = "watch-area.json";
/// Longest text shown on the pill.
const PILL_CHARS: usize = 24;

/// A rectangle in physical pixels of the virtual screen.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The window the area was picked on, so a moved or closed window pauses the watch.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub id: u64,
    pub bounds: Region,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub region: Region,
    pub anchor: Option<Anchor>,
}

/// What the pill shows for the watch. `paused` says why nothing is being read, or is empty.
#[derive(Serialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub text: String,
    pub paused: String,
}

static AREA: Mutex<Option<Area>> = Mutex::new(None);
static READING: Mutex<Option<Reading>> = Mutex::new(None);

/// The reading for the Island snapshot; `None` when there is nothing to show.
pub fn reading() -> Option<Reading> {
    READING.lock().unwrap().clone()
}

// ---- Pure logic ----

/// The first number in `text`: optional minus, digits with thousands commas, optional decimals.
/// "1,234.5%" is 1234.5 and "\u{2212}12 %" is -12.
pub fn parse_number(text: &str) -> Option<f64> {
    let chars: Vec<char> = text.chars().collect();
    let start = chars.iter().position(|c| c.is_ascii_digit())?;
    let negative = start > 0 && matches!(chars[start - 1], '-' | '\u{2212}' | '\u{2013}');
    let mut out = String::new();
    let mut i = start;
    while i < chars.len() {
        match chars[i] {
            c if c.is_ascii_digit() => out.push(c),
            ',' if chars.get(i + 1).is_some_and(char::is_ascii_digit) => {}
            '.' if !out.contains('.') && chars.get(i + 1).is_some_and(char::is_ascii_digit) => out.push('.'),
            _ => break,
        }
        i += 1;
    }
    let n: f64 = out.parse().ok()?;
    Some(if negative { -n } else { n })
}

#[derive(Debug, Clone, PartialEq)]
pub enum Rule {
    Changes,
    Equals(String),
    Above(f64),
    Below(f64),
    Contains(String),
}

impl Rule {
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim().to_lowercase();
        let after = |prefixes: &[&str]| prefixes.iter().find_map(|p| t.strip_prefix(p)).map(|r| r.trim().to_string()).filter(|r| !r.is_empty());
        if matches!(t.as_str(), "change" | "changes" | "any change") {
            return Some(Self::Changes);
        }
        if let Some(v) = after(&["equals ", "equal to ", "==", "="]) {
            return Some(Self::Equals(v));
        }
        if let Some(v) = after(&["above ", "over ", "more than ", ">"]) {
            return parse_number(&v).map(Self::Above);
        }
        if let Some(v) = after(&["below ", "under ", "less than ", "<"]) {
            return parse_number(&v).map(Self::Below);
        }
        after(&["contains "]).map(Self::Contains)
    }

    fn holds(&self, text: &str) -> bool {
        match self {
            Self::Changes => false,
            Self::Equals(v) => match (parse_number(v), parse_number(text)) {
                (Some(a), Some(b)) => (a - b).abs() < 1e-9,
                _ => text.trim().to_lowercase() == *v,
            },
            Self::Above(n) => parse_number(text).is_some_and(|x| x > *n),
            Self::Below(n) => parse_number(text).is_some_and(|x| x < *n),
            Self::Contains(v) => text.to_lowercase().contains(v.as_str()),
        }
    }

    /// Whether the rule fires for a new reading. A condition fires when it becomes true, not on
    /// every reading while it stays true; "changes" needs an earlier reading to compare with.
    pub fn fires(&self, previous: Option<&str>, text: &str) -> bool {
        match self {
            Self::Changes => previous.is_some_and(|p| p != text),
            _ => self.holds(text) && !previous.is_some_and(|p| self.holds(p)),
        }
    }
}

/// Collapses whitespace and cuts the text to what fits on the pill.
pub fn pill_text(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= PILL_CHARS {
        return flat;
    }
    let mut cut: String = flat.chars().take(PILL_CHARS - 1).collect();
    cut.push('\u{2026}');
    cut
}

/// Why a watch on a window must pause, from where the window is now (`None` when it is gone).
pub fn pause_reason(anchor: Option<Anchor>, now: Option<Region>) -> &'static str {
    match (anchor, now) {
        (None, _) => "",
        (Some(_), None) => "The watched window closed.",
        (Some(a), Some(b)) if a.bounds != b => "The watched window moved.",
        _ => "",
    }
}

/// A size no bigger than the recogniser takes, and big enough that small text is readable.
pub fn ocr_size(w: u32, h: u32, max_side: u32) -> (u32, u32) {
    let up = if h < 100 { (100.0 / f64::from(h.max(1))).ceil().min(4.0) } else { 1.0 };
    let scale = up.min(f64::from(max_side) / f64::from(w.max(h).max(1)));
    (((f64::from(w) * scale).round() as u32).max(1), ((f64::from(h) * scale).round() as u32).max(1))
}

// ---- Screen and text recognition (Windows) ----

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;

    use image::RgbaImage;
    use windows::{
        Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
        Media::Ocr::OcrEngine,
        Storage::Streams::DataWriter,
        Win32::{
            Graphics::Gdi::{
                BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO,
                BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, ROP_CODE, SRCCOPY,
            },
            System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
            UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN},
        },
    };

    use super::Region;

    pub fn virtual_screen() -> Region {
        unsafe {
            Region {
                x: GetSystemMetrics(SM_XVIRTUALSCREEN),
                y: GetSystemMetrics(SM_YVIRTUALSCREEN),
                w: GetSystemMetrics(SM_CXVIRTUALSCREEN),
                h: GetSystemMetrics(SM_CYVIRTUALSCREEN),
            }
        }
    }

    pub fn grab(r: Region) -> Option<RgbaImage> {
        if r.w <= 0 || r.h <= 0 {
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
                let mut info = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: r.w,
                        biHeight: -r.h,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                if GetDIBits(screen, bitmap, 0, r.h as u32, Some(pixels.as_mut_ptr() as *mut c_void), &mut info, DIB_RGB_COLORS) != 0 {
                    for p in pixels.chunks_exact_mut(4) {
                        p.swap(0, 2);
                        p[3] = 255;
                    }
                    out = RgbaImage::from_raw(r.w as u32, r.h as u32, pixels);
                }
            }
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            out
        }
    }

    /// The text Windows reads in the image, lines joined by a space.
    pub fn read(img: &RgbaImage) -> Result<String, String> {
        let img = img.clone();
        // WinRT calls run on a thread of their own that is set up for them.
        std::thread::spawn(move || {
            let initialised = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
            let out = recognise(&img);
            if initialised {
                unsafe { CoUninitialize() };
            }
            out
        })
        .join()
        .unwrap_or_else(|_| Err("Text recognition stopped unexpectedly.".into()))
    }

    fn recognise(img: &RgbaImage) -> Result<String, String> {
        let err = |e: windows::core::Error| e.message();
        let engine = OcrEngine::TryCreateFromUserProfileLanguages()
            .map_err(|_| "No text recognition language is installed. Add one in Windows Settings under Time & language.".to_string())?;
        let max_side = OcrEngine::MaxImageDimension().unwrap_or(2600);
        let (w, h) = super::ocr_size(img.width(), img.height(), max_side);
        let scaled = if (w, h) == (img.width(), img.height()) {
            img.clone()
        } else {
            let filter = if w > img.width() { image::imageops::FilterType::CatmullRom } else { image::imageops::FilterType::Triangle };
            image::imageops::resize(img, w, h, filter)
        };
        let bgra: Vec<u8> = scaled.pixels().flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
        let writer = DataWriter::new().map_err(err)?;
        writer.WriteBytes(&bgra).map_err(err)?;
        let buffer = writer.DetachBuffer().map_err(err)?;
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, scaled.width() as i32, scaled.height() as i32).map_err(err)?;
        let result = engine.RecognizeAsync(&bitmap).and_then(|op| op.join()).map_err(err)?;
        let mut lines = Vec::new();
        for line in result.Lines().map_err(err)? {
            lines.push(line.Text().map_err(err)?.to_string());
        }
        Ok(lines.join(" "))
    }
}

#[cfg(not(windows))]
mod sys {
    use image::RgbaImage;

    use super::Region;

    pub fn virtual_screen() -> Region {
        Region::default()
    }

    pub fn grab(_: Region) -> Option<RgbaImage> {
        None
    }

    pub fn read(_: &RgbaImage) -> Result<String, String> {
        Err("Watching a window is only built for Windows so far.".into())
    }
}

fn read_region(region: Region) -> Result<String, String> {
    let img = sys::grab(region).ok_or("Notifi could not read that part of the screen.")?;
    sys::read(&img)
}

// ---- Stored area ----

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(FILE))
}

fn load(app: &AppHandle) -> Option<Area> {
    serde_json::from_slice(&std::fs::read(path(app)?).ok()?).ok()
}

fn store(app: &AppHandle, area: Option<Area>) {
    *AREA.lock().unwrap() = area;
    let Some(file) = path(app) else { return };
    match area {
        Some(a) => {
            if let (Some(dir), Ok(bytes)) = (file.parent(), serde_json::to_vec(&a)) {
                let _ = std::fs::create_dir_all(dir);
                let _ = std::fs::write(&file, bytes);
            }
        }
        None => {
            let _ = std::fs::remove_file(&file);
        }
    }
}

fn publish(app: &AppHandle, next: Option<Reading>) {
    let changed = {
        let mut current = READING.lock().unwrap();
        let changed = *current != next;
        *current = next;
        changed
    };
    if changed {
        app.state::<super::IslandState>().wake();
    }
}

// ---- The watcher ----

fn interval(s: &Settings) -> Duration {
    let secs = s.option(NS, "interval").and_then(|v| v.as_f64()).unwrap_or(2.0).clamp(1.0, 60.0);
    Duration::from_millis((secs * 1000.0) as u64)
}

fn rules(s: &Settings) -> Vec<(String, Rule)> {
    let list = s.option(NS, "rules").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    list.iter().filter_map(|v| v.as_str()).filter_map(|t| Rule::parse(t).map(|r| (t.trim().to_string(), r))).collect()
}

fn window_bounds(app: &AppHandle, id: u64) -> Option<Region> {
    let platform = app.state::<Platform>();
    let list = platform.windows().list().ok()?;
    list.iter().find(|w| w.id.0 == id).map(|w| Region { x: w.bounds.x, y: w.bounds.y, w: w.bounds.width as i32, h: w.bounds.height as i32 })
}

pub fn start(app: &AppHandle) {
    *AREA.lock().unwrap() = load(app);
    let app = app.clone();
    let _ = std::thread::Builder::new().name("notifi-watch".into()).spawn(move || {
        let mut previous: Option<String> = None;
        let mut next = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let s = app.state::<Store>().get();
            let area = *AREA.lock().unwrap();
            let Some(area) = area.filter(|_| s.enabled(NS)) else {
                publish(&app, None);
                previous = None;
                continue;
            };
            if Instant::now() < next || app.state::<super::IslandState>().inner.lock().unwrap().locked {
                continue;
            }
            next = Instant::now() + interval(&s);
            let show = s.option(NS, "showInPill").and_then(|v| v.as_bool()).unwrap_or(true);
            let paused = pause_reason(area.anchor, area.anchor.and_then(|a| window_bounds(&app, a.id)));
            if !paused.is_empty() {
                previous = None;
                publish(&app, show.then(|| Reading { text: String::new(), paused: paused.into() }));
                continue;
            }
            match read_region(area.region) {
                Ok(text) => {
                    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                    for (label, rule) in rules(&s) {
                        if rule.fires(previous.as_deref(), &text) {
                            super::notice_to(&app, format!("Watch: {label} ({})", pill_text(&text)), Some("settings:island"));
                        }
                    }
                    publish(&app, show.then(|| Reading { text: pill_text(&text), paused: String::new() }));
                    previous = Some(text);
                }
                Err(e) => publish(&app, show.then(|| Reading { text: String::new(), paused: e })),
            }
        }
    });
}

// ---- Commands for the settings page ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screen {
    /// PNG data URL of the whole virtual screen, shrunk to fit `max_width`.
    image: String,
    region: Region,
}

#[tauri::command]
pub fn watch_area() -> Option<Area> {
    *AREA.lock().unwrap()
}

/// Takes a picture of the screen with the settings window out of the way, for the page to pick on.
#[tauri::command]
pub async fn watch_screen(app: AppHandle, max_width: u32) -> Result<Screen, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let window = app.get_webview_window(crate::window::SETTINGS);
        if let Some(w) = &window {
            let _ = w.hide();
            std::thread::sleep(Duration::from_millis(350));
        }
        let region = sys::virtual_screen();
        let shot = sys::grab(region);
        if let Some(w) = &window {
            let _ = w.show();
            if !crate::window::no_focus() {
                let _ = w.set_focus();
            }
        }
        let shot = shot.ok_or("Notifi could not read the screen.")?;
        let width = max_width.clamp(320, 4096).min(shot.width());
        let height = ((u64::from(shot.height()) * u64::from(width)) / u64::from(shot.width().max(1))).max(1) as u32;
        let small = image::imageops::resize(&shot, width, height, image::imageops::FilterType::Triangle);
        let mut png = Vec::new();
        {
            use image::{
                codecs::png::{CompressionType, FilterType, PngEncoder},
                ExtendedColorType, ImageEncoder,
            };
            PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::NoFilter)
                .write_image(small.as_raw(), small.width(), small.height(), ExtendedColorType::Rgba8)
                .map_err(|e| e.to_string())?;
        }
        Ok(Screen { image: format!("data:image/png;base64,{}", super::base64(&png)), region })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Saves the picked rectangle (physical pixels of the virtual screen) and the window under its centre.
#[tauri::command]
pub fn watch_set_area(app: AppHandle, region: Region) -> Result<Area, String> {
    if region.w < 4 || region.h < 4 {
        return Err("Pick a bigger area.".into());
    }
    let anchor = app
        .state::<Platform>()
        .windows()
        .window_at(region.x + region.w / 2, region.y + region.h / 2)
        .ok()
        .flatten()
        .map(|w| Anchor { id: w.id.0, bounds: Region { x: w.bounds.x, y: w.bounds.y, w: w.bounds.width as i32, h: w.bounds.height as i32 } });
    let area = Area { region, anchor };
    store(&app, Some(area));
    Ok(area)
}

#[tauri::command]
pub fn watch_clear_area(app: AppHandle) {
    store(&app, None);
    publish(&app, None);
}

/// Reads the saved area once, so the page can show what Notifi sees.
#[tauri::command]
pub async fn watch_read() -> Result<String, String> {
    let area = (*AREA.lock().unwrap()).ok_or("No area is picked yet.")?;
    tauri::async_runtime::spawn_blocking(move || read_region(area.region)).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_in_text() {
        assert_eq!(parse_number("45%"), Some(45.0));
        assert_eq!(parse_number("Loading 1,234.5 %"), Some(1234.5));
        assert_eq!(parse_number("-12%"), Some(-12.0));
        assert_eq!(parse_number("\u{2212}7"), Some(-7.0));
        assert_eq!(parse_number("12,5"), Some(125.0));
        assert_eq!(parse_number("score 3."), Some(3.0));
        assert_eq!(parse_number("1500%"), Some(1500.0));
        assert_eq!(parse_number("Done"), None);
    }

    #[test]
    fn rules_parse() {
        assert_eq!(Rule::parse("changes"), Some(Rule::Changes));
        assert_eq!(Rule::parse("Equals 100%"), Some(Rule::Equals("100%".into())));
        assert_eq!(Rule::parse("above 50"), Some(Rule::Above(50.0)));
        assert_eq!(Rule::parse("< 10"), Some(Rule::Below(10.0)));
        assert_eq!(Rule::parse("contains Done"), Some(Rule::Contains("done".into())));
        assert_eq!(Rule::parse("above lots"), None);
        assert_eq!(Rule::parse("whatever"), None);
        assert_eq!(Rule::parse(""), None);
    }

    #[test]
    fn changes_need_an_earlier_reading() {
        assert!(!Rule::Changes.fires(None, "5%"));
        assert!(!Rule::Changes.fires(Some("5%"), "5%"));
        assert!(Rule::Changes.fires(Some("5%"), "6%"));
    }

    #[test]
    fn conditions_fire_when_they_become_true() {
        let equals = Rule::parse("equals 100%").unwrap();
        assert!(equals.fires(Some("99%"), "100%"));
        assert!(equals.fires(None, "100 %"));
        assert!(!equals.fires(Some("100%"), "100%"));
        let above = Rule::parse("above 50").unwrap();
        assert!(above.fires(Some("40"), "51"));
        assert!(!above.fires(Some("51"), "60"));
        assert!(!above.fires(Some("60"), "40"));
        let contains = Rule::parse("contains done").unwrap();
        assert!(contains.fires(Some("Working"), "All DONE"));
        let words = Rule::parse("equals ready").unwrap();
        assert!(words.fires(Some("busy"), " Ready "));
    }

    #[test]
    fn pill_text_is_flat_and_short() {
        assert_eq!(pill_text("  45 \n %  "), "45 %");
        assert_eq!(pill_text("a very long sentence about nothing at all"), "a very long sentence ab\u{2026}");
        assert_eq!(pill_text("a very long sentence about nothing at all").chars().count(), PILL_CHARS);
    }

    #[test]
    fn a_moved_or_closed_window_pauses_the_watch() {
        let b = Region { x: 10, y: 10, w: 400, h: 300 };
        let anchor = Some(Anchor { id: 7, bounds: b });
        assert_eq!(pause_reason(anchor, Some(b)), "");
        assert_eq!(pause_reason(anchor, Some(Region { x: 20, ..b })), "The watched window moved.");
        assert_eq!(pause_reason(anchor, None), "The watched window closed.");
        assert_eq!(pause_reason(None, None), "");
    }

    #[test]
    fn recognition_size() {
        assert_eq!(ocr_size(300, 40, 2600), (900, 120));
        assert_eq!(ocr_size(300, 200, 2600), (300, 200));
        assert_eq!(ocr_size(5200, 400, 2600), (2600, 200));
        assert_eq!(ocr_size(1000, 10, 2600), (2600, 26));
    }
}
