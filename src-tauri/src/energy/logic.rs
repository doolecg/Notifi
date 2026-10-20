//! Decisions that do not touch the OS, so they can be tested on their own.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Combine {
    Any,
    All,
}

impl Combine {
    pub fn parse(value: &str) -> Self {
        if value == "all" { Self::All } else { Self::Any }
    }
}

/// `results` holds one entry per trigger the user selected. No selected trigger never holds.
pub fn triggers_hold(combine: Combine, results: &[bool]) -> bool {
    match combine {
        _ if results.is_empty() => false,
        Combine::Any => results.iter().any(|r| *r),
        Combine::All => results.iter().all(|r| *r),
    }
}

/// On battery power below the cutoff level.
pub fn battery_blocks(cutoff_on: bool, on_ac: bool, percent: f32, cutoff: f32) -> bool {
    cutoff_on && !on_ac && percent < cutoff
}

/// Whether the machine should be held awake right now.
pub fn session_wanted(manual: bool, trigger: bool, locked: bool, pause_on_lock: bool) -> bool {
    (manual || trigger) && !(locked && pause_on_lock)
}

/// A running process matches a wanted name ignoring case and `.exe`; helper processes whose name
/// starts with the wanted name count too.
pub fn app_matches(process: &str, wanted: &str) -> bool {
    let strip = |s: &str| s.trim().to_lowercase().trim_end_matches(".exe").to_string();
    let (process, wanted) = (strip(process), strip(wanted));
    !wanted.is_empty() && (process == wanted || process.starts_with(&format!("{wanted} ")) || process.starts_with(&format!("{wanted}_")) || process.starts_with(&format!("{wanted}-")))
}

pub fn nudge_due(last_ms: Option<u64>, now_ms: u64, interval_s: u64) -> bool {
    last_ms.is_none_or(|last| now_ms.saturating_sub(last) >= interval_s.saturating_mul(1000))
}

pub fn time_left_text(ms: u64) -> String {
    let secs = ms.div_ceil(1000);
    match secs {
        0..=59 => format!("{secs} s left"),
        60..=3599 => format!("{} min left", secs.div_ceil(60)),
        _ => {
            let minutes = secs / 60;
            format!("{} h {} min left", minutes / 60, minutes % 60)
        }
    }
}

pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// Recolours every visible pixel, keeping its transparency.
pub fn tint(rgba: &mut [u8], rgb: [u8; 3]) {
    for px in rgba.chunks_exact_mut(4).filter(|p| p[3] > 0) {
        px[..3].copy_from_slice(&rgb);
    }
}

/// Brightness change per key press, in percent.
pub fn key_step(step: &str) -> f32 {
    match step {
        "half" => 5.0,
        "quarter" => 2.5,
        _ => 10.0,
    }
}

/// One slider scale for hardware and software dimming. `level` is 0..=100 on the combined scale;
/// the bottom `share` percent is software dimming. Returns (hardware percent, black overlay opacity 0..=1).
pub fn split_level(level: f32, share: f32, floor: f32) -> (f32, f32) {
    let share = share.clamp(1.0, 99.0);
    let level = level.clamp(floor.clamp(0.0, share), 100.0);
    if level >= share {
        ((level - share) / (100.0 - share) * 100.0, 0.0)
    } else {
        (0.0, (share - level) / share)
    }
}

/// The combined-scale level for a hardware percent and overlay opacity (the inverse of [`split_level`]).
pub fn combined_level(hardware: f32, dim: f32, share: f32) -> f32 {
    let share = share.clamp(1.0, 99.0);
    if dim > 0.0 { share * (1.0 - dim) } else { share + hardware / 100.0 * (100.0 - share) }
}

/// Index of the next (or previous) entry, wrapping round.
pub fn cycle_index(len: usize, current: usize, forward: bool) -> usize {
    if len == 0 {
        0
    } else if forward {
        (current + 1) % len
    } else {
        (current + len - 1) % len
    }
}

/// Counts presses of the unlock key inside a time window.
pub struct UnlockCounter {
    need: u32,
    window_ms: u64,
    hits: Vec<u64>,
}

impl UnlockCounter {
    pub fn new(need: u32, window_ms: u64) -> Self {
        Self { need: need.max(1), window_ms, hits: Vec::new() }
    }

    /// Records a press; true when it completes the sequence.
    pub fn press(&mut self, now_ms: u64) -> bool {
        self.hits.retain(|t| now_ms.saturating_sub(*t) <= self.window_ms);
        self.hits.push(now_ms);
        if self.hits.len() as u32 >= self.need {
            self.hits.clear();
            true
        } else {
            false
        }
    }

    pub fn reset(&mut self) {
        self.hits.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_rules() {
        assert!(!triggers_hold(Combine::Any, &[]));
        assert!(!triggers_hold(Combine::All, &[]));
        assert!(triggers_hold(Combine::Any, &[false, true]));
        assert!(!triggers_hold(Combine::All, &[false, true]));
        assert!(triggers_hold(Combine::All, &[true, true]));
        assert_eq!(Combine::parse("all"), Combine::All);
        assert_eq!(Combine::parse("other"), Combine::Any);
    }

    #[test]
    fn battery_cutoff() {
        assert!(battery_blocks(true, false, 19.0, 20.0));
        assert!(!battery_blocks(true, false, 20.0, 20.0));
        assert!(!battery_blocks(true, true, 5.0, 20.0));
        assert!(!battery_blocks(false, false, 5.0, 20.0));
    }

    #[test]
    fn session_and_lock() {
        assert!(session_wanted(true, false, false, true));
        assert!(session_wanted(false, true, false, false));
        assert!(!session_wanted(true, true, true, true));
        assert!(session_wanted(true, false, true, false));
        assert!(!session_wanted(false, false, false, false));
    }

    #[test]
    fn app_names() {
        assert!(app_matches("Code.exe", "code"));
        assert!(app_matches("chrome_crashpad", "Chrome"));
        assert!(app_matches("slack helper", "slack.exe"));
        assert!(!app_matches("codex", "code"));
        assert!(!app_matches("anything", " "));
    }

    #[test]
    fn nudge_timing() {
        assert!(nudge_due(None, 0, 60));
        assert!(!nudge_due(Some(1_000), 30_000, 60));
        assert!(nudge_due(Some(1_000), 61_000, 60));
    }

    #[test]
    fn time_text() {
        assert_eq!(time_left_text(30_000), "30 s left");
        assert_eq!(time_left_text(61_000), "2 min left");
        assert_eq!(time_left_text(3_660_000), "1 h 1 min left");
    }

    #[test]
    fn hex_colours() {
        assert_eq!(parse_hex("#4C8DFF"), Some([0x4C, 0x8D, 0xFF]));
        assert_eq!(parse_hex("4c8dff"), Some([0x4C, 0x8D, 0xFF]));
        assert_eq!(parse_hex("#fff"), None);
        assert_eq!(parse_hex("zzzzzz"), None);
        let mut px = [1, 2, 3, 255, 9, 9, 9, 0];
        tint(&mut px, [10, 20, 30]);
        assert_eq!(px, [10, 20, 30, 255, 9, 9, 9, 0]);
    }

    #[test]
    fn dim_split() {
        assert_eq!(split_level(100.0, 25.0, 5.0), (100.0, 0.0));
        assert_eq!(split_level(25.0, 25.0, 5.0), (0.0, 0.0));
        let (hw, dim) = split_level(15.0, 25.0, 5.0);
        assert_eq!(hw, 0.0);
        assert!((dim - 0.4).abs() < 1e-6);
        // the floor keeps the screen from going black
        let (_, deepest) = split_level(0.0, 25.0, 5.0);
        assert!((deepest - 0.8).abs() < 1e-6);
        assert!((combined_level(0.0, 0.4, 25.0) - 15.0).abs() < 1e-4);
        assert!((combined_level(50.0, 0.0, 20.0) - 60.0).abs() < 1e-4);
    }

    #[test]
    fn steps_and_cycle() {
        assert_eq!(key_step("full"), 10.0);
        assert_eq!(key_step("quarter"), 2.5);
        assert_eq!(cycle_index(3, 2, true), 0);
        assert_eq!(cycle_index(3, 0, false), 2);
        assert_eq!(cycle_index(0, 0, true), 0);
    }

    #[test]
    fn unlock_presses() {
        let mut c = UnlockCounter::new(3, 2_000);
        assert!(!c.press(0));
        assert!(!c.press(500));
        assert!(c.press(1_000));
        assert!(!c.press(1_100));
        assert!(!c.press(4_000), "old presses fall out of the window");
        c.reset();
        assert!(!c.press(4_100));
        assert!(!c.press(4_200));
        assert!(c.press(4_300));
    }
}
