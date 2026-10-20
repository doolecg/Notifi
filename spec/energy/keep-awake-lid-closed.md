# Keep awake with lid closed

Keep running with the laptop lid shut, normally with the built-in screen dimmed.

## User-visible behaviour
- An extra option inside Keep awake: when a session is active, closing the lid does not suspend the machine, so downloads, builds and music carry on.
- The session ends the same ways as any Keep awake session (timer, battery cutoff, trigger gone) and always restores the normal lid action afterwards, including when Notifi quits or is uninstalled. After uninstalling, normal lid sleep is verified and, if it was not restored, the user is asked to authorise the fix so the machine is never left unable to sleep.
- Optional built-in display dimming: while the lid is closed with the session active, the built-in screen is dimmed or switched off. It applies immediately if enabled while the lid is already closed.
- Closed-lid mode with the built-in display selected for the top pill keeps the pill off external monitors and returns it to the laptop screen when that is available again; machines without a built-in display keep the pill on the main display.
- If setup cannot complete (permission denied), the panel tells the user plainly to toggle the feature off and then on to try again, and offers the system authorisation prompt as a fallback.

## Settings
- Lid-closed awake: off.
- Dim built-in display with lid closed: off.
- Behaviour on lid close follows the current Keep awake session; no separate timer.

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Rapid open/close or on/off toggling must keep the order of lid-action changes, so the final state matches the last request.
- Account names with unusual characters (spaces, @, non-ASCII) must not break the permission setup; grant by user ID or group, not by name.
- Failed one-time permission setup falls back to a normal authorisation prompt.
- Heat and bag warning: keeping a closed laptop running in a bag can overheat it; show a short note when first enabled.
- On battery below the protection threshold the session ends and lid sleep returns.
- Applies only to portable devices with a lid switch; hidden on desktops.

## Permissions
- Changing the lid action needs one-time administrator / polkit authorisation. A password-free rule may be offered after the first approval and is removed on uninstall.

## Platform notes
- Windows: set the lid-close action to "Do nothing" in the active power plan for the session and restore the previous value afterwards.
- X11: logind handle-lid-switch inhibitor (block lid switch) held for the session.
- KDE Wayland: same logind inhibitor; PowerDevil's lid setting is left unchanged.
- GNOME Wayland: same logind inhibitor; GNOME's lid action is overridden only while it is held.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/PERMISSIONS.md
