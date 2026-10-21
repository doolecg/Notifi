//! Lock screen options (spec/everyday/lock-screen.md): the OS draws the lock screen, so Notifi only
//! pauses its Island while locked, plays optional lock and unlock sounds and can show a short notice
//! on the pill before it locks the screen itself.

use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::settings::{Settings, Store};

const NS: &str = "lockScreen";
/// How long the notice is on screen before the lock runs.
const NOTICE_LEAD: Duration = Duration::from_millis(900);

fn flag(s: &Settings, key: &str, default: bool) -> bool {
    s.option(NS, key).and_then(Value::as_bool).unwrap_or(default)
}

/// Whether the Island stays hidden: the session is locked and "Pause overlays while locked" is on.
pub fn overlays_paused(s: &Settings, locked: bool) -> bool {
    locked && s.enabled(NS) && flag(s, "pauseOverlays", true)
}

/// The session was locked or unlocked (called from the system event loop in `energy`).
pub fn session_changed(app: &AppHandle, locked: bool) {
    let s = app.state::<Store>().get();
    super::set_locked(app, locked);
    if s.enabled(NS) && flag(&s, "sounds", false) {
        super::click::play();
    }
}

/// For the lock action: with "Show a short notice before locking" on, shows the notice and runs
/// `lock` after a moment. Returns `false` when the option is off and the caller should lock at once.
pub fn notice_then(app: &AppHandle, lock: impl FnOnce() + Send + 'static) -> bool {
    let s = app.state::<Store>().get();
    if !(s.enabled(NS) && flag(&s, "lockedNotice", false)) {
        return false;
    }
    super::notice(app, "Locking".into());
    std::thread::spawn(move || {
        std::thread::sleep(NOTICE_LEAD);
        lock();
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(enabled: bool, pause: Option<bool>) -> Settings {
        let mut s = Settings::default();
        let ns = s.features.entry(NS.into()).or_default();
        ns.insert("enabled".into(), enabled.into());
        if let Some(p) = pause {
            ns.insert("pauseOverlays".into(), p.into());
        }
        s
    }

    #[test]
    fn overlays_pause_only_while_locked_with_the_feature_and_option_on() {
        assert!(overlays_paused(&with(true, None), true), "pausing is on by default");
        assert!(!overlays_paused(&with(true, None), false));
        assert!(!overlays_paused(&with(true, Some(false)), true));
        assert!(!overlays_paused(&with(false, Some(true)), true));
    }
}
