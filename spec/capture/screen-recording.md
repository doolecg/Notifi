# Screen recording

Record an area, window or screen with separate system and microphone audio, then trim, cut, zoom, blur, overlay and export as video or GIF.

## User-visible behaviour
- Source: area, window or whole display. A window recording captures only that app's sound; if a browser starts a video mid-recording, the recording still keeps only the chosen app's audio.
- Audio: two independent switches, system sound and microphone, saved as separate tracks so they can be balanced or removed in the editor. Pausing keeps video, audio and cursor effects in sync, with no blank opening.
- Quality presets: Balanced keeps the full native resolution so text stays sharp. Output falls back from HEVC to H.264 when the size cannot be hardware encoded. Starting a recording never freezes the interface.
- Cursor and keyboard effects: cursor highlight and click rings, optional typing zoom.
- After stopping, the recording opens in the editor:
  - Filmstrip timeline: scrub frames and drag across it to select a range to cut; trim ends; split and delete segments.
  - Auto zoom: zooms follow clicks and typing; a zoom can also be placed by hand with a chosen focus point while the full frame is shown.
  - Timed blur: draw a blur or pixelate area and give it a start and end time; undone selections are dropped.
  - Picture overlays: add images at nine anchor positions with size and opacity, for the whole video or a time range; presets restore them independently of the original recording.
  - Speed: 0.25x to 4x with pitch-preserving audio.
  - Export: MP4 (H.264/HEVC), WebM, GIF; copy to clipboard; share link. GIF export works for any length and can be cancelled without leaving a partial file. Copy as GIF is available.
  - Compress to target size: video or GIF is re-encoded to fit a chosen number of megabytes.
- Saving never overwrites an existing file if export fails or is cancelled; incomplete edits and damaged pointer data are rejected with a message.

## Settings
- Default source (area), frame rate (default 30; 60 optional), quality (Balanced)
- System audio on/off (default on), microphone on/off (default off), microphone device
- Cursor effects, typing zoom (default on)
- Countdown before start (default 3 s)
- Save folder and name template; default export format MP4
- Hide Notifi UI from the recording (always)

## Default shortcuts
Record start/stop: Ctrl+Alt+Shift+R (Notifi design, not from source docs). Pause: Ctrl+Alt+Shift+P (Notifi design). Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.

## Edge cases
- Audio device format changes during recording must not make saving fail.
- Output devices with many channels (4 to 8): system audio must not come out quieter; downmix with proper gain.
- Stopping while typing, or cancelling during startup, releases audio devices exactly once.
- Screenshot, text and colour shortcuts act during a recording without stopping it (see capture-selector.md).
- Window recording while the window is minimised pauses video and notes the gap.

## Permissions
Windows: microphone privacy setting. X11: none beyond audio server access. Wayland: screen-share portal consent each session (may be remembered); microphone via PipeWire permission.

## Platform notes
- Windows: Windows.Graphics.Capture plus WASAPI loopback (per-process loopback for window audio) and capture; Media Foundation encode.
- X11: PipeWire or XShm with FFmpeg encode; per-app audio from PipeWire/Pulse monitors.
- KDE Wayland: ScreenCast portal; per-app audio via PipeWire stream selection.
- GNOME Wayland: ScreenCast portal; window recording limited to what the portal offers (PARTIAL).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/recorder-export-speed.md
