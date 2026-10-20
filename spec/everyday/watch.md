# Watch (live activity from part of any window)

Pick a region of any window (a progress bar, a score, a percentage) and Notifi shows what it reads as a live activity in the Top Pill, and alerts you when it changes or equals a value or phrase you choose. Everything stays on the computer.

## User-visible behaviour
- "Pick area": drag a rectangle on any window. Notifi reads it repeatedly with on-device text recognition and shows the value in the closed pill.
- Notify when the text changes or when it matches a value you set (equals, above, below, contains).
- Reads full percentages including values above 999 and negative signs.
- Watching pauses while the display sleeps or the screen is locked, and a late area selection never resumes a stopped watch.

## Settings
- Enable (default off); watched area; check interval (default 2 s); rules; show in pill

## Default shortcuts
None.

## Edge cases
- If the source window moves or closes the watch pauses and says so.

## Permissions
Screen capture (portal prompt on Wayland).

## Platform notes
- Windows: Graphics Capture of the window region plus Tesseract OCR. Linux: portal capture plus Tesseract.
- Status: Windows PARTIAL, Linux PARTIAL (window-relative regions depend on window lists on Wayland).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
