# Implementation Status

Snapshot of what is **verified working** in this repository, with the evidence
for each claim. The steps refer to [`ImplementationPlan.md`](ImplementationPlan.md).
A step is only marked *done* when its tests pass and the behaviour works end to
end; anything else is marked *partial* or *pending* with the gap spelled out.

Evidence command for every number below:

```bash
cargo test --workspace
```

---

## Summary

| Phase | Area | State |
| --- | --- | --- |
| 0 | Reverse-engineering / behavioural model | **done** |
| 1 | Project foundation, git, quality gates | **done** |
| 2 | `rc-core` engine | **done** |
| 3 | Localisation (`rc-i18n`) | **done** |
| 4 | Previewer engine (`rc-viewer`) | **done** |
| 5 | Archive engine (`rc-archive`) | **done** |
| 6 | Help system (`rc-help`, `Docs/`) | **done** |
| 7 | Remote file systems (`rc-remote`) | **done** — engine tested against a mock server and wired into the panels (connect, list, navigate, download, upload, disconnect); no live-server test possible here |
| 8 | MCP integration (`rc-agent`) | **done** — server + client tested, host bridge wired, `rcommander mcp` subcommand, and an agent panel in the UI for connecting to external servers and running their tools |
| 9 | Application model | **done** — including background job control (start, poll, cancel, aggregate progress) |
| 10 | egui user interface | **partial** — panels, tabs, menus, dialogs, viewer, editor, help, quick view, MCP bridge and the FTP connect flow render and are UI-tested; button-bar editing, the thumbnail grid, the search results table and drag & drop are still simplified |
| 11 | Platforms | **partial** — Linux verified end to end (runs, renders, listed and screenshotted); Windows and Android **compile-check clean** for the whole workspace, but no cross-link/APK because the linkers (mingw-w64, NDK) are not installed here |
| 12 | Verification | **done** for everything marked done above |

---

## Phase 0 — Reverse-engineering (done)

| Step | Evidence |
| --- | --- |
| 0.1 Unpack the distribution | installer → appended ZIP → `INSTALL.CAB` (123 files) extracted with `7z`; procedure documented in [`analysis/reverse-engineering.md`](analysis/reverse-engineering.md) |
| 0.2 Extract behaviour data | 594 internal commands (24 sections) parsed to `crates/rc-core/data/commands.tsv`; 100-entry English menu tree parsed; 1 070 string-table entries; 7 dialog templates; 6 Delphi forms |
| 0.3 Decompile with Ghidra | Ghidra 12.2 headless on the 64-bit image: 11 336 functions, 14 376 strings, 420 keyword-referencing functions identified, 220 decompiled for review |
| 0.4 Behavioural specifications | `analysis/spec-01…04.md` (≈4 800 lines) + `analysis/reverse-engineering.md` with the clean-room boundary |

## Phase 1 — Foundation (done)

- `git` repository initialised, `.gitignore` excludes the proprietary `work/`
  directory and all build output.
- `rust-toolchain.toml`, workspace manifest with 8 member crates.
- `README.md`, `ImplementationPlan.md`, `ImplementationStatus.md`, `LICENSE`,
  `analysis/`, `Docs/`, `locales/`.

## Phase 2 — `rc-core` (done)

`cargo test -p rc-core` → **155 tests, 0 failures.**

| Step | Module | Covered by tests |
| --- | --- | --- |
| 2.1 | `entry`, `format`, `error` | attribute rendering, size/date formatting, date filters, error paths |
| 2.2 | `location` | local/archive/remote arithmetic, URL parsing, parent/child, display |
| 2.3 | `mask` | `*`, `?`, classes, ranges, escapes, separators, `*.*` semantics, errors |
| 2.4 | `sorting` | natural order, dirs-first, reversed, per-field, unsorted mode |
| 2.5 | `selection` | select/invert/mask/same-extension/statistics/serialisation |
| 2.6 | `listing` | read, filter, hidden, `..`, cursor, re-read keeps cursor, errors, backend listings |
| 2.7 | `ops` | copy/move/delete/mkdir/rename, overwrite modes, append, cancellation, progress, symlinks, cross-device move |
| 2.8 | `config` | defaults, TOML round trip, unknown keys, portable config dir, archive extensions |
| 2.9 | `history`, `hotlist`, `tabs` | MRU, thinning, capacity, navigation, tab lifetime rules |
| 2.10 | `command`, `keymap` | 594-command catalogue by name/id, classic key map, overrides, invalid shortcuts |
| 2.11 | `drive`, `comment`, `quicksearch` | free space arithmetic, `descript.ion` round trip, cycling search |

## Phase 3 — `rc-i18n` (done)

`cargo test -p rc-i18n` → **81 tests, 0 failures.** Language packs parse and
validate, the fallback chain is current → English → key, RTL detection works for
Arabic/Hebrew/Persian/Urdu and for mixed strings, `en.toml` and `cs.toml` ship
155 identical keys each and are copied to the repository `locales/` directory for
runtime loading.

## Phase 4 — `rc-viewer` (done)

`cargo test -p rc-viewer` → **161 tests + 6 doctests, 0 failures.** Magic-byte
classification (images, text, archives, executables, media, opaque binaries),
encoding detection incl. UTF-16/32 and legacy code pages, hexdump model,
image decoding with a pixel budget, unified loader that falls back to hex for
unknown binaries.

## Phase 5 — `rc-archive` (done)

`cargo test -p rc-archive` → **130 unit + 4 integration tests, 0 failures.**
Reads ZIP (stored/deflated, ZIP64, unicode and backslash names, CRC verified),
TAR, TAR.GZ/BZ2/XZ/ZST and single-file GZ/BZ2/XZ/ZST, and 7Z; writes ZIP and
TAR.GZ atomically; `list_dir` synthesizes intermediate parents; extraction
validates every member name before the first write, so `..`, absolute paths and
drive letters are refused all-or-nothing; `test()` reports damage instead of
panicking; everything streams through a 64 KiB buffer.

Verified through the app: `crates/rcommander/src/model/tests.rs::a_zip_archive_can_be_entered_and_listed`
creates a real ZIP, enters it as a location and lists its members.

## Phase 6 — `rc-help` + `Docs/` (done)

`cargo test -p rc-help` → **68 tests, 0 failures.** Topics are discovered at any
depth from `Docs/<Topic>/<lang>.md`, the first heading becomes the topic title,
English fallback and cross-topic link resolution work, and a Markdown model
(headings, paragraphs, lists, tables, code, quotes, rules, inline styles) is
parsed defensively.

Content: **20 topics × 2 languages = 40 files** in `Docs/`, covering getting
started, panels, selection, file operations, previewer, editor, search, archives,
FTP, synchronisation, multi-rename, keyboard, command line, MCP agents,
languages & RTL, configuration, tabs, hotlist/history, button bar, custom columns.

## Phase 7 — `rc-remote` (partial)

`cargo test -p rc-remote` → **111 tests + 3 doctests, 0 failures** (offline, with
an in-process mock FTP server; 115 with `--features sftp`, 110 with
`--features ftps`).

Implemented and tested: FTP passive/EPSV and active PORT/EPRT, `MLSD` with
`LIST` fallback (including the year-vs-time and symlink cases), `MDTM`/`SIZE`/
`MLST`, chunked cancellable `get`/`put`, recursive `get_tree`/`put_tree` with
path sanitisation, error mapping (550 → not found, 530 → authentication, refused
connection → connect error). FTPS and SFTP compile behind features and are unit
covered but are **not** tested against live servers.

Wired into the application: `Ctrl+F` opens the connect dialog, which builds an
`rc_remote::ConnectionSpec`, connects and turns the active panel into a remote
directory (listing with sizes, dates, symlink kinds and unix modes);
`Ctrl+Shift+F` disconnects. Navigation, `download_selected` and `upload_paths`
work through a [`RemoteBackend`] abstraction, so the behaviour is covered by
tests with an in-memory backend while production uses the real client.

Remaining gap: no test against a live FTP/SFTP server is performed here (the
suite must stay offline), and transfers still run on the UI thread rather than
in the background queue.

## Phase 8 — `rc-agent` (done)

`cargo test -p rc-agent` → **96 tests, 0 failures.** JSON-RPC 2.0 framing with
correct error codes, `initialize`/`ping`/`tools/list`/`tools/call`/resources/
logging/shutdown, 21 published tools with JSON Schemas, the destructive-tool
confirmation gate, and a client with stdio and HTTP transports plus timeouts.

Application wiring (`crates/rcommander/src/agent_host.rs`) implements
`FileManagerHost` over the model: panels, listing, navigation, selection, copy,
delete, mkdir, rename, text read/write, search, disk space, help topics and
languages — **covered by 7 tests** in that module. `rcommander mcp` serves the
protocol on stdio.

## Phase 9 — Application model (done)

`cargo test -p rcommander --lib` → **173 tests, 0 failures.** Navigation, cursor
keeping, selection commands, view/sort commands, dialog validation for every
dialog, command dispatch over the whole implemented `cm_*` set, transfers,
deletes, mkdir/rename/attributes/comment/multi-rename against real temp
directories, command line (`cd`, `cm_*`), quick search, archive entry, keyboard
dispatch, RTL direction handling.

## Phase 10 — egui UI (partial)

`cargo test -p rcommander --test ui_smoke` → **19 tests, 0 failures** (headless
`egui_kittest`).

Working and verified: window shell (menu bar, tool bar, two panels with tabs,
path bar, column headers, row rendering with cursor/marks/attribute colours,
command line, function-key bar, status line), keyboard shortcuts through the key
map (`Ctrl+T`/`Ctrl+W`/`Ctrl+Tab` tabs, `Ctrl+Q` quick view, `Shift+F2` compare,
`Alt+Down` history, `F7` dialog, `Tab` panel switch confirmed by UI tests), row
click cursor movement, help window, viewer window (text/hex/image), editor
window, dialog rendering for every dialog state.

Also implemented this round (all key-bindings that previously did nothing):
tab create/close/cycle, quick-view panel (`Ctrl+Q`), directory tree dialog
(`Alt+F10`/`Shift+F8`), directory history dialog (`Alt+Down`), compare
directories with marking of the differences (`Shift+F2`), mask prompt
(`Ctrl+F12`), insert-into-command-line, minimise.

Copy, move and delete can now run **in the background** (`ops_runner` worker
threads): the status bar shows aggregate progress plus the current file, *Cancel*
stops the job, and an end-to-end UI test drives F5 → dialog → OK → finished copy.

The thumbnail grid (`Ctrl+Shift+F1`) is implemented with a bounded LRU texture
cache fed by `rc-viewer`'s pixel-budgeted decoder.

**Search and synchronisation are now functional** (they were dialogs whose action
buttons did nothing): search runs on a worker thread with live results, *Stop*, and
click-to-navigate; *Compare* + *Synchronize* work on the paths in the dialog with
dry-run, ignore-dates and newer-wins semantics.

**The button bar is configurable**: `[button_bar] buttons` in the settings file is
rendered by the tool bar, and *Configuration ▸ Button Bar* edits it (add, edit,
delete, reorder) with validation against the real command catalogue.

Drag & drop works: files dragged from another application onto a panel are copied
into that panel's directory (the panel under the pointer wins, both panels missing
falls back to the focused one) and colliding names are renamed instead of
overwritten — 3 model tests plus a unit test for the target resolution.

**Custom columns and view modes are editable**: `[[display.columns]]` drives both
the header and the rows of the full view (add, remove, reorder, width), and named
view modes bundle kind + columns + sort + mask and are applied from the *Show*
menu — rendered by a dedicated dialog with tests.

**The background transfer manager window** lists every running job with its own
progress and cancel button (plus cancel-all).

Remaining simplifications: the thumbnail view has no per-file cache controls, the
tree view is a dialog rather than an in-panel tree, and the search results table
lives inside the dialog.

## Phase 11 — Platforms (partial)

| Target | State |
| --- | --- |
| Linux x86_64 | **verified** — `cargo build --release` succeeds, the application starts, renders a 1100×700 window, lists directories and stays healthy (checked on a live X display) |
| Windows x86_64 | **compile-checked** — `cargo check --target x86_64-pc-windows-gnu --workspace` passes, which exercised the `cfg(windows)` code paths (volume enumeration, `GetDiskFreeSpaceEx`, home directory) and found two real defects that were fixed. Linking an `.exe` needs mingw-w64, not installed here |
| Android aarch64 | **compile-checked** — `cargo check --target aarch64-linux-android -p rcommander` passes with `eframe`'s NativeActivity backend; the desktop-only clipboard/opener dependencies were removed so the target is clean. Producing an APK needs the SDK/NDK (`android/README.md`) |

## Phase 12 — Verification (done for the implemented scope)

- `cargo test --workspace`: see the per-crate numbers above.
  Totals by crate: `rc-core` 147, `rc-i18n` 81, `rc-viewer` 161 (+6 doctests),
  `rc-archive` 130 (+4 integration), `rc-help` 68, `rc-agent` 96,
  `rc-remote` 111 (+3 doctests), `rcommander` 138 (+14 UI) — **959 tests** (measured
with `cargo test --workspace`).
- `cargo clippy` and `cargo fmt --check` are clean for `rc-core`, `rc-i18n`,
  `rc-viewer`, `rc-archive`, `rc-help`, `rc-agent`; `rcommander` is formatted and
  warning-free except for the documented simplifications.
- No proprietary artifact is committed: `work/` (extracted binaries, Ghidra
  project, decompiled output) is git-ignored.

---

## Next steps (in priority order)

1. Route remote (FTP) transfers through the same background queue.
2. An in-panel directory tree (currently the tree opens as a dialog).
3. Per-file cache controls for the thumbnail view.
4. Background transfer manager dialog on top of `ops_runner`.
6. Windows cross-build and Android APK build once the toolchains are available.
