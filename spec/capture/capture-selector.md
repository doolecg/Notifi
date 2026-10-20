# Capture selector and magnifier

One selector for screenshot, recording, text recognition and colour picking, with a pixel magnifier for precise edges.

## User-visible behaviour
- A dim overlay covers the display under the pointer. A small control strip offers the modes: screenshot, recording, copy text (and QR), colour picker.
- Selection targets: drag an area, click a window (including dialogs and sheets stacked on it, and windows that straddle two displays), or take the whole display under the pointer with a Full screen button. Window grabs skip the decorative borders and shadows other apps draw around windows.
- A capture starts on the first drag, so there is no extra click to arm it.
- Magnifier: a pixel grid around the pointer, optionally on by default. The pixel under the pointer is ringed (no crosshair through it) and sits in the exact centre of the grid, so the colour shown is the colour that will be copied. Magnification is adjustable, and a fast-zoom mode sweeps the whole range with only a handful of wheel clicks.
- Keyboard pixel selection: arrow keys nudge the pointer one pixel at a time; C copies the colour of the ringed pixel. Crop and resize edges snap to whole pixel boundaries so the cut matches the source pixel.
- Guides, palette and hints stay readable over bright, dark and mixed backgrounds and in both light and dark themes.
- The outline of the previous capture can be shown or hidden.
- Mode switching never drops the overlay (no flash of the desktop) and only refreshes the frozen source when the new mode needs it.
- Each mode can have its own shortcut, and the mode menu on the strip is optional. Its position keeps clear of the Top Pill when the pill hosts the controls (see island.md).
- Esc cancels. Controls start compact and open while the pointer rests on them.

## Settings
- Default mode (default: screenshot)
- Show mode menu (default on)
- Magnifier on by default (default on) and magnification level (default 8x)
- Show last capture outline (default on)
- Freeze screen while selecting (default on)
- Per-mode shortcuts (see below)

## Default shortcuts
- Screenshot: Print Screen. Record: Ctrl+Alt+Shift+R. Copy text: Ctrl+Alt+Shift+T. Colour picker: Ctrl+Alt+Shift+C. (Notifi design, not from source docs.) Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.
- Inside the selector: arrows nudge one pixel, Shift+arrows ten pixels (Notifi design), C copies colour, Esc cancels, Tab cycles modes.

## Edge cases
- While recording, the screenshot, text and colour shortcuts keep working without interrupting it if the mode menu is switched off; with the menu switched on, they hold off until the recording is over.
- Multi-monitor and mixed-DPI: the overlay is one window per display, magnifier samples physical pixels, not scaled ones.
- Pointer is over the control strip: the magnifier hides rather than sampling the strip itself.
- Windows that cross displays or are partly off screen are still captured whole.

## Permissions
- Windows: none. X11: none. Wayland: the portal's screen-share picker; a one-time consent may be remembered. Global capture shortcuts need the compositor or the shipped GNOME extension to deliver them.

## Platform notes
- Windows: Windows.Graphics.Capture for frames, a topmost layered overlay per monitor; per-monitor DPI aware.
- X11: XShm grab of the root window, override-redirect overlay windows.
- KDE Wayland: xdg-desktop-portal ScreenCast/Screenshot with a PipeWire frame; area selection is drawn by Notifi over the portal frame (PARTIAL: overlay placement relies on layer-shell).
- GNOME Wayland: portal only; the pixel magnifier works from the portal frame, not live (PARTIAL), and the shipped extension is needed for always-on-top overlay behaviour.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
