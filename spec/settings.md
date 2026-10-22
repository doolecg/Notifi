# Settings window

One page per feature, found through a grouped sidebar with search, and back/forward history like a browser.

## Layout
- **Sidebar** (left, about 230 px, its own scroll):
  - A search box at the top: "Search settings".
  - Collapsible groups. Each has a small-caps header with a chevron and holds rows of an icon plus the page name.
  - The selected row is filled with the accent colour, and the selection slides with `selectionMove`.
  - A feature that's switched off shows its row dimmed but still clickable.
- **Content area** (right):
  - A top bar with back and forward buttons and breadcrumbs for the current page (Settings > Installed > Island); each part is a link.
  - Below it, the page.
- **Window frame:** native on Windows and Linux, so no fake Mac traffic lights. The minimum size is about 900 × 640.

## Sidebar groups and pages
- **Essentials:**
  - General: startup, theme, reduced motion, language, updates, reset everything.
  - Tray & panel: tray icon, which panel tabs show, tab order, left-click action.
  - Installed: the features you have installed, as a list or a grid. Clicking one opens its page; each can be uninstalled.
  - Install features: every feature, with search, a filter by kind, a list or grid view and an Install button on each ("install only what you use"). A feature that is not installed does not run, and is not in the sidebar.
  - Island.
  - Monitor: system monitor, tray readouts, alerts.
  - Shortcuts: every shortcut in one list, rebindable, with clashes shown.
  - Permissions.
- **Utilities:** Command Bar, Quick panel, Quick toggles, Camera preview, Scratchpad, Cleaning Mode, Screenshot, Screen recording, Copy text from screen, Color picker, Radial menu, Media tools, Shelf, AI agents.
- **Sound:** Media-key app blocker, Volume mixer, Per-app output, Output switcher, Microphone.
- **Energy and display:** Keep awake, Displays and brightness, Extra dimming, Bluetooth on sleep.
- **Windows:** App switcher, Window layout, Quit on close, Close protection, Taskbar preview (Linux), Taskbar clicks (Linux).
- **Keyboard and mouse:** Text snippets, Smooth scrolling, Linear scrolling, Pointer acceleration, Focus follows mouse, Scroll direction, Sideways scrolling, Mouse buttons, Touchpad middle click, Apps to leave alone, Click filter, Key debounce, Super key.
- **Clipboard and files:** Clipboard history, Auto clear, Paste as plain text, Paste image as file, Clean URL, AppImage installer (Linux only).
- **Apps:** App updates, Package manager, Uninstaller, Cleaner, Messaging downloads, Port manager.
- **About:** version, licence, clean-room note, release notes.

## Feature page
- **Header:**
  - The page title in large text.
  - A one- or two-line description below it.
  - The feature's on/off switch at the top right.
  - With the feature off, the page content stays visible but is dimmed and can't be used, except the switch.
- **Optional sub-tabs:** a segmented control under the header when a page has several areas. For example, Island has Layout / Content / Activity / Behaviour. A small "open" button at the end of the control opens or previews the feature itself.
- **Content:**
  - Rounded cards, each with a small heading.
  - Rows inside a card: an optional small coloured icon, a label, a one-line hint, and the control on the right (switch, picker, slider, shortcut capture).
  - Choice cards for exclusive options: a row of equal tiles, each with an icon and a label. The selected tile is outlined in the accent colour. Example: Island size Compact / Spacious / Custom.
- **Reset:** a "Reset this page" link at the bottom of each page.

## Search
- Typing filters the sidebar to the matching pages. It matches page names and also the labels of the options inside each page.
- Each match shows the option it found under the page name.
- Pressing Enter or clicking a match opens the page, scrolls to that option and briefly highlights it.

## Navigation
- **History:** back and forward step through the visited pages. Mouse back/forward buttons and Alt+Left / Alt+Right do the same.
- **Opening a page directly:** other parts of the app can open settings at a given page and option. Examples: the panel's "customise" icon, the Utilities cards, and the Island orbit "settings" button.
- **Remembered page:** the last page reopens next time.

## Notes
- Every option the app has lives on one of these pages, is applied live and is saved.
- The existing settings store and catalog stay. Only the grouping and the page UI change; each feature's options move to its own page.

Sources: user-supplied screenshot of Vorssaint's settings window (Dynamic Island page), described in our own words.
