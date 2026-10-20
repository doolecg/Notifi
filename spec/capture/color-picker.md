# Colour picker

Sample the colour under any screen pixel and copy it in HEX, RGB or HSL form, or as a snippet ready to paste into code.

## User-visible behaviour
- Mode in the capture selector, or its own shortcut. The magnifier shows a pixel grid; the ringed centre pixel is the exact one that will be copied, so what you see is what you get.
- Arrow keys move one pixel; clicking, Enter or C copies. Enter or Return also closes the picker after copying.
- Formats: HEX (with or without the leading #, option), RGB, HSL, and a code snippet for the platform in use: CSS colour, Qt QColor, WinUI/XAML brush, or a Swift/other snippet. Clicking a format chip changes which one is copied.
- A short history of recent colours with swatches; copied values in clipboard history also show a swatch.
- The Command Bar accepts a typed colour too (see command-bar.md).
- Works over video and over other apps' windows; samples what is on screen, not a colour-managed guess: the colour copied matches the sampled pixel.

## Settings
- Default format (default HEX)
- HEX without # (default off), uppercase HEX (default on)
- Keep history (default on, 20 entries)
- Close after copy (default on)

## Default shortcuts
Colour picker: Ctrl+Alt+Shift+C (Notifi design, not from source docs). In the selector: C or Enter copy, arrows nudge, Esc cancels. Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.

## Edge cases
- HDR or wide-gamut displays: values are sRGB-converted and the picker notes it.
- Pixel under overlay/magnifier is never sampled from Notifi's own overlay.
- Mixed-DPI setups: samples a physical pixel.
- Semi-transparent window effects (blur, acrylic): picks what is visible.

## Permissions
Same as capture-selector.md. Wayland: portal consent for screen content.

## Platform notes
- Windows: Windows.Graphics.Capture frame; native eyedropper not used so the magnifier is consistent.
- X11: XGetImage/XShm on the root window.
- KDE Wayland: portal frame sample; KDE's own colour picker portal may be used when present.
- GNOME Wayland: Screenshot portal PickColor where available, else portal frame (PARTIAL).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
