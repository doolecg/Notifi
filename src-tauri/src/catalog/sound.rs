use super::*;

pub const PAGES: &[Page] = &[
    Page {
        id: "mediaKeyBlocker",
        label: "Media-key app blocker",
        icon: "music",
        blurb: "Stops a chosen app from opening when you press a media key and nothing is playing.",
        cards: &[card("Blocked apps", &[
            list("blockedApps", "Apps to block", "Launches of these apps right after a media key press are cancelled.", "Media player name"),
            num("detectionWindow", "Detection window", "How long after a key press a launch counts as caused by it.", 200.0, 5000.0, 100.0, "ms", 1500.0),
        ])],
        specs: &["sound/music-app-blocker.md"],
        ..PAGE
    },
    Page {
        id: "volumeMixer",
        label: "Volume mixer",
        icon: "mixer",
        blurb: "Set the volume of each app on its own, and boost quiet apps past 100%.",
        cards: &[
            card("Levels", &[
                sw("allowBoost", "Allow boost past 100%", "Needs a system audio driver on Windows.", true),
                slider("boostCap", "Boost limit", "", 100.0, 400.0, 25.0, "%", 200.0),
                sw("rememberLevels", "Remember each app's level", "", true),
            ]),
            card("Apps shown", &[
                pick("showApps", "Show", "", &[c("active", "Only apps playing now"), c("recent", "Playing now and the last 5 minutes")], "recent"),
                list("pinnedApps", "Pinned apps", "Always shown at the top, in this order.", "App name"),
            ]),
            card("Shortcut", &[
                key("openMixer", "Open the mixer", "", "Ctrl+Alt+M"),
            ]),
        ],
        specs: &["sound/volume-mixer.md"],
        ..PAGE
    },
    Page {
        id: "perAppOutput",
        label: "Per-app output",
        icon: "output",
        blurb: "Choose which output device each app plays through.",
        cards: &[card("Routing", &[
            sw("rememberDevice", "Remember the device for each app", "Applied again when the app or device returns.", true),
            custom("resetRoutes", "Reset all routes", "Send every app back to the default output."),
        ])],
        specs: &["sound/per-app-output.md"],
        ..PAGE
    },
    Page {
        id: "outputSwitcher",
        label: "Output switcher",
        icon: "headphones",
        blurb: "Switch the sound output with a shortcut and set the volume when headphones come and go.",
        cards: &[
            card("Switching", &[
                key("cycleOutput", "Next output", "Cycles through the devices ticked below.", "Ctrl+Alt+O"),
                list("cycleDevices", "Devices to cycle", "Leave empty to use every output.", "Device name"),
            ]),
            card("Headphones", &[
                sw("setOnDisconnect", "Set the volume when headphones disconnect", "", true),
                slider("volumeOnDisconnect", "Volume on disconnect", "0% also mutes.", 0.0, 100.0, 5.0, "%", 0.0),
                sw("setOnConnect", "Set the volume when headphones connect", "", false),
                slider("volumeOnConnect", "Volume on connect", "", 0.0, 100.0, 5.0, "%", 50.0),
            ]),
            Card { title: "Device priority", feature: "devicePriority", options: &[
                list("outputRanking", "Output order", "Highest connected device becomes the default.", "Output device name"),
                list("microphoneRanking", "Microphone order", "Highest connected device becomes the default.", "Microphone name"),
                sw("ignoreVirtual", "Ignore virtual devices", "", true),
            ], ..CARD },
        ],
        specs: &["sound/output-switcher.md", "sound/device-priority.md"],
        ..PAGE
    },
    Page {
        id: "microphone",
        label: "Microphone",
        icon: "mic",
        blurb: "Pick a preferred microphone and mute every microphone with one shortcut.",
        cards: &[
            card("Microphone", &[
                text("preferredMicrophone", "Preferred microphone", "Becomes the default whenever it is connected.", "Device name", ""),
                key("muteAll", "Mute all microphones", "Press again to unmute.", "Ctrl+Alt+Shift+M"),
            ]),
            card("Notices", &[
                sw("showMuteNotices", "Show mute and unmute notices", "", true),
            ]),
        ],
        specs: &["sound/microphone-tools.md"],
        ..PAGE
    },
];
