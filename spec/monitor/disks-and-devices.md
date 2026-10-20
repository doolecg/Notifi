# Disks, drives and connected devices

Storage and attached-device readings in the monitor.

## User-visible behaviour
- **Disks section:** each disk with storage used and free (decimal units), live read/write activity, session totals, and health details (S.M.A.R.T.) when the OS exposes them. Select a disk to see its details. Tray readout can show disk usage or activity, or used/free space instead of a percentage.
- **Eject:** every removable drive has its own eject button, plus Eject all for external drives; the system disk can never be ejected. Disk format (NTFS, exFAT, ext4) is shown next to the name.
- **Connected devices:** a list of connected external USB devices and Bluetooth accessories; the tray can show a count of external USB devices.
- **Accessory batteries:** battery level of keyboards, mice, trackpads and Bluetooth audio devices, updating within seconds of connect/disconnect, shown in the System tab and optionally in the tray readout; the Power card can list them even without the tray readout.
- **Per-app network and energy:** recent download/upload per app in the Network view; a list of apps using the most energy.

## Settings (defaults)
- Show disks: on; show S.M.A.R.T.: on; accessory batteries: off; connected devices card: off

## Default shortcuts
None.

## Edge cases
- Odd or missing S.M.A.R.T. values must not crash anything.
- Mirrored or duplicated devices appear once.

## Permissions
Windows: none for basics (S.M.A.R.T. may need administrator). Linux: udisks2 for eject, smartctl via the helper for S.M.A.R.T.

## Platform notes
- Windows: volume APIs, storage reliability counters, device notifications. Linux: udisks2, /sys/block, upower for peripheral batteries.
- Status: Windows PORT, Linux PORT.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
