# Quick panel

A floating palette of favourite tools, opened with a shortcut, where small utilities run without leaving the current app.

## User-visible behaviour
- A compact panel near the pointer (or screen centre) lists tools one line each: utilities, controls and quick toggles together in a single card. Descriptions are tooltips; permission notes, status and side effects (for example "restarts the file manager") stay visible on the row.
- Tools can be embedded: choosing one opens it inside the panel with separate Back and Close buttons. Examples: Cleaner (its Clean button stays reachable in compact layouts), clipboard quick window, timer, volume mixer, camera.
- Controls include recording start/stop, keyboard light on/off, and toggles from quick-toggles.md.
- Edit mode reorders, hides and shows tools, and shows the descriptions.
- A wand-shaped Tools button distinguishes tools from the sections button.
- Esc clears a selection or closes the panel (in the clipboard view it clears the selection first); with text composition in progress (Chinese, Japanese, Korean), Esc cancels the composition instead of closing.
- Number keys, including the keypad, activate the matching row. Clicking outside closes it.
- Weekday pickers and similar rows wrap to their own line when they do not fit.

## Settings
- Tools shown and order (default: all, sensible order)
- Close after running (default on)
- Open under pointer or centred (default under pointer)
- Panel size (default medium)

## Default shortcuts
Open: Ctrl+Alt+V (Notifi design, not from source docs). Number keys activate rows, Esc closes. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Panel never extends past the display edge; it flips or shifts to stay on screen.
- A tool needing a missing permission shows the note and a button to fix it, not an error.
- Muting the microphone from the panel does not move the panel.
- Uninstalled features are greyed with a link to the Features page.

## Permissions
Depends on the tool; each row states it.

## Platform notes
- Windows: topmost non-activating tool window.
- X11: override-redirect window; focus is taken on demand.
- KDE Wayland: layer-shell surface where available, else a normal window (PARTIAL).
- GNOME Wayland: normal window kept on top by the shipped extension (PARTIAL).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
