# Installer helper (Windows)

The Windows half of the Mac "disk image installer". The Mac feature opens a mounted disk image, copies the app into Applications, ejects the image and offers to delete the download. On Windows the closest thing is installing a downloaded program and tidying up. (Linux keeps the AppImage installer; see appimage-installer.md.)

## User-visible behaviour
- When a downloaded installer (.msi, .exe) or portable archive (.zip) appears in the downloads folder, Notifi offers to install it.
- .msi: run silently with the standard silent switches. .exe: run silently only when the program is known to support it (common installer frameworks); otherwise launch it normally and watch for it to finish.
- Portable .zip: unpack into a chosen apps folder (default a Notifi-managed Apps folder), create a Start menu shortcut and optionally a desktop one.
- Progress is shown while it copies and verifies; afterwards Notifi offers to move the download to the Recycle Bin and to reveal or launch the program. Both answers can be remembered.
- Installers cached by package managers (winget, Scoop caches) are ignored.
- Off by default; turn on in Features.

## Settings
- Enable (default off)
- Apps folder for portable archives
- Silent install allowed (default on)
- Remove the download afterwards: ask / always / never (default ask)
- Reveal the program afterwards (default ask)

## Default shortcuts
None.

## Edge cases
- Installers that need administrator rights trigger the normal elevation prompt; Notifi never bypasses it.
- A failed install keeps the download and shows the installer's exit code in plain words.
- Archives with several apps ask which to install.
- Unsigned installers show a warning before running.

## Permissions
Elevation only when the installer asks for it.

## Platform notes
- Windows: msiexec, installer switches, Start menu shortcut creation, Recycle Bin move.
- Linux: not applicable here; AppImage installer covers it.
- Status: Windows ADAPT, Linux ADAPT (AppImage).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
