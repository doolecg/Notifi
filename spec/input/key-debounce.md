# Key debounce

Suppresses duplicate letters produced by a keyboard whose switches are wearing out.

## User-visible behaviour
- A press of the same key within the interval of its previous press is dropped.
- Interval adjustable.
- Auto-repeat from holding a key is not treated as bounce.
- Suspended during Cleaning Mode.

## Settings
- Interval in ms (default 30) (Notifi design, not from source docs)

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Does not affect modifier keys.

## Permissions
Wayland: input group.

## Platform notes
- Windows: Keyboard hook.
- X11: XRecord/XTest or evdev.
- KDE Wayland: evdev grab.
- GNOME Wayland: Same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
