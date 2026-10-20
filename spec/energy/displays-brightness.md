# Displays and brightness

Control each display's brightness, with half and quarter steps, per-display on/off and a keyboard backlight slider.

## User-visible behaviour
- A Displays panel lists every connected screen with its own brightness slider and a power/on-off button. Mirrored displays stay listed individually, each with its own slider and button, including the built-in screen.
- Hardware control is used where the display supports it (DDC/CI for external monitors, the backlight interface for laptop panels). Where it does not, software dimming is the fallback.
- Brightness keys and shortcuts glide the display to a new level rather than snapping. They act on the display under the pointer (or the primary display, as chosen); keys from external keyboards work too, and a level just set with a slider is kept.
- Step size for the keys: full, half or quarter steps, for finer control near the low end.
- Optional custom shortcuts for brightness up/down, aimed at either the main display or whichever display the pointer is on.
- Optional brightness overlay showing the percentage after a change.
- Keyboard backlight: a slider next to volume and brightness; its icon switches the light off and on, and shortcuts change it one step at a time.
- Per-display off: switch a monitor off (or dim to black) without disconnecting it; it comes back on its next brightness change or click.
- Extra dimming below a monitor's hardware minimum is covered in extra-dimming.md.
- Reconnected monitors get their previous brightness restored on the right screen; newly connected monitors appear promptly with controls still loading.
- Shortcut actions are available to move to the previous or next display, to switch a display off or on, and to raise or lower the keyboard backlight, in addition to raising or lowering display brightness.

## Settings
- Control displays: off (nothing is touched until enabled).
- Brightness key steps: full.
- Target for keys: display under the pointer.
- Custom shortcuts: off. Overlay: on.
- Keyboard backlight slider: shown only where a backlight exists.

## Default shortcuts
Brightness keys. Custom brightness shortcuts: none until set. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Stale or late DDC replies from another display's controls must be ignored; unsupported monitors stop being retried.
- A press and release of a brightness key stay together even when displays reconnect or the pointer crosses displays.
- Opening the panel must not change a display already at that level.
- A monitor must never go fully dark by accident while adjusting; the minimum keeps the picture visible.
- Restore the built-in display after the last external one is unplugged.

## Permissions
- Linux DDC needs access to the i2c devices (a udev rule or the i2c group); laptop backlight uses logind or a polkit rule. Windows needs none.

## Platform notes
- Windows: DDC/CI via the monitor configuration API for externals, WMI for the built-in panel.
- X11: ddcutil for externals, /sys backlight or logind for the built-in panel.
- KDE Wayland: logind SetBrightness and PowerDevil for built-in; ddcutil for externals.
- GNOME Wayland: same as KDE via logind; ddcutil for externals.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, app UI strings (release build 3.4.1)
