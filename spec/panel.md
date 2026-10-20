# Tray panel

The main way people use Notifi. Clicking the tray icon opens a compact panel next to it. The panel holds every feature's quick view in tabs. The settings window is for deeper options only.

## Opening and closing
- Left-click on the tray icon toggles the panel. Right-click keeps the small menu (Open Settings, Quit, Install update).
- Windows: the panel sits just above the tray icon, aligned to the taskbar edge (handle a taskbar on any side). Linux: placed by the tray click position when known, otherwise at the top-right of the screen with the pointer.
- It closes when it loses focus, on Esc, or on a second tray click. Reopening shows the last-used tab.
- Opens and closes with the `panelOpen` / `panelClose` motion tokens; tab changes use `tabSwitch`. The tab highlight slides with `selectionMove`.
- Fixed width of about 360 px. Height fits the content up to about 80% of the screen height, then the content scrolls (the header and footer stay put).
- Frameless, rounded, with a shadow, and following the light/dark theme.

## Layout (top to bottom)
1. Small Notifi logo mark.
2. Tab strip: a rounded bar of icon-only buttons with tooltips. Only tabs whose features are switched on are shown, except Controls and Utilities, which are always shown. Order: Keep awake, Mixer, System, Network, Storage, Power, Utilities, Controls.
3. Tab title in small caps on the left. On the right, an optional "customise" icon that opens that tab's settings section.
4. Tab content (below).
5. Footer: two equal buttons, Settings (opens the settings window) and Quit.

## Tabs
- **Keep awake:** a big on/off control, a duration choice (15 min, 1 h, 2 h, until turned off) and a countdown while active. Uses the platform keep-awake inhibit.
- **Mixer:** one row per app playing audio, with the app icon, the app name, an output-device picker ("Default" plus devices), a volume slider, the percentage, a reset-to-100% button (shown only when not 100%) and a mute button. Above 100% the slider and number switch to a warning colour with a boost mark. Empty state: "Nothing is playing". If the audio backend isn't available yet, show "Volume mixer arrives in a later version".
- **System:**
  - Temperature tiles (CPU, GPU, battery) where readable; hide the ones that can't be read.
  - "Hardware usage": CPU and GPU rows, each with a bar, the percentage and a small live history line (last ~60 s). A row can expand to show per-core CPU or GPU details.
  - Battery row with charge bar and history (laptops only).
  - An expandable "Apps using the most CPU" list (top 5).
  - "Memory": a pressure label (Normal / Elevated / High as a coloured pill), used / total and a history line.
  - Uptime line at the bottom.
  - Update once a second while the panel is open; don't poll while it's closed.
- **Network:** live down/up speed with history lines, session totals and the local IP (click to copy).
- **Storage:** each mounted disk with a used/total bar. Removable disks get an eject button.
- **Power** (laptops; hide the tab when there's no battery), top to bottom, rows separated by thin lines:
  - **Battery:** an icon, a charge bar coloured by level and the percentage. Below it, a small history graph with a dashed 100% guide line.
  - **"Apps using the most energy":** expandable, showing the top 5 by CPU as a stand-in.
  - **Battery temperature,** where readable.
  - **System power draw** in watts with a history graph and a dashed reference line (e.g. 10 W).
  - **Adapter:** watts in now, with its rated maximum as a sub-line (e.g. "30 W max"), when on AC.
  - **Battery:** watts in or out, with a sub-line of Charging / Discharging / Full.
  - **Battery health:** a percentage of the design capacity, with the cycle count as a sub-line.
  - Each row has a small coloured icon on the left and the value right-aligned in bold.
  - **Data sources:** Windows battery WMI (`BatteryStatus`, `BatteryFullChargedCapacity`, `BatteryStaticData`, `BatteryCycleCount`); Linux `/sys/class/power_supply` (`power_now`, `energy_full` vs `energy_full_design`, `cycle_count`). Hide any row the platform can't read.
  - Power plan (Windows) / power profile (Linux) picker at the bottom.
- **Tab strip additions:** a **Displays** tab after Keep awake (brightness per display, arriving with phase 8, hidden until then). An **About** tab at the end (version, update status, a link to the release notes).
- **Utilities:** a list of tool cards. Each card has an icon, a name, a one-line description, its shortcut shown as a key chip if it has one, and a chevron. Clicking opens the tool (or its settings section until the tool exists). Show only the tools whose feature is switched on, plus a last card, "Add tools…", which opens settings.
- **Controls:** feature switches grouped into collapsible groups (Windows, Keyboard & mouse, Clipboard & files, Everyday). Each group header shows "n/m on". Each feature is a card with an icon, a name, a short description and an on/off switch. A feature's sub-options (if any) appear indented under its card while it's on. The switches edit the same settings as the settings window, live in both directions.

## Notes
- All values come from the platform layer or `sysinfo`. Anything a platform can't provide is hidden, not shown as zero.
- Notifi uses its own icons, colours and wording; this spec describes structure and behaviour only.

Sources: https://github.com/vorssaint/vorssaint-utils README screenshots (docs/assets/readme/panel-mixer.png, panel-system.png, panel-controls.png, panel-utilities.png), described in our own words.
