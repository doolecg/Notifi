# Media tools

Shrink and edit videos, convert images in batches, stamp watermarks, create GIFs and pull out text, all on the device.

## User-visible behaviour
- Drop files into the Media window (or onto the Top Pill, which asks whether to put them on the shelf or into the media tools). Everything runs on the device; nothing is uploaded.
- Video: compress, trim, change speed, extract audio, make a GIF. Compression can aim for a target file size in megabytes (video and GIF), choosing bitrate or frame rate and size automatically; it reports if the target is not reachable.
- Image converter: batch convert between PNG, JPEG, WebP, AVIF and others, and image to PDF (one page per image, in the order shown). Options: max side (profiles remember values up to 20,000 px without shrinking them), quality, strip metadata, watermark (text or image, position, opacity).
- Output naming uses a menu of placeholders that can be combined in the name template: original name, index, date, time, width, height, format, and similar. A batch can be saved into its own subfolder next to the sources or in a chosen folder.
- Saved profiles bundle the format, size, quality, watermark and naming.
- Extract text from an image or video frame (see text-and-qr.md).
- Progress per file, cancel, and results listed with reveal-in-folder buttons; originals are never overwritten unless asked.
- GIF export options: loop forever or play once, a smoothness setting (frame rate), and an output size; results can be copied as a GIF or saved as a file.
- Converted or compressed files can keep the original modified date instead of taking the time of conversion (default off).

## Settings
- Default output folder (default: next to source), subfolder for batches (default on)
- Name template (default original name + suffix), overwrite behaviour (default never)
- Default image format and quality, resize max side
- Video preset and target size
- Keep metadata (default off)
- Keep original modified date (default off), GIF loop (default on), GIF smoothness and size

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Failed or cancelled jobs remove partial files; GIF export cancel leaves no empty file.
- Large videos stay responsive (work on a background thread, progress throttled); video details are read without loading the whole file.
- Name collisions get a numeric suffix.
- Unsupported codecs show a clear message rather than a blank output.

## Permissions
None beyond file access to chosen folders. Sandboxed Linux packages need the file-chooser portal.

## Platform notes
- Windows: Media Foundation and WIC for basics, bundled FFmpeg for the rest.
- X11: bundled or system FFmpeg and libvips/ImageMagick equivalents.
- KDE Wayland: same; drop targets via Wayland DnD, folders via the portal.
- GNOME Wayland: same.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, app UI strings (release build 3.4.1)
