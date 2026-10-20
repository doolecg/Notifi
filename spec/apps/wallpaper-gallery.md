# Wallpaper gallery

An optional panel section that applies still wallpapers from a chosen folder.

## User-visible behaviour
- A thumbnail grid of images in a chosen folder (and the OS wallpaper folder); click to apply to the current display or to all.
- Thumbnails are prepared only when the gallery opens.
- Off by default.

## Settings
- Enable (default off); folders (default pictures folder); apply to: current display / all displays

## Default shortcuts
None.

## Edge cases
- Large folders load lazily; unreadable files are skipped.

## Permissions
None.

## Platform notes
- Windows: the desktop wallpaper API per monitor. KDE: plasma-apply-wallpaperimage / D-Bus. GNOME: gsettings. X11: depends on desktop.
- Status: Windows PORT, Linux PORT (tiling window managers best effort).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
