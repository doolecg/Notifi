//! One trait per OS capability. Every method defaults to `Unsupported`, so a backend
//! only overrides what it really provides.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::caps::Feature;
use crate::common::{EventStream, Guard, Image, Rect};
use crate::error::{PlatformError, Result};
use crate::window_control::{DesktopState, PointerState, WindowState, WorkArea};

fn ni<T>(feature: Feature) -> Result<T> {
    Err(PlatformError::not_implemented(feature))
}

// ---- Audio ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceDirection {
    Output,
    Input,
}

/// What the device is, for its icon and the headphone rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceKind {
    Speakers,
    Headphones,
    Headset,
    /// HDMI / DisplayPort audio on a monitor or TV.
    Display,
    Microphone,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub direction: DeviceDirection,
    pub kind: DeviceKind,
    pub is_default: bool,
    /// 0.0..=1.0, above 1.0 only where boost is available; 0.0 when `has_volume` is false.
    pub volume: f32,
    pub muted: bool,
    /// False when the device has no usable volume control; hide its slider.
    pub has_volume: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AudioSession {
    pub id: String,
    pub pid: Option<u32>,
    /// Stable per app across restarts (executable or binary name), for saved levels and routes.
    pub app_id: Option<String>,
    pub app_name: String,
    /// An image file, an executable to take the icon from, or an icon theme name (Linux).
    pub icon_path: Option<String>,
    /// 0.0..=1.0, may go above 1.0 where boost is available.
    pub volume: f32,
    pub muted: bool,
    /// Current output level, 0.0..=1.0; always 0.0 where meters are unavailable.
    pub peak: f32,
    /// The output picked for this app; `None` follows the system default.
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioEvent {
    DeviceAdded(String),
    DeviceRemoved(String),
    DefaultChanged { direction: DeviceDirection, device_id: String },
    /// Volume or mute of a device changed (by anyone, including Notifi).
    VolumeChanged { device_id: String, volume: f32, muted: bool },
    SessionsChanged,
}

pub trait Audio: Send + Sync {
    fn sessions(&self) -> Result<Vec<AudioSession>> {
        ni(Feature::AudioSessions)
    }
    fn set_session_volume(&self, _session_id: &str, _volume: f32) -> Result<()> {
        ni(Feature::AudioSessions)
    }
    fn set_session_mute(&self, _session_id: &str, _muted: bool) -> Result<()> {
        ni(Feature::AudioSessions)
    }
    /// An empty `device_id` sends the app back to the system default.
    fn set_session_output(&self, _session_id: &str, _device_id: &str) -> Result<()> {
        ni(Feature::AudioPerAppOutput)
    }
    fn devices(&self, _direction: DeviceDirection) -> Result<Vec<AudioDevice>> {
        ni(Feature::AudioDevices)
    }
    fn default_device(&self, _direction: DeviceDirection) -> Result<Option<AudioDevice>> {
        ni(Feature::AudioDevices)
    }
    fn set_default_device(&self, _device_id: &str) -> Result<()> {
        ni(Feature::AudioDevices)
    }
    fn set_device_volume(&self, _device_id: &str, _volume: f32) -> Result<()> {
        ni(Feature::AudioDevices)
    }
    fn set_device_mute(&self, _device_id: &str, _muted: bool) -> Result<()> {
        ni(Feature::AudioDevices)
    }
    /// Mutes or unmutes every capture device; fails with the devices that refused when only some did.
    fn mute_all_microphones(&self, _muted: bool) -> Result<()> {
        ni(Feature::AudioMicrophone)
    }
    fn events(&self) -> Result<EventStream<AudioEvent>> {
        ni(Feature::AudioEvents)
    }
}

// ---- Windows ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowId(pub u64);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WindowInfo {
    pub id: WindowId,
    pub title: String,
    /// Executable name on Windows, app id / WM_CLASS on Linux.
    pub app_id: Option<String>,
    pub pid: Option<u32>,
    pub bounds: Rect,
    /// The [`DisplayInfo::id`] of the display holding most of the window.
    pub monitor: Option<String>,
    pub minimized: bool,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WindowEvent {
    Created(WindowId),
    Destroyed(WindowId),
    Focused(WindowId),
    Moved(WindowId, Rect),
    TitleChanged(WindowId),
    /// The user began dragging or resizing the window with the pointer (Windows only; X11 has no such event).
    MoveSizeStarted(WindowId),
    MoveSizeEnded(WindowId),
}

pub trait WindowManager: Send + Sync {
    /// Visible top-level app windows, front to back.
    fn list(&self) -> Result<Vec<WindowInfo>> {
        ni(Feature::WindowList)
    }
    fn foreground(&self) -> Result<Option<WindowInfo>> {
        Ok(self.list()?.into_iter().find(|w| w.focused))
    }
    /// True when the foreground app window fills `monitor` (physical pixels) and isn't the desktop.
    fn foreground_fullscreen(&self, monitor: Rect) -> Result<bool> {
        Ok(self.foreground()?.is_some_and(|w| !w.minimized && crate::common::covers(w.bounds, monitor)))
    }
    fn focus(&self, _id: WindowId) -> Result<()> {
        ni(Feature::WindowControl)
    }
    fn set_bounds(&self, _id: WindowId, _bounds: Rect) -> Result<()> {
        ni(Feature::WindowControl)
    }
    fn minimize(&self, _id: WindowId) -> Result<()> {
        ni(Feature::WindowControl)
    }
    fn close(&self, _id: WindowId) -> Result<()> {
        ni(Feature::WindowControl)
    }
    /// Maximises with the OS's own maximise.
    fn maximize(&self, _id: WindowId) -> Result<()> {
        ni(Feature::WindowControl)
    }
    /// Undoes a maximise or minimise.
    fn restore(&self, _id: WindowId) -> Result<()> {
        ni(Feature::WindowControl)
    }
    /// Shows or removes the title bar and border.
    fn set_decorations(&self, _id: WindowId, _on: bool) -> Result<()> {
        ni(Feature::WindowControl)
    }
    fn window_state(&self, _id: WindowId) -> Result<WindowState> {
        ni(Feature::WindowControl)
    }
    /// Every display with the area apps may use (without taskbars and panels), in physical pixels.
    fn work_areas(&self) -> Result<Vec<WorkArea>> {
        ni(Feature::WindowControl)
    }
    fn pointer(&self) -> Result<PointerState> {
        ni(Feature::WindowControl)
    }
    /// The frontmost app window under a point.
    fn window_at(&self, x: i32, y: i32) -> Result<Option<WindowInfo>> {
        Ok(self.list()?.into_iter().find(|w| !w.minimized && crate::window_control::contains(w.bounds, x, y)))
    }
    /// Every visible top-level window of a process, including tool windows and floating bars.
    fn windows_of_pid(&self, pid: u32) -> Result<Vec<WindowInfo>> {
        Ok(self.list()?.into_iter().filter(|w| w.pid == Some(pid)).collect())
    }
    /// Asks the process to close, and ends it if it has not after a few seconds.
    fn quit_process(&self, _pid: u32) -> Result<()> {
        ni(Feature::WindowControl)
    }
    fn desktop_state(&self) -> Result<DesktopState> {
        ni(Feature::WindowControl)
    }
    /// Switches to desktop `n` (1-based).
    fn switch_desktop(&self, _n: u32) -> Result<()> {
        ni(Feature::WindowControl)
    }
    /// Adds one desktop at the end.
    fn create_desktop(&self) -> Result<()> {
        ni(Feature::WindowControl)
    }
    /// Sends a window to desktop `n` (1-based).
    fn move_to_desktop(&self, _id: WindowId, _n: u32) -> Result<()> {
        ni(Feature::WindowControl)
    }
    /// Draws a live thumbnail of `source` into `host` (one of our windows) at `dest`, until the guard drops.
    fn thumbnail(&self, _source: WindowId, _host: WindowId, _dest: Rect) -> Result<Guard> {
        ni(Feature::WindowThumbnails)
    }
    fn events(&self) -> Result<EventStream<WindowEvent>> {
        ni(Feature::WindowEvents)
    }
}

// ---- Input ----

/// Linux evdev key codes (KEY_*); the Windows backend maps them to and from virtual keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyCode(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InputEvent {
    Key { key: KeyCode, pressed: bool },
    Button { button: MouseButton, pressed: bool },
    /// Relative motion.
    Motion { dx: i32, dy: i32 },
    /// In wheel notches; fractions for high-resolution wheels.
    Scroll { dx: f32, dy: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Swallow,
}

pub type InputFilter = Box<dyn Fn(&InputEvent) -> Verdict + Send + Sync>;

pub trait Input: Send + Sync {
    /// Observes global input without changing it.
    fn hook(&self) -> Result<EventStream<InputEvent>> {
        ni(Feature::InputHook)
    }
    fn inject(&self, _events: &[InputEvent]) -> Result<()> {
        ni(Feature::InputInject)
    }
    /// Runs `filter` on every event before apps see it, until the guard drops.
    fn grab(&self, _filter: InputFilter) -> Result<Guard> {
        ni(Feature::InputGrab)
    }
}

// ---- Clipboard ----

#[derive(Debug, Clone, PartialEq)]
pub enum ClipboardContent {
    Text(String),
    Image(Image),
    Files(Vec<PathBuf>),
}

pub trait Clipboard: Send + Sync {
    /// Fires once per clipboard change.
    fn watch(&self) -> Result<EventStream<()>> {
        ni(Feature::ClipboardWatch)
    }
    fn read(&self) -> Result<Option<ClipboardContent>> {
        ni(Feature::ClipboardReadWrite)
    }
    fn write(&self, _content: &ClipboardContent) -> Result<()> {
        ni(Feature::ClipboardReadWrite)
    }
}

// ---- Display ----

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DisplayInfo {
    pub id: String,
    pub name: String,
    pub bounds: Rect,
    pub scale: f32,
    pub primary: bool,
    pub internal: bool,
    pub ddc: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VcpValue {
    pub current: u16,
    pub max: u16,
}

#[async_trait]
pub trait Display: Send + Sync {
    async fn list(&self) -> Result<Vec<DisplayInfo>> {
        ni(Feature::DisplayList)
    }
    /// Percent, 0..=100.
    async fn brightness(&self, _display_id: &str) -> Result<u8> {
        ni(Feature::DisplayBrightness)
    }
    async fn set_brightness(&self, _display_id: &str, _percent: u8) -> Result<()> {
        ni(Feature::DisplayBrightness)
    }
    async fn ddc_get(&self, _display_id: &str, _vcp: u8) -> Result<VcpValue> {
        ni(Feature::DisplayDdc)
    }
    async fn ddc_set(&self, _display_id: &str, _vcp: u8, _value: u16) -> Result<()> {
        ni(Feature::DisplayDdc)
    }
    /// Makes one of our windows a click-through, always-on-top dim overlay.
    fn make_dim_overlay(&self, _window: WindowId) -> Result<()> {
        ni(Feature::DisplayDim)
    }
    /// Gamma-ramp dimming, 0.0 (none) ..= 1.0 (black). Released when the guard drops.
    fn gamma_dim(&self, _display_id: &str, _level: f32) -> Result<Guard> {
        ni(Feature::DisplayDim)
    }
}

// ---- Power ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeepAwake {
    pub keep_display_on: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerEvent {
    Suspending,
    Resumed,
    Locked,
    Unlocked,
    OnBattery(bool),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PowerProfile {
    pub id: String,
    pub name: String,
    pub active: bool,
}

#[async_trait]
pub trait Power: Send + Sync {
    /// Blocks idle sleep (and display blanking if asked) until the guard drops.
    async fn keep_awake(&self, _request: KeepAwake) -> Result<Guard> {
        ni(Feature::KeepAwake)
    }
    /// Keeps the machine running with the lid closed until the guard drops.
    async fn inhibit_lid(&self, _reason: &str) -> Result<Guard> {
        ni(Feature::LidInhibit)
    }
    async fn lock_screen(&self) -> Result<()> {
        ni(Feature::LockScreen)
    }
    fn events(&self) -> Result<EventStream<PowerEvent>> {
        ni(Feature::PowerEvents)
    }
    /// Name of the active power plan / profile.
    async fn power_profile(&self) -> Result<String> {
        ni(Feature::PowerProfile)
    }
    async fn power_profiles(&self) -> Result<Vec<PowerProfile>> {
        ni(Feature::PowerProfile)
    }
    async fn set_power_profile(&self, _id: &str) -> Result<()> {
        ni(Feature::PowerProfile)
    }
}

// ---- Capture ----

#[derive(Debug, Clone, PartialEq)]
pub enum CaptureTarget {
    Screen { display_id: String },
    Window(WindowId),
    Region(Rect),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordAudio {
    pub system: bool,
    pub microphone: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordingOptions {
    pub target: CaptureTarget,
    pub audio: RecordAudio,
    pub fps: u32,
    pub show_cursor: bool,
    pub output: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordingId(pub u64);

#[async_trait]
pub trait Capture: Send + Sync {
    async fn screenshot(&self, _target: CaptureTarget) -> Result<Image> {
        ni(Feature::Screenshot)
    }
    async fn start_recording(&self, _options: RecordingOptions) -> Result<RecordingId> {
        ni(Feature::ScreenRecording)
    }
    /// Finishes the file and returns its path.
    async fn stop_recording(&self, _id: RecordingId) -> Result<PathBuf> {
        ni(Feature::ScreenRecording)
    }
}

// ---- Packages ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageSource {
    Winget,
    Scoop,
    PackageKit,
    Flatpak,
    Snap,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageRef {
    pub source: PackageSource,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Package {
    pub reference: PackageRef,
    pub name: String,
    pub installed_version: Option<String>,
    pub available_version: Option<String>,
}

#[async_trait]
pub trait Packages: Send + Sync {
    async fn installed(&self) -> Result<Vec<Package>> {
        ni(Feature::Packages)
    }
    async fn updates(&self) -> Result<Vec<Package>> {
        ni(Feature::Packages)
    }
    async fn search(&self, _query: &str) -> Result<Vec<Package>> {
        ni(Feature::Packages)
    }
    async fn install(&self, _package: &PackageRef) -> Result<()> {
        ni(Feature::Packages)
    }
    async fn update(&self, _package: &PackageRef) -> Result<()> {
        ni(Feature::Packages)
    }
    async fn remove(&self, _package: &PackageRef) -> Result<()> {
        ni(Feature::Packages)
    }
}

// ---- Sensors ----

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CpuReading {
    pub usage_percent: f32,
    pub per_core_percent: Vec<f32>,
    pub frequency_mhz: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MemoryReading {
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GpuReading {
    pub name: String,
    pub usage_percent: Option<f32>,
    pub vram_used_bytes: Option<u64>,
    pub vram_total_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TemperatureReading {
    pub label: String,
    pub celsius: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FanReading {
    pub label: String,
    pub rpm: Option<u32>,
    pub pwm_percent: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatteryReading {
    pub percent: f32,
    pub charging: bool,
    pub on_ac: bool,
    pub state: BatteryState,
    /// Battery charge or discharge rate, always positive; `charging` gives the direction.
    pub power_watts: Option<f32>,
    pub seconds_remaining: Option<u64>,
    /// Full-charge capacity as a percentage of design capacity.
    pub health_percent: Option<f32>,
    pub cycle_count: Option<u32>,
    pub temperature_celsius: Option<f32>,
    pub adapter_watts: Option<f32>,
    pub adapter_max_watts: Option<f32>,
    /// On battery: the discharge rate. On AC: adapter watts minus the charge rate, when both are known.
    pub system_watts: Option<f32>,
    pub design_capacity_mwh: Option<u32>,
    pub full_capacity_mwh: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BatteryState {
    Charging,
    Discharging,
    Full,
    NotCharging,
}

pub trait Sensors: Send + Sync {
    fn cpu(&self) -> Result<CpuReading> {
        ni(Feature::SensorsCpu)
    }
    fn memory(&self) -> Result<MemoryReading> {
        ni(Feature::SensorsMemory)
    }
    fn gpus(&self) -> Result<Vec<GpuReading>> {
        ni(Feature::SensorsGpu)
    }
    fn temperatures(&self) -> Result<Vec<TemperatureReading>> {
        ni(Feature::SensorsTemperature)
    }
    fn fans(&self) -> Result<Vec<FanReading>> {
        ni(Feature::SensorsFans)
    }
    fn battery(&self) -> Result<Option<BatteryReading>> {
        ni(Feature::SensorsBattery)
    }
}

// ---- Session ----

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemovableVolume {
    pub id: String,
    pub label: String,
    pub mount_point: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrashInfo {
    pub items: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub icon: Option<PathBuf>,
}

#[async_trait]
pub trait Session: Send + Sync {
    async fn dark_mode(&self) -> Result<bool> {
        ni(Feature::DarkModeRead)
    }
    async fn set_dark_mode(&self, _dark: bool) -> Result<()> {
        ni(Feature::DarkModeWrite)
    }
    async fn desktop_icons_visible(&self) -> Result<bool> {
        ni(Feature::DesktopIcons)
    }
    async fn set_desktop_icons_visible(&self, _visible: bool) -> Result<()> {
        ni(Feature::DesktopIcons)
    }
    async fn trash_info(&self) -> Result<TrashInfo> {
        ni(Feature::Trash)
    }
    async fn move_to_trash(&self, _path: &Path) -> Result<()> {
        ni(Feature::Trash)
    }
    async fn empty_trash(&self) -> Result<()> {
        ni(Feature::Trash)
    }
    async fn removable_volumes(&self) -> Result<Vec<RemovableVolume>> {
        ni(Feature::Eject)
    }
    /// `volume_id` is the volume's mount point ("E:\\", "/media/me/USB").
    async fn eject(&self, _volume_id: &str) -> Result<()> {
        ni(Feature::Eject)
    }
    async fn notify(&self, _notification: Notification) -> Result<()> {
        ni(Feature::Notifications)
    }
}

// ---- Media ----

/// One media player (an SMTC session on Windows, an MPRIS player on Linux).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MediaSession {
    /// Source app id (AUMID or MPRIS bus name); pass it to [`Media::set_current`].
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cover {
    pub bytes: Vec<u8>,
    /// e.g. `image/png`
    pub mime: String,
}

/// A snapshot of the current player. `position_ms` is already moved on to "now" while playing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NowPlaying {
    pub session: MediaSession,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Raw image bytes; left out of serde output, so send it to the UI separately.
    #[serde(skip)]
    pub cover: Option<Cover>,
    pub duration_ms: Option<u64>,
    pub position_ms: u64,
    pub state: PlaybackState,
    pub can_play_pause: bool,
    pub can_prev: bool,
    pub can_next: bool,
    pub can_seek: bool,
    /// Title of the next track, only where the player publishes its queue (MPRIS TrackList).
    pub next_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaEvent {
    /// Title, artist, album or cover changed, or the current player changed.
    TrackChanged,
    PlaybackChanged,
    /// The position jumped (seek); read it again with `now_playing`.
    Seeked,
    SessionsChanged,
}

pub trait Media: Send + Sync {
    fn sessions(&self) -> Result<Vec<MediaSession>> {
        ni(Feature::Media)
    }
    /// The player now shown: the one set with `set_current` if still running, else the system's own pick.
    fn current(&self) -> Result<Option<MediaSession>> {
        ni(Feature::Media)
    }
    /// `None` goes back to following the system's choice.
    fn set_current(&self, _session_id: Option<&str>) -> Result<()> {
        ni(Feature::Media)
    }
    /// `None` when nothing is running.
    fn now_playing(&self) -> Result<Option<NowPlaying>> {
        ni(Feature::Media)
    }
    fn play(&self) -> Result<()> {
        ni(Feature::Media)
    }
    fn pause(&self) -> Result<()> {
        ni(Feature::Media)
    }
    fn toggle(&self) -> Result<()> {
        ni(Feature::Media)
    }
    fn next(&self) -> Result<()> {
        ni(Feature::Media)
    }
    fn previous(&self) -> Result<()> {
        ni(Feature::Media)
    }
    fn seek(&self, _position_ms: u64) -> Result<()> {
        ni(Feature::Media)
    }
    fn events(&self) -> Result<EventStream<MediaEvent>> {
        ni(Feature::Media)
    }
}
