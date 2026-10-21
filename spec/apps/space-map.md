# Space map

A disk usage map: every folder and file on a drive drawn as a block whose area matches its size. Notifi's own name for the idea of a portable treemap disk analyzer (SpaceSniffer by Uderzo Software is the inspiration; no code, images or text were copied).

Sources used (public product page only): https://space-sniffer.com/

## User-visible behaviour
- A "Disk space" tab in the App tools window.
- Pick a drive from the list (name, used, free) or choose any folder, then Scan. The map fills in while the scan runs, and can be explored before it finishes.
- Blocks are nested: a folder is a frame with a title strip, its contents laid out inside it, largest first. Files are plain blocks. Tiny items are merged into one "N small files" block per folder.
- Click selects a block and shows its path, size, share of the parent and file count in the status strip. Double-click on a folder zooms so it fills the map. A breadcrumb above the map goes back up to any parent.
- Colours depend on the file kind (archives, video, images, audio, documents, code, applications, other), folders are neutral. Four colour tags (red, yellow, green, blue) can be put on any block from its context menu; they live until Notifi quits.
- Filter box above the map. Entries are separated by `;`. A name pattern with `*` and `?` keeps matching files (`*.zip`). `>500mb` and `<1gb` keep files above or below a size (units b, kb, mb, gb, tb). `>30d` and `<2w` keep files older or newer than an age (units d, w, m, y). `:yellow` keeps blocks with that tag. All entries must match (AND); an entry starting with `!` excludes matches. Folders show only what survives; empty folders vanish. The map redraws as the text changes.
- Context menu on a block: Open, Show in file manager, Copy path, set or clear tag.
- Export report: writes a text file of the current view (path, filter, totals, then the largest items per folder down to a chosen depth) and opens its folder.
- Status strip while scanning: files and folders seen, bytes counted, current folder. Cancel stops the scan and keeps what was found. Rescan restarts the same root.
- Hover on a block shows its name and size.

## Settings
- Default root when opened: last used (otherwise the system drive).
- Smallest file drawn on its own: 16 KB (smaller files are merged per folder; name filters do not see merged files).
- Show hidden and system files: on.
- Report depth: 3.

## Edge cases
- Symbolic links and reparse points are not followed, so a link cannot be counted twice or loop.
- Folders that cannot be read are skipped and counted as "unreadable".
- Sizes are the file's logical length. Changes on disk during a scan are not tracked; Rescan picks them up.
- A drive with millions of files stays responsive because the view returns only the biggest entries per level.

## Not built
- NTFS alternate data stream inspection, several map views at once, saved filter presets, reading the NTFS master file table directly.

## Platform notes
- Windows: drives come from the system disk list; hidden and system flags from file attributes.
- Linux: mounted disks from the system disk list; dotfiles count as hidden.
