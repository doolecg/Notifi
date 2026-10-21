//! Island geometry in logical px. The window is a fixed transparent box at the top centre;
//! the island hangs from its top edge (or rises from the taskbar when docked) and its buttons are a tab strip inside it.

use serde::Serialize;

pub const ORBIT_D: f64 = 40.0;
pub const ORBIT_GAP: f64 = 10.0;
/// Space above the first button of a side column.
pub const TOP_PAD: f64 = 8.0;
/// Room around everything for shadows.
pub const EDGE: f64 = 10.0;
pub const OPEN_RADIUS: f64 = 24.0;
/// Docked bottom-left: the bar fills the taskbar, so no gap under it or from the screen's left edge.
pub const DOCK_GAP: f64 = 0.0;
/// Docked and open: space between the taskbar and the island, like the Start menu.
pub const FLY_GAP: f64 = 12.0;
pub const DOCK_LEFT: f64 = 0.0;

pub const MIN_W: f64 = 380.0;
pub const MAX_W: f64 = 2400.0;
pub const MIN_H: f64 = 200.0;
pub const MAX_H: f64 = 1200.0;

/// Height of the stretched menu (the expand button), for every module except the chat.
pub const TALL_H: f64 = 640.0;
/// The AI chat's size when it opens.
pub const CHAT: (f64, f64) = (1340.0, 1100.0);
/// What is kept free of the screen's edges so the menu never runs off it: sides, and the top and the taskbar.
const SCREEN_SIDE: f64 = 40.0;
const SCREEN_VERTICAL: f64 = 100.0;

/// Size of the open island for a size setting.
pub fn open_size(size: &str, custom_w: f64, custom_h: f64) -> (f64, f64) {
    match size {
        "spacious" => (1700.0, 270.0),
        "custom" => (
            if custom_w.is_finite() { custom_w.clamp(MIN_W, MAX_W) } else { MIN_W },
            if custom_h.is_finite() { custom_h.clamp(MIN_H, MAX_H) } else { MIN_H },
        ),
        _ => (1200.0, 270.0),
    }
}

/// Size of the docked menu for a size setting; it rises from the taskbar and its modules need the height.
pub fn dock_open_size(size: &str, custom_w: f64, custom_h: f64) -> (f64, f64) {
    match size {
        "spacious" => (1700.0, 330.0),
        "custom" => open_size(size, custom_w, custom_h),
        _ => (1200.0, 300.0),
    }
}

/// `size` kept inside a screen of this size.
pub fn fit(size: (f64, f64), screen: (f64, f64)) -> (f64, f64) {
    (size.0.min((screen.0 - SCREEN_SIDE).max(MIN_W)), size.1.min((screen.1 - SCREEN_VERTICAL).max(MIN_H)))
}

/// Window size that holds the open island and its edge.
pub fn window_size(open: (f64, f64)) -> (f64, f64) {
    (open.0 + 2.0 * EDGE, open.1 + EDGE)
}

/// Window size that holds a docked open island.
pub fn dock_window_size(open: (f64, f64)) -> (f64, f64) {
    (open.0 + EDGE, open.1 + EDGE + DOCK_GAP)
}

/// How many buttons fit in a side column next to an island `h` tall.
pub fn max_side(h: f64) -> usize {
    ((h - TOP_PAD + ORBIT_GAP) / (ORBIT_D + ORBIT_GAP)).floor().max(0.0) as usize
}

/// How many buttons fit in the row under an island `w` wide.
pub fn max_bottom(w: f64) -> usize {
    ((w + ORBIT_GAP) / (ORBIT_D + ORBIT_GAP)).floor().max(0.0) as usize
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub dock: bool,
    pub window: (f64, f64),
    /// The size in use now: `base`, `tall` or `chat`.
    pub open: (f64, f64),
    /// The three sizes, whichever is in use, so the page can start animating to one before the window has grown.
    pub base: (f64, f64),
    pub tall: (f64, f64),
    pub chat: (f64, f64),
    /// Bottom corner radius of the open island.
    pub radius: f64,
    pub max_left: usize,
    pub max_right: usize,
    pub max_bottom: usize,
    pub left: Vec<(f64, f64)>,
    pub right: Vec<(f64, f64)>,
    pub bottom: Vec<(f64, f64)>,
}

/// Full layout for a size, kept inside `screen`. `tall` stretches the menu; `chat` gives it the chat's size.
pub fn layout(size: &str, custom_w: f64, custom_h: f64, tall: bool, chat: bool, screen: (f64, f64)) -> Layout {
    build(false, fit(open_size(size, custom_w, custom_h), screen), tall, chat, screen)
}

/// Layout for the bottom-left dock.
pub fn layout_dock(size: &str, custom_w: f64, custom_h: f64, tall: bool, chat: bool, screen: (f64, f64)) -> Layout {
    build(true, fit(dock_open_size(size, custom_w, custom_h), screen), tall, chat, screen)
}

fn build(dock: bool, base: (f64, f64), tall: bool, chat: bool, screen: (f64, f64)) -> Layout {
    let stretched = fit((base.0, TALL_H.max(base.1)), screen);
    let chat_size = fit(CHAT, screen);
    let open = if chat { chat_size } else if tall { stretched } else { base };
    // Only as big as the menu: anything wider is invisible but still takes clicks meant for the desktop.
    // Switching sizes keeps the old size for a moment instead (see `hold` in island/mod.rs).
    Layout {
        dock,
        window: if dock { dock_window_size(open) } else { window_size(open) },
        open,
        base,
        tall: stretched,
        chat: chat_size,
        radius: OPEN_RADIUS,
        max_left: if dock { 0 } else { max_side(open.1) },
        max_right: max_side(open.1),
        max_bottom: max_bottom(open.0),
        left: vec![],
        right: vec![],
        bottom: vec![],
    }
}

/// One piece of the clickable shape, in logical px relative to the window's top-left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Piece {
    /// Rounded at the bottom only; the top is flush with the screen edge.
    Hanging { x: f64, w: f64, h: f64, r: f64 },
    Circle { cx: f64, cy: f64, d: f64 },
}

/// The clickable shape: the island (`w` x `h`, bottom radius `r`) plus, while open, the orbit buttons.
pub fn shape(window_w: f64, island: (f64, f64), r: f64, orbits: &[(f64, f64)]) -> Vec<Piece> {
    let cx = window_w / 2.0;
    let mut out = vec![Piece::Hanging { x: cx - island.0 / 2.0, w: island.0, h: island.1, r }];
    out.extend(orbits.iter().map(|&(x, y)| Piece::Circle { cx: cx + x, cy: y, d: ORBIT_D }));
    out
}

/// The smallest box centred on the window's middle that holds every piece (for platforms without window regions).
pub fn bounds(window_w: f64, pieces: &[Piece]) -> (f64, f64) {
    let cx = window_w / 2.0;
    let mut half = 0.0f64;
    let mut bottom = 0.0f64;
    for p in pieces {
        let (l, rgt, b) = match *p {
            Piece::Hanging { x, w, h, .. } => (x, x + w, h),
            Piece::Circle { cx, cy, d } => (cx - d / 2.0, cx + d / 2.0, cy + d / 2.0),
        };
        half = half.max(cx - l).max(rgt - cx);
        bottom = bottom.max(b);
    }
    ((2.0 * (half + EDGE)).min(window_w), bottom + EDGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIG: (f64, f64) = (2560.0, 1440.0);

    #[test]
    fn sizes_and_custom_clamping() {
        assert_eq!(open_size("compact", 0.0, 0.0), (1200.0, 270.0));
        assert_eq!(open_size("spacious", 0.0, 0.0), (1700.0, 270.0));
        assert_eq!(open_size("custom", 5000.0, 10.0), (MAX_W, MIN_H));
        assert_eq!(open_size("custom", f64::NAN, 300.0), (MIN_W, 300.0));
        assert_eq!(open_size("whatever", 1.0, 1.0), open_size("compact", 0.0, 0.0));
    }

    #[test]
    fn window_holds_the_island_and_its_edge() {
        let (w, h) = window_size((1700.0, 270.0));
        assert_eq!(w, 1700.0 + 2.0 * EDGE);
        assert_eq!(h, 270.0 + EDGE);
    }

    #[test]
    fn capacity() {
        // 236 tall: 8 + 4 buttons of 40 with 3 gaps of 10 = 198, a fifth needs 248.
        assert_eq!(max_side(236.0), 4);
        assert_eq!(max_side(248.0), 5);
        assert_eq!(max_bottom(440.0), 9);
        assert_eq!(max_side(0.0), 0);
    }

    #[test]
    fn the_three_sizes_and_which_is_in_use() {
        let l = layout("spacious", 0.0, 0.0, false, false, BIG);
        assert_eq!((l.base, l.tall, l.chat), ((1700.0, 270.0), (1700.0, TALL_H), CHAT));
        assert_eq!(l.open, l.base);
        // Stretching keeps the width: it used to fall back to a narrower custom size and clip the menu.
        assert_eq!(layout("spacious", 0.0, 0.0, true, false, BIG).open, (1700.0, TALL_H));
        assert_eq!(layout("spacious", 0.0, 0.0, true, true, BIG).open, CHAT);
        let d = layout_dock("compact", 0.0, 0.0, false, false, BIG);
        assert!(d.dock && d.left.is_empty() && d.right.is_empty() && d.bottom.is_empty());
        // The window is only as big as the menu in use, so it never covers the desktop with an invisible margin.
        assert_eq!(l.window, window_size(l.base));
        assert_eq!(layout("spacious", 0.0, 0.0, false, true, BIG).window, window_size(CHAT));
        assert_eq!(d.window, dock_window_size(d.base));
    }

    #[test]
    fn sizes_are_kept_inside_the_screen() {
        let small = (1366.0, 768.0);
        let l = layout("spacious", 0.0, 0.0, false, true, small);
        assert_eq!(l.base.0, 1366.0 - SCREEN_SIDE);
        assert_eq!(l.chat, (1340.0f64.min(1366.0 - SCREEN_SIDE), 768.0 - SCREEN_VERTICAL));
        assert!(l.window.0 <= small.0 && l.window.1 <= small.1);
    }

    #[test]
    fn shape_and_bounds() {
        let l = layout("compact", 0.0, 0.0, false, false, BIG);
        let pieces = shape(l.window.0, l.open, OPEN_RADIUS, &[]);
        assert_eq!(pieces.len(), 1);
        // The shape is the island and its edge, even though the window is wider.
        assert_eq!(bounds(l.window.0, &pieces), window_size(l.open));
        let pill = shape(l.window.0, (180.0, 32.0), 16.0, &[]);
        assert_eq!(bounds(l.window.0, &pill), (180.0 + 2.0 * EDGE, 32.0 + EDGE));
    }
}
