//! Modifier-drag: moving a window from anywhere inside it, or resizing it from the edge or corner
//! nearest to where the drag started.

use notifi_platform::Rect;

/// Which sides move while resizing: -1 the left or top, 1 the right or bottom, 0 neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub h: i8,
    pub v: i8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Move,
    Resize(Anchor),
}

fn third(pos: i64, start: i64, len: i64) -> i8 {
    let rel = pos - start;
    if rel * 3 < len {
        -1
    } else if rel * 3 >= len * 2 {
        1
    } else {
        0
    }
}

/// The edge or corner of `r` nearest to `start`. The middle of the window counts as its nearest corner.
pub fn anchor_at(start: (i32, i32), r: Rect) -> Anchor {
    let (x, y) = (i64::from(start.0), i64::from(start.1));
    let (w, h) = (i64::from(r.width).max(1), i64::from(r.height).max(1));
    let mut a = Anchor { h: third(x, i64::from(r.x), w), v: third(y, i64::from(r.y), h) };
    if a.h == 0 && a.v == 0 {
        a.h = if x - i64::from(r.x) < w / 2 { -1 } else { 1 };
        a.v = if y - i64::from(r.y) < h / 2 { -1 } else { 1 };
    }
    a
}

fn resize_axis(pos: i64, size: i64, delta: i64, side: i8, min: i64) -> (i64, i64) {
    match side {
        -1 => {
            let size_new = (size - delta).max(min);
            (pos + size - size_new, size_new)
        }
        1 => (pos, (size + delta).max(min)),
        _ => (pos, size),
    }
}

/// The window's rectangle for a pointer that has moved from `start_ptr` to `ptr`.
pub fn drag_rect(mode: Mode, start: Rect, start_ptr: (i32, i32), ptr: (i32, i32), min: (u32, u32)) -> Rect {
    let (dx, dy) = (i64::from(ptr.0 - start_ptr.0), i64::from(ptr.1 - start_ptr.1));
    match mode {
        Mode::Move => Rect { x: (i64::from(start.x) + dx) as i32, y: (i64::from(start.y) + dy) as i32, ..start },
        Mode::Resize(a) => {
            let (x, w) = resize_axis(i64::from(start.x), i64::from(start.width), dx, a.h, i64::from(min.0));
            let (y, h) = resize_axis(i64::from(start.y), i64::from(start.height), dy, a.v, i64::from(min.1));
            Rect { x: x as i32, y: y as i32, width: w as u32, height: h as u32 }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: Rect = Rect { x: 100, y: 100, width: 600, height: 300 };

    #[test]
    fn anchors_by_where_the_drag_starts() {
        assert_eq!(anchor_at((110, 110), W), Anchor { h: -1, v: -1 });
        assert_eq!(anchor_at((690, 390), W), Anchor { h: 1, v: 1 });
        assert_eq!(anchor_at((400, 105), W), Anchor { h: 0, v: -1 });
        assert_eq!(anchor_at((695, 250), W), Anchor { h: 1, v: 0 });
        assert_eq!(anchor_at((105, 250), W), Anchor { h: -1, v: 0 });
        assert_eq!(anchor_at((400, 395), W), Anchor { h: 0, v: 1 });
    }

    #[test]
    fn the_middle_picks_the_nearest_corner() {
        assert_eq!(anchor_at((350, 220), W), Anchor { h: -1, v: -1 });
        assert_eq!(anchor_at((450, 280), W), Anchor { h: 1, v: 1 });
    }

    #[test]
    fn moving_keeps_the_size() {
        let r = drag_rect(Mode::Move, W, (300, 200), (350, 150), (100, 100));
        assert_eq!(r, Rect { x: 150, y: 50, width: 600, height: 300 });
    }

    #[test]
    fn resizing_from_the_left_keeps_the_right_edge_fixed() {
        let r = drag_rect(Mode::Resize(Anchor { h: -1, v: 0 }), W, (110, 250), (60, 250), (100, 100));
        assert_eq!((r.x, r.width), (50, 650));
        assert_eq!(r.x + r.width as i32, W.x + W.width as i32);
        assert_eq!((r.y, r.height), (100, 300));
    }

    #[test]
    fn resizing_from_a_corner_moves_two_sides() {
        let r = drag_rect(Mode::Resize(Anchor { h: 1, v: 1 }), W, (690, 390), (790, 340), (100, 100));
        assert_eq!(r, Rect { x: 100, y: 100, width: 700, height: 250 });
    }

    #[test]
    fn a_window_cannot_shrink_below_the_minimum() {
        let r = drag_rect(Mode::Resize(Anchor { h: -1, v: -1 }), W, (110, 110), (900, 900), (200, 150));
        assert_eq!((r.width, r.height), (200, 150));
        assert_eq!((r.x + 200, r.y + 150), (700, 400), "the opposite corner stays put");
    }
}
