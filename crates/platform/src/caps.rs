use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Feature {
    AudioSessions,
    AudioPerAppOutput,
    AudioDevices,
    AudioMicrophone,
    AudioEvents,
    WindowList,
    WindowControl,
    WindowThumbnails,
    WindowEvents,
    InputHook,
    InputInject,
    InputGrab,
    ClipboardWatch,
    ClipboardReadWrite,
    DisplayList,
    DisplayBrightness,
    DisplayDdc,
    DisplayDim,
    KeepAwake,
    LidInhibit,
    LockScreen,
    PowerEvents,
    PowerProfile,
    Screenshot,
    ScreenRecording,
    Packages,
    SensorsCpu,
    SensorsMemory,
    SensorsGpu,
    SensorsTemperature,
    SensorsFans,
    SensorsBattery,
    DarkModeRead,
    DarkModeWrite,
    DesktopIcons,
    Trash,
    Eject,
    Notifications,
    Media,
}

impl Feature {
    pub const ALL: &'static [Feature] = &[
        Feature::AudioSessions,
        Feature::AudioPerAppOutput,
        Feature::AudioDevices,
        Feature::AudioMicrophone,
        Feature::AudioEvents,
        Feature::WindowList,
        Feature::WindowControl,
        Feature::WindowThumbnails,
        Feature::WindowEvents,
        Feature::InputHook,
        Feature::InputInject,
        Feature::InputGrab,
        Feature::ClipboardWatch,
        Feature::ClipboardReadWrite,
        Feature::DisplayList,
        Feature::DisplayBrightness,
        Feature::DisplayDdc,
        Feature::DisplayDim,
        Feature::KeepAwake,
        Feature::LidInhibit,
        Feature::LockScreen,
        Feature::PowerEvents,
        Feature::PowerProfile,
        Feature::Screenshot,
        Feature::ScreenRecording,
        Feature::Packages,
        Feature::SensorsCpu,
        Feature::SensorsMemory,
        Feature::SensorsGpu,
        Feature::SensorsTemperature,
        Feature::SensorsFans,
        Feature::SensorsBattery,
        Feature::DarkModeRead,
        Feature::DarkModeWrite,
        Feature::DesktopIcons,
        Feature::Trash,
        Feature::Eject,
        Feature::Notifications,
        Feature::Media,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Feature::AudioSessions => "audio-sessions",
            Feature::AudioPerAppOutput => "audio-per-app-output",
            Feature::AudioDevices => "audio-devices",
            Feature::AudioMicrophone => "audio-microphone",
            Feature::AudioEvents => "audio-events",
            Feature::WindowList => "window-list",
            Feature::WindowControl => "window-control",
            Feature::WindowThumbnails => "window-thumbnails",
            Feature::WindowEvents => "window-events",
            Feature::InputHook => "input-hook",
            Feature::InputInject => "input-inject",
            Feature::InputGrab => "input-grab",
            Feature::ClipboardWatch => "clipboard-watch",
            Feature::ClipboardReadWrite => "clipboard-read-write",
            Feature::DisplayList => "display-list",
            Feature::DisplayBrightness => "display-brightness",
            Feature::DisplayDdc => "display-ddc",
            Feature::DisplayDim => "display-dim",
            Feature::KeepAwake => "keep-awake",
            Feature::LidInhibit => "lid-inhibit",
            Feature::LockScreen => "lock-screen",
            Feature::PowerEvents => "power-events",
            Feature::PowerProfile => "power-profile",
            Feature::Screenshot => "screenshot",
            Feature::ScreenRecording => "screen-recording",
            Feature::Packages => "packages",
            Feature::SensorsCpu => "sensors-cpu",
            Feature::SensorsMemory => "sensors-memory",
            Feature::SensorsGpu => "sensors-gpu",
            Feature::SensorsTemperature => "sensors-temperature",
            Feature::SensorsFans => "sensors-fans",
            Feature::SensorsBattery => "sensors-battery",
            Feature::DarkModeRead => "dark-mode-read",
            Feature::DarkModeWrite => "dark-mode-write",
            Feature::DesktopIcons => "desktop-icons",
            Feature::Trash => "trash",
            Feature::Eject => "eject",
            Feature::Notifications => "notifications",
            Feature::Media => "media",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "detail", rename_all = "kebab-case")]
pub enum Support {
    Supported,
    Partial(String),
    NeedsPermission(String),
    Unsupported(String),
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Capabilities {
    features: BTreeMap<Feature, Support>,
}

impl Capabilities {
    /// Every feature starts as "not implemented yet"; backends then mark what they provide.
    pub fn not_implemented() -> Self {
        let features = Feature::ALL
            .iter()
            .map(|f| (*f, Support::Unsupported("not implemented yet".into())))
            .collect();
        Self { features }
    }

    pub fn set(&mut self, feature: Feature, support: Support) -> &mut Self {
        self.features.insert(feature, support);
        self
    }

    pub fn get(&self, feature: Feature) -> &Support {
        static UNKNOWN: Support = Support::Unsupported(String::new());
        self.features.get(&feature).unwrap_or(&UNKNOWN)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Feature, &Support)> {
        self.features.iter().map(|(f, s)| (*f, s))
    }
}
