# Shortcuts manager

One page lists every Notifi shortcut, lets you rebind them, and warns when they clash with others.

## User-visible behaviour
- Table of features and shortcuts with a capture widget.
- Shows clashes with other Notifi features, radial menu profiles and known OS shortcuts.
- Optional takeover of an OS shortcut while a feature is active.
- Numeric keypad digits work wherever number shortcuts exist.

## Settings
- Per-shortcut binding
- Reset to default

## Default shortcuts
See each feature page. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Radial menu profiles refuse a combination already used.
- Reserved system combinations are warned about.

## Permissions
Global hotkeys per platform.

## Platform notes
- Windows: RegisterHotKey.
- X11: X11 key grab.
- KDE Wayland: GlobalShortcuts portal.
- GNOME Wayland: GlobalShortcuts portal (GNOME 48+).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
