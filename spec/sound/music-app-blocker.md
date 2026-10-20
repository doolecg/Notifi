# Music app blocker (adapted)

Stops a chosen media app from launching when a media key (play/pause, next, previous) is pressed and nothing else is handling playback.

## User-visible behaviour
- User picks one or more apps to block (e.g. a default media player).
- When a media key is pressed and no media session is active, a launch of a blocked app within a short window is cancelled.
- If the key listener is not available, blocking stays off (fail open): the app is allowed to launch.
- The key press itself still reaches whatever player is active.

## Settings
- Blocked app list (empty by default)
- Detection window in ms (default 1500) (Notifi design, not from source docs)

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Blocking must never stop an app the user opened themselves by clicking.
- Disable automatically if the keyboard listener cannot start.

## Permissions
Windows: low-level keyboard hook and process-start watch. Linux: X11 key grab or evdev read (input group) plus process watch.

## Platform notes
- Windows: Keyboard hook plus process creation watch; terminate the new process.
- X11: X11 key listening + process watch.
- KDE Wayland: Needs evdev (input group) for media keys and a process watch.
- GNOME Wayland: Same as KDE.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
