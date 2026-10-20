# Port manager

See which processes are bound to network ports, and end them.

## User-visible behaviour
- A live table of listening sockets: port, protocol (TCP/UDP), bound address, process name, PID and owning user. Sortable, searchable by port or process name, refreshed every few seconds.
- Listeners bound to every network interface (0.0.0.0 or ::) are visibly marked as reachable from other machines, unlike those bound to loopback only.
- Ports and PIDs are shown without thousands separators.
- Row actions: copy port, copy PID, copy address (host:port), and for TCP listeners open the address in the default browser.
- Stop: ends the owning process after a confirmation. A normal stop asks the process to exit first; Force kill ends it immediately. Where a Kill Process tool is installed, the stop action hands off to it, including restart and ending child processes.
- The exact process is revalidated just before ending it, so a recycled PID can never end a different program.
- The same Force kill action appears in app-usage lists in the monitor.

## Settings
- Auto-refresh interval: 3 seconds.
- Show UDP listeners: on.
- Show only listeners on all interfaces: off.
- Confirm before stopping: on (cannot be turned off for system processes).

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Processes owned by other users or the system need elevation; they are listed but Stop offers an elevation prompt.
- Sockets without a resolvable process (short-lived or kernel) show as unknown with Stop disabled.
- A port that disappears between refresh and action reports "already gone" rather than an error.
- Critical system processes are protected from Stop.

## Permissions
- Linux: seeing every user's PIDs needs elevation; without it the list shows the user's own processes. Windows lists all sockets without elevation, but ending protected processes needs administrator.

## Platform notes
- Windows: the IP helper TCP/UDP tables with owning PID; end via TerminateProcess.
- X11: /proc/net socket tables (ss-equivalent); SIGTERM then SIGKILL.
- KDE Wayland: same as X11.
- GNOME Wayland: same as X11.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
