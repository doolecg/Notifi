use super::*;

const MODULES: &[Choice] = &[
    c("controls", "Controls"),
    c("mixer", "Volume mixer"),
    c("nowPlaying", "Now Playing"),
    c("clipboard", "Clipboard"),
    c("captures", "Recent captures"),
    c("shelf", "Files"),
    c("system", "System"),
    c("tools", "Tools"),
    c("calendar", "Calendar"),
    c("notifications", "Notifications"),
    c("timer", "Timer"),
    c("camera", "Camera mirror"),
    c("downloads", "Downloads"),
    c("scratchpad", "Scratchpad"),
    c("agents", "AI Agents"),
];

const MODULE_IDS: &[&str] = &[
    "controls", "mixer", "nowPlaying", "clipboard", "captures", "shelf", "system", "tools", "calendar", "notifications", "timer",
    "camera", "downloads", "scratchpad", "agents",
];

pub const PAGES: &[Page] = &[
    Page {
        id: "general",
        label: "General",
        icon: "settings",
        blurb: "Startup, appearance, language, updates, backups and resetting everything.",
        feature: false,
        cards: &[
            card("Startup", &[
                sw("autostart", "Start at login", "Notifi waits in the tray until you need it.", false),
            ]),
            card("Appearance", &[
                tiles("theme", "Theme", "", &[ci("system", "System", "displays"), ci("light", "Light", "sun"), ci("dark", "Dark", "moon")], "system"),
                sw("translucent", "Translucent windows", "Acrylic on Windows, blur where the Linux desktop supports it.", false),
                sw("reducedMotion", "Reduce motion", "Also follows your system's animation setting.", false),
                pick("language", "Language", "More languages arrive as translations are contributed.", &[c("system", "Same as the system"), c("en", "English")], "system"),
            ]),
            card("Updates", &[
                sw("autoUpdate", "Update automatically", "Checks GitHub releases and downloads new versions in the background.", true),
                num("updateCheckHours", "Check every", "0 checks only at startup.", 0.0, 24.0, 1.0, "hours", 3.0),
                sw("installOnQuit", "Install on quit", "Installs a downloaded update when you quit Notifi.", true),
                sw("betaChannel", "Beta versions", "Also offer pre-releases.", false),
                sw("previewNotes", "Preview release notes before updating", "", true),
                sw("whatsNew", "Show what's new after updating", "", true),
                custom("updateStatus", "Status", "Check for an update now."),
            ]),
            card("Help", &[
                sw("showTour", "Show the tour after updates", "A short look at new features, once.", true),
                sw("haptics", "Haptic feedback", "Small taps on touchpads that support them.", false),
            ]),
            card("Backup", &[
                custom("exportBackup", "Export settings", "Saves your setup to a file. Folders and personal content stay out."),
                custom("importBackup", "Import settings", "Loads a setup exported from Notifi."),
            ]),
            card("Advanced", &[
                custom("resetAll", "Reset everything", "Every page goes back to its defaults."),
            ]),
        ],
        specs: &["app/backup-language-appearance.md", "app/updates-and-whats-new.md", "app/haptics.md"],
        ..PAGE
    },
    Page {
        id: "panel",
        label: "Tray & panel",
        icon: "controls",
        blurb: "The tray icon and what the panel shows when you click it.",
        feature: false,
        cards: &[
            card("Tray icon", &[
                tiles("trayIcon", "Icon", "", &[ci("default", "Notifi", "about"), ci("mono", "Plain", "controls"), ci("dot", "Dot", "check")], "default"),
                sw("hideIconWithReadouts", "Hide the icon while readouts show", "Click the first readout to open the panel.", false),
                tiles("leftClick", "Left click", "", &[ci("panel", "Open the panel", "controls"), ci("settings", "Open settings", "settings")], "panel"),
            ]),
            card("Panel", &[
                custom("tabs", "Tabs", "Which tabs show and in what order."),
                sw("compact", "Compact layout", "Tighter rows so more fits.", false),
            ]),
        ],
        specs: &["app/tray-icon.md", "panel.md"],
        ..PAGE
    },
    Page {
        id: "installed",
        label: "Installed",
        icon: "check",
        blurb: "What you have installed, as a list or a grid. Open one to change its settings.",
        feature: false,
        custom: true,
        cards: &[card("Installed", &[custom("list", "Installed features", "Open one for its settings, or uninstall it.")])],
        specs: &["app/features-hub.md"],
        ..PAGE
    },
    Page {
        id: "features",
        label: "Install features",
        icon: "arrowDown",
        blurb: "Install only what you use. Features you have not installed do not run and use nothing.",
        feature: false,
        custom: true,
        cards: &[card("Features", &[custom("grid", "Every feature", "Install or open each one.")])],
        specs: &["app/features-hub.md"],
        ..PAGE
    },
    Page {
        id: "island",
        label: "Island",
        icon: "island",
        blurb: "A black shape at the top of the screen that grows to show music, controls and small tools.",
        tabs: &["Layout", "Content", "Activity", "Behaviour"],
        // The cards holding a custom option are drawn by ui/src/pages/IslandPage.svelte; the other
        // options in them are listed so they are stored, checked and found by search.
        custom: true,
        cards: &[
            tab_card("Layout", "Layout", &[
                custom("preview", "Open the Island", "Shows the real Island as a preview."),
                custom("layout", "Orbit buttons", "Click + to add a button. Drag buttons around the Island. Click a button to edit it."),
                list_of("orbitLeft", "Left column", "", "Module or action", &["grid", "timer"]),
                list_of("orbitRight", "Right column", "", "Module or action", &["settings", "volume"]),
                list_of("orbitBottom", "Below the Island", "", "Module or action", &["nowPlaying"]),
                tiles("size", "Size", "", &[ci("compact", "Compact", "island"), ci("spacious", "Spacious", "island"), ci("custom", "Custom", "customise")], "compact"),
                num("customWidth", "Custom width", "", 380.0, 2400.0, 10.0, "px", 1200.0),
                num("customHeight", "Custom height", "", 200.0, 1200.0, 10.0, "px", 270.0),
                sw("translucent", "Translucent background", "Blurs what's behind the open Island where the system allows it.", false),
            ]),
            tab_card("Content", "Modules", &[
                list_of("modules", "Modules in the grid", "In this order.", "Module", MODULE_IDS),
                pick("defaultModule", "Opens on", "", MODULES, "nowPlaying"),
                list_of("controlsButtons", "Controls quick buttons", "", "Module or action", &["mixer", "keepAwake", "timer", "calendar"]),
                sw("showLyrics", "Lyrics", "Only where the player provides them.", true),
                sw("equaliser", "Equaliser", "", true),
                text("preferredPlayer", "Player to prefer", "When several are playing.", "Any player", ""),
            ]),
            tab_card("Content", "Resting pill", &[
                sw("idleThinBar", "Thin bar when idle", "Shrinks to a thin bar and grows back when you point at it.", true),
                sw("idleAiUsage", "AI usage", "Share of the Claude 5-hour session used.", true),
                sw("idleCpuRam", "CPU and memory", "", true),
                sw("idleBattery", "Battery", "Shows CPU instead on computers without a battery.", true),
                pick("pillStyle", "Pill style", "How the resting pill looks. Calm: weather, rings and one Ask AI button. Three chips: separate small pills. Native text: no black shape, like the taskbar. Agents first: how many agents are working leads. Ask field: a search-style Ask AI box. Rings only: the smallest.", &[c("calm", "Calm"), c("chips", "Three chips"), c("native", "Native text"), c("agents", "Agents first"), c("field", "Ask field"), c("rings", "Rings only")], "calm"),
                sw("idleNotifications", "Notification count", "A bell with the number of notifications waiting in the list.", true),
                text("weatherCity", "Weather city", "Shows the weather on the pill. Only this city name is sent, to Open-Meteo (open-meteo.com), a free weather service. Leave empty for no weather and no internet use.", "For example Manchester", ""),
                list_of("aiPrompts", "Quick AI prompts", "Up to four buttons on the pill. A tap copies the prompt (with any text you had copied after it) and opens the AI chat, ready to paste. Needs the AI chat button turned on.", "Prompt", &["Explain this", "Summarize this", "Fix this"]),
            ]),
            tab_card("Content", "AI questions", &[
                sw("askInIsland", "Show AI questions in the Island", "When Claude Code asks you something or needs permission, it appears in the Island so you can answer there.", false),
                custom("askHooks", "Claude Code link", "Adds or removes Notifi's hooks in your Claude Code settings. Your other hooks are kept."),
            ]),
            tab_card("Content", "AI chat", &[
                sw("chatEnabled", "AI chat button", "Adds an AI chat tab and a dictate button to the Island, and a chat button to the taskbar AI Agents bar. The chat opens inside the Island and is T3 Code's own chat, so T3 Code must be installed and running. Notifi asks T3 Code for a one-time pairing link, the same as pairing a phone, which gives that window full access to your T3 Code, including your projects and files. Dictation uses Windows voice typing (Win+H): it needs Windows 10 or 11 with online speech recognition turned on and a microphone, and Microsoft processes your speech. Operant, OpenCode and Claude are not supported yet.", false),
            ]),
            tab_card("Content", "AI agents", &[
                custom("claudeUsage", "Claude usage", "Show your real 5 hour and weekly limits from your Claude Code login."),
                num("claudePlanLimitUsd", "Claude plan limit per session", "API value of one 5-hour session on your plan. 0 shows elapsed time instead.", 0.0, 1000.0, 1.0, "$", 0.0),
                pick("agentsPeriod", "Spending period", "", &[c("today", "Today"), c("7d", "7 days"), c("30d", "30 days")], "7d"),
            ]),
            tab_card("Activity", "Events", &[
                custom("activity", "Events", "Which events may grow the resting pill, and for how long."),
                sw("privacyMode", "Privacy mode", "Stops Notifi's notifications while on. Also a quick toggle with its own shortcut. Windows Do Not Disturb stops them too.", false),
                sw("activitySound", "Notification sound", "A short chime, as loud as the Windows notification sound, when a notification shows on the pill. Silent during Do Not Disturb and Privacy mode.", true),
                sw("alertsInIsland", "Alerts on the Island, not Windows toasts", "Notifi's own alerts (temperatures, disks, updates) show on the Island while it is on screen. A Windows toast is only the fallback.", true),
                sw("activityTrack", "Track changes", "", true),
                sw("activityTimer", "Timer done", "", true),
                sw("activityAgent", "AI agent task finished", "", true),
                num("agentFinishMinutes", "Only for agent tasks longer than", "", 0.0, 120.0, 1.0, "min", 2.0),
                sw("hideTimerCountdown", "Hide the timer countdown on the resting pill", "", false),
                slider("activityMs", "Show events for", "", 1000.0, 15000.0, 500.0, "ms", 3000.0),
            ]),
            tab_card("Activity", "Dynamic notifications", &[
                sw("dynamicNotifications", "Follow Windows notifications", "Reads the notifications Windows shows, including ones from websites in your browser. Windows asks for access the first time. Nothing is stored or sent anywhere.", false),
                list_of("dynamicApps", "Show live progress for", "Delivery and ride notifications from these services become a progress card on the Island. Matched by app name or notification text.", "Service", &["Just Eat", "Uber Eats", "Uber", "Deliveroo"]),
                sw("dynamicOther", "Flash other notifications on the pill", "", false),
            ]),
            Card { title: "Companion", tab: "Activity", feature: "companion", options: &[
                sw("hideWhenIdle", "Hide when nothing is happening", "", true),
                sw("commandBarInPill", "Command Bar comes out of the Island", "", false),
            ] },
            Card { title: "Watch a window", tab: "Activity", feature: "watch", options: &[
                custom("pickArea", "Watched area", "Drag a rectangle on any window."),
                num("interval", "Check every", "", 1.0, 60.0, 1.0, "s", 2.0),
                list("rules", "Notify when", "For example: changes, equals 100%, above 50, contains Done.", "Rule"),
                sw("showInPill", "Show the value in the resting pill", "", true),
            ] },
            tab_card("Behaviour", "Opening", &[
                custom("behaviour", "Opening", "Hover or click, delays and display."),
                pick("openOn", "Open", "", &[c("click", "On click only"), c("hover", "On hover")], "click"),
                pick("dock", "Position", "Bottom left sits on the taskbar and grows upward.", &[c("top", "Top centre"), c("bottomLeft", "Bottom left")], "top"),
                sw("agentsTaskbar", "AI Agents bar on the taskbar", "Keeps the AI Agents bar on its own at the taskbar's left while the Island stays at the top.", false),
                num("hoverDelayMs", "Hover delay", "", 0.0, 1500.0, 50.0, "ms", 250.0),
                num("collapseDelayMs", "Collapse delay", "", 0.0, 3000.0, 100.0, "ms", 600.0),
                text("display", "Display", "primary, pointer or a monitor name.", "primary", "primary"),
                sw("hideInFullscreen", "Hide when an app is full screen", "", true),
            ]),
            tab_card("Behaviour", "Shortcuts", &[
                custom("shortcuts", "Shortcuts", "One per module, all rebindable."),
            ]),
            Card { title: "Lock screen", tab: "Behaviour", feature: "lockScreen", options: &[
                sw("sounds", "Lock and unlock sounds", "", false),
                sw("pauseOverlays", "Pause overlays while locked", "", true),
                sw("lockedNotice", "Show a short notice before locking", "", false),
            ] },
        ],
        specs: &["apps/chat-host.md", "apps/dynamic-notifications.md","everyday/island.md", "everyday/top-pill.md", "everyday/companion.md", "everyday/watch.md", "everyday/lock-screen.md"],
        ..PAGE
    },
    super::monitor::MONITOR,
    Page {
        id: "shortcuts",
        label: "Shortcuts",
        icon: "input",
        blurb: "Every shortcut in one list. Click one and press the new keys.",
        feature: false,
        custom: true,
        cards: &[card("Shortcuts", &[custom("all", "All shortcuts", "Clashes are shown next to the shortcut.")])],
        specs: &["input/shortcuts-manager.md"],
        ..PAGE
    },
    Page {
        id: "permissions",
        label: "Permissions",
        icon: "about",
        blurb: "What Notifi needs from your system, and why.",
        feature: false,
        custom: true,
        cards: &[card("Permissions", &[
            custom("list", "Permissions", "Each permission and the features that use it."),
        ])],
        specs: &["app/onboarding-permissions.md"],
        ..PAGE
    },
];

pub const ABOUT: &[Page] = &[Page {
    id: "about",
    label: "About",
    icon: "about",
    blurb: "Version, licence and release notes.",
    feature: false,
    custom: true,
    cards: &[card("About", &[custom("info", "About Notifi", "Version, licence and the clean-room note.")])],
    ..PAGE
}];
