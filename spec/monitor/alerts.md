# Alerts

Notifications for long CPU spikes, heat, memory strain, a nearly full disk or a low battery.

## User-visible behaviour
- Each alert has an on/off switch (all off by default) and a limit: CPU load, CPU temperature, memory pressure, free disk space, low battery, and a battery temperature limit.
- Sustained rule: an alert only fires when the value stays past its limit for a set time, so brief spikes do not notify. The settings page explains when a limit triggers a notification and when short CPU bursts are ignored.
- Repeat interval: how often the same alert may fire again, as short as every 2 minutes, with longer choices up to a day.
- Temperature limits and messages use the selected unit.
- Notifications use the OS notification system and show the current value, the limit and, for CPU, the top app causing it. Clicking opens the monitor.
- The alerts controls live in Settings only (not duplicated in the panel).
- Machines without a battery or sensor do not offer those alerts.
- Alerts keep working with the monitor window closed, at the background interval.

## Settings
- Per alert: on/off (default off), limit (CPU 90 percent, CPU temperature 90 C, memory pressure high, disk 10 percent free, battery 15 percent, battery temperature 45 C), sustained time (default 60 s)
- Repeat interval (default 10 minutes, minimum 2)
- Quiet during Do Not Disturb / focus (default on)
- Sound on/off (default off)

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Stopped or disabled alerts never deliver queued notifications after restart.
- Sleep/wake: the sustained timer resets on wake.
- Plugged in and full: no low-battery alert; charging suppresses it.
- Duplicate alerts collapse into one notification per category.

## Permissions
OS notification permission. Linux: a notification daemon; Windows: app allowed in notification settings.

## Platform notes
- Windows: toast notifications through the Windows App SDK.
- X11: desktop notifications over D-Bus (org.freedesktop.Notifications).
- KDE Wayland: same over D-Bus; KDE honours do-not-disturb.
- GNOME Wayland: same over D-Bus.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
