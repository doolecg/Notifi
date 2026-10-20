//! Window management: layouts and snapping, fill screen, virtual desktops, quit on close, quit and
//! close protection, and focus follows mouse. The geometry, snap-area, drag and guard modules are
//! pure and tested; `events`, `win_hooks` and `ffm` connect them to the OS through the platform crate.

pub mod actions;
pub mod config;
pub mod drag;
pub mod events;
pub mod ffm;
pub mod geometry;
pub mod guard;
pub mod overlay;
pub mod session;
pub mod snap;
#[cfg(windows)]
pub mod win_hooks;
#[cfg(windows)]
pub mod win_sys;

use std::sync::{Arc, OnceLock, RwLock};

use notifi_platform::Platform;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::keybinds::FeatureShortcut;
use crate::settings::Store;
use actions::Outcome;
pub use config::Config;

static APP: OnceLock<AppHandle> = OnceLock::new();
static PLATFORM: OnceLock<Platform> = OnceLock::new();
static CONFIG: RwLock<Option<Arc<Config>>> = RwLock::new(None);

/// Every layout, fill-screen and desktop shortcut on the Window layout page, as rebindable actions.
pub const SHORTCUTS: &[FeatureShortcut] = &[
    FeatureShortcut { namespace: "windowLayout", key: "leftHalf", action: "wm.leftHalf" },
    FeatureShortcut { namespace: "windowLayout", key: "rightHalf", action: "wm.rightHalf" },
    FeatureShortcut { namespace: "windowLayout", key: "topHalf", action: "wm.topHalf" },
    FeatureShortcut { namespace: "windowLayout", key: "bottomHalf", action: "wm.bottomHalf" },
    FeatureShortcut { namespace: "windowLayout", key: "topLeft", action: "wm.topLeft" },
    FeatureShortcut { namespace: "windowLayout", key: "topRight", action: "wm.topRight" },
    FeatureShortcut { namespace: "windowLayout", key: "bottomLeft", action: "wm.bottomLeft" },
    FeatureShortcut { namespace: "windowLayout", key: "bottomRight", action: "wm.bottomRight" },
    FeatureShortcut { namespace: "windowLayout", key: "maximize", action: "wm.maximize" },
    FeatureShortcut { namespace: "windowLayout", key: "center", action: "wm.center" },
    FeatureShortcut { namespace: "windowLayout", key: "maximizeMargin", action: "wm.maximizeMargin" },
    FeatureShortcut { namespace: "windowLayout", key: "nextDisplay", action: "wm.nextDisplay" },
    FeatureShortcut { namespace: "windowLayout", key: "restore", action: "wm.restore" },
    FeatureShortcut { namespace: "fillScreen", key: "shortcut", action: "wm.fill" },
    FeatureShortcut { namespace: "virtualDesktops", key: "previous", action: "wm.prevDesktop" },
    FeatureShortcut { namespace: "virtualDesktops", key: "next", action: "wm.nextDesktop" },
    FeatureShortcut { namespace: "virtualDesktops", key: "switch1", action: "wm.switch1" },
    FeatureShortcut { namespace: "virtualDesktops", key: "switch2", action: "wm.switch2" },
    FeatureShortcut { namespace: "virtualDesktops", key: "switch3", action: "wm.switch3" },
    FeatureShortcut { namespace: "virtualDesktops", key: "switch4", action: "wm.switch4" },
    FeatureShortcut { namespace: "virtualDesktops", key: "switch5", action: "wm.switch5" },
    FeatureShortcut { namespace: "virtualDesktops", key: "switch6", action: "wm.switch6" },
    FeatureShortcut { namespace: "virtualDesktops", key: "switch7", action: "wm.switch7" },
    FeatureShortcut { namespace: "virtualDesktops", key: "switch8", action: "wm.switch8" },
    FeatureShortcut { namespace: "virtualDesktops", key: "switch9", action: "wm.switch9" },
    FeatureShortcut { namespace: "virtualDesktops", key: "move1", action: "wm.move1" },
    FeatureShortcut { namespace: "virtualDesktops", key: "move2", action: "wm.move2" },
    FeatureShortcut { namespace: "virtualDesktops", key: "move3", action: "wm.move3" },
    FeatureShortcut { namespace: "virtualDesktops", key: "move4", action: "wm.move4" },
    FeatureShortcut { namespace: "virtualDesktops", key: "move5", action: "wm.move5" },
    FeatureShortcut { namespace: "virtualDesktops", key: "move6", action: "wm.move6" },
    FeatureShortcut { namespace: "virtualDesktops", key: "move7", action: "wm.move7" },
    FeatureShortcut { namespace: "virtualDesktops", key: "move8", action: "wm.move8" },
    FeatureShortcut { namespace: "virtualDesktops", key: "move9", action: "wm.move9" },
];

pub fn config() -> Arc<Config> {
    CONFIG.read().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_else(|| Arc::new(Config::from_settings(&crate::settings::Settings::default())))
}

pub fn platform() -> Platform {
    PLATFORM.get().expect("windows_mgmt::start runs first").clone()
}

pub fn app() -> Option<AppHandle> {
    APP.get().cloned()
}

pub fn start(app: &AppHandle) -> tauri::Result<()> {
    let _ = APP.set(app.clone());
    let _ = PLATFORM.set(app.state::<Platform>().inner().clone());
    overlay::create(app)?;
    reload(app);
    Ok(())
}

pub fn on_settings_changed(app: &AppHandle) {
    reload(app);
}

pub fn on_exit() {
    #[cfg(windows)]
    win_sys::restore_all();
}

fn reload(app: &AppHandle) {
    let Some(store) = app.try_state::<Store>() else { return };
    let new = Arc::new(Config::from_settings(&store.get()));
    let old = CONFIG.write().unwrap_or_else(|e| e.into_inner()).replace(new.clone());
    if old.as_deref() == Some(&*new) {
        return;
    }
    ffm::apply(app, &new);
    events::ensure(app, &new);
    #[cfg(windows)]
    {
        win_hooks::ensure(&new);
        win_sys::set_native_snap_suspended(new.snap_edges);
    }
}

fn notify(app: &AppHandle, text: &str) {
    let platform = platform();
    let wm = platform.windows();
    let display = wm
        .pointer()
        .ok()
        .zip(wm.work_areas().ok())
        .and_then(|(p, areas)| geometry::area_at(&areas, p.x, p.y).map(|a| a.bounds));
    if let Some(display) = display {
        overlay::hint(app, display, text);
    }
}

fn report(app: &AppHandle, outcome: Outcome, announce_skips: bool) {
    match outcome {
        Outcome::Done => {}
        Outcome::Skipped(why) => {
            if announce_skips {
                notify(app, &capitalise(why));
            }
        }
        Outcome::Failed(e) => {
            eprintln!("windows_mgmt: {e}");
            notify(app, "Could not do that");
        }
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Runs a `wm.<name>` shortcut action off the shortcut thread.
pub fn run_action(app: &AppHandle, action: &str) {
    let Some(name) = action.strip_prefix("wm.") else { return };
    let (app, name) = (app.clone(), name.to_string());
    std::thread::spawn(move || dispatch(&app, &name));
}

fn desktop_number(name: &str, prefix: &str) -> Option<u32> {
    name.strip_prefix(prefix)?.parse().ok().filter(|n| (1..=9).contains(n))
}

fn dispatch(app: &AppHandle, name: &str) {
    let cfg = config();
    let platform = platform();
    let wm = platform.windows();
    if matches!(name, "fill") {
        if !cfg.fill {
            return;
        }
        let outcome = match actions::target(wm, &cfg) {
            Ok(win) => actions::toggle_fill(wm, &cfg, &win),
            Err(o) => o,
        };
        return report(app, outcome, false);
    }
    if name == "prevDesktop" || name == "nextDesktop" {
        if cfg.desktops {
            let outcome = actions::step_desktop(wm, name == "nextDesktop");
            show_indicator(app, &cfg, &outcome);
            report(app, outcome, true);
        }
        return;
    }
    if let Some(n) = desktop_number(name, "switch") {
        if cfg.desktops {
            let outcome = actions::switch_desktop(wm, &cfg, n);
            show_indicator(app, &cfg, &outcome);
            report(app, outcome, true);
        }
        return;
    }
    if let Some(n) = desktop_number(name, "move") {
        if cfg.desktops {
            let outcome = match actions::target(wm, &cfg) {
                Ok(win) => actions::move_to_desktop(wm, &cfg, &win, n),
                Err(o) => o,
            };
            show_indicator(app, &cfg, &outcome);
            report(app, outcome, true);
        }
        return;
    }
    if !cfg.layout {
        return;
    }
    let win = match actions::target(wm, &cfg) {
        Ok(w) => w,
        Err(o) => return report(app, o, false),
    };
    let outcome = match name {
        "nextDisplay" => actions::next_display(wm, &win),
        "restore" => actions::restore(wm, &win),
        key => match actions::placement_for(key) {
            Some(p) => actions::apply_placement(wm, &cfg, &win, p),
            None => return,
        },
    };
    report(app, outcome, false);
}

/// Name for desktop `n`: the one the user gave it, else "Desktop n".
pub fn desktop_label(cfg: &Config, n: u32) -> String {
    cfg.desktop_names.get(n as usize - 1).filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| format!("Desktop {n}"))
}

fn show_indicator(app: &AppHandle, cfg: &Config, outcome: &Outcome) {
    if !cfg.show_indicator || *outcome != Outcome::Done {
        return;
    }
    if let Ok(state) = platform().windows().desktop_state() {
        notify(app, &format!("{} ({}/{})", desktop_label(cfg, state.current), state.current, state.count));
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopInfo {
    count: u32,
    current: u32,
    names: Vec<String>,
    label: String,
}

/// The desktop count, the current one and its name, for indicators.
#[tauri::command]
pub fn desktop_info(platform: State<Platform>) -> Result<DesktopInfo, String> {
    let cfg = config();
    let state = platform.windows().desktop_state().map_err(|e| e.to_string())?;
    Ok(DesktopInfo {
        count: state.count,
        current: state.current,
        names: (1..=state.count).map(|n| desktop_label(&cfg, n)).collect(),
        label: desktop_label(&cfg, state.current),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shortcut_option_exists_in_the_catalog() {
        let fields: Vec<_> = crate::catalog::fields().collect();
        for s in SHORTCUTS {
            assert!(fields.iter().any(|f| f.namespace == s.namespace && f.key == s.key), "{}.{} is not in the catalog", s.namespace, s.key);
        }
    }

    #[test]
    fn every_shortcut_action_is_handled() {
        for s in SHORTCUTS {
            let name = s.action.strip_prefix("wm.").unwrap();
            let known = matches!(name, "fill" | "prevDesktop" | "nextDesktop" | "nextDisplay" | "restore")
                || desktop_number(name, "switch").is_some()
                || desktop_number(name, "move").is_some()
                || actions::placement_for(name).is_some();
            assert!(known, "{name} has no handler");
        }
    }

    #[test]
    fn labels_use_names_then_numbers() {
        let mut cfg = Config::from_settings(&crate::settings::Settings::default());
        cfg.desktop_names = vec!["Work".into(), String::new()];
        assert_eq!(desktop_label(&cfg, 1), "Work");
        assert_eq!(desktop_label(&cfg, 2), "Desktop 2");
        assert_eq!(desktop_label(&cfg, 5), "Desktop 5");
    }

    #[test]
    fn desktop_numbers_are_one_to_nine() {
        assert_eq!(desktop_number("switch3", "switch"), Some(3));
        assert_eq!(desktop_number("switch0", "switch"), None);
        assert_eq!(desktop_number("move10", "move"), None);
        assert_eq!(desktop_number("moveX", "move"), None);
    }
}
