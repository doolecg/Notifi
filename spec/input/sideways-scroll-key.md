# Scroll sideways while holding a key

Hold a chosen key and the vertical wheel scrolls horizontally.

## User-visible behaviour
- Pick a key (default Shift).
- While held, wheel movement is sent as horizontal scroll.

## Settings
- Key (default Shift)
- Enable (default off)

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Apps that already treat Shift+wheel as horizontal should not double up.

## Permissions
Wayland: input group.

## Platform notes
- Windows: Mouse hook to horizontal wheel.
- X11: uinput REL_HWHEEL.
- KDE Wayland: Same.
- GNOME Wayland: Same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md
