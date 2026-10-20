//! System monitor parts used by the sampler in `stats.rs`: graphs, memory pressure, GPUs,
//! per-app use, tray readouts, alerts and the speed test.

pub mod alerts;
pub mod devices;
pub mod gpu;
pub mod history;
pub mod memory;
pub mod notify;
#[cfg(windows)]
pub mod pdh;
pub mod procs;
pub mod readouts;
pub mod sensors;
pub mod speedtest;
pub mod units;

use tauri::{AppHandle, Manager};

use crate::{
    keybinds::FeatureShortcut,
    panel::{self, Anchor, PanelState},
};

const OPEN: &str = "monitor.open";

/// The "Open the monitor" shortcut on the Monitor page.
pub const SHORTCUTS: &[FeatureShortcut] = &[FeatureShortcut { namespace: "monitor", key: "openMonitor", action: OPEN }];

/// Opens the panel on its System tab by the pointer; pressed again while it shows that tab, closes it.
pub fn run_action(app: &AppHandle, action: &str) {
    if action != OPEN {
        return;
    }
    let state = app.state::<PanelState>();
    if state.visible() && state.tab() == "system" {
        panel::hide(app);
    } else {
        panel::show(app, Anchor::Pointer, Some("system".into()));
    }
}
