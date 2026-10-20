# Fan control (beta)

Choose fixed fan speeds or temperature-driven curves, and see each fan's current RPM.

## User-visible behaviour
- Per fan, a live RPM readout with the current and target speed.
- Modes: System (the machine decides), continuous Manual (a slider from 0 to 100 percent), and Curve (an editable temperature-to-speed graph with draggable points and several sensors to choose from; adding points keeps all sensors).
- Fans that are stopped are prepared before taking control, and the app keeps retrying on hardware that lacks an unlock mechanism. Maximum cooling stays engaged when the system might otherwise reclaim control.
- Optional (off by default): reapply the most recent manual speed or curve when the machine restarts or wakes.
- Control requires a privileged helper, installed on request. If the machine has no controllable fan, the feature cannot be installed (including from first-run setup or "install all") and explains why. If the helper is unavailable, the fan speeds can still be read.
- The Top Pill's fan card opens this page (see island.md).
- The curve editor stops redrawing when its window closes, so CPU use drops.
- Uninstalling removes the helper, its stored data and caches completely.

## Settings
- Mode per fan (default System)
- Curve points per sensor
- Resume after restart or sleep (default off)
- Safety floor (minimum speed, default 20 percent) and maximum temperature override that returns to System mode (default 90 C)

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Helper crash or app quit: fans return to System control.
- Sensor missing while a curve uses it: fall back to System mode and notify.
- Rapid slider changes are rate-limited; updates apply during an active session without timeouts.
- Laptops with EC-locked fans: read-only, with a clear explanation.

## Permissions
Administrator/root required once to install the helper; no prompts after that. Writing fan speeds can void warranties or damage hardware; the first-run page says so.

## Platform notes
- Windows: vendor EC or SMBus drivers where available (PARTIAL; many laptops read-only); helper runs as a service.
- X11: hwmon pwm files through a small root helper/polkit action; fancontrol-style fan-to-sensor mapping.
- KDE Wayland: same as X11.
- GNOME Wayland: same as X11.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
