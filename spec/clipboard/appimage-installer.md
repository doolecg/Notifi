# AppImage installer (Linux half of the disk-image installer)

Replaces the Mac disk-image installer: install an AppImage into your applications folder.

## User-visible behaviour
- Offers to install an AppImage you open from Downloads.
- Moves it to ~/Applications, makes it executable, adds a menu entry.
- Shows progress and offers to delete the download and reveal the app.
- Windows: covered by installer-helper.md.

## Settings
- Offer on AppImage open
- Remember answers

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Ignore AppImages already in the applications folder.

## Permissions
Linux only; Windows uses installer-helper.md.

## Platform notes
- Windows: see installer-helper.md (the Windows half of this feature).
- X11: Yes.
- KDE Wayland: Yes.
- GNOME Wayland: Yes.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
