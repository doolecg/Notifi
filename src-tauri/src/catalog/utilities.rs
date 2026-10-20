use super::*;

pub const PAGES: &[Page] = &[
    Page {
        id: "commandBar",
        label: "Command Bar",
        icon: "search",
        blurb: "One search field for apps, windows, files, clipboard, snippets, menu commands, sums, conversions, colours, emoji and your own scripts.",
        tool: true,
        cards: &[
            card("Opening", &[
                key("shortcut", "Shortcut to open", "", "Alt+Space"),
                sw("compact", "Compact mode", "Open as a lone field and show results only once you type.", false),
                sw("noSuggestions", "Open without suggestions", "Nothing is listed until you type.", false),
                pick("position", "Position", "Where the bar appears.", &[c("center", "Centred"), c("edge", "Near a screen edge"), c("remember", "Where you left it")], "center"),
            ]),
            card("Sources to search", &[
                sw("searchApps", "Apps", "", true),
                sw("searchWindows", "Windows", "", true),
                sw("searchFiles", "Files", "", true),
                sw("searchClipboard", "Clipboard history", "", true),
                sw("searchSnippets", "Snippets", "", true),
                sw("searchMenus", "App menu commands", "", true),
                sw("searchSystem", "System actions and features", "", true),
                sw("searchEmoji", "Emoji", "", true),
                sw("calculator", "Calculator and unit conversion", "", true),
                sw("colors", "Colours", "Preview and convert a colour typed on its own.", true),
                list("fileFolders", "File search folders", "Folders the file search looks in.", "C:\\Users\\you\\Documents"),
            ]),
            card("Scripts", &[
                text("scriptsFolder", "Scripts folder", "Shell, PowerShell or any executable in here shows up as a row.", "Choose a folder", ""),
                list("scriptShortcuts", "Script shortcuts", "One per line: script name, then its shortcut.", "backup = Ctrl+Alt+B"),
                list("scriptBareNames", "Scripts that run on their bare name", "Names of scripts that run with no argument.", "backup"),
            ]),
            card("Per-app shortcuts", &[
                list("appShortcuts", "App shortcuts", "Up to 64, one per app or command. Pressing one while the app is in front hides it.", "Firefox = Ctrl+Alt+F"),
                list("aliases", "Aliases", "Your own short names for apps and commands.", "ff = Firefox"),
                list("pins", "Pinned entries", "Always listed first.", "Firefox"),
                list("hidden", "Hidden entries", "Never listed in results.", "Character Map"),
            ]),
        ],
        specs: &["everyday/command-bar.md"],
        ..PAGE
    },
    Page {
        id: "quickPanel",
        label: "Quick panel",
        icon: "panel",
        blurb: "A floating palette of your favourite tools that opens near the pointer, so small jobs run without leaving the current app.",
        tool: true,
        cards: &[
            card("Panel", &[
                key("shortcut", "Shortcut to open", "", "Ctrl+Alt+V"),
                list("tools", "Tools shown", "Top to bottom. Leave empty to show them all in the usual order.", "Timer"),
                sw("closeAfterRun", "Close after running", "", true),
            ]),
            card("Look", &[
                pick("position", "Open", "", &[c("pointer", "Under the pointer"), c("center", "Centred")], "pointer"),
                pick("size", "Panel size", "", &[c("small", "Small"), c("medium", "Medium"), c("large", "Large")], "medium"),
            ]),
        ],
        specs: &["everyday/quick-panel.md"],
        ..PAGE
    },
    Page {
        id: "quickToggles",
        label: "Quick toggles",
        icon: "toggle",
        blurb: "One-tap switches for common chores: dark mode, desktop icons, ejecting disks, emptying the bin, locking the screen and more.",
        cards: &[
            card("Toggles", &[
                list("shown", "Toggles shown", "Which ones appear, in order. Leave empty for the common set.", "Dark mode"),
                list("ejectExceptions", "Drives to leave mounted", "Eject all skips these, so backups stay connected.", "Backup"),
            ]),
            card("Behaviour", &[
                sw("confirmEmptyTrash", "Confirm before emptying the bin", "", true),
                pick("awakeDuration", "Keep awake duration", "Used by the keep awake toggle.", &[c("15", "15 minutes"), c("60", "1 hour"), c("120", "2 hours"), c("0", "Until turned off")], "60"),
            ]),
        ],
        specs: &["everyday/quick-toggles.md"],
        ..PAGE
    },
    Page {
        id: "cameraPreview",
        label: "Camera preview",
        icon: "camera",
        blurb: "See yourself in a floating mirror without starting a call app. Nothing is recorded or sent anywhere.",
        tool: true,
        cards: &[
            card("Camera", &[
                key("shortcut", "Shortcut", "", ""),
                text("camera", "Preferred camera", "Leave empty to use the system default.", "System default", ""),
                sw("mirror", "Mirror the image", "", true),
            ]),
            card("Window", &[
                pick("size", "Window size", "", &[c("small", "Small"), c("medium", "Medium"), c("large", "Large")], "small"),
                sw("rememberPosition", "Remember window position", "", true),
            ]),
        ],
        specs: &["capture/camera-preview.md"],
        ..PAGE
    },
    Page {
        id: "scratchpad",
        label: "Scratchpad",
        icon: "notes",
        blurb: "Notes that save themselves, in tabs, with Markdown preview and export, in a floating window.",
        tool: true,
        cards: &[
            card("Writing", &[
                key("shortcut", "Shortcut to open and close", "", "Ctrl+Alt+Shift+N"),
                num("textSize", "Text size", "", 8.0, 32.0, 1.0, "pt", 14.0),
                sw("preview", "Show Markdown preview", "", false),
                sw("spellCheck", "Spell check", "", true),
            ]),
            card("Window", &[
                slider("opacity", "Background opacity", "", 30.0, 100.0, 5.0, "%", 90.0),
                sw("closeOnOutsideClick", "Close when you click outside", "", true),
                sw("keepPinned", "Keep the pinned note open", "", false),
            ]),
            card("Storage", &[
                text("folder", "Storage folder", "Leave empty to keep notes in the app data folder.", "App data folder", ""),
            ]),
        ],
        specs: &["everyday/scratchpad.md"],
        ..PAGE
    },
    Page {
        id: "cleaningMode",
        label: "Cleaning Mode",
        icon: "broom",
        blurb: "Lock the keyboard and pointer while you wipe them or the screen, with a black screen or a small corner marker.",
        tool: true,
        cards: &[
            card("Mode", &[
                key("shortcut", "Shortcut to start", "", "Ctrl+Alt+Shift+K"),
                tiles("displayMode", "Display", "", &[ci("blackout", "Black out", "moon"), ci("indicator", "Corner marker", "eye")], "blackout"),
                sw("allowPointer", "Allow pointer movement", "Only in corner marker mode.", false),
                sw("showTimer", "Show elapsed time", "", false),
            ]),
            card("Unlock", &[
                num("unlockPresses", "Escape presses to unlock", "", 3.0, 10.0, 1.0, "", 5.0),
                num("unlockWindow", "Time allowed for the presses", "", 2.0, 20.0, 1.0, "s", 6.0),
            ]),
        ],
        specs: &["everyday/cleaning-mode.md"],
        ..PAGE
    },
    Page {
        id: "screenshot",
        label: "Screenshot",
        icon: "image",
        blurb: "Capture an area, window or screen, mark it up in the editor, stitch long pages together and share it.",
        tool: true,
        tabs: &["Capture", "Editor", "Scrolling", "Sharing"],
        cards: &[
            tab_card("Capture", "Selector", &[
                key("shortcut", "Screenshot shortcut", "", "PrintScreen"),
                pick("defaultMode", "Default mode", "", &[c("screenshot", "Screenshot"), c("recording", "Recording"), c("text", "Copy text"), c("color", "Colour picker")], "screenshot"),
                sw("showModeMenu", "Show the mode menu", "", true),
                sw("freezeScreen", "Freeze the screen while selecting", "", true),
                sw("showLastOutline", "Show the last capture outline", "", true),
            ]),
            tab_card("Capture", "Magnifier", &[
                sw("magnifier", "Magnifier on by default", "", true),
                slider("magnification", "Magnification", "", 2.0, 16.0, 1.0, "x", 8.0),
            ]),
            tab_card("Capture", "After a capture", &[
                sw("autoCopy", "Copy to clipboard", "", true),
                sw("autoShelf", "Add to the Shelf", "", false),
                sw("autoEditor", "Open in the editor", "", false),
                sw("autoPin", "Pin it to the screen", "Shows the capture as a small window that stays on top.", false),
                pick("previewDuration", "Preview", "", &[c("0", "Hidden"), c("1", "1 second"), c("3", "3 seconds"), c("5", "5 seconds"), c("10", "10 seconds"), c("-1", "Until dismissed")], "5"),
                sw("saveAt1x", "Save at 1x size on high-density displays", "Smaller files; the default keeps full pixels.", false),
                sw("includePillInShots", "Include the Top Pill in shots", "Only when the pill is set to show in screenshots.", false),
            ]),
            tab_card("Editor", "Saving", &[
                text("folder", "Folder", "Leave empty for the Pictures folder.", "Pictures", ""),
                text("nameTemplate", "File name", "", "Screenshot {date} {time}", "Screenshot {date} {time}"),
                pick("format", "Format", "", &[c("png", "PNG"), c("jpeg", "JPEG"), c("webp", "WebP")], "png"),
                slider("quality", "Quality", "For JPEG and WebP.", 10.0, 100.0, 5.0, "%", 90.0),
            ]),
            tab_card("Editor", "Tools", &[
                num("textSize", "Text size", "", 8.0, 96.0, 1.0, "px", 24.0),
                slider("blurStrength", "Blur strength", "", 1.0, 50.0, 1.0, "", 12.0),
                text("watermark", "Watermark text", "Added on export. Leave empty for none.", "Your name", ""),
            ]),
            tab_card("Scrolling", "Scrolling capture", &[
                pick("scrollMode", "Scrolling", "", &[c("auto", "Automatic"), c("manual", "Manual")], "auto"),
                pick("scrollSpeed", "Scroll speed", "", &[c("slow", "Slow"), c("medium", "Medium"), c("fast", "Fast")], "medium"),
                num("maxHeight", "Maximum height", "It stops with a notice when it gets this tall.", 2000.0, 100000.0, 1000.0, "px", 30000.0),
            ]),
            Card { title: "Temporary links", tab: "Sharing", feature: "shareLinks", options: &[
                text("server", "Server address", "Your own server that hosts the files.", "https://share.example.com", ""),
                text("token", "Access token", "Kept in Notifi's settings on this computer and only sent to your server.", "Optional", ""),
                pick("expiry", "Default expiry", "", &[c("1", "1 hour"), c("6", "6 hours"), c("24", "24 hours")], "1"),
                num("recordingLimit", "Compress recordings under", "", 10.0, 1000.0, 10.0, "MB", 100.0),
                key("uploadShortcut", "Shortcut for the latest screenshot", "Uploads it and copies the link.", ""),
            ], ..CARD },
            Card { title: "Recent captures", tab: "Sharing", feature: "recentCaptures", options: &[
                key("shortcut", "Shortcut to open", "", ""),
                num("keep", "Keep the last", "", 5.0, 500.0, 5.0, "captures", 50.0),
                custom("list", "Recent captures", "The captures kept so far."),
            ], ..CARD },
        ],
        specs: &[
            "capture/capture-selector.md",
            "capture/screenshot-editor.md",
            "capture/scrolling-capture.md",
            "capture/share-links.md",
            "capture/recent-captures.md",
        ],
        ..PAGE
    },
    Page {
        id: "screenRecording",
        label: "Screen recording",
        icon: "video",
        blurb: "Record an area, window or display with sound, then trim, zoom and export from the editor.",
        tool: true,
        cards: &[
            card("Recording", &[
                key("shortcut", "Start and stop shortcut", "", "Ctrl+Alt+Shift+R"),
                key("pauseShortcut", "Pause shortcut", "", "Ctrl+Alt+Shift+P"),
                pick("source", "Default source", "", &[c("area", "Area"), c("window", "Window"), c("display", "Whole display")], "area"),
                pick("frameRate", "Frame rate", "", &[c("30", "30 fps"), c("60", "60 fps")], "30"),
                pick("quality", "Quality", "", &[c("balanced", "Balanced"), c("small", "Smaller files"), c("high", "Highest")], "balanced"),
                num("countdown", "Countdown before start", "", 0.0, 10.0, 1.0, "s", 3.0),
            ]),
            card("Sound", &[
                sw("systemAudio", "Record system sound", "", true),
                sw("microphone", "Record the microphone", "", false),
                text("microphoneDevice", "Microphone", "Leave empty to use the default.", "Default microphone", ""),
            ]),
            card("Effects and saving", &[
                sw("cursorEffects", "Cursor highlight and click rings", "", true),
                sw("typingZoom", "Zoom while typing", "", true),
                text("folder", "Save folder", "Leave empty for the Videos folder.", "Videos", ""),
                text("nameTemplate", "File name", "", "Recording {date} {time}", "Recording {date} {time}"),
                pick("format", "Export format", "", &[c("mp4", "MP4"), c("webm", "WebM"), c("gif", "GIF")], "mp4"),
            ]),
        ],
        specs: &["capture/screen-recording.md"],
        ..PAGE
    },
    Page {
        id: "textFromScreen",
        label: "Copy text from screen",
        icon: "text",
        blurb: "Drag over any text or QR code on screen and copy it. Recognition happens on this computer.",
        tool: true,
        cards: &[
            card("Recognition", &[
                key("shortcut", "Shortcut", "", "Ctrl+Alt+Shift+T"),
                list("languages", "Languages", "Leave empty to use the system languages.", "English"),
                sw("joinLines", "Join lines into a paragraph", "", false),
                sw("keepLineBreaks", "Keep line breaks", "Used when joining is off.", true),
            ]),
            card("Results", &[
                sw("showToast", "Show a confirmation", "", true),
                sw("openQrLinks", "Open QR links automatically", "It still asks first.", false),
            ]),
        ],
        specs: &["capture/text-and-qr.md"],
        ..PAGE
    },
    Page {
        id: "colorPicker",
        label: "Color picker",
        icon: "palette",
        blurb: "Pick the exact colour of any pixel on screen and copy it in the format you need.",
        tool: true,
        cards: &[
            card("Picking", &[
                key("shortcut", "Shortcut", "", "Ctrl+Alt+Shift+C"),
                pick("format", "Default format", "", &[c("hex", "HEX"), c("rgb", "RGB"), c("hsl", "HSL"), c("css", "CSS"), c("qt", "Qt QColor"), c("xaml", "XAML brush")], "hex"),
                sw("hexNoHash", "HEX without the #", "", false),
                sw("hexUppercase", "Uppercase HEX", "", true),
                sw("closeAfterCopy", "Close after copying", "", true),
            ]),
            card("History", &[
                sw("keepHistory", "Keep recent colours", "", true),
                num("historySize", "Colours to keep", "", 5.0, 100.0, 5.0, "", 20.0),
            ]),
        ],
        specs: &["capture/color-picker.md"],
        ..PAGE
    },
    Page {
        id: "radialMenu",
        label: "Radial menu",
        icon: "radial",
        blurb: "A wheel of actions that appears under the pointer. Build as many wheels as you like, each with its own trigger.",
        tool: true,
        cards: &[
            card("Wheels", &[
                custom("profiles", "Wheels", "Add, edit and delete wheels and their slices."),
                key("shortcut", "Shortcut for the starter wheel", "", "Ctrl+Alt+R"),
                pick("mouseButton", "Mouse button trigger", "An extra button that opens the starter wheel.", &[c("none", "None"), c("back", "Back"), c("forward", "Forward")], "none"),
                pick("openMode", "Opens", "", &[c("hold", "While held"), c("tap", "On tap")], "hold"),
            ]),
            card("Look", &[
                num("slices", "Slices per ring", "", 4.0, 12.0, 1.0, "", 8.0),
                pick("theme", "Colour", "", &[c("accent", "System accent"), c("dark", "Dark"), c("light", "Light")], "accent"),
                num("deadZone", "Dead zone radius", "", 0.0, 100.0, 5.0, "px", 20.0),
                sw("showLabels", "Show labels", "", true),
                sw("animations", "Animations", "Follows the system reduce motion setting.", true),
            ]),
        ],
        specs: &["everyday/radial-menu.md"],
        ..PAGE
    },
    Page {
        id: "mediaTools",
        label: "Media tools",
        icon: "film",
        blurb: "Compress, trim, convert and resize videos and images by dropping them in. Everything runs on this computer.",
        tool: true,
        cards: &[
            card("Output", &[
                key("shortcut", "Shortcut to open Media tools", "", ""),
                text("folder", "Output folder", "Leave empty to save next to the originals.", "Next to the source", ""),
                sw("subfolder", "Put batches in a subfolder", "", true),
                text("nameTemplate", "File name", "Placeholders: {name} {index} {date} {time} {width} {height} {format}.", "{name}", "{name}-converted"),
                pick("overwrite", "If the file exists", "", &[c("never", "Never overwrite"), c("ask", "Ask"), c("replace", "Replace")], "never"),
                sw("keepModifiedDate", "Keep the original modified date", "", false),
                sw("keepMetadata", "Keep metadata", "", false),
            ]),
            card("Images", &[
                pick("imageFormat", "Default format", "", &[c("png", "PNG"), c("jpeg", "JPEG"), c("webp", "WebP"), c("avif", "AVIF")], "webp"),
                slider("imageQuality", "Quality", "", 10.0, 100.0, 5.0, "%", 85.0),
                num("maxSide", "Longest side", "0 keeps the original size.", 0.0, 20000.0, 100.0, "px", 0.0),
            ]),
            card("Video and GIF", &[
                pick("videoPreset", "Video preset", "", &[c("balanced", "Balanced"), c("small", "Smaller files"), c("high", "Highest quality")], "balanced"),
                num("targetSize", "Target size", "0 means no target.", 0.0, 2000.0, 5.0, "MB", 0.0),
                sw("gifLoop", "Loop GIFs forever", "", true),
                num("gifFrameRate", "GIF smoothness", "", 5.0, 30.0, 1.0, "fps", 15.0),
                num("gifWidth", "GIF width", "", 100.0, 1920.0, 10.0, "px", 640.0),
            ]),
        ],
        specs: &["capture/media-tools.md"],
        ..PAGE
    },
    Page {
        id: "shelf",
        label: "Shelf",
        icon: "shelf",
        blurb: "A floating tray at the screen edge that holds files and captures while you drag them between apps.",
        tool: true,
        cards: &[
            card("Shelf", &[
                key("shortcut", "Shortcut to show and hide", "", "Ctrl+Alt+H"),
                pick("edge", "Screen edge", "", &[c("left", "Left"), c("right", "Right"), c("top", "Top"), c("bottom", "Bottom")], "right"),
                sw("autoAddCaptures", "Add captures automatically", "", false),
                sw("clearOnQuit", "Clear when Notifi quits", "", false),
            ]),
            card("Drag gestures", &[
                sw("shake", "Shake to summon", "Shake a dragged item to call the shelf up beside the pointer.", true),
                sw("edgeDrop", "Open when a drag nears the edge", "", true),
                list("excludedApps", "Apps that never trigger it", "Shake and the edge zone are ignored for drags started in these.", "photoshop.exe"),
            ]),
        ],
        specs: &["clipboard/shelf.md"],
        ..PAGE
    },
    Page {
        id: "aiAgents",
        label: "AI agents",
        icon: "bot",
        blurb: "See what your coding agents are doing, their usage and plan limits. It reads local logs only and sends nothing anywhere.",
        tool: true,
        cards: &[
            card("Agents", &[
                list("agents", "Agents to follow", "Leave empty to follow the ones found on this computer.", "Claude Code"),
                list("logFolders", "Log folders", "Leave empty to use each tool's usual folder.", "C:\\Users\\you\\.claude"),
                pick("limitShown", "Limit to show", "On the closed pill.", &[c("closest", "Closest to running out"), c("session", "Session"), c("week", "Week")], "closest"),
            ]),
            card("Cards", &[
                sw("showLimits", "Plan limits", "", true),
                sw("showResets", "Renewal times", "", true),
                sw("showModels", "Models", "", true),
                sw("showProjects", "Projects", "", true),
                sw("showActivity", "Activity map", "", true),
            ]),
            card("Alerts", &[
                num("finishedAfter", "Tell me when a task takes longer than", "", 1.0, 60.0, 1.0, "min", 2.0),
                slider("limitWarning", "Warn when a limit reaches", "", 50.0, 100.0, 5.0, "%", 80.0),
                num("dailyBudget", "Daily budget", "In API value. 0 turns it off.", 0.0, 1000.0, 1.0, "$", 0.0),
            ]),
        ],
        specs: &["everyday/ai-agents.md"],
        ..PAGE
    },
];
