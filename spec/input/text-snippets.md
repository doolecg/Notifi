# Text snippets

Short triggers expand into longer text, with clipboard, date and time variables. Snippets can also be inserted from a searchable menu.

## User-visible behaviour
- Type a trigger (for example ;sig) and it is replaced by the snippet text.
- Variables: clipboard contents, date, time, with a visual date and time builder with timezone and live preview.
- Searchable popup menu inserts a snippet at the cursor with Enter or number keys 1-9 (keypad works too).
- Snippet editor with line move (Alt+Up/Down), groups, and search.
- The first character of a trigger is not left behind in address bars that auto-complete.
- A newer copy replaces the clipboard cancels a pending paste.
- Typed triggers are Phase 4; the menu is Phase 1.

## Settings
- Trigger prefix (default none)
- Case sensitive (default off)
- Apps to leave alone
- Insert method: type or paste

## Default shortcuts
Open snippet menu: Ctrl+Alt+S (Notifi design, not from source docs). (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Secure input (password fields) disables typed expansion.
- Settings names the app that currently holds secure input.

## Permissions
Windows: none. X11: none. Wayland: input group for evdev read and uinput write.

## Platform notes
- Windows: Keyboard hook plus SendInput.
- X11: XRecord + XTest.
- KDE Wayland: evdev + uinput.
- GNOME Wayland: evdev + uinput.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
