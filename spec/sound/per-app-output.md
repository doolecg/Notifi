# Per-app output

Choose which output device each app plays through, for example music on speakers while a call uses the headset.

## User-visible behaviour
- Each app row in the mixer has an output menu listing available devices plus 'System default'.
- Choosing a device moves only that app's audio; other apps keep playing where they were.
- The choice is remembered per app and re-applied when the app or device returns.
- If the chosen device disappears, the app falls back to the default output and the row shows a note.

## Settings
- Remember device per app (default on)
- Reset all routes

## Default shortcuts
None by default (Notifi design, not from source docs). (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- A device unplugged mid-play must not leave the app silent.
- Some apps re-open their stream on change; a short gap is acceptable.

## Permissions
None beyond normal audio access.

## Platform notes
- Windows: Uses the undocumented per-app audio policy interface that Windows' own sound settings use (may break across Windows builds; hide the menu if the call fails).
- X11: Move the sink input to another sink.
- KDE Wayland: Same.
- GNOME Wayland: Same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md

## Output to a network speaker
- AirPlay-style speakers (Mac feature): Notifi routes one app to a network speaker where the OS exposes one. Windows: Cast to device (Miracast/DLNA) endpoints show as outputs when paired. Linux: PipeWire network sinks (RAOP/Cast modules) appear in the output menu. Status: ADAPT on both. (Notifi design, not from source docs)
