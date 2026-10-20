//! One window drag, followed from pointer sample to pointer sample: which snap area the pointer is
//! in, whether to hold back at a border shared with another display, and what the window would become.

use notifi_platform::{Rect, WorkArea};

use super::geometry::{area_at, place, Placement};
use super::snap::{edge_of, shared_border, zone_at, Rule, Speed, SLOW_PX_PER_S};

/// What releasing the pointer now would do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub placement: Placement,
    pub rect: Rect,
    /// Bounds of the display the pointer is on.
    pub display: Rect,
}

pub struct Tracker {
    areas: Vec<WorkArea>,
    rules: Vec<Rule>,
    gap: u32,
    margin: u32,
    speed: Speed,
}

impl Tracker {
    pub fn new(areas: Vec<WorkArea>, rules: Vec<Rule>, gap: u32, margin: u32) -> Self {
        Self { areas, rules, gap, margin, speed: Speed::default() }
    }

    /// `current` is the window's present rectangle, used by placements that keep its size.
    pub fn update(&mut self, t_ms: u64, pointer: (i32, i32), current: Rect) -> Option<Target> {
        self.speed.push(t_ms, pointer.0, pointer.1);
        let area = area_at(&self.areas, pointer.0, pointer.1)?;
        let scale = f64::from(area.scale.max(1.0));
        let edge_px = (3.0 * scale).round() as i32;
        let corner_px = (40.0 * scale).round() as i32;
        let rule = zone_at(&self.rules, pointer, area.bounds, edge_px, corner_px)?;
        if let Some(edge) = edge_of(&rule) {
            if shared_border(&self.areas, area.bounds, edge, pointer) && self.speed.px_per_s() >= SLOW_PX_PER_S {
                return None;
            }
        }
        Some(Target { placement: rule.placement, rect: place(area.work, rule.placement, self.gap, self.margin, current), display: area.bounds })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_mgmt::geometry::LEFT_HALF;
    use crate::windows_mgmt::snap::rules_from;

    fn r(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, width: w, height: h }
    }

    fn two_displays() -> Vec<WorkArea> {
        vec![
            WorkArea { display_id: "A".into(), bounds: r(0, 0, 1920, 1080), work: r(0, 0, 1920, 1040), scale: 1.0 },
            WorkArea { display_id: "B".into(), bounds: r(1920, 0, 2560, 1440), work: r(1920, 0, 2560, 1400), scale: 1.0 },
        ]
    }

    fn tracker() -> Tracker {
        Tracker::new(two_displays(), rules_from(&[], true), 0, 0)
    }

    #[test]
    fn a_slow_approach_to_the_shared_border_snaps() {
        let mut t = tracker();
        let cur = r(500, 300, 800, 600);
        let mut last = None;
        for i in 0..10 {
            last = t.update(i * 20, (1910 + i as i32, 500), cur);
        }
        let target = last.expect("slow enough to snap on the right edge of A");
        assert_eq!(target.rect, r(960, 0, 960, 1040));
        assert_eq!(target.display, r(0, 0, 1920, 1080));
    }

    #[test]
    fn a_fast_sweep_across_the_shared_border_does_not() {
        let mut t = tracker();
        let cur = r(500, 300, 800, 600);
        let mut at_edge = None;
        for i in 0..8 {
            at_edge = t.update(i * 16, (1500 + i as i32 * 60, 500), cur);
        }
        assert_eq!(at_edge, None, "pointer at x=1920-ish moving at several thousand px/s");
        // The same edge position, reached slowly, snaps.
        let mut slow = tracker();
        assert!(slow.update(0, (1919, 500), cur).is_some());
    }

    #[test]
    fn the_far_edges_snap_regardless_of_speed() {
        let mut t = tracker();
        let cur = r(500, 300, 800, 600);
        t.update(0, (300, 500), cur);
        let target = t.update(16, (0, 500), cur).unwrap();
        assert_eq!(target.placement, LEFT_HALF);
        assert_eq!(target.rect, r(0, 0, 960, 1040));
    }

    #[test]
    fn targets_follow_the_display_the_pointer_is_on() {
        let mut t = tracker();
        let cur = r(500, 300, 800, 600);
        let target = t.update(0, (1920, 700), cur).unwrap();
        assert_eq!(target.rect, r(1920, 0, 1280, 1400));
        assert_eq!(target.display, r(1920, 0, 2560, 1440));
    }

    #[test]
    fn nothing_in_the_middle_of_a_display() {
        let mut t = tracker();
        assert_eq!(t.update(0, (960, 540), r(0, 0, 100, 100)), None);
    }
}
