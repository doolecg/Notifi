use super::*;

pub const PAGES: &[Page] = &[
    Page {
        id: "clipboardHistory",
        label: "Clipboard history",
        icon: "clipboard",
        blurb: "Keep a searchable history of what you copy, pin favourites and paste any entry again. It is saved as plain text in Notifi's data folder on this computer and never sent anywhere.",
        tool: true,
        tabs: &["History", "Layout", "Privacy"],
        cards: &[
            Card { title: "Keeping", tab: "History", options: &[
                num("maxItems", "Items to keep", "Oldest unpinned items go first. 0 means no limit.", 0.0, 10000.0, 100.0, "items", 1000.0),
                num("keepDays", "Remove after", "Pinned items stay. 0 keeps items until the limit is reached.", 0.0, 365.0, 1.0, "days", 0.0),
                sw("saveImagesAndFiles", "Save images and files", "Otherwise only text is recorded.", true),
            ], ..CARD },
            Card { title: "Pasting", tab: "History", options: &[
                key("openShortcut", "Open history", "", "Ctrl+Shift+V"),
                sw("autoPaste", "Paste straight away", "Off copies the entry and leaves the pasting to you.", true),
            ], ..CARD },
            Card { title: "Window", tab: "Layout", options: &[
                tiles("layout", "Layout", "", &[ci("list", "List with preview", "panel"), ci("shelf", "Shelf of cards", "shelf")], "list"),
                sw("showTrayLatest", "Show latest copy in the tray", "Only where the tray can show text.", false),
            ], ..CARD },
            Card { title: "Privacy", tab: "Privacy", options: &[
                sw("sensitiveFilter", "Skip sensitive text", "Leaves out copies that look like passwords or keys. History is not encrypted, so keep this on.", true),
                list("ignoredApps", "Ignored apps", "Copies from these apps are never saved.", "App name"),
            ], ..CARD },
        ],
        specs: &["clipboard/clipboard-history.md", "clipboard/history-options.md"],
        ..PAGE
    },
    Page {
        id: "autoClear",
        label: "Auto clear",
        icon: "broom",
        blurb: "Empty the clipboard a little while after you copy, or when the computer locks or sleeps. Saved history is kept.",
        cards: &[card("When to clear", &[
            num("delaySeconds", "Clear after", "A newer copy restarts the timer. 0 turns the timer off.", 0.0, 600.0, 5.0, "s", 60.0),
            sw("clearOnLockSleep", "Clear on lock and sleep", "", true),
        ])],
        specs: &["clipboard/auto-clear.md"],
        ..PAGE
    },
    Page {
        id: "plainTextPaste",
        label: "Paste as plain text",
        icon: "text",
        blurb: "Paste what you copied without its formatting. Your original clipboard comes back afterwards.",
        cards: &[card("Shortcut", &[
            key("shortcut", "Paste as plain text", "Pick one that does not clash with the history shortcut.", "Ctrl+Alt+Shift+V"),
        ])],
        specs: &["clipboard/paste-plain-text.md"],
        ..PAGE
    },
    Page {
        id: "pasteImageAsFile",
        label: "Paste image as file",
        icon: "image",
        blurb: "Save an image from the clipboard as a PNG file in the folder open in your file manager.",
        cards: &[card("Shortcut", &[
            key("shortcut", "Save image here", "Works in Explorer, Dolphin and Nautilus.", "Ctrl+Alt+V"),
        ])],
        specs: &["clipboard/file-manager-shortcuts.md"],
        ..PAGE
    },
    Page {
        id: "cleanUrl",
        label: "Clean URL",
        icon: "link",
        blurb: "Remove tracking parameters from links you copy, by shortcut or automatically.",
        cards: &[
            card("Cleaning", &[
                sw("automatic", "Clean links as I copy them", "Only plain links are changed, never longer text.", false),
                key("cleanShortcut", "Clean the copied link now", "", "Ctrl+Alt+L"),
            ]),
            card("Rules", &[
                list("siteRules", "Site rules", "One per site, listing the parameters to remove.", "example.com: utm_source"),
            ]),
        ],
        specs: &["clipboard/clean-url.md"],
        ..PAGE
    },
    Page {
        id: "appImageInstaller",
        label: "AppImage installer",
        icon: "package",
        blurb: "Offer to install an AppImage you download: move it to your applications folder, make it runnable and add a menu entry.",
        platform: "linux",
        cards: &[card("Found in Downloads", &[custom("offers", "AppImages to install", "")]), card("Installing", &[
            sw("offerOnOpen", "Offer when an AppImage is opened", "", true),
            sw("rememberAnswers", "Remember my answers", "Reuse the choices about deleting the download and showing the app.", false),
        ])],
        specs: &["clipboard/appimage-installer.md"],
        ..PAGE
    },
    Page {
        id: "installerHelper",
        label: "Installer helper",
        icon: "download",
        blurb: "When an installer or portable zip lands in Downloads, offer to install it and tidy up afterwards.",
        platform: "windows",
        cards: &[
            card("Found in Downloads", &[custom("offers", "Installers to run", "")]),
            card("Installing", &[
                sw("silentInstall", "Allow silent install", "Used only for installers known to support it.", true),
                text("portableFolder", "Folder for portable apps", "Leave empty for the folder Notifi manages.", "C:\\Apps", ""),
            ]),
            card("Afterwards", &[
                pick("removeDownload", "Delete the download", "", &[c("ask", "Ask me"), c("always", "Always"), c("never", "Never")], "ask"),
                pick("revealProgram", "Show the program", "", &[c("ask", "Ask me"), c("always", "Always"), c("never", "Never")], "ask"),
            ]),
        ],
        specs: &["clipboard/installer-helper.md"],
        ..PAGE
    },
];
