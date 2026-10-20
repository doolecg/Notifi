# App updates

A single list of installed apps with newer versions available; tick the ones you want, and updates handled by a package manager install as a group.

## User-visible behaviour
- A single page lists every app with a newer version found, whatever installed it: Microsoft Store, winget, Scoop, Chocolatey, PackageKit, Flatpak, Snap, AppImage, or a publisher's own feed.
- Each row shows the installed and available version, the source, and a tick box. "Update selected" runs every ticked package-manager update in one go; apps that cannot be updated silently have an "Open updater" action that launches the app's own updater so it stays in control.
- Sources are checked in layers: package managers first, then publisher feeds, then a public version catalogue queried in a privacy-preserving way (no list of installed apps leaves the machine in the clear).
- Partial results are kept. When a source fails or an app cannot be compared (no usable version number), the app appears in a "Versions not compared" section instead of being silently dropped, and a missing-source warning is cleared only when another source can verify the app.
- Per-app rules, from a right-click on a row (page or tray panel): skip this one version, or stop checking the app entirely. Skipped or ignored apps stay out of results and out of alerts. Every rule is listed under "Update rules" on the same page and can be undone there.
- Update exceptions: a settings list that excludes any app, including apps with no update showing at the moment.
- Download progress is shown as a real percentage in the panel banner. An update badge appears on the tray icon when updates exist, and an optional alert fires when new ones are found.
- Notifi's own update is never listed here (it has its own banner).

## Settings
- Check at startup and every 6 hours (default on, interval adjustable).
- Sources: package managers on, publisher feeds on, public catalogue on (each toggle).
- Update exceptions list: empty.
- Update rules (skipped versions, ignored apps): empty.
- Alert when updates are found: on.
- Include pre-release channels: off.

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Version comparison is tolerant: a leading "v" is ignored, letter case differences do not create an update, and pre-release numbering compares numerically (beta10 is newer than beta9, rc10 newer than rc9).
- Apps whose catalogue numbering differs from the installed one (vendor build numbers) must not produce false updates; they go to "Versions not compared".
- Store updates must match the right app (no matching by similar name alone).
- A stale package-manager index must not report an old version as new; refresh the index before comparing.
- Opening destinations (app pages, updaters) must not freeze the list.
- Downloads that exceed the release's expected size are aborted.
- Behind a proxy or mirror, package-manager commands and checks honour the user's exported proxy environment.

## Permissions
- Network access only. Installing system-wide packages may show a UAC / polkit prompt.

## Platform notes
- Windows: winget upgrade, Scoop and Chocolatey status, Store updates via the Store's own page; per-app publisher feeds.
- X11: PackageKit (apt/dnf/pacman via the distro backend), Flatpak, Snap; AppImage update info where embedded.
- KDE Wayland: same as X11; "Open updater" may launch Discover.
- GNOME Wayland: same as X11; "Open updater" may launch Software.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
