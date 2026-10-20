# Notifi spec index

Behaviour specs for each Notifi feature, written clean-room from Vorssaint's public documentation only.

## Clean-room rules

- Sources are limited to public non-code files: README, CHANGELOG, TRADEMARKS, SUPPORT, CONTRIBUTING (user-facing parts), docs/*.md, docs/assets images, vorssaint.com.
- Never open, fetch, list or search source code (.swift, .m, .h, .c, Package.swift, Sources/, build scripts, Tools/). Never clone the repo.
- Describe behaviour only, in our own words: no Vorssaint type or file names, identifiers or copied sentences; no verbatim UI text.
- Bullets marked (Notifi design, not from source docs) are our own decisions.
- Builders use only spec/ and docs/plan.html. Do not open github.com/vorssaint or any copy of its source.

## app

- [Features hub](app/features-hub.md): Install or uninstall each feature, bundles and presets.
- [First run, permissions and tour](app/onboarding-permissions.md): Onboarding, the permissions page and guide.
- [Updates and What's New](app/updates-and-whats-new.md): Auto-update, beta channel, release notes, feedback.
- [Backup, language, appearance, reset](app/backup-language-appearance.md): Settings export/import, languages, theme, advanced reset.
- [Tray icon and menu](app/tray-icon.md): Icon choices, indicators, recovery and launch at login.
- [Haptic feedback](app/haptics.md): Gentle touchpad taps when opening the Top Pill, switching sections or adjusting timers, where the hardware supports it.

## sound

- [Volume mixer](sound/volume-mixer.md): Set the volume of each app that is playing sound on its own, with the option to push quiet apps above 100%.
- [Per-app output](sound/per-app-output.md): Choose which output device each app plays through, for example music on speakers while a call uses the headset.
- [Output switcher](sound/output-switcher.md): Switch the default audio output with a shortcut and automatically drop the volume when headphones are unplugged.
- [Audio device priority](sound/device-priority.md): Ranked outputs and microphones; switch to the best connected one.
- [Microphone tools](sound/microphone-tools.md): Pick a preferred microphone, change its level where the hardware allows, and mute every microphone at once with one shortcut.
- [Music app blocker (adapted)](sound/music-app-blocker.md): Stops a chosen media app from launching when a media key (play/pause, next, previous) is pressed and nothing else is handling playback.

## monitor

- [CPU, memory, battery and power](monitor/system-stats.md): A live system monitor with CPU, memory, disk, battery health and power use, history graphs and a list of the apps using the most energy.
- [GPU monitor](monitor/gpu.md): Shows GPU load, memory use and, where available, temperature and power.
- [Temperatures](monitor/temperatures.md): Shows CPU, GPU, disk and battery temperatures alongside the other readings.
- [Fan control (beta)](monitor/fan-control.md): Choose fixed fan speeds or temperature-driven curves, and see each fan's current RPM.
- [Tray readouts](monitor/tray-readouts.md): Put selected figures, usage bars, remaining battery time or fan speed in the system tray.
- [Network and speed test](monitor/network-speedtest.md): Live upload and download, session totals, local IP address and a built-in speed test.
- [Disks, drives and devices](monitor/disks-and-devices.md): Disk usage, activity, SMART, eject, USB devices and accessory batteries.
- [Alerts](monitor/alerts.md): Notifications for long CPU spikes, heat, memory strain, a nearly full disk or a low battery.

## windows

- [App switcher with live previews](windows/app-switcher.md): A better Alt+Tab: switch between apps and individual windows with live thumbnails, search and per-display filters.
- [Window layouts and snapping](windows/window-layout.md): Arrange windows into layouts, send them to other displays and bring back earlier positions via shortcuts, screen-edge drags or modifier-drag.
- [Quit on close](windows/quit-on-close.md): Chosen apps exit completely once their final window is closed, instead of idling in the background.
- [Quit and close protection](windows/quit-protection.md): Stops accidental quit or close shortcuts: each app can require holding the keys, pressing twice or adding another modifier.
- [Taskbar preview (adapted)](windows/taskbar-preview.md): Hover a taskbar button for a large preview of the app's windows across all desktops, with switch, close, move and snap.
- [Taskbar clicks (adapted)](windows/taskbar-clicks.md): Clicking the active app's taskbar button minimises it, hides it or cycles its windows.
- [Fill screen (adapted)](windows/fill-screen.md): Maximise any window without a new desktop, with gaps, and restore on a second press.
- [Virtual desktops (adapted)](windows/virtual-desktops.md): Fixed numbered desktops: switch to or move a window to desktop N, names and an indicator.
- [App switcher options](windows/switcher-options.md): Large-icon and simple modes, tab merging, filters, delay and per-app rules.
- [Window layout options](windows/layout-options.md): Snap areas, modifier drag, pointer layouts, grid cells and gaps.

## input

- [Text snippets](input/text-snippets.md): Short triggers expand into longer text, with clipboard, date and time variables.
- [Smooth scrolling](input/smooth-scrolling.md): Makes a mouse wheel scroll with a fluid glide; speed, response and coasting are adjustable.
- [Linear scrolling](input/linear-scrolling.md): A wheel notch always moves the page by the same distance, whatever the spin speed.
- [Pointer acceleration](input/pointer-acceleration.md): Turn off mouse acceleration and get your previous setting back when switched off.
- [Focus follows mouse](input/focus-follows-mouse.md): After a configurable pause, the window beneath the pointer takes focus, with an option not to raise it.
- [Scroll direction (mouse only)](input/scroll-direction.md): Invert vertical and horizontal mouse wheel scrolling independently of the touchpad.
- [Scroll sideways while holding a key](input/sideways-scroll-key.md): Hold a chosen key and the vertical wheel scrolls horizontally.
- [Mouse buttons to shortcuts and drag gestures](input/button-shortcuts-gestures.md): Bind key shortcuts to extra mouse buttons and side wheels, or drag with a button held to trigger desktop switching and the overview.
- [Side buttons (adapted)](input/side-buttons.md): Back and Forward side buttons already work on Windows and Linux in most apps, so Notifi folds them into button remapping.
- [Middle click from touchpad (adapted)](input/middle-click-touchpad.md): Make a three-finger touchpad press a middle click.
- [Apps to leave alone](input/apps-to-leave-alone.md): Exclude chosen apps from mouse and keyboard enhancements.
- [Extra click filter](input/click-filter.md): Ignore accidental rapid double clicks from a worn mouse button without delaying normal clicks.
- [Key debounce](input/key-debounce.md): Suppresses duplicate letters produced by a keyboard whose switches are wearing out.
- [Super key](input/super-key.md): Turn Caps Lock or a right-side modifier into a combination of modifiers, with a separate action for a quick tap and app exclusions.
- [Shortcuts manager](input/shortcuts-manager.md): One page lists every Notifi shortcut, lets you rebind them, and warns when they clash with others.

## clipboard

- [Clipboard history](clipboard/clipboard-history.md): Search a local history of copied text, images and files, pin favourites, preview entries and paste them with shortcuts.
- [Auto clear clipboard](clipboard/auto-clear.md): Clear the system clipboard after a delay, on sleep or on lock, while keeping saved history.
- [Paste as plain text](clipboard/paste-plain-text.md): Paste copied content without formatting while the original clipboard stays intact.
- [Shelf](clipboard/shelf.md): A temporary holding area near the cursor for files, text and links picked up during a drag, to drop or share afterwards.
- [File manager shortcuts (adapted)](clipboard/file-manager-shortcuts.md): Only one Mac idea is kept: paste a copied image as a PNG file in Explorer, Dolphin and Nautilus.
- [Clean URL](clipboard/clean-url.md): Strips tracking parameters from links, either on request or automatically whenever a link is copied.
- [Clipboard history options](clipboard/history-options.md): Retention, expiry, layouts, previews, filters and ignored apps.
- [Installer helper (adapted, Windows)](clipboard/installer-helper.md): Install a downloaded .msi, .exe or portable .zip and clean up.
- [AppImage installer (adapted, Linux)](clipboard/appimage-installer.md): Install an AppImage into your applications folder.

## everyday

- [Command Bar](everyday/command-bar.md): One search field for apps, windows, files, clipboard, snippets, app menu commands, a calculator, unit conversion, emoji and saved scripts.
- [Quick panel](everyday/quick-panel.md): A floating palette of favourite tools opened with a shortcut.
- [Quick toggles](everyday/quick-toggles.md): Switch dark mode, hide desktop icons, eject disks, empty the recycle bin or trash, lock the screen and more.
- [Radial menu](everyday/radial-menu.md): A circular menu that opens around the pointer holding apps, files, shortcuts and tools, with profiles and nested menus you can customise.
- [Scratchpad](everyday/scratchpad.md): Notes that save themselves, organised in tabs, with Markdown preview and export, shown in a floating window or inside the Top Pill.
- [Cleaning Mode](everyday/cleaning-mode.md): Lock keyboard input while wiping the keyboard, with a black screen or a small corner indicator.
- [Top Pill (adapted from the island)](everyday/top-pill.md): An always-on-top bar at the top centre of the screen showing music, notifications, calendar, timers, downloads and quick controls.
- [Watch](everyday/watch.md): Follow a region of any window as a live activity, with alerts when it changes.
- [Companion](everyday/companion.md): A small character beside the Top Pill that reacts to activity.
- [Lock screen activity (adapted)](everyday/lock-screen.md): What Notifi does on lock and unlock, since apps cannot draw on the lock screen.
- [Volume, brightness and mute overlays](everyday/osd-overlays.md): Level overlays, keyboard light and muted-mic indicator.
- [AI agents panel](everyday/ai-agents.md): Follow coding assistants such as Claude Code, Codex, OpenCode and Copilot CLI: tokens, models, projects and a notice when a long task finishes.

## capture

- [Capture selector and magnifier](capture/capture-selector.md): One selector for screenshot, recording, text recognition and colour picking with a pixel magnifier.
- [Screenshot and editor](capture/screenshot-editor.md): Capture then annotate, crop, redact, add a background or watermark, and pin.
- [Scrolling capture](capture/scrolling-capture.md): Capture a whole scrolling page by stitching frames.
- [Screen recording](capture/screen-recording.md): Record area, window or screen with separate system and microphone audio, then trim, cut, zoom, blur and export video or GIF.
- [Copy text from screen and QR](capture/text-and-qr.md): Recognize text offline from any area, or read a QR code.
- [Colour picker](capture/color-picker.md): Copy a screen colour as HEX, RGB, HSL or CSS/Qt/WinUI snippet.
- [Camera preview](capture/camera-preview.md): See yourself on camera in a floating mirror or inside the Top Pill, without starting a call app.
- [Share links](capture/share-links.md): Expiring links through a self-hosted share server.
- [Recent captures](capture/recent-captures.md): History of recent screenshots and recordings.
- [Media tools](capture/media-tools.md): Shrink and edit videos, convert images in batches, stamp watermarks, create GIFs and pull out text, all on the device.

## apps

- [App updates](apps/app-updates.md): One list of apps with newer versions; install managed updates together.
- [Package manager](apps/package-manager.md): Search, install and remove packages without a terminal.
- [Uninstaller and leftovers](apps/uninstaller.md): Remove apps and review related caches, preferences and helpers first.
- [Cleaner](apps/cleaner.md): Remove caches, logs and leftovers manually or on a schedule.
- [Messaging downloads](apps/messaging-downloads.md): Review, organize or trash messaging downloads using retention rules.
- [Kill process](apps/kill-process.md): Search, restart or end processes and their children.
- [Wallpaper gallery](apps/wallpaper-gallery.md): Apply still wallpapers from a folder.
- [Port manager](apps/port-manager.md): See which processes are bound to network ports, and end them.

## energy

- [Keep awake](energy/keep-awake.md): Stop the computer sleeping for a set time, or whenever chosen apps, mains power or an external display are in use.
- [Keep awake with lid closed](energy/keep-awake-lid-closed.md): Keep running with the lid shut.
- [Displays and brightness](energy/displays-brightness.md): Control each display's brightness, with half and quarter steps.
- [Extra dimming](energy/extra-dimming.md): Dim below a monitor's minimum.
- [Extra brightness (HDR)](energy/extra-brightness.md): Raise SDR content brightness on HDR displays.
- [Bluetooth off during sleep](energy/bluetooth-on-sleep.md): Turns Bluetooth off while the computer sleeps and back on at wake, only when Notifi was the one that switched it off.

Also: panel.md, settings.md, everyday/island.md and motion.md (owned by another agent).

## Parity

- [Parity checklist](parity.md): every Vorssaint feature and sub-feature, the Notifi spec that covers it, and its Windows and Linux status.
