# Extra dimming

Dim a monitor below its hardware minimum brightness.

## User-visible behaviour
- Offered for external monitors whose brightness can be read through hardware control. It is an opt-in setting, switchable from the Displays panel too.
- The brightness slider is split: the lower part dims the picture in software, below the monitor's hardware minimum; the rest uses real hardware brightness. Hardware brightness is first taken to its floor before the software dim deepens.
- Brightness keys and shortcuts move through both ranges as one scale, in the chosen step size.
- Dimming survives switching virtual desktops and is released cleanly when the feature is switched off, a display disconnects or Notifi quits.
- Settings shows which monitors support it and which fall back to plain software dimming.

## Settings
- Extra dimming: off, per display.
- Share of the slider used by the software range: a fixed lower portion (for example the bottom quarter).
- Never below a minimum floor so the screen cannot go black.

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- The dim layer must not cover system permission or administrator prompts or purchase confirmations; it steps aside for them.
- Opening settings or the panel must not alter displays already at their level.
- A monitor that is reconnected keeps its extra dimming value.
- HDR or night-colour modes may interact; the dim is applied on top and removed on disable.
- If the monitor's brightness cannot be read, the option is hidden rather than guessed.

## Permissions
- None beyond display brightness access. Overlay mode needs no special rights.

## Platform notes
- Windows: gamma-ramp reduction per monitor.
- X11: gamma via the colour-ramp (xrandr) path, per output.
- KDE Wayland: a click-through full-screen dimming overlay per monitor, since gamma is not available to clients.
- GNOME Wayland: same overlay approach; the overlay is hidden over lock screen and prompts.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
