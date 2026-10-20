# Uninstaller and leftovers

Remove apps and review related caches, preferences and helpers before anything is deleted.

## User-visible behaviour
- Lists installed apps (searchable, with size and last-used date) that Notifi can actually remove; apps it cannot remove are not offered, and a refusal always says why (for example a component that lives inside another app, or a protected system package).
- Choosing an app runs a scan for related items: configuration, caches, logs, data folders, startup entries, services and helper programs. Only items verified through the app's identifiers or package ownership are pre-ticked; items found only by a similar name start unchecked.
- Every result can be opened in the file manager. The scan can be cancelled.
- Before removing, the app's background parts (services, tray helpers, scheduled tasks) are stopped. Removal then runs the app's own uninstaller or its package manager, and related items go to the Recycle Bin / trash.
- Apps owned by a package manager (winget, Scoop, apt/dnf, Flatpak, Snap) are recognised and removed through it so its records stay correct.
- Afterwards it reports what was removed, items it could not remove by name, and recovered space (counted once). If missing rights are the cause it offers to retry with elevated access.
- Once an app is removed, its Command Bar shortcut becomes available for a different app. Newly installed apps appear when the list reopens.
- Phone-platform apps (for example Android apps under WSL or Waydroid) can be removed where the platform allows it.
- Leftovers found only by analysis are listed unticked with a prompt to check the path first. A deeper scan needs broader file access, requested only when asked for; without it the scan stays within per-user locations.
- The installed app can be revealed in the file manager from its row.

## Settings
- Pre-tick verified leftovers: on.
- Show name-matched leftovers: on (unchecked).
- Move leftovers to trash rather than delete: on.
- Stop background parts before removal: on.

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Leftovers of apps still installed are not flagged; shared components, nested app data and version folders are refused as ambiguous. Short two-part vendor identifiers are matched correctly.
- A removal that fails midway is shown as a failure, not a success tick; sandboxed app data that could not be reached is listed.
- A stale confirmation (app changed since scanning) is rejected.

## Permissions
- Administrator / polkit for system-wide apps. Reading protected data folders may need extra access, offered at the point it blocks a removal.

## Platform notes
- Windows: uninstall registry entries, Store apps, AppData and ProgramData, services, scheduled tasks, startup entries.
- X11: apt/dnf/pacman, Flatpak (including data under ~/.var/app), Snap, AppImage files and their desktop entries.
- KDE Wayland: same as X11, plus KDE config files under ~/.config and ~/.local/share.
- GNOME Wayland: same as X11, plus GNOME extension and dconf leftovers.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md, app UI strings (release build 3.4.1)
