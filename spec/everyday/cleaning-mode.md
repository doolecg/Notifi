# Cleaning Mode

Lock keyboard (and pointer) input while wiping the keyboard or screen, with a black screen or a small corner indicator.

## User-visible behaviour
- Starts from the menu, quick panel, radial menu or its shortcut. All input is blocked: keys, mouse buttons, and the special keys for brightness, media, volume and lock.
- Display options: black out every display (default), or leave the picture visible and show only a small marker in a corner, so you can wipe the display while still seeing it.
- Unlock: click the Unlock button, or press Escape five times; the five presses must fall within a 6 second window and the count resets if a modifier key is pressed.
- Ends if the login session leaves the screen (fast user switch, lock), so the input lock never reaches another user.
- If a mouse button is still down when unlocking, closing waits until it is released so that no stray click gets through.
- Middle-click fixes and key debounce stay suspended during the mode and resume on exit; input hooks stopped for permission cleanup or shutdown do not restart.
- Display changes (plug, unplug, resolution) while active resize the cover without leaving gaps.

## Settings
- Display mode: blackout or indicator (default blackout)
- Unlock key count (default 5) and time window (default 6 s)
- Allow pointer movement in indicator mode (default off)
- Show a countdown or elapsed time (default off)

## Default shortcuts
Start: Ctrl+Alt+Shift+K (Notifi design, not from source docs). Unlock: Esc five times. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Ctrl+Alt+Delete on Windows cannot be blocked from user mode; documented and harmless.
- If the app crashes, the OS releases the grab; no stuck lock.
- External keyboards and mouse are locked too.
- Cannot start during a recording without a warning.

## Permissions
Windows: low-level keyboard/mouse hooks, no prompt. X11: XGrabKeyboard/XGrabPointer. Wayland: needs an evdev grab (input group/helper) or compositor support; otherwise limited (PARTIAL).

## Platform notes
- Windows: WH_KEYBOARD_LL and WH_MOUSE_LL plus topmost cover windows.
- X11: XGrabKeyboard and XGrabPointer with override-redirect covers.
- KDE Wayland: evdev grab through a small helper, since clients cannot grab input.
- GNOME Wayland: same evdev helper, with the shipped extension drawing the cover (PARTIAL).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
