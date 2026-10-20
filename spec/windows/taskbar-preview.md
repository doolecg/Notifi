# Taskbar preview

Hover a taskbar (or panel) button and get Notifi's own large preview of the app's windows, across every virtual desktop, with actions. Replaces the Mac Dock Preview.

## User-visible behaviour
- Hovering a taskbar button for a short delay opens a preview above or beside it with one card per window of that app: live thumbnail, title and app icon.
- Cards include windows on other virtual desktops (with a desktop badge) and minimised windows (dimmed thumbnail from the last capture). A filter can limit the preview to the current desktop.
- From a card: click to switch (bringing it forward and changing desktop if needed); close it (red button on hover, or middle-click); move it to another display or desktop (drag the card to a drop target, or use its context menu); snap it to a layout position (context menu).
- Dragging a card onto the screen moves that window there, keeping its size; a minimised window is restored at the drop point.
- Minimal mode: smaller cards with titles only. Card size: Normal, Large, Extra large.
- Multi-window apps show a position counter and previous/next arrows in the header; the selected card stays in view in long rows; scrolling does not jump back to the hovered card.
- A card can be pinned: the preview stays open until unpinned, can be dragged elsewhere, and several pinned previews can stay on screen together.
- The close button can optionally quit the app instead of only closing the window.
- A temporary "peek" (the window shown full size in place) while hovering a card, before choosing it.
- Opening delay (default 200 ms) and app-switching delay (default 100 ms) are adjustable.
- Panel background opacity slider (disabled when the system uses translucent materials).
- The list of apps whose thumbnails pause is separate from the app switcher's list.

## Settings
- Enable (default off)
- Opening delay (default 200 ms), app-switching delay (default 100 ms)
- Card size, minimal mode, show windows from other desktops (default on)
- Close button quits the app (default off)
- Peek on hover (default on)
- Keep an auto-hidden taskbar visible while browsing (experimental, default off)
- Panel background opacity
- Apps where thumbnails pause

## Default shortcuts
None.

## Edge cases
- Hovering must never wait on a frozen app: window lists are collected with a timeout and unresponsive apps show a placeholder.
- The preview stays put when the taskbar auto-hides.
- Apps that run as several processes act on the right instance.
- Thumbnails stop updating while displays sleep or the screen is locked.
- Notifi does not remove the native Windows preview system-wide; it only suppresses it while ours is shown, and reports if both appear.
- Taskbar on any edge, and one taskbar per display, are supported.

## Permissions
Windows: UI Automation only (no prompt). Linux: KWin script or GNOME Shell extension on Wayland; X11 none.

## Platform notes
- Windows: UI Automation finds the hovered taskbar button and its app; DWM thumbnails give live previews; the window list comes from the shell hook.
- X11: panel buttons found through the panel's accessibility tree where it exposes one.
- KDE Wayland: the task manager's hovered task via the KWin script, thumbnails through the script.
- GNOME Wayland: the dock's hovered icon via the extension, thumbnails through it.
- Status: Windows PARTIAL (hit-testing the taskbar depends on UI Automation and may break after taskbar redesigns), Linux PARTIAL (depends on the panel).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/TROUBLESHOOTING.md
