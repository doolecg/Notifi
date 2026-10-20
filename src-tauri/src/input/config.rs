//! The input options, read from the settings store with the catalog's defaults for anything missing.

use std::collections::HashMap;

use serde_json::Value;

use super::logic::{self, normalise_app, Action, Btn, Dir};
use crate::settings::Settings;

#[derive(Clone, Debug, PartialEq)]
pub struct ClickCfg {
    pub threshold_ms: u32,
    /// Left, right, middle, back, forward.
    pub buttons: [bool; 5],
}

#[derive(Clone, Debug, PartialEq)]
pub struct SmoothCfg {
    pub speed: f64,
    pub tau_ms: f64,
    pub excluded: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ButtonsCfg {
    pub actions: HashMap<usize, Action>,
    pub overrides: Vec<(String, Btn, Action)>,
    pub gesture_button: Btn,
    pub gestures: HashMap<Dir, Action>,
    pub threshold_px: i32,
    pub excluded: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SuperCfg {
    pub source_vk: u32,
    pub held: Vec<u16>,
    pub tap: Option<Action>,
    pub tap_ms: u32,
    pub excluded: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub click: Option<ClickCfg>,
    pub debounce_ms: Option<u32>,
    pub invert_vertical: bool,
    pub invert_horizontal: bool,
    pub scroll_excluded: Vec<String>,
    pub sideways_vk: Option<[u16; 2]>,
    pub smooth: Option<SmoothCfg>,
    pub linear: Option<(f64, Vec<String>)>,
    pub buttons: Option<ButtonsCfg>,
    pub super_key: Option<SuperCfg>,
    pub flat_pointer: bool,
    pub touchpad_page: bool,
    /// The shared "Apps to leave alone" list, applied to every feature.
    pub global_excluded: Vec<String>,
}

fn flag(s: &Settings, ns: &str, key: &str, default: bool) -> bool {
    s.option(ns, key).and_then(Value::as_bool).unwrap_or(default)
}

fn num(s: &Settings, ns: &str, key: &str, default: f64) -> f64 {
    s.option(ns, key).and_then(Value::as_f64).unwrap_or(default)
}

fn text<'a>(s: &'a Settings, ns: &str, key: &str, default: &'a str) -> &'a str {
    s.option(ns, key).and_then(Value::as_str).unwrap_or(default)
}

fn list(s: &Settings, ns: &str, key: &str) -> Vec<String> {
    s.option(ns, key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
        .unwrap_or_default()
}

/// The page's own exclusions plus the shared list, normalised for matching.
fn excluded(s: &Settings, ns: &str) -> Vec<String> {
    list(s, ns, "excludedApps")
        .iter()
        .chain(list(s, "appExclusions", "apps").iter())
        .map(|a| normalise_app(a))
        .filter(|a| !a.is_empty())
        .collect()
}

impl Config {
    pub fn from_settings(s: &Settings) -> Self {
        let on = |ns: &str| s.enabled(ns);
        let global_excluded = excluded(s, "appExclusions");

        let click = on("clickFilter").then(|| ClickCfg {
            threshold_ms: num(s, "clickFilter", "threshold", 40.0).clamp(1.0, 200.0) as u32,
            buttons: ["left", "right", "middle", "back", "forward"].map(|k| flag(s, "clickFilter", k, true)),
        });

        let debounce_ms = on("keyDebounce").then(|| num(s, "keyDebounce", "interval", 30.0).clamp(5.0, 200.0) as u32);

        let sideways_vk = on("sidewaysScroll").then(|| match text(s, "sidewaysScroll", "key", "Shift") {
            "Ctrl" => [0x11, 0x11],
            "Alt" => [0x12, 0x12],
            "Super" => [0x5B, 0x5C],
            _ => [0x10, 0x10],
        });

        let smooth = on("smoothScrolling").then(|| SmoothCfg {
            speed: logic::speed_factor(text(s, "smoothScrolling", "speed", "medium")),
            tau_ms: logic::glide_tau(num(s, "smoothScrolling", "response", 50.0), text(s, "smoothScrolling", "coast", "low")),
            excluded: excluded(s, "smoothScrolling"),
        });

        let linear = on("linearScrolling")
            .then(|| (num(s, "linearScrolling", "linesPerStep", 3.0).clamp(1.0, 20.0), excluded(s, "linearScrolling")));

        let buttons = on("mouseButtons").then(|| {
            let mut actions = HashMap::new();
            for (btn, action) in list(s, "mouseButtons", "buttonActions").iter().filter_map(|l| logic::parse_button_line(l)) {
                actions.insert(btn.index(), action);
            }
            let gestures = logic::gesture_map(&list(s, "mouseButtons", "gestures"));
            ButtonsCfg {
                actions,
                overrides: list(s, "mouseButtons", "appOverrides").iter().filter_map(|l| logic::parse_override_line(l)).collect(),
                gesture_button: Btn::from_gesture_name(text(s, "mouseButtons", "gestureButton", "right")).unwrap_or(Btn::Right),
                gestures,
                threshold_px: num(s, "mouseButtons", "gestureThreshold", 60.0).clamp(10.0, 300.0) as i32,
                excluded: excluded(s, "mouseButtons"),
            }
        });

        let super_key = on("superKey").then(|| {
            let tap = match text(s, "superKey", "tapAction", "key") {
                "language" => logic::parse_action("switch language"),
                "nothing" => None,
                _ => logic::parse_combo(text(s, "superKey", "tapKey", "Escape")).map(Action::Keys),
            };
            SuperCfg {
                source_vk: match text(s, "superKey", "sourceKey", "capsLock") {
                    "rightAlt" => 0xA5,
                    "rightCtrl" => 0xA3,
                    "rightShift" => 0xA1,
                    _ => 0x14,
                },
                held: logic::parse_combo(text(s, "superKey", "heldCombo", "Ctrl+Alt+Shift+Super")).unwrap_or_default(),
                tap,
                tap_ms: num(s, "superKey", "tapTimeout", 200.0).clamp(50.0, 1000.0) as u32,
                excluded: excluded(s, "superKey"),
            }
        });

        Self {
            click,
            debounce_ms,
            invert_vertical: on("scrollDirection") && flag(s, "scrollDirection", "invertVertical", false),
            invert_horizontal: on("scrollDirection") && flag(s, "scrollDirection", "invertHorizontal", false),
            scroll_excluded: excluded(s, "scrollDirection"),
            sideways_vk,
            smooth,
            linear,
            buttons,
            super_key,
            flat_pointer: on("pointerAcceleration"),
            touchpad_page: on("touchpadMiddleClick"),
            global_excluded,
        }
    }

    pub fn scroll_direction_on(&self) -> bool {
        self.invert_vertical || self.invert_horizontal
    }

    pub fn buttons_active(&self) -> bool {
        self.buttons.as_ref().is_some_and(|b| !b.actions.is_empty() || !b.overrides.is_empty() || !b.gestures.is_empty())
    }

    pub fn wants_wheel(&self) -> bool {
        self.scroll_direction_on() || self.sideways_vk.is_some() || self.smooth.is_some() || self.linear.is_some()
    }

    pub fn wants_mouse_hook(&self) -> bool {
        self.click.is_some() || self.wants_wheel() || self.buttons_active() || self.super_key.is_some()
    }

    pub fn wants_key_hook(&self) -> bool {
        self.debounce_ms.is_some() || self.super_key.as_ref().is_some_and(|s| s.tap.is_some() || !s.held.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn with(settings: &mut Settings, ns: &str, key: &str, v: Value) {
        settings.features.entry(ns.to_string()).or_default().insert(key.to_string(), v);
    }

    #[test]
    fn defaults_are_all_off() {
        let c = Config::from_settings(&Settings::default());
        assert!(!c.wants_mouse_hook() && !c.wants_key_hook() && !c.flat_pointer && !c.touchpad_page);
    }

    #[test]
    fn reads_each_page() {
        let mut s = Settings::default();
        with(&mut s, "clickFilter", "enabled", json!(true));
        with(&mut s, "clickFilter", "threshold", json!(55));
        with(&mut s, "clickFilter", "middle", json!(false));
        with(&mut s, "keyDebounce", "enabled", json!(true));
        with(&mut s, "scrollDirection", "enabled", json!(true));
        with(&mut s, "scrollDirection", "invertVertical", json!(true));
        with(&mut s, "scrollDirection", "excludedApps", json!(["Game.exe"]));
        with(&mut s, "appExclusions", "apps", json!(["Blender"]));
        with(&mut s, "mouseButtons", "enabled", json!(true));
        with(&mut s, "mouseButtons", "buttonActions", json!(["Button 4 = Alt+Left"]));
        with(&mut s, "superKey", "enabled", json!(true));
        with(&mut s, "superKey", "sourceKey", json!("rightAlt"));
        with(&mut s, "pointerAcceleration", "enabled", json!(true));
        let c = Config::from_settings(&s);
        let click = c.click.clone().unwrap();
        assert_eq!(click.threshold_ms, 55);
        assert_eq!(click.buttons, [true, true, false, true, true]);
        assert_eq!(c.debounce_ms, Some(30));
        assert!(c.invert_vertical && !c.invert_horizontal && c.wants_wheel());
        assert_eq!(c.scroll_excluded, vec!["game".to_string(), "blender".to_string()]);
        assert_eq!(c.buttons.as_ref().unwrap().actions.len(), 1);
        let sk = c.super_key.unwrap();
        assert_eq!(sk.source_vk, 0xA5);
        assert_eq!(sk.tap, Some(Action::Keys(vec![0x1B])));
        assert_eq!(sk.held.len(), 4);
        assert!(c.flat_pointer);
    }

    #[test]
    fn empty_button_lists_need_no_hook() {
        let mut s = Settings::default();
        with(&mut s, "mouseButtons", "enabled", json!(true));
        assert!(!Config::from_settings(&s).wants_mouse_hook());
    }
}
