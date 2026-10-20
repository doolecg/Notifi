# Output switcher

Switch the default audio output with a shortcut and automatically drop the volume when headphones are unplugged.

## User-visible behaviour
- A shortcut cycles through enabled output devices; a small toast shows the new device name and icon.
- Devices can be ticked on or off for cycling and given a priority order.
- Optional: when headphones (wired or Bluetooth) disconnect, lower the speakers to a chosen volume (0% also mutes).
- Optional: set a chosen volume when headphones connect.

## Settings
- Devices included in cycle
- Priority order (highest connected device is auto-selected, off by default)
- Volume on headphone disconnect (default 0%)
- Volume on headphone connect (default unchanged)

## Default shortcuts
Cycle output: Ctrl+Alt+O (Notifi design, not from source docs). (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- With only one eligible device, the shortcut shows a toast and does nothing.
- Disconnect handling must not trigger when the user switches device themselves.

## Permissions
None.

## Platform notes
- Windows: Set default endpoint via the policy-config interface; device-change events trigger the disconnect rule.
- X11: Default sink; sink-removed event.
- KDE Wayland: Same.
- GNOME Wayland: Same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
