# Editor (F4)

`F4` opens the file under the cursor in the internal editor, `Shift+F4` creates a
new file first. An external editor can be configured instead — any command with
`%1` as the file name placeholder works.

## Window controls

The editor (like the agent chat) has its own title row in the top right corner:
**min**, **max**, **pin** and **x**.

- **min** folds the window down to its title bar, so it stops covering the panels
  but keeps what you typed;
- **max** fills the area the two panels occupy, and follows the main window when you
  resize it;
- **pin** does the same and keeps the window in front for as long as you work in it,
  with an accent border so it is obvious which window is pinned;
- **x** closes the window. The text stays in memory until you close it with unsaved
  changes, which asks first.

## What the editor does

- loads and saves text in UTF-8, detecting the original encoding and preserving
  the line endings you opened it with,
- `Enter` inserts the line ending of the document, not of the platform, so a Unix
  file stays a Unix file,
- asks before discarding unsaved changes,
- shows the cursor position and the byte size in the status line,
- **refuses to open a file that is not text**: a NUL byte or invalid UTF-8 makes it
  binary, and editing that would corrupt it. Use `F3` for the hex dump instead.

## When you cannot write the file

The window warns before you type, not after you fail: *this file cannot be written;
saving will offer a copy or escalation*. Saving then reports the system's own error
and offers two honest ways out:

- **Save a copy…** writes the text to your documents folder, with the same file name
  (and `(1)`, `(2)`… if one is already there — a copy never overwrites anything). The
  window then shows the copy's path, because the original is still unchanged.
- **Save as administrator** asks the system for elevated rights: `pkexec` on Linux,
  the UAC prompt on Windows, the administrator dialog on macOS. When elevation is not
  available the message says so instead of pretending to try.

Nothing is ever reported as saved when it was not.

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


## Workspace placement

By default the document covers both file panels. The placement controls move
it to the left or right panel or a floating child window inside the main app.
Moving it preserves the open document and editor buffer. Click the other
panel to use it. A separate OS window and embedded terminal are not included
in this slice.
