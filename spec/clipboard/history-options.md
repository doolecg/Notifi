# Clipboard history: options and layouts

Sub-features of clipboard history (see clipboard-history.md for the core behaviour).

## User-visible behaviour
- **Retention:** keep up to 100 up to 10,000 items, or unlimited; optionally remove items older than a set number of days. Pinned items stay.
- **Layouts:** a resizable list with a preview pane, or a shelf of cards along the bottom; the window remembers its size; each entry shows the app it was copied from.
- **Search:** highlights matched words; supports input-method composition; arrow keys select, Enter activates, Ctrl+N / Ctrl+P move; large histories search fast.
- **Quick paste:** the first nine entries paste with Ctrl+1 to Ctrl+9 (also on the numeric keypad); the number shortcuts show while the window has the keyboard. Pasting or copying an entry moves it to the top, except pinned entries.
- **Multi-select:** mark several entries and paste or copy them as one stack; delete several at once.
- **Clear unpinned:** asks first and says how many it will remove; anything copied while it asks is kept.
- **Previews:** images (can be opened in the screenshot editor while leaving the clipboard untouched, and text can be lifted out of the picture), copied colours as swatches, JSON laid out one value per line, files as links.
- **Sensitive-text filter:** optional (default off). Copies marked confidential by the source app and ignored apps stay out. The filter must not reject normal commands or links just for mentioning words like "password".
- **Apps the history ignores:** chosen apps never reach the history.
- **Menu or tray readout:** can show the latest copy next to the tray icon (where the tray supports text).
- **Expired and failed copies:** expired copies are skipped; failed writes are reported and no automatic paste follows.
- **Pinned items** persist across restarts.

## Settings (defaults)
- Items to keep: 1000; remove after days: off; layout: list
- Save images and files: on; sensitive filter: off; ignored apps: empty
- Show latest copy in the tray: off

## Default shortcuts
Open history: Ctrl+Shift+V (Notifi design); inside: Ctrl+1..9, Ctrl+N/P, Enter, Del.

## Edge cases
- Keeps recording after the clipboard service restarts.
- A new copy while the window is open does not move the highlight.
- Personal clipboard content is stored privately and excluded from settings backups.

## Permissions
Linux Wayland: data-control protocol (KDE yes, GNOME via extension).

## Platform notes
Same as clipboard-history.md.
- Status: Windows PORT, Linux PARTIAL.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
