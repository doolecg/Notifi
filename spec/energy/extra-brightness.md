# Extra brightness (HDR headroom)

Adapts the Mac feature that uses an XDR display's HDR reserve to go beyond its normal maximum brightness. On Windows and Linux Notifi raises the brightness of ordinary (SDR) content on displays that are in HDR mode.

## User-visible behaviour
- A toggle in the Displays panel and in Settings turns extra brightness on and off, with an intensity slider.
- When on, SDR content (the desktop, browsers, documents) gets brighter than the OS slider's maximum on HDR-capable displays.
- Extra brightness steps aside during full-screen HDR video so the video is not blown out, and returns afterwards.
- It stays active while Notifi's own controls are used and is switched off before the app updates or a system prompt (such as an administrator prompt) could be hidden.
- Per-display: only displays that report HDR support get the switch; others show an explanation.

## Settings
- Enable (default off)
- Intensity (default 50%)
- Pause during HDR video (default on)
- Which displays (default all HDR displays)

## Default shortcuts
None.

## Edge cases
- If HDR is off in the OS, the page offers to open the OS HDR setting; Notifi does not switch HDR on by itself.
- Heavy use can raise power draw and heat; a note says so.
- Restores the previous SDR white level on quit, on crash recovery and when switched off.

## Permissions
None.

## Platform notes
- Windows: raise the display's SDR content brightness (SDR white level) through the display configuration API, restored afterwards.
- X11: not possible (no HDR); the toggle is hidden.
- KDE Wayland: HDR SDR brightness through KWin's per-output setting over D-Bus on Plasma versions that offer HDR.
- GNOME Wayland: not available until the compositor exposes it; the toggle is hidden.
- Status: Windows PARTIAL (HDR displays only), Linux PARTIAL (KDE HDR only).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
