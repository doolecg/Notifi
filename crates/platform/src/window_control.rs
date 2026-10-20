//! Types for moving, sizing and organising other apps' windows.

use serde::Serialize;

use crate::common::Rect;

/// A display and the part of it apps may use, in physical pixels.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkArea {
    /// The [`crate::DisplayInfo::id`] of the display.
    pub display_id: String,
    pub bounds: Rect,
    pub work: Rect,
    pub scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DesktopState {
    pub count: u32,
    /// 1-based.
    pub current: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct PointerState {
    pub x: i32,
    pub y: i32,
    pub left: bool,
    pub right: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct WindowState {
    pub maximized: bool,
    pub minimized: bool,
    pub fullscreen: bool,
    pub resizable: bool,
}

/// The point lies inside `r`.
pub fn contains(r: Rect, x: i32, y: i32) -> bool {
    let (x, y) = (i64::from(x), i64::from(y));
    x >= i64::from(r.x) && y >= i64::from(r.y) && x < i64::from(r.x) + i64::from(r.width) && y < i64::from(r.y) + i64::from(r.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_in_rect() {
        let r = Rect { x: -100, y: 0, width: 100, height: 50 };
        assert!(contains(r, -100, 0));
        assert!(contains(r, -1, 49));
        assert!(!contains(r, 0, 10));
        assert!(!contains(r, -50, 50));
    }
}
