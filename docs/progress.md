# Notifi progress notes

Last saved: 2026-10-20 (after wave 4 and the UI test pass).

## Where things stand
- Branch `dev-0.2.0`, local only, nothing pushed. Version 0.2.0 in `package.json`.
- Run it: `npm run dev` (tauri dev) from the repo root. The Island is enabled and docked bottom-left in the user's settings at `%APPDATA%\io.notifi.app\settings.json` (`island.dock = bottomLeft`, `island.claudeUsageConnect = true`).
- Checks last run: `cargo test --workspace` app crate 453 passed + platform 46 passed; `npx svelte-check --tsconfig ui/tsconfig.json` 0 errors. The app launches on this code.

## Island (done and seen on screen)
- Docked bar sits flush on the taskbar's left, full taskbar height, rounded right end, colour sampled from the taskbar and made darker by `island.dockDarkness`.
- Pages on the bar, switched by scrolling over it: AI Agents, Timer, Music, Info.
- AI Agents pill: status (Running / Done / Needs you / Idle), Todo count, 5h and 7d limit pills, today's tokens, working-agent logos, session count, pixel field with change colours.
- Music pill: cover, title, artist, song-progress gradient, prev / play / next.
- Edge highlight, pop and pixel burst fire on any change.
- Clicking the bar opens a flyout above the taskbar. Grid of 15 modules opens when nothing is live. Extend button at the top of the flyout stretches it to 520 px.
- Modules working: Controls, Now Playing, Timer, AI Agents, Mixer, System, Tools, Calendar, Clipboard, Scratchpad, Files, Captures, Downloads, Camera, Notifications.
- Claude 5h and 7d limits come from Anthropic's usage endpoint using Claude Code's saved login (read per request, never stored).

## Wave 1 (written and compiled, not yet tried live)
- Input: click filter, key debounce, mouse buttons, scroll direction, smooth and linear scrolling, sideways scroll, pointer acceleration, Super key, touchpad settings link. Code in `src-tauri/src/input`.
- Energy: Keep awake options, displays, extra dimming, Bluetooth off at sleep, cleaning mode. Code in `src-tauri/src/energy` and `keepawake.rs`.
- Tools: colour picker (`src-tauri/src/picker`), media tools (`src-tauri/src/media_tools`), AI agent settings, tray tooltip for latest clipboard entry, update alerts.
- Try first: click filter, scroll direction, smooth scrolling, Keep awake triggers, colour picker.

## Wave 2 (written, compiled, tests pass, not yet tried live)
- Screenshot and text from screen: `src-tauri/src/capture`, UI in `ui/src/capture`. Area/window/screen, selector, scrolling capture, editor (pen, arrow, rectangle, blur, text), OCR via Windows.Media.Ocr, QR via `rqrr`.
- Screen recording: `src-tauri/src/recording`. DXGI capture, Media Foundation MP4 (H.264 + AAC) or GIF (`gif` crate), cursor effects, typing zoom.
- Radial menu: `src-tauri/src/radial`, plus Monitor, Command bar, App switcher (mergeTabs), Island lock notice/sounds, backup tests, cleaning mode suspends click filter and debounce.
- Island pill now shows "sessions" = terminals open (counted from running claude/codex/opencode programs) and "subagents" running; subagent logs no longer count as sessions, and a turn waiting on subagents no longer reads "Needs you".
- Not built: share links (expiry, recordingLimit), screenshot editor extras (spotlight, ellipse, crop, layers), WebM, post-recording editor, fan curves (no Windows API), app switcher other-desktop lists, browser tabs, Island pickArea/hideWhenIdle.

## Wave 3 (written, compiled, tests pass, not yet tried live)
- UI audit: cut-off fixes for scratchpad, shelf, switcher, taskbar preview, recording bar, colour picker, radial wheel editor, capture editor, apps window. `ui/src/lib/fill.css` gives a window's `#app` full height; import it only in windows that fill their size (not panel or command bar, which measure their content).
- Island menu is roomier when docked: 500x320 (compact), 620x420 (spacious), 600 px when stretched. `DOCK_TALL` in `island/layout.rs` and `TALL_H` in `Island.svelte` must match.
- Screenshot extras: editor tools (line, ellipse, highlighter, spotlight, step badges, crop, text-only blur), pin windows, recent captures window and list, PrintWindow window grabs, after-recording trimmer (GIF and MP4).
- Switcher: other-desktop, hidden (tray) and windowless apps (Windows 11 22H2+), taskbar-preview extras, Media tools shortcut, per-display extra dimming page, brightness slider in the tray panel.
- Island watch (pick a screen area, show its OCR text as a pill page named Watch).
- Still not built: browser tabs in the switcher (needs a browser extension), switcher badge for other-desktop windows, share links, select/move and eraser and layers in the editor, post-recording editor beyond trim.

## Wave 4 and UI test (written, compiled, tests pass; rendered in a stubbed browser, not tried live)
- Share links to the user's own server (`POST <server>/upload`, multipart, optional bearer token, JSON reply with `url`), shelf auto-add for captures, editor select/move/eraser/image layers/canvas padding.
- Recording editor (cut, speed, zoom, blur, text and arrow overlays), separate system and microphone tracks, floating camera preview window.
- Command bar menu search (UI Automation), monitor shortcut, touchpad settings button, switcher "Other desktop" and "No window" tags, Scoop dependency grouping.
- UI test fixed Trim, Camera, Switcher, Taskbar preview, Select overlay layouts. Tests: app crate 516 passed, platform 59, svelte-check 0 errors.
- Not built: WebM, browser tabs in the switcher, Island hideWhenIdle (mascot), fan control, keyboard backlight, SDR extra brightness, Bluetooth connect notices, recording share button and size-target re-encode.
- Never tried live: nearly everything from waves 1 to 4 beyond the Island and Settings window. Do the live pass when the user is not typing; do not send synthetic clicks while they are working.

## Known gaps
- Cannot be built on Windows: keyboard backlight, extra brightness (SDR white level), Bluetooth connect notices.
- Needs UI hookups: per-display extra dimming picks, combined brightness slider, cleaning mode start button (Tools tile exists), Media tools has no shortcut option.
- Cleaning mode does not yet suspend click filter and debounce.
- Linux: input, energy extras and picker are not available yet.
- Not yet seen on screen: click-to-jump on Done or Needs you, extend-button shrink, Mixer, Tools, Clipboard, Scratchpad, Files and Camera modules.
- Windows notifications from other apps are not shown (Windows needs package identity for that API).

## Next: wave 2
- Screenshot (about 21 dead options), screen recording, text from screen (OCR), radial menu.
- Dead options on Monitor, Command bar, App switcher, Island, General.
- Method: count options with no consumer in `src-tauri/src/catalog/*.rs`, give agents separate folders, build and test once per wave in the main session.

## Rules worth remembering
- New dependencies are allowed if open source with a permissive licence (MIT, Apache-2.0, BSD, ISC, MPL-2.0, Zlib). No GPL.
- Builder agents see only `spec/`, never Vorssaint source.
- `ui/src/island/Island.svelte` is large; edit with exact-string scripts and keep a copy (a bad write once emptied it).

## 0.2.0 release (2026-10-20)
- Released from `main` (tag `0.2.0`, commit `3a78ccf`, MSI attached, default branch is now `main`). `dev-0.2.0` is pushed and keeps everything.
- Held back from main, still on `dev-0.2.0`: App switcher (Alt+Tab replacement, user said it does not work), Port manager and Kill process (settings page, Ports tab), Processes tab. To release them, fix them on `dev-0.2.0`, then merge into `main`.
- Added this session: AI questions in the Island (Claude Code hooks, `src-tauri/src/ask/`, `ui/src/island/modules/Ask.svelte`; off by default, switch in Settings, Island; tested live with a fake question in both dock modes, not with a real Claude Code run), taller question menu with an expand button in both modes, Open buttons on tool pages (`src-tauri/src/tools.rs`).
- Stuck Cleaner and Uninstaller pages were slowness, not hangs: Uninstaller list 6 s, Cleaner 40 s (Temp files is 37 GB). Cleaner now measures in parallel and streams each category (`apps-clean://category`). Ports and the Alt+Tab complaint were not found to be broken in testing.
- Test trick: start `notifi.exe` with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`, then drive any window over CDP (no clicks on the user's screen). `open_settings_section` opens Settings from the Island window.
- Not done: real Claude Code run against the ask hooks; live pass of waves 1 to 4; Linux; items under Known gaps.

## Disk space (space map)
- New Disk space tab in the App tools window, feature id `spaceMap`: scanner `src-tauri/src/apps/space.rs`, UI `ui/src/apps/Space.svelte` and `space-layout.ts`, spec `spec/apps/space-map.md`. Clean-room from the public SpaceSniffer page only.
- Tried live: scan, nested treemap, filter, tags, text report, C: drive scan stays responsive. Not built: NTFS alternate streams, several views, saved filters, MFT reading.

## Island tab strip
- The round side buttons are gone. Both docked and top Islands now have a tab strip across the top of the open menu: Nook first, then your module buttons as tabs, Settings, Mixer and Keep awake as icons at the right, and the taller/shorter toggle.
- Open sizes are wider: top compact 760 x 280, spacious 900 x 360; docked compact 820 x 300, spacious 900 x 420.
- New Nook home: Now Playing, Calendar and Scratchpad side by side. It opens first when nothing is live in docked mode.
- Not changed: the button slot editor in Settings > Island still draws the old side slots. Tab order is left, then right, then bottom lists.

## AI Agents bar on the taskbar
- New switch Island > Behaviour > 'AI Agents bar on the taskbar' (agentsTaskbar). With the Island at the top, a separate bar for AI Agents sits at the taskbar's left; the top pill no longer shows agents. Ignored when the Island itself is docked bottom left.
- Clicking the bar jumps to the session when it is Done or Needs you, otherwise opens the Island on AI Agents.
- Running time now shows beside the status on the agents pill and on the bar.
- The other working-agent logo is no longer repeated next to the main one; its count reads 'N running'.
- Not done: the bar opens no flyout of its own.

## Dynamic notifications
- Spec: spec/apps/dynamic-notifications.md. Code: src-tauri/src/island/dynamic.rs.
- Island > Activity > 'Dynamic notifications': off by default. Reads Windows toasts (browser push included) with the UserNotificationListener, Windows only. Just Eat, Uber Eats, Uber and Deliveroo notifications that name a stage become one live card on the pill: service, stage, progress fill, minutes left. Other notifications can flash on the pill.
- Tried live with a test Just Eat toast: card showed 'On the way', 12 min, 70% fill. Windows did not ask for permission on this machine.
- Not built: signing in to the services, a browser extension, more than one card at a time, Linux.
- Also fixed a flaky Disk space test (parallel tests shared one temp folder).

## Quiet modes, alerts on the Island, deep click
- Do Not Disturb (Windows) and the new Privacy mode stop all of Notifi's own notifications: pill notices, alerts and the click. Privacy mode is in Island > Activity > Events and is a quick toggle (shortcut action 'Toggle: Privacy mode'). AI questions still show.
- Notifi alerts show on the Island instead of as Windows toasts while the Island is on screen (switch 'Alerts on the Island, not Windows toasts', on by default).
- One sound: a deep click made in code (src-tauri/src/island/click.rs). It replaces the toast ding and the lock/unlock beeps and plays on pill notifications ('Deep click sound', on).
- Notifications taken by a progress card are removed from the Windows list; the banner itself cannot be stopped.
- Checked live: privacy toggle holds back and releases pill notifications. Not checked: Do Not Disturb live, hearing the click, alerts routing, lock sounds.

## Past notifications in the Nook
- The Nook has a Notifications column (past notifications since Notifi started, newest first, up to 100, 3 lines each). The Nook now scrolls sideways: wheel over it, or drag a trackpad; lists inside it scroll up and down as before.
- Do Not Disturb still records notifications there but shows and sounds nothing. Privacy mode records nothing.
- Fixed: the Shelf 'Drop here' panel could not be hidden (it was shown with a raw Windows call, so the window toolkit ignored hide). Radial, switcher and taskbar preview windows use the same show call and the same hide call; not checked for the same fault.
- The list is kept in memory only, so it empties when Notifi restarts.

## Island animation clean-up
- Resting pill grows and shrinks with the non-overshooting spring (pillRise); the bouncy drop is only for the open Island. Before, the pill overshot by about 8% and the window clipped it (18 px past a window 20 px wider than the pill target), then it bounced back over about 900 ms.
- The window moves and resizes in one Windows call, and waits for a shrinking pill to finish before it shrinks.
- Old text leaves in 70 ms before the new text comes in; the text swap no longer blurs or scales text. The edge-flash 'pop' scale on every change is gone (the glow stays).
- The agents dot field is a fixed 340 px canvas clipped by the pill, so it is no longer rebuilt every frame while the pill resizes.
- Measured over the debug port: thin to pill, 120 to 363 px, peak 364.5 px, no clipped frames, one window change, worst frame gap 8 ms. Not measured: the agents dot field, open and close, the taskbar bar.

## Notification sound
- Notifications play docs/sounds/universfield-new-notification-07-210334.mp3, converted to src-tauri/sounds/notification.wav and lowered 2.4 dB (-19.3 to -21.7 LUFS, the loudness of C:\Windows\Media\Windows Notify System Generic.wav, the default toast sound). Lock and unlock sounds stay the synthesized deep click.
- The mp3's licence is the Pixabay content licence (free to use, not open source); the file name says Universfield. Not checked in the running app by ear.

- The notification chime now plays in Windows' 'System sounds' audio session (PlaySound with SND_SYSTEM), so it follows the System sounds slider and mute in the volume mixer. Checked: while it played, the mixer showed activity on 'System sounds' (peak 0.29) and no Notifi session.

## AI chat in the Island
- Spec: spec/apps/chat-host.md. Code: src-tauri/src/chat/mod.rs, ui/src/island/modules/Chat.svelte. Off by default: Island > Content > AI chat button (carries the disclaimers).
- An AI chat tab in the Island opens T3 Code's own chat inside the tall menu, as a second webview over the menu (not an iframe: T3's sign-in cookie is blocked in frames, tested). Sessions, project, model and effort, prompt box and panels are T3's. Uses tauri's 'unstable' feature for the child webview.
- Sign-in: one-time link from 't3 auth pairing create --json', once per run of Notifi. It gives the chat full T3 access.
- Opens on the last chat (the chat address is saved in the app data folder). Dictate button sends Win+H. The AI question menu has a 'Prompt chat' button and the chat has 'Questions (n)'. The taskbar AI Agents bar has a chat button.
- Checked live: chat opens in the Island, signed in, shows T3's sidebar and prompt box; after restarting Notifi it came back to the last chat. Not checked: dictation, the question-to-chat switch, light taskbar, dock mode, typing in the chat. Other hosts (Operant, OpenCode, Claude) are not built.
