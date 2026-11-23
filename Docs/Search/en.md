# Search

`Alt+F7` opens the search dialog. The search runs in the active panel's directory
and, when *include subdirectories* is on, recursively through everything below.

## What you can search for

- **name mask** — wildcards such as `*.log;*.tmp`, or a plain substring when
  wildcards are absent,
- **file content** — a text pattern, optionally regular expression, optionally
  case sensitive or whole words only,
- **inside archives** — read and search archives without extracting them.

## Results

Results appear in a list with the path, size and, for content matches, the
matching line. `Enter` on a result opens its directory in the other panel and
puts the cursor on the file, so you can act on it immediately.

## Limits

The search stops after a configurable number of results (10 000 by default) and
is cancellable at any time. Hidden files and symlinks are only followed when you
ask for it, which prevents infinite loops.
