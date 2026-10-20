# Radial menu

A circular menu that opens around the pointer holding apps, files, shortcuts and tools, with profiles and nested menus you can customise.

## User-visible behaviour
- Profiles: any number of named wheels, each with its own slices, colour theme, keyboard shortcut and trigger, plus starter presets. Deleting a profile asks for confirmation.
- Triggers: a keyboard shortcut (hold or tap), a pointer button other than the main two (Back, Forward or any extra button; a button opens only one wheel), or a multi-finger touchpad tap where the OS leaves that gesture free. Holding an extra button highlights slices as the pointer moves and picks the one under it on release.
- Slice actions: open app, file or folder, website (icon fetched from the site's own origin only), run a shortcut or script, toggle a quick toggle (appearance, screen lock and others), Shelf, Cleaning Mode, Keep Awake, start/stop recording, Now Playing card, and a submenu that opens another ring.
- Now Playing card: shows the current track beside the wheel; clicking it brings the playing app forward, restoring its window if closed.
- Editor: an interactive wheel preview where slices are dragged to swap, clicked to configure, and submenus entered; a searchable icon catalogue filtered by what the system can draw; a warning shown if no trigger exists to open a profile.
- Conflicts: a profile refuses a shortcut already used by another profile or feature; other shortcut settings give a warning if the chosen combination is already taken by a radial profile. The shortcuts page lists the radial shortcuts and links back to the editor.

## Settings
- Profiles (default: one starter wheel), slices per ring (default 8), theme (default system accent)
- Trigger per profile (default Ctrl+Alt+R; extra mouse button optional)
- Hold-to-open vs tap-to-open (default hold), dead zone radius
- Show labels (default on), animations (follow reduce motion)

## Default shortcuts
Open: Ctrl+Alt+R (Notifi design, not from source docs). Esc or clicking outside closes. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- The wheel is kept inside the display when the pointer is near an edge.
- Trackpads or mice without extra buttons: explain that the keyboard shortcut is the way in.
- Account/session switch releases the mouse hook and closes any open wheel.
- Website icon downloads reject redirects, oversized files and unsafe dimensions.

## Permissions
Windows: low-level mouse hook, no prompt. X11: XInput2. Wayland: needs the compositor's shortcut portal; pointer buttons are limited (PARTIAL).

## Platform notes
- Windows: WH_MOUSE_LL and layered topmost window.
- X11: XInput2 raw events and an override-redirect window.
- KDE Wayland: global shortcuts via KGlobalAccel; mouse trigger through a KWin script (PARTIAL).
- GNOME Wayland: global shortcut via the shipped extension; mouse trigger limited (PARTIAL).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
