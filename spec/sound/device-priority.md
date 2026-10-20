# Audio device priority

Rank your outputs and microphones; Notifi switches to the highest-ranked device that is connected.

## User-visible behaviour
- Two ranked lists, one for outputs and one for microphones. Drag to reorder; a disabled device is skipped.
- When a device appears or disappears, Notifi makes the highest-ranked connected device the default.
- A manual choice by the user lasts until the next device change.
- Off until installed from Features.
- Works with the output switcher shortcut (cycling uses the selected devices) and the microphone tools' preferred input.

## Settings
- Enable (default off)
- Output ranking, microphone ranking (default order of first seen)
- Ignore virtual devices (default on)

## Default shortcuts
None.

## Edge cases
- Switching to the already active device reports success.
- Switching during a call is delayed a few seconds to avoid dropouts.
- Headphone disconnect still lowers the speaker volume if that option is on.

## Permissions
None.

## Platform notes
- Windows: default endpoint changes through the policy config interface; device events from the multimedia device enumerator.
- Linux: default sink and source through PipeWire/Pulse; device events from the server.
- Status: Windows PORT, Linux PORT.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
