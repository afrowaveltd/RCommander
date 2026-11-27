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

### WebView modes and settings (planned)

The same backend should support a passive F3 preview, interactive local web
development, and an explicitly selected browser mode with an address bar,
navigation history, reload, and normal web capabilities where the platform
engine supports them. Do not permanently disable ordinary browser features.
New installations default to passive preview; users can change their defaults
in Settings and override them per WebView session.

Expose independent settings for page JavaScript, network access, local project
resources, navigation/popups, cookies and persistent storage, downloads, and
agent access. Offer three presets without hiding the individual controls:

| Preset | Intended use | Default capabilities |
| --- | --- | --- |
| Passive preview | Untrusted files and routine F3 | No page scripts or network; inert document rendering |
| Development | Local project and localhost testing | Page scripts and explicitly scoped project resources; network opt-in; agent access opt-in |
| Browser | User-directed web browsing | Ordinary web JavaScript, network and navigation; normal browser permissions; agent access opt-in |

These are proposed defaults, not hardcoded restrictions on user-selected
browser functionality. This follows the project-wide capability breadth and
user control principle in [ImplementationPlan.md](../../ImplementationPlan.md). Display the effective mode and permissions in the
toolbar. Use separate browser storage profiles where supported; passive preview
must not inherit authenticated browser sessions. Switching to a more capable
mode must be explicit, and a page must not be able to grant itself permissions.

Page scripts are distinct from privileged RCommander IPC and from agent tools.
Never expose unrestricted filesystem, shell, credentials or agent execution
to document scripts. Route such actions through the existing host-side
authorization layer. Resource resolution must enforce the chosen scope,
including CSS imports, frames, redirects and symlinks; downloads and
device permissions follow user-controlled browser permissions. Keep explicit
Source mode and bounded resource use. If a backend cannot enforce a selected
policy, report the missing capability and fall back safely rather than silently
granting access.

### Agent integration and live web development (planned)

Panel context follows agent placement automatically: a left-panel agent
receives right-panel context, a right-panel agent receives left-panel
context, and an agent outside the panels receives both. A WebView is a
normal typed panel surface within this rule. Context discovery does not
bypass resource, action or remote-model egress permissions.

Provide a platform-neutral WebView session interface so the AI toolkit and
MCP tools can access an explicitly selected viewer session without relying
on a platform-specific WebView implementation.

- Read capabilities: current URL, title, visible text, bounded DOM snapshot,
  selected element, computed CSS, viewport and screenshot; expose bounded
  console diagnostics and network metadata where supported.
- Interactive capabilities: click, type, scroll, navigate and reload within
  the authorized session. Keep them distinct from read-only access.
- Development capabilities: inspect the DOM, apply temporary CSS overrides,
  edit a DOM element, and execute JavaScript when the user grants the
  corresponding permission. Make unsupported engine capabilities explicit.
- Live changes appear in the same visible viewer. Temporary CSS/DOM changes
  are session-local; support undo/reset and a separate explicit action to
  apply a reviewed patch to the original project files. Reuse the existing
  editor/file tools for persistent modifications and reload after saving.
- Expose per-session identifiers, capability discovery, scoped grants and
  revocation. Invalidate document-scoped grants on cross-origin navigation;
  do not share browser cookies, tokens, or other secrets with the model.
  Treat DOM text, page logs and site instructions as untrusted data rather
  than agent authorization. Sensitive submissions, downloads or changes
  with external effects use the existing confirmation/authorization path.
- Keep page JavaScript permission separate from agent script injection;
  neither permission implies the other. A user can enable both for a
  trusted development session. Record agent actions without logging secrets.

Suggested first development loop: inspect an element, inspect computed style,
apply a temporary CSS patch, display the result to the user, iterate, then
save a reviewed stylesheet diff through the editor. The architecture should
also work for a user-directed browser session, subject to its granted
capabilities and the native engine's technical limits.

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
