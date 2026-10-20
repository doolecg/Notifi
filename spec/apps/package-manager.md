# Package manager

Search, install, update and remove packages without a terminal, through the package managers already on the machine.

## User-visible behaviour
- One panel with Search, Installed and Updates tabs, backed by winget and Scoop on Windows, and PackageKit, Flatpak and Snap on Linux.
- Search results are sorted by popularity where the source offers it. Each result has a direct Install button, also available from the tray panel and the top pill, and the row reflects the installed state as soon as the operation finishes.
- In the Installed list, dependencies sit beneath the package that requires them; a toggle returns to a plain flat list. With the grouping option on, pending dependency updates are also nested under their parent packages.
- Installed packages that have updates are marked, with a compact update count. Actions: update one, update all, refresh the manager's own index, uninstall.
- Section and filter headers show package counts. Rows can be selected by clicking the free space beside the name. Clicking outside the panel while browsing or searching closes it; it stays open during a confirmation or running job.
- Uninstall asks for confirmation and refuses a stale confirmation (the package changed since it was shown).
- A guided setup appears when no supported manager is found, offering to install one.
- Jobs show progress and can be cancelled. A job that hangs is noticed without locking up the panel, yet one that is simply slow to download or build is left to complete. After a partly finished run the lists refresh so no stale versions remain.
- Commands and checks honour the user's exported proxy and mirror environment variables.

## Settings
- Enabled managers: all detected.
- Group dependencies: on.
- Check for updates: every 6 hours.
- Show packages from third-party sources (buckets, remotes): on.

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Package manager warnings must not stop the installed list loading.
- Packages from another source still show their update on the row and update correctly.
- If two managers provide the same app, rows show the source so the right one is acted on.
- Two operations on one manager are queued, not run in parallel.

## Permissions
- System-wide installs trigger UAC or polkit. User-scope sources (Scoop, Flatpak --user) need none.

## Platform notes
- Windows: winget, Scoop, optional Chocolatey; Store apps listed read-only.
- X11: PackageKit via D-Bus (distro-neutral), Flatpak, Snap.
- KDE Wayland: same as X11; polkit prompts appear through the KDE agent.
- GNOME Wayland: same as X11; polkit prompts through the GNOME agent.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/TROUBLESHOOTING.md
