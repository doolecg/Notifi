# Smooth scrolling

Makes a mouse wheel scroll with a fluid glide; speed, response and coasting are adjustable.

## User-visible behaviour
- Wheel notches become short animated scrolls instead of jumps.
- Speed and response sliders, plus an optional coast slider for how far it keeps gliding.
- Works on standard and high-refresh displays.
- Trackpads are untouched.

## Settings
- Speed (default medium)
- Response
- Coast (default low)
- Apps to leave alone

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Disabled in apps listed under 'apps to leave alone'.
- Must not fight with linear scrolling: only one of them is active.

## Permissions
Windows: none. Wayland: input group.

## Platform notes
- Windows: Mouse hook swallows the wheel and sends small steps.
- X11: Same on X11.
- KDE Wayland: evdev grab with uinput high-res wheel events.
- GNOME Wayland: Same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
