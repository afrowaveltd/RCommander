# Previewer (F3)

`F3` opens the file under the cursor in the built-in previewer. The previewer
decides what the file *is* from its first bytes, not only from its extension, and
picks the best representation:

| Content | How it is shown |
| --- | --- |
| images (PNG, JPEG, GIF, BMP, WebP) | decoded and scaled to fit the window |
| text (any encoding) | decoded with automatic encoding detection, RTL aware |
| Markdown | rendered with the shared chat/help renderer; Source shows original text |
| HTML, JSON, XML, CSV | source text; a dedicated renderer is not yet available |
| archives, executables, databases, media | described in the status bar; bytes available in hex |
| anything unknown or binary | integrated hex dump |

## Modes

Choose **Preview**, **Source**, **Hex**, or **Image** on the toolbar. Markdown is cached and bounded; large documents show a limited-preview notice. Links show their destination without launching it. F4 edits the original source with lexical syntax colours.

### The hex dump

The dump shows the offset, the bytes in groups and the printable ASCII gutter, and
it is meant to be *read*, not just looked at:

- **the width is yours**: the *bytes/line* slider re-formats the dump immediately,
  from 1 to 64 bytes per row, without re-reading the file;
- **it scrolls properly** in both directions and draws only the visible rows, so a
  multi-megabyte file is no slower than a small one;
- **click a row** to select it: the strip at the bottom shows the offset in hex and
  decimal and that row's bytes as unsigned, signed, 16-bit and 32-bit numbers (little
  endian, which is what x86 file formats mean by those columns), plus the ASCII;
- **go to** takes `0x200`, `512`, `200h` or plain hex and jumps there, which is how
  you follow a header that says "the table starts at 0x200".

A file that is not text opens here directly: there is nothing to decode, and
pretending otherwise would show mojibake.

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


## Workspace placement

By default the document covers both file panels. The placement controls move
it to the left or right panel or a floating child window inside the main app.
Moving it preserves the open document and editor buffer. Click the other
panel to use it. A separate OS window and embedded terminal are not included
in this slice.
