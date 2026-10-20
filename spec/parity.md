# Vorssaint parity checklist

Every Vorssaint feature and notable sub-feature (from its public README and changelog) against the Notifi spec that covers it. Nothing is dropped. Statuses: PORT (works the same on that platform), PARTIAL (works with limits, see the notes), ADAPT (a Mac-only idea rebuilt as the closest Windows/Linux version). Spec paths are relative to spec/. Linux status is for KDE and GNOME, X11 and Wayland, with Wayland needing Notifi's script or extension where noted in the spec.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md (every version), docs/PERMISSIONS.md, docs/PRIVACY.md, docs/TROUBLESHOOTING.md, app UI strings (release build 3.4.1). Public documents only; no source code was read. The release build 3.4.1 was also inspected for app UI strings only (labels and descriptions; no code analysis); the extra rows are in the last section.

## Whole app

| Vorssaint feature | Notifi spec | Windows | Linux | Notes |
|---|---|---|---|---|
| Tray (menu bar) panel with tabs | panel.md | PORT | PARTIAL | GNOME needs the AppIndicator extension. |
| Panel section reorder, hide, compact layout, section navigation | panel.md, app/backup-language-appearance.md | PORT | PORT | |
| Settings window, grouped sidebar, search, back/forward, hide sidebar | settings.md | PORT | PORT | |
| Features hub: install/uninstall each feature, presets, never-turned-on | app/features-hub.md | PORT | PORT | |
| Onboarding, what brought you here, tour | app/onboarding-permissions.md | PORT | PORT | |
| Permissions page, unused-permission warning, floating guide | app/onboarding-permissions.md | PORT | PORT | Permissions differ (input group, portals, extensions). |
| Auto-update, beta channel, release-notes preview, What's New | app/updates-and-whats-new.md | PORT | PORT | |
| Feedback and bug reports, support, donation links | app/updates-and-whats-new.md, app/backup-language-appearance.md | PORT | PORT | |
| Settings backup export/import | app/backup-language-appearance.md | PORT | PORT | |
| Languages (more than a dozen), live switching | app/backup-language-appearance.md | PORT | PORT | |
| Light/dark override, Liquid Glass toggle | app/backup-language-appearance.md | ADAPT | ADAPT | Acrylic/blur toggle instead. |
| Reduced motion | motion.md, app/backup-language-appearance.md | PORT | PORT | |
| Advanced: clear permissions, uninstall completely | app/backup-language-appearance.md | PORT | PORT | |
| Custom menu bar icon, hide icon, icon recovery, launch at login | app/tray-icon.md | PORT | PARTIAL | Tray unavailable on some GNOME sessions. |
| Keyboard shortcuts page, takeover of OS shortcuts | input/shortcuts-manager.md | PORT | PORT | |
| Motion and animation style | motion.md | PORT | PORT | Owned by another file. |
| Private by default (no telemetry) | app/updates-and-whats-new.md | PORT | PORT | |

## Sound

| Vorssaint feature | Notifi spec | Windows | Linux | Notes |
|---|---|---|---|---|
| Volume mixer per app | sound/volume-mixer.md | PARTIAL | PORT | Boost past 100% on Windows needs an APO. |
| Boost, reset button, amber warning, perceived-loudness sliders | sound/volume-mixer.md | PARTIAL | PORT | |
| Favorites, pin, reorder, hide inactive apps | sound/volume-mixer.md | PORT | PORT | |
| Per-app output device | sound/per-app-output.md | PORT | PORT | |
| AirPlay speaker for one app | sound/per-app-output.md | ADAPT | ADAPT | Cast/network sinks: Windows Cast-to-device, Linux PipeWire RAOP/Cast modules. |
| Output switcher shortcut | sound/output-switcher.md | PORT | PORT | |
| Lower volume when headphones disconnect | sound/output-switcher.md | PORT | PORT | |
| Audio device priority list | sound/device-priority.md | PORT | PORT | |
| Microphone tools (preferred input, level) | sound/microphone-tools.md | PORT | PORT | |
| Mute all microphones, muted indicator | sound/microphone-tools.md, everyday/osd-overlays.md | PORT | PORT | |
| Mic volume in mixer | sound/volume-mixer.md | PORT | PORT | |
| Music app blocker | sound/music-app-blocker.md | ADAPT | ADAPT | Blocks a chosen player from starting on a media key. |
| Volume, brightness, keyboard backlight overlays | everyday/osd-overlays.md | PORT | PORT | Backlight hardware dependent. |

## System monitor

| Vorssaint feature | Notifi spec | Windows | Linux | Notes |
|---|---|---|---|---|
| CPU, memory, battery, power, uptime | monitor/system-stats.md | PORT | PORT | |
| Per-core CPU load | monitor/system-stats.md | PORT | PORT | |
| Memory pressure, swap, compressed and cached memory | monitor/system-stats.md | PARTIAL | PORT | Windows has no pressure level; derived from commit/available. |
| History graphs, graph scale | monitor/system-stats.md | PORT | PORT | |
| Per-app resource breakdown, energy-hungry apps | monitor/system-stats.md | PORT | PORT | |
| Battery health, cycles, adapter vs battery power, time remaining | monitor/system-stats.md | PORT | PORT | |
| GPU | monitor/gpu.md | PARTIAL | PARTIAL | Vendor tools needed for some data. |
| Temperatures | monitor/temperatures.md | PARTIAL | PORT | Windows reads LibreHardwareMonitor data or ACPI fallback; no driver shipped. |
| Fan control (beta), curves, resume after sleep | monitor/fan-control.md | PARTIAL | PARTIAL | Windows needs the user's own driver; Linux needs the helper and hwmon PWM. |
| Menu bar (tray) readouts, bars or values, compact spacing, separate items | monitor/tray-readouts.md | PARTIAL | PARTIAL | Windows draws into icons; GNOME needs the extension. |
| Hide app icon when readouts show | monitor/tray-readouts.md, app/tray-icon.md | PORT | PORT | |
| Temperature unit, Celsius/Fahrenheit | monitor/temperatures.md | PORT | PORT | |
| Network speed, totals, local IP, bits or bytes | monitor/network-speedtest.md | PORT | PORT | |
| Per-app network traffic | monitor/disks-and-devices.md, monitor/network-speedtest.md | PARTIAL | PARTIAL | Windows ETW/netstat counters, Linux nethogs-style; limited per-app accuracy. |
| Speed test | monitor/network-speedtest.md | PORT | PORT | Open endpoint. |
| Alerts (CPU, temperature, memory, disk, battery), 2-minute interval | monitor/alerts.md | PORT | PORT | |
| Disks: usage, activity, SMART, eject, eject all | monitor/disks-and-devices.md | PORT | PORT | |
| Connected devices, USB count, accessory batteries | monitor/disks-and-devices.md | PORT | PORT | |
| Disk readout in tray (used/free) | monitor/tray-readouts.md, monitor/disks-and-devices.md | PORT | PORT | |

## Windows and the taskbar

| Vorssaint feature | Notifi spec | Windows | Linux | Notes |
|---|---|---|---|---|
| App switcher with live previews, search | windows/app-switcher.md | PARTIAL | PARTIAL | |
| Large-icon mode, simple (no capture) mode, columns, size, delay, tabs | windows/switcher-options.md | PORT | PARTIAL | |
| Browser tabs as entries | windows/switcher-options.md | PARTIAL | PARTIAL | Needs an extension or remote debugging. |
| Window layouts, snapping, shortcuts | windows/window-layout.md | PORT | PORT | |
| Snap areas, modifier drag, pointer layouts, grid cells, ignore apps | windows/layout-options.md | PORT | PORT | |
| Maximize without a new Space | windows/fill-screen.md | ADAPT | ADAPT | Fill screen toggle. |
| Fixed Space order | windows/virtual-desktops.md | ADAPT | ADAPT | Numbered desktop shortcuts and names. |
| Dock Preview | windows/taskbar-preview.md | PARTIAL | PARTIAL | Full preview with switch, close, move, snap. |
| Dock clicks (minimise, hide, cycle) | windows/taskbar-clicks.md | ADAPT | ADAPT | |
| Quit on close | windows/quit-on-close.md | PORT | PORT | |
| Quit and close protection | windows/quit-protection.md | PORT | PARTIAL | |
| Focus follows mouse (with and without raising) | input/focus-follows-mouse.md | PORT | PORT | |

## Keyboard and mouse

| Vorssaint feature | Notifi spec | Windows | Linux | Notes |
|---|---|---|---|---|
| Text snippets, variables, date builder, snippet menu | input/text-snippets.md | PORT | PORT | |
| Smooth scrolling, coast | input/smooth-scrolling.md | PORT | PORT | |
| Linear scrolling | input/linear-scrolling.md | PORT | PORT | |
| Pointer acceleration off | input/pointer-acceleration.md | PORT | PORT | |
| Scroll direction (mouse only) | input/scroll-direction.md | PORT | PORT | |
| Sideways scroll while holding a key | input/sideways-scroll-key.md | PORT | PORT | |
| Side buttons back/forward | input/side-buttons.md | ADAPT | ADAPT | Native; folded into remapping. |
| Mouse button shortcuts, side wheels, gestures, radial trigger | input/button-shortcuts-gestures.md | PORT | PORT | |
| Middle click from touchpad (3 or 4 finger) | input/middle-click-touchpad.md | ADAPT | ADAPT | OS touchpad setting. |
| Apps to leave alone | input/apps-to-leave-alone.md | PORT | PORT | |
| Extra click filter | input/click-filter.md | PORT | PORT | |
| Key debounce | input/key-debounce.md | PORT | PORT | |
| Super key, right-side modifiers, tap action, input-source switch | input/super-key.md | PORT | PORT | |
| Keyboard shortcuts manager | input/shortcuts-manager.md | PORT | PORT | |

## Clipboard, files and links

| Vorssaint feature | Notifi spec | Windows | Linux | Notes |
|---|---|---|---|---|
| Clipboard history | clipboard/clipboard-history.md | PORT | PARTIAL | |
| Retention, expiry, layouts, previews, filters, ignored apps | clipboard/history-options.md | PORT | PARTIAL | |
| Auto clear clipboard | clipboard/auto-clear.md | PORT | PORT | |
| Paste as plain text | clipboard/paste-plain-text.md | PORT | PORT | |
| Shelf, drop zone, pinned items, share | clipboard/shelf.md | PORT | PORT | |
| Finder shortcuts (cut/paste files, rename) | clipboard/file-manager-shortcuts.md | ADAPT | ADAPT | Native; only paste-image-as-PNG kept. |
| Clean URL | clipboard/clean-url.md | PORT | PORT | |
| Disk image installer | clipboard/installer-helper.md, clipboard/appimage-installer.md | ADAPT | ADAPT | Windows installer helper; Linux AppImage installer. |

## Everyday tools

| Vorssaint feature | Notifi spec | Windows | Linux | Notes |
|---|---|---|---|---|
| Dynamic Island | everyday/island.md, everyday/top-pill.md | PARTIAL | PARTIAL | Top Pill; no notch. |
| Island lyrics, queue, music bars, covers | everyday/island.md | PORT | PORT | MPRIS / SMTC. |
| Island notifications mirror | everyday/island.md | PARTIAL | PORT | Windows needs an app identity. |
| Island calendar, countdowns | everyday/island.md | PORT | PORT | ICS/CalDAV. |
| Island timers (timer, focus cycles, stopwatch) | everyday/island.md | PORT | PORT | |
| Island downloads | everyday/island.md | PORT | PORT | |
| Island on every display, follow pointer, hidden until hover | everyday/island.md | PORT | PORT | |
| Island on the Lock Screen | everyday/lock-screen.md | ADAPT | ADAPT | No drawing on the lock screen. |
| Companion | everyday/companion.md | PORT | PORT | |
| Watch any window | everyday/watch.md | PARTIAL | PARTIAL | |
| AI agents panel | everyday/ai-agents.md | PORT | PORT | |
| Command Bar | everyday/command-bar.md | PARTIAL | PARTIAL | App menu commands limited on Linux. |
| Quick panel | everyday/quick-panel.md | PORT | PORT | |
| Quick toggles | everyday/quick-toggles.md | PORT | PORT | |
| Radial menu, profiles, themes | everyday/radial-menu.md | PORT | PORT | |
| Scratchpad | everyday/scratchpad.md | PORT | PORT | |
| Cleaning Mode | everyday/cleaning-mode.md | PORT | PORT | |
| Camera preview | capture/camera-preview.md | PORT | PORT | |

## Capture and create

| Vorssaint feature | Notifi spec | Windows | Linux | Notes |
|---|---|---|---|---|
| Capture selector, magnifier | capture/capture-selector.md | PARTIAL | PARTIAL | Wayland portal picker. |
| Screenshot, preview, editor, pin | capture/screenshot-editor.md | PORT | PORT | |
| Watermark, spotlight, blur/erase, canvas, layers, background | capture/screenshot-editor.md | PORT | PORT | |
| Scrolling capture | capture/scrolling-capture.md | PARTIAL | PARTIAL | |
| Screen recording, tracks, zoom, blur, overlays, trim, speed | capture/screen-recording.md | PORT | PORT | |
| GIF export, compress to size | capture/screen-recording.md, capture/media-tools.md | PORT | PORT | |
| Expiring share links | capture/share-links.md | PARTIAL | PARTIAL | Self-hosted server only. |
| Recent captures | capture/recent-captures.md | PORT | PORT | |
| Copy text from screen, QR | capture/text-and-qr.md | PORT | PORT | |
| Color picker | capture/color-picker.md | PORT | PORT | |
| Media tools (compress, convert, watermark, GIF, text) | capture/media-tools.md | PORT | PORT | |

## App management

| Vorssaint feature | Notifi spec | Windows | Linux | Notes |
|---|---|---|---|---|
| App updates, update exceptions | apps/app-updates.md | PORT | PORT | |
| Cleaner (schedule, screenshots, iOS-backup-like items) | apps/cleaner.md | PORT | PORT | Mac-only backups map to old installers and caches. |
| Messaging downloads | apps/messaging-downloads.md | PORT | PORT | |
| Uninstaller and leftovers | apps/uninstaller.md | PORT | PORT | |
| Homebrew manager | apps/package-manager.md | PORT | PORT | winget/Scoop; PackageKit/Flatpak. |
| Port manager | apps/port-manager.md | PORT | PORT | |
| Kill process | apps/kill-process.md | PORT | PORT | |
| Wallpaper gallery | apps/wallpaper-gallery.md | PORT | PORT | |

## Energy and display

| Vorssaint feature | Notifi spec | Windows | Linux | Notes |
|---|---|---|---|---|
| Keep awake, triggers, modes | energy/keep-awake.md | PORT | PORT | |
| Keep awake with lid closed, dim built-in screen | energy/keep-awake-lid-closed.md | PORT | PORT | |
| Displays, per-display brightness and power | energy/displays-brightness.md | PORT | PORT | |
| Brightness key half/quarter steps, custom shortcuts, keyboard light | energy/displays-brightness.md | PORT | PORT | |
| Extra dimming, software dimming | energy/extra-dimming.md | PORT | PORT | |
| Extra brightness (HDR) | energy/extra-brightness.md | PARTIAL | PARTIAL | HDR displays; KDE only on Linux. |
| Bluetooth on sleep | energy/bluetooth-on-sleep.md | PORT | PORT | |

## Added from the release build's UI strings (3.4.1)

| Vorssaint feature | Notifi spec | Windows | Linux | Notes |
|---|---|---|---|---|
| Haptic feedback on touchpad taps | app/haptics.md | PARTIAL | PARTIAL | Hardware dependent; effectively absent on Linux. |
| Shelf shake gesture, edge peek, shake/drop-zone app exclusions | clipboard/shelf.md | PORT | PORT | |
| Optimise-media drop on the island | everyday/top-pill.md, capture/media-tools.md | PORT | PORT | |
| Hide the original notification while the island shows it | everyday/top-pill.md | PARTIAL | PARTIAL | Only where the OS lets a listener dismiss banners. |
| Hide timer countdown; per-content closed/expanded display | everyday/top-pill.md | PORT | PORT | |
| Fingerprint-based duplicate check in messaging downloads | apps/messaging-downloads.md | PORT | PORT | |
| Pointer layouts: nearest edge/corner start, right-button resize | windows/layout-options.md | PORT | PORT | |
| Restart now to unload uninstalled features | app/features-hub.md | PORT | PORT | |
| AI agents: read/written token split, minimum price, daily budget, limits after next reply | everyday/ai-agents.md | PORT | PORT | |
| Feedback form: optional contact, rate-limit message | app/updates-and-whats-new.md | PORT | PORT | |
| Command Bar: draggable position, recentre, no-suggestions mode | everyday/command-bar.md | PORT | PORT | |
| Previous/next display, display on/off, keyboard backlight step shortcuts | energy/displays-brightness.md | PORT | PORT | |
| GIF loop, smoothness, size; keep original modified date | capture/media-tools.md | PORT | PORT | |
| Save captures at 1x on high-density displays | capture/screenshot-editor.md | PORT | PORT | |
| Cleaner/uninstaller: analysis-found items unticked, deeper scan with broader access | apps/cleaner.md, apps/uninstaller.md | PORT | PORT | Elevation instead of Full Disk Access. |
| Per-core bars by logical core, average, grouped helper processes | monitor/system-stats.md | PORT | PORT | |
