use std::sync::mpsc::Receiver;

use serde::{Deserialize, Serialize};

/// Events pushed by a backend thread; the app drains it on its own thread or task.
pub type EventStream<T> = Receiver<T>;

/// Holds an OS resource (inhibitor, grab, thumbnail) and releases it on drop.
#[must_use = "the resource is released as soon as the guard is dropped"]
pub struct Guard {
    release: Option<Box<dyn FnOnce() + Send>>,
}

impl Guard {
    pub fn new(release: impl FnOnce() + Send + 'static) -> Self {
        Self { release: Some(Box::new(release)) }
    }

    pub fn release(mut self) {
        if let Some(f) = self.release.take() {
            f();
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(f) = self.release.take() {
            f();
        }
    }
}

impl std::fmt::Debug for Guard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guard").field("held", &self.release.is_some()).finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Straight RGBA8 pixels, row-major, no padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// On battery the system draws what the battery gives; on AC it is the adapter output minus what charges the battery.
pub(crate) fn system_watts(on_ac: bool, charging: bool, battery_watts: Option<f32>, adapter_watts: Option<f32>) -> Option<f32> {
    if !on_ac {
        return battery_watts;
    }
    let charge = if charging { battery_watts? } else { 0.0 };
    adapter_watts.map(|a| (a - charge).max(0.0))
}

/// `outer` covers all of `inner`.
pub fn covers(outer: Rect, inner: Rect) -> bool {
    inner.width > 0
        && inner.height > 0
        && outer.x <= inner.x
        && outer.y <= inner.y
        && outer.x as i64 + outer.width as i64 >= inner.x as i64 + inner.width as i64
        && outer.y as i64 + outer.height as i64 >= inner.y as i64 + inner.height as i64
}

/// Area of the overlap of two rectangles.
pub fn overlap(a: Rect, b: Rect) -> u64 {
    let w = (a.x as i64 + a.width as i64).min(b.x as i64 + b.width as i64) - (a.x as i64).max(b.x as i64);
    let h = (a.y as i64 + a.height as i64).min(b.y as i64 + b.height as i64) - (a.y as i64).max(b.y as i64);
    if w > 0 && h > 0 { (w * h) as u64 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, width: w, height: h }
    }

    #[test]
    fn covers_and_overlap() {
        let mon = r(1920, 0, 2560, 1440);
        assert!(covers(r(1920, 0, 2560, 1440), mon));
        assert!(covers(r(1910, -10, 2580, 1460), mon));
        assert!(!covers(r(1920, 0, 2560, 1400), mon));
        assert!(!covers(r(0, 0, 1920, 1080), mon));
        assert!(!covers(r(0, 0, 10, 10), r(0, 0, 0, 0)));
        assert_eq!(overlap(r(0, 0, 100, 100), r(50, 50, 100, 100)), 2500);
        assert_eq!(overlap(r(0, 0, 10, 10), r(10, 0, 10, 10)), 0);
    }
}
