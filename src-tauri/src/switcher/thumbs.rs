//! Window previews for the switcher and taskbar preview. Windows: live DWM thumbnails placed over
//! the WebView's empty preview slots. X11: XComposite snapshots sent to the WebView as images and
//! refreshed while the slot stays on screen. Elsewhere: none (cards show the app icon).

use std::collections::HashMap;
use std::io::Cursor;
#[cfg(target_os = "linux")]
use std::time::{Duration, Instant};

use notifi_platform::{Image, WindowId};
use serde::{Deserialize, Serialize};

/// A preview slot reported by the WebView, in CSS pixels from the window's top-left.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Slot {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// Drawn by the compositor over the slot; the WebView leaves it empty.
    Live,
    /// Sent as images that the WebView draws.
    Snapshot,
    None,
}

pub fn kind(backend: notifi_platform::Backend) -> Kind {
    match backend {
        notifi_platform::Backend::Windows if cfg!(windows) => Kind::Live,
        notifi_platform::Backend::X11 if cfg!(target_os = "linux") => Kind::Snapshot,
        _ => Kind::None,
    }
}

pub fn png_data_url(img: &Image) -> Option<String> {
    let buf = image::RgbaImage::from_raw(img.width, img.height, img.rgba.clone())?;
    let mut out = Cursor::new(Vec::new());
    buf.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(format!("data:image/png;base64,{}", base64(out.get_ref())))
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// The window's icon as a PNG data URL.
pub fn icon_url(id: u64) -> Option<String> {
    #[cfg(windows)]
    let img = notifi_platform::win_thumbnail::icon(WindowId(id), 64);
    #[cfg(target_os = "linux")]
    let img = notifi_platform::x11_switcher::icon(WindowId(id), 64);
    #[cfg(not(any(windows, target_os = "linux")))]
    let img: Option<Image> = None;
    png_data_url(&img?)
}

/// Previews registered into one of our windows.
#[derive(Default)]
pub struct Thumbs {
    #[cfg(windows)]
    live: HashMap<u64, notifi_platform::win_thumbnail::Thumbnail>,
    #[cfg(target_os = "linux")]
    taken: HashMap<u64, Instant>,
    #[cfg(not(any(windows, target_os = "linux")))]
    _none: HashMap<u64, ()>,
}

impl Thumbs {
    pub fn clear(&mut self) {
        #[cfg(windows)]
        self.live.clear();
        #[cfg(target_os = "linux")]
        self.taken.clear();
    }

    /// Places live thumbnails over `slots` in window `host` (scale = physical / CSS pixels) and
    /// drops the rest. Returns the ids whose snapshots should be refreshed (X11).
    pub fn update(&mut self, host: u64, scale: f64, slots: &[Slot]) -> Vec<u64> {
        #[cfg(windows)]
        {
            use notifi_platform::win_thumbnail::Thumbnail;
            self.live.retain(|id, _| slots.iter().any(|s| s.id == *id));
            for s in slots {
                if !self.live.contains_key(&s.id) {
                    match Thumbnail::register(host, WindowId(s.id)) {
                        Ok(t) => {
                            self.live.insert(s.id, t);
                        }
                        Err(_) => continue,
                    }
                }
                let t = &self.live[&s.id];
                let cell = (s.x * scale, s.y * scale, s.w * scale, s.h * scale);
                let (x, y, w, h) = super::model::fit_rect(cell, t.source_size().unwrap_or((s.w as u32, s.h as u32)));
                let rect = notifi_platform::Rect { x: x.round() as i32, y: y.round() as i32, width: w.round() as u32, height: h.round() as u32 };
                let _ = t.place(rect, 255);
            }
            let _ = host;
            Vec::new()
        }
        #[cfg(target_os = "linux")]
        {
            let _ = (host, scale);
            self.taken.retain(|id, _| slots.iter().any(|s| s.id == *id));
            let now = Instant::now();
            let due: Vec<u64> = slots
                .iter()
                .filter(|s| self.taken.get(&s.id).is_none_or(|t| now.duration_since(*t) > Duration::from_millis(900)))
                .map(|s| s.id)
                .collect();
            for id in &due {
                self.taken.insert(*id, now);
            }
            due
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            let _ = (host, scale, slots);
            Vec::new()
        }
    }
}

/// A snapshot of the window as a data URL, sized for a slot (X11 only).
pub fn snapshot_url(id: u64, max_w: u32, max_h: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let img = notifi_platform::x11_switcher::snapshot(WindowId(id), max_w, max_h).ok()?;
        png_data_url(&img)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (id, max_w, max_h);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn png_url() {
        let img = Image { width: 1, height: 1, rgba: vec![1, 2, 3, 255] };
        assert!(png_data_url(&img).unwrap().starts_with("data:image/png;base64,iVBOR"));
    }
}
