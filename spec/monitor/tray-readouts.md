# Tray readouts

Put selected figures, usage bars, remaining battery time or fan speed in the system tray.

## User-visible behaviour
- Pick any number of readings: CPU, memory, GPU, disk usage, live disk activity, network, battery (with time remaining), power, fan speed, temperature. Each can be its own tray item or grouped into one.
- Style per reading: numeric value or a small usage bar. Bars and values can be mixed. The memory bar can show pressure colour; the battery icon can be split into its own item and keeps its shape.
- Disk readout can show a percentage or available or used space.
- Network readout shows download and upload; upload can be placed above or below download (order option), and the unit is bytes or bits per second. Width stays steady so the item does not jitter.
- Compact spacing: a spacing choice between readouts (normal or compact) so many items fit. Text adapts to light and dark panels and wallpapers for legibility.
- Pinned readouts sit centred beside the app icon. The plain app icon can be hidden when readouts are shown; a way back to the menu is always kept (click the first readout).
- Values update at the monitor interval, no more than once a second; the readout recovers from OS-reported glitches without false spikes.
- Monitor uses little energy while showing live readouts; sampling stops when none are enabled.

## Settings
- Readings shown (default CPU and memory)
- Style per reading: value or bar (default value)
- Network speed unit: bytes or bits (default bytes); network order (default download first)
- Spacing: normal or compact (default normal)
- Hide the plain app icon when readouts show (default off)
- Update interval (default 2 s)

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Very small tray areas: truncate to the first reading; overflowing items fall back to the menu.
- Hidden tray overflow on Windows: readouts must be on the visible tray or they are not seen; the settings page says so.
- Values never shorten abruptly (fixed width with tabular digits).
- Desktops without battery or fans: those readings are not offered.

## Permissions
None.

## Platform notes
- Windows: the tray cannot show text, so the readout is drawn into the tray icon bitmap and refreshed (PARTIAL: one icon per reading; Windows 11 overflow rules apply).
- X11: AppIndicator or StatusNotifier with a text label, or a drawn icon.
- KDE Wayland: Plasma shows the text label; bars drawn into the icon.
- GNOME Wayland: needs the shipped extension to show a label; without it, drawn icon only (PARTIAL).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
