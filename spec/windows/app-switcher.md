# App switcher with live previews

A better Alt+Tab: switch between apps and individual windows with live thumbnails, search and per-display filters. A simpler icon-only mode works without screen capture.

## User-visible behaviour
- Hold the trigger and tap Tab to move the selection; release to switch. Shift reverses.
- Grid of windows with live preview, title and app icon; selection highlight moves with an animation.
- Type to search by app or window title.
- Filters: current desktop only, current display only, include minimised, include fullscreen windows from other desktops.
- Maximum columns can be limited.
- Icon-only mode is used when capture is unavailable or off.
- Close or quit a window from the switcher (hover buttons or key).

## Settings
- Replace the system switcher (default on)
- Trigger shortcut (default Alt+Tab; Windows/Linux)
- Preview size
- Maximum columns
- Filters above
- Pause thumbnails for chosen apps

## Default shortcuts
Alt+Tab (replaces system switcher); Alt+` cycles windows of the same app (Notifi design, not from source docs). (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Thumbnails pause while displays sleep or the screen is locked.
- Hidden or closed windows and blank helper entries are not listed.
- Choosing a window raises only that window.
- If another app holds exclusive keyboard capture, the switcher cannot open and Settings says why.

## Permissions
No special permission on Windows. X11: none. KDE and GNOME Wayland: Notifi's KWin script or GNOME Shell extension must be enabled.

## Platform notes
- Windows: Low-level keyboard hook plus DWM thumbnails.
- X11: XComposite thumbnails.
- KDE Wayland: KWin script over D-Bus.
- GNOME Wayland: Shipped GNOME Shell extension (thumbnails and focus).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/TROUBLESHOOTING.md
