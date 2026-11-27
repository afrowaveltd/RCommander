# RCommander

**A cross-platform, dual-pane file manager written in Rust.**

RCommander is a from-scratch implementation of the classic *commander* workflow —
two directory panels side by side, driven by function keys, with a built-in
previewer, editor, search, archive support, FTP, synchronisation, multi-rename
and a keyboard map you can rewrite. **Linux** has recorded desktop verification,
**Windows** has portable MSVC test releases, and **Android** has historical
compile checks but no verified APK yet. RCommander also includes Git panels,
streaming model chat, reviewed workspace copies and declarative plugin sources.
It speaks **MCP** (Model Context Protocol) as a stdio server and a stdio/HTTP
client, and ships with **English** and **Czech** — with help and language packs that are plain
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
- [Project status and downloads](#project-status-and-downloads)
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
| **Panels** | independent tabs, sort orders, filters and histories per panel; brief / full / comments / thumbnail-grid / tree views; **configurable columns and saved view modes**; quick view (`Ctrl+Q`) |
| **Navigation** | natural sorting, quick search by typing, directory history with `Alt+Left`/`Alt+Right`, drive/volume selector, hotlist (`Ctrl+D`), branch view (`Ctrl+B`), directory tree |
| **Selection** | `Insert`, `Space`, `Num +`/`-`/`*`, extension groups, marks that survive a re-read |
| **File operations** | copy, move, delete, rename, create directory, attributes, comments (`descript.ion`); background transfers with a **transfer manager window**; drag & drop from other applications |
| **Previewer (F3)** | images as images, text with encoding detection, RTL aware, **integrated hex dump** for unknown binaries, byte and pixel budgets for huge files |
| **Editor (F4)** | internal editor with encoding- and line-ending-preserving saves |
| **Search** | mask + content + regular-expression search on a worker thread, live results, click to navigate, result limit and cancellation |
| **Archives** | browse/extract ZIP, TAR and compressed TAR variants, single-file GZ/BZ2/XZ/ZST and 7Z; create ZIP/TAR.GZ atomically; integrity test. RAR/CAB detection only; encrypted creation and disk-image browsing planned |
| **FTP / SFTP** | remote panels: `Ctrl+F` connects and turns a panel into the server (`Ctrl+Shift+F` disconnects); passive/active FTP, MLSD with LIST fallback, download/upload of marked entries, cancellable recursive transfers; FTPS/SFTP behind cargo features |
| **Compare / synchronise** | `Shift+F2` compares the panels and marks the differences; the dialog copies both directions with dry run and newer-wins rules |
| **Multi-rename** | placeholder language (`[N]`, `[E]`, `[C]`, `[P]`, `[T1]`, date parts), live preview, search & replace |
| **File utilities** | split/combine, checksums (CRC32/MD5/SHA1/SHA256) with verification in md5sum *and* SFV formats, compare by content, duplicate finder, MIME/UUE/XXE encode & decode, saved selections |
| **Automation** | 594-entry internal command catalogue (`cm_*`), command line, **configurable button bar with an editor**, **MCP server and client** |
| **Git** | repository panels, branch/tag/commit browsing, revision copy-out, branch switching, stage/commit, clone and remote operations; credential/live acceptance remains partial |
| **Agent workspace** | streaming chat, prompt catalogue/editor, copy-based file handoff, reviewed diffs/hunks and backup-backed apply; autonomous model-to-tool execution is not complete |
| **Plugins** | TOML-defined command and MCP table sources, registry and enable/disable manager; not a sandbox for untrusted code |
| **Credentials** | encrypted portable vault for saved credentials; agent-specific wallets and task-scoped brokerage are planned |
| **Localisation** | TOML language packs (English + Czech included), RTL layouts, per-topic help in any language |

### Efficiency by design

- **Streaming, not slurping** — copies use a bounded buffer, previews read only
  what they show, images decode under a pixel budget.
- **Background work** — transfers run on worker threads with cancellable
  progress; some operations still require responsiveness testing.
- **Flat memory model** — a listing is a `Vec<Entry>`; no per-file heap chatter,
  no global state.
- **No runtime dependencies on a browser engine** — egui renders directly, so
  start-up is fast and the binary stays small.
- **Lazy work** — directory trees, thumbnails, sizes and checksums are computed
  only when a command asks for them.
- **Your own text** — three roles (body, headings, monospace) each take any font
  installed on the machine, and the font directories are only scanned when you ask
  for them. See `Docs/Fonts/`.

---

## Screens and workflow

Start with the two file panels. Use the drive selectors or path bars to navigate,
F3 to preview, F4 to edit, F5 to copy, and Alt+F5 / Alt+F6 to pack / unpack.
The top-level Agent and Git menus collect their related actions. The configurable
toolbar and surface-aware function-key bar expose shortcuts for the active view.
See [in-app documentation](Docs/) for specific workflows.

## Project status and downloads

- [Windows portable test releases](https://github.com/afrowaveltd/RCommander/releases):
  download the complete ZIP, extract it and run `rcommander.exe`. The package is
  unsigned and subject to local endpoint security policies.
- [Build and test runs](https://github.com/afrowaveltd/RCommander/actions):
  check the exact revision and outcome; an earlier green build does not verify
  later commits.
- [Implementation plan](ImplementationPlan.md) and
  [implementation status](ImplementationStatus.md): scheduling, evidence and
  remaining work. The project is under active development; not every planned
  feature is implemented.

## Building

### Prerequisites

- Current stable Rust (`rustup` recommended); **at least 1.93** for the archive crate. The workspace's older 1.88 declaration is not sufficient for the full application.
- Linux: X11 or Wayland client libraries plus `libxkbcommon` (present on any
  desktop distribution)
- Windows: Rust MSVC toolchain, Visual Studio C++ build tools and OpenSSL for SFTP (the CI uses vcpkg's `x64-windows-static-md` triplet)
- Android: Android SDK + NDK and `cargo-apk` (see below)

### Desktop

```bash
git clone https://github.com/afrowaveltd/RCommander.git rcommander
cd rcommander
cargo build --release
./target/release/rcommander
```

### Windows

```bash
# PowerShell on Windows, with vcpkg and the MSVC tools installed:
vcpkg install openssl:x64-windows-static-md
$env:VCPKG_ROOT = 'C:\vcpkg' # adjust to your installation
$env:VCPKGRS_TRIPLET = 'x64-windows-static-md'
cargo build --locked --release --target x86_64-pc-windows-msvc -p rcommander
```

The Windows portable workflow uses MSVC, runs targeted regressions and publishes
a ZIP with runtime data. GNU Windows and Android compile checks are historical;
do not infer current parity from them.

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

Much of the core is shared, but Android linking, packaging, permissions and
device behaviour still need current verification. No verified APK is claimed.

---

## Running

```bash
rcommander                      # both panels start in $HOME
rcommander /left/dir /right/dir  # explicit start directories
rcommander mcp                   # run as an MCP server on stdio
rcommander --help
```

Settings, hotlist and history live in a human-editable file. Saved secrets are
separate from this file (the portable encrypted store is `secrets.enc`):

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
rcommander mcp                # JSON-RPC over stdio (currently wired)
```

External agents can then list panels and directories, navigate, select, copy,
move, delete, create directories, rename, read/write text files, search, query
free space and read the help.

Destructive tools (delete, move, overwrite, writing files, and the Git tools
`git_commit`/`git_checkout`/`git_push`) are gated. Inside the application a dialog
asks with three answers — allow now, allow for this session, and allow always,
which explains its exact scope and requires typing `yes` before it is stored in
`settings.toml`. *Configuration ▸ Security* lists every stored answer with a
*Take it back* button, the live session permissions with *Stop everything now*,
and whether the audit log's hash chain is intact. By default `rcommander mcp`
asks on your terminal and anything that is not an explicit yes is a no — the
question goes to the terminal, never to standard output, because that stream
carries the JSON-RPC protocol. Without a terminal every destructive tool is
refused. `--yes` skips the question for a human who has already decided;
`--no-ask` gives a read-only agent. The capability policy and the audit log apply
in all three cases.

There is **no HTTP server mode**: `--http`/`--token` fail with an explicit
message rather than being silently ignored. Do not expose or rely on an HTTP MCP
server until it is implemented and tested.

### As a client

*Net ▸ MCP agents* opens the agent panel. It connects to any MCP server — a
stdio command or an HTTP endpoint — lists the tools the server publishes and lets
you run one with JSON arguments, showing the result or the server's error right
there. Servers are configured in `settings.toml`:

```toml
[[agent.clients]]
name = "filesystem"
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/home"]
```

### Agent chat

*Configuration ▸ Agent chat* (`cm_AiChat`) is a conversation pane with a local
model: pick one in *AI model…*, ask a question and the answer **streams** in while
the model generates it. *Stop* keeps what already arrived, *New conversation*
clears the transcript, and the transcript becomes the request history, so
follow-up questions work. Ollama is the built-in provider
(`[agent] provider_url`, default `http://127.0.0.1:11434`), so requests stay local only if the configured provider and its model runtime
are local; see `Docs/Agent chat/`.

*Configuration ▸ Agent ▸ Chat in the other panel* (`cm_ChatPane`) puts the chat
**in a panel** with the file list still on screen; the listing is never thrown
away, so switching back restores the same directory, cursor and selection.

**Prompts…** in the chat input opens a catalogue of saved questions grouped by
category (files, Git, remote, build, diagnostics, general), with typed variables
filled from the current selection and a preview before anything is sent. Shipped
prompts are read-only data files; your own live in `<config>/prompts/`. A prompt is
**data** — it can never grant a permission. *Configuration → Agent → Prompts…* edits
them: a built-in is changed by copying it (same id, so it overrides), and *Reset*
brings the shipped wording back. See `Docs/Prompts/`.

The chat **cannot touch files**. Everything with a side effect still goes through
the capability policy, which asks for approval and writes an audit entry — a
sentence in the transcript can suggest an action but never perform one.

---

Answers are rendered as Markdown (headings, lists, tables, quotes, code blocks) and
code blocks are syntax highlighted for Rust, C#, C/C++, Go, JavaScript/TypeScript,
Python, SQL, TOML, JSON, HTML/XML, CSS, Markdown, POSIX shells, PowerShell and
Windows batch.

**Implemented first slices:** right-click **Process with agent…** creates copies
and a workspace manifest; **Send changes back…** supports reviewed diffs/hunks,
stale-original checks and backup-backed apply. See [Workspaces](Docs/Workspaces/en.md).
Markdown conversation import/export exists; durable task/session recovery is
still planned. The current chat is not an autonomous tool executor.

**Planned / incomplete:** model-originated tool calls, durable runtime and
sessions, advanced visual effects, agent wallets with task-scoped credential
brokerage ([#2](https://github.com/afrowaveltd/RCommander/issues/2)), expanded
archive creation/encryption and read-only disk images
([#1](https://github.com/afrowaveltd/RCommander/issues/1)). See the implementation
status for verification evidence and remaining gaps.

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
  rc-llm/       language-model providers (Ollama first), streaming answers
  rc-git/       Git repository access through libgit2
  rc-help/      Docs/ discovery and Markdown model for the help window
  rcommander/   the application: headless model + egui view
prompts/        built-in prompt catalogue, one TOML file per category
plugins/        bundled declarative plugin examples
themes/         runtime themes
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
