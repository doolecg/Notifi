# Quit and close protection

Stops accidental quit or close shortcuts: each app can require holding the keys, pressing twice or adding another modifier.

## User-visible behaviour
- Guards the quit shortcut (Ctrl+Q / Alt+F4) and the close-tab/window shortcut (Ctrl+W), per app.
- Modes: hold for a moment (progress ring shows), press twice, or add an extra modifier.
- Remote desktop, virtual machine and Screen Sharing-like apps pass a confirmed shortcut on to the remote machine instead of closing locally.
- Confirming for a launcher app quits it rather than only closing its window.

## Settings
- Per-app mode and key
- Hold time (default 700 ms) (Notifi design, not from source docs)
- Global default mode

## Default shortcuts
None (guards system shortcuts). (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Unconfirmed presses are swallowed without side effects.
- If the key hook fails to start, protection is off and the user is told.

## Permissions
None on Windows. X11: key grab. Wayland: evdev grab (input group).

## Platform notes
- Windows: Keyboard hook on Alt+F4 / Ctrl+W / Ctrl+Q.
- X11: X11 key grab.
- KDE Wayland: evdev grab.
- GNOME Wayland: evdev grab (PARTIAL).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
