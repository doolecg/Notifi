# Scroll direction (mouse only)

Invert vertical and horizontal mouse wheel scrolling independently of the touchpad.

## User-visible behaviour
- Two switches: vertical and horizontal.
- Applies to mice only; touchpad direction is unchanged.

## Settings
- Invert vertical (default off)
- Invert horizontal (default off)
- Apps to leave alone

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- A mouse and touchpad in use together keep separate directions.

## Permissions
Wayland: input group (or libinput setting).

## Platform notes
- Windows: Mouse hook inverts the wheel.
- X11: libinput natural scroll per device.
- KDE Wayland: Same.
- GNOME Wayland: Same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
