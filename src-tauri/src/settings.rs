use std::{collections::BTreeMap, fs, path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use crate::keybinds;

pub const CHANGED_EVENT: &str = "settings://changed";

#[derive(Serialize, Clone, Copy)]
pub struct Feature {
    pub id: &'static str,
    pub label: &'static str,
    pub blurb: &'static str,
    /// Something the user opens (listed under Utilities) rather than a background behaviour.
    pub tool: bool,
}

#[derive(Serialize, Clone, Copy)]
pub struct FeatureGroup {
    pub id: &'static str,
    pub label: &'static str,
    pub features: &'static [Feature],
}

const fn f(id: &'static str, label: &'static str, blurb: &'static str) -> Feature {
    Feature { id, label, blurb, tool: false }
}

const fn t(id: &'static str, label: &'static str, blurb: &'static str) -> Feature {
    Feature { id, label, blurb, tool: true }
}

pub const FEATURE_GROUPS: &[FeatureGroup] = &[
    FeatureGroup {
        id: "sound",
        label: "Sound",
        features: &[
            f("volumeMixer", "Volume mixer", "Set the volume of each app on its own"),
            f("perAppOutput", "Per-app output", "Send each app to a different speaker or headset"),
            f("outputSwitcher", "Output switcher", "Change the default output in one click"),
            f("microphoneTools", "Microphone tools", "Mic level and a mute-all key"),
            f("mediaKeyBlocker", "Media key app blocker", "Stop media keys from starting a chosen app"),
        ],
    },
    FeatureGroup {
        id: "system",
        label: "System",
        features: &[
            f("systemStats", "CPU, memory, battery and power", "Live usage in the tray panel"),
            f("gpu", "GPU", "Graphics usage and memory"),
            f("temperatures", "Temperatures", "CPU, GPU and battery heat where readable"),
            f("fanControl", "Fan control (beta)", "Set fan curves where the hardware allows"),
            f("trayReadouts", "Tray readouts", "Show numbers in the tray icon"),
            f("network", "Network and speed test", "Live speeds, totals and a speed test"),
            f("alerts", "Alerts", "Get told when a value crosses a limit"),
        ],
    },
    FeatureGroup {
        id: "windows",
        label: "Windows",
        features: &[
            f("appSwitcher", "App switcher with live previews", "A switcher with a picture of every window"),
            f("layouts", "Window layouts and snapping", "Drag or press keys to tile windows"),
            f("quitOnClose", "Quit on close", "Closing an app's last window ends the app"),
            f("closeProtection", "Quit and close protection", "Hold the key to close, so slips do nothing"),
            f("panelPreview", "Panel preview strip (Linux)", "Window previews over the desktop panel"),
            f("panelClicks", "Minimise on panel click (Linux)", "Click a running app in the panel to hide it"),
        ],
    },
    FeatureGroup {
        id: "input",
        label: "Keyboard & Mouse",
        features: &[
            t("textSnippets", "Text snippets", "Type a short word, get a whole phrase"),
            f("smoothScrolling", "Smooth scrolling", "Even, gliding mouse wheel scrolling"),
            f("pointerAcceleration", "Pointer acceleration", "Turn pointer speed-up on or off"),
            f("focusFollowsMouse", "Focus follows mouse", "The window under the pointer gets the keys"),
            f("scrollDirection", "Scroll direction (mouse only)", "Flip the wheel without touching the touchpad"),
            f("sidewaysScroll", "Hold key to scroll sideways", "Hold a key to make the wheel scroll across"),
            f("mouseButtons", "Mouse buttons and drag gestures", "Give extra buttons and drags their own jobs"),
            f("touchpadMiddleClick", "Middle click from touchpad", "Tap with three fingers to middle click"),
            f("appExclusions", "Apps to leave alone", "Turn input changes off in chosen apps"),
            f("clickFilter", "Extra click filter and key debounce", "Ignore double clicks and repeats from worn parts"),
            f("superKey", "Super key", "Use Caps Lock as an extra modifier"),
        ],
    },
    FeatureGroup {
        id: "clipboard",
        label: "Clipboard",
        features: &[
            t("clipboardHistory", "Clipboard history", "Everything you copied, searchable"),
            f("autoClear", "Auto clear clipboard", "Wipe the clipboard after a while or on lock"),
            f("plainTextPaste", "Paste as plain text", "Paste without the formatting"),
            t("shelf", "Shelf", "A place to park files while you drag"),
            f("pasteImageAsFile", "Paste image as file", "Paste a copied image into a folder as a PNG"),
            f("cleanUrl", "Clean URL", "Strip tracking bits from copied links"),
            f("appImageInstaller", "AppImage installer (Linux)", "Install a downloaded AppImage with a menu entry"),
        ],
    },
    FeatureGroup {
        id: "everyday",
        label: "Everyday",
        features: &[
            t("commandBar", "Command Bar", "Search apps, files, windows and commands"),
            t("quickPanel", "Quick panel", "A small floating palette of actions"),
            f("quickToggles", "Quick toggles", "Dark mode, desktop icons, eject and more"),
            t("radialMenu", "Radial menu", "A ring of actions at the pointer"),
            t("scratchpad", "Scratchpad", "Quick notes with Markdown"),
            t("cleaningMode", "Cleaning Mode", "Lock the keyboard while you wipe it"),
            f("topPill", "Top Pill", "Media, timers and notices at the top of the screen"),
            t("aiAgents", "AI agents panel", "Token use and status of coding agents"),
        ],
    },
    FeatureGroup {
        id: "capture",
        label: "Capture",
        features: &[
            t("captureSelector", "Capture selector and magnifier", "Pick an area, window or screen to grab"),
            t("screenshotEditor", "Screenshot editor", "Mark up, crop and blur screenshots"),
            t("scrollingCapture", "Scrolling capture", "Grab a whole long page in one image"),
            t("screenRecording", "Screen recording", "Record the screen with sound"),
            t("textFromScreen", "Copy text from screen and QR", "Read text or a QR code off the screen"),
            t("colorPicker", "Color picker", "Pick any colour on screen"),
            t("cameraPreview", "Camera preview", "Your webcam in a floating window"),
            t("mediaTools", "Media tools", "Convert and trim video, audio and images"),
        ],
    },
    FeatureGroup {
        id: "apps",
        label: "Apps",
        features: &[
            t("appUpdates", "App updates", "See and install updates for your apps"),
            t("packageManager", "Package manager", "Search, install and remove apps"),
            t("uninstaller", "Uninstaller and leftovers", "Remove apps and the files they leave"),
            t("cleaner", "Cleaner", "Clear caches, temp files and logs"),
            f("messagingDownloads", "Messaging downloads", "Tidy chat app download folders"),
            t("portManager", "Port manager", "See which app holds a port and end it"),
        ],
    },
    FeatureGroup {
        id: "energy",
        label: "Energy & Display",
        features: &[
            f("keepAwake", "Keep awake", "Stop the computer from sleeping"),
            f("lidClosedAwake", "Keep awake with lid closed", "Keep running with the laptop lid shut"),
            t("displays", "Displays and brightness", "Brightness for every screen, in fine steps"),
            f("softwareDim", "Extra dimming", "Go darker than the lowest brightness"),
            f("bluetoothSleep", "Bluetooth off during sleep", "Turn Bluetooth off while asleep"),
        ],
    },
];

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct PanelTab {
    pub id: &'static str,
    pub label: &'static str,
    /// `group.feature` that must be on for the tab to show; `None` means always available.
    pub feature: Option<&'static str>,
    /// Settings section the tab's customise button opens.
    pub section: Option<&'static str>,
    /// False until the tab's backend ships; the tab stays hidden.
    pub ready: bool,
}

pub const PANEL_TABS: &[PanelTab] = &[
    PanelTab { id: "keepAwake", label: "Keep awake", feature: Some("energy.keepAwake"), section: Some("energy"), ready: true },
    PanelTab { id: "displays", label: "Displays", feature: Some("energy.displays"), section: Some("energy"), ready: false },
    PanelTab { id: "mixer", label: "Mixer", feature: Some("sound.volumeMixer"), section: Some("sound"), ready: true },
    PanelTab { id: "system", label: "System", feature: Some("system.systemStats"), section: Some("system"), ready: true },
    PanelTab { id: "network", label: "Network", feature: Some("system.network"), section: Some("system"), ready: true },
    PanelTab { id: "storage", label: "Storage", feature: None, section: Some("panel"), ready: true },
    PanelTab { id: "power", label: "Power", feature: Some("system.systemStats"), section: Some("energy"), ready: true },
    PanelTab { id: "utilities", label: "Utilities", feature: None, section: Some("everyday"), ready: true },
    PanelTab { id: "controls", label: "Controls", feature: None, section: Some("panel"), ready: true },
    PanelTab { id: "about", label: "About", feature: None, section: Some("general"), ready: true },
];

pub const KEEP_AWAKE_CHOICES: &[u32] = &[15, 60, 120, 0];

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Panel {
    pub last_tab: String,
    /// Tabs the user wants in the panel; a tab also needs its feature on.
    pub tabs: BTreeMap<String, bool>,
    /// Keep awake duration in minutes, 0 = until turned off.
    pub keep_awake_minutes: u32,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            last_tab: "controls".into(),
            tabs: PANEL_TABS.iter().map(|t| (t.id.to_string(), true)).collect(),
            keep_awake_minutes: 60,
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
    pub reduced_motion: bool,
    pub auto_update: bool,
    pub update_check_hours: u32,
    pub install_on_quit: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            autostart: false,
            theme: Theme::System,
            reduced_motion: false,
            auto_update: true,
            update_check_hours: 3,
            install_on_quit: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Settings {
    pub general: General,
    pub shortcuts: BTreeMap<String, String>,
    pub features: BTreeMap<String, BTreeMap<String, bool>>,
    pub panel: Panel,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            general: General::default(),
            shortcuts: keybinds::defaults(),
            features: FEATURE_GROUPS
                .iter()
                .map(|g| {
                    let features = g.features.iter().map(|f| (f.id.to_string(), false)).collect();
                    (g.id.to_string(), features)
                })
                .collect(),
            panel: Panel::default(),
        }
    }
}

impl Settings {
    /// Fills in missing keys from the defaults and drops unknown ones.
    fn normalized(self) -> Self {
        let mut out = Self::default();
        out.general = self.general;
        out.general.update_check_hours = out.general.update_check_hours.clamp(0, 24);
        if PANEL_TABS.iter().any(|t| t.id == self.panel.last_tab) {
            out.panel.last_tab = self.panel.last_tab;
        }
        for (id, on) in out.panel.tabs.iter_mut() {
            if let Some(saved) = self.panel.tabs.get(id) {
                *on = *saved;
            }
        }
        if KEEP_AWAKE_CHOICES.contains(&self.panel.keep_awake_minutes) {
            out.panel.keep_awake_minutes = self.panel.keep_awake_minutes;
        }
        for (id, accel) in out.shortcuts.iter_mut() {
            if let Some(saved) = self.shortcuts.get(id) {
                *accel = saved.clone();
            }
        }
        for (group, features) in out.features.iter_mut() {
            let Some(saved) = self.features.get(group) else { continue };
            for (id, on) in features.iter_mut() {
                if let Some(saved) = saved.get(id) {
                    *on = *saved;
                }
            }
        }
        out
    }

    fn set_section(&mut self, section: &str, value: Value) -> Result<(), String> {
        let mut next = self.clone();
        match section {
            "general" => next.general = serde_json::from_value(value).map_err(|e| e.to_string())?,
            "panel" => next.panel = serde_json::from_value(value).map_err(|e| e.to_string())?,
            "shortcuts" => {
                let map: BTreeMap<String, String> = serde_json::from_value(value).map_err(|e| e.to_string())?;
                validate_shortcuts(&map)?;
                next.shortcuts = map;
            }
            group if self.features.contains_key(group) => {
                let map = serde_json::from_value(value).map_err(|e| e.to_string())?;
                next.features.insert(group.to_string(), map);
            }
            _ => return Err(format!("unknown settings section `{section}`")),
        }
        *self = next.normalized();
        Ok(())
    }

    fn reset_section(&mut self, section: &str) -> Result<(), String> {
        let defaults = Self::default();
        match section {
            "general" => self.general = defaults.general,
            "panel" => self.panel = defaults.panel,
            "shortcuts" => self.shortcuts = defaults.shortcuts,
            group if defaults.features.contains_key(group) => {
                self.features.insert(group.to_string(), defaults.features[group].clone());
            }
            _ => return Err(format!("unknown settings section `{section}`")),
        }
        Ok(())
    }
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
        let settings = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
            .unwrap_or_default()
            .normalized();
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
    groups: &'static [FeatureGroup],
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
        groups: FEATURE_GROUPS,
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

#[tauri::command]
pub fn reset_section(app: AppHandle, store: State<Store>, section: String) -> Result<Settings, String> {
    store.update(&app, |s| s.reset_section(&section))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_settings_normalize() {
        let mut s = Settings::default();
        s.panel.last_tab = "nope".into();
        s.panel.keep_awake_minutes = 7;
        s.panel.tabs.insert("storage".into(), false);
        s.panel.tabs.insert("unknown".into(), true);
        let n = s.normalized();
        assert_eq!(n.panel.last_tab, Panel::default().last_tab);
        assert_eq!(n.panel.keep_awake_minutes, 60);
        assert_eq!(n.panel.tabs.get("storage"), Some(&false));
        assert!(!n.panel.tabs.contains_key("unknown"));
        assert_eq!(n.panel.tabs.len(), PANEL_TABS.len());
    }

    #[test]
    fn panel_section_round_trips() {
        let mut s = Settings::default();
        s.set_section("panel", serde_json::json!({ "lastTab": "system", "keepAwakeMinutes": 0 })).unwrap();
        assert_eq!(s.panel.last_tab, "system");
        assert_eq!(s.panel.keep_awake_minutes, 0);
        s.reset_section("panel").unwrap();
        assert_eq!(s.panel, Panel::default());
    }
}
