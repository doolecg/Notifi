# Pointer acceleration

Turn off mouse acceleration and get your previous setting back when switched off.

## User-visible behaviour
- Switch for flat pointer response.
- Previous setting is saved and restored when the feature is turned off or Notifi quits cleanly.
- Trackpads keep their behaviour if the system separates devices.

## Settings
- Disable acceleration (default off)

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Restore on crash: restore at next start if a saved value exists.

## Permissions
None.

## Platform notes
- Windows: SPI_SETMOUSE save and restore.
- X11: xinput libinput accel profile flat.
- KDE Wayland: kcminputrc / D-Bus.
- GNOME Wayland: gsettings flat profile.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
