# Island

A black shape that hangs from the top centre of the screen, like a camera notch. It grows to show music, controls and a grid of small tools, with round shortcut buttons floating beside it. Notifi calls it the **Island** (the plan's working name was Top Pill). Optional: switched off, the separate panels keep working.

## States
- **Resting:** a small black pill flush with the top edge, centred on the chosen display, with square top corners and rounded bottom corners. While music plays it can show a tiny cover and a live equaliser on its two ends. (Notifi design, not from source docs.)
- **Open:** hovering (after a short delay) or clicking it expands it with the `pillExpand` spring into the last-used module. Moving the pointer away (after a delay) or pressing Esc collapses it with `pillCollapse`. While it's open, round **orbit buttons** fade in around it: a column on the left, a column on the right and a row below.
- **Live activity:** while something long-running is going on (an AI agent working, music playing, a timer running), the resting pill widens a little to show it on its two ends: the activity's icon on the left, tinted in the activity's colour, and a live value on the right in the same colour (for example the agent's elapsed time, "0:36", ticking). When more than one is live, hovering shows a row of chips under the ends, one per activity (for example "AI Agents" and "Music"), each with an icon. The selected chip is filled light and the others are dark; clicking a chip chooses which activity the ends show.
- **Activity:** short events (track change, timer done, download finished, an agent task finished, a notification) briefly grow the resting pill to show one line, then shrink back.

## Modules (one shown at a time inside the open Island)
- **Now Playing:**
  - Title "Now Playing" and a "…" menu at the top.
  - A large cover with a soft glow taken from the cover's colours.
  - Track title and artist, a source-app picker (when several players are running) and a small live equaliser mark.
  - A seek bar with elapsed time on the left and remaining time on the right.
  - Previous, play/pause (large and round) and next.
  - A bottom row with a volume slider and an output-device picker, plus "Lyrics" and "Up next" buttons. Show those two only when the player provides that data.
- **Module grid (launcher):**
  - A top row with back, search and "…".
  - A grid of tiles, 5 across and 3 down per page, with dots on the right edge to page through.
  - Each tile has an icon, a name and its shortcut.
  - A tick marks the module that opens by default; set it from "…" or by right-clicking a tile.
  - Modules: Controls, Volume mixer, Now Playing, Clipboard, Recent captures, Files (shelf), System, Tools, Calendar, Notifications, Timer, Camera mirror, Downloads, Scratchpad, AI Agents.
  - A module whose feature isn't built yet shows a short "coming in a later version" card.
- **Controls:**
  - A "Nothing playing" / mini player card.
  - Volume and brightness sliders with percentages.
  - A row of quick buttons: Volume mixer, Keep awake, Timer, Calendar. The buttons can be customised.

## Orbit buttons
- Round, black, with one icon each.
- Defaults:
  - Left column: module grid and Timer.
  - Right column: settings and volume.
  - Below the Island: Now Playing.
- Each button opens a module or runs an action. Inside a module, the top-left button becomes "back".
- The buttons are fully customisable in settings (see below).

## Settings page (in Notifi's settings window)
- An on/off switch with a one-line description.
- A segmented control with four tabs: **Layout**, **Content**, **Activity**, **Behaviour**, and a button that opens the real Island for a preview.
- **Layout:**
  - A live preview of the Island with its orbit buttons.
  - Empty "+" slots in each column and row: click one to add a button, drag buttons between slots, and click a button to edit what it does.
  - A resize handle for the custom size.
  - Hint text: "Click + to add a button. Drag buttons around the Island. Click a button to edit it."
  - Size cards: Compact, Spacious and Custom.
  - A "Translucent background" switch, which blurs what's behind the open Island. That's acrylic/Mica on Windows; on Linux, only where the compositor supports blur, otherwise the switch is greyed out.
- **Content:** which modules appear in the grid and in what order, the default module, and per-module options. For Now Playing that's lyrics on or off, the equaliser and the player to prefer.
- **Activity:** which events may grow the resting pill, and for how long.
- **Behaviour:**
  - Open on hover or on click only, plus the hover delay and the collapse delay.
  - Which display it appears on.
  - Hide it when an app is fullscreen.
  - Shortcuts: one per module, all rebindable.

## Platform notes
- **Window:** a transparent, frameless, always-on-top window at the top centre that never takes focus when it appears (Windows: no-activate style). Hide it while a fullscreen app or game is in front.
- **Media:**
  - Windows: the system media session API (SMTC). It gives title, artist, cover, position and controls for most players.
  - Linux: MPRIS over D-Bus, with lyrics only where the player exposes them. Optional online lyric lookup is off by default.
- **Volume and brightness:** come from the platform layer (Audio and Display).
- **Linux placement:**
  - X11: a dock-type window at the top.
  - KDE Wayland: layer-shell.
  - GNOME Wayland: needs Notifi's GNOME Shell extension to sit at the top edge. Without it, the Island isn't offered.
- **Notifications:**
  - Windows: needs packaged-app identity, so off unless packaged.
  - Linux: a D-Bus notification monitor.

Sources: user-supplied screenshots of Vorssaint's Dynamic Island (now playing, module grid, settings page), described in our own words; Vorssaint README "Dynamic Island" and "AI agents" entries.
