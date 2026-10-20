# Messaging downloads

Review, organise or move to the trash the media that messaging apps leave in your Downloads folder, using retention rules.

## User-visible behaviour
- A tool in the cleaner area (hidden until switched on) that finds files saved by messaging apps (WhatsApp, Telegram, Signal, Discord and similar desktop apps) in the user's downloads and the apps' media folders.
- Files are grouped by app and by kind (images, video, audio, documents, voice notes, stickers/GIFs), each with size and age, and can be reviewed before anything is touched.
- Retention rules decide what is eligible: keep items newer than N days, optionally keep files you opened or moved yourself, and optionally cap the total size per app.
- Actions: move matching files to the Recycle Bin / trash (never permanent delete), or organise them into dated subfolders per app instead of removing them.
- A preview of what the rules would remove is always shown first; the result reports files moved, space freed and anything that failed.
- Can run manually or on the cleaner's schedule; manual runs show the review list, scheduled runs apply the saved rules and notify.
- Duplicate detection compares a content fingerprint rather than names or sizes alone. After a file has been organised, the new copy is checked again before any remaining duplicate is removed, so nothing is lost if the copy failed.
- Only local file metadata is inspected to decide whether a file came from the messaging app; chats and file contents are never read.

## Settings
- Feature enabled: off (tools hidden until turned on; setups already using it keep it).
- Retention age: 30 days.
- Action: move to trash (alternative: organise into folders).
- Apps included: all detected.
- Size cap per app: off.
- Run on schedule: off.

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Only files inside the recognised messaging folders are considered; anything in the user's own subfolders is left alone.
- Files currently open or still downloading are skipped.
- Files moved to trash by Notifi can be restored from the Recycle Bin / trash until it is emptied.
- Apps that are not installed or have no media folder simply show no group.

## Permissions
- Read/write access to the user's Downloads and the apps' media folders (no elevation). Sandboxed packages (Flatpak, Store apps) may keep data where Notifi cannot read; such an app is listed as unreadable.

## Platform notes
- Windows: Downloads plus messaging apps' media folders under Documents and AppData; deletion goes to the Recycle Bin.
- X11: ~/Downloads and app media dirs, including Flatpak data under ~/.var/app; trash via the freedesktop trash spec.
- KDE Wayland: same as X11.
- GNOME Wayland: same as X11.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, app UI strings (release build 3.4.1)
