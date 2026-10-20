# Clipboard history

Search a local history of copied text, images and files, pin favourites, preview entries and paste them with shortcuts.

## User-visible behaviour
- Records text, images and file lists with source app and time.
- Searchable list or shelf of cards; resizable window; preview pane.
- Pin favourites; pinned items never expire.
- Paste with Enter or number shortcuts 1-9 (keypad works); automatic paste or copy-only.
- Expiry: remove items after N days (default off).
- Ignored apps and items marked confidential are not saved.
- Optional sensitive-text filter (off by default).
- Everything stays on the machine.

## Settings
- History on/off
- Max items (default 500) (Notifi design, not from source docs)
- Keep for N days
- Ignored apps
- Sensitive-text filter (default off)
- Auto paste (default on)

## Default shortcuts
Open history: Ctrl+Shift+V (Notifi design, not from source docs). (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- A failed copy shows a warning and does not paste.
- Requests time out after 5 s.
- Large images are stored compressed.

## Permissions
Windows: none (auto paste sends Ctrl+V). Wayland: none on KDE; GNOME needs the extension.

## Platform notes
- Windows: Clipboard format listener.
- X11: XFixes.
- KDE Wayland: wlr/ext data-control (KDE yes).
- GNOME Wayland: GNOME extension (PARTIAL).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/TROUBLESHOOTING.md
