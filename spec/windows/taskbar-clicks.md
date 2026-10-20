# Taskbar clicks

Clicking the taskbar button of the app you are already using minimises it, hides it or cycles its windows. Replaces the Mac Dock clicks.

## User-visible behaviour
- Three choices for what a click on the active app's button does: minimise its windows, hide the app, or cycle through its windows.
- With minimise and cycle both enabled: apps with several windows cycle, apps with a single window minimise.
- With "hide" on, clicking the active app when it has no open windows creates a fresh window rather than hiding it.
- A quick toggle for cycle mode also appears in the panel's Controls tab.
- Only a click on the button of the app that already has focus is changed; other clicks, middle-click, right-click and drag-to-reorder keep native behaviour.
- Works while recording or while overlays let pointer input pass through.

## Settings
- Enable (default off)
- Click action: minimise (default), hide, cycle
- Apps to leave alone

## Default shortcuts
None. Cycling windows of the same app by keyboard is Alt+` (see the app switcher spec).

## Edge cases
- Apps that run as several processes act on the right instance.
- Full-screen apps are respected.
- Apps with odd window structures (Java apps): Notifi acts on the window the OS reports as active.
- A click while the native thumbnail list is open is left to the OS.

## Permissions
Windows: none. Linux Wayland: KWin script or GNOME extension; X11 none.

## Platform notes
- Windows: mouse hook plus UI Automation to know the clicked button; Windows normally shows a thumbnail list on click, so Notifi swallows that click and acts.
- X11: the panel's own click-to-minimise setting where it has one, else the window list applet through accessibility.
- KDE Wayland: the task manager's "minimise on click" setting is toggled; cycling uses the KWin script.
- GNOME Wayland: the dock's click action setting (Dash to Dock / Ubuntu Dock) through gsettings; cycling through the extension.
- Status: Windows ADAPT, Linux ADAPT (setting exposure varies by panel).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/TROUBLESHOOTING.md
