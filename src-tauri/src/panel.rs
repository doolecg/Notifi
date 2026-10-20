use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Mutex,
    },
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, Rect, State, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};

use notifi_platform::Platform;

use crate::{settings::Store, window};

pub const PANEL: &str = "panel";
const OPEN_EVENT: &str = "panel://open";
const CLOSE_EVENT: &str = "panel://close";

/// Card width plus the transparent margin that holds its shadow, in logical px.
const CARD_WIDTH: f64 = 360.0;
const MARGIN: f64 = 18.0;
const GAP: f64 = 6.0;
/// A tray click that lands this soon after a blur-hide is the same click; don't reopen.
const REOPEN_GUARD: Duration = Duration::from_millis(300);

/// Where the panel hangs from, in physical pixels.
#[derive(Clone, Copy, Debug)]
pub enum Anchor {
    Rect { x: f64, y: f64, w: f64, h: f64 },
    Point { x: f64, y: f64 },
    /// Top-right of the screen with the pointer.
    Pointer,
}

impl Anchor {
    pub fn from_rect(rect: Rect) -> Self {
        let pos = rect.position.to_physical::<f64>(1.0);
        let size = rect.size.to_physical::<f64>(1.0);
        if size.width > 0.0 && size.height > 0.0 {
            Anchor::Rect { x: pos.x, y: pos.y, w: size.width, h: size.height }
        } else {
            Anchor::Point { x: pos.x, y: pos.y }
        }
    }
}

pub struct PanelState {
    visible: AtomicBool,
    anchor: Mutex<Anchor>,
    /// Natural card height reported by the page, logical px.
    height: Mutex<f64>,
    hidden_at: Mutex<Option<Instant>>,
    tab: Mutex<String>,
    wake: Mutex<Sender<()>>,
    wake_rx: Mutex<Option<Receiver<()>>>,
}

impl Default for PanelState {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            visible: AtomicBool::new(false),
            anchor: Mutex::new(Anchor::Pointer),
            height: Mutex::new(420.0),
            hidden_at: Mutex::new(None),
            tab: Mutex::new(String::new()),
            wake: Mutex::new(tx),
            wake_rx: Mutex::new(Some(rx)),
        }
    }
}

impl PanelState {
    pub fn visible(&self) -> bool {
        self.visible.load(Ordering::SeqCst)
    }

    pub fn tab(&self) -> String {
        self.tab.lock().unwrap().clone()
    }

    /// The stats sampler's wake-up channel; taken once.
    pub fn take_wake(&self) -> Option<Receiver<()>> {
        self.wake_rx.lock().unwrap().take()
    }

    fn wake(&self) {
        let _ = self.wake.lock().unwrap().send(());
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OpenPayload {
    tab: Option<String>,
    has_battery: bool,
    visible: bool,
}

fn has_battery(app: &AppHandle) -> bool {
    app.state::<Platform>().sensors().battery().ok().flatten().is_some()
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, PANEL, WebviewUrl::App("panel.html".into()))
        .title("Notifi")
        .inner_size(CARD_WIDTH + 2.0 * MARGIN, 420.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .visible(false)
        .focused(false)
        .build()?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            if !window::no_focus() {
                hide(&handle);
            }
        }
    });
    Ok(())
}

pub fn toggle(app: &AppHandle, anchor: Anchor) {
    let state = app.state::<PanelState>();
    if state.visible() {
        hide(app);
        return;
    }
    let recently_hidden = state.hidden_at.lock().unwrap().is_some_and(|t| t.elapsed() < REOPEN_GUARD);
    if !recently_hidden {
        show(app, anchor, None);
    }
}

/// Opens the panel by the tray icon when its position is known, otherwise by the pointer.
pub fn show_by_tray(app: &AppHandle, tab: Option<String>) {
    let anchor = app
        .tray_by_id(crate::tray::TRAY_ID)
        .and_then(|t| t.rect().ok().flatten())
        .map(Anchor::from_rect)
        .unwrap_or(Anchor::Pointer);
    show(app, anchor, tab);
}

pub fn show(app: &AppHandle, anchor: Anchor, tab: Option<String>) {
    let Some(window) = app.get_webview_window(PANEL) else { return };
    let state = app.state::<PanelState>();
    if let Some(tab) = tab.clone() {
        // Also lets a page that is still loading pick the tab up from the settings.
        let _ = app.state::<Store>().update(app, |s| {
            s.panel.last_tab = tab;
            Ok(())
        });
    }
    *state.anchor.lock().unwrap() = anchor;
    place(app);
    let _ = window.show();
    if !window::no_focus() {
        let _ = window.set_focus();
    }
    state.visible.store(true, Ordering::SeqCst);
    state.wake();
    let _ = app.emit_to(PANEL, OPEN_EVENT, OpenPayload { tab, has_battery: has_battery(app), visible: true });
}

/// Starts the close animation; the page calls `panel_closed` when it ends.
pub fn hide(app: &AppHandle) {
    let state = app.state::<PanelState>();
    if !state.visible.swap(false, Ordering::SeqCst) {
        return;
    }
    *state.hidden_at.lock().unwrap() = Some(Instant::now());
    let _ = app.emit_to(PANEL, CLOSE_EVENT, ());
    // In case the page never answers.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        finish_hide(&app);
    });
}

fn finish_hide(app: &AppHandle) {
    if app.state::<PanelState>().visible() {
        return;
    }
    if let Some(window) = app.get_webview_window(PANEL) {
        let _ = window.hide();
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Area {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl Area {
    fn of(monitor: &Monitor) -> (Self, Self) {
        let (p, s) = (monitor.position(), monitor.size());
        let screen = Area { left: p.x as f64, top: p.y as f64, right: (p.x + s.width as i32) as f64, bottom: (p.y + s.height as i32) as f64 };
        let w = monitor.work_area();
        let work = Area {
            left: w.position.x as f64,
            top: w.position.y as f64,
            right: (w.position.x + w.size.width as i32) as f64,
            bottom: (w.position.y + w.size.height as i32) as f64,
        };
        (screen, work)
    }
}

/// The screen edge the tray sits on: where the taskbar takes space, else the edge nearest the anchor.
fn tray_edge(screen: Area, work: Area, cx: f64, cy: f64) -> Edge {
    if work.bottom < screen.bottom {
        return Edge::Bottom;
    }
    if work.top > screen.top {
        return Edge::Top;
    }
    if work.left > screen.left {
        return Edge::Left;
    }
    if work.right < screen.right {
        return Edge::Right;
    }
    let distances = [
        (cy - screen.top, Edge::Top),
        (screen.bottom - cy, Edge::Bottom),
        (cx - screen.left, Edge::Left),
        (screen.right - cx, Edge::Right),
    ];
    distances.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).map(|d| d.1).unwrap_or(Edge::Bottom)
}

/// Card origin (physical px) for a card of `w`x`h` hanging off `anchor`.
fn card_origin(anchor: Anchor, screen: Area, work: Area, w: f64, h: f64, gap: f64) -> (f64, f64) {
    let clamp_x = |x: f64| x.clamp(work.left + gap, (work.right - w - gap).max(work.left + gap));
    let clamp_y = |y: f64| y.clamp(work.top + gap, (work.bottom - h - gap).max(work.top + gap));
    let (left, top, right, bottom) = match anchor {
        Anchor::Rect { x, y, w, h } => (x, y, x + w, y + h),
        Anchor::Point { x, y } => (x, y, x, y),
        Anchor::Pointer => return (work.right - w - gap, work.top + gap),
    };
    let (cx, cy) = ((left + right) / 2.0, (top + bottom) / 2.0);
    match tray_edge(screen, work, cx, cy) {
        Edge::Bottom => (clamp_x(cx - w / 2.0), clamp_y(top.min(work.bottom) - gap - h)),
        Edge::Top => (clamp_x(cx - w / 2.0), clamp_y(bottom.max(work.top) + gap)),
        Edge::Left => (clamp_x(right.max(work.left) + gap), clamp_y(cy - h / 2.0)),
        Edge::Right => (clamp_x(left.min(work.right) - gap - w), clamp_y(cy - h / 2.0)),
    }
}

fn place(app: &AppHandle) {
    let Some(window) = app.get_webview_window(PANEL) else { return };
    let state = app.state::<PanelState>();
    let anchor = *state.anchor.lock().unwrap();
    let (px, py) = match anchor {
        Anchor::Rect { x, y, w, h } => (x + w / 2.0, y + h / 2.0),
        Anchor::Point { x, y } => (x, y),
        Anchor::Pointer => app.cursor_position().map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0)),
    };
    let monitor = app
        .monitor_from_point(px, py)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let scale = monitor.scale_factor();
    let (screen, work) = Area::of(&monitor);
    let max_card = ((screen.bottom - screen.top) * 0.8).min(work.bottom - work.top - 2.0 * GAP * scale);
    let card_w = CARD_WIDTH * scale;
    let card_h = (*state.height.lock().unwrap() * scale).min(max_card).round();
    let (x, y) = card_origin(anchor, screen, work, card_w, card_h, GAP * scale);
    let margin = MARGIN * scale;
    let _ = window.set_size(PhysicalSize::new((card_w + 2.0 * margin).round() as u32, (card_h + 2.0 * margin) as u32));
    let _ = window.set_position(PhysicalPosition::new((x - margin).round() as i32, (y - margin).round() as i32));
}

/// The page reports its natural card height whenever the content changes size.
#[tauri::command]
pub fn panel_resize(app: AppHandle, state: State<PanelState>, height: f64) {
    let height = height.clamp(120.0, 4000.0);
    let changed = {
        let mut h = state.height.lock().unwrap();
        let changed = (*h - height).abs() >= 1.0;
        *h = height;
        changed
    };
    if changed {
        place(&app);
    }
}

/// For a page that loads after `show` already fired its open event.
#[tauri::command]
pub fn panel_status(app: AppHandle, state: State<PanelState>) -> OpenPayload {
    OpenPayload { tab: None, has_battery: has_battery(&app), visible: state.visible() }
}

#[tauri::command]
pub fn panel_hide(app: AppHandle) {
    hide(&app);
}

#[tauri::command]
pub fn panel_closed(app: AppHandle) {
    finish_hide(&app);
}

/// The tab on screen: the sampler only reads what it shows, and reopening starts on it.
#[tauri::command]
pub fn panel_set_tab(app: AppHandle, state: State<PanelState>, store: State<Store>, tab: String) {
    *state.tab.lock().unwrap() = tab.clone();
    state.wake();
    if store.get().panel.last_tab != tab {
        let _ = store.update(&app, |s| {
            s.panel.last_tab = tab;
            Ok(())
        });
    }
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Area = Area { left: 0.0, top: 0.0, right: 1920.0, bottom: 1080.0 };

    #[test]
    fn bottom_taskbar_hangs_above_icon() {
        let work = Area { bottom: 1032.0, ..SCREEN };
        let anchor = Anchor::Rect { x: 1700.0, y: 1040.0, w: 24.0, h: 32.0 };
        let (x, y) = card_origin(anchor, SCREEN, work, 360.0, 500.0, 6.0);
        assert_eq!(y, 1032.0 - 6.0 - 500.0);
        assert_eq!(x, 1712.0 - 180.0);
    }

    #[test]
    fn clamps_inside_the_work_area() {
        let work = Area { bottom: 1032.0, ..SCREEN };
        let anchor = Anchor::Rect { x: 1890.0, y: 1040.0, w: 24.0, h: 32.0 };
        let (x, _) = card_origin(anchor, SCREEN, work, 360.0, 500.0, 6.0);
        assert_eq!(x, 1920.0 - 360.0 - 6.0);
    }

    #[test]
    fn handles_every_taskbar_edge() {
        let top = Area { top: 40.0, ..SCREEN };
        let (_, y) = card_origin(Anchor::Rect { x: 1800.0, y: 4.0, w: 24.0, h: 32.0 }, SCREEN, top, 360.0, 400.0, 6.0);
        assert_eq!(y, 46.0);
        let left = Area { left: 48.0, ..SCREEN };
        let (x, _) = card_origin(Anchor::Rect { x: 8.0, y: 900.0, w: 32.0, h: 24.0 }, SCREEN, left, 360.0, 400.0, 6.0);
        assert_eq!(x, 54.0);
        let right = Area { right: 1872.0, ..SCREEN };
        let (x, _) = card_origin(Anchor::Rect { x: 1880.0, y: 900.0, w: 32.0, h: 24.0 }, SCREEN, right, 360.0, 400.0, 6.0);
        assert_eq!(x, 1872.0 - 6.0 - 360.0);
    }

    #[test]
    fn point_near_top_opens_below_and_pointer_goes_top_right() {
        let (_, y) = card_origin(Anchor::Point { x: 1500.0, y: 10.0 }, SCREEN, SCREEN, 360.0, 400.0, 6.0);
        assert_eq!(y, 16.0);
        let (x, y) = card_origin(Anchor::Pointer, SCREEN, SCREEN, 360.0, 400.0, 6.0);
        assert_eq!((x, y), (1920.0 - 366.0, 6.0));
    }
}
