# Command Bar

One search field for apps, windows, files, clipboard, snippets, app menu commands, a calculator, unit conversion, colours, emoji and saved scripts.

## User-visible behaviour
- Opens centred on the display under the pointer and takes keyboard focus as it appears. Rows are prepared once on opening; battery and memory are read in the background so typing stays responsive.
- Results are grouped: apps, windows (every window of an app, including after it was hidden and shown again; closed or blank helper windows are not listed, and choosing one raises only that window), files, clipboard history, snippets, app menu commands, system actions (including kill process, uninstall app, restart Notifi, set volume or brightness on the display under the pointer), features (a feature's switch is listed first), emoji.
- Ranking: typo-tolerant matching. The bar learns which app or emoji you pick for a query and a general usage ranking, both saved across restarts; a "forget learned ranking" button clears them. Query-specific learning is not stored on disk.
- Calculator: plain and scientific expressions, chained operations (100 - 20 - 30, 8 / 2 / 2), percentages, large results keep meaningful digits while hiding rounding noise. Dates and times are not mistaken for sums. Decimal separators of either style are accepted; thousands grouping with space or apostrophe is accepted, misplaced separators are rejected, and a number starting with 0 is never read as thousands. Date maths handles 29 February and rejects impossible dates (31 April).
- Unit conversion: length, weight, temperature, volume, data, time, currency if enabled offline rates.
- Colours: typing a colour on its own previews it; adding a conversion phrase (to hex, rgb, hsl or css/qt) converts it; Enter copies the result.
- Emoji search with skin tone options.
- Scripts: saved scripts (shell, PowerShell, or any executable) appear as rows. Arguments typed after the name are passed in. A script can be marked to run on its bare name with no argument (off by default), and can have its own global shortcut that runs it directly. The script's output is copied and the bar closes whenever the row was matched on a partial name or launched from its own shortcut.
- Per-app shortcuts: up to 64 custom shortcuts, one per app or command, enough for every letter plus others. Pressing an app's shortcut while it is in front hides it; pressing again brings it back. Shortcuts can take over an OS shortcut and are checked for conflicts. The capture field records any combination, including Ctrl+Q.
- Compact mode: opens as a lone field and shows results only once you type; Down reveals the full list (default off).
- Number keys 1 to 9 (also the numeric keypad) pick rows. Select all, copy, cut and paste work in the field. Failed copies are reported.
- The bar can be dragged by a handle at the left of the field to sit anywhere on the screen; double-clicking the handle puts it back in the centre. A separate option opens the bar with no suggestions listed until something is typed, and another option opens it near a screen edge.

## Settings
- Shortcut to open (default Alt+Space), compact mode (default off)
- Sources to search (all on), file search folders and index
- Scripts folder, per-script shortcut and bare-name flag
- Per-app shortcuts (up to 64), aliases, pins, hidden entries
- Forget learned ranking; clear clipboard unpinned (asks first)
- Position memory and recentre (default centred), open without suggestions (default off)

## Default shortcuts
Open: Alt+Space (Notifi design, not from source docs). Enter runs, Ctrl+Enter alternate action, Esc closes, Tab completes. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Removing an app frees its shortcut, pins and aliases; uninstalling a feature removes its pins.
- Alternate keyboard layouts must be honoured when recording and matching shortcuts.
- Secure input fields elsewhere can block global shortcuts; settings name the app, not Notifi itself.
- Clipboard stalls do not freeze the bar.

## Permissions
Windows: none for basics; reading other apps' menu commands uses UI Automation. Linux: AT-SPI for menu commands.

## Platform notes
- Windows: UI Automation for menus and windows, Everything-style index or Windows Search for files.
- X11: Limited: AT-SPI menus, wmctrl-style window list, global grabs for shortcuts.
- KDE Wayland: Limited: windows via KWin script/D-Bus; global shortcuts via KGlobalAccel.
- GNOME Wayland: Limited (PARTIAL): window list and shortcuts need the shipped extension.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, app UI strings (release build 3.4.1)
