//! Where the camera window goes: back where it was, but always on a screen that exists now.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Distance from the edge of the screen for the window's first position.
pub const MARGIN: i32 = 24;

fn overlap(a: Rect, b: Rect) -> i64 {
    let w = (i64::from(a.x) + i64::from(a.w)).min(i64::from(b.x) + i64::from(b.w)) - i64::from(a.x).max(i64::from(b.x));
    let h = (i64::from(a.y) + i64::from(a.h)).min(i64::from(b.y) + i64::from(b.h)) - i64::from(a.y).max(i64::from(b.y));
    if w > 0 && h > 0 {
        w * h
    } else {
        0
    }
}

fn centre_gap(a: Rect, b: Rect) -> i64 {
    let (ax, ay) = (i64::from(a.x) + i64::from(a.w) / 2, i64::from(a.y) + i64::from(a.h) / 2);
    let (bx, by) = (i64::from(b.x) + i64::from(b.w) / 2, i64::from(b.y) + i64::from(b.h) / 2);
    (ax - bx).pow(2) + (ay - by).pow(2)
}

/// The saved window moved onto the screens that are connected now. Left alone when at least nine
/// tenths of it is visible; otherwise shrunk if needed and put inside the screen it overlaps most
/// (or is nearest to). `None` when no screen is known.
pub fn clamp_onto(window: Rect, monitors: &[Rect]) -> Option<Rect> {
    if monitors.is_empty() || window.w <= 0 || window.h <= 0 {
        return None;
    }
    let visible: i64 = monitors.iter().map(|m| overlap(window, *m)).sum();
    if visible * 10 >= i64::from(window.w) * i64::from(window.h) * 9 {
        return Some(window);
    }
    let best = monitors.iter().copied().max_by_key(|m| (overlap(window, *m), -centre_gap(window, *m)))?;
    let (w, h) = (window.w.min(best.w), window.h.min(best.h));
    Some(Rect { x: window.x.clamp(best.x, best.x + best.w - w), y: window.y.clamp(best.y, best.y + best.h - h), w, h })
}

/// First position: the bottom-right corner of `screen`, `MARGIN` in from both edges.
pub fn corner(size: (i32, i32), screen: Rect, margin: i32) -> Rect {
    let (w, h) = (size.0.min(screen.w), size.1.min(screen.h));
    Rect { x: screen.x + screen.w - w - margin.min(screen.w - w), y: screen.y + screen.h - h - margin.min(screen.h - h), w, h }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: Rect = Rect { x: 0, y: 0, w: 1920, h: 1080 };
    const RIGHT: Rect = Rect { x: 1920, y: 0, w: 1920, h: 1080 };

    fn at(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn a_visible_window_stays_put() {
        assert_eq!(clamp_onto(at(100, 100, 320, 240), &[LEFT]), Some(at(100, 100, 320, 240)));
        // Straddling two screens is fine while it is all on screen.
        assert_eq!(clamp_onto(at(1800, 100, 320, 240), &[LEFT, RIGHT]), Some(at(1800, 100, 320, 240)));
    }

    #[test]
    fn a_window_on_a_missing_screen_comes_back() {
        // Saved on a second screen that is no longer plugged in.
        assert_eq!(clamp_onto(at(2500, 300, 320, 240), &[LEFT]), Some(at(1600, 300, 320, 240)));
        assert_eq!(clamp_onto(at(-900, -500, 320, 240), &[LEFT]), Some(at(0, 0, 320, 240)));
        // Half off the bottom edge is pulled in.
        assert_eq!(clamp_onto(at(100, 1000, 320, 240), &[LEFT]), Some(at(100, 840, 320, 240)));
    }

    #[test]
    fn the_nearest_connected_screen_is_used() {
        assert_eq!(clamp_onto(at(5000, 100, 320, 240), &[LEFT, RIGHT]), Some(at(3520, 100, 320, 240)));
        assert_eq!(clamp_onto(at(-3000, 100, 320, 240), &[LEFT, RIGHT]), Some(at(0, 100, 320, 240)));
    }

    #[test]
    fn a_window_bigger_than_the_screen_is_shrunk() {
        assert_eq!(clamp_onto(at(10, 10, 4000, 3000), &[LEFT]), Some(at(0, 0, 1920, 1080)));
    }

    #[test]
    fn no_screens_means_no_answer() {
        assert_eq!(clamp_onto(at(0, 0, 320, 240), &[]), None);
        assert_eq!(clamp_onto(at(0, 0, 0, 240), &[LEFT]), None);
    }

    #[test]
    fn the_first_position_is_the_bottom_right_corner() {
        assert_eq!(corner((320, 240), LEFT, 24), at(1576, 816, 320, 240));
        assert_eq!(corner((320, 240), RIGHT, 24), at(3496, 816, 320, 240));
        assert_eq!(corner((4000, 4000), LEFT, 24), at(0, 0, 1920, 1080));
    }
}
