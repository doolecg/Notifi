# Tray icon and menu

## User-visible behaviour
- Tray icon replaceable with a gallery icon or a user image; active/inactive states; a status badge for updates and a muted-microphone indicator.
- Option to hide the icon while readouts are showing; it returns when needed.
- Icon recovery after updates if the OS hides it; guidance on where to allow it.
- Launch at login, with a note when the OS needs approval.
- Right-click menu: Settings, Quit, update.

## Settings (defaults)
- Icon: default; hide while readouts show: off; launch at login: off

## Default shortcuts
None.

## Edge cases
- If the tray is not available (some GNOME sessions), the Top Pill or a window opens the panel.

## Permissions
None.

## Platform notes
- Windows: overflow area promotion instructions. Linux: AppIndicator; GNOME needs the extension.
- Status: Windows PORT, Linux PARTIAL.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
