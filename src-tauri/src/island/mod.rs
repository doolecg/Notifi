//! The Island (spec/everyday/island.md): a black pill hanging from the top centre of a display.
//! A worker thread keeps the timer, media, agent and control readings and pushes one snapshot
//! to the page whenever something changes. The window is resized to the visible shape, so the
//! transparent area around a resting pill never blocks clicks.

pub mod agents;
pub mod claude_status;
pub mod click;
pub mod config;
pub mod dynamic;
pub mod files;
pub mod idle;
pub mod layout;
pub mod lock;
pub mod opencode;
pub mod lyrics;
pub mod terminals;
pub mod timer;
pub mod usage;
pub mod watch;
pub mod weather;

use std::{
    sync::{
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use notifi_platform::{DeviceDirection, MediaSession, NowPlaying, PlaybackState, Platform};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::{settings::Store, window};
use agents::{AgentSnap, Tracker};
use config::{IslandConfig, MODULES};
use layout::Layout;
use timer::{Phase, Timer};

pub const ISLAND: &str = "island";
/// The AI Agents bar kept on the taskbar while the Island hangs from the top.
pub const BAR: &str = "agentsbar";
const BAR_W: f64 = 640.0;
/// The bar window's one size (like the Island's, never resized): the widest bar, and room above the taskbar
/// for three rows of session buttons. Matches BAR_MAX and ROOM in agentsbar/AgentsBar.svelte.
const BAR_MAX_W: f64 = 1000.0;
const BAR_ROOM: f64 = 120.0;
const STATE_EVENT: &str = "island://state";
const COVER_EVENT: &str = "island://cover";
const ACTIVITY_EVENT: &str = "island://activity";
const COMMAND_EVENT: &str = "island://command";

const TICK: Duration = Duration::from_millis(1000);
const AGENTS_BUSY: Duration = Duration::from_secs(2);
const AGENTS_IDLE: Duration = Duration::from_secs(15);
const BUDGET_EVERY: Duration = Duration::from_secs(30);

/// Minutes the local clock is ahead of UTC.
pub(crate) fn local_offset_min() -> i32 {
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
        let (l, u) = unsafe { (GetLocalTime(), GetSystemTime()) };
        let d = (i32::from(l.wHour) * 60 + i32::from(l.wMinute)) - (i32::from(u.wHour) * 60 + i32::from(u.wMinute));
        return if d > 720 { d - 1440 } else if d <= -720 { d + 1440 } else { d };
    }
    #[cfg(not(windows))]
    {
        0
    }
}
/// How often the dashboard history is brought up to date, and saved at most.
const USAGE_EVERY: Duration = Duration::from_secs(20);
const USAGE_SAVE_EVERY: Duration = Duration::from_secs(60);
const FULLSCREEN_CHECK: Duration = Duration::from_secs(2);
const MAX_COVER: usize = 2 * 1024 * 1024;

/// Activity colours: Notifi's own palette.
const AGENT_COLOR: &str = "#f0a24a";
const MUSIC_COLOR: &str = "#2cc9b2";
const TIMER_COLOR: &str = "#8fb3ff";
/// Provider colours, as each tool presents itself.
const CLAUDE_COLOR: &str = "#d97757";
const CODEX_COLOR: &str = "#7b8ceb";

pub struct IslandState {
    inner: Mutex<Inner>,
    wake: Mutex<Sender<()>>,
    wake_rx: Mutex<Option<Receiver<()>>>,
    usage: Arc<Mutex<usage::History>>,
    /// Delivery and ride cards read from Windows notifications.
    orders: Mutex<dynamic::Orders>,
    /// The latest weather for the city in settings, and why there is none when there isn't.
    weather: Mutex<WeatherState>,
}

#[derive(Default, Clone)]
struct WeatherState {
    now: Option<weather::Weather>,
    error: Option<String>,
}

#[derive(Default)]
struct Inner {
    timer: Timer,
    open: bool,
    /// Logical size of the resting shape the page asked for.
    rest: (f64, f64),
    visible: bool,
    bar_visible: bool,
    /// Logical width the AI Agents bar asked for; 0 until it asks.
    bar_w: f64,
    /// Logical height the bar's session buttons take above the taskbar while shown.
    bar_extra: f64,
    /// Taskbar height (logical px) the bar fills.
    bar_h: f64,
    /// The bar window's last physical rect, so it is only moved when that changes.
    bar_placed: Option<(i32, i32, u32, u32)>,
    fullscreen: bool,
    /// The session is locked.
    locked: bool,
    cover: Option<String>,
    last: Option<Value>,
    /// Docked and open: the menu is stretched up to its tallest size.
    tall: bool,
    /// Open on the AI chat: the menu has the chat's size.
    chat: bool,
    /// The window's size and the menu sizes, for the pointer check.
    geo: Geo,
    /// The window's last physical rect, so it is only moved when that changes.
    placed: Option<(i32, i32, u32, u32)>,
    /// The Windows Start button's left edge, logical px from the Island's screen's left; read every few seconds.
    start_x: Option<f64>,
}

impl Default for IslandState {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self { inner: Mutex::new(Inner { rest: (200.0, 32.0), ..Inner::default() }), wake: Mutex::new(tx), wake_rx: Mutex::new(Some(rx)), usage: Arc::default(), orders: Mutex::default(), weather: Mutex::default() }
    }
}

impl IslandState {
    fn wake(&self) {
        let _ = self.wake.lock().unwrap().send(());
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

pub fn config(app: &AppHandle) -> IslandConfig {
    let settings = serde_json::to_value(app.state::<Store>().get()).unwrap_or_default();
    IslandConfig::from_settings(&settings)
}

#[derive(Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModuleInfo {
    id: &'static str,
    label: &'static str,
    ready: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaSnap {
    title: String,
    artist: String,
    album: String,
    app: String,
    app_id: String,
    playing: bool,
    position_ms: u64,
    duration_ms: Option<u64>,
    can_play_pause: bool,
    can_prev: bool,
    can_next: bool,
    can_seek: bool,
    next_title: Option<String>,
}

impl From<&NowPlaying> for MediaSnap {
    fn from(n: &NowPlaying) -> Self {
        Self {
            title: n.title.clone(),
            artist: n.artist.clone(),
            album: n.album.clone(),
            app: n.session.name.clone(),
            app_id: n.session.id.clone(),
            playing: n.state == PlaybackState::Playing,
            position_ms: n.position_ms,
            duration_ms: n.duration_ms,
            can_play_pause: n.can_play_pause,
            can_prev: n.can_prev,
            can_next: n.can_next,
            can_seek: n.can_seek,
            next_title: n.next_title.clone(),
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimerSnap {
    #[serde(flatten)]
    timer: Timer,
    left_ms: u64,
}

/// A long-running thing shown on the ends of the resting pill.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Live {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub color: &'static str,
    /// Text for the right end; empty when the end draws something else (the equaliser).
    pub value: String,
    /// "running", "done" or "needs" for agents; empty otherwise.
    pub status: &'static str,
    /// Sessions behind an agent entry; 0 otherwise.
    pub count: u32,
    /// (done, total) of the lead agent's to-do list.
    pub todos: Option<(u32, u32)>,
    /// Tokens the lead agent has used (read plus written).
    pub tokens: u64,
    /// Percent used of the 5 hour and weekly limits, when the tool reports them.
    pub session_pct: Option<f64>,
    pub week_pct: Option<f64>,
    /// Project folder of the agent session shown, for jumping to its window.
    pub project: Option<String>,
    /// Terminals open for the agent shown ("sessions" on the pill).
    pub terminals: u32,
    /// Subagents running for the agent shown.
    pub subagents: u32,
    /// 0..=1 for an order or ride card.
    pub progress: Option<f64>,
    /// The stage of an order or ride card.
    pub detail: Option<String>,
}

/// `m:ss`, or `h:mm:ss` from an hour up.
pub fn clock(ms: u64) -> String {
    let s = ms / 1000;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Live activities in the order the ends prefer them: a working agent, a timer, then music.
pub fn live(playing: bool, timer: &Timer, hide_countdown: bool, agents: &[AgentSnap], now: u64) -> Vec<Live> {
    let mut out = Vec::new();
    let count: u32 = agents.iter().map(|a| a.count).sum();
    let lead = agents.iter().filter(|a| a.working).max_by_key(|a| a.elapsed_ms).or_else(|| agents.iter().find(|a| a.done));
    let look = |a: &AgentSnap| match a.id {
        agents::Kind::Claude => ("claude", CLAUDE_COLOR),
        agents::Kind::Codex => ("codex", CODEX_COLOR),
        agents::Kind::OpenCode => ("opencode", AGENT_COLOR),
        agents::Kind::Copilot => ("copilot", AGENT_COLOR),
    };
    if let Some(a) = lead {
        let (icon, color) = look(a);
        let status = if agents.iter().any(|a| a.needs_you) {
            "needs"
        } else if a.working {
            "running"
        } else {
            "done"
        };
        let value = if a.working { clock(a.elapsed_ms) } else { String::new() };
        out.push(Live { id: "agents", label: "AI Agents", icon, color, value, status, count, todos: a.todos, tokens: a.usage.read + a.usage.written, session_pct: a.session_limit.map(|l| l.used_percent), week_pct: a.week_limit.map(|l| l.used_percent), project: a.project.clone(), terminals: a.terminals, subagents: a.subagents, progress: None, detail: None });
    }
    if matches!(timer.phase, Phase::Running | Phase::Paused) {
        let value = if hide_countdown { String::new() } else { clock(timer.remaining(now).div_ceil(1000) * 1000) };
        out.push(Live { id: "timer", label: "Timer", icon: "timer", color: TIMER_COLOR, value, status: "", count: 0, todos: None, tokens: 0, session_pct: None, week_pct: None, project: None, terminals: 0, subagents: 0, progress: None, detail: None });
    }
    if playing {
        out.push(Live { id: "music", label: "Music", icon: "music", color: MUSIC_COLOR, value: String::new(), status: "", count: 0, todos: None, tokens: 0, session_pct: None, week_pct: None, project: None, terminals: 0, subagents: 0, progress: None, detail: None });
    }
    // With nothing running, the agent that was used last still shows its limits, after the other items.
    if lead.is_none() {
        if let Some(a) = agents.iter().filter(|a| a.supported && a.last_active > 0).max_by_key(|a| a.last_active) {
            let (icon, color) = look(a);
            out.push(Live { id: "agents", label: "AI Agents", icon, color, value: String::new(), status: "idle", count: 0, todos: a.todos, tokens: a.usage.read + a.usage.written, session_pct: a.session_limit.map(|l| l.used_percent), week_pct: a.week_limit.map(|l| l.used_percent), project: a.project.clone(), terminals: a.terminals, subagents: a.subagents, progress: None, detail: None });
        }
    }
    out
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    enabled: bool,
    config: IslandConfig,
    layout: Layout,
    modules: Vec<ModuleInfo>,
    media: Option<MediaSnap>,
    players: Vec<MediaSession>,
    timer: TimerSnap,
    agents: Vec<AgentSnap>,
    /// Sessions worth a button on the taskbar bar: working, waiting on you, or just finished.
    sessions: Vec<agents::SessionChip>,
    live: Vec<Live>,
    idle: idle::Idle,
    /// 0..=1, `None` where the platform can't read it.
    volume: Option<f32>,
    brightness: Option<u8>,
    /// False when the platform has no media session API.
    media_supported: bool,
    open: bool,
    /// The text read from the watched screen area, for the resting pill; `None` when nothing is watched or it is not shown.
    watch: Option<watch::Reading>,
    /// The weather for the city in settings; `None` with no city, or while it is being fetched.
    weather: Option<weather::Weather>,
    /// Why there is no weather (an unknown city, no network).
    weather_error: Option<String>,
    /// An AI app (T3 Code, a chat app, a terminal showing an agent) is in front, so the taskbar shows its buttons.
    ai_app: bool,
}

/// A window that takes typed prompts: an AI chat app, or a terminal whose title names an agent.
/// Editors and browsers don't count: typing a command there would land in a file or a page.
fn ai_window(app_id: Option<&str>, title: &str) -> bool {
    const APPS: &[&str] = &["t3 code", "t3code", "claude", "chatgpt", "codex"];
    const TERMINALS: &[&str] = &["windowsterminal", "wt", "powershell", "pwsh", "cmd", "conhost", "alacritty", "wezterm", "mintty", "kitty", "gnome-terminal", "konsole"];
    let app = app_id.unwrap_or("").to_ascii_lowercase();
    let app = app.trim_end_matches(".exe");
    let title = title.to_ascii_lowercase();
    APPS.iter().any(|a| app.starts_with(a))
        || (TERMINALS.contains(&app) && (title.contains('\u{2733}') || ["claude", "codex", "opencode", "copilot"].iter().any(|k| title.contains(k))))
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Activity {
    icon: &'static str,
    color: &'static str,
    text: String,
    ms: u32,
    /// Held back by Do Not Disturb: kept in the list of past notifications, not shown.
    quiet: bool,
    /// Where clicking it in the list of past notifications goes: `module:<id>`, `apps:<view>` or `settings:<section>`.
    target: Option<&'static str>,
}

/// Top-left and size (physical px) for a box `w` x `h` logical px at the top centre of a monitor.
pub fn top_centre(mon: (i32, i32, u32, u32), scale: f64, w: f64, h: f64) -> (i32, i32, u32, u32) {
    let (pw, ph) = ((w * scale).round(), (h * scale).round());
    let x = mon.0 as f64 + (mon.2 as f64 - pw) / 2.0;
    (x.round() as i32, mon.1, pw.max(1.0) as u32, ph.max(1.0) as u32)
}

/// Top-left and size (physical px) for a box `w` x `h` logical px at the bottom-left of a monitor, its bottom edge at `bottom`.
pub fn bottom_left(mon: (i32, i32, u32, u32), bottom: i32, scale: f64, w: f64, h: f64) -> (i32, i32, u32, u32) {
    let (pw, ph) = ((w * scale).round(), (h * scale).round());
    let x = mon.0 as f64 + (layout::DOCK_LEFT * scale).round();
    let y = f64::from(bottom) - ph;
    (x.round() as i32, y.round() as i32, pw.max(1.0) as u32, ph.max(1.0) as u32)
}

/// Space kept between the bar on the taskbar and the Start button.
const START_GAP: f64 = 12.0;

/// Top-left and size (physical px) for a box `w` x `h` logical px centred on a monitor, its bottom edge at `bottom`.
pub fn bottom_centre(mon: (i32, i32, u32, u32), bottom: i32, scale: f64, w: f64, h: f64) -> (i32, i32, u32, u32) {
    let (x, _, pw, ph) = top_centre(mon, scale, w, h);
    (x, bottom - ph as i32, pw, ph)
}

/// Whether the Start button is at the taskbar's far left (a left-aligned taskbar); `start` is its left edge in logical px.
fn start_is_left(start: Option<f64>, screen_w: f64) -> bool {
    start.is_some_and(|x| x < screen_w * 0.25)
}

/// The Island's size and where it sits, kept inside the screen it is on. `start` is the Start button's
/// left edge (logical px from the screen's left), when known.
fn layout_for(c: &IslandConfig, tall: bool, chat: bool, screen: (f64, f64), start: Option<f64>) -> Layout {
    // With Start at the taskbar's far left there is no room for the bar beside it, so the docked Island
    // becomes the top Island turned upside down, at the bottom centre.
    let start_left = start_is_left(start, screen.0);
    let docked = c.dock == "bottomLeft";
    let mut l = if docked && !start_left {
        layout::layout_dock(&c.size, c.custom_width, c.custom_height, tall, chat, screen)
    } else {
        layout::layout(&c.size, c.custom_width, c.custom_height, tall, chat, screen)
    };
    l.rise = docked && start_left;
    l.bar_max = start.filter(|_| !start_left).map(|x| (x - START_GAP - layout::DOCK_LEFT).max(240.0));
    l
}

/// Logical size of the screen the Island is on; a common size when it cannot be read.
fn screen_of(mon: Option<&tauri::Monitor>) -> (f64, f64) {
    mon.map_or((1920.0, 1080.0), |m| (f64::from(m.size().width) / m.scale_factor(), f64::from(m.size().height) / m.scale_factor()))
}

fn monitor(app: &AppHandle, c: &IslandConfig) -> Option<tauri::Monitor> {
    let primary = || app.primary_monitor().ok().flatten();
    match c.display.as_str() {
        "primary" => primary(),
        "pointer" => app
            .cursor_position()
            .ok()
            .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
            .or_else(primary),
        name => app
            .available_monitors()
            .ok()
            .and_then(|ms| ms.into_iter().find(|m| m.name().is_some_and(|n| n == name)))
            .or_else(primary),
    }
}

fn mon_rect(m: &tauri::Monitor) -> (i32, i32, u32, u32) {
    (m.position().x, m.position().y, m.size().width, m.size().height)
}

/// The taskbar's rectangle in physical px (left, top, right, bottom).
#[cfg(windows)]
fn taskbar_rect() -> Option<(i32, i32, i32, i32)> {
    use windows::core::w;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowRect};
    let h = unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }.ok()?;
    let mut r = RECT::default();
    unsafe { GetWindowRect(h, &mut r) }.ok()?;
    Some((r.left, r.top, r.right, r.bottom))
}

#[cfg(not(windows))]
fn taskbar_rect() -> Option<(i32, i32, i32, i32)> {
    None
}

/// Top and bottom (physical px) of the band the docked island lives in: the taskbar when it runs along
/// this monitor's bottom, else a strip of the default bar height.
fn taskbar_band(mon: &tauri::Monitor) -> (i32, i32) {
    let (mx, my, mw, mh) = mon_rect(mon);
    let (bottom, height) = (my + mh as i32, (32.0 * mon.scale_factor()).round() as i32);
    taskbar_rect()
        .filter(|&(l, t, r, b)| l >= mx - 1 && r <= mx + mw as i32 + 1 && t >= my + mh as i32 / 2 && b <= bottom + 1 && r - l > mw as i32 / 2)
        .map_or((bottom - height, bottom), |(_, t, _, b)| (t, b))
}

/// Height in logical px of a normal window title bar on the island's display.
#[tauri::command]
pub fn island_bar_height(app: AppHandle) -> f64 {
    let c = config(&app);
    if c.dock == "bottomLeft" {
        if let Some(m) = monitor(&app, &c) {
            let (top, bottom) = taskbar_band(&m);
            return (f64::from(bottom - top) / m.scale_factor()).round();
        }
    }
    let scale = monitor(&app, &c).map_or(1.0, |m| m.scale_factor());
    #[cfg(windows)]
    {
        use windows::Win32::UI::HiDpi::GetSystemMetricsForDpi;
        use windows::Win32::UI::WindowsAndMessaging::{SM_CXPADDEDBORDER, SM_CYCAPTION, SM_CYSIZEFRAME};
        let dpi = (96.0 * scale).round() as u32;
        let px = unsafe {
            GetSystemMetricsForDpi(SM_CYCAPTION, dpi) + GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
        };
        return (f64::from(px) / scale).round();
    }
    #[cfg(not(windows))]
    {
        let _ = scale;
        32.0
    }
}

/// Moves and resizes in one step. Done as two steps, a centred pill jumps sideways for a frame every time the window changes width.
fn set_rect(win: &tauri::Window, x: i32, y: i32, w: u32, h: u32) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
        let Ok(hwnd) = win.hwnd() else { return };
        unsafe {
            let _ = SetWindowPos(hwnd, None, x, y, w as i32, h as i32, SWP_NOACTIVATE | SWP_NOZORDER);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = win.set_size(tauri::PhysicalSize::new(w, h));
        let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    }
}

/// The window's one size: the biggest menu, with room for the spring overshooting it. Resizing a see-through
/// WebView2 window flickers and shifts what it shows, so the window keeps this size while the Island is up and
/// lets clicks through wherever the pill or menu is not (see `start_hit`).
fn frame(l: &Layout, bar: f64) -> (f64, f64) {
    let most = (l.base.0.max(l.tall.0).max(l.chat.0), l.base.1.max(l.tall.1).max(l.chat.1));
    let (w, h) = if l.dock { layout::dock_window_size(most) } else { layout::window_size(most) };
    // Docked, the menu opens above the taskbar and the pill sits in it.
    // Rising from the bottom centre, it opens above the taskbar the same way.
    let h = if l.dock { h - layout::DOCK_GAP + bar + layout::FLY_GAP } else if l.rise { h + bar + layout::FLY_GAP } else { h };
    ((w * 1.04).round(), h + 12.0)
}

/// Where the window is and what it holds, kept for the pointer check.
#[derive(Clone, Copy, Default)]
struct Geo {
    dock: bool,
    /// At the bottom centre, growing up (the top Island upside down).
    rise: bool,
    frame: (f64, f64),
    /// Taskbar height (logical px) when docked.
    bar: f64,
    base: (f64, f64),
    tall: (f64, f64),
    chat: (f64, f64),
}

/// Extra room around the shape that still takes the pointer, so a fast move off the edge is still seen by the page.
const HIT_SLACK: f64 = 8.0;

/// The part of the window that takes the pointer now, as (x, y, w, h) in logical px from its top-left.
fn hit_rect(g: &Geo, open: bool, tall: bool, chat: bool, rest: (f64, f64)) -> (f64, f64, f64, f64) {
    let menu = if chat { g.chat } else if tall { g.tall } else { g.base };
    let (fw, fh) = g.frame;
    let s = HIT_SLACK;
    if g.dock {
        let pill = (0.0, fh - layout::DOCK_GAP - rest.1 - s, rest.0 + s, rest.1 + layout::DOCK_GAP + s);
        if !open {
            return pill;
        }
        // The flyout above the taskbar down to the pill in it.
        let top = fh - (menu.1 + layout::EDGE + g.bar + layout::FLY_GAP) - s;
        return (0.0, top.max(0.0), (menu.0 + layout::EDGE + s).max(pill.2), fh - top.max(0.0));
    }
    if g.rise {
        let pill = ((fw - rest.0) / 2.0 - s, fh - rest.1 - s, rest.0 + 2.0 * s, rest.1 + s);
        if !open {
            return pill;
        }
        // The flyout centred above the taskbar down to the pill in it.
        let top = (fh - (menu.1 + g.bar + layout::FLY_GAP) - s).max(0.0);
        return ((fw - menu.0) / 2.0 - s, top, menu.0 + 2.0 * s, fh - top);
    }
    let (w, h) = if open { menu } else { rest };
    ((fw - w) / 2.0 - s, 0.0, w + 2.0 * s, h + s)
}

fn place(app: &AppHandle) {
    let Some(win) = app.get_window(ISLAND) else { return };
    let c = config(app);
    let Some(mon) = monitor(app, &c) else { return };
    let state = app.state::<IslandState>();
    let start = state.inner.lock().unwrap().start_x;
    let l = layout_for(&c, false, false, screen_of(Some(&mon)), start);
    let scale = mon.scale_factor();
    let (top, bottom) = taskbar_band(&mon);
    let bar = if l.dock || l.rise { f64::from(bottom - top) / scale } else { 0.0 };
    let (w, h) = frame(&l, bar);
    let rect = if l.dock {
        bottom_left(mon_rect(&mon), bottom, scale, w, h)
    } else if l.rise {
        bottom_centre(mon_rect(&mon), bottom, scale, w, h)
    } else {
        top_centre(mon_rect(&mon), scale, w, h)
    };
    let moved = {
        let mut i = state.inner.lock().unwrap();
        i.geo = Geo { dock: l.dock, rise: l.rise, frame: (w, h), bar, base: l.base, tall: l.tall, chat: l.chat };
        std::mem::replace(&mut i.placed, Some(rect)) != Some(rect)
    };
    if moved {
        set_island_rect(app, &win, rect.0, rect.1, rect.2, rect.3);
    }
    #[cfg(windows)]
    if l.dock || l.rise {
        raise(&win);
    }
}

/// The chat sits at a fixed spot in the window, so when the window moves it is moved back the same amount and stays still on screen.
fn set_island_rect(app: &AppHandle, win: &tauri::Window, x: i32, y: i32, w: u32, h: u32) {
    let before = win.outer_position().ok();
    set_rect(win, x, y, w, h);
    if let Some(b) = before {
        crate::chat::shift(app, b.x - x, b.y - y);
    }
}

/// The AI Agents bar sits at the taskbar's left; its window also covers the room above for the session buttons.
fn place_bar(app: &AppHandle) {
    let Some(win) = app.get_window(BAR) else { return };
    let c = config(app);
    let Some(mon) = monitor(app, &c) else { return };
    let scale = mon.scale_factor();
    let (top, bottom) = taskbar_band(&mon);
    let bar = f64::from(bottom - top) / scale;
    let state = app.state::<IslandState>();
    let (start, bar_w) = {
        let i = state.inner.lock().unwrap();
        (i.start_x, i.bar_w)
    };
    // With the taskbar left-aligned the Start button is where the bar would sit, so the bar goes to the middle instead.
    let mut rect = bottom_left(mon_rect(&mon), bottom, scale, BAR_MAX_W, bar + BAR_ROOM);
    if start_is_left(start, screen_of(Some(&mon)).0) {
        let mon_r = mon_rect(&mon);
        rect.0 = (f64::from(mon_r.0) + (f64::from(mon_r.2) - bar_w * scale) / 2.0).round() as i32;
    }
    let moved = {
        let mut i = state.inner.lock().unwrap();
        i.bar_h = bar;
        std::mem::replace(&mut i.bar_placed, Some(rect)) != Some(rect)
    };
    if moved {
        set_rect(&win, rect.0, rect.1, rect.2, rect.3);
    }
    #[cfg(windows)]
    raise(&win);
}

fn set_bar_visible(app: &AppHandle, on: bool) {
    let Some(win) = app.get_webview_window(BAR) else { return };
    let state = app.state::<IslandState>();
    let changed = {
        let mut i = state.inner.lock().unwrap();
        let changed = i.bar_visible != on;
        i.bar_visible = on;
        changed
    };
    if !changed {
        return;
    }
    if on {
        place_bar(app);
        let _ = win.show();
    } else {
        let _ = win.hide();
    }
}

/// Builds the Island and agents bar windows that don't exist yet.
pub fn build_windows(app: &AppHandle) -> tauri::Result<()> {
    for (label, url, title, size) in [(ISLAND, "island.html", "Notifi Island", (220.0, 42.0)), (BAR, "agentsbar.html", "Notifi AI Agents", (BAR_W, 32.0))] {
        if app.get_window(label).is_some() {
            continue;
        }
        WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
            .title(title)
            .inner_size(size.0, size.1)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .resizable(false)
            .focused(false)
            .focusable(false)
            .visible(false)
            .build()?;
    }
    Ok(())
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    crate::monitor::notify::init(app);
    start(app);
    Ok(())
}

fn set_visible(app: &AppHandle, on: bool) {
    let Some(win) = app.get_window(ISLAND) else { return };
    let state = app.state::<IslandState>();
    let changed = {
        let mut i = state.inner.lock().unwrap();
        let changed = i.visible != on;
        i.visible = on;
        if !on {
            i.open = false;
        }
        changed
    };
    if !changed {
        return;
    }
    if on {
        place(app);
        let _ = win.show();
    } else {
        let _ = win.set_focusable(false);
        // The page keeps its menu open while hidden, so it would come back open but cut to the pill and out of reach.
        let _ = app.emit_to(ISLAND, COMMAND_EVENT, "close");
        let _ = win.hide();
    }
}

/// Whether Windows' Do Not Disturb is on, read at most every 3 seconds.
fn system_dnd() -> bool {
    static CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap();
    if let Some((at, on)) = *cache {
        if at.elapsed() < Duration::from_secs(3) {
            return on;
        }
    }
    let on = crate::monitor::notify::do_not_disturb();
    *cache = Some((Instant::now(), on));
    on
}

/// Every notification Notifi shows on the pill goes through here: a deep click, and nothing on screen
/// when quiet. Do Not Disturb still sends it to the Nook's list of past notifications; Privacy mode drops it.
fn activity(app: &AppHandle, c: &IslandConfig, icon: &'static str, color: &'static str, text: String) {
    activity_to(app, c, icon, color, text, None);
}

fn activity_to(app: &AppHandle, c: &IslandConfig, icon: &'static str, color: &'static str, text: String, target: Option<&'static str>) {
    if c.privacy_mode {
        return;
    }
    let held = system_dnd();
    if c.activity_sound && !held {
        click::notify();
    }
    let _ = app.emit_to(ISLAND, ACTIVITY_EVENT, Activity { icon, color, text, ms: c.activity_ms, quiet: held, target });
}

/// An alert from another feature. Shown on the Island while it is on screen, so no Windows toast is
/// needed; returns false when the caller should send a toast. Swallowed (true) while quiet.
pub fn deliver(app: &AppHandle, title: &str, body: &str, sound: bool) -> bool {
    let c = config(app);
    if c.privacy_mode {
        return true;
    }
    let held = system_dnd();
    if !(c.enabled && c.alerts_in_island && (held || app.state::<IslandState>().inner.lock().unwrap().visible)) {
        return held;
    }
    let text = if body.is_empty() { title.to_string() } else { format!("{title} \u{b7} {body}") };
    if sound && !held {
        click::notify();
    }
    let _ = app.emit_to(ISLAND, ACTIVITY_EVENT, Activity { icon: "bell", color: AGENT_COLOR, text, ms: c.activity_ms, quiet: held, target: Some("module:system") });
    true
}

/// Records that the session locked or unlocked and wakes the worker so the Island hides or returns.
pub(crate) fn set_locked(app: &AppHandle, locked: bool) {
    let state = app.state::<IslandState>();
    state.inner.lock().unwrap().locked = locked;
    state.wake();
}

/// How far down the screen the resting pill reaches, in logical pixels, when the Island is on and hangs from the top.
pub fn pill_bottom(app: &AppHandle) -> Option<f64> {
    let c = config(app);
    (c.enabled && c.dock == "top").then(|| app.state::<IslandState>().inner.lock().unwrap().rest.1)
}

/// A short notice on the pill from another feature; nothing shows while the Island is off.
pub fn notice(app: &AppHandle, text: String) {
    notice_to(app, text, None);
}

/// A notice that belongs to a feature: clicking it in the list of past notifications opens `target` (`module:<id>`, `apps:<view>` or `settings:<section>`).
pub fn notice_to(app: &AppHandle, text: String, target: Option<&'static str>) {
    let c = config(app);
    if c.enabled {
        activity_to(app, &c, "bell", AGENT_COLOR, text, target);
    }
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn volume(p: &Platform) -> Option<f32> {
    p.audio().default_device(DeviceDirection::Output).ok().flatten().map(|d| if d.muted { 0.0 } else { d.volume })
}

fn brightness(p: &Platform) -> Option<u8> {
    let display = p.display();
    tauri::async_runtime::block_on(async {
        let list = display.list().await.ok()?;
        let d = list.iter().find(|d| d.primary).or(list.first())?;
        display.brightness(&d.id).await.ok()
    })
}

fn fullscreen_app(app: &AppHandle, p: &Platform, c: &IslandConfig) -> bool {
    let Some(mon) = monitor(app, c) else { return false };
    let (x, y, width, height) = mon_rect(&mon);
    p.windows().foreground_fullscreen(notifi_platform::Rect { x, y, width, height }).unwrap_or(false)
}

/// Puts the island back above the taskbar, which jumps over other topmost windows when it is clicked.
#[cfg(windows)]
fn raise(win: &tauri::Window) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE};
    let Ok(h) = win.hwnd() else { return };
    unsafe {
        let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

#[cfg(windows)]
fn start_raise(app: &AppHandle) {
    let app = app.clone();
    std::thread::Builder::new()
        .name("notifi-island-raise".into())
        .spawn(move || {
          let mut tick = 0u32;
          loop {
            std::thread::sleep(Duration::from_millis(250));
            let state = app.state::<IslandState>();
            let (visible, bar) = {
                let i = state.inner.lock().unwrap();
                (i.visible, i.bar_visible)
            };
            // Where the Start button is (it moves as apps open and close, and with the taskbar's alignment), every 2 s.
            tick += 1;
            if tick % 8 == 1 && (visible || bar) {
                let c = config(&app);
                let start = monitor(&app, &c).zip(notifi_platform::win_taskbar::start_button()).map(|(m, r)| (f64::from(r.x - m.position().x) / m.scale_factor()).round());
                let changed = std::mem::replace(&mut state.inner.lock().unwrap().start_x, start) != start;
                if changed {
                    place(&app);
                    if bar {
                        place_bar(&app);
                    }
                    state.wake();
                }
            }
            if visible && config(&app).dock == "bottomLeft" {
                if let Some(win) = app.get_window(ISLAND) {
                    raise(&win);
                }
            }
            if bar {
                if let Some(win) = app.get_window(BAR) {
                    raise(&win);
                }
            }
          }
        })
        .expect("spawn island raise thread");
}

/// How often the weather is read, and how long to wait after a failed read.
const WEATHER_EVERY: Duration = Duration::from_secs(15 * 60);
const WEATHER_RETRY: Duration = Duration::from_secs(5 * 60);

/// Keeps the weather for the city in settings up to date. Nothing is sent while the city is empty.
fn start_weather(app: &AppHandle) {
    let app = app.clone();
    let _ = std::thread::Builder::new().name("notifi-weather".into()).spawn(move || {
        let (mut city, mut place, mut next) = (String::new(), None::<weather::Place>, Instant::now());
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let state = app.state::<IslandState>();
            let c = config(&app);
            let wanted = c.weather_city.trim().to_string();
            if !c.enabled || wanted.is_empty() {
                if !city.is_empty() || state.weather.lock().unwrap().error.is_some() {
                    *state.weather.lock().unwrap() = WeatherState::default();
                    (city, place) = (String::new(), None);
                    state.wake();
                }
                continue;
            }
            if wanted != city {
                (city, place, next) = (wanted, None, Instant::now());
            }
            if Instant::now() < next {
                continue;
            }
            let found = tauri::async_runtime::block_on(async {
                let at = match &place {
                    Some(p) => p.clone(),
                    None => weather::find_place(&city).await?,
                };
                weather::current(&at).await.map(|w| (at, w))
            });
            match found {
                Ok((at, w)) => {
                    place = Some(at);
                    next = Instant::now() + WEATHER_EVERY;
                    *state.weather.lock().unwrap() = WeatherState { now: Some(w), error: None };
                }
                Err(e) => {
                    next = Instant::now() + WEATHER_RETRY;
                    let mut s = state.weather.lock().unwrap();
                    s.error = Some(e);
                    // A city that is not found shows nothing; a failed refresh keeps the last weather.
                    if place.is_none() {
                        s.now = None;
                    }
                }
            }
            state.wake();
        }
    });
}

/// A quick AI button: the saved prompt goes on the clipboard (with any text already copied after it) and the AI chat opens, ready to paste.
#[tauri::command]
pub fn island_ai_prompt(app: AppHandle, index: usize) {
    if let Err(e) = ai_prompt(&app, index) {
        notice_to(&app, e, Some("module:chat"));
    }
}

fn ai_prompt(app: &AppHandle, index: usize) -> Result<(), String> {
    let c = config(app);
    if !c.chat_enabled {
        return Err("Turn on the AI chat button in the Island settings first.".into());
    }
    let prompt = c.ai_prompts.get(index).ok_or("That prompt is not in the list any more.")?;
    let copied = crate::clipboard::read_text().map(|t| t.trim().to_string()).filter(|t| !t.is_empty() && t.chars().count() <= 20_000);
    crate::clipboard::set_text(&match copied {
        Some(text) => format!("{prompt}\n\n{text}"),
        None => prompt.clone(),
    })?;
    open_chat(app);
    Ok(())
}

/// The part of the bar window that takes the pointer: the bar along its bottom, and the session buttons above it while shown.
fn bar_hit_rect(width: f64, bar: f64, extra: f64) -> (f64, f64, f64, f64) {
    let top = BAR_ROOM - extra - if extra > 0.0 { HIT_SLACK } else { 0.0 };
    (0.0, top.max(0.0), width + HIT_SLACK, bar + BAR_ROOM - top.max(0.0))
}

/// Whether the pointer is inside `rect` (logical px from the window's top-left).
fn pointer_in(app: &AppHandle, win: &tauri::Window, (rx, ry, rw, rh): (f64, f64, f64, f64)) -> Option<bool> {
    let (Ok(p), Ok(at), Ok(scale)) = (app.cursor_position(), win.outer_position(), win.scale_factor()) else { return None };
    let (x, y) = ((p.x - f64::from(at.x)) / scale, (p.y - f64::from(at.y)) / scale);
    Some(x >= rx && y >= ry && x < rx + rw && y < ry + rh)
}

/// Whether a mouse button is down. Clicks outside Notifi's windows go to other apps, so this is how an outside click is seen; only on Windows.
#[cfg(windows)]
fn mouse_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON};
    [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON].iter().any(|k| unsafe { GetAsyncKeyState(i32::from(k.0)) } as u16 & 0x8000 != 0)
}

#[cfg(not(windows))]
fn mouse_down() -> bool {
    false
}

/// The Island and the bar keep one window size each, so they let clicks through unless the pointer is over
/// what they show. Checked about 60 times a second; the pointer moving off a shape is still seen by the page first (HIT_SLACK).
fn start_hit(app: &AppHandle) {
    let app = app.clone();
    std::thread::Builder::new()
        .name("notifi-island-hit".into())
        .spawn(move || {
            let mut taking: [Option<bool>; 2] = [None, None];
            let mut bar_away = 0u32;
            let mut was_down = false;
            loop {
                std::thread::sleep(Duration::from_millis(16));
                let down = mouse_down();
                let pressed = down && !was_down;
                was_down = down;
                let mut over_any = false;
                let state = app.state::<IslandState>();
                let (visible, open, tall, chat, rest, geo, bar_visible, bar) = {
                    let i = state.inner.lock().unwrap();
                    (i.visible, i.open, i.tall, i.chat, i.rest, i.geo, i.bar_visible, bar_hit_rect(i.bar_w, i.bar_h, i.bar_extra))
                };
                for (n, label, shown, rect) in [(0, ISLAND, visible, hit_rect(&geo, open, tall, chat, rest)), (1, BAR, bar_visible, bar)] {
                    let Some(win) = app.get_window(label).filter(|_| shown) else {
                        taking[n] = None;
                        continue;
                    };
                    let Some(over) = pointer_in(&app, &win, rect) else {
                        over_any = true;
                        continue;
                    };
                    over_any |= over;
                    if taking[n] != Some(over) && win.set_ignore_cursor_events(!over).is_ok() {
                        taking[n] = Some(over);
                    }
                    // The bar's buttons stay up only while the pointer is near; tell the page if it missed the pointer leaving.
                    if n == 1 {
                        bar_away = if over { 0 } else { bar_away + 1 };
                        if bar_away == 20 && state.inner.lock().unwrap().bar_extra > 0.0 {
                            let _ = app.emit_to(BAR, "agentsbar://leave", ());
                        }
                    }
                }
                // A click anywhere else while the menu is open puts it away.
                if pressed && open && visible && !over_any {
                    let _ = app.emit_to(ISLAND, COMMAND_EVENT, "close");
                }
            }
        })
        .expect("spawn island hit thread");
}

fn start(app: &AppHandle) {
    let Some(wake) = app.state::<IslandState>().wake_rx.lock().unwrap().take() else { return };
    start_usage(app);
    watch::start(app);
    dynamic::start(app);
    start_weather(app);
    #[cfg(windows)]
    start_raise(app);
    start_hit(app);
    let app = app.clone();
    let platform = app.state::<Platform>().inner().clone();
    std::thread::Builder::new()
        .name("notifi-island".into())
        .spawn(move || run(app, platform, wake))
        .expect("spawn island thread");
}

/// Keeps the dashboard history current on its own thread, so a first read of months of logs
/// never holds up the Island.
fn start_usage(app: &AppHandle) {
    let history = app.state::<IslandState>().usage.clone();
    let cache = app.path().app_data_dir().ok().map(|d| d.join("agent-usage.json"));
    let app = app.clone();
    let _ = std::thread::Builder::new().name("notifi-agent-usage".into()).spawn(move || {
        if let Some(path) = &cache {
            *history.lock().unwrap() = usage::History::load(path);
        }
        let Some(home) = agents::home() else { return };
        let mut saved: Option<Instant> = None;
        loop {
            if config(&app).enabled {
                usage::scan(&history, &home);
                if let Some(path) = &cache {
                    let mut h = history.lock().unwrap();
                    if h.dirty() && saved.is_none_or(|t| t.elapsed() >= USAGE_SAVE_EVERY) {
                        let _ = h.save(path);
                        saved = Some(Instant::now());
                    }
                }
            }
            std::thread::sleep(USAGE_EVERY);
        }
    });
}

fn run(app: AppHandle, platform: Platform, wake: Receiver<()>) {
    let home = agents::home();
    let mut tracker = Tracker::default();
    let mut terminal_counter = terminals::Counter::new();
    let mut agent_snaps: Vec<AgentSnap> = Vec::new();
    let mut chips: Vec<agents::SessionChip> = Vec::new();
    let mut next_agents = Instant::now();
    let mut next_fullscreen = Instant::now();
    let mut track: Option<String> = None;
    let mut first_track = true;
    let mut preferred = String::new();
    let mut idle_sampler = idle::Sampler::new();
    let mut warned: std::collections::HashSet<(agents::Kind, u8)> = Default::default();
    let mut budget = (0i64, 0u8);
    let mut next_budget = Instant::now();
    let media_supported = platform.media().sessions().is_ok();

    loop {
        let c = config(&app);
        if !c.enabled {
            set_visible(&app, false);
            set_bar_visible(&app, false);
            app.state::<IslandState>().inner.lock().unwrap().last = None;
            if matches!(wake.recv_timeout(Duration::from_secs(5)), Err(RecvTimeoutError::Disconnected)) {
                return;
            }
            continue;
        }
        let now = now_ms();
        let state = app.state::<IslandState>();
        config::set_log_folders(&c.log_folders);

        if Instant::now() >= next_fullscreen {
            next_fullscreen = Instant::now() + FULLSCREEN_CHECK;
            let fs = c.hide_in_fullscreen && fullscreen_app(&app, &platform, &c);
            state.inner.lock().unwrap().fullscreen = fs;
        }
        let fullscreen = state.inner.lock().unwrap().fullscreen;
        let paused = lock::overlays_paused(&app.state::<Store>().get(), state.inner.lock().unwrap().locked);
        set_visible(&app, !fullscreen && !paused);
        set_bar_visible(&app, !fullscreen && !paused && c.agents_taskbar && c.dock == "top");

        let timer_done = state.inner.lock().unwrap().timer.tick(now);
        if timer_done && c.activity_timer {
            activity_to(&app, &c, "timer", TIMER_COLOR, "Timer done".into(), Some("module:timer"));
        }

        if media_supported {
            if c.preferred_player != preferred {
                preferred = c.preferred_player.clone();
                let pick = (!preferred.is_empty()).then_some(preferred.as_str());
                let _ = platform.media().set_current(pick);
            }
        }
        let np = if media_supported { platform.media().now_playing().ok().flatten() } else { None };
        let key = np.as_ref().map(|n| format!("{}\u{1f}{}\u{1f}{}", n.session.id, n.title, n.artist));
        if key != track {
            track = key;
            let cover = np.as_ref().and_then(|n| n.cover.as_ref()).filter(|c| c.bytes.len() <= MAX_COVER);
            let url = cover.map(|c| format!("data:{};base64,{}", c.mime, base64(&c.bytes)));
            state.inner.lock().unwrap().cover = url.clone();
            let _ = app.emit_to(ISLAND, COVER_EVENT, url);
            if let Some(n) = np.as_ref().filter(|n| !first_track && c.activity_track && !n.title.is_empty()) {
                let text = if n.artist.is_empty() { n.title.clone() } else { format!("{} \u{2013} {}", n.title, n.artist) };
                activity_to(&app, &c, "music", MUSIC_COLOR, text, Some("module:nowPlaying"));
            }
            first_track = false;
        }

        if let Some(home) = home.as_deref() {
            if Instant::now() >= next_agents {
                let finished = tracker.poll(home, now, u64::from(c.agent_finish_minutes) * 60_000);
                if c.activity_agent {
                    for f in finished.into_iter().filter(|f| config::follows(&c.agents_follow, f.kind)) {
                        let what = f.project.unwrap_or_else(|| f.kind.name().to_string());
                        activity_to(&app, &c, "agents", AGENT_COLOR, format!("Done \u{b7} {what} \u{b7} {}", clock(f.elapsed_ms)), Some("module:agents"));
                    }
                }
                next_agents = Instant::now() + if tracker.busy(now) { AGENTS_BUSY } else { AGENTS_IDLE };
            }
            agent_snaps = tracker.snapshot(home, now);
            agent_snaps.retain(|a| config::follows(&c.agents_follow, a.id));
            chips = tracker.chips(home, now);
            chips.retain(|s| config::follows(&c.agents_follow, s.agent));
            // A Claude session that T3 Code runs shows under its chat's title and opens that chat.
            for chip in chips.iter_mut().filter(|s| s.agent == agents::Kind::Claude) {
                let stem = std::path::Path::new(&chip.id).file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
                if let Some(t3) = crate::chat::t3_chat_of(&stem) {
                    chip.title = Some(t3.title);
                    chip.thread = Some(t3.thread);
                }
            }
            let open = terminal_counter.tick();
            for a in &mut agent_snaps {
                a.terminals = open[terminals::index(a.id)];
            }
            if let Some((session, week)) = claude_status::read(now) {
                for a in agent_snaps.iter_mut().filter(|a| a.id == agents::Kind::Claude) {
                    a.session_limit = session.or(a.session_limit);
                    a.week_limit = week.or(a.week_limit);
                }
            }
            for a in &agent_snaps {
                for (i, (label, limit)) in [("session", a.session_limit), ("week", a.week_limit)].into_iter().enumerate() {
                    let key = (a.id, i as u8);
                    match limit.filter(|l| l.used_percent >= f64::from(c.limit_warning)) {
                        Some(l) => {
                            if warned.insert(key) && c.activity_agent {
                                activity_to(&app, &c, "agents", AGENT_COLOR, format!("{} {label} limit at {}%", a.name, l.used_percent.round()), Some("module:agents"));
                            }
                        }
                        None => {
                            warned.remove(&key);
                        }
                    }
                }
            }
            if c.daily_budget_usd > 0.0 && Instant::now() >= next_budget {
                next_budget = Instant::now() + BUDGET_EVERY;
                let offset = local_offset_min();
                if let Ok(h) = state.usage.try_lock() {
                    let spent = h.summary(now, offset, 1).cost_usd;
                    let day = (now as i64 + i64::from(offset) * 60_000).div_euclid(86_400_000);
                    if budget.0 != day {
                        budget = (day, 0);
                    }
                    let level = config::budget_level(spent, c.daily_budget_usd);
                    if level > budget.1 {
                        budget.1 = level;
                        if c.activity_agent {
                            let text = if level >= 2 { format!("Daily budget used up (${spent:.2})") } else { format!("Daily budget nearly used (${spent:.2})") };
                            activity_to(&app, &c, "agents", AGENT_COLOR, text, Some("module:agents"));
                        }
                    }
                }
            }
        }

        let (timer, open, tall, chat) = {
            let i = state.inner.lock().unwrap();
            (i.timer, i.open, i.tall, i.chat)
        };
        let screen = screen_of(monitor(&app, &c).as_ref());
        let (vol, bright) = if open { (volume(&platform), brightness(&platform)) } else { (None, None) };
        let mut idle = idle_sampler.tick(&c, &platform, &state.usage, !fullscreen, now);
        if c.idle_ai_usage {
            let top = |pick: fn(&AgentSnap) -> Option<agents::Limit>| agent_snaps.iter().filter_map(|a| pick(a)).map(|l| l.used_percent).reduce(f64::max);
            if let Some(p) = config::pick_limit(&c.limit_shown, top(|a| a.session_limit), top(|a| a.week_limit)) {
                idle.ai = Some(p.round().clamp(0.0, 100.0) as u8);
            }
        }
        let media = np.as_ref().map(MediaSnap::from);
        let playing = media.as_ref().is_some_and(|m| m.playing);
        // One lock, taken before the struct below: two in the same statement would wait on each other.
        let wx = state.weather.lock().unwrap().clone();
        let snap = Snapshot {
            enabled: true,
            layout: layout_for(&c, tall, chat, screen, state.inner.lock().unwrap().start_x),
            modules: MODULES.iter().map(|&(id, label, ready)| ModuleInfo { id, label, ready }).collect(),
            players: if media_supported { platform.media().sessions().unwrap_or_default() } else { Vec::new() },
            live: {
                let mut l = live(playing, &timer, c.hide_timer_countdown, &agent_snaps, now);
                l.extend(state.orders.lock().unwrap().live(now));
                l
            },
            timer: TimerSnap { timer, left_ms: timer.remaining(now) },
            agents: agent_snaps.clone(),
            sessions: chips.clone(),
            idle,
            config: c,
            media,
            volume: vol,
            brightness: bright,
            media_supported,
            open,
            watch: watch::reading(),
            weather: wx.now,
            weather_error: wx.error,
            ai_app: platform.windows().foreground().ok().flatten().is_some_and(|w| ai_window(w.app_id.as_deref(), &w.title)),
        };
        if let Ok(value) = serde_json::to_value(&snap) {
            let mut i = state.inner.lock().unwrap();
            if i.last.as_ref() != Some(&value) {
                i.last = Some(value.clone());
                drop(i);
                let _ = app.emit_to(BAR, STATE_EVENT, value.clone());
                let _ = app.emit_to(ISLAND, STATE_EVENT, value);
            }
        }

        if matches!(wake.recv_timeout(TICK), Err(RecvTimeoutError::Disconnected)) {
            return;
        }
    }
}

/// Settings changed: re-read the config on the worker and re-place the window.
pub fn on_settings_changed(app: &AppHandle) {
    app.state::<IslandState>().wake();
    place(app);
    if app.state::<IslandState>().inner.lock().unwrap().bar_visible {
        place_bar(app);
    }
}

/// A click on the taskbar's AI Agents bar: open the Island on its AI Agents module.
#[tauri::command]
pub fn island_open_agents(app: AppHandle) {
    run_action(&app, "islandModuleAgents");
}

/// A taskbar AI button: types its prompt into the window in front (the bar never takes focus), or presses Escape.
/// Does nothing unless an AI window is in front, so a stale button can't type into another app.
#[tauri::command]
pub fn island_ai_send(app: AppHandle, action: String) {
    let p = app.state::<Platform>().inner().clone();
    if !p.windows().foreground().ok().flatten().is_some_and(|w| ai_window(w.app_id.as_deref(), &w.title)) {
        return;
    }
    match action.as_str() {
        "stop" => crate::clipboard::escape_focused(),
        "compact" => crate::clipboard::type_into_focused("/compact\n"),
        "clear" => crate::clipboard::type_into_focused("/clear\n"),
        "continue" => crate::clipboard::type_into_focused("Continue\n"),
        _ => {}
    }
}

/// How wide the taskbar bar is drawn now, so only that much of its window takes the pointer.
#[tauri::command]
pub fn island_bar_width(app: AppHandle, state: State<IslandState>, width: f64) {
    let width = if width.is_finite() { width.clamp(0.0, BAR_MAX_W) } else { BAR_W };
    let changed = std::mem::replace(&mut state.inner.lock().unwrap().bar_w, width) != width;
    if changed && start_is_left(state.inner.lock().unwrap().start_x, monitor(&app, &config(&app)).map_or(1920.0, |m| screen_of(Some(&m)).0)) {
        place_bar(&app);
    }
}

/// How much room above the taskbar the bar's session buttons take while shown (0 when hidden).
#[tauri::command]
pub fn island_bar_extra(state: State<IslandState>, extra: f64) {
    state.inner.lock().unwrap().bar_extra = if extra.is_finite() { extra.clamp(0.0, BAR_ROOM) } else { 0.0 };
}

/// Opens the Island on the AI chat.
pub fn open_chat(app: &AppHandle) {
    run_action(app, "islandChat");
}

/// The taskbar bar's chat button: open the Island on the AI chat.
#[tauri::command]
pub fn island_open_chat(app: AppHandle) {
    open_chat(&app);
}

/// Runs a keybind action (`island…` ids from keybinds.rs).
pub fn run_action(app: &AppHandle, action: &str) {
    if !config(app).enabled {
        return;
    }
    let command = match action {
        "islandChat" => "module:chat".to_string(),
        "islandToggle" => "toggle".to_string(),
        other => match other.strip_prefix("islandModule") {
            Some(m) => format!("module:{}", lower_first(m)),
            None => return,
        },
    };
    focus(app);
    let _ = app.emit_to(ISLAND, COMMAND_EVENT, command);
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_lowercase().collect::<String>() + c.as_str())
}

/// Lets the window take keys (Esc, search) after the user clicked it or pressed its shortcut.
fn focus(app: &AppHandle) {
    let Some(win) = app.get_window(ISLAND) else { return };
    let _ = win.set_focusable(true);
    if !window::no_focus() {
        let _ = win.set_focus();
    }
}

#[tauri::command]
pub fn island_status(state: State<IslandState>) -> Option<Value> {
    state.wake();
    state.inner.lock().unwrap().last.clone()
}

#[tauri::command]
pub fn island_cover(state: State<IslandState>) -> Option<String> {
    state.inner.lock().unwrap().cover.clone()
}

/// The page reports whether the island is open and the logical size of the resting shape.
#[tauri::command]
pub fn island_shape(app: AppHandle, state: State<IslandState>, open: bool, width: f64, height: f64) {
    let changed = {
        let mut i = state.inner.lock().unwrap();
        let rest = (width.clamp(40.0, 1200.0), height.clamp(8.0, 600.0));
        let changed = i.open != open || (i.rest.0 - rest.0).abs() >= 0.5 || (i.rest.1 - rest.1).abs() >= 0.5;
        i.open = open;
        if !open {
            i.tall = false;
            i.chat = false;
        }
        i.rest = rest;
        changed
    };
    if !changed {
        return;
    }
    if !open {
        if let Some(win) = app.get_window(ISLAND) {
            let _ = win.set_focusable(false);
        }
    }
    state.wake();
}

/// Brings forward the window that most likely holds an agent's session: a terminal or editor whose title
/// names the project or the agent. True when one was found.
#[tauri::command]
pub fn island_goto_agent(app: AppHandle, agent: String, project: Option<String>) -> bool {
    let p = app.state::<Platform>().inner().clone();
    let Ok(windows) = p.windows().list() else { return false };
    let Some(id) = pick_agent_window(&windows, &agent, project.as_deref()) else { return false };
    p.windows().focus(id).is_ok()
}

fn pick_agent_window(windows: &[notifi_platform::WindowInfo], agent: &str, project: Option<&str>) -> Option<notifi_platform::WindowId> {
    const HOSTS: &[&str] = &["windowsterminal", "wt", "powershell", "pwsh", "cmd", "conhost", "alacritty", "wezterm", "mintty", "code", "cursor", "idea64", "pycharm64", "webstorm64", "rider64", "gnome-terminal", "konsole", "kitty"];
    let agent = agent.to_ascii_lowercase();
    let project = project.map(str::to_ascii_lowercase).filter(|p| !p.is_empty());
    let mut best: Option<(u32, &notifi_platform::WindowInfo)> = None;
    for w in windows {
        let title = w.title.to_ascii_lowercase();
        let app = w.app_id.as_deref().unwrap_or("").to_ascii_lowercase();
        let app = app.trim_end_matches(".exe");
        if app.starts_with("notifi") || title.starts_with("notifi") {
            continue;
        }
        let host = HOSTS.contains(&app);
        let by_project = project.as_deref().is_some_and(|p| title.contains(p));
        let by_agent = title.contains(&agent) || (agent == "claude" && title.contains('\u{2733}'));
        // Needs a real match; being a terminal alone is not enough.
        if !(by_project || by_agent) {
            continue;
        }
        let score = u32::from(by_project) * 4 + u32::from(by_agent) * 3 + u32::from(host);
        if best.is_none_or(|(s, _)| score > s) {
            best = Some((score, w));
        }
    }
    best.map(|(_, w)| w.id)
}

/// The docked menu's expand button: stretch it up to its tallest size, or back.
#[tauri::command]
pub fn island_tall(state: State<IslandState>, tall: bool) {
    let changed = std::mem::replace(&mut state.inner.lock().unwrap().tall, tall) != tall;
    if changed {
        state.wake();
    }
}

/// The AI chat tab: give the menu the chat's size, or take it back.
#[tauri::command]
pub fn island_chat(state: State<IslandState>, on: bool) {
    let changed = std::mem::replace(&mut state.inner.lock().unwrap().chat, on) != on;
    if changed {
        state.wake();
    }
}

/// A click on the island: take keys so Esc and search work.
#[tauri::command]
pub fn island_focus(app: AppHandle) {
    focus(&app);
}

#[tauri::command]
pub async fn island_media(app: AppHandle, action: String, value: Option<f64>, player: Option<String>) -> Result<(), String> {
    let p = app.state::<Platform>().inner().clone();
    let m = p.media();
    let r = match action.as_str() {
        "toggle" => m.toggle(),
        "next" => m.next(),
        "previous" => m.previous(),
        "seek" => m.seek(value.unwrap_or(0.0).max(0.0) as u64),
        "player" => m.set_current(player.as_deref()),
        other => return Err(format!("unknown media action `{other}`")),
    };
    app.state::<IslandState>().wake();
    r.map_err(|e| e.to_string())
}

/// The dedicated media keys (play/pause, next, previous), handled here whether or not the Island is open.
pub fn run_media_key(app: &AppHandle, action: &str) {
    let p = app.state::<Platform>().inner().clone();
    let m = p.media();
    let _ = match action {
        "mediaPlayPause" => m.toggle(),
        "mediaNext" => m.next(),
        "mediaPrevious" => m.previous(),
        _ => return,
    };
    app.state::<IslandState>().wake();
}

#[tauri::command]
pub fn island_timer(state: State<IslandState>, action: String, ms: Option<u64>) -> Result<(), String> {
    let now = now_ms();
    {
        let mut i = state.inner.lock().unwrap();
        let t = &mut i.timer;
        match action.as_str() {
            "start" => t.start(now, ms.unwrap_or(300_000)),
            "add" => t.add(now, ms.unwrap_or(60_000)),
            "pause" => t.pause(now),
            "resume" => t.resume(now),
            "cancel" => t.cancel(),
            other => return Err(format!("unknown timer action `{other}`")),
        }
    }
    state.wake();
    Ok(())
}

#[tauri::command]
pub async fn island_set_volume(app: AppHandle, volume: f32) -> Result<(), String> {
    let p = app.state::<Platform>().inner().clone();
    let device = p.audio().default_device(DeviceDirection::Output).map_err(|e| e.to_string())?.ok_or("no output device")?;
    p.audio().set_device_volume(&device.id, volume.clamp(0.0, 1.0)).map_err(|e| e.to_string())?;
    app.state::<IslandState>().wake();
    Ok(())
}

#[tauri::command]
pub async fn island_set_brightness(app: AppHandle, percent: u8) -> Result<(), String> {
    let p = app.state::<Platform>().inner().clone();
    let list = p.display().list().await.map_err(|e| e.to_string())?;
    let d = list.iter().find(|d| d.primary).or(list.first()).ok_or("no display")?;
    p.display().set_brightness(&d.id, percent.min(100)).await.map_err(|e| e.to_string())?;
    app.state::<IslandState>().wake();
    Ok(())
}

/// Orbit and quick buttons that act outside the island.
#[tauri::command]
pub fn island_run(app: AppHandle, action: String) -> Result<(), String> {
    match action.as_str() {
        "settings" => window::open_settings_at(&app, Some("island")),
        "agentLimits" => window::open_settings_at(&app, Some("island/claudePlanLimitUsd")),
        "mixer" | "keepAwake" | "system" | "controls" => crate::panel::show_by_tray(&app, Some(action)),
        other => return Err(format!("unknown island action `{other}`")),
    }
    Ok(())
}

/// The settings page's preview button: show the island and open it.
#[tauri::command]
pub fn island_preview(app: AppHandle) {
    if !config(&app).enabled {
        return;
    }
    focus(&app);
    let _ = app.emit_to(ISLAND, COMMAND_EVENT, "open");
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    id: agents::Kind,
    name: &'static str,
    /// Logs are read for usage; otherwise only detection and last use are shown.
    parsed: bool,
    plan: Option<String>,
    last_used: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitView {
    used_percent: f64,
    resets_at: Option<u64>,
    /// Where use would be at an even rate through the window (0..=1).
    pace: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowView {
    start: u64,
    resets_at: u64,
    cost_usd: f64,
    tokens: u64,
    pace: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStats {
    ready: bool,
    now: u64,
    tools: Vec<ToolInfo>,
    claude_window: Option<WindowView>,
    plan_limit_usd: f64,
    codex_session: Option<LimitView>,
    codex_week: Option<LimitView>,
    summary: usage::Summary,
}

fn limit_view(l: Option<agents::Limit>, default_window: u64, now: u64) -> Option<LimitView> {
    let l = l?;
    let window = l.window_ms.unwrap_or(default_window);
    match l.resets_at {
        // The window has renewed since the tool last reported.
        Some(r) if r <= now => Some(LimitView { used_percent: 0.0, resets_at: None, pace: 0.0 }),
        Some(r) => Some(LimitView { used_percent: l.used_percent, resets_at: Some(r), pace: usage::pace(r, window, now) }),
        None => Some(LimitView { used_percent: l.used_percent, resets_at: None, pace: 0.0 }),
    }
}

/// The AI Agents dashboard: per-tool cards, spending, trend, models, projects and activity map.
/// `days` is 1 (today), 7 or 30; `tz_offset_min` is the local clock's offset from UTC.
#[tauri::command]
pub fn island_agent_stats(app: AppHandle, state: State<IslandState>, days: u32, tz_offset_min: i32) -> AgentStats {
    let now = now_ms();
    let c = config(&app);
    let h = state.usage.lock().unwrap();
    let tools = agents::Kind::ALL
        .into_iter()
        .enumerate()
        .filter(|&(i, k)| (h.found[i] || h.last_used(k).is_some()) && config::follows(&c.agents_follow, k))
        .map(|(i, k)| ToolInfo {
            id: k,
            name: k.name(),
            parsed: k.parsed(),
            plan: match k {
                agents::Kind::Claude => h.claude_plan.clone(),
                agents::Kind::Codex => h.codex_plan(),
                _ => None,
            },
            last_used: h.last_used(k).or(h.newest[i]),
        })
        .collect();
    let claude_window = h.claude_window(now).map(|(start, resets_at, cost_usd, tokens)| WindowView {
        start,
        resets_at,
        cost_usd,
        tokens,
        pace: usage::pace(resets_at, usage::CLAUDE_WINDOW_MS, now),
    });
    let limits = h.codex_limits();
    AgentStats {
        ready: h.ready,
        now,
        tools,
        claude_window,
        plan_limit_usd: c.claude_plan_limit_usd,
        codex_session: limit_view(limits.and_then(|l| l.session), usage::CODEX_SESSION_MS, now),
        codex_week: limit_view(limits.and_then(|l| l.week), usage::CODEX_WEEK_MS, now),
        summary: h.summary(now, tz_offset_min.clamp(-14 * 60, 14 * 60), days),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_windows() {
        assert!(ai_window(Some("T3 Code (Nightly).exe"), "T3 Code (Nightly)"));
        assert!(ai_window(Some("claude.exe"), "Claude"));
        assert!(ai_window(Some("WindowsTerminal.exe"), "\u{2733} Fix the build"));
        assert!(ai_window(Some("pwsh.exe"), "codex"));
        assert!(!ai_window(Some("WindowsTerminal.exe"), "PowerShell"));
        assert!(!ai_window(Some("firefox.exe"), "Claude \u{2014} Mozilla Firefox"));
        assert!(!ai_window(Some("Code.exe"), "claude.md - Notifi"));
        assert!(!ai_window(None, "claude"));
    }

    #[test]
    fn clocks() {
        assert_eq!(clock(0), "0:00");
        assert_eq!(clock(36_400), "0:36");
        assert_eq!(clock(61_000), "1:01");
        assert_eq!(clock(3_600_000 + 5_000), "1:00:05");
    }

    #[test]
    fn only_the_shape_takes_the_pointer() {
        let g = Geo { dock: false, rise: false, frame: (1768.0, 1122.0), bar: 0.0, base: (1700.0, 270.0), tall: (1700.0, 640.0), chat: (1340.0, 1100.0) };
        let s = HIT_SLACK;
        assert_eq!(hit_rect(&g, false, false, false, (200.0, 32.0)), (784.0 - s, 0.0, 200.0 + 2.0 * s, 32.0 + s));
        assert_eq!(hit_rect(&g, true, false, true, (200.0, 32.0)), (214.0 - s, 0.0, 1340.0 + 2.0 * s, 1100.0 + s));
        // Upside down, the same shape sits on the bottom edge.
        let up = Geo { rise: true, bar: 40.0, ..g };
        assert_eq!(hit_rect(&up, false, false, false, (200.0, 32.0)), (784.0 - s, 1122.0 - 32.0 - s, 200.0 + 2.0 * s, 32.0 + s));
        // Open, the flyout floats above the taskbar and its gap, down to the bar in it.
        let (_, y, _, h) = hit_rect(&up, true, false, false, (200.0, 32.0));
        assert_eq!((y, y + h), (1122.0 - 270.0 - s - (40.0 + layout::FLY_GAP), 1122.0));
        let d = Geo { dock: true, bar: 48.0, ..g };
        let (x, y, w, h) = hit_rect(&d, false, false, false, (300.0, 48.0));
        assert_eq!((x, y + h, w), (0.0, 1122.0, 300.0 + s));
    }

    #[test]
    fn start_button_sets_where_the_docked_island_goes() {
        let mut c = IslandConfig::default();
        c.dock = "bottomLeft".into();
        let screen = (2560.0, 1440.0);
        let centred = layout_for(&c, false, false, screen, Some(1040.0));
        assert!(centred.dock && !centred.rise);
        assert_eq!(centred.bar_max, Some(1040.0 - START_GAP - layout::DOCK_LEFT));
        let left = layout_for(&c, false, false, screen, Some(0.0));
        assert!(!left.dock && left.rise && left.bar_max.is_none());
        let unknown = layout_for(&c, false, false, screen, None);
        assert!(unknown.dock && !unknown.rise && unknown.bar_max.is_none());
        c.dock = "top".into();
        assert!(!layout_for(&c, false, false, screen, Some(0.0)).rise);
        assert_eq!(bottom_centre((0, 0, 2560, 1440), 1440, 1.0, 200.0, 100.0), (1180, 1340, 200, 100));
    }

    #[test]
    fn placement_is_centred_on_the_monitor() {
        assert_eq!(top_centre((0, 0, 1920, 1080), 1.0, 220.0, 42.0), (850, 0, 220, 42));
        assert_eq!(top_centre((1920, -200, 2560, 1440), 1.5, 200.0, 40.0), (1920 + 1130, -200, 300, 60));
    }

    #[test]
    fn docked_box_sits_on_the_given_bottom_edge() {
        assert_eq!(bottom_left((0, 0, 2560, 1440), 1440, 1.0, 200.0, 32.0), (0, 1408, 200, 32));
        assert_eq!(bottom_left((0, 0, 2560, 1440), 1440, 1.5, 200.0, 32.0), (0, 1392, 300, 48));
    }


    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    fn agent(working: bool, elapsed_ms: u64) -> AgentSnap {
        AgentSnap {
            id: agents::Kind::Claude,
            name: "Claude Code",
            supported: true,
            working,
            count: u32::from(working),
            needs_you: false,
            subagents: 0,
            terminals: 0,
            done: false,
            elapsed_ms,
            model: None,
            project: None,
            usage: Default::default(),
            session_limit: None,
            week_limit: None,
            last_active: 0,
            todos: None,
        }
    }

    #[test]
    fn renewed_limits_read_as_empty() {
        let l = |r| Some(agents::Limit { used_percent: 80.0, resets_at: Some(r), window_ms: None });
        let v = limit_view(l(1_000), usage::CODEX_SESSION_MS, 2_000).unwrap();
        assert_eq!((v.used_percent, v.resets_at), (0.0, None));
        let v = limit_view(l(2_000 + usage::CODEX_SESSION_MS / 2), usage::CODEX_SESSION_MS, 2_000).unwrap();
        assert_eq!(v.used_percent, 80.0);
        assert!((v.pace - 0.5).abs() < 1e-9);
    }

    #[test]
    fn live_activities_order_and_values() {
        let mut t = Timer::default();
        assert!(live(false, &t, false, &[], 0).is_empty());
        t.start(0, 300_000);
        let l = live(true, &t, false, &[agent(false, 0), agent(true, 36_000)], 1_500);
        let ids: Vec<_> = l.iter().map(|x| x.id).collect();
        assert_eq!(ids, ["agents", "timer", "music"]);
        assert_eq!(l[0].value, "0:36");
        // Rounded up, so a fresh 5 minute timer reads 5:00, not 4:59.
        assert_eq!(l[1].value, "4:59");
        assert_eq!(live(false, &t, false, &[], 0)[0].value, "5:00");
        assert_eq!(live(false, &t, true, &[], 0)[0].value, "");
        assert_eq!(l[2].value, "");
    }
}
