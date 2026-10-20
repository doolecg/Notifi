# Notifi motion spec

Source: public Vorssaint README clips and release notes only (no source code). Every value is marked **measured** (read from a clip's frames), **text** (stated in release notes, value chosen by us), or **inferred** (consistent with the measured style, no clip shows it).

## 1. Style summary

- Fast and quiet. Overlays appear almost at once (one GIF frame, 40 ms or less) and leave with a short fade of about 300 ms. Nothing is slow or floaty.
- The selection highlight in the app switcher does not glide. It swaps in 1 to 2 frames (about 70 ms), with the label weight and opacity cross-fading.
- Moves that travel (a window rising from its thumbnail, an island growing) are ease-out or lightly sprung: fast start, soft landing, roughly 150 to 300 ms, overshoot 0 to 5 percent. No bouncy motion anywhere in the clips.
- Release notes say the Dynamic Island (pill) opens and closes on a spring, notices shrink into it, compact music fades into other activity, and Reduce Motion skips the animation. The app switcher has an option to skip highlight, scroll and panel-resize animation, with smooth selection the default.
- Site CSS uses plain ease-out 300 ms ("boot-in") and ease 300 ms (width changes). Notifi defaults: ease-out for fades, light springs (damping ratio 0.7 to 0.85) for size and position.
- Only opacity, transform (scale, translate) and size are animated. Blur and corner radius are not animated in any clip; the glass blur appears and disappears with the panel opacity.

Spring convention used below: `stiffness k, damping c, mass m = 1`; damping ratio = c / (2 * sqrt(k * m)); overshoot = exp(-pi * z / sqrt(1 - z^2)). Settle time (2 percent) is about 4 / (z * sqrt(k / m)) seconds.

## 2. Motion tokens

Curves: `easeOut = cubic-bezier(0.22, 1, 0.36, 1)`, `easeInOut = cubic-bezier(0.4, 0, 0.2, 1)`, `linear`.
Springs: `S-snap = k 420, c 34, m 1 (z 0.83, overshoot 0.9%)`, `S-soft = k 300, c 25, m 1 (z 0.72, 3.8%)`, `S-lively = k 260, c 20, m 1 (z 0.62, 8.4%)`.

| Token | Properties | Duration ms | Curve / spring | Overshoot | Status | Evidence |
|---|---|---|---|---|---|---|
| panelOpen (tray panel) | opacity 0 to 1, scale 0.96 to 1, translateY -6 to 0 px | 180 | S-snap | 1% | inferred (text: menu panel uses the native popover animation, so keep it short) | release notes 1.x "native macOS popover animation" |
| panelClose | opacity 1 to 0, scale 1 to 0.98 | 120 | easeOut | 0 | inferred | same |
| tabSwitch | content opacity 0 to 1, translateX 8 to 0 px (direction of travel) | 160 | easeOut | 0 | inferred | none |
| switcherShow | opacity 0 to 1 (panel appears fully in one GIF frame; use a tiny fade to avoid a pop) | 60 (measured at most 40) | easeOut | 0 | measured | window-switcher.gif frame 9 empty (t 330 ms), frame 10 full (t 370 ms) |
| switcherHide | panel opacity 1 to 0 and blur fades with it; starts about 40 ms after the chosen window starts rising | 300 | easeInOut (near linear) | 0 | measured | window-switcher.gif frames 96-104 (t 3560-3850 ms) and 191-201 (t 7070-7440 ms); frame diffs fall off steadily over 6-9 frames |
| windowRaise (chosen window rises from its thumbnail on commit) | position and size from thumbnail rect to full window | 150 | easeOut (fast start, settles in the last frame, no visible overshoot) | 0 | measured | window-switcher.gif frames 95-99 (t 3520-3670 ms): thumbnail-size at 95, about half at 96, near full at 97, full by 99; repeated at 191-194 (t 7070-7190) |
| selectionMove | highlight border and fill swap between cards; label weight and opacity cross-fade; no sliding highlight | 70 | easeOut | 0 | measured | window-switcher.gif frames 60-62 (t 2220-2300 ms): old card still highlighted at 60, new card highlighted at 61 and label settled by 62; same at 141-143 and 153-156 |
| pillExpand (Dynamic Island) | width, height, content opacity; island grows from its compact size | 320 | S-soft | 4% | text (spring named in release notes), values inferred | release notes: "spring animation for opening and closing" |
| pillCollapse | width, height shrink, content fades out first (80 ms) | 260 | S-snap | 1% | text, values inferred | release notes: "Notices shrink into the island" |
| toastIn | opacity 0 to 1, translateY 10 to 0 px, scale 0.97 to 1 | 220 | S-soft | 3.8% | inferred | none |
| toastOut | opacity 1 to 0, scale 1 to 0.97 (or shrink into the pill when the pill is present) | 180 | easeOut | 0 | inferred (shrink-into-island behaviour from release notes) | release notes |
| commandBarOpen | opacity 0 to 1, translateY -12 to 0 px, scale 0.98 to 1; keyboard focus after the drop finishes | 220 | S-soft | 3.8% | inferred (release notes say the bar only accepts typing once its opening motion has ended) | release notes |
| radialOpen | ring scale 0.85 to 1, opacity 0 to 1 | 200 | S-lively | 8% | inferred | none |
| radialItemStagger | each item scale 0.6 to 1 and opacity 0 to 1 | 180 per item, 20 ms stagger between items | S-lively | 8% | inferred | none |
| shelfIn | shelf appears in one GIF frame at the cursor; Notifi adds opacity 0 to 1 and scale 0.97 to 1 | 120 (measured at most 60) | easeOut | 0 | measured appear time, scale inferred | shelf-demonstration.gif frame 36 (t 2250 ms) no shelf, frame 37 (t 2310 ms) full shelf |
| shelfOut | opacity 1 to 0 | 160 | easeOut | 0 | measured | shelf-demonstration.gif frames 194-207 (t 12130-12940 ms), Finder window and shelf fade over the hand-off |
| shelfDropHighlight (drop zone while an item is over it) | border colour and fill tint, dashed border to accent | 80 | easeOut | 0 | measured | uninstall-demo.gif frames 36-38 (t 1800-1900 ms): grey dashed to blue dashed with tinted fill within 2 frames; shelf-demonstration.gif frames 49-51 |
| snapPreview | rect position and size follow the target zone; opacity 0 to 1 | 160 | S-snap | 1% | inferred | none |
| captureSelector | selection rect follows the pointer with no easing; dim layer opacity 0 to 0.4 | 100 | linear (rect), easeOut (dim) | 0 | inferred; the rubber-band rect in the Finder desktop selection in shelf-demonstration.gif frames 11-14 is also direct tracking | shelf-demonstration.gif frames 11-14 (t 690-880 ms) |
| holdToQuitProgress | ring or bar fill 0 to 100 percent while the key is held; release drains back at 3x speed | equals the hold time (default hold time setting); drain 150 | linear fill, easeOut drain | 0 | inferred | none |
| listReorder | rows translate to new slots, dragged row lifts (scale 1.02, shadow) | 220 | S-soft | 3.8% | inferred | none |
| toggleSwitch | knob translate and track colour | 160 | S-snap | 1% | inferred | none |
| hoverLift | scale 1 to 1.03, background tint; return on leave | 120 | easeOut | 0 | inferred | none |
| tooltipIn | opacity 0 to 1 after a short dwell | 120 (dwell 400) | easeOut | 0 | inferred | quit-on-close.gif frames 33-34 (t 1000-1030 ms): tooltip full in one frame (dock tooltip is the OS, so only the speed is borrowed) |

Counts: 25 tokens total, 5 measured (switcherShow, switcherHide, windowRaise, selectionMove, shelfOut; shelfIn and shelfDropHighlight are measured for timing), 3 text-backed with inferred values (pillExpand, pillCollapse, commandBarOpen), the rest inferred.

Limits of the evidence: the GIFs run at 25 to 33 fps (frame gaps 30 to 70 ms), so anything under about 60 ms reads as "one frame". The window-switcher clip shows macOS raising the window itself, so windowRaise is partly the operating system's; on Windows and Linux Notifi animates its own overlay thumbnail to the window rect and lets the OS handle the real window. quit-on-close.gif and uninstall-demo.gif show mostly OS animations (Dock tooltip, window close) and were only used for the drop-zone highlight and the tooltip speed. No clip shows the island, toasts, command bar, radial menu, snap preview or capture selector in motion.

## 3. Reduced-motion fallbacks

Triggered by the OS setting (Windows "Animation effects" off, GNOME or KDE reduce-animation) or Notifi's own switch on the settings page. Release notes say Reduce Motion skips the island animation, and the switcher has its own skip-animations option.

| Token group | Fallback |
|---|---|
| All springs and translate or scale moves (panelOpen, tabSwitch, toastIn, commandBarOpen, radialOpen, radialItemStagger, pillExpand, pillCollapse, snapPreview, listReorder, shelfIn) | Opacity-only fade of 80 ms, or an instant swap for size changes. No scale, no translate, no overshoot, no stagger (all items fade together). |
| Fades (panelClose, toastOut, switcherHide, shelfOut) | Keep, shortened to 80 ms. |
| switcherShow, switcherHide | Instant show, 80 ms hide. Also controlled by the separate switcher option "skip highlight, scrolling and panel resize animations". |
| windowRaise | Skipped: the overlay hides at once and the real window is focused. |
| selectionMove | Instant swap (0 ms). |
| holdToQuitProgress | Keep (it carries information); fill steps in 10 percent increments with no easing. |
| listReorder | Rows jump to their slot; the dragged row keeps a static lift style (shadow only). |
| toggleSwitch, hoverLift | Toggle swaps instantly with a colour change; hover keeps only the tint, no scale. |
| captureSelector, snapPreview | Rect tracks directly with no easing; dim layer appears instantly. |

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/assets/readme/window-switcher.gif, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/assets/readme/shelf-demonstration.gif, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/assets/readme/uninstall-demo.gif, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/assets/readme/quit-on-close.gif, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/docs/assets/readme/cut-and-paste.gif, https://vorssaint.com, https://api.github.com/repos/vorssaint/vorssaint-utils/releases
