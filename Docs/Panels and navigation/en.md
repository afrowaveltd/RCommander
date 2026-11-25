# Panels and navigation

## The two panels

The active panel draws its cursor inverted. The status line of the active panel
shows how many files and directories it contains, their total size, and how many
entries you have marked when a selection exists.

## Navigating

| Key | Action |
| --- | --- |
| `Enter` | open directory / archive under the cursor |
| `Backspace`, `Ctrl+PgUp` | go to the parent directory |
| `Alt+Left`, `Alt+Right` | previous / next directory of the history |
| `Alt+F1`, `Alt+F2` | choose the drive or volume of the left / right panel |
| `Ctrl+R`, `F2` | re-read the directory |
| `Ctrl+Left/Right` | show the directory under the cursor in the other panel |
| `Tab` | switch the active panel |
| `Ctrl+U` | swap the two panels |
| `Ctrl+Shift+U` | set the target panel to the source directory |

## View modes

| Key | Mode |
| --- | --- |
| `Ctrl+F1` | brief — names only, wrapped into columns |
| `Ctrl+F2` | full — name, size, date, attributes |
| `Ctrl+Shift+F2` | full plus the file comment |
| `Ctrl+Shift+F1` | thumbnails |
| `Ctrl+Q` | quick view panel |

## Sorting

`Ctrl+F3` name, `Ctrl+F4` extension, `Ctrl+F5` time, `Ctrl+F6` size,
`Ctrl+F7` unsorted. Pressing the same shortcut twice reverses the order. Names
are sorted naturally, so `file2` comes before `file10`.

## Filtering

A mask in the panel (`*.txt;*.md`) hides files that do not match; directories
always stay visible. `Ctrl+F10` shows all files again.

## Quick view and the tree panel

`Ctrl+Q` replaces the inactive panel with a quick preview of the file under the
cursor: images are shown as images, text as text (with the detected encoding and
right-to-left support) and unknown binaries as a hex dump. Press `Ctrl+Q` again to
get the file list back.

`Ctrl+Shift+F8` shows a directory tree instead of the quick view, which makes
jumping around a deep folder hierarchy much faster.

## Choosing a disk

A **volume selector** sits at the left of each panel's path bar. On Windows it lists every
drive as `C: System`, `D: Data` or just `C:` when the drive has no label. On Linux it lists
the file systems the user can actually use — removable disks, network shares, optical media —
and leaves out the system root, the `snap` images and the kernel pseudo file systems, because
a selector full of `/snap/core22/3129` entries helps nobody. The root stays reachable by typing
`/` into the path bar.

Labels come from the volume itself (`/dev/disk/by-label` on Linux, the volume information on
Windows) and fall back to the mount path.
