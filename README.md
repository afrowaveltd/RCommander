# RCommander

**A cross-platform, dual-pane file manager written in Rust.**

RCommander is a from-scratch implementation of the classic *commander* workflow —
two directory panels side by side, driven by function keys, with a built-in
previewer, editor, search, archive support, FTP, synchronisation, multi-rename
and a keyboard map you can rewrite. It runs on **Linux**, **Windows** and
**Android**, speaks **MCP** (Model Context Protocol) in both directions, and
ships with **English** and **Czech** — with help and language packs that are plain
text files you can extend without recompiling anything.

> **Independent project.** RCommander is an independent, clean-room
> implementation. It contains no code, icons, bitmaps or string resources from
> any other file manager, is not affiliated with or endorsed by any commercial
> commander product, and does not use their trademarks. The feature set was
> reconstructed from behavioural and documentation evidence (see
> [`analysis/`](analysis/)) and every line of code here was written for this
> project.

---

## Table of contents

- [Features](#features)
- [Screens and workflow](#screens-and-workflow)
- [Building](#building)
- [Running](#running)
- [Testing](#testing)
- [Language packs](#language-packs)
- [In-app help](#in-app-help)
- [MCP integration](#mcp-integration)
- [Project layout](#project-layout)
- [Reverse-engineering evidence](#reverse-engineering-evidence)
- [Contributing](#contributing)
- [Licence](#licence)

---

## Features

### Two panels, one keyboard

| Area | What you get |
| --- | --- |
| **Panels** | independent tabs, sort orders, filters and histories per panel; brief / full / comments / thumbnail / tree / custom-column views |
| **Navigation** | natural sorting, quick search by typing, directory history with `Alt+Left`/`Alt+Right`, drive/volume selector, hotlist (`Ctrl+D`) |
| **Selection** | `Insert`, `Space`, `Num +`/`-`/`*`, extension groups, marks that survive a re-read |
| **File operations** | copy, move, delete (trash or permanent), rename, create directory, attributes, comments (`descript.ion`) |
| **Previewer (F3)** | images as images, text with encoding detection, RTL aware, **integrated hex dump** for unknown binaries, byte and pixel budgets for huge files |
| **Editor (F4)** | internal editor with encoding- and line-ending-preserving saves |
| **Search** | mask + content search, recursion, regular expressions, archives |
| **Archives** | enter ZIP/TAR/TAR.GZ/TAR.BZ2/TAR.XZ/TAR.ZST like directories; pack and unpack atomically; integrity test |
| **FTP / SFTP** | remote panels: `Ctrl+F` connects and turns a panel into the server (`Ctrl+Shift+F` disconnects); passive/active FTP, MLSD with LIST fallback, download/upload of marked entries, cancellable recursive transfers; FTPS/SFTP behind cargo features |
| **Synchronise dirs** | file-by-file comparison with direction markers, dry run, per-file results |
| **Multi-rename** | placeholder language (`[N]`, `[E]`, `[C]`, `[P]`, `[T1]`, date parts), live preview, search & replace |
| **Automation** | 594-entry internal command catalogue (`cm_*`), command line, editable button bar, **MCP server and client** |
| **Localisation** | TOML language packs (English + Czech included), RTL layouts, per-topic help in any language |

### Efficiency by design

- **Streaming, not slurping** — copies use a bounded buffer, previews read only
  what they show, images decode under a pixel budget.
- **Background work** — transfers run on worker threads with cancellable
  progress; the UI never blocks.
- **Flat memory model** — a listing is a `Vec<Entry>`; no per-file heap chatter,
  no global state.
- **No runtime dependencies on a browser engine** — egui renders directly, so
  start-up is fast and the binary stays small.
- **Lazy work** — directory trees, thumbnails, sizes and checksums are computed
  only when a command asks for them.

---

## Building

### Prerequisites

- Rust **1.88+** (`rustup` recommended)
- Linux: X11 or Wayland client libraries plus `libxkbcommon` (present on any
  desktop distribution)
- Windows: nothing beyond the Rust MSVC toolchain
- Android: Android SDK + NDK and `cargo-apk` (see below)

### Desktop

```bash
git clone <your-remote-url> rcommander
cd rcommander
cargo build --release
./target/release/rcommander
```

### Windows

```bash
cargo build --release --target x86_64-pc-windows-gnu   # with mingw-w64 installed
# or on a Windows machine:
cargo build --release
```

### Android

The application is written against `eframe`, which supports Android through
`winit`'s `NativeActivity`. Building the APK needs the SDK/NDK:

```bash
rustup target add aarch64-linux-android
cargo install cargo-apk
export ANDROID_HOME=$HOME/Android/Sdk
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/<version>
cargo apk build --release --target aarch64-linux-android
```

The desktop core (everything in `rc-core`) is platform-independent and fully
tested on the host, so the Android build only adds the window/input shell.

---

## Running

```bash
rcommander                      # both panels start in $HOME
rcommander /left/dir /right/dir  # explicit start directories
rcommander mcp                   # run as an MCP server on stdio
rcommander --help
```

Settings, hotlist and history live in one human-editable file:

- Linux: `~/.config/rcommander/settings.toml`
- Windows: `%APPDATA%\RCommander\settings.toml`
- Portable: set `RCOMMANDER_CONFIG_DIR` to a directory next to the executable

---

## Testing

Every crate is test-driven; the test suite is the contract:

```bash
cargo test --workspace          # unit + integration tests
cargo test -p rc-core           # engine: listing, sorting, masks, ops, config
cargo test -p rc-viewer         # previewer: detection, decoding, hexdump
cargo test -p rc-archive        # archives: zip-slip safety, round trips
cargo test -p rcommander        # model + UI behaviour
cargo clippy --workspace --all-targets
cargo fmt --check
```

UI behaviour is tested without a display: the whole application state lives in a
headless `Model`, and widget-level tests use `egui_kittest`.

---

## Language packs

A language pack is one TOML file in `locales/`:

```toml
[meta]
code = "de"
name = "German"
native_name = "Deutsch"
direction = "ltr"      # optional; derived from the code otherwise

[strings]
"menu.file" = "Datei"
"dialog.copy.title" = "Kopieren"
"panel.selection_of" = "{count} von {total} Dateien markiert"
```

Drop `de.toml` into `locales/`, restart, and pick it under
*Configuration ▸ Options ▸ Language*. Missing keys fall back to English, so a
partial translation is usable immediately. Right-to-left languages are detected
from the code and the `direction` field, and the whole layout mirrors.

---

## In-app help

`F1` opens the help window, which reads plain Markdown from `Docs/`:

```
Docs/
  Previewer/
    en.md
    cs.md
  FTP connections/
    en.md
    cs.md
  <Topic>/
    <lang>.md
```

Add a folder for a topic and a `<lang>.md` file for each language; the first
heading becomes the topic title. No compilation, no registration step — the app
rescans the directory. Missing translations are shown in English and listed in
the help-completeness report.

---

## MCP integration

### As a server

```bash
rcommander mcp                # JSON-RPC over stdio
rcommander mcp --http 8787 --token <secret>
```

External agents can then list panels and directories, navigate, select, copy,
move, delete, create directories, rename, read/write text files, search, query
free space and read the help. Destructive tools are gated by a confirmation
policy.

### As a client

The agent panel connects to any MCP server (stdio command or HTTP endpoint) and
lets you run its tools from inside the file manager. Configure servers in the
`[agent.clients]` section of `settings.toml`.

---

## Project layout

```
crates/
  rc-core/      engine: entries, listings, sorting, masks, selection,
                operations, configuration, histories, hotlist, tabs,
                command catalogue, keyboard map — no GUI, no I/O threads
  rc-i18n/      language packs, fallback chain, RTL detection
  rc-viewer/    F3 content detection, text decoding, hexdump, image decoding
  rc-archive/   archive reading/writing with path-safety guarantees
  rc-remote/    FTP/SFTP client and connection manager
  rc-agent/     MCP server and client (JSON-RPC, stdio + HTTP)
  rc-help/      Docs/ discovery and Markdown model for the help window
  rcommander/   the application: headless model + egui view
locales/        runtime language packs (en.toml, cs.toml)
Docs/           in-app help, one folder per topic, one file per language
analysis/       reverse-engineering evidence for the reconstructed feature set
```

---

## Reverse-engineering evidence

The feature set was reconstructed from a shipped binary distribution with
Ghidra and static resource/documentation analysis. The evidence trail lives in
[`analysis/`](analysis/):

| Document | Contents |
| --- | --- |
| `spec-01-ui-layout.md` | window anatomy, panel model, view modes, dialog inventory |
| `spec-02-keyboard-commands.md` | complete keyboard map, command-line syntax, 594-entry command catalogue analysis |
| `spec-03-config-settings.md` | full configuration model, 800+ documented options, colour/font/icon model |
| `spec-04-features.md` | behaviour of every feature area: operations, packs, FTP, sync, search, rename, plugins |
| `reverse-engineering.md` | tooling, method, what was extracted, and the clean-room boundary |

Only *behavioural facts* were extracted (command identifiers, option names,
keyboard bindings, dialog structures). No original code, assets or text were
copied; the extracted proprietary artifacts themselves are **not** part of this
repository.

---

## Contributing

1. Every behaviour change needs a test first — the `crates/*/src/**` test modules
   are the specification of record.
2. `cargo test --workspace`, `cargo clippy --workspace --all-targets` and
   `cargo fmt --check` must pass.
3. New user-visible strings go into `locales/en.toml` **and** `locales/cs.toml`;
   new topics go into `Docs/` in every language you can provide.
4. Update [`ImplementationStatus.md`](ImplementationStatus.md) when a step of
   [`ImplementationPlan.md`](ImplementationPlan.md) is finished.

---

## Licence

MIT. See [`LICENSE`](LICENSE).
