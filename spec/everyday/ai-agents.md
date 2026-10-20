# AI agents panel

Follow local coding assistants (Claude Code, Codex, OpenCode, GitHub Copilot CLI): plan limits, usage, live work and a notice when a long task finishes. Usage data stays on this computer.

## User-visible behaviour
- Reads the logs and local state each tool already keeps; nothing is sent anywhere. On each launch only new changes are read, and polling idles when no session is active and no log is fresh.
- Per agent: live working state with elapsed time, model, project, tokens, estimated API value, and a 13-week activity map.
- Plan limits and when they renew: the 5-hour session and the weekly limit where the tool has them. The closed pill can show the session or the week limit instead of whichever is closest to running out. Claude's limits are read through Claude Code when installed, so use on other computers counts. Renewal times are estimated safely after long gaps.
- Codex: limits for each separate Codex account, including business plans; banked resets card shows how many remain and when the next expires, and can use one after confirmation. Edited conversations stay one task with their reviews and subagents.
- OpenCode (including version 2) and GitHub Copilot are followed alongside the others.
- An agent that is busy can appear next to the camera or the resting strip of the pill, alongside a timer or music. A notice appears when a long task finishes or a limit is running low. Alerts queued by a stopped session do not appear after a restart.
- Turn accounting: a turn stops counting after about 20 seconds offline unless a shell command is still running, and when its terminal or session ends mid-task.
- Token counts are split into tokens read and tokens written, with totals for each agent; usage is shown as an estimate made from this computer's own logs.
- Models with no known price are costed at zero, so the shown total is labelled as a minimum. API value is explained in the UI as what the same work would cost at pay-per-token list prices, whereas subscription plans charge a fixed amount.
- An optional daily budget, expressed in API value, warns when the day's usage approaches it.
- Plan limits for a tool appear only after that tool's next reply, because the limit figures arrive with its responses; until then the card says so instead of showing zeros.

## Settings
- Which agents to follow (default: those detected)
- Limit to show: closest, session, week (default closest)
- Cards shown (limits, resets, models, projects, activity map)
- Notifications: task finished threshold (default 2 minutes), limit warning at 80 percent
- Log folders (default tool locations)
- Daily budget in API value (default off)

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Large histories are read incrementally with limited memory; first launch never blocks the UI.
- Missing or renamed log formats: that agent shows as unsupported, others keep working.
- Multiple accounts and machines: only local logs are counted unless the tool reports cross-machine limits.

## Permissions
File read access to each tool's data folder in the user's home. No network access required.

## Platform notes
- Windows: reads under the user profile (%USERPROFILE%, %APPDATA%); process state via the process list.
- X11: reads under ~/.claude, ~/.codex, ~/.local/share, ~/.config.
- KDE Wayland: same as X11.
- GNOME Wayland: same as X11.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, app UI strings (release build 3.4.1)
