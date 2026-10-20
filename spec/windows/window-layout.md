# Window layouts and snapping

Arrange windows into layouts, send them to other displays and bring back earlier positions via shortcuts, screen-edge drags or modifier-drag.

## User-visible behaviour
- Layout actions: halves, thirds, two-thirds, quarters, centre, maximise, maximise with adjustable margin, move to next display.
- Edge and corner drag: every edge or corner can be tied to any placement, and a single edge may be cut into up to four zones.
- Windows snap across the edge shared by two displays when you slow down there; a quick drag carries the window to the other display.
- Dragging a window to the top edge maximises it.
- Modifier-drag moves or resizes a window from anywhere inside it; snapping also applies.
- An animated snap preview overlay shows the target area.
- Restore the previous size and position (undo).

## Settings
- Shortcut per layout action
- Snap at screen edges (default on)
- Edge areas and placements
- Modifier for drag (default Alt)
- Margin size for maximise
- Apps to leave alone

## Default shortcuts
Ctrl+Alt+Arrows for halves, Ctrl+Alt+Enter maximise, Ctrl+Alt+C centre, Ctrl+Alt+U/I/J/K quarters (Notifi design, not from source docs). (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Fullscreen apps are ignored.
- Different DPI scales across displays must keep sizes correct.
- Shortcuts keep working while other Notifi alerts are open.

## Permissions
Windows: none (may not control elevated windows unless Notifi runs elevated). X11: none. Wayland: KWin script or GNOME extension required.

## Platform notes
- Windows: SetWindowPos with a mouse hook for edge and modifier drag.
- X11: EWMH.
- KDE Wayland: KWin scripts.
- GNOME Wayland: GNOME extension.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
