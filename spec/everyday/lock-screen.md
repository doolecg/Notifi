# Lock screen activity

Vorssaint shows a music player and activities on the Mac Lock Screen. Windows and Linux do not let apps draw on the lock screen, so Notifi adapts this.

## User-visible behaviour
- Windows: the OS lock screen already shows media controls; Notifi shows nothing there. On lock, Notifi pauses its overlays and thumbnail captures; on unlock, the Top Pill returns and finished timers ring once.
- Optional lock and unlock sounds, and an optional short "locked" notice in the pill the moment before locking.
- Linux: same; GNOME and KDE lock screens already show media controls.
- Related actions (clipboard auto clear on lock, Bluetooth on sleep, Keep awake pause on lock) are handled in their own specs.

## Settings
- Lock and unlock sounds (default off); pause overlays on lock (default on)

## Default shortcuts
None.

## Edge cases
- Timers finishing while locked ring after unlock.

## Permissions
None (session lock events).

## Platform notes
- Windows: session change notifications. Linux: logind lock/unlock signals.
- Status: Windows ADAPT, Linux ADAPT (no drawing on the lock screen).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
