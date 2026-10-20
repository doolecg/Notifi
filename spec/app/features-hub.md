# Features hub (install only what you use)

One page lists every feature as a card and lets you install or uninstall each one.

## User-visible behaviour
- Uninstalled features stop loading (no CPU, memory or energy) and disappear from the whole interface; reinstalling restores their settings. Nothing is deleted.
- Each feature has an install/uninstall button and an honest energy note saying what it keeps alive while on (global hooks, polling timers).
- Three one-click bundles (volume, windows, battery) and a "presets" choice.
- Choosing to install a feature through the hub, a preset or the first-run picker also switches on its headline behaviour, unless you had already saved a different choice. "Install all" only makes features available.
- New features added by an update wait on the hub; they never install themselves.
- If a number of installed features have never been switched on, the hub proposes removing them (Undo available) or keeping them ("Never turned on").
- A Permissions tab maps each permission to the features that rely on it, and highlights any granted permission that no installed feature needs.
- Rows say which features listen to the keyboard or mouse.
- Options needing an uninstalled feature read as off and grayed with a link to the hub.
- Uninstalling a feature during a session leaves it loaded in memory until the app restarts. The hub says so and offers a restart-now action that unloads it immediately.

## Settings
- Per feature installed state (defaults: bundle chosen at first run)

## Default shortcuts
None.

## Edge cases
- Installing or removing while scrolled does not make the page jump.
- Settings stay when a feature is uninstalled.

## Permissions
None.

## Platform notes
Same on both platforms.
- Status: Windows PORT, Linux PORT.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, app UI strings (release build 3.4.1)
