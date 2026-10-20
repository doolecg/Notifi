# Quit on close

Chosen apps exit completely once their final window is closed, instead of idling in the background.

## User-visible behaviour
- User picks apps in a list; when the last window closes the app's process is ended cleanly.
- Apps that still show something (floating control bar, slideshow, open dialog) are kept running.
- Apps with an incoming call or background job are left alone when they have no window.
- Exceptions list for apps that must keep running (screen recorders, sync tools).

## Settings
- Apps to quit on close
- Grace delay in ms (default 800) (Notifi design, not from source docs)
- Exceptions

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Settings warns that recorders and similar tools may keep working without windows.
- Watch must not raise CPU use in watched apps.

## Permissions
None on Windows. Linux: none.

## Platform notes
- Windows: Win event hook for window destroy, then end process.
- X11: Window-closed signal then end process.
- KDE Wayland: Same via KWin script.
- GNOME Wayland: Via extension.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
