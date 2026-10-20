//! The switcher's hold-to-switch keyboard logic, shared by the Windows hook and the X11 grab.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl Mods {
    fn without_shift(self) -> Self {
        Self { shift: false, ..self }
    }

    /// Every modifier in `need` is held.
    fn holds(self, need: Mods) -> bool {
        (!need.ctrl || self.ctrl) && (!need.alt || self.alt) && (!need.shift || self.shift) && (!need.meta || self.meta)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Tab,
    Backquote,
    Space,
    Escape,
    Enter,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    /// A letter (lowercase), digit or punctuation that types itself.
    Char(char),
    F(u8),
    /// A key the switcher has no use for.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accel {
    pub mods: Mods,
    pub key: Key,
}

/// Parses "Alt+Tab", "Ctrl+Alt+`", "Super+Shift+Space"; `None` for empty or unknown keys.
pub fn parse(accel: &str) -> Option<Accel> {
    let mut mods = Mods::default();
    let mut key = None;
    for part in accel.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "cmdorctrl" | "commandorcontrol" => mods.ctrl = true,
            "alt" | "option" => mods.alt = true,
            "shift" => mods.shift = true,
            "super" | "win" | "meta" | "cmd" | "command" => mods.meta = true,
            other => key = Some(key_name(other)?),
        }
    }
    key.map(|key| Accel { mods, key })
}

fn key_name(name: &str) -> Option<Key> {
    Some(match name {
        "tab" => Key::Tab,
        "`" | "backquote" | "grave" | "backtick" => Key::Backquote,
        "space" => Key::Space,
        "esc" | "escape" => Key::Escape,
        "enter" | "return" => Key::Enter,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "left" | "arrowleft" => Key::Left,
        "right" | "arrowright" => Key::Right,
        "up" | "arrowup" => Key::Up,
        "down" | "arrowdown" => Key::Down,
        f if f.len() > 1 && f.starts_with('f') && f[1..].parse::<u8>().is_ok() => Key::F(f[1..].parse().ok()?),
        k if k.starts_with("key") && k.len() == 4 => Key::Char(k.chars().nth(3)?),
        k if k.starts_with("digit") && k.len() == 6 => Key::Char(k.chars().nth(5)?),
        k if k.chars().count() == 1 => Key::Char(k.chars().next()?),
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Open the switcher; `same_app` lists only the foreground app's windows.
    Open { reverse: bool, same_app: bool },
    Step(isize),
    StepWindow(isize),
    Arrow(isize, isize),
    Type(char),
    Backspace,
    Commit,
    Cancel,
    CloseWindow,
    QuitApp,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Bindings {
    pub switcher: Option<Accel>,
    pub same_app: Option<Accel>,
}

/// Per-hook state: which trigger is being held, and which key presses were swallowed.
#[derive(Debug, Default)]
pub struct Hook {
    held: Option<Mods>,
    swallowed: Vec<Key>,
}

impl Hook {
    /// One non-modifier key event, with the modifiers held after it. Returns whether to swallow
    /// the event and what the switcher should do.
    pub fn key(&mut self, b: &Bindings, open: bool, key: Key, down: bool, mods: Mods) -> (bool, Option<Command>) {
        if !down {
            let i = self.swallowed.iter().position(|k| *k == key);
            return (i.map(|i| self.swallowed.remove(i)).is_some(), None);
        }
        let hit = |a: Option<Accel>| a.is_some_and(|a| a.key == key && mods.holds(a.mods) && extra_is_shift(mods, a.mods));
        let cmd = if hit(b.switcher) {
            let reverse = mods.shift && !b.switcher.is_some_and(|a| a.mods.shift);
            if open {
                Some(Command::Step(if reverse { -1 } else { 1 }))
            } else {
                self.held = b.switcher.map(|a| a.mods);
                Some(Command::Open { reverse, same_app: false })
            }
        } else if hit(b.same_app) {
            let reverse = mods.shift && !b.same_app.is_some_and(|a| a.mods.shift);
            if open {
                Some(Command::StepWindow(if reverse { -1 } else { 1 }))
            } else {
                self.held = b.same_app.map(|a| a.mods);
                Some(Command::Open { reverse, same_app: true })
            }
        } else if open {
            Some(match key {
                Key::Escape => Command::Cancel,
                Key::Enter => Command::Commit,
                Key::Left => Command::Arrow(-1, 0),
                Key::Right => Command::Arrow(1, 0),
                Key::Up => Command::Arrow(0, -1),
                Key::Down => Command::Arrow(0, 1),
                Key::Tab => Command::Step(if mods.shift { -1 } else { 1 }),
                Key::Backspace | Key::Delete => Command::Backspace,
                Key::Char('w') if self.held.is_some() => Command::CloseWindow,
                Key::Char('q') if self.held.is_some() => Command::QuitApp,
                Key::Char(c) => Command::Type(if mods.shift { c.to_ascii_uppercase() } else { c }),
                Key::Space => Command::Type(' '),
                _ => {
                    self.swallow(key);
                    return (true, None);
                }
            })
        } else {
            return (false, None);
        };
        self.swallow(key);
        (true, cmd)
    }

    fn swallow(&mut self, key: Key) {
        self.swallowed.push(key);
        if self.swallowed.len() > 16 {
            self.swallowed.remove(0);
        }
    }

    /// A modifier changed. Releasing the held trigger's modifiers commits.
    pub fn modifiers(&mut self, open: bool, mods: Mods) -> Option<Command> {
        let need = self.held?;
        if mods.without_shift().holds(need.without_shift()) {
            return None;
        }
        self.held = None;
        open.then_some(Command::Commit)
    }

    /// The switcher closed for another reason (click, focus change); forget the held trigger.
    pub fn reset(&mut self) {
        self.held = None;
    }

    pub fn holding(&self) -> bool {
        self.held.is_some()
    }
}

fn extra_is_shift(held: Mods, need: Mods) -> bool {
    (held.ctrl == need.ctrl) && (held.alt == need.alt) && (held.meta == need.meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALT: Mods = Mods { ctrl: false, alt: true, shift: false, meta: false };
    const ALT_SHIFT: Mods = Mods { ctrl: false, alt: true, shift: true, meta: false };
    const NONE: Mods = Mods { ctrl: false, alt: false, shift: false, meta: false };

    fn binds() -> Bindings {
        Bindings { switcher: parse("Alt+Tab"), same_app: parse("Alt+`") }
    }

    #[test]
    fn parses_accels() {
        assert_eq!(parse("Alt+Tab"), Some(Accel { mods: ALT, key: Key::Tab }));
        assert_eq!(parse("Ctrl+Alt+`").unwrap().key, Key::Backquote);
        assert_eq!(parse("Super+KeyK").unwrap(), Accel { mods: Mods { meta: true, ..NONE }, key: Key::Char('k') });
        assert_eq!(parse("Alt+F4").unwrap().key, Key::F(4));
        assert_eq!(parse(""), None);
        assert_eq!(parse("Alt+Nonsense"), None);
    }

    #[test]
    fn hold_tab_release() {
        let b = binds();
        let mut h = Hook::default();
        assert_eq!(h.key(&b, false, Key::Tab, true, ALT), (true, Some(Command::Open { reverse: false, same_app: false })));
        assert_eq!(h.key(&b, true, Key::Tab, false, ALT), (true, None));
        assert_eq!(h.key(&b, true, Key::Tab, true, ALT), (true, Some(Command::Step(1))));
        assert_eq!(h.key(&b, true, Key::Tab, true, ALT_SHIFT), (true, Some(Command::Step(-1))));
        assert_eq!(h.modifiers(true, ALT), None);
        assert_eq!(h.modifiers(true, ALT_SHIFT), None);
        assert_eq!(h.modifiers(true, NONE), Some(Command::Commit));
        assert!(!h.holding());
    }

    #[test]
    fn passes_other_keys_when_closed() {
        let b = binds();
        let mut h = Hook::default();
        assert_eq!(h.key(&b, false, Key::Tab, true, NONE), (false, None));
        assert_eq!(h.key(&b, false, Key::Char('w'), true, ALT), (false, None));
        let ctrl_alt = Mods { ctrl: true, ..ALT };
        assert_eq!(h.key(&b, false, Key::Tab, true, ctrl_alt), (false, None));
        assert_eq!(h.key(&b, false, Key::Tab, false, ALT), (false, None));
    }

    #[test]
    fn keys_while_open() {
        let b = binds();
        let mut h = Hook::default();
        h.key(&b, false, Key::Tab, true, ALT);
        assert_eq!(h.key(&b, true, Key::Char('w'), true, ALT).1, Some(Command::CloseWindow));
        assert_eq!(h.key(&b, true, Key::Char('q'), true, ALT).1, Some(Command::QuitApp));
        assert_eq!(h.key(&b, true, Key::Escape, true, ALT).1, Some(Command::Cancel));
        assert_eq!(h.key(&b, true, Key::Backquote, true, ALT).1, Some(Command::StepWindow(1)));
        assert_eq!(h.key(&b, true, Key::F(5), true, ALT), (true, None));
        h.reset();
        assert_eq!(h.key(&b, true, Key::Char('w'), true, NONE).1, Some(Command::Type('w')));
        assert_eq!(h.key(&b, true, Key::Down, true, NONE).1, Some(Command::Arrow(0, 1)));
        assert_eq!(h.modifiers(true, NONE), None);
    }

    #[test]
    fn same_app_trigger() {
        let b = binds();
        let mut h = Hook::default();
        assert_eq!(h.key(&b, false, Key::Backquote, true, ALT).1, Some(Command::Open { reverse: false, same_app: true }));
        assert_eq!(h.modifiers(true, NONE), Some(Command::Commit));
    }
}
