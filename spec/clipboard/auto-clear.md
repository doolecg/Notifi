# Auto clear clipboard

Clear the system clipboard after a delay, on sleep or on lock, while keeping saved history.

## User-visible behaviour
- Timer after each copy.
- Also on lock and sleep.
- Saved history is untouched.

## Settings
- Delay (default off; 60 s when on) (Notifi design, not from source docs)
- Clear on lock/sleep

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- A newer copy resets the timer.

## Permissions
None.

## Platform notes
- Windows: Timer and session events.
- X11: Same.
- KDE Wayland: Same.
- GNOME Wayland: Same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md
