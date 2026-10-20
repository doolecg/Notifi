# Window layouts: snap areas and extra actions

Sub-features of window layouts (see window-layout.md for the core behaviour).

## User-visible behaviour
- **Actions:** halves, quarters, thirds, two-thirds, centre, maximise at the top edge, centred half-width, any cell of a six-part grid, quarter rows and columns, vertical thirds, next display, fill screen, maximise with an adjustable margin, restore. Each action can have its own shortcut or none; unused actions can be hidden from the grid while their shortcuts keep working.
- **Repeat to move:** pressing left or right repeatedly moves the window on to the neighbouring display; repeating Top or Bottom half can move to a stacked display. Moving a window between displays keeps its size.
- **Cycle widths:** repeating a side shortcut can cycle through half, third and two-thirds.
- **Pointer layouts:** hold a shortcut plus a pointer direction to place the window toward one of eight directions. Pointer shortcuts can be made only of modifiers.
- **Modifier-drag:** hold chosen modifiers and drag anywhere in a window to move it; add Shift to resize; the right mouse button can do the same. With snapping on, a modifier-drag also snaps.
- **Screen-edge snapping:** drag to an edge or corner for a preview, release to place. Each edge and corner may be given any placement; one edge can be divided into as many as four zones; at a border shared by two displays the window snaps only when you slow down, while a quick drag moves it straight across. Top edge maximises.
- **Gaps:** an adjustable gap between snapped windows and screen edges.
- **Ignore apps list** for apps that should never be moved; an option to keep native full-screen in chosen apps.
- **Pointer helpers:** a shortcut to move the pointer to the next display.
- **Restore:** earlier positions can be restored.
- Pointer-driven layouts: where the drag starts inside the window chooses the nearest edge or corner to resize from; a right-button drag also resizes when using a mouse.

## Settings (defaults)
- Snap at screen edges: off; snap preview: on; gap: 0
- Modifier-drag: off; modifiers: Alt (Notifi design)
- Per-edge placement table; ignore apps: empty; native full-screen apps: empty

## Default shortcuts
All empty by default. Suggested Notifi design: Win+Alt+arrows for halves.

## Edge cases
- Dragging inside apps that draw their own title bar (creative tools) must not stutter or stall other input.
- A window moved from another display must not keep the old display's size.
- Edge snapping stays available when displays share one desktop.
- Dialogs open while another feature shows an alert do not block the shortcuts.

## Permissions
None on Windows/X11; script or extension on Wayland.

## Platform notes
- Windows: SetWindowPos, low-level mouse hook, snap preview overlay window. Aero snap is left in place unless the user turns Notifi's edges on, in which case Notifi asks to disable the native one.
- X11: EWMH. KDE and GNOME Wayland: script or extension.
- Status: Windows PORT, Linux PORT (Wayland PARTIAL until script/extension is enabled).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, app UI strings (release build 3.4.1)
