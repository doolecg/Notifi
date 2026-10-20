# Shelf

A temporary holding area near the cursor for files, text and links picked up during a drag, to drop or share afterwards.

## User-visible behaviour
- A floating window appears at a screen edge when a drag starts.
- Items can be dropped in, dragged out again, opened or removed.
- Screenshots and captures can be added automatically.
- Images open in the screenshot editor from the item menu.
- Shake gesture: moving the pointer rapidly from side to side while holding a dragged item calls the shelf up beside the cursor, as an alternative to waiting for a drag near the edge.
- Edge peek: dragging an item toward the chosen screen edge slides the shelf partly into view; letting go there adds the item, and pulling away makes the shelf slide back out.
- A list of apps can be excluded so that drags started in them never trigger the shake or the edge drop zone; the keyboard shortcut and an explicit open-now action still work in those apps.

## Settings
- Enable
- Edge and position
- Auto-add captures (default off)
- Clear on quit
- Shake to summon (default on), apps excluded from shake and edge drop zone (default empty), open when a drag nears the edge (default on)

## Default shortcuts
Toggle shelf: Ctrl+Alt+H (Notifi design, not from source docs). (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Window closes when an item is dragged out of it.

## Permissions
Wayland: drag detection varies by compositor.

## Platform notes
- Windows: Floating drop window.
- X11: Same.
- KDE Wayland: Same.
- GNOME Wayland: Same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, app UI strings (release build 3.4.1)
