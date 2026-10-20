# Clean URL

Strips tracking parameters from links, either on request or automatically whenever a link is copied.

## User-visible behaviour
- Manual: a shortcut cleans the clipboard link.
- Automatic: runs on copy.
- Site-specific editable rules; shows what was removed; each rule can be switched off.
- Automatic mode leaves alone copies with more than a link and case-sensitive paths.

## Settings
- Automatic (default off)
- Per-site rules

## Default shortcuts
Clean now: Ctrl+Alt+L (Notifi design, not from source docs). (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- A newer copy arriving mid-clean is untouched.

## Permissions
None.

## Platform notes
- Windows: Rules in core.
- X11: Same.
- KDE Wayland: Same.
- GNOME Wayland: Same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
