# Scratchpad

Notes that save themselves, organised in tabs, with Markdown preview and export, shown in a floating window or inside the Top Pill.

## User-visible behaviour
- Several named notes in tabs, in order; new tabs are numbered from 1. New tab Ctrl+T, close Ctrl+W (asks first when the note has text). Each tab has its own close button. The tab strip uses all the width it is given.
- Floating window: drag from anywhere in the top bar, generous resize borders, a minimum size, a pin button that keeps the current note open until closed, background opacity slider up to fully opaque. By default it closes when you click outside (a toggle turns that off).
- Markdown: every note stays editable; a preview shows formatting. Task lists (- [ ] and - [x]) render as checkboxes you can click; Shift+Enter in a list continues it. A formatting toolbar (bold, italic, heading, list, task, link, code) is opened from a Formatting button, in the floating pad and the Top Pill.
- Editing: Alt+Up and Alt+Down move the current line; Ctrl+F searches inside the note; text size set in settings.
- Export and copy: save a note as Markdown or text, copy all.
- Autosave: saves continuously to private app storage, kept out of settings backups; unreadable originals are preserved, not overwritten. If a save fails a warning shows in both views and again if the last save fails while closing.
- The Top Pill version has the same notes (see island.md); the shortcut opens and closes it every time.

## Settings
- Text size (default 14), background opacity (default 90 percent)
- Close on outside click (default on), keep pinned note (default off)
- Preview mode (default off), spell check (default on)
- Storage folder (default app data)

## Default shortcuts
Toggle: Ctrl+Alt+N (Notifi design, not from source docs). Ctrl+T new tab, Ctrl+W close tab, Ctrl+F find, Alt+Up/Down move line. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Dialogs (save, rename, close-with-unsaved) appear above the pad or pill without changing its size or focus.
- Crash or power loss: last autosave is recovered on next open.
- Very large notes (over 5 MB) disable live preview.
- Resizing never shrinks the window below usable size.

## Permissions
None.

## Platform notes
- Windows: topmost tool window, system spell check.
- X11: _NET_WM_STATE_ABOVE window.
- KDE Wayland: normal window with a KWin rule suggestion for keep-above (PARTIAL).
- GNOME Wayland: keep-above only via the shipped extension (PARTIAL).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
