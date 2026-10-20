# Super key

Turn Caps Lock or a right-side modifier into a combination of modifiers, with a separate action for a quick tap and app exclusions.

## User-visible behaviour
- Pick the source key (Caps Lock, right Alt, right Ctrl, right Shift).
- Held: acts as a chosen modifier combination (e.g. Ctrl+Alt+Shift+Win).
- Tap alone: sends a chosen key (default Escape) or switches input language.
- Held modifiers are preserved; the source key is restored if Notifi crashes.
- Apps can be excluded.

## Settings
- Source key (default Caps Lock)
- Held combination
- Tap action
- Tap timeout (default 200 ms) (Notifi design, not from source docs)
- Apps to leave alone

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Caps Lock LED state is restored on exit.

## Permissions
Wayland: input group.

## Platform notes
- Windows: Keyboard hook.
- X11: XRecord.
- KDE Wayland: evdev + uinput.
- GNOME Wayland: Same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
