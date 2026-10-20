//! What a layout, fill-screen or desktop shortcut does to the active window.

use std::collections::HashMap;
use std::sync::Mutex;

use notifi_platform::{Rect, WindowId, WindowInfo, WindowManager, WorkArea};

use super::config::{listed, Config};
use super::geometry::{
    area_for, centred_on, move_between, neighbour, next_in_cycle, on_any_display, opposite, parse_placement, place,
    same_place, side_of, Dir, Placement, LEFT_HALF, RIGHT_HALF,
};

const TOLERANCE: i32 = 3;
const MAX_REMEMBERED: usize = 64;

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Done,
    /// Nothing to do, with the reason for a toast or a log line.
    Skipped(&'static str),
    Failed(String),
}

static PREVIOUS: Mutex<Option<HashMap<u64, Rect>>> = Mutex::new(None);
static FILLED: Mutex<Option<HashMap<u64, Rect>>> = Mutex::new(None);

fn remember(store: &Mutex<Option<HashMap<u64, Rect>>>, id: WindowId, r: Rect) {
    let mut g = store.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.get_or_insert_with(HashMap::new);
    if map.len() >= MAX_REMEMBERED {
        map.clear();
    }
    map.insert(id.0, r);
}

fn recall(store: &Mutex<Option<HashMap<u64, Rect>>>, id: WindowId, take: bool) -> Option<Rect> {
    let mut g = store.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.as_mut()?;
    if take { map.remove(&id.0) } else { map.get(&id.0).copied() }
}

pub fn is_filled(id: WindowId) -> bool {
    recall(&FILLED, id, false).is_some()
}

/// The window an action should work on: the focused one, unless it is Notifi's own, ignored or truly fullscreen.
pub fn target(wm: &dyn WindowManager, cfg: &Config) -> Result<WindowInfo, Outcome> {
    let win = wm.foreground().map_err(|e| Outcome::Failed(e.to_string()))?.ok_or(Outcome::Skipped("no active window"))?;
    if win.pid == Some(std::process::id()) {
        return Err(Outcome::Skipped("Notifi's own window"));
    }
    if cfg.ignores(win.app_id.as_deref()) {
        return Err(Outcome::Skipped("app is on the leave-alone list"));
    }
    if wm.window_state(win.id).is_ok_and(|s| s.fullscreen) {
        return Err(Outcome::Skipped("window is fullscreen"));
    }
    Ok(win)
}

fn set(wm: &dyn WindowManager, win: &WindowInfo, rect: Rect) -> Outcome {
    if !same_place(win.bounds, rect, 0) {
        remember(&PREVIOUS, win.id, win.bounds);
    }
    match wm.set_bounds(win.id, rect) {
        Ok(()) => Outcome::Done,
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

fn areas(wm: &dyn WindowManager) -> Result<Vec<WorkArea>, Outcome> {
    let a = wm.work_areas().map_err(|e| Outcome::Failed(e.to_string()))?;
    if a.is_empty() { Err(Outcome::Failed("no displays found".into())) } else { Ok(a) }
}

pub fn apply_placement(wm: &dyn WindowManager, cfg: &Config, win: &WindowInfo, p: Placement) -> Outcome {
    let areas = match areas(wm) {
        Ok(a) => a,
        Err(o) => return o,
    };
    let Some(area) = area_for(&areas, win.bounds) else { return Outcome::Skipped("window is on no display") };
    if p == Placement::Maximize && listed(&cfg.native_fullscreen_apps, win.app_id.as_deref()) {
        return match wm.maximize(win.id) {
            Ok(()) => Outcome::Done,
            Err(e) => Outcome::Failed(e.to_string()),
        };
    }
    let side = side_of(p);
    if cfg.cycle_widths && matches!(side, Some(Dir::Left | Dir::Right)) {
        let next = next_in_cycle(side.unwrap(), area.work, cfg.gap, win.bounds, TOLERANCE);
        return set(wm, win, place(area.work, next, cfg.gap, cfg.margin, win.bounds));
    }
    let here = place(area.work, p, cfg.gap, cfg.margin, win.bounds);
    if cfg.repeat_to_move && same_place(win.bounds, here, TOLERANCE) {
        if let Some(dir) = side {
            if let Some(next) = neighbour(&areas, area, dir) {
                return set(wm, win, place(next.work, opposite(p), cfg.gap, cfg.margin, win.bounds));
            }
        }
    }
    set(wm, win, here)
}

/// Moves the window to the next display (by position, wrapping), keeping its size in logical terms.
pub fn next_display(wm: &dyn WindowManager, win: &WindowInfo) -> Outcome {
    let mut areas = match areas(wm) {
        Ok(a) => a,
        Err(o) => return o,
    };
    if areas.len() < 2 {
        return Outcome::Skipped("only one display");
    }
    areas.sort_by_key(|a| (a.bounds.x, a.bounds.y));
    let Some(from) = area_for(&areas, win.bounds).cloned() else { return Outcome::Skipped("window is on no display") };
    let index = areas.iter().position(|a| a.display_id == from.display_id).unwrap_or(0);
    let to = &areas[(index + 1) % areas.len()];
    set(wm, win, move_between(win.bounds, &from, to))
}

/// Places a window that was just dragged, remembering where it started.
pub fn snap_to(wm: &dyn WindowManager, id: WindowId, previous: Rect, rect: Rect) -> Outcome {
    remember(&PREVIOUS, id, previous);
    match wm.set_bounds(id, rect) {
        Ok(()) => Outcome::Done,
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

/// Puts the window back where it was before the last layout action, and remembers where it was.
pub fn restore(wm: &dyn WindowManager, win: &WindowInfo) -> Outcome {
    let Some(prev) = recall(&PREVIOUS, win.id, true) else { return Outcome::Skipped("nothing to restore") };
    let areas = wm.work_areas().unwrap_or_default();
    let prev = if areas.is_empty() || on_any_display(&areas, prev) { prev } else { centred_on(areas[0].work, prev) };
    remember(&PREVIOUS, win.id, win.bounds);
    match wm.set_bounds(win.id, prev) {
        Ok(()) => Outcome::Done,
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

/// First call: the window loses its frame and covers the work area (or the display). Second: it goes back.
pub fn toggle_fill(wm: &dyn WindowManager, cfg: &Config, win: &WindowInfo) -> Outcome {
    if let Some(saved) = recall(&FILLED, win.id, true) {
        let areas = wm.work_areas().unwrap_or_default();
        let back = if areas.is_empty() || on_any_display(&areas, saved) {
            saved
        } else {
            areas.iter().find(|a| a.bounds.x == 0 && a.bounds.y == 0).or(areas.first()).map_or(saved, |a| centred_on(a.work, saved))
        };
        let _ = wm.set_decorations(win.id, true);
        return match wm.set_bounds(win.id, back) {
            Ok(()) => Outcome::Done,
            Err(e) => Outcome::Failed(e.to_string()),
        };
    }
    if wm.window_state(win.id).is_ok_and(|s| !s.resizable) {
        return Outcome::Skipped("window cannot be resized");
    }
    let areas = match areas(wm) {
        Ok(a) => a,
        Err(o) => return o,
    };
    let Some(area) = area_for(&areas, win.bounds) else { return Outcome::Skipped("window is on no display") };
    let cover = if cfg.fill_whole_display { area.bounds } else { area.work };
    let target = place(cover, Placement::Maximize, cfg.fill_gap, 0, win.bounds);
    // A maximised window remembers its restored size, not the screen-sized one it shows.
    let before = if wm.window_state(win.id).is_ok_and(|s| s.maximized) {
        let _ = wm.restore(win.id);
        wm.list().ok().and_then(|l| l.into_iter().find(|w| w.id == win.id)).map_or(win.bounds, |w| w.bounds)
    } else {
        win.bounds
    };
    remember(&FILLED, win.id, before);
    let _ = wm.set_decorations(win.id, false);
    match wm.set_bounds(win.id, target) {
        Ok(()) => Outcome::Done,
        Err(e) => {
            let _ = recall(&FILLED, win.id, true);
            let _ = wm.set_decorations(win.id, true);
            Outcome::Failed(e.to_string())
        }
    }
}

pub fn forget(id: WindowId) {
    let _ = recall(&PREVIOUS, id, true);
    let _ = recall(&FILLED, id, true);
}

/// Switches to desktop `n`, adding desktops first when the option allows it.
pub fn switch_desktop(wm: &dyn WindowManager, cfg: &Config, n: u32) -> Outcome {
    let state = match wm.desktop_state() {
        Ok(s) => s,
        Err(e) => return Outcome::Failed(e.to_string()),
    };
    if n > state.count {
        if !cfg.create_on_demand {
            return Outcome::Skipped("that desktop does not exist");
        }
        for _ in state.count..n {
            if let Err(e) = wm.create_desktop() {
                return Outcome::Failed(e.to_string());
            }
        }
    }
    match wm.switch_desktop(n) {
        Ok(()) => Outcome::Done,
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

/// Steps one desktop back or forward, never wrapping.
pub fn step_desktop(wm: &dyn WindowManager, forward: bool) -> Outcome {
    let s = match wm.desktop_state() {
        Ok(s) => s,
        Err(e) => return Outcome::Failed(e.to_string()),
    };
    let n = if forward { s.current + 1 } else { s.current.saturating_sub(1) };
    if n == 0 || n > s.count {
        return Outcome::Skipped("no desktop in that direction");
    }
    match wm.switch_desktop(n) {
        Ok(()) => Outcome::Done,
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

/// Sends the active window to desktop `n`, and follows it when the option is on.
pub fn move_to_desktop(wm: &dyn WindowManager, cfg: &Config, win: &WindowInfo, n: u32) -> Outcome {
    let state = match wm.desktop_state() {
        Ok(s) => s,
        Err(e) => return Outcome::Failed(e.to_string()),
    };
    if n > state.count {
        if !cfg.create_on_demand {
            return Outcome::Skipped("that desktop does not exist");
        }
        for _ in state.count..n {
            if let Err(e) = wm.create_desktop() {
                return Outcome::Failed(e.to_string());
            }
        }
    }
    if let Err(e) = wm.move_to_desktop(win.id, n) {
        return Outcome::Failed(e.to_string());
    }
    if cfg.follow_window {
        let _ = wm.switch_desktop(n);
        let _ = wm.focus(win.id);
    }
    Outcome::Done
}

/// Layout action ids as written in the shortcut list.
pub fn placement_for(key: &str) -> Option<Placement> {
    Some(match key {
        "leftHalf" => LEFT_HALF,
        "rightHalf" => RIGHT_HALF,
        "topHalf" => parse_placement("top half")?,
        "bottomHalf" => parse_placement("bottom half")?,
        "topLeft" => parse_placement("top left")?,
        "topRight" => parse_placement("top right")?,
        "bottomLeft" => parse_placement("bottom left")?,
        "bottomRight" => parse_placement("bottom right")?,
        "maximize" => Placement::Maximize,
        "maximizeMargin" => Placement::MaximizeMargin,
        "center" => Placement::Center,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use notifi_platform::{DesktopState, Result, WindowState};
    use std::sync::Mutex as StdMutex;

    fn r(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, width: w, height: h }
    }

    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1000);

    /// A window manager over two displays, with one window whose bounds follow `set_bounds`.
    struct Fake {
        win: StdMutex<WindowInfo>,
        areas: Vec<WorkArea>,
        decorated: StdMutex<bool>,
        desktops: StdMutex<DesktopState>,
        moved_to: StdMutex<Option<u32>>,
    }

    impl Fake {
        fn new(bounds: Rect) -> Self {
            Self {
                win: StdMutex::new(WindowInfo {
                    id: WindowId(NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)),
                    title: "t".into(),
                    app_id: Some("notepad.exe".into()),
                    pid: Some(1),
                    bounds,
                    monitor: None,
                    minimized: false,
                    focused: true,
                }),
                areas: vec![
                    WorkArea { display_id: "A".into(), bounds: r(0, 0, 1920, 1080), work: r(0, 0, 1920, 1040), scale: 1.0 },
                    WorkArea { display_id: "B".into(), bounds: r(1920, 0, 3840, 2160), work: r(1920, 0, 3840, 2120), scale: 2.0 },
                ],
                decorated: StdMutex::new(true),
                desktops: StdMutex::new(DesktopState { count: 2, current: 1 }),
                moved_to: StdMutex::new(None),
            }
        }
        fn info(&self) -> WindowInfo {
            self.win.lock().unwrap().clone()
        }
    }

    impl WindowManager for Fake {
        fn list(&self) -> Result<Vec<WindowInfo>> {
            Ok(vec![self.info()])
        }
        fn set_bounds(&self, _id: WindowId, b: Rect) -> Result<()> {
            self.win.lock().unwrap().bounds = b;
            Ok(())
        }
        fn set_decorations(&self, _id: WindowId, on: bool) -> Result<()> {
            *self.decorated.lock().unwrap() = on;
            Ok(())
        }
        fn window_state(&self, _id: WindowId) -> Result<WindowState> {
            Ok(WindowState { resizable: true, ..Default::default() })
        }
        fn work_areas(&self) -> Result<Vec<WorkArea>> {
            Ok(self.areas.clone())
        }
        fn desktop_state(&self) -> Result<DesktopState> {
            Ok(*self.desktops.lock().unwrap())
        }
        fn create_desktop(&self) -> Result<()> {
            self.desktops.lock().unwrap().count += 1;
            Ok(())
        }
        fn switch_desktop(&self, n: u32) -> Result<()> {
            self.desktops.lock().unwrap().current = n;
            Ok(())
        }
        fn move_to_desktop(&self, _id: WindowId, n: u32) -> Result<()> {
            *self.moved_to.lock().unwrap() = Some(n);
            Ok(())
        }
        fn focus(&self, _id: WindowId) -> Result<()> {
            Ok(())
        }
        fn maximize(&self, _id: WindowId) -> Result<()> {
            Ok(())
        }
    }

    fn cfg() -> Config {
        Config::from_settings(&crate::settings::Settings::default())
    }

    #[test]
    fn left_half_then_restore() {
        let wm = Fake::new(r(300, 200, 800, 600));
        let c = cfg();
        assert_eq!(apply_placement(&wm, &c, &wm.info(), LEFT_HALF), Outcome::Done);
        assert_eq!(wm.info().bounds, r(0, 0, 960, 1040));
        assert_eq!(restore(&wm, &wm.info()), Outcome::Done);
        assert_eq!(wm.info().bounds, r(300, 200, 800, 600));
        assert_eq!(restore(&wm, &wm.info()), Outcome::Done, "restore toggles back");
        assert_eq!(wm.info().bounds, r(0, 0, 960, 1040));
    }

    #[test]
    fn repeat_moves_to_the_next_display_and_mirrors() {
        let wm = Fake::new(r(0, 0, 960, 1040));
        let mut c = cfg();
        c.repeat_to_move = true;
        assert_eq!(apply_placement(&wm, &c, &wm.info(), RIGHT_HALF), Outcome::Done);
        assert_eq!(wm.info().bounds, r(960, 0, 960, 1040), "first press goes to the right half");
        assert_eq!(apply_placement(&wm, &c, &wm.info(), RIGHT_HALF), Outcome::Done);
        assert_eq!(wm.info().bounds, r(1920, 0, 1920, 2120), "second press crosses to B's left half");
    }

    #[test]
    fn cycling_widths() {
        let wm = Fake::new(r(100, 100, 500, 500));
        let mut c = cfg();
        c.cycle_widths = true;
        let mut widths = Vec::new();
        for _ in 0..3 {
            apply_placement(&wm, &c, &wm.info(), LEFT_HALF);
            widths.push(wm.info().bounds.width);
        }
        assert_eq!(widths, vec![960, 640, 1280]);
    }

    #[test]
    fn next_display_scales_the_window() {
        let wm = Fake::new(r(100, 100, 800, 600));
        assert_eq!(next_display(&wm, &wm.info()), Outcome::Done);
        let b = wm.info().bounds;
        assert_eq!((b.width, b.height), (1600, 1200));
        assert!(b.x >= 1920);
        assert_eq!(next_display(&wm, &wm.info()), Outcome::Done);
        assert_eq!(wm.info().bounds.width, 800, "wraps back to A at the original logical size");
    }

    #[test]
    fn fill_toggles_and_restores_exactly() {
        let wm = Fake::new(r(300, 200, 800, 600));
        let mut c = cfg();
        c.fill_gap = 10;
        assert_eq!(toggle_fill(&wm, &c, &wm.info()), Outcome::Done);
        assert_eq!(wm.info().bounds, r(10, 10, 1900, 1020));
        assert!(!*wm.decorated.lock().unwrap());
        assert!(is_filled(wm.info().id));
        assert_eq!(toggle_fill(&wm, &c, &wm.info()), Outcome::Done);
        assert_eq!(wm.info().bounds, r(300, 200, 800, 600));
        assert!(*wm.decorated.lock().unwrap());
        assert!(!is_filled(wm.info().id));
    }

    #[test]
    fn fill_can_cover_the_whole_display() {
        let wm = Fake::new(r(300, 200, 800, 600));
        let mut c = cfg();
        c.fill_whole_display = true;
        toggle_fill(&wm, &c, &wm.info());
        assert_eq!(wm.info().bounds, r(0, 0, 1920, 1080));
        toggle_fill(&wm, &c, &wm.info());
    }

    #[test]
    fn restoring_onto_an_unplugged_display_centres_on_the_first() {
        let wm = Fake::new(r(9000, 200, 800, 600));
        let c = cfg();
        let mut win = wm.info();
        win.bounds = r(9000, 200, 800, 600);
        remember(&FILLED, win.id, win.bounds);
        assert_eq!(toggle_fill(&wm, &c, &win), Outcome::Done);
        assert_eq!(wm.info().bounds, r(560, 220, 800, 600));
    }

    #[test]
    fn desktops_are_created_only_on_demand() {
        let wm = Fake::new(r(0, 0, 100, 100));
        let mut c = cfg();
        assert!(matches!(switch_desktop(&wm, &c, 4), Outcome::Skipped(_)));
        c.create_on_demand = true;
        assert_eq!(switch_desktop(&wm, &c, 4), Outcome::Done);
        let s = *wm.desktops.lock().unwrap();
        assert_eq!((s.count, s.current), (4, 4));
    }

    #[test]
    fn stepping_does_not_wrap() {
        let wm = Fake::new(r(0, 0, 100, 100));
        assert!(matches!(step_desktop(&wm, false), Outcome::Skipped(_)));
        assert_eq!(step_desktop(&wm, true), Outcome::Done);
        assert!(matches!(step_desktop(&wm, true), Outcome::Skipped(_)));
    }

    #[test]
    fn moving_a_window_follows_it_when_asked() {
        let wm = Fake::new(r(0, 0, 100, 100));
        let mut c = cfg();
        c.follow_window = false;
        assert_eq!(move_to_desktop(&wm, &c, &wm.info(), 2), Outcome::Done);
        assert_eq!(*wm.moved_to.lock().unwrap(), Some(2));
        assert_eq!(wm.desktops.lock().unwrap().current, 1);
        c.follow_window = true;
        move_to_desktop(&wm, &c, &wm.info(), 2);
        assert_eq!(wm.desktops.lock().unwrap().current, 2);
        c.create_on_demand = false;
        assert!(matches!(move_to_desktop(&wm, &c, &wm.info(), 9), Outcome::Skipped(_)));
    }

    #[test]
    fn ignored_and_own_windows_are_left_alone() {
        let wm = Fake::new(r(0, 0, 100, 100));
        let mut c = cfg();
        c.ignore_apps = vec!["notepad".into()];
        assert!(matches!(target(&wm, &c), Err(Outcome::Skipped(_))));
    }

    #[test]
    fn layout_keys_map_to_placements() {
        for k in ["leftHalf", "rightHalf", "topHalf", "bottomHalf", "topLeft", "topRight", "bottomLeft", "bottomRight", "maximize", "maximizeMargin", "center"] {
            assert!(placement_for(k).is_some(), "{k}");
        }
        assert!(placement_for("restore").is_none());
    }

    /// Starts Notepad, snaps it to the left half and screenshots the result into `NOTIFI_SNAP_SHOT`, then closes it by pid.
    #[cfg(windows)]
    #[test]
    #[ignore = "opens and moves a real Notepad window; run by hand with NOTIFI_SNAP_SHOT set"]
    fn live_snap_notepad_to_left_half() {
        use std::process::Command;
        use std::time::Duration;

        let shot = std::env::var("NOTIFI_SNAP_SHOT").expect("set NOTIFI_SNAP_SHOT to the png to write");
        let platform = notifi_platform::Platform::detect();
        let wm = platform.windows();
        let before: Vec<WindowId> = wm.list().unwrap().into_iter().map(|w| w.id).collect();
        let mut child = Command::new("notepad.exe").spawn().expect("start notepad");
        let mut mine = None;
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(200));
            mine = wm.list().unwrap().into_iter().find(|w| !before.contains(&w.id) && w.app_id.as_deref().is_some_and(|a| a.eq_ignore_ascii_case("notepad.exe")));
            if mine.is_some() {
                break;
            }
        }
        let win = mine.expect("a new Notepad window appeared");
        let pid = win.pid.expect("window has a pid");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let cfg = cfg();
            let areas = wm.work_areas().unwrap();
            let area = crate::windows_mgmt::geometry::area_for(&areas, win.bounds).unwrap().clone();
            assert_eq!(apply_placement(wm, &cfg, &win, LEFT_HALF), Outcome::Done);
            std::thread::sleep(Duration::from_millis(700));
            let now = wm.list().unwrap().into_iter().find(|w| w.id == win.id).unwrap();
            let want = place(area.work, LEFT_HALF, 0, 0, win.bounds);
            assert!(same_place(now.bounds, want, 2), "{:?} vs {:?}", now.bounds, want);
            let script = format!(
                "Add-Type -AssemblyName System.Windows.Forms,System.Drawing; $b=[Windows.Forms.SystemInformation]::VirtualScreen; $bmp=New-Object Drawing.Bitmap $b.Width,$b.Height; $g=[Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($b.Location,[Drawing.Point]::Empty,$b.Size); $bmp.Save('{shot}')"
            );
            Command::new("powershell").args(["-NoProfile", "-Command", &script]).status().expect("screenshot");
            println!("snapped to {:?}, screenshot at {shot}", now.bounds);
        }));
        let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).status();
        let _ = child.kill();
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
