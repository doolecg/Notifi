# Camera preview

See yourself on camera in a floating mirror or inside the Top Pill, without starting a call app.

## User-visible behaviour
- Opens from the menu/tray, the quick panel, a shortcut or the Top Pill. It shows a live mirrored image of the chosen camera.
- Floating window: small, draggable, resizable, always on top, with a Stop camera button over the image. The Top Pill version fills the pill's page at a 4:3 shape with Stop camera over it (see island.md).
- If more than one camera is present it asks which to use and remembers the choice.
- Mirror toggle (default on), and brightness check by eye only; no recording from here.
- Closing the window or pressing Stop releases the camera immediately so the camera light goes off.

## Settings
- Preferred camera (default: system default)
- Mirror image (default on)
- Window size (default small)
- Remember window position (default on)

## Default shortcuts
None by default; assignable (Notifi design, not from source docs). Esc closes. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Camera in use by another app: shows a "busy" message; does not fight for it.
- Camera disconnected while open: closes with a notice, no frozen frame.
- Privacy shutter or disabled device: message pointing to system settings.
- Never starts on its own; no frames are saved or sent anywhere.

## Permissions
Windows: camera privacy toggle for desktop apps. X11: access to /dev/video via the video group or PipeWire. Wayland: camera portal consent.

## Platform notes
- Windows: Media Foundation capture; permission via Settings > Privacy > Camera.
- X11: V4L2 or PipeWire.
- KDE Wayland: PipeWire camera through the portal.
- GNOME Wayland: PipeWire camera through the portal.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
