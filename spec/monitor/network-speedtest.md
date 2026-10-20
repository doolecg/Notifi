# Network and speed test

Live upload and download, session totals, local IP address, recent per-app traffic and a built-in speed test.

## User-visible behaviour
- Network card: live download and upload rates with a history graph, session totals since launch, and the local IP address (copy on click). Rates move to the next unit at the boundary.
- Speed unit: bytes or bits per second, applied everywhere (card, tray, Top Pill).
- Per-app network: a recent list of apps by download and upload traffic.
- Speed test: run on demand; measures download, upload and latency against a public test server and shows a result with the server used. Server errors are reported plainly instead of a blank result. Results are kept in a short history.
- Readings recover after sleep or interface change without false spikes; Wi-Fi details (name, signal, link speed) are read without blocking the UI.
- Can feed the tray readout (see tray-readouts.md) and the Top Pill.

## Settings
- Speed unit (default bytes)
- Interface to follow (default: all active, aggregated)
- Per-app network on/off (default off)
- Speed test server (default automatic)
- Ask before running a speed test (default off)

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Virtual and VPN interfaces: avoid double counting; option to include or exclude them.
- Counter wrap or reset: drop that sample.
- Offline: card shows "no connection" and the speed test explains why it cannot run.
- Metered connections: warn before the speed test.

## Permissions
Windows: per-app traffic needs ETW or an elevated helper (PARTIAL; otherwise hidden). Linux: per-app traffic needs CAP_NET_ADMIN, a nethogs-style helper or eBPF; without it the list is hidden.

## Platform notes
- Windows: GetIfTable2/IP Helper counters; per-process via ETW network provider.
- X11: /proc/net/dev interface counters; per-process via helper.
- KDE Wayland: same as X11; Wi-Fi via NetworkManager D-Bus.
- GNOME Wayland: same as X11.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/TROUBLESHOOTING.md
