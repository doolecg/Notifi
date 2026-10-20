use super::*;

pub const PAGES: &[Page] = &[
    Page {
        id: "appUpdates",
        label: "App updates",
        icon: "apps",
        blurb: "Every app with a newer version, whatever installed it, updated in one go.",
        tool: true,
        cards: &[
            card("Checking", &[
                sw("checkAtStartup", "Check at startup", "", true),
                num("checkHours", "Then every", "", 1.0, 48.0, 1.0, "hours", 6.0),
                sw("prerelease", "Include pre-release channels", "", false),
                sw("alertOnFound", "Tell me when updates are found", "", true),
            ]),
            card("Sources", &[
                sw("sourcePackageManagers", "Package managers", "winget, Scoop, Chocolatey, PackageKit, Flatpak and Snap.", true),
                sw("sourcePublisherFeeds", "Publisher feeds", "", true),
                sw("sourceCatalogue", "Public version catalogue", "Asked without sending your list of apps in the clear.", true),
            ]),
            card("Exceptions", &[
                list("exceptions", "Never check", "", "App name"),
                custom("rules", "Update rules", "Skipped versions and ignored apps, each can be undone."),
            ]),
        ],
        specs: &["apps/app-updates.md"],
        ..PAGE
    },
    Page {
        id: "packageManager",
        label: "Package manager",
        icon: "apps",
        blurb: "Search, install, update and remove apps through winget and Scoop, or PackageKit, Flatpak and Snap.",
        tool: true,
        cards: &[card("Managers", &[
            list("managers", "Managers to use", "Leave empty to use every one found.", "winget"),
            sw("groupDependencies", "Group dependencies under their package", "", true),
            num("checkHours", "Check for updates every", "", 1.0, 48.0, 1.0, "hours", 6.0),
            sw("thirdPartySources", "Show packages from third-party sources", "Buckets and remotes.", true),
        ])],
        specs: &["apps/package-manager.md"],
        ..PAGE
    },
    Page {
        id: "uninstaller",
        label: "Uninstaller",
        icon: "apps",
        blurb: "Remove apps together with the settings, caches and helpers they leave behind.",
        tool: true,
        cards: &[card("Apps", &[custom("list", "Installed apps", "")]), card("Leftovers", &[
            sw("preTickVerified", "Tick verified leftovers", "", true),
            sw("showNameMatched", "List leftovers found by name", "Shown unticked.", true),
            sw("trashLeftovers", "Move leftovers to the bin", "Instead of deleting them.", true),
            sw("stopBackground", "Stop background parts first", "Services, tray helpers and scheduled tasks.", true),
        ])],
        specs: &["apps/uninstaller.md"],
        ..PAGE
    },
    Page {
        id: "cleaner",
        label: "Cleaner",
        icon: "apps",
        blurb: "Clear caches, temp files, logs and leftovers, by hand or on a schedule.",
        tool: true,
        cards: &[
            card("Clean now", &[custom("scan", "Scan and clean", "")]),
            card("Categories", &[
                sw("caches", "Caches", "", true),
                sw("logs", "Logs", "", true),
                sw("temp", "Temp files", "", true),
                sw("recycleBin", "Recycle Bin or trash", "", false),
                sw("screenshots", "Forgotten screenshots", "", false),
                num("screenshotAge", "Screenshots older than", "", 1.0, 365.0, 1.0, "days", 30.0),
                sw("leftovers", "Leftovers of removed apps", "", false),
                sw("oldInstallers", "Old installers", "Installers in Downloads over 60 days old, found by analysis and shown unticked.", false),
            ]),
            card("Schedule", &[
                sw("autoClean", "Clean automatically", "Only ticked categories.", false),
                pick("frequency", "How often", "", &[c("daily", "Daily"), c("weekly", "Weekly")], "weekly"),
                pick("weekday", "Day", "", &[
                    c("monday", "Monday"), c("tuesday", "Tuesday"), c("wednesday", "Wednesday"), c("thursday", "Thursday"),
                    c("friday", "Friday"), c("saturday", "Saturday"), c("sunday", "Sunday"),
                ], "sunday"),
                text("time", "Time", "24-hour clock.", "03:00", "03:00"),
                sw("notifyAfter", "Notify after an automatic run", "", true),
            ]),
        ],
        specs: &["apps/cleaner.md"],
        ..PAGE
    },
    Page {
        id: "messagingDownloads",
        label: "Messaging downloads",
        icon: "download",
        blurb: "Tidy the files chat apps save, moving old ones to the bin or into dated folders. Chats are never read.",
        cards: &[
            card("Review", &[custom("review", "Review and clean", "")]),
            card("Rules", &[
                num("retentionDays", "Keep files newer than", "", 1.0, 365.0, 1.0, "days", 30.0),
                sw("keepTouched", "Keep files I opened or moved", "", false),
                num("sizeCap", "Cap per app", "0 means no cap.", 0.0, 100000.0, 100.0, "MB", 0.0),
                list("apps", "Apps", "Leave empty for every one found.", "WhatsApp"),
            ]),
            card("Action", &[
                tiles("action", "Old files", "", &[ci("trash", "Move to the bin", "eject"), ci("organise", "Sort into folders", "storage")], "trash"),
                sw("onSchedule", "Run on the cleaner's schedule", "", false),
            ]),
        ],
        specs: &["apps/messaging-downloads.md"],
        ..PAGE
    },
];
