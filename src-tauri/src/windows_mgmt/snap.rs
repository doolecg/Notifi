//! Screen-edge snapping: which placement an edge or corner is tied to, where the pointer has to be to
//! hit it, and whether a border shared with another display should let the window through.

use notifi_platform::{Rect, WorkArea};

use super::geometry::{parse_placement, Placement, BOTTOM_LEFT, BOTTOM_RIGHT, LEFT_HALF, RIGHT_HALF, TOP_LEFT, TOP_RIGHT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneKey {
    /// Zone `index` (from the top or the left) of an edge cut into `count` equal zones.
    Edge { edge: Edge, index: u8, count: u8 },
    Corner(Corner),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rule {
    pub key: ZoneKey,
    pub placement: Placement,
}

pub const MAX_ZONES: u8 = 4;

/// `Left edge = Left half`, `Top edge 2/3 = Right half`, `Top left corner = Top left`.
pub fn parse_rule(line: &str) -> Option<Rule> {
    let (lhs, rhs) = line.split_once('=')?;
    let placement = parse_placement(rhs.trim())?;
    let lhs = lhs.trim().to_lowercase();
    let words: Vec<&str> = lhs.split_whitespace().collect();
    let key = match words.as_slice() {
        [side, "edge"] => ZoneKey::Edge { edge: edge_named(side)?, index: 0, count: 1 },
        [side, "edge", frac] => {
            let (n, m) = frac.split_once('/')?;
            let (n, m): (u8, u8) = (n.parse().ok()?, m.parse().ok()?);
            if m == 0 || m > MAX_ZONES || n == 0 || n > m {
                return None;
            }
            ZoneKey::Edge { edge: edge_named(side)?, index: n - 1, count: m }
        }
        [v, h, "corner"] => ZoneKey::Corner(match (*v, *h) {
            ("top", "left") => Corner::TopLeft,
            ("top", "right") => Corner::TopRight,
            ("bottom", "left") => Corner::BottomLeft,
            ("bottom", "right") => Corner::BottomRight,
            _ => return None,
        }),
        _ => return None,
    };
    Some(Rule { key, placement })
}

fn edge_named(s: &str) -> Option<Edge> {
    Some(match s {
        "left" => Edge::Left,
        "right" => Edge::Right,
        "top" => Edge::Top,
        "bottom" => Edge::Bottom,
        _ => return None,
    })
}

pub fn default_rules() -> Vec<Rule> {
    let edge = |edge, placement| Rule { key: ZoneKey::Edge { edge, index: 0, count: 1 }, placement };
    vec![
        edge(Edge::Left, LEFT_HALF),
        edge(Edge::Right, RIGHT_HALF),
        edge(Edge::Top, Placement::Maximize),
        Rule { key: ZoneKey::Corner(Corner::TopLeft), placement: TOP_LEFT },
        Rule { key: ZoneKey::Corner(Corner::TopRight), placement: TOP_RIGHT },
        Rule { key: ZoneKey::Corner(Corner::BottomLeft), placement: BOTTOM_LEFT },
        Rule { key: ZoneKey::Corner(Corner::BottomRight), placement: BOTTOM_RIGHT },
    ]
}

/// The user's rules, or the defaults when the list is empty. Lines that do not parse are skipped.
pub fn rules_from(lines: &[String], top_edge_maximizes: bool) -> Vec<Rule> {
    let mut rules: Vec<Rule> = if lines.iter().all(|l| l.trim().is_empty()) {
        default_rules()
    } else {
        lines.iter().filter_map(|l| parse_rule(l)).collect()
    };
    if !top_edge_maximizes {
        rules.retain(|r| {
            !(matches!(r.key, ZoneKey::Edge { edge: Edge::Top, .. }) && r.placement == Placement::Maximize)
        });
    } else if !rules.iter().any(|r| matches!(r.key, ZoneKey::Edge { edge: Edge::Top, .. })) {
        rules.push(Rule { key: ZoneKey::Edge { edge: Edge::Top, index: 0, count: 1 }, placement: Placement::Maximize });
    }
    rules
}

/// Distances from a point to each side of a display, in pixels (0 on the border pixel).
struct Gaps {
    left: i64,
    right: i64,
    top: i64,
    bottom: i64,
}

fn gaps(p: (i32, i32), d: Rect) -> Gaps {
    let (x, y) = (i64::from(p.0), i64::from(p.1));
    Gaps {
        left: x - i64::from(d.x),
        right: i64::from(d.x) + i64::from(d.width) - 1 - x,
        top: y - i64::from(d.y),
        bottom: i64::from(d.y) + i64::from(d.height) - 1 - y,
    }
}

/// The rule under the pointer. `edge_px` is how close to the border counts as touching it; a corner is
/// the part of a border within `corner_px` of the end. Rules for a corner fall back to the edge's own.
pub fn zone_at(rules: &[Rule], p: (i32, i32), display: Rect, edge_px: i32, corner_px: i32) -> Option<Rule> {
    let g = gaps(p, display);
    let edge_px = i64::from(edge_px);
    let corner_px = i64::from(corner_px.max(edge_px as i32));
    let touching = |edge| match edge {
        Edge::Left => g.left,
        Edge::Right => g.right,
        Edge::Top => g.top,
        Edge::Bottom => g.bottom,
    };
    let near: Vec<Edge> = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom].into_iter().filter(|e| touching(*e) <= edge_px).collect();
    if near.is_empty() || g.left < 0 || g.right < 0 || g.top < 0 || g.bottom < 0 {
        return None;
    }
    let horizontal = near.iter().any(|e| matches!(e, Edge::Left | Edge::Right));
    let vertical = near.iter().any(|e| matches!(e, Edge::Top | Edge::Bottom));
    let in_corner = (horizontal && (g.top <= corner_px || g.bottom <= corner_px)) || (vertical && (g.left <= corner_px || g.right <= corner_px));
    if in_corner {
        let corner = match (g.left <= g.right, g.top <= g.bottom) {
            (true, true) => Corner::TopLeft,
            (false, true) => Corner::TopRight,
            (true, false) => Corner::BottomLeft,
            (false, false) => Corner::BottomRight,
        };
        if let Some(r) = rules.iter().find(|r| r.key == ZoneKey::Corner(corner)) {
            return Some(*r);
        }
    }
    let edge = near.into_iter().min_by_key(|e| touching(*e))?;
    let (along, len) = match edge {
        Edge::Left | Edge::Right => (g.top, i64::from(display.height)),
        Edge::Top | Edge::Bottom => (g.left, i64::from(display.width)),
    };
    rules
        .iter()
        .filter_map(|r| match r.key {
            ZoneKey::Edge { edge: e, index, count } if e == edge && along * i64::from(count) / len.max(1) == i64::from(index) => Some((count, *r)),
            _ => None,
        })
        .max_by_key(|(count, _)| *count)
        .map(|(_, r)| r)
}

/// The touched edge of `display` borders another display at this point, so a window may be on its way across.
pub fn shared_border(areas: &[WorkArea], display: Rect, edge: Edge, p: (i32, i32)) -> bool {
    let (x, y) = (i64::from(p.0), i64::from(p.1));
    areas.iter().any(|a| {
        let b = a.bounds;
        if b == display {
            return false;
        }
        let (l, r, t, bt) = (i64::from(b.x), i64::from(b.x) + i64::from(b.width), i64::from(b.y), i64::from(b.y) + i64::from(b.height));
        let (dl, dr, dt, db) =
            (i64::from(display.x), i64::from(display.x) + i64::from(display.width), i64::from(display.y), i64::from(display.y) + i64::from(display.height));
        match edge {
            Edge::Left => r == dl && (t..bt).contains(&y),
            Edge::Right => l == dr && (t..bt).contains(&y),
            Edge::Top => bt == dt && (l..r).contains(&x),
            Edge::Bottom => t == db && (l..r).contains(&x),
        }
    })
}

pub fn edge_of(rule: &Rule) -> Option<Edge> {
    match rule.key {
        ZoneKey::Edge { edge, .. } => Some(edge),
        ZoneKey::Corner(_) => None,
    }
}

/// Recent pointer samples, for telling a slow approach from a fast sweep.
#[derive(Default)]
pub struct Speed {
    samples: Vec<(u64, i32, i32)>,
}

const SPEED_WINDOW_MS: u64 = 120;
/// Slower than this at a shared border and the window snaps; faster and it crosses.
pub const SLOW_PX_PER_S: f64 = 350.0;

impl Speed {
    pub fn push(&mut self, t_ms: u64, x: i32, y: i32) {
        self.samples.push((t_ms, x, y));
        self.samples.retain(|s| t_ms.saturating_sub(s.0) <= SPEED_WINDOW_MS);
    }

    pub fn px_per_s(&self) -> f64 {
        let (Some(first), Some(last)) = (self.samples.first(), self.samples.last()) else { return 0.0 };
        let dt = last.0.saturating_sub(first.0);
        if dt == 0 {
            return 0.0;
        }
        let d = f64::from(last.1 - first.1).hypot(f64::from(last.2 - first.2));
        d * 1000.0 / dt as f64
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_mgmt::geometry::{BOTTOM_HALF, TOP_HALF};

    fn r(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, width: w, height: h }
    }
    const D: Rect = Rect { x: 0, y: 0, width: 1920, height: 1080 };

    #[test]
    fn rules_parse() {
        let e = parse_rule("Left edge = Left half").unwrap();
        assert_eq!(e.key, ZoneKey::Edge { edge: Edge::Left, index: 0, count: 1 });
        assert_eq!(e.placement, LEFT_HALF);
        let z = parse_rule("top edge 2/3 = Right half").unwrap();
        assert_eq!(z.key, ZoneKey::Edge { edge: Edge::Top, index: 1, count: 3 });
        let c = parse_rule("Bottom right corner = Grid 6").unwrap();
        assert_eq!(c.key, ZoneKey::Corner(Corner::BottomRight));
        assert!(parse_rule("Top edge 5/5 = Left half").is_none());
        assert!(parse_rule("Top edge 0/2 = Left half").is_none());
        assert!(parse_rule("Middle edge = Left half").is_none());
        assert!(parse_rule("Left edge = Nowhere").is_none());
        assert!(parse_rule("no equals").is_none());
    }

    #[test]
    fn defaults_cover_sides_top_and_corners() {
        let rules = rules_from(&[], true);
        let at = |x, y| zone_at(&rules, (x, y), D, 4, 40).map(|r| r.placement);
        assert_eq!(at(0, 540), Some(LEFT_HALF));
        assert_eq!(at(1919, 540), Some(RIGHT_HALF));
        assert_eq!(at(960, 0), Some(Placement::Maximize));
        assert_eq!(at(960, 1079), None);
        assert_eq!(at(0, 0), Some(TOP_LEFT));
        assert_eq!(at(1919, 1079), Some(BOTTOM_RIGHT));
        assert_eq!(at(960, 540), None);
        assert_eq!(at(10, 540), None, "10 px in is not touching");
    }

    #[test]
    fn top_edge_maximise_can_be_switched_off() {
        let rules = rules_from(&[], false);
        assert_eq!(zone_at(&rules, (960, 0), D, 4, 40), None);
        assert!(zone_at(&rules, (0, 540), D, 4, 40).is_some());
    }

    #[test]
    fn user_rules_replace_the_defaults() {
        let lines = vec!["Bottom edge = Bottom half".to_string(), "Left edge = Left third".to_string()];
        let rules = rules_from(&lines, true);
        let at = |x, y| zone_at(&rules, (x, y), D, 4, 40).map(|r| r.placement);
        assert_eq!(at(960, 1079), Some(BOTTOM_HALF));
        assert_ne!(at(0, 540), Some(LEFT_HALF));
        assert_eq!(at(1919, 540), None);
        assert_eq!(at(960, 0), Some(Placement::Maximize), "top edge still maximises unless switched off");
    }

    #[test]
    fn one_edge_cut_into_four_zones() {
        let lines: Vec<String> = (1..=4).map(|i| format!("Left edge {i}/4 = Grid {i}")).collect();
        let rules = rules_from(&lines, false);
        let at = |y| zone_at(&rules, (0, y), D, 4, 0).map(|r| r.placement);
        assert_eq!(at(100), parse_placement("grid 1"));
        assert_eq!(at(300), parse_placement("grid 2"));
        assert_eq!(at(600), parse_placement("grid 3"));
        assert_eq!(at(1000), parse_placement("grid 4"));
    }

    #[test]
    fn finer_zone_beats_a_whole_edge() {
        let lines = vec!["Left edge = Left half".to_string(), "Left edge 1/2 = Top left".to_string()];
        let rules = rules_from(&lines, false);
        assert_eq!(zone_at(&rules, (0, 100), D, 4, 0).unwrap().placement, TOP_LEFT);
        assert_eq!(zone_at(&rules, (0, 900), D, 4, 0).unwrap().placement, LEFT_HALF);
    }

    #[test]
    fn corner_falls_back_to_the_edge_when_it_has_no_rule() {
        let rules = rules_from(&["Left edge = Left half".to_string(), "Top edge = Top half".to_string()], false);
        assert_eq!(zone_at(&rules, (0, 0), D, 4, 40).unwrap().placement, LEFT_HALF);
        assert_eq!(zone_at(&rules, (500, 0), D, 4, 40).unwrap().placement, TOP_HALF);
    }

    #[test]
    fn hit_test_on_a_display_that_is_not_at_the_origin() {
        let d2 = r(1920, -200, 2560, 1440);
        let rules = rules_from(&[], true);
        assert_eq!(zone_at(&rules, (1920, 500), d2, 4, 40).unwrap().placement, LEFT_HALF);
        assert_eq!(zone_at(&rules, (4479, 500), d2, 4, 40).unwrap().placement, RIGHT_HALF);
        assert_eq!(zone_at(&rules, (3000, -200), d2, 4, 40).unwrap().placement, Placement::Maximize);
        assert_eq!(zone_at(&rules, (1919, 500), d2, 4, 40), None, "outside the display");
    }

    #[test]
    fn shared_borders_between_two_displays() {
        let a = WorkArea { display_id: "A".into(), bounds: D, work: D, scale: 1.0 };
        let b = WorkArea { display_id: "B".into(), bounds: r(1920, 0, 2560, 1440), work: r(1920, 0, 2560, 1440), scale: 1.0 };
        let all = vec![a, b];
        assert!(shared_border(&all, D, Edge::Right, (1919, 500)));
        assert!(!shared_border(&all, D, Edge::Left, (0, 500)));
        assert!(!shared_border(&all, D, Edge::Top, (500, 0)));
        let bd = r(1920, 0, 2560, 1440);
        assert!(shared_border(&all, bd, Edge::Left, (1920, 500)));
        assert!(!shared_border(&all, bd, Edge::Left, (1920, 1300)), "A ends at y 1080");
    }

    #[test]
    fn slow_and_fast_pointer() {
        let mut s = Speed::default();
        for i in 0..8 {
            s.push(i * 16, 1900 + i as i32, 500);
        }
        assert!(s.px_per_s() < SLOW_PX_PER_S, "{}", s.px_per_s());
        s.clear();
        for i in 0..8 {
            s.push(i * 16, 1500 + i as i32 * 60, 500);
        }
        assert!(s.px_per_s() > SLOW_PX_PER_S * 4.0, "{}", s.px_per_s());
        s.clear();
        assert_eq!(s.px_per_s(), 0.0);
    }

    #[test]
    fn top_half_exists_for_rules() {
        assert_eq!(parse_rule("Top edge = Top half").unwrap().placement, TOP_HALF);
    }
}
