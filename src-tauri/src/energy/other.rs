//! Stand-in for `win.rs` where the window, hook and input pieces are not built: every call does
//! nothing and `SUPPORTED` is false, so callers report the feature as unavailable.
#![allow(dead_code)]

use notifi_platform::Rect;

pub const SUPPORTED: bool = false;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sys {
    Locked,
    Unlocked,
    Suspending,
    Resumed,
    Lid(bool),
    CleaningUnlocked,
}

pub struct CleanSpec {
    pub rects: Vec<Rect>,
    pub blackout: bool,
    pub allow_pointer: bool,
    pub show_timer: bool,
    pub presses: u32,
    pub window_ms: u64,
}

pub fn start(_on_event: impl Fn(Sys) + Send + Sync + 'static) {}
pub fn dim(_key: &str, _rect: Rect, _alpha: u8) {}
pub fn remove_dim(_key: &str) {}
pub fn clean_start(_spec: CleanSpec) {}
pub fn clean_stop() {}
pub fn nudge() -> bool {
    false
}
