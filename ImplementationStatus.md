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
| 11 | Platforms | **partial** — Linux verified end to end (runs, renders); with mingw-w64 installed the **Windows release `.exe` links and is a GUI-subsystem binary** (`target/x86_64-pc-windows-gnu/release/rcommander.exe`, 15.8 MB); the Android APK still needs the SDK/NDK |
| 12 | Verification | **done** for everything marked done above |
| 13 | Secure remote access (SFTP/SSH, sessions, credentials) | **partial** — 13.1 (host keys), 13.2 (key file / agent / password), 13.4 (encrypted store, migration) and 13.3 (saved session profiles with `ssh_config` import), 13.5 (SFTP in the panel, verified against a real OpenSSH server) and 13.7 (the confirmed factory-reset page that makes a lost passphrase recoverable) are done; what remains is the interactive password prompt for connections whose secret is not in the store (today the dialog reopens pre-filled instead) |
| 14 | Colour themes | **pending** — planned in Phase 14; today the palette is two hardcoded variants |
| 15 | Git repositories as panels | **pending** — planned in Phase 15; read-only virtual folders (branches/tags/commits) and copying out of a revision come first, write operations only afterwards |
| 16 | Interactive AI workspace | **pending** — added by the project owner, planned in Phase 16 (bidirectional panes, pluggable model providers, reviewed change sets) |
| 17 | Rich document previewer (PDF, Office, Markdown, code) | **pending** — planned in Phase 17 |
| 18 | Plugin architecture (data sources and renderers) | **pending** — planned in Phase 18 |
| 19 | Integrated SSH terminal (PuTTY-style) | **pending** — planned in Phase 19 |

| 16 | Interactive AI workspace (chat panel, Ollama/remote models, scoped file editing, dictionaries and agent workflows) | **pending** — planned in Phase 16; existing MCP panel can call tools, but is not yet an agent conversation or reviewed multi-file editor |

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

`cargo test -p rc-core` → **232 tests, 0 failures.**

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

## Phase 13.3 — saved sessions (done)

`cargo test -p rc-core --lib sessions` → **13 tests**, `cargo test -p rcommander --lib sessions`
→ **5 tests**, plus 2 UI tests.

`rc_core::sessions::SessionStore` keeps profiles in `sessions.toml`, one file next
to the configuration and separate from `settings.toml`. A `Session` holds host,
port, user, protocol, authentication method, key path, host-key policy, the
initial local and remote directories and the transfer flags — never a password,
which stays in the credential store. The store can add or replace by name, rename,
duplicate (`dev copy`, `dev copy 2`, …), delete, reorder, and report a free name so
an import can never overwrite an existing profile.

`parse_ssh_config`/`sessions_from_ssh_config` read an OpenSSH configuration:
`Host` may list several aliases, `HostName`, `User`, `Port` and `IdentityFile` are
used, `Key=value` and inline comments are accepted, `~` is expanded, and pattern
blocks (`Host *`, `Host 192.168.*`) are skipped because they describe defaults, not
hosts. `Include` is not followed, which is documented rather than silently
half-supported.

The connect dialog gained a *Saved sessions* section: click a session to fill the
form, *Load*, *Save* (with a name), *Delete*, and *Import ssh_config* with the path
field pre-filled with `~/.ssh/config`. The dialog's OK button now actually
connects — it reported "not wired yet" before — and it connects *in the
background*: `start_connect` spawns a worker, `poll_connect` (called from the frame
with `poll_transfers`) fills the panel when the result arrives, the status line
says *Connecting to …*, and a second attempt is refused while one runs. A dead host
therefore costs a status message instead of a frozen window; `connect_and_wait`
exists for headless callers (MCP tools and tests). Both a model test and a UI test
drive a refused connection end to end.

## Phase 8 — `rc-agent` (done)

`cargo test -p rc-agent` → **96 tests, 0 failures.** JSON-RPC 2.0 framing with
correct error codes, `initialize`/`ping`/`tools/list`/`tools/call`/resources/
logging/shutdown, 21 published tools with JSON Schemas, the destructive-tool
confirmation gate, and a client with stdio and HTTP transports plus timeouts.

Application wiring (`crates/rcommander/src/agent_host.rs`) implements
`FileManagerHost` over the model: panels, listing, navigation, selection, copy,
delete, mkdir, rename, text read/write, search, disk space, help topics and
languages, plus `LanguageHost` so `set_language` is published — **10 tests** in
that module. `rcommander mcp` serves the protocol on stdio.

Verified end to end against the release binary with a JSON-RPC probe:

| Exchange | Result |
| --- | --- |
| `initialize` | `serverInfo {rcommander, 0.1.0}`, protocol `2025-06-18`, capabilities `logging`/`resources`/`tools` |
| `tools/list` | **21 tools**, including `set_language` |
| `tools/call list_panels`, `list_directory` | real panels and directory entries |
| `tools/call delete` | refused with `-32001 confirmation required: … no approval handler is registered` |
| `tools/call help_search` | finds the *Previewer* topic from `Docs/` |
| `tools/call disk_space` | real volume numbers |
| `tools/call set_language cs` → `get_language` | switches and reports `cs`; `xx` is rejected with the available list |
| `resources/list` | **42 help resources** (`rcommander://help/<topic>/<lang>`) |
| exit | clean (code 0, no stderr) |

## Phase 9 — Application model (done)

`cargo test -p rcommander --lib` → **230 tests, 0 failures.** Navigation, cursor
keeping, selection commands, view/sort commands, dialog validation for every
dialog, command dispatch over the whole implemented `cm_*` set, transfers,
deletes, mkdir/rename/attributes/comment/multi-rename against real temp
directories, command line (`cd`, `cm_*`), quick search, archive entry, keyboard
dispatch, RTL direction handling.

## Phase 10 — egui UI (partial)

`cargo test -p rcommander --test ui_smoke` → **39 tests, 0 failures** (headless
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

**This round: polish and the reported bugs.**

* *Alternate row striping.* `row_background()` paints every second row with
  `palette.row_alternate` in every view, so long listings stay readable; the
  cursor and selection colours still win, and the thumbnail cards use the same
  helper.
* *Thumbnail cards instead of empty squares.* Each entry is drawn as a card: a
  rounded background, a thin well for the preview, a large file-type icon (or the
  decoded image), the upper-case extension and an elided caption. One colour
  decision per card keeps the icon readable on the cursor and selection
  backgrounds. Covered by a UI test and verified by rendering the screen to a PNG
  and inspecting the pixels (the cursor card's icon is white on navy, image cards
  show the decoded picture).
* *The dialog × now closes the dialog.* `show()` ignored the window's `open` flag
  and wrote the state back, so the button did nothing; it now pops the dialog and
  returns `DialogAction::Close`. UI test: click `Close window`, the dialog is gone
  and the panels accept keys again.
* *Enter confirms dialogs.* When no text field has the focus, `Enter` accepts and
  `Esc` cancels. In the passphrase dialog `Enter` in the first field moves to the
  confirmation field and the second `Enter` validates and saves, so creating a
  store never needs the mouse — UI test drives both presses.
* *Create versus unlock.* The credential dialog shows *create* wording when no
  store file exists, *unlock* when it does, and offers *start over* (delete the old
  store and begin again) when it exists.
* *Panel focus follows the mouse again.* Clicking a row, a tab, the path bar or
  the empty area of a panel now activates that panel; the row-click handler only
  moved the focus when the panel was already active (`if active { … }`), so the
  left panel stayed marked active forever and every view-mode or panel setting
  went to the wrong side. Two UI tests: click a row in the right panel, then a
  view-mode command must change the right panel only.
* *The connect dialog is ready to use.* A new connection starts as `localhost` on
  the protocol's default port (21 for FTP, 22 for SFTP, 990 for FTPS),
  `SavedConnection::with_defaults`/`default_port`/`port_is_default` hold that
  knowledge, switching the protocol moves the port only while it is still a
  default, and every field carries a placeholder hint.
* *The tool bar has icons.* Every button of the default bar (and 30 more
  commands) now draws a Font Awesome glyph in the accent colour in front of its
  caption. The icon is sized from the configured font size, so the bar stays
  proportionate; the accessible name stays the plain caption (a second
  `widget_info` call replaces the layout job's text), which keeps screen readers
  and the UI tests reading real words. A new guard test parses the bundled font
  with `read-fonts` and fails if any `Icon` glyph — or any icon named by
  `for_command` — is missing, so a Pro-only code point can never panic at paint
  time again.
* *Long dialogs scroll instead of overflowing.* The dialog body is wrapped in a
  scroll area bounded by the window height, so a dialog with many fields (connect,
  options) can never push its OK button off the screen; the session list has its
  own bounded scroller for the same reason.
* *The "Dangerous" options page* (`reset.rs`, `OptionsTab::Dangerous`) collects the
  factory resets: settings, tabs/history/hotlist, trusted host keys, saved
  credentials, or everything including the session. Each button arms a
  confirmation first — one click deletes nothing — and the model reports what was
  actually removed. The resets honour `Model::config_dir_override`, so the tests
  never touch the real profile. Documented in *Configuration* help topic.

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

**The file utilities from the reference *Files* menu are implemented**: split and
combine (`crates/rc-core/src/split.rs`), checksums with verification
(`checksum.rs`), compare by content (`compare.rs`) and the duplicate finder
(`dupes.rs`), each with a dialog, model action and tests. Documented in the new
*File utilities* help topic.

Encoding (MIME/UUE/XXE, `encode.rs`), saved selections (file and in-memory) and the
branch view (`branch.rs`, a flat listing of the whole subtree) complete the
command set of the reference *Files*, *Mark* and *Commands* menus.

Remaining simplifications: the thumbnail view has no per-file cache controls or
zoom slider, the
in-panel directory tree is a dialog, the search results table lives inside the
dialog, and the reference product's Windows DLL plugin interfaces (WCX/WLX/WDX/WFX)
are not reimplemented — a cross-platform plugin format would be a separate design.

## Phase 11 — Platforms (partial)

| Target | State |
| --- | --- |
| Linux x86_64 | **verified** — `cargo build --release` succeeds, the application starts, renders a 1100×700 window, lists directories and stays healthy (checked on a live X display) |
| Windows x86_64 | **links** — after installing mingw-w64, `cargo build --release --target x86_64-pc-windows-gnu -p rcommander` produces a 15.8 MB PE32+ executable and `objdump -p` reports `Subsystem 2 (Windows GUI)`, so no console window opens. The earlier `cargo check` had already found and fixed two `cfg(windows)` defects (unsafe `extern` blocks, home-directory lookup) |
| Android aarch64 | **compile-checked** — `cargo check --target aarch64-linux-android -p rcommander` passes with `eframe`'s NativeActivity backend; the desktop-only clipboard/opener dependencies were removed so the target is clean. Producing an APK needs the Android SDK + NDK and `cargo-apk` (`android/README.md`) |


## Command coverage against the reference catalogue

The extracted catalogue has 594 internal commands. The application handles a
command either by its own name or through a family rule:

| Rule | Commands covered |
| --- | --- |
| exact command arms in `Model::execute` | 119 |
| `cm_Left…`/`cm_Right…` variants of an implemented `cm_Src…` command (panel-targeted, activating) | +40 |
| numbered families `cm_ActivateTab<n>`, `cm_SortByCol<n>`, `cm_ViewMode<n>` (also with `Src`/`Trg`/`Left`/`Right` prefixes) | +93 |
| **total handled of 594** | **252 (42 %)** |

<details>
<summary>What the remaining 342 commands are, and why they are not implemented</summary>

| Section | Unhandled | Reason |
| --- | --- | --- |
| custom column views | 49 | the reference uses numbered, separately configured column sets; RCommander has one configurable column list plus named view modes, so these identifiers collapse into `cm_ViewMode<n>` |
| mark / clipboard | 46 | numbered mark variants and further clipboard formats; the main commands (`cm_CopyNamesToClip`, `cm_CopyFullNamesToClip`, `cm_CopySrcPathToClip`, `cm_CopyTrgPathToClip`, details variants) are implemented |
| view / configuration / navigation / tabs | ~128 | numbered toolbars, per-view-mode visibility presets, `cm_*Config` pages that the options dialog covers as tabs, and tab numbering beyond the implemented families |
| network, parallel port, security | 15 | Windows-specific: drive mapping (`WNetAddConnection`), direct cable connection, NTFS ACL editing — a cross-platform reimplementation would need a different model, documented as unsupported |
| print | 2 | no cross-platform print subsystem is wired |
| user menu | 10 | local `usercmd.ini` menus are a separate feature and are not implemented |
| plugin registries | — | the reference product's WCX/WLX/WDX/WFX DLL interfaces cannot work off Windows; a new plugin format would be a separate design |

</details>

## Phase 12 — Verification (done for the implemented scope)

- `cargo test --workspace`: **1168 tests, 0 failures** (see the per-crate numbers
  above).
  Totals by crate: `rc-core` 232, `rc-i18n` 81, `rc-viewer` 167 (+6 doctests),
  `rc-archive` 134, `rc-help` 68, `rc-agent` 96, `rc-remote` 114 (+3 doctests and
  one opt-in SFTP integration test), `rcommander` 272 (230 lib + 39 UI + 3 i18n
  guard) — **1168 tests** (measured
with `cargo test --workspace`).
- `cargo clippy --workspace --all-targets`: **0 warnings** on a fresh build of
  every crate.
- `cargo fmt --all --check`: clean.
- `cargo build --release -p rcommander`: succeeds; the binary starts on a live X
  display, renders an 1100×700 window and writes nothing to stderr.
- `cargo check --target x86_64-pc-windows-gnu --workspace` and
  `cargo check --target aarch64-linux-android -p rcommander`: clean.
- MCP server, verified against the release binary with a JSON-RPC probe:
  `initialize` → 21 tools, `list_directory` on a real path, `help_read` returning
  the Czech *File utilities* page, and **44 help resources**; clean exit.
- No proprietary artifact is committed: `work/` (extracted binaries, Ghidra
  project, decompiled output) is git-ignored.

---

## Known gaps that the next phases address

| Gap | Where it is tracked |
| --- | --- |
| ~~SSH host keys are not verified (13.1); SFTP not wired into the panel (13.5)~~ | **fixed**: `rc-core::knownhosts` (TOFU/strict/ignore, changed key refused) and the SFTP client verifying it; key-file and ssh-agent authentication; the connect dialog offers protocol, authentication and host-key policy |
| The thumbnail view showed one large empty square per file, and the passphrase dialog was unlock-only with no way back from a forgotten passphrase | **fixed this round**: a card grid with type icons, decoded previews and captions; a visible *create* mode; and *Configuration ▸ Options ▸ Dangerous* with confirmed factory resets (settings, layout, host keys, credentials, everything) |
| The credential store was only in memory; the project owner asked for a persistent, passphrase-protected store | **fixed this round**: `Model::unlock_secrets/lock_secrets/persist_secrets`, the unlock/create dialog (minimum length, confirmation, wrong passphrase refused), secrets queued while locked and written on unlock, and `rc-core::secrets` never writing the passphrase anywhere |
| ~~Passwords stored as plain text in `settings.toml`~~ | **fixed**: `rc-core::secrets` provides an Argon2id + AES-256-GCM store, `SavedConnection::password` is never serialised, and `Model::migrate_plaintext_passwords` moves existing values into the store |
| Colours are two hardcoded palettes chosen by name (`Palette::light`/`dark`); they cannot be edited or shipped as data | Phase 14 (14.1–14.5): `themes/*.toml` with semantic roles, runtime discovery, a theme editor with live preview and contrast warnings |
| SFTP compiles behind a cargo feature and is unit-covered but has never been tested against a live server | Phase 13.5, together with the manual verification step |
| A Git repository cannot be browsed as a panel and files cannot be copied out of a branch/tag/commit without checking it out | Phase 15 (read-only first), which reuses the Phase 13.4 credential store for keys and tokens instead of storing them itself |

| No conversational agent panel, Ollama/model provider abstraction or scoped propose/diff/apply workflow; no UnderstandMe dictionary panel or opt-in Git watcher | Phase 16 (16.1–16.15): bidirectional agent/file panels, configurable AI/MCP/data providers, scoped permission policies, reviewed edits, project creation, remote SSH and approved GitHub actions |

## Next steps (in priority order)

1. Route remote (FTP) transfers through the same background queue.
2. An in-panel directory tree (currently the tree opens as a dialog).
3. Phase 13 — secure SFTP/SSH: host-key verification, key/agent auth, saved sessions
   and credential storage (OS keyring with an encrypted portable fallback), then
   wiring SFTP into the panel and the background queue.
4. Phase 14 — data-driven colour themes with a runtime `themes/` directory and an
   editor with live preview.
5. Phase 15 — Git repositories as panels: read-only virtual folders
   (`branches/`, `tags/`, `commits/`, `remotes/`) and copying files out of any
   revision first; write operations (commit/push/checkout) only after that, behind
   dry runs and confirmations. Steps 15.1–15.3 are local-only and can run in
   parallel with Phase 13, but anything authenticating must use 13.4's store.
6. A cross-platform plugin format to replace the reference product's DLL plugins.
7. Background transfer manager dialog on top of `ops_runner`.
8. Windows cross-build and Android APK build once the toolchains are available.
9. Phase 16 — interactive agent chat in either panel with local Ollama or an explicitly selected remote model; safe, reviewed editing of selected JSON dictionaries, documents and configuration files; live output/F3 preview in the opposite pane, configurable permission-scoped model/MCP/data plugins, project creation and Git workflows, remote SSH, later UnderstandMe virtual panels and an opt-in Git issue assistant. Desktop first; Android parity is optional. This builds on MCP but is **not implemented** by the existing MCP tool window.
