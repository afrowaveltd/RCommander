# Archives and packing

Archives behave like directories: `Enter` on a `.zip`, `.tar.gz` or `.7z` opens
it and the panel lists its members. `Backspace` leaves the archive again.

## Creating an archive

`Alt+F5` opens the pack dialog for the marked files (or the file under the
cursor). Choose the format — ZIP and TAR.GZ can be created — and the compression
level from 0 (store) to 9 (maximum). The archive is written atomically: it
appears only when it is complete, so a cancelled operation never leaves a
half-written file behind.

## Unpacking

`Alt+F6` unpacks the selected archive (or the marked members of the archive you
are inside) into the target panel. Paths inside the archive are created below the
destination; entries that would escape the destination (`..`, absolute paths) are
refused.

## Supported formats

| Format | Read | Create |
| --- | --- | --- |
| ZIP | yes | yes |
| TAR, TAR.GZ, TAR.BZ2, TAR.XZ, TAR.ZST | yes | TAR.GZ |
| GZ, BZ2, XZ, ZST (single file) | yes | no |
| 7Z | yes (when the backend is enabled) | no |
| RAR, CAB | detected and described only | no |

`Alt+Shift+F9` tests an archive for integrity without extracting it.
