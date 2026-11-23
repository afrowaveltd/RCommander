# Spec 02 — Keyboard map, function keys, command line, internal command catalogue

Source: Total Commander 11.58 English help (extracted from `TOTALCMD.CHM`), pages `keyboard.htm`,
`cmdline.htm`, `add_cmdline.htm`, `env_vars.htm`, `commandlineparameters.htm`, `quick_view.htm`,
`multi_rename_tool.htm`, `dialog_box___multi_rename_tool.htm`, `synchronize_dirs.htm`,
`dlg_synchronize_dirs.htm`, `dlg_overwrite.htm`, and the Lister pages (`listercontents.htm`,
`lister_menufile.htm`, `lister_menuedit.htm`, `lister_menuoptions.htm`, `lister_menupercent.htm`,
`lister_menuplugins.htm`, `lister__encoding_menu.htm`, `dlg_listerconfig.htm`,
`dlg_printoptions_lister.htm`), plus `dlg_synchronize.htm`, `go_back.htm`, `directoryhotlist.htm`,
`add_hothist.htm`, and `work/re/out/internal-commands.json` (594 entries from `TOTALCMD.INC`).

Conventions: **core** = behaviour that any clone needs; **win** = tied to a Windows API/shell
feature and can be stubbed or omitted on Linux/Android; **opt** = documented feature but not
required for a usable clone. Statements not literally in the accepted pages are marked
*(inferred)*.

---

## 1. Keyboard map

### 1.1 File panel, general (from `keyboard.htm`)

| Key | Action | Feature area |
|:--|:--|:--|
| `F1` | Help | Help |
| `F2` | Reread source window | Panel refresh |
| `F3` | List files (Lister) | Viewer |
| `F4` | Edit files | Editor |
| `F5` | Copy files | File operations |
| `F6` | Rename or move files | File operations |
| `F7` | Create directory | File operations |
| `F8` or `DEL` | Delete files — to recycle bin or directly, per configuration | File operations |
| `F9` | Activate menu above source window (left or right) | Menu / focus |
| `F10` | Activate left menu or leave menu | Menu / focus |
| `ALT+F1` | Change left drive | Drive selection |
| `ALT+F2` | Change right drive | Drive selection |
| `ALT+F3` | Use alternate (external or internal) viewer | Viewer |
| `ALT+SHIFT+F3` | Start Lister with internal viewer, no plugins / no multimedia | Viewer |
| `ALT+F4` | Exit, or Minimize when option `MinimizeOnClose` is set in `wincmd.ini` | Application |
| `ALT+F5` | Pack selected files | Archives |
| `ALT+SHIFT+F5` | Move to archive (pack, then delete) | Archives |
| `ALT+F6` | Unpack specified files from archive under cursor, or selected archives | Archives |
| `ALT+F7` | Find | Search |
| `ALT+SHIFT+F7` | Find in a separate process | Search |
| `ALT+F8` | Open the command-line history list | Command line |
| `ALT+F9` | Same as `ALT+F6` (legacy: `ALT+F6` broken on Windows 95) | Archives |
| `ALT+SHIFT+F9` | Test archives for integrity | Archives |
| `ALT+F10` | Dialog box with the current directory tree | Navigation |
| `ALT+F11` | Open left current-directory (breadcrumb) bar | Navigation / UI |
| `ALT+F12` | Open right current-directory (breadcrumb) bar | Navigation / UI |
| `ALT+SHIFT+F11` | Focus the horizontal button bar for keyboard use | Button bar |
| `ALT+SHIFT+F12` | Focus the vertical button bar for keyboard use | Button bar |
| `ALT+left` / `ALT+right` | Go to previous / next already-visited directory | History |
| `ALT+down` | Open history list of visited directories (browser-style) | History |
| `ALT+SHIFT+down` | History list **without** history thinning | History |
| `SHIFT+F1` | Custom columns view menu | View mode |
| `SHIFT+F2` | Compare file lists | Compare / sync |
| `SHIFT+F3` | List only the file under the cursor when multiple files are selected | Viewer |
| `SHIFT+F4` | Create new text file and load it into the editor | File operations |
| `SHIFT+F5` | Copy files (with rename) in the same directory | File operations |
| `SHIFT+CTRL+F5` | Create shortcuts of the selected files | File operations |
| `SHIFT+F6` | Rename files in the same directory (in-place) | File operations |
| `SHIFT+F7` | Create directory in the target folder, name under cursor as default | File operations |
| `SHIFT+F8` / `SHIFT+DEL` | Delete directly / to recycle bin — the opposite of plain `F8`, per configuration | File operations |
| `SHIFT+F10` | Show context menu | Context menu |
| `SHIFT+ESC` | Minimize Total Commander to an icon | Application |
| `NUM +` | Expand selection (configurable: files only, or files and folders) | Selection |
| `NUM -` | Shrink selection | Selection |
| `NUM *` | Invert selection | Selection |
| `NUM /` | Restore selection | Selection |
| `SHIFT+NUM +` | Like `NUM +`, but the other of files / files+folders | Selection |
| `SHIFT+NUM -` | Always removes selection from files only (`NUM -` acts on files and folders) | Selection |
| `SHIFT+NUM *` | Like `NUM *`, but the other of files / files+folders | Selection |
| `CTRL+NUM +` | Select all (configurable files / files+folders) | Selection |
| `CTRL+SHIFT+NUM +` | Select all with the other of files / files+folders | Selection |
| `CTRL+NUM -` | Deselect all (always files **and** folders) | Selection |
| `CTRL+SHIFT+NUM -` | Deselect all (always files only, no folders) | Selection |
| `ALT+NUM +` | Select all files with the same extension | Selection |
| `ALT+NUM -` | Remove selection from files with the same extension | Selection |
| `CTRL+PgUp` or `Backspace` | Change to parent directory (same as `cd ..`) | Navigation |
| `CTRL+PgDn` | Open directory / archive (including self-extracting `.EXE` archives) | Navigation / archives |
| `CTRL+<` | Jump to root directory (most European keyboards) | Navigation |
| `CTRL+\` | Jump to root directory (US keyboard) | Navigation |
| `CTRL+left` / `CTRL+right` | Open directory/archive and display it in the **target** window; if the cursor is not on a directory, or the other panel is active, shows the current directory instead | Navigation |
| `CTRL+F1` | File display "brief" (names only) | View |
| `CTRL+SHIFT+F1` | Thumbnails view | View |
| `CTRL+F2` | File display "full" (all details) | View |
| `CTRL+SHIFT+F2` | Comments view (new comments created with `CTRL+Z`) | View |
| `CTRL+F3` | Sort by name | Sorting |
| `CTRL+F4` | Sort by extension | Sorting |
| `CTRL+F5` | Sort by date/time | Sorting |
| `CTRL+F6` | Sort by size | Sorting |
| `CTRL+F7` | Unsorted | Sorting |
| `CTRL+F8` | Display directory tree | Tree view |
| `CTRL+SHIFT+F8` | Cycle separate-tree states: one tree, two trees, off | Tree view |
| `CTRL+F9` | Print file under cursor using the associated program | Print |
| `CTRL+F10` | Show all files | Filter |
| `CTRL+F11` | Show only programs | Filter |
| `CTRL+F12` | Show user-defined files | Filter |
| `TAB` | Switch between left and right file list | Panel focus |
| `SHIFT+TAB` | Switch between current file list and separate tree (if enabled) | Panel focus |
| any `Letter` | Redirected to command line; cursor jumps to the command line | Command line |
| `INSERT` | Select file or directory | Selection |
| `SPACE` | Select file or directory (like `INSERT`); on an **unselected directory under the cursor**, counts its contents and shows the size in "full" view instead of `<DIR>` (can be disabled via Configuration – Options – Operation – "Selection with Space") | Selection / dir size |
| `ENTER` | Change directory / run program / run associated program / execute the command line if not empty; inside an archive, shows further information about the packed file | Navigation / execution |
| `SHIFT+ENTER` | (1) Run command line or program under cursor via the Windows shell, leaving its window open (requires `NOCLOSE.EXE` in the TC directory). (2) For ZIP files: use the alternative of "treat archives like directories" vs "call associated program", as chosen in Packer config. (3) In the directory history list (`CTRL+D`), open the directory on a new tab | Execution / archives / history |
| `ALT+SHIFT+ENTER` | Counts the contents of all directories in the current directory and shows sizes in "full" view instead of `<DIR>`; abort by holding `ESC` | Directory size |
| `ALT+ENTER` | Show property sheet; on `[..]` shows properties of the current directory | Properties |
| `CTRL+A` | Select all | Selection |
| `CTRL+SHIFT+A` | Show list of all open tabs as a menu (Chrome-style); pressing again toggles title/directory display | Tabs |
| `CTRL+B` | Directory branch: contents of current dir and all subdirs in one list | Branch view |
| `CTRL+SHIFT+B` | Branch view for the **selected** directories only | Branch view |
| `CTRL+C` | Copy files to clipboard | Clipboard |
| `CTRL+D` | Open directory hotlist ("bookmarks") | Hotlist |
| `CTRL+F` | Connect to FTP server | FTP |
| `CTRL+SHIFT+F` | Disconnect from FTP server, port connection, and some file system plugins | FTP |
| `CTRL+I` | Switch to target directory | Navigation |
| `CTRL+L` | Calculate occupied space of the selected files | Directory size |
| `CTRL+M` | Multi-Rename Tool | Multi-rename |
| `CTRL+SHIFT+M` | Change FTP transfer mode | FTP |
| `CTRL+N` | New FTP connection (enter URL or host address) | FTP |
| `CTRL+P` | Copy current path to the command line | Command line |
| `CTRL+Q` | Show Quick View panel instead of a file list | Quick view |
| `CTRL+SHIFT+Q` | Separate Quick View window (Lister-like, updates with cursor movement) | Quick view |
| `CTRL+R` | Reread source directory | Panel refresh |
| `CTRL+S` | Open Quick Filter dialog and activate filter (deactivate with `ESC` or `CTRL+F10`) | Filter |
| `CTRL+SHIFT+S` | Open Quick Filter dialog and re-activate the last-used filter | Filter |
| `CTRL+T` | Open new folder tab and activate it | Tabs |
| `CTRL+SHIFT+T` | Open new folder tab, but do not activate it | Tabs |
| `CTRL+U` | Exchange directories | Panels |
| `CTRL+SHIFT+U` | Exchange directories **and** tabs | Panels |
| `CTRL+V` | Paste from clipboard to current dir | Clipboard |
| `CTRL+W` | Close currently active tab | Tabs |
| `CTRL+SHIFT+W` | Close all open tabs | Tabs |
| `CTRL+X` | Cut files to clipboard | Clipboard |
| `CTRL+Z` | Edit file comment | Comments |
| `CTRL+UP` | Open dir under cursor in a new tab | Tabs |
| `CTRL+SHIFT+UP` | Open dir under cursor in the other window (new tab) | Tabs |
| `CTRL+TAB` | Jump to next tab | Tabs |
| `CTRL+SHIFT+TAB` | Jump to previous tab | Tabs |
| `ALTGR+Letter(s)` or `CTRL+ALT+Letter(s)` | Quick search for a file name starting with the typed letters in the current directory; supports `Ctrl+X/C/V/A`, and `Ctrl+S` to toggle the search filter | Quick search |

### 1.2 Command line (`cmdline.htm`)

Nearly all keyboard input goes to the command line while TC is active.

| Key | Action |
|:--|:--|
| `ENTER` | Executes the command line if it contains at least one character, otherwise starts the program under the cursor in the source window. `cd`, `md`, `rd` are executed internally; an internal DOS command runs DOS with that command; otherwise a program of the given name is started. |
| `SHIFT+ENTER` | Like `ENTER` but via the Windows shell; the called DOS program's window is not auto-closed (requires `noclose.exe` in the TC directory). |
| `CTRL+ENTER` | Appends the file under the cursor in the source directory to the command line. |
| `CTRL+SHIFT+ENTER` | Appends the file under the cursor **including its path**. |
| `CTRL+CURSORDOWN` or `ALT+F8` | Opens the command-line history list; select with `up`/`down` (with or without `CTRL`); pressing `left`/`right` edits the command line and closes the list. |
| `TAB` | Puts the cursor back to the source directory. |
| `Cursor left` / `right` | Move through the command line even if the cursor was in the source directory; in Brief view they move through the source directory instead, so `SHIFT` is needed to move inside the command line. |
| `Cursor up` / `down` | Move the cursor bar inside the source window even if the cursor was in the command line. |
| `ESC` | Clears the command line and puts the cursor back in the file window. |
| `CTRL+A` | Select the entire command line. |
| `CTRL+Y` | Clear the command line, cursor stays in the command line. |
| `CTRL+C` / `CTRL+X` / `CTRL+V` | Copy / cut selected text to clipboard / paste text. |
| `CTRL+E`, `CTRL+K`, `CTRL+W`, `CTRL+T` | Copy previous command / delete to end of line / delete word left of cursor / delete word right of cursor. |

### 1.3 Quick view (`quick_view.htm`)

| Key | Action |
|:--|:--|
| `CTRL+Q` | Toggle Quick view mode; pressing again (or selecting another drive in the Quick view panel) disables it. |
| `CTRL+SHIFT+Q` | Separate Quick view **window** (Lister-based; title bar shows `Lister*`). |
| Lister keys | The Quick view pane *is* a Lister window, so all Lister shortcuts apply while it is active (e.g. `CTRL+F` or `F7` to search). |

### 1.4 Multi-Rename Tool (`multi_rename_tool.htm`, `dialog_box___multi_rename_tool.htm`)

| Key | Action |
|:--|:--|
| `CTRL+M` | Open the Multi-Rename Tool. |
| `SHIFT+Del` | Remove a no-longer-wanted entry from the mask/history combo box. |
| `F2` | Open the load/save-settings list. |
| `F5` | "(next step)" — load the rename results as input for the next rename step (apply multiple rule sets to the same files). |
| `F10` | Open the context menu ("Load names from file" / "Edit names" / "Configure editor"). |
| `SHIFT+Cursor` keys | Reorder individual items in the file list (changes file order for the counter function). |
| `Del` | Remove files from the list so they are not renamed. |
| `Alt+124` | Typing the vertical bar (used to separate multiple search/replace pairs). |

### 1.5 Synchronize dirs (`dlg_synchronize_dirs.htm`, `synchronize_dirs.htm`)

| Key | Action |
|:--|:--|
| `F2` | Open saved directory/settings list. |
| `F6` | Rename the **left** file of the focused pair. |
| `SHIFT+F6` | Rename the **right** file of the focused pair. |
| `CTRL+PageDown` / `CTRL+PageUp` | Jump to next / previous subdirectory block. |
| `CTRL+SHIFT+L`, `E`, `N`, `R`, `D`, `S` | Toggle the "Show:" filter buttons (left-to-right, equal, different, right-to-left, duplicates, singles) — except while focus is in a path edit box. |
| Right-click menu / double-click | Choose copy direction for multiple files; display, delete, or compare individual files by content. |

### 1.6 Overwrite dialog (`dlg_overwrite.htm`)

| Key | Action |
|:--|:--|
| `Alt`+underlined letter | Activate the corresponding button. |
| `F3` | View the upper (target) file. |
| `SHIFT+F3` | View the lower (source) file. |
| `ALT+F3` / `ALT+SHIFT+F3` | View the file without plugins. |
| `CTRL+F3` | Compare the two files by content. |
| `CTRL+SHIFT+F3` | Use the internal compare tool when an external tool is configured via `wincmd.ini`. |

### 1.7 Lister / viewer (`listercontents.htm` and the Lister menu pages)

General and multi-file:

| Key | Action |
|:--|:--|
| `F1` | Open Help. |
| `F2` | Refresh/reload file contents (also "File – Reload file"). |
| `F11` | Toggle maximized window. |
| `Alt+Enter` | Toggle full-screen mode. |
| `Ctrl+P` | Print file. |
| `Esc`, `Alt+F4`, `F10`, `Q` | Close Lister window. |
| `N` / `P` | Next / previous file (when multiple files are loaded). |
| `Alt+Left` / `Alt+Right` | Switch to previous / next file; `<<` and `>>` in the menu bar do the same. |
| `4` | Cycle through all matching Lister plugins (when several are defined, only through the ones defined for the extension). |
| `F4` | Open the current file in the editor (not for temporary files, or in Image/Multimedia, Explorer Preview, or plugin modes). |
| `Shift+F10` or Menu key | Context menu (also right-click). |

Text modes:

| Key | Action |
|:--|:--|
| `Ctrl+A` | Select all text. |
| `Ctrl+C` | Copy selected text (all `#0` converted to blanks). |
| `F7`, `Ctrl+F` | Find text. |
| `F3`, `F5`, `Shift+F7` | Find next. |
| `Shift+F3`, `Shift+F5` | Find previous. `Ctrl+F3` / `Ctrl+F5` reverse the search direction. |
| Mouse wheel / `Shift`+wheel / `Ctrl+Alt`+wheel | Scroll vertically / horizontally / page by page. |
| Cursor keys, `Home`, `End`, `PgUp`, `PgDn`, `Space` | Scroll through the text (text cursor off). Menu-bar item `%n` scrolls to a position in percent (accepts decimals, e.g. `50.5`). |
| `Ctrl+Home` / `Ctrl+End` | Go to beginning / end of file (text cursor on). |
| `Shift`+arrows | Select text (text cursor on). |
| `Ctrl+Left` / `Ctrl+Right` | Previous/next word (text cursor on) or scroll sideways (text cursor off). |
| `+` / `-` (numeric keypad), `Ctrl`+wheel, `Ctrl+0` | Increase / decrease font size, reset to stored size. |
| `Ctrl+Right Shift` / `Ctrl+Left Shift` | Right-aligned / left-aligned text (Unicode or Arabic/Hebrew text). |
| `a` / `s` / `v` | ANSI (Windows) / ASCII (DOS) / variable-width charset. `F6` shows a text cursor for selecting and copying (no editing). |

Image mode: `+`/`-` (keypad) or `Ctrl`+wheel zoom, `Ctrl+0` reset zoom to 100 %, `F` fit image to window, `L` fit only if larger than window.

Media-player mode: `E` single step, `C` show/hide cover image, `M` mute/unmute, `Up`/`Down` volume by 1 %, `Ctrl`/`Shift`+`Up`/`Down` volume by 10 %.

Lister search syntax also accepts `\t` (tab), `\n` (newline, CR+LF only), `\\` (backslash) and regular expressions.

Modifier semantics on `F3` (from `proc_show.htm`): plain `F3` uses the configured default (file under
cursor, or selected files); `SHIFT` selects the *other* of the two; `ALT` toggles external/internal
viewer; `ALT+SHIFT+F3` starts Lister with no plugins and no multimedia (`F3` itself is described as
"multiple only with internal viewer").

---

## 2. Function-key bar

Source: `add_buttons.htm` (the bar itself), `keyboard.htm` / `menu_fileoperations.htm` (labels), plus
`cm_VisKeyButtons` in the command catalogue.

| Button | Label / key | Meaning | Notes |
|:--|:--|:--|:--|
| — | `F1` Help | Online help | **No button is drawn** for F1 or F2, "to avoid overcrowding the window". |
| — | `F2` Reread directory | Re-read the source panel and drive bar | No button; same as `CTRL+R`. |
| `F3` | View / "List files" | Open the Lister on the cursor file or the selected files | Also a **Drag & Drop target**. |
| `F4` | Edit | Open the cursor/selected file in the configured editor | Also a **Drag & Drop target**. |
| `F5` | Copy | Copy selected files/dirs to the target side | Dialog also accepts `F2` = Queue (background transfer manager). |
| `F6` | Rename/Move | Rename or move to another directory/drive | |
| `F7` | Make directory | Create a subdirectory in the source directory | |
| `F8` | Delete | Delete to recycle bin or directly, per configuration | Also a **Drag & Drop target**; **right-click the F8 button opens the Recycle Bin**. |

Additional documented behaviour:

- Clicking a button or pressing `F1`–`F8` activates the same function.
- `cm_VisKeyButtons` shows/hides the function-key button bar; `SHIFT+ESC` minimizes the program.
- The equivalent menu/`ALT` forms of the same operations: `ALT+F5` pack, `ALT+SHIFT+F5` move to
  archive, `ALT+F6`/`ALT+F9` unpack, `ALT+SHIFT+F9` test archives, `ALT+F4` exit.
- The function-key bar is a distinct element from the (configurable) **button bar**
  (`add_btnbar.htm`), which runs arbitrary programs, internal `cm_*` commands, `cd`, `FTPOPEN`,
  `OPENTABS`/`APPENDTABS`, sub-bars, etc., and from the **vertical button bar** (`ALT+SHIFT+F12`).

---

## 3. Command line

### 3.1 What the command line accepts (`cmdline.htm`, `add_cmdline.htm`, `env_vars.htm`)

| Input form | Behaviour |
|:--|:--|
| `ENTER` with empty line | Starts the program under the cursor in the source window. |
| Program name + parameters | Program of that name is executed (single-asterisk prefix ⇒ run as administrator; double asterisk ⇒ run as a different user; `*cmd` is replaced by `cmd.exe` in `System32`). |
| `cd`, `md`, `rd` (lowercase) | Executed **internally**. `cd` changes the active panel's directory; giving a *file* name places the cursor on that file. |
| Internal DOS commands | DOS/command processor is started with that command. |
| Other commands | Program with the given name is executed. |
| `cm_…` / `em_…` internal commands | Handled by TC. Commands with parameters (e.g. `OPENTABS`) must be written in **capital letters**. |
| Aliases | Defined in Configuration – Options – Misc. (e.g. `np` → `notepad.exe`). |
| Environment variables | Supported anywhere (see 3.2). |
| History | Old command lines are saved; `CTRL+Cursor down` or `ALT+F8` opens the list; `Shift+Del` removes an entry. |
| Command line switched off | It is enabled automatically as soon as characters are typed. |
| `SHIFT+ENTER` | Runs the command in a command-line window that stays open after the command ends. |
| Max length | 2047 characters on Windows 2000, 8191 on Windows XP (OS limit). |

Supported path formats for `cd` (from `add_cmdline.htm`):

`c:\Windows` · `\\server\share\directory` · `file:///c|/Windows` · `file:///c:/Windows` ·
`file://localhost/c|/Windows` · `file://localhost/c:/Windows` · `file://server/share/directory` ·
`file://c:/Program%20Files`

Remote / plugin panels:

- On an FTP panel, commands are sent **directly to the FTP server**; `HELP` lists server commands.
  Lowercase `cd` is translated to the FTP command; uppercase `CD` is TC's own change-directory.
- On a file-system-plugin panel (e.g. SFTP), commands go to the plugin first; unhandled ones are
  processed by TC (`cd dir` → plugin, `CD dir` → TC). Actual behaviour depends on the plugin.
- `GET`, `PUT`, `RETR`, `STOR` are intercepted by TC when an FTP panel is active (`ftp_menu.htm`).

### 3.2 Environment variables (`env_vars.htm`)

Usable "in most places where directories are used" — button bar, start menu, command line.
Partial-variable syntax follows `cmd.exe`: `%var:~5,7%` skips 5 characters and takes the next 7;
negative numbers count from the end.

| # | Class | Examples / members |
|--:|:--|:--|
| 1 | Regular environment variables | `%TEMP%`, `%USERNAME%` |
| 2 | Internal TC variables | `%COMMANDER_DRIVE%`, `%COMMANDER_EXE%`, `%COMMANDER_INI%`, `%COMMANDER_INI_PATH%`, `%COMMANDER_INSTANCE%` (first instance = 1), `%COMMANDER_PATH%` (no trailing backslash), `%COMMANDER_SESSIONNAME%` (`admin` if started elevated, else `user`), `%COMMANDER_OPEN%` (launch origin: `bar`, `changeini`, `cmd`, `cmdline`, `contextmenu`, `delete`, `edit`, `editlistfile`, `editname`, `editnew`, `find`, `hotkey`, `hotlist`, `lister`, `menu`, `mrtlist`, `new`, `open`, `opencontextmenu`, `pack`, `sync`, `unpack`, `view`, `viewmode`) |
| 3 | `%$DATE%` pseudo-variable | `%$DATE%` = `YYYYMMDDhhmmss`; `%$DATE:placeholders%` with `Y y M D h H i i1 m s` (same letters as the multi-rename placeholders; non-alpha characters are inserted literally); `%$DATE±offset%` where the unit is `s m h D M Y`; `%$DATE:placeholders±offset%` |
| 4 | Clipboard pseudo-variables | `%$CLIPBOARD%` (up to 8191 chars, unmodified), `%$CLIPBOARD0%` (line breaks → spaces, cut at last line break), `%$CLIPBOARD1%`, `%$CLIPBOARD2%` (line n); `%$CLIPNAME%` (same but `*?\|<>` replaced by `_`), `%$CLIPNAME0%`, `%$CLIPNAME1%`/`2` (also trims leading/trailing spaces and codes < 32), `%$CLIPNAME_NE1%`/`2` (skip empty lines), `%$CLIPNAMEQ1%`/`Q2%` and `%$CLIPNAME_NEQ1%`/`Q2%` (add double quotes if the name contains a space; lowercase `q` always quotes) |
| 5 | `ShGetSpecialFolderLocation` names | `APPDATA`, `DESKTOP`, `PERSONAL`, `PROGRAMS`, `STARTMENU`, `STARTUP`, `FONTS`, `MYPICTURES`, `LOCAL_APPDATA`, `COMMON_APPDATA`, `COMMON_DESKTOPDIRECTORY`, `COMMON_DOCUMENTS`, `COMMON_PROGRAMS`, `COMMON_STARTMENU`, `COMMON_STARTUP`, `COMMON_PICTURES`, `MYVIDEO`, `MYMUSIC` (first character must be uppercase) |
| 6 | `SHGetKnownFolderPath` names | ~90 names, e.g. `AccountPictures`, `AdminTools`, `CameraRoll`, `CDBurning`, `Contacts`, `Cookies`, `Documents`, `Downloads`, `Favorites`, `History`, `Libraries`, `Links`, `LocalAppData`, `LocalAppDataLow`, `Music`, `Pictures`, `Playlists`, `PrintHood`, `Profile`, `ProgramData`, `ProgramFiles`, `ProgramFilesX64`, `ProgramFilesX86`, `ProgramFilesCommon`, `Public*`, `QuickLaunch`, `Recent`, `Ringtones`, `RoamingAppData`, `SavedGames`, `SavedPictures`, `SavedSearches`, `Screenshots`, `SearchHistory`, `SendTo`, `SkyDrive*`, `System`, `SystemX86`, `Templates`, `Videos`, `Windows` |
| 7 | GUID passthrough | `%${GUID}%`, e.g. `%${374DE290-123F-4565-9164-39C4925E467B}%` = Downloads folder |

Classes 5–7 are Windows shell APIs; on Linux/Android only class 1, 2, 3, 4 are portable
(*(inferred)* mapping of the shell folders to XDG dirs).

### 3.3 Program command-line parameters (`commandlineparameters.htm`)

Syntax:

```
totalcmd.exe [/o] [/n] [Drive1:\Directory1 [Drive2:\Directory2]] [/i=name.ini] [/f=ftpname.ini]
totalcmd.exe [/o] [/n] [/L=Drive1:\Directory1] [/R=Drive2:\Directory2] [/i=name.ini] [/f=ftpname.ini]
```

Switches are **not** case sensitive.

| Parameter | Meaning |
|:--|:--|
| `Drive:\Directory` (1 or 2 positional) | First directory → left window, second → right window. Names with spaces **must** be quoted; full path required. |
| `FTPOPEN:stored_ftp_name` | As a path: open the stored FTP connection. |
| `LOADLIST:path\listfile.txt` | Load a file/folder list from a text file into the panel (same as button-bar `LOADLIST0`). |
| `\\server\share\subdir`, `smb://server/share/subdir` | Accepted as a path (shares). |
| `shell:Folder`, `::{GUID}` | Accepted as a path (virtual folders). |
| `*.tab` file as path | Load the saved folder tabs, replacing the current tabs. |
| `/O` | If TC is already running, activate it and pass the path(s) to that instance (overrides the multiple-windows setting). Optional argument: `0` never open a new instance (do nothing if none running); a *number* = same as `OnlyOnce=` in `wincmd.ini`; `-` = `-1`. Ignored when `/S=` is used. |
| `/N` | Always open a new TC window (overrides single-instance). |
| `/L=` / `/R=` | Set path in left / right window. |
| `/A` | If an archive is passed: don't open it; open its directory and put the cursor on the archive. |
| `/P` | (no `=`) Open the **parent** of the passed path and put the cursor on its last component, e.g. `/P c:\Windows\System32` → `c:\Windows`, cursor on `System32`. |
| `/S` | Treat passed dirs as source/target instead of left/right (for use with `/O`). |
| `/S=C` | Start "Compare by content" without the TC main window: `totalcmd.exe /S=C c:\file1 d:\file2`. |
| `/S=F` | Open "Find Files" without the main window: `/S=F "c:\start path"`. Sub-parameters after a colon: `S` = sort by `1`name/`2`extension/`3`size/`4`date (negative = descending, `S*` = take sort order from the active TC), then as **last** parameter `L<x>` = load search `<x>`, `L=<x>` = load and start immediately, `L==<x>` = feed results to listbox when done. Example `/S=F:S1Lname`. |
| `/S=L` | Start Lister directly on a file (full path required; may include an HTML bookmark `file.html#bm`). Sub-parameters after a colon: `A` ANSI, `S` ASCII/DOS, `V` variable width, `F` full screen, `T1..T8` view mode (1 text, 2 binary, 3 hex, 4 multimedia, 5 HTML, 6 Unicode, 7 UTF-8, 8 Explorer Preview), `C<nr>` codepage (e.g. `C1251`), `N` auto-detect with no multimedia/plugins, and as **last** parameter `P<x>` = choose plugin. Example `/S=L:AT1C1250`. |
| `/S=P:PORT` | Start a parallel or USB port server for direct cable connection (`LPT1` or `USB`). |
| `/S=S` | Start "Synchronize dirs" with two paths, or with a settings name in the form `/S=S:SettingsName`. A leading `=` in the parameter (e.g. `/S=S:=SettingsName`) starts the comparison immediately; a lone `=` starts with the passed directories and last used options. |
| `/O=list_name` | Only with `/S=S:` — pass a list file for "Only selected (in main window)", e.g. `/O=%F`. |
| `/T` | Open the passed dir(s) in new tab(s); works even if TC wasn't running. |
| `/B` | Like `/T`, but the tab is opened in the background when TC is already running. |
| `/M=` | Dark mode: `0` light, `1` auto (like Windows 10 apps), `2` dark. |
| `/P=` | Active panel at start: `/P=L` left, `/P=R` right; overrides `ActiveRight=`. |
| `/i=name.ini` | Use `name.ini` instead of `wincmd.ini`. |
| `/F=ftpname.ini` | Use `ftpname.ini` instead of `wcx_ftp.ini` for the built-in FTP client. |
| `/d=<x>` | Delay the start of TC by `<x>` seconds. |
| `/INSTALLDRIVER` | (NT-based Windows) install the parallel-port driver; must run as administrator. |
| `/INSTALLDRIVERQ` | Same, silent. |
| `/REMOVEDRIVER` | Remove the driver installed by `/INSTALLDRIVER`. |
| `/REMOVEDRIVERQ` | Same, silent. |

Documented examples: `totalcmd.exe c:\windows`; `totalcmd.exe d:\data c:\programs`;
`totalcmd.exe /R=d:\data`; `totalcmd.exe "d:\Letters to Mr. Smith"`;
`totalcmd.exe c:\data\backup.zip` (opens the archive in the left window);
`totalcmd.exe /I=Miller.ini /F=MFtp.ini`; `totalcmd.exe /O /L=c:\ /R="d:\doc"`;
`totalcmd.exe /O /S /R=c:\somepath` (opens in the current **target** panel);
`totalcmd.exe /O /T /L=c:\somepath` (new left tab in the running instance).

Platform note: `/O`, `/N`, `/i=`, `/F=`, `/T`, `/B`, `/M=`, `/P=`, `/L=`, `/R=`, `/P`, `/A`, `/S`,
`/S=F`, `/S=L`, `/S=S`, `/d=` are meaningful on any platform (argument parsing + IPC to a running
instance). `/INSTALLDRIVER*`, `/REMOVEDRIVER*`, `/S=P:` and the `::%GUID%`/`shell:` path forms are
Windows-only.

## 4. Internal command catalogue (594 entries from `TOTALCMD.INC`)

Grouped by the `section` field of `internal-commands.json`; the "meaning" is the `description`
field with only the redundant section prefix (`Source:`, `Left:`, `Right:`, `Target:`, `Conf:`)
removed. Platform marks: **core** = required behaviour, **opt** = documented feature a clone may
defer, **[win]** = Windows API/shell dependency (stub or omit on Linux/Android). Numbered families
(e.g. `cm_SrcActivateTab1` … `cm_SrcActivateTab10`) are collapsed; the help text states they run
"up to 99 items" / "until 299" / "until 8749" respectively.

### 4.0 Panel-scoped display/sort/view commands (Source 22 + Left 24 + Right 24 = 70)

The panel-scoped families are exact mirrors: the `Source` set uses the base ids, the `Left` set
uses `base-200` and the `Right` set `base-100`; the two branch-view entries exist only for
`Left`/`Right`. A command without the prefix (`cm_Comments`, `cm_Short`, …) is not present in the
catalogue, so "source" is the implicit default *(inferred)*.

| Source (id) | Left (id) | Right (id) | Meaning | Platform |
|:--|:--|:--|:--|:--|
| `cm_SrcComments` (300) | `cm_LeftComments` (100) | `cm_RightComments` (200) | Show comments | core |
| `cm_SrcShort` (301) | `cm_LeftShort` (101) | `cm_RightShort` (201) | Only file names | core |
| `cm_SrcLong` (302) | `cm_LeftLong` (102) | `cm_RightLong` (202) | All file details | core |
| `cm_SrcTree` (303) | `cm_LeftTree` (103) | `cm_RightTree` (203) | Directory tree | core |
| `cm_SrcQuickView` (304) | `cm_LeftQuickView` (104) | `cm_RightQuickView` (204) | Quick view panel | core |
| `cm_SrcQuickInternalOnly` (306) | `cm_LeftQuickInternalOnly` (106) | `cm_RightQuickInternalOnly` (206) | Quick view, no plugins | core |
| `cm_SrcHideQuickView` (307) | `cm_LeftHideQuickView` (107) | `cm_RightHideQuickView` (207) | Quick view panel off | core |
| `cm_SrcExecs` (311) | `cm_LeftExecs` (111) | `cm_RightExecs` (211) | Only programs | core |
| `cm_SrcAllFiles` (312) | `cm_LeftAllFiles` (112) | `cm_RightAllFiles` (212) | All files | core |
| `cm_SrcUserSpec` (313) | `cm_LeftUserSpec` (113) | `cm_RightUserSpec` (213) | Last selected | core |
| `cm_SrcUserDef` (314) | `cm_LeftUserDef` (114) | `cm_RightUserDef` (214) | Select user type | core |
| `cm_SrcByName` (321) | `cm_LeftByName` (121) | `cm_RightByName` (221) | Sort by name | core |
| `cm_SrcByExt` (322) | `cm_LeftByExt` (122) | `cm_RightByExt` (222) | Sort by extension | core |
| `cm_SrcBySize` (323) | `cm_LeftBySize` (123) | `cm_RightBySize` (223) | Sort by size | core |
| `cm_SrcByDateTime` (324) | `cm_LeftByDateTime` (124) | `cm_RightByDateTime` (224) | Sort by date | core |
| `cm_SrcUnsorted` (325) | `cm_LeftUnsorted` (125) | `cm_RightUnsorted` (225) | Unsorted | core |
| `cm_SrcNegOrder` (330) | `cm_LeftNegOrder` (130) | `cm_RightNegOrder` (230) | Reversed order | core |
| `cm_SrcOpenDrives` (331) | `cm_LeftOpenDrives` (131) | `cm_RightOpenDrives` (231) | Open drive list | win |
| `cm_SrcThumbs` (269) | `cm_LeftThumbs` (69) | `cm_RightThumbs` (169) | Thumbnail view | core |
| `cm_SrcCustomViewMenu` (270) | `cm_LeftCustomViewMenu` (70) | `cm_RightCustomViewMenu` (170) | Custom view menu | core |
| `cm_SrcPathFocus` (332) | `cm_LeftPathFocus` (132) | `cm_RightPathFocus` (232) | Put focus on path | core |
| `cm_SrcViewModeList` (333) | `cm_LeftViewModeList` (133) | `cm_RightViewModeList` (233) | View mode menu | core |
| — | `cm_LeftDirBranch` (2034) | `cm_RightDirBranch` (2035) | Branch view | core |
| — | `cm_LeftDirBranchSel` (2047) | `cm_RightDirBranchSel` (2048) | Branch view, only selected | core |
### 4.1 File Operations (37)

`cm_List` (903) View with Lister · `cm_ListInternalOnly` (1006) Lister without plugins/multimedia, single file
`cm_ListInternalMulti` (2933) Lister without plugins/multimedia, selected · `cm_ListOnly` (2934) Lister with plugins/multimedia, single file
`cm_ListMulti` (2935) Lister with plugins/multimedia, selected · `cm_ListExternal` (3026) Use external file viewer
`cm_Edit` (904) Edit (Notepad) · `cm_EditNewFile` (2931) Create+Open new text file
`cm_EditFileMenu` (2943) Show 'New' menu from context menu · `cm_EditExistingFile` (2932) Edit file under cursor (Shift ignored)
`cm_Copy` (905) Copy files · `cm_CopySamePanel` (3100) Copy within panel
`cm_CopyOtherPanel` (3101) Copy to other · `cm_RenMov` (906) Rename/Move files
`cm_MkDir` (907) Make directory · `cm_MkDirOther` (911) Make directory in target panel
`cm_Delete` (908) Delete files · `cm_TestArchive` (518) Test selected archives
`cm_PackFiles` (508) Pack files · `cm_CopyToArchive` (2955) Pack files (copy)
`cm_MoveToArchive` (2956) Pack files (move) · `cm_UnpackFiles` (509) Unpack all
`cm_RenameOnly` (1002) Rename (Shift+F6) · `cm_RenameSingleFile` (1007) Rename file under cursor
`cm_MoveOnly` (1005) Move (F6) · `cm_Properties` (1003) Properties dialog
`cm_ModernShare` (1008) Windows 10 modern share dialog **[win]** · `cm_CreateShortcut` (1004) Create a shortcut **[win]**
`cm_Return` (1001) Simulate: Return pressed · `cm_OpenAsUser` (2800) Open program under cursor as different user **[win]**
`cm_Split` (560) Split file into pieces **[opt]** · `cm_Combine` (561) Combine partial files **[opt]**
`cm_Encode` (562) Encode MIME/UUE/XXE **[opt]** · `cm_Decode` (563) Decode MIME/UUE/XXE/BinHex **[opt]**
`cm_CRCcreate` (564) Create CRC checksums **[opt]** · `cm_CRCcheck` (565) Verify CRC checksums **[opt]**
`cm_SetAttrib` (502) Change attributes

### 4.2 Mark (46)

`cm_SpreadSelection` (521) Select group · `cm_SpreadSelectionCurrentExt` (546) Select group, pre-load extension under cursor
`cm_SelectBoth` (3311) Select group: files+folders · `cm_SelectFiles` (3312) Select group: just files
`cm_SelectFolders` (3313) Select group: just folders · `cm_ShrinkSelection` (522) Unselect group
`cm_ShrinkSelectionCurrentExt` (547) Unselect group, pre-load extension under cursor · `cm_ClearFiles` (3314) Unselect group: just files
`cm_ClearFolders` (3315) Unselect group: just folders · `cm_ClearSelCfg` (3316) Unselect group (files or both, as configured)
`cm_SelectAll` (523) Select all (files or both, as configured) · `cm_SelectAllBoth` (3301) Select both files+folders
`cm_SelectAllFiles` (3302) Select all files · `cm_SelectAllFolders` (3303) Select all folders
`cm_ClearAll` (524) Unselect all (files+folders) · `cm_ClearAllFiles` (3304) Unselect all files
`cm_ClearAllFolders` (3305) Unselect all folders · `cm_ClearAllCfg` (3306) Unselect all (files or both, as configured)
`cm_ExchangeSelection` (525) Invert selection · `cm_ExchangeSelBoth` (3321) Invert selection (files+folders)
`cm_ExchangeSelFiles` (3322) Invert selection (files) · `cm_ExchangeSelFolders` (3323) Invert selection (folders)
`cm_SelectCurrentExtension` (527) Select all files with same ext. · `cm_UnselectCurrentExtension` (528) Unselect all files with same ext.
`cm_SelectCurrentName` (541) Select all files with same name · `cm_UnselectCurrentName` (542) Unselect all files with same name
`cm_SelectCurrentNameExt` (543) Select all files with same name+ext. · `cm_UnselectCurrentNameExt` (544) Unselect all files with same name+ext.
`cm_SelectCurrentPath` (537) Select all in same path (for branch view+search) · `cm_UnselectCurrentPath` (538) Unselect all in same path
`cm_RestoreSelection` (529) Selection before last operation · `cm_SaveSelection` (530) Temporarily save selection
`cm_SaveSelectionToFile` (2031) Save file selection to file · `cm_SaveSelectionToFileA` (2041) Save file selection to file (ANSI)
`cm_SaveSelectionToFileW` (2042) Save file selection to file (Unicode) · `cm_SaveDetailsToFile` (2039) Save all shown columns to file
`cm_SaveDetailsToFileA` (2043) Save all shown columns to file (ANSI) · `cm_SaveDetailsToFileW` (2044) Save all shown columns to file (Unicode)
`cm_SaveHdrDetailsToFile` (2093) Save all shown columns to file, with headers · `cm_SaveHdrDetailsToFileA` (2094) Save all shown columns to file, with headers (ANSI)
`cm_SaveHdrDetailsToFileW` (2095) Save all shown columns to file, with headers (Unicode) · `cm_LoadSelectionFromFile` (2032) Read file selection from file
`cm_LoadSelectionFromClip` (2033) Read file selection from clipboard · `cm_Select` (2936) Select file under cursor, go to next
`cm_UnSelect` (2937) Remove selection of file, go to next · `cm_Reverse` (2938) Reverse selection of file, go to next

### 4.3 Clipboard (15)

`cm_CutToClipboard` (2007) Cut selected files to clipboard · `cm_CopyToClipboard` (2008) Copy selected files to clipboard
`cm_PasteFromClipboard` (2009) Paste from clipboard to current dir · `cm_CopyNamesToClip` (2017) Copy filenames to clipboard
`cm_CopyFullNamesToClip` (2018) Copy names with full path · `cm_CopyNetNamesToClip` (2021) Copy names with UNC path
`cm_CopyPathOfFilesToClip` (3035) Copy path name of each file to clipboard · `cm_CopySrcPathToClip` (2029) Copy source path to clipboard
`cm_CopyTrgPathToClip` (2030) Copy target path to clipboard · `cm_CopyFileDetailsToClip` (2036) Copy all shown columns
`cm_CopyFpFileDetailsToClip` (2037) Copy all columns, with full path · `cm_CopyNetFileDetailsToClip` (2038) Copy all columns, with UNC path
`cm_CopyHdrFileDetailsToClip` (2090) Copy all shown columns, with headers · `cm_CopyHdrFpFileDetailsToClip` (2091) Copy all columns, with headers+full path
`cm_CopyHdrNetFileDetailsToClip` (2092) Copy all columns, with headers+UNC path

### 4.4 Navigation (37)

`cm_GoToPreviousDir` (570) Go back · `cm_GoToNextDir` (571) Go forward
`cm_DirectoryHistory` (572) History list · `cm_DirectoryHistoryNoThinning` (575) History list, unfiltered
`cm_GoToPreviousLocalDir` (573) Go back, no ftp · `cm_GoToNextLocalDir` (574) Go forward, no ftp
`cm_DirectoryHotlist` (526) Directory popup menu · `cm_GoToRoot` (2001) Go to root directory
`cm_GoToParent` (2002) Go to parent directory · `cm_GoToDir` (2003) Open dir or zip under cursor
`cm_OpenDesktop` (2121) Desktop folder **[win]** · `cm_OpenDrives` (2122) My computer **[win]**
`cm_OpenControls` (2123) Control panel **[win]** · `cm_OpenFonts` (2124) Fonts folder **[win]**
`cm_OpenNetwork` (2125) Network neighborhood **[win]** · `cm_OpenPrinters` (2126) Printers folder **[win]**
`cm_OpenRecycled` (2127) Recycle bin **[win]** · `cm_CDtree` (500) Popup directory tree
`cm_TransferLeft` (2024) Transfer dir under cursor to left window · `cm_TransferRight` (2025) Transfer dir under cursor to right window
`cm_EditPath` (2912) Edit path field above file list · `cm_GoToFirstEntry` (2049) Place cursor on first folder or file
`cm_GoToFirstFile` (2050) Place cursor on first file in list · `cm_GoToNextDrive` (2051) Go one drive up (C->D)
`cm_GoToPreviousDrive` (2052) Go one drive down · `cm_GoToNextSelected` (2053) Go to next selected file
`cm_GoToPrevSelected` (2054) Go to previous selected file · `cm_GoToNext` (2055) Go to next file
`cm_GoToPrev` (2056) Go to previous file · `cm_GoToLast` (2057) Go to last file
`cm_GoToDriveA` (2061) Switch to drive A · `cm_GoToDriveC` (2063) Switch to drive C
`cm_GoToDriveD` (2064) Switch to drive D · `cm_GoToDriveE` (2065) Switch to drive E
`cm_GoToDriveF` (2066) (etc, define your own if) · `cm_GoToDriveZ` (2086) (you need more drives)
`cm_OpenDriveByIndex` (3025) Open 1st, 2nd etc. displayed drive (in button bar or alternatively in combobox)

### 4.5 Command Line (7)

`cm_ClearCmdLine` (2004) Clear command line · `cm_NextCommand` (2005) Next command line
`cm_PrevCommand` (2006) Previous command line · `cm_AddPathToCmdline` (2019) Copy path to command line
`cm_AddFileNameToCmdline` (3021) Copy file name only to command line · `cm_AddPathAndFileNameToCmdline` (3022) Copy path+file name to command line
`cm_ShowCmdLineHistory` (3023) Open/close command line history

### 4.6 View (51)

`cm_VisButtonBar` (2901) Show/hide button bar · `cm_VisButtonBar2` (2944) Show/hide vertical button bar
`cm_VisDriveButtons` (2902) Show/hide drive button bars · `cm_VisTwoDriveButtons` (2903) Show/hide two drive bars
`cm_VisFlatDriveButtons` (2904) Buttons: Flat/normal mode · `cm_VisFlatInterface` (2905) Interface: Flat/normal mode
`cm_VisDriveCombo` (2906) Show/hide drive combobox · `cm_VisCurDir` (2907) Show/hide current directory
`cm_VisBreadCrumbs` (2926) Show/hide Breadcrumb bar · `cm_VisTabHeader` (2908) Show/hide tab header (sorting)
`cm_VisStatusBar` (2909) Show/hide status bar · `cm_VisCmdLine` (2910) Show/hide Command line
`cm_VisKeyButtons` (2911) Show/hide function key buttons · `cm_ShowHint` (2914) Show file tip window
`cm_ShowQuickSearch` (2915) Show name search window · `cm_QuickSearch` (3018) Show name search window, quick filter off
`cm_QuickFilter` (3019) Show name search window, quick filter on · `cm_VerticalPanels` (305) File windows above each other
`cm_SwitchLongNames` (2010) Turn long names on and off **[win]** · `cm_RereadSource` (540) Reread source
`cm_ShowOnlySelected` (2023) Hide files which aren't selected · `cm_SwitchHidSys` (2011) Turn hidden/system files on and off
`cm_SwitchHid` (3013) Turn hidden files on and off · `cm_SwitchSys` (3014) Turn system files on and off
`cm_Switch83Names` (2013) Turn 8.3 names lowercase on/off **[win]** · `cm_SwitchDirSort` (2012) Turn directory sorting by name on/off
`cm_DirBranch` (2026) Show all files in current dir and all subdirs · `cm_DirBranchSel` (2046) Show selected files, and all in selected subdirs
`cm_50Percent` (909) Window separator at 50% · `cm_100Percent` (910) Window separator at 100%
`cm_VisDirTabs` (2916) Show/hide folder tabs · `cm_VisXPthemeBackground` (2923) Show/hide XP theme background **[win]**
`cm_SwitchOverlayIcons` (2917) Switch icon overlays on/off **[win]** · `cm_VisHistHotButtons` (2919) Show/hide dir history+hotlist
`cm_SwitchWatchDirs` (2921) Enable/disable WatchDirs auto-refresh temporarily **[win]** · `cm_SwitchIgnoreList` (2922) Enable/disable ignore list file to not show file names
`cm_SwitchX64Redirection` (2925) 64-bit Windows: Redirect 32-bit system32 dir off/on **[win]** · `cm_SeparateTreeOff` (3200) Disable separate tree panel
`cm_SeparateTree1` (3201) One separate tree panel · `cm_SeparateTree2` (3202) Two separate tree panels
`cm_SwitchSeparateTree` (3203) Switch through tree panel options · `cm_ToggleSeparateTree1` (3204) One separate tree panel on/off
`cm_ToggleSeparateTree2` (3205) Two separate tree panels on/off · `cm_ChangeArchiveEncoding` (2948) Show popup menu to change archive name encoding
`cm_SwitchDarkMode` (2950) Turn dark mode on and off · `cm_EnableDarkMode` (2951) Turn dark mode on
`cm_DisableDarkMode` (2952) Turn dark mode off · `cm_SwitchColorsByFileType` (2957) Enable/disable color filters
`cm_SwitchFileTipWindows` (2959) Enable/disable file tooltips · `cm_ZoomIn` (2953) Enlarge thumbnails (max 200%)
`cm_ZoomOut` (2954) Make thumbnails smaller (min 10%)

### 4.7 Sorting (44)

`cm_SrcSortByCol1`…`cm_SrcSortByCol99` (6001–6099) Sort by first column (family) · `cm_TrgSortByCol1`…`cm_TrgSortByCol99` (6101–6199) Sort by first column (family)
`cm_LeftSortByCol1`…`cm_LeftSortByCol99` (6201–6299) Sort by first column (family) · `cm_RightSortByCol1`…`cm_RightSortByCol99` (6301–6399) Sort by first column (family)

### 4.8 Custom Column Views (49)

`cm_SrcCustomView1`…`cm_SrcCustomView9` (271–279) Custom columns 1 (family) · `cm_TrgCustomView1`…`cm_TrgCustomView9` (421–429) Custom columns 1 (family)
`cm_LeftCustomView1`…`cm_LeftCustomView9` (71–79) Custom columns 1 (family) · `cm_RightCustomView1`…`cm_RightCustomView9` (171–179) Custom columns 1 (family)
`cm_SrcNextCustomView` (5501) Next custom view · `cm_SrcPrevCustomView` (5502) Previous custom view
`cm_TrgNextCustomView` (5503) Next custom view · `cm_TrgPrevCustomView` (5504) Previous custom view
`cm_LeftNextCustomView` (5505) Next custom view · `cm_LeftPrevCustomView` (5506) Previous custom view
`cm_RightNextCustomView` (5507) Next custom view · `cm_RightPrevCustomView` (5508) Previous custom view
`cm_LoadAllOnDemandFields` (5512) Load on demand fields for all files · `cm_LoadSelOnDemandFields` (5513) Load on demand fields for selected files
`cm_ContentStopLoadFields` (5514) Stop loading on demand fields · `cm_LeftSwitchToThisCustomView` (5510) For scripting, lparam=view
`cm_RightSwitchToThisCustomView` (5511) For scripting, lparam=view

### 4.9 View Modes (41)

`cm_ToggleAutoViewModeSwitch` (2947) Turn automatic view mode switching on/off · `cm_SrcViewMode1`…`cm_SrcViewMode9` (8500–8509) Default view mode (no colors or icons) (family)
`cm_TrgViewMode1`…`cm_TrgViewMode9` (8750–8759) Default view mode (no colors or icons) (family) · `cm_LeftViewMode1`…`cm_LeftViewMode9` (8000–8009) Default view mode (no colors or icons) (family)
`cm_RightViewMode1`…`cm_RightViewMode9` (8250–8259) Default view mode (no colors or icons) (family)

### 4.10 Tabs (69)

`cm_OpenNewTab` (3001) Open new tab · `cm_OpenNewTabBg` (3002) Open new tab in background
`cm_OpenNewTabOther` (3027) Open new tab (other window) · `cm_OpenNewTabBgOther` (3028) Open new tab in background (other window)
`cm_OpenDirInNewTab` (3003) Open dir under cursor in tab · `cm_OpenDirInNewTabOther` (3004) Open dir under cursor (other window)
`cm_SwitchToNextTab` (3005) Switch to next Tab (as Ctrl+Tab) · `cm_SwitchToPreviousTab` (3006) Switch to previous Tab (Ctrl+Shift+Tab)
`cm_MoveTabLeft` (3015) Move current tab to the left · `cm_MoveTabRight` (3016) Move current tab to the right
`cm_CloseCurrentTab` (3007) Close tab · `cm_CloseAllTabs` (3008) Close all
`cm_CloseDuplicateTabs` (3017) Close duplicate tabs · `cm_DirTabsShowMenu` (3009) Show tab menu
`cm_RenameTab` (3024) Rename current tab · `cm_ToggleLockCurrentTab` (3010) Turn on/off tab locking
`cm_ToggleLockDcaCurrentTab` (3012) Same but with dir changes allowed · `cm_SetTabOptionNormal` (3029) Turn off tab locking
`cm_SetTabOptionPathLocked` (3030) Turn on tab locking · `cm_SetTabOptionPathResets` (3031) Turn on tab locking with dir changes allowed
`cm_SetAllTabsOptionNormal` (3032) Turn off tab locking for all tabs · `cm_SetAllTabsOptionPathLocked` (3033) Turn on tab locking for all tabs
`cm_SetAllTabsOptionPathResets` (3034) Turn on tab locking with dir changes allowed for all tabs · `cm_ExchangeWithTabs` (535) Swap all Tabs
`cm_GoToLockedDir` (3011) Go to the base dir of locked tab · `cm_SrcTabsList` (5515) Show list of all open tabs
`cm_TrgTabsList` (5516) Show list of all open tabs · `cm_LeftTabsList` (5517) Show list of all open tabs
`cm_RightTabsList` (5518) Show list of all open tabs · `cm_SrcActivateTab1`…`cm_SrcActivateTab10` (5001–5010) Activate first tab (family)
`cm_TrgActivateTab1`…`cm_TrgActivateTab10` (5101–5110) Activate first tab (family) · `cm_LeftActivateTab1`…`cm_LeftActivateTab10` (5201–5210) Activate first tab (family)
`cm_RightActivateTab1`…`cm_RightActivateTab10` (5301–5310) Activate first tab (family)

### 4.11 Tools (15)

`cm_MultiRenameFiles` (2400) Rename multiple files · `cm_SysInfo` (506) System information
`cm_OpenTransferManager` (559) Background transfer manager · `cm_SearchFor` (501) Search for
`cm_SearchForInCurDir` (517) Search in directory under the cursor · `cm_SearchStandalone` (545) Search in separate process
`cm_FileSync` (2020) Synchronize directories · `cm_Associate` (507) Associate **[win]**
`cm_InternalAssociate` (519) Define internal associations · `cm_CompareFilesByContent` (2022) File comparison
`cm_IntCompareFilesByContent` (2040) Use internal compare tool · `cm_CommandBrowser` (2924) Browse internal commands
`cm_SeparateQuickView` (2941) Separate quick view window · `cm_SeparateQuickInternalOnly` (2942) Separate quick view, no plugins
`cm_UpdateQuickView` (2946) Reload file in current quick view

### 4.12 Ftp (9)

`cm_FtpConnect` (550) Connect to FTP **[opt]** · `cm_FtpNew` (551) New FTP connection **[opt]**
`cm_FtpDisconnect` (552) Disconnect from FTP **[opt]** · `cm_FtpHiddenFiles` (553) Show hidden FTP files **[opt]**
`cm_FtpAbort` (554) Abort current FTP command **[opt]** · `cm_FtpResumeDownload` (555) Resume aborted download **[opt]**
`cm_FtpSelectTransferMode` (556) Select Binary, ASCII or Auto mode **[opt]** · `cm_FtpAddToList` (557) Add selected files to download list **[opt]**
`cm_FtpDownloadList` (558) Download files in download list **[opt]**

### 4.13 Network (6)

`cm_NetConnect` (512) Network connections **[win]** · `cm_NetDisconnect` (513) Disconnect network drives **[win]**
`cm_NetShareDir` (514) Share directory **[win]** · `cm_NetUnshareDir` (515) Unshare directory **[win]**
`cm_AdministerServer` (2204) Connect to admin share to open \\server\c$ etc. **[win]** · `cm_ShowFileUser` (2203) Which remote user has opened a local file **[win]**

### 4.14 Security (3)

`cm_EditPermissionInfo` (2200) Permissions dialog (NTFS) **[win]** · `cm_EditAuditInfo` (2201) File auditing (NTFS) **[win]**
`cm_EditOwnerInfo` (2202) Take ownership (NTFS) **[win]**

### 4.15 Parallel Port (3)

`cm_DirectCableConnect` (2300) Connect to other PC by cable **[win]** · `cm_NTinstallDriver` (2301) Install parallel port driver on Windows NT..11 **[win]**
`cm_NTremoveDriver` (2302) Remove parallel port driver on Windows NT..11 **[win]**

### 4.16 Print (3)

`cm_PrintDir` (2027) Print current directory (with preview) **[opt]** · `cm_PrintDirSub` (2028) Print dir with subdirs **[opt]**
`cm_PrintFile` (504) Print file **[opt]**

### 4.17 Window (4)

`cm_Exit` (24340) Exit Total Commander · `cm_Minimize` (2000) Minimize Total Commander
`cm_Maximize` (2015) Maximize Total Commander · `cm_Restore` (2016) Restore normal size

### 4.18 Configuration (36)

`cm_Config` (490) Layout (first page) · `cm_LayoutConfig` (476) Layout
`cm_DisplayConfig` (486) Display · `cm_IconConfig` (477) Icons
`cm_FontConfig` (492) Font · `cm_ColorConfig` (494) Colors
`cm_ConfTabChange` (497) Tabstops/Data formats · `cm_DirTabsConfig` (488) Folder tabs
`cm_CustomColumnConfig` (483) Custom columns · `cm_CustomColumnDlg` (2920) Change current custom columns
`cm_ConfigViewModes` (2939) View Modes · `cm_ConfigViewModeSwitch` (2940) Auto-switch view modes
`cm_LanguageConfig` (499) Language · `cm_Config2` (516) Operation
`cm_EditConfig` (496) Viewer/Editor · `cm_ConfigLister` (3036) Lister
`cm_CopyConfig` (487) Copy/Delete · `cm_RefreshConfig` (478) Refresh file lists
`cm_QuickSearchConfig` (479) Quick Search · `cm_FtpConfig` (489) FTP options
`cm_PluginsConfig` (484) Plugins · `cm_ThumbnailsConfig` (482) Thumbnails
`cm_LogConfig` (481) Log file · `cm_IgnoreConfig` (480) Ignore list
`cm_HistoryConfig` (475) History · `cm_PackerConfig` (491) Packer
`cm_ZipPackerConfig` (485) ZIP packer · `cm_7ZipPackerConfig` (474) 7-ZIP packer
`cm_Confirmation` (495) Misc, Confirmation · `cm_ConfigSavePos` (493) Save position
`cm_ButtonConfig` (498) Button bar · `cm_ButtonConfig2` (583) Button bar 2 (vertical)
`cm_ConfigSaveSettings` (580) Save current paths etc. · `cm_ConfigChangeIniFiles` (581) Open ini files in notepad
`cm_ConfigSaveDirHistory` (582) Save directory history · `cm_ChangeStartMenu` (700) Change Start menu

### 4.19 Help (6)

`cm_HelpIndex` (610) Help index · `cm_Keyboard` (620) Keyboard help
`cm_Register` (630) Registration info **[win]** · `cm_VisitHomepage` (640) Visit https://www.ghisler.com/ **[win]**
`cm_CheckForUpdates` (650) Check for new versions **[win]** · `cm_About` (690) Help/About Total Commander

### 4.20 Misc (33)

`cm_GetFileSpace` (503) Calculate space · `cm_VolumeId` (505) Volume label **[win]**
`cm_VersionInfo` (510) Version information **[win]** · `cm_ExecuteDOS` (511) Open command prompt window **[win]**
`cm_CompareDirs` (533) Compare dirs · `cm_CompareDirsWithSubdirs` (536) Also mark subdirs not present in other dir
`cm_ContextMenu` (2500) Show context menu · `cm_DriveContextMenu` (3020) Show drive context menu
`cm_ContextMenuInternal` (2927) Show context menu for internal associations · `cm_ContextMenuInternalCursor` (2928) Internal context menu for file under cursor
`cm_ShowRemoteMenu` (2930) Context menu for Media Center remote control Play/Pause **[win]** · `cm_SyncChangeDir` (2600) Synchronous directory changing in both windows
`cm_EditComment` (2700) Edit file comment · `cm_FocusLeft` (4001) Focus on left file list
`cm_FocusRight` (4002) Focus on right file list · `cm_FocusSrc` (4005) Focus on source file list
`cm_FocusTrg` (4006) Focus on target file list · `cm_FocusCmdLine` (4003) Focus on command line
`cm_FocusButtonBar` (4004) Focus on button bar · `cm_FocusLeftTree` (4007) Focus on left separate tree
`cm_FocusRightTree` (4008) Focus on right separate tree · `cm_FocusSrcTree` (4009) Focus on source separate tree
`cm_FocusTrgTree` (4010) Focus on target separate tree · `cm_FocusMainMenu` (4011) Focus on main menu
`cm_FocusButtonBarVertical` (4012) Focus on vertical button bar · `cm_CountDirContent` (2014) Calculate space occupied by subdirs in current dir
`cm_UnloadPlugins` (2913) Unload all plugins · `cm_DirMatch` (534) Mark newer
`cm_Exchange` (531) Swap panels · `cm_MatchSrc` (532) target=Source
`cm_ReloadSelThumbs` (2918) Re-load selected thumbnails · `cm_ReloadBarIcons` (2945) Force reload icons in button bars and main menu
`cm_ReloadFileIcons` (2958) Re-load all file list icons and overlays

### 4.21 User (10)

`cm_UserMenu1`…`cm_UserMenu10` (701–710) Start first menu item in Start menu (family)

### 4.22 Platform classification summary

Totals above: **537 core**, **18 opt**, **39 win**.

`win` (stub or omit on Linux/Android): shell virtual folders `cm_OpenDesktop`, `cm_OpenDrives`,
`cm_OpenControls`, `cm_OpenFonts`, `cm_OpenNetwork`, `cm_OpenPrinters`, `cm_OpenRecycled` and the
drive-list openers (`cm_SrcOpenDrives`/`cm_LeftOpenDrives`/`cm_RightOpenDrives`); Windows networking
`cm_NetConnect`, `cm_NetDisconnect`, `cm_NetShareDir`, `cm_NetUnshareDir`, `cm_AdministerServer`,
`cm_ShowFileUser`, `cm_ModernShare`; NTFS/Windows security `cm_EditPermissionInfo`,
`cm_EditAuditInfo`, `cm_EditOwnerInfo`; shell integration `cm_CreateShortcut` (`.lnk`),
`cm_OpenAsUser`, `cm_Associate`, `cm_ShowRemoteMenu`, `cm_SwitchOverlayIcons`; Windows filename/API
quirks `cm_SwitchLongNames`, `cm_Switch83Names`, `cm_SwitchX64Redirection`,
`cm_VisXPthemeBackground`, `cm_SwitchWatchDirs`; hardware/port `cm_DirectCableConnect`,
`cm_NTinstallDriver`, `cm_NTremoveDriver`; OS housekeeping `cm_VolumeId`, `cm_VersionInfo`,
`cm_ExecuteDOS`; vendor/help `cm_Register`, `cm_VisitHomepage`, `cm_CheckForUpdates`.

`opt`: the built-in FTP client (`cm_FtpConnect` … `cm_FtpDownloadList`), file utilities `cm_Split`,
`cm_Combine`, `cm_Encode`, `cm_Decode`, `cm_CRCcreate`, `cm_CRCcheck`, and printing `cm_PrintDir`,
`cm_PrintDirSub`, `cm_PrintFile`.

Id ranges reveal the internal layout *(inferred from the values)*: `1xx` left panel, `2xx` right
panel, `3xx` source panel, `5xx` file ops/mark/tools, `9xx` file commands, `2xxx` misc & navigation,
`3xxx` newer additions, `4xxx` focus commands, `5xxx` tab activation, `6xxx` sort-by-column,
`8xxx`/`87xx` view modes.

---

## 5. Saved selections, history, and the directory hotlist

### 5.1 Selection persistence (`proc_select.htm`, `options_mark.htm`)

| Facility | Behaviour | Trigger |
|:--|:--|:--|
| Save selection (internal) | Saves the current selection internally; restored by the next menu item. **Every file operation saves the current selection automatically.** | Mark menu → Save selection (`cm_SaveSelection`, 530) |
| Restore selection | Restores the selection that was active before the last file operation | `NUM /`; Mark menu (`cm_RestoreSelection`, 529) |
| Save selection to file | Writes the selected names to a plain text file; `A`/`W` variants force ANSI / UTF-16, otherwise the OS default (NT-based ⇒ Unicode) | `cm_SaveSelectionToFile` 2031 / `…A` 2041 / `…W` 2042 |
| Load selection from file | Restores the selection from a plain text file | `cm_LoadSelectionFromFile` 2032 |
| Load selection from clipboard | Same, reading the names from the clipboard | `cm_LoadSelectionFromClip` 2033 |
| Save details to file | Saves all shown columns (optionally with headers) to a file | `cm_SaveDetailsToFile*` 2039/2043/2044, `cm_SaveHdrDetailsToFile*` 2093/2094/2095 |
| Select / unselect / invert under cursor, then advance | Repeat-friendly selection stepping | `cm_Select` 2936, `cm_UnSelect` 2937, `cm_Reverse` 2938 |

Selection-group rules worth mirroring (from `proc_select.htm`):

- `NUM +` / `NUM -` take a mask; several masks may be given, and a `|` separator introduces
  **exclude** masks (`w*.*|*.bak *.old`, `|*.exe`).
- Masks may be restricted to subdirectories: `\subdir1\subdir2\*.ext` (relative to base dir),
  `subdir1\subdir2\*.ext` (relative to any subdir), and `**` matches any number of subdirs.
- A mask starting with `<` is a **regular expression**.
- Related-file selection: `*.jpg>*.cr2` selects jpg files plus the matching raw file that sits in
  the same directory; more complex schemes allowed (`*.jpg>crw*.cr2`).
- A predefined filter ("define" button) can be used instead of a mask and has the same options as
  the search function; `Shift+Del` removes stale entries from the mask history list.
- When showing duplicate-finder results, a different, richer dialog is used (`dlg_duplicates.htm`).

### 5.2 "Only selected files" and comparisons (`only_selected.htm`, `options_marknewer.htm`, `options_compare.htm`)

- **Only selected files** (`cm_ShowOnlySelected` 2023): hides all files that are not currently
  selected. Return to a normal view by toggling it off, re-reading (`F2`/`CTRL+R`), or changing
  directory.
- **Mark newer / hide same** (`cm_DirMatch` 534): compares source and target directory; files that
  are identical are *hidden only* (never deleted); of the differing files only the **newer** are
  selected; files missing on one side are marked in the other. The paths on both panels get the
  prefix `Comparison: ` while this view is active. Cancelled by switching away and back,
  minimize/restore, `F2`/`CTRL+R`, or changing dir/drive.
- **Compare directories** (`cm_CompareDirs` 533, `cm_CompareDirsWithSubdirs` 536): newer files are
  selected; files missing in one directory are also selected in the other; only *newer* files are
  marked, not all differing files.

### 5.3 Directory history and "go back" (`go_back.htm`, `keyboard.htm`, `dlg_confighistory.htm`, `add_hothist.htm`)

| Aspect | Documented fact |
|:--|:--|
| Depth | TC remembers the **last 50** directories shown, **separately for the left and right panel**. |
| Back / forward | `ALT+left` = back, `ALT+right` = forward (browser-like). Commands `cm_GoToPreviousDir` 570 / `cm_GoToNextDir` 571; `cm_GoToPreviousLocalDir` 573 / `cm_GoToNextLocalDir` 574 skip FTP entries. |
| Open history list | `ALT+down`, or a **one-second right click on the current path**; also the arrow-down button next to the current path. `cm_DirectoryHistory` 572. |
| Unfiltered list | `ALT+SHIFT+down` (`cm_DirectoryHistoryNoThinning` 575) — shows all entries without history thinning. |
| History thinning | A directory is only listed if the user stayed there at least a configured time (default **5000 ms**) or started any file operation there. Configured in Configuration – History. |
| Saved state | History is stored in `wincmd.ini` sections `[LeftHistory]` and `[RightHistory]` (up to 200 items per side); "Maximum to show" caps the visible part. `cm_ConfigSaveDirHistory` 582, `cm_SaveSettings` via `cm_ConfigSaveSettings` 580. |
| Quick search in list | Typing filters the list; a leading `*` searches the **whole path** instead of only the last name part. |
| List context menu | Open in a new tab / in the other panel / in both; remove an entry from history or from the most-frequently-used list; copy path to clipboard; "show all entries" (bypass thinning); reverse order (history top or bottom); show/hide the most-frequently-used list. |
| `SHIFT+ENTER` in the history list | Opens the directory in a **new tab**. |
| Frequently used folders | Stored separately in `tcDirFrq.txt` (default under `Appdata\Local\GHISLER`, or beside `wincmd.ini`, or a user directory). |

### 5.4 Directory hotlist (`directoryhotlist.htm`, `add_hothist.htm`, `dlg_dirmenu.htm`)

| Aspect | Documented fact |
|:--|:--|
| Open | `CTRL+D`; Commands menu; **double click on the title bar of a file window** (the current-path display). Command `cm_DirectoryHotlist` 526. |
| Two built-in entries | **Add/remove current dir** — adds or removes the currently displayed directory; **Configure** — opens a dialog to sort, modify, add and remove entries (built like the start-menu editor). |
| Also opens as history | The `*` button opens the **hotlist**; the arrow-down button opens the **recently visited / frequently used** list. |
| Hotlist editor keys | `CTRL+D` then Configure; in the "Change directory menu" dialog the cursor can be moved with `SHIFT`+`up`/`down` (or drag & drop). `&` in a title underlines the next letter; a single `-` creates a separator; items may be submenus. Entry commands may be `cd Drive:\Directory` (append `\:` or `\\:` to go to the parent and place the cursor on the directory) or any internal command such as `cm_OpenDesktop`; a **Target path** field sets the other window at the same time. |
| Open entry in new tab | Hold `SHIFT` while choosing a hotlist entry. |
