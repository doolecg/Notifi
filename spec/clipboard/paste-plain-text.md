# Paste as plain text

Paste copied content without formatting while the original clipboard stays intact.

## User-visible behaviour
- Sets plain text, sends the paste shortcut, then restores the original content.
- Images, files and videos are preserved when no text is involved.
- Works if bound to the normal paste shortcut.
- Cancels if a newer copy replaces the clipboard first.

## Settings
- Shortcut
- Enable

## Default shortcuts
Ctrl+Shift+V (clashes with history default; choose one) (Notifi design, not from source docs). (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Slow source apps must not freeze it (timeout).

## Permissions
Windows: none. Wayland: input group for synthetic paste.

## Platform notes
- Windows: Set text, SendInput Ctrl+V, restore.
- X11: XTest.
- KDE Wayland: uinput.
- GNOME Wayland: Same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/TROUBLESHOOTING.md
