# Scrolling capture

Capture a whole scrolling page or list by taking frames while it scrolls and stitching them into one tall image.

## User-visible behaviour
- Started from the capture selector by choosing a window or area and then the scrolling option. A small banner explains what to do and shows progress; long hints or copied text are clamped to the screen instead of overflowing it.
- Scrolling can be driven three ways: Notifi sends wheel steps itself, the user scrolls with the wheel or keyboard, or the user drags the scrollbar. Capture follows whichever movement happens and responds quickly to each.
- Frames are matched by overlap and merged; repeated fixed footers and headers (sticky bars) are detected and kept only once, so they do not repeat down the image.
- Seams are blended so there are no transparent or doubled lines.
- Content already captured is kept if a later frame fails; the result ends at the last good frame rather than discarding everything.
- Stop with Esc, Enter or a Stop button. The result opens like any screenshot: preview, then the editor and the after-capture actions.
- Text-recognition and text-only redaction in the editor must cover the full stitched height.

## Settings
- Automatic scrolling vs manual (default automatic)
- Scroll speed / step size (default medium)
- Maximum height (default 30000 px) before it stops with a notice
- Hide the banner in the saved image (always on; banner is never part of the capture)

## Default shortcuts
None of its own; chosen from the selector. Esc or Enter stops (Notifi design, not from source docs). (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Pages with animations or lazy-loaded content: wait a short settle time after each step; a frame identical to the previous one ends the capture (end of content).
- Inertial/smooth scrolling: frames are taken after movement settles to avoid blur.
- Horizontal scrolling is not stitched; only vertical.
- Very long captures are written to disk in strips to avoid running out of memory.
- Capturing a window that moves, resizes or loses focus mid-capture ends the capture cleanly.

## Permissions
Windows: none. X11: none (synthetic wheel via XTest). Wayland: portal capture; injecting wheel events needs the compositor's remote-input portal or the user scrolls manually (PARTIAL).

## Platform notes
- Windows: Windows.Graphics.Capture on the window plus SendInput wheel events.
- X11: XShm window capture plus XTest wheel events.
- KDE Wayland: ScreenCast portal for frames, RemoteDesktop portal for wheel; otherwise manual scrolling.
- GNOME Wayland: same portals; automatic scrolling only after the RemoteDesktop consent, manual always works (PARTIAL).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
