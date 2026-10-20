use super::*;

pub const PAGES: &[Page] = &[
    Page {
        id: "keepAwake",
        label: "Keep awake",
        icon: "keepAwake",
        blurb: "Stop the computer sleeping for a while, or whenever chosen apps, mains power or an external display are in use.",
        cards: &[
            card("Triggers", &[
                list("triggerApps", "Stay awake while these apps run", "", "App name"),
                sw("triggerPower", "Stay awake on mains power", "", false),
                sw("triggerDisplay", "Stay awake with an external display", "", false),
                pick("combine", "When several triggers are set", "", &[c("any", "Any one is enough"), c("all", "All must apply")], "any"),
            ]),
            card("Behaviour", &[
                sw("pauseOnLock", "Pause while the screen is locked", "", false),
                sw("allowDisplaySleep", "Let displays sleep", "Keeps the computer running but lets the screen turn off.", false),
                sw("startAtLaunch", "Start a session when Notifi opens", "", false),
                sw("rightClickToggle", "Toggle with a right click on the tray icon", "", false),
                sw("showTimeLeft", "Show time left by the tray icon", "", true),
                key("toggle", "Toggle shortcut", "Starts or stops the last session.", "Ctrl+Alt+A"),
            ]),
            card("Pointer nudge", &[
                sw("pointerNudge", "Nudge the pointer", "For apps that count idle time.", false),
                num("nudgeInterval", "Nudge every", "", 10.0, 600.0, 10.0, "seconds", 60.0),
            ]),
            card("Battery", &[
                sw("batteryCutoff", "Stop on low battery", "Ends or refuses a session on battery power.", true),
                num("cutoffLevel", "Stop below", "", 5.0, 80.0, 5.0, "%", 20.0),
            ]),
            card("Tray icon", &[
                tiles("icon", "Icon when active", "", &[ci("notifi", "Notifi", "keepAwake"), ci("coffee", "Coffee", "heart"), ci("eye", "Eye", "eye"), ci("moon", "Moon", "moon")], "notifi"),
                text("iconColor", "Icon colour", "A hex colour for the active icon.", "#4C8DFF", ""),
            ]),
            Card { title: "Lid closed", feature: "lidClosedAwake", options: &[
                sw("dimBuiltIn", "Dim the built-in display", "Dims or turns off the laptop screen while the lid is closed.", false),
            ], ..CARD },
        ],
        specs: &["energy/keep-awake.md", "energy/keep-awake-lid-closed.md"],
        ..PAGE
    },
    Page {
        id: "displays",
        label: "Displays and brightness",
        icon: "displays",
        blurb: "Control the brightness of each display, with finer steps, per-display off and a keyboard backlight slider.",
        tool: true,
        tabs: &["Brightness", "Overlays", "Wallpaper"],
        cards: &[
            tab_card("Brightness", "Keys", &[
                tiles("keyStep", "Brightness key step", "", &[ci("full", "Full", "sun"), ci("half", "Half", "sun"), ci("quarter", "Quarter", "sun")], "full"),
                pick("keyTarget", "Keys change", "", &[c("pointer", "Display under the pointer"), c("primary", "Main display")], "pointer"),
                sw("customShortcuts", "Use custom shortcuts", "", false),
                key("brightnessUp", "Brightness up", "", ""),
                key("brightnessDown", "Brightness down", "", ""),
                key("previousDisplay", "Previous display", "", ""),
                key("nextDisplay", "Next display", "", ""),
                key("displayOff", "Switch display off or on", "", ""),
            ]),
            tab_card("Brightness", "Keyboard backlight", &[
                sw("keyboardBacklight", "Show the backlight slider", "Only where a backlight exists.", true),
                key("backlightUp", "Backlight up", "", ""),
                key("backlightDown", "Backlight down", "", ""),
            ]),
            Card { title: "Extra brightness", tab: "Brightness", feature: "extraBrightness", options: &[
                slider("intensity", "Intensity", "Makes normal content brighter on displays in HDR mode.", 0.0, 100.0, 5.0, "%", 50.0),
                sw("pauseDuringHdr", "Pause during HDR video", "", true),
                list("onlyDisplays", "Only these displays", "Leave empty for every HDR display.", "Display name"),
            ], ..CARD },
            Card { title: "Level overlays", tab: "Overlays", feature: "osdOverlays", options: &[
                sw("volumeOverlay", "Volume overlay", "", true),
                sw("brightnessOverlay", "Brightness overlay", "", true),
                sw("backlightSlider", "Keyboard light slider", "", true),
                sw("micMuteIndicator", "Mic muted indicator in the tray", "", true),
                sw("bluetoothNotices", "Bluetooth audio connect notices", "", true),
                pick("overlayPlace", "Show in", "", &[c("pill", "Top pill"), c("floating", "Floating overlay")], "pill"),
            ], ..CARD },
            Card { title: "Wallpaper gallery", tab: "Wallpaper", feature: "wallpaper", options: &[
                list("folders", "Folders", "Your Pictures folder is used when empty.", "Folder path"),
                pick("applyTo", "Apply to", "", &[c("current", "Current display"), c("all", "All displays")], "current"),
                custom("gallery", "Gallery", "Click a picture to set it as the wallpaper."),
            ], ..CARD },
        ],
        specs: &["energy/displays-brightness.md", "energy/extra-brightness.md", "everyday/osd-overlays.md", "apps/wallpaper-gallery.md"],
        ..PAGE
    },
    Page {
        id: "extraDimming",
        label: "Extra dimming",
        icon: "moon",
        blurb: "Dim a monitor below its lowest hardware brightness.",
        cards: &[card("Dimming", &[
            custom("perDisplay", "Displays", "Choose which monitors use extra dimming."),
            slider("softwareShare", "Share of the slider used for extra dimming", "The lower part of the slider dims in software.", 10.0, 50.0, 5.0, "%", 25.0),
            slider("minimumFloor", "Never dimmer than", "Keeps the screen from going black.", 1.0, 30.0, 1.0, "%", 5.0),
        ])],
        specs: &["energy/extra-dimming.md"],
        ..PAGE
    },
    Page {
        id: "bluetoothSleep",
        label: "Bluetooth on sleep",
        icon: "bluetooth",
        blurb: "Turns Bluetooth off while the computer sleeps and back on at wake, only if Notifi turned it off.",
        cards: &[card("Sleep", &[
            sw("disconnectHeadphones", "Disconnect headphones when sleep begins", "", true),
        ])],
        specs: &["energy/bluetooth-on-sleep.md"],
        ..PAGE
    },
];
