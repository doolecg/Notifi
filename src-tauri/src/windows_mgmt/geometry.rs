//! Where windows go: grid cells, gaps, work areas of several displays and moving between them.
//! Pure geometry in physical pixels, no OS calls.

use notifi_platform::{Rect, WorkArea};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// `col_span` by `row_span` cells of a `cols` by `rows` grid, counted from the top left.
    Cell { cols: u8, rows: u8, col: u8, row: u8, col_span: u8, row_span: u8 },
    Maximize,
    MaximizeMargin,
    /// Keeps the window's size.
    Center,
    /// Half the width, full height, centred.
    CenterHalf,
}

const fn cell(cols: u8, rows: u8, col: u8, row: u8, col_span: u8, row_span: u8) -> Placement {
    Placement::Cell { cols, rows, col, row, col_span, row_span }
}

pub const LEFT_HALF: Placement = cell(2, 1, 0, 0, 1, 1);
pub const RIGHT_HALF: Placement = cell(2, 1, 1, 0, 1, 1);
pub const TOP_HALF: Placement = cell(1, 2, 0, 0, 1, 1);
pub const BOTTOM_HALF: Placement = cell(1, 2, 0, 1, 1, 1);
pub const TOP_LEFT: Placement = cell(2, 2, 0, 0, 1, 1);
pub const TOP_RIGHT: Placement = cell(2, 2, 1, 0, 1, 1);
pub const BOTTOM_LEFT: Placement = cell(2, 2, 0, 1, 1, 1);
pub const BOTTOM_RIGHT: Placement = cell(2, 2, 1, 1, 1, 1);

/// Every name a placement can be written as in the settings lists, lower case without separators.
pub fn parse_placement(text: &str) -> Option<Placement> {
    let key: String = text.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let key = key.replace("centre", "center").replace("maximise", "maximize").replace("quarter", "");
    let numbered = |prefix: &str, n: u8| -> Option<u8> {
        key.strip_prefix(prefix)?.parse::<u8>().ok().filter(|v| (1..=n).contains(v))
    };
    if let Some(i) = numbered("grid", 6) {
        return Some(cell(3, 2, (i - 1) % 3, (i - 1) / 3, 1, 1));
    }
    if let Some(i) = numbered("column", 4) {
        return Some(cell(4, 1, i - 1, 0, 1, 1));
    }
    if let Some(i) = numbered("row", 4) {
        return Some(cell(1, 4, 0, i - 1, 1, 1));
    }
    Some(match key.as_str() {
        "lefthalf" | "left" => LEFT_HALF,
        "righthalf" | "right" => RIGHT_HALF,
        "tophalf" | "top" => TOP_HALF,
        "bottomhalf" | "bottom" => BOTTOM_HALF,
        "topleft" => TOP_LEFT,
        "topright" => TOP_RIGHT,
        "bottomleft" => BOTTOM_LEFT,
        "bottomright" => BOTTOM_RIGHT,
        "maximize" | "full" => Placement::Maximize,
        "maximizemargin" => Placement::MaximizeMargin,
        "center" => Placement::Center,
        "centerhalf" => Placement::CenterHalf,
        "leftthird" => cell(3, 1, 0, 0, 1, 1),
        "centerthird" | "middlethird" => cell(3, 1, 1, 0, 1, 1),
        "rightthird" => cell(3, 1, 2, 0, 1, 1),
        "lefttwothirds" => cell(3, 1, 0, 0, 2, 1),
        "righttwothirds" => cell(3, 1, 1, 0, 2, 1),
        "topthird" => cell(1, 3, 0, 0, 1, 1),
        "middlethirdrow" | "centerrow" => cell(1, 3, 0, 1, 1, 1),
        "bottomthird" => cell(1, 3, 0, 2, 1, 1),
        _ => return None,
    })
}

fn inset(rect: Rect, left: i64, top: i64, right: i64, bottom: i64) -> Rect {
    let w = (i64::from(rect.width) - left - right).max(1);
    let h = (i64::from(rect.height) - top - bottom).max(1);
    Rect { x: (i64::from(rect.x) + left) as i32, y: (i64::from(rect.y) + top) as i32, width: w as u32, height: h as u32 }
}

/// A grid cell of `work`; sides on the work area's border keep `gap`, sides between cells keep half each.
fn cell_rect(work: Rect, p: Placement, gap: u32) -> Rect {
    let Placement::Cell { cols, rows, col, row, col_span, row_span } = p else { return work };
    let (cols, rows) = (i64::from(cols.max(1)), i64::from(rows.max(1)));
    let (col, row) = (i64::from(col).min(cols - 1), i64::from(row).min(rows - 1));
    let (c_end, r_end) = ((col + i64::from(col_span)).min(cols), (row + i64::from(row_span)).min(rows));
    let (w, h) = (i64::from(work.width), i64::from(work.height));
    let x0 = i64::from(work.x) + w * col / cols;
    let x1 = i64::from(work.x) + w * c_end / cols;
    let y0 = i64::from(work.y) + h * row / rows;
    let y1 = i64::from(work.y) + h * r_end / rows;
    let g = i64::from(gap);
    let (half_lo, half_hi) = (g / 2, g - g / 2);
    inset(
        Rect { x: x0 as i32, y: y0 as i32, width: (x1 - x0) as u32, height: (y1 - y0) as u32 },
        if col == 0 { g } else { half_hi },
        if row == 0 { g } else { half_hi },
        if c_end == cols { g } else { half_lo },
        if r_end == rows { g } else { half_lo },
    )
}

/// The rectangle `p` gives a window currently at `current` on a display with work area `work`.
pub fn place(work: Rect, p: Placement, gap: u32, margin: u32, current: Rect) -> Rect {
    match p {
        Placement::Cell { .. } => cell_rect(work, p, gap),
        Placement::Maximize => inset(work, i64::from(gap), i64::from(gap), i64::from(gap), i64::from(gap)),
        Placement::MaximizeMargin => {
            let m = i64::from(margin.max(gap));
            inset(work, m, m, m, m)
        }
        Placement::Center => {
            let width = current.width.min(work.width);
            let height = current.height.min(work.height);
            Rect {
                x: work.x + ((work.width - width) / 2) as i32,
                y: work.y + ((work.height - height) / 2) as i32,
                width,
                height,
            }
        }
        Placement::CenterHalf => {
            let mut r = cell_rect(work, cell(4, 1, 1, 0, 2, 1), gap);
            r.y = work.y + gap as i32;
            r.height = work.height.saturating_sub(2 * gap).max(1);
            r
        }
    }
}

/// Two rectangles agree to within `tol` pixels on every side.
pub fn same_place(a: Rect, b: Rect, tol: i32) -> bool {
    (a.x - b.x).abs() <= tol
        && (a.y - b.y).abs() <= tol
        && (a.width as i32 - b.width as i32).abs() <= tol
        && (a.height as i32 - b.height as i32).abs() <= tol
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// The side a placement hugs, when it hugs exactly one (halves).
pub fn side_of(p: Placement) -> Option<Dir> {
    match p {
        p if p == LEFT_HALF => Some(Dir::Left),
        p if p == RIGHT_HALF => Some(Dir::Right),
        p if p == TOP_HALF => Some(Dir::Up),
        p if p == BOTTOM_HALF => Some(Dir::Down),
        _ => None,
    }
}

pub fn opposite(p: Placement) -> Placement {
    match side_of(p) {
        Some(Dir::Left) => RIGHT_HALF,
        Some(Dir::Right) => LEFT_HALF,
        Some(Dir::Up) => BOTTOM_HALF,
        Some(Dir::Down) => TOP_HALF,
        None => p,
    }
}

/// Half, third and two-thirds of a side, the order repeated presses walk through.
pub fn width_cycle(side: Dir) -> [Placement; 3] {
    match side {
        Dir::Right => [RIGHT_HALF, cell(3, 1, 2, 0, 1, 1), cell(3, 1, 1, 0, 2, 1)],
        _ => [LEFT_HALF, cell(3, 1, 0, 0, 1, 1), cell(3, 1, 0, 0, 2, 1)],
    }
}

/// The step after whichever of the cycle `current` already matches; the first when none does.
pub fn next_in_cycle(side: Dir, work: Rect, gap: u32, current: Rect, tol: i32) -> Placement {
    let cycle = width_cycle(side);
    let at = cycle.iter().position(|p| same_place(place(work, *p, gap, 0, current), current, tol));
    cycle[at.map_or(0, |i| (i + 1) % cycle.len())]
}

/// The display holding most of `r`, else the one whose centre is nearest.
pub fn area_for(areas: &[WorkArea], r: Rect) -> Option<&WorkArea> {
    let best = areas.iter().max_by_key(|a| notifi_platform::overlap(r, a.bounds))?;
    if notifi_platform::overlap(r, best.bounds) > 0 {
        return Some(best);
    }
    let (cx, cy) = (i64::from(r.x) + i64::from(r.width) / 2, i64::from(r.y) + i64::from(r.height) / 2);
    areas.iter().min_by_key(|a| {
        let (ax, ay) = (i64::from(a.bounds.x) + i64::from(a.bounds.width) / 2, i64::from(a.bounds.y) + i64::from(a.bounds.height) / 2);
        (ax - cx).pow(2) + (ay - cy).pow(2)
    })
}

pub fn area_at(areas: &[WorkArea], x: i32, y: i32) -> Option<&WorkArea> {
    areas.iter().find(|a| notifi_platform::contains(a.bounds, x, y))
}

/// The display next to `from` in direction `dir`: the nearest one past that side that shares part of its span.
pub fn neighbour<'a>(areas: &'a [WorkArea], from: &WorkArea, dir: Dir) -> Option<&'a WorkArea> {
    let f = from.bounds;
    let (f_l, f_r, f_t, f_b) = (i64::from(f.x), i64::from(f.x) + i64::from(f.width), i64::from(f.y), i64::from(f.y) + i64::from(f.height));
    areas
        .iter()
        .filter(|a| a.display_id != from.display_id)
        .filter_map(|a| {
            let b = a.bounds;
            let (l, r, t, bt) = (i64::from(b.x), i64::from(b.x) + i64::from(b.width), i64::from(b.y), i64::from(b.y) + i64::from(b.height));
            let (gap, shared) = match dir {
                Dir::Left => (f_l - r, (f_b.min(bt) - f_t.max(t)).max(0)),
                Dir::Right => (l - f_r, (f_b.min(bt) - f_t.max(t)).max(0)),
                Dir::Up => (f_t - bt, (f_r.min(r) - f_l.max(l)).max(0)),
                Dir::Down => (t - f_b, (f_r.min(r) - f_l.max(l)).max(0)),
            };
            (gap >= 0 && shared > 0).then_some((gap, -shared, a))
        })
        .min_by_key(|(gap, shared, _)| (*gap, *shared))
        .map(|(_, _, a)| a)
}

/// The window's rectangle after moving from one display to another: the same logical size (scaled by the
/// ratio of display scales) at the same relative position, never larger than the new work area.
pub fn move_between(r: Rect, from: &WorkArea, to: &WorkArea) -> Rect {
    let ratio = if from.scale > 0.0 { f64::from(to.scale) / f64::from(from.scale) } else { 1.0 };
    let width = ((f64::from(r.width) * ratio).round() as u32).clamp(1, to.work.width);
    let height = ((f64::from(r.height) * ratio).round() as u32).clamp(1, to.work.height);
    let fx = relative(r.x, r.width, from.work.x, from.work.width);
    let fy = relative(r.y, r.height, from.work.y, from.work.height);
    Rect {
        x: to.work.x + (fx * f64::from(to.work.width - width)).round() as i32,
        y: to.work.y + (fy * f64::from(to.work.height - height)).round() as i32,
        width,
        height,
    }
}

/// 0.0 when the window sits at the start of the span, 1.0 at the end.
fn relative(pos: i32, size: u32, origin: i32, span: u32) -> f64 {
    let free = i64::from(span) - i64::from(size);
    if free <= 0 {
        return 0.0;
    }
    ((f64::from(pos) - f64::from(origin)) / free as f64).clamp(0.0, 1.0)
}

/// A rectangle is visible on at least one display.
pub fn on_any_display(areas: &[WorkArea], r: Rect) -> bool {
    areas.iter().any(|a| notifi_platform::overlap(r, a.bounds) > 0)
}

pub fn centred_on(work: Rect, r: Rect) -> Rect {
    place(work, Placement::Center, 0, 0, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, width: w, height: h }
    }

    fn area(id: &str, b: Rect, work: Rect, scale: f32) -> WorkArea {
        WorkArea { display_id: id.into(), bounds: b, work, scale }
    }

    const WORK: Rect = Rect { x: 0, y: 0, width: 1920, height: 1040 };

    #[test]
    fn halves_and_quarters_tile_the_work_area() {
        assert_eq!(place(WORK, LEFT_HALF, 0, 0, WORK), r(0, 0, 960, 1040));
        assert_eq!(place(WORK, RIGHT_HALF, 0, 0, WORK), r(960, 0, 960, 1040));
        assert_eq!(place(WORK, TOP_HALF, 0, 0, WORK), r(0, 0, 1920, 520));
        assert_eq!(place(WORK, BOTTOM_RIGHT, 0, 0, WORK), r(960, 520, 960, 520));
        assert_eq!(place(WORK, Placement::Maximize, 0, 0, WORK), WORK);
    }

    #[test]
    fn work_area_offset_is_respected() {
        let work = r(1920, 40, 2560, 1400);
        assert_eq!(place(work, LEFT_HALF, 0, 0, work), r(1920, 40, 1280, 1400));
        assert_eq!(place(work, RIGHT_HALF, 0, 0, work), r(3200, 40, 1280, 1400));
    }

    #[test]
    fn gap_is_kept_between_windows_and_at_edges() {
        let left = place(WORK, LEFT_HALF, 10, 0, WORK);
        let right = place(WORK, RIGHT_HALF, 10, 0, WORK);
        assert_eq!(left.x, 10);
        assert_eq!(left.y, 10);
        assert_eq!(right.x - (left.x + left.width as i32), 10);
        assert_eq!(WORK.width as i32 - (right.x + right.width as i32), 10);
        assert_eq!(place(WORK, Placement::Maximize, 10, 0, WORK), r(10, 10, 1900, 1020));
    }

    #[test]
    fn odd_sizes_leave_no_hole() {
        let work = r(0, 0, 1921, 1081);
        let a = place(work, cell(3, 1, 0, 0, 1, 1), 0, 0, work);
        let b = place(work, cell(3, 1, 1, 0, 1, 1), 0, 0, work);
        let c = place(work, cell(3, 1, 2, 0, 1, 1), 0, 0, work);
        assert_eq!(a.x + a.width as i32, b.x);
        assert_eq!(b.x + b.width as i32, c.x);
        assert_eq!(c.x + c.width as i32, 1921);
    }

    #[test]
    fn grid_cells_row_major() {
        let g = |n: &str| place(WORK, parse_placement(n).unwrap(), 0, 0, WORK);
        assert_eq!(g("Grid 1"), r(0, 0, 640, 520));
        assert_eq!(g("grid-3"), r(1280, 0, 640, 520));
        assert_eq!(g("Grid 4"), r(0, 520, 640, 520));
        assert_eq!(g("grid 6"), r(1280, 520, 640, 520));
        assert!(parse_placement("grid 7").is_none());
    }

    #[test]
    fn quarter_columns_rows_and_thirds() {
        let g = |n: &str| place(WORK, parse_placement(n).unwrap(), 0, 0, WORK);
        assert_eq!(g("Column 2"), r(480, 0, 480, 1040));
        assert_eq!(g("Row 4"), r(0, 780, 1920, 260));
        assert_eq!(g("Left two-thirds"), r(0, 0, 1280, 1040));
        assert_eq!(g("Right two thirds"), r(640, 0, 1280, 1040));
        assert_eq!(g("Bottom third"), r(0, 693, 1920, 347));
    }

    #[test]
    fn names_parse_in_either_spelling() {
        assert_eq!(parse_placement("Maximise"), Some(Placement::Maximize));
        assert_eq!(parse_placement("Centre"), Some(Placement::Center));
        assert_eq!(parse_placement("Top left quarter"), Some(TOP_LEFT));
        assert_eq!(parse_placement("nonsense"), None);
    }

    #[test]
    fn centre_keeps_size_and_centred_half_does_not() {
        let w = r(5, 5, 800, 600);
        assert_eq!(place(WORK, Placement::Center, 0, 0, w), r(560, 220, 800, 600));
        assert_eq!(place(WORK, Placement::CenterHalf, 0, 0, w), r(480, 0, 960, 1040));
        let huge = r(0, 0, 5000, 5000);
        assert_eq!(place(WORK, Placement::Center, 0, 0, huge), WORK);
    }

    #[test]
    fn margin_uses_the_larger_of_margin_and_gap() {
        assert_eq!(place(WORK, Placement::MaximizeMargin, 0, 50, WORK), r(50, 50, 1820, 940));
        assert_eq!(place(WORK, Placement::MaximizeMargin, 80, 50, WORK), r(80, 80, 1760, 880));
    }

    #[test]
    fn width_cycle_walks_half_third_two_thirds() {
        let mut cur = r(100, 100, 400, 400);
        let mut seen = Vec::new();
        for _ in 0..4 {
            let p = next_in_cycle(Dir::Left, WORK, 0, cur, 2);
            cur = place(WORK, p, 0, 0, cur);
            seen.push(cur.width);
        }
        assert_eq!(seen, vec![960, 640, 1280, 960]);
        let right = next_in_cycle(Dir::Right, WORK, 0, place(WORK, RIGHT_HALF, 0, 0, WORK), 2);
        assert_eq!(place(WORK, right, 0, 0, WORK), r(1280, 0, 640, 1040));
    }

    #[test]
    fn neighbours_side_by_side_and_stacked() {
        let a = area("A", r(0, 0, 1920, 1080), r(0, 0, 1920, 1040), 1.0);
        let b = area("B", r(1920, 0, 2560, 1440), r(1920, 0, 2560, 1400), 1.0);
        let c = area("C", r(0, 1080, 1920, 1080), r(0, 1080, 1920, 1080), 1.0);
        let all = vec![a.clone(), b.clone(), c.clone()];
        assert_eq!(neighbour(&all, &a, Dir::Right).unwrap().display_id, "B");
        assert_eq!(neighbour(&all, &b, Dir::Left).unwrap().display_id, "A");
        assert_eq!(neighbour(&all, &a, Dir::Down).unwrap().display_id, "C");
        assert_eq!(neighbour(&all, &c, Dir::Up).unwrap().display_id, "A");
        assert!(neighbour(&all, &a, Dir::Left).is_none());
        assert_eq!(neighbour(&all, &c, Dir::Right).unwrap().display_id, "B", "B starts at C's right and shares rows 1080 to 1440");
        assert!(neighbour(&all, &a, Dir::Up).is_none());
    }

    #[test]
    fn nearest_neighbour_wins_with_three_in_a_row() {
        let m = |id: &str, x| area(id, r(x, 0, 1000, 800), r(x, 0, 1000, 800), 1.0);
        let all = vec![m("A", 0), m("B", 1000), m("C", 2000)];
        assert_eq!(neighbour(&all, &all[0], Dir::Right).unwrap().display_id, "B");
        assert_eq!(neighbour(&all, &all[2], Dir::Left).unwrap().display_id, "B");
    }

    #[test]
    fn monitor_lookup_by_overlap_and_point() {
        let all = vec![area("A", r(0, 0, 1920, 1080), r(0, 0, 1920, 1040), 1.0), area("B", r(1920, 0, 2560, 1440), r(1920, 0, 2560, 1400), 1.0)];
        assert_eq!(area_for(&all, r(1700, 100, 400, 300)).unwrap().display_id, "A");
        assert_eq!(area_for(&all, r(1800, 100, 400, 300)).unwrap().display_id, "B");
        assert_eq!(area_for(&all, r(2000, 100, 400, 300)).unwrap().display_id, "B");
        assert_eq!(area_for(&all, r(9000, 0, 10, 10)).unwrap().display_id, "B");
        assert_eq!(area_at(&all, 1919, 5).unwrap().display_id, "A");
        assert_eq!(area_at(&all, 1920, 5).unwrap().display_id, "B");
        assert!(area_at(&all, -1, 5).is_none());
    }

    #[test]
    fn moving_between_displays_keeps_logical_size_and_relative_place() {
        let a = area("A", r(0, 0, 1920, 1080), r(0, 0, 1920, 1040), 1.0);
        let b = area("B", r(1920, 0, 3840, 2160), r(1920, 0, 3840, 2120), 2.0);
        let moved = move_between(r(960, 0, 960, 1040), &a, &b);
        assert_eq!((moved.width, moved.height), (1920, 2080));
        assert_eq!(moved.x + moved.width as i32, 1920 + 3840, "stays against the right edge");
        let back = move_between(moved, &b, &a);
        assert_eq!((back.width, back.height), (960, 1040));
    }

    #[test]
    fn a_window_larger_than_the_target_is_clamped_to_it() {
        let big = area("A", r(0, 0, 3840, 2160), r(0, 0, 3840, 2120), 1.0);
        let small = area("B", r(3840, 0, 1280, 720), r(3840, 0, 1280, 680), 1.0);
        let moved = move_between(r(100, 100, 3000, 2000), &big, &small);
        assert!(covers_inside(small.work, moved), "{moved:?}");
    }

    fn covers_inside(outer: Rect, inner: Rect) -> bool {
        notifi_platform::covers(outer, inner)
    }

    #[test]
    fn same_place_tolerance() {
        assert!(same_place(r(0, 0, 100, 100), r(1, 1, 99, 101), 2));
        assert!(!same_place(r(0, 0, 100, 100), r(5, 0, 100, 100), 2));
    }

    #[test]
    fn opposite_halves() {
        assert_eq!(opposite(LEFT_HALF), RIGHT_HALF);
        assert_eq!(opposite(BOTTOM_HALF), TOP_HALF);
        assert_eq!(opposite(TOP_LEFT), TOP_LEFT);
    }
}
