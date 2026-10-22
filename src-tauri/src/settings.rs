use std::{collections::BTreeMap, fs, path::PathBuf, sync::Mutex};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{catalog, island::config::IslandConfig, keybinds};

pub const CHANGED_EVENT: &str = "settings://changed";

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct PanelTab {
    pub id: &'static str,
    pub label: &'static str,
    /// Feature namespace whose `enabled` switch must be on for the tab to show; `None` means always available.
    pub feature: Option<&'static str>,
    /// Settings page the tab's customise button opens.
    pub section: Option<&'static str>,
    /// False until the tab's backend ships; the tab stays hidden.
    pub ready: bool,
}

pub const PANEL_TABS: &[PanelTab] = &[
    PanelTab { id: "keepAwake", label: "Keep awake", feature: Some("keepAwake"), section: Some("keepAwake"), ready: true },
    PanelTab { id: "displays", label: "Displays", feature: Some("displays"), section: Some("displays"), ready: false },
    PanelTab { id: "mixer", label: "Mixer", feature: Some("volumeMixer"), section: Some("volumeMixer"), ready: true },
    PanelTab { id: "system", label: "System", feature: Some("monitor"), section: Some("monitor"), ready: true },
    PanelTab { id: "network", label: "Network", feature: Some("network"), section: Some("monitor"), ready: true },
    PanelTab { id: "storage", label: "Storage", feature: None, section: Some("panel"), ready: true },
    PanelTab { id: "power", label: "Power", feature: Some("monitor"), section: Some("monitor"), ready: true },
    PanelTab { id: "utilities", label: "Utilities", feature: None, section: Some("features"), ready: true },
    PanelTab { id: "controls", label: "Controls", feature: None, section: Some("panel"), ready: true },
    PanelTab { id: "about", label: "About", feature: None, section: Some("about"), ready: true },
];

pub const KEEP_AWAKE_CHOICES: &[u32] = &[15, 60, 120, 0];

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Panel {
    pub last_tab: String,
    /// Tabs the user wants in the panel; a tab also needs its feature on.
    pub tabs: BTreeMap<String, bool>,
    /// Tab ids in the order the panel shows them.
    pub order: Vec<String>,
    /// Keep awake duration in minutes, 0 = until turned off.
    pub keep_awake_minutes: u32,
    pub tray_icon: String,
    pub hide_icon_with_readouts: bool,
    pub left_click: String,
    pub compact: bool,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            last_tab: "controls".into(),
            tabs: PANEL_TABS.iter().map(|t| (t.id.to_string(), true)).collect(),
            order: PANEL_TABS.iter().map(|t| t.id.to_string()).collect(),
            keep_awake_minutes: 60,
            tray_icon: "default".into(),
            hide_icon_with_readouts: false,
            left_click: "panel".into(),
            compact: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct General {
    pub autostart: bool,
    pub theme: Theme,
    pub translucent: bool,
    pub reduced_motion: bool,
    pub language: String,
    pub auto_update: bool,
    pub update_check_hours: u32,
    pub install_on_quit: bool,
    pub beta_channel: bool,
    pub preview_notes: bool,
    pub whats_new: bool,
    pub show_tour: bool,
    pub haptics: bool,
    /// The first-run chooser (which features to install) has been answered or skipped.
    pub setup_done: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            autostart: false,
            theme: Theme::System,
            translucent: false,
            reduced_motion: false,
            language: "system".into(),
            auto_update: true,
            update_check_hours: 3,
            install_on_quit: true,
            beta_channel: false,
            preview_notes: true,
            whats_new: true,
            show_tour: true,
            haptics: false,
            setup_done: false,
        }
    }
}

/// Option values of every catalog namespace that is not a typed section, keyed by namespace.
pub type Features = BTreeMap<String, Map<String, Value>>;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Settings {
    pub general: General,
    pub shortcuts: BTreeMap<String, String>,
    pub features: Features,
    pub panel: Panel,
    pub island: IslandConfig,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            general: General::default(),
            shortcuts: keybinds::defaults(),
            features: catalog::feature_defaults().into_iter().collect(),
            panel: Panel::default(),
            island: IslandConfig::default(),
        }
    }
}

impl Settings {
    /// Whether the feature (or sub-feature) namespace is switched on.
    #[allow(dead_code)]
    pub fn enabled(&self, namespace: &str) -> bool {
        self.option(namespace, "enabled").and_then(Value::as_bool).unwrap_or(false)
    }

    /// The stored value of `namespace.key` in the features map.
    pub fn option(&self, namespace: &str, key: &str) -> Option<&Value> {
        self.features.get(namespace)?.get(key)
    }

    /// Whether anything is installed (a feature that is on, or the Island).
    pub fn any_installed(&self) -> bool {
        self.island.enabled || self.features.values().any(|m| m.get("enabled") == Some(&Value::Bool(true)))
    }

    /// Someone who already has features on from before the install chooser existed is set up.
    fn mark_existing_setup(&mut self) {
        if !self.general.setup_done && self.any_installed() {
            self.general.setup_done = true;
        }
    }

    /// Installs these features (only real, installable ones) and marks the first-run chooser as answered.
    fn finish_setup(&mut self, install: &[String]) -> Result<(), String> {
        for id in install {
            let page = catalog::page(id).filter(|p| p.feature).ok_or_else(|| format!("`{id}` is not a feature"))?;
            self.set_section(page.id, serde_json::json!({ "enabled": true }))?;
        }
        self.general.setup_done = true;
        Ok(())
    }

    /// Fills in missing keys from the defaults, drops unknown ones and fixes values that don't fit.
    fn normalized(self) -> Self {
        let mut out = Self::default();
        out.general = fit_typed("general", &self.general);
        let mut panel: Panel = fit_typed("panel", &self.panel);
        if !PANEL_TABS.iter().any(|t| t.id == panel.last_tab) {
            panel.last_tab = out.panel.last_tab.clone();
        }
        for (id, on) in out.panel.tabs.iter_mut() {
            if let Some(saved) = panel.tabs.get(id) {
                *on = *saved;
            }
        }
        panel.tabs = std::mem::take(&mut out.panel.tabs);
        let mut order: Vec<String> = Vec::new();
        for id in panel.order.iter().chain(out.panel.order.iter()) {
            if PANEL_TABS.iter().any(|t| t.id == id) && !order.contains(id) {
                order.push(id.clone());
            }
        }
        panel.order = order;
        if !KEEP_AWAKE_CHOICES.contains(&panel.keep_awake_minutes) {
            panel.keep_awake_minutes = out.panel.keep_awake_minutes;
        }
        out.panel = panel;
        out.island = fit_typed("island", &self.island);
        for (id, accel) in out.shortcuts.iter_mut() {
            if let Some(saved) = self.shortcuts.get(id) {
                *accel = saved.clone();
            }
        }
        for field in catalog::fields().filter(|f| !catalog::TYPED.contains(&f.namespace)) {
            let saved = self.option(field.namespace, field.key).and_then(|v| accept(&field.control, v));
            if let (Some(value), Some(map)) = (saved, out.features.get_mut(field.namespace)) {
                map.insert(field.key.to_string(), value);
            }
        }
        out
    }

    /// Merges `value` (an object holding some or all keys) into a section.
    fn set_section(&mut self, section: &str, value: Value) -> Result<(), String> {
        let mut next = self.clone();
        match section {
            "general" => next.general = merge_typed(&self.general, value)?,
            "panel" => next.panel = merge_typed(&self.panel, value)?,
            "island" => next.island = merge_typed(&self.island, value)?,
            "shortcuts" => {
                let mut map = self.shortcuts.clone();
                let patch: BTreeMap<String, String> = serde_json::from_value(value).map_err(|e| e.to_string())?;
                map.extend(patch);
                validate_shortcuts(&map)?;
                next.shortcuts = map;
            }
            ns if self.features.contains_key(ns) => {
                let Value::Object(patch) = value else { return Err(format!("`{ns}` expects an object")) };
                for (key, v) in &patch {
                    let control = catalog::control(ns, key).ok_or_else(|| format!("unknown option `{ns}.{key}`"))?;
                    if accept(&control, v).is_none() {
                        return Err(format!("`{v}` does not fit `{ns}.{key}`"));
                    }
                }
                next.features.get_mut(ns).unwrap().extend(patch);
            }
            _ => return Err(format!("unknown settings section `{section}`")),
        }
        *self = next.normalized();
        Ok(())
    }

    /// Resets one section, or everything with `all`.
    fn reset_section(&mut self, section: &str) -> Result<(), String> {
        let defaults = Self::default();
        match section {
            "all" => *self = defaults,
            // Resetting a page's options never uninstalls it, and never brings the first-run chooser back.
            "general" => {
                let done = self.general.setup_done;
                self.general = defaults.general;
                self.general.setup_done = done;
            }
            "panel" => self.panel = defaults.panel,
            "island" => {
                let installed = self.island.enabled;
                self.island = defaults.island;
                self.island.enabled = installed;
            }
            "shortcuts" => self.shortcuts = defaults.shortcuts,
            ns if defaults.features.contains_key(ns) => {
                let installed = self.features.get(ns).and_then(|m| m.get("enabled")).cloned();
                self.features.insert(ns.to_string(), defaults.features[ns].clone());
                if let (Some(v), Some(map)) = (installed, self.features.get_mut(ns)) {
                    map.insert("enabled".into(), v);
                }
            }
            _ => return Err(format!("unknown settings section `{section}`")),
        }
        Ok(())
    }

    /// A copy without machine-specific values (folders and paths), for backups.
    fn exported(&self) -> Self {
        let mut out = self.clone();
        for map in out.features.values_mut() {
            map.retain(|key, _| !is_local(key));
        }
        out
    }

    /// `backup` applied over these settings, keeping this machine's folders and paths.
    fn imported(&self, backup: Settings) -> Self {
        let mut out = backup;
        for (ns, map) in &self.features {
            for (key, value) in map.iter().filter(|(k, _)| is_local(k)) {
                out.features.entry(ns.clone()).or_default().insert(key.clone(), value.clone());
            }
        }
        out.normalized()
    }
}

/// A saved file; before the Island had its own section, its switch was `features.everyday.topPill`.
fn from_saved(saved: Value) -> Settings {
    let legacy = saved.get("island").is_none() && saved.pointer("/features/everyday/topPill") == Some(&Value::Bool(true));
    let mut settings: Settings = serde_json::from_value(saved).unwrap_or_default();
    if legacy {
        settings.island.enabled = true;
    }
    settings
}

fn is_local(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("folder") || key.contains("path")
}

/// The catalog's check, plus shortcuts must be ones the OS can register.
fn accept(control: &catalog::Control, value: &Value) -> Option<Value> {
    let value = control.accept(value)?;
    if let (catalog::Control::Shortcut { .. }, Some(accel)) = (control, value.as_str()) {
        if !accel.is_empty() && keybinds::parse(accel).is_err() {
            return None;
        }
    }
    Some(value)
}

fn merge_typed<T: Serialize + DeserializeOwned>(current: &T, patch: Value) -> Result<T, String> {
    let mut value = serde_json::to_value(current).map_err(|e| e.to_string())?;
    match (&mut value, patch) {
        (Value::Object(map), Value::Object(patch)) => map.extend(patch),
        (_, other) => value = other,
    }
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// Runs a typed section's values through its catalog controls (clamping numbers, dropping bad choices).
fn fit_typed<T: Serialize + DeserializeOwned + Default>(namespace: &str, section: &T) -> T {
    let Ok(Value::Object(mut map)) = serde_json::to_value(section) else { return T::default() };
    let Ok(Value::Object(defaults)) = serde_json::to_value(T::default()) else { return T::default() };
    for field in catalog::fields().filter(|f| f.namespace == namespace) {
        let fitted = map.get(field.key).and_then(|v| accept(&field.control, v));
        match fitted.or_else(|| defaults.get(field.key).cloned()) {
            Some(v) => map.insert(field.key.to_string(), v),
            None => map.remove(field.key),
        };
    }
    serde_json::from_value(Value::Object(map)).unwrap_or_default()
}

fn validate_shortcuts(map: &BTreeMap<String, String>) -> Result<(), String> {
    let mut seen = BTreeMap::new();
    for (id, accel) in map.iter().filter(|(_, a)| !a.is_empty()) {
        let key = keybinds::parse(accel).map_err(|e| format!("{accel}: {e}"))?.id();
        if let Some(other) = seen.insert(key, id) {
            return Err(format!("{accel} is used by both {other} and {id}"));
        }
    }
    Ok(())
}

pub struct Store {
    path: PathBuf,
    settings: Mutex<Settings>,
}

impl Store {
    pub fn load(path: PathBuf) -> Self {
        let mut settings = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .map(from_saved)
            .unwrap_or_default()
            .normalized();
        settings.mark_existing_setup();
        Self { path, settings: Mutex::new(settings) }
    }

    pub fn get(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub(crate) fn update(&self, app: &AppHandle, change: impl FnOnce(&mut Settings) -> Result<(), String>) -> Result<Settings, String> {
        let (old, new) = {
            let mut settings = self.settings.lock().unwrap();
            let old = settings.clone();
            change(&mut settings)?;
            self.save(&settings)?;
            (old, settings.clone())
        };
        crate::on_settings_changed(app, &old, &new);
        let _ = app.emit(CHANGED_EVENT, &new);
        Ok(new)
    }

    fn save(&self, settings: &Settings) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, text).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    groups: &'static [catalog::Group],
    /// Features the first-run chooser ticks for "Recommended".
    recommended: &'static [&'static str],
    actions: &'static [keybinds::Action],
    reserved: &'static [keybinds::Reserved],
    panel_tabs: &'static [PanelTab],
    keep_awake_choices: &'static [u32],
}

#[tauri::command]
pub fn get_settings(store: State<Store>) -> Settings {
    store.get()
}

#[tauri::command]
pub fn get_catalog() -> Catalog {
    Catalog {
        groups: catalog::GROUPS,
        recommended: catalog::RECOMMENDED,
        actions: keybinds::ACTIONS,
        reserved: keybinds::RESERVED,
        panel_tabs: PANEL_TABS,
        keep_awake_choices: KEEP_AWAKE_CHOICES,
    }
}

#[tauri::command]
pub fn set_section(app: AppHandle, store: State<Store>, section: String, value: Value) -> Result<Settings, String> {
    store.update(&app, |s| s.set_section(&section, value))
}

/// The first-run chooser's answer: installs the ticked features and never asks again.
#[tauri::command]
pub fn finish_setup(app: AppHandle, store: State<Store>, install: Vec<String>) -> Result<Settings, String> {
    store.update(&app, |s| s.finish_setup(&install))
}

#[tauri::command]
pub fn reset_section(app: AppHandle, store: State<Store>, section: String) -> Result<Settings, String> {
    store.update(&app, |s| s.reset_section(&section))
}

/// Writes a backup to the Downloads folder and returns its path.
#[tauri::command]
pub fn export_settings(app: AppHandle, store: State<Store>) -> Result<String, String> {
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    let path = dir.join("notifi-settings.json");
    let text = serde_json::to_string_pretty(&store.get().exported()).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Applies a backup's text (as written by `export_settings`).
#[tauri::command]
pub fn import_settings(app: AppHandle, store: State<Store>, text: String) -> Result<Settings, String> {
    let backup: Settings = serde_json::from_str(&text).map_err(|e| format!("Not a Notifi backup: {e}"))?;
    store.update(&app, |s| {
        let next = s.imported(backup);
        validate_shortcuts(&next.shortcuts)?;
        *s = next;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn panel_settings_normalize() {
        let mut s = Settings::default();
        s.panel.last_tab = "nope".into();
        s.panel.keep_awake_minutes = 7;
        s.panel.tabs.insert("storage".into(), false);
        s.panel.tabs.insert("unknown".into(), true);
        s.panel.order = vec!["about".into(), "nope".into()];
        s.panel.left_click = "nope".into();
        let n = s.normalized();
        assert_eq!(n.panel.last_tab, Panel::default().last_tab);
        assert_eq!(n.panel.keep_awake_minutes, 60);
        assert_eq!(n.panel.tabs.get("storage"), Some(&false));
        assert!(!n.panel.tabs.contains_key("unknown"));
        assert_eq!(n.panel.tabs.len(), PANEL_TABS.len());
        assert_eq!(n.panel.order.first().map(String::as_str), Some("about"));
        assert_eq!(n.panel.order.len(), PANEL_TABS.len());
        assert_eq!(n.panel.left_click, "panel");
    }

    #[test]
    fn setup_chooser_installs_what_was_ticked_once() {
        let mut s = Settings::default();
        assert!(!s.any_installed() && !s.general.setup_done);
        s.finish_setup(&["island".into(), "clipboardHistory".into()]).unwrap();
        assert!(s.general.setup_done && s.island.enabled && s.enabled("clipboardHistory") && !s.enabled("screenshot"));
        // Only real features can be installed, and a bad id installs nothing.
        let mut t = Settings::default();
        assert!(t.finish_setup(&["general".into()]).is_err());
        assert!(t.finish_setup(&["nope".into()]).is_err());
        assert!(!t.general.setup_done);
    }

    #[test]
    fn people_with_features_already_on_skip_the_chooser() {
        let mut s = Settings::default();
        s.mark_existing_setup();
        assert!(!s.general.setup_done);
        s.features.get_mut("monitor").unwrap().insert("enabled".into(), json!(true));
        s.mark_existing_setup();
        assert!(s.general.setup_done);
        let mut only_island = Settings::default();
        only_island.island.enabled = true;
        only_island.mark_existing_setup();
        assert!(only_island.general.setup_done);
    }

    #[test]
    fn resetting_a_page_keeps_it_installed() {
        let mut s = Settings::default();
        s.finish_setup(&["island".into(), "screenshot".into()]).unwrap();
        s.reset_section("screenshot").unwrap();
        s.reset_section("island").unwrap();
        s.reset_section("general").unwrap();
        assert!(s.enabled("screenshot") && s.island.enabled && s.general.setup_done);
        s.reset_section("all").unwrap();
        assert!(!s.any_installed() && !s.general.setup_done);
    }

    #[test]
    fn panel_section_round_trips() {
        let mut s = Settings::default();
        s.set_section("panel", json!({ "lastTab": "system", "keepAwakeMinutes": 0 })).unwrap();
        assert_eq!(s.panel.last_tab, "system");
        assert_eq!(s.panel.keep_awake_minutes, 0);
        s.reset_section("panel").unwrap();
        assert_eq!(s.panel, Panel::default());
    }

    #[test]
    fn general_is_clamped_by_the_catalog() {
        let mut s = Settings::default();
        s.set_section("general", json!({ "updateCheckHours": 99, "language": "xx" })).unwrap();
        assert_eq!(s.general.update_check_hours, 24);
        assert_eq!(s.general.language, "system");
    }

    /// Every catalog option on a typed page is a field of that section, with the same default.
    #[test]
    fn typed_sections_match_the_catalog() {
        let defaults = serde_json::to_value(Settings::default()).unwrap();
        for f in catalog::fields().filter(|f| catalog::TYPED.contains(&f.namespace)) {
            let value = defaults[f.namespace].get(f.key).cloned();
            let expected = f.control.default_value();
            let same = match (&value, &expected) {
                (Some(Value::Number(a)), Some(Value::Number(b))) => a.as_f64() == b.as_f64(),
                _ => value == expected,
            };
            assert!(same, "{}.{}: {value:?} vs {expected:?}", f.namespace, f.key);
        }
    }

    #[test]
    fn feature_options_merge_and_validate() {
        let mut s = Settings::default();
        assert!(!s.enabled("appSwitcher"));
        s.set_section("appSwitcher", json!({ "enabled": true, "layout": "icons" })).unwrap();
        assert!(s.enabled("appSwitcher"));
        assert_eq!(s.option("appSwitcher", "layout"), Some(&json!("icons")));
        assert_eq!(s.option("appSwitcher", "thumbnailSize"), Some(&json!("normal")));
        assert!(s.set_section("appSwitcher", json!({ "layout": "huge" })).is_err());
        assert!(s.set_section("appSwitcher", json!({ "nope": 1 })).is_err());
        assert!(s.set_section("appSwitcher", json!({ "trigger": "not a key" })).is_err());
        s.set_section("appSwitcher", json!({ "appearDelay": 99999 })).unwrap();
        assert_eq!(s.option("appSwitcher", "appearDelay"), Some(&json!(1000)));
        s.reset_section("appSwitcher").unwrap();
        // The options are back to their defaults but the feature stays installed.
        assert!(s.enabled("appSwitcher"));
        assert_ne!(s.option("appSwitcher", "layout"), Some(&json!("icons")));
        assert_ne!(s.option("appSwitcher", "appearDelay"), Some(&json!(1000)));
    }

    #[test]
    fn every_default_shortcut_parses() {
        for f in catalog::fields() {
            if let catalog::Control::Shortcut { default } = f.control {
                assert!(default.is_empty() || keybinds::parse(default).is_ok(), "{}.{}: {default}", f.namespace, f.key);
            }
        }
    }

    #[test]
    fn old_files_load() {
        let old = json!({ "features": { "sound": { "volumeMixer": true } }, "general": { "theme": "dark" } });
        let s: Settings = serde_json::from_value(old).unwrap();
        let s = s.normalized();
        assert_eq!(s.general.theme, Theme::Dark);
        assert!(!s.features.contains_key("sound"));
        assert!(s.features.contains_key("volumeMixer"));
    }

    #[test]
    fn island_section() {
        let mut s = Settings::default();
        s.set_section("island", json!({ "enabled": true, "size": "spacious", "display": "DELL U2720Q" })).unwrap();
        assert!(s.island.enabled);
        assert_eq!(s.island.size, "spacious");
        assert_eq!(s.island.display, "DELL U2720Q");
        s.set_section("island", json!({ "size": "huge", "hoverDelayMs": 99999 })).unwrap();
        assert_eq!(s.island.size, "compact");
        assert_eq!(s.island.hover_delay_ms, 1500);
        s.reset_section("island").unwrap();
        // Back to the defaults, still installed.
        assert_eq!(s.island, IslandConfig { enabled: true, ..IslandConfig::default() });
        let legacy = from_saved(json!({ "features": { "everyday": { "topPill": true } } })).normalized();
        assert!(legacy.island.enabled);
    }

    #[test]
    fn backups_keep_local_paths() {
        let mut here = Settings::default();
        here.set_section("installerHelper", json!({ "portableFolder": "D:\\Apps" })).unwrap();
        here.set_section("autoClear", json!({ "delaySeconds": 30 })).unwrap();
        let mut there = Settings::default();
        there.set_section("installerHelper", json!({ "portableFolder": "E:\\Other" })).unwrap();
        let backup: Settings = serde_json::from_str(&serde_json::to_string(&here.exported()).unwrap()).unwrap();
        let merged = there.imported(backup);
        assert_eq!(merged.option("installerHelper", "portableFolder"), Some(&json!("E:\\Other")));
        assert_eq!(merged.option("autoClear", "delaySeconds"), Some(&json!(30)));
    }

    #[test]
    fn a_backup_restores_changed_options_and_reset_all_undoes_them() {
        let mut s = Settings::default();
        s.set_section("radialMenu", json!({ "slices": 10, "deadZone": 40 })).unwrap();
        s.set_section("monitor", json!({ "perCoreDefault": true })).unwrap();
        let text = serde_json::to_string_pretty(&s.exported()).unwrap();

        let mut fresh = Settings::default();
        fresh = fresh.imported(serde_json::from_str(&text).unwrap());
        assert_eq!(fresh.option("radialMenu", "slices"), Some(&json!(10)));
        assert_eq!(fresh.option("monitor", "perCoreDefault"), Some(&json!(true)));

        fresh.reset_section("all").unwrap();
        assert_eq!(fresh.option("radialMenu", "slices"), Settings::default().option("radialMenu", "slices"));
        assert_eq!(fresh.option("monitor", "perCoreDefault"), Some(&json!(false)));
        assert!(serde_json::from_str::<Settings>("not a backup").is_err());
    }
}
