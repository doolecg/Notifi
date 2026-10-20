# Keep awake

Stop the computer sleeping for a set time, or whenever chosen apps, mains power or an external display are in use.

## User-visible behaviour
- One click starts a session from the tray panel using duration chips (for example 15 min, 1 h, 4 h, indefinite). The switch and the shortcut restart the last choice.
- Two modes: Duration (run for a length of time from now) and Until (run until a chosen clock time, via an end-time picker that stays usable from the top pill without closing it).
- Automatic triggers keep the machine awake without a manual start: while chosen apps are running (helper processes inside another app count too), while connected to mains power, while an external display is connected. When several triggers are selected, an option decides whether any one is enough or all must hold at once.
- Pause on lock: the session can pause while the screen is locked and the rest of it resumes on unlock.
- Allow displays to sleep: keep the system running but let the screen turn off.
- Pointer nudge (optional): move the pointer by a tiny amount at a chosen interval during active sessions, for apps that count idle time.
- Time left is shown beside the tray icon and in the closed top pill; a running session restores a hidden tray icon. A notice appears when the session finishes.
- Icon choices for the active state: the Notifi icon, coffee, eye, moon or light bulb, plus a colour for the active tray icon.
- Optional extras: start a session automatically when Notifi opens, toggle with a right click on the tray icon, show remaining time as a tray option.
- Battery protection: below a chosen charge on battery power the session ends (or refuses to start), and the panel explains why instead of failing silently.

## Settings
- Last chosen duration or end time: 1 hour.
- Mode: Duration.
- Triggers (apps list, power, external display): all off; combine rule: any.
- Pause on lock: off. Let displays sleep: off.
- Pointer nudge: off, interval 60 seconds.
- Battery cutoff: on, 20 %.
- Start at launch: off. Right-click toggle: off. Icon: Notifi.

## Default shortcuts
Toggle: Ctrl+Alt+A (Notifi design, not from source docs). (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Battery protection can block a start or end a running session; the normal sleep behaviour is restored on timer end, battery cutoff, quit and uninstall.
- Rapid on/off toggling must not leave the system stuck awake; requests are applied in order.
- A manual session and trigger sessions combine: the machine stays awake until all of them have ended.
- Locking with Pause on lock stops the nudge and the timer countdown continues unless stated otherwise.
- Time zone or clock changes recompute an Until end time.

## Permissions
None.

## Platform notes
- Windows: SetThreadExecutionState (system required, optionally display required).
- X11: logind inhibitor lock (idle and sleep) plus the screensaver inhibit; pointer nudge via XTest.
- KDE Wayland: logind inhibitor plus PowerDevil/ScreenSaver inhibit over D-Bus; nudge may need a portal or be unavailable.
- GNOME Wayland: logind inhibitor plus the session idle inhibitor; nudge may need a portal or be unavailable.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
