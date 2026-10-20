# Middle click from touchpad (adapted)

Make a three-finger touchpad press a middle click.

## User-visible behaviour
- Windows: opens the Precision Touchpad settings page where the gesture is set.
- Linux: sets the libinput tap button map so three-finger tap is middle.

## Settings
- Enable (Linux)

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Needs a touchpad.

## Permissions
None.

## Platform notes
- Windows: Opens system touchpad settings.
- X11: libinput tap map via xinput.
- KDE Wayland: KDE input settings D-Bus.
- GNOME Wayland: gsettings.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
