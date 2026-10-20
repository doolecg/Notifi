# Copy text from screen and QR

Read text from any part of the screen without a network connection, or decode a QR code, and place the outcome on the clipboard.

## User-visible behaviour
- Chosen as a mode in the capture selector or by its own shortcut. Drag an area; the text inside is recognised locally (no upload) and copied, with a small confirmation showing what was copied.
- If the area holds a QR code, its content is shown and copied instead of text recognition output; links can be opened from the confirmation.
- Join lines option: recognised lines are merged into one paragraph, with spacing that suits the script (no spaces inserted between CJK characters, normal spaces for Latin text, hyphenated line breaks rejoined).
- If the first recognition pass finds nothing, a second pass is tried with a different recogniser path or language set before giving up.
- Available from the screenshot preview: copy the text in the latest capture. Also from an image in clipboard history.
- Recognition languages: system languages by default, selectable.

## Settings
- Recognition languages (default: system languages)
- Join lines into a paragraph (default off)
- Keep line breaks (default on when join is off)
- Open QR links automatically (default off; always asks first)
- Show confirmation toast (default on)

## Default shortcuts
Copy text: Ctrl+Alt+Shift+T (Notifi design, not from source docs). Also a mode in the selector. Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.

## Edge cases
- Nothing recognised: says so and leaves the clipboard unchanged.
- Very large selections are downscaled in tiles so recognition stays quick; very small ones are upscaled.
- Text on gradients, dark mode or tiny fonts: image is contrast-normalised first.
- Multiple QR codes in the area: lists them and copies the first, others offered.
- Works during a recording without stopping it.

## Permissions
Same as capture-selector.md. No network; models ship with the app or are installed once on user request.

## Platform notes
- Windows: Windows.Media.Ocr (Windows 10+) plus Windows.Graphics.Capture; QR through a bundled decoder.
- X11: Tesseract (bundled or system) with ZXing/zbar for QR.
- KDE Wayland: same engines on the portal frame.
- GNOME Wayland: same engines on the portal frame; selection overlay as in capture-selector.md.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
