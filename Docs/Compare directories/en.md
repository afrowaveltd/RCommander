# Compare directories

`Shift+F2` compares the two panels and reports, name by name, whether the entries
are identical, differ, or exist on only one side.

## What is compared

For every name found on either side the comparison looks at the kind (file or
directory), the size and the modification time. The result is a table:

| Marker | Meaning |
| --- | --- |
| `=` | present on both sides and identical |
| `!=` | present on both sides but different |
| `<` | only in the source panel |
| `>` | only in the target panel |
| `<DIR>` | a directory that exists on one side only |

The entries that differ are marked in the active panel, so `F5` (copy), `F8`
(delete) or `Ctrl+M` (multi-rename) can act on exactly those files right after
the comparison. The status line reports how many entries differ.

## Compare or synchronise?

*Compare directories* is a quick, read-only check that leaves you in full control:
it marks the differences and does nothing else. *Synchronise directories* goes
further and lets you copy the differences in either direction, with a dry run and
per-file results.

Use the comparison when you only want to know what changed, and the
synchronisation when you want to make both sides equal.

## Fast byte comparison

For a definitive answer on files that look identical by size and date but are
suspected to differ, open the pair in the previewer (`F3`) after comparing, or
use the synchronise dialog with the *compare by content* option. Timestamps are
not always reliable — a file copied by a tool that does not preserve times will
show as different even when the bytes match.
