# Mouse buttons to shortcuts and drag gestures

Bind key shortcuts to extra mouse buttons and side wheels, or drag with a button held to trigger desktop switching and the overview.

## User-visible behaviour
- Each extra button can fire a key combination or an action.
- Drag gestures with a held button (left/right/up/down) map to switch desktop or open overview.
- A mouse button opens only one radial wheel.
- Per-app overrides and exclusions.

## Settings
- Button to action table
- Gesture threshold in px (Notifi design, not from source docs)
- Apps to leave alone

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Buttons used for gestures are not also sent to the app.
- Back and Forward buttons keep working on non-US layouts.

## Permissions
Wayland: input group.

## Platform notes
- Windows: Mouse hook.
- X11: evdev + uinput.
- KDE Wayland: Same.
- GNOME Wayland: Same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
