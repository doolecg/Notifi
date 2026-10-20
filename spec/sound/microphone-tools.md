# Microphone tools

Pick a preferred microphone, change its level where the hardware allows, and mute every microphone at once with one shortcut.

## User-visible behaviour
- Preferred input device is chosen in a list and becomes the default whenever connected.
- Level slider with percentage entry for devices that expose volume; hidden when unsupported.
- Mute-all shortcut mutes or unmutes every capture device; a toast and tray state show the result.
- Mute state is visible in the tray icon and optionally in the Top Pill.
- Mute and unmute notices can be switched off.

## Settings
- Preferred microphone
- Mute-all on/off
- Show mute notices (default on)

## Default shortcuts
Mute all microphones: Ctrl+Alt+Shift+M (Notifi design, not from source docs). (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- If only some devices accept mute, report the partial failure instead of pretending success.
- A new microphone connecting while muted stays muted.

## Permissions
Microphone privacy permission is not needed to change volume or mute.

## Platform notes
- Windows: Endpoint volume and mute on all capture endpoints.
- X11: Sources: volume and mute.
- KDE Wayland: Same.
- GNOME Wayland: Same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
