# Quick toggles

One-tap switches for common system chores: dark mode, hide desktop icons, eject disks, empty the recycle bin or trash, lock the screen and more.

## User-visible behaviour
- Toggles appear as rows in the menu panel and quick panel and as radial-menu slices. Each shows its state and any side effect ("restarts the file manager").
- Included: appearance (light/dark), hide desktop icons, show hidden files, eject all disks (with a list of drives to leave mounted so backups stay connected), empty the recycle bin/trash, lock screen, mute microphone (all devices, see sound/microphone-tools.md), keyboard light on/off, keep awake with duration chips or an end time (the switch and its shortcut repeat the previous choice, and the panel says when battery protection blocks a session), do-not-disturb, night light where the OS has one, show file extensions.
- Mute microphone sits here and does not shift the panel when pressed.
- Order and visibility are editable; descriptions move to tooltips.
- State is read live, so toggling elsewhere updates the row.

## Settings
- Which toggles appear and their order (default: common set)
- Eject exceptions (default none)
- Confirm before emptying trash (default on)
- Keep awake default duration (default 1 hour)

## Default shortcuts
None by default; each toggle may be assigned one. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Eject with files in use: report which drive failed and why; never force.
- Appearance toggle respects automatic schedule: manual toggle pauses the schedule until next change.
- Toggles for features the OS lacks are not offered (for example night light on a plain X11 session).
- Lock screen with a pending recording or Cleaning Mode: warns first.

## Permissions
Windows: none for most; eject may need admin for system drives. Linux: udisks2 policy, logind lock, gsettings/KDE D-Bus access.

## Platform notes
- Windows: registry and Win32 APIs (dark mode, desktop icons, file extensions), SHEmptyRecycleBin, LockWorkStation.
- X11: gsettings/D-Bus, udisks2, freedesktop trash spec, logind.
- KDE Wayland: KDE D-Bus (plasma-apply-colorscheme, KGlobalAccel, KScreenLocker).
- GNOME Wayland: gsettings and org.gnome.ScreenSaver.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
