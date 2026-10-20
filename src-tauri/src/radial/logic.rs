//! Radial menu rules that need no window or hook: slice geometry, the wheel's data and what a
//! click or a button release does.

use std::f64::consts::TAU;

use serde::{Deserialize, Serialize};

pub const MIN_SLICES: usize = 4;
pub const MAX_SLICES: usize = 12;
const MAX_DEPTH: usize = 3;
const MAX_TEXT: usize = 200;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Slice {
    pub label: String,
    pub icon: String,
    /// "" (empty slot), "open" (file, folder, app or script), "url", "action" (a shortcut action id) or "submenu".
    pub kind: String,
    pub arg: String,
    pub children: Vec<Slice>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Wheel {
    pub slices: Vec<Slice>,
}

const KINDS: &[&str] = &["open", "url", "action", "submenu"];

fn slice(label: &str, icon: &str, action: &str) -> Slice {
    Slice { label: label.into(), icon: icon.into(), kind: "action".into(), arg: action.into(), children: Vec::new() }
}

/// The starter wheel: tools that exist in every install.
pub fn starter_wheel() -> Wheel {
    Wheel {
        slices: vec![
            slice("Command bar", "utilities", "commandBar"),
            slice("Clipboard", "clipboard", "clipboardHistory"),
            slice("Quick panel", "controls", "quickPanel"),
            slice("Shelf", "apps", "shelfToggle"),
            slice("Scratchpad", "notes", "scratchpadToggle"),
            slice("Cleaning mode", "leaf", "energy.cleaning"),
            slice("Play or pause", "sound", "mediaPlayPause"),
            slice("Settings", "settings", "openSettings"),
        ],
    }
}

/// Drops what the wheel cannot run: unknown kinds, empty targets, extra depth, too many slices.
pub fn sanitize(wheel: Wheel) -> Wheel {
    fn level(slices: Vec<Slice>, depth: usize) -> Vec<Slice> {
        slices
            .into_iter()
            .take(MAX_SLICES)
            .map(|mut s| {
                s.label = s.label.trim().chars().take(40).collect();
                s.icon = s.icon.trim().chars().take(30).collect();
                s.arg = s.arg.trim().chars().take(MAX_TEXT).collect();
                let children = std::mem::take(&mut s.children);
                let valid = KINDS.contains(&s.kind.as_str());
                if !valid || (s.kind != "submenu" && s.arg.is_empty()) || (s.kind == "submenu" && depth + 1 >= MAX_DEPTH) {
                    return Slice::default();
                }
                if s.kind == "submenu" {
                    s.children = level(children, depth + 1);
                }
                s
            })
            .collect()
    }
    Wheel { slices: level(wheel.slices, 0) }
}

/// The slice under a pointer `dx`,`dy` away from the wheel's centre, or `None` inside the dead zone.
/// Slice 0 is centred straight up and the rest follow clockwise. Slots past the end count as slices.
pub fn slice_at(dx: f64, dy: f64, dead_zone: f64, count: usize) -> Option<usize> {
    let dist = dx.hypot(dy);
    if count == 0 || dist <= 0.0 || dist < dead_zone {
        return None;
    }
    let step = TAU / count as f64;
    let angle = dx.atan2(-dy);
    let shifted = (angle + step / 2.0).rem_euclid(TAU);
    Some(((shifted / step) as usize).min(count - 1))
}

/// Where to centre a square of side `2 * half` around `cursor` so it stays inside `area`
/// (left, top, right, bottom); centred on the area when it cannot fit.
pub fn clamp_center(cursor: (i32, i32), half: i32, area: (i32, i32, i32, i32)) -> (i32, i32) {
    let fit = |v: i32, lo: i32, hi: i32| if hi - lo < 2 * half { lo + (hi - lo) / 2 } else { v.clamp(lo + half, hi - half) };
    (fit(cursor.0, area.0, area.2), fit(cursor.1, area.1, area.3))
}

/// The slices of the ring reached by following `path` through submenus.
pub fn level<'a>(root: &'a [Slice], path: &[usize]) -> Option<&'a [Slice]> {
    let mut current = root;
    for &i in path {
        let s = current.get(i)?;
        if s.kind != "submenu" {
            return None;
        }
        current = &s.children;
    }
    Some(current)
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Run(Slice),
    Enter(usize),
    Back,
    Close,
    /// An empty slot was clicked.
    Stay,
}

/// What picking `hovered` does. In the dead zone a click steps back out of a submenu and a release closes.
pub fn choose(ring: &[Slice], hovered: Option<usize>, click: bool, in_submenu: bool) -> Outcome {
    let Some(i) = hovered else {
        return if click && in_submenu { Outcome::Back } else { Outcome::Close };
    };
    match ring.get(i) {
        Some(s) if s.kind == "submenu" => Outcome::Enter(i),
        Some(s) if !s.kind.is_empty() => Outcome::Run(s.clone()),
        _ if click => Outcome::Stay,
        _ => Outcome::Close,
    }
}

/// Shortcut actions that may appear in a slice, with their names for the editor.
pub const ACTIONS: &[(&str, &str)] = &[
    ("commandBar", "Command bar"),
    ("clipboardHistory", "Clipboard history"),
    ("plainTextPaste", "Paste as plain text"),
    ("shelfToggle", "Shelf"),
    ("scratchpadToggle", "Scratchpad"),
    ("quickPanel", "Quick panel"),
    ("energy.cleaning", "Cleaning mode"),
    ("energy.keepAwake", "Keep awake"),
    ("openSettings", "Open settings"),
    ("mediaPlayPause", "Media: play or pause"),
    ("mediaNext", "Media: next track"),
    ("mediaPrevious", "Media: previous track"),
    ("quickToggleDarkMode", "Toggle dark mode"),
    ("quickToggleMuteMic", "Toggle microphone mute"),
    ("quickToggleLockScreen", "Lock screen"),
    ("switcherOpen", "App switcher"),
    ("wm.leftHalf", "Window: left half"),
    ("wm.rightHalf", "Window: right half"),
    ("wm.maximize", "Window: maximise"),
    ("wm.center", "Window: centre"),
    ("wm.nextDisplay", "Window: next display"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_run_clockwise_from_the_top() {
        assert_eq!(slice_at(0.0, -100.0, 20.0, 8), Some(0));
        assert_eq!(slice_at(100.0, 0.0, 20.0, 8), Some(2));
        assert_eq!(slice_at(0.0, 100.0, 20.0, 8), Some(4));
        assert_eq!(slice_at(-100.0, 0.0, 20.0, 8), Some(6));
        assert_eq!(slice_at(71.0, -71.0, 20.0, 8), Some(1));
    }

    #[test]
    fn slice_edges_split_halfway_between_centres() {
        // 8 slices: slice 0 covers -22.5 to 22.5 degrees from straight up.
        let at = |deg: f64| slice_at(deg.to_radians().sin() * 100.0, -deg.to_radians().cos() * 100.0, 10.0, 8);
        assert_eq!(at(21.0), Some(0));
        assert_eq!(at(24.0), Some(1));
        assert_eq!(at(-21.0), Some(0));
        assert_eq!(at(-24.0), Some(7));
    }

    #[test]
    fn dead_zone_and_empty_wheel_select_nothing() {
        assert_eq!(slice_at(5.0, 5.0, 20.0, 8), None);
        assert_eq!(slice_at(0.0, 0.0, 0.0, 8), None);
        assert_eq!(slice_at(50.0, 0.0, 20.0, 0), None);
        assert_eq!(slice_at(20.0, 0.0, 20.0, 4), Some(1));
    }

    #[test]
    fn every_slice_count_has_a_slice_for_each_direction() {
        for n in MIN_SLICES..=MAX_SLICES {
            for i in 0..n {
                let mid = TAU * i as f64 / n as f64;
                assert_eq!(slice_at(mid.sin() * 80.0, -mid.cos() * 80.0, 10.0, n), Some(i), "{n} slices, slice {i}");
            }
        }
    }

    #[test]
    fn wheel_stays_inside_the_display() {
        let area = (0, 0, 1920, 1080);
        assert_eq!(clamp_center((960, 540), 180, area), (960, 540));
        assert_eq!(clamp_center((10, 5), 180, area), (180, 180));
        assert_eq!(clamp_center((1915, 1075), 180, area), (1740, 900));
        assert_eq!(clamp_center((50, 50), 180, (0, 0, 300, 300)), (150, 150));
        assert_eq!(clamp_center((2000, 100), 180, (1920, 0, 3840, 1080)), (2100, 180));
    }

    fn open(arg: &str) -> Slice {
        Slice { label: "x".into(), kind: "open".into(), arg: arg.into(), ..Slice::default() }
    }

    fn submenu(children: Vec<Slice>) -> Slice {
        Slice { label: "more".into(), kind: "submenu".into(), children, ..Slice::default() }
    }

    #[test]
    fn choosing_runs_enters_or_closes() {
        let ring = vec![open("C:\\a.exe"), submenu(vec![open("b")]), Slice::default()];
        assert_eq!(choose(&ring, Some(0), true, false), Outcome::Run(open("C:\\a.exe")));
        assert_eq!(choose(&ring, Some(1), false, false), Outcome::Enter(1));
        assert_eq!(choose(&ring, Some(2), true, false), Outcome::Stay);
        assert_eq!(choose(&ring, Some(7), true, false), Outcome::Stay);
        assert_eq!(choose(&ring, Some(2), false, false), Outcome::Close);
        assert_eq!(choose(&ring, None, false, true), Outcome::Close);
        assert_eq!(choose(&ring, None, true, true), Outcome::Back);
        assert_eq!(choose(&ring, None, true, false), Outcome::Close);
    }

    #[test]
    fn levels_follow_submenus_only() {
        let root = vec![open("a"), submenu(vec![open("b"), submenu(vec![open("c")])])];
        assert_eq!(level(&root, &[]).unwrap().len(), 2);
        assert_eq!(level(&root, &[1]).unwrap()[0].arg, "b");
        assert_eq!(level(&root, &[1, 1]).unwrap()[0].arg, "c");
        assert!(level(&root, &[0]).is_none());
        assert!(level(&root, &[5]).is_none());
    }

    #[test]
    fn sanitize_blanks_bad_slices_and_limits_depth() {
        let bad_kind = Slice { kind: "format-disk".into(), arg: "x".into(), ..Slice::default() };
        let no_target = open("  ");
        let deep = submenu(vec![submenu(vec![submenu(vec![open("z")])])]);
        let many: Vec<Slice> = (0..20).map(|_| open("a")).collect();
        let clean = sanitize(Wheel { slices: vec![bad_kind, no_target, deep] });
        assert_eq!(clean.slices[0], Slice::default());
        assert_eq!(clean.slices[1], Slice::default());
        let inner = &clean.slices[2].children[0].children[0];
        assert_eq!(inner, &Slice::default(), "a submenu three levels down is dropped");
        assert_eq!(sanitize(Wheel { slices: many }).slices.len(), MAX_SLICES);
    }

    #[test]
    fn starter_wheel_survives_sanitising_and_only_uses_known_actions() {
        let wheel = starter_wheel();
        assert_eq!(sanitize(wheel.clone()), wheel);
        for s in &wheel.slices {
            assert!(ACTIONS.iter().any(|(id, _)| *id == s.arg), "{} is not in ACTIONS", s.arg);
        }
    }

    #[test]
    fn wheel_round_trips_through_json() {
        let wheel = Wheel { slices: vec![open("C:\\x.exe"), submenu(vec![open("y")])] };
        let back: Wheel = serde_json::from_str(&serde_json::to_string(&wheel).unwrap()).unwrap();
        assert_eq!(back, wheel);
        let sparse: Wheel = serde_json::from_str(r#"{"slices":[{"kind":"url","arg":"https://a.b"}]}"#).unwrap();
        assert_eq!(sparse.slices[0].label, "");
    }
}
