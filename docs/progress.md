# Notifi progress notes

Last saved: 2026-10-20 (after wave 2 was built and tested).

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
