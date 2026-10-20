//! Taskbar preview (hover a taskbar button for large previews of that app's windows) and taskbar
//! clicks (clicking the active app's button minimises it or cycles its windows).
//!
//! Windows: a worker thread polls the pointer; when it is over a taskbar it hit-tests a cached
//! list of the taskbar's UI Automation buttons (refreshed at most every 1.5 s while hovering), so
//! no UIA call ever runs inside the mouse hook. The low-level mouse hook only compares the click
//! point with the hovered button rect the worker last published. Linux: not available yet.

use std::collections::HashSet;
use std::sync::Mutex;

use notifi_platform::{Platform, Rect, WindowId, WindowInfo};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use super::model::{self, Grid};
use super::thumbs::{self, Kind, Slot, Thumbs};
use super::{listed, opt_bool, opt_list, opt_num, opt_str};
use crate::settings::{Settings, Store};

pub const WINDOW: &str = "taskbar";
const PREVIEW: &str = "taskbarPreview";
const CLICKS: &str = "taskbarClicks";
const OPEN_EVENT: &str = "taskbar://open";
const HIDE_EVENT: &str = "taskbar://hide";
const IMAGE_EVENT: &str = "taskbar://image";

pub fn clicks_wanted(s: &Settings) -> bool {
    cfg!(windows) && s.enabled(CLICKS)
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub id: u64,
    pub title: String,
    pub app: String,
    pub minimized: bool,
    pub icon: Option<String>,
    pub no_thumb: bool,
    /// The window is on another virtual desktop.
    pub other_desktop: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OpenPayload {
    app: String,
    cards: Vec<Card>,
    grid: Grid,
    kind: Kind,
    minimal: bool,
    close_quits: bool,
    opacity: f64,
}

#[derive(Default)]
pub struct TaskbarState {
    thumbs: Mutex<Thumbs>,
    /// Physical rect of the shown preview, and the button key it is for.
    shown: Mutex<Option<(Rect, String)>>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    app.manage(TaskbarState::default());
    let window = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("taskbar.html".into()))
        .title("Notifi taskbar preview")
        .inner_size(480.0, 220.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .focusable(false)
        .visible(false)
        .focused(false)
        .build()?;
    #[cfg(windows)]
    {
        super::prepare_window(&window);
        win::start(app);
    }
    #[cfg(not(windows))]
    let _ = window;
    Ok(())
}

fn settings(app: &AppHandle) -> Settings {
    app.state::<Store>().get()
}

pub(crate) fn hide(app: &AppHandle) {
    let state = app.state::<TaskbarState>();
    if state.shown.lock().unwrap().take().is_none() {
        return;
    }
    state.thumbs.lock().unwrap().clear();
    let _ = app.emit_to(WINDOW, HIDE_EVENT, ());
    if let Some(w) = app.get_webview_window(WINDOW) {
        let _ = w.hide();
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
/// Where the preview goes: above a bottom taskbar, below a top one, beside a side one.
pub fn place(button: Rect, bar: Rect, display: Rect, size: (u32, u32)) -> (i32, i32) {
    const GAP: i32 = 8;
    let (w, h) = (size.0 as i32, size.1 as i32);
    let clamp_x = |x: i32| x.clamp(display.x, (display.x + display.width as i32 - w).max(display.x));
    let clamp_y = |y: i32| y.clamp(display.y, (display.y + display.height as i32 - h).max(display.y));
    let horizontal = bar.width >= bar.height;
    if horizontal {
        let x = clamp_x(button.x + button.width as i32 / 2 - w / 2);
        let bottom = bar.y + bar.height as i32 / 2 > display.y + display.height as i32 / 2;
        (x, if bottom { bar.y - h - GAP } else { bar.y + bar.height as i32 + GAP })
    } else {
        let y = clamp_y(button.y + button.height as i32 / 2 - h / 2);
        let right = bar.x + bar.width as i32 / 2 > display.x + display.width as i32 / 2;
        (if right { bar.x - w - GAP } else { bar.x + bar.width as i32 + GAP }, y)
    }
}

/// Shows the preview for `windows` next to `button`.
#[cfg_attr(not(windows), allow(dead_code))]
fn show(app: &AppHandle, key: String, app_name: String, windows: Vec<WindowInfo>, other_desktop: &HashSet<u64>, button: Rect, bar: Rect) {
    let s = settings(app);
    let kind = thumbs::kind(app.state::<Platform>().backend());
    let minimal = opt_bool(&s, PREVIEW, "minimal", false);
    let size = if minimal { "minimal".to_string() } else { opt_str(&s, PREVIEW, "cardSize", "normal") };
    let paused = opt_list(&s, PREVIEW, "pauseApps");
    let cards: Vec<Card> = windows
        .iter()
        .map(|w| {
            let name = super::app_name(w);
            Card {
                id: w.id.0,
                title: w.title.clone(),
                minimized: w.minimized,
                icon: thumbs::icon_url(w.id.0),
                no_thumb: w.minimized || kind == Kind::None || listed(&paused, &name),
                other_desktop: other_desktop.contains(&w.id.0),
                app: name,
            }
        })
        .collect();
    let Some(mon) = app.monitor_from_point(button.x as f64, button.y as f64).ok().flatten() else { return };
    let sc = mon.scale_factor();
    let display = Rect { x: mon.position().x, y: mon.position().y, width: mon.size().width, height: mon.size().height };
    let mut grid = model::grid(cards.len(), model::thumb_size(&size), 0, (display.width as f64 / sc * 0.9, display.height as f64 / sc * 0.6));
    grid.height -= model::SEARCH - 32.0;
    let phys = ((grid.width * sc).round() as u32, (grid.height * sc).round() as u32);
    let (x, y) = place(button, bar, display, phys);
    let payload = OpenPayload {
        app: app_name,
        cards,
        grid,
        kind,
        minimal,
        close_quits: opt_bool(&s, PREVIEW, "closeQuits", false),
        opacity: opt_num(&s, PREVIEW, "opacity", 100.0),
    };
    let state = app.state::<TaskbarState>();
    state.thumbs.lock().unwrap().clear();
    *state.shown.lock().unwrap() = Some((Rect { x, y, width: phys.0, height: phys.1 }, key));
    let _ = app.emit_to(WINDOW, OPEN_EVENT, payload);
    if let Some(w) = app.get_webview_window(WINDOW) {
        let _ = w.set_size(tauri::PhysicalSize::new(phys.0, phys.1));
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
        #[cfg(windows)]
        super::show_inactive(&w);
        #[cfg(not(windows))]
        let _ = w.show();
    }
}

#[tauri::command]
pub fn taskbar_slots(app: AppHandle, slots: Vec<Slot>) {
    let Some(window) = app.get_webview_window(WINDOW) else { return };
    let state = app.state::<TaskbarState>();
    if state.shown.lock().unwrap().is_none() {
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    let due = state.thumbs.lock().unwrap().update(super::host_id(&window), scale, &slots);
    super::send_snapshots(&app, WINDOW, IMAGE_EVENT, &slots, &due, scale);
}

/// Half-screen and next-display targets for a window, from the work areas.
pub fn snap_target(action: &str, bounds: Rect, areas: &[Rect]) -> Option<Rect> {
    let cx = bounds.x + bounds.width as i32 / 2;
    let cy = bounds.y + bounds.height as i32 / 2;
    let i = areas
        .iter()
        .position(|a| cx >= a.x && cy >= a.y && cx < a.x + a.width as i32 && cy < a.y + a.height as i32)
        .unwrap_or(0);
    let a = *areas.get(i)?;
    let half = a.width / 2;
    Some(match action {
        "snapLeft" => Rect { width: half, ..a },
        "snapRight" => Rect { x: a.x + half as i32, width: a.width - half, ..a },
        "nextDisplay" => {
            let b = areas[(i + 1) % areas.len()];
            let (w, h) = (bounds.width.min(b.width), bounds.height.min(b.height));
            let dx = ((bounds.x - a.x) as f64 / a.width.max(1) as f64 * b.width as f64) as i32;
            let dy = ((bounds.y - a.y) as f64 / a.height.max(1) as f64 * b.height as f64) as i32;
            let x = (b.x + dx).clamp(b.x, b.x + (b.width - w) as i32);
            let y = (b.y + dy).clamp(b.y, b.y + (b.height - h) as i32);
            Rect { x, y, width: w, height: h }
        }
        _ => return None,
    })
}

/// Card actions: focus, close, minimise, maximise, snapLeft, snapRight, nextDisplay.
#[tauri::command]
pub fn taskbar_action(app: AppHandle, id: u64, action: String) {
    let platform = app.state::<Platform>();
    let wm = platform.windows();
    let wid = WindowId(id);
    match action.as_str() {
        "focus" => {
            hide(&app);
            let _ = wm.focus(wid);
        }
        "close" => {
            let quits = opt_bool(&settings(&app), PREVIEW, "closeQuits", false);
            let pid = wm.list().ok().and_then(|l| l.into_iter().find(|w| w.id == wid)).and_then(|w| w.pid);
            match (quits, pid) {
                (true, Some(pid)) => {
                    let _ = wm.quit_process(pid);
                }
                _ => {
                    let _ = wm.close(wid);
                }
            }
            hide(&app);
        }
        "minimise" => {
            let _ = wm.minimize(wid);
        }
        "maximise" => {
            let _ = wm.maximize(wid);
            hide(&app);
        }
        other => {
            let bounds = wm.list().ok().and_then(|l| l.into_iter().find(|w| w.id == wid)).map(|w| w.bounds);
            let areas: Vec<Rect> = wm.work_areas().map(|a| a.into_iter().map(|w| w.work).collect()).unwrap_or_default();
            if let Some(target) = bounds.and_then(|b| snap_target(other, b, &areas)) {
                let _ = wm.restore(wid);
                let _ = wm.set_bounds(wid, target);
                let _ = wm.focus(wid);
                hide(&app);
            }
        }
    }
}

#[cfg(windows)]
mod win {
    use std::collections::{HashMap, HashSet};
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};

    use notifi_platform::win_taskbar::{self, TaskbarButton};
    use notifi_platform::{Platform, WindowInfo};
    use tauri::{AppHandle, Manager};

    use super::super::hook_win::CLICKS;
    use super::super::{listed, opt_bool, opt_list, opt_num, opt_str, sources};
    use super::{settings, TaskbarState, CLICKS as CLICKS_NS, PREVIEW};

    fn key(b: &TaskbarButton) -> String {
        format!("{}|{}|{}", b.automation_id, b.rect.x, b.rect.y)
    }

    struct Ids(HashMap<u32, Option<String>>);

    impl Ids {
        fn aumid(&mut self, pid: Option<u32>) -> Option<String> {
            let pid = pid?;
            if self.0.len() > 512 {
                self.0.clear();
            }
            self.0.entry(pid).or_insert_with(|| win_taskbar::process_app_id(pid)).clone()
        }
    }

    fn windows_of(b: &TaskbarButton, list: &[WindowInfo], ids: &mut Ids) -> Vec<WindowInfo> {
        let me = std::process::id();
        list.iter()
            .filter(|w| w.pid != Some(me) && b.matches(w.app_id.as_deref(), ids.aumid(w.pid).as_deref()))
            .cloned()
            .collect()
    }

    /// Keeps an auto-hiding taskbar up while a preview is open, and puts auto-hide back afterwards.
    fn hold_taskbar(held: &mut bool, want: bool) {
        if want == *held {
            return;
        }
        if want {
            if win_taskbar::autohide() {
                win_taskbar::set_autohide(false);
                *held = true;
            }
        } else {
            win_taskbar::set_autohide(true);
            *held = false;
        }
    }

    fn click(app: &AppHandle, b: &TaskbarButton, ids: &mut Ids) {
        let platform = app.state::<Platform>();
        let wm = platform.windows();
        let Ok(list) = wm.list() else { return };
        let wins = windows_of(b, &list, ids);
        let action = opt_str(&settings(app), CLICKS_NS, "action", "minimise");
        if action == "cycle" && wins.len() >= 2 {
            if let Some(last) = wins.last() {
                let _ = wm.focus(last.id);
            }
            return;
        }
        for w in wins.iter().filter(|w| !w.minimized) {
            let _ = wm.minimize(w.id);
        }
    }

    pub fn start(app: &AppHandle) {
        let (tx, rx) = channel::<(i32, i32)>();
        CLICKS.lock().unwrap().tx = Some(tx);
        let app = app.clone();
        let _ = std::thread::Builder::new().name("taskbar".into()).spawn(move || {
            let mut buttons: Vec<TaskbarButton> = Vec::new();
            let mut fetched = Instant::now() - Duration::from_secs(60);
            let mut hovered: Option<(String, Instant)> = None;
            let mut left_at: Option<Instant> = None;
            let mut ids = Ids(HashMap::new());
            let mut held = false;
            loop {
                let s = settings(&app);
                let (preview, clicks) = (s.enabled(PREVIEW), s.enabled(CLICKS_NS));
                if !preview && !clicks {
                    CLICKS.lock().unwrap().target = None;
                    super::hide(&app);
                    hold_taskbar(&mut held, false);
                    std::thread::sleep(Duration::from_millis(400));
                    continue;
                }
                let Ok(p) = app.cursor_position() else {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                };
                let (x, y) = (p.x as i32, p.y as i32);
                let hit = if win_taskbar::is_over_taskbar(x, y) {
                    let stale = fetched.elapsed() > Duration::from_millis(1500)
                        || (win_taskbar::hit(&buttons, x, y).is_none() && fetched.elapsed() > Duration::from_millis(400));
                    if stale {
                        buttons = win_taskbar::buttons().unwrap_or_default();
                        fetched = Instant::now();
                    }
                    win_taskbar::hit(&buttons, x, y).cloned()
                } else {
                    None
                };

                let platform = app.state::<Platform>();
                let excluded = opt_list(&s, CLICKS_NS, "excludedApps");
                let target = hit.as_ref().filter(|_| clicks).map(|b| {
                    let fg = platform.windows().foreground().ok().flatten();
                    let active = fg.as_ref().is_some_and(|w| {
                        let name = super::super::app_name(w);
                        b.matches(w.app_id.as_deref(), ids.aumid(w.pid).as_deref())
                            && !listed(&excluded, &name)
                            && !w.app_id.as_deref().is_some_and(|a| listed(&excluded, a))
                    });
                    (b.rect, active)
                });
                CLICKS.lock().unwrap().target = target;
                while let Ok((cx, cy)) = rx.try_recv() {
                    if let Some(b) = win_taskbar::hit(&buttons, cx, cy).cloned() {
                        click(&app, &b, &mut ids);
                    }
                }

                if preview {
                    let state = app.state::<TaskbarState>();
                    let shown = state.shown.lock().unwrap().clone();
                    let over_preview = shown.as_ref().is_some_and(|(r, _)| notifi_platform::contains(*r, x, y));
                    match &hit {
                        Some(b) => {
                            left_at = None;
                            let k = key(b);
                            if hovered.as_ref().is_none_or(|(h, _)| *h != k) {
                                hovered = Some((k.clone(), Instant::now()));
                            }
                            let delay = if shown.is_some() { opt_num(&s, PREVIEW, "switchDelay", 100.0) } else { opt_num(&s, PREVIEW, "openDelay", 200.0) };
                            let since = hovered.as_ref().map(|(_, t)| t.elapsed()).unwrap_or_default();
                            if since >= Duration::from_millis(delay.max(0.0) as u64) && shown.as_ref().is_none_or(|(_, sk)| *sk != k) {
                                let all = sources::windows(&platform, false);
                                let other: HashSet<u64> = all.iter().filter(|(_, f)| f.other_desktop).map(|(w, _)| w.id.0).collect();
                                let show_other = opt_bool(&s, PREVIEW, "otherDesktops", true);
                                let list: Vec<WindowInfo> = all.into_iter().map(|(w, _)| w).filter(|w| show_other || !other.contains(&w.id.0)).collect();
                                let wins = windows_of(b, &list, &mut ids);
                                if wins.is_empty() {
                                    super::hide(&app);
                                } else {
                                    super::show(&app, k, b.app_name().to_string(), wins, &other, b.rect, b.taskbar);
                                }
                            }
                        }
                        None if over_preview => {
                            hovered = None;
                            left_at = None;
                        }
                        None => {
                            hovered = None;
                            if shown.is_some() {
                                let t = *left_at.get_or_insert_with(Instant::now);
                                if t.elapsed() > Duration::from_millis(350) {
                                    super::hide(&app);
                                    left_at = None;
                                }
                            }
                        }
                    }
                }
                let keep = preview && opt_bool(&s, PREVIEW, "keepTaskbarVisible", false) && app.state::<TaskbarState>().shown.lock().unwrap().is_some();
                hold_taskbar(&mut held, keep);
                std::thread::sleep(Duration::from_millis(50));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISPLAY: Rect = Rect { x: 0, y: 0, width: 1920, height: 1080 };

    #[test]
    fn places_by_taskbar_edge() {
        let bottom = Rect { x: 0, y: 1032, width: 1920, height: 48 };
        let button = Rect { x: 900, y: 1032, width: 44, height: 48 };
        assert_eq!(place(button, bottom, DISPLAY, (400, 200)), (722, 824));
        let corner = Rect { x: 0, y: 1032, width: 44, height: 48 };
        assert_eq!(place(corner, bottom, DISPLAY, (400, 200)).0, 0);
        let top = Rect { x: 0, y: 0, width: 1920, height: 48 };
        assert_eq!(place(Rect { y: 0, ..button }, top, DISPLAY, (400, 200)).1, 56);
        let left = Rect { x: 0, y: 0, width: 48, height: 1080 };
        assert_eq!(place(Rect { x: 0, y: 500, width: 48, height: 44 }, left, DISPLAY, (400, 200)), (56, 422));
    }

    #[test]
    fn snap_targets() {
        let areas = [Rect { x: 0, y: 0, width: 1920, height: 1040 }, Rect { x: 1920, y: 0, width: 1280, height: 1000 }];
        let win = Rect { x: 100, y: 100, width: 800, height: 600 };
        assert_eq!(snap_target("snapLeft", win, &areas), Some(Rect { x: 0, y: 0, width: 960, height: 1040 }));
        assert_eq!(snap_target("snapRight", win, &areas), Some(Rect { x: 960, y: 0, width: 960, height: 1040 }));
        let moved = snap_target("nextDisplay", win, &areas).unwrap();
        assert_eq!((moved.x, moved.width, moved.height), (1920 + 66, 800, 600));
        assert_eq!(snap_target("nope", win, &areas), None);
    }
}
