//! The window-management options, read from the settings store with the catalog's defaults for anything missing.

use serde_json::Value;

use super::guard::{self, ExtraModifier};
use super::snap;
use crate::settings::Settings;

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub layout: bool,
    pub repeat_to_move: bool,
    pub cycle_widths: bool,
    pub margin: u32,
    pub gap: u32,
    pub native_fullscreen_apps: Vec<String>,
    pub snap_edges: bool,
    pub snap_preview: bool,
    pub edge_rules: Vec<snap::Rule>,
    pub modifier_drag: bool,
    pub drag_modifiers: Modifiers,
    pub right_button_resize: bool,
    pub ignore_apps: Vec<String>,

    pub fill: bool,
    pub fill_whole_display: bool,
    pub fill_gap: u32,
    pub take_over_maximize: bool,
    pub native_maximize_apps: Vec<String>,

    pub desktops: bool,
    pub follow_window: bool,
    pub create_on_demand: bool,
    pub show_indicator: bool,
    pub desktop_names: Vec<String>,

    pub quit_on_close: bool,
    pub quit_apps: Vec<String>,
    pub quit_exceptions: Vec<String>,
    pub grace_ms: u64,

    pub protection: bool,
    pub guard_quit: bool,
    pub guard_close: bool,
    pub guard_mode: guard::Mode,
    pub hold_ms: u64,
    pub extra_modifier: ExtraModifier,
    pub app_rules: Vec<(String, guard::Mode)>,

    pub focus_follows: bool,
    pub focus_delay_ms: u32,
    pub focus_raise: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub meta: bool,
}

impl Modifiers {
    pub fn parse(s: &str) -> Self {
        let mut m = Self::default();
        for part in s.split('+') {
            match part.trim().to_lowercase().as_str() {
                "alt" => m.alt = true,
                "ctrl" | "control" => m.ctrl = true,
                "shift" => m.shift = true,
                "super" | "win" | "meta" => m.meta = true,
                _ => {}
            }
        }
        if m == Self::default() {
            m.alt = true;
        }
        m
    }
}

struct Reader<'a>(&'a Settings);

impl Reader<'_> {
    fn flag(&self, ns: &str, key: &str, default: bool) -> bool {
        self.0.option(ns, key).and_then(Value::as_bool).unwrap_or(default)
    }
    fn num(&self, ns: &str, key: &str, default: f64) -> f64 {
        self.0.option(ns, key).and_then(Value::as_f64).unwrap_or(default)
    }
    fn text(&self, ns: &str, key: &str, default: &str) -> String {
        self.0.option(ns, key).and_then(Value::as_str).unwrap_or(default).to_string()
    }
    fn list(&self, ns: &str, key: &str) -> Vec<String> {
        self.0
            .option(ns, key)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect())
            .unwrap_or_default()
    }
}

impl Config {
    pub fn from_settings(s: &Settings) -> Self {
        let r = Reader(s);
        let apps = |ns, key| r.list(ns, key).iter().map(|a| guard::normalise_app(a)).collect::<Vec<_>>();
        Self {
            layout: r.flag("windowLayout", "enabled", false),
            repeat_to_move: r.flag("windowLayout", "repeatToMove", false),
            cycle_widths: r.flag("windowLayout", "cycleWidths", false),
            margin: r.num("windowLayout", "marginSize", 0.0) as u32,
            gap: r.num("windowLayout", "gap", 0.0) as u32,
            native_fullscreen_apps: apps("windowLayout", "nativeFullscreenApps"),
            snap_edges: r.flag("windowLayout", "snapEdges", false),
            snap_preview: r.flag("windowLayout", "snapPreview", true),
            edge_rules: snap::rules_from(&r.list("windowLayout", "edgeRules"), r.flag("windowLayout", "topEdgeMaximize", true)),
            modifier_drag: r.flag("windowLayout", "modifierDrag", false),
            drag_modifiers: Modifiers::parse(&r.text("windowLayout", "dragModifier", "Alt")),
            right_button_resize: r.flag("windowLayout", "rightButtonResize", false),
            ignore_apps: apps("windowLayout", "ignoreApps"),

            fill: r.flag("fillScreen", "enabled", false),
            fill_whole_display: r.text("fillScreen", "cover", "workArea") == "display",
            fill_gap: r.num("fillScreen", "gap", 0.0) as u32,
            take_over_maximize: r.flag("fillScreen", "takeOverMaximize", false),
            native_maximize_apps: apps("fillScreen", "nativeMaximizeApps"),

            desktops: r.flag("virtualDesktops", "enabled", false),
            follow_window: r.flag("virtualDesktops", "followWindow", true),
            create_on_demand: r.flag("virtualDesktops", "createOnDemand", false),
            show_indicator: r.flag("virtualDesktops", "showIndicator", false),
            desktop_names: r.list("virtualDesktops", "names"),

            quit_on_close: r.flag("quitOnClose", "enabled", false),
            quit_apps: apps("quitOnClose", "apps"),
            quit_exceptions: apps("quitOnClose", "exceptions"),
            grace_ms: r.num("quitOnClose", "graceDelay", 800.0) as u64,

            protection: r.flag("closeProtection", "enabled", false),
            guard_quit: r.flag("closeProtection", "guardQuit", true),
            guard_close: r.flag("closeProtection", "guardClose", true),
            guard_mode: guard::Mode::parse(&r.text("closeProtection", "mode", "hold")).unwrap_or(guard::Mode::Hold),
            hold_ms: r.num("closeProtection", "holdTime", 700.0) as u64,
            extra_modifier: ExtraModifier::parse(&r.text("closeProtection", "extraModifier", "Shift")),
            app_rules: r.list("closeProtection", "appRules").iter().filter_map(|l| guard::parse_app_rule(l)).collect(),

            focus_follows: r.flag("focusFollowsMouse", "enabled", false),
            focus_delay_ms: r.num("focusFollowsMouse", "delay", 300.0) as u32,
            focus_raise: r.flag("focusFollowsMouse", "raise", false),
        }
    }

    pub fn ignores(&self, app: Option<&str>) -> bool {
        listed(&self.ignore_apps, app)
    }

    pub fn mode_for(&self, app: Option<&str>) -> guard::Mode {
        let app = app.map(guard::normalise_app).unwrap_or_default();
        self.app_rules.iter().find(|(a, _)| *a == app).map_or(self.guard_mode, |(_, m)| *m)
    }

    pub fn wants_events(&self) -> bool {
        self.snap_edges || self.quit_on_close || (self.fill && self.take_over_maximize)
    }

    pub fn wants_mouse_hook(&self) -> bool {
        self.modifier_drag
    }

    pub fn wants_key_hook(&self) -> bool {
        self.protection
    }
}

/// `app` is on `list`; both are lower case without `.exe`.
pub fn listed(list: &[String], app: Option<&str>) -> bool {
    let Some(app) = app.map(guard::normalise_app) else { return false };
    list.iter().any(|a| *a == app)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(settings: &mut Settings, ns: &str, key: &str, v: Value) {
        settings.features.entry(ns.to_string()).or_default().insert(key.to_string(), v);
    }

    #[test]
    fn catalog_defaults_load() {
        let c = Config::from_settings(&Settings::default());
        assert!(!c.layout && !c.snap_edges && !c.modifier_drag && !c.protection && !c.quit_on_close && !c.focus_follows);
        assert!(c.snap_preview);
        assert_eq!(c.grace_ms, 800);
        assert_eq!(c.hold_ms, 700);
        assert_eq!(c.focus_delay_ms, 300);
        assert_eq!(c.guard_mode, guard::Mode::Hold);
        assert_eq!(c.drag_modifiers, Modifiers { alt: true, ..Modifiers::default() });
        assert!(!c.edge_rules.is_empty(), "edge defaults apply when the list is empty");
        assert!(!c.wants_events());
    }

    #[test]
    fn options_are_read_from_their_namespaces() {
        let mut s = Settings::default();
        with(&mut s, "windowLayout", "gap", Value::from(12));
        with(&mut s, "windowLayout", "snapEdges", Value::from(true));
        with(&mut s, "windowLayout", "ignoreApps", serde_json::json!(["Notepad.exe", " Calc "]));
        with(&mut s, "windowLayout", "dragModifier", Value::from("Ctrl+Alt"));
        with(&mut s, "closeProtection", "appRules", serde_json::json!(["Chrome.exe = twice", "garbage"]));
        with(&mut s, "closeProtection", "mode", Value::from("modifier"));
        with(&mut s, "fillScreen", "cover", Value::from("display"));
        let c = Config::from_settings(&s);
        assert_eq!(c.gap, 12);
        assert!(c.snap_edges && c.wants_events());
        assert!(c.ignores(Some("NOTEPAD.EXE")) && c.ignores(Some("calc")) && !c.ignores(Some("code")) && !c.ignores(None));
        assert_eq!(c.drag_modifiers, Modifiers { alt: true, ctrl: true, ..Modifiers::default() });
        assert_eq!(c.mode_for(Some("chrome.exe")), guard::Mode::Twice);
        assert_eq!(c.mode_for(Some("code.exe")), guard::Mode::Modifier);
        assert!(c.fill_whole_display);
    }

    #[test]
    fn modifier_names() {
        assert_eq!(Modifiers::parse("Super"), Modifiers { meta: true, ..Modifiers::default() });
        assert_eq!(Modifiers::parse("nonsense"), Modifiers { alt: true, ..Modifiers::default() });
    }
}
