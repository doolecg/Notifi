//! Capture is only built for Windows so far; these stand-ins keep the rest of the module compiling.

use image::RgbaImage;

use super::logic::{civil_from_unix, DateParts, Frame, Rect};

pub fn available() -> bool {
    false
}

pub fn virtual_screen() -> Rect {
    Rect::new(0, 0, 0, 0)
}

pub fn grab(_r: &Rect) -> Option<Frame> {
    None
}

pub fn top_windows(_skip: &[isize]) -> Vec<(isize, Rect)> {
    Vec::new()
}

pub fn grab_window(_hwnd: isize, _frame: &Rect) -> Option<RgbaImage> {
    None
}

pub fn set_excluded(_hwnd: isize, _on: bool) -> bool {
    false
}

pub fn cursor() -> (i32, i32) {
    (0, 0)
}

pub fn set_cursor(_x: i32, _y: i32) {}

pub fn wheel(_notches: i32) {}

pub fn key_down(_vk: i32) -> bool {
    false
}

pub fn local_time() -> DateParts {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    civil_from_unix(secs)
}

pub fn confirm_open(_url: &str) -> bool {
    false
}

pub fn open_url(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

pub fn ocr_lines(_img: &RgbaImage, _languages: &[String]) -> Result<Vec<String>, String> {
    Err("Text recognition is only built for Windows so far.".into())
}

pub fn watermark(_img: &mut RgbaImage, _text: &str) {}

pub fn ocr_boxes(_img: &RgbaImage, _languages: &[String]) -> Result<Vec<[f64; 4]>, String> {
    Err("Text recognition is only built for Windows so far.".into())
}

pub fn trash(path: &std::path::Path) -> Result<(), String> {
    std::fs::remove_file(path).map_err(|e| e.to_string())
}
