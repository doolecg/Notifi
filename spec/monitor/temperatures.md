# Temperatures

Shows CPU, GPU, disk and battery temperatures alongside the other readings.

## User-visible behaviour
- A temperatures block in the System card and optional tray readouts. Each reading names its sensor group (CPU package, hottest core, GPU, NVMe/SSD, battery).
- CPU temperature uses mapped processor sensors plus, for chips with many cores, the warmest ones, which lets alerts and fan curves respond early. A missing helper is distinguished from unsupported hardware, with a hint when a helper or permission is needed.
- Battery temperature can have its own alert threshold (off by default).
- Unit selectable (Celsius or Fahrenheit); limits and notifications use the chosen unit.
- Sensors are only polled while something shows them, and polling stops when the window closes.
- Readings feed fan curves (see fan-control.md) and alerts (see alerts.md).

## Settings
- Temperature unit (default Celsius)
- Which sensors appear (default: CPU, GPU, disk, battery where present)
- Prefer hottest core vs package value (default package)

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Implausible values (0, below ambient, above 150) are dropped.
- Several sensors with the same label are merged by taking the maximum.
- Laptops that only expose one thermal zone show just that one.
- Virtual machines: usually nothing available; section hidden.

## Permissions
Windows: CPU temperature needs a driver or WMI thermal zones; kernel-level access is optional and off by default (PARTIAL: many machines show only ACPI zones). Linux: hwmon/thermal files are world-readable on most systems; NVMe/drivetemp may need the module loaded.

## Platform notes
- Windows: ACPI thermal zones via WMI, NVMe via storage IOCTL SMART, GPU via vendor API; CPU die sensors only with an optional signed driver (PARTIAL).
- X11: /sys/class/hwmon and /sys/class/thermal (coretemp, k10temp, amdgpu, nvme, BAT).
- KDE Wayland: same as X11.
- GNOME Wayland: same as X11.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
