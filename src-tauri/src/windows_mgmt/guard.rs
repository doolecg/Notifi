//! Quit and close protection: decides what happens to a guarded shortcut. The keyboard hook asks it
//! about every Alt+F4, Ctrl+W and Ctrl+Q and acts on the answer.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combo {
    /// Ctrl+Q and Alt+F4.
    Quit,
    /// Ctrl+W.
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Hold,
    Twice,
    /// Add an extra modifier to the shortcut.
    Modifier,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "hold" => Some(Self::Hold),
            "twice" | "double" => Some(Self::Twice),
            "modifier" => Some(Self::Modifier),
            _ => None,
        }
    }
}

/// What the hook does with a key press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Let the real key through.
    Pass,
    /// Drop it.
    Swallow,
    /// Drop it and send the original shortcut ourselves (the extra modifier is still held).
    Replay,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tick {
    Idle,
    Progress(f32),
    /// The hold time has passed: send the shortcut.
    Fire(Combo),
}

/// Window within which a second press counts.
pub const TWICE_WINDOW_MS: u64 = 1500;

#[derive(Default)]
pub struct Guard {
    holding: Option<(Combo, u64)>,
    armed: Option<(Combo, u64)>,
    fired: bool,
}

impl Guard {
    /// A guarded shortcut went down. `extra_held` says whether the configured extra modifier is down.
    pub fn key_down(&mut self, now: u64, combo: Combo, mode: Mode, repeat: bool, extra_held: bool) -> Step {
        match mode {
            Mode::Modifier => {
                if extra_held { Step::Replay } else { Step::Swallow }
            }
            Mode::Twice => {
                if repeat {
                    return Step::Swallow;
                }
                match self.armed {
                    Some((c, t)) if c == combo && now.saturating_sub(t) <= TWICE_WINDOW_MS => {
                        self.armed = None;
                        Step::Pass
                    }
                    _ => {
                        self.armed = Some((combo, now));
                        Step::Swallow
                    }
                }
            }
            Mode::Hold => {
                if self.holding.is_none() && !repeat {
                    self.holding = Some((combo, now));
                    self.fired = false;
                }
                Step::Swallow
            }
        }
    }

    /// The main key of a guarded shortcut came up. True when that cancelled a hold in progress.
    pub fn key_up(&mut self, combo: Combo) -> bool {
        match self.holding {
            Some((c, _)) if c == combo => {
                self.holding = None;
                !std::mem::take(&mut self.fired)
            }
            _ => false,
        }
    }

    /// Called every few milliseconds while a hold is running.
    pub fn tick(&mut self, now: u64, hold_ms: u64) -> Tick {
        let Some((combo, start)) = self.holding else { return Tick::Idle };
        if self.fired {
            return Tick::Idle;
        }
        let held = now.saturating_sub(start);
        if held >= hold_ms {
            self.fired = true;
            return Tick::Fire(combo);
        }
        Tick::Progress(held as f32 / hold_ms.max(1) as f32)
    }

    pub fn holding(&self) -> bool {
        self.holding.is_some() && !self.fired
    }

}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtraModifier {
    Shift,
    Ctrl,
    Alt,
}

impl ExtraModifier {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "ctrl" => Self::Ctrl,
            "alt" => Self::Alt,
            _ => Self::Shift,
        }
    }
}

pub const VK_F4: u32 = 0x73;
pub const VK_Q: u32 = 0x51;
pub const VK_W: u32 = 0x57;

/// The guarded shortcut a key press belongs to, and whether the extra modifier is held on top of it.
/// `None` for any other shortcut, including the same key with a different modifier.
pub fn classify(vk: u32, alt: bool, ctrl: bool, shift: bool, extra: ExtraModifier) -> Option<(Combo, bool)> {
    let (combo, base_alt, base_ctrl) = match vk {
        VK_F4 => (Combo::Quit, true, false),
        VK_Q => (Combo::Quit, false, true),
        VK_W => (Combo::Close, false, true),
        _ => return None,
    };
    if base_alt != alt && !(extra == ExtraModifier::Alt && alt) || (base_ctrl && !ctrl) {
        return None;
    }
    // The extra modifier must be one the shortcut does not already use.
    let extra = match (extra, base_alt, base_ctrl) {
        (ExtraModifier::Alt, true, _) | (ExtraModifier::Ctrl, _, true) => ExtraModifier::Shift,
        (e, _, _) => e,
    };
    let held = (alt && !base_alt, ctrl && !base_ctrl, shift);
    match (held, extra) {
        ((false, false, false), _) => Some((combo, false)),
        ((true, false, false), ExtraModifier::Alt) | ((false, true, false), ExtraModifier::Ctrl) | ((false, false, true), ExtraModifier::Shift) => {
            Some((combo, true))
        }
        _ => None,
    }
}

/// A per-app rule: `app = twice`. App names compare without case or `.exe`.
pub fn parse_app_rule(line: &str) -> Option<(String, Mode)> {
    let (app, mode) = line.split_once('=')?;
    let app = normalise_app(app);
    (!app.is_empty()).then_some(())?;
    Some((app, Mode::parse(mode)?))
}

pub fn normalise_app(name: &str) -> String {
    let n = name.trim().to_lowercase();
    n.strip_suffix(".exe").map(str::to_string).unwrap_or(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn press_twice() {
        let mut g = Guard::default();
        assert_eq!(g.key_down(0, Combo::Quit, Mode::Twice, false, false), Step::Swallow);
        assert_eq!(g.key_down(500, Combo::Quit, Mode::Twice, false, false), Step::Pass);
        assert_eq!(g.key_down(600, Combo::Quit, Mode::Twice, false, false), Step::Swallow, "armed again from scratch");
    }

    #[test]
    fn the_second_press_has_to_come_in_time_and_for_the_same_shortcut() {
        let mut g = Guard::default();
        g.key_down(0, Combo::Quit, Mode::Twice, false, false);
        assert_eq!(g.key_down(TWICE_WINDOW_MS + 1, Combo::Quit, Mode::Twice, false, false), Step::Swallow);
        assert_eq!(g.key_down(TWICE_WINDOW_MS + 100, Combo::Close, Mode::Twice, false, false), Step::Swallow);
        assert_eq!(g.key_down(TWICE_WINDOW_MS + 200, Combo::Close, Mode::Twice, false, false), Step::Pass);
    }

    #[test]
    fn auto_repeat_never_counts_as_the_second_press() {
        let mut g = Guard::default();
        g.key_down(0, Combo::Close, Mode::Twice, false, false);
        assert_eq!(g.key_down(30, Combo::Close, Mode::Twice, true, false), Step::Swallow);
        assert_eq!(g.key_down(60, Combo::Close, Mode::Twice, true, false), Step::Swallow);
    }

    #[test]
    fn extra_modifier() {
        let mut g = Guard::default();
        assert_eq!(g.key_down(0, Combo::Quit, Mode::Modifier, false, false), Step::Swallow);
        assert_eq!(g.key_down(0, Combo::Quit, Mode::Modifier, false, true), Step::Replay);
    }

    #[test]
    fn hold_fires_once_after_the_hold_time() {
        let mut g = Guard::default();
        assert_eq!(g.key_down(1000, Combo::Quit, Mode::Hold, false, false), Step::Swallow);
        assert!(g.holding());
        assert_eq!(g.tick(1350, 700), Tick::Progress(0.5));
        assert_eq!(g.key_down(1400, Combo::Quit, Mode::Hold, true, false), Step::Swallow, "repeats are dropped");
        assert_eq!(g.tick(1700, 700), Tick::Fire(Combo::Quit));
        assert_eq!(g.tick(1800, 700), Tick::Idle, "only once per hold");
        assert!(!g.key_up(Combo::Quit), "releasing after firing cancels nothing");
        assert_eq!(g.tick(5000, 700), Tick::Idle);
    }

    #[test]
    fn releasing_early_cancels() {
        let mut g = Guard::default();
        g.key_down(0, Combo::Close, Mode::Hold, false, false);
        assert_eq!(g.tick(300, 700), Tick::Progress(300.0 / 700.0));
        assert!(g.key_up(Combo::Close));
        assert_eq!(g.tick(900, 700), Tick::Idle);
        assert!(!g.holding());
    }

    #[test]
    fn releasing_a_different_key_does_not_cancel() {
        let mut g = Guard::default();
        g.key_down(0, Combo::Close, Mode::Hold, false, false);
        assert!(!g.key_up(Combo::Quit));
        assert!(g.holding());
    }

    #[test]
    fn shortcuts_are_recognised_exactly() {
        use ExtraModifier::*;
        assert_eq!(classify(VK_F4, true, false, false, Shift), Some((Combo::Quit, false)));
        assert_eq!(classify(VK_Q, false, true, false, Shift), Some((Combo::Quit, false)));
        assert_eq!(classify(VK_W, false, true, false, Shift), Some((Combo::Close, false)));
        assert_eq!(classify(VK_W, false, true, true, Shift), Some((Combo::Close, true)), "Ctrl+Shift+W with Shift as the extra");
        assert_eq!(classify(VK_F4, true, false, true, Shift), Some((Combo::Quit, true)));
        assert_eq!(classify(VK_F4, true, false, false, Shift).map(|c| c.1), Some(false));
        assert_eq!(classify(VK_F4, false, false, false, Shift), None, "F4 alone");
        assert_eq!(classify(VK_W, false, false, false, Shift), None, "W alone");
        assert_eq!(classify(VK_F4, true, true, false, Shift), None, "Ctrl+Alt+F4 is another shortcut");
        assert_eq!(classify(VK_W, false, true, true, Alt), None, "Shift is not the configured extra");
        assert_eq!(classify(0x41, false, true, false, Shift), None);
        // An extra that the shortcut already uses falls back to Shift.
        assert_eq!(classify(VK_W, false, true, true, Ctrl), Some((Combo::Close, true)));
        assert_eq!(classify(VK_F4, true, false, true, Alt), Some((Combo::Quit, true)));
    }

    #[test]
    fn app_rules_parse() {
        assert_eq!(parse_app_rule("Chrome.exe = twice"), Some(("chrome".into(), Mode::Twice)));
        assert_eq!(parse_app_rule("code=Hold"), Some(("code".into(), Mode::Hold)));
        assert_eq!(parse_app_rule("code = never"), None);
        assert_eq!(parse_app_rule(" = hold"), None);
        assert_eq!(parse_app_rule("nothing"), None);
        assert_eq!(normalise_app(" Notepad.EXE "), "notepad");
    }
}
