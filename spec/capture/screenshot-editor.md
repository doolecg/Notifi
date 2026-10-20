# Screenshot and editor

Capture, preview, then annotate, crop, redact, add a background or watermark, pin, share or upload.

## User-visible behaviour
- After a capture a confirmation preview appears. It can be hidden, shown for 1 to 10 seconds, or kept until dismissed. A timed preview closes by itself even while the mouse moves.
- After-capture actions (each can be automatic): copy to clipboard, save to the chosen folder, add to the Shelf, upload as a temporary link, copy recognised text, open in the editor, pin on screen, share. Turning off automatic copy also removes the Copy button from the preview, and "Save and copy" becomes "Save".
- Temporary link: a separate shortcut (off by default) uploads the latest capture and copies the link. It never uploads an edited or discarded capture; if the capture is edited or discarded mid-upload, or links are turned off, the late link is not copied and is revoked when the server can be reached.
- Pin: keeps the image floating above other windows, draggable and resizable, dismissed individually; pinned images are optionally hidden while the selector is open.
- Share: through the OS share/save targets (Windows share sheet, portal share or "open with" on Linux), from both the preview and the editor.
- Editor tools: select/move, pen, line, arrow, rectangle, ellipse, highlighter, text (own size setting), numbered step badge, crop, Blur, Spotlight, eraser.
  - Blur tool: pixelate, blur or erase. Applies to a whole rectangle, or only to the text found inside it (text-only redaction). Strength is adjustable; small sampled mosaics are cached per strength.
  - Spotlight: dims everything outside a chosen shape.
  - Canvas expansion: enlarge the canvas on any side, filled with a colour or transparent.
  - Image layers: paste an image from the clipboard (Ctrl+V) as a movable, resizable layer with opacity.
  - Background: gradient, solid, or padded with rounded corners and shadow for presentation shots.
  - Watermark: custom text or image, position, opacity and size, applied on export.
- Crop snaps its edges to whole pixels. Every tool stays reachable even in short editor windows; controls never overflow small captures.
- Undo/redo, zoom, pan. Pen strokes that return to their start point stay visible.
- Export: PNG, JPEG, WebP; copy; save as; send to Shelf. Opening an image from Shelf or clipboard history in the editor never replaces the clipboard.
- On high-density displays, an option saves captures at half their pixel size (1x) so the files are smaller; the default keeps full pixels. On Windows and Linux this applies when display scaling is above 100 percent.

## Settings
- Folder, file name template, format and quality (default PNG)
- Automatic copy (default on), automatic Shelf add (default off), automatic editor open (default off)
- Preview duration (default 5 s; options hidden, 1-10 s, until dismissed)
- Temporary links on/off (default off), watermark profile, blur strength, text size
- Include shell-drawn overlays (Top Pill) in shots when "show in screenshots" is on (default off)
- Save at 1x size on high-density displays (default off)

## Default shortcuts
Capture shortcuts live in capture-selector.md. Editor: Ctrl+Z / Ctrl+Y undo/redo, Ctrl+C copy, Ctrl+S save, Ctrl+V paste layer, Delete remove layer, Esc close, single-letter tool keys (Notifi design, not from source docs). Link upload: Ctrl+Alt+Shift+U (Notifi design). Cmd maps to Ctrl.

## Edge cases
- Very large and scrolling captures: text-only blur must still find every line of text.
- Editing memory use is kept low by caching downscaled mosaics and releasing the full bitmap when the editor closes.
- Preview releases keyboard focus before the editor opens.
- A failed save keeps the capture in memory and says so; it never silently loses it.

## Permissions
Same as capture-selector.md. Network use only for temporary links, which is opt-in.

## Platform notes
- Windows: Win2D/Direct2D or Skia rendering, Share contract, always-on-top tool windows for pins.
- X11: Same editor; share falls back to save and open-with; pins use _NET_WM_STATE_ABOVE.
- KDE Wayland: pins rely on layer-shell/KWin rules (PARTIAL); share via portal.
- GNOME Wayland: always-on-top pins need the shipped extension (PARTIAL); share via portal.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, app UI strings (release build 3.4.1)
