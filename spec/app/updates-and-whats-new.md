# Updates and What's New

## User-visible behaviour
- Notifi checks its own GitHub releases at startup and every few hours, downloads in the background with a progress bar and checksum, and offers install on click or on quit.
- Optional beta channel for pre-releases.
- Before updating, release notes can be previewed (can be turned off). After updating, a What's New window shows what changed for the features you use.
- The tray icon shows a badge when an update is ready.
- Bug reports and feedback are sent from the General page or the Command Bar; any technical details attached are displayed first so you can review them.
- A support link.
- Feedback form: a bug report or feature idea goes straight to the maintainer. Contact details are optional; without them the sender is told no reply will come. Too many submissions from one network in a short time are refused with a message to try later, and a thank-you confirms delivery.

## Settings
- Check automatically (default on); beta channel (default off); preview notes before updating (default on); What's New after updating (default on)

## Default shortcuts
None.

## Edge cases
- No telemetry; the network is touched only by update checks and actions the user starts.

## Permissions
Network.

## Platform notes
- Windows: msi/installer swap. Linux: AppImage replace or package manager hint.
- Status: Windows PORT, Linux PORT.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PRIVACY.md, app UI strings (release build 3.4.1)
