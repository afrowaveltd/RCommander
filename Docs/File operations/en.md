# File operations

## Copy and move

`F5` copies, `F6` moves. Both open a dialog with the destination pre-filled from
the target panel — you can type another path. Options in the dialog:

- **preserve times** — keep the original modification time,
- **verify** — re-read the written data and compare it with the source,
- **overwrite mode** — replace, skip, rename the new file automatically, or stop
  with an error for each existing file,
- **background** — run the transfer in the background queue so the panels stay
  usable.

A move inside the same file system is a rename and takes no time; across
filesystems the data is copied and the source removed afterwards.

## Delete

`F8` deletes to the platform recycle bin when available and to the trash setting
is enabled, `Shift+F8` deletes permanently. Both ask for confirmation unless you
turn the confirmation off in the options.

## Create and rename

- `F7` creates a directory in the source panel, `Shift+F7` in the target panel.
- `Shift+F6` renames in place, `F6` moves.
- Names with ` (2)` are generated automatically when a copy would collide and
  renaming is selected.

## Attributes and comments

*Change attributes* toggles read-only/hidden/system/archive flags and, on
Linux, the unix permission bits. `Ctrl+Z` edits the file comment, which is
stored in the `descript.ion` file of the directory and therefore travels with
your files.

## Cancelling

Every long operation shows progress and can be cancelled; the partial result
stays on disk and the status line reports what was completed.
