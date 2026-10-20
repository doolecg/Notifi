# Cleaner

Remove caches, logs and leftovers manually or on a schedule, with a scan you can review and cancel first.

## User-visible behaviour
- Scan lists categories with sizes and a tick box each: user and system temp files, application caches, logs, crash dumps, thumbnail caches, package-manager caches, browser caches (not history or logins), recycle bin or trash, and orphaned app leftovers.
- Scanning can be cancelled at any time; partial results are still shown. "Clean" moves/deletes only ticked items and reports the space recovered (counted once, not per overlapping category).
- Items that could not be removed are named, with the reason (in use, protected, no rights); a failed run is reported, never shown as a success tick.
- Leftover detection is conservative: only paths verifiably owned by an uninstalled app (matching vendor/product identifiers) are listed. Preferences, config and data of apps that are still installed are never offered. Shared components, nested app data, version folders and links are refused as ambiguous.
- Installed customisation and localisation folders (themes, language packs, user plug-ins) are preserved in both manual and scheduled cleaning.
- Forgotten screenshots can be listed as an unchecked category, governed by an age setting; screenshots the user has renamed are not treated as untouched.
- The Clean button stays reachable in compact layouts (tray panel, top pill, quick panel).
- Automatic cleanup runs on a schedule: daily or weekly (weekday + time), only with ticked categories, skipping protected files, and showing a notification if it could not finish.
- The messaging-downloads tools are a separate feature (see messaging-downloads.md) and stay hidden here until switched on.
- Each category explains why it is safe: removing it breaks nothing, though apps may start a little slower once and re-download things such as offline media.
- Files edited recently wait out the full age period again before they are offered. Items found only by heuristic analysis are listed unticked, with a prompt to check the path before ticking.
- A more thorough scan is available after granting broader file access (on Windows, running the scan elevated; on Linux, polkit approval); without it the scan covers per-user locations only.

## Settings
- Categories included: caches, logs, temp on; recycle bin, screenshots, leftovers off (unchecked by default).
- Automatic cleanup: off. When on: weekly, Sunday 03:00; daily option.
- Screenshot age threshold: 30 days.
- Notify after automatic run: on.

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Files in use are skipped and listed, not forced.
- Never touches files newer than a few minutes, system directories, or anything outside the known category roots.
- If the machine is asleep or on very low battery at the scheduled time, the run waits for the next opportunity.
- Cancelling mid-clean leaves already removed items removed and reports the remainder.

## Permissions
- Standard user rights for per-user locations. System-wide caches and logs need administrator / polkit approval, requested only when those categories are ticked.

## Platform notes
- Windows: %TEMP%, Windows Update download cache, Prefetch (admin), WER crash reports, thumbnail and icon caches, Recycle Bin, package caches (winget, Scoop, npm).
- X11: ~/.cache, ~/.local/share/Trash, thumbnails, journald vacuum, apt/dnf/pacman package cache, Flatpak unused runtimes.
- KDE Wayland: same as X11, plus KDE-specific caches (file-indexer data excluded unless ticked).
- GNOME Wayland: same as X11, plus GNOME tracker caches excluded unless ticked.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, app UI strings (release build 3.4.1)
