use super::*;

const EXCLUDED: Opt = list("excludedApps", "Apps to leave alone", "Also uses the shared list.", "App name or program path");

pub const PAGES: &[Page] = &[
    Page {
        id: "textSnippets",
        label: "Text snippets",
        icon: "notes",
        blurb: "Type a short trigger and get the whole phrase, or pick a snippet from a searchable menu. To spot triggers Notifi watches the keys you type: only the last 64 characters are held in memory, password fields are skipped, and nothing is saved or sent.",
        tool: true,
        cards: &[
            card("Snippets", &[
                custom("editor", "Snippets", "Add, edit, group and search your snippets."),
                key("menuShortcut", "Open the snippet menu", "Enter or 1 to 9 inserts a snippet.", "Ctrl+Alt+S"),
            ]),
            card("Typing", &[
                text("prefix", "Trigger prefix", "Only words that start with it expand.", ";", ""),
                sw("caseSensitive", "Match case", "", false),
                tiles("insertMethod", "Insert by", "", &[ci("paste", "Pasting", "clipboard"), ci("type", "Typing", "input")], "paste"),
                EXCLUDED,
            ]),
        ],
        specs: &["input/text-snippets.md"],
        ..PAGE
    },
    Page {
        id: "smoothScrolling",
        label: "Smooth scrolling",
        icon: "mouse",
        blurb: "Wheel notches become short gliding scrolls instead of jumps. Touchpads are left alone.",
        cards: &[card("Feel", &[
            pick("speed", "Speed", "", &[c("slow", "Slow"), c("medium", "Medium"), c("fast", "Fast")], "medium"),
            slider("response", "Response", "How quickly a scroll starts.", 0.0, 100.0, 5.0, "%", 50.0),
            pick("coast", "Coast", "How far it keeps gliding.", &[c("off", "Off"), c("low", "Low"), c("medium", "Medium"), c("high", "High")], "low"),
            EXCLUDED,
        ])],
        specs: &["input/smooth-scrolling.md"],
        ..PAGE
    },
    Page {
        id: "linearScrolling",
        label: "Linear scrolling",
        icon: "mouse",
        blurb: "Removes wheel acceleration, so one notch always scrolls the same distance.",
        cards: &[card("Steps", &[
            num("linesPerStep", "Lines per notch", "", 1.0, 20.0, 1.0, "lines", 3.0),
            EXCLUDED,
        ])],
        specs: &["input/linear-scrolling.md"],
        ..PAGE
    },
    Page {
        id: "pointerAcceleration",
        label: "Pointer acceleration",
        icon: "mouse",
        blurb: "Turn pointer speed-up off for a flat response. Your previous setting comes back when this is off or Notifi quits.",
        specs: &["input/pointer-acceleration.md"],
        ..PAGE
    },
    Page {
        id: "focusFollowsMouse",
        label: "Focus follows mouse",
        icon: "mouse",
        blurb: "The window under the pointer gets the keyboard after a short pause.",
        cards: &[card("Focus", &[
            num("delay", "Delay", "", 100.0, 3000.0, 50.0, "ms", 300.0),
            sw("raise", "Bring the window to the front", "Off focuses it without raising it.", false),
        ])],
        specs: &["input/focus-follows-mouse.md"],
        ..PAGE
    },
    Page {
        id: "scrollDirection",
        label: "Scroll direction",
        icon: "mouse",
        blurb: "Flip the mouse wheel without changing the touchpad.",
        cards: &[card("Direction", &[
            sw("invertVertical", "Invert vertical", "", false),
            sw("invertHorizontal", "Invert horizontal", "", false),
            EXCLUDED,
        ])],
        specs: &["input/scroll-direction.md"],
        ..PAGE
    },
    Page {
        id: "sidewaysScroll",
        label: "Sideways scrolling",
        icon: "mouse",
        blurb: "Hold a key to make the wheel scroll across instead of up and down.",
        cards: &[card("Key", &[
            pick("key", "Key to hold", "", &[c("Shift", "Shift"), c("Ctrl", "Ctrl"), c("Alt", "Alt"), c("Super", "Super")], "Shift"),
        ])],
        specs: &["input/sideways-scroll-key.md"],
        ..PAGE
    },
    Page {
        id: "mouseButtons",
        label: "Mouse buttons",
        icon: "mouse",
        blurb: "Give extra buttons and drag gestures their own keys or actions, for every app or just some.",
        cards: &[
            card("Buttons", &[
                list("buttonActions", "Button actions", "One per line, for example: Button 4 = Alt+Left.", "Button = keys or action"),
            ]),
            card("Drag gestures", &[
                pick("gestureButton", "Hold", "", &[c("right", "Right button"), c("middle", "Middle button"), c("button4", "Button 4"), c("button5", "Button 5")], "right"),
                list("gestures", "Gestures", "For example: Left = Previous desktop.", "Direction = action"),
                num("gestureThreshold", "Drag distance", "", 10.0, 300.0, 5.0, "px", 60.0),
            ]),
            card("Apps", &[
                list("appOverrides", "Per-app buttons", "For example: firefox: Button 4 = Ctrl+W.", "App: button = action"),
                EXCLUDED,
            ]),
        ],
        specs: &["input/button-shortcuts-gestures.md", "input/side-buttons.md"],
        ..PAGE
    },
    Page {
        id: "touchpadMiddleClick",
        label: "Touchpad middle click",
        icon: "mouse",
        blurb: "Tap with several fingers to middle click. On Windows this opens the touchpad settings where the gesture lives.",
        cards: &[card("Tap", &[
            tiles("fingers", "Fingers", "Linux only.", &[ci("three", "Three", "mouse"), ci("four", "Four", "mouse")], "three"),
            custom("systemSettings", "Touchpad settings", "Open the system page."),
        ])],
        specs: &["input/middle-click-touchpad.md"],
        ..PAGE
    },
    Page {
        id: "appExclusions",
        label: "Apps to leave alone",
        icon: "apps",
        blurb: "Input changes are switched off while one of these apps is in front.",
        feature: false,
        cards: &[card("Apps", &[
            list("apps", "Shared list", "Programs that are not normal apps can be added by path.", "App name or program path"),
        ])],
        specs: &["input/apps-to-leave-alone.md"],
        ..PAGE
    },
    Page {
        id: "clickFilter",
        label: "Click filter",
        icon: "mouse",
        blurb: "Drops a click that comes too soon after the last one on the same button, for worn mouse switches.",
        cards: &[
            card("Filter", &[
                num("threshold", "Ignore clicks within", "", 1.0, 200.0, 1.0, "ms", 40.0),
            ]),
            card("Buttons", &[
                sw("left", "Left", "", true),
                sw("right", "Right", "", true),
                sw("middle", "Middle", "", true),
                sw("back", "Back", "", true),
                sw("forward", "Forward", "", true),
            ]),
        ],
        specs: &["input/click-filter.md"],
        ..PAGE
    },
    Page {
        id: "keyDebounce",
        label: "Key debounce",
        icon: "input",
        blurb: "Drops a second press of the same key that comes too soon, for keyboards that double type.",
        cards: &[card("Debounce", &[
            num("interval", "Ignore presses within", "Holding a key to repeat is not affected.", 5.0, 200.0, 1.0, "ms", 30.0),
        ])],
        specs: &["input/key-debounce.md"],
        ..PAGE
    },
    Page {
        id: "superKey",
        label: "Super key",
        icon: "input",
        blurb: "Turn Caps Lock or a right-hand modifier into a combination of modifiers, with its own action when tapped.",
        cards: &[
            card("Key", &[
                pick("sourceKey", "Key", "", &[c("capsLock", "Caps Lock"), c("rightAlt", "Right Alt"), c("rightCtrl", "Right Ctrl"), c("rightShift", "Right Shift")], "capsLock"),
                text("heldCombo", "When held", "", "Ctrl+Alt+Shift+Super", "Ctrl+Alt+Shift+Super"),
            ]),
            card("Tap", &[
                tiles("tapAction", "When tapped alone", "", &[ci("key", "Send a key", "input"), ci("language", "Switch language", "globe"), ci("nothing", "Nothing", "close")], "key"),
                text("tapKey", "Key to send", "", "Escape", "Escape"),
                num("tapTimeout", "Tap if released within", "", 50.0, 1000.0, 10.0, "ms", 200.0),
            ]),
            card("Apps", &[EXCLUDED]),
        ],
        specs: &["input/super-key.md"],
        ..PAGE
    },
];
