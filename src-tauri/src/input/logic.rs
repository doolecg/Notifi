//! The pure parts of the input features: filters, button and key maps, wheel routing and the glide
//! curve. Nothing here touches the OS, so all of it is tested.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Btn {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

impl Btn {
    pub fn index(self) -> usize {
        self as usize
    }

    /// Buttons are numbered the usual way: 1 left, 2 right, 3 middle, 4 back, 5 forward.
    pub fn from_number(n: u32) -> Option<Btn> {
        Some(match n {
            1 => Btn::Left,
            2 => Btn::Right,
            3 => Btn::Middle,
            4 => Btn::Back,
            5 => Btn::Forward,
            _ => return None,
        })
    }

    pub fn from_gesture_name(name: &str) -> Option<Btn> {
        match name {
            "right" => Some(Btn::Right),
            "middle" => Some(Btn::Middle),
            "button4" => Some(Btn::Back),
            "button5" => Some(Btn::Forward),
            _ => None,
        }
    }
}

/// Drops a press that follows the previous event of the same button too closely; the release of a
/// dropped press is dropped with it.
#[derive(Default)]
pub struct ClickFilter {
    last: [Option<u32>; 5],
    swallow_up: [bool; 5],
}

impl ClickFilter {
    /// `true` lets the press through. Times are the hook's millisecond ticks.
    pub fn down(&mut self, b: Btn, now: u32, threshold: u32) -> bool {
        let i = b.index();
        if self.last[i].is_some_and(|t| now.wrapping_sub(t) < threshold) {
            self.swallow_up[i] = true;
            return false;
        }
        self.swallow_up[i] = false;
        self.last[i] = Some(now);
        true
    }

    pub fn up(&mut self, b: Btn, now: u32) -> bool {
        let i = b.index();
        if std::mem::take(&mut self.swallow_up[i]) {
            return false;
        }
        self.last[i] = Some(now);
        true
    }
}

/// Drops a second press of a key that comes too soon after the previous press or release of it.
/// Holding a key (repeat presses without a release) is never bounce.
pub struct KeyDebounce {
    last: [Option<u32>; 256],
    down: [bool; 256],
    dropped: [bool; 256],
}

impl Default for KeyDebounce {
    fn default() -> Self {
        Self { last: [None; 256], down: [false; 256], dropped: [false; 256] }
    }
}

impl KeyDebounce {
    /// `true` lets the event through.
    pub fn event(&mut self, vk: u32, down: bool, now: u32, interval: u32) -> bool {
        let i = (vk & 0xFF) as usize;
        if down {
            if self.down[i] {
                return !self.dropped[i];
            }
            self.down[i] = true;
            if self.last[i].is_some_and(|t| now.wrapping_sub(t) < interval) {
                self.dropped[i] = true;
                return false;
            }
            self.dropped[i] = false;
            self.last[i] = Some(now);
            true
        } else {
            let was_dropped = std::mem::take(&mut self.dropped[i]);
            self.down[i] = false;
            if was_dropped {
                return false;
            }
            self.last[i] = Some(now);
            true
        }
    }
}

// ---- Keys and actions ----

pub const VK_LCTRL: u16 = 0xA2;
pub const VK_LALT: u16 = 0xA4;
pub const VK_LSHIFT: u16 = 0xA0;
pub const VK_LWIN: u16 = 0x5B;

/// What a button, gesture or tap does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Keys pressed in order and released in reverse: modifiers first, then the key.
    Keys(Vec<u16>),
    Click(Btn),
    /// Swallows the button and does nothing.
    Nothing,
}

pub fn is_modifier_vk(vk: u16) -> bool {
    matches!(vk, 0x10..=0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
}

/// Keys that need the extended flag so they are not read as their numpad twins.
pub fn is_extended_vk(vk: u16) -> bool {
    matches!(vk, 0x21..=0x28 | 0x2D | 0x2E | 0x5B | 0x5C | 0xA3 | 0xA5 | 0x6F)
}

fn key_vk(token: &str) -> Option<u16> {
    let t = token.to_lowercase();
    let mut chars = t.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        match c {
            'a'..='z' => return Some(0x41 + (c as u16 - 'a' as u16)),
            '0'..='9' => return Some(0x30 + (c as u16 - '0' as u16)),
            _ => {}
        }
    }
    if let Some(n) = t.strip_prefix('f').and_then(|n| n.parse::<u16>().ok()) {
        if (1..=24).contains(&n) {
            return Some(0x6F + n);
        }
    }
    Some(match t.as_str() {
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "escape" | "esc" => 0x1B,
        "enter" | "return" => 0x0D,
        "tab" => 0x09,
        "space" => 0x20,
        "backspace" => 0x08,
        "delete" | "del" => 0x2E,
        "insert" | "ins" => 0x2D,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" | "pgup" => 0x21,
        "pagedown" | "pgdn" => 0x22,
        "minus" => 0xBD,
        "equals" => 0xBB,
        "comma" => 0xBC,
        "period" => 0xBE,
        "capslock" => 0x14,
        _ => return None,
    })
}

/// "Ctrl+Alt+Left" into key codes, modifiers first. `None` when a token is unknown or there is
/// more than one non-modifier key.
pub fn parse_combo(text: &str) -> Option<Vec<u16>> {
    let mut mods: Vec<u16> = Vec::new();
    let mut key: Option<u16> = None;
    for token in text.split('+').map(str::trim) {
        let vk = match token.to_lowercase().as_str() {
            "ctrl" | "control" => VK_LCTRL,
            "alt" | "option" => VK_LALT,
            "shift" => VK_LSHIFT,
            "super" | "win" | "windows" | "meta" | "cmd" => VK_LWIN,
            _ => {
                if key.is_some() {
                    return None;
                }
                key = Some(key_vk(token)?);
                continue;
            }
        };
        if !mods.contains(&vk) {
            mods.push(vk);
        }
    }
    mods.extend(key);
    (!mods.is_empty()).then_some(mods)
}

pub fn parse_action(text: &str) -> Option<Action> {
    let t = text.trim();
    let named = |keys: &[u16]| Some(Action::Keys(keys.to_vec()));
    match t.to_lowercase().as_str() {
        "none" | "nothing" | "disabled" | "ignore" | "do nothing" => Some(Action::Nothing),
        "middle click" => Some(Action::Click(Btn::Middle)),
        "previous desktop" => named(&[VK_LCTRL, VK_LWIN, 0x25]),
        "next desktop" => named(&[VK_LCTRL, VK_LWIN, 0x27]),
        "overview" | "task view" | "mission control" => named(&[VK_LWIN, 0x09]),
        "show desktop" => named(&[VK_LWIN, 0x44]),
        "back" => named(&[VK_LALT, 0x25]),
        "forward" => named(&[VK_LALT, 0x27]),
        "volume up" => named(&[0xAF]),
        "volume down" => named(&[0xAE]),
        "mute" => named(&[0xAD]),
        "play/pause" | "play pause" => named(&[0xB3]),
        "next track" => named(&[0xB0]),
        "previous track" => named(&[0xB1]),
        "switch language" => named(&[VK_LWIN, 0x20]),
        _ => parse_combo(t).map(Action::Keys),
    }
}

fn parse_button(text: &str) -> Option<Btn> {
    let t = text.trim().to_lowercase();
    if let Some(n) = t.strip_prefix("button") {
        return Btn::from_number(n.trim().parse().ok()?);
    }
    match t.as_str() {
        "middle" => Some(Btn::Middle),
        "back" => Some(Btn::Back),
        "forward" => Some(Btn::Forward),
        _ => None,
    }
}

/// "Button 4 = Alt+Left". The left button is never remapped.
pub fn parse_button_line(line: &str) -> Option<(Btn, Action)> {
    let (b, a) = line.split_once('=')?;
    let btn = parse_button(b)?;
    (btn != Btn::Left).then_some(())?;
    Some((btn, parse_action(a)?))
}

/// "firefox: Button 4 = Ctrl+W". The app may be a path, so the split is at the first colon that is
/// followed by a button.
pub fn parse_override_line(line: &str) -> Option<(String, Btn, Action)> {
    for (i, _) in line.match_indices(':') {
        let rest = line[i + 1..].trim_start();
        if rest.to_lowercase().starts_with("button") || parse_button(rest.split('=').next().unwrap_or("")).is_some() {
            let app = normalise_app(&line[..i]);
            let (btn, action) = parse_button_line(rest)?;
            return (!app.is_empty()).then_some((app, btn, action));
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// "Left = Previous desktop".
pub fn parse_gesture_line(line: &str) -> Option<(Dir, Action)> {
    let (d, a) = line.split_once('=')?;
    let d = d.trim().to_lowercase();
    let dir = match d.strip_prefix("drag ").unwrap_or(&d) {
        "left" => Dir::Left,
        "right" => Dir::Right,
        "up" => Dir::Up,
        "down" => Dir::Down,
        _ => return None,
    };
    Some((dir, parse_action(a)?))
}

pub fn gesture_map(lines: &[String]) -> HashMap<Dir, Action> {
    lines.iter().filter_map(|l| parse_gesture_line(l)).collect()
}

/// The direction of a drag once it has moved `threshold` pixels along its longer axis.
pub fn drag_direction(dx: i32, dy: i32, threshold: i32) -> Option<Dir> {
    if dx.abs().max(dy.abs()) < threshold {
        return None;
    }
    Some(if dx.abs() >= dy.abs() {
        if dx < 0 { Dir::Left } else { Dir::Right }
    } else if dy < 0 {
        Dir::Up
    } else {
        Dir::Down
    })
}

// ---- App lists ----

/// Lower case, no `.exe`, forward slashes made back slashes.
pub fn normalise_app(text: &str) -> String {
    let t = text.trim().to_lowercase().replace('/', "\\");
    t.strip_suffix(".exe").unwrap_or(&t).to_string()
}

/// `path` is a full program path; an entry matches its name or the whole path.
pub fn app_listed(list: &[String], path: &str) -> bool {
    if list.is_empty() || path.is_empty() {
        return false;
    }
    let full = normalise_app(path);
    let name = full.rsplit('\\').next().unwrap_or(&full);
    list.iter().any(|a| a == name || *a == full)
}

// ---- Wheel ----

/// One physical notch.
pub const NOTCH: i32 = 120;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollCfg {
    pub invert_vertical: bool,
    pub invert_horizontal: bool,
    /// Sideways scrolling is on and its key is down for this event.
    pub sideways: bool,
    /// Lines per notch asked for, with the system's own lines (or characters) per notch.
    pub linear: Option<(f64, f64)>,
    pub smooth: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WheelOut {
    Pass,
    Replace { horizontal: bool, delta: i32 },
    Glide { horizontal: bool, delta: i32 },
}

/// Decides what to do with a wheel event. Deltas that are not whole notches come from touchpads and
/// high-resolution wheels and are left alone.
pub fn route_wheel(cfg: &ScrollCfg, horizontal: bool, delta: i32) -> WheelOut {
    if delta == 0 || delta % NOTCH != 0 {
        return WheelOut::Pass;
    }
    let (mut h, mut d) = (horizontal, delta);
    if (h && cfg.invert_horizontal) || (!h && cfg.invert_vertical) {
        d = -d;
    }
    if !h && cfg.sideways {
        h = true;
        d = -d;
    }
    if !cfg.smooth {
        if let Some((lines, system)) = cfg.linear {
            d = d.signum() * (f64::from(NOTCH) * lines / system.max(1.0)).round().max(1.0) as i32;
        }
    }
    if cfg.smooth {
        return WheelOut::Glide { horizontal: h, delta: d };
    }
    if h == horizontal && d == delta {
        WheelOut::Pass
    } else {
        WheelOut::Replace { horizontal: h, delta: d }
    }
}

/// An eased scroll: the remaining distance shrinks by a fixed share each tick. Every unit put in
/// comes out as whole wheel units, so the page ends exactly where the wheel asked.
#[derive(Default, Debug)]
pub struct Glide {
    remaining: f64,
    carry: f64,
}

impl Glide {
    pub fn push(&mut self, delta: f64) {
        if delta * self.remaining < 0.0 {
            self.remaining = 0.0;
            self.carry = 0.0;
        }
        self.remaining += delta;
    }

    pub fn idle(&self) -> bool {
        self.remaining == 0.0 && self.carry == 0.0
    }

    /// Wheel units to send for the last `dt_ms`; `tau_ms` is how long the glide takes to close
    /// about two thirds of the gap.
    pub fn step(&mut self, dt_ms: f64, tau_ms: f64) -> i32 {
        let part = self.remaining * (1.0 - (-dt_ms / tau_ms.max(1.0)).exp());
        self.remaining -= part;
        self.carry += part;
        if self.remaining.abs() < 1.0 {
            self.carry += self.remaining;
            self.remaining = 0.0;
            let out = self.carry.round();
            self.carry = 0.0;
            return out as i32;
        }
        let out = self.carry.trunc();
        self.carry -= out;
        out as i32
    }
}

/// Distance per notch relative to a normal notch.
pub fn speed_factor(speed: &str) -> f64 {
    match speed {
        "slow" => 0.7,
        "fast" => 1.5,
        _ => 1.0,
    }
}

/// Glide time constant: quicker with a high response, longer with more coast.
pub fn glide_tau(response: f64, coast: &str) -> f64 {
    let base = 180.0 - 1.4 * response.clamp(0.0, 100.0);
    base + match coast {
        "low" => 30.0,
        "medium" => 70.0,
        "high" => 140.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_filter_drops_fast_repeat_and_its_release() {
        let mut f = ClickFilter::default();
        assert!(f.down(Btn::Left, 1000, 40));
        assert!(f.up(Btn::Left, 1020));
        assert!(!f.down(Btn::Left, 1030, 40));
        assert!(!f.up(Btn::Left, 1035));
        assert!(f.down(Btn::Left, 1100, 40));
        assert!(f.up(Btn::Left, 1180));
        assert!(f.down(Btn::Right, 1181, 40), "other buttons are separate");
    }

    #[test]
    fn click_filter_keeps_drags_and_survives_tick_wrap() {
        let mut f = ClickFilter::default();
        assert!(f.down(Btn::Left, 10, 40));
        assert!(f.up(Btn::Left, 5000));
        assert!(f.down(Btn::Left, u32::MAX - 5, 40));
        assert!(f.up(Btn::Left, 3));
        assert!(!f.down(Btn::Left, 10, 40));
    }

    #[test]
    fn debounce_drops_bounce_not_repeat() {
        let mut d = KeyDebounce::default();
        assert!(d.event(0x41, true, 100, 30));
        assert!(d.event(0x41, true, 600, 30), "auto repeat passes");
        assert!(d.event(0x41, false, 700, 30));
        assert!(!d.event(0x41, true, 715, 30));
        assert!(!d.event(0x41, true, 740, 30), "repeat of a dropped press stays dropped");
        assert!(!d.event(0x41, false, 745, 30));
        assert!(d.event(0x41, true, 900, 30));
        assert!(d.event(0x42, true, 901, 30), "other keys are separate");
    }

    #[test]
    fn parses_combos_and_actions() {
        assert_eq!(parse_combo("Ctrl+Alt+Left"), Some(vec![VK_LCTRL, VK_LALT, 0x25]));
        assert_eq!(parse_combo("ctrl+shift+t"), Some(vec![VK_LCTRL, VK_LSHIFT, 0x54]));
        assert_eq!(parse_combo("Ctrl+Alt+Shift+Super"), Some(vec![VK_LCTRL, VK_LALT, VK_LSHIFT, VK_LWIN]));
        assert_eq!(parse_combo("F5"), Some(vec![0x74]));
        assert_eq!(parse_combo("Escape"), Some(vec![0x1B]));
        assert_eq!(parse_combo("A+B"), None);
        assert_eq!(parse_combo("Blah"), None);
        assert_eq!(parse_combo(""), None);
        assert_eq!(parse_action("Previous desktop"), Some(Action::Keys(vec![VK_LCTRL, VK_LWIN, 0x25])));
        assert_eq!(parse_action("none"), Some(Action::Nothing));
        assert_eq!(parse_action("Middle click"), Some(Action::Click(Btn::Middle)));
    }

    #[test]
    fn parses_button_lines() {
        assert_eq!(parse_button_line("Button 4 = Alt+Left"), Some((Btn::Back, Action::Keys(vec![VK_LALT, 0x25]))));
        assert_eq!(parse_button_line("button 3 = none"), Some((Btn::Middle, Action::Nothing)));
        assert_eq!(parse_button_line("Button 1 = Ctrl+C"), None);
        assert_eq!(parse_button_line("Button 9 = Ctrl+C"), None);
        let (app, b, a) = parse_override_line("Firefox.exe: Button 4 = Ctrl+W").unwrap();
        assert_eq!((app.as_str(), b), ("firefox", Btn::Back));
        assert_eq!(a, Action::Keys(vec![VK_LCTRL, 0x57]));
        let (app, ..) = parse_override_line("C:\\Tools\\app.exe: Button 5 = Ctrl+W").unwrap();
        assert_eq!(app, "c:\\tools\\app");
        assert!(parse_override_line("firefox Button 4").is_none());
    }

    #[test]
    fn gestures() {
        assert_eq!(parse_gesture_line("Left = Previous desktop").map(|g| g.0), Some(Dir::Left));
        assert_eq!(parse_gesture_line("drag up = Overview").map(|g| g.0), Some(Dir::Up));
        assert!(parse_gesture_line("Sideways = Overview").is_none());
        assert_eq!(drag_direction(-70, 10, 60), Some(Dir::Left));
        assert_eq!(drag_direction(5, 80, 60), Some(Dir::Down));
        assert_eq!(drag_direction(30, -30, 60), None);
    }

    #[test]
    fn app_lists_match_names_and_paths() {
        let list = vec!["firefox".to_string(), "c:\\tools\\app".to_string()];
        assert!(app_listed(&list, "C:\\Program Files\\Mozilla Firefox\\firefox.exe"));
        assert!(app_listed(&list, "C:\\Tools\\App.exe"));
        assert!(!app_listed(&list, "C:\\Other\\app.exe"));
        assert!(!app_listed(&[], "C:\\x.exe"));
    }

    #[test]
    fn wheel_routing() {
        let none = ScrollCfg::default();
        assert_eq!(route_wheel(&none, false, 120), WheelOut::Pass);
        assert_eq!(route_wheel(&none, false, 30), WheelOut::Pass);
        let inv = ScrollCfg { invert_vertical: true, ..none };
        assert_eq!(route_wheel(&inv, false, 120), WheelOut::Replace { horizontal: false, delta: -120 });
        assert_eq!(route_wheel(&inv, true, 120), WheelOut::Pass);
        assert_eq!(route_wheel(&inv, false, 45), WheelOut::Pass, "touchpad deltas are untouched");
        let side = ScrollCfg { sideways: true, ..none };
        assert_eq!(route_wheel(&side, false, -120), WheelOut::Replace { horizontal: true, delta: 120 });
        let lin = ScrollCfg { linear: Some((6.0, 3.0)), ..none };
        assert_eq!(route_wheel(&lin, false, 360), WheelOut::Replace { horizontal: false, delta: 240 });
        assert_eq!(route_wheel(&lin, false, -240), WheelOut::Pass, "already the target step");
        let same = ScrollCfg { linear: Some((3.0, 3.0)), ..none };
        assert_eq!(route_wheel(&same, false, 120), WheelOut::Pass);
        assert_eq!(route_wheel(&same, false, 480), WheelOut::Replace { horizontal: false, delta: 120 }, "acceleration removed");
        let smooth = ScrollCfg { smooth: true, linear: Some((9.0, 3.0)), ..none };
        assert_eq!(route_wheel(&smooth, false, 120), WheelOut::Glide { horizontal: false, delta: 120 }, "smooth wins over linear");
    }

    #[test]
    fn glide_keeps_the_whole_distance() {
        let mut g = Glide::default();
        g.push(360.0);
        let (mut total, mut ticks) = (0, 0);
        while !g.idle() && ticks < 10_000 {
            total += g.step(8.0, 90.0);
            ticks += 1;
        }
        assert_eq!(total, 360);
        assert!(ticks > 5, "spread over several ticks");
        g.push(-100.0);
        g.push(-20.0);
        let mut total = 0;
        while !g.idle() {
            total += g.step(8.0, 40.0);
        }
        assert_eq!(total, -120);
    }

    #[test]
    fn glide_reverses_at_once() {
        let mut g = Glide::default();
        g.push(240.0);
        g.step(8.0, 90.0);
        g.push(-120.0);
        let mut total = 0;
        while !g.idle() {
            total += g.step(8.0, 90.0);
        }
        assert!(total <= 0);
    }

    #[test]
    fn glide_curve_follows_settings() {
        assert!(glide_tau(100.0, "off") < glide_tau(0.0, "off"));
        assert!(glide_tau(50.0, "high") > glide_tau(50.0, "low"));
        assert!(speed_factor("fast") > speed_factor("medium") && speed_factor("medium") > speed_factor("slow"));
    }
}
