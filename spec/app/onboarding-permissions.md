# First run, permissions guide and tour

## User-visible behaviour
- **First run:** language, then the permissions Notifi's chosen features need (one at a time, with an explanation), a short tour of what is available, and a question "what brought you here?" whose answer applies a matching bundle.
- **Permissions page:** every OS permission or group (input group on Linux, screen capture, notifications, startup), plain words on what it does, which features use it, and a flag when something is granted but unused, with a shortcut to revoke.
- **Floating guide:** when a permission needs a trip to OS settings (such as joining the input group or enabling the GNOME extension), a small window shows the steps and notices by itself when it is done.
- **Tour:** a short tour opens once after updates, showing the new features with buttons to set up or try each one.
- **Troubleshooting link** for launch problems and missing previews.

## Settings
- Show tour after updates (default on)

## Default shortcuts
None.

## Edge cases
- Every permission is optional; features that lack theirs say so instead of failing.
- The permissions page never asks for more than the installed features need.

## Permissions
The page itself needs none.

## Platform notes
- Windows: mostly none; notifications and startup. Linux: input group, portals, extensions.
- Status: Windows PORT, Linux PORT.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
