# Previewer (F3)

`F3` opens the file under the cursor in the built-in previewer. The previewer
decides what the file *is* from its first bytes, not only from its extension, and
picks the best representation:

| Content | How it is shown |
| --- | --- |
| images (PNG, JPEG, GIF, BMP, WebP) | decoded and scaled to fit the window |
| text (any encoding) | decoded with automatic encoding detection, RTL aware |
| Markdown | rendered with the shared chat/help renderer; Source shows original text |
| CSV / TSV | numbered records in a table, no assumed header |
| JSON / TOML | expandable normalized value tree |
| INI / CFG / CONF | section and key/value tree for the common `key=value` dialect; duplicates retained |
| XML | expandable element/attribute/text tree; DTD and custom entities not supported |
| HTML / SVG | source or XML structure only; visual rendering is not yet available |
| archives, executables, databases, media | described in the status bar; bytes available in hex |
| anything unknown or binary | integrated hex dump |

## Planned visual HTML and SVG previews

HTML should use an optional native WebView backend (candidate: Wry with
WebView2 on Windows, WebKitGTK on Linux and WKWebView on macOS).
The current egui/winit UI needs an explicit GTK/Wayland integration
prototype before making this a default dependency. A static Rust HTML
renderer may be evaluated if it supports the needed CSS subset.

Treat every previewed file as untrusted. Default to passive rendering:
no JavaScript, network requests, external navigation, downloads, host IPC,
or unrestricted local-file access. Resource resolution must be explicitly
restricted and tested, including CSS imports, frames, redirects and
symlinks. If a backend cannot enforce these guarantees, keep Source mode
as the fallback. Loading a file or switching panels must not launch a
browser. Keep an explicit Source mode and bounded resource use.

For SVG, evaluate resvg/usvg (static rendering, MIT/Apache-2.0) with
external resources disabled and a bounded raster size. Verify dependency
licenses and platform packaging before enabling either backend.

## Modes

Choose **Preview**, **Source**, **Hex**, or **Image** on the toolbar. Markdown is cached and bounded; large documents show a limited-preview notice. Links show their destination without launching it. INI previews preserve section and assignment order, including duplicate names; interpolation and includes are never evaluated. Non-assignment dialects, invalid or oversized input fall back to Source. Comments and original whitespace remain available in Source mode. F4 edits the original source with lexical syntax colours.

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
