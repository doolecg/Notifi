# App switcher: extra modes and options

Sub-features of the app switcher (see app-switcher.md for the core behaviour). Each is its own option.

## User-visible behaviour
- **Large-icon mode:** a single icon for each app in a row, with previews of the highlighted app's windows shown above it so an individual window can still be picked. A separate shortcut moves between windows of the selected app.
- **Simple app mode:** a list of apps with window titles, no previews and no screen capture. Still restores minimised windows. Used automatically when capture is unavailable.
- **Replace the system switcher or use a second shortcut:** the system Alt+Tab can be left alone and Notifi's switcher opens on another combination. When replacing, Settings explains which system shortcuts are locked.
- **Window thumbnails size:** Normal, Large, Extra large, kept separate from the taskbar preview size; a minimal mode shows titles with small cards.
- **Wider preview layouts when there is room**, including two side by side; the number of columns can be limited.
- **Window controls on hover:** a close button on each card; middle-click closes the window; W closes the highlighted window and leaves the app; Q quits the highlighted app.
- **Apps without windows:** optionally listed after windowed apps, separated from them; per-app rules can keep an app window-only or include it when it has none.
- **Minimised and hidden windows:** an option shows or hides minimised windows and apps hidden to the tray; fullscreen windows on other desktops can be included.
- **Merge tabs:** where an app's tabs are real separate windows, a setting can treat them as one entry showing the active tab.
- **Browser tabs as entries:** each browser tab can be its own entry; choosing one focuses that tab. On Windows and Linux this needs a browser extension or the browser's remote debugging interface, so it is an optional PARTIAL add-on.
- **Display filter:** lists just the windows on the pointer's display, and does not open at all if that display is empty. Also current desktop only.
- **Appearance delay and instant selection:** a delay before the panel appears (quick Alt+Tab flips straight to the last window), and an option to skip highlight, scroll and resize animations.
- **Browse with wheel or touchpad** while the switcher is open, with consistent direction.
- **Hide the file manager** when it has no windows.
- **Pause thumbnails** for chosen apps and programs that are not apps (for example a game launcher's Java process), with its own list.
- **Secure-input note:** when another program is capturing the keyboard exclusively (password fields on some systems), the system switcher may keep answering; Settings says so.

## Settings (defaults)
- Layout: previews (default) / large icons / simple
- Thumbnail size: Normal; maximum columns: automatic
- Show minimised windows: on; show hidden-app windows: on; show apps without windows: off
- Merge tabs: off; browser tabs: off
- Appearance delay: 0; skip animations: off
- Current display only: off; current desktop only: off
- Pause-thumbnail apps: empty

## Default shortcuts
Alt+Tab (switcher); Alt+` (windows of the same app); inside the switcher: Tab / Shift+Tab, arrow keys, W, Q, Esc, Enter. (Cmd maps to Ctrl, Option to Alt.)

## Edge cases
- Typing and shortcuts stay responsive while the list is being collected from slow apps; busy apps still appear.
- A newly opened window is focused; windows that closed while hidden are dropped.
- Wine/Proton games: switching must not send a stray mouse press.
- Restoring the system shortcut after a crash is part of the startup check.

## Permissions
As the core switcher: none on Windows/X11; script or extension on Wayland.

## Platform notes
- Windows: keyboard hook, DWM thumbnails, UI Automation for tab-merged apps.
- X11: XComposite and EWMH. KDE Wayland: KWin script. GNOME Wayland: extension.
- Status: Windows PORT (browser tabs PARTIAL), Linux PARTIAL (thumbnails depend on the script/extension).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
