# Bluetooth off during sleep

Turns Bluetooth off while the computer sleeps and back on at wake, only when Notifi was the one that switched it off.

## User-visible behaviour
- When the computer goes to sleep, Bluetooth is switched off, so a sleeping or closed laptop stops reconnecting to headphones, phones and other accessories that should be left alone.
- On wake, Bluetooth is switched back on only if Notifi was the one that turned it off. If it was already off before sleep it stays off.
- The state is remembered across the sleep, and across a crash or restart during it, so a restore that is owed is still carried out. Bluetooth power is only queried when a restore is owed.
- Headphones disconnect when sleep begins, if enabled.
- The setting is a single switch in the energy settings; no panel is needed.

## Settings
- Switch Bluetooth off during sleep: off.
- Also applies on lid close / lock only: no (sleep only).

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- If the user turns Bluetooth on themselves during sleep or after waking, Notifi does not turn it off again and does not claim ownership.
- Machines without a Bluetooth adapter hide the option.
- A failure to switch Bluetooth off or on is reported once and does not block sleep.
- Sleep triggered by Keep awake never happens, so nothing is changed during a Keep awake session.
- Wake from hibernate is treated the same as wake from sleep.

## Permissions
- Windows: none. Linux: BlueZ access over D-Bus as the logged-in user is normally allowed; if polkit blocks it, an authorisation prompt is shown once.

## Platform notes
- Windows: the Radios API (Windows.Devices.Radios) to set the Bluetooth radio state around the suspend and resume power events.
- X11: BlueZ adapter Powered property over D-Bus, driven by the logind PrepareForSleep signal.
- KDE Wayland: same as X11.
- GNOME Wayland: same as X11.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
