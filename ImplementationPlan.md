# Implementation Plan

This plan describes how RCommander is built, step by step. Each step lists what
is produced and its **definition of done** (DoD). A step may only be marked
complete in [`ImplementationStatus.md`](ImplementationStatus.md) when its DoD is
met — normally "the listed tests pass and the behaviour works end to end".

Development is **test-first**: for every step the tests are written (or extended)
before the implementation, and no step is accepted with a failing or skipped
test.

---

## Phase 0 — Reconstruction of the feature set

The product functionality is reconstructed from a shipped Windows binary
distribution using static analysis, not from source code.

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 0.1 | Unpack the distribution (self-extracting installer → payload cabinet) | extracted application binaries, data files, help | payload extracted, file inventory reproducible from a documented command sequence |
| 0.2 | Extract machine-readable behaviour data: internal command catalogue, menu layout, string tables, dialog resources, keyboard map, help topics | `analysis/` raw data + parsed JSON | command catalogue parsed (name/id/section), menu tree parsed, help TOC parsed |
| 0.3 | Decompile the application with Ghidra (headless) and locate the subsystems behind the interesting strings (viewer, masks, sorting, transfers) | decompiled function inventory + selected functions | analysis completes, function/string cross-reference list produced, findings recorded |
| 0.4 | Write the behavioural specifications from the documentation and the binary evidence | `analysis/spec-01…04.md`, `analysis/reverse-engineering.md` | every spec section cites its evidence source; the clean-room boundary is documented |

## Phase 1 — Project foundation

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 1.1 | Local git repository, workspace manifest, `.gitignore` excluding proprietary artifacts | repo, `Cargo.toml` | `git status` clean of build output and of the original binaries |
| 1.2 | Quality gates | `rust-toolchain.toml`, clippy/fmt expectations documented | `cargo fmt --check` and `cargo clippy` run clean on every crate |
| 1.3 | Documentation skeleton | `README.md`, `ImplementationPlan.md`, `ImplementationStatus.md`, `LICENSE` | documents exist, plan and status agree |

## Phase 2 — `rc-core`, the headless engine

No GUI, no network, no async: everything here is usable from the desktop app, the
CLI and the MCP server alike.

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 2.1 | Entry model and formatting helpers | `entry.rs`, `format.rs`, `error.rs` | attribute rendering, size/date formatting, date-filter parsing tested |
| 2.2 | Locations (local / archive / remote) and path arithmetic | `location.rs` | parent/child/display round trips and URL parsing tested |
| 2.3 | Wildcards and mask sets | `mask.rs` | `*`, `?`, `[a-z]`, `[!…]`, separators, escapes, error cases tested |
| 2.4 | Sorting | `sorting.rs` | natural order, dirs-first, reversed, per-field sorting tested |
| 2.5 | Selection model | `selection.rs` | select/invert/expand-by-mask/same-extension/statistics/serialisation tested |
| 2.6 | Listings | `listing.rs` | read, filter, hidden files, `..` row, cursor movement, re-read keeps cursor, error paths tested |
| 2.7 | File operations | `ops.rs` | copy/move/delete/mkdir/rename incl. overwrite modes, cancellation, progress, symlink preservation, cross-device move tested |
| 2.8 | Configuration model | `config.rs` | defaults, TOML round trip, unknown-key tolerance, portable config dir tested |
| 2.9 | Histories, hotlist, tabs | `history.rs`, `hotlist.rs`, `tabs.rs` | MRU ordering, thinning, capacity, tab lifetime rules tested |
| 2.10 | Command catalogue and keyboard map | `command.rs`, `keymap.rs` | 594-entry catalogue resolves by name and id; classic key map binds correctly; user overrides tested |
| 2.11 | Volumes, comments, quick search | `drive.rs`, `comment.rs`, `quicksearch.rs` | free-space arithmetic, `descript.ion` round trip, quick-search cycling tested |

## Phase 3 — Localisation (`rc-i18n`)

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 3.1 | Language-pack format and parser | `Locale`, `Translator` | TOML packs parse, invalid packs report errors, missing keys fall back to English |
| 3.2 | Direction model | `Direction`, `direction_for_code`, `text_is_rtl` | RTL languages detected; mixed text resolves by first strong character |
| 3.3 | Bundled packs | `en.toml`, `cs.toml` | identical key sets, no empty values, completeness report works |
| 3.4 | Runtime loading | `Translator::load_dir` | packs can be dropped in at runtime without recompiling |

## Phase 4 — Previewer engine (`rc-viewer`)

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 4.1 | Content detection by magic bytes | `detect`, `probe` | images, text, archives, executables, media, empty and unknown binaries classified correctly; extension alone never decides |
| 4.2 | Text decoding | `load_text`, `decode_text` | UTF-8/16/32, legacy code pages, truncation and RTL handled |
| 4.3 | Hex dump model | `hex_document`, `load_hex` | exact formatting, configurable width, offsets and ASCII gutter tested |
| 4.4 | Image decoding | `load_image` | RGBA output, corrupt input rejected, pixel budget enforced |
| 4.5 | Unified loader | `load`, `open_bytes`, `describe` | unknown binaries fall back to hex, status text never panics |

## Phase 5 — Archive engine (`rc-archive`)

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 5.1 | Format detection and readers | `detect`, `open` | ZIP/TAR/TAR.GZ/TAR.BZ2/TAR.XZ/TAR.ZST readable; unsupported formats say so clearly |
| 5.2 | Virtual directory listing | `list_dir`, `is_dir` | intermediate directories synthesized; panel can render archive contents |
| 5.3 | Extraction with path safety | `extract_all`, `extract_members` | zip-slip, absolute paths and `..` refused; cancellation honoured |
| 5.4 | Creation and tests | `create`, `add_to_zip`, `remove_from_zip`, `test` | atomic writes, corrupt archives reported, round trips verified |

## Phase 6 — Help system (`rc-help` + `Docs/`)

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 6.1 | Help index over `Docs/<Topic>/<lang>.md` | `HelpIndex`, `HelpTopic` | topics discovered in any depth, languages and titles read from the first heading |
| 6.2 | Markdown model | `parse_markdown`, `Block`, `Span` | headings, lists, tables, code, quotes, inline styles, tolerance of malformed input |
| 6.3 | Fallback and search | `body`, `resolve_link`, `search`, `missing_translations` | English fallback, cross-topic links, completeness report |
| 6.4 | Content | `Docs/` tree in English and Czech | every implemented feature has a topic in both languages |

## Phase 7 — Remote file systems (`rc-remote`)

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 7.1 | Connection model | `ConnectionSpec`, `SavedConnection` bridge | validation and round trip tested |
| 7.2 | Listing parsers | `parse_list_line`, `parse_mlsd_line` | unix/MLSD fixtures incl. symlinks, spaces, year-vs-time, garbage |
| 7.3 | FTP client | `RemoteFs` over FTP | connect/list/transfer/delete/rename against an in-process mock server |
| 7.4 | Recursive transfers and cancellation | `get_tree`, `put_tree` | trees transferred, cancellation safe |
| 7.5 | SFTP and FTPS | feature-gated implementations | default build unaffected when disabled; limitations documented |

## Phase 8 — MCP integration (`rc-agent`)

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 8.1 | JSON-RPC layer | request/response handling | initialize, ping, unknown-method, parse-error, notification behaviour tested |
| 8.2 | Server tools | `tools/list`, `tools/call` | ≥15 tools with valid JSON Schemas, destructive tools identified |
| 8.3 | Host bridge and confirmation gate | `FileManagerHost`, `Confirmer` | refused actions do not execute; approved actions do |
| 8.4 | Client | `McpClient` over stdio/HTTP | handshake, tool discovery, calls, error propagation, timeout |
| 8.5 | Resources | help topics as `rcommander://` resources | round trip tested |
| 8.6 | Application wiring | `rcommander mcp` subcommand, agent panel | the app raises as a server and can drive an external server |

## Phase 9 — Application model (`rcommander::model`)

Headless and fully testable; the UI is only a view over it.

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 9.1 | Panels and navigation | `Panel`, `Model`, `Side` | open/enter/parent/swap/match, cursor keeping, error reporting tested |
| 9.2 | Dialog state models | `dialogs.rs` | every dialog validates its input; invalid names/paths rejected |
| 9.3 | Command dispatch | `commands.rs` | every implemented `cm_*` command performs its action; unknown commands are inert |
| 9.4 | Operations from dialogs | transfer/delete/mkdir/rename/attributes/comment/multi-rename | real temp-dir effects asserted by tests |
| 9.5 | Keyboard and command line | `handle_key`, `run_command_line` | shortcuts dispatch through the key map; `cd`/`cm_*`/unknown handled |
| 9.6 | Background operations | `ops_runner.rs` | progress, cancellation and failure reporting tested |

## Phase 10 — User interface (`egui`)

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 10.1 | Window shell and theme | `ui/app.rs`, `theme.rs` | menu bar, button bar, both panels, command line, function-key bar, status line render |
| 10.2 | File list rendering | panel widget | brief/full/comments views, colours by entry type, marks, cursor, mouse and keyboard |
| 10.3 | Dialogs | dialog rendering | copy/move/delete/mkdir/rename/search/attributes/comment/multi-rename/options/pack |
| 10.4 | Previewer window | `ui/viewer_window.rs` | text, hex and image modes bound to `rc-viewer`, keyboard handled |
| 10.5 | Editor window | `ui/editor_window.rs` | edit, save, unsaved-change prompt |
| 10.6 | Help window | `ui/help_window.rs` | topic list, rendering of `rc-help` blocks, language switch, RTL layout |
| 10.7 | Archive and FTP panels | integration | archives open as directories; remote panel lists and transfers |
| 10.8 | Agent panel | integration | MCP server toggle, client connections, tool calls |
| 10.9 | UI tests | `tests/ui_*.rs` with `egui_kittest` | toolbar interactions produce the expected model state |

## Phase 11 — Platforms and packaging

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 11.1 | Linux verification | release build + smoke run | app starts, lists directories, copies a file |
| 11.2 | Windows build | cross-compilation instructions/CI config | `cargo build --target x86_64-pc-windows-gnu` succeeds where the toolchain is available; documented otherwise |
| 11.3 | Android build | `cargo-apk` configuration and manifest | build configuration present and documented; desktop core shared |
| 11.4 | Release hygiene | versioning, `--help`, error messages | CLI surface complete and tested |

## Phase 12 — Final verification

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 12.1 | Full test suite | `cargo test --workspace` | all tests pass |
| 12.2 | Static checks | clippy + fmt | clean |
| 12.3 | Documentation audit | README/plan/status/Docs | every implemented feature documented; status matches reality |
| 12.4 | Repository state | commits | history readable, no proprietary artifacts committed |

---

## Phase 13 — Secure remote access (SFTP/SSH, sessions, credentials)

Today the FTP/SFTP engine exists (`rc-remote`) and FTP is wired into the panels, but
two things are not acceptable for SSH use: the SFTP transport is untested against a
live server and, more importantly, **passwords are currently stored in plain text**
in `settings.toml` (`SavedConnection::password`). This phase fixes both, and turns
SFTP into a first-class transport with saved sessions and safe credential storage.

| # | Step | Output | DoD (definition of done) |
| --- | --- | --- | --- |
| 13.1 | Host-key verification for SSH | `known_hosts` model: `HostKeyStore` (host, port, algorithm, fingerprint), first-use ("trust on first use") confirmation, hard refusal on a changed key, per-session override | tests for unknown host, trusted host, changed key (must refuse), and for a corrupted store file; the refusal message names host and both fingerprints |
| 13.2 | Authentication methods | password, key file (with optional passphrase), `ssh-agent` delegation, keyboard-interactive fallback; auth method is a property of the session, not of the transport | tests: each method builds the right libssh2 call; missing key file and wrong passphrase produce a clear error; agent absence degrades gracefully |
| 13.3 | Saved sessions | named session profiles stored separately from the settings file (`sessions.toml`), each with host, port, user, protocol, initial remote/local dirs, encoding, MLSD/passive flags, auth method and key reference | round-trip tests, rename/duplicate/delete, a session without credentials is still listed, and an import of an OpenSSH `ssh_config` fixture (aliases, `HostName`, `User`, `Port`, `IdentityFile`) |
| 13.4 | **Secure credential storage** | never plain text: OS keyring first (Secret Service/libsecret on Linux, DPAPI/Credential Manager on Windows, Android Keystore), encrypted portable fallback (Argon2id + AES-256-GCM, master passphrase) for environments without a keyring; one-time migration of existing plain-text passwords with a warning; secrets zeroed after use | tests: the crypto envelope round trip, wrong passphrase rejected, tampered ciphertext rejected, and an assertion that no saved file in a fixture configuration contains the password string; migration test that the plain value is removed from `settings.toml` |
| 13.5 | SFTP wired into the panel | SFTP through the existing `RemoteBackend`/background queue, `sftp` feature enabled by default on desktop, host-key prompt shown before the first connection | panel-level tests with an in-process SFTP double; documented manual verification against a real server (the test suite stays offline) |
| 13.6 | Documentation | a threat-model section in the remote-connections help topic: what is stored where, what a compromised machine still exposes, why host keys matter, how to rotate a session | help topic updated in both languages; the status file links the threat model |

## Phase 14 — Colour themes

Colours exist, but as two hardcoded palettes (`Palette::light`/`Palette::dark`)
selected by a string. This phase makes them data: themes are files, users can edit
them, and every window uses the same semantic roles.

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 14.1 | Theme file format | `themes/<name>.toml` with `[meta]` (name, base = light/dark, author, version) and one entry per semantic role (`background`, `foreground`, `cursor_*`, `selected_*`, `directory`, `hidden`, `readonly`, `archive`, `symlink`, `header_background`, `panel_border`, `status_background`, `accent`), plus `[file_types]` colour rules | parser + validation tests: unknown role rejected, bad hex rejected, missing role inherited from the base theme, duplicate names resolved deterministically |
| 14.2 | Built-in themes | classic light, dark and a high-contrast variant shipped in `themes/`, discovered at runtime exactly like `locales/` and `Docs/` | tests that all shipped themes parse, that every role is present after inheritance, and that a theme dropped into the directory appears without recompiling |
| 14.3 | Theme editor | dialog to edit every role with a live preview, duplicate/save/delete/import/export, plus contrast warnings (foreground vs background, cursor vs selection) | model tests for add/edit/delete/validation and a UI test that editing a role changes the rendered palette |
| 14.4 | One palette everywhere | every window (panels, dialogs, viewer text/hex/image chrome, editor, help, transfer manager, agent panel) reads the active theme instead of `Palette::by_name` | UI tests for at least two windows asserting the applied colour, and no remaining hardcoded `Color32` literals outside the theme module |
| 14.5 | Documentation | a "Themes" help topic describing the file format, the roles, how to add a theme and how per-file-type colours are evaluated | topic present in English and Czech |

## Phase 15 — Git repositories as panels

A repository is a natural fit for a commander: the working tree is already an
ordinary directory, so the value is what a plain file manager cannot do —
**treating branches, tags, commits and remotes as virtual folders** and copying
files out of any revision without ever running `checkout`.

The first round is deliberately **read-only and one-directional** (repository →
panel). Write operations are a separate, riskier step (15.8) and come only after
the read path is proven.

Ordering: 15.1–15.3 only touch local repositories and can start immediately;
everything that authenticates (15.4, 15.5) **must reuse the credential store from
Phase 13.4** instead of inventing its own.

| # | Step | Output | DoD (definition of done) |
| --- | --- | --- | --- |
| 15.1 | Backend decision and fixture harness | evaluated choice between `gix` (gitoxide, pure Rust — works on Android, no external binary) and shelling out to the system `git`; the decision and its trade-offs recorded in `analysis/` and in the status file; a test harness that builds real repositories from fixtures (init, two commits, a branch, a tag, an ignored file, a file:// remote) | fixtures are created by the harness itself, every later step can be tested offline, and the chosen backend is documented with the reason |
| 15.2 | Read-only repository view | a `Location::Git` variant for the panel: the repository root shows the working tree with a branch indicator, and virtual folders expose `branches/`, `tags/`, `commits/` (by short hash) and `remotes/`; opening a branch shows that revision's tree | model tests: list branches/tags of a fixture repository, open a commit and see its entries, and navigate back; an invalid repository reports a clear error instead of an empty panel |
| 15.3 | Copy out of a repository (one direction) | copy files and directories from any branch, tag or commit into the other panel; colliding names are renamed (never overwritten, same policy as drag & drop); streaming copy with progress and cancellation; works for large blobs | tests: copy a file from `HEAD`, from an older commit and from a tag, verify the bytes and the timestamps, verify rename-on-collision, and cancellation leaves no partial file behind |
| 15.4 | Credentials and remotes | repositories that need authentication use the Phase 13 credential store (SSH key or HTTPS token), per-remote sessions, no plain-text secret anywhere | tests that the secret is fetched through the storage abstraction, that no saved file contains it, and that error messages redact it |
| 15.5 | Clone into a panel | clone an HTTPS (token) or SSH (key) remote into the local panel with progress and cancel; the new working tree opens in the target panel | offline test against a local `file://` remote and an in-process HTTP git server; a failing clone leaves no partially initialised directory that looks usable |
| 15.6 | Repository status in the panel | read-only status per file (modified / new / deleted / untracked / ignored) shown through the attribute or colour column, plus the current branch in the panel header | tests that status codes are computed for a fixture with each state and that the renderer maps them to the configured colours |
| 15.7 | Documentation | a "Git repositories" help topic (English + Czech): what the virtual folders are, how copying out of a revision works, and the security note about keys and tokens | topic present in both languages and linked from the keyboard overview |
| 15.8 | Write operations (**next round, explicitly deferred**) | stage/unstage, commit, branch create/switch, merge, pull, push, a log view and a diff viewer; every destructive action behind a dry run and a confirmation, never rewriting history automatically | planned only after 15.1–15.7 are green; safety rules (dry run first, confirmation, no automatic force-push, refuse to touch a dirty tree without asking) are part of the DoD when it is implemented |

## Working rules

1. **Tests first.** A step's tests exist before its implementation; a step is done
   only when they pass.
2. **Headless first.** Behaviour belongs in a library crate so it can be tested
   without a window system; the UI stays a thin view.
3. **No unspecified data in the repository.** Proprietary binaries, extracted
   resources and decompiled output stay outside the git history; only behavioural
   facts and original code/docs are committed.
4. **Honest status.** `ImplementationStatus.md` records what is verified, with the
   evidence (test counts, commands). Pending work is listed, never implied to be
   finished.
