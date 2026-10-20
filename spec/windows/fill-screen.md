# Fill screen (maximize without a new desktop)

Adapts the Mac "green button fills the screen without making a new Space". On Windows and Linux, Notifi gives any window a one-press "fill screen" that covers the work area (or the whole display) without moving it to another virtual desktop, and a second press puts it back.

## User-visible behaviour
- A shortcut toggles fill screen for the active window. First press: the window loses its title bar and border (where the OS lets us) and fills the work area, minus the chosen gap. Second press: it returns to its exact previous position, size and decorations.
- Fill target is the work area (taskbar and panels stay visible) or, if chosen, the entire display covering the taskbar.
- A gap (margin) around the filled window can be set, matching the "maximize with margin" action in window layouts.
- Optional: take over the window's own maximize button. Clicking maximize then runs fill screen instead of the native maximize, so the window keeps a predictable size and stays on its current virtual desktop. Off by default.
- A window moved from another display and then filled remembers the size it had before, not the old display's size; restoring puts it back where it was filled.
- Chosen apps can be left on native maximize (an exceptions list), for apps that have their own full-screen mode.
- Fill screen also appears as an action in the window layout grid (it is the same action).

## Settings
- Enable fill screen (default off)
- Shortcut (default none; Notifi design suggestion: Ctrl+Alt+Enter)
- Cover: work area (default) or whole display
- Gap in pixels (default 0)
- Take over the maximize button (default off)
- Apps to leave on native maximize (default empty)

## Default shortcuts
None by default.

## Edge cases
- A window that cannot be resized (fixed-size dialogs) is skipped silently.
- Windows that are already truly fullscreen (games, video) are left alone.
- If the saved restore size would now be off screen (display unplugged), the window is restored onto the primary display, centred.
- Minimised windows are restored first, then filled.
- Display scale changes while filled keep the window filled to the new area.

## Permissions
None on Windows and X11. KDE and GNOME Wayland need Notifi's KWin script or GNOME Shell extension for window resizing.

## Platform notes
- Windows: SetWindowPos plus a window event hook; the maximize-button takeover uses the hook to see the maximize request and replace it. Borderless is done by changing the style bits and restoring them afterwards.
- X11: EWMH move/resize; decorations toggled through the motif hint or the fullscreen/maximized state atoms.
- KDE Wayland: KWin script sets the geometry and the "no border" flag.
- GNOME Wayland: extension moves and resizes the window; borders cannot be removed, so the fill uses the full work area including the title bar.
- Status: Windows ADAPT, Linux ADAPT (the Mac way of avoiding a new Space has no direct equivalent).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
