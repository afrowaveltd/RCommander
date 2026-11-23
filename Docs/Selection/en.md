# Selection

Marked files are the input of every operation. When nothing is marked, the file
under the cursor is used.

| Key | Action |
| --- | --- |
| `Insert` | mark / unmark the entry under the cursor |
| `Space` | mark / unmark and move one row down |
| `Num +` | mark all files matching a mask |
| `Num -` | unmark all files matching a mask |
| `Num *` | invert the selection |
| `Ctrl+Num +` | mark everything |
| `Ctrl+Num -` | unmark everything |
| `Alt+Num +` | mark all files with the extension of the cursor file |
| `Alt+Num -` | unmark all files with that extension |

Marks survive a re-read of the directory: entries that no longer exist are
dropped, everything else stays marked. Marks are kept per panel, so each side
can carry its own selection.

The status line always reports `N of M files selected` together with the size of
the marked files, so you can check a selection before deleting or copying.
