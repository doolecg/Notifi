# Extra click filter

Ignore accidental rapid double clicks from a worn mouse button without delaying normal clicks.

## User-visible behaviour
- A click that arrives within the threshold after the previous one on the same button is dropped.
- Threshold adjustable in 1 ms steps.
- Normal clicks pass without delay.
- Suspended while Cleaning Mode is on.

## Settings
- Threshold in ms (default 40) (Notifi design, not from source docs)
- Per-button on/off

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Drag operations are not affected.

## Permissions
Wayland: input group.

## Platform notes
- Windows: Hook drops repeats under the threshold.
- X11: evdev grab drops repeats.
- KDE Wayland: Same.
- GNOME Wayland: Same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
