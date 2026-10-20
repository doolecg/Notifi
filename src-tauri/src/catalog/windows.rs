use super::*;

pub const PAGES: &[Page] = &[
    Page {
        id: "appSwitcher",
        label: "App switcher",
        icon: "window",
        blurb: "A better Alt+Tab with live previews, search and filters for apps and individual windows.",
        tabs: &["General", "Previews", "Filters"],
        cards: &[
            tab_card("General", "Shortcuts", &[
                sw("replaceSystem", "Replace the system switcher", "Off leaves the system Alt+Tab alone and uses the shortcut below instead.", true),
                key("trigger", "Open the switcher", "Hold it and tap Tab to move; release to switch.", "Alt+Tab"),
                key("sameAppKey", "Cycle windows of one app", "", "Alt+`"),
            ]),
            tab_card("General", "Behaviour", &[
                tiles("layout", "Layout", "Simple mode needs no screen capture.", &[ci("previews", "Previews", "image"), ci("icons", "Large icons", "apps"), ci("simple", "Simple list", "text")], "previews"),
                num("appearDelay", "Delay before it appears", "A quick flip goes straight to the last window.", 0.0, 1000.0, 50.0, "ms", 0.0),
                sw("skipAnimations", "Skip animations", "Selection and resize happen instantly.", false),
                sw("wheelBrowse", "Browse with the wheel or touchpad", "", true),
                sw("hoverButtons", "Close buttons on cards", "Also middle-click to close, W to close, Q to quit.", true),
            ]),
            tab_card("Previews", "Thumbnails", &[
                pick("thumbnailSize", "Thumbnail size", "", &[c("normal", "Normal"), c("large", "Large"), c("extraLarge", "Extra large"), c("minimal", "Minimal")], "normal"),
                num("maxColumns", "Maximum columns", "0 picks the number automatically.", 0.0, 12.0, 1.0, "", 0.0),
                list("pauseApps", "Pause thumbnails for", "Apps and other programs whose previews should freeze.", "App or program name"),
            ]),
            tab_card("Filters", "Which windows are listed", &[
                sw("currentDesktopOnly", "Current desktop only", "", false),
                sw("currentDisplayOnly", "Current display only", "Does not open if that display is empty.", false),
                sw("showMinimised", "Show minimised windows", "", true),
                sw("showHiddenApps", "Show apps hidden to the tray", "", true),
                sw("showOtherDesktopFullscreen", "Include fullscreen windows from other desktops", "", false),
                sw("hideFileManager", "Hide the file manager when it has no windows", "", false),
            ]),
            tab_card("Filters", "Apps and tabs", &[
                sw("showAppsWithoutWindows", "List apps without windows", "Shown after the windowed apps.", false),
                list("windowOnlyApps", "Apps to keep window-only", "These are never listed without a window.", "App name"),
                sw("mergeTabs", "Merge tabs into one entry", "For apps whose tabs are separate windows.", false),
                sw("browserTabs", "Browser tabs as entries", "Needs a browser extension or remote debugging.", false),
            ]),
        ],
        specs: &["windows/app-switcher.md", "windows/switcher-options.md"],
        ..PAGE
    },
    Page {
        id: "windowLayout",
        label: "Window layout",
        icon: "layout",
        blurb: "Arrange windows into halves, thirds and quarters with shortcuts, screen edges or modifier-drag.",
        tabs: &["Layouts", "Snapping", "Fill screen", "Desktops"],
        cards: &[
            tab_card("Layouts", "Shortcuts", &[
                key("leftHalf", "Left half", "", "Ctrl+Alt+Left"),
                key("rightHalf", "Right half", "", "Ctrl+Alt+Right"),
                key("topHalf", "Top half", "", "Ctrl+Alt+Up"),
                key("bottomHalf", "Bottom half", "", "Ctrl+Alt+Down"),
                key("topLeft", "Top left quarter", "", "Ctrl+Alt+U"),
                key("topRight", "Top right quarter", "", "Ctrl+Alt+I"),
                key("bottomLeft", "Bottom left quarter", "", "Ctrl+Alt+J"),
                key("bottomRight", "Bottom right quarter", "", "Ctrl+Alt+K"),
                key("maximize", "Maximise", "", "Ctrl+Alt+Enter"),
                key("center", "Centre", "", "Ctrl+Alt+C"),
                key("maximizeMargin", "Maximise with margin", "", ""),
                key("nextDisplay", "Move to next display", "", ""),
                key("restore", "Restore previous position", "", ""),
            ]),
            tab_card("Layouts", "Placement", &[
                sw("repeatToMove", "Repeat to move to the next display", "Pressing left or right again carries the window across.", false),
                sw("cycleWidths", "Repeat to cycle widths", "Half, third, then two-thirds.", false),
                num("marginSize", "Margin for maximise", "", 0.0, 200.0, 1.0, "px", 0.0),
                num("gap", "Gap between windows", "Also applies to screen edges.", 0.0, 100.0, 1.0, "px", 0.0),
                list("nativeFullscreenApps", "Keep native full screen in", "", "App name"),
            ]),
            tab_card("Snapping", "Screen edges", &[
                sw("snapEdges", "Snap at screen edges", "Drag a window to an edge or corner, release to place.", false),
                sw("snapPreview", "Show a preview", "", true),
                sw("topEdgeMaximize", "Top edge maximises", "", true),
                list("edgeRules", "Edge and corner placements", "One per line, for example: Left edge = Left half.", "Edge = placement"),
            ]),
            tab_card("Snapping", "Modifier-drag", &[
                sw("modifierDrag", "Move windows by dragging with a modifier", "Hold it and drag anywhere in the window; add Shift to resize.", false),
                pick("dragModifier", "Modifier", "", &[c("Alt", "Alt"), c("Ctrl", "Ctrl"), c("Super", "Super"), c("Ctrl+Alt", "Ctrl+Alt")], "Alt"),
                sw("rightButtonResize", "Right button resizes", "", false),
            ]),
            tab_card("Snapping", "Exclusions", &[
                list("ignoreApps", "Apps to leave alone", "Never moved or resized.", "App name"),
            ]),
            Card { title: "Fill screen", tab: "Fill screen", feature: "fillScreen", options: &[
                key("shortcut", "Shortcut", "Press again to put the window back.", ""),
                pick("cover", "Cover", "", &[c("workArea", "Work area"), c("display", "Whole display")], "workArea"),
                num("gap", "Gap", "", 0.0, 200.0, 1.0, "px", 0.0),
                sw("takeOverMaximize", "Take over the maximise button", "Keeps the window on its current desktop.", false),
                list("nativeMaximizeApps", "Apps to leave on native maximise", "", "App name"),
            ], ..CARD },
            Card { title: "Virtual desktops", tab: "Desktops", feature: "virtualDesktops", options: &[
                sw("followWindow", "Follow the window when moving it", "", true),
                sw("createOnDemand", "Create desktops on demand", "Using a higher number adds desktops up to it.", false),
                sw("showIndicator", "Show a desktop indicator", "", false),
                list("names", "Desktop names", "One per desktop, in order. Empty uses system names.", "Name"),
                key("previous", "Previous desktop", "", ""),
                key("next", "Next desktop", "", ""),
                key("switch1", "Switch to desktop 1", "", ""),
                key("switch2", "Switch to desktop 2", "", ""),
                key("switch3", "Switch to desktop 3", "", ""),
                key("switch4", "Switch to desktop 4", "", ""),
                key("switch5", "Switch to desktop 5", "", ""),
                key("switch6", "Switch to desktop 6", "", ""),
                key("switch7", "Switch to desktop 7", "", ""),
                key("switch8", "Switch to desktop 8", "", ""),
                key("switch9", "Switch to desktop 9", "", ""),
                key("move1", "Move window to desktop 1", "", ""),
                key("move2", "Move window to desktop 2", "", ""),
                key("move3", "Move window to desktop 3", "", ""),
                key("move4", "Move window to desktop 4", "", ""),
                key("move5", "Move window to desktop 5", "", ""),
                key("move6", "Move window to desktop 6", "", ""),
                key("move7", "Move window to desktop 7", "", ""),
                key("move8", "Move window to desktop 8", "", ""),
                key("move9", "Move window to desktop 9", "", ""),
            ], ..CARD },
        ],
        specs: &["windows/window-layout.md", "windows/layout-options.md", "windows/fill-screen.md", "windows/virtual-desktops.md"],
        ..PAGE
    },
    Page {
        id: "quitOnClose",
        label: "Quit on close",
        icon: "close",
        blurb: "Chosen apps exit completely when their last window is closed, instead of idling in the background.",
        cards: &[
            card("Apps", &[
                list("apps", "Apps to quit on close", "", "App name"),
                list("exceptions", "Exceptions", "Apps that must keep running, such as recorders and sync tools.", "App name"),
            ]),
            card("Timing", &[
                num("graceDelay", "Wait before quitting", "Gives apps time to open another window.", 0.0, 5000.0, 100.0, "ms", 800.0),
            ]),
        ],
        specs: &["windows/quit-on-close.md"],
        ..PAGE
    },
    Page {
        id: "closeProtection",
        label: "Close protection",
        icon: "shield",
        blurb: "Stop accidental quit and close shortcuts by asking for a hold, a double press or an extra key.",
        cards: &[
            card("Guarded shortcuts", &[
                sw("guardQuit", "Guard quit", "Ctrl+Q and Alt+F4.", true),
                sw("guardClose", "Guard close tab or window", "Ctrl+W.", true),
            ]),
            card("Confirming", &[
                pick("mode", "Default way to confirm", "", &[c("hold", "Hold the keys"), c("twice", "Press twice"), c("modifier", "Add a modifier")], "hold"),
                num("holdTime", "Hold time", "", 200.0, 3000.0, 100.0, "ms", 700.0),
                pick("extraModifier", "Extra modifier", "Used by the add-a-modifier way.", &[c("Shift", "Shift"), c("Ctrl", "Ctrl"), c("Alt", "Alt")], "Shift"),
            ]),
            card("Apps", &[
                list("appRules", "Per-app settings", "One per line, for example: app = twice.", "App = way to confirm"),
            ]),
        ],
        specs: &["windows/quit-protection.md"],
        ..PAGE
    },
    Page {
        id: "taskbarPreview",
        label: "Taskbar preview",
        icon: "taskbar",
        blurb: "Hover a taskbar button to see large live previews of that app's windows across every desktop.",
        platform: "linux",
        cards: &[
            card("Opening", &[
                num("openDelay", "Opening delay", "", 0.0, 2000.0, 50.0, "ms", 200.0),
                num("switchDelay", "App-switching delay", "", 0.0, 2000.0, 50.0, "ms", 100.0),
                sw("keepTaskbarVisible", "Keep an auto-hidden taskbar visible", "Experimental.", false),
            ]),
            card("Cards", &[
                pick("cardSize", "Card size", "", &[c("normal", "Normal"), c("large", "Large"), c("extraLarge", "Extra large")], "normal"),
                sw("minimal", "Minimal mode", "Smaller cards with titles only.", false),
                sw("otherDesktops", "Show windows from other desktops", "", true),
                sw("closeQuits", "Close button quits the app", "", false),
                sw("peek", "Peek on hover", "Shows the window full size before you choose it.", true),
                slider("opacity", "Panel opacity", "", 20.0, 100.0, 5.0, "%", 100.0),
            ]),
            card("Apps", &[
                list("pauseApps", "Pause thumbnails for", "", "App name"),
            ]),
        ],
        specs: &["windows/taskbar-preview.md"],
        ..PAGE
    },
    Page {
        id: "taskbarClicks",
        label: "Taskbar clicks",
        icon: "taskbar",
        blurb: "Choose what clicking the taskbar button of the app you are already using does.",
        platform: "linux",
        cards: &[
            card("Click action", &[
                tiles("action", "When clicking the active app", "With several windows, cycling is used if chosen.", &[ci("minimise", "Minimise", "window"), ci("hide", "Hide app", "eye"), ci("cycle", "Cycle windows", "refresh")], "minimise"),
            ]),
            card("Apps", &[
                list("excludedApps", "Apps to leave alone", "", "App name"),
            ]),
        ],
        specs: &["windows/taskbar-clicks.md"],
        ..PAGE
    },
];
