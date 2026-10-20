# CPU, memory, battery and power

A live system monitor with CPU (including per core), memory, disk, battery health and power use, history graphs and a list of the apps using the most resources.

## User-visible behaviour
- A System card shows total CPU, memory (used, cached, compressed where the OS reports it, and a memory pressure indicator), disk, and temperatures when available.
- Per-core CPU: clicking the CPU row opens a view of every core's load, split by core type on CPUs that mix performance and efficiency cores. Option to show it by default.
- Memory: used/total, swap, cached files; optional pressure dot beside the Memory row.
- Disk: usage as a percentage, or as available or used space instead of a percentage; shows available space and physical used space (and reclaimable space where the OS exposes it). Live disk activity (read and write rates) is also shown. A disk selector picks which volume the readout follows, and it never traps vertical scrolling of the panel.
- Power card: battery charge, health and capacity, charge history graph, temperature, time remaining, plugged-in state (charging paused with the adapter connected still counts as plugged in), and accessory batteries (keyboard, mouse, trackpad, Bluetooth audio) listed even if the tray readout is off. Machines without a battery hide battery cards, rows, settings and alerts entirely.
- Top apps: processes grouped per app by CPU, memory, energy and recent per-app network traffic. Per-app CPU is normalised against active cores and grouped GPU/energy is capped at 100%.
- History graphs for CPU, memory, network and power, with sensible unit rollover (KB to MB at the boundary).
- Battery readings refresh promptly when power is plugged or unplugged, even with a long background interval. Sampling stops when the window closes.
- The per-core view shows one bar per logical core, with fill height showing current utilisation (not spare capacity), plus an average across all cores shown per core type where the CPU has them, and a total logical core count.
- Helper processes are grouped under the app that owns them in the top-apps list, with an option to turn the grouping off.

## Settings
- Update interval (default 2 s while open, 5 s in background)
- Per-core by default (default off)
- Disk readout as percent, available or used (default percent), disk to follow
- Memory pressure dot (default off)
- Temperature unit (default Celsius)
- Show accessory batteries (default on)

## Default shortcuts
Open monitor: Ctrl+Alt+Y (Notifi design, not from source docs). (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Odd or missing values from battery, disk health or accessories must be ignored, never crash or show nonsense.
- Network or power values after sleep may spike; discard the first sample.
- Hybrid CPUs: core types come from the OS topology; if unknown, show a flat list.
- Desktops without a battery or without per-process energy data: hide those parts.

## Permissions
Windows: none for basics; some process data needs admin for protected processes (shown as unknown). Linux: reads /proc and /sys; per-process network may need a helper with CAP_NET_ADMIN or eBPF, otherwise hidden.

## Platform notes
- Windows: PDH/performance counters, WMI/IOCTL for disk and battery, Energy estimation from ETW where possible.
- X11: /proc, /sys/class/power_supply, upower for accessories.
- KDE Wayland: same as X11.
- GNOME Wayland: same as X11.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/TROUBLESHOOTING.md, app UI strings (release build 3.4.1)
