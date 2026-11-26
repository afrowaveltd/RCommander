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
Windows) and fall back to the mount path. Each entry shows the free space (`DATA — 220 GB`) and
removable media are listed first, because the disk you just plugged in is the one you are
looking for.

## Right-click menu

Right-clicking a row selects it (as Explorer does) and opens a menu of the operations that
matter there: *Open* for a directory, *View* and *Edit* for a file, then *Copy*, *Move*, *New
folder*, *Rename*, *Delete* and *Attributes*, each with its keyboard shortcut next to the name —
so someone who does not know the keys learns them by using the menu.

Right-clicking **the empty space** below the rows (or the panel border) opens the menu for the
directory itself: *New folder*, *Reload*, *Parent directory*, *Select all*, and the credential
store and configuration entries. The panel becomes active on the click, so the menu always acts
on the panel you pointed at.

Commands that would grow past this (compile this project, check with the agent, publish to the
server) belong here too once the agent layer exists; the menu is the natural place for them.

## Opening a file

`Enter` and a **double-click** mean *the default action of your system*: a program
runs, anything else opens in the application your desktop associates with it, and
the status line says which happened. The built-in viewer stays on `F3`, so looking at
a text file never costs you a detour through another program.

The right-click menu adds the actions people expect from a file manager:

| Entry | What it does |
| --- | --- |
| **Open (default application)** | the same as Enter |
| **Open with…** | on Windows the native chooser; elsewhere a searchable list of the applications your desktop knows (read from the `.desktop` files), showing only what a user is meant to see |
| **Send by e-mail…** | attaches the file to a new message: `xdg-email`, Mail on macOS, the default handler on Windows |
| **Run in terminal** | runs the program in a terminal, so its output stays on screen instead of vanishing |
| **Open terminal here** | a terminal in the panel's own folder, whichever emulator you have (`$TERMINAL`, `gnome-terminal`, `konsole`, `xfce4-terminal`, `alacritty`, `kitty`, `xterm`) |

Nothing is passed through a shell command line, so a file called `; rm -rf ~` is a
file name and not an instruction.

## Right-click menus

TBD

## Panels that are not directories

A panel does not have to show a file system. A plugin can provide a **source** — a mail inbox, a
photo catalogue, a list of tickets — and the panel draws it the same way it draws a folder, because
all it ever asks is *what is in this location* and *what does this row do*.

What changes is what the source declares:

| The source declares | What you see |
| --- | --- |
| **columns** | its own headers (*Subject, Priority, Age*), replacing the configured file columns in that panel only. The other panel keeps its own |
| **row verbs** | extra entries at the bottom of the right-click menu — *Reply*, *Develop*, *Assign to me*. The source performs them; RCommander does not guess what they mean |
| **capabilities** | which file operations make sense here. This is the important one |

Capabilities are answered in the negative: a source that does not say it supports deletion
**cannot have its rows deleted**, and the function key is drawn as unavailable rather than greyed
in secret. No dialog opens, no approval is requested, and nothing pretends the operation nearly
worked. Copy, move, create, delete, rename and view are declared one by one; a source that declares
none of them is a listing you can read and nothing else.

That is not a permission — it is the shape of the place. Even where a source *does* declare
deletion, the policy, the approval dialog and the audit log decide exactly as they do for a local
file.

A panel whose source cannot be reached says so in the panel. It never shows an empty list, because
"the server did not answer" and "this folder is empty" must never look the same.

