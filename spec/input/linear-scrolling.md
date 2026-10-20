# Linear scrolling

A wheel notch always moves the page by the same distance, whatever the spin speed.

## User-visible behaviour
- Removes wheel acceleration so one notch equals a fixed distance.
- Lines per step setting.
- Separate list of apps to leave alone.

## Settings
- Lines per step (default 3)
- Apps to leave alone

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Off by default; the user installs it deliberately.

## Permissions
As smooth scrolling.

## Platform notes
- Windows: Mouse hook.
- X11: Same.
- KDE Wayland: evdev + uinput.
- GNOME Wayland: Same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
