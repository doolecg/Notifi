# Notifi 0.3.1

## Changed

- The installer now asks before it installs Microsoft WebView2 on a PC that doesn't have it. It says what will be downloaded, shows the disclaimer, and installs only if you choose Yes. No cancels setup.

## Fixed

- Setup could look frozen on a new PC: the WebView2 download ran hidden with no progress and no time limit. It now shows a progress line, gives up on the download after 2 minutes, and stops with an error if WebView2 is still missing afterwards.

---

# Notifi 0.3.0

## New

- Taskbar buttons for AI apps: with T3 Code, Claude, ChatGPT, Codex or a terminal running an agent in front, the bar shows Stop, Continue, Clear and Compact, which type into that app, beside your 5-hour and weekly limits.
- Claude plan limits with no login: read from T3 Code's own logs, or from Claude Code's status line for the `claude` command (one switch in Settings, Island).
- Disk space: delete to the Recycle Bin or for good, open files and folders in File Explorer, colour by size from green to orange, and a quicker parallel scan.
- Install on demand: windows and background work are only built for the features you installed. Settings keeps core pages in the sidebar and opens each feature from the Installed page.
- Scratchpad in the Nook: edit and delete notes, with undo.
- Notifications that belong to a feature open it when clicked. New songs are no longer listed.
- A click anywhere outside the open Island puts it away.
- Links in the AI chat open in your browser.
- A new Permissions page and data-use notes in About and the feature pages. The README is rewritten.

## Changed

- On a left-aligned taskbar the bar now stays in the taskbar and the menu opens above it.
- The Island opens on the Nook.
- Clipboard "Skip sensitive text" is on by default, and the page says history is stored unencrypted.
- The Claude plan-limits call to Anthropic is gone: Anthropic's terms keep Claude Code's login to its own apps.
- The Cleaner asks twice before it cleans.

## Fixed

- Settings no longer freezes when opened. The Island puts its menu away before it hides, an attempt at the freeze during Windows screen grabs, not yet confirmed on a real screen grab.
- Keybinds can be cleared again (Backspace while recording, or the Clear button).
- The Island has a Close button, and Escape handling on every taskbar layout.
- Security pass: updates are checked again just before they run, the command bar no longer runs a file name's `&` as a command, share uploads are limited to your recent captures, settings exports leave out the share token, installers and shortcuts can only be started for what was just installed, Disk space only opens files it scanned, image and video inputs are limited, `tokio` is updated (RUSTSEC-2026-0384), and the commands that delete or install can only be called from the Settings and Apps windows. Pages now run under a content security policy.

## Not in this release

- The App switcher (Alt+Tab replacement), Port manager and Processes tool are still held back until they work properly.

---

# Notifi 0.2.0

## New

- The Island: sits on the taskbar or at the top of the screen. It shows now playing, timer, AI agents, a watch page and a menu of modules.
- AI agents in the Island: working, done and needs-you status, Claude plan limits, token use, and sessions open.
- AI questions in the Island: when Claude Code asks a question or needs permission, the Island shows it with buttons and a box to type your own answer. Turn it on in Settings, Island.
- Screenshots, screen recording, radial menu, colour picker, media tools, camera preview, input tools, energy and display tools, cleaner and uninstaller.
- Command bar, clipboard, sound and energy pages, and a rebuilt settings window.

## Changed

- The Cleaner measures its categories side by side and shows each as it finishes.
- The Open button on tool pages now opens the tool.

## Fixed

- Nothing yet.

## Not in this release

- The App switcher (Alt+Tab replacement), Port manager and Processes tool are held back until they work properly. They stay on the `dev-0.2.0` branch.

---

# Notifi 0.1.0

First build: tray icon, settings page, global shortcuts.
