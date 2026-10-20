# Apps to leave alone

Exclude chosen apps from mouse and keyboard enhancements.

## User-visible behaviour
- One shared exclusion list plus per-feature lists.
- Checked against the foreground app before every remap.
- Programs that are not normal apps (e.g. a Java process) can be added by executable path.

## Settings
- Global list
- Per-feature lists

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Exclusions apply at the moment of the event, so switching apps takes effect immediately.

## Permissions
Same as the features that use it.

## Platform notes
- Windows: Foreground-process check.
- X11: Active window class.
- KDE Wayland: KWin script reports active app.
- GNOME Wayland: Extension reports active app.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
