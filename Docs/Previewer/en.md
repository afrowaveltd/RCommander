# Previewer (F3)

`F3` opens the file under the cursor in the built-in previewer. The previewer
decides what the file *is* from its first bytes, not only from its extension, and
picks the best representation:

| Content | How it is shown |
| --- | --- |
| images (PNG, JPEG, GIF, BMP, WebP) | decoded and scaled to fit the window |
| text (any encoding) | decoded with automatic encoding detection, RTL aware |
| HTML, Markdown, JSON, XML, CSV | as text, with the format named in the status bar |
| archives, executables, databases, media | described in the status bar; bytes available in hex |
| anything unknown or binary | integrated hex dump |

## Modes

`T` text, `H` hex, `I` image, `A` automatic. The hex dump shows the offset, the
bytes in groups and the printable ASCII gutter; the number of bytes per line is
configurable.

## Keys

| Key | Action |
| --- | --- |
| `F3` / `Enter` | open the next file |
| `F2` | re-read the file |
| `T`, `H`, `I`, `A` | switch the mode |
| `F7`, `Ctrl+F` | find text |
| `F3`, `Shift+F7` | find next |
| `+`, `-` | zoom in / out |
| `Ctrl+0` | reset the zoom |
| `F` | fit the image to the window |
| `W` | toggle word wrap |
| `Esc`, `Alt+F4` | close the window |

## Large files

Only a limited amount of data is read for text and hex previews (8 MiB by
default). The status bar says when the view is truncated, and the exact file size
is always shown. Images are decoded with a pixel budget, so a 200-megapixel image
cannot exhaust memory; instead it is downscaled for display.

## Orientation

When the text is Arabic, Hebrew, Persian or Urdu, the previewer switches to
right-to-left flow automatically. `Ctrl+Right Shift` and `Ctrl+Left Shift`
override the alignment manually.
