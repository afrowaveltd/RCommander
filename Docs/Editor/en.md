# Editor (F4)

`F4` opens the file under the cursor in the internal editor, `Shift+F4` creates a
new file first. An external editor can be configured instead — any command with
`%1` as the file name placeholder works.

## What the editor does

- loads and saves text in UTF-8, detecting the original encoding and preserving
  the line endings you opened it with,
- `Enter` inserts the line ending of the document, not of the platform, so a Unix
  file stays a Unix file,
- asks before discarding unsaved changes,
- shows the cursor position and the byte size in the status line.

## Keys

| Key | Action |
| --- | --- |
| `Ctrl+S` | save |
| `Ctrl+Z` / `Ctrl+Y` | undo / redo |
| `Ctrl+F` | find |
| `Ctrl+G` | go to line |
| `Ctrl+A` | select all |
| `Tab` / `Shift+Tab` | indent / outdent the selection |
| `Esc` | close (asking about unsaved changes) |

## Editing files inside archives

A file opened from inside an archive is extracted to a temporary place; when you
save it, the archive is repacked with the new version. The same applies to files
opened from an FTP panel: saving uploads the new version back.
