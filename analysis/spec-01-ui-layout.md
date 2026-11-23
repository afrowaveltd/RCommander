# Total Commander 11.58 — UI / Behavioural Specification (Part 1)

**Scope:** main window layout, two-panel model, file-list rendering, complete dialog inventory, menu
structure, viewer/editor integration, and implementation-relevant behaviours.

**Source of facts:** the extracted English help set in `work/e_chm/` (from `TOTALCMD.CHM`). Every
statement below is taken from those pages; anything not directly stated there is marked
**(inferred)**. Page names are given in backticks so each claim can be traced.

---

## 0. Page map used

| Area | Pages |
|---|---|
| Main window hotspot map | `cmdinterface.htm` → `add_menu`, `add_btnbar`, `add_drivebuttons`, `add_drive`, `add_freemem`, `add_root`, `foldertabs`, `add_curdir`, `add_hothist`, `add_tabstopheader`, `add_filelist`, `add_markedfiles`, `add_cmdline`, `add_buttons` |
| Two-panel / views | `add_srcdir`, `add_trgpath`, `vertical_alignment`, `quick_view`, `show_tree`, `show_separatetree`, `show_shortlong`, `show_thumbnails`, `show_customcolumns`, `show_customviewmodes`, `show_files`, `show_sort`, `only_selected`, `foldertabs` |
| Keyboard | `keyboard.htm`, `cmdline.htm`, `proc_*.htm` |
| Menus | `menu_files`, `menu_mark`, `menu_fileactions`, `menu_net`, `menu_show`, `menu_configuration`, `menu_starter`, `menu_help` |
| Config dialog | `options_config.htm` + one `dlg_config*.htm` / `dlg_display` / `dlg_config*` page per tab |
| All other dialogs | `dlg_*.htm`, `dialog_*.htm`, `dialog_box__*.htm` |
| Lister | `listercontents`, `lister_menu{file,edit,options,plugins,percent}`, `lister__encoding_menu`, `dlg_listerconfig`, `dlg_printoptions_lister` |

Note: `cmdinterface.htm` itself is only a clickable hotspot figure. Each named element is documented
on its own `add_*.htm` page, which is what section 1 summarises.

---

## 1. Main window anatomy

### 1.1 Vertical stack (top → bottom)

Order below follows the hotspot figure in `cmdinterface.htm`, with position statements from the
individual element pages.

| # | Element | Position | Documented on |
|---|---|---|---|
| 1 | Menu bar | very top | `add_menu.htm` |
| 2 | Button bar (horizontal) | below menu bar | `add_btnbar.htm` |
| 3 | Vertical button bar (optional, TC9+) | along the panel separator **(inferred)** | `add_btnbar.htm` |
| 4 | Drive button bar (optional, per panel possible) | above each file panel | `add_drivebuttons.htm` |
| 5 | Drive selection combo box | top-left of each panel row | `add_drive.htm` |
| 6 | Volume label + free/total space | right of the drive box | `add_freemem.htm` |
| 7 | Root `\` and parent `..` buttons | next to the drive box | `add_root.htm` |
| 8 | Folder tabs | above the file list | `foldertabs.htm` |
| 9 | Current directory bar (breadcrumb + inline edit) + hotlist/history buttons | directly above the file window | `add_curdir.htm`, `add_hothist.htm` |
| 10 | Tabstop header | top of the file list itself | `add_tabstopheader.htm` |
| 11 | File windows (two) | central area | `add_filelist.htm`, `add_srcdir` |
| 12 | Count / selection status line (footer) | directly below each file window | `add_markedfiles.htm` |
| 13 | Command line | above the function key bar | `add_cmdline.htm` |
| 14 | Function key bar | bottom | `add_buttons.htm` |
| 15 | FTP toolbar (only while an FTP connection is open) | appears after connecting | `ftp_menu.htm` |

### 1.2 Element detail

**Menu bar** (`add_menu.htm`) — purpose: discoverable access to all commands. Position: top strip.
Interaction: user-replaceable; `Configuration → Change main menu` swaps the `.mnu` file. Keyboard:
`F9` activates the menu above the active panel; `F10` activates the left menu (or leaves the menu);
`ALT+letter` jumps to an underlined item. Groups: Files, Mark, Commands, Net, Show (named Left/Right
in the "old style" menu), Configuration, Start, Help. The Show menu carries per-panel view settings
(sort order, displayed file types); Commands holds actions with no function key (DOS shell, find);
Start is fully user-defined and can receive the file under the cursor as a parameter.

**Button bar** (`add_btnbar.htm`) — a configurable strip of icon buttons that can run menu commands,
external programs, or switch to another bar (subbar). Tooltips appear after a short hover
(`ToolTipDelay`). `SHIFT`+drag a file onto the bar adds a button; right-click opens a local menu for
that single button; full editing is `Configuration → Change Button Bar`. Keyboard: `ALT+SHIFT+F11`
focuses it (`cm_FocusButtonBar`), then cursor keys / `TAB` / `SHIFT+TAB` navigate, `ENTER` activates,
`ESC` leaves, `SPACE` / `SHIFT+F10` / menu key open its context menu. Default file is `default.bar`
from the program directory; if that directory is not writable the file is copied next to
`wincmd.ini`. `redirect=0` inside the bar file suppresses that redirect. Any `.bar` can be promoted
to default (via `>>` in the Change-button-bar dialog, then confirming the prompt).

**Vertical button bar** (`add_btnbar.htm`) — same configuration model as the horizontal bar. Hidden
via right-click between/below icons → "Hide button bar"; restored via right-click on the panel
separator → "Show vertical button bar". Keyboard focus: `ALT+SHIFT+F12` (`cm_FocusButtonBarVertical`).

**Drive button bar** (`add_drivebuttons.htm`) — off by default, enabled in configuration; gives
one-click drive changes. Right-click a button for a context menu (copy/format disk, properties…).
Four button kinds are documented: normal drives (letter + type icon), Network Neighborhood (`\`),
FTP connections (digits `0..9`), and USB devices without a drive letter. A separate bar per file
panel can be shown ("two drive button bars, fixed width, above file windows" — `dlg_config.htm`).

**Drive selection box / combo box** (`add_drive.htm`) — opens a list of drives via the down-arrow or
`ALT+F1` (left) / `ALT+F2` (right). Shows volume labels or `\\server\share` names; unconnected
network drives are prefixed with `(X)`; floppy labels are deliberately omitted for speed (delay
configurable via `DriveTipDelay`, default 0 = immediate). The list also contains "My Computer" and a
"My documents" shortcut, plus up to 3 external devices lacking drive letters (TC9+). In terminal
server / XP-mode environments host drive letters are suffixed with `\` and must be picked with
`CTRL`+letter. Typing a drive letter jumps to it. `SHIFT`+drive selection opens the drive in a *new
tab* (`foldertabs.htm`). Right-click works directly inside the dropdown without first switching.
Per-drive friendly names can be added in `wincmd.ini` under `[DriveHints]` (`E=ZIP drive`).

**Free-space label** (`add_freemem.htm`) — shows the volume label in brackets followed by free and
total size of the current volume in kilobytes (1024-byte units).

**Root / parent buttons** (`add_root.htm`) — left button jumps to the root of the current volume,
right button to the parent directory. Keyboard equivalents: `CTRL+\` (or `CTRL+<` on European
layouts) for root, `CTRL+PgUp` / `BACKSPACE` for parent (`keyboard.htm`). See also `CTRL+left/right`
in the synchronize/asymmetric semantics section.

**Folder tabs** (`foldertabs.htm`, `dlg_configfoldertabs.htm`) — one tab header per panel, holding an
independent list of directories; each tab preserves sort order, brief/full mode and the selection.
Creation: `CTRL+T` (new tab, same directory), `CTRL+UP` (folder/archive under cursor in a new tab),
`SHIFT`+drive selection, `SHIFT` while using the directory hotlist, double-click / middle-click on
empty tab-header space, or dragging multiple folders onto empty header space. `CTRL+SHIFT+T` creates a
tab without activating it. Switching: mouse click, `CTRL+TAB`, `CTRL+SHIFT+TAB`; `CTRL+SHIFT+A`
shows the tab list as a menu and toggles between titles and paths; `CTRL+L` puts recently visited tabs
first. Closing: `CTRL+W`, middle click, double click (if enabled), ctrl+click, or the context menu.
Locking: tabs can be renamed and locked — fully locked (directory cannot change; changing directory
spawns a new tab) or "locked, but directory changes allowed" (returns to the saved directory on
re-activation, on the root button, or on re-clicking the active tab). Locked tabs are marked with `*`;
a locked tab whose current directory differs shows a leading `!`. Tabs can be duplicated, copied to the
other panel, saved to and loaded from `.tab` files, and reordered by drag & drop (even across panels).
`Configuration → Options → Folder tabs` controls: header visibility with a single tab, multi-line
tabs vs. scroll buttons, uniform width, max title length, confirm-before-close-all, double-click close,
`CTRL+UP` foreground behaviour, `CTRL+TAB`-as-list behaviour, insertion position of new tabs, asterisk
on locked tabs, whether clicking an inactive panel's tab activates that panel, drive letter in the tab
title, tab icons and their priority, return-to-locked-root, and whether the Show→Custom file filter is
stored per tab or per panel.

**Current directory bar / breadcrumb** (`add_curdir.htm`, `dlg_config.htm`) — shows the path above the
file list; when inside a packed file the archive name plus the internal subdirectories are shown. With
the clickable-path option: hovering underlines the path element under the cursor and reveals a small
triangle button beneath it. Clicking the underlined element jumps there; clicking the triangle opens a
list of that directory's subdirectories, and moving the mouse right inside that list descends further.
Triangle states are documented: none = no subdirectories (or none currently accessible), filled =
listable subdirectories exist, hollow = not yet determined (e.g. virtual folders). Keyboard: `ALT+F11`
/ `ALT+F12` move the cursor into the path bar, left/right arrow or `TAB` move between path elements,
`ENTER` / down-arrow open a directory or drop the subdirectory list, `ESC` closes the list or leaves
the bar. A single click *outside* the path area turns the bar into an editable field (copy the path out,
or paste a new one; `ENTER` navigates and the edit box stays open if the target is unreachable);
auto-complete suggests matching directory names, and picking a suggestion requires a second `ENTER`.
Double-click on the line opens the directory hotlist; right-click opens the context menu for the
current directory; a slow (≈1 s) right-click opens the visited-directory history. Options:
show/hide breadcrumb behaviour, auto-open submenu on hover (≈250 ms delay instead of showing the
triangle), and show/hide the hotlist and history buttons.

**Directory Hotlist and History buttons** (`add_hothist.htm`, `directoryhotlist.htm`) — `*` opens the
user-maintained hotlist (bookmarks); the down-arrow opens recently visited and most frequently used
directories. Both lists support type-ahead quick search and filtering, with a leading `*` searching the
whole path rather than just the final name segment. Context menu can open an entry in a new tab, in the
other panel, or in both; remove entries; copy the path to the clipboard; show the un-thinned history;
reverse the order; and show/hide the most-frequently-used section. Hotlist is also reachable via
`CTRL+D` or a double-click on the path header, and contains an "Add/remove current dir" entry plus
"Configure".

**Tabstop header** (`add_tabstopheader.htm`) — the sortable column header. Click a header to sort by
name / extension / size / date; clicking the same header again reverses. A small arrow left of the
header text shows direction. `CTRL+click` or `SHIFT+click` on a header defines an *additional*
(secondary, tertiary…) sort key, used to break ties in the primary key. With a header already in use:
`SHIFT` removes that key, `CTRL` reverses its direction. Tabstop positions can be dragged in the
header, but require `Configuration → Save position` to persist. Right-click the header to choose a
different view (Brief / Full / custom-columns views, plus plugin-specific views). When listing an
archive, right-clicking `[auto]` changes the name encoding (e.g. Cyrillic ZIPs). Double-clicking a
separator in full or custom-columns view auto-fits the column to the left of the separator (in custom
columns only fields already loaded at click time are measured).

**File window** (`add_filelist.htm`, `add_srcdir.htm`) — see section 3.

**Count / selection status line** (`add_markedfiles.htm`) — sits under the file window and reports
the bytes occupied by the marked files, then the bytes occupied by *all* files in the window, plus the
number of marked files and the total file count. Options add the directory counts (`5 / 7 dir(s)` —
`dlg_configtabs.htm`), and the size-format for header/footer is configurable independently
(`Header/footer` size styles). The header above the list shows total/free space for the volume.

**Command line** (`add_cmdline.htm`, `cmdline.htm`) — accepts typed text, `ENTER` executes. Supports
environment variables, a history list, aliases, internal `cm_` / `em_` commands, and a set of
`OPENTABS`-style parameterised commands written in capitals. `cd` is intercepted to change the active
panel's directory; giving a plain file name puts the cursor on it. Paths in `c:\Windows`,
`\\server\share\dir`, and several `file:///…` URL forms are accepted. While the active panel shows an
FTP directory, typed commands are sent to the server (`cd` lowercase is translated to the FTP command,
`CD` uppercase is handled locally); the same lowercase/uppercase split applies to file-system plugins.
Limits: 2047 characters on Windows 2000, 8191 on Windows XP and later. If the command line is hidden it
auto-appears as soon as a character is typed. Key map: `ENTER` execute (or launch the file under the
cursor when empty; `cd`/`md`/`rd` handled internally; internal DOS commands run via DOS),
`SHIFT+ENTER` execute via the Windows shell and keep the DOS window open (needs `noclose.exe`),
`CTRL+ENTER` append the cursor file, `CTRL+SHIFT+ENTER` append it with path, `CTRL+CURSORDOWN` /
`ALT+F8` open the history (arrow keys select, left/right return to editing, `SHIFT+DEL` deletes an entry),
`TAB` return focus to the file list, `ESC` clear and return focus, `CTRL+A/C/X/V/Y/K/W/T` as text
editing, `CTRL+E` recall the previous command. A leading `*` runs as administrator, `**` as a different
user; `*cmd` is rewritten to `cmd.exe` in System32.

**Function key bar** (`add_buttons.htm`) — clickable buttons plus `F1`–`F8`. Buttons exist only for
`F3..F8`; `F1` (help) and `F2` (reread) deliberately have none "to avoid overcrowding". `F3`, `F4` and
`F8` buttons are drag & drop targets. Right-clicking the `F8` button opens the Recycle Bin.

**FTP toolbar** (`ftp_menu.htm`) — appears after an FTP connection is established and exposes the
transfer mode (binary / text / automatic; shortcut `CTRL+SHIFT+M`).

**Panel separator** — draggable to change the relative panel width **(inferred)**; its right-click menu
offers "Show vertical button bar" (`add_btnbar.htm`). Commands `cm_100Percent` / `cm_50Percent`
manipulate the split numerically (`dlg_choosecommand.htm`).

### 1.3 Visibility toggles

`Configuration → Options → Layout` (`dlg_config.htm`) is the master switchboard for the elements above:

| Toggle | Effect |
|---|---|
| Show button bar | horizontal bar |
| Show button bar 2 (vertical) | vertical bar |
| Show drive buttons | drive button bar |
| Show two drive button bars (fixed width, above file windows) | per-panel drive bars, equal width, not adapted to panel width |
| Flat icons | flat drive-bar icons |
| Show drive combobox | drive dropdown |
| Show folder tabs | tab headers (hidden automatically when only one tab unless overridden) |
| Show current directory | path header — *this header also indicates the active window* |
| Show clickable path parts ('breadcrumb bar') | underline/triangle behaviour |
| Auto-open when moving mouse over it | submenu on hover (≈250 ms) instead of the arrow button |
| Show buttons for history list and hotlist | `*` and triangle behind the path |
| Show tabstop header | column header |
| Show status bar | total & selected file counts and their space |
| Show command line | command line (auto-revealed on typing) |
| Run command line only when focused | `ENTER` in a panel no longer executes the command line |
| Show function key buttons | the F-key bar |
| Flat user interface | flat widgets for everything except button bar / drive bar |
| Windows XP theme background (menu+all bars) | themed background under menu, button bar, drive bar, FTP toolbar |

---

## 2. Two-panel model

### 2.1 Source and target

* Two independent file windows are shown; the one "surrounded by a frame" is the **source** panel and
  receives keyboard focus and cursor-key navigation (`add_srcdir.htm`).
* The **target** panel is where `F5` copy / `F6` move write by default. The target path is
  pre-filled from the target panel and the target directory is created automatically if it does not
  exist; `..` or `\` are accepted as target and are resolved relative to the source; an empty target
  means "same as source" (`add_trgpath.htm`).
* The target panel's own directory is used as the default destination string, so an implicit copy with
  an unchanged dialog copies across panels.

### 2.2 Active-panel indication

Three mechanisms are documented:

1. A frame around the source window (`add_srcdir.htm`, `proc_newread.htm` — "the source window (with
   the thick frame around it)").
2. The current-directory header at the top of each panel is drawn with separately configurable text
   and background colours for the active vs. inactive side
   (`dlg_configcolor.htm` — "Text/background color of the active side", "…inactive side"), and the
   Layout page states that this header "also indicates the active window".
3. Cursor placement semantics: all panel-level keys (arrows, `ENTER`, `INSERT`, `NUM +/-`) act on the
   focused panel; `TAB` moves focus.

### 2.3 Switching and swapping

| Action | Keys |
|---|---|
| Switch focus between the two file lists | `TAB` |
| Switch between file list and separate tree | `SHIFT+TAB` (when a separate tree is enabled) |
| Exchange panel contents (sort order etc. retained) | `CTRL+U` (`options_swap.htm`) |
| Exchange panel contents **and** tabs | `CTRL+SHIFT+U` |
| Open directory/archive under cursor in the *other* panel | `CTRL+LEFT/RIGHT`; falls back to Target=Source when the cursor is not on a directory or the other panel is already active (`options_match.htm`) |
| Set target = current source path | `Target=Source` command / `CTRL+LEFT/RIGHT` fallback |
| Put cursor in the target panel's current file | `%M` parameter semantics (`dlg_usermenu.htm`) |

### 2.4 Layouts

* **Side by side** is the default; **`Show → Vertical alignment`** stacks the two file windows one
  above the other (`vertical_alignment.htm`), toggleable by `cm_VerticalPanels`
  (`dlg_choosecommand.htm`).
* **Panel split ratio** is adjustable by dragging and by `cm_100Percent <n>` (value applied to the
  active panel) / `cm_50Percent <±n>` (value added to the active panel's width).
* When the Quick-view panel is resized and `Configuration → Save position` is used, that width is
  remembered *independently* of the normal panel sizes used when Quick view is off
  (`quick_view.htm`).
* Drive assignment is per panel (independent drive combo boxes), and a per-panel drive button bar may
  be enabled.

### 2.5 Quick view

`CTRL+Q` (`quick_view.htm`, `cm_*QuickView`). The content of the file under the cursor in the active
panel is rendered in the **other** panel; the bar above the content shows the file name plus the name
of the Lister plugin used, or (for music) track and artist. When the cursor rests on a folder,
occupied-space information is shown instead. The pane is a real Lister window, so Lister shortcuts apply
(`CTRL+F` / `F7` to search). Toggle off with `CTRL+Q` again or by selecting another drive in that panel.
`CTRL+SHIFT+Q` opens a **separate** Quick-view window which follows the cursor; its title bar shows
`Lister *` to distinguish it from a normal Lister window, and `Options → Save position` stores its own
position (independent of normal Lister windows).

### 2.6 Tree panel and separate tree

* `Show → Tree` (`CTRL+F8`) replaces the panel's file list with a directory tree (`show_tree.htm`).
  By default only top-level directories are read (Explorer-like); branches expand with `+`/`-` on the
  numeric keypad or by clicking the `+`/`-` glyph. The legacy mode (full tree up front) is enabled via
  `Options → Display → Always load complete directory tree`; the structure is then cached in
  `treeinfo.wc` in the drive root if writable, otherwise in `%LOCALAPPDATA%\GHISLER\`, and only that
  file is updated on directory create/delete. External changes require `F2`/`CTRL+R` to rescan.
  Selecting Tree in the left menu shows the tree of the *right* panel's drive. Cache file location
  override: `TreeFileLocation=` in `[Configuration]`. In tree view, `ENTER` opens the directory in the
  other window by default — the Display option "Tree: press ENTER to open dir in other window"
  inverts this.
* `Show → Separate tree` (`show_separatetree.htm`) adds one, two or no tree **panels** beside the two
  file panels. Separate trees support only expandable (never fully pre-expanded) trees. One tree
  follows and scrolls to the active panel's directory; two trees give each panel its own. The tree is
  lazily expanded — first expansion happens at the first directory change or panel swap. Supported
  operations inside the separate tree: copy/cut/paste, delete, rename (`SHIFT+F6`), properties
  (`ALT+ENTER`), right-click context menu, drag & drop. `CTRL+SHIFT+F8` cycles one tree → two trees →
  off.

### 2.7 Quick search and quick filter (panel-level)

`dlg_quicksearch.htm`, `dlg_quicksearch` config page. Modes configurable: `CTRL+ALT+letter`,
`ALT+letter`, letters only (Explorer style, ~1 s forget delay), letters with a visible search dialog,
or disabled (for `cm_ShowQuickSearch` on a hotkey). In dialog modes a leading `*` searches anywhere in
the name; a filter toggle button hides non-matching files, and an active filter persists after the
dialog closes, signalled by a filter icon in the panel's status bar; cleared by changing directory,
`ESC`, or `F2`/`CTRL+R`. `CTRL+S` opens the quick-filter dialog and toggles the filter;
`CTRL+SHIFT+S` re-applies the last filter; `CTRL+F10` clears it. In a separate tree, quick search maps
`ENTER` = open directory, `ESC` = back, `INSERT` = stay on the new entry.

---

## 3. File list rendering

### 3.1 Display modes

All switched from the **Show** menu (`menu_show.htm`) and by hotkey (`keyboard.htm`).

| Mode | Hotkey | Content / notes |
|---|---|---|
| **Brief** | `CTRL+F1` | file names only, laid out in multiple columns; column width may be capped ("Limit width of brief view column to:") |
| **Full** (details) | `CTRL+F2` | name, size, date+time, attributes |
| **Comments** | `CTRL+SHIFT+F2` | comments from `descript.ion` or `files.bbs`; if both exist the preferred type from `Configuration → Operation` wins |
| **Custom columns** | via `SHIFT+F1` menu | up to 29 user-defined column sets (`show_customcolumns.htm`) |
| **Custom view modes** | Show menu / `cm_*ViewModeList` | named presets bundling column mode + sort order + panel background + tab colour/icon + auto-run commands (`show_customviewmodes.htm`, `dlg_configviewmode.htm`) |
| **Tree** | `CTRL+F8` | directory tree in place of the file list (section 2.6) |
| **Thumbnail view** | `CTRL+SHIFT+F1` | previews; size configurable; optional extra user-defined text lines below the name; zoom 10–200 % in 10 % steps with `CTRL`+wheel, `cm_ZoomIn`/`cm_ZoomOut`. Works in normal file mode only — not in archives, not on FTP; some FS plugins support it; in virtual folders Lister-plugin thumbnails cannot be loaded |
| **Quick view** | `CTRL+Q` / `CTRL+SHIFT+Q` | Lister in the other panel / separate window (section 2.5) |

View switching is also possible from the **tabstop header's right-click menu**, which additionally
offers plugin-specific column views and archive-name encodings.

### 3.2 Columns and their meaning

| Column | Content | Notes |
|---|---|---|
| Name | file or directory name | directories rendered as `<DIR>`; NTFS symlinks as `<LNK>`; optionally wrapped in `[ ]` or suffixed with `\` (both combinable) |
| Extension | shown appended directly to the name (Explorer style) or TAB-aligned in its own column | Tabstops page controls which |
| Size | bytes / kbytes / dynamic with 0–2 decimals, units k/M/G/T | `Size display` (list) and `Header/footer` styles are separate settings |
| Date | date of last modification, month-first / day-first / ISO, optional 4-digit year | century display toggle; separator characters configurable |
| Time | time of last modification, 24 h or 12 h AM/PM, optional seconds | separator configurable |
| Attributes | letter flags (see 3.4) | in a column to the right in full view |
| Comments | comment text | in Comments mode |
| Custom columns | arbitrary field expressions `[=plugin.field.unit]`, `[="prefix"plugin.field"suffix"]`, partial ranges `:4-7`, alignment left/right, per-column caption and width | `dialog_usercolumns.htm` |
| UNIX permissions (FTP) | 3-digit octal number | `dialog_unix_attribs.htm`: first digit owner, second group, third world; 4 = read, 2 = write, 1 = execute; `754` = owner all, group read+execute, world read |

Header/footer aggregates: the status footer shows marked-vs-all bytes and marked-vs-total file counts,
optionally extended with directory counts.

### 3.3 Sorting

`show_sort.htm`, `add_tabstopheader.htm`, `dlg_configtabs.htm`.

* Primary keys: **Name**, **Extension**, **Size**, **Time**, **Unsorted** (only separates files from
  directories, otherwise DOS `dir` order).
* **Reversed order** inverts the whole ordering (`Z→A`, largest→smallest, newest→oldest).
* Documented default directions: Name and Extension sort ascending; Size and Time sort descending,
  "since these combinations are the most used".
* Hotkeys `CTRL+F3` name, `CTRL+F4` extension, `CTRL+F5` date/time, `CTRL+F6` size, `CTRL+F7` unsorted.
* **Secondary/tertiary sort keys** are added by `CTRL`/`SHIFT`+clicking header columns; a continuous
  block of equal primary keys is then ordered by the next key. Clicking an already-active key with
  `SHIFT` deletes it, with `CTRL` reverses it.
* The sort direction indicator is a small arrow drawn to the left of the header caption.
* Directory sorting is a separate setting: always by name ascending, or "like files" (also
  chronological and reversible).
* Sort algorithm/compare function is configurable on the Display page: locale-aware alphabetical with
  accents, strict numeric character code, natural sort (character code + numbers, so `file5` before
  `file10`), or natural + accents (requires an IE6-era OS function).
* Programmatic control: `cm_*ByName`, `cm_*ByExt`, `cm_*BySize`, `cm_*ByDateTime`,
  `cm_*NegOrder`, `cm_*SortByCol1..99` accept `0`=toggle, `1`=ascending, `2`=descending, plus bit `4`
  = "also use additional sort columns", `8` = store sort state, `9` = restore it
  (`dlg_choosecommand.htm`).

### 3.4 Attribute markers and colour

Attribute letters shown in full view (`add_filelist.htm`):

| Letter | Meaning |
|---|---|
| `r` | read-only |
| `a` | archive bit set |
| `h` | hidden |
| `s` | system |
| `c` | NTFS-compressed |
| `e` | NTFS-encrypted |

Other markers:

* `<DIR>` for directories (or `[name]` / `name\` per Display settings); `<LNK>` for NTFS symbolic links.
* FTP/UNIX panels additionally show the 3-digit permission number (`dialog_unix_attribs.htm`).
* Icon column (when enabled) encodes type: folder, archive (parcel), program (window), associated file
  (the associated program's icon), plain file (white sheet), and hidden/system (sheet with `!`). Icon
  modes: all icons incl. EXE/LNK extraction (slow), associated files only, standard icons only, no
  icons; plus "EXE/LNK not on floppy", "EXE/LNK not on net", special-folder icons, Explorer default
  icons, overlay icons (LNK arrows and shell overlays), dimmed hidden files, optional `!` badge on
  hidden/system, selectable icon size and horizontal/vertical padding (`dlg_configicons.htm`).
* Ignored items from the ignore list are not shown at all and are excluded from counting, copying,
  moving and synchronize (`dlg_configignorelist.htm`).

Colour model (`dlg_configcolor.htm`, `def_colors_by_file_type.htm`):

* **Font colour** and **background** for the panels; **background 2** colours every even row.
* **Mark colour** (default red per `add_filelist.htm`) for selected files; optional **inverted
  selection** (solid bar instead of coloured text).
* **Cursor colour**; cursor drawn either as a rectangle frame around the current file (default, "dashed
  inverted frame"), as a solid/inverted bar, or using the current Windows theme. The cursor text
  colour can be overridden.
* Note in `add_filelist.htm`: "Marked files are shown in red (can be changed in Configuration-Colors).
  The current file is surrounded by a fine rectangle."
* Panel title bar colours for the active vs. inactive side.
* **Define colours by file type**: an ordered filter list, each entry a file-type/attribute filter plus
  a colour; **the order defines priority** (the first matching filter wins). Filters may be quickly
  enabled/disabled by clicking the leading `*`.
* Separate colour sets exist for Lister, Compare-by-content (text and binary, selected/unselected,
  equal/different), Synchronize-directories selected lines, hint windows, and dark mode (text,
  background, highlight, highlight-2, border, horizontal separators).
* Dark mode supports a tri-state: always normal, always dark, or follow the Windows app-mode setting.

### 3.5 Selection and cursor

* Selection: mouse click (button configurable left/right), `INSERT`, `SPACE`, and since 8.5 a single
  click on the file's icon. `SPACE` on an unselected *directory* also counts the directory's contents
  and replaces `<DIR>` with that size in full view (option "Selection with Space").
* Ranges: click first item, `SHIFT`+click last; with right-button selection mode you may click-drag
  over the range; with left-button mode dragging *behind* the file name draws a rubber band
  (Explorer-like).
* Numeric-keypad families (all documented in `proc_select.htm` and `keyboard.htm`):
  `NUM+`/`NUM-` select/unselect by mask, `NUM*` invert, `NUM/` restore previous selection,
  `CTRL+NUM+`/`CTRL+NUM-` all/none, `ALT+NUM+`/`ALT+NUM-` same extension as the cursor file,
  `SHIFT+NUM*` and `CTRL+SHIFT+NUM+/-` flip the "files only vs. files+folders" variant.
  **`NUM-` always unselects files and folders, and `CTRL+NUM-` always unselects everything, for
  safety**; `SHIFT+NUM-` and `CTRL+SHIFT+NUM-` restrict to files.
* Mask syntax: space-separated masks, `|` separates exclude masks, `>` links related files
  (`*.jpg>*.cr2`), a leading `<` marks a regular expression, and `\subdir\` / `**` scope masks to
  subdirectories.
* Selection is restored automatically after each file operation; explicit save/restore and save/load
  to a text file also exist.
* Copy selected names / names with path / with all details / with path+details to the clipboard are
  Mark-menu entries (`options_mark.htm`).
* The cursor rectangle is the *fine rectangle* by default; an inverted-bar cursor is an option. Extra
  lines above/below the cursor when scrolling are configurable (`ScrollingTopMargin` /
  `ScrollingBottomMargin`, default 4 each).
* After a file operation the cursor stays in the panel, and Quick search/history overlays are closed by
  the next action.

### 3.6 File-type filter in the list

`show_files.htm`, `only_selected.htm`. The Show menu exposes **All files**, **Programs**
(`*.exe;*.com;*.bat;*.pif;*.cmd`), the last user-defined mask, **Custom…** (space-separated include
masks, `|` then excludes; `*.txt | *\` hides all folders; `.cvs\` matches a directory name), and
**Only selected files** (hides everything not selected — useful in huge directories). The per-tab
persistence of the custom filter is a Folder-tabs option.

---

## 4. Dialog inventory

Conventions: "OK/Cancel/Help" plus, in the big Options dialog, a global **Apply** that re-applies
settings to both panels without closing (`options_config.htm`). All Options pages persist to
`wincmd.ini`.

### 4.1 Configuration → Options, all 26 pages

Page order as listed on `options_config.htm`; the documenting page is in the second column.

| # | Page (tab) | Page file | Key contents |
|---|---|---|---|
| 1 | Layout | `dlg_config.htm` | element visibility toggles (see 1.3) |
| 2 | Display | `dlg_display.htm` | see 4.1.1 |
| 3 | Icons | `dlg_configicons.htm` | icon extraction mode, overlay icons, icon size/padding, menu icons, drive icons |
| 4 | Font | `dlg_configfont.htm` | dialog DPI scaling, auto-scale dialogs for large fonts, file-list font, main-window font, dialog font, "Change font" (platform font dialog) |
| 5 | Color | `dlg_configcolor.htm` | see 3.4 |
| 6 | Tabstops / Data Formats | `dlg_configtabs.htm` | see 4.1.2 |
| 7 | Folder Tabs | `dlg_configfoldertabs.htm` | see section 1.2 (folder tabs) |
| 8 | Custom Columns | `dlg_configcustomcolumns.htm` | see 4.1.3 |
| 9 | View Mode | `dlg_configviewmode.htm` | see 4.1.4 |
| 10 | Auto Switch Mode | `dlg_configautomodeswitch.htm` | rule list for automatic view-mode changes |
| 11 | Language | `dlg_configlanguage.htm` | language picker (`.lng`), main menu file (`.mnu`) with `>>`/Edit/Download |
| 12 | Operation | `dlg_config2.htm` | see 4.1.5 |
| 13 | Edit/View | `dlg_configedit.htm` | viewer/editor selection for F3/F4 |
| 14 | Copy/Delete | `dlg_configcopydelete.htm` | copy engine and delete semantics |
| 15 | Refresh | `dlg_configrefresh.htm` | auto-refresh and directory watching |
| 16 | Quick Search | `dlg_configquicksearch.htm` | see 2.7 |
| 17 | FTP | `dlg_configftp.htm` | see 4.1.6 |
| 18 | Plugins | `dlg_configplugin.htm` | five plugin-type sub-dialogs + download link |
| 19 | Thumbnails | `dlg_configthumbs.htm` | see 4.1.7 |
| 20 | Log file | `dlgconfiglogfile.htm` | log file name + per-operation toggles + rotation |
| 21 | Ignore list | `dlg_configignorelist.htm` | ignore list editor |
| 22 | History | `dlg_confighistory.htm` | history/frequency limits and storage |
| 23 | Packer | `dlg_configpack.htm` | external packer paths, internal un-ARJ/LZH/RAR/ACE switches |
| 24 | ZIP Packer | `dlg_internalzipconfig.htm` | internal ZIP + Unicode-name policy |
| 25 | 7-ZIP Packer | `dlg_internal7zipconfig.htm` | internal 7-Zip, compression settings, encryption |
| 26 | Misc. | `dlg_configconfirmation.htm` | see 4.1.8 |

> The page document for Misc. is `dlg_configconfirmation.htm` and the page document for Operation is
> `dlg_config2.htm`; the titles inside those files are "Configuration: Misc." and
> "Configuration: Operation" respectively.

#### 4.1.1 Display page (`dlg_display.htm`)

| Field | Meaning / default / validation |
|---|---|
| Show hidden files | separate from the system-file switch (formerly one option) |
| Show system files | second, independent switch |
| Use long filenames (recommended) | disabling truncates renames to 8.3; cannot be changed while a background transfer runs |
| Show old 8.3 filenames lowercase (like Explorer) | lowercase short names except first letter; may break UNIX network drives |
| Show parent dir also in root of drive | `..` in the root goes to "My Computer" |
| Show square brackets `[]` around directories / Append `\` | both can be combined (tick `Append \` first) |
| Limit width of brief view column to: | caps the brief-view column width |
| Sort method | 4 choices, see 3.3 |
| Always load complete directory tree | legacy full-tree mode (ignored by separate trees) |
| Tree: press ENTER to open dir in other window | inverts default tree ENTER behaviour |
| Sorting directories | always by name, or like files (also by time/reversed) |
| Show help texts for buttons (tooltips) | `ToolTipDelay` |
| Show volume labels/network paths for drives | applies to the drive combo |
| Show filename in file list as tooltip if truncated | ~1 s hover |
| Win32-style tips with file comments | rich tips including comments; hidden on any key press |
| Comments from descript.ion/files.bbs; Comments from Explorer (OLE2); User-defined `[+]` | which comment sources feed the tips; `[+]` allows per-file-type tips via content plugins |

#### 4.1.2 Tabstops / Data Formats page (`dlg_configtabs.htm`)

| Field | Meaning / default |
|---|---|
| Show file extensions: directly after filename / aligned (with TAB) | alignment only editable in the aligned mode |
| Tabstops (extension, size, date/time, attributes) | numeric fields with scrollbars; header drag also works but needs Save position |
| Separators (Date, Time, Thousands, Decimal) | default from Control Panel |
| Adjust tabs to window width | name column resizes so the other columns stay right-aligned |
| Show century in date | `1996` vs `96` |
| Use 12 hour AM/PM format | `a`/`p` suffix else 24 h |
| Show directory count in footer | e.g. `5 / 7 dir(s)` |
| Size display | bytes / kbytes / dynamic with `k/M`, `k/M/G`, `k/M/G/T` and 0, 1 or 2 decimals |
| Header/footer | separate size style for the two headers and two footers; also affects the free-space display and the copy progress dialog unless `SizeTransfer` is set |
| Date format | month-first, day-first, year-first (ISO); seconds toggle |
| Default | resets tabstops to first-run values |

#### 4.1.3 Custom Columns page + column editor (`dlg_configcustomcolumns.htm`, `dialog_usercolumns.htm`)

Configuration page: choose the target file system (`<General>` or a plugin), list of up to 29 column
sets (reorderable by drag & drop), and buttons **New / Edit / Delete / Copy**. Changes are committed on
Apply/OK.

Column editor: a per-column grid with **Name** (menu title; empty = not listed in the menu but still
reachable via `cm_SrcCustomView1..`), **Caption** (header text), **Width** (dialog units), **Align**
(left/right), **Field Contents** (the expression), plus **Add column**, **Delete active column**,
reordering by arrow buttons or `CTRL+PgUp/PgDn`, **Adjust Tabs to window width**, **Horizontal
scrollbar**, and (plugin columns only) **Switch to this view when using this plugin**. Field syntax:
`[=pluginname.fieldname.unit]`, `[=pluginname.fieldname.unit:4-7]`,
`[="prefix"plugin.field"suffix"]` (prefix/suffix only emitted when the field has a valid value).
Date/time formatters: `Y y M D h m s v a P p`.

#### 4.1.4 View Mode page (`dlg_configviewmode.htm`)

A list of named modes (`<default>` is the fallback) with Add / Delete / Rename / reorder. Per mode:
**Columns view** (Brief / Full / any custom-columns set), **Sort order**, **Reversed**,
**Additional sort orders**, **Tab colour + icon** with a **Priority** flag (icon wins over
desktop.ini/drive icons), **Background** and **Background 2** colours (separate for normal and dark),
and **Auto-run commands** supporting `cm_`/`em_` commands with modifiers: a leading `-` runs them only
on directory changes inside a tab (not on tab switches), a leading `*` runs them on every directory
change, `|` separates commands to run when leaving the mode (e.g. `cm_100percent|cm_50percent` widens
the active panel while the mode is active and restores 50:50 on exit).

#### 4.1.5 Operation page (`dlg_config2.htm`)

| Group | Field | Meaning |
|---|---|---|
| Main program | Allow only 1 copy of Total Commander at a time | second launch activates the first |
| | Move icon to system tray when minimized | `ALT+TAB` still works |
| | Always go to the root of a drive when changing drives | otherwise last-used directory per drive |
| | Open files and directories with a single click | |
| | Select only the file name when renaming (not the extension) | governs `SHIFT+F6`; repeated `SHIFT+F6` toggles |
| | Auto-complete paths / Auto-append suggested name | dropdown vs. inline completion (`TAB`/`CTRL+SPACE` to accept) |
| | Check for updates | includes beta channel |
| | Extra lines below cursor | default 4; `ScrollingTopMargin` / `ScrollingBottomMargin` |
| | Mouse selection mode: right / left button | plus **Rubberband selection** (left mode only) and **Select by clicking on icon** (+ "Only on Touchscreen") |
| | Selection (with NUM +): files only / also directories | |
| NTFS DST | Ignore 1 hour time delay | for FAT32↔NTFS comparisons |
| Save on exit | Directories / Panels / Old command lines | also saves tabs |
| Calculate space | when selecting with SPACE: subdirectories / via 'Everything' | |
| | Before copying/deleting | pre-counts size to enable a second progress bar |
| File comments | Preferred type: `descript.ion` / `files.bbs`; encoding (Plain text, Unicode, UTF16/UTF8, `files.bbs`); DOS charset; Copy comments with files; Read from both | `files.bbs` always uses the DOS charset |

#### 4.1.6 FTP page (`dlg_configftp.htm`)

Retry count and delay, default transfer mode (binary/text/automatic), where new connections open
(active panel / left / right / inactive), log file, "all transfers in background", "passive mode by
default", PORT-mode limited port range and external address (static IP, DynDNS name, or a URL of a
script that echoes the client IP), MODE Z compression with "Not on LAN", and auto-resume after *n*
seconds without data.

#### 4.1.7 Thumbnails page (`dlg_configthumbs.htm`)

Database location (default `%LOCALAPPDATA%\GHISLER` on NT-family), copy/delete of thumbnails with files
(done in a background thread), custom field lines below the thumbnail (`\n` for line breaks), thumbnail
size (multiples of 16 recommended), per-file-type field sets, and an ordered loader chain with
include/exclude masks per loader: internal EXIF/RAW thumbnails, Lister plugins, Explorer OLE2 method,
IrfanView/XnView, plain-text previews. Database maintenance: compact, drop thumbnails for missing files,
for uninserted CDs, for inaccessible disks.

#### 4.1.8 Misc. page (`dlg_configconfirmation.htm`)

Confirmation switches (all on = "maximum security" by default): deleting non-empty directories,
overwriting files, overwriting/deleting read-only files, overwriting/deleting hidden/system files,
Drag&Drop copy destination prompt. Then the **Redefine hotkeys** table (hotkey / alias / command /
parameters, with `cm_`+`em_` commands chosen through the "Choose command" dialog, `>` and a
magnifying-glass picker, OEM_ key codes, `CTRL`+letter not remapped while in the command line), and
**Custom sounds** (WAV per operation, "only play for long operations" above *n* seconds).

### 4.2 File-operation dialogs

| Dialog | Purpose | Fields / controls & meaning | Defaults, validation | Keyboard |
|---|---|---|---|---|
| **Copy** `proc_copy.htm` (F5) | copy files/dirs | target path + file mask combobox (offers all open tabs and target subdirs); **Only files of this type** filter (space-separated, `\`/`**\`/`\|` scoping, plus a `+F8` search-filter picker); **Tree** button (directory tree chooser with drive combo); **Copy NTFS permissions**; **Verify**; **Keep relative paths** (branch view); **Options** (overwrite default: ask / overwrite all / skip all / overwrite all older, plus "ignore read-only/hidden/system", pinnable); `+F7` add the current target to the dropdown; `F2 Queue`; **OK** | target defaults to the target panel + `*.*`; empty target = source; non-existent target auto-created; `..`/`\` relative to source. Wildcard rewrite rules documented (`?`, `*`, `*rest`, `start**`, `start*?`) | pressing F5 again cycles selection: path+name → name only → name without extension; `CTRL+HOME` puts the caret before the file name; right-click on **OK** or **F2 Queue** inverts copy↔move |
| Copy **progress** dialog `proc_copy.htm` | live progress | copied/total files and copied/total bytes, each clickable to switch to remaining (shown with a leading minus); hover shows full names; **Background** button; a second overall percentage bar appears when the pre-count option is on | moved back to the foreground if no other dialog is open | |
| **Overwrite** `dlg_overwrite.htm` | per-collision decision | source and target details, optional **Thumbnails** and **Custom fields**; buttons **Overwrite**, **Overwrite all**, **Skip**, **Cancel**, **Overwrite all older**, **Skip all**, **Rename**, **Append**, **More Options >>** → Compare, Rename existing target, Auto-rename copied, Auto-rename target, Rename older target/skip newer, Overwrite all older *and same age*, Copy all larger, Copy all smaller | "Overwrite all older" first compares internal version numbers (EXE/DLL) then the timestamp; read-only and hidden/system files still prompt separately per configuration | `ALT`+letter accelerators, `F3` view target, `SHIFT+F3` view source, `ALT+F3`/`ALT+SHIFT+F3` view without plugins, `CTRL+F3` compare by content, `CTRL+SHIFT+F3` force internal compare |
| **Rename/Move** `proc_renmov.htm` (F6) | rename in place or move | same target-path + mask combobox; **Tree** chooser; "Rename each file separately" when multiple files selected | free target = rename in place; free mask = name unchanged; unavailable inside archives except single `SHIFT+F6` renames in ZIP | second `F6` cycles path → name → name w/o extension; `SHIFT+F6` = in-place edit box, `ENTER` commits, click outside cancels, repeated `SHIFT+F6` toggles name/extension selection; inline rename can be disabled with `InplaceRename=0` |
| **Create directory** `proc_mkdir.htm` (F7) | new subdirectory | name field pre-seeded from the cursor entry; `[+]` inserts multi-rename placeholders when the name starts with `<`; `\` creates nested dirs; `\|` creates several dirs at once | `SHIFT+F7` creates in the target panel; unavailable in archives except ZIP/RAR/TAR; range/placeholder syntax `<1-250>Project No [C:3]`, `[C]` with prefix/suffix and optional digit | — |
| **Delete** `proc_delete.htm` (F8/DEL) | delete | confirmation dialog(s); extra prompt per non-empty directory; **Cancel** aborts at any time; automatic "As Administrator" retry on failure | Recycle Bin used when available; `SHIFT`+`F8` bypasses it (configuration may invert this); archives delete through the packer without a Recycle Bin | |
| **Test archives** `options_testarchives.htm` | integrity | unpacks to memory and verifies CRC; TAR cannot be tested | `ALT+SHIFT+F9` | |
| **Compare by content** `dlg_compare_contents.htm` | see 4.6 | | | |
| **Properties** `options_versioninfo.htm` | shell property sheet | identical to Explorer; version info on the Details page | `ALT+ENTER`; on `..` shows the current directory's properties | |

### 4.3 Search family

**(a) Find files — General** (`dlg_search.htm`), pages General / Advanced / Plugins / Load-Save.

| Field | Meaning / rules |
|---|---|
| Search for: | masks with `?`/`*`, multiple masks space-separated, `\|` then exclude masks; `\**\` for any depth; names with spaces need no quotes *unless* an exclude mask is used; a string containing a dot is matched exactly; `SHIFT+DEL` removes a history entry; a leading `>` on save means "don't overwrite this field when loading"; `ev:` passes the query to Everything, `ed:` also restricts it to "Search in" |
| Search in: | one or more start paths separated by `;` (all of one kind); may point inside an archive; `>` prefix as above; `@file.txt` reads a list of files/directories (ANSI/UTF-8/UTF-16 with BOM), supporting wildcards and regexes with documented interaction rules for the subdirectory depth setting |
| `>>` / Drives | choose a start directory / select multiple drives |
| RegEx | regular expressions on file names |
| Only search in selected directories/files | restricts to the active panel's selection; forcibly unchecked and warned about when using the separate-process search |
| Everything | uses the Everything MFT index; only name search is accelerated unless Everything ≥ 1.4 |
| Search archives | ZIP, ARJ, LZH, RAR, TAR, GZ, CAB, ACE, 7Z; content search inside archives works (unpack → search → delete on the fly) |
| Search in subdirectories | all / current dir only / a fixed depth |
| Find text | `\t` tab, `\n` newline, `\\` backslash; warning if the text cannot be converted to the chosen encodings |
| Whole words only / Case sensitive / RegEx (2) / Hex | text-match modifiers; hex accepts mixed forms like `"PK"0102`; case-insensitive hex also matches the other case of letters |
| Find files NOT containing the text | invert the content match |
| Encodings | multi-selectable: ANSI (Windows), ASCII (DOS), Unicode UTF-16, UTF-8, Office-XML + EPUB; plus plugin fields |
| `<` / `>` | history of previous search results |
| F2 "Search in found files" | iterate the search inside the current result list (semantics depend on the depth setting) |
| Result list | double-click a hit to go to its directory; `CTRL+C` copies the whole list; details shown in the footer because of space |
| Status line | shows the directory currently being scanned; clicking a path part skips that directory after confirmation |
| Feed to listbox | the hits become a virtual panel; `SHIFT` opens it on a new tab; `F2`/`CTRL+R` returns to normal display; if all hits came from one archive the archive contents are shown, otherwise the archives themselves |
| Go to file | jump to the directory of the selected hit (`SHIFT` = new tab) |
| Start search / Cancel | `ESC` interrupts; dialog position is remembered |

**(b) Advanced** (`dlg_advancedsearch.htm`): date range **between** the two fields, **not older than**,
**older than** (combined with AND to form a band, e.g. older than 7 but not older than 14 days), file
size equal/smaller/larger with units B/k/M/G (factor 1024) and fractions, attribute filters (hidden,
system, directory, NTFS-encrypted), and the **Find duplicate files** block: same name / same size /
same contents / same plugin fields (all specified fields must match; value ranges supported). Result
groups are separated by dashed lines and sorted by the first enabled criterion.

**(c) Plugins** (`dlg_searchplugins.htm`): enable/disable list, AND/OR combination of rules, and per
rule: Plugin (`tc` = internal properties), Property, operator (=, !=, `=(case)`, `!=(case)`, >, <, >=,
<=, contains, !contains, `cont.(case)`, `!cont.(case)`, regex), Value (dates in the main-window format,
optional units). Content plugins do not work in virtual folders, FTP, archives or FS plugins. Rules are
reorderable with `CTRL+CURSOR UP/DOWN`.

**(d) Load/Save** (`dlg_srch_loadsave.htm`): list of saved searches with **Load** (jumps back to the
General page), **Save**, **Delete**. When a saved search is used as an F5 copy filter, these General
fields are ignored: Search in, Only search in selected directories/files, Everything, Search archives,
Search in subdirectories.

**(e) Select duplicate files** (`dlg_duplicates.htm`) — reached with `NUM+` after feeding duplicates to
the listbox. Tab *Files*: select/remove by name or extension (with `>` saved searches), then by
date/time (all older, all newer, just the oldest, just the newest) and by size (all larger than the
smallest, all smaller than the largest, just the largest, just the smallest), each relative to the
group. Tab *Select by folder*: folder checklist, "also select all subfolders", select/remove by pattern
(optionally restricted to the direct parent folder name), All/None/Reverse. "Leave at least one file in
each group unselected" resolves by up to three ordered criteria. Buttons OK / Cancel / Help / **Apply**
(apply without closing).

**(f) Quick search box** (`dlg_quicksearch.htm`): small box under the active panel, `CTRL+ALT+letter`
opens it and the modifiers may be released immediately; `DOWN`/`UP` step through matches; `ESC`
closes; the box auto-closes when another action starts. `AltGr` works as `CTRL+ALT`.

### 4.4 Synchronize family

**Synchronize directories** (`dlg_synchronize_dirs.htm`, overview `synchronize_dirs.htm`):

| Element | Meaning |
|---|---|
| Left / right directory comboboxes, wildcard box `*.*` | the two trees compared and the include/exclude filter (search filters via the button behind the box) |
| Compare-mode button `1x/2x/<-/<+/->/+>` with `!` inversion | how a filter that matches on only one side is treated (regex/filter compares) |
| Save directories and settings | to `wincmd.ini`; cannot store FTP; "Only selected" needs a list-file name |
| Compare | runs the comparison |
| Only selected (in main window) | restrict to the current selection (one panel is enough) |
| Empty directories | also handle directories, symmetric or asymmetric rules |
| **Asymmetric** | treat the left side as the master: right-only files are marked for deletion (backup semantics) |
| Subdirs | recurse; changing it forces a new comparison |
| by content | compare equal-size/same-date files; on FTP only with XCRC/XMD5/MD5/XSHA1 via FEAT; a `[++]`/`[>>]` button selects content plugins (AND by default, `\|` for OR, `[=tc.compare as text]` is internal) |
| ignore date | same name+size ⇒ identical; if "by content" is also on, equal-size files are still compared; result is only equal/not-equal with no default direction |
| Show: toggles for left→right, equal, different, right→left; plus **duplicates** and **singles** | effective immediately; a filter icon and hidden-file count appear in the status bar; a warning `!` marks duplicates/singles toggles that would hide results on open; keyboard `CTRL+SHIFT`+`L/E/N/R/D/S` |
| `dt=..h`, `mdtm`, `utc` | FTP-only: time-zone offset (saved per server; must be 0 when MLSD returns UTC), set uploaded file time via MDTM/MFMT/SITE UTIME, send dates in UTC |
| `(0/100) <subdir>` | subdirectory index/total; right-click scrolls to the next subdirectory, left-click to the previous |
| File list | one row per pair; grey bars delimit subdirectories; the middle symbol selects the copy direction; colour encodes the comparison result — green left→right, blue right→left (or delete in asymmetric mode, with a cross symbol), red different with no default direction, black identical |
| Footer | during compare: left+right directory count; during content compare: pairs done/total, size done/total, percentage; after: rows, identical, different, unique-left, unique-right, and `[total files, dirs scanned]` |
| `Synchronize` | opens the Synchronize dialog |
| Context menu / hotkeys | set direction for many files, view, delete, compare by content (also double-click), "Compare left & right" for unrelated files, "Set local file date to remote date" (FTP), rename with two single clicks or `F6`/`SHIFT+F6`, "Copy file properties" (timestamp/attributes/NTFS permissions), `CTRL+PgDn`/`PgUp` to step subdirectories |

**Synchronize (execution) dialog** (`dlg_synchronize.htm`): target path for left→right and right→left
(a `zip:` prefix packs into a ZIP archive instead, no other packer), "Right: Delete xxx file(s)",
"Right: Delete all empty directories" (asymmetric mode), **Confirm overwrites**, **Copy NTFS
permissions**. `OK` starts; `SHIFT` deletes directly when the Recycle Bin is enabled.

### 4.5 Pack / unpack family

| Dialog | Fields | Rules |
|---|---|---|
| **Pack** `dlg_pack.htm` (`ALT+F5`) | `Pack n files to the archive` (packer prefix + `:` is mandatory, e.g. `zip:`), options **also pack path names (recursed only)**, **Recursively pack subdirectories**, **Multiple disk archives**, **Move to archive**, **Create self extracting archive**, **Create separate archives one per selected file/dir**, **Leave out base directory when packing folders**, **Encrypt**, **Compression rate** (0–9, 10 = LZMA where supported), **Packer** selector, **Configure** | Without a packer prefix the files are merely copied. `EXT:`/`INT:` force external/internal ZIP. Subdirectories inside an archive are addressable as `zip:c:\test.zip/this/is/a/subdir`. `CTRL` while choosing File→Pack defaults the archive to the *source* directory. Internal ZIP is CRC-checked after packing (disable with `VerifyZip=0`); the check can be aborted without deleting the archive |
| **Unpack** `dlg_unpack.htm` (`ALT+F9`) | target path, **Files to unpack** (only shown for `ALT+F9`; spaces in names need quotes), **Encoding of names**, **Unpack path names if stored with files**, **Overwrite existing files**, **Unpack each archive to a separate subdir (name of the archive)** | Empty target ⇒ archive's own directory |
| **Internal ZIP config** `dlg_internalzipconfig.htm` | external zip/unzip paths, use internal ZIP / internal unzip, `7z`/`7zg`/Info-ZIP alternatives, compression level (0–9, default 6), temp directory (default/other), **Use 8.3 short filenames**, **Open partial ZIP files**, **Keep damaged files (bad CRC)**, **Pack Unicode names** policy (ask every time / UTF-8 / all-as-UTF-8 if one has Unicode / all-as-UTF-8 if non-English / Info-ZIP extra field / non-English to extra field / `?`) | |
| **Internal 7-Zip config** `dlg_internal7zipconfig.htm` | external 7-Zip path, use internal 7-Zip (based on 7-Zip sources, `tc7z.dll`/`tc7z64.dll` replaceable), prefer the 7-Zip plugin if installed, **Extra parameters** (only `-m…`, `-slp`, `-stl`, `-v`; long/short/default forms), compression level, **Compression method** (LZMA2/LZMA/PPMd/BZip2), dictionary size, word size, **Create solid archive**, solid block size, CPU thread count, memory usage for compression and decompression, **Also encrypt names** | Values are stored per method *and* per level on Apply; 32-bit builds are limited to 2 GB RAM |

### 4.6 Compare by content (`dlg_compare_contents.htm`)

Two path edit boxes (accept drag & drop), **Compare**, **Next difference** / **Previous difference**
(a contiguous block of differing lines counts as one difference), **Font**, search-in-both-files,
**Show the two windows above each other**, save, **Edit mode** (`F6`), **Show only differences with
extra lines**, **Sync: 0/1/2/3** (1 = legacy 3-matching-character resync mid-word, 2 = resume at a
space, 3 = user separator, 0 = no resync — useful for tables), **Copy ->** / **Copy <-**, **Undo**,
**Binary** mode with **8/16/24/32 Bytes** per line, encoding pickers, **Case sensitive**, **Ignore
repeated spaces**, **Ignore frequent lines**, a two-line strip showing the active line plus its
counterpart (`F9` hides it, `CTRL+TAB` focuses it), and a right-click menu with *Copy selection*,
*Paste here* (edit mode), *Resync comparison from here*, *Swap Panels*. Files ≤ 1 MB
(`MaxCompareSizeInMem`) are loaded fully so they stay editable; larger files are memory-mapped and
remain locked. Hotkeys: `CTRL+C`, `CTRL+F`/`F7` search, `CTRL+U` swap, `ALT+UP/DOWN` next/prev
difference, `F3` continue, `CTRL+F3` reverse direction, `SHIFT+F3` search backwards, `F6` edit toggle,
`F9` two-line strip, `ESC` close; in edit mode `CTRL+S`, `CTRL+Z`/`ALT+BACKSPACE`, `ALT+LEFT/RIGHT`
copy to the left/right panel, `F2`/`SHIFT+F2` next/previous difference *within* a line.

### 4.7 FTP family

| Dialog | Fields / controls | Rules and keyboard |
|---|---|---|
| **Connect to FTP server** `dialog_box__connect.htm` (`CTRL+F`) | connection list with icons distinguishing plain / secure / master-password-protected entries; **Connect**, **New connection**, **New URL**, **Duplicate entry**, **New folder** (drag & drop into it), **Edit**, **Delete** (immediate), **Encrypt** (apply/change master password), **Cancel** | cursor starts on the last used connection; saved alphabetically; an entered master password is remembered until minimize or screensaver |
| **New connection** `dlg_newconnection.htm` (`CTRL+N`) | **Connect to:** combo (host name or full URL — `ftp`, `ftps`, `ftpes`, `ftpis`, `http`, `https`; any part except the host may be omitted), **Anonymous connection**, **Use firewall (proxy server)** | if no user name is given, `anonymous` + the e-mail address are used when the anonymous box is ticked, otherwise credentials are requested; the box unticks itself when an `@` is present; `OK` adds the entry to the MRU list **without** storing the password |
| **Connection details** `dlg_connectdetails.htm` | **Session** name (subfolder shown as `folder\| name`), **Host name** (with `:port`; `ftpes://` forces AUTH TLS, `ftpis://` forces implicit SSL; since 11.50 also `\\\Pluginname\Path` and plain directories), **SSL/TLS**, **Anonymous login**, **User name**, **Password**, **Change password**, **Use master password to protect the password** (AES-256, with strength estimate; three documented ways to unprotect when the old master password is unknown), **Remote dir** (`/`-separated), **Local dir** (`\`-separated), **Use passive mode for transfers**, **Use firewall (proxy server)**, **Send commands:** (`;`-separated, `;;` escapes a semicolon), **Server type** (autodetect / define new type / import from file), **Send command to keep connection alive** (max 1 hour), **Encoding of file name** (automatic = `OPTS UTF8 ON` probe, else ANSI or manual code page), **Use MLSD for directory listings** (tri-state: auto via FEAT / force / use LIST), **Remember contents of all visited directories (in cache)**, **Client Certificate:** | MLSD makes the server-type definition moot because its format is fixed |
| **Firewall settings** `dialog_box__firewall_settings.htm` | **Connect method**: `USER user@hostname`, `SITE` with logon, `OPEN`, `USER user@firewalluser@hostname / PASS pass@firewallpass` (two commands), **Transparent**, **HTTP Proxy with FTP support**, **SOCKS4**, **SOCKS5 (basic authentication)**, `USER user@hostname firewalluser PASS pass ACCT firewallpass` (three commands), **HTTP CONNECT** (preferred over HTTP-proxy-with-FTP because it supports all FTP commands incl. delete and mkdir; passive only). Then **Firewall logon**: host name (`:port`), user name, password | settings are global to all connections; at least one connection must be configured to enable the proxy |
| **Define server type** `define_server_type.htm` | **Definition string**, raw listing pane, **Additional definition strings**, **Interpreted data for selected line**, **Load existing type…** | Character grammar documented (name `n`, VMS `v`, dot-trim `.`, `D/M/Y/TTT/UUUUU/h/H/m/s`, `S`+`S=1024`, `p` permissions, `d`/`d=x` dir flag, `*` numeric/name wildcards, `!`/`!=x` skip, `$`, `\` line continuation, `-` leading ignore, `x`/`x=c`/`X`/`X=c` strict single-character assertions). With ≤2 errors and no second string, errors are tolerated; otherwise the string with the fewest errors wins |

FTP operational notes (`ftp_menu.htm`): up to 10 simultaneous servers; the standard file keys operate on
FTP panels; `F5` then `F2` queues the transfer in the background; `CTRL+SHIFT+M` cycles the transfer
mode; `FTP Show hidden files` issues `LIST -la`; `FTP Download from list` supports list-file commands
`get/put/copy/move/sleep:/header:/copyflags:`; resume of upload and download; server-to-server copy is
possible when both servers are connected (private server in the left panel because the left side gets
PORT and the right PASV); `GET/PUT/RETR/STOR` typed on the command line are intercepted; directory
listings are cached per session so `F2`/`CTRL+R` is needed to see remote changes.

### 4.8 Multi-rename tool (`dialog_box___multi_rename_tool.htm`, overview `multi_rename_tool.htm`)

| Control | Meaning |
|---|---|
| **Rename mask: file name** and **Extension** | two separate masks joined by a literal `.` — the split exists so a mask cannot accidentally delete the extension and thereby break associations. Placeholders in `[ ]`; all other characters are copied literally. `\` in the name mask moves files into (auto-created) subdirectories. `+`/`#` insert less common fields; `SHIFT+DEL` removes a list history entry |
| **Search & Replace** | `Search for` / `Replace with` with `*` and `?` wildcards, case-insensitive by default; applies **after** the rename mask; multiple pairs separated by `\|`; toggles: `^` respect case, `1x` first occurrence only, `[E]` also replace inside the extension |
| **RegEx / Subst.** | regular expression search; *Subst.* replaces the whole name rather than the matched fragment |
| **Upper/lowercase** | whole-string case conversion applied after mask and after search&replace; "First of each word uppercase" excludes a documented English stop-word list |
| Edit menu (`F10`) | Load names from file, Edit names (export → edit externally → paste back), Configure editor |
| **Define counter [C]** | Start at, Step by, Digits (`a` = derive from the number of files) |
| `F2` Load/save settings | named setting sets, `<Default>` = no change |
| Result list | old name/new name pairs, sortable by header, live preview before `Start!`; `<Error!>` appears when the mask is invalid; items can be reordered by drag & drop or `SHIFT`+arrows (this changes counter order) and removed with `DEL` |
| **(next step)** | loads the *results* as the input for the next rename pass (`F5`) |
| **Start!** | performs the rename, warning about conflicts; conflicting names can auto-resolve to `name (2).ext` (not shown in the preview) |
| **Undo** | reverses the last rename in reverse order, and works after closing the tool |
| **Result list** | protocol of the performed renames |

Placeholder inventory (exact semantics documented on the same page): `[N]` name without extension with
range forms `[N1]`, `[N2-5]`, `[N2,5]`, `[N2-]`, `[N02-9]` (zero fill), `[N 2-9]` (space fill),
`[N-8,5]`, `[N-8-5]`, `[N2--5]`, `[N-5-]`; `[A]` full name incl. extension; `[I]` toggle treating folders
as extension-less; `[2-5]` characters incl. path; `[P]` parent directory (+ranges), `[G]` grandparent,
`[B0]`…`[B9]` ancestors, `[B+0]`…`[B+9]` from the drive letter downward; `[E]` extension; `[C]` counter
with `[C10+5:3]`, `[C+1/100]`, `[Caa+1]`, `[C:a]`; `[c]`/`[c1]`/`[c2]`/`[c3]` file counts and last counter
value; `[S]` size in bytes; time switches `[T1]`…`[T4]` (file time / dialog-open time / last-changed
time / EXIF or MP4 recording date) with `[T1+60m]` offsets; `[d]`, `[Y]`, `[y]`, `[M]`, `[D]`, `[t]`,
`[h]`, `[H]`, `[i]`/`[i1]`, `[m]`, `[s]`, `[v]`; `[u]` precomposed Unicode normalization (must come
first); case switches `[U]`, `[L]`, `[F]`, `[f]`, `[n]`; `[%envvar%]` with trailing `\` (last path part)
or a substitute character for backslashes; `[X]`/`[X+]` clipboard text (forbidden characters replaced by
`_`, cached); `[[]`/`[]]` literal brackets; and `[=plugin.field.unit]` forms with partial ranges,
conditional prefix/suffix and numeric formatting (`._x2`, `._x04`, `._d3,`).

### 4.9 Attribute, comment, encoding, column dialogs

| Dialog | Content |
|---|---|
| **Change attributes** `dlg_attrib.htm` | **Recurse subdirectories**; tri-state `Archive (a)`, `Read only (r)`, `Hidden (h)`, `System (s)` (gray = unchanged, checked = set, unchecked = clear); **Change date/time** with date and time fields that accept **partial** input (letters `a..z` act as placeholders, so `aaaa.05.aa` changes only the month; time letters `b..o` and `q..z`, `a`/`p` reserved for AM/PM; fractional seconds up to 3 digits); **Current** button; **Change plugin attributes** (Plugin / Property / Value, `+` reads the value from another plugin); `F2` load/save the plugin settings; **Load from file under cursor** from the source (1) or target (2) panel. On an FTP connection this is replaced by the UNIX attributes dialog. Parameterised variant `OPENATTRIBUTES` documented in `options_setattr.htm` |
| **Change UNIX attributes** `dialog_unix_attribs.htm` | Owner / Group / World × Read / Write / Execute, plus **Set UID/GID at startup** and **Sticky bit**; tri-state rendering is explicitly documented (black check = set, gray = unchanged, empty = clear); operates via `SITE chmod` so not all servers accept it |
| **Edit comment** `dlg_edit_comment.htm` (`CTRL+Z`) | target file name, multi-line edit field, **Use OEM (DOS) font**; `descript.ion` limited to 4095 characters, warning beep plus bold character count above 511; multi-line comments are converted to the appropriate storage format; F2/OK saves, `ESC` cancels. Comments work only on normal files — not in archives, FTP or plugins (`edit_comment.htm`) |
| **Choose encoding** `dialog_choose_encoding.htm` **(the referenced `dlg_choose_encoding.htm` page is absent from this extract)** | ANSI (plain text), Unicode UTF-16 (BOM `FF FE`), Unicode MacOS (byte-swapped, BOM `FE FF`), UTF-8 (BOM `EF BB BF`, HTML/XML/CSS auto-detected from headers); applies per side while the file is unmodified |
| **Custom columns editor** | see 4.1.3 |
| **Choose plugin property** `dlg_choosepluginfield.htm` | left list of content plugins, right list of properties with type glyphs (text, numeric, date, time, boolean, multiple choice), a **Filter** field, and a live **preview** of the field applied to the current file |
| **Define colors by file type** `def_colors_by_file_type.htm` | ordered filter+colour list with the same filter grammar as `NUM+`; order = priority; double-click or `SPACE` edits; clicking the leading `*` disables a rule (rewrites it as `-.ext`, which still matches a file literally named `-.ext`); `Dark<>Normal` converts normal-mode colours into dark-mode colours |

### 4.10 Association and action dialogs

| Dialog | Content |
|---|---|
| **Associate** `dlg_assotiate.htm` | extension field (no leading `*`), **Associate with:** program (with browse), **New type**, **Edit type**, **Delete** (sets "(none)"), and (Windows 10, affected types only) a "Replace with: PhotoViewer.dll" checkbox. Written to the registry plus `win.ini`, or to `wincmd.ini` for viewer associations. Documented warning: after Explorer's "Open with → Always open with" Windows blocks further association changes, leaving internal associations as the only override |
| **Edit file type** `dlg_edit_file_type.htm` | Change icon, read-only type name, description, the verb list with **New / Edit / Remove / Set as default**, the current default verb's command, and a **DDE** toggle |
| **Edit action** `dlg_edit_action.htm` | verb name, description shown in the context menu, command (needs `%1`/`"%1"` to receive the file), and DDE options (Use DDE, DDE message, application name, "message when application is not running", topic) — DDE is not available for internal associations |
| **Edit file type (internal)** `dialog_box__edit_file_type_internal.htm` | internal-only associations. `Open` is always the default verb; `%1` opens each selected file in its own editor while `%2` passes all files to one program. Special command syntax: `*` shows a verb menu on `ENTER`; `**` uses the system association; `*verb` runs a named system verb; `**ext` opens via a packer plugin; `**internal_ext` forces an internal packer; `**internal_sha` runs Verify checksums; `**path\file.bar` opens a button bar as a dropdown; `***command` runs an internal command such as `***cm_List %1`. The `Open` verb of archive extensions can only be redefined with `***command`, and the extension must be spelled out (`*.zip *.rar`) |
| **Configure context menu shell extensions** `dlg_contextmenuexts.htm` | include/exclude lists for 64-bit (or 32-bit) shell extensions shown in the X64/X32 submenu; holding `SHIFT` while opening the submenu shows only "Configure" as a recovery path |
| **Choose command** `dlg_choosecommand.htm` | Category / Command / Nr / Description / Filter, plus `usercmd.ini` and language-`wcmd_lng.ini` pages with New/Edit/Delete/Rename-Copy. `CTRL+C` copies a command name, `CTRL+SHIFT+C` copies the full line with description. The same page documents every parameterised internal command (`cm_Copy`, `cm_RenMov`, `cm_PackFiles`, `cm_UnpackFiles`, `cm_Edit`, `cm_MkDir`, `cm_List*`, `cm_Select*`, `cm_*ByName` …), including the `/O<n>`, `/T="path"`, `/W="mask"`, `/G`, `/B0\|B1\|B2` families |

### 4.11 Split / combine / encode / decode / checksum dialogs

| Dialog | Fields | Notes |
|---|---|---|
| **Split file** `dlg_filessplit.htm` | target directory, **Bytes per file** (free entry, presets, `automatic`; units `k`/`M`/`G` = 1024-based), **Delete files on target disk (removable disks only)** | parts named `name.001`…`name.999` (max 999); a `.crc` file records original name, size and CRC — optional for recombination; `automatic` uses the free space of removable media or 1.44 MB on hard disks; deletion requires a confirmation list |
| **Combine files** `dlg_filescombine.htm` | target directory | parts detected by ascending numeric or alphabetic suffixes (`file.001`, `fileaa`, `name.00000.zip.part`, also Unix `split -b`); a matching `.crc` is used both to verify and to recover the original name; the found part count is shown in the title |
| **Encode file** `dlg_filesencode.htm` | target directory, **Encode as** MIME / UUEncode / XXEncode, **Lines per file**, **Bytes per file** | output extensions `.B64`/`.UUE`/`.XXE`; empty line count ⇒ a single output file; header lines are not counted against the byte limit |
| **Decode file** `dlg_filesdecode.htm` | target directory | formats MIME, UUEncode, XXEncode, BinHex 4.0 and yEnc; same ascending-suffix detection; invalid characters in target names become `_`; duplicates can be auto-renamed `name(2)` |
| **Create checksum file(s)** `dlg_createsfv.htm` | target path/name (must contain `*` when one file per input), **Create separate SFV files for each file**, **…for each directory**, **Always use UTF-8 in names**, **Unix format: line breaks, `/` in paths**, method **CRC32 (SFV) / MD5 / SHA1 / BLAKE3 / SHA* / SHA3_\*** | with two `*` the new extension is appended to the full name; "per directory" ignores the target path; BLAKE3 uses multithreading in 64-bit builds (`CrcBlake3BlockSize`) |
| **Verify checksums** `dlg_verifysfv.htm` | **Only Errors** filter, result list | three input modes: verify against checksum files, against a hash on the clipboard (ambiguity between BLAKE3/SHA2-256/SHA3-256 requires choosing the method), or against a `[CRC]` embedded in the file name. Runs in a background process so TC stays usable; failing files are selected in the active panel and highlighted; `CTRL+DOWN/UP` or the arrow buttons step between failures; `CTRL+A` select all, `CTRL+C` copy, `ESC` abort with confirmation |

### 4.12 Menu-building dialogs

| Dialog | Content |
|---|---|
| **Main menu file layout** `dlg_mainmenu.htm` | text format grammar: `POPUP "&Title"` … `END_POPUP` (nestable), `MENUITEM "text\tACCEL", cm_Command`, `MENUITEM SEPARATOR`, plus the specials `STARTMENU` (inserts the user Start menu at that position) and `HELP_BREAK` (everything after it is right-aligned; cannot be followed directly by `STARTMENU`; no effect in RTL languages). `&` marks the accelerator; `\t` becomes a tabstop. Only identifiers present in `totalcmd.inc` (or `em_` commands) may be used, or their numeric values — `totalcmd.inc` must exist at runtime for an external menu |
| **Change start menu** `dlg_usermenu.htm` | entry list (reorder with `SHIFT`+arrows or drag), **Add item…**, **Add submenu…**, **Delete Item**, **Change Title…**, **Command:**, **Parameters:**, **Start path:**, **Shortcut key:**, `run minimized` / `maximized`. A leading `?` in the parameters opens a pre-run confirmation/edit dialog. `cd` in the command sets the *other* panel's path. Full parameter grammar: `%P %N %T %M %O %E`, `%B/%BT/%B-/%B+` with indices, `%S`/`%R` with `n`, `+n`, `+n-m`, `-n`, `-n-m` selectors, `%P%S`, `%C1..%C9`/`%c1..%c9`, list-file parameters `%L %l %F %f %D %d %UL %UF %WL %WF` (plus `%a`), `%Q %q %v %V %X %x %Y %y %Z`, `%\|envvar`, and `:~start,length` substring syntax |
| **Change directory menu** `dlg_dirmenu.htm` | same editor shape; **Command:** is normally `cd Drive:\Directory` (append `\:` to also focus a named subdirectory), or an internal command such as `cm_OpenDesktop`; **Target path:** sets the other panel, and may also be a `cd …` command; "Hide menu items starting with this character:" (default `?`, any character except `-`; double-click an item to toggle the marker) |
| **Change button bar** `dlg_configbuttonbar.htm` | bar file name + `>>` chooser, raw bar preview, **Size** (32 = normal icons, stored per DPI), **XP-Style**, **Flat icons**, **Small icons**, **Add…** (inserts *behind* the current position), **Delete**, and the button editor (Icon, Command, Start path, Parameters). Command field accepts: external programs (with `*`/`**` elevation prefixes and env vars), comma-separated internal commands (`cm_wait <ms>` supported), `cd "path"` (optionally with a file mask, `/user:` for UNC), `cd ftp://…` or `FTPOPEN session[\|/subdir]`, `ZIPFROMLIST`, subbar switching, and the parameterised commands `APPENDTABS`, `OPENTABS`, `SYNCOPEN`, `LOADSEARCH`, `OPENBAR`/`OPENBAR1`/`OPENBAR2`/`OPENBARMENU`, `MULTIRENAME` (with `<UNDO>`), `OPENCUSTOMVIEW[L/R/S/T/B]`, `SELECTFILES`, `SAVETABS*`, `SAVESELECTION*`, `SAVEDETAILS*`, `LOADSELECTION`, `OPENLANGUAGEFILE`, `LOADLIST`/`LOADLIST0`, `OPENATTRIBUTES`. A command value of `-2` with no icon forces a manual line break. Since TC 11, editing a bar notifies all other running instances |

### 4.13 Lister and print dialogs

| Dialog | Content |
|---|---|
| **Configure lister** `dlg_listerconfig.htm` | *General* tab: startup display (text / binary / hex / autodetect), startup font (ANSI / ASCII / variable / autodetect), font pickers per class, display width for text and binary modes, how many lines above the top border found text is shown. *Multimedia* tab: use plugins/bitmaps at startup, multimedia playback, RTF display, IrfanView/XnView import with additional/excluded types, show HTML as text with links and `{}` around links, LS-plugin install/order, Explorer preview handlers, thumbnail providers, per-file-type preferred view method, extra multimedia types. *Player* tab: internal media player filter choice (system / recommended LAV+ffdshow / legacy Video-for-Windows / user-defined), source and decoder filter lists, `grf` graph files |
| **Print options (Lister)** `dlg_printoptions_lister.htm` | printer, font (fixed for ANSI/ASCII, variable for the variable mode), margins (silently raised to the printer minimum) |
| **Print** `print_dialog.htm` | font, **Show:** page + zoom for the preview only, printer selection, **Setup…**, **Margins…** (including a header with `%p %t %d %D %T %%`), preview window with draggable margin lines, **Print…** with range All / Pages / Selection and a **Duplex** mode that prints every second page first and then asks for the stack to be reinserted. The same dialog prints directory trees and synchronize comparisons |
| **System information** `dlg_sysinfo.htm`, `dlg_sysinfosoft.htm`, `dlg_network.htm` | three pages reached by buttons. *Hardware*: CPU type, BIOS version, display driver, resolution and colour depth, current printer, COM/LPT port counts, installed RAM (DOS-accessible + extended). *Software*: Windows version, internal build, ProductId (Win10), swap file size, free memory with a "memory load" percentage, free GDI/USER resources (min of both, not available on NT), uptime, temporary directory. *Network*: primary/secondary network, MS-Network user name, computer name, workgroup, comment; shows "No network installed" when unsupported and does not work on NT |

### 4.14 Archive/compare/other overview pages

`packedfiles.htm` documents archive semantics reused by the UI: archives open like directories,
nested archives open (and are auto-deleted from the temp directory on exit), `CTRL+PgDn` opens
self-extracting archives, `ENTER` on a packed file shows full path/compression method/ratio, F3/F4 on a
file inside an archive unpack to a temp file and offer to repack, and CAB archives can only be
unpacked. `plugins.htm` defines the four plugin classes (packer, file system, Lister, content) plus
language packs and their install path (`Configuration → Options → Plugins`).

---

## 5. Menu structure (summary)

Only groupings and behaviourally significant entries are listed; `totalcmd.inc` holds the exhaustive
item list.

**Files** (`menu_files.htm`) — attribute/comment/timestamp editing (Change attributes, Edit comment);
archive work (Pack, Unpack specific files, Test archives); comparison (Compare by content); association
management (Associate with…, Internal Associations, Properties); size accounting (Calculate occupied
space…); bulk rename (Multi-rename tool…); **Print** (a submenu: File list / File list with subdirs /
File contents — `options_print.htm`); transfer helpers (Split file, Combine files, Encode, Decode,
Create Checksum File(s), Verify CRC checksums); Quit.

**Mark** (`menu_mark.htm`) — the twelve selection commands (select/unselect group, all, none, invert,
same extension, save/restore selection, save/load selection file, four clipboard-copy variants), plus
two comparison commands that **mutate the panel state**: *Compare directories* (selects newer files and
files missing on the other side only — not all differing files) and *Mark newer, hide same* (hides
identical files without deleting them and prefixes both panel paths with `Comparison: `; the prefix
disappears only after a refresh, a panel swap, or minimizing/restoring).

**Commands** (`menu_fileactions.htm`) — navigation (CD-tree, Directory hotlist, Go back), search
(Search, Search in separate process), volume label, system information, Synchronize dirs, DOS prompt,
Branch view (with subdirs) and its selected-only variant, Open desktop folder, Background transfer
manager…, Source↔Target, Target=Source.

**Net** (`menu_net.htm`) — the system network connect/disconnect dialogs, Share/Unshare directory (only
present on WfW/NT 3.51-era systems), Show admin shares (`C$` etc., needs the remote administrator
password), the FTP submenu, and PORT connection to another PC.

**Show** (`menu_show.htm`) — view modes (Brief, Full, Comments, Custom columns, Custom view modes,
Tree, Separate tree, Thumbnail view, Quick view, Vertical alignment), New Folder Tab, the file-type
filter group (All files, Programs, the last user mask, Custom…, Only selected files), the sort group
(Name, Extension, Time, Size, Unsorted, Reversed order), and Reread source.

**Configuration** (`menu_configuration.htm`) — Options, Change Button Bar, Button Bar 2 (vertical)…,
Change Settings Files Directly (opens `wincmd.ini`/`wcx_ftp.ini` in an editor; usually needs a
restart), Dark↔Normal, Save Position, Save Settings.

**Start** (`menu_starter.htm`) — entirely user-defined; supports passing the file under the cursor.

**Help** (`menu_help.htm`) — Index, Keyboard, Registration info, web site, Check for updates now (a
failed check offers to enable/disable the automatic check), About (version, settings-file and program
locations).

Notable modal/confirm behaviours tied to menus: *Compare directories* and *Mark newer, hide same* alter
the selection/visibility without a prompt; *Branch view* runs long operations that can be aborted with
`ESC`; *Change Settings Files Directly* warns that a restart is normally needed; *Dark↔Normal* is
permanent unless the follow-Windows mode is enabled.

---

## 6. Files / viewer integration

### 6.1 F3 — internal Lister (`listercontents.htm`)

Window structure: title bar, **menu bar** with `File`, `Edit`, `Options`, `Plugins`, `Encoding`,
`<percent> %`, then `<<` and `>>` navigation items; the text/image area; and a status/percentage readout
in the menu bar itself (there is no separate status-bar widget described — **(inferred)** the percent
item doubles as the status line).

Capabilities: any file size up to 2^63 bytes; only a small window of the file is kept in memory (except
bitmaps); unlimited simultaneous windows, all closed when TC exits; WMF images disabled by default for
security (`WmfAllowed=3` re-enables).

Display modes (`lister_menuoptions.htm`): **Text only** (`#13`/`#10` as newline; lines longer than 80
characters — configurable — are wrapped), **Binary (fixed line length)** (74-character default),
**Hex** (32-bit offset, 16 hex bytes, then the same 16 bytes as plain text), **Image/Multimedia**
(internal BMP viewer, IrfanView, or the internal media player), **HTML-Text (strip tags)** (relative
links open inside Lister, absolute `http://` links open the default browser, cursor changes for external
links, UTF-8/UTF-16 honoured via the META header), **Unicode (UTF-16)**, **UTF-8**, **Explorer Preview**
(`IPreviewHandler`, falling back to `IThumbnailProvider`; title shows `<Explorer>`). Character-set
options: ANSI (`a`), ASCII/DOS (`s`), variable width (`v`); plus *Wrap text*, *Show text cursor*
(`F6`), *Fit Image to window* (`f`), *Fit only larger images*, *Center images*, *Configure*,
*Save position*.

Find bar: no persistent bar is documented; searching is via `F7`/`CTRL+F` with options *whole words
only*, *case sensitive*, *search hex string* (accepts `00 FF 12` or mixed `"PK"0102`), `\t`/`\n`/`\\`
escapes and regular expressions; matches are shown inverted; `F3`/`F5`/`SHIFT+F7` repeat,
`CTRL+F3`/`CTRL+F5` reverse, `SHIFT+F3`/`SHIFT+F5` always search backwards. The current search string is
shared with Total Commander's own search.

Key map: `F1` help, `F2` reload, `F11` maximize toggle, `ALT+ENTER` full screen, `CTRL+P` print,
`ESC`/`ALT+F4`/`F10`/`Q` close; `N`/`P` and `ALT+LEFT/RIGHT` step through multiple loaded files;
`F4` opens the current file in the configured editor (unavailable for temporary/archived files and in
image, multimedia, Explorer-preview and plugin modes); text mode adds `CTRL+A/C`, `F6` cursor toggle,
wheel/`SHIFT`+wheel/`CTRL+ALT`+wheel scrolling, `+`/`-`/`CTRL`+wheel font zoom, `CTRL+0` reset,
`CTRL+SHIFT+RIGHT/LEFT` bidi alignment; image mode adds `+`/`-`/`CTRL`+wheel zoom, `CTRL+0` = 100 %,
`F` fit; media mode adds `E` step, `C` cover, `M` mute, arrows ±1 %, `CTRL`/`SHIFT`+arrows ±10 %.
Context menu (right-click or `SHIFT+F10`): copy selection as text or as hex, search the selection on
Google, download a URL, with user-defined entries via `[Lister] Contextmenu<n>=Title|URL?var=%s`.
Touch support: drag to scroll, flick to auto-scroll, double-tap to select a word.

Plugin selection: the **Plugins** menu is ordered in three sections — plugins explicitly mapped to the
file type via *Options → Configure → Multimedia → Define view method by file type*, then plugins whose
detection string matches, then all remaining plugins. Pressing `4` cycles through matching plugins.

Encoding menu (`lister__encoding_menu.htm`): as configured for the current font, ANSI (local code page),
ASCII/DOS, MacOS, then explicit code pages (1250 Eastern Europe … KOI8-R 20866 …), plus a manual
code-page entry that can be named and added to the menu.

Viewer selection (`dlg_configedit.htm`): F3 can start the internal viewer with the cursor file **or**
with all selected files (the other variant is available via `SHIFT+F3`); external viewers can be
associated per extension, and `ALT+F3` uses the external viewer (falling back to the Windows quick view,
then the default viewer). `ALT+SHIFT+F3` starts Lister with no plugins or multimedia views.

### 6.2 F4 — editor integration

* `F4` launches the editor named in `Configuration → Options → Edit/View` (default `notepad.exe`) with
  the current file. The file name can be passed as `"%1"` (long name, always quoted), `%1` (short 8.3
  name if available, otherwise the long name quoted when it contains spaces), or with no parameter at
  all (`proc_edit.htm`, `dlg_configedit.htm`).
* `ENTER`/double-click instead launches the program **associated** with the file type.
* Inside an archive: the file is unpacked to a temp file, opened in the editor, and on close TC asks
  whether to repack it; answering no discards the temp file and leaves the archive untouched
  (`proc_edit.htm`, `packedfiles.htm`). Repacking never works for TGZ/GZ/CAB — the modified file simply
  stays in the temp directory.
* With a multi-document editor already running, TC cannot detect the close, so it shows a small
  "done with this file" dialog to trigger the repack step.
* FTP/plugin files: the *Plugins/FTP: Automatically re-upload changed files* option chooses between
  re-uploading on every save or only after the editor closes (with confirmation).
* `SHIFT+F4` creates a new text file and loads it into the editor.

---

## 7. Implementation-affecting notes

### 7.1 Operations that must run off the main thread or be interruptible

| Operation | Evidence |
|---|---|
| Copy / pack / unpack / delete moved to the **background** via a `Background` button in the progress dialog; can be brought back to the foreground when no other dialog is open; the directory may need `F2`/`CTRL+R` afterwards | `proc_copy.htm`, `proc_delete.htm`, `options_pack.htm` |
| **Background transfer manager** — separate window with its own queue (`+`/`-`, Pause, speed limit, progress bar per file plus an overall bar), accepting upload/download/copy/move jobs, `sleep:` entries, and list-file-driven jobs (`Edit list file`) | `options_background_transfer_manager.htm` |
| `F2 Queue` in the copy dialog appends to the last opened transfer manager; `cm_Copy /B2` does the same | `proc_copy.htm`, `dlg_choosecommand.htm` |
| **Search in separate process** (`ALT+SHIFT+F7`) so TC stays usable and multiple searches can run in parallel; selection-dependent options are disabled there | `options_search.htm`, `dlg_search.htm` |
| **Verify checksums** runs in a background process | `dlg_verifysfv.htm` |
| Thumbnail database maintenance/updates run in a background thread | `dlg_configthumbs.htm` |
| All FTP transfers can be backgrounded, each opening a *new* server connection which may fail at the server's user limit | `ftp_menu.htm`, `dlg_configftp.htm` |
| Branch view can be long and is aborted with `ESC`; directory-size counting (`ALT+SHIFT+ENTER`, `SPACE`, `Calculate occupied space`) is likewise long-running | `options_branch_view.htm`, `keyboard.htm`, `options_memory.htm` |
| Directory watching / auto-refresh with a ~1 s debounce; footer/header refresh can be disabled because it is slow | `dlg_configrefresh.htm` |

### 7.2 Progress and status reporting

* Copy progress exposes **copied/total files** and **copied/total bytes**, each toggling to a remaining
  count (displayed negative); hovering reveals full names.
* A **second overall percentage bar** appears only when the "count size before copying/deleting" option
  pre-computes totals (`dlg_config2.htm`).
* Synchronize's footer is a three-phase progress line (directories visited → pairs compared → totals).
* Search's status line names the directory being scanned and allows skipping it by clicking, after
  confirmation.
* Lister reports file position or image zoom in percent, with decimal input accepted.
* Background transfer manager has two bars (current file, overall) and a numeric speed limit.
* The panel footer doubles as a status area for the active quick **filter** (filter icon + hidden count).

### 7.3 Confirmation prompts (all configurable)

From `dlg_configconfirmation.htm`, defaults are "maximum security": deleting non-empty directories,
overwriting files, overwriting/deleting read-only files, overwriting/deleting hidden or system files,
Drag&Drop copy destination. Additional mandatory or near-mandatory prompts documented elsewhere:
per-directory prompt when deleting each non-empty directory (`proc_delete.htm`); "As Administrator"
retry prompts on copy (Win2000+), delete and mkdir failure; repack-after-edit inside an archive;
repack after a Lister-plugin modification; confirmation before deleting files during "split to removable
disk"; warning on name conflicts in the multi-rename tool; `cm_CloseCurrentTab`/`cm_CloseAllTabs`
confirmation switches; synchronize's *Confirm overwrites*; and the FTP master-password unprotection
paths.

### 7.4 Persistence and state that must survive a restart

`wincmd.ini` holds panel directories, tabs, panels state (brief/full/tree, file filter, sort order,
saved when the corresponding Save-on-exit options are on), command-line history, directory history
(`[LeftHistory]`/`[RightHistory]`, up to 200 per side), hotkeys/aliases, saved searches, saved
synchronize settings, saved multi-rename settings, button-bar and menu files, viewer associations, ignore
list, thumbnail database path, and log-file settings. Frequently-used directories live in
`tcDirFrq.txt`. Window position/size and tabstops are stored **per screen resolution**; dialog DPI
scaling and button-bar size likewise. `Configuration → Save Position` and `Save Settings` force a write
even when auto-save is off. The icon-size and font pages note that the values are kept per resolution.

### 7.5 Cross-cutting behaviours worth modelling explicitly

* **Command line pre-emption** — almost all keystrokes go to the command line while TC is active, and
  the line auto-appears when hidden; `Run command line only when focused` restores panel-first `ENTER`.
* **Archive transparency** — an archive path behaves like a directory for the file list, tabs,
  synchronize, search and copy; the packer prefix (`zip:`, `7z:` …) decides whether F5 copies or packs,
  and a missing prefix silently degrades to a copy.
* **Virtual panels** — search results ("feed to listbox"), branch view, and plugin panels are panels with
  a different backing store; the path header shows a descriptive prefix (`Comparison: `, the archive
  name, the plugin name) and refresh semantics differ (F2/CTRL+R returns to the normal listing).
* **Ignore list vs. hidden attribute** — ignore is a TC-only filter that also affects copy, move,
  counting, synchronize and delete-with-warning behaviour, and is toggled at runtime by
  `cm_SwitchIgnoreList`.
* **Verify on copy** uses BLAKE3 when available, otherwise MD5, and bypasses the disk cache.
* **`totalcmd.inc`** must be present at runtime for an external main menu; command ids may also be given
  numerically.
