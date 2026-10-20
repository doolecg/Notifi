# Virtual desktops (fixed order, switch and move by number)

Adapts the Mac "fixed Space order". Windows 10/11 and most Linux desktops do not reorder desktops by recent use, so the Mac pain point is gone. What Notifi adds is the useful remainder: a stable numbered set of virtual desktops with direct shortcuts, names and an indicator.

## User-visible behaviour
- One shortcut per desktop number (1 to 9) to switch to it, and a second family to move the active window to desktop N (optionally following it).
- Previous and next desktop shortcuts.
- Optional desktop names kept by Notifi, shown in the indicator and in the app switcher.
- Optional small desktop indicator (tray readout or Top Pill) showing the current number or name.
- "Create on demand": using a number higher than the existing count adds desktops up to it.
- "Stay put": Notifi never reorders. If the desktop environment has an option that reorders by recent use (GNOME dynamic workspaces, some tiling setups), the page explains it and links to that setting.
- Turning the feature off restores any desktop setting Notifi changed.

## Settings
- Enable (default off)
- Shortcuts: switch to 1 to 9, move window to 1 to 9, previous, next (default none)
- Follow the window when moving (default on)
- Desktop names (default empty = OS names)
- Show indicator (default off)
- Create desktops on demand (default off)

## Default shortcuts
None by default. Suggested: Ctrl+Alt+1..9 to switch, Ctrl+Alt+Shift+1..9 to move.

## Edge cases
- Moving a window to a desktop that does not exist creates it only if "create on demand" is on; otherwise nothing happens and a toast says why.
- Pinned (all-desktops) windows are not moved.
- The OS shortcuts (Win+Ctrl+Left/Right) keep working; Notifi adds only the numbered ones and warns about clashes in the shortcuts manager.
- Wayland does not let apps list desktops without the script or extension.

## Permissions
Windows: none. Linux Wayland: KWin script or GNOME extension.

## Platform notes
- Windows: the virtual desktop COM interfaces (undocumented, with a table keyed by Windows build; falls back to simulating Win+Ctrl+Arrow for plain switching).
- X11: EWMH current-desktop and window-desktop messages.
- KDE Wayland: KWin D-Bus (desktop list, current desktop, window desktop).
- GNOME Wayland: extension over D-Bus; static workspaces recommended.
- Status: Windows ADAPT, Linux ADAPT.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
