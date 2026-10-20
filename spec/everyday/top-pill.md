# Top Pill (adapted from the island)

An always-on-top bar at the top centre of the screen showing music, notifications, calendar, timers, downloads and quick controls. This file is only the overview; the full behaviour (sections, layouts, opening, activity strips, lock screen) is in island.md.

## User-visible behaviour
- Rests as a small pill, opens by click or hover into pages, and can be pinned. Section list and order are editable with a live preview.
- Hosts these tools: music (seek, lyrics, queue), timer/focus/stopwatch, calendar with countdowns, notifications, downloads, Shelf, clipboard, scratchpad (see scratchpad.md), volume mixer, camera preview (see camera-preview.md), capture controls and previews (see capture-selector.md), AI agents (see ai-agents.md), fan card (see fan-control.md) and the Command Bar (see command-bar.md).
- A hidden pill can still show activity, indicators and notifications, then hide again.
- Compact activity can sit over crowded tray/menu areas when enabled; a notch-style cutout is simulated only on displays that have none and is optional.
- Other features' dialogs appear above the pill without changing its size.
- Dragging images or a single video onto the pill offers an optimise-media choice that opens the media tools to pick how to save a smaller copy (see capture/media-tools.md).
- Optional: suppress the operating system's own notification banner for an app while the pill is showing that notification itself, so it is not shown twice (Windows and Linux: only where the OS lets a listener dismiss the banner, otherwise the option is hidden).
- The running timer countdown can be hidden from the closed pill, and each kind of content (recording controls, file tray and similar) can be set to show in the closed pill or only once the pill is expanded.

## Settings
Defined in island.md. Key defaults: opening by click, compact start, section list common set, show in screenshots off.

## Default shortcuts
Open: Ctrl+Alt+P (Notifi design, not from source docs). Others per feature. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Stays out of tiling window managers and full-screen games unless asked.
- With "hide in full screen" on, a clickable stub remains.
- Multi-monitor: pill belongs to one chosen display.

## Permissions
See island.md (calendar, notifications, media session).

## Platform notes
- Windows: topmost tool window centred on the chosen monitor; works with the taskbar at top or bottom.
- X11: dock-type window with _NET_WM_STRUT optional; MPRIS for music.
- KDE Wayland: layer-shell overlay; MPRIS.
- GNOME Wayland: extension needed to overlap the top panel and stay above windows.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md, app UI strings (release build 3.4.1)
