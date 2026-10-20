# Focus follows mouse

After a configurable pause, the window beneath the pointer takes focus, with an option not to raise it.

## User-visible behaviour
- Hover delay from 100 ms to 3 s.
- Option to focus without raising the window.
- Pop-up windows and menus are not closed as the pointer travels between them and the window that owns them.
- Does not flicker toward the window behind an open dialog or sheet.

## Settings
- Enable (default off)
- Delay (default 300 ms) (Notifi design, not from source docs)
- Raise on focus (default off)

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Ignores the desktop and panels.
- Native desktop setting used where available.

## Permissions
None.

## Platform notes
- Windows: SPI_SETACTIVEWINDOWTRACKING with delay.
- X11: Native setting in most window managers; Notifi toggles it.
- KDE Wayland: Native KWin option.
- GNOME Wayland: Native GNOME focus mode.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
