# Volume mixer

Set the volume of each app that is playing sound on its own, with the option to push quiet apps above 100%. Favourite apps can be pinned and reordered. A microphone fader sits beside the output.

## User-visible behaviour
- Lists every app currently producing or recently producing audio, each with an icon, name, slider and mute button.
- Per-app slider from 0% to 100%, with optional boost beyond 100% (up to a configurable cap).
- Sliders follow perceived loudness (a curve, not a straight linear gain), and saved levels remember how loud each app plays.
- Apps can be pinned as favourites and manually reordered; the custom order is saved.
- A master output slider and a microphone slider with mute and an editable percentage sit beside the per-app list, where the device supports them.
- Setting an app to 0% also mutes it. Setting the output to 0% mutes the output.
- Boost must keep outputs with many channels (4 to 8, e.g. HDMI TV or audio interface) at the same loudness as stereo outputs.

## Settings
- Boost cap (default 200%) (Notifi design, not from source docs)
- Show only active apps vs also recently active (default: active plus last 5 min) (Notifi design, not from source docs)
- Remember per-app levels (default on)
- Pinned apps list and custom order

## Default shortcuts
Open the mixer panel: Ctrl+Alt+M (Windows/Linux) (Notifi design, not from source docs). Cmd maps to Ctrl, Option maps to Alt. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- An app that exits and returns keeps its saved level.
- A device that reports an invalid volume must not crash the mixer; hide the control instead.
- Apps with several audio sessions (browser tabs) are combined into one row.

## Permissions
No special permission on Windows. Linux: access to the PipeWire/Pulse socket (normal for a desktop session). Boost past 100% on Windows needs a system-wide audio processing driver (APO) to be installed; without one, the slider stops at 100%.

## Platform notes
- Windows: Per-app volume through the audio session manager (per-session volume). Boost past 100% only when an APO such as Equalizer APO is installed (PARTIAL).
- X11: PipeWire/Pulse sink inputs; boost past 100% is built in.
- KDE Wayland: Same as X11 (audio is not desktop-specific).
- GNOME Wayland: Same as X11.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
