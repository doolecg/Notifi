# Haptic feedback
Gentle taps on a compatible touchpad when opening the Top Pill, switching sections or adjusting timer minutes.

## User-visible behaviour
- A single on/off option enables small tactile taps for a few interface actions: opening the pill, moving between its sections and stepping a timer's minutes up or down.
- Only touchpads that support tactile output respond; on everything else the option has no effect and is shown as unavailable.
- Taps are short and never repeat while a control is held.

## Settings
- Haptic feedback (default off)

## Default shortcuts
None.

## Edge cases
- Reduced motion does not silence haptics; they are a separate switch.
- If the device stops reporting haptic support (for example the touchpad is unplugged), the option greys out without losing its saved value.

## Permissions
None.

## Platform notes
- Windows: only precision touchpads that expose haptic output through the OS haptics API; PARTIAL, hidden when no device supports it.
- X11: no standard haptic interface for touchpads; the option stays hidden.
- KDE Wayland: same as X11.
- GNOME Wayland: same as X11.
- Status: Windows PARTIAL, Linux PARTIAL (hardware dependent; effectively absent on Linux).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, app UI strings (release build 3.4.1)
