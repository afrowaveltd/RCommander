# SPEC-03 — Configuration & Settings Model

Source: Total Commander 11.58 English help (extracted from `TOTALCMD.CHM`), pages
`inisettings.htm`, `inisettings1.htm`, `inisettings2.htm`, `inisettings3.htm`,
`wcx_ftp_ini_settings.htm`, `commandlineparameters.htm`, and the `dlg_config*` /
`dlg_display` / `dlg_listerconfig` / `def_colors_by_file_type` / `dark___normal` /
`change_settings_files_directly` dialog pages.

Sections 1–5 are fact extraction only. Section 6 is a **proposed** portable design and is
labelled as such.

---

## 1. Configuration model overview

### 1.1 Two settings files

| File | Owns | Typed as |
|---|---|---|
| `wincmd.ini` | Everything about the main program: window/panel layout, view modes, colours, fonts, icons, copy/delete behaviour, confirmation prompts, packers, plugins, histories, saved searches, saved dialogs. | Windows INI (ANSI or UTF-16 LE) |
| `wcx_ftp.ini` | The built-in FTP client only: connection list, per-site parameters, firewall/proxy, certificates, master-password data, FTP connection cache. | Windows INI |

All other program settings live in `wincmd.ini`; the FTP page states this explicitly ("All other
settings for Total Commander are stored in the file wincmd.ini"). A few auxiliary files sit next
to the ini files and hold *state*, not settings: `treeinfo.wc` (per-drive directory tree),
`tcDirFrq.txt` (most-frequently-used directories), `tcignore.txt` (default ignore list),
the thumbnail database, and log files.

### 1.2 Where the ini files live

The help gives four overriding mechanisms, in decreasing precedence as described:

1. **Command line** — `/i=name.ini` replaces `wincmd.ini`, `/f=ftpname.ini` replaces
   `wcx_ftp.ini`. This is the documented way to let several users share one installation.
2. **Registry** — the location is stored during installation and can be changed by the installer
   or by the standalone tool `inireloc.exe` (Addons section of the vendor site).
3. **Program directory** — `[Configuration] UseIniInProgramDir` is a bit mask that makes the
   program prefer the ini files beside the executable; it is only read when the ini itself is
   already in the program directory, and `RedirectSection` has no effect on it.
4. **Templates** — since 10.50, if no `wincmd.ini` exists at the configured location, the program
   copies `maintemplate.txt` → `wincmd.ini` and `ftptemplate.txt` → `wcx_ftp.ini`. This is aimed
   at automated corporate deployment; the installer normally creates the ini itself.

The effective location is displayed to the user under *Help → About Total Commander*.

### 1.3 Encoding constraints

Windows' own profile-INI API does not support UTF-8. The help warns that recent Notepad versions
may save Unicode text as UTF-8 without a BOM, which "will not work and result in scrambled
accents"; ini files must be ANSI (plain text) or UTF-16 LE. The key `SetEncoding` exists purely
to keep Notepad from re-saving the ini as UTF-8, and `Test` / `Firstmnu` exist to probe whether
the ini file is writable at all (an error box is shown when it is not).

### 1.4 "Save settings" semantics

There is no single settings store flushed at every change; instead a small set of independent
flags decides what is written when the program closes, plus explicit "save now" actions:

- `Savepanels` — persist brief/full/tree display type, sort order and the all/programs/custom
  filter on exit.
- `Savepath` — persist the two panel paths on exit (also persists folder tabs when they are on).
- `SaveCommands` — persist command-line history on exit.
- `SaveHistory` / `SaveFrequentlyUsed` — persist the visited-directory history and the
  most-frequently-used list.
- `Configuration → Save position` — an explicit action that stores window position, splitter
  positions and column/tabstop geometry. Geometry is stored **per screen resolution**
  (see 4.1), and `RestoreSize` controls when it is re-applied.
- Dialogs with their own state (search, synchronize, multi-rename, pack, print, connect) persist
  their last options in dedicated sections, and several have a "save settings under a name"
  mechanism (`[Searches]`, `[SyncOptions]`, `[RenameTemplates]`).

`Configuration → Change Settings Files Directly` opens both ini files in the configured editor;
the help notes that most settings changed that way need a restart to take effect.

### 1.5 Redirecting and splitting the ini

Two mechanisms let part of the configuration live in another file — the documented motivations
are making the main ini read-only and letting volatile history be deleted by simply deleting
(or redirecting to a read-only) replacement file.

1. **Bulk redirect** — `[Configuration] AlternateUserIni=<somename.ini>` moves all
   *user-changeable* sections to that file, **except**: `[Configuration]`, `[Layout]`,
   `[Packer]`, `[Confirmation]`, `[Extensions]`, `[Associations]`, `[Shortcuts]`,
   `[ShortcutsWin]`, `[PackerPlugins]`, `[FileSystemPlugins]`, `[ListerPlugins]`,
   `[ContentPlugins]`.
2. **Per-section redirect** — a `RedirectSection=` entry inside a section with values
   `0` (never redirect, even if `AlternateUserIni` would), `1` (redirect to the
   `AlternateUserIni` target) or `<ininame.ini>` (redirect to that specific file).

Rules that fall out of the documentation: redirects must be declared in the **main** ini —
redirects in secondary files and in `wcx_ftp.ini` are ignored; a `RedirectSection` placed in
`[Configuration]` redirects that section's variables *except* `AlternateUserIni` and
`UseIniInProgramDir`, which are always read from the main ini. There is also
`[ReplaceIniLocation]` for redirecting only the ini file handed to a specific plugin.

### 1.6 Startup switches that affect configuration

Extracted from `commandlineparameters.htm` (switches are case-insensitive):

| Switch | Effect on configuration |
|---|---|
| `/i=name.ini` | Use another main settings file. |
| `/f=ftpname.ini` | Use another FTP settings file. |
| `/M=0\|1\|2` | Force light / automatic / dark mode at startup (overrides the stored `DarkMode`). |
| `/P=L\|R` | Set the active panel at startup, overriding `ActiveRight`. |
| `/O[=…]`, `/N` | Reuse (or never reuse) an already running instance; `/O=<n>` behaves like `OnlyOnce=`. |
| `/T`, `/B` | Open passed path(s) in new tabs (foreground / background). |
| `/L=`, `/R=`, `/S` | Split passed paths into left/right (or source/target) panels. |
| `/A`, `/P` (no `=`), `/d=<s>` | Archive-aware path opening, parent-directory opening, delayed start. |
| `/S=C`, `/S=F`, `/S=L`, `/S=S`, `/S=P:` | Start a sub-tool directly (compare, find, lister, synchronize, port server) with inline options. |

---

## 2. Option inventory

The tables below list **every key documented in the four INI reference pages and in the FTP INI
page**, grouped by INI section and, inside `[Configuration]`, in the alphabetical order the help
uses (page 1 = A–M, page 2 = N–Z). `Type` is read off the documented value domain:
`bool` (documented as 1/0), `enum` (numbered alternatives), `bits` (documented as a sum of flags),
`int`, `string`, `path`, `pattern` (wildcards), `color` (RGB `r+256*g+65536*b` or `$BBGGRR`).
Empty defaults are shown as `—` (the key is simply absent until written).

### 2.1 `[Configuration]` — key syntax

The key `Aligned extension` contains a literal space; every other `[Configuration]` key is a
single token. Values that are "sum of" lists are additive bit masks unless stated otherwise.

### 2.2 `[Configuration]` A–B

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `AccessTime` | bool | 0 | Use the last-accessed timestamp instead of last-modified when evaluating time criteria. | Help deprecates it in favour of custom columns. |
| `ActiveRight` | bool | 0 | Right panel is the active one at start. | `/P=L\|R` overrides. |
| `AdminShares` | bool | 0 | Always list administrative shares such as `C$`. | Win NT-family only. |
| `AdminTimeout` | int (s) | 180 | Idle timeout after which the elevated helper used for copy/delete as admin is terminated; `0` = never, `-1` = disable copy-as-admin. | Win-only helper process. |
| `Aligned extension` | bool | 1 | File extension is drawn left-aligned in its column. | Dialog: Tabstops. Key name contains a space. |
| `AlignNumberFix` | bool | 1 | Compensate misalignment of numeric columns caused by some OpenType fonts. | Help notes the default is off on Linux. |
| `AllHiddenNetNames` | bool | 1 | When showing admin shares, list all hidden shares rather than only drive-letter ones. | |
| `Allowed` | string | — | Restrict visible drives to the given drive letters; `\` denotes Network Neighborhood. | Unlike `DriveBarHide`, hidden drives stay reachable via drive list/`cd`. |
| `AllowSetCursorPos` | bool | 1 | When expanding a big tree node with the mouse, keep the cursor on the expanded directory. | |
| `AlternateUserIni` | path | — | Bulk-redirect user-changeable sections to another ini file. | See 1.5. |
| `AltSearch` | enum | 0 | Quick-search invocation: 0 = Ctrl+Alt, 1 = Alt, 2 = letters only, 3 = letters with dialog, 4 = off. | Dialog: Quick Search. |
| `AltTabWindow` | bool | 1 | Keep an invisible window while minimised to the tray so Alt+Tab+Enter can restore the program. | Tray-related; Win-only. |
| `AlwaysAsAdmin` | bool | 0 | After the first "All as administrator" confirmation, treat all further operations as admin. | |
| `AlwaysCopyInBackground` | enum | 0 | 1 = always copy in background; 2 = always use the background transfer manager. | |
| `AlwaysPackInBackground` | enum | 0 | 1 = archive in background; 2 = internal+external packers; 3 = as 2 but external packer minimised. | |
| `AlwaysUnpackInBackground` | enum | 0 | Same ladder as `AlwaysPackInBackground`, for unpacking. | |
| `AlwaysRequestAgain` | bool | 0 | Ask for the master password on every connection instead of caching it. | FTP/password manager. |
| `AlwaysShowAppdata` | bool | 1 | Keep showing `c:\users\<user>\Appdata` even when hidden files are hidden. | Win path-specific. |
| `AlwaysToRoot` | bool | 0 | Changing drive always lands in the drive root instead of the last directory. | Dialog: Operation. |
| `AppendLnk` | enum | 2 | Shortcut naming: 0 = `file.lnk`, 1 = `file.ext.lnk`, 2 = 0 for files / 1 for directories. | |
| `AppendWarning` | bool | 1 | Warn before appending to an existing file. | |
| `AssociationMultiVerbs` | bool | 1 | Allow multiple commands from all matching internal file-type associations. | |
| `AssocQueryString` | bool | 1 | Resolve the associated application through the shell API rather than the registry. | Win API-specific. |
| `AutoComplete` | bits | 3 | 1 = dropdown of completions, 2 = append the suggested remainder inline. | Dialog: Operation. |
| `AutoCompleteTab` | enum | 0 | Tab key during completion: 0 = jump to file list, 1 = next match, 2 = next match and ignore Tab if nothing matches. | |
| `AutoInstallPlugins` | bool | 1 | Automatically offer plugin installation when entering a plugin archive. | |
| `AutoRenameMode` | bits | 3 | Auto-rename on copy / in multi-rename: 1 = space before `(n)`, 2 = start numbering at 2, 4 = always append instead of incrementing. | |
| `AutoReUpload` | enum | 1 | Handling of edited temp files from plugins/FTP: 0 = re-upload on editor close with confirmation, 1 = ask once about the new method, 2 = re-upload on every save. | Dialog: Edit/View. |
| `AutoSizeDialogs` | bool | 1 | Scale dialogs up when the dialog font is larger than default. | Dialog: Font. |
| `Autotreechange` | bool | 0 | A single click in the tree switches the *other* panel to that directory. | |
| `AutoUpdateCheck` | enum | 0 | 0 = off, 1 = check release+beta, 2 = check release only. | Dialog: Operation. |
| `AutoUpdateCheckBeta` | bool | 0 | Include beta announcements even when running a final release. | |
| `AutoUpdateCheckDate` | string | — | Next scheduled update check, `YYYYMMDDHH`. | |
| `AutoUpdateInformedVersion` | string | — | Last version the user was told about, so it is not reported twice. | |
| `BackgroundCopyFlags` | enum | 1 | Overwrite flags in the background transfer manager: 0 = keep the chosen flags forever, 1 = reset at list end, 2 = reset whenever files are added. | |
| `BackgroundSpeed` | enum | 2 | Background dialog shows 0 = file name, 1 = speed, 2 = alternate both. | |
| `BitlockerAlwaysDecrypt` | enum | 0 | 1 = silently decrypt a BitLocker volume on drive change, `-1` = always report an error. | Win-only. |
| `BreadcrumbDelayButton` | ms | 250 | Hover time before the breadcrumb dropdown button appears; negative shows the list immediately without a button. | Dialog: Layout. |
| `BreadcrumbDelayDir` | ms | 250 | Hover time in the "open subfolder" zone before the subdirectory is opened. | |
| `BreadcrumbDelayFtp` | ms | 500 | As `BreadcrumbDelayDir`, for FTP connections. | |
| `BreadcrumbDelayPlugin` | ms | 500 | As above, for file-system plugin paths. | |
| `BreadcrumbDelayVirtual` | ms | 500 | As above, for virtual folders. | |
| `BreadCrumbMaxWidth` | int (px) | -1 | Cap the breadcrumb bar width; `-1` = half the screen, clamped to 100 px … screen width. | |
| `BreadcrumbSeparator` | int | -3 | Start of the zone that opens a breadcrumb submenu: 0–100 = percent from the left, negative = separator position from the right in line heights. | |
| `BriefViewWidthLimit` | int (px) | — | Maximum column width in brief view (0/absent = unlimited). | Dialog: Display. |
| `BriefViewWidthLimitMin` | int (px) | 0 | Minimum column width in brief view; 0 = width of `wwwwwwww.www` in the current font. | |
| `BrushForCursor` | bool | 0 | Workaround for graphics drivers that leave frame trails behind the cursor. | |

### 2.3 `[Configuration]` C

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `CdCloseDoor` | bool | 1 | On a failed drive change to a CD, try to close the tray first. | |
| `CDlabel` | bool | 1 | Show volume labels of CD/removable drives in the drive selection list. | |
| `ChecksumExtraTypes` | pattern | — | Extra file masks recognised as checksum files by Verify Checksums, e.g. `*.sum *.crc32`. | |
| `CheckUsbHdd` | bool | 1 | Detect hard disks attached over USB and draw them with the removable-drive icon. | Win drive enumeration. |
| `ClearListBackground` | bool | 1 | 1 = erase the list background before repainting, 0 = skip it to reduce flicker. | |
| `CmdUpDown` | bool | 0 | Cursor up/down also moves in the file list while the command line has focus. | |
| `CmtOemFont` | bool | 0 | Write `descript.ion` in the DOS/OEM code page. | Recommended only with 4DOS/4NT. |
| `ColorFilters` | bool | 1 | Master switch for colouring by file type. | Dialog: Colors → Define colors by file type. |
| `CommentPreferredFormat` | enum | 4 | Preferred `descript.ion` encoding: 0 = plain (ANSI/OEM), 1 = UTF-16 LE, 2 = UTF-16 BE, 3 = UTF-8, 4 = plain with UTF-16 LE fallback, 5 = plain with UTF-16 BE fallback, 6 = plain with UTF-8 fallback. | Dialog: Operation → File comments. |
| `CompareAlwaysSaveWithBom` | bool | 0 | The compare editor always writes a BOM for UTF-8/16 files. | |
| `CompareBytesPerLine` | enum | 8 | Bytes per line in binary compare: 8, 16, 24 or 32. | |
| `CompareCaseSensitive` | bool | 0 | Text comparison is case sensitive. | |
| `CompareDeleteBak` | bool | 0 | Delete `.bak` files when the compare tool is closed. | |
| `CompareExtraLines` | int | 2 | Context lines shown above and below each difference. | |
| `CompareF2Mode` | enum | 1 | Where F2 looks next: 0 = current line only, 1 = continue after the last match, 2 = restart the line and wrap. | |
| `CompareFindLines` | int | 1 | Matching lines required to re-synchronise after a difference; 0 = never re-sync, ≥2 = two-pass (block match first, then line match). | |
| `CompareIgnoreRepeatedLines` | bool | 1 | Ignore lines repeated very often (boilerplate) when comparing. | |
| `CompareIgnoreRepSpace` | bool | 1 | Treat differing runs of spaces as equal. | |
| `CompareResync` | enum | 1 | Intra-line re-sync granularity: 0 = none, 1 = mid-word, 2 = at spaces only, 3 = at custom separators. | |
| `CompareResyncSeparators` | string | `., ;:_+-*/\?!()[]{}"'@` | Separator set used when `CompareResync=3`; a space is included by default. | |
| `CompareSaveOriginal` | bool | 1 | 1 = write and preserve the original as `.bak` in place (keeps streams/attributes); 0 = write `.tmp`, rename chain, single write. | |
| `CompareSkipLineNumbers` | bool | 0 | Skip leading digits on each line before comparing. | |
| `Comparetool` | path | — | External compare tool used for Compare by Contents. | |
| `CompareTwoLineList` | bool | 1 | Show the two-line difference panel at the bottom of the compare tool. | |
| `CompareUtf8Detect` | enum | 1 | UTF-8 detection in compare: 0 = BOM only, 1 = by content (multi-byte char or XML/HTML header), 2 = accept if no invalid sequences. | |
| `CompareVertical` | bool | 0 | 1 = stack the two compare panes, 0 = side by side. | |
| `ContextProgress` | bool | 1 | Show a progress bar while a long context menu is being built. | |
| `CopyAllTimes` | bool | 0 | 1 = also copy creation and last-access times, not just last-modified. | |
| `CopyComments` | bits | 6 | Comment handling on copy: 1 = default type `files.bbs`, 2 = default type `descript.ion`, 4 = read both types, 8 = copy comments along with files (skipping the comment files themselves), 16 = DOS charset for `descript.ion`. | If 1 and 2 are both set, 2 wins. |
| `CopyCompatDrives` | string | — | Drives copied in compatibility mode; `\` = Network Neighborhood, `*` = all. | Not available on Win9x/ME. |
| `CopyCompatUsbSticks` | bool | 1 | Auto-apply compatibility mode to USB sticks. | |
| `CopyDefaultMethod` | bool | 1 | Always use the default copy method on NT-family systems. | Dialog: Copy/Delete. |
| `CopyDirsAsAdmin` | enum | 1 | When a directory cannot be copied as a normal user: 0 = skip, 1 = ask, 2 = always retry elevated. | |
| `CopyDirTimeStamp` | bool | 0 | Copy the last-modified timestamp of directories. | Dialog: Copy/Delete. |
| `CopyLinks` | enum | 0 | Reparse points/junctions: 0 = copy as directory or ignore, 1 = copy the link unchanged, 2 = rebase the link onto the target drive when source and link share a drive. | |
| `CopyOverwriteDefault` | enum | 1 | Default preselection (1…8) in the expanded F5 overwrite options; 0 = like 1 but do not reset flags when queueing to the background dialog. | |
| `CopySparseFiles` | bool | 0 | Copy only the non-zero extents of sparse files. | |
| `CopyStayOnFile` | bool | 0 | Keep the cursor on the source file after Shift+F5 creates a new file. | |
| `CopyStreams` | bool | 1 | Copy NTFS alternate data streams. | Win/NTFS-specific; always copied with the default method. |
| `CopyToDirWarn` | bool | 1 | Warn when the F5 target is typed as an existing directory name. | |
| `CopyBlockSize` | int (KiB) | 32 | Copy buffer size within one physical disk; minimum and default 32 = 32 KiB. | Dialog: Copy/Delete. |
| `CopyHugeBlockSize` | int (KiB) | — | Buffer for very large files, read/written bypassing the OS cache; feature off when absent, minimum 10 MiB. | |
| `CopyBlockSizeOther` | int (KiB) | 32 | Buffer size when source and target are on different physical disks (smaller is better). | |
| `CopyHugeBlockSizeOther` | int (KiB) | — | Huge-file buffer across disks; minimum 256 KiB. | Help suggests 1024/10240 and 128/32. |
| `SamePartitions` | string | — | Declares which drive letters sit on the same physical disk, comma-separated, e.g. `CEF,DGH`; same-disk copies use the non-"Other" block sizes. | Dialog: Copy/Delete. |
| `CountMarked` | bool | 1 | Pre-count files in subdirectories before copy/delete so a second progress bar is possible. | |
| `CountMarkedBackground` | bool | 1 | Same pre-count for background transfers; falls back to `CountMarked` when unset. | |
| `CountMarkedInDialog` | bool | 1 | Same pre-count directly inside the F5 dialog; falls back to `CountMarked`. | |
| `CountSpace` | bool | 1 | Sum directory sizes when selecting with the space bar. | Dialog: Operation. |
| `CrcAppendExt` | bool | 1 | Checksums per file: 0 = replace the extension (`test.sfv`), 1 = append (`test.txt.sfv`). | Only with `CrcSeparateFiles`/`CrcSeparateFolders`. |
| `CrcBlake3BlockSize` | int (MiB) | 1024 | BLAKE3 block size; 0 = 256 KiB without memory mapping, 1 = 1 MiB single-threaded, ≥2 enables multi-threading, 2047 = maximum. | 64-bit/SSD only. |
| `CrcForceUtf8` | bool | 0 | State of "Always use UTF-8 in names" in the checksum dialog. | |
| `CrcForwardSlashes` | bool | 0 | State of "Unix format: line breaks, '/' in paths" for checksum files. | |
| `CrcSeparateFiles` | bool | 0 | Create one checksum file per checked file instead of one combined file. | |
| `CrcSeparateFolders` | bool | 0 | Create one checksum file per folder. | |
| `CtrlArrow` | bool | 0 | Use the internal Ctrl+arrow word-jump in edit fields instead of the OS behaviour. | |
| `CtrlArrowOpenArchive` | bits | 5 | Whether Ctrl+Left/Right enters archives: 0 = never, 1 = when Enter would, 2 = always except `Packer.ZIPlikeDirectory=0`, 3 = always, +4 = leave the cursor on the file when not opened, +8 = honour `PreferOpenTypes` (with 1). | Not applied in search-feed-to-listbox mode. |
| `CustomDateFormat` | enum | -1 | `-1` = system, 0 = M-D-Y, 1 = D-M-Y, 2 = Y-M-D. | Dialog: Tabstops/Data formats. |
| `CustomFieldsInDlg` | bool | 0 | Show custom fields (e.g. `[tc.comment]`) in overwrite dialogs. | |
| `CustomSeparators` | string | 0000 | Date, time, thousands and decimal separator, one character each; `0` = take from the system. | Dialog: Tabstops. |
| `CustomTimeFormat` | bool | 0 | Show seconds in the Date column. | Dialog: Tabstops. |

### 2.4 `[Configuration]` D–E

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `DarkMode` | enum | 0 | 0 = off, 1 = follow the app/OS mode, 2 = on. | Dialog: Colors; `/M=` overrides at startup. |
| `DeleteDelay` | ms | 0 | Pause after moving a file to the recycle bin. | Increase only to dodge a third-party crash. |
| `DeviceChange` | enum | 2 | React to device-change notifications: 1 = always, 2 = only while foreground, 3 = as 2 plus never switch away from removed drives, 0 = ignore. | Win message-based. |
| `DirBrackets` | bits | 1 | Directory rendering: 1 = `[name]`, 2 = trailing `\`, 3 = both. | Dialog: Display. |
| `DirsInFooter` | bool | 1 | Show selected/total directory counts in the footer. | Dialog: Tabstops. |
| `DirMenuHotkeys` | bool | 1 | Use `+`, `-`, `*` as add/remove/configure hotkeys in the directory hotlist. | |
| `DirMenuLast` | int | — | Index of the last shown entry in the hotlist configure dialog, 1-based like `[DirMenu]`. | |
| `DirTabFilters` | bool | 1 | Remember the display filter per tab rather than per panel. | Dialog: Folder tabs. |
| `DirTabGotoLastVisited` | bool | 0 | Ctrl+Tab returns to the last visited tab (only when `DirTabOpenList=0`). | |
| `DirTabIcons` | bits | 255 | Tab icons: 1 = enabled, 2 = drive icons at non-network roots, 4 = folders with `desktop.ini`, 8 = virtual folders, 16 = file-system plugins, 32 = FTP, 64 = icons from view modes, 128 = reserved. | Dialog: Folder tabs → ">>". |
| `DirTabLimit` | int | 32 | Maximum characters of text on a tab. | Needs bit 8 of `DirTabOptions`. |
| `DirTabOpenList` | bool | 0 | 0 = Ctrl+Tab cycles tabs, 1 = Ctrl+Tab opens the open-tabs list. | |
| `DirTabOptions` | bits | 824 | 1 = tabs always visible, 2 = multiple rows, 4 = equal widths, 8 = length limit, 16 = confirm close-all, 32 = double-click closes, 64 = new tabs foreground, 128 = new tabs near current, 256 = `*` on locked tabs, 512 = clicking a tab activates its panel, 1024 = drive letter in title. | Dialog: Folder tabs. |
| `DirTabRevert` | bool | 1 | On tab change return to the locked tab's root directory. | Dialog: Folder tabs. |
| `DirUsageLocation` | enum | 1 | Where the most-frequently-used list (`tcDirFrq.txt`) is kept: 1 = `%LOCALAPPDATA%\GHISLER\`, 2 = beside `wincmd.ini`, 0 = feature off. | Dialog: History. |
| `DisconnectOnSuspend` | bits | 0 | 1 = drop all FTP connections on system suspend, 2 = disconnect file-system plugins that support it. | |
| `DllLoad` | enum | 0 | DLL load policy for plugins and shell extensions: `-1` = never from the current directory, 0 = only in the context menu, 1 = also from the current directory. | Security-relevant. |
| `DnsIpAddress` | string | — | Up to four comma-separated IPv4 addresses used to resolve the update server. | |
| `DpiPerScreen` | bool | 1 | 0 = let the OS scale the whole window across mixed-DPI monitors instead of per-screen scaling. | Win10 DPI model. |
| `DriveBarHide` | string | — | Hide drive-bar elements: letters = drives, `0-9` = FTP, `\` = network, `.` = parent/root buttons, `-` = volume label and free-space label, `;` = letter-less USB devices; `$`-prefix hides terminal-server drives. | Dialog: Layout / Icons. |
| `DriveisCD` | string | FGH | Treat these letters as CD-ROM (do not copy the read-only attribute when copying from them). | |
| `DriveLib` | path | — | DLL supplying the drive-button icons. | Dialog: Icons. |
| `DriveLib.bak` | path | — | Backup of the user's `DriveLib` value, kept when the user switches to Explorer drive icons. | Runtime bookkeeping. |
| `DrivesExportUpcase` | bool | -1 | `1` = uppercase drive letters when exporting names/running programs, `≤0` = lowercase. | Defaults to `DrivesShowUpcase`. |
| `DrivesShowUpcase` | enum | -1 | Drive-letter case in UI: 1 = uppercase, 0 = lowercase, `-1` = leave unchanged. | |
| `DriveTipDelay` | ms | 0 | Delay before drive descriptions appear in the drive combo box. | |
| `DropExtensions` | bool | 1 | Allow shell drag&drop extensions to handle right-button drags. | |
| `EditNewName` | string | — | Default new name offered by Shift+F4 when the cursor is on a directory. | |
| `Editor` | path | notepad.exe | Editor used by F4. | Dialog: Edit/View; `[Lister] Editor` can override for Lister. |
| `EditWaitTime` | s | 2 | Wait after an editor closes before assuming an archive/FTP temp file is free again; `0` = off, `-1` = never show the follow-up dialog. | |
| `EncodeLinesPerFile` | int | 0 | Maximum lines per output file when encoding; 0 = unlimited. | |
| `EncodeMethod` | enum | 0 | Last encoding method: 0 = MIME, 1 = UUE, 2 = XXE. | |
| `ESCcheck` | bool | 1 | Check for a stuck Esc key before starting a copy. | |
| `Everything` | path+args | Everything.exe -startup | Command line of the Everything search tool to launch on demand. | Third-party Win tool; see `UseEverything`. |
| `Everythingforsize` | bool | 0 | Use Everything to answer directory-size queries (space / Alt+Shift+Enter). | |
| `EverythingForTree` | bool | 0 | Use Everything to build the Alt+F10 tree. | |
| `ExecCmdLineFocused` | bool | 0 | Enter runs the command line only when it has focus. | Dialog: Layout. |
| `ExecUseContextMenu` | bool | 0 | Open files by invoking the shell context menu's default verb instead of the direct call. | |
| `ExplorerForCopy` | bool | 0 | 1 = perform F5/F6 through Explorer copy+paste instead of the internal engine. | |

### 2.5 `[Configuration]` F–H

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `FileTipWindows` | bool | 0 | Tooltip with the full name for names truncated in full view. | Dialog: Display. |
| `FileTipWindowsOnIcon` | bits | 0 | 1 = show the tip only over the icon, 2 = Win32-style tip on the icon and full name on the truncated name. | Requires `FileTipWindows≠0`. |
| `FirstFTPSync` | bool | 1 | Show the first-time warning for FTP synchronization. | One-shot education flag. |
| `FirstMLSDsync` | bool | 1 | Show the first-time warning when synchronizing over FTP using MLSD instead of LIST. | |
| `Firstmnu` | any | — | Writability probe written without an error box, like `Test`. | Internal. |
| `FirstTime` | bool | 1 | First run → show release notes. | Internal education flag. |
| `FirstTimeIconLib` | bool | 1 | First run of a multi-icon-library version → ask which library to use. | |
| `FirstTimeUnpack` | bool | 1 | First "unpack all" → hint about pressing Enter on a file. | |
| `FirstTimeZIP` | bool | 1 | First pack/unpack → show the packer dialog. | |
| `FixEmptyPanel` | bool | 0 | Workaround when the program starts with two completely empty panels. | |
| `FontPreview` | bits | 0 | 1 = use the Explorer preview for fonts, 2 = only on Win10 1607+. | |
| `FontQuality` | enum | 0 | Font rendering quality override: 0 = system default, 3 = non-antialiased, 4 = antialiased, 5 = ClearType, 6 = ClearType natural. | Dialog: Font. |
| `FontQualityMenu` | enum | 0 | Same as `FontQuality` for the main menu (64-bit build). | |
| `FontSampleText` | string | — | Text used in the font preview; empty = built-in sample. | Dialog: Font. |
| `ForceDarkModeOptions` | bits | — | Force dark-mode workarounds on older Windows: 1 = enable, 2 = handle dark functions internally except the title bar, 3 = also skip the dark title bar, 4 = do not recolour menus, 5 = 3+4; 0 = disable. | Win-version-specific. |
| `FrequentlyUsedLen` | int | 30 | Number of most-frequently-used directories shown (max 200). | Dialog: History. |
| `FrequentlyUsedLenStored` | int | 200 | Number of them stored in `tcDirFrq.txt` (no upper limit). | |
| `GetDirSize` | enum | 0 | Directory-size calculation: 0 = manual only, 1 = automatic on local drives, 2 = also on network, 3 = only via Everything. | |
| `GetTextMode` | enum | 0 | Screen-reader output formatting: column delimiter 0 = space, 1 = tab, 2/4 = CR, 3/5 = CRLF; modes 4 and 5 add tab-separated column headers. | Accessibility. |
| `HebrewListerFix` | bool | 0 | Coordinate correction when selecting English text in Lister on Hebrew Windows. | Win-only workaround. |
| `HideRemovableNoMedia` | bits | 0 | Hide removable drives with no media: 1 = drive bar, 2 = drive combo, 4 = search dialog, 7 = all. | Can be slow. |
| `HintMultiFilters` | bool | 0 | Show tooltips from all matching tooltip filters instead of only the first. | |
| `HistoryAppend` | bool | 1 | Selecting from the history appends the directory at the end instead of moving the current position. | |
| `HistoryCut` | bool | 1 | Cut the history at the current position when navigating away from the end. | |
| `HistoryFont` | string | 0 | History-list font: 0 = file-list font, 1 = menu font, otherwise a font name. | Dialog: History. |
| `HistoryIcons` | bool | 1 | Show icons in the directory history. | |
| `HistoryLen` | int | 26 | Entries shown in the directory history (max 100). | |
| `HistoryLenStored` | int | — | Entries written to the ini (default 200 on NT-family, max 200). | |
| `HistoryMaxWidth` | int | 120 | Maximum history panel width in average character widths. | |
| `HistoryScrollTo` | enum | 0 | Initial scroll position of the combined list: 0 = last visited, 1 = most used, 2 = top, 3 = bottom, 4 = middle of history, 5 = middle of frequent list. | Dialog: History. |
| `HistoryThinning` | bool | 1 | Only record a directory when the user stayed or performed a file operation there. | Dialog: History. |
| `HistoryThinningDelta` | ms | 5000 | Minimum dwell time for thinning; 0 = only file operations count. | |

### 2.6 `[Configuration]` I–L

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `IconClickSelection` | enum | 1 | 1 = clicking an icon selects the file, 2 = same but only for touch input. | Dialog: Operation. |
| `IconFromIcoFile` | bool | 1 | Load `.ico` icons from the file itself rather than from the association. | Internal associations still apply. |
| `IconLib` | path | — | DLL/ICL that supplies all default list icons. | Dialog: Icons. |
| `IconLib.bak` | path | — | Backup of the user's `IconLib` value when switching to Explorer icons. | Runtime bookkeeping. |
| `IconsInDriveList` | bool | 1 | Drive icons in the drive combo box and the "drive not found" dialog. | |
| `IconsInMenus` | bits | 17 | 1 = menu icons, 2 = frame around them, 4 = 24×24, 8 = 32×32 (4+8 = 48×48), 16 = show toggled items as pressed buttons. | Dialog: Icons. |
| `IconsInThread` | bool | 1 | Extract EXE/LNK icons in a worker thread. | |
| `IconsOnNet` | bool | 1 | Extract EXE/LNK icons on network drives too. | |
| `IconsOnNetReadOnly` | bool | 0 | Also load folder icons on read-only network shares. | Help warns this is very slow on shared CDs. |
| `IconOverlays` | bool | 0 | Show shell overlay icons (e.g. the shortcut arrow). | |
| `IconOverlaysOneDrive` | bits | 3 | 1 = cloud-only files, 2 = always-available marker, 4 = also in OneDrive for Business paths. | Win-only. |
| `IconOverlaysUsb` | bool | 1 | Eject overlay on USB drives. | |
| `IconsSpecialFolders` | bits | 3 | Special folder icons: 1 = local file system, 2 = virtual folders / network neighborhood, 3 = both, 0 = none. | Dialog: Icons. |
| `IgnoreDirErrors` | enum | 0 | 1 = allow entering unreadable directories and show `<Read error!>`; `-1` = never show it and always navigate away; 0 = show it only at startup for locked tabs. | |
| `IgnoreLinks` | bits | 0 | Ignore links/junctions in: 1 = copy/move/size, 2 = internal packers, 4 = branch view, 8 = find and tree rescan, 16 = even when the link itself is selected. | "Synchronize dirs" uses `SyncIgnoreJunctions` instead. |
| `IgnoreListFile` | path | — | Text file listing names to hide/exclude; supports bare names, per-drive-root names, absolute paths and wildcard masks, quoted when a name mixes spaces and wildcards. | Dialog: Ignore list. |
| `IgnoreListFileEnabled` | bool | 1 | Whether the ignore list is currently active (toggled by `cm_SwitchIgnoreList`). | |
| `IgnoreWheel` | bool | 0 | Ignore IntelliMouse wheel events entirely. | |
| `InactiveFocus` | color/bool | 0 | Mark the cursor in the inactive panel: 1/`0` = off/on, or an RGB value; `$1000000` = dashed, `$2000000` = dash-dot, `$3000000` = solid frame. | Dialog: Colors. |
| `InactiveFocusDark` | color | — | `InactiveFocus` for dark mode; defaults to the normal-mode value. | |
| `InheritCaseSensitiveDir` | enum | 1 | New folders inherit the "case sensitive names" flag: 0 = off, 1 = only when no elevation is needed, 2 = always. | Win/NTFS-specific. |
| `InplaceOkButton` | enum | 1 | Inplace-rename confirm button: 1 = right, 2 = left, 0 = none. | |
| `InplaceRename` | bool | 1 | Enable inplace rename on a second click on a name. | |
| `InplaceRenamePath` | enum | 1 | Inplace editing of the current path: 0 = off, 1 = on, 2 = off but single click changes directory. | Defaults to `InplaceRename` when absent. |
| `InplaceRenameSync` | bool | 1 | Inplace rename also in "Synchronize dirs". | |
| `InstallDir` | path | — | Last installation directory, mirrored in the registry. | Informational. |
| `KeyboardID` | hex/int | 0 | Switch to this keyboard layout on activation: 0 = disabled, 1 = system default for the locale, otherwise a hex language id (`$0409` US, `$0407` DE, …). | Win-only. |
| `KeyPath` | path | — | Directory (no file name) holding the registration key; `$` = read from the registry. | Licensing. |
| `LanguageINI` | path | — | Active language file, e.g. `WCMD_DEU.LNG`. | Dialog: Language. |
| `Lastmd5` | enum | 0 | Last checksum type: 0 CRC32, 1 MD5, 2 SHA1, 3 SHA256, 4 SHA512, 5 SHA224, 6 SHA384, 7 SHA3-256, 8 SHA3-512, 9 SHA3-224, 10 SHA3-384, 11 BLAKE3. | |
| `Lastrunas` | string | — | Last user name used by "run as different user". | |
| `LastSearchOptions` | bits | 0 | Last search flags: 1 = whole words, 2 = case, 4 = regex, 8 = hex, 16 = not-containing, 32 = ANSI, 64 = DOS, 128 = UTF-16, 256 = UTF-8, 512 = Office XML, 1024 = archives, 2048 = plugin text search. | |
| `ListName` | path | — | Last download-list name used by "download later" / "FTP download from list". | |
| `LoadDriveIconsInBackground` | int | 1 | Preload Explorer drive icons in a thread; values >500 are also used as the delay in ms. | |
| `LockedGoToDriveRoot` | bool | 0 | The `[ \ ]` button goes to the drive root (1) instead of the locked tab's base directory (0). | |
| `LockedTabChange` | enum | 1 | Directory change on a locked tab: 0 = always new tab, 1 = reuse a tab showing the same directory, 2 = reuse the first unlocked tab, 3 = reuse the last unlocked tab. | |
| `LogEncryptedNames` | enum | 0 | 0 = never log names inside header-encrypted archives, 1 = log all unpacks but warn when opening such an archive. | |
| `LogFile` | path | — | File-operation log target (FTP excluded). | |
| `LogKeepCount` | int | 30 | Number of rotated log files to keep. | |
| `LogOptions` | bits | 7198 | What to log: 1 = enabled, 2 = copy/move/shortcut, 4 = delete files, 8 = create/delete dirs, 16 = archive ops, 64 = file-system plugin ops, 1024 = success, 2048 = failure, 4096 = skipped. | Bit 32 is documented as unused. |
| `LogRotateLimit` | int | 0 | `-1` = new log file each day after midnight, `>0` = rotate at that size in KiB. | |
| `LongInStatus` | bool | 0 | Show the long name in the status bar of full view. | |
| `LongNameCopy` | enum | 0 | Warn about >259-character targets: 0 = always, 1 = never, 2 = only when the source is short, 3 = disallow long names. | |
| `LongNamesToDos` | bool | 0 | Pass short/long names to the DOS box when launching via Shift+Enter on Win9x. | |
| `LowercaseStrings` | string | `English in Titles:a,an,the,…` | Word list for the multi-rename "English in Titles" case function. | One entry per rule set, `:` separates name and list. |

### 2.7 `[Configuration]` M–O

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `Mainmenu` | path | — | Location and file name of a custom main menu. | Dialog: Language. |
| `MarkDirectories` | bool | 1 | The `+`/`-` selection keys also select directories. | Dialog: Operation. |
| `MaxCompareSizeInMem` | int (B) | 1048576 | Files up to this size are compared fully in memory; larger ones are memory-mapped. | |
| `MaxItemsInComboBox` | int | 20 | Entry cap for various history combo boxes (new folder, search, multi-rename, new connection); max 500. | |
| `MediaRemote` | enum | 0 | Media-centre remote handling: 1 = full mapping (back/play/tab/menu…), 0 = browser buttons only, `-1` = ignore all media keys. | Win input. |
| `MenuHideChar` | char | — | Prefix character that hides entries in the directory hotlist; `-` disables the feature. | |
| `MftAllowed` | bool | 0 | Allow access below `drive:\$MFT`. | Blocked by default to avoid a Windows crash. |
| `MinimizeOnClose` | bits | 0 | Minimise to tray instead of closing: 1 = close button, 2 = Alt+F4, 4 = the Alt+F4 function-key button. | Shift or Files→Quit forces the close. |
| `MinIconWidth` | int (px) | -2 | Minimum icon width; `>0` absolute, `<0` added to the normal icon width. | Dialog: Icons exposes only negative values. |
| `MinLineHeight` | int (px) | -1 | Minimum list line height; `>0` absolute, `<0` added to the normal line height. | Dialog: Icons. |
| `MoveToDirWarn` | bool | 1 | F6/Shift+F6: warn when the typed target is an existing directory name. | |
| `MrtReplacePluginChars` | bool | 1 | Auto-replace characters that are invalid for plugin results in the multi-rename tool. | |
| `MultiRenameEdit` | path | — | Editor used to edit new names in the multi-rename tool. | |
| `MultiRenameLoadSettings` | string | — | Startup state of the multi-rename tool: empty = last used, a name = load that saved setting, anything else = defaults. | |
| `MultiRenameUndoHistoryLen` | int | 20 | Number of multi-rename operations that can be undone; 0 = disable undo. | |
| `NetHoodInThread` | bool | 1 | Enumerate the network neighborhood in a background thread. | |
| `NetHoodReadInfo` | bits | — | Network enumeration calls: 1 = GetAttributesOf, 2 = SHGetDataFromIDList. | Win shell API. |
| `NetHoodStyle` | enum | 1 | Network neighborhood layout: 0 = as the OS provides, 1 = add the XP-style "Workgroup" level, 2 = XP style only. | |
| `NetwareBugfix` | bool | 0 | Never rename files on remote (NetWare) drives. | Legacy network workaround. |
| `NewFolderOverPanel` | bool | 1 | Show the F7 dialog centred over the target file panel instead of the whole window. | |
| `NewStyleProgress` | enum | — | 1 = OS progress bar (default on XP+), 0 = internal bar, 2 = OS bar without segmentation. | |
| `NewTimeZoneMethod` | bool | — | 1 = Windows 7-style time-zone handling, 0 = legacy. | |
| `NoReread` | string | — | Drives never re-read automatically; `\` = network, `/` = plugins, `:` = virtual folders. | Dialog: Refresh. |
| `NoReread2` | string | — | Drives not re-read when the program is restarted (falls back to `NoReread`). | Dialog: Refresh. |
| `OldCmdLineCount` | int | 20 | Number of command lines kept in the command-line history. | |
| `OldDriveIcons` | enum | 0 | 0 = Explorer drive icons, 1 = TC 6.5x icons, 2 = TC 7/8 icons. | Ignored when `DriveLib` is set. |
| `OldListIcons` | bool | 0 | 1 = use the TC 7/8 file/folder icons, 0 = Explorer icons. | Ignored when `IconLib` is set. |
| `OldStyleDir` | bool | 0 | Verify directories with `SetCurDir` instead of `FindFirstFile`. | Legacy workaround. |
| `OldStyleHistory` | bool | 0 | Render the directory history as a plain Win32 menu. | |
| `OldStyleTree` | enum | 0 | Tree in a file panel: 0 = Explorer-style for all drives, 1 = always load the full drive tree, 2 = expandable tree for the current drive only. | Dialog: Display. |
| `OleComments` | enum | 1 | Comment sources: 0 = comment files only, 1 = Explorer (NTFS streams, Office docs) plus comment files, 2 = Explorer only, 3 = none (plugins only). | Dialog: Display. |
| `OnlyOnce` | enum | 0 | Single-instance policy: 1 = one instance per user, `-1` = one per computer, `abs(x)>1` = same with more instances allowed. | Overridden by `/N`, `/O`. |
| `OpenInThread` | bool | 1 | Launch programs from a worker thread. | |
| `OpenOnSameMonitor` | bool | 1 | Ask the OS to place launched programs on the monitor the file manager is on. | |
| `OpenUrls` | bool | 1 | `.url` files pointing at file-system directories are opened internally. | |
| `OverrideDPI` | int | — | Dialog scaling in DPI, overriding the system value. | Dialog: Font. |
| `OverrideDPI96` | int | — | Per-scale-factor form (`OverrideDPI96`, `OverrideDPI120`, …) of `OverrideDPI`. | Multi-monitor DPI. |

### 2.8 `[Configuration]` P–R

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `PanelsVertical` | bool | 0 | 0 = panels side by side, 1 = stacked vertically. | Dialog: Layout. |
| `PathExtraHeight` | int (px) | -100 | Extra/reduced height of the current-path field (at 96 dpi, scaled). | |
| `PhotoAppMode` | enum | 0 | How the Photos app is launched with file navigation: 0 = auto by version, 1 = legacy call, 2 = `ms-photos:` URL with encoding, 3 = without encoding, `-1` = no navigation. | Win-only. |
| `PhotoAppFilter` | string | — | `-` disables the "open photo app with sibling images" behaviour. | Win-only. |
| `PluginBaseDir` | path | — | Root below which new plugins are auto-installed (the plugin type and default folder are appended). | Dialog: Plugins. |
| `PopupDelay` | ms | 500 | Right-button selection mode: delay before the context menu appears. | |
| `PortInDriver` | bool | 1 | Do parallel-port transfers inside the driver (faster, blocks the machine) instead of user mode. | Win9x-era; NT requires the driver. |
| `PreferHashType256` | enum | 1 | Preferred 256-bit hash when verifying from clipboard/name: 1 = SHA-2, 2 = SHA-3, 3 = both, 4 = BLAKE3, 5 = BLAKE3+SHA-2, 6 = BLAKE3+SHA-3, 7 = all; add 16 to stop the dialog choice from being saved. | |
| `PreferHashTypeOther` | enum | 1 | Same preference for other hash lengths (1 = SHA-2, 2 = SHA-3, 3 = both; +16 as above). | |
| `PreferOpenTypes` | pattern list | `+` | Types a double click should open in their associated application rather than as archives; a leading `+` extends the built-in list, and a pipe character marks types that must always be opened as archives. | |
| `PreloadThumbnails` | enum | 1 | 1 = preload on directory open, 2 = also in branch view, 3 = only the next two rows, 0 = on scroll only. | Dialog: Thumbnails. |
| `PreventScrollbarCrash` | bool | — | Enable the Lister scrollbar crash workaround (auto-enabled on Windows 8.1). | |
| `PromptForUser` | bool | 0 | Also set the "prompt for credentials" flag when connecting a network drive. | |
| `QuickSearchAllowNoMatch` | bool | 0 | Allow typing or pasting text that matches nothing. | |
| `QuickSearchAutoFilter` | enum | 0 | Automatic Ctrl+S filter: 0 = off, 1 = on, `-1`/`2` = as 0/1 but the state is not persisted. | Dialog: Quick Search. |
| `QuickSearchExactMatch` | bool | 0 | Typing a dot is taken literally instead of as `*.`. | Dialog: Quick Search. |
| `QuickSearchHistoryAutoFilter` | bool | 0 | Automatic filter inside the history list. | |
| `QuickSearchMatchBeginning` | bool | 1 | Typed text must match the beginning of the name (0 = implicit leading `*`). | Dialog: Quick Search. |
| `QuickSearchTabsAutoFilter` | bool | 0 | Automatic filter inside the open-tabs list. | |
| `QuickViewFollowLinks` | bits | 1 | Ctrl+Q through links: 1 = always (network only when the link is also on the network), 2 = always, 0 = never, +4 = also follow `.url` files. | |
| `QuickViewUnzipReadMe` | enum | 1 | Show readme files with Ctrl+Q on an archive: 1 = always, `-1` = only for real archive extensions, 0 = never. | |
| `RedirectSection` | string | — | Per-section ini redirection target, see 1.5. | Valid only in the main ini. |
| `RelativePathCopy` | bits | 2 | F5/F6 "keep relative paths": 1 = enabled at start, 2 = remember while running, 4 = remember across restarts, 8 = drop the drive/server from search results, `-1` = hide the option. | |
| `RememberTransferManagerPos` | bool | 0 | Remember position and size of the background transfer manager. | Dialog: Layout-ish, window state. |
| `RenameEditUnicode` | bool | 0 | Multi-rename "edit names" writes Unicode always (1) or only when needed (0). | |
| `RenameSelOnlyName` | bool | 0 | Inplace rename selects only the name, not the extension. | Dialog: Operation. |
| `RenameUpDown` | bool | 1 | During rename, up/down commit the current name and jump to the previous/next file. | |
| `ReplacePhotoApp` | bool | 1 | Open images with the classic Photo Viewer instead of the tile app. | Win-only. |
| `ResolutionSpecific` | bool | 1 | 0 = keep resolution-specific data in one `[AllResolutions]` section, 1 = one section per resolution. | Dialog: Font/Icons. |
| `RestoreSize` | bits | 7 | Re-apply the stored geometry on: 1 = display change, 2 = DPI change, 4 = window move. | |
| `RestrictInterface` | bits | — | Disable UI parts: 1 = Start menu, 2 = command line, 4 = button-bar editing, 8 = drive context menu, 16 = file context menu, 32 = hotlist editing, 64 = FTP, 128 = Enter/execute, 256 = file-system plugins, 512 = port connection, 1024 = update checks, 2048 = only signed plugins, 4096 = only digitally signed plugins, 6144 = no plugins, 8192 = do not redirect plugin inis. | Administrative lock-down. |
| `ReverseHistory` | bool | 0 | Show the directory history newest-first. | |

### 2.9 `[Configuration]` S

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `SameScreenAsTC` | bits | 1 | Multi-monitor placement: 1 = modal dialogs, 2 = Lister, 4 = compare window follow the main window's screen. | |
| `SaveCommands` | bool | 1 | Save command lines on exit. | Dialog: Operation → Save on exit. |
| `SaveFrequentlyUsed` | bool | — | Save the most-frequently-used directory list; default is the value of `SaveHistory`. | |
| `SaveHistory` | bool | 1 | Save the visited-directory history into `[LeftHistory]`/`[RightHistory]`. | Dialog: History. |
| `Savepath` | bool | 1 | Save the two panel paths on exit (and folder tabs when enabled). | Dialog: Operation → Save on exit. |
| `Savepanels` | bool | 1 | Save panel display type, sort order and filter on exit. | Dialog: Operation → Save on exit. |
| `ScrollingTopMargin` | int (lines) | 4 | Extra lines kept visible above the cursor when scrolling by keyboard. | Dialog: Operation. |
| `ScrollingBottomMargin` | int (lines) | 4 | Extra lines kept visible below the cursor. | |
| `ScrollingMarginFlags` | bits | 3 | 1 = vertical lists, 2 = brief view. | |
| `SearchInFiles` | pattern | — | Additional archive types searched by the content search, e.g. `*.EXE *.JAR`. | |
| `SearchKeepLast` | bool | 1 | Keep the last search results when the Find dialog closes (except after "feed to listbox"). | |
| `SelectionFocus` | color/bool | 0 | Frame colour for the cursor when files are selected; add `$4000000` to apply only to unselected files. | Same value grammar as `InactiveFocus`. |
| `SelectionFocusDark` | color | — | `SelectionFocus` for dark mode. | |
| `SelectionMarker` | string | — | Characters returned by the list control for selected rows, for screen readers. | Accessibility. |
| `SeparateTree` | enum | 0 | 0 = no separate tree, 1 = one, 2 = two trees. | Dialog: Layout. |
| `SeparateTreeWidth` | int (px) | 160 | Width of separate tree panels. | |
| `SetArchiveBitOnRename` | bool | 1 | Set the archive attribute when renaming. | |
| `SetEncoding` | string | `äö.do.not.remove` | Marker that keeps Notepad from re-saving the ini as UTF-8. | Do not remove. |
| `ShellExtIcons` | bool | 1 | Show per-file icons supplied by shell extensions. | Dialog: Icons. |
| `ShowAMPM` | bool | 0 | 12-hour AM/PM time format. | Dialog: Tabstops. |
| `ShowCentury` | bool | 1 | Show the century in dates. | Dialog: Tabstops. |
| `ShowCopyOptions` | bool | 0 | Always show (pin) the extended options in the F5 dialog. | |
| `ShowEXEandLNKicons` | enum | 2 | 0 = standard icons only, 1 = extract EXE/LNK icons, 2 = as 1 but not from floppy drives. | Dialog: Icons. |
| `ShowExtraDrives` | bits | 7 | Extra entries in the drive combo: 1 = My Computer, 2 = My Documents, 4 = devices without a drive letter. | Win-only items. |
| `ShowFrequentlyUsed` | bool | 1 | Show the most-frequently-used directories next to the history. | Dialog: History. |
| `ShowFrequentlyUsedMaxInMiddle` | bool | 0 | Place the most-used entry in the middle of the list instead of at the edge. | |
| `ShowHiddenDimmed` | bool | 1 | Draw hidden files/folders dimmed. | Dialog: Icons. |
| `ShowHiddenSystemOverlay` | bool | 1 | Exclamation-mark overlay on hidden/system files. | |
| `ShowHiddenSystem` | enum | 0 | 0 = neither, 1 = hidden and system, 2 = hidden only, 3 = system only. | Dialog: Display. |
| `Showicons` | enum | 2 | 0 = no icons, 1 = default icons, 2 = all icons. | Dialog: Icons. |
| `ShowInstanceNr` | bool | 1 | Append the instance number to the title when several copies run. | |
| `ShowLastVisitedInMiddle` | bool | 0 | Place the last visited directory next to the most-used list instead of at the edge. | |
| `ShowParentDirInRoot` | bool | 0 | Show `[..]` also inside a drive root. | Dialog: Display. |
| `ShowProgressBarSpeed` | bool | 1 | Show transfer speed in the progress bar when available. | |
| `ShowWarningIcons` | bool | 1 | Show the warning marker beside options the user has enabled in dialogs. | |
| `SimulateUpdate` | bool | 0 | Pretend an update is available. | Testing aid. |
| `SingleClickStart` | bits | 0 | 0 = double click, 1 = single click opens files and folders, 2 = single click only opens folders, +4 = the text cursor no longer follows the mouse. | Dialog: Operation. |
| `SizeCheck` | bool | 1 | Check free space on the target before copying. | |
| `SizeFooter` | enum | 1 | Size style in header/footer: 0 = bytes, 1 = KiB, 2 = dynamic, 3 = dynamic with 1 decimal, 4/5 = as 2/3 without GB, 6/7 = as 2/3 with TB, 8/9 = as 3/7 with 2 decimals. | Dialog: Tabstops → Header/footer. |
| `SizeStyle` | enum | 0 | Size style inside the file list; same value ladder as `SizeFooter`. | Dialog: Tabstops → Size display. |
| `SizeStyle1000` | bool | 0 | 1 = decimal units (1 k = 1000), 0 = binary units (1 k = 1024). | |
| `SizeTransfer` | enum | 1 | Size style in the copy dialog; defaults to `SizeFooter`; values 0 and 1 are forced to automatic. | |
| `SkipCharOnCursor` | enum | 1 | Cursor left/right with a selection: 0 = jump to the selection edge, 3 = ignore the selection, 1/2 = 3 for one direction only. | |
| `SkippedFileCopy` | enum | 1 | 1 = copy everything that needs no question first and ask at the end, 0 = ask in order, ≥100 = as 1 with that many ms of initial asking. | |
| `Small83names` | bool | 0 | Show 8.3 names in lower case as older Explorer did. | Dialog: Display. |
| `SortDirExt` | bool | 0 | Allow sorting directories by extension (only when `SortDirsByName=0`). | |
| `SortDirsByName` | bool | 1 | Always sort directories by name. | Dialog: Display. |
| `SortUpper` | enum | 0 | 0 = default, 1 = names starting with `_` last, 2 = natural (numeric-aware), 3 = natural via the OS comparison function. | Dialog: Display → Sort method. |
| `SoundDelay` | s | -10 | Minimum operation duration before the "finished" sound plays; negative = disabled. | Dialog: Misc. |
| `SpaceMovesDown` | bool | 0 | Space also moves down to the next file, like Insert. | |
| `SpecialCursorMovement` | bits | 0 | 1 = left/right jump to first/last file, 2 = left goes up a directory and right enters the entry, 4 = override the horizontal scrollbar in custom columns, 8 (with 1) = page instead of jump. | |
| `SpeedLimit` | int (KiB/s) | 5.0 | Last used transfer speed limit in the background manager. | |
| `SpeedLimitEnabled` | bool | 0 | Enable the speed limit immediately when the background manager opens. | |
| `SplitBytesPerFile` | int (B) | 0 | Last used part size for Files → Split File. | |
| `SplitBytesPerFileZip` | int (B) | 0 | Last used part size for multi-part ZIP. | |
| `StartupScreen` | bool | 1 | Show the splash screen. | |
| `SubdirCopyOrder` | enum | 4 | Order of files inside subdirectories: 4 = unsorted, 0–3 = ascending by name/ext/size/date, `-1`…`-4` = descending, 5 = same as the active panel. | |
| `SyncButtons` | bits | 63 | Visible buttons in Synchronize dirs: 1 = left→right, 2 = equal, 4 = not equal, 8 = right→left, 16 = duplicates, 32 = singles. | |
| `SyncChangeDirMode` | bits | 2 | `cm_SyncChangeDir` behaviour: 0 = switch off when the twin directory is missing, 1 = keep on, +2 = beep on first attempt, +4 = suspend while descending into a one-sided directory. | |
| `SyncFlags` | bits | 1 | Last settings of Synchronize dirs: 1 = subdirs, 2 = by content, 4 = ignore dates, 8 = asymmetric, 16 = empty directories, 32 = plugin compare functions (needs 2). | |
| `SyncHideHiddenSystem` | bits | 0 | 0 = do not hide, 1 = hide when both sides are hidden/system, 2 = hide when either side is, +4 = use the main program's hidden filter too. | |
| `SyncIgnoreJunctions` | bool | 0 | 1 = exclude directory junctions from synchronizing; 0 = show and follow them. | The help warns that a junction then looks like a file that exists on one side only. |
| `SyncMatch` | int | 1 | Preselected line of the `[1x]` match-mode combo; negative hides matching files. | |
| `SyncRecompareAfterCopy` | bool | 1 | Compare again automatically after synchronizing. | |
| `SyncWarnErrors` | enum | 1 | Warn about read errors: 0 = never, 1 = read failures, 2 = access denied, 3 = both. | |

### 2.10 `[Configuration]` T–Z

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `TabDir` | path | — | Remembered location for storing/loading folder-tab sets. | |
| `TabEnterDirChange` | enum | 1 | Enter on a directory inside a locked tab: 0 = nothing, 1 = open a new tab, 2 = ask. | |
| `TabFullColor` | int | 0 | 0 = tab colour from view modes drawn as a gradient, 1–100 = solid colour blended at that percentage. | |
| `TabSeparator` | char | `:` | Separator between drive letter and folder in tab captions. | The help line right after it documents `TabsListLastVisited=1` (last-visited-first in the Ctrl+Shift+A list) without printing the `=`. |
| `TabSwitchMode` | enum | 0 | 0 = Ctrl+Tab switches when Ctrl is released, 1 = immediately on Tab. | |
| `tcmatch` | path | — | Full path to `tcmatch.dll`, the custom quick-search filter DLL. | Env. vars allowed. |
| `tcmatch64` | path | — | Same for the 64-bit build. | |
| `Test` | any | — | Writability probe: written at start, error box on failure. | Internal. |
| `TestIfSharedDir` | enum | 1 | Show a hand overlay on shared directories: 0 = off, 1 = background NetShareEnum, 2 = legacy OLE2 method. | Win-only. |
| `ThreadFindFirst` | bool | 1 | Run directory enumeration in a thread so a hung network read can be aborted. | |
| `ThumbExplTypes` | pattern | `*.*` (with `*.htm *.html` excluded) | Masks for thumbnails extracted by the shell method; HTML is excluded for safety. | Dialog: Thumbnails. |
| `ThumbGdiplusTypes` | pattern | — | Masks extracted internally through GDI+. | |
| `ThumbHeightNoCache` | int (px) | — | Thumbnail height when no cache is used. | |
| `ThumbIconSize` | int (px) | 48 | Size of the icons overlay in thumbnails mode. | |
| `ThumbIrfXnTypes` | pattern | `*.*` | Masks extracted by IrfanView/XnView. | |
| `ThumbOptions` | bits | 15 | 1 = shell method, 2 = lister plugins, 4 = IrfanView/XnView, 8 = plain text, +16 = disable EXIF thumbnails. | Dialog: Thumbnails → Loading thumbnails. |
| `ThumbPlgTypes` | pattern | `*.*` | Masks extracted by lister plugins. | |
| `ThumbsCopyDel` | bool | 1 | Update the thumbnail cache when files are copied or deleted. | Dialog: Thumbnails. |
| `ThumbsCustomField` | string | — | Custom fields shown below thumbnails (legacy single-value form). | `[ThumbsCustomField]` holds the current form. |
| `ThumbsCustomFieldsEnabled` | bool | 1 | Enable custom fields below thumbnails. | |
| `ThumbsInDlg` | bool | 0 | Show thumbnails in overwrite dialogs. | |
| `ThumbsLocation` | path | — | Thumbnail database location (default `%LOCALAPPDATA%\GHISLER`). | Dialog: Thumbnails → Database location. |
| `ThumbTxtTypes` | pattern | `*.txt *.ini` | Masks rendered as plain-text thumbnails. | |
| `ThumbnailMemoryLimit` | int | 200 | Legacy per-panel thumbnail cache count (superseded by `ThumbnailSizeLimit`). | |
| `ThumbnailPrealloc` | int (B) | 100000 | Bytes pre-allocated when creating the thumbnail database to avoid fragmentation. | |
| `ThumbnailSizeLimit` | int (MB) | — | Memory budget for loaded thumbnails (default 20 MB on Win9x, 500 MB on NT-family; clamped to 1–1024 MB and ≤10 % of RAM). | |
| `ThumbWidthNoCache` | int (px) | — | Thumbnail width when no cache is used. | |
| `Tips` | bits | 3 | 1 = button-bar tooltips, 2 = drive tooltips, 3 = both. | Dialog: Display. |
| `TitleExtraText` | string | — | Extra title-bar text: `*text` prepended, `-text` prepended without the instance number, `$text` appended. | |
| `ToolTipDelay` | ms | 500 | Delay before button-bar tooltips appear. | Dialog: Display. |
| `TranslateMessageButtons` | bool | 1 | Translate standard message-box buttons into the program language. | |
| `TrayIcon` | bool | 0 | Move the icon to the notification area when minimised. | Win-only. |
| `Tree` | bool | 0 | Tree ordering: 1 = unsorted, 0 = sorted by name. | |
| `TreeFileLocation` | path | — | Alternative location of `treeinfo.wc`; `%D%`, `%R%`, `%L%` expand to drive, root path and volume label (must be uppercase). | |
| `TreeFileLocationE` | path | — | Tree file location for drive E only; requires `TreeFileLocation` to be set. | Same pattern applies per drive/server. |
| `TreeFileLocation__server_share` | path | — | Tree file location for one UNC share; requires `TreeFileLocation`. | Key form doubles the leading underscore. |
| `TreeUpdateLimit` | int (KiB) | 2048 | Size up to which the tree file is updated during file operations. | |
| `tsclient` | string | `\\tsclient` | Host name whose single-character shares are listed as drives. | Terminal-server/XPMode only. |
| `UnicodeListWarn` | any | — | Documented as no longer used since 7.5. | Obsolete. |
| `UpdateEnv` | bits | 3 | 1 = pick up environment changes from the system, 2 = also add new variables. | |
| `UpdateTreeAtStartup` | bool | 1 | A separate tree expands to the current directory at startup (0 = on first directory change). | |
| `UsbBufSize` | int (KiB) | -1 | Buffer size for the USB/LPT connection; `-1` = maximum (64 KiB). | Legacy port connection. |
| `UseChecksumCryptoApi` | enum | 1 | 0 = internal hash code, 1 = use the OS crypto API when reliable, 2 = force it. | |
| `UseEverything` | enum | 1 | Use the Everything tool for searching: `-1` = hide the option, 1 = use when present, 0 = off. | Win-only integration. |
| `UseEverythingInstance` | string | — | Name of the Everything instance to talk to. | |
| `UseFastCtrlC` | bool | 1 | Implement Ctrl+C/X by invoking the shell context menu without extensions, else the full menu. | |
| `UseFindForTime` | bool | 1 | Read file times via the directory enumeration instead of a per-file call. | |
| `UseIniInProgramDir` | bits | 0 | Portable-location control: 1 = use `wincmd.ini` beside the program, 2 = same for `wcx_ftp.ini`, 4 = override the registry but not the command line. | Only honoured when the ini is already there; no `RedirectSection`. |
| `UseLongNames` | bool | 1 | Use long file names. | Dialog: Display. |
| `UseNewDefFont` | enum | -1 | 1 = "Segoe UI Semibold", 0 = "Microsoft Sans Serif", `-1` = choose automatically for fresh vs upgraded installations. | Win fonts. |
| `UseRightButton` | bool | 1 | 1 = right mouse button selects, 0 = left. | Dialog: Operation. |
| `UseRubberBandSelection` | bool | 1 | Rubber-band (rectangle) selection in left-button mode. | |
| `UseShellConnect` | enum | 0 | Legacy network-share connection method; `-1` also shows separate user and password dialogs. | Workaround for old network stacks/drivers. |
| `UseTrash` | bool | 1 | Delete to the recycle bin when it is available. | Dialog: Copy/Delete. |
| `UseWindowsHello` | bool | 0 | Keep the master password protected by Windows Hello until exit. | Win-only. |
| `VerifyCopy` | bool | 0 | Verify copies by reading them back and comparing hashes. | Dialog: Copy/Delete. |
| `VerifyEnabled` | bool | 1 | `0` hides the "verify after copy" feature entirely. | |
| `Version` | string | — | Version stamp written by the program (not always updated). | Informational. |
| `VersionInOverwrite` | bool | 1 | Show file version info in the overwrite dialog; 0 recommended on slow networks. | |
| `Viewer` | path | none | Default viewer for Alt+F3; `none` = built-in. | Dialog: Edit/View. |
| `Viewertype` | enum | 1 | 0 = internal viewer on the file under the cursor, 1 = internal viewer on the marked files, 2 = external viewer. | Dialog: Edit/View. |
| `VirtualFolderDetails` | bool | 1 | Show sizes and timestamps in virtual folders (can be slow). | |
| `VirtualListbox` | bool | 1 | Use a virtual list control for the file lists; 0 for screen-reader compatibility. | Accessibility. |
| `VistaDelete` | enum | -1 | 1 = use the modern shell delete (moves whole directories), 0 = legacy delete, `-1` = modern if available but with the program's own confirmation. | Win Vista+. |
| `WantXPLook` | bool | 1 | Draw main-window buttons in themed style (32-bit, XP+). | |
| `WantZoneData` | bits | 7 | Copy the `Zone.Identifier` stream: 1 = enabled for internal copies/unpacks and plugins, 2 = internet http(s), 4 = anonymous ftp, 8 = authenticated ftp, 16 = intranet http(s). | Win/NTFS. |
| `WarnMissingExtensions` | bool | 1 | Warn in MTP-style virtual folders when extensions are hidden. | |
| `WarnSearchText` | enum | 1 | Warn when search text cannot be converted to the selected ANSI/ASCII code page: 0 = silent, `-1` = disable the affected modes, 2 = both. | |
| `WatchDirs` | bits | 0 | Directory auto-refresh: 1 = on, 2 = also size/date/attributes, 4 = append new files at the end, 8 = ignore while in background, 16 = also update the footer, 32 = also update the header, 256 = beep on each update. | Dialog: Refresh. |
| `WatchDirsExclude` | string | — | Drives excluded from directory watching; `\` = network, `/` = plugins, `:` = virtual folders. | Dialog: Refresh. |
| `Win32TipWindows` | bool | 0 | Win32-style tooltips; defaults to `FileTipWindows` when unset. | |
| `Win95Delete` | enum | -1 | Delete implementation: 0 = internal, 1 = Explorer method, `-1` = Explorer method only while the crash-prone third-party tool is running. | |
| `Wsock32` | path | — | Path of the socket library used by FTP. | Legacy. |
| `WipeDir` | path | — | Temporary directory used (and wiped on restart); ignored unless it contains `$wc` or `\_tc`. | |
| `WmfAllowed` | enum | -1 | 0 = no WMF display (and no folder thumbnails), 1 = WMF in Lister/thumbnails, 2 = folder thumbnails, 3 = both, `-1` = folder thumbnails only when the OS patch is present. | Security-sensitive (historical WMF bug). |
| `x64DisableRedirection` | bool | 0 | Disable the `system32` → `syswow64` redirection for the 32-bit build. | Win64-only; can break plugins. |
| `X64menu` | bool | — | Show the `x64`/`x32` submenu in the context menu (default 1 for the 32-bit build). | |
| `X64WaitTime` | s | 15 | Timeout for the 64-bit helper to return the context submenu. | |
| `XPMoveMethod` | enum | — | In-drive moves and permissions: 1 = take the target folder's permissions, 2 = as 1 and recurse into moved folders, 0 = keep the file's permissions. | Win ACLs. |
| `XPMoveMethodAdmin` | enum | 1 | Same for moves performed elevated. | |
| `ZipMagic` | bool | 0 | Skip the third-party ZipMagic detection when opening ZIPs. | |

---

### 2.11 Other `wincmd.ini` sections — keyboard remapping, aliases

| Section | Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|---|
| `[Shortcuts]` | *command* = key (or `key → command`) entries | string | — | Replaces the built-in shortcut keys: each entry maps a command name to a key sequence. | Dialog: Misc. → Redefine hotkeys. Layout is documented as `command=key` pairs. |
| `[ShortcutsWin]` | same entries | string | — | Like `[Shortcuts]`, but the keys include the Windows key. | Win-only modifier. |
| `[Alias]` | *name* = command line | string | — | Command-line aliases defined under Configuration → Misc. | Usable as the first word of the command line. |
| `[user]` | `menu1`, `cmd1`, `param1`, `path1`, `iconic1`, `key1` | mixed | — | User menu entries: title, command, parameters, start path, run-minimised flag, hotkey; numbered per entry. | Legacy start-menu format (dialog: `dlg_usermenu`). |
| `[dirmenu]` | numbered entries | string | — | User directory hotlist (Ctrl+D) as `title`/`command` lines. | Edited via the hotlist dialog. |
| `[Command line history]` | `0=`, `1=`, … | string | — | Last `OldCmdLineCount` command lines, newest at index 0. | Written only with `SaveCommands=1`. |
| `[DriveHints]` | `E` (the key is the drive letter) | string | — | One line per drive giving the description shown in drive selection, e.g. `E=ZIP drive`. | Dialog: Drive selection. |
| `[attrplugin]` | plugin-specific | mixed | — | Remembers the plugin used in the Attributes dialog and its option values. | |
| `[IgnoreMenuExtensions]` | extension entries | string | — | Filters context-menu extensions out of the `x64`/`x32` submenu. | |

### 2.12 File-type associations and Alt+F3 viewers

| Section | Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|---|
| `[Associations]` | `Filter<n>` | pattern | — | User-defined association filter (`Filter1`, `Filter2`, …), evaluated in numeric order until one matches. | Overrides OS associations inside the program only; aimed at USB-stick use. |
| `[Associations]` | `Filter1_open`, `Filter1_View`, … (`Filter<n>_<verb>`) | string | — | Command line for one verb, separated from the filter name by an underscore; `open` is the default verb used by Enter, other verbs appear in the context menu. | |
| `[Associations]` | `Filter1.icon`, `Filter2.icon`, … | string | — | Icon for the filter: `file,index`; negative index = icon index, positive = resource id. | |
| `[Extensions]` | `bat` (the key is the extension) | string | — | Association used by Alt+F3, e.g. `bat=c:\windows\notepad.exe ^.bat`; `%1` is also accepted. | Per-extension override of the configured viewer. |
| `[Alias]`, `[Associations]` | — | — | — | Both live in the visible, non-redirected part of the ini (see 1.5). | |

### 2.13 Resolution-dependent state and panel state

Resolution-specific data lives in a section named after the current screen, e.g.
`[1024x768 (10x20)]` — the pattern is `<width>x<height> (<fontheight>x<charwidth>)`. When
`ResolutionSpecific=0`, all of it is written to the single section `[AllResolutions]`. The
same keys exist in both.

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `x` | int (px) | — | Window x position. | |
| `y` | int (px) | — | Window y position. | |
| `dx` | int (px) | — | Window width. | |
| `dy` | int (px) | — | Window height. | |
| `maximized` | bool | 0 | 0 = normal, 1 = maximised. | |
| `monitor(x,y,dx,dy;dpi)` | tuple | — | Per-monitor position and size keyed by the monitored rectangle and DPI, stored inside the main monitor's section. | Multi-monitor geometry. |
| `Divider` | int (‰) | 500 | Splitter position between the panels, 0…1000. | |
| `DividerComments` | int (‰) | 500 | Splitter position while in comments view. | |
| `DividerQuickView` | int (‰) | 500 | Splitter position while quick view is open. | Stored independently from `Divider`. |
| `CompareDivider` | int (‰) | 500 | Splitter position in Compare by content. | |
| `ExtraDriveHeight`, `ExtraDriveWidth` | int (px) | 0 | Extra drive-bar button height and width. | |
| `ExtraDriveHeight_dpi`, `ExtraDriveWidth_dpi` | int (dpi) | — | DPI at which the two values above were stored, so they can be rescaled when the window moves between screens. | |
| `ExtraDriveHeight_120`, `ExtraDriveWidth_120` | int (px) | 0 | Per-screen overrides of the two values for a secondary screen with a specific DPI (`_120` = 120 dpi; the pattern generalises to `_<dpi>`). | |
| `Fontname` | string | Segoe UI Semibold / Microsoft Sans Serif / MS Sans Serif | File-list font. | Dialog: Font. |
| `Fontsize` | int | 8 (9 for Segoe UI Semibold) | File-list font size. | |
| `FontWeight` | int | 700 | 400 = normal, 600 = semibold, 700 = bold. | |
| `FontCharset` | int | 1 | Character set of the list font (ignored on NT-family). | |
| `FontNameWindow` / `FontSizeWindow` / `FontWeightWindow` / `FontCharsetWindow` | string/int | — | Font of the rest of the main window. | Dialog: Font. |
| `FontNameDialog` / `FontSizeDialog` / `FontWeightDialog` / `FontCharsetDialog` | string/int | — | Dialog font. | Dialog: Font. |
| `FontNameTip` / `FontSizeTip` / `FontWeightTip` / `FontCharsetTip` | string/int | — | Tooltip font. | |
| `CompFontName` / `CompFontSize` / `CompFontStyle` / `CompCharSet` | string/int | — | Font of the compare window. | |
| `CursorPenWidth` | int (px) | 0 | Caret width in the file lists; 0 = hairline, positive = pixels scaled with DPI. | |
| `CursorPenWidth_96`, `CursorPenWidth_<dpi>` | int (px) | 0 | Per-DPI caret width for secondary screens (Win10 Creators+). | |
| `IconsInMenus_<dpi>` | enum | 0 | Menu icon size for that DPI: 0 = 16×16, 1 = 24×24, 2 = 32×32, 3 = 48×48. | Complements `IconsInMenus`. |
| `Iconsize` | int (pt) | 10 | Icon size for the legacy 16-bit build. | Obsolete. |
| `Iconsize32` | int (px) | 16 | Icon size in the file list for 32/64-bit builds. | Dialog: Icons. |
| `Iconsize32_dpi`, `Iconsize32_<dpi>` | int | 16 | DPI stamp and per-DPI overrides of `Iconsize32` (multiple monitors). | |
| `IconWin95` | bool | 0 | Force small icons in standard size. | Obsolete. |
| `RenameTabs` | 5 ints | — | Multi-rename tool tab positions: extension, new name, size, date, location. | |
| `SyncTabs` | 6 ints | — | Synchronize-dirs tab positions: size, date, direction, date, size, right name. | |
| `Tabstops` | 6 ints | 104,109,158,58,374,93 | Column tab stops: size, date/time, attributes, extension, window width, comments, size-minus-extension. | Overridden by `[Tabstops]` when present. |
| `ThumbScaleFactor` | int (%) | 100 | Thumbnail zoom, 10–200 %, changed by Ctrl+wheel or `cm_ZoomIn`/`cm_ZoomOut`. | Dialog: Thumbnails. |
| `ThumbScaleFactor_dpi`, `ThumbScaleFactor_<number>` | int | — | DPI stamp and per-screen zoom overrides (e.g. `ThumbScaleFactor_120=50`), the latter taking precedence. | |
| `Btm*`, `CmdSel*`, `Compare*`, `Connect*`, `CustColumn*`, `DriveSel*`, `MenuChange*`, `PluginSel*`, `Print*`, `Rename*`, `Search*`, `SelectDuplicates*`, `Sync*`, `TreeDlg*` | int (px) | — | Per-dialog window coordinates/options for the background manager, command chooser, compare, connect, custom-column chooser, drive selection in search, menu-change, plugin-property chooser, print preview, rename, search, duplicate selection, synchronize and tree dialogs. | Search-drive coordinates are relative to the search dialog. |

Panel state, per side, in `[Left]` and `[Right]`:

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `path` | path | — | Last active directory of that panel. | Written with `Savepath=1`. |
| `ShowAllDetails` | enum | 1 | 0 = names only (brief), 1 = all details (full), 2 = tree. | Written with `Savepanels=1`. |
| `show` | enum | 1 | 1 = all files, 2 = programs only, 3 = user-defined filter. | |
| `SpecialView` | enum | — | 1 = comments, 2 = thumbnails, `n+2` = custom-columns view *n*. | Separate variable for backward compatibility. |
| `sortorder` | enum | 0 | 0 = name, 1 = extension, 2 = size, 3 = date, 4 = unsorted. | |
| `customsortorder` | int | 0 | Sort key in custom-columns mode: 100 = first custom column, 101 = second, … | |
| `negative Sortorder` | bool | 0 | Reverse the sort order. | Key printed with a space in the help; likely two words by mistake. |
| `AddSortOrders` | int list | — | Additional sort keys, e.g. `1,-3,2` = column 1 ascending, column 3 descending, column 2 ascending. | |
| `userspec` | pattern | *.wav | User-defined file mask for the "custom" display filter. | |
| `ViewMode` | int | -1 | Active tab's view mode; `-1` or `10001` force a re-read. | |
| `AutoViewMode` | int | -1 | The auto-switch-determined mode at the moment the user overrode it manually. | Kept until the automatic rule result changes. |
| `ActivePanelColor` | color | — | Background of the current tab's list. | Set by view modes. |
| `ActivePanelColorDark` | color | — | Same for dark mode. | |
| `ActivePanelColor2` | color | — | Secondary (odd-line) background of the current tab. | |
| `ActivePanelColorDark2` | color | — | Same for dark mode. | |

### 2.14 `[Layout]` — visible UI elements

Every key is a 0/1 switch; the section as a whole is documented as "which part of the user
interface is shown". Dialog: Configuration → Layout.

| Setting | Default | Meaning |
|---|---|---|
| `BreadCrumbBar` | 1 | Clickable path parts in the current-directory header. |
| `ButtonBar` | 1 | Main horizontal button bar. |
| `ButtonBarVertical` | 1 | Secondary vertical button bar. |
| `CmdLine` | 1 | Command line. |
| `CurDir` | 1 | Current-directory header for each panel. |
| `DarkListBorders` | 1 | List-box borders in dark mode. |
| `DirectoryTabs` | 1 | Folder tabs. |
| `DriveBar1` | 0 | Drive button bar. |
| `DriveBar2` | 0 | A second drive bar (only meaningful when `DriveBar1=1`). |
| `DriveBarFlat` | 1 | Flat drive buttons. |
| `DriveCombo` | 1 | Drive dropdown box. |
| `HistoryHotlistButtons` | 1 | The `*` (hotlist) and `▾` (history) buttons beside the path. |
| `InterfaceFlat` | 1 | Flat style for all other UI elements. |
| `KeyButtons` | 1 | Function-key buttons F3…F8 at the bottom. |
| `StatusBar` | 1 | Status bar under the file lists. |
| `TabHeader` | 1 | Sortable column header above each list. |
| `TabHeaderBoldArrow` | 1 | Bold sort arrows (defaults to `TabHeaderThemed`). |
| `TabHeaderThemed` | 1 | Themed column headers. |
| `XPthemeBg` | 1 | Themed background behind menu, button bar, drive bar and FTP toolbar. |

### 2.15 Colours — `[Colors]`, `[ColorsDark]`

Colour values are `r + 256*g + 65536*b`, `$BBGGRR` hex, or `-1` for "use default".
`[ColorsDark]` accepts the same field names for dark mode and is documented as referring to
`[Colors]` for the field descriptions. Dialog: Configuration → Colors.

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `BackColor` | color | -1 | Background of the file lists. | |
| `BackColor2` | color | -1 | Background for odd/even alternating rows. | |
| `ForeColor` | color | -1 | Text colour. | |
| `MarkColor` | color | 255 | Selection (marked-file) colour. | |
| `ActiveTabColor` | color | -1 | Colour of the active folder tab; `-1` = disabled. | |
| `CursorColor` | color | -1 | Cursor colour. | |
| `CursorText` | color | -1 | Text colour inside a solid cursor bar. | Only with `InverseCursor=1`. |
| `ActiveTitle` | color | -1 | Active panel's title bar background. | |
| `ActiveTitleText` | color | -1 | Active panel's title bar text. | |
| `InactiveTitle` | color | -1 | Inactive panel's title bar background. | |
| `InactiveTitleText` | color | -1 | Inactive panel's title bar text. | |
| `InverseCursor` | bool | 0 | Draw the cursor as a solid bar instead of a frame. | |
| `InverseSelection` | bool | 0 | Draw selected files as solid bars. | Mutually exclusive with `InverseCursor` per the dialog. |
| `ThemedCursor` | enum | -100 | 1 = use the OS theme for the cursor, 0 = do not, `-100` = automatic (only when `InverseCursor=0`). | |
| `ThumbnailFrame` | color | -1 | Frame around thumbnails; `-2` = invisible. | |
| `TreeLineColor` | color | -1 | Connecting lines in trees (panel trees, separate trees, Alt+F10 dialog, FTP connect dialog); `-2` = invisible. | |
| `TreeTriangles` | enum | -1 | Tree expanders: 0 = `[+]`, 1 = triangle when supported, `-1` = let the program decide. | |
| `ColorFilterN` | pattern | — | Search string (mask) of colour filter *N*. | Dialog: Define colors by file type. |
| `ColorFilterNColor` | color | — | Colour for filter *N*. | |
| `ColorFilterNColorDark` | color pair | — | Dark-mode colour for filter *N*, followed by the normal-mode colour, comma-separated. | Lets older versions detect malformed values. |
| `CompareText` | color | — | Text-mode difference text colour in Compare by content. | |
| `CompareTextSel` | color | — | Same, for selected text. | |
| `CompareBinary` | color | — | Binary-mode difference colour. | |
| `CompareBinarySel` | color | — | Same, for selected text. | |
| `CompareDifferenceBack` | color | -2 | Background of lines containing differences; `-1` = as normal lines, `-2` = slightly shaded. | |
| `CompareNormalBack` | color | -1 | Background of non-differing lines. | |
| `CompareNormalText` | color | -1 | Text colour of non-differing lines. | |
| `CompareSearchLine` | color | -1 | Colour of underlined search hits. | |
| `CompareSelBack` | color | -1 | Background of selected text in the compare window. | |
| `CompareSelText` | color | -1 | Text colour of selected text in the compare window. | |
| `SyncSelBack` | color | -1 | Background of selected rows in Synchronize dirs. | |
| `HintForeground` | color | — | Tooltip text colour. | |
| `HintBackground` | color | — | Tooltip background colour. | |
| `HintBorder` | color | — | Tooltip border colour. | |
| `ProgressBarBottom` | color | — | Lower half of the progress bar when a transfer speed is drawn. | |
| `ProgressBarTop` | color | — | Upper half; derived brighter from `ProgressBarBottom` when unset. | |

### 2.16 `[Packer]` — archive handling

Dialog: Configuration → Packer; the ZIP and 7-Zip pages supply the `Internal*`/`Zip*`/`7z*` keys.

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `ZIP`, `UnZIP` | path | example paths | External PKZIP / PKUNZIP executables. | |
| `ACE`, `ARJ`, `LHA`, `RAR`, `UC2` | path | example paths | External packers for those formats. | UC2 has no 64-bit support. |
| `7zip` | path | — | External 7-Zip executable (`7zG.exe`). | |
| `ARJlongnames` | bool | 0 | Pass long names to ARJ. | |
| `DefPlugin` | string | cab | Last used packer plugin. | |
| `ExpertMode` | bool | 0 | 1 = Enter runs a file inside an archive directly, 0 = show the property sheet. | |
| `ExtNeverUppercase` | bool | 0 | Never uppercase the archive extension. | |
| `ExtractPathInZIP` | bool | 1 | Restore stored paths when unpacking. | |
| `InternalUnzip` / `InternalUnarj` / `InternalUnlzh` / `InternalUnace` / `InternalUnrar` | bool | 1 | Use the built-in unpacker for that format. | Internal unpackers do not handle encrypted ACE files. |
| `InternalZIP` | bool | 1 | Use the built-in ZIP packer. | |
| `InternalZipRate` | enum | 6 | Internal ZIP compression level 0…9. | |
| `InternalzipTemp` | path | `c:\temp` | Temp directory of the internal ZIP packer. | |
| `KeepPackerPassword` | bits | 0 | Keep the archive password while 1 = minimised, 2 = screen saver active, 3 = both, +4 = forget when leaving the archive. | Works with `RememberLastPassword`. |
| `LastUsedPacker` | string | 1 | Last packer used. | |
| `LastUsedPacker64` | string | 1 | Same for the 64-bit build (plugin sets differ). | |
| `LinuxCompatible` | bool | 1 | 1 = GNU/Linux-compatible TAR (long-name format), 0 = SunOS. | Dialog: Packer → TAR. |
| `LzmaAllowed` | bool | 0 | Allow LZMA compression inside ZIP (needs WinZip ≥12 to unpack) and enable rate 10. | |
| `NoDelete` | bool | 0 | Keep extracted files whose CRC was bad. | |
| `OpenPartial` | bool | 0 | Open incomplete (e.g. partially downloaded) ZIPs. | |
| `OverrideCodepage` | int | -1 | Code page for archive names when unpacking. | |
| `PackEncrypted` | enum | 1 | When to pre-check "Encrypt": 0 = never, 1 = when the target exists and already holds encrypted files, 2 = always. | |
| `PluginOverrideZip` | bool | 0 | Let a packer plugin handle ZIP instead of the internal unpacker. | |
| `PreferUtf8ForZip` | bool | 1 | Prefer the UTF-8 reading of ambiguous ZIP names even without the UTF-8 flag. | |
| `RememberLastPassword` | bool | 0 | Reuse the last archive password for all archives until exit/minimise/sleep. | |
| `RemovableInTemp` | bool | 0 | 1 = modify ZIPs on removable media via a temp copy, 0 = only for floppies. | |
| `SkipFirstDir` | bool | 0 | Initial state of "leave out base directory when packing". | Not auto-saved. |
| `UnZIPOverwrite` | bool | 0 | Always overwrite when unpacking. | |
| `UnZIPSeparateSubdirs` | enum | -1 | "Unpack each archive to a separate subdirectory": 0/1 = off/on and remember changes, `-1`/`-2` = off/on without remembering, `-3` = as `-2` only for multiple archives. | |
| `VerifyZIP` | bool | 1 | Test the archive after packing with the internal packer. | |
| `WarnTooBig` | bool | 1 | Warn when an archive exceeds 2 GB and some packers cannot open it. | |
| `WipeEncrypted` | bool | 1 | Overwrite temp files extracted from encrypted archives with zeroes before deleting. | |
| `zip7z` | bool | 0 | The program configured in `ZIP` is actually 7-Zip. | |
| `Zip83Name` | bool | 0 | Store 8.3 names instead of long names. | |
| `ZipAES` | enum | 0 | 0 = ZIP 2.0 encryption, 1/2/3 = AES-128/192/256. | |
| `ZipAnsiNames` | bool | 0 | Store names in ANSI instead of OEM. | Some unpackers then mangle accents. |
| `ZIPDirectoryOfFile` | bool | 1 | Store path names together with the files. | |
| `ZIPlikeDirectory` | bool | 1 | Treat archives as directories. | Dialog: Packer. |
| `zipnt` | enum | 0 | 0 = no Zip-NT, 1 = patched Zip-NT 2.0.1, 2 = unrecognised version. | Legacy. |
| `ZIPRecurseSubdirs` | bool | 1 | Recurse into subdirectories when packing. | |
| `ZipSetDateToNewest` | bool | 0 | Set the archive timestamp to the newest member's timestamp. | |
| `ZipTestWarnExplorer` | bool | 0 | Warn when a produced archive cannot be opened by Windows Explorer. | |
| `ZipUnicode` | enum | 0 | Unicode name policy: 0 = ask, 1 = UTF-8, 2 = all UTF-8 if any is Unicode, 3 = all UTF-8 if any is non-English, 4 = Info-Zip extra field, 5 = extra field for all non-English, 6 = `?`. | |
| `7zInternal` | bool | 1 | Use the internal 7-Zip engine. | |
| `7zPreferPlugin` | bool | 1 | Prefer an installed 7-Zip plugin when entering an archive. | |
| `7zRate` | enum | 6 | Internal 7-Zip compression level 0…9. | |
| `7zSolid` | bool | 1 | Create solid archives. | |
| `7zThreads` | int | 2 | Thread count for the 7-Zip engine. | |
| `7ZencryptNames` | bool | 0 | Also encrypt file names in encrypted archives. | |
| `7zMethod` | enum | LZMA2 | Compression method: LZMA2, LZMA, PPMD or BZIP2. | |
| `7ZdictSize<method>_<rate>` | int | 0 | Dictionary size per method and level; >0 MiB, <0 KiB, 0 = default. | e.g. `7ZdictSizeLZMA2_6`. |
| `7ZwordSize<method>_<rate>` | int | 0 | Word size per method and level; 0 = default. | |
| `7zSolidSize<method>_<rate>` | int (MiB) | — | Solid-block chunk size per method and level. | |

### 2.17 Button bars — `[Buttonbar]`, `[ButtonbarVertical]`, `[ButtonbarCache]`

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `Buttonbar` | path | — | Default button-bar file (`.bar`). | Dialog: Change button bar. |
| `BarLimitLines` | int | 0 | Maximum number of rows of a button bar; 0 = unlimited. | |
| `Buttonheight` | int (px) | 29 | Button height. | |
| `Buttonheight96` | int (px) | 29 | Button height for a specific DPI value. | |
| `DefaultDpi` | int | — | DPI at which `Buttonheight`/`SmallIconSize` were written, used for rescaling. | |
| `DefaultLib` | string | wcmicons.dll | Icon library used for both bars and the main menu. | Dialog: Icons → Set default icon library. |
| `FlatIcons` | bool | 1 | Icons without the 3-D effect. | |
| `PopupIconSize` | int (px) | — | Icon size in the dropdown when a `.bar` is shown as a menu; defaults to the bar's icon size. | |
| `PopupOverlay` | bool | 1 | Small arrow overlay on buttons that open a submenu. | |
| `SmallIcons` | bool | 1 | Resize icons to `SmallIconSize` instead of the button size. | |
| `SmallIconSize` | int (px) | 16 | Icon size when `SmallIcons=1`. | |
| `SmallIconSize96` | int (px) | 16 | Same for a specific DPI. | |
| `XPstyle` | bool | 1 | Draw buttons with the current theme. | |
| `[ButtonbarVertical]` | `ButtonHeight`, `FlatIcons`, `SmallIcons`, `SmallIconSize`, `XPstyle`, `Buttonbar` | same types | — | Overrides of the `[Buttonbar]` keys for the vertical bar. | Documented as "see section `[Buttonbar]`". |
| `IconDll_<barfile>` | string | — | Last icon library used for a given bar file, e.g. `IconDll_default.bar`, `IconDll_vertical.bar`. | Cache section. |

### 2.18 `[Lister]` and its companions

Dialog: Configure lister (General / Multimedia / Player tabs).

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `Maximized` | bool | 1 | Lister opens full screen. | |
| `QuickMaximized` | bool | 1 | Separate quick view (Ctrl+Shift+Q) opens full screen. | |
| `x`, `y` | int (px) | 54, -1 | Lister window position. | |
| `dx`, `dy` | int (px) | 587, 483 | Lister window size. | |
| `QuickX`, `QuickY`, `QuickDx`, `QuickDy` | int (px) | — | Position/size of the separate quick view window. | |
| `TextWidth` | int (chars) | 81 | Wrap width in text mode. | |
| `binwidth` | int | 75 | Width of the binary view. | |
| `BmpStartup` | bool | 1 | Try to display images immediately on open (0 = open as binary but still allow switching). | |
| `AlbumArt` | bool | 1 | Show album cover art in the internal player. | |
| `AutoRotate` | bool | 1 | Rotate JPEGs according to their EXIF orientation. | |
| `Caret` | bool | 1 | Show the text caret so text can be selected from the keyboard. | |
| `CenterBitmap` | bool | 0 | Centre the bitmap in the window. | |
| `Codepage` | int | -1 | User-chosen code page; `-1` = derived from the current font. | |
| `Contextmenu1` (…`n`) | string | Google search template | Custom context-menu entries: `caption\|command`, `%s` replaced by the selection. | Numbered keys. |
| `FgColor`, `BgColor` | color | — | Text and background colour of the text modes. | |
| `FgSelColor`, `BgSelColor` | color | — | Colours of selected text. | |
| `MediaFgColor`, `MediaBgColor` | color | — | Colours of the player controls. | |
| `LinkColor` | color | 8388608 | Colour of links in the HTML/text view. | |
| `Editor` | path | — | Separate editor for F4 inside Lister; falls back to `[Configuration] Editor`. | |
| `F4Edit` | enum | 1 | F4 in Lister: 0 = ignore, 1 = open editor, 2 = as 1 and close Lister after viewing one file, 3 = as 2 also for multiple files. | |
| `Filter32dir`, `Filter64dir` | path | `filter32`/`filter64` in the program dir | Location of DirectShow filter DLLs. | |
| `Font1`, `Font2`, `Font3`, `Font4`, `Font5` | string | Arial/-15/b …, Courier New/18/b, Arial/20 | Five lister fonts: ANSI, ASCII, variable, fixed-width printer, variable printer. | Format `name,size,attrs[,codepage][,Wweight][,D<n>]`. |
| `GifMinDelay` | ms | 80 | Minimum delay between animated-GIF frames. | |
| `HTMLAsText` | bool | 1 | Render HTML as text with links instead of source. | |
| `IView` | bool | 1 | Use IrfanView/XnView to load images other than BMP. | Dialog: Multimedia. |
| `IviewAdditionalTypes` | pattern | — | Additional types handed to IrfanView/XnView; `\|`-separated exclusions allowed. | |
| `IViewPath` | path | — | IrfanView/XnView executable. | |
| `LinkBraces` | bool | 1 | Wrap links in `{}` in the HTML-text mode. | |
| `ModeByfileType` | bool | 1 | Enable "Define view method by file type". | `[ListerModes]` holds the rules. |
| `MediaAdditionalTypes` | pattern | — | Additional types for the internal player, with `\|` exclusions. | |
| `Multimedia` | bool | 1 | Try to play audio/video on open (0 = open as binary unless the picture mode is chosen). | |
| `NoMediaFileTypes` | pattern | — | Never load these as media/image/plugin unless chosen explicitly. | |
| `NoPluginFileTypes` | pattern | — | Never load these through a plugin. | |
| `OldDefaultFonts` | bool | 0 | 1 = use legacy non-scalable bitmap fonts. | |
| `PlayDelta0` … `PlayDelta3` | s | -60, -10, 10, 60 | Jump offsets of the four player seek buttons. | Numbered keys. |
| `PlayerVolume` | int | 100 | Last player volume 0…100. | |
| `PopupItems` | bits | 127 | Allowed context-menu entries: 1 = copy as text, 2 = copy as hex, 4 = copy URL, 8 = copy all URLs, 16 = download target (HTML), 32 = download target (text), 64 = open in browser. | |
| `PreferFiltersFromFile` | bool | 1 | Prefer the DirectShow filters in the filter directory over the registered ones. | |
| `PreloadFilterType32`, `PreloadFilterType64` | enum | 1 | Video filter preference: `-1` = old Video-for-Windows, 0 = system choice, 1 = recommended (LAV/ffdshow), 2 = user-defined lists. | |
| `PreloadFilters32`, `PreloadFilters64` | string | LAV/FFDShow defaults | CLSIDs of decoder filters to preload; `\|` = alternatives, `,` = groups. | |
| `PreloadInputFilters32`, `PreloadInputFilters64` | string | LAV splitter | CLSIDs of source filters, or a GraphEdit `.grf` file. | |
| `PreviewHandlers` | bool | 0 | Use Explorer preview handlers as a view mode. | |
| `ReplaceWrongWidthChars` | bool | 1 | Replace wrong-width characters with dots in binary/hex mode. | |
| `RTF` | bool | 1 | Auto-detect RTF files. | |
| `SavePositionLimit` | s | 600 | Save the playback position of tracks longer than this; `-1` = never. | |
| `SearchGoBack` | int | 3 | Lines of context shown above a search hit. | |
| `ShrinkBitmap` | enum | 0 | 1 = fit the image to the window (grow and shrink), 2 = shrink only. | |
| `Startup` | bits | 0 | Startup mode/font: 0 = auto, 1 = text, 2 = binary, 3 = hex; plus 0 = auto, 16 = ANSI, 32 = ASCII, 48 = variable. | |
| `StopInMainThread` | bool | 1 | Stop the player in the main thread (Windows XP only). | |
| `TabWidth` | int | 8 | Tab stop width in text mode. | |
| `ThumbnailProviders` | bool | 1 | Use Explorer thumbnail providers. | |
| `Uniscribe` | enum | 1 | 1 = use Uniscribe for Unicode/bidirectional text, 2 = always. | Win-only library. |
| `UnwrapWidth` | int (chars) | 1024 | Maximum line width without wrapping (max 16384). | |
| `UserEncodings` | string | — | User encodings as `Name (number)` separated by `\|`. | |
| `Wrap` | bool | 1 | Word wrap on/off. | |
| `ZoomFont` | enum | 1 | Font zoom with the mouse wheel: 0 = off, 1 = Ctrl+wheel, `-1` = Ctrl+Alt+wheel. | |
| `LeftMargin`, `RightMargin`, `TopMargin`, `BottomMargin` | int (0.1 mm) | 200, 0, 200, 200 | Print margins. | |

| Section | Setting | Type | Meaning |
|---|---|---|---|
| `[ListerDark]` | same field names as `[Lister]` | colour | Lister colours for dark mode. |
| `[ListerModes]` | per-extension view-method rules | string | Backing store for "Define view method by file type" (`ModeByfileType`). |
| `[LastTracks]` | `date-time` = `offset in seconds,track file name` | mixed | Playback positions remembered by the media player. |
| `[ListerPlugins]` | `0=<pluginname>` | path | Installed lister plugins, indexed; `0_detect=<detect string>` gives an optional detection string so a plugin need not be loaded to test support. |
| `[ListerPlugins64]` | index = 0/1 | bool | Which lister plugins also exist as 64-bit builds. |

### 2.19 `[Confirmation]`

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `deleteDirs` | bool | 1 | Ask before deleting a non-empty directory. | Dialog: Misc. |
| `MouseActions` | bool | 1 | Ask when files are dropped for copy/move. | |
| `MouseDelete` | bool | 1 | Ask when files are dropped on the F8 delete button. | |
| `OverwriteFiles` | bool | 1 | Ask before overwriting files. | |
| `OverwriteHidSys` | bool | 1 | Ask before overwriting/deleting hidden/system files. | |
| `OverwriteReadonly` | bool | 1 | Ask before overwriting/deleting read-only files. | |
| `OverwriteHidSysNotReadonly` | bool | 0 | Additionally ask for hidden/system files without the read-only attribute (standard copy method only). | |
| `SyncConfirmOverwrite` | bool | 1 | Overwrite confirmation inside Synchronize dirs. | |

### 2.20 Custom columns and the custom-field sub-sections

`[CustomFields]` stores up to 29 custom-columns views; `<n>` in the keys is the 0-based view index.

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `Titles` | string | — | View names separated by `\|`. | Dialog: Custom columns. |
| `Headersx` | string | — | Column headers of the view with index *x*, separated by `\n`. | The help writes the index as a literal `x` suffix. |
| `Contentsx` | string | — | Column field expressions of the view with index *x*, separated by `\n` (plugin fields use `[=plugin.field]` syntax). | |
| `Widthsx` | int list | — | Column widths; negative = right-aligned. | |
| `Optionsx` | string | — | `<window width>` pipe `<horizontal scrollbar 1/0>` pipe `<dpi at which the widths were stored>`. | |
| `AutoLoad` | bool | 1 | (`[CustomFields_<PluginName>]`) load this plugin's saved view automatically when entering its path. | |
| `Contentsx` | string | — | (`[CustomFields_<PluginName>]`) as above, but `[=<fs>.<field>]` addresses the file-system plugin itself. | |

| Section | Contents |
|---|---|
| `[HintsCustomField]` | Custom fields shown in file tooltips. |
| `[CompareCustomField]` | Custom fields used as text for comparisons in Synchronize dirs. |
| `[OverwriteCustomField]` | Custom fields shown in overwrite dialogs. |
| `[ThumbsCustomField]` | Custom fields shown below thumbnails (current form of `ThumbsCustomField=`). |
| `[SearchCustomField]` | Custom fields for the plugin page of Find files: `xexts` names the matching file masks, `xfields` the fields to display when the mask matches. |
| `[TargetList]` | Recent target directories for the F5 copy dialog combo box. |
| `[Selection]` | Last wildcards used for select/deselect by mask (Num `+`/`-`). |
| `[SplitPerFile]` | User-defined part sizes used by Files → Split and multi-volume archives. |
| `[DirSelection]` | Last patterns of "select duplicate files" → "select by folder". |
| `[PrintDir]` | Auto-saved print-dialog settings. |
| `[PluginDuplicates]` | History of "same plugin fields" in the search dialog's plugin page. |
| `[MkDirHistory]` | Last directories created with F7. |
| `[NewFileHistory]` | Last files created with Shift+F4. |

### 2.21 View modes — `[ViewModes]`, `[ViewModeSwitch]`

| Section | Setting | Type | Meaning | Notes |
|---|---|---|---|---|
| `[ViewModes]` | `x_name` | string | Name of view mode *x*. | Dialog: View Mode. |
| `[ViewModes]` | `x_icon` | path | Icon shown for the mode on folder tabs. | |
| `[ViewModes]` | `x_options` | packed string | View, sort order, reversed flag, additional sort orders, tab colour, background, background 2. | Also stored per tab in `[LeftTabs]`/`[RightTabs]`. |
| `[ViewModes]` | `x_commands` | string | Commands to auto-run when the mode is entered; `-` prefix = only on directory changes, `*` prefix = on every change, commands after a pipe symbol run when leaving the mode. | Supports `cm_*` and `em_*`. |
| `[ViewModeSwitch]` | `Enabled` | bool (1) | Master switch for automatic mode switching. | Dialog: Auto Switch Mode. |
| `[ViewModeSwitch]` | `x_rules` | string | Sub-rules combined with `&` (AND) and `\|` (OR); rule prefixes `+`, `-`, `%`, `?` select all / none / at-least-half / at-least-one file matching, and a folder-type letter selects D/L/U/V/F/A/P/S. | AND binds tighter than OR. |
| `[ViewModeSwitch]` | `x_mode` | string | Mode to apply when rule *x* matches. | Rules are evaluated in order. |

### 2.22 Plugins — installer registries

| Section | Form | Meaning |
|---|---|---|
| `[PackerPlugins]` | `bz2=27,c:\totalcmd\bzip2dll.dll` (`<ext>=<capabilities>,<path>`) | Installed packer plugins; the number is a bit mask of capabilities: 1 = create, 2 = modify, 4 = multi-file archives, 8 = delete inside archive, 16 = options dialog, 32 = pack in memory, 64 = detect by content, 128 = text search inside archives, 256 = open only with Ctrl+PgDn, 512 = encryption. |
| `[FileSystemPlugins]` | `<pluginname>=<path>` | Installed file-system plugins, addressed by name. |
| `[ContentPlugins]` | indexed entries | Installed content (field-provider) plugins. |
| `[ListerPlugins]` | `0=<name>`, `0_detect=<detect string>` | Lister plugins in call order plus optional detection strings. |
| `[PackerPlugins64]` | `$checksum$`, `exts` | Index of packer plugins that also have a 64-bit build; `$checksum$` is an Adler-32 of the corresponding 32-bit section and `exts` lists the extensions that have a 64-bit version. |
| `[ContentPlugins64]`, `[ListerPlugins64]` | `0=1`, … | Index flags marking which plugins have 64-bit builds. |
| `[FileSystemPlugins64]` | `<name>=1` | File-system plugins that have 64-bit builds. |
| `[ReplaceIniLocation]` | `pluginname.ext` = `<path or ini>` | Redirects the ini file handed to one plugin; `*` = send the default but fall back to the user profile when unwritable, `-` = send the default and never fall back. |

### 2.23 Saved dialog state, tabs and histories

| Section | Setting | Type | Meaning | Notes |
|---|---|---|---|---|
| `[LeftTabs]`, `[RightTabs]` | `0_path`, `1_path`, … | path | Directory of the tab with that 0-based index. | The *active* tab's own state lives in `[Left]`/`[Right]`, not here. |
| `[LeftTabs]`, `[RightTabs]` | `0_caption`, … | string | Tab caption when the user renamed it. | |
| `[LeftTabs]`, `[RightTabs]` | `0_filter`, … | pattern | Per-tab display filter, used when `DirTabFilters=1`. | |
| `[LeftTabs]`, `[RightTabs]` | `0_options`, … | pipe-packed string | Per-tab options in order: view mode, sort order, sort direction, packer start-of-archive-name, packer archive type, locked flag, custom-view sort order, additional sort orders, custom colours (header/background/background-2 plus the two dark-mode backgrounds), view mode, auto-switch view mode. | |
| `[LeftTabs]`, `[RightTabs]` | `activetab` | int | Index of the active tab — documented as *not* stored here but in `[Left]`/`[Right]`. | |
| `[LeftTabs]`, `[RightTabs]` | `activelocked` | enum | 1 = the active tab is locked, 2 = locked with directory changes allowed. | |
| `[LeftTabs]`, `[RightTabs]` | `activecaption` | string | Caption of the active tab when renamed. | |
| `[LeftHistory]`, `[RightHistory]` | numbered lines | path | Visited directories per panel, written only when `SaveHistory=1`. | Deleting the section requires "Delete now" in the History page. |
| `[MkDirHistory]` | numbered lines | string | Last directories created with F7. | |
| `[NewFileHistory]` | numbered lines | string | Last files created with Shift+F4. | |
| `[Tabstops]` | `AdjustWidth` | bool | 1 = widen the name column when the window grows, keeping the other columns right-aligned. | Only used when the resolution section holds no tab stops. |
| `[SyncOptions]` | `<name>_L`, `<name>_R` | path | Left/right path of the saved synchronize configuration `<name>`. | Dialog: Synchronize dirs. |
| `[SyncOptions]` | `<name>_Wcs` | pattern | Wildcard filter of that configuration. | |
| `[SyncOptions]` | `<name>_Flags` | `\|`-packed string | Four blocks: compare options (1 subdirs, 2 by content, 4 ignore dates, 8 asymmetric, 16 empty dirs), display options (1 left→right, 2 equal, 4 not equal, 8 right→left, 16 duplicates, 32 singles), sort order (negative = reversed), how regular expressions/saved searches apply (1…4, negative = hide matching files). | |
| `[Searches]` | named entries | string | Saved search-dialog option sets. | |
| `[SearchName]`, `[SearchIn]`, `[SearchText]` | numbered lines | string | Previous search names, start directories and search texts. | The help calls these "three sections contain previous searches". |
| `[rename]` | named/mixed | string | Saved options of the multi-rename tool. | Paired with `MultiRenameLoadSettings`. |
| `[RenameTemplates]` | numbered lines | string | Last-used rename mask strings. | |
| `[RenameSearchFind]`, `[RenameSearchReplace]` | numbered lines | string | Last-used find/replace strings of the multi-rename search&replace. | |
| `[PrintDir]` | mixed | string | Auto-saved print-dialog settings. | |

### 2.24 `wcx_ftp.ini` — FTP client settings

The FTP file mirrors the same design: a `[General]` block of global options, a `[default]` block
that seeds new connections, per-site sections, and helper sections. Dialog: Configuration → FTP.

`[General]`

| Setting | Type | Default | Meaning | Notes |
|---|---|---|---|---|
| `AESVerify` | string | — | Verifies that the master password entered for the password manager is correct. | Integrity check value. |
| `Allowhook` | enum | 2 | Keyboard hook in the master-password dialog: 0 = disallow (show a warning icon), 1 = allow and show an indicator, 2 = allow without indicator. | The hook lets the program swallow key presses so naive keyloggers cannot read them. |
| `Ascii` | pattern | `*.txt *.htm*` | Extensions treated as text files. | Drives automatic transfer mode. |
| `AutoDisconnect` | enum | 0 | Connecting elsewhere in the same window while connected: 0 = ask, 1 = disconnect the previous session, 2 = keep it. | |
| `AutoResume` | s | 30 | Timeout before trying to restore a hung connection and resume the transfer; `-1` = off. | |
| `ConnectRetries` | int | 10 | Retries for anonymous connections. | Dialog: FTP. |
| `DecodePercent` | bool | 0 | Decode `%hex` sequences in URL user names and passwords. | |
| `DataConnectTimeout` | s | 10 | Timeout for the data connection, maximum 300 s. | |
| `E-Mail` | string | — | Password sent for anonymous logins. | |
| `ForceHTTP11` | bool | 0 | 1 = HTTP/1.1 for all transfers, 0 = HTTP/1.0 except resumed downloads. | HTTP/1.1 can lose the total size (chunked) and thus the progress bar. |
| `FtpInBackground` | bool | 0 | Always start FTP transfers in the background. | |
| `FtpOpenIn` | enum | 0 | Where new connections open: 0 = active panel, 1 = left, 2 = right, 3 = inactive panel. | Dialog: FTP. |
| `FtpShowHidden` | bool | 0 | "Show hidden files" pre-checked at startup. | |
| `GetModeFromLocalFile` | bool | 0 | Choose binary/text from the *local* file's extension instead of the remote one. | |
| `Httptimeout` | s | 30 | HTTP response timeout. | |
| `KeepAliveTransfer` | bool | 0 | Send keep-alives on the control connection during transfers. | Some servers dislike it. |
| `KeepMasterPassword` | bits | 0 | Keep the master password in memory while 1 = minimised, 2 = screen saver active, 3 = both. | |
| `LogFile`, `LogFile2` | path | — | Full FTP log and a reduced log of transferred files. | |
| `MDTM` | bits | 3 | Set remote timestamps with MDTM (synchronize only): 1 = enabled, 2 = send Universal Time. | |
| `MLSD` | enum | -1 | `-1` = MLSD when advertised, 0 = always LIST, 1 = always MLSD. | Can be overridden per site. |
| `ModeZ` | enum | — | Compressed transfers: 0 = off, 1 = internet only, 2 = also LAN. | |
| `PortFrom`, `PortTo` | int | — | Port range for active mode. | |
| `PortIP` | string | — | External IPv4 address (literal, DNS name, or an `http://` URL returning the address). | |
| `PortIPEnabled` | bool | 0 | Enable `PortIP`. | |
| `PreferIPv6` | enum | -1 | `-1` = IPv4 first, 0 = IPv4, 1 = IPv6 when both exist, 2 = whatever order DNS returns, `-2` = IPv4 only. | |
| `PreferOpenSSL` | enum | 1 | 1 = OpenSSL when installed else the OS TLS stack, 0 = OS stack, `-1` = OpenSSL only if present. | |
| `PreventFEAT` | bool | 0 | Do not send the FEAT command (some proxies hang). | |
| `ReturnToReUploadDir` | bool | 0 | After re-uploading an edited file, return to the directory it came from. | |
| `SaveNewConnection` | bool | 1 | Ask whether a new Ctrl+N connection should be saved on disconnect. | |
| `SpecialFlags` | bits | 0 | 1 = force implicit SSL, 2 = force explicit SSL, 4 = force TLS auth, 8 = force SSLv23 auth, 16 = verbose background log, 32 = skip the SIZE check after upload, 64 = skip size verification after download, 128 = do not reuse SSL sessions for data connections, 256 = do not send "send commands" in the background, 512 = request hidden files with `MLSD -la`, 1024 = do not rewrite PASV addresses, 2048 = duplicate characters with code 255, 4096 = MDTM for remote timestamps, 8192 = MFMT, 12288 = SITE UTIME. | |
| `sslwarn` | bool | 1 | Warn about invalid SSL certificates (the red padlock is still shown). | |
| `SyncAllowChangeLocalTime` | bool | 1 | Allow changing local timestamps when the server cannot set remote timestamps. | |
| `TcpKeepAlive` | bool | 0 | TCP keep-alive on all connections. | |
| `TransferMode` | char | I | Startup transfer mode: A = text, I = binary, X = automatic. | |
| `UploadBlockSize` | int (KiB) | 1 | Upload buffer; 1 = automatic after the first 128 KiB. | |
| `UserAgent` | string | Mozilla/4.0 (compatible; Totalcmd; Windows x) | HTTP user agent. | |
| `VerifyCerts` | enum | 1 | 1 = always verify, 0 = never, `-1` = only when `wincmd.pem` exists, `-2` = as `-1` without the OS certificate store. | |
| `WaitDelay` | s | 5 | Wait between connection retries. | |
| `WarnWrongMode` | bool | 1 | Warn when binaries are transferred in text mode. | |
| `ZlibRatioBin`, `ZlibRatioTxt` | enum 0…9 | 3, 3 | MODE Z compression levels for binary and text uploads. | Help notes one firewall aborts level 1. |

| Section | Setting | Meaning |
|---|---|---|
| `[CertAlias]` | `server.com=hostcompany.com` | Aliases the user accepted by clicking the open-padlock icon for an FTPS certificate. |
| `[default]` | `pasvmode`, `PreserveDates`, `PreserveDatesUpload`, `LowercaseUpload`, `ViaLocalPC` | Defaults offered for new connections: passive mode, keep timestamps on download and on upload, lowercase uploaded names, route server-to-server copies through the local PC. |
| `[firewall]` | `host`, `username`, `password`, `Method`, `default`, `httphost`, `httpuser`, `httppass` | Proxy/firewall address, credentials, method index (order as in the firewall dialog), whether new connections use it by default, and separate HTTP proxy credentials. `default` is rewritten whenever a connection is saved. |
| `[connections]` | `1=Site1`, `2=Site2`, …, `default=<name>` | Ordered list of configured connections shown in the connect dialog, plus the name (not number) of the last used connection. |
| `[OldConnections]` | numbered URLs / `user@host` | Previously used targets in the "new connection" dialog. |
| `[Site1]` (per-site) | `host`, `username`, `password`, `acct`, `anonymous`, `directory`, `localdir`, `pasvmode`, `firewall`, `Encoding`, `hosttype`, `InitCommands`, `keepalive`, `MLSD`, `CacheDirs`, `ClientCert`, `SpecialFlags` | Everything for one connection: `host:port`, credentials (the password is merely scrambled, not encrypted), account password, anonymous flag, initial remote and local directories, passive mode, firewall use, name code page (`-1` = auto-detect, otherwise a code page number such as 65001), custom host type, commands sent after login, `keepalive=<seconds>,<command>`, listing command, directory-cache toggle, client certificate serial plus SHA1 prefix, and per-site overrides of `[General] SpecialFlags`. |
| `[ServerType1]` | `Template1`, `Template2` | User-defined server types. |
| `[TimeZone]` | `Site1=<offset>` | Per-site time-zone difference used by synchronize. |
| `[\|empty_folders\|]` | folder list | Empty-folder names remembered in the FTP connection dialog. |
| `[\|FSPlugin\|PluginName]` | encrypted passwords | Stored passwords of one file-system plugin, so the master password can be changed later. |

---

## 3. Colour, font and icon configuration

### 3.1 Two colour schemes: normal and dark

Dark mode arrived in 9.50 and is a **parallel set of colours**, not a filter over the normal
scheme. The switch is `DarkMode` (0 = off, 1 = follow the Windows "default app mode" setting,
2 = on), overridable per start with `/M=0|1|2`, also reachable as the menu item
`Configuration → Dark<->Normal`, whose effect is permanent unless dark mode follows the system.
Consequences documented in the help:

- `[ColorsDark]` repeats the `[Colors]` field names for dark mode; `[ListerDark]` likewise repeats
  the `[Lister]` colour fields.
- Colour filters carry a second value: `ColorFilterNColorDark` holds the dark colour first and the
  normal colour second, comma-separated, so a mismatch caused by editing with an older version is
  detectable. The colours-by-file-type dialog offers a `Dark<>Normal` toggle that *converts*
  normal-mode colours into suitable dark ones, which can then be edited separately.
- Panel background overrides exist twice per tab: `ActivePanelColor`/`ActivePanelColor2` and
  `ActivePanelColorDark`/`ActivePanelColorDark2`; view modes store the same pairs.
- Cursor and selection highlight have dark variants: `InactiveFocusDark`, `SelectionFocusDark`
  (each falls back to its normal-mode key when unset), plus `HintForeground`, `HintBackground`,
  `HintBorder`, `ProgressBarTop`, `ProgressBarBottom` and `DarkListBorders` for chrome.
- `ForceDarkModeOptions` exists to force the dark-mode workarounds on systems that do not offer
  dark mode; the help states dark mode is only *native* from Windows 10 1809 on, and simulated on
  older systems. Some colours (most context menus, standard Windows dialogs) only take effect
  after a restart.

### 3.2 File-list colours and the "others" group

The Colors page exposes text colour, background, a second background for alternating rows, mark
(selection) colour, cursor colour, cursor text colour, plus `InverseCursor` (solid cursor bar),
`InverseSelection` (solid selection bars — the dialog notes the two are mutually exclusive),
`ThemedCursor` and `CursorPenWidth`. An "Others" sub-tree covers Lister text and
selection colours and its link colour, the compare window (difference colours for text and binary
mode, with and without selection, plus normal/selected backgrounds and the search-hit underline),
the two panel title bars (active and inactive, text and background), the selected-row background in
Synchronize dirs, tooltip text/background/border, and the dark-mode highlight, border and
horizontal-line colours.

### 3.3 Colouring by file type

Enabled by `ColorFilters` (`cm_SwitchColorsByFileType` toggles it; the dialog is reached from
Colors → "Define colors by file type"). The model is an ordered list of filters, each a mask or an
attribute-based search expression using the same grammar as the Num `+` selection dialog, mapped to
one colour per mode:

- **Order is priority**: the first matching filter in the list wins. The help's example is a ZIP
  colour versus a "read only" colour — a read-only ZIP gets the colour of whichever filter is
  earlier in the list. Entries are added, edited, deleted and drag-reordered.
- **Quick enabling**: an entry whose mask starts with `*` can be clicked to neutralize it; the `*`
  becomes `-` (the filter text changes to `-.ext`, which still matches a file literally named
  `-.ext`), so filters can be parked without deleting them.
- Filters are stored in the same section as the rest of the colours as `ColorFilter<n>` /
  `ColorFilter<n>Color` / `ColorFilter<n>ColorDark`.
- Per-entry colours are independent of the panel and tab background overrides described in 3.5;
  the help recommends keeping alternate backgrounds only slightly different from the main one.

### 3.4 Fonts

Fonts are configured per role, and the file-list role is stored **per screen resolution** (or in
`[AllResolutions]` when `ResolutionSpecific=0`):

| Role | Keys | Notes |
|---|---|---|
| File list | `Fontname`, `Fontsize`, `FontWeight`, `FontCharset` | Default changed over versions: Segoe UI Semibold (Vista+), Microsoft Sans Serif (older NT), MS Sans Serif (9x). Only normal and bold are supported — italic is explicitly not. Changing the font resizes the window, capped at screen size. |
| Main window | `FontNameWindow`, `FontSizeWindow`, `FontWeightWindow`, `FontCharsetWindow` | Frame of the application, not the list. |
| Dialogs | `FontNameDialog`, `FontSizeDialog`, `FontWeightDialog`, `FontCharsetDialog` | Dialog *size* is not adjusted to the font unless `AutoSizeDialogs=1`; the help recommends 8–10 pt. |
| Tooltips | `FontNameTip`, `FontSizeTip`, `FontWeightTip`, `FontCharsetTip` | Tooltip font. |
| Compare window | `CompFontName`, `CompFontSize`, `CompFontStyle`, `CompCharSet` | Compare by content. |
| Directory history | `HistoryFont` | `0` = file-list font, `1` = menu font, otherwise a font name. |
| Lister | `Font1`…`Font5` | ANSI, ASCII, variable, fixed printer, variable printer; a compact `name,size,attrs[,codepage][,Wweight][,D<dpi>]` string. `OldDefaultFonts=1` selects legacy bitmap fonts. |
| Dialog scaling | `OverrideDPI`, `OverrideDPI96`, `OverrideDPI120`, … | DPI-based scaling that affects dialogs and their font but *not* the file list or main-window font. |

Related keys: `FontQuality`/`FontQualityMenu` (rendering mode override, including forcing ClearType),
`FontSampleText` (preview text), `UseNewDefFont` (which default family fresh vs upgraded installs
get), `AlignNumberFix` (numeric-column alignment with fonts whose digits are not monospaced).

### 3.5 Icons

| Aspect | Keys | Behaviour |
|---|---|---|
| Presence and level | `Showicons` (0 = none, 1 = default, 2 = all), `ShellExtIcons`, `ShowEXEandLNKicons` (0/1/2 = not on floppies), `IconsOnNet`, `IconsOnNetReadOnly`, `IconsInThread`, `IconFromIcoFile` | Per-file icon extraction is expensive, so separate switches allow it everywhere, only on local drives, or in a background thread. |
| Size and spacing | `Iconsize32` (+`Iconsize32_dpi`, `Iconsize32_<dpi>`), `MinIconWidth`, `MinLineHeight` | Size is stored per resolution; minimum width/height can be absolute (`>0`) or relative (`<0`). |
| Icon source | `IconLib` / `IconLib.bak`, `OldListIcons`, `DefaultLib`, `DriveLib` / `DriveLib.bak`, `OldDriveIcons` | A user icon library overrides the OS icon source; the `.bak` keys preserve the user's value when the Explorer source is enabled instead. `DefaultLib` names the library that substitutes for `wcmicons.dll` references inside button bars and menus. |
| Overlays | `IconOverlays`, `IconOverlaysOneDrive`, `IconOverlaysUsb`, `ShowHiddenSystemOverlay`, `ShowHiddenDimmed`, `TestIfSharedDir` | Shell overlays, hidden/system markers, dimmed hidden files, and the hand overlay for shared folders. |
| Special folders | `IconsSpecialFolders` (1 = local FS, 2 = virtual folders/network, 3 = both) | Which folders get non-generic icons. |
| Menus | `IconsInMenus` (frame, 24/32/48 px, pressed-style toggles), `IconsInMenus_<dpi>` | Menu icon rendering and per-DPI size class. |
| Tabs | `DirTabIcons` | Bit mask per tab-icon source (drives, desktop.ini folders, virtual folders, plugins, FTP, view modes). |
| Drive bars | `ExtraDriveWidth`/`ExtraDriveHeight` (+ `_dpi`, `_<dpi>`), `DriveBarFlat`, `HideRemovableNoMedia` | Spacing and visibility of drive buttons. |

### 3.6 Custom columns and view modes

**Custom columns** (`[CustomFields]`): up to **29** named views, reorderable by drag&drop, each made
of parallel `\|`/`\n`-separated lists of headers, field expressions and widths, plus a packed
`Options<n>` holding the auto-sizing window width, a horizontal-scrollbar flag and the DPI the
widths were stored at. Views can also be defined **per file-system plugin**
(`[CustomFields_<PluginName>]`, with `[=<fs>.<field>]` addressing the plugin's own fields) and can
be marked `AutoLoad=1` so the view activates automatically on entering that plugin's path. Custom
fields are also reused outside the list: file tooltips, compare-as-text, overwrite dialogs,
thumbnail captions and the search dialog's plugin page each have their own section.

**View modes** (`[ViewModes]`, `[ViewModeSwitch]`): a view mode bundles a columns view (built-in
"Full"/"Brief" or one of the custom-column views), a sort order, the reversed flag, additional sort
orders, a tab colour and icon, a colour priority flag that beats desktop.ini/drive icons, two
background colours (normal and odd rows, separately for normal and dark mode) and a list of
commands to auto-run. Command prefixes: `-` = only on directory changes within a tab (not on tab
switches), `*` = on every directory change, commands after `|` run when the mode is *left*. The
help's example expands the active panel to full width on entering a thumbnails mode and restores
50:50 on leaving it.

**Auto Switch Mode** (`[ViewModeSwitch]`): an ordered rule list; each rule combines sub-rules with
`&` (AND) and `|` (OR) where AND binds tighter. A sub-rule consists of a folder-type letter —
`D` plain directory, `L` directory on a drive of given types (1 removable, 2 USB stick, 3 fixed,
4 remote, 5 CD-ROM, 6 RAM disk), `U` UNC path, `V` virtual folder, `F` FTP, `A` archive, `P`
file-system plugin, `S` search result — a directory pattern (trailing `*` = include subdirectories)
and a mask rule: `+` all files must match, `-` must not match, `%` at least half must match, `?` at
least one must match. The first matching rule sets its mode; `<default>` applies when nothing
matches. `[Left]/[Right] AutoViewMode` records the auto-chosen mode so a manual override can be
kept until the automatic result changes.

### 3.7 Thumbnails

Thumbnails are cached in a database whose location is `ThumbsLocation` (default
`%LOCALAPPDATA%\GHISLER`); the help warns that the database must not be shared between users because
only one process can open it. Extraction is an **ordered method list**
(`ThumbOptions` bits + one mask key per method):

| Method | Bit | Mask key | Notes |
|---|---|---|---|
| EXIF thumbnail embedded in the image | 16 cleared | — | Fastest; also reads RAW formats listed in the help (Canon CRW/CR2/CR3, Fuji RAF, Nikon NEF, Olympus ORF, Panasonic RW2, Pentax PEF, Sony ARW, DNG). Bit 16 *disables* it. |
| Lister plugins | 2 | `ThumbPlgTypes` | Needs plugins with the thumbnail function. |
| Shell / OLE2 | 1 | `ThumbExplTypes` | The only method that works in virtual folders such as MTP phones; HTML is excluded by default. |
| IrfanView / XnView | 4 | `ThumbIrfXnTypes` | Path configured in the Lister options. |
| Plain text | 8 | `ThumbTxtTypes` | Shows the first lines of a text file. |
| Internal GDI+ | — | `ThumbGdiplusTypes` | Built-in picture decode. |

Each mask accepts the search syntax `include | exclude`. Sizing: `ThumbIconSize` (icon overlay),
`ThumbWidthNoCache`/`ThumbHeightNoCache` (when no cache is used), `ThumbnailSizeLimit` (memory
budget in MB, default 20 MB on Win9x / 500 MB on NT-family, clamped to 1–1024 MB and ≤10 % of RAM),
`ThumbnailMemoryLimit` (legacy per-panel count), `ThumbnailPrealloc` (bytes pre-allocated so the
compound-file database does not fragment), plus `ThumbScaleFactor`/`ThumbScaleFactor_<dpi>` zoom
between 10 % and 200 % in 10 % steps (Ctrl+wheel, `cm_ZoomIn`, `cm_ZoomOut`). Behaviour keys:
`PreloadThumbnails`, `ThumbsCopyDel` (keep the cache in sync on copy/delete),
`ThumbsCustomFieldsEnabled` + `[ThumbsCustomField]` (caption lines below the thumbnail),
`ThumbsInDlg` (thumbnails in overwrite dialogs). Maintenance from the same page: compact the
database, drop entries for missing files, for un-inserted CDs and for inaccessible disks — each a
separate action because deleting entries for temporarily unreachable media would be destructive.

---

## 4. Historical and session state

The configuration file is also the session store. Everything below is documented as written by the
program (subject to the `Save*` switches) rather than entered by the user.

### 4.1 What is persisted, and when

| Group | Where | Written when |
|---|---|---|
| Window geometry, splitter positions, panel fonts/icon sizes, tab stops, thumbnail zoom | `[<resolution>]` or `[AllResolutions]`, plus `monitor(x,y,dx,dy;dpi)` keys per monitor | On `Configuration → Save position`, and on exit; re-applied according to `RestoreSize` on display change, DPI change and window move. |
| Panel paths | `[Left]/[Right] path` | On exit, with `Savepath=1`. |
| Panel display type, sort order, filter, comments/thumbnails/custom-columns selection, active tab's view mode | `[Left]/[Right] ShowAllDetails`, `show`, `sortorder`, `customsortorder`, `negative Sortorder`, `AddSortOrders`, `userspec`, `SpecialView`, `ViewMode`, `AutoViewMode` | On exit, with `Savepanels=1`. |
| Folder tabs per side | `[LeftTabs]/[RightTabs] <n>_path`, `<n>_caption`, `<n>_filter`, `<n>_options`, and `activelocked`/`activecaption` | Together with `Savepath`; `activelocked`/`activecaption` describe the active tab while its index is mirrored in `[Left]/[Right]`. |
| Directory history | `[LeftHistory]`, `[RightHistory]` (numbered entries), bounds from `HistoryLen`/`HistoryLenStored` | On exit, with `SaveHistory=1`; also "Save now"/"Delete now" actions in the History page, which exist precisely because disabling the switch does *not* clear the data. |
| Most-frequently-used directories | `tcDirFrq.txt` (location from `DirUsageLocation`), sizes bounded by `FrequentlyUsedLen`/`FrequentlyUsedLenStored` | On exit with `SaveFrequentlyUsed` (default = `SaveHistory`); separate save/delete actions. |
| Command line history | `[Command line history]`, length `OldCmdLineCount` | On exit, with `SaveCommands=1`. |
| Created directories / files | `[MkDirHistory]`, `[NewFileHistory]` | Continuously. |
| Recent F5 targets | `[TargetList]` | Continuously. |
| Last search options and saved searches | `[Configuration] LastSearchOptions`, `[Searches]`, `[SearchName]`, `[SearchIn]`, `[SearchText]`, `[PluginDuplicates]` | Continuously; `SearchKeepLast` decides whether the last result list survives closing the dialog. |
| Synchronize configurations | `[SyncOptions] <name>_L/_R/_Wcs/_Flags`, `SyncFlags`, `SyncButtons`, `SyncMatch` | Continuously. |
| Multi-rename state | `[rename]`, `[RenameTemplates]`, `[RenameSearchFind]`, `[RenameSearchReplace]`, `MultiRenameUndoHistoryLen` | Continuously; `MultiRenameLoadSettings` decides whether the tool restarts with the last settings. |
| Split sizes, selection masks, duplicate-selection patterns, print dialog | `[SplitPerFile]`, `[Selection]`, `[DirSelection]`, `[PrintDir]` | Continuously. |
| Recent directories in the directory hotlist editor / attribute-dialog plugin options | `DirMenuLast`, `[attrplugin]` | Continuously. |
| Button-bar icon library per bar file | `[ButtonbarCache] IconDll_<barfile>` | When a bar is loaded. |
| Tree files | `treeinfo.wc` (per drive, location from `TreeFileLocation`/`TreeFileLocationE`/`TreeFileLocation__server_share`, size guard `TreeUpdateLimit`) | During file operations, when the tree is small enough. |
| Thumbnail database | `ThumbsLocation` | On demand; kept in sync by `ThumbsCopyDel`. |
| FTP connection list, last connection, recent targets, per-site state, empty-folder memory, saved plugin passwords, tracks | `wcx_ftp.ini` `[connections] default=`, `[OldConnections]`, `[Site1]…`, `[\|empty_folders\|]`, `[\|FSPlugin\|<name>]`, `[CertAlias]`, `[TimeZone]`; media positions in `wincmd.ini [LastTracks]` | Continuously / on exit per the dialog switches. |

### 4.2 Documented "remembered while running only" state

Several dialogs remember their options only for the session unless the corresponding key says
otherwise. The help marks these explicitly, and a reimplementation should keep the distinction:

- `UnZIPSeparateSubdirs` values `-1`/`-2` mean "state applies but is not remembered"; `-3` is the
  same for the multi-archive case only.
- `SkipFirstDir` is documented as *not* auto-saved but remembered while the program runs.
- `MultiRenameLoadSettings` empty means "start with the last used settings, which are not
  remembered after closing".
- `QuickSearchAutoFilter` `-1`/`2` mean "state is not persisted" (filter always off / always on at
  the next start).
- `RememberLastPassword` and `KeepMasterPassword` keep secrets only until the program closes,
  minimises, the screen saver starts, or the system sleeps, with `KeepPackerPassword`'s bit 4
  clearing the archive password when leaving the archive.
- `RelativePathCopy` bit 2 remembers the F5 "keep relative paths" choice for the session, bit 4 for
  the session *and* across restarts.
- `SearchKeepLast=1` keeps the last result list until the dialog is closed except after "feed to
  listbox".
- `ShowWarningIcons`/`WarnWrongMode`/`AutoReUpload=1` are one-shot "did the user acknowledge this"
  states stored in the ini.

### 4.3 Elements deliberately *not* stored per session

- The active panel is stored (`ActiveRight`) but can be overridden at start with `/P=L|R`.
- Panels that failed to read keep showing `<Read error!>` only when `IgnoreDirErrors` says so; the
  help ties this to locked tabs at startup.
- `NoReread`/`NoReread2` distinguish "do not refresh while running" from "do not restore on next
  start", which is exactly the split between session state and persisted state.

---

## 5. Platform notes

### 5.1 Windows-only settings (documented as such, or bound to a Windows mechanism)

- **Elevation and OS integrity**: `AdminTimeout`, `AlwaysAsAdmin`, `CopyDirsAsAdmin`,
  `InheritCaseSensitiveDir` (needs the admin helper for some cases), `XPMoveMethodAdmin`,
  `X64WaitTime`/`X64menu`/`x64DisableRedirection`, `RestrictInterface` bit 2048/4096 (signed
  plugins), `MftAllowed`.
- **NTFS / Windows file system semantics**: `CopyStreams`, `WantZoneData`, `CopyLinks` (junctions
  and reparse points), `IgnoreLinks`, `SyncIgnoreJunctions`, `CopySparseFiles`, `OleComments` (NTFS
  streams, Office documents), `BitlockerAlwaysDecrypt`, `VistaDelete`, `Win95Delete`, `UseTrash`,
  `1hourdif` and `NewTimeZoneMethod` (FAT vs NTFS daylight-saving discrepancy), `ShortName`-era keys
  such as `Small83names`/`UseLongNames`/`LongNameCopy`/`LongNamesToDos`/`Zip83Name`.
- **Shell integration**: `AssocQueryString`, `AssociationMultiVerbs`, `DropExtensions`,
  `UseFastCtrlC`, `ExecUseContextMenu`, `ShellExtIcons`, `TestIfSharedDir`, `NetHood*`,
  `AdminShares`, `AllHiddenNetNames`, `PromptForUser`, `UseShellConnect`, `tsclient`,
  `HideRemovableNoMedia`, `ShowExtraDrives` (My Computer / My Documents),
  `ReplacePhotoApp`/`PhotoAppMode`/`PhotoAppFilter`, `Everything*`, `UseWindowsHello`,
  `IconOverlaysOneDrive`.
- **Win32 UI/DPI plumbing**: `DpiPerScreen`, `OverrideDPI*`, `AutoSizeDialogs`, `FontQuality*`,
  `WantXPLook`, `XPthemeBg`, `ThemedCursor`, `ForceDarkModeOptions`, `AltTabWindow`, `TrayIcon`,
  `MediaRemote`, `KeyboardID`, `StartupScreen`, `TranslateMessageButtons`, `DeviceChange`,
  `LoadDriveIconsInBackground`, `RestoreSize`, `SameScreenAsTC`, `OpenOnSameMonitor`, `IconWin95`,
  `Iconsize`, `WantZoneData`, every `_dpi`-suffixed key, `monitor(...)` geometry keys.
- **Media/codec stack**: entire second half of `[Lister]` (`PreloadFilterType32/64`,
  `PreloadFilters*`, `PreloadInputFilters*`, `Filter32dir`/`Filter64dir`,
  `PreferFiltersFromFile`, `StopInMainThread`), `Uniscribe`, `IView*` (IrfanView/XnView helper),
  `PreviewHandlers`, `ThumbnailProviders`, `WmfAllowed`, `HebrewListerFix`,
  `PreventScrollbarCrash`, `ThumbExplTypes` (shell extraction), `[ImagesPlugins]`-style HTML
  rendering keys (`HTMLAsText`, `LinkBraces`).
- **Windows INI API**: the encoding restriction in 1.3 (`SetEncoding`, `Test`, `Firstmnu`) is
  purely an artefact of `Get/WritePrivateProfileString`.
- **Drive letters and shares** are baked into many keys' value grammar: `Allowed`,
  `CopyCompatDrives`, `DriveBarHide`, `DriveisCD`, `SamePartitions`, `NoReread`,
  `WatchDirsExclude`, `DriveHints`, `[connections]`, `TreeFileLocationE`,
  `TreeFileLocation__server_share`.

### 5.2 Groups relevant to a cross-platform product

- **Copy/verify engine** (`CopyBlockSize*`, `CopyHugeBlockSize*`, `SamePartitions`,
  `CountMarked*`, `SkippedFileCopy`, `VerifyCopy`, `ESCcheck`, `SizeCheck`, `BackgroundCopyFlags`,
  `Always*InBackground`, `RelativePathCopy`, `CopyAllTimes`, `CopyDirTimeStamp`, `AutoRenameMode`,
  `SpeedLimit*`, `DeleteDelay`, `AppendWarning`): all portable if "physical disk" is replaced by a
  device/st_dev grouping, "drive letters" by mount points, and "recycle bin" by a platform trash
  service.
- **Comparison and hashing** (`Compare*`, `Crc*`, `PreferHashType*`, `Lastmd5`,
  `MaxCompareSizeInMem`, `CompareUtf8Detect`, `UseChecksumCryptoApi`): fully portable; only
  `CompareResyncSeparators` and line-ending/BOM handling need OS-neutral wording.
- **Display/list behaviour** (`Show*`, `Sort*`, `SizeStyle*`, `CustomDateFormat`,
  `CustomSeparators`, `ShowCentury`, `ShowAMPM`, `DirBrackets`, `Tabstops*`, `BriefViewWidthLimit*`,
  `Scrolling*`, `SpaceMovesDown`, `SpecialCursorMovement`, `SingleClickStart`, `Inplace*`,
  `IconClickSelection`, `MarkDirectories`, `QuickSearch*`, `History*`, `FrequentlyUsed*`): portable
  as-is; dates/number formats should come from a locale API rather than a "control panel" setting.
- **Colours/themes/fonts** (`[Colors]`, `[ColorsDark]`, `[Lister]` colours, font keys, `DarkMode`,
  `IconLib`-style theming): portable if the colour model becomes a named theme plus token set, and
  if font roles and per-DPI stamps map to a font descriptor (family, size, weight) resolved per
  platform.
- **Archives** (`[Packer]`): all keys portable except `7zip`/`ZIP`/`ACE`… external program paths
  and the Windows-Explorer-compat warning keys; `LinuxCompatible` becomes the default convention.
- **FTP** (`wcx_ftp.ini`): nearly all portable; substitute the platform TLS stack for
  `PreferOpenSSL`/`VerifyCerts`' Windows-certificate-store notes, drop the port-driver and
  keyboard-hook quirks, keep the proxy, keep-alive, time-zone and encoding keys.
- **Plugins**: the section layouts (`0=`, `0_detect=`, `<name>=`, capability bit masks,
  `*64` mirrors) are portable concepts; the `*64` duplication disappears in a product that is
  64-bit-only, and the `DllLoad` policy maps to a shared-library loading policy.
- **Lock-down** (`RestrictInterface`, `Allowed`, `IgnoreListFile`, `HideRemovableNoMedia`):
  portable and often more relevant outside Windows (kiosk/server installs).
- **Explicitly Windows-shaped and best re-modelled**: drive bars, `DriveLib`, `OldDriveIcons`,
  `DriveCombo`, `ShowExtraDrives`, `CDlabel`, `CdCloseDoor`, `CheckUsbHdd`, `AdminShares`,
  `NetHood*`, `DeviceChange`, terminal-server/`tsclient` keys.

---

## 6. **Proposed** portable settings schema (design proposal — not extracted fact)

Derived design sketch, not description of the original. Goals: one file per profile, no
codepage-limited INI API, string enums and string arrays instead of magic numbers and bit sums,
explicit `platform` overrides for anything that cannot be abstracted, and a hard split between
*settings*, *history* and *session* so volatile data can be deleted or redirected on its own (the
job `AlternateUserIni`/`RedirectSection` did). Format: TOML, `snake_case`, durations with unit
suffix (`_ms`, `_s`), sizes with `_kib`/`_mib`, colours `"#RRGGBB"` or `"system"`, masks as arrays.

```toml
schema_version = 1
[storage]                     # replaces UseIniInProgramDir / -i / registry location
mode = "portable"             # portable | system | custom
config_location = ""; data_location = ""; temp_location = ""
volatile_split = true         # history.toml / session.toml / plugins.toml beside settings
[app]
single_instance = "per_user"  # OnlyOnce          ; off | per_user | per_machine
restore_geometry_on = ["display_change","dpi_change","move"]   # RestoreSize
check_updates = "off"         # AutoUpdateCheck    ; off | release | release_and_beta
splash_screen = true; admin_timeout_s = 180
```

**`ui.*` — appearance and layout**

| Table | snake_case keys (former names in comments) |
|---|---|
| `ui.layout` | `breadcrumb_bar`, `command_line`, `command_line_focus_only` (`ExecCmdLineFocused`), `cur_dir_header`, `status_bar`, `directory_tabs`, `tab_header`, `tab_header_themed`, `key_buttons`, `button_bar`, `button_bar_vertical`, `drive_bar`, `drive_bar_second`, `flat_interface`, `dark_list_borders` — one boolean each, replacing the whole `[Layout]` section. |
| `ui.panels` | `split_permille` (`Divider`), `split_permille_comments`, `split_permille_quick_view`, `separate_trees` (`SeparateTree` 0/1/2), `separate_tree_width_px`, `vertical` (`PanelsVertical`), `active_panel_on_start`, `go_to_root_on_drive_change`, `breadcrumb_delay_ms`, per-kind breadcrumb delays, `breadcrumb_max_width_px`, `breadcrumb_separator`. |
| `[[ui.panels.display]]` | One entry per resolution replacing `[<width>x<height> (…)]` and every `_dpi` stamp: `match = {width_px, height_px}`, `geometry = {x,y,width_px,height_px,maximized}`, `monitors = [...]` (replaces `monitor(x,y,dx,dy;dpi)`), `scale_factor`, `font_list`, `font_ui`, `tab_stops_px`, `icon_size_px`, `thumbnail_zoom`, `split_*` overrides. |
| `list.*` | `show_hidden`, `show_system`, `dim_hidden`, `overlay_hidden_system`, `icons`, `directories_brackets`, `sort_directories_by_name`, `sort_method`, `sort_dotfiles_last`, `use_long_names`, `show_parent_in_root`, `brief_view_max_width_px`, `brief_view_min_width_px`, `size_style`, `size_style_transfer`, `decimal_units`, `date_format`, `show_seconds`, `show_century`, `show_am_pm`, `scroll_margin_top/bottom`, `inplace_rename`, `inplace_rename_path`, `single_click`, `cursor_style`, `selection_style`, `mouse_selection_button`, `rubber_band_selection`, `confirmations[]`. |
| `list.quick_search` | `invoke` (`AltSearch`), `match_beginning`, `exact_dot_match`, `allow_no_match`, `auto_filter` (`off\|on\|on_not_persisted`), plus the two per-list auto-filter switches. |
| `list.tabs` | `always_visible`, `multiple_rows`, `equal_widths`, `title_length_limit`, `close_on_double_click`, `new_tab_foreground`, `new_tab_near_current`, `mark_locked_with_asterisk`, `show_drive_in_title`, `ctrl_tab` (`cycle\|list`), `last_visited_first`, `revert_locked_tab`, `per_tab_filter`, `icons[]` — replacing `DirTabOptions`/`DirTabIcons` bit sums. |
| `columns.full` | `extensions` (`inline\|aligned`), `auto_adjust_name_width` (`[Tabstops] AdjustWidth`). |
| `[[columns.custom]]` | `name`, `fs_plugin` (for `[CustomFields_<Plugin>]`), `auto_load`, and nested `[[columns.custom.field]] {header, content, width_px, align}` — replacing `Titles`/`Headers<n>`/`Contents<n>`/`Widths<n>`/`Options<n>`. |
| `[[view_modes]]` | `name`, `icon`, `columns`, `sort {by, reversed, additional[]}`, `tab_color`, `background`, `background_alt`, `background_dark`, `background_alt_dark`, `commands_on_enter[]`, `commands_on_leave[]` (the `-`/`*`/`\|` prefixes become two separate lists). |
| `[[view_modes.auto_switch.rule]]` | `enabled`, then per rule `condition` (structured predicate over location kind + mask rule) and `mode`. |
| `ui.colors.scheme` + `ui.colors.light` / `ui.colors.dark` | `text`, `background`, `background_alt`, `selection`, `cursor`, `cursor_text`, `panel_title_active(_text)`, `panel_title_inactive(_text)`, `tooltip{text,background,border}`, `progress{top,bottom}`, `border`, `horizontal_lines`, `highlight`, `highlight_pressed`, `cursor_pen_width_px`, `themed_cursor`. Two sibling tables replace `[Colors]` + `[ColorsDark]`. |
| `[[ui.colors.filters]]` | `mask`, `light`, `dark` — one entry replacing `ColorFilter<n>` + `…Color` + `…ColorDark`. |
| `ui.colors.compare`, `ui.colors.lister` | The compare-pane and Lister colour sets from §3.2. |
| `ui.fonts` | `dialog`, `tooltip`, `compare`, `history` (`inherit_list\|inherit_ui\|family`), `lister{ansi,ascii,variable,printer_fixed,printer_variable}`, `antialiasing`, `auto_scale_dialogs` — each font a descriptor `{family,size_pt,weight}`. |
| `ui.icons` | `library`, `drive_source`, `size_px`, `min_icon_width_px`, `min_line_height_px`, `menu{show,frame,size_px,toggled_as_pressed}`, `overlays[]`, `special_folders`. |
| `ui.thumbnails` | `database`, `icon_size_px`, `width_px`, `height_px`, `memory_limit_mib`, `preallocate_bytes`, `preload`, `sync_with_copy_delete`, `show_in_overwrite_dialog`, `fields_enabled`, and an ordered `[[ui.thumbnails.methods]] {provider, include[], exclude[]}` replacing `ThumbOptions` + six mask keys. |

**`fileops.*` — copy, delete, rename, compare, checksum**

| Table | Keys |
|---|---|
| `copy` | `block_size_kib`, `block_size_cross_disk_kib`, `huge_block_size_kib`, `huge_block_size_cross_disk_kib`, `same_device_groups[][]` (`SamePartitions`), `copy_all_times`, `copy_dir_timestamps`, `preserve_links`, `sparse`, `copy_streams`, `verify_after_copy`, `count_files_first` (+ per-context overrides), `background`, `overwrite_default`, `background_overwrite`, `long_name_warn`, `warn_on_existing_dir_target`, `case_sensitivity_inherit`, `relative_path_keep`, `stay_on_source_file`. |
| `delete` | `use_trash`, `delay_ms`, `explorer_method` (platform flag), `warn_append`. |
| `rename` | `select_name_only`, `arrow_keys_move_files`, `auto_rename_mode[]`, `set_archive_flag`, `undo_history_len`, `inplace_confirm_button`, `plugin_char_sanitise`. |
| `comments` | `preferred_format`, `read_explorer_comments`, `read_both_types`, `copy_with_files`, `dos_charset`. |
| `ignore_list` | `enabled`, `file`, `patterns[]`. |
| `checksums` | `separate_per_file`, `separate_per_folder`, `append_extension`, `preference_256[]`, `preference_other[]`, `utf8_names`, `unix_format`, `blake3_block_size_mib`, `force_crypto_api`. |
| `compare` | `case_sensitive`, `ignore_repeated_lines`, `ignore_space_runs`, `skip_line_numbers`, `resync`, `resync_separators`, `extra_lines`, `f2_mode`, `find_lines`, `bytes_per_line`, `utf8_detection`, `vertical`, `two_line_list`, `keep_original_as_bak`, `always_save_bom`, `delete_bak_on_close`, `external_tool`, `max_in_memory_bytes`. |
| `archive` | `treat_as_directories`, `expert_mode`, `external_packers{}`, `remember_last_password`, `wipe_temp_of_encrypted`, and per-format sub-tables `archive.zip{level,unicode_names,aes,oem_names,recurse,store_paths,test_after_pack,set_date_to_newest,temp_dir,allow_lzma}`, `archive.sz7{engine,level,solid,threads,method,encrypt_names,dictionary/solid per level}`, `archive.tar{variant}`. |
| `plugins` | `base_dir`, `auto_install`, `load_policy`, `restrict`, plus `[[plugins.packer]] {name,path,capabilities[]}`, `[[plugins.filesystem]]`, `[[plugins.content]]`, `[[plugins.lister]] {name,path,detect}`; a product that is 64-bit-only drops the `*64` mirror sections. |
| `ftp` | `open_in`, `transfer_mode`, `ascii_masks[]`, `passive_mode_default`, `show_hidden`, `retries`, `retry_delay_s`, `connect_timeout_s`, `http_timeout_s`, `auto_resume_s`, `auto_disconnect`, `background_transfers`, `keep_alive{transfer,tcp}`, `mode_z`, `mdmt{set_timestamps,universal_time}`, `proxy{host,method,user,password}`, `tls{verify,provider,implicit,explicit}`, `anonymous_email`, `log_file`, `special_flags[]`, plus `[[ftp.site]]` entries mirroring the per-site group in 2.24 with `password_store` instead of a scrambled password. |
| `search`, `sync` | `keep_results_on_close`, `last_options[]`, `extra_archive_masks[]`; `sync{subdirs,by_content,ignore_dates,asymmetric,empty_dirs,hide_hidden_system,ignore_junctions,recompare_after_copy,warn_read_errors}` plus named `[[sync.saved]]` entries replacing `[SyncOptions] <name>_*`. |
| `history` | `save_on_exit`, `max_stored_per_panel`, `max_shown`, `thinning_ms`, `thinning_requires_operation`, `newest_first`, `frequent_max_shown`, `frequent_max_stored`, `frequent_location`, `store[]`. |
| `session` | `save_panel_paths`, `save_panel_view`, `save_command_lines`, `[[session.panel]] {path, view, filter, mask, sort{by,reversed,additional[]}, special_view, tabs[[…]]}` — replaces `[Left]`/`[Right]`, `[LeftTabs]`, `[RightTabs]`, `ActiveRight`, `Savepath`, `Savepanels`. |
| `keys`, `policy`, `logging` | `[[keys.binding]] {chord, command, parameters}`, `[[keys.alias]] {name, command_line}` (`[Shortcuts]`, `[ShortcutsWin]`, `[Alias]`); `hide_ui_parts[]`, `allowed_drives[]`, `disable_update_checks` (`RestrictInterface`, `Allowed`); `file_operations`, `options[]`, `rotate`, `rotate_size_kib`, `keep_count` (the `LogOptions` bit sum becomes a list). |

Notes on the proposal:

- **Per-display instead of per-resolution-plus-DPI-suffix.** Every `_dpi` stamp, `_120` overlay and
  `[<width>x<height> (f>x<c>)]` section collapses into one `[[ui.panels.display]]` entry keyed by
  display match plus a `scale_factor`; geometry re-application follows `restore_geometry_on`.
- **Enums and sets become strings/arrays** (`DirTabOptions`, `LogOptions`, `WantZoneData`,
  `IgnoreLinks`, `ShowExtraDrives`, `SyncFlags`, `RestrictInterface`, plugin capability masks), so a
  new flag never reinterprets a stored integer.
- **Light/dark pairing becomes structural**: one palette per scheme plus one `{light, dark}` pair for
  colour filters, instead of parallel `[Colors]`/`[ColorsDark]` key sets and comma-packed values.
- **Volatile state is isolated** in `history` and `session`, which preserves the original motivation
  for `AlternateUserIni`/`RedirectSection`: deleting one file resets history, and preferences can be
  shipped read-only.
- **Platform-bound behaviour** (elevation, shell associations, media filters, tray/taskbar, recycle
  bin, drive enumeration, MTP/virtual folders, NTFS streams) should live under explicit
  `[platform.windows]` / `[platform.linux]` / `[platform.android]` override blocks rather than being
  emulated with Windows-shaped flags such as `Win95Delete`, `VistaDelete`, `DriveisCD` or `DriveLib`.
- **Preserved semantics worth keeping verbatim**: the ordered-filter priority model (colours and auto
  switch rules), the ordered thumbnail-provider list, the "save history but do not clear it" split,
  the three independent save switches (paths / panel view / command lines), and the distinction
  between "remembered for the session" and "remembered across restarts".
