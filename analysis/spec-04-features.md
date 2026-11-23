# Spec 04 — Feature specification

Derived only from the Total Commander 11.58 English help pages extracted from `TOTALCMD.CHM` (`work/e_chm/`). Each feature area gives **purpose**, **user-visible behaviour**, **options**, **algorithm / edge cases**, and **platform notes**. Facts are paraphrased; only short exact quotations are used where wording matters. Anything not stated by the accepted pages is marked *(inferred)*.

Platform-marker convention: **[core]** required for any port, **[win]** tied to a Windows API or shell, **[opt]** documented but deferrable. Each feature area ends with a one-line **Platform** note.

---

## 1. File operations

Sources: `menu_fileoperations.htm`, `proc_copy.htm`, `proc_renmov.htm`, `proc_delete.htm`, `proc_mkdir.htm`, `proc_edit.htm`, `proc_editnew.htm`, `proc_newread.htm`, `proc_show.htm`, `proc_select.htm`, `proc_quit.htm`, `dlg_overwrite.htm`, plus `dlg_configcopydelete.htm`, `dlg_configconfirmation.htm`.

### 1.1 Overview

Purpose: the classic dual-pane core — copy, move/rename, delete, create directory, view, edit, reread, quit — operating on the *selection* in the source panel and a target panel. All operations are reachable by function key and by menu.

| Key | Operation | Dialog/behaviour summary |
|:--|:--|:--|
| `F2` / `CTRL+R` | Reread source directory | Re-reads the source panel **and** the drive bar. No mouse equivalent. `proc_newread.htm` also calls it the command for "source window". |
| `F3` | View | Lister; `ALT` toggles external/internal, `SHIFT` toggles one/many, `ALT+SHIFT+F3` = no plugins/multimedia. |
| `F4` | Edit | Editor configured in the configuration dialog, default Notepad; loads the cursor/selected file. |
| `SHIFT+F4` | New text file | Asks for a name, creates it, opens the editor. |
| `F5` | Copy | Target path + file mask dialog; `SHIFT+F5` copies *within* the same directory (rename-on-copy); `CTRL+SHIFT+F5` creates shortcuts. |
| `F6` | Rename/Move | Target path + file mask; `SHIFT+F6` = in-place rename. |
| `F7` | Make directory | Type the name; `SHIFT+F7` creates it in the target panel. |
| `F8` / `DEL` | Delete | Recycle bin or direct, per configuration; `SHIFT` inverts the configured choice. |
| `ALT+F4` | Exit | Closes TC and saves the current directories to `wincmd.ini`. |

### 1.2 Copy (`F5`, `proc_copy.htm`)

Behaviour:

- Copies files **and whole directories** from the source to another directory.
- The dialog pre-fills the **target window path** with mask `*.*`. Any valid file name including
wildcards is accepted as the target name. **No target path** ⇒ files are copied into the source
directory.
- Pressing `F5` a second time selects only the file name without path; a third time also selects the
extension. `Ctrl+Home` puts the cursor directly in front of the file name (behind the last `\`).
- The target combobox offers all open tabs and target subdirs; `"+ F7"` adds/removes the current
target directory from the list; the directory hotlist can be used to pick a destination; a
`Tree` button picks from a directory tree (a drive letter including `:` may be typed first).
- `F2 Queue` adds the files to the **last opened background transfer manager** — documented as more
efficient for many large files than many parallel background copies.
- **Right-clicking** OK or F2-Queue converts the operation: copy dialog ⇒ move, move dialog ⇒ copy.
- `Copy NTFS permissions` copies permissions and owner (and auditing info if accessible). [win]
- `Verify` — after the copy finishes TC **reads the copied file again** and compares a **BLAKE3**
checksum (MD5 if BLAKE3 is unavailable) with the original, **bypassing the disk cache**.
- `Keep relative paths` (offered when copying from branch view `Ctrl+B`) preserves the directory
structure relative to the current directory.
- `Options` sets unattended defaults: "Overwrite all", "Skip all", "Overwrite all older", savable as
default; also lets read-only, hidden and system attributes be ignored when overwriting or moving.
- If the **target** panel holds selected directories/links, TC offers to copy to **all** of those
locations (via the background transfer manager). If the target panel holds ordinary files (e.g.
after Search → Feed to listbox), TC copies into the directories those files live in — since 11.55
this honours `Keep relative paths` too.
- With "Use copy+paste via Explorer" enabled a **simplified** dialog with only confirm/cancel is used.
[win]
- Copying a file **onto itself** with overwrite confirmations on offers an **Append** button.
- Archives: if the *source* shows an archive, the **unpack** dialog appears instead; if the *target*
shows an archive, the **pack** dialog appears. Unpack archive → FTP is supported (unpack to a
temporary directory, then upload); FTP → archive is **not** supported (risk of losing the
download).

**Only files of this type** filter — syntax and examples (this is the shared filter language used by copy, and partly by search/select):

| Filter | Effect |
|:--|:--|
| `*.txt *.doc` | Only those types — **also applies to files in subdirectories**. |
| `*.* \| *.bak *.old` | Everything except `.bak`/`.old`. |
| `*.* \| somedir1\ somedir2\` | Excludes files in subdirs named `somedir1`, `somedir2`. |
| `*.* \somedir1\` | Current directory **and** direct subfolders named `somedir1`. |
| `*.* \somedir1\**\` | Same, plus all their subfolders. |
| `*.* somedir1\` | Current dir and subfolders **anywhere below** named `somedir1`. |
| `*.* somedir1\**\` | Same, plus all their subfolders. |
| `\| \` appended | Exclude files in the base folder. |
| `\| **\` appended | Exclude directories from which no files were copied (combinable: `\| \ **\`). |
| `*.jpg>*.cr2` | Related files: copy the `.cr2` **only if** a matching `.jpg` is in the same directory. Append `\| **` to skip copying empty folders. |

Selection bookkeeping during a filtered copy (important, non-obvious):

- Files excluded by the filter **stay selected** (so they can be handled later with another filter).
- Files that were **fully copied are deselected**.
- **Folders are deselected even when only partly copied or not copied at all** — because files inside
may have been filtered out, and the folder may already exist in the target. Only folders
completely skipped by the filter (e.g. with `| somedir1\`) stay selected.

`"+ F8"` opens the **search-filter** dialog, giving size/date/content-plugin filtering. When a saved search is used as a copy filter, these fields are **not** used: `Search in`, `Only search in selected directories/files`, `Everything`, `Search archives`, `Search in subdirectories`.

Wildcard rewrite rules for the target name (`?` and `*`):

| Rule | Example |
|:--|:--|
| `?` = character at that position of the old name | `test.txt` → `h?.txt` gives `he.txt` |
| `*` = all characters from that position to the end of the name part | `test.txt` → `f*.txt` gives `fest.txt` |
| `*rest` — text after `*` is appended | `F*Rest.*` → `FestRest.ext` |
| `start**` — text inserted before the name | `start_**_rest.*` → `start_test_rest.txt` |
| `start*?` — text inserted before the name and position reset | `start_*?do*_rest.*` → `start_dome_rest.txt` |

### 1.3 Copy modes: foreground, background, queue

| Mode | How | Documented facts |
|:--|:--|:--|
| Foreground | default | Progress dialog shows copied/total file counts and bytes; clicking the numbers toggles to remaining counts (shown with a minus sign). |
| Background, mid-operation | press the **Background** button *during* the copy | Supported for copy, pack/unpack (ZIP, TAR, RAR, 7Z, external packers, some plugins) and delete. Hovering a name shows the full name. A background operation can be moved **back to the foreground** if no other dialog is open. After completion `F2`/`CTRL+R` may be needed to refresh. |
| Queue (background transfer manager) | `F2 Queue` in the copy dialog; `+` in the transfer manager | Jobs run one after another; the manager has progress bar, overall progress bar, listbox (reorder by drag&drop while paused), `+`/`-`, Pause, **Speed limit** (applies to uploads/downloads *and* local copies). |

### 1.4 Rename/Move (`F6`, `proc_renmov.htm`)

- Renames files **and whole directories** in place, or moves them to other directories/drives, or
moves (pack + delete) into archives.
- Dialog mirrors the copy dialog: target path + mask; F6 pressed again selects name only, third time
also extension; empty target = source directory (pure rename); empty mask = name unchanged.
- `SHIFT+F6` = **in-place** edit window around the file name. Pressing `Shift+F6` again selects only
the name without extension. `ENTER` confirms; **clicking outside cancels**. Setting
`InplaceRename=0` in `[Configuration]` of `wincmd.ini` disables the in-place editor on
second click.
- With multiple files selected, `SHIFT+F6` opens a dialog accepting `*` and `?` wildcards, and
optionally `Rename each file separately` (asks for each new name).
- With a single file, `SHIFT+F6` then allows `up`/`down` to jump to the previous/next file; an edited
name is **renamed** when moving away.
- Renaming a `.ZIP` to `.EXE` asks whether to create a **self-extracting** archive (also for `.7Z`).
- Inside an archive the function is unavailable **except** `Shift+F6` on single files/dirs inside
ZIP.
- "Use copy+paste via Explorer" ⇒ simplified confirm/cancel dialog. [win]
- Large-scale renaming is delegated to the Multi-Rename Tool (`CTRL+M`).

### 1.5 Delete (`F8`/`DEL`, `proc_delete.htm`)

- After confirmation, files are deleted; the process can be interrupted at any time via the Cancel
dialog.
- For **each non-empty directory** an additional confirmation is asked. All files *and directories*
in it will be deleted.
- If supported, files go to the **Recycle Bin**; `SHIFT`+`F8`/`DEL` deletes directly. The
configured default is inverted by `SHIFT`. Recycle-Bin use can be disabled in
Configuration – Copy/Delete (and that option is unavailable when "Use Explorer delete method" is
checked).
- Inside an archive the corresponding **packer** is started to delete the files; they are deleted
directly (no recycle bin).
- The delete operation can be moved to the background during the operation.
- On failure (Windows XP..11) TC asks whether to delete "As Administrator".
- Configuration – Copy/Delete also offers "Use Explorer delete method" — faster, but **cannot handle
file comments**. [win]

### 1.6 Create directory (`F7`, `proc_mkdir.htm`)

- Creates a subdirectory in the **source** directory; `SHIFT+F7` creates it in the **target** folder.
- The name of the file/dir under the cursor is pre-offered as the default name.
- **Multiple subdirectories in one operation**: separate with a backslash
(`directory1\directory2\directory3`), or create several siblings with `|`
(`dir1|dir2|dir3`, or full paths `c:\dir1|c:\dir2|c:\dir3`).
- Unavailable inside archives **except** ZIP, RAR and TAR.
- A name starting with `<` enables **Multi-Rename-Tool placeholders** (`[+]` button inserts them);
date/time placeholders use the time the dialog was opened unless `[T1]`–`[T4]` modifiers are used.
- Ranges: put the range in `<>` at the start and use the `[C]` placeholder. `[C]` accepts an optional
prefix/postfix: `["prefix"C"postfix"]`, in which case the **first** directory is created **without**
an index.
- `[C]` **without** a counter (`<test[C].txt`) produces a name with an optional digit that exists
only in the file system: it creates `test3.txt` when `test.txt` and `test2.txt` already exist
(likewise `test[" ("C")"].txt` → `test.txt`, `test (2).txt`, …).
- `[F]`/`[f]` leave the `[C]` field unchanged when alphanumeric counters are used.

Examples from the page:

| Mask | Result |
|:--|:--|
| `<[Y]-[M]-[D] downloads` | `2017-09-22 downloads` |
| `<Copy of [N]` | `Copy of <name under cursor>` |
| `<1-10>[C]` | `1`, `2` … `10` |
| `<1-250>Project No [C:3]` | `Project No 001` … `Project No 250` |
| `<5-8>Example [C] folder` | `Example 5 folder` … `Example 8 folder` |
| `<1-30>[Y]-[M]-[C:2] photos` | `2017-09-01 photos` … `2017-09-30 photos` |
| `<1-3>test[" ("C")"]` | `test`, `test (2)`, `test (3)` |

- On failure (XP+) TC asks whether to create the directory "As Administrator". [win]

### 1.7 Edit (`F4`) and new file (`SHIFT+F4`)

- `F4` starts the editor chosen in configuration (default **notepad**, limited to text files of
limited size) with the selected file.
- **Inside an archive**: the file is unpacked to a temporary file and loaded; on editor close TC asks
whether to **repack** it. Yes ⇒ archive updated and temp file erased; No ⇒ temp file erased,
archive unchanged. With a multi-document editor already running TC cannot be notified, so it shows
a small dialog with the file name that must be dismissed to trigger the repack logic.
- `SHIFT+F4` asks for a name, creates the file and opens the editor (unless the editor is explicitly
set to none).
- Multiple files at once **without opening the editor**: separate names with `|`
(`file 1.txt|file 2.txt|file 3.txt`) or with double quotes separated by spaces/vertical bars
(`"file 1.txt" "file 2.txt" "file 3.txt"`, also mixed). **Local files only.**
- `SHIFT+F4` also works in archives, on FTP servers and in file-system plugins: an empty file is
created in the **TEMP** folder, opened in the editor, and after saving TC asks whether to pack it
into the archive or upload it to the server.
- Same `<` placeholder / `[C]` range mechanism as `F7`, with the same examples; when multiple files
are created this way the editor is **not** launched.
- Files of other types: right-click the panel background or current path and use the `New` submenu.

### 1.8 View (`F3`) and `proc_quit.htm`

- `F3` shows the selected files, or the file under the cursor (as configured).
- `SHIFT` selects the alternative of those two; `ALT` switches external/internal; `ALT+SHIFT+F3`
starts Lister with no plugins and no multimedia.
- Inside an archive, files are unpacked automatically to the directory of the environment variable
`temp` (falling back to the TC program directory), into a subdirectory `\_tc`; that directory is
removed on exit if empty.
- `ALT+F4` closes TC and writes the current directories to `wincmd.ini`.

### 1.9 Overwrite dialog (`dlg_overwrite.htm`)

| Element | Meaning |
|:--|:--|
| Overwrite / With file | Details of the target file to be overwritten / of the source file. |
| Thumbnails | Optional previews of both files (same images as thumbnail view); support the normal right-click menu. Useful for pictures. |
| Custom fields | Content-plugin fields, defined via the `+` button. |
| Overwrite | Overwrite just this file. |
| Overwrite all | Overwrite all collisions; per settings TC may still ask separately for read-only or hidden/system files. |
| Skip | Skip this file, continue. |
| Cancel | Abort the whole operation. |
| Overwrite all older | Only newer files are copied. Files are first checked for an **internal version number** (EXEs, DLLs etc.), then the last-modified date is used. |
| Skip all | Skip all files already present in the target. |
| Rename | Manually rename the copied file. |
| Append | Append the source file to the target file. |
| More Options >> | `Compare` (by content), `Rename existing target file`, `Auto-rename copied` (`filename(1).ext`, …), `Auto-rename target files`, `Rename older target files, skip newer`, `Overwrite all older **and of the same age**`, `Copy all larger (overwrite smaller)`, `Copy all smaller (overwrite larger)`. |

Hotkeys: `Alt`+underlined letter; `F3` view target; `SHIFT+F3` view source; `ALT+F3`/`ALT+SHIFT+F3` view without plugins; `CTRL+F3` compare by content; `CTRL+SHIFT+F3` force the internal compare tool. [the hotkey list is a Windows-dialog feature; a clone needs an equivalent mapping]

"Copy As Administrator": on Windows 2000+ TC offers to copy a file as administrator, starting `tcmadmin.exe`, which performs the operation with different/extended rights and terminates after `AdminTimeout`, during which no password is asked again; it does not terminate while active. [win]

### 1.10 Confirmation and copy-engine options (`dlg_configcopydelete.htm`, `dlg_configconfirmation.htm`)

| Option | Documented effect |
|:--|:--|
| Use standard copy method | TC chooses: `ReadFile/WriteFile` on Win9x/ME, `CopyFileEx` on NT-family. [win] |
| Use default method only | Slower but reliable; larger block size speeds it; a larger buffer within one drive than between two drives avoids read/write head jumps. |
| Also use big file copy mode | Bypasses the Windows disk cache; recommended ≥ 10240 k within one disk and 32 k between disks; may slow special drives (USB sticks); some vendor drivers are buggy in this mode. |
| Drive letters on the same physical harddisk | e.g. `CDE,FGH` so intra-disk strategy is used between partitions. |
| Compatibility mode for drives | Per-drive workaround using `CopyFileEx`; `\` = network neighbourhood, `*` = all; auto-detect USB sticks. [win] |
| Use copy+paste via Explorer | Clipboard-based; helps when virus scanners slow normal operations. **Warning:** loses file comments and the extended overwrite/rename options. [win] |
| Copy date/time of directories | Copies the directory's last-modified timestamp (also when unpacking explicitly packed directories). On NTFS a directory timestamp may change when its contents change. |
| Verify after copying (MD5/BLAKE3) | Reads the file back and compares checksum, bypassing the disk cache; slow, for known disk problems; also settable directly in the `F5` dialog; BLAKE3 preferred when available. |
| Use Explorer delete method | Faster delete, cannot handle file comments. [win] |
| `F8/DEL` deletes to recycle bin (SHIFT = directly) | Inverts which modifier deletes directly. No undelete program ships with Windows other than the recycle bin. [win] |
| Confirmations: deleting non-empty directories / overwriting files / overwriting-deleting read-only files / overwriting-deleting hidden-system files / Drag&Drop copies | Each can be enabled separately; "more confirmations mean more security, but also slower working speed"; default is maximum security. |

**Undo:** the accepted pages document **no undo for copy/move/delete**. Undo exists only for (a) the Multi-Rename Tool ("Undo" button, also usable after closing the dialog) and (b) the Compare-by-content editor. Restoring a selection is documented (`NUM /`), deleting is not reversible except through the recycle bin.

**Platform:** [core] copy/move/delete/mkdir/edit/view/reread, the overwrite decision matrix, the filter language, wildcard rewrite rules and `[C]`-range multi-create, verify-after-copy with any strong hash. [win] NTFS permission copying, "As Administrator" operations, recycle bin, Explorer copy/delete methods, `CopyFileEx` engine choice, drive-letter→disk mapping. Linux analogues *(inferred)*: XDG/Android trash, polkit/`sudo` for elevation, `st_dev` for "same physical disk"; no documented undo for copy/move/delete exists.

---

## 2. Selection and marks

Sources: `proc_select.htm`, `options_mark.htm`, `options_marknewer.htm`, `only_selected.htm`, `dlg_duplicates.htm`, `show_files.htm` (mask language used by "Custom...").

### 2.1 Purpose and user-visible behaviour

Selection is the operand of every file operation. TC supports mouse selection with either mouse button (configurable), keyboard selection with `INSERT`/`SPACE`, and pattern-based group selection from the `Mark` menu or the numeric keypad.

| Action | Keyboard | Notes |
|:--|:--|:--|
| Select under cursor, advance | `cm_Select` 2936 | repeat-friendly |
| Unselect under cursor, advance | `cm_UnSelect` 2937 | |
| Invert under cursor, advance | `cm_Reverse` 2938 | |
| Select / unselect group | `NUM +` / `NUM -` | asks for a mask or predefined filter |
| Select all / deselect all | `CTRL+NUM +` / `CTRL+NUM -` | configurable files-only vs files+folders |
| Invert selection | `NUM *` | |
| Restore selection | `NUM /` | selection before the last operation |
| Same extension | `ALT+NUM +` / `ALT+NUM -` | uses the extension of the cursor file |
| Same name / name+ext / path | `cm_SelectCurrentName` 541, `cm_SelectCurrentNameExt` 543, `cm_SelectCurrentPath` 537 (+ unselect variants 542/544/538) | path variant is meant for branch view and search results |
| Save / restore selection | `cm_SaveSelection` 530 / `cm_RestoreSelection` 529 | saved automatically before every file operation |
| Save to / load from file | 2031 (A 2041 / W 2042) / 2032 | plain text, one name per line |
| Load from clipboard | `cm_LoadSelectionFromClip` 2033 | |
| Save details | 2039/2043/2044, with headers 2093/2094/2095 (clipboard equivalents 2036–2038, 2090–2092) | all shown columns |

Mouse behaviour (documented):

- With the right mouse button configured for selection, the left button only moves the cursor.
- Consecutive range: click the first file with the correct button, hold `SHIFT`, click the last. With
right-button selection you can also drag over the files. On touch screens: hold a finger until a
square appears, then drag; releasing without selecting shows the context menu.
- Since TC 8.5 files can be selected by single-clicking their **icons**.
- TC 9 left-button mode: click **behind** the file name of an unselected file and drag a rectangle
(Explorer-like).
- `SPACE` on an **unselected directory** counts its contents and replaces `<DIR>` with the size in
the full view; disabled by Configuration – Options – Operation – "Selection with Space".

### 2.2 Selection mask language

| Form | Meaning |
|:--|:--|
| `*.txt` | types |
| `w*.*\|*.bak *.old` | include `w*.*`, exclude `*.bak`/`*.old` |
| `\|*.exe` | everything except programs |
| `*.jpg>*.cr2` | related files: also select `.cr2` when a matching `.jpg` exists in the same directory; complex schemes supported (`*.jpg>crw*.cr2`) |
| `<regex` | a leading `<` makes the mask a regular expression |
| `\subdir1\subdir2\*.ext`, `\*.ext` | wildcards limited relative to the base dir (TC 9.1, e.g. in branch view) |
| `subdir1\subdir2\*.ext` (no leading `\`) | relative to **any** subdir |
| `subdir\**\*.txt` | `**` matches any number of subdirs |
| saved search via `>>` | a predefined filter with the same options as the search function |

Configuration decides whether plain `NUM +` selects just files or files+folders; the `SHIFT` variant always does the other. **`NUM -` always acts on files and folders** and `CTRL+NUM -` always deselects files and folders — "for security reasons"; the `SHIFT` variants act on files only. `Shift+Del` removes stale entries from the mask history list.

Note: when showing duplicate-finder results in the main program, a **different** selection dialog is used (`dlg_duplicates.htm`, section 2.5).

### 2.3 Mark newer / hide same (`cm_DirMatch` 534, `options_marknewer.htm`)

Algorithm, as documented:

1. Compare the contents of the source and destination directory.
2. **Hide** (not delete) files that are identical — hiding is user-visible only.
3. Of the differing files, select only the **newer** ones.
4. Files **missing in one directory are also marked in the other**.

Both panel paths get the prefix `Comparison: ` while this mode is active; its presence means the content is shown only partially. Exit the mode by switching away from TC and back, minimizing and restoring, re-reading each panel with `F2`/`CTRL+R`, or switching directory/drive.

Related commands: `cm_CompareDirs` 533 (select newer, same behaviour "similar to a popular Commander for DOS", only newer files marked, not all differing ones) and `cm_CompareDirsWithSubdirs` 536 ("also mark subdirs not present in other dir").

### 2.4 Only selected files (`cm_ShowOnlySelected` 2023, `only_selected.htm`)

Hides every file that is not currently selected — meant for directories with thousands of files. Leaving the mode: toggle the command off, or refresh (`F2`/`CTRL+R`), or change directory.

### 2.5 Duplicate-selection dialog (`dlg_duplicates.htm`, reached with `NUM +`)

Purpose: after Commands – Search – Advanced – "Duplicate files" plus "Feed to Listbox", select which duplicates to act on. Duplicate *groups* are separated by dashed lines in the result.

Tab **Files**:

| Control | Meaning |
|:--|:--|
| Select by name/extension | masks as in 2.2 (`*.doc *.xls`, `*.* \| *.doc *.xls`, `<regex`, `>saved search` via `>>`) |
| Remove selection by name/extension | applied **after** the above, same syntax |
| Select by date/time | `All older files` (older than the newest in the group), `All newer files` (newer than the oldest), `Just the oldest file`, `Just the newest file` (may select several on ties) |
| Select by file size | `All larger than the smallest`, `All smaller than the largest`, `Just the largest file`, `Just the smallest file` (ties ⇒ several) |

Tab **Select by folder**:

| Control | Meaning |
|:--|:--|
| Choose folders … | `Shift+Click` selects multiple, `Space` toggles |
| Also select all subfolders | selecting a folder also selects its subfolders (and deselects likewise) |
| Select folder by pattern | substring or wildcards; `Select`/`Remove` apply it |
| Apply this pattern only to the direct parent directory | otherwise the whole path is matched (e.g. path `c:\Windows\System32` with pattern `Windows` matches only when unchecked) |
| `All` / `None` / `Reverse` | bulk folder selection |

`Leave at least one file in each group unselected` — tries method 1, then 2, then 3 (date/time, size, position) until a file per group remains unselected.

**Platform:** [core] the whole selection model and mask engine (`|` exclusions, `>` related files, `**`, `<regex`), saved/restored selections with automatic save before every file operation, mark-newer-hide-same, only-selected mode, duplicate-group selection. [win] the hidden/system attribute wording. The regex flavor must match the documented TRegExpr semantics (spec-04 §5.6).

---

## 3. Viewing and filtering the file list

Sources: `show_files.htm`, `show_shortlong.htm`, `show_sort.htm`, `show_tree.htm`, `show_thumbnails.htm`, `show_customcolumns.htm`, `show_customviewmodes.htm`, `show_separatetree.htm`, `quick_view.htm`, `reread_source.htm`, `options_branch_view.htm`, `options_cdtree.htm`, `def_colors_by_file_type.htm`, `dark___normal.htm`, plus `dlg_listerconfig.htm` (viewer side).

### 3.1 Purpose

A panel is a *rendering* of a directory with four independent, per-panel and per-tab settings: **filter** (which files), **display mode** (brief / long / comments / thumbnails / custom columns), **sort order**, and **tree/branch state**. These are preserved per tab.

### 3.2 Show files of type (`show_files.htm`) — filter

| Option | Effect |
|:--|:--|
| All files | everything |
| Programs | `*.exe;*.com;*.bat;*.pif;*.cmd` |
| (user selected) | the last user-defined specification |
| Custom… | dialog taking space-delimited masks; `\|` separates exclusions |

Mask examples: `*.bmp *.gif *.jpg` (show these), `*.* | *.bmp *.gif *.jpg` (everything **except** these), `*.* | .cvs\` (do not show directories named `.cvs`), `*.txt | *\` (only text files, hide all folders).

Keyboard: `CTRL+F10` all files, `CTRL+F11` only programs, `CTRL+F12` user-defined, `CTRL+F8` tree.

### 3.3 Brief / long / comments (`show_shortlong.htm`)

- **Brief** — only file names (`CTRL+F1`).
- **Long** — names plus size, creation date+time, attributes (`CTRL+F2`).
- **Comments** — file comments from `descript.ion` or `files.bbs`; if both exist in a directory TC
uses the file preferred in Configuration – Operation (`CTRL+SHIFT+F2`).
- Thumbnails (`CTRL+SHIFT+F1`) and custom columns (`SHIFT+F1` menu / `cm_SrcCustomViewN`) are the
other display modes.

### 3.4 Sort (`show_sort.htm`, `cm_SrcByName` etc. + `cm_SrcSortByColN` 6001–6099)

| Sort | Behaviour |
|:--|:--|
| Name | by name, then extension (**ascending** by default) |
| Extension | by type, then name (**ascending** by default) |
| Size | by size on disk (**descending** by default) |
| Time | by creation date+time (**descending** by default) |
| Unsorted | only separates files from directories; files keep the order returned by the `dir` command |
| Reversed order | inverts (Z→A, largest→smallest, newest→oldest) |

Documented hint: Name/Extension default ascending and Size/Time descending because "these combinations are the most used". Directory sorting by name can be toggled (`cm_SwitchDirSort` 2012). Custom-column views add `cm_SrcSortByCol1`…`cm_SrcSortByCol99` (target/left/right variants exist).

### 3.5 Directory tree (`show_tree.htm`)

- Default: only the **top level** of the tree is read (Explorer-like); branches expand with `+`/`-`
on the numeric keypad or by clicking the `+`/`-` sign left of the folder icon.
- Legacy mode (Configuration – Options – Display – "Always load complete directory tree") reads all
directories/subdirectories of the current drive; after reading, the tree is **cached** in
`treeinfo.wc` in the drive root if writable, otherwise in `%LOCALAPPDATA%\GHISLER\` on NT-based
systems. Only that file is updated when directories are added/deleted — much faster than
rescanning.
- Choosing "tree" in the **left menu** shows the tree of the **right drive** (and vice versa).
- After external programs added/deleted directories, rescan with `F2`/`CTRL+R` (fully expanded tree).
- The `treeinfo.wc` location can be changed with `TreeFileLocation=` in `[Configuration]`.
- `ALT+F10` opens a **dialog** with the current directory tree (`options_cdtree.htm`: pick a directory
with OK or double click → source window changes there).

### 3.6 Separate tree (`show_separatetree.htm`)

- Shows **one, two or no** separate tree panels, *in addition to* the two file panels; only
expandable (never fully expanded) trees.
- One tree ⇒ it follows and scrolls to the current directory of the **active** panel; two trees ⇒ one
per panel.
- "The tree isn't expanded initially to make Total Commander load faster"; it is expanded the first
time the directory changes or the panels are swapped.
- Operations available inside the separate tree: Copy/Cut/Paste, Delete, Rename (`Shift+F6`),
Properties (`Alt+Enter`), right-click context menu, Drag&Drop.
- Commands: `cm_SeparateTreeOff` 3200, `cm_SeparateTree1` 3201, `cm_SeparateTree2` 3202,
`cm_SwitchSeparateTree` 3203, `cm_ToggleSeparateTree1/2` 3204/3205; keyboard `CTRL+F8`,
`CTRL+SHIFT+F8` (cycle), `SHIFT+TAB` (focus).

### 3.7 Thumbnails (`show_thumbnails.htm`)

- Shows preview pictures for various file types; size and other options in Configuration –
Thumbnails.
- Besides thumbnail and file name, user-defined fields can be shown (file size, or via content plugin
e.g. exposure time).
- **Limitations**: works in normal file mode only — **not** with archives or FTP connections. Some
file-system plugins may support them. In virtual folders thumbnails cannot be loaded from Lister
plugins.
- WMF images are disabled in Lister and thumbnails for security (Windows vulnerability); can be
enabled with `[Configuration] WmfAllowed=3` in `wincmd.ini`. [win]
- Zapping: `cm_ZoomIn` 2953 (max 200 %), `cm_ZoomOut` 2954 (min 10 %),
`cm_ReloadSelThumbs` 2918.

### 3.8 Custom columns (`show_customcolumns.htm`, `cm_SrcCustomView1`…`9` / 271–279 and relatives)

- **29 custom column modes** can be chosen; fields are defined in Configuration – Custom columns.
- Intended for extra fields such as creation date+time, program version number, or (via plugin) mp3
tags (Artist, Track name …).
- Menu `SHIFT+F1`; cycling commands `cm_SrcNextCustomView` 5501 / `cm_SrcPrevCustomView` 5502
(target/left/right variants 5503–5508); on-demand field loading
`cm_LoadAllOnDemandFields` 5512, `cm_LoadSelOnDemandFields` 5513,
`cm_ContentStopLoadFields` 5514; scripting hooks `cm_LeftSwitchToThisCustomView` 5510 /
`cm_RightSwitchToThisCustomView` 5511.

### 3.9 Custom view modes (`show_customviewmodes.htm`)

- A view mode bundles a custom panel **background color**, **tab-header color**, a **header icon**, a
**columns mode** and a **sort order**.
- Defined in Configuration – Options – View mode; 10 slots per panel (`cm_SrcViewMode0`…`9`,
and `0` = "Default view mode (no colors or icons)").
- A view mode can carry `em_` commands to run automatically (`%COMMANDER_OPEN%=viewmode`).
- Automatic switching: `cm_ToggleAutoViewModeSwitch` 2947, configured in
Configuration – Auto-switch view modes.

### 3.10 Quick view (`quick_view.htm`)

- `CTRL+Q` shows the file under the cursor in the **other** panel. The bar above the contents shows
the file name plus the Lister plugin name used, or for music tracks the track name and artist. On a
folder, information about occupied space is shown.
- Disable by pressing `CTRL+Q` again or by selecting another drive in the Quick view panel.
- The pane **is** a Lister window ⇒ Lister shortcuts apply while it is active (`CTRL+F`/`F7` search).
- Width: enlarge the pane, then Configuration – Save position; TC then uses that width independently
of panel sizes when Quick view is off.
- `CTRL+SHIFT+Q` ⇒ **Separate** Quick view window, also Lister-based, title shows `Lister*`; its
position is saved separately from normal Lister windows.
- Commands: `cm_SrcQuickView` 304, `cm_SrcQuickInternalOnly` 306, `cm_SrcHideQuickView` 307,
`cm_SeparateQuickView` 2941, `cm_SeparateQuickInternalOnly` 2942, `cm_UpdateQuickView` 2946.
- In Quick view TC can show archive information without opening the archive (ZIP, ARJ, LZH, GZ): it
shows `file_id.diz`, else any `read*.*`, else `.NFO`, else any `.TXT`; if none exists, the archive
is shown as binary.

### 3.11 Reread source (`reread_source.htm`, `F2` / `CTRL+R`)

Re-reads the active file window's contents **and the drive bar**. Documented situations: a replaced floppy/removable disk, another program or remote computer wrote to the current directory, another user changed files on the connected FTP server. Also the only way to escape branch view back to the branch-view root after moving away, and to escape "Comparison: " mode and "only selected" mode.

### 3.12 Branch view (`options_branch_view.htm`, `cm_DirBranch` 2026 / `cm_DirBranchSel` 2046)

- Shows all files of the current directory **and all subdirectories** in one list. Use case: find a
file whose directory is unknown, or run the Multi-Rename Tool across subdirectories.
- Warning: do not use it in the root of very big disks — it can take a long time. `ESC` aborts.
Shortcut `Ctrl+B`; selected-directories variant `Ctrl+Shift+B`.
- Leaving branch view with `Ctrl+B` while the cursor stands on a file in a subdirectory: TC goes to
**that subdir** instead of the branch-view root; use `F2`/`CTRL+R` to get back to the root.
- Copies from branch view can honour `Keep relative paths` (see 1.2).

### 3.13 Colour rules per file type (`def_colors_by_file_type.htm`)

- Dialog reached via Configuration – Options – Colors – "Define colors by file type".
- The list shows defined types in their colour; **sort order defines priority** (first match wins —
e.g. a ZIP rule placed before a read-only rule decides the colour of a read-only ZIP).
- Double-click or `Space` edits an entry.
- Entries whose type starts with `*` (e.g. `*.txt`) can be toggled by clicking the `*`, which becomes
`-.ext` (a file literally named `-.ext` still matches).
- `Add…` (asks for type, then colour), `Edit…`, `Delete`.
- `Dark<>Normal` switches the editor between dark and normal mode: normal-mode colours are converted
automatically to suitable dark colours but can then be tuned separately.
- The mask language is "the same possibilities as for the file selection using the `+` key".
- Global toggle: `cm_SwitchColorsByFileType` 2957.

### 3.14 Dark <-> normal mode (`dark___normal.htm`)

- Configuration – Dark<->Normal switches modes; the change is **permanent** unless TC is configured
to follow the Windows colour mode automatically.
- Dark colours live under Configuration – Options – Color – Others – Dark mode; normal mode uses the
normal Windows theme colours.
- Dark mode is only activated on Windows XP and newer; for Win95/98/ME/NT4/2000 set
`[Configuration] ForceDarkModeOptions=1`.
- Windows 10 1809+ supports dark mode for GUI elements; on older versions TC **simulates** it. [win]
- Commands: `cm_SwitchDarkMode` 2950, `cm_EnableDarkMode` 2951, `cm_DisableDarkMode` 2952; start
switch `/M=0|1|2` (light/auto/dark).

### 3.15 Other view toggles documented in the catalogue

`cm_VerticalPanels` 305 (file windows above each other), `cm_50Percent` 909 / `cm_100Percent` 910 (panel separator), `cm_SwitchLongNames` 2010, `cm_SwitchHidSys` 2011 / `cm_SwitchHid` 3013 / `cm_SwitchSys` 3014, `cm_Switch83Names` 2013, `cm_SwitchDirSort` 2012, `cm_RereadSource` 540, `cm_ShowOnlySelected` 2023, `cm_Vis*` family 2901–2926 (button bar, drive buttons, breadcrumbs, tab header, status bar, command line, key buttons, tabs, history/hotlist buttons), `cm_SeparateTree*` 3200–3205, `cm_SwitchOverlayIcons` 2917 [win], `cm_SwitchWatchDirs` 2921, `cm_SwitchIgnoreList` 2922, `cm_SwitchX64Redirection` 2925 [win], `cm_SwitchColorsByFileType` 2957, `cm_SwitchFileTipWindows` 2959, `cm_ZoomIn/Out` 2953/2954.

**Platform:** [core] filter language, brief/long/comments, the sort rules and their default directions, tree caching to an on-disk index, separate tree, thumbnails, custom columns, view modes, quick view, branch view, colour rules with priority ordering, dark mode. [win] WMF thumbnails, Explorer preview handlers, Windows-theme colours, drive letters, `treeinfo.wc` under `%LOCALAPPDATA%`. Linux analogues *(inferred)*: cache file in `~/.cache`, mount points instead of drive letters, `stat` birth time where available; `descript.ion`/`files.bbs` comments are plain text and fully portable.

---

## 4. Packing and archives

Sources: `packedfiles.htm`, `dlg_pack.htm`, `dlg_unpack.htm`, `dlg_internalzipconfig.htm`, `dlg_internal7zipconfig.htm`, `options_pack.htm`, `options_testarchives.htm`, `options_unpackall.htm`, `plugins.htm`, `aes_encryption_information.htm`.

### 4.1 Purpose and the "archives are directories" model

- Total Commander can work with packed files **as if they were directories**: `ENTER`/double-click on
an archive shows its contents in a normal file window; directories stored inside the archive appear
as normal directories; `ENTER` on a packed file shows details (full path, compression method,
ratio).
- **Archives inside archives** are supported. Unpacking to a temporary directory happens on `ENTER`;
for unusual extensions or self-extracting archives use `CTRL+PgDn` instead. When leaving the
archive (going to its parent or switching drive) the temporary copy is deleted.
- Formats: `.ZIP .ARJ .LZH .RAR .UC2 .CAB .ACE .7Z` plus `.TAR` (often inside `.GZ`), `.TGZ`. `.CAB`
can be **unpacked only, never created**. Self-extracting `.EXE`/`.COM` archives are opened with
`CTRL+PgDn`.
- Built-in **unpackers**: ZIP, ARJ, LZH, RAR, TAR, GZ, CAB, ACE, 7Z (all supporting long file names).
- Built-in **packers**: ZIP and 7-ZIP, TAR, GZ, TGZ (`TGZ` creates a TAR inside a GZ in one
operation). External packers (`pkzip`, `arj`, `lha`, `rar`, `uc2`, `ace`, `7zg`) can still be used
as a shell.
- Long-name caveat: most older packers do not support Win95/98/NT long file names; exceptions are the
internal ZIP packer, Info-ZIP, and current UC2/RAR/LHA/ACE. Advice: prefer the internal ZIP packer.

### 4.2 Pack dialog (`dlg_pack.htm`, `ALT+F5`; `ALT+SHIFT+F5` = move to archive)

**Critical syntax:** the packer name and a colon must precede the archive name (`zip:c:\test.zip`, `7z:`, `arj:`, `lha:`, `rar:`, `uc2:`, `ace:`), otherwise the files are copied/moved normally. `EXT:` forces the external ZIP packer, `INT:` forces the internal one. Long names with spaces must be quoted (`zip:"c:\long name.zip"`). Packing into a subdirectory of an archive is possible only with the internal ZIP/TAR packers and external RAR/UC2 (and some plugins); separate the subdir with a **forward slash**: `zip:c:\test.zip/this/is/a/subdir`.

| Option | Documented meaning |
|:--|:--|
| Pack n files to the archive | Target archive; created if absent. Proposed default = target path + source directory name. |
| also pack path names (recursed only) | Marked subdirectories are stored with their path **relative to the source directory** (`[test]` containing `document.txt` ⇒ `test\document.txt`). Unchecked ⇒ files are stored without paths. |
| Recursively pack subdirectories | Also descends into marked subdirectories. |
| Multiple disk archives | Multi-volume ZIP/7-ZIP/ARJ/RAR/ACE; asked to swap media or give another location when full. ZIP allows unpacking selected files from all disks; ARJ only all files or selected files from one disk at a time. |
| Move to archive | Files are deleted only after a **successful** packing operation. |
| Create self extracting archive | EXE that unpacks itself on start; ZIP, 7-ZIP, RAR, ACE, ARJ and some plugins (SQX); 32-bit self extractors that also run on 64-bit Windows; no distribution licence required. |
| Create separate archives, one per selected file/dir | One archive per file; useful for formats that hold one file per archive (e.g. GZ); without a target dir the archive goes next to the original. |
| Leave out base directory when packing folders | With the option above: unchecked ⇒ the folder itself is packed with its name; checked ⇒ only its contents. |
| Encrypt | Requires the internal ZIP/7-ZIP packer, external RAR/ARJ, or a plugin supporting encryption (e.g. SQX). |
| Compression rate | Internal ZIP, GZ, TGZ (and separately 7-ZIP): `0` = store, `1..9` = strength, `10` = LZMA if enabled. 7-ZIP/GZ/TGZ accept only 1..9. |
| Packer | Selects ZIP/ARJ/LHA/RAR/7-ZIP/ACE; a not-on-PATH packer needs its full path in Configuration – Options – Packer. |
| Configure | Opens Configuration – Packer. |

`CTRL` while choosing Files – Pack makes the default archive location the **source** directory instead of the target directory.

Self-extracting ZIP/SFX archives created by TC accept command-line arguments which start unpacking immediately without initial confirmation: `Drive:\Path` (unpack there, ask before overwriting), `/S` (unpack to the current directory, ask before overwriting), `/O` (unpack to the current directory, overwrite without asking; can be combined with a path, `/O` first).

### 4.3 Unpack dialog (`dlg_unpack.htm`, `ALT+F9` / `F5` on an archive panel)

| Option | Meaning |
|:--|:--|
| Unpack n file(s) to | Target path; default = target directory path; empty ⇒ the archive's own directory. |
| Files to unpack | Only shown for `ALT+F9`; wildcards like `*.doc *.txt`; names with spaces must be quoted (`"Letter to Mr. Smith.doc"`). |
| Encoding of names | Change the name encoding, e.g. for a ZIP from a different language encoding. |
| Unpack path names if stored with files | Marked-directory contents are unpacked with their paths. |
| Overwrite existing files | Otherwise the unpacker asks per file. |
| Unpack each archive to a separate subdir (name of the archive) | `filename.zip` ⇒ `filename\`. |

Also from `options_unpackall.htm`: move the cursor to a valid archive and press `ALT+F9`, choose the target directory, then enter masks. In some RAR files even the **file names are encrypted**; TC then shows `<Encrypted -> F5>` and such archives can only be unpacked completely with `ALT+F9` or `F5`.

Adding files: open the archive in one panel, select files in the other, press `F5` (pack) or `F6` (move = pack & delete). **TGZ, GZ and CAB cannot be modified.** You cannot add files to a *subdirectory* of an archive except with the internal ZIP, TAR and TGZ packers, with external RAR, UC2 and some packer plugins — but whole directories can be added.

Viewing/editing inside an archive: `F3` shows a warning, unpacks and displays the file; the temp file is deleted after closing and the archive is untouched, except that after modifying with a **Lister plugin** TC offers to re-pack the changed file. `F4` unpacks and re-packs if changed; repacking does **not** work for TGZ, GZ, CAB — the modified file then stays in the temporary directory.

### 4.4 Internal ZIP configuration (`dlg_internalzipconfig.htm`)

| Field/option | Meaning |
|:--|:--|
| ZIP-packer / ZIP-unpacker | Path of external `pkzip.exe` / `pkunzip.exe`; `unzip.exe` and 7-Zip cannot be used as the unpacker. |
| Use internal ZIP-packer | Self-contained; the archive is checked for errors after packing; the check can be aborted without deleting the ZIP; disable with `[Packer] VerifyZip=0`. |
| 7z | The given packer is `7zg.exe` (preferred) or `7z.exe`, used **to create ZIP archives, not 7z archives**; parameters such as `-mcu=on` may follow the program name but not commands like `a`; 7-Zip can only pack, not unpack. |
| Use ZIP-NT 2.0.1 | Info-ZIP 2.0.1 must be patched or files with spaces are packed wrongly; TC patches an unpatched 2.0.1 automatically; later versions need no patch. |
| Use internal unzipping | Internal unzip by Info-ZIP, always used when checked. |
| Compression | 1 (fastest) … 9 (best, slowest), 0 = no compression; default 6 because 9 is very slow for marginal gain. |
| Directory for temporary files | `default` (uses `%temp%`) or a user directory that must exist (needed when packing to removable disks). |
| Use 8.3 short filenames | Writes only the short alias so plain DOS can unpack. |
| Open partial ZIP files | Open only partially downloaded ZIPs, even during download — useful for very large ZIPs. |
| Keep damaged files (bad CRC) | Normally files with a bad CRC are deleted on unpack; this keeps the possibly damaged data. |
| Pack Unicode names | Six policies: ask each time; store as UTF-8 (Pkzip 4.5/WinZip 11.2 method); all as UTF-8 if at least one contains Unicode; all as UTF-8 if at least one contains non-English characters; store the Unicode name in an extra field (Info-ZIP method, best for Unix/Linux recipients); store all non-English names in the extra field; store Unicode characters as `?` (legacy TC 7.0x behaviour, `?` becomes `_` on unpack). |

### 4.5 Internal 7-Zip configuration (`dlg_internal7zipconfig.htm`)

| Field/option | Meaning |
|:--|:--|
| 7-Zip packer | Path of `7zg.exe`; only needed for the external packer. |
| Use internal 7-Zip packer | Based on 7-Zip sources; `tc7z.dll`/`tc7z64.dll` can be replaced by newer `7z.dll` (correct bitness required). Used for subdirectory packing **even when the internal packer is disabled**, because the external one cannot do it. |
| Prefer 7-Zip plugin if installed | If the `7zip` or `Total7zip` plugin exists, open archives with it (`7z` vs `7-ZIP` choice when creating). |
| Extra parameters | Passed to internal and external 7-Zip. The internal packer supports only parameters starting with `-m` plus `-slp` (large page mode), `-stl` (archive date = newest file), `-v` (volume size); accepts `-mx=9`, `-mx9` and bare `-mx` (which means the default, `-mx=9`). |
| 7-Zip compression | 1..9 plus 0; default 6; officially only odd numbers + 0, but even numbers work. |
| Compression method | LZMA2 (recommended), LZMA, PPMd, BZip2. |
| Dictionary size / Word size | Default shown with an asterisk; higher = better ratio but more memory and slower; stored separately per method and ratio on Apply. |
| Create solid archive | All files compressed as one large unit: better ratio for similar files, but unpacking/changing is slower — the last file requires all preceding ones. |
| Solid block size | Smaller blocks ⇒ slightly bigger archive but only one block must be unpacked for a single file. |
| Number of CPU threads | Parallel compression; raises memory use. |
| Memory usage | Shows RAM needed for compression/decompression with the current settings; 32-bit programs can only address 2 GB in total, so dictionary size or thread count must be reduced if allocation fails. Decompression memory matters for **all** machines that will unpack. |
| Also encrypt names | Whether file names and attributes are encrypted too; if yes, the password is needed even to open the archive, not only to unpack/change it. |

### 4.6 Pack options page and archive test (`options_pack.htm`, `options_testarchives.htm`)

- Pack/unpack and copy can be moved to the background from the progress dialog; `F2`/`CTRL+R` may be
needed afterwards.
- Most packers have an **upper limit of ~2 GB per archive**; the built-in ZIP packer supports larger
archives (WinZip/PKZIP-compatible) that not every ZIP packer can unpack.
- `uc2:` is the only way to keep creating new UC2 archives.
- **Test archives** (`ALT+SHIFT+F9`): unpacks every selected archive **to memory** and recomputes the
CRC, reporting mismatches or other errors. **TAR files have no CRC and therefore cannot be tested.**

### 4.7 Plugin interfaces (`plugins.htm`) — functional summary

Four fundamental plugin types plus languages. The page states a plugin writer's guide and sample sources exist per type, most in C, some in Delphi.

| Type | Purpose / typical examples (as listed) | Interface role |
|:--|:--|:--|
| **Packer plugins** (WCX) | Unpack specific file types, usually archive formats; some also **create** and **modify** archives. Examples: bzip2 pack/unpack; "create a list of files in the selected directories" (disk catalogues); "create a batch file for all selected files". | Contributes archive handlers to the "archives are directories" model: browse/unpack, optionally pack/add/delete, optionally multi-volume/encryption. |
| **File system plugins** (WFX) | Accessed via Network Neighborhood; expose parts of the PC not reachable via drive letters, or remote systems. Examples: Windows CE/PocketPC device, remote web servers via HTTP (download a list of files), a mail server. | Behaves like a source/target panel: directory listing, navigation, copy in/out, delete, mkdir, rename; TC falls back to its own handling for commands the plugin does not implement. |
| **Lister plugins** (WLX) | Display special file types in the built-in Lister and in the Quick view panel. Examples: DBF databases, PDF, source code with syntax highlighting. | Provide additional Lister "view methods"; several may match one file, cycled with the `4` key; detection strings decide the ordering in the Plugins menu. |
| **Content plugins** (WDX) | Several purposes: **searching** for file properties/contents, **displaying** properties in file lists, and **using them in the Multi-Rename Tool**. Examples: mp3 id-tags (Artist, Title, Album), EXIF photo data (aperture, exposure time, flash used), attributes such as creation time or program version number. | Supplies named fields with units; consumed by custom columns, search (Plugins tab), multi-rename `[=plugin.field.unit]`, sync "plugin compare", attribute editing. |
| Languages | Additional languages are installed like plugins by double-clicking the language ZIP. | — |

Installation: newer plugins install by double-clicking the plugin ZIP (TC asks where to put it); otherwise unpack and add via Configuration – Options – Plugins using the button of the plugin type. Lister plugin order matters and is changeable with `Shift`+arrow keys in the plugin list; `cm_UnloadPlugins` 2913 unloads all plugins.

**Platform:** [core] the archive-as-directory model, pack/unpack into and out of archives, test-by-unpack-to-memory, name-encoding policy, multi-volume, SFX argument handling, and the plugin type taxonomy with the "plugin first, TC fallback" dispatch rule. [win] external packer shell-outs (`pkzip.exe`, `7zg.exe`), 8.3 short names, Pkzip/WinZip Unicode methods, CAB, SFX EXEs. A Linux clone can use `libarchive`/native `.tar`/`gzip`/`xz` and drop the shell-out entirely *(inferred)*.

---

## 5. Search (Find files)

Sources: `dlg_search.htm`, `dlg_advancedsearch.htm`, `dlg_searchplugins.htm`, `dlg_srch_loadsave.htm`, `reg_ex.htm`, `options_search.htm`.

### 5.1 Purpose and entry points

- Finds files/directories by name mask, location, date, size, attributes, content, content-plugin
values and duplicates. Results can be **fed to a listbox** (a real file panel of the source window)
and then be copied, deleted, renamed, compared, etc.
- `ALT+F7` = in-process search; `ALT+SHIFT+F7` = **separate process** (`cm_SearchStandalone` 545).
In the standalone version, `Search in selected directories/files` gets unchecked and a warning icon
appears when switching away from the dialog (the selection could change); later selection changes
do not affect that search. **FTP searching is not available in the standalone tool.** Title shows
`Find Files (separate)`.
- `cm_SearchForInCurDir` 517 searches inside the directory under the cursor.
- Search can start inside an archive; the button `>` jumps between previous/next search result sets
(history of results). `ESC` or Cancel interrupts. The dialog position is saved automatically on
close.

### 5.2 General page

**"Search for" mask:**

- `?` = exactly one character, `*` = any number of characters; `\**\` in paths matches any number of
subdirectories.
- Multiple masks separated by **spaces** (all are searched); everything after `|` is an **exclude**
mask.
- Names with spaces need no quotes (`Letter to Mr. Smith.doc`) — TC searches both the whole name and
the individual parts. **Exception:** when `|` is used for an exclude mask, names with spaces must be
quoted.
- If the search string contains a **dot**, TC looks for an exact name match.
- `Shift+Del` removes unwanted entries from the search history.
- A leading `>` in this field means "keep this field unchanged when loading a saved search".
- `ev:` prefix ⇒ pass the string unchanged to **Everything** (Everything syntax). `ed:` prefix ⇒ like
`ev:` but restricted to the `Search in` directories (TC prefixes `path:c:\path` and optionally
`parents:<nr`).

**Traverse filters** (include/exclude directories being descended into) — a directory expression must end with `\`:

| Expression | Matches |
|:--|:--|
| `subdir1\`, `subdir1\subdir2\` | subdirectories **anywhere** below the search location |
| `\subdir1\`, `\subdir1\subdir2\`, `\` | subdirectories **directly** in the search location only |
| `\subdir1\subdir2\**\` | terminate with `\**\` to also find files in subdirectories |
| `\|**\` appended | skip directories in which no files were found |
| `\| \` appended | exclude the base directory |

Documented mask examples:

| Mask | Finds |
|:--|:--|
| `*.ini` | e.g. `win.ini` |
| `Smith` | `Letter to Mr. Smith.doc` |
| `*.bak *.sik *.old` | all three backup extensions |
| `*n.ini` | names that **must** contain an `n` before the dot |
| `*wof*.doc` | names containing `wof` with extension `.doc` |
| `dir1\**\dir2\*.dll` | `.dll` files only in `dir2\`, where `dir2\` is at any level within `dir1\` |
| `w*.*\|*.bak *.old` | start with `w`, not `.bak`/`.old` |
| `*.ini \| windows\` | ini files except in directories called `Windows` and their subdirs |
| `*.htm? \| _vti*\` | html files except in subdirs starting with `_vti` (FrontPage) |
| `windows\ system32\ *.ini` | ini files only in `windows\` and `system32\` dirs |
| `totalcmd\**\ \| \` | everything within total commander dirs, `\| \` excludes the base dir |

**"Search in"** — one or more start paths (directories, FTP or plugin paths) separated by `;`; all must be the same type (e.g. several FTP paths, but not a local dir plus an FTP path). The start path may instead be put in the `Search for` field. A leading `>` also means "keep unchanged when loading".

**List-file input**: `@c:\path\filelist.txt` (ANSI, UTF-8 or UTF-16 **with BOM**), one file/directory per line, absolute or relative to the list file's location; the list supports wildcards and regular expressions. Semantics:

| Case | Behaviour |
|:--|:--|
| `Search in subdirectories` = 1 level or higher | Listed directories are searched **within**; the directories themselves are not results. `Search for` is ignored when a line contains wildcards or a regex. |
| `Search in subdirectories` = "current dir only" | Behaves like loading a file/folder list from file: only the listed files/dirs are found, nothing inside them; lines with wildcards/regex are ignored; `Search for` is ignored. |

Sample list-file lines (verbatim semantics from the page's table): `c:\testdir\filename.txt` searches only for that file in `c:\testdir` (depth has no effect); `c:\testdir\` searches **inside** it with the `Search for` string when depth ≥ 1; with "current dir only" only `c:\testdir` itself is found; `c:\testdir\*.txt *.doc` searches for txt and doc files when depth ≥ 1; `c:\testdir\<regex` searches with a regular expression when depth ≥ 1.

**Toggles on the general page:**

| Control | Meaning |
|:--|:--|
| `>>` | choose a starting directory |
| Drives | multi-select drives |
| **RegEx** | search file **names** with regular expressions |
| Only search in selected directories/files | restricts to the active panel's selection; allows both including and excluding |
| Everything | use the Everything tool's NTFS MFT index. Works without building its own index; the MFT holds only names, so size/date are accelerated only with Everything 1.4+ (which caches them); attributes, content-plugin fields and file contents are not accelerated. Note: files with hidden/system attribute in hidden/system subfolders are still found by Everything but not by a regular search. |
| Search archives | ZIP, ARJ, LZH, RAR, TAR, GZ, CAB, ACE, 7Z — text can be searched inside; files are unpacked, searched and deleted on the fly |
| Search in subdirectories | all, current directory only, or up to a specific depth |
| Find text | `\t` = tab, `\n` = line break, `\\` = backslash; warns when the text cannot be converted to a chosen encoding (`WarnSearchText` in `wincmd.ini`) |
| Whole words only / Case sensitive / RegEx (2) / Hex | content matching modes; hex accepts `00 FF 12` or mixed `"PK"0102`. When "Case sensitive" is off, entering lowercase hex codes also matches the uppercase letters (e.g. `6B` also finds `K` = `4B`). |
| Find files NOT containing the text | inverse content match, still honouring all other criteria (use case: find the files that have not been modified yet) |
| Encodings (multi-select) | ANSI (Windows charset), ASCII (DOS), Unicode UTF-16, UTF-8, **Office xml (docx, xlsx, odt) + EPUB** — the latter searches renamed ZIP containers with XML/HTML text and does not require "Search archives" |
| Plugins: … | search inside the named content plugins for the `Find text`; `+` selects plugins (e.g. comments, PDF text) |
| `<` / `>` | jump to previous/next search **result set** (history) |
| `F2 Search in found files` | next search runs only within the displayed results. With depth ≥ 1 listed directories are searched within but are not results; with "current dir only" only the listed files/dirs can be found. |
| Result list | date/time/size are shown in the footer; double-click a file to go to its directory; click the list and press `Ctrl+C` to copy the whole list to the clipboard |
| Status line / footer | shows the directory being searched and, after the search, the file details; clicking part of the directory path skips that directory (after confirmation) |
| Feed to listbox | transfer results to the source panel; `Shift` opens the result list on a **new tab**; `F2`/`CTRL+R` returns to normal display |
| Go to file | switch to the result's directory; `Shift` opens a new tab |

Archive nuance: if **all** found files came from a single archive, the files are shown in the source window; otherwise the archives themselves are shown.

### 5.3 Advanced page

| Control | Meaning |
|:--|:--|
| Date between | lower/upper bound on last-modified; one or both may be given |
| Not older than | maximum age |
| Older than | minimum age; combined with `Not older than` by **AND** to express a range (older than 7 days but not older than 14) |
| File size | equal / smaller / larger; units Bytes, kBytes, MBytes, GBytes (each ×1024), fractional values allowed (e.g. `0.1 GBytes`) |
| Attributes | hidden, system, directories, NTFS-encrypted, etc. [win for the NTFS part] |
| Find duplicate files | groups duplicates, groups separated by dashed lines, sorted by the first enabled criterion (name → size, else size/content → size, else plugin fields → first plugin field) |
| same name / same size / same contents | byte-identical check for contents ("takes long but ensures 100 % identical") |
| same plugin fields | all specified properties must be identical (**AND**); supports ranges such as `[=tc.name:1-3]` = first 3 characters. Note: an "any identical" (OR) variant is documented as impossible because three files could pair up on different properties. |

### 5.4 Plugins page (`dlg_searchplugins.htm`)

| Field | Meaning |
|:--|:--|
| Search in plugins | enable/disable without clearing the list |
| Combine with | `AND (all match)` (e.g. > 1 MB **and** < 2 MB) or `OR (any match)` (e.g. hidden **or** system) |
| Plugin | plugin name; `tc` = Total Commander's own properties (size, date, time …) |
| Property | property of that plugin |
| OP | `=`, `!=`, `=(case)`, `!=(case)`, `>`, `<`, `>=`, `<=`, `contains`, `!contains`, `cont.(case)`, `!cont.(case)`, `regex` |
| Value | for date/time use the format shown in the main file windows; numeric fields may offer a unit (bytes, kbytes …) |
| More rules / Fewer rules | append / delete the last rule |

Rules can be reordered with `Ctrl`+`cursor up/down` or the arrow buttons. **Content plugins work only on the normal file system** — not with virtual folders such as the desktop, FTP connections, files in archives, or file-system plugins.

### 5.5 Load/Save page (`dlg_srch_loadsave.htm`)

`Previous searches` list with `Load` (also switches to the General page), `Save`, `Delete`. A leading `>` in `Search for`/`Search in` keeps those fields unchanged when the search is loaded.

**Saved search used as an F5 copy filter** — these General-page fields are **not** used: `Search in`, `Only search in selected directories/files`, `Everything`, `Search archives`, `Search in subdirectories` (stated identically on three pages).

### 5.6 Regular expressions (`reg_ex.htm`)

Used in: Commands – Search (names **and** contents), Lister, Multi-Rename Tool, and the selection dialog. Engine: the free Delphi library **TRegExpr** by Andrey V. Sorokin.

Meta-characters / initial parts: `. \ ( ) [ ] { } ^ $ + * ?` and `-` only inside character classes.

| Feature | Syntax / behaviour |
|:--|:--|
| Literal | `test` finds `test` anywhere in a name or line |
| Escapes | `\t` tab, `\xnn` hex character (`\x20` = space), `\x{nnnn}` Unicode code point (needed for characters outside the local codepage since TC uses Unicode names), `\[`, `\\`, `\.` |
| Character class | `[aeiou]`, ranges `[a-z]`, negation `[^aeiou]`; `M[ae][iy]er` matches Mayer/Meyer/Maier/Meier |
| Anchors / classes | `^` line start, `$` line end, `.` any char, `\w \W \d \D \s \S \b \B` |
| Iterators | `?` 0..1, `*` 0..n, `+` 1..n, `{n}`, `{n,}`, `{n,m}`; all **greedy**, a following `?` makes them non-greedy (`b+` on `abbbbc` ⇒ `bbbb`, `b+?` ⇒ `b`) |
| Alternatives | `(John\|James\|Peter)` |
| Look around | `q(?!u)` (no `u` after), `q(?=u)`, `(?<!a)q`, `(?<=a)q`. **Limitation:** look-ahead must be at the **end** of the expression and look-behind at the **start**, otherwise an error is shown. `(?>bc\|c)` atomic grouping is **not supported by Everything**. |
| Backreferences | `\1` … ; `\s([0-9]+) \1\s`; `(.+)\1+` matches `abab` |
| Subroutine calls | `(?n)` inserts sub-expression n (vs. backreference which matches its *result*); `(?&name)` for named groups. Example: `([a-z]+)_\1` does not match `abc_def`, `([a-z]+)_(?1)` does |
| Subexpressions for search+replace | up to **89**; `$1`, `$2`, … in the replacement; `\U`, `\L`, `\F` convert the following placeholder to upper/lower/title case, e.g. Search `(.*) - (.*)\.mp3` → Replace `\U$2 - \L$1.mp3` |
| Special groups | `(?:expr)` non-capturing, `(?P<name>expr)` / `(?'name'expr)` named, `(?P=name)` re-use, `(?#comment)`, `(?R)?` recursion (`a(?R)?z` matches `az`, `aazz`, …) |
| Modifiers | `(?i)` ignore case (**default for file names in TC**), `(?-i)` case-sensitive, `(?g)` greedy (default), `(?-g)` non-greedy. Other modifiers are irrelevant: TC only supports searching within a single line. |

### 5.7 Options page (`options_search.htm`)

Documents Find files plus "Search in separate process": allows continuing to work in TC during the search and running multiple searches in parallel; FTP searching is not possible there; the title bar shows `Find Files (separate)` so it can be distinguished from the internal search.

**Platform:** [core] the mask and traverse-filter language, list-file input semantics, content search with the documented encodings (OOXML/EPUB are plain ZIP+XML and portable), the plugin rule engine, duplicate grouping, saved searches and their "unused fields as copy filter" rule, and the regex dialect. [win] Everything/MFT acceleration, the NTFS-encrypted attribute, `WarnSearchText` as an INI key. A Linux clone should map TRegExpr onto a compatible engine and keep the documented look-around placement restriction only if it keeps that engine *(inferred)*.

---

## 6. Multi-Rename Tool

Sources: `multi_rename_tool.htm`, `dialog_box___multi_rename_tool.htm`.

### 6.1 Purpose

Rename a list of selected files/folders with powerful rules: numbered lists, case conversion, string replacement (including multiple pairs at once), date/time insertion, character extraction. The stated capabilities: create a numbered file list, convert to upper/lowercase, replace a string, add current date/time, keep only certain characters of the name. `CTRL+M`; command `cm_MultiRenameFiles` 2400.

### 6.2 Workflow and user-visible behaviour

1. Select files in the panel, press `CTRL+M`.
2. Placeholders use **square brackets `[]`**, not `*`/`?` wildcards; all other characters are copied
literally.
3. The result list shows the old and the **New name** immediately; nothing is renamed until
`Start!`.
4. Effectively the target name is: *name mask* + `.` + *extension mask*. The two fields are separate
to prevent accidental removal of extensions (which would break program associations).
5. Name masks may contain `\` to **move** files into other directories; missing directories are
created automatically.
6. If the rename mask has an error, the new-name column shows `<Error!>`.
7. `Start!` renames; a warning appears on name conflicts. Since TC 9 a collision offers
automatic renaming to `name (2).ext`, `name (3).ext` … — **not shown in the preview**.
8. `Undo` reverses the operation in **reverse order** (last renamed first) and also works **after the
dialog has been closed** — just reopen the MRT with any files.
9. `Result list` writes a protocol of the renamed files.
10. `Close` dismisses without any action.

### 6.3 Masks, counters and extensions

| Control | Meaning |
|:--|:--|
| Rename mask: file name | Builds the new name; buttons insert placeholders for the old name, name parts, counter, file date/time, plugin field; `+`/`#` insert rarer fields such as the parent directory name. |
| Extension | Extension mask; all placeholders are allowed in either box. |
| **Search & Replace** | `Search for` → `Replace with`, **case-insensitive** by default; both fields support `*` and `?`. **Applied AFTER the rename mask.** Multiple pairs in one step, separated by `\|` (Alt+124), e.g. `ä\|ö\|ü\|é\|è\|ê\|à` → `ae\|oe\|ue\|e\|e\|e\|a`. |
| `^` | Respect upper/lowercase in search&replace |
| `1x` | Replace only the **first** occurrence |
| `[E]` | Also replace inside the extension. When unchecked: (a) with a rename mask only the "File name" part is modified; (b) when names are loaded from an external text file, the part after the last dot is not modified. |
| **RegEx** | Use regular expressions for search&replace |
| **Subst.** | Substitution: the whole name is replaced by the `Replace` text; otherwise only the found expression is replaced; subexpressions usable |
| Upper/lowercase | Whole string → upper / lower / first letter upper + rest lower. **Applied after the rename mask and after search&replace.** `[U]`, `[L]`, `[n]` convert only certain parts. |
| First of each word uppercase (English in Titles) | excludes these words from uppercasing except at start or end: `a, an, the, at, by, for, in, of, on, to, from, up, and, as, but, or, nor, yet, so, vs` |
| **Define counter [C]** | `Start at` (number of the first file — numbering follows the result list order, which can be sorted like the main window, reordered by drag&drop or `Shift`+cursor keys), `Step by`, `Digits` (leading zeros; option `a` derives the width from the number of files) |
| `F2 Load/save settings` | `<Default>` (no changes), `Save settings`, `Delete entry`, load earlier entries |
| Context menu (`F10`) | `Load names from file`, `Edit names` (write target names to a text file, edit, paste back), `Configure editor` (must save plain text) |
| File list header | sort by old names, extensions, sizes or time stamps |
| File list | shows all files with the `New name` column; items movable with drag&drop or `Shift`+cursor keys (changes counter order); `Del` removes a file from the list (not renamed) |
| `(next step)` / `F5` | Loads the rename results as the input for the next rename step (apply several rule sets to the same files) |

Since TC 9, with name collisions the tool offers auto-renaming to `name (2).ext`, `name (3).ext` etc. (not previewed).

### 6.4 Placeholder reference (all fields, exact case matters)

**Name / path parts**

| Placeholder | Meaning |
|:--|:--|
| `[N]` | Old file name **without** extension |
| `[N1]` | First character of the original name |
| `[N2-5]` | Characters 2 to 5 (4 characters). Double-byte characters count as **1**; the first letter is index 1. |
| `[N2,5]` | 5 characters starting at character 2 |
| `[N2-]` | All characters from character 2 |
| `[N02-9]` | Characters 2–9, left-filled with **zeroes** when the name is shorter (`abc` → `000000bc`) |
| `[N 2-9]` | Same but left-filled with **spaces** (`abc` → ` bc`) |
| `[N-8,5]` | 5 characters starting at the 8th-last character |
| `[N-8-5]` | Characters from the 8th-last to the 5th-last |
| `[N2--5]` | Characters from the 2nd to the 5th-last |
| `[N-5-]` | From the 5th-last character to the end |
| `[A]` | File/folder name **including** extension (mainly for renaming folders) |
| `[I]` | Toggles the "folders are treated as if they have extensions" behaviour for `[N]`/`[E]`: when on, `[N]` is the full folder name and `[E]` empty. Placement decides when it activates; repeating toggles it. |
| `[2-5]` | Characters 2–5 of the name **including path and extension** |
| `[P]` | Name of the parent directory (`c:\directory\file.txt` → `directory`); `[P2-5]`, `[P2,5]`, `[P-8,5]`, `[P-8-5]`, `[P2-]` work as for `[N]` |
| `[G]` | Grandparent directory (same range syntax) |
| `[B0]…[B9]` | Directory from the path: `[B0]`=`[P]`, `[B1]`=`[G]`, `[B2]`=great-grandparent …; partial ranges `[B2_2-5]` |
| `[B+0]…[B+9]` | Directory from the **left**: `[B+0]`=drive letter, `[B+1]`=first directory …; partial ranges `[B+1_2-5]` |
| `[E]` | Extension; `[E1-2]` = characters 1–2 (same range rules as `[N]`) |

**Counter**

| Placeholder | Meaning |
|:--|:--|
| `[C]` | Counter as defined in *Define counter* |
| `[C10+5:3]` | Inline definition: start 10, step 5, width 3. Partial forms `[C10]`, `[C+5]`, `[C:3]` also accepted. Inline values **override** the Define-counter fields. |
| `[C+1/100]` | Fractional counter: increases only every n files (here every 100) — e.g. `[C+1/100]\[N]` moves groups into subdirectories |
| `[Caa+1]` | Alphanumeric counter starting at `aa`, step 1 letter, width from `aa` |
| `[C:a]` | Digits width derived automatically from the number of files; combinations like `[C10+10:a]` allowed |
| `[c]` / `[c1]` | Number of files/folders in the list |
| `[c2]` | Same, but with the digit width from *Define counter* |
| `[c3]` | Last value of the counter defined in *Define counter* |

**Size, time, case, environment, clipboard, plugins**

| Placeholder | Meaning |
|:--|:--|
| `[S]` | File size in bytes |
| `[T1]` | **After this position**: use the renamed file's last-modification date/time for all date/time fields (default) |
| `[T2]` | After this position: current date/time (**when the rename dialog was opened**) |
| `[T3]` | After this position: current date/time (**when the rename options were last changed**) |
| `[T4]` | After this position: EXIF date/time for images and MP4 recording date if available, **otherwise skip the file** |
| `[T1+60m]` | Add/remove a time offset (`+`/`-` then value+unit) |
| `[d]` | Date in current country settings, `/` replaced by a dash |
| `[Y]` / `[y]` | Year 4-digit / 2-digit |
| `[M]` / `[D]` | Month / day, always 2 digits |
| `[t]` | Time in current country settings, `:` replaced by a dot |
| `[h]` / `[H]` | Hour 24h (0–23) / 12h (1–12), always 2 digits |
| `[i]` / `[i1]` | am/pm indicator in English / just `a` or `p` |
| `[m]` / `[s]` / `[v]` | Minutes / seconds (2 digits) / milliseconds (3 digits) |
| `[u]` | Convert composite characters (separate `a` + `^`) to precomposed (`â`); must be the **first** placeholder |
| `[U]` / `[L]` | Everything after this position upper / lower case |
| `[F]` / `[f]` | First letter of each word uppercase (rest lower) / same using English-in-titles rules (`a`, `in`, `the`, … stay lowercase) |
| `[n]` | Everything after this position again as in the original name (case unchanged) |
| `[%x%]` | Value of environment variable `x`; ranges `[%x%1-5]` |
| `[%x%\]` | Same, but only the last part of a path (`c:\temp` → `temp`) |
| `[%x%_]` | Same, but all backslashes replaced by the given character (here `_`) and the `:` removed |
| `[X]` | Clipboard text, with `*?:\|"<>` replaced by `_`; cached and reloaded only when the name field changes or `+`/`#` is clicked |
| `[X+]` | Like `[X]` but empty string when the clipboard is empty or holds no text |
| `[[]` / `[]]` | Literal `[` / literal `]` (the closing one cannot be combined with other commands) |
| `[=pluginname.fieldname.unit]` | Content-plugin field, optional unit or a formatter like `YMD hms` for date/time fields (`v` = milliseconds, only for combined date+time fields); inserted via the `[=?] Plugin` button |
| `[=pluginname.fieldname.unit:4-7]` | Same, partial string (letters 4–7); same range rules as `[N]`, including leading spaces/zeroes |
| `[="prefix"pluginname.fieldname.unit"suffix"]` | prefix/suffix added **only when** the field exists and has a valid value |
| `[=pluginname.fieldname._format]` | Numeric formatting: type `d` decimal, `x` lower hex, `X` upper hex, `o` octal, `b` binary; optional `0` at start keeps leading zeroes, then a group length, then an optional separator (default: control-panel separator for decimal, space otherwise). Examples: `[=p.f._x2]` → `01 FF AB CE`; `[=p.f._x04]` → `0000 0000 01FF ABCE`; `[=p.f._d3,]` → `1,234,567` |

### 6.5 Algorithm / edge cases

1. Build `newbase = name_mask_expanded + "." + ext_mask_expanded`, unless the extension mask is empty
*(inferred from "Fields in name mask + '.' + Fields in extension mask" and the folder-rename note
that the extension mask should be left empty)*.
2. Apply search&replace (in mask order), then case conversion, then the `[U]/[L]/[F]/[f]/[n]`
positional conversions are applied **inside** the mask expansion.
3. Any `\` in the resulting name creates directories.
4. Conflicts are detected at `Start!`; with the TC 9 collision option names become `name (2).ext`,
`name (3).ext` — deliberately not shown in the preview.
5. `Undo` replays the recorded rename list in reverse order and survives closing the dialog.
6. `[N02-9]` example shows the fill is applied so that the *requested* range width is honoured, not the
name length.

**Platform:** [core] the full placeholder set, mask/counter/extension fields, search&replace ordering, collision auto-rename and cross-session Undo. [win] `[T4]` EXIF/MP4 metadata (portable via libraries *(inferred)*). Names are case-sensitive and `/` is the separator on Linux/Android, while the mask uses `\` to create subdirectories — a port must define escaping for literal backslashes *(inferred)*.

---

## 7. Compare and synchronize

Sources: `compare_by_content.htm`, `dlg_compare_contents.htm`, `synchronize_dirs.htm`, `dlg_synchronize.htm`, `dlg_synchronize_dirs.htm`, `sync_ftp.htm`, `synchronize_mlsd.htm`, `options_compare.htm`, `options_match.htm`, `dlg_duplicates.htm`.

### 7.1 Compare by content (`cm_CompareFilesByContent` 2022, `cm_IntCompareFilesByContent` 2040)

**Choosing the two files** (documented precedence):

1. More than one file selected ⇒ the **first two** are compared.
2. Only one file selected ⇒ it is compared with the **first selected file in the target window**.
3. Only one file selected and none selected in the target ⇒ compared with a same-named file in the
other window, if it exists.
4. No file selected ⇒ the file under the cursor is compared with the same-named file in the other
window.

`/S=C` starts the tool standalone (`totalcmd.exe /S=C c:\file1 d:\file2`).

**Behaviour**

- Differences are shown in colour in a dual list; binary files in hex, text files in text mode.
- Identical files ⇒ only a message box.
- Files up to **1 MB** (configurable via `MaxCompareSizeInMem`) are loaded completely, so the originals
stay editable; **larger files are only memory-mapped and remain locked** during the comparison.
- `Next difference` / `Previous difference`: a continuous block of differing lines counts as **one**
difference.
- Both files can be replaced (or dropped by drag&drop onto the edit boxes) and re-compared.
- Search text in both files simultaneously; search continuation; toggle the two windows above each
other; save the changed file(s).
- Multiple compare dialogs can stay open while TC keeps working.

**Options**

| Option | Meaning |
|:--|:--|
| Edit mode (`F6`) | Direct editing of both files, text and Unicode; the two files stay synchronised automatically. Newly added/changed lines get line number 0, existing lines keep their original numbers. On close/re-compare TC asks to save. On save the line-end type (Windows/Unix/Mac) is decided by the **majority** of the original line ends. |
| Show only differences, with extra lines | Shows only lines that differ or were changed, with a configurable number of context lines; the checkbox is hidden in edit mode (temporarily leave edit mode to toggle it). |
| Sync: 1 / 2 / 3 / 0 | How inserted text inside a line is handled when the comparison re-synchronises. TC 10 and older searched for **3 matching characters** and continued there even mid-word (option 1). Now the comparison can resume at a space (2), at a user-defined separator (3), or synchronisation can be disabled (0) — e.g. for tables. |
| Copy -> / Copy <- | Copy the selected text (or the whole line if nothing is selected) into the other window, replacing the corresponding lines; visible only in edit mode. |
| Undo | Undo the last change, down to the original state of both files; the cursor is placed at its position after the last change before actually undoing. `CTRL+Z` or `ALT+BACK`. |
| Binary / 8/16/24/32 Bytes | Binary mode (fixed line width, hex) and bytes per line. |
| ANSI<->ANSI | Choose the text encoding per side (e.g. UTF-8 on one side, ANSI on the other). |
| Case sensitive | Off ⇒ texts differing only in case are **equal** (suitable for Pascal/Basic); on ⇒ different (suitable for C). |
| Ignore repeated spaces | Multiple spaces count as one, leading/trailing spaces ignored entirely (`"test1 test2 "` = `"test1 test2"`). **Only used to decide whether two lines are the same** — once a difference is detected or a line is edited, this option is not used for characters *within* a line. Documented note: in text mode consecutive whitespace/tabs are treated as one anyway unless this option is deactivated. |
| Ignore frequent lines | When searching for new matching lines after a difference, ignore lines that occur very often (e.g. `begin`/`end`), which gives much better results when whole paragraphs were added/removed. |
| Right-click menu | Copy selection to clipboard; Paste here (edit mode only); **Resync comparison from here** (select the two lines, then realign; after a modification Undo is possible only up to this point); Swap Panels. |
| Two-line list | Shows the current line from the active side with its counterpart to spot small differences; scroll with cursor keys/PageUp/PageDown/Home/End/wheel; hide with the status-bar icon or `F9`; `Ctrl+Tab` jumps between the main panels and this list. |

**Hotkeys (window)**

| Key | Action |
|:--|:--|
| `Ctrl+C`, `Ctrl+Insert` | Copy selected text |
| `Ctrl+F` / `F7` | Search |
| `F3` | Continue search |
| `Ctrl+F3` | Reverse search direction |
| `Shift+F3` | Search backwards (next `F3` searches forwards) |
| `Ctrl+U` | Swap panels |
| `Shift+F10`, Menu key | Context menu |
| `Alt+Up` / `Alt+Down` | Previous / next difference |
| `F6` | Enable/disable edit mode |
| `F9` | Enable/disable the two-line list |
| `ESC` | Close dialog |
| Edit mode only | `Ctrl+V`/`Shift+Insert` paste, `Ctrl+S` save dialog, `Ctrl+Z`/`Alt+Backspace` undo, `Ctrl+Left/Right` move by words, `Alt+Left/Right` copy selection to left/right panel, `Delete`/`Backspace`, double-click = select word, triple-click = select line, `F2`/`Shift+F2` = next/previous difference **within a line** |
| Read-only mode only | `Insert`/`Space` select line, `Left`/`Right` scroll 1 character, `Alt+Left/Right` scroll 30 characters |

Encoding dialog (`dialog_choose_encoding.htm`): ANSI (plain text), Unicode UTF-16 (BOM `FF FE`), Unicode MacOS (swapped bytes, BOM `FE FF`), UTF-8 (BOM `EF BB BF`); UTF-8-encoded HTML/XML/CSS is detected automatically by the header. Choosing an encoding only works while the file has not been modified.

### 7.2 Synchronize dirs (`synchronize_dirs.htm`, `dlg_synchronize_dirs.htm`, `cm_FileSync` 2020)

**Purpose:** compare two directories including subdirectories and copy the newer/differing files to the other side (or to any other directory). Documented combinations:

- two directories; a directory and an archive; two archives (**not** all types by content);
- a local directory and an FTP directory; a local directory and a parallel-port directory;
- a local directory and a file-system-plugin directory.

Also useful to verify a burned CD-R against the originals (use "by content").

**Workflow**

1. Choose the two directories and optionally the file types (wildcards in the middle combobox).
2. Select `Subdirs`, `by content` and/or `ignore date`.
3. Press `Compare`.
4. Under `Show`, choose which files are displayed.
5. Select the files to copy — by clicking the checkbox between the comparison columns, or via the
right-click menu. **By default all newer files are selected for copying**; double-click the arrow
to reverse the copy selection.
6. Press `Synchronize` → dialog (`dlg_synchronize.htm`) to copy. **Only visible files are copied.**

**Dialog elements**

| Element | Meaning |
|:--|:--|
| Save directories/settings | To `wincmd.ini`; sorted by name; **FTP connections cannot be saved**; "Only selected" is stored in a **separate list file** whose name is asked for. `F2`. |
| `*.*` wildcard box | Include/exclude types (same mask language as the selection dialog); search filters supported via the button behind the box. |
| `1x`/`2x`/`<-`/`<+`/`->`/`+>` filters | Compare mode when regex or saved search filters may match on only one side: `1x` = show if the filter matches on **at least one** side; `2x` = show when it matches on **both** sides (dupes) or there is only one file (singles); `<-` = must match on the left, files only on the right are not shown; `<+` = must match on the left, but a right-only file must match itself; `->` / `+>` = the mirrored right-side rules. A leading `!` inverts the option. |
| Compare | Starts the comparison. |
| Only selected (in main window) | Compares only what is selected in TC — selecting in one window suffices. Saving this asks for a list-file name; loading shows that name instead of "(in main window)". |
| Empty directories | Also synchronise empty dirs: in symmetric mode missing dirs are copied from the other side; in **asymmetric** mode dirs missing on the left are **deleted** on the right and dirs missing on the right are copied from the left. |
| **Asymmetric** | Assumes the left side is the master: files that do **not exist on the left** are marked for **deletion** on the right. Meant for backups — explicitly **not** for desktop↔notebook synchronisation. |
| Subdirs | Compare subdirectories; changing the option requires a new comparison. |
| by content | Compares content of files with the same size **and** date. Changing it requires a new comparison. Over FTP only with a server announcing `XCRC`, `XMD5`, `MD5` or `XSHA1` via `FEAT`; TC then computes the same checksum type locally. For file-system (non-FTP, non-archive) files a button chooses content plugins: `[++]` = plugin compare **active**, `[>>]` = available but inactive. `[=tc.compare as text]` compares as plain text ignoring Windows/Unix/MacOS line-break differences. Several methods may be combined, AND by default, or OR when the search string starts with `\|`. |
| ignore date | If unset, files are additionally compared by timestamp; **differences of 1–3 seconds are ignored** (NTFS↔FAT32 rounding). If set, same size + same name ⇒ identical; with `by content` also same size ⇒ content compared, and the result is only *equal* / *not equal*, so the user must choose the direction. Changing it requires a new comparison. |
| Show: | Display filters taking effect immediately: **show left→right**, **equal**, **different** (no default direction), **right→left** (or "delete on the right" in asymmetric mode), **duplicates**, **singles**. A filter symbol and the count of hidden files appear in the status bar. Keyboard: `Ctrl+Shift` + `L`, `E`, `N`, `R`, `D`, `S` (except inside path edit boxes). If both `duplicates` and `singles` are up when closing, they are down next time the dialog opens; an exclamation mark warns when duplicates/singles are hidden. |
| FTP time offset `dt=..h` | Only for local↔network comparisons. Positive if the server is to the **west**, negative if to the east. Saved per server name. **With MLSD most servers return UTC, so the offset must be 0.** |
| `mdtm` / `utc` | Use `MDTM <file date>` to set the timestamp of uploaded files (if available, `MFMT` or `SITE UTIME` are preferred); `utc` sends the date/time in UTC (most servers expect UTC, some use local time). |
| `(0/100)` + subdir name | Subdirectory index from the top (0 = base dir) and total count plus name; reflects the focused entry if visible, else the topmost visible one. Right-click scrolls to the next subdirectory block, left-click to the previous. |
| File list | Subdirectories delimited by a grey bar. The middle symbol defines the copy direction (user-choosable). Text colour encodes the **default** direction: **green** = left→right, **blue** = right→left (or delete on the right in asymmetric mode, showing a cross), **red** = different but no direction, **black** = identical. |
| Extra symbols in the list | A symbol shown only when comparing with an FTP server = files identical *only if* the remote file is a Unix text file; a "?" before a pair is byte-compared; a slashed circle = excluded from sync (e.g. files differing only in case, `aa.txt` vs `AA.txt`). |
| Sorting | Click `Name`, `Size`, `Date`; if a side has no file, the other side's file is used for sorting. |
| Rename | Two clicks on a file name (not a double-click) or `F6` (left file) / `Shift+F6` (right file). |
| Copy file properties | Copies only timestamp, attributes and/or NTFS permissions to the other panel. |
| `Ctrl+PageDown`/`Ctrl+PageUp` | Next / previous subdirectory block. |
| Right-click menu | Choose the copy direction for several files at once; display, delete, or compare individual files by content (also by double-click); "Compare left & right" for two unrelated files; for FTP also **"Set local file date to remote date"**. |

**Footer / result summary** — during comparison: number of left+right directories visited; while comparing by content: compared and total files, compared and total size, percentage. After completion: rows with files (same + different + single left + single right), `Identical`, `Different`, `Unique left`, `Unique right`, and in square brackets total files found (left+right) and directories found/scanned.

**Synchronize dialog** (`dlg_synchronize.htm`): target directory per side; prefix the path with `zip:` to pack the files into a ZIP archive (**no other packer supported**). `Confirm overwrites`, `Copy NTFS permissions (may need administrator rights)`. In asymmetric mode: `Right: Delete xxx file(s)` (delete on the right those files that no longer exist on the left) and `Right: Delete all empty directories` (**this also deletes empty directories that still exist on the left**). `OK` starts; holding `Shift` deletes directly when the recycle bin is enabled.

### 7.3 Synchronize with FTP (`sync_ftp.htm`, `synchronize_mlsd.htm`)

| Problem | Documented solution |
|:--|:--|
| Uploading sets the remote file's date to the **current server time**; many servers support no way to set it. | After a successful upload TC sets the **local** file's timestamp to the remote timestamp (only if the server supports none of `MDTM`, `MFMT`, `SITE UTIME`). Consequence: the local directory then **cannot be synced to any other location**, because every upload changes its timestamps. Not done when "ignore dates" is checked; can be disabled with `wcx_ftp.ini` `SyncAllowChangeLocalTime=0`. |
| Server in a different time zone. | Select the time-zone difference in hours before comparing (positive when the server is west). With **MLSD** most servers return UTC ⇒ difference must be **0**. |
| Files uploaded earlier by other tools look different although identical. | Select them, right-click, **"Set local file date to remote date"** — same-size files then compare equal. |
| Unix distinguishes case; Windows does not. | When uploading and a same-named-but-differently-cased file exists on the server, the uploaded file **takes the existing name** (avoiding two files in one directory). To upload a different case you must delete the target first. For files not yet on the server you can choose unchanged or lowercase upload. |
| Unix stores text line ends as 1 character, DOS/Windows as 2 ⇒ sizes differ. | For two files with the **same timestamp but different size**, TC counts the local line ends and computes the Unix text size; if the sizes match, a special symbol shows them as equal. |

**MLSD** (`synchronize_mlsd.htm`): TC uses `MLSD` instead of `LIST` by default. Advantages: 1-second time resolution (LIST often returns only minutes, and for older files only the date), UTC timestamps, standardised format. Caveats: (1) synchronisations made with TC ≤ 7.50a may be based on inaccurate timestamps, so files previously "equal" may now look newer/older — copy just the timestamps from remote to local if you know they are identical; (2) some older servers report symbolic links as files or directories, and Synchronize dirs **ignores links** to avoid infinite loops — with such a server turn off **Use MLSD for directory listings** in Ctrl+F → Edit connection.

### 7.4 Related commands

| Command | Meaning |
|:--|:--|
| `cm_CompareDirs` 533 | Compare directories; newer files selected; files missing in one side are also selected in the other; only newer files are marked. |
| `cm_CompareDirsWithSubdirs` 536 | Also mark subdirs not present in the other directory. |
| `cm_DirMatch` 534 | Mark newer, hide same (see 2.3). |
| `cm_MatchSrc` 532 ("target=Source") | Set the target path to the current source path. With `CTRL+left/right` the directory/archive under the cursor opens in the other window; with no directory under the cursor the shortcut behaves like Target=Source. |
| `cm_Exchange` 531 / `cm_ExchangeWithTabs` 535 | Swap panels / swap panels including tabs. |
| `cm_FileSync` 2020 | Synchronize dirs. |
| `Synchronize dirs` on arguments | `/S=S` with two paths, or `/S=S:SettingsName`; a leading `=` starts the comparison immediately; a lone `=` uses the passed directories and the last used options. `/O=list_name` passes a list file for "Only selected". Button bar: `SYNCOPEN saved_sync_name` (with `=` starts immediately). |

**Platform:** [core] both comparators, the direction/colour model, the `1x`/`2x`/`<-`/`->`/`!` filter matrix, asymmetric backup with deletion, empty-directory handling, copy-file-properties, in-place rename, statistics, settings persistence, and the whole compare-by-content editor including the `Sync:` modes and the majority line-end decision. [win] NTFS permissions, memory-mapped comparison of files >1 MB (`MaxCompareSizeInMem`), recycle-bin-aware `Shift`-delete. The Unix-vs-DOS text-size heuristic and the timestamp/case rules generalise to any remote backend lacking a "set mtime" primitive *(inferred)*.

---

## 8. FTP and remote connections

Sources: `ftp_menu.htm`, `dialog_box__connect.htm`, `dialog_box__firewall_settings.htm`, `dlg_newconnection.htm`, `dlg_connectdetails.htm`, `dlg_configftp.htm`, `wcx_ftp_ini_settings.htm`, `define_server_type.htm`, `options_background_transfer_manager.htm`, `show_admin_shares.htm`, `dlg_network.htm`.

### 8.1 Purpose and model

A **built-in FTP client** handles up to **10 simultaneous FTP servers**; files can also be transferred **directly from one remote server to another**. The remote server is presented as one of the two file panels and is operated with exactly the same functions as local files — `F5` copy, `SHIFT+F6` rename, `F8` delete, `F7` mkdir, `SHIFT+F5` copy within the same directory. `F5` then `F2` copies in the background without affecting the "background" option (the job goes to the last opened background transfer manager).

Supported server types: Unix, Windows NT, VMS, PC/TCP, partially OS9 and AS-400. If no listing appears the type is probably unsupported, and the user is advised to use an external client or to define a custom server type. IPv6 addresses are supported (e.g. `[1234:56:789A:BC::DE]:21`), enabled via `PreferIPv6=` in `wcx_ftp.ini`.

### 8.2 Connection management

| Function | Key / command | Behaviour |
|:--|:--|:--|
| FTP Connect | `CTRL+F` (`cm_FtpConnect` 550) | Dialog listing saved connections; double-click opens one; cursor starts on the last used connection. Distinct icons for plain FTP, secure FTP, and master-password-protected entries. |
| New connection | `CTRL+N` (`cm_FtpNew` 551) | Enter a host name or a URL: `ftp`, `ftps`, `ftpes`, `ftpis`, `http`, `https`. All URL parts except the server name may be omitted (`ftp://somewebhost.com/home/myusername`, `https://www.google.com`, `myself@myserver.com:1021`). A web address is downloaded with HTTP/HTTPS. |
| Disconnect | `CTRL+SHIFT+F` (`cm_FtpDisconnect` 552) | Also disconnects port connections and some file-system plugins. On disconnect TC offers to **save** the current connection. |
| Hidden files | `cm_FtpHiddenFiles` 553 | Requests the listing with `LIST -la`, which shows Unix dot-files. |
| Abort / resume | `cm_FtpAbort` 554, `cm_FtpResumeDownload` 555 | Resume is offered after the normal download dialog and first re-downloads the last few bytes to verify the partial file is unchanged. Not all servers support it. |
| Transfer mode | `CTRL+SHIFT+M` (`cm_FtpSelectTransferMode` 556) | Binary, text, automatic. In automatic mode the **file association** decides. Not settable via an HTTP proxy (the proxy decides by MIME type). |
| Download list | `cm_FtpAddToList` 557 / `cm_FtpDownloadList` 558 | Right-click a remote file → "add to download list"; then "FTP download from list" downloads to a target directory. The list may also target HTTP servers. |

**Connect dialog** (`dialog_box__connect.htm`): saved connections list; `Connect`; `New connection`; `New URL`; `Duplicate entry`; `New folder` (connections moved in by drag&drop); `Edit`; `Delete` (saved immediately); **`Encrypt`** — protect all/selected passwords with a **master password**, or change it. Encrypted entries show a lock icon. "An entered master password is remembered until the user minimises Total Commander, or the screen saver starts." Rationale given: without a master password "a virus or worm may read out your password and modify your Web site".

**Connection details** (`dlg_connectdetails.htm`):

| Field | Meaning |
|:--|:--|
| Session | Arbitrary name; stored in a subfolder it is shown in front of the name separated by `\|`. |
| Host name | Server or IP, optionally `:port`. `ftpes://` forces explicit SSL (AUTH TLS), `ftpis://` forces implicit SSL. Since 11.50 also accepts **file-system plugins** as `\\\Pluginname\Path`, and any directory with a drive letter or UNC path. |
| SSL/TLS | Encrypted connection; OpenSSL `libssl.dll`/`libeay.dll` no longer required on Windows > XP (sChannel); root certificates may be put in `wincmd.pem` next to `wincmd.ini` when using OpenSSL (not used with sChannel). OpenSSL3 is supported since 10.50 (`libssl-3.dll`/`libcrypto-3.dll` in the program directory, **not** the `64` subdirectory). `PROT C` in *Send commands* turns off data-connection encryption. |
| Anonymous login | Sends `anonymous` as user name and the e-mail address as password; TC asks for the e-mail each time the button is pressed. |
| User name / Password | Password may be left empty so TC asks at connect time. |
| Use master password | AES-256 protection of the password, strength estimator shown (dictionary words lower the estimate). Documented escape routes when the master password is unknown (three variants involving the *Change password* button and the Connect dialog's *Encrypt → Remove Encryption*). |
| Remote dir | Directory to enter right after connecting; parts separated by `/`. |
| Local dir | Local directory set in the **other** window after connecting; parts separated by `\`. |
| Use passive mode | Passive = client opens the data connection (helps through firewalls); active = server connects. |
| Use firewall | Opens the firewall dialog (8.3). |
| Send commands | Commands sent after login, `;`-separated. A **literal** semicolon is written doubled: `CWD /test/a;;b;;c;PWD` sends `CWD /test/a;b;c` and `PWD`. |
| Server type | Autodetect normally; `Define new type` configures proprietary listings; `Import from file` loads definitions created elsewhere. Has **no effect when MLSD is used** because that format is standardised. |
| Send command to keep connection alive | A user command every *xx* seconds, sent for **at most 1 hour**. |
| Encoding of file name | Automatic ⇒ TC sends `OPTS UTF8 ON` and reverts to ANSI if unconfirmed; also inspects the `SYST` reply (e.g. `UNIX Type: L8`, `CP:1252`). Manual codepage or a numeric code page may be entered. |
| Use MLSD for directory listings | Three states: grayed = auto-detect via `FEAT`; checked = try MLSD even if unreported or `FEAT` is disabled; unchecked = use `LIST`. |
| Remember contents of all visited directories (in cache) | Refresh only on first entry or when TC itself changes the directory; `F2`/`CTRL+R` refreshes on demand. Uncheck if the server changes often. |
| Client Certificate | Certificate from the Windows certificate store; **only with sChannel** (Windows Vista+), not with the OpenSSL DLLs. |

### 8.3 Firewall / proxy (`dialog_box__firewall_settings.htm`)

Settings are global for all connections; at least one connection must be configured in the connection details to use the firewall. Methods:

| Method | Wire behaviour |
|:--|:--|
| Send command `USER user@hostname` | Firewall/proxy logon (used by e.g. Wingate); the user name and password fields must stay **empty**. |
| Send command `SITE` (with logon) | Send user name + password to the proxy (if given), then `SITE machinename`. User name/password usually needed. |
| Send command `OPEN` | Same but with `OPEN machinename`; user name/password sometimes not needed. |
| `USER user@firewalluser@hostname` / `PASS pass@firewallpass` | Two commands used to log on. |
| Transparent | `USER`/`PASS` with the firewall credentials first, then with the target host credentials. |
| HTTP Proxy with FTP support | Same method as Netscape/IE; not all proxies work because the returned HTML page follows no standard; the most important proxies are supported. Transfer mode cannot be set (the proxy decides by MIME type). |
| SOCKS4 | Usually needs a user name/id but no password; may require an identd server; DNS/name server needed for domain names. |
| SOCKS5 (basic authentication) | Usually user name + password; GSSAPI not supported. |
| `USER user@hostname firewalluser` + `PASS pass` + `ACCT firewallpass` | Three commands used to log on. |
| HTTP CONNECT | FTP tunnel over an HTTP 1.1 proxy supporting `CONNECT`; **preferred** over "HTTP Proxy with FTP support" because it supports all FTP commands including delete and mkdir; only passive mode possible. |

Firewall logon fields: host name (or IP, with optional `:port`, e.g. `firewall.mycompany.com:1021`), user name, password (only stored if the PC is secure; otherwise TC asks at connect time).

### 8.4 FTP configuration (`dlg_configftp.htm`)

| Option | Meaning |
|:--|:--|
| Number of times to retry if server busy | For overcrowded (also authenticated) servers; 0 disables. |
| Delay between retries | Seconds to wait before retrying. |
| Default transfer mode | Upload/download mode; "automatic" uses the extension to decide text vs binary. |
| Open new connections | Always in the currently active panel, or force left / right / inactive (target). |
| Create a log file | Appends the messages of the small FTP log window to a file. |
| All uploads/downloads in the background | Transfer by default in the background. |
| Use passive mode by default | Mainly for firewalls that disallow incoming connections; a newly configured connection's passive setting becomes the new default. Also used by the background transfer manager when adding FTP URLs. |
| PORT mode: only ports from limited range | For firewalls opened for a limited number of incoming PORT connections; at least 100 ports per computer, preferably 1000; ports must be 1024–65535. Used together with the external-address option. |
| PORT mode: external address | Needed only for routers without automatic FTP support. Accepts a static IP, a DynDNS name, or a URL whose page returns the IP as text (example PHP snippet `print $_SERVER["REMOTE_ADDR"];`). |
| Compress during transfer (MODE Z) | ZIP-compatible on-the-fly compression when the server supports it. |
| Not on LAN | Disables MODE Z for the same subnet or private addresses such as `192.168.*.*` — recommended inside a company network. |
| Auto-resume transfer if no data received for | After the given seconds, drop, reconnect and resume. |

### 8.5 `wcx_ftp.ini` settings (`wcx_ftp_ini_settings.htm`) — most relevant keys

Global `[General]`: `AESVerify`, `Allowhook=2` (keyboard hook in the master-password dialog: 0 disallow, 1 allow + show icon, 2 allow + no icon), `Ascii=*.txt *.htm*`, `AutoDisconnect` (0 ask / 1 disconnect previous / 2 keep previous), `AutoResume=30` (or `-1`), `ConnectRetries=10`, `DecodePercent`, `DataConnectTimeout=10` (max 300), `E-Mail=`, `ForceHTTP11`, `FtpInBackground`, `FtpOpenIn` (0 active, 1 left, 2 right, 3 inactive), `FtpShowHidden`, `GetModeFromLocalFile`, `Httptimeout=30`, `KeepAliveTransfer`, `KeepMasterPassword` (1 minimized / 2 screen saver / 3 both), `LogFile`, `LogFile2`, `MDTM=3` (bit 1 enable, bit 2 send UTC), `MLSD=-1|0|1`, `ModeZ` (0 off / 1 Internet / 2 also LAN), `PortFrom`/`PortTo`/`PortIP`/`PortIPEnabled`, `PreferIPv6` (-1 prefer IPv4 with `wsock32.dll`, 0 prefer IPv4, 1 prefer IPv6, 2 order returned by the name server, -2 IPv6 off), `PreferOpenSSL`, `PreventFEAT=0`, `ReturnToReUploadDir`, `SaveNewConnection=1`, `SpecialFlags` (bitmask: 1 implicit SSL, 2 explicit SSL, 4 TLS auth, 8 SSL23 auth, 16 detailed background log, 32 skip SIZE check after upload, 64 skip downloaded-size verification, 128 no SSL session reuse, 256 no *Send commands* in background, 512 `MLSD -la`, 1024 don't convert the PASV address, 2048 duplicate `#255` for Telnet-style escapes, 4096 use MDTM, 8192 use MFMT, 12288 use SITE UTIME), `sslwarn`, `SyncAllowChangeLocalTime=1`, `TcpKeepAlive`, `TransferMode=I` (A/I/X), `UploadBlockSize=1` (1 = auto after 128 kB), `UserAgent`, `VerifyCerts` (1 / 0 / -1 / -2), `WaitDelay=5`, `WarnWrongMode=1`, `ZlibRatioBin=3`, `ZlibRatioTxt=3`.

`[CertAlias]` server→certificate alias (when the user trusts a certificate for a different host). `[default]`: `pasvmode`, `PreserveDates`, `PreserveDatesUpload`, `LowercaseUpload`, `ViaLocalPC`. `[firewall]`: `host`, `password`, `username`, `Method` (order as in the dialog), `default`, `httphost`, `httpuser`, `httppass`. `[connections]` numbered list + `default` = **name** (not number) of the last used connection. `[OldConnections]` recent URLs. Per-site `[SiteN]`: `anonymous`, `directory`, `localdir`, `Encoding` (-1 auto, else a Windows codepage; 65001 = UTF-8), `firewall`, `host`, `password` (scrambled, **not** encrypted — the file states this is impossible by principle), `acct`, `pasvmode`, `username`, `hosttype`, `InitCommands`, `keepalive` (`90,NOOP`), `MLSD`, `CacheDirs`, `ClientCert`, `SpecialFlags`. `[ServerTypeN]` `Template1=`, `Template2=`. `[TimeZone]` per site. `[|empty_folders|]`. `[|FSPlugin|PluginName]` encrypted plugin passwords (allows changing the master password).

### 8.6 Custom server types (`define_server_type.htm`)

Needed when autodetection fails; **not shown when MLSD is used**. Items: a **definition string** parsed against the raw listing, the raw listing itself, optional additional definition strings (TC picks the one with the fewest errors), and an "interpreted data" preview for the selected line. `Load existing type…` imports definitions; definitions can be copied from `wcx_ftp.ini` into a distribution INI for a company (example section `[Unix1]` with `Template1=pppppppppp !S* TTT DD UUUUU n*`).

Definition characters (case matters):

| Char | Meaning |
|:--|:--|
| `n` | file name (`nnnnnnnn` = 8-letter name; `n*` see `*`) |
| `v` | VMS-style name (`name.ext;1`, `dirname.dir;1`) |
| `.` | remove trailing spaces from the name and append following characters separated by a dot |
| `D` `M` `Y` | day, month, year |
| `TTT` | 3-letter month name (English, German, French) |
| `TTT=…` | 12 user month names, 3 letters each (example Polish) |
| `UUUUU` | Unix-style year/time mix (either `2000 ` or `20:30` at the same place) |
| `h` `H` `m` `s` | hours, am/pm modifier letter, minutes, seconds |
| `S` | size; `S=1024` multiplies the size (for servers reporting blocks) |
| `p` | Unix permissions (`-rwxrwxrwx`) |
| `d` | directory flag (char `D`/`d`, e.g. part of `<DIR>`); `d=x` defines the directory character |
| `*` | after a numeric field: all digits up to the first non-digit; for a name not at line end: all chars up to the next space; for a name at line end: to the end of the line |
| `!` | skip until the first space or end of line; `!=x` skip until character `x` (also skipped) |
| `$` | skip spaces/tabs until the next character or end of line |
| `\` | data continues on the next line (max 2 lines) |
| space / `-` | ignore one character; `-` must be used at the beginning of a line instead of a space |
| `x` | exactly one space — a different character rejects the whole line |
| `x=c` / `X` / `X=c` | exactly one character equal to `c` / one non-space / one character not `c` — mismatch rejects the line |

Documented worked samples: AIX Unix (`pppppppppp SSSSSSS TTT DD UUUUU n*`, improved as `pppppppppp !S* TTT DD UUUUU n*` because owner/group names can be longer), PC-NFSD (`nnnnnnnn.nnn dSSSSSSSSSSS MM DD YY hh mmH` — commas in numeric fields are detected automatically; errors up to 2 are tolerated when there is no second definition string), VOS/Stratus (two strings, one `- SSSSS YY MM DD hh mm ss n*` and one `d= SSSSS …`), Unisys Clearpath (two strings, one with `n*\x`).

### 8.7 Background transfer manager (`options_background_transfer_manager.htm`)

Separate window that copies, moves or downloads in the background. Add jobs with `+` (button or numeric keypad) → context menu; `Ctrl+V`/`Shift+Insert` pastes an HTTP URL for immediate download into the active panel's directory. Elements: progress bar (current file), second progress bar (overall; not available e.g. for folder downloads from FTP), listbox with the queue (reorderable by drag&drop **while paused**), `+`, `-`, **Pause**, **Speed limit** (also limits local copies). The `+` menu offers `Add URL to download` (URL or `sleep:10000` pause), `Add selected items for downloading`, `… for copying`, `… for moving`, `Edit list file` (append operations to a `FTP download from list` list; must be ANSI or UTF-8 with BOM). `-` on the first item offers to abort the current transfer and continue with the next; on other items it removes them from the list.

### 8.8 Network / shares (`show_admin_shares.htm`, `dlg_network.htm`)

- **Show admin shares**: shows administrative shares such as `C$` on the remote machine; normally
requires the administrator password of the target; for Win9x/ME remote administration must be enabled
in the target's control panel. [win]
- **System Information: Network**: installed network(s), and for a Microsoft Network user name, computer
name, workgroup and comment; shows "No network installed" when Windows does not directly support the
network (e.g. plain TCP/IP); "doesn't yet work on Windows NT". [win]
- Network commands in the catalogue: `cm_NetConnect` 512, `cm_NetDisconnect` 513, `cm_NetShareDir` 514,
`cm_NetUnshareDir` 515, `cm_AdministerServer` 2204, `cm_ShowFileUser` 2203.

### 8.9 FTP how-tos documented on `ftp_menu.htm`

| Question | Answer |
|:--|:--|
| Resume an aborted **download** | Select the remote file with the partial file shown in the other window, press `F5`; after the normal dialog TC offers resume (or overwrite). TC re-downloads the last bytes to verify the file is unchanged. Server support varies. |
| Resume an aborted **upload** | Only in binary mode (or automatic mode for a binary file). Same procedure. **No guarantee** the file will be correct; some WarFTPd versions appear to resume but produce an unusable file. |
| Copy server→server | Connect both servers so each is in one panel, select, `F5`. Remote copy (FXP) is not supported by all servers and may fail through a firewall. The **left** server receives `PORT`, the **right** one `PASV`, so put the private/firewalled server in the **left** window. |
| Send commands directly to the server | Type the command in TC's command line and press `ENTER` while an FTP panel is shown. `GET`, `PUT`, `RETR`, `STOR` are intercepted and executed directly; `HELP` lists supported commands; lowercase `cd` is translated to the FTP command, uppercase `CD` is TC's own. |
| Refresh a listing | TC caches every listing downloaded in a session, so force a re-read with `F2`/`CTRL+R`. |
| Transfer in the background | Tick "in background" in the download/upload dialog; each transfer opens its own window. **A new connection is required per background transfer** and may fail (e.g. user limit); workaround: foreground download, or start a second TC instance. |
| Define the transfer mode | Toolbar shown after connecting (binary/text/automatic), shortcut `CTRL+SHIFT+M`. Automatic mode uses the file association. Not settable through an HTTP proxy. |
| OTP secure FTP authentication (MD4/MD5/SHA1) | Enter the plain-text password; TC computes and sends the correct response. |

**Platform:** [opt] the FTP client as a whole; [core] the parts a clone must keep — remote panel behaves like a local directory, transfer modes, resume, listing cache plus forced refresh, background job queue. [win] `wcx_ftp.ini`, OpenSSL vs sChannel, the Windows certificate store, the master-password keyboard hook, `wsock32.dll` selection, admin shares/NetBIOS. A Linux/Android port would use `ftp`/`ftps`/`sftp`/`http(s)` libraries while keeping the protocol-agnostic rules (mtime handling, MLSD/LIST trade-off, case collisions, text-size heuristic) *(inferred)*.

---

## 9. File utilities

Sources: `dlg_filessplit.htm`, `dlg_filescombine.htm`, `dlg_filesencode.htm`, `dlg_filesdecode.htm`, `dlg_createsfv.htm`, `dlg_verifysfv.htm`, `dlg_attrib.htm`, `dialog_unix_attribs.htm`, `edit_comment.htm`, `dlg_edit_comment.htm`, `dialog_choose_encoding.htm`, `aes_encryption_information.htm`; commands `cm_Split` 560, `cm_Combine` 561, `cm_Encode` 562, `cm_Decode` 563, `cm_CRCcreate` 564, `cm_CRCcheck` 565, `cm_SetAttrib` 502, `cm_EditComment` 2700, `cm_GetFileSpace` 503, `cm_CountDirContent` 2014.

### 9.1 Split file (`dlg_filessplit.htm`)

| Aspect | Documented behaviour |
|:--|:--|
| Purpose | Split one (big) file into several smaller files, usually of equal size, e.g. for Internet transport or diskettes. Parts can be written to diskettes or a hard-disk directory. |
| Naming | The name is kept, the extension becomes `.001`, `.002`, … ⇒ **max 999 parts**. |
| Target directory | Field may be left empty ⇒ current directory. |
| Bytes per file | Manual size, a predefined size, or `automatic`: the whole free space on removable disks, or **1.44 MB** when splitting to a hard disk. Units `k`, `M`, `G` are binary (1k = 1024). |
| `.crc` side file | After splitting TC creates a file with extension **`.crc`** containing the original file's name **and size** and the **CRC checksum**. Not needed to recombine with other tools, but TC can use it to test that the original was recreated correctly. |
| Delete files on target disk | Removable disks only; deletes all files in the target directory and below, after showing the list and asking for confirmation. |
| External recombination | Plain DOS `copy /b part.001 + part.002 + … original.dat` works. |

### 9.2 Combine files (`dlg_filescombine.htm`)

Put the cursor **on the first partial file** and choose "combine files". Requirements and behaviour:

- File names must be numbered in ascending order or have letters in ascending order. Recognised:
`file.001`, `file.002`, `file.003`; also `fileaa`, `fileab`, `fileac`; also
`filename.00000.zip.part`; also files produced by the Unix `split -b` command.
- If a file with the **same name and extension `.crc`** exists in the same directory as one of the parts,
it is used to verify the combined file, and **the original file name stored in the CRC file becomes the
target name**.
- After combining, the number of found parts is shown in the dialog title.
- Target directory field may be empty ⇒ current directory.

### 9.3 Encode / decode (`dlg_filesencode.htm`, `dlg_filesdecode.htm`)

| Aspect | Encode | Decode |
|:--|:--|:--|
| Purpose | Binary → plain text (programs/archives for e-mail transport) | One file or several partial files → one file |
| Formats | **MIME**, **UUEncode**, **XXEncode** | MIME, UUEncode, XXEncode, **BinHex 4.0**, **yEnc** |
| Output naming | Extension changed to `.B64` (MIME), `.UUE` (UU), `.XXE` (XX) | Target name is read **from the encoded file**; invalid characters changed to `_`; on duplicate names use "auto-rename copied" (`name(2)`, `name(3)` …) |
| Splitting | `Lines per file` sets encoded lines per output file (header lines are **not** counted); empty ⇒ one output file. `Bytes per file` sets the part size (file headers are not counted, so parts are slightly larger than requested). | Parts must be numbered/lettered in ascending order (`file001.uue`, `file002.uue`, …; also `fileaa`, `fileab`, …) |
| Compatibility note | UUE is old and widespread but "doesn't come through everywhere"; XXE avoids UUE's drawbacks but is much less widespread | — |

### 9.4 Create / verify checksums (`dlg_createsfv.htm`, `dlg_verifysfv.htm`)

Create:

| Option | Meaning |
|:--|:--|
| Save CRC files to | Target path and name; the name **must contain `*`** when creating one file per selected file (`*.*.sfv` appends the new extension to the full name/extension). |
| Create separate SFV files for each file | one checksum file per selected file (e.g. to send files to different recipients) |
| Create separate SFV files for each directory | one checksum file per subdirectory; **the target path is ignored** — files are created next to the checked directory |
| Always use UTF-8 in names | otherwise UTF-8 is only used when at least one name needs a different codepage |
| Unix format: line breaks, `/` in paths | default is Windows format (`<cr><lf>`, backslash separators); this option uses `<lf>` and forward slashes |
| Checksum method | **CRC32 (SFV)** — fastest, least secure; **MD5** (stored in lowercase); **SHA1**; **BLAKE3** — fastest cryptographically safe hash, multi-threaded in 64-bit (tunable via `CrcBlake3BlockSize`); **SHA\*** (SHA-2 of the given length, e.g. SHA256); **SHA3_\*** (e.g. SHA3_256) |

Verify ("only errors" checkbox shows only failing rows plus the summary; runs in a background process so TC stays usable; failing files are **selected in the active panel** and highlighted; `Ctrl+Down`/`Ctrl+Up` or the bottom-left arrows move between failures). Three documented ways:

1. Verify using checksum **files** — select them (or put the cursor on one) and start the function.
2. Verify using a checksum/hash on the **clipboard** — copy the code, put the cursor on the file, start
the function. Methods with the same digest length (BLAKE3, SHA2-256, SHA3-256) require selecting the
right method; all suitable methods may be checked simultaneously at a large speed cost.
3. Verify using a checksum contained **in the file name** — the checksum must be in **square brackets**.

Supported formats: CRC32, MD5, SHA1, SHA2, SHA3, BLAKE3; checksum files may be ANSI, UTF-8 (BOM) or UTF-16 (with or without BOM). Hotkeys: `Ctrl+A` select all, `Ctrl+C` copy selected lines, `ESC` abort (with confirmation).

### 9.5 Change attributes (`dlg_attrib.htm`, `cm_SetAttrib` 502)

- Changes attributes and the date/time of the selected files. Hidden/system files are only listed if the
corresponding Configuration option is set.
- `Recurse subdirectories` — with directories selected, also change the attributes of the files inside;
otherwise only the directory attributes themselves.
- Attribute tri-state per flag: **Gray = unchanged, Checked = set, Unchecked = cleared**, for
`Archive (a)`, `Read only (r)`, `Hidden (h)`, `System (s)`. Semantics: the archive bit is set
automatically whenever a file changes; read-only prevents accidental erasure (TC can still delete such
files after asking); hidden files are absent from a `dir` listing; system marks files like
`PAGEFILE.SYS`/`SWAPFILE.SYS` that disk optimizers must not move.
- `Change date/time` — date must follow the shown format; **partial input is supported**: with format
`YYYY.MM.dd` entering `YYYY.05.dd` only changes the month, and any letters a..z can be used as
placeholders (`aaaa.05.aa`). Time: seconds may be omitted (set to 0), fractions of a second up to 3
digits are supported, and partial input works (`hh:10:ss` changes only minutes; any letters b..o and
q..z except `a`/`p` which mean am/pm, and any separators, so `xy.10.xy` works). `Current` inserts the
current date/time.
- **Change plugin attributes**: plugin + property (only writable properties are listed) + value; `+`
reads the value from another plugin (e.g. set the comment to the short name); `F2` loads/saves that
section's settings.
- `Load from file under cursor`: `From active panel (source) (1)` loads attributes from the file under the
cursor in the last active panel — standard attributes only when no plugin options are chosen; `From
target (2)` same for the target panel, active only when the cursor was not on `[..]`.

### 9.6 UNIX attributes (`dialog_unix_attribs.htm`, FTP servers)

Sets permissions via `SITE chmod xyz file.ext` — "the command doesn't work with all servers (only with some UNIX servers)". Matrix of Owner / Group / World × **Read / Write / Execute**, plus `Set UID/GID at startup` (run a program with the owner's/group's rights) and `Sticky bit` (as on `/tmp`: only the file owner, directory owner or administrator may rename/delete). Tri-state: black checkmark = set, gray = unchanged, none = removed. In the file lists UNIX attributes are shown as a **3-digit number** (owner, group, world; 4 = Read, 2 = Write, 1 = Execute; `754` = owner all, group read+execute, world read).

### 9.7 File comments (`edit_comment.htm`, `dlg_edit_comment.htm`, `cm_EditComment` 2700)

- Sets/changes a comment stored in `descript.ion` or `files.bbs`. Comments can be displayed with
**Comments view** (`Ctrl+Shift+F2`) or as a tooltip on the file name (requires Win32-style tips in
Configuration – Display).
- **Only for normal files** — not for files in archives, on FTP servers, or in plugins.
- Reached via the Files menu or `CTRL+Z`. Length limit **4095 characters** with `descript.ion`; a warning
beep and a bold character count appear above **511 characters** because some tools do not support
longer comments.
- Multi-line comments are allowed even with `descript.ion`; the comment is converted to the correct format
automatically. The preferred comment format is set in Configuration – Operation; if only a comment file
of the non-preferred type exists, the comment is written there.
- Unicode comments (UTF-16 / UTF-8) are supported since TC 9; by default comments are stored as plain
ANSI text unless the file name and/or comment contains characters from a different codepage.
- `Use OEM (DOS) font` renders the comment in a DOS font — needed for `files.bbs` comments stored in the
DOS charset (lines, ASCII art).
- `F2`/OK saves, `ESC`/Cancel discards.

### 9.8 Choose encoding (`dialog_choose_encoding.htm`)

Per-side encoding choice for the compare tool when autodetection failed (only while the file is unedited): ANSI (font determines the encoding), Unicode UTF-16 (BOM `FF FE`), Unicode MacOS (byte-swapped, BOM `FE FF`), UTF-8 (BOM `EF BB BF`; HTML/XML/CSS with a UTF-8 header is detected automatically).

### 9.9 AES encryption information (`aes_encryption_information.htm`)

The ZIP AES implementation is Dr Brian Gladman's code (the same code WinZIP uses, FIPS-197 certified; TC itself does **not** hold such a certification). Licence: free distribution/use in source and binary form with the copyright notice retained, otherwise the GPL applies; provided "as is" with no warranty.

### 9.10 Other utility commands

`cm_GetFileSpace` 503 (calculate occupied space: marked files plus all subdirectories; if nothing is selected the cursor entry; sizes rounded up to the **cluster size of the source and destination** directories, directory information itself not counted; unavailable inside archives), `cm_CountDirContent` 2014 (calculate space occupied by subdirs in the current dir), `cm_VolumeId` 505 [win], `cm_VersionInfo` 510 [win], `cm_ShowOnlySelected` 2023.

**Platform:** [core] split/combine, encode/decode, checksum create/verify, attribute+timestamp editing, `descript.ion`/`files.bbs` comments, occupied-space calculation. [win] the Archive/Hidden/System bits, `PAGEFILE.SYS`-style system files, Win32 tooltips, OEM/DOS font rendering. `SITE chmod` on FTP is the *portable* case. Linux/Android mapping *(inferred)*: mode bits + dot-prefix + xattrs; the UNIX-attributes owner/group/world matrix is the natural target model.

---

## 10. Tabs, hotlist, button bar, local menus

Sources: `foldertabs.htm`, `cm_new_folder_tab.htm`, `add_btnbar.htm`, `add_buttons.htm`, `add_menu.htm`, `local_menus.htm`, `dlg_configbuttonbar.htm`, `directoryhotlist.htm`, `dlg_dirmenu.htm`, `dlg_configfoldertabs.htm`, `add_hothist.htm`.

### 10.1 Folder tabs (`foldertabs.htm`)

Purpose: switch quickly between multiple directories, **separately for the left and right panel**. Settings such as the sort order, brief/full details and the selections **are preserved per tab**.

Opening a tab:

| Trigger | Effect |
|:--|:--|
| `CTRL+T` | New tab keeping the current directory (`cm_OpenNewTab` 3001) |
| `CTRL+UP` | Open the folder/archive under the cursor in a new tab (`cm_OpenDirInNewTab` 3003) |
| `SHIFT`+drive selection | Open the drive in a new tab |
| `SHIFT` while using Directory Hotlist (`CTRL+D`) | Open the directory in a new tab |
| Right-click a tab | Duplicate the tab, or copy it to the opposite panel |
| Double-click / middle-click the free space in the tab header | New tab |
| Drag folders onto empty tab-header space | Added as additional tabs; also lets you copy/move to those tabs |

Switching: click, `CTRL+TAB` (next), `CTRL+SHIFT+TAB` (previous), `CTRL+SHIFT+A` (list of all open tabs as a menu, Chrome-style; pressing it again toggles between tab **title** and tab **directory** display), `CTRL+L` (most recently visited tabs at the top; in `dlg_configfoldertabs` this is "Last visited first").

Icons on tabs: configurable via Configuration – Options – Folder tabs; by default for local drives (configured via `desktop.ini` folder properties), virtual folders such as the Desktop, file-system plugins, FTP connections and custom view modes.

Tab context menu: Duplicate this tab · Rename/lock tab · Lock this tab (directory cannot be changed) · Locked, but directory changes allowed (reverts to the saved directory when switching tabs, when clicking the root button, or when clicking the header of an already-active tab) · Copy tab to other panel · Save tabs to file · Save tabs on both sides to file · Load tabs from file, **keep current** (append) · Load tabs from file, **replace tabs** · Close this tab · Close all tabs (**except locked tabs**, current dir kept) · Close duplicate tabs (keeps one tab per identical dir) · Recently closed tabs (only if tabs were closed since start) · Configure view modes · list of view modes.

Other operations: close via `CTRL+W`, middle mouse button/wheel click, double-click, ctrl+click or the context menu; rearrange by drag&drop, even onto the other panel; save/load tab sets from the context menu. Button-bar commands: `APPENDTABS tabfile.tab` (append) and `OPENTABS tabfile.tab` (replace) — append `L`/`R` to target a side; `SAVETABS`/`SAVETABSL`/`SAVETABSR`/`SAVETABS2`/`SAVETABS2L`/`SAVETABS2R` save one side / both sides. To change tabs in both windows by hand, combine two tab files and rename one section from `[activetabs]` to `[inactivetabs]`.

Commands: `cm_OpenNewTab` 3001, `cm_OpenNewTabBg` 3002, `cm_OpenNewTabOther` 3027, `cm_OpenNewTabBgOther` 3028, `cm_OpenDirInNewTab` 3003, `cm_OpenDirInNewTabOther` 3004, `cm_SwitchToNextTab` 3005, `cm_SwitchToPreviousTab` 3006, `cm_MoveTabLeft` 3015, `cm_MoveTabRight` 3016, `cm_CloseCurrentTab` 3007, `cm_CloseAllTabs` 3008, `cm_CloseDuplicateTabs` 3017, `cm_DirTabsShowMenu` 3009, `cm_RenameTab` 3024, `cm_ToggleLockCurrentTab` 3010, `cm_ToggleLockDcaCurrentTab` 3012, `cm_SetTabOptionNormal` 3029, `cm_SetTabOptionPathLocked` 3030, `cm_SetTabOptionPathResets` 3031 (+ `cm_SetAllTabsOption*` 3032–3034), `cm_ExchangeWithTabs` 535, `cm_GoToLockedDir` 3011, tab lists 5515–5518, `cm_SrcActivateTab1`…`10` (5001–5010, "up to 99"), `cm_Trg/Left/RightActivateTab1`…`10`, `cm_NewFolderTab` (`cm_new_folder_tab.htm`: "Shows a new folder tab in the active panel").

Tab configuration (`dlg_configfoldertabs.htm`):

| Option | Meaning |
|:--|:--|
| Show tab header also when there is only one tab | otherwise the header appears only with ≥ 2 tabs, and new tabs need `Ctrl+T` or `Ctrl+Up` on a file |
| Tabs on multiple lines | wrap instead of showing two scroll buttons |
| All tabs have same width | fixed width, Mozilla-like |
| Limit tab length to | maximum characters shown on a tab |
| Confirm close all tabs | ask before closing all via the context menu |
| Close tab on double click | in addition to middle mouse button |
| Ctrl+Up opens tab in foreground | otherwise the new tab is created but not activated (allows opening several subdirs quickly) |
| Ctrl+Tab opens as list | behaves like `Ctrl+Shift+A` |
| Last visited first | only with "Ctrl+Tab opens as list"; most recent at the top |
| Ctrl+Tab switches to last visited | only when the list mode is off; holding `Ctrl` and pressing `Tab` again continues to the right as before |
| Open new tabs near current tab | insert right of the active tab instead of appending |
| Show locked tabs with an asterisk `*` | locked tabs are prefixed with `*` |
| Activate target panel when clicking on one of its tabs | otherwise the other panel stays active |
| Always show drive letter in tab title | e.g. `c:windows`; FTP shows the connection number (`0:tmp`), `#` for a disconnected connection |
| Show icons on tabs | with `>>` to choose folder types |
| Return to locked tab root directory on tab change | for "locked but directory changes allowed"; when off, the user must click the active tab, the `\` button or press `Ctrl+\`. A `!` precedes the title while the current directory differs from the locked one |
| Save displayed file types separately for each tab | otherwise the filter is per panel (as in versions before 10.00) |

### 10.2 Directory hotlist (`directoryhotlist.htm`, `dlg_dirmenu.htm`)

- Opens in three documented ways: the **Commands** menu, **double-clicking the title bar** of a file window
(the current-path display), or **`CTRL+D`**.
- The menu lists previously added directories plus two commands: **Add/remove current dir** and
**Configure** (a dialog to sort/modify/add/remove entries, "built like the dialog box to change the
start menu").
- Editor (`add_dirmenu` page): items can be reordered by putting the cursor on an entry, holding `SHIFT`
and using `up`/`down`, or by dragging. `Add item…` asks for a title (`&` underlines the next letter; a
single `-` inserts a separator), `Add submenu…` creates a submenu (shown with a dash in front, ended by
two dashes), `Delete Item`, `Change Title…`. `Command:` holds `cd Drive:\Directory` (append `\:` or
`\\:` to go to the parent **and** place the cursor on the directory) or an internal command such as
`cm_OpenDesktop`; `Target path:` sets the other window at the same time.
- History variant: the `*` button opens the hotlist, the **arrow-down** button opens the list of recently
visited and frequently used directories (see spec-02 §5.3–5.4 for the full keyboard/behaviour matrix).

### 10.3 Button bar (`add_btnbar.htm`, `dlg_configbuttonbar.htm`)

- Calls menu commands, external programs and other button bars. Hovering shows a tooltip.
- Add buttons by **drag&drop with `SHIFT` pressed**; right-click opens a local menu to change or delete the
button under the mouse.
- Keyboard access: `ALT+SHIFT+F11` (horizontal) / `ALT+SHIFT+F12` (vertical), then cursor keys,
`Tab`/`Shift+Tab`, `Enter`/`Escape`; `Space`, `Shift+F10` or the context-menu key open the context menu.
Commands: `cm_FocusButtonBar` 4004, `cm_FocusButtonBarVertical` 4012.
- A **vertical button bar** exists (TC 9+), configured identically. Hide it via the right-click menu
between/below the icons; restore it by right-clicking the panel separator → "Show vertical button
bar". `cm_VisButtonBar` 2901 / `cm_VisButtonBar2` 2944.
- Default bar file: `default.bar` in the program directory if no location is configured; if not writable
there it is copied next to `wincmd.ini` and used from there. `[options] redirect=0` inside the `.bar`
prevents this. Any `.bar` can be made the default (choose it in "Change button bar", press OK, confirm);
it is then loaded at every start and whenever the bar is hidden/shown.
- Since TC 11, changing a bar notifies **all other running TC instances** to update their bar.

Dialog fields:

| Field | Meaning |
|:--|:--|
| Button bar | file name of the bar; `>>` chooses another bar. The name cannot be changed inside a **subbar**; only the default bar (loaded at startup) can be renamed. `Shift`+arrow keys move buttons; drag&drop also works. |
| Size | Button size (32 = normal icon size); since TC 9.20 stored separately per screen DPI scale factor. |
| XP-Style / Flat icons / Small icons | theme-drawn buttons / no 3D effect (supports themed toolbar buttons) / resize icons to the given size (default 16×16) instead of the button size. |
| Add… / Delete | Insert a new entry directly **behind** the current position / delete the selected icon. |
| Command | One of: a program with path and fixed parameters (`.com`/`.bat` extensions are required because Windows only finds `.exe`), with `*`/`**` prefixes for elevated/other-user start, environment variables supported; **or** one or more internal `cm_*` commands separated by **commas**, e.g. `cd "c:\windows",cm_SelectAll,cm_Copy`, optionally with `cm_wait x` (delay in ms) and with command **numbers** instead of names (`300` = `cm_SrcComments`); `cd directory` / `cd directory\*.txt` to change directory and filter; `cd \\server\share /user:username` (all-lowercase `/user:`); `cd shell:Fonts` or `cd ::{GUID}` for virtual folders; `cd ftp://…` or `FTPOPEN session_name`, `FTPOPEN session\|/subdir`, `FTPOPEN session\|*/subdir` (always active panel); `ZIPFROMLIST` (command = zip file name, parameters = list file; modifiers `-m` move, `-o` overwrite, `-os` skip all, `-on` overwrite older, `-0..-9` compression, `-c0` zip20 / `-c1` AES128 / `-c2` AES192 / `-c3` AES256, `-c0pass`… encrypt with password); `Add subbar >>` to link another bar (circular references are possible but a bar already open for editing must not be edited); `APPENDTABS`/`OPENTABS` (+`L`/`R`); `SYNCOPEN saved_sync_name` (`=` starts immediately, a lone `=` uses current dirs and last options); `LOADSEARCH saved_search_name` (`=` start immediately, `==` show results in TC, empty list if nothing found); `OPENBAR saved_bar` (also `OPENBAR1`, `OPENBAR2`, `OPENBARMENU`; `.\filename.bar` for the current directory; parameter `*` = the default bar); `MULTIRENAME saved_rename_settings` (`=` start immediately, `==` close the tool if no errors, `<UNDO>` reverses the last MRT operation); `OPENCUSTOMVIEW title` (+`L`/`R`/`S`/`T`/`B`, case-sensitive); `SELECTFILES file types` with modifiers `D` only folders, `B` both, `U` unselect, `L` left, `R` right, `T` target, `S` mark folders too when `Shift` is held (e.g. `selectfilesDUT`); `SAVETABS*`; `SAVESELECTION`/`SAVESELECTIONA`/`SAVESELECTIONW`; `SAVEDETAILS`/`SAVEDETAILSA`/`SAVEDETAILSW` (+`H` for headers); `LOADSELECTION`; `OPENLANGUAGEFILE`; `LOADLIST:` / `LOADLIST0:` (no error messages); `OPENATTRIBUTES`; a value of **-2 with no icon** inserts a manual line break. |
| Start path | Path set before starting the program; defaults to the source window path. **If the command is `cd drive:\directory`, the path in this box is set in the *other* window** — enabling both paths at once. |
| Parameters | Fixed parameters must directly follow the file name. Positional substring fields are supported: `%N:~2,5`, `%N:~-8,5`, `%N:~2,5~2`; `%P:~0,-1` cuts the trailing backslash; `-0` is special: `%N:~-0,20` = first 20 characters of the name without extension, `%N:~-0,-20` = first 20 characters of the extension. |
| Special parameters | `?` as first parameter opens an editable confirmation dialog (execution can be prevented). `%P` source path incl. trailing `\`; `%N` file name under cursor; `%T` target path; `%M` target-panel file name; `%O` name without extension; `%E` extension without dot; `%B`/`%B0..%B9` directory from the path (incl. branch-view relative path), `%BT*` same from the target path, `%B-*` path without branch-view relative path (empty in search results), `%B+`/`%B+0..%B+9` from the start (drive letter with/without `:`, then directories/shares); `%S` all selected names (space-containing names quoted; max 32767 characters), `%S<n>`, `%S+<n>`, `%S+<n>-<m>`, `%S-<n>`, `%S-<n>-<m>`; `%P%S` all selected with full path (do not add quotes yourself); `%R` like `%S` from the target panel; `%C1`/`%C2` the compare-by-content operands (with the documented reversal when the right panel is active and fewer than 2 files are selected), `%C3..%C9`, `%c1..%c9` (8.3 names); `%%` a literal percent sign; `%L`/`%l`/`%F`/`%f`/`%D`/`%d`/`%UL`/`%UF`/`%WL`/`%WF` create a **list file in TEMP** and append its name (long/short names, with/without path, DOS charset, UTF-8, UTF-16; `%a` also appends target-panel selections); `%Q` suppress automatic quoting; `%q` strip quotes from `%A1`-style and `%\|envvar\|` parameters (toggles); `%v`/`%V` virtual file name for file-system plugins (with full path incl. plugin name for `%V`); `%X`/`%x` switch the following parameters to left/right instead of source/target and back (`%X%P %T`); `%Y` pass an empty list when nothing is selected, `%y` pass all other parameters except the list parameter; `%Z` allow passing archives as the path for `%P`/`%T` while inside an archive; `%\|envvar\|` inserts an environment variable (the normal `%var%` form is unusable for parameters); alias-only: `%A`, `%A<n>`, `%A-<n>`, `%A<n>+`, `%A<n>-`, `%A<n>-<m>`, `%A-<n>-<m>`. Notes: `%N`/`%M` = long names, `%n`/`%m` = 8.3 aliases; `%P`/`%T` long paths, `%p`/`%t` short paths (and the FTP URL for FTP connections); `%S`/`%R`/`%P%S`/`%T%R` support prefix/suffix text inside the quotes (`"beforename%Sbeforeext%Eafterext"`); `%L`-style parameters support only **one list per command**. |
| Run minimized / Run maximized / Show as menu | start as an icon / full screen / (for `.bar` files) show the linked bar as a popup menu instead of switching to it (`Shift` uses the other option) |
| Icon file / Icon / `->.ico` / Tooltip | Icon source (for Windows programs the `exe` itself; `wcmicons.dll` for internal commands, substituted by the configured icon library; `@anyname.ext` for the default icon of an extension); icon picker listing all icons with numbers (negative = resource IDs); extract the current icon to a separate `.ico` (faster and independent of network availability); tooltip text where `\|` is a manual line break, `\|\|` shows one, and a single space disables the tooltip. |

### 10.4 Function key bar, main menu, local menus

- **Function key bar** — see spec-02 §2. `cm_VisKeyButtons` 2911 hides/shows it; there is no button for
`F1` (help) or `F2` (reread); `F3`/`F4`/`F8` are drag&drop targets; right-clicking `F8` opens the Recycle
Bin [win].
- **Main menu** (`add_menu.htm`) — user-modifiable via **Start → Change main menu**. Menus: Show (or Left
and Right in the old style), Files, Commands, Net, Configuration, Start, Help. The Start menu can hold any
commands and can pass the file under the cursor as the actual parameter. Command `cm_ChangeStartMenu`
700 opens the start-menu editor; `cm_UserMenu1`…`cm_UserMenu10` (701–710, extensible) run its items;
`cm_Config` 490 and the `cm_*Config` family open individual configuration pages.
- **Local (context) menus** (`local_menus.htm`) — available for file windows (also `SHIFT+F10`), the current
path, the drive list/drive buttons, the button bar (edit/delete a single button), the FTP client (add files
to the download list), and the `F8` button (Recycle Bin). With right-button selection the context menu is
reached by holding the button ~1 second. Copy/Cut use the clipboard, and Paste is available in the target
path's context menu. On 64-bit Windows an extra **X64** submenu exposes 64-bit shell extensions to 32-bit
TC (and vice versa in the 64-bit build); which extensions appear there is configurable. [win]

**Platform:** [core] the tab model (per-tab directory, sort, view, filter, selection; the three lock modes; `.tab` files with `[activetabs]`/`[inactivetabs]`), the hotlist/menu editor semantics, the button bar as a command container with the full parameter/placeholder language including `%L`-style temp list files, and local context menus. [win] `desktop.ini`-driven tab icons, XP-style/flat button rendering, `wcmicons.dll` and resource-ID icons, the X64/X32 shell-extension submenu, the Recycle-Bin button menu, DPI-dependent sizing.

---

## 11. Miscellaneous

Sources: `options_sysinfo.htm`, `dlg_sysinfo.htm`, `print_dialog.htm`, `dlg_printoptions_lister.htm`, `drag_drop.htm`, `internal_associations.htm`, `options_assotiate.htm`, `dlg_assotiate.htm`, `save_settings.htm`, `change_settings_files_directly.htm`, `options_memory.htm`, `dlg_configconfirmation.htm`.

### 11.1 System information (`options_sysinfo.htm`, `dlg_sysinfo.htm`)

Dialog with several pages: **hardware**, **software**, **network** (`dlg_network.htm`, see 8.8). Hardware page fields: `CPU Type`, `BIOS Version` (revision date of the system BIOS, used for disk and screen access; on some computers it cannot be retrieved and general BIOS info is shown instead), `Display driver` (board-specific such as "ATI ULTRA" or general purpose VGA/Super VGA), `Display resolution` (width × height in pixels plus the number of possible colours), `Current printer`, `COM/LPT ports` (counts of serial and parallel ports), `Installed RAM` (first number = memory accessible from DOS, second = extended memory accessible from Windows). The page warns that some items show garbage under Windows NT because NT no longer offers BIOS calls. Command `cm_SysInfo` 506. [win]

### 11.2 Printing (`print_dialog.htm`, `dlg_printoptions_lister.htm`)

The same print dialog is used for printing selected file names, a directory tree, and the result of a comparison in Synchronize dirs.

| Element | Meaning |
|:--|:--|
| Font | Font for the file list; the preview is reformatted on every font change (name, size, bold, italic). Printer fonts not present on screen are substituted for display but letter spacing still matches the printer because each letter is drawn individually. |
| Show | Preview-only settings: page to display (upper combobox) and display size (lower). No effect on printing. |
| `<printer name>` | Printer selection. Fonts, character spacing and page borders may differ per printer, so the printer must be chosen **before** printing, not in the dialog that appears after pressing print — otherwise preview and output differ. |
| Setup… | Page format, resolution and other parameters; set these first because they affect layout. Defaults can be set in Control Panel – Printers. |
| Margins… | Page margins plus header content. Margins of 0 make TC use the smallest border the printer supports. Borders can also be dragged in the preview. Header parameters: `%p` current page, `%t` total pages, `%d` base directory of the list, `%D` current date, `%T` current time, `%%` percent sign. |
| Print… / range | `All`, `Pages` (range), `Selection` (prints the current page). |
| Duplex | Prints every second page first, then asks to re-insert the pages to print the backs; the paper must be inserted so the first-printed page is printed first again, on its back. |
| Preview window | Shows the preview and allows dragging margins; to omit the header, set the header text to an empty string under *Margins*. |

Lister print options (`dlg_printoptions_lister.htm`): `Printer` (determines the available fonts), `Font` with separate **Fixed** (used for ANSI and ASCII modes) and **Variable** selections — in ASCII mode all characters are converted from ASCII to ANSI before printing — and `Margins` (adjusted automatically if smaller than the printer minimum).

Commands: `cm_PrintFile` 504, `cm_PrintDir` 2027 (with preview), `cm_PrintDirSub` 2028; `CTRL+F9` prints the file under the cursor using the associated program. [printing is optional on Linux/Android — CUPS or the platform print framework *(inferred)*]

### 11.3 Drag & drop (`drag_drop.htm`)

Cursor feedback: a **slashed circle** = the target will not accept the drop, or the dragged files have the wrong extension; an **arrow with three small squares** = the application will accept the files. Hovering a subdirectory or archive inside TC draws a frame around it, meaning the files will be moved/copied there on release; when the target directory is full of entries, drop slightly above or below the directory row.

Targets and modifiers:

| Drop target / modifier | Effect |
|:--|:--|
| Another file window or a drag&drop-capable application | copy (with confirmation) |
| The `F3`/`F4`/`F8` buttons | same as pressing the corresponding key |
| The button bar, on an icon | start that program with the file (or the first marked file) |
| The button bar, on empty space (or drop with `SHIFT`) | **insert** the file into the bar at that position |
| Subdirectory, drive combobox, drive buttons, the current-directory bar (including its individual name parts and subdirs that open on hover) | copy there |
| `ALT` or `SHIFT` held | **move** instead of copy (minus sign in the drag cursor); alternatively press the right button before releasing the left, then release the left first |
| `CTRL`+`SHIFT` held | create a **shortcut** instead of copying (arrow sign in the drag cursor) [win for `.lnk`] |
| An archive | add the files to the archive |
| A separate tree panel | `Space` expands/collapses a branch during the drag; TC auto-expands a collapsed directory after hovering a moment and the cursor flashes to signal it |
| A printer in the printers folder | print the file (the associated printing program is started) |
| The Windows task bar | cannot drop there; dragging brings the associated program to the front so the drop can happen there [win] |
| Desktop / Explorer folders | allowed; `Ctrl+Shift` creates a link [win] |
| The command line | adds the **short (DOS) name**; `Ctrl+Enter` adds the long name; `Shift` adds the name with full path |

With left-button selection mode the right button can drag too, and dropping shows a menu to choose copy / move / create shortcut. To use drag&drop while in right-button selection mode, `SHIFT` must be held **before** starting the drag.

### 11.4 Associations

**Internal associations** (`internal_associations.htm`, `cm_InternalAssociate` 519) — active only inside TC, without affecting other programs; explicitly recommended for USB-stick use, with `%commander_drive%`/`%commander_path%` making the target program independent of the stick's drive letter.

| Aspect | Behaviour |
|:--|:--|
| List & priority | Defined types are listed with their colour; **sort order defines priority** (first match wins). Double-click or `Space` edits. |
| Quick disable | Types starting with `*` can be toggled by clicking the `*`, which becomes `-.ext` (so a file literally named `-.ext` still matches). |
| `Exe, com, bat, cmd, lnk` | Treated differently: **the default verb cannot be overridden** via internal associations. [win] |
| Multiple selected types, "open" verb | TC uses the **first matching internal association for each file**, possibly a different one per file. Example: `*.txt → Open=notepad.exe` and `*.txt *.rtf → Open=Winword.exe`; selecting `test.txt` + `test.rtf` and choosing Open opens txt with Notepad and rtf with Winword. |
| Multiple selected types, other verbs | The first internal association that matches **all** selected files populates the context menu. In the same example, "Edit all" (from `*.txt *.rtf`) is shown and opens all files with Winword. |
| Load extra commands from all matching types | By default only the first matching type's commands are used; with this option, all extra actions (except open) of all matching types are also shown — e.g. with `*.txt Edit: notepad.exe` and `*.* Wordpad: wordpad.exe`, text files get both `Edit` and `Wordpad`. |
| Add…/Edit…/Delete | Add asks for the type first, then lets you change the default verb (open), the icon, and define additional verbs. |

**Windows associations** (`options_assotiate.htm`, `dlg_assotiate.htm`, `cm_Associate` 507) — the shared system association, also affecting Explorer and the Start menu. Stored in the registry; on Windows 2000+ there are additional private Explorer associations under `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts` created by "Open with", which TC **can read but cannot create or change**. Icons of associated programs can be shown left of the file name. Documented warning: "Associate with…" stops working once the user has used "Open with → Always open with"; Windows then blocks association changes, and only internal associations can override them. Dialog: extension (no leading `*`; a leading dot is optional), `Associate with` (program name incl. search path, or browse), `New type` (browse `*.exe`; for other extensions type `*.com`
+ ENTER), `Edit type` (define extra verbs beyond `open`; not available for viewer associations, default `Alt+F3`), `Delete` (sets "(none)"), and for affected types a `Replace with: PhotoViewer.dll` checkbox (Windows 10, when the Photo app is the viewer of e.g. jpg: opens the Image Viewer desktop program so next/previous image switching works when loaded by other programs). OK writes to the registry and `win.ini`, or to `wincmd.ini` for viewer associations. [win]

### 11.5 Settings persistence (`save_settings.htm`, `change_settings_files_directly.htm`)

| Command | Behaviour |
|:--|:--|
| Configuration – Save settings (`cm_ConfigSaveSettings` 580) | Saves the current file display settings **and directories** even when auto-save is off; also saves the window position and size and the current tabstops in the file windows. **The position is saved separately for each screen resolution.** |
| Save current paths etc. | Same command family; `cm_ConfigSavePos` 493 saves only the position. |
| Save directory history (`cm_ConfigSaveDirHistory` 582) | Stores the directory history (see spec-02 §5.3). |
| Change settings files directly (`cm_ConfigChangeIniFiles` 581) | Opens `wincmd.ini` and `wcx_ftp.ini` for direct editing, to change settings not exposed in the dialogs; in most cases TC must be closed and reopened for the changes to take effect. |
| Confirmation / hotkey remapping (`dlg_configconfirmation.htm`) | Misc page; see 11.6. |

### 11.6 Confirmation, hotkeys, sounds (`dlg_configconfirmation.htm`)

| Field | Meaning |
|:--|:--|
| Get Confirmation before: deleting non-empty directories | asked once per non-empty directory |
| … overwriting files | asked when copying/renaming onto existing files |
| … overwriting/deleting read-only files | asked a **second** time as an extra safety feature ("normally this option should not be disabled") |
| … overwriting/deleting hidden/system files | prevents deleting files the system needs to start, e.g. `swapfile.sys` |
| … Drag&Drop (copying files with the mouse) | when disabled, mouse copies happen immediately without the target-directory dialog |
| Redefine hotkeys (keyboard remapping) | Assign any `cm_`/`em_` command to unused **or already used** shortcut keys, or define a command-line **alias**. `Hotkey:` supports the Windows key where unassigned to a system function (e.g. `Win+E`); click the list twice (to focus and close it) then press the desired key. `OEM_`-prefixed key codes depend on the keyboard layout (e.g. `OEM_US[{` is the key labelled `[{` on a US keyboard). Some hotkeys such as `Ctrl+Letter` are **not** remapped while the cursor is in the command line. The list shows all defined hotkeys; to remove one, select it, click the magnifying glass and choose the very first option (`Predefined command`). `Alias:` e.g. `np` for Notepad. `Command:` an internal or external command via "Choose command"; to undefine, choose `Default command (no remapping)`. Note: TC does **not** show whether a hotkey is used internally, because it uses a more efficient internal hotkey system. `Parameters:` not all commands accept parameters. Variants are expressed through parameters, e.g. choose `OPENTABS` and set the parameter to `L filename.tab` for `OPENTABSL`. The checkmark button (re)defines the hotkey/alias and shows a red `X` for an existing assignment. |
| Custom sounds | Play WAV files at the end of certain operations (packing, copying …). `Configure` opens the sound section of the control panel (there is a separate section for TC, usually at the end of the list). `Only play for long operations` plus `> xx s` restricts playback to operations exceeding the given number of seconds, so short copies are silent. |

### 11.7 Calculate occupied space (`options_memory.htm`)

Calculates the space occupied by the **marked files plus all subdirectories**; with nothing selected, by the file/directory under the cursor. Additionally the file size is **rounded up to the cluster size of the source and destination directory** to get the real space required. The space for the directory information itself is not considered. Not available when the source directory shows an archive. `CTRL+L`; command `cm_GetFileSpace` 503.

**Platform:** [core] settings persistence (display settings + directories + per-screen-resolution window geometry, plus an explicit "save now" independent of auto-save), hotkey/alias remapping with a command picker, the confirmation matrix, occupied-space calculation with cluster-size rounding, internal associations as a per-application type database, drag&drop modifiers. [win] registry associations and the "Always open with" lock, `win.ini`, Windows-key hotkeys, `OEM_` key codes, BIOS/COM/LPT/display-driver system information, the printer/duplex dialog, `.lnk` creation, sound-scheme integration. Linux/Android analogues *(inferred)*: INI/TOML settings file, MIME database or a private mapping, XDG trash, monitor identifier for geometry, `statvfs` block size.
