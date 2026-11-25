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

## Encode and decode

*Encode File* turns a binary file into printable text and *Decode File* turns it
back. Three encodings are supported:

| Encoding | Alphabet | Typical use |
| --- | --- | --- |
| MIME | base64, `A–Z a–z 0–9 + /` | mail attachments, the modern default |
| UUE | the classic UUencode set starting at the space character | files from older Unix tools |
| XXE | `+ - 0–9 A–Z a–z` | variants that avoid shell-hostile characters |

The output keeps the classic envelope (`begin 644 <name>` … `end`) and is wrapped
at 60 characters (base64) or 45 bytes per line (UU/XX). Decoding does not need to
be told which encoding was used: a payload whose line lengths are consistent is
read as UU/XX, anything else is decoded as base64, and the file name from the
envelope is restored — so `report.pdf.b64` becomes `report.pdf` again.

## Branch view

*Commands → Branch View* (`Ctrl+B`) replaces the panel content with a flat list of
everything below the current directory, using paths as names (`src/main.rs`). The
whole subtree becomes one sortable, filterable, markable list — useful to find all
files of a type, to copy a selection out of many folders in one go, or to check
what a folder really contains. Running the command again returns to the normal
listing.

## Saved selections

The *Mark* menu can store the current selection and restore it later
(`Ctrl+Num /` restores the last one), or write it to a file in the current
directory and read it back — handy to keep a set of interesting files across
sessions or to hand it to a script. Entries that no longer exist are dropped when
the selection is restored.
