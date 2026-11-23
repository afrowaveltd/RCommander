# File utilities

These tools live in the *Files* menu and work on the marked files, or on the file
under the cursor when nothing is marked.

## Split and combine

*Split File* cuts a file into numbered parts (`report.001`, `report.002`, …) of a
configurable size, which is the classic way to fit a large file onto media or to
send it in chunks. *Combine Files* appends the parts back in numeric order into a
new file.

The numbering is scanned from `.001` upwards and stops at the first gap, so an
incomplete set is detected instead of silently producing a truncated file. The
source file is never modified by either operation, and both honour the cancel
button.

## Checksums

*Create Checksum File(s)* writes a hashes list next to the files. The supported
algorithms are **CRC32** (`.sfv`), **MD5** (`.md5`), **SHA1** (`.sha1`) and
**SHA256** (`.sha256`), and the file name follows the algorithm.

The written format is the `md5sum`/`sha256sum` convention, `<hash>  <name>`, with
paths relative to the current directory so the list verifies from anywhere below
it. *Verify Checksums* reads such a file and reports per file whether the current
content still matches.

The reader also accepts the older **SFV** convention (`<name> <hash>`), so lists
created by other tools verify without conversion. Comments (`#`, `;`) and blank
lines are ignored, and a missing file is reported as a failure instead of being
skipped.

## Compare by content

*Compare By Content* compares two files byte by byte and reports either that they
are identical or the offset of the first difference. For directories use
*Compare Directories* (`Shift+F2`), which compares names, sizes and times, or the
synchronise dialog with *compare by content* for a byte-level check.

## Find duplicates

The duplicate finder searches a directory tree in two stages: files are grouped by
size first, and only files that share a size are hashed. A folder of thousands of
unique files therefore costs almost nothing, while real duplicates are confirmed
by their hash.

The result is a list of groups ordered by wasted space, with the full path of every
copy. Nothing is deleted automatically — you decide what to keep, and the marks can
be used with `F5`, `F8` or the multi-rename tool afterwards.

Hidden files and very small files are skipped by default; both can be changed in
the dialog.
