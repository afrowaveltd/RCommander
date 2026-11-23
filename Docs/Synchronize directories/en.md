# Synchronise directories

*Commands ▸ Synchronize Dirs* compares the two panels and shows, file by file,
what differs. Use it to mirror a folder, to make a backup, or to compare a local
directory with an FTP server.

## The comparison

For every name that appears on either side you see the sizes and times of both
copies and a direction marker:

| Marker | Meaning |
| --- | --- |
| `=` | identical (size and time) |
| `->` | only in the left panel — copy to the right |
| `<-` | only in the right panel — copy to the left |
| `!=` | present on both sides, different |

Options decide what counts as different: ignore dates, compare content byte by
byte, or run asymmetrically (only copy left→right). A *dry run* shows the plan
without touching any file.

## Running it

Mark the rows you want and press *Synchronize*. Each copy appears in the progress
list with its own result, and failures are reported per file instead of aborting
the whole batch.

## Time zone caveat for FTP

When one side is an FTP server, the server's time zone may differ from yours. If
the server offers no reliable modification time, dates can appear to differ for
files that are in fact identical — compare by content in that case.

## In the dialog

*Compare* (the same algorithm as `Shift+F2`, but on the two paths typed in the
dialog) fills the table; *Synchronize* then copies every row whose marker is
`->`, `<-` or `!=` in the direction the marker points. With `!=` the newer copy
wins. *Dry run* shows the plan and touches no file. *Ignore dates* treats entries
with equal size as identical, which is useful when timestamps are unreliable —
for example right after copying without preserving times.
