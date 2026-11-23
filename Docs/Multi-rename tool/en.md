# Multi-rename tool

`Ctrl+M` renames many files at once. The dialog has three parts: the name
pattern, optional search & replace, and a live preview of the result.

## Placeholders

| Placeholder | Meaning |
| --- | --- |
| `[N]` | original name without extension |
| `[n]`, `[U]` | the same, lower / upper case |
| `[E]`, `[e]` | extension, as is / lower case |
| `[C]` | counter, starting at the configured value |
| `[C+3]`, `[C-3]` | counter of another file in the batch |
| `[C4]` | counter padded to four digits |
| `[P]` | parent directory name |
| `[T1]`…`[T9]` | directory levels upwards |
| `[Y] [M] [D] [h] [m] [s]` | date and time parts |
| `[Q]` | any unknown token stays verbatim |

The extension has its own pattern field, so `[N] — [E]` style output is easy.

## Search and replace

A plain-text search & replace runs on the result of the pattern, which is the
fastest way to strip a prefix or normalise separators. A regular expression mode
is available for harder cases.

## Safety

Nothing is renamed until you confirm. The preview shows the original name next
to the new one for every marked file, collisions are refused, and a file that
would end up with an unchanged name is skipped.
