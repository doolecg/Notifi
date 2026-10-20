# Volume, brightness and mute overlays

Small overlays and indicators that show a level when it changes. Mac Vorssaint shows them in the island or as an overlay; Notifi shows them in the Top Pill or a floating overlay.

## User-visible behaviour
- **Volume overlay:** shows level, mute state and the device icon (headphones, speakers) when volume keys or Notifi shortcuts change it. Volume keys move in finer steps; reaching 0 also mutes; coarse-step outputs still move; the overlay is hidden while headphones adjust their own level.
- **Brightness overlay:** shows the percentage after changes on the laptop screen and external monitors, with easing from level to level.
- **Keyboard backlight:** a slider in the panel and the Top Pill, with an icon that turns the light on and off, plus shortcuts to step it.
- **Mic mute indicator:** a red crossed-out microphone beside the tray icon while the microphone is muted (on by default; invisible until muted). In the Top Pill it appears as a short notice.
- **Accessory notices:** connect/disconnect notices for Bluetooth audio devices, with the right device icon and no misleading battery level.
- Each overlay can be turned off, leaving the system's own indicator in charge.

## Settings (defaults)
- Volume overlay: on; brightness overlay: off; keyboard light slider: on where supported
- Mic mute indicator in the tray: on
- Place: Top Pill or floating overlay

## Default shortcuts
Volume and brightness keys; mute key; optional custom brightness shortcuts for the main display or the display under the pointer.

## Edge cases
- After sleep, volume and mute start from the real level.
- Switching outputs must not show a wrong volume notice.
- If the output device drops its mute control immediately after the key is pressed, the mute key leaves the volume unchanged.

## Permissions
None on Windows; Linux Wayland may need the input group to see media keys.

## Platform notes
- Windows: media-key hook, endpoint volume, brightness through WMI/DDC.
- Linux: PipeWire/Pulse events, logind brightness and ddcutil, keyboard backlight through the sysfs LED class via logind.
- Status: Windows PORT, Linux PORT (keyboard backlight PARTIAL: hardware dependent).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
