//! Tray readouts. The Windows tray can't show text, so values and bars are drawn into tray
//! icon images with a small pixel font; on Linux the same text also goes into the tray label
//! (Plasma shows it; GNOME needs the extension, otherwise only the drawn icon shows).
//!
//! Icons are drawn on a 16-unit grid at 2 px per unit (32 px), so they stay sharp at 100% and
//! 200% scale. Every glyph has a fixed width, so values never jitter.

use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::{image::Image, AppHandle};

use super::{memory::Pressure, units};
use crate::settings::Settings;

const GRID: u32 = 16;
const UNIT: u32 = 2;
pub const SIZE: u32 = GRID * UNIT;
const MIN_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    Cpu,
    Memory,
    Gpu,
    Disk,
    DiskActivity,
    Network,
    Battery,
    Power,
    Fan,
    Temperature,
}

impl Reading {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "cpu" => Self::Cpu,
            "memory" | "mem" => Self::Memory,
            "gpu" => Self::Gpu,
            "disk" => Self::Disk,
            "diskactivity" => Self::DiskActivity,
            "network" | "net" => Self::Network,
            "battery" => Self::Battery,
            "power" => Self::Power,
            "fan" => Self::Fan,
            "temperature" | "temp" => Self::Temperature,
            _ => return None,
        })
    }

    fn id(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::Gpu => "gpu",
            Self::Disk => "disk",
            Self::DiskActivity => "diskActivity",
            Self::Network => "network",
            Self::Battery => "battery",
            Self::Power => "power",
            Self::Fan => "fan",
            Self::Temperature => "temperature",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Memory => "MEM",
            Self::Gpu => "GPU",
            Self::Disk => "DSK",
            Self::DiskActivity => "I0",
            Self::Network => "NET",
            Self::Battery => "BAT",
            Self::Power => "PWR",
            Self::Fan => "FAN",
            Self::Temperature => "TMP",
        }
    }

    fn letter(self) -> &'static str {
        &self.label()[..1]
    }

    fn name(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Memory => "Memory",
            Self::Gpu => "GPU",
            Self::Disk => "Disk",
            Self::DiskActivity => "Disk activity",
            Self::Network => "Network",
            Self::Battery => "Battery",
            Self::Power => "Power",
            Self::Fan => "Fan",
            Self::Temperature => "Temperature",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DiskMode {
    #[default]
    Percent,
    Available,
    Used,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub bar: bool,
    pub separate: bool,
    pub compact: bool,
    pub up_first: bool,
    pub bits: bool,
    pub disk: DiskMode,
    pub fahrenheit: bool,
    /// Light text for a dark taskbar or panel.
    pub dark_panel: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self { bar: false, separate: false, compact: false, up_first: false, bits: false, disk: DiskMode::Percent, fahrenheit: false, dark_panel: true }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub readings: Vec<Reading>,
    pub style: Style,
    pub interval: Duration,
    pub hide_app_icon: bool,
}

impl Config {
    /// `None` while tray readouts are switched off or nothing is picked.
    pub fn from_settings(s: &Settings) -> Option<Self> {
        if !s.enabled("monitor") || !s.enabled("trayReadouts") {
            return None;
        }
        let opt = |key: &str| s.option("trayReadouts", key);
        let mut readings: Vec<Reading> = Vec::new();
        for r in opt("readings").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).filter_map(Reading::parse) {
            if !readings.contains(&r) {
                readings.push(r);
            }
        }
        if readings.is_empty() {
            return None;
        }
        let style = Style {
            bar: opt("style").and_then(Value::as_str) == Some("bar"),
            separate: opt("separateItems").and_then(Value::as_bool).unwrap_or(false),
            compact: opt("spacing").and_then(Value::as_str) == Some("compact"),
            up_first: opt("networkOrder").and_then(Value::as_str) == Some("upFirst"),
            bits: s.option("network", "unit").and_then(Value::as_str) == Some("bits"),
            disk: match s.option("monitor", "diskReadout").and_then(Value::as_str) {
                Some("available") => DiskMode::Available,
                Some("used") => DiskMode::Used,
                _ => DiskMode::Percent,
            },
            fahrenheit: s.option("temperatures", "unit").and_then(Value::as_str) == Some("fahrenheit"),
            dark_panel: true,
        };
        let secs = opt("interval").and_then(Value::as_f64).unwrap_or(2.0);
        Some(Self {
            readings,
            style,
            interval: Duration::from_secs_f64(secs).max(MIN_INTERVAL),
            hide_app_icon: opt("hideAppIcon").and_then(Value::as_bool).unwrap_or(false),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct Values {
    pub cpu: f32,
    pub mem: f32,
    pub pressure: Pressure,
    pub gpu: Option<f32>,
    /// Used percent, available bytes, used bytes.
    pub disk: Option<(f32, u64, u64)>,
    pub disk_io: Option<f64>,
    pub down: f64,
    pub up: f64,
    /// Percent, seconds left, charging.
    pub battery: Option<(f32, Option<u64>, bool)>,
    pub power: Option<f32>,
    pub fan: Option<u32>,
    pub temp: Option<f32>,
}

impl Values {
    /// Whether this machine can show the reading at all.
    fn offers(&self, r: Reading) -> bool {
        match r {
            Reading::Gpu => self.gpu.is_some(),
            Reading::Disk => self.disk.is_some(),
            Reading::DiskActivity => self.disk_io.is_some(),
            Reading::Battery => self.battery.is_some(),
            Reading::Power => self.power.is_some(),
            Reading::Fan => self.fan.is_some(),
            Reading::Temperature => self.temp.is_some(),
            _ => true,
        }
    }

    /// Fill share for a bar, 0-100.
    fn share(&self, r: Reading) -> f32 {
        match r {
            Reading::Cpu => self.cpu,
            Reading::Memory => self.mem,
            Reading::Gpu => self.gpu.unwrap_or(0.0),
            Reading::Disk => self.disk.map_or(0.0, |d| d.0),
            Reading::Battery => self.battery.map_or(0.0, |b| b.0),
            Reading::Temperature => self.temp.unwrap_or(0.0),
            Reading::Power => self.power.map_or(0.0, |w| w * 2.0),
            _ => 0.0,
        }
        .clamp(0.0, 100.0)
    }
}

fn pct(v: f32) -> String {
    format!("{:.0}%", v.clamp(0.0, 100.0))
}

/// The short value drawn for a reading (network gives download/upload in display order).
pub fn value_text(r: Reading, v: &Values, s: &Style) -> Vec<String> {
    let one = |t: String| vec![t];
    match r {
        Reading::Cpu => one(pct(v.cpu)),
        Reading::Memory => one(pct(v.mem)),
        Reading::Gpu => one(v.gpu.map_or("-".into(), pct)),
        Reading::Disk => one(match (v.disk, s.disk) {
            (None, _) => "-".into(),
            (Some(d), DiskMode::Percent) => pct(d.0),
            (Some(d), DiskMode::Available) => units::compact_size(d.1),
            (Some(d), DiskMode::Used) => units::compact_size(d.2),
        }),
        Reading::DiskActivity => one(units::compact_rate(v.disk_io.unwrap_or(0.0), false)),
        Reading::Network => {
            let (down, up) = (units::compact_rate(v.down, s.bits), units::compact_rate(v.up, s.bits));
            if s.up_first {
                vec![up, down]
            } else {
                vec![down, up]
            }
        }
        Reading::Battery => {
            let Some((p, left, _)) = v.battery else { return one("-".into()) };
            let mut out = vec![pct(p)];
            if let Some(secs) = left.filter(|s| *s > 0) {
                out.push(units::hours_minutes(secs));
            }
            out
        }
        Reading::Power => one(v.power.map_or("-".into(), |w| format!("{:.0}W", w.max(0.0)))),
        Reading::Fan => one(v.fan.map_or("-".into(), |rpm| units::compact_size(u64::from(rpm)))),
        Reading::Temperature => one(v.temp.map_or("-".into(), |c| format!("{:.0}°", units::to_unit(c, s.fahrenheit)))),
    }
}

/// Plain text for tooltips and the Linux tray label.
pub fn label_text(cfg: &Config, v: &Values) -> String {
    let sep = if cfg.style.compact { " " } else { "  " };
    shown(cfg, v)
        .map(|r| {
            let values = value_text(r, v, &cfg.style);
            let body = if r == Reading::Network {
                let (a, b) = if cfg.style.up_first { ("↑", "↓") } else { ("↓", "↑") };
                format!("{a}{} {b}{}", values[0], values[1])
            } else {
                values.join(" ")
            };
            format!("{} {body}", r.label())
        })
        .collect::<Vec<_>>()
        .join(sep)
}

fn shown<'a>(cfg: &'a Config, v: &'a Values) -> impl Iterator<Item = Reading> + 'a {
    cfg.readings.iter().copied().filter(|r| v.offers(*r))
}

// ---- Drawing ----

type Rgba = [u8; 4];

const ACCENT: Rgba = [78, 161, 255, 255];
const WARM: Rgba = [255, 159, 67, 255];
const GOOD: Rgba = [52, 199, 89, 255];
const WARN: Rgba = [245, 166, 35, 255];
const BAD: Rgba = [255, 95, 87, 255];

fn fg(s: &Style) -> Rgba {
    if s.dark_panel {
        [255, 255, 255, 255]
    } else {
        [24, 24, 28, 255]
    }
}

fn faint(s: &Style) -> Rgba {
    let mut c = fg(s);
    c[3] = 90;
    c
}

fn level(share: f32) -> Rgba {
    if share >= 90.0 {
        BAD
    } else if share >= 70.0 {
        WARN
    } else {
        ACCENT
    }
}

fn pressure_colour(p: Pressure) -> Rgba {
    match p {
        Pressure::Normal => GOOD,
        Pressure::Elevated => WARN,
        Pressure::High | Pressure::Critical => BAD,
    }
}

/// Glyph width in units and its five rows, high bit on the left.
fn glyph(c: char) -> (u32, [u8; 5]) {
    let g3 = |rows: [u8; 5]| (3, rows);
    match c {
        '0' | 'O' => g3([0b111, 0b101, 0b101, 0b101, 0b111]),
        '1' => g3([0b010, 0b110, 0b010, 0b010, 0b111]),
        '2' => g3([0b111, 0b001, 0b111, 0b100, 0b111]),
        '3' => g3([0b111, 0b001, 0b111, 0b001, 0b111]),
        '4' => g3([0b101, 0b101, 0b111, 0b001, 0b001]),
        '5' => g3([0b111, 0b100, 0b111, 0b001, 0b111]),
        '6' => g3([0b111, 0b100, 0b111, 0b101, 0b111]),
        '7' => g3([0b111, 0b001, 0b010, 0b010, 0b010]),
        '8' => g3([0b111, 0b101, 0b111, 0b101, 0b111]),
        '9' => g3([0b111, 0b101, 0b111, 0b001, 0b111]),
        '%' => g3([0b101, 0b001, 0b010, 0b100, 0b101]),
        'A' => g3([0b010, 0b101, 0b111, 0b101, 0b101]),
        'B' => g3([0b110, 0b101, 0b110, 0b101, 0b110]),
        'C' => g3([0b111, 0b100, 0b100, 0b100, 0b111]),
        'D' => g3([0b110, 0b101, 0b101, 0b101, 0b110]),
        'E' => g3([0b111, 0b100, 0b110, 0b100, 0b111]),
        'F' => g3([0b111, 0b100, 0b110, 0b100, 0b100]),
        'G' => g3([0b111, 0b100, 0b101, 0b101, 0b111]),
        'I' => g3([0b111, 0b010, 0b010, 0b010, 0b111]),
        'K' => g3([0b101, 0b101, 0b110, 0b101, 0b101]),
        'M' => g3([0b101, 0b111, 0b111, 0b101, 0b101]),
        'N' => g3([0b110, 0b101, 0b101, 0b101, 0b101]),
        'P' => g3([0b111, 0b101, 0b111, 0b100, 0b100]),
        'R' => g3([0b110, 0b101, 0b110, 0b101, 0b101]),
        'S' => g3([0b111, 0b100, 0b111, 0b001, 0b111]),
        'T' => g3([0b111, 0b010, 0b010, 0b010, 0b010]),
        'U' => g3([0b101, 0b101, 0b101, 0b101, 0b111]),
        'W' => g3([0b101, 0b101, 0b111, 0b111, 0b101]),
        '-' => g3([0, 0, 0b111, 0, 0]),
        '.' => (1, [0, 0, 0, 0, 1]),
        ':' => (1, [0, 1, 0, 1, 0]),
        '°' => (2, [0b11, 0b11, 0, 0, 0]),
        _ => (1, [0; 5]),
    }
}

pub fn text_width(s: &str, scale: u32) -> u32 {
    let n = s.chars().count() as u32;
    if n == 0 {
        return 0;
    }
    (s.chars().map(|c| glyph(c).0).sum::<u32>() + n - 1) * scale
}

pub struct Canvas {
    pub rgba: Vec<u8>,
}

impl Canvas {
    fn new() -> Self {
        Self { rgba: vec![0; (SIZE * SIZE * 4) as usize] }
    }

    /// Fills a rectangle given in grid units; parts outside the icon are clipped.
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgba) {
        let u = UNIT as i32;
        for py in (y * u).max(0)..((y + h) * u).min(SIZE as i32) {
            for px in (x * u).max(0)..((x + w) * u).min(SIZE as i32) {
                let i = ((py as u32 * SIZE + px as u32) * 4) as usize;
                self.rgba[i..i + 4].copy_from_slice(&c);
            }
        }
    }

    fn text(&mut self, x: i32, y: i32, s: &str, scale: u32, c: Rgba) {
        let k = scale as i32;
        let mut cx = x;
        for ch in s.chars() {
            let (w, rows) = glyph(ch);
            for (ry, bits) in rows.iter().enumerate() {
                for col in 0..w {
                    if bits >> (w - 1 - col) & 1 == 1 {
                        self.rect(cx + col as i32 * k, y + ry as i32 * k, k, k, c);
                    }
                }
            }
            cx += (w as i32 + 1) * k;
        }
    }

    fn centred(&mut self, y: i32, s: &str, scale: u32, c: Rgba) {
        let x = (GRID as i32 - text_width(s, scale) as i32) / 2;
        self.text(x, y, s, scale, c);
    }

    #[cfg(test)]
    pub fn lit(&self) -> usize {
        self.rgba.chunks(4).filter(|p| p[3] > 0).count()
    }
}

/// Text that fits the 16-unit width: as is, without the % sign, or cut to fit.
fn fit(s: &str, room: u32) -> String {
    if text_width(s, 1) <= room {
        return s.to_string();
    }
    let bare = s.trim_end_matches('%');
    if text_width(bare, 1) <= room {
        return bare.to_string();
    }
    let mut out = String::new();
    for ch in bare.chars() {
        let next = format!("{out}{ch}");
        if text_width(&next, 1) > room {
            break;
        }
        out = next;
    }
    out
}

fn bar_colour(r: Reading, v: &Values) -> Rgba {
    match r {
        Reading::Memory => pressure_colour(v.pressure),
        Reading::Battery => {
            let p = v.share(r);
            if p <= 15.0 {
                BAD
            } else if p <= 30.0 {
                WARN
            } else {
                GOOD
            }
        }
        _ => level(v.share(r)),
    }
}

fn battery_shape(c: &mut Canvas, v: &Values, s: &Style) {
    let share = v.share(Reading::Battery);
    let colour = bar_colour(Reading::Battery, v);
    c.rect(0, 3, 14, 1, fg(s));
    c.rect(0, 12, 14, 1, fg(s));
    c.rect(0, 3, 1, 10, fg(s));
    c.rect(13, 3, 1, 10, fg(s));
    c.rect(14, 6, 2, 4, fg(s));
    let fill = ((share / 100.0) * 11.0).round() as i32;
    c.rect(1, 4, 12, 8, faint(s));
    c.rect(1, 4, fill.clamp(0, 12), 8, colour);
    if v.battery.is_some_and(|b| b.2) {
        c.rect(6, 5, 1, 6, fg(s));
        c.rect(4, 7, 5, 1, fg(s));
    }
}

/// One reading on its own icon.
pub fn draw_single(r: Reading, v: &Values, s: &Style) -> Canvas {
    let mut c = Canvas::new();
    let values = value_text(r, v, s);
    if s.bar && r != Reading::Network && r != Reading::DiskActivity {
        if r == Reading::Battery {
            battery_shape(&mut c, v, s);
            return c;
        }
        c.text(0, 5, r.letter(), 1, fg(s));
        let fill = ((v.share(r) / 100.0) * 14.0).round() as i32;
        c.rect(5, 1, 11, 14, faint(s));
        c.rect(5, 15 - fill.clamp(0, 14), 11, fill.clamp(0, 14), bar_colour(r, v));
        return c;
    }
    if values.len() == 2 {
        let colours = if r == Reading::Network {
            if s.up_first { [WARM, ACCENT] } else { [ACCENT, WARM] }
        } else {
            [fg(s), fg(s)]
        };
        c.centred(2, &fit(&values[0], GRID), 1, colours[0]);
        c.centred(9, &fit(&values[1], GRID), 1, colours[1]);
        return c;
    }
    let value = &values[0];
    if s.compact {
        let big = text_width(value, 2) <= GRID;
        if big {
            c.centred(3, value, 2, fg(s));
        } else {
            c.centred(5, &fit(value, GRID), 1, fg(s));
        }
        return c;
    }
    c.centred(2, r.label(), 1, faint_label(s));
    c.centred(9, &fit(value, GRID), 1, fg(s));
    c
}

fn faint_label(s: &Style) -> Rgba {
    let mut col = fg(s);
    col[3] = 190;
    col
}

/// Several readings stacked on one icon, one row each; network takes two rows. Extra rows
/// beyond what fits are left out.
pub fn draw_group(cfg: &Config, v: &Values) -> Canvas {
    let s = &cfg.style;
    let mut rows: Vec<(Option<Reading>, String, Rgba)> = Vec::new();
    for r in shown(cfg, v) {
        let values = value_text(r, v, s);
        if r == Reading::Network {
            let colours = if s.up_first { [WARM, ACCENT] } else { [ACCENT, WARM] };
            rows.push((None, values[0].clone(), colours[0]));
            rows.push((None, values[1].clone(), colours[1]));
        } else {
            rows.push((Some(r), values[0].clone(), fg(s)));
        }
    }
    let (gap, max_rows) = if s.compact { (0, 3) } else { (1, 2) };
    rows.truncate(max_rows);
    let mut c = Canvas::new();
    let n = rows.len() as i32;
    if n == 0 {
        return c;
    }
    let total = n * 5 + (n - 1) * gap;
    let mut y = (GRID as i32 - total) / 2;
    for (reading, value, colour) in rows {
        match reading {
            Some(r) if s.bar && r != Reading::DiskActivity => {
                c.text(0, y, r.letter(), 1, fg(s));
                let fill = ((v.share(r) / 100.0) * 12.0).round() as i32;
                c.rect(4, y + 1, 12, 3, faint(s));
                c.rect(4, y + 1, fill.clamp(0, 12), 3, bar_colour(r, v));
            }
            Some(r) => {
                let short = fit(&value, GRID - 4);
                if short.len() >= value.trim_end_matches('%').len() {
                    c.text(0, y, r.letter(), 1, faint_label(s));
                    let w = text_width(&short, 1) as i32;
                    c.text(GRID as i32 - w, y, &short, 1, colour);
                } else {
                    c.centred(y, &fit(&value, GRID), 1, colour);
                }
            }
            None => c.centred(y, &fit(&value, GRID), 1, colour),
        }
        y += 5 + gap;
    }
    c
}

// ---- Tray items ----

#[derive(Default)]
pub struct Tray {
    ids: Vec<String>,
    last: Option<Instant>,
    hidden_app_icon: bool,
}

impl Tray {
    pub fn due(&self, cfg: &Config) -> bool {
        self.last.is_none_or(|t| t.elapsed() + Duration::from_millis(50) >= cfg.interval)
    }

    /// Draws the readouts into tray items, adding and removing items to match.
    pub fn update(&mut self, app: &AppHandle, cfg: Option<&Config>, v: &Values) {
        let Some(cfg) = cfg else {
            self.clear(app);
            return;
        };
        self.last = Some(Instant::now());
        let mut style = cfg.style.clone();
        style.dark_panel = dark_panel();
        let mut items: Vec<(String, Canvas, String)> = Vec::new();
        if style.separate {
            for r in shown(cfg, v) {
                let one = Config { readings: vec![r], ..cfg.clone() };
                items.push((format!("readout-{}", r.id()), draw_single(r, v, &style), label_text(&one, v)));
            }
        } else if shown(cfg, v).next().is_some() {
            let cfg = Config { style: style.clone(), ..cfg.clone() };
            items.push(("readout-group".into(), draw_group(&cfg, v), label_text(&cfg, v)));
        }
        let wanted: Vec<String> = items.iter().map(|(id, ..)| id.clone()).collect();
        for id in self.ids.iter().filter(|id| !wanted.contains(id)) {
            crate::tray::remove_readout(app, id);
        }
        for (id, canvas, text) in items {
            let tooltip = cfg.readings.iter().find(|r| id.ends_with(r.id())).map_or(text.clone(), |r| format!("{}: {text}", r.name()));
            let image = Image::new_owned(canvas.rgba, SIZE, SIZE);
            crate::tray::set_readout(app, &id, image, &tooltip, cfg!(target_os = "linux").then_some(text.as_str()));
        }
        self.ids = wanted;
        let hide = cfg.hide_app_icon && !self.ids.is_empty();
        if hide != self.hidden_app_icon {
            crate::tray::set_app_icon_visible(app, !hide);
            self.hidden_app_icon = hide;
        }
    }

    pub fn clear(&mut self, app: &AppHandle) {
        for id in self.ids.drain(..) {
            crate::tray::remove_readout(app, &id);
        }
        if self.hidden_app_icon {
            crate::tray::set_app_icon_visible(app, true);
            self.hidden_app_icon = false;
        }
        self.last = None;
    }
}

/// Whether the taskbar or panel is dark, so readouts use light text.
#[cfg(windows)]
fn dark_panel() -> bool {
    use windows::{
        core::w,
        Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD},
    };
    let mut value = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    status.is_err() || value == 0
}

#[cfg(not(windows))]
fn dark_panel() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values() -> Values {
        Values {
            cpu: 42.0,
            mem: 63.0,
            pressure: Pressure::Elevated,
            gpu: Some(7.0),
            disk: Some((80.0, 256_000_000_000, 768_000_000_000)),
            disk_io: Some(1_500_000.0),
            down: 1_200_000.0,
            up: 45_000.0,
            battery: Some((85.0, Some(8_100), false)),
            power: Some(12.4),
            fan: None,
            temp: Some(72.0),
        }
    }

    fn cfg(readings: &[Reading]) -> Config {
        Config { readings: readings.to_vec(), style: Style::default(), interval: Duration::from_secs(2), hide_app_icon: false }
    }

    #[test]
    fn value_texts() {
        let v = values();
        let s = Style::default();
        assert_eq!(value_text(Reading::Cpu, &v, &s), ["42%"]);
        assert_eq!(value_text(Reading::Network, &v, &s), ["1.2M", "45K"]);
        let up = Style { up_first: true, bits: true, ..Style::default() };
        assert_eq!(value_text(Reading::Network, &v, &up), ["360K", "9.6M"]);
        assert_eq!(value_text(Reading::Disk, &v, &Style { disk: DiskMode::Available, ..Style::default() }), ["256G"]);
        assert_eq!(value_text(Reading::Battery, &v, &s), ["85%", "2:15"]);
        assert_eq!(value_text(Reading::Temperature, &v, &Style { fahrenheit: true, ..Style::default() }), ["162°"]);
        assert_eq!(value_text(Reading::Power, &v, &s), ["12W"]);
    }

    #[test]
    fn readings_without_hardware_are_not_offered() {
        let v = Values { battery: None, temp: None, ..values() };
        let c = cfg(&[Reading::Cpu, Reading::Battery, Reading::Fan, Reading::Temperature]);
        assert_eq!(shown(&c, &v).collect::<Vec<_>>(), vec![Reading::Cpu]);
        assert_eq!(label_text(&c, &v), "CPU 42%");
    }

    #[test]
    fn labels_join_with_spacing() {
        let v = values();
        let mut c = cfg(&[Reading::Cpu, Reading::Memory, Reading::Network]);
        assert_eq!(label_text(&c, &v), "CPU 42%  MEM 63%  NET ↓1.2M ↑45K");
        c.style.compact = true;
        assert_eq!(label_text(&c, &v), "CPU 42% MEM 63% NET ↓1.2M ↑45K");
    }

    #[test]
    fn every_value_fits_the_icon() {
        let v = Values { cpu: 100.0, down: 999_999_999.0, ..values() };
        for r in [Reading::Cpu, Reading::Memory, Reading::Disk, Reading::DiskActivity, Reading::Network, Reading::Battery, Reading::Power, Reading::Temperature] {
            for t in value_text(r, &v, &Style::default()) {
                assert!(text_width(&fit(&t, GRID), 1) <= GRID, "{r:?} {t}");
            }
        }
        assert_eq!(fit("100%", 12), "100");
    }

    #[test]
    fn draws_something_for_each_style() {
        let v = values();
        for bar in [false, true] {
            for compact in [false, true] {
                let style = Style { bar, compact, ..Style::default() };
                for r in [Reading::Cpu, Reading::Memory, Reading::Network, Reading::Battery, Reading::Disk] {
                    assert!(draw_single(r, &v, &style).lit() > 0, "{r:?} bar={bar} compact={compact}");
                }
                let c = Config { style: style.clone(), ..cfg(&[Reading::Cpu, Reading::Memory, Reading::Gpu, Reading::Network]) };
                let canvas = draw_group(&c, &v);
                assert_eq!(canvas.rgba.len(), (SIZE * SIZE * 4) as usize);
                assert!(canvas.lit() > 0);
            }
        }
    }

    #[test]
    fn reads_settings() {
        let mut s = Settings::default();
        assert!(Config::from_settings(&s).is_none());
        s.features.get_mut("monitor").unwrap().insert("enabled".into(), true.into());
        let t = s.features.get_mut("trayReadouts").unwrap();
        t.insert("enabled".into(), true.into());
        t.insert("readings".into(), serde_json::json!(["cpu", "bogus", "cpu", "network"]));
        t.insert("interval".into(), 0.2.into());
        let c = Config::from_settings(&s).unwrap();
        assert_eq!(c.readings, vec![Reading::Cpu, Reading::Network]);
        assert_eq!(c.interval, MIN_INTERVAL);
    }
}
