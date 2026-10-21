//! Island geometry in logical px. The window is a fixed transparent box at the top centre;
//! the island hangs from its top edge and the orbit buttons sit around the open island.

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
pub const MAX_W: f64 = 900.0;
pub const MIN_H: f64 = 200.0;
pub const MAX_H: f64 = 600.0;

/// Size of the open island for a size setting.
pub fn open_size(size: &str, custom_w: f64, custom_h: f64) -> (f64, f64) {
    match size {
        "spacious" => (1000.0, 360.0),
        "custom" => (
            if custom_w.is_finite() { custom_w.clamp(MIN_W, MAX_W) } else { MIN_W },
            if custom_h.is_finite() { custom_h.clamp(MIN_H, MAX_H) } else { MIN_H },
        ),
        _ => (860.0, 280.0),
    }
}

/// Window size that holds the open island; its buttons are a tab strip inside it.
pub fn window_size(open: (f64, f64)) -> (f64, f64) {
    (open.0 + 2.0 * EDGE, open.1 + EDGE)
}

/// Window size that holds a docked open island; its buttons are a tab strip inside it.
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
    /// Docked bottom-left: buttons are placed from the island's left edge and top, and `left` is empty.
    pub dock: bool,
    pub window: (f64, f64),
    pub open: (f64, f64),
    /// Bottom corner radius of the open island.
    pub radius: f64,
    pub max_left: usize,
    pub max_right: usize,
    pub max_bottom: usize,
    pub left: Vec<(f64, f64)>,
    pub right: Vec<(f64, f64)>,
    pub bottom: Vec<(f64, f64)>,
}

/// Full layout for a size and the number of buttons on each side (extra buttons are dropped).
pub fn layout(size: &str, custom_w: f64, custom_h: f64, _counts: (usize, usize, usize)) -> Layout {
    let open = open_size(size, custom_w, custom_h);
    Layout {
        dock: false,
        window: window_size(open),
        open,
        radius: OPEN_RADIUS,
        max_left: max_side(open.1),
        max_right: max_side(open.1),
        max_bottom: max_bottom(open.0),
        left: vec![],
        right: vec![],
        bottom: vec![],
    }
}

/// The docked menu is roomier than the floating one: it rises from the taskbar and its modules need the height.
pub const DOCK_TALL: f64 = 600.0;

pub fn dock_open_size(size: &str, custom_w: f64, custom_h: f64) -> (f64, f64) {
    match size {
        "spacious" => (1000.0, 420.0),
        "custom" => {
            let (w, h) = open_size(size, custom_w, custom_h);
            // The stretched menu asks for the tallest custom height.
            if h >= MAX_H { (w, DOCK_TALL) } else { (w, h) }
        }
        _ => (880.0, 300.0),
    }
}

/// Layout for the bottom-left dock: the buttons are a tab strip along the top of the menu, so no slots are reserved.
pub fn layout_dock(size: &str, custom_w: f64, custom_h: f64, _counts: (usize, usize, usize)) -> Layout {
    let open = dock_open_size(size, custom_w, custom_h);
    Layout {
        dock: true,
        window: dock_window_size(open),
        open,
        radius: OPEN_RADIUS,
        max_left: 0,
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

    #[test]
    fn sizes_and_custom_clamping() {
        assert_eq!(open_size("compact", 0.0, 0.0), (860.0, 280.0));
        assert_eq!(open_size("spacious", 0.0, 0.0), (1000.0, 360.0));
        assert_eq!(open_size("custom", 2000.0, 10.0), (MAX_W, MIN_H));
        assert_eq!(open_size("custom", f64::NAN, 300.0), (MIN_W, 300.0));
        assert_eq!(open_size("whatever", 1.0, 1.0), open_size("compact", 0.0, 0.0));
    }

    #[test]
    fn window_holds_the_island_and_its_edge() {
        let (w, h) = window_size((860.0, 280.0));
        assert_eq!(w, 860.0 + 2.0 * EDGE);
        assert_eq!(h, 280.0 + EDGE);
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
    fn buttons_are_a_tab_strip_in_both_modes() {
        let l = layout("compact", 0.0, 0.0, (2, 2, 1));
        assert!(!l.dock && l.left.is_empty() && l.right.is_empty() && l.bottom.is_empty());
        let d = layout_dock("compact", 0.0, 0.0, (2, 2, 1));
        assert!(d.dock && d.left.is_empty() && d.right.is_empty() && d.bottom.is_empty());
        assert_eq!(d.window, (880.0 + EDGE, 300.0 + EDGE + DOCK_GAP));
        let tall = layout_dock("custom", 880.0, MAX_H, (2, 2, 1));
        assert_eq!(tall.open, (880.0, DOCK_TALL));
    }

    #[test]
    fn shape_and_bounds() {
        let l = layout("compact", 0.0, 0.0, (2, 2, 1));
        let pieces = shape(l.window.0, l.open, OPEN_RADIUS, &[]);
        assert_eq!(pieces.len(), 1);
        assert_eq!(bounds(l.window.0, &pieces), l.window);
        let pill = shape(l.window.0, (180.0, 32.0), 16.0, &[]);
        assert_eq!(bounds(l.window.0, &pill), (180.0 + 2.0 * EDGE, 32.0 + EDGE));
    }}
