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
| 13.3 | Saved sessions | named session profiles stored separately from the settings file (`sessions.toml`), each with host, port, user, protocol, initial remote/local dirs, encoding, MLSD/passive flags, auth method and key reference | round-trip tests, rename/duplicate/delete, a session without credentials is still listed, and an import of an OpenSSH `ssh_config` fixture (aliases, `HostName`, `User`, `Port`, `IdentityFile`) — **done**: `rc_core::sessions` (15 engine tests) + `model::sessions` (5 model tests) + the *Saved sessions* section of the connect dialog with 2 UI tests |
| 13.4 | **Secure credential storage** | never plain text: OS keyring first (Secret Service/libsecret on Linux, DPAPI/Credential Manager on Windows, Android Keystore), encrypted portable fallback (Argon2id + AES-256-GCM, master passphrase) for environments without a keyring; one-time migration of existing plain-text passwords with a warning; secrets zeroed after use | tests: the crypto envelope round trip, wrong passphrase rejected, tampered ciphertext rejected, and an assertion that no saved file in a fixture configuration contains the password string; migration test that the plain value is removed from `settings.toml` |
| 13.5 | SFTP wired into the panel | SFTP through the existing `RemoteBackend`/background queue, `sftp` feature enabled by default on desktop, host-key prompt shown before the first connection | panel-level tests with an in-process SFTP double; documented manual verification against a real server (the test suite stays offline) |
| 13.6 | Documentation | a threat-model section in the remote-connections help topic: what is stored where, what a compromised machine still exposes, why host keys matter, how to rotate a session | help topic updated in both languages; the status file links the threat model |
| 13.7 | **Recovering from a lost passphrase** | a *Dangerous* page in the options dialog with confirmed factory resets (settings, tabs/history/hotlist, trusted host keys, credentials, everything) so a forgotten master passphrase is not a dead end; the view never deletes on the first click | model tests for each scope (what it removes and what it leaves alone), a UI test that the first click only arms the reset, and the *Configuration* help topic describing every entry — **done** |

## Phase 14 — Colour themes

Colours exist, but as two hardcoded palettes (`Palette::light`/`Palette::dark`)
selected by a string. This phase makes them data: themes are files, users can edit
them, and every window uses the same semantic roles.

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 14.1 | Theme file format | `themes/<name>.toml` with `[meta]` (name, base = light/dark, author, version) and one entry per semantic role (`background`, `foreground`, `cursor_*`, `selected_*`, `directory`, `hidden`, `readonly`, `archive`, `symlink`, `header_background`, `panel_border`, `status_background`, `accent`, `row_alternate`, `warning`), plus `[file_types]` colour rules | parser + validation tests: unknown role rejected, bad hex rejected, missing role inherited from the base theme, duplicate names resolved deterministically — **done** (`rc_core::theme`, 12 tests) |
| 14.2 | Built-in themes | classic light, dark and a high-contrast variant shipped in `themes/`, discovered at runtime exactly like `locales/` and `Docs/` | tests that all shipped themes parse, that every role is present after inheritance, and that a theme dropped into the directory appears without recompiling — **done**: `themes/classic-light.toml`, `classic-dark.toml`, `high-contrast.toml`, discovered by `ThemeSet::load_dir` with 2 model tests |
| 14.3 | Theme editor | dialog to edit every role with a live preview, duplicate/save/delete/import/export, plus contrast warnings (foreground vs background, cursor vs selection) | model tests for add/edit/delete/validation and a UI test that editing a role changes the rendered palette — **done**: the *Colors* page of the options dialog (role grid with colour pickers, live preview, contrast warnings, save/delete/reload), 3 model tests and 2 UI tests; import/export are covered by the file format itself (copy a file) |
| 14.4 | One palette everywhere | every window (panels, dialogs, viewer text/hex/image chrome, editor, help, transfer manager, agent panel) reads the active theme instead of `Palette::by_name` | UI tests for at least two windows asserting the applied colour, and no remaining hardcoded `Color32` literals outside the theme module — **partial**: `Palette::for_model` is the single source (window chrome, panels, dialogs, options), `apply_theme` re-applies on change and a UI test asserts the applied background; a few dialog badges still use literal colours |
| 14.5 | Documentation | a "Themes" help topic describing the file format, the roles, how to add a theme and how per-file-type colours are evaluated | topic present in English and Czech — **done**: `Docs/Themes/{en,cs}.md` |

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
| 15.1 | Backend decision and fixture harness | evaluated choice between `gix` (gitoxide, pure Rust — works on Android, no external binary) and shelling out to the system `git`; the decision and its trade-offs recorded in `analysis/` and in the status file; a test harness that builds real repositories from fixtures (init, two commits, a branch, a tag, an ignored file, a file:// remote) | fixtures are created by the harness itself, every later step can be tested offline, and the chosen backend is documented with the reason — **done**: `git2` + `vendored-libgit2`, recorded in [`analysis/git-backend.md`](analysis/git-backend.md); `crates/rc-git` reads revisions (12 tests) and its fixture builds the repositories the suite uses |
| 15.2 | Read-only repository view | a `Location::Git` variant for the panel: the repository root shows the working tree with a branch indicator, and virtual folders expose `branches/`, `tags/`, `commits/` (by short hash) and `remotes/`; opening a branch shows that revision's tree | model tests: list branches/tags of a fixture repository, open a commit and see its entries, and navigate back; an invalid repository reports a clear error instead of an empty panel — **done**: `Location::Git` in `rc-core`, `git_bridge` in the app (6 tests), `cm_GitPanel` opens the repository of the active directory, and two UI tests list the virtual folders, open a branch and check the status colours |
| 15.3 | Copy out of a repository (one direction) | copy files and directories from any branch, tag or commit into the other panel; colliding names are renamed (never overwritten, same policy as drag & drop); streaming copy with progress and cancellation; works for large blobs | tests: copy a file from `HEAD`, from an older commit and from a tag, verify the bytes and the timestamps, verify rename-on-collision, and cancellation leaves no partial file behind |
| 15.4 | Credentials and remotes | repositories that need authentication use the Phase 13 credential store (SSH key or HTTPS token), per-remote sessions, no plain-text secret anywhere; **reading the machine's Git configuration is offered first, behind an explicit approval** (identity, credential helper, `store` credentials, `url.*.insteadOf`, `core.sshCommand`) | tests that the secret is fetched through the storage abstraction, that no saved file contains it, that error messages redact it, and that nothing is read before the user allows it — **partly done**: `rc-git::config` + the approval dialog are in place (10 engine tests, 5 model tests, 2 UI tests); the remote authentication itself is still open |
| 15.5 | Clone into a panel | clone an HTTPS (token) or SSH (key) remote into the local panel with progress and cancel; the new working tree opens in the target panel | offline test against a local `file://` remote and an in-process HTTP git server; a failing clone leaves no partially initialised directory that looks usable |
| 15.6 | Repository status in the panel | read-only status per file (modified / new / deleted / untracked / ignored) shown through the attribute or colour column, plus the current branch in the panel header | tests that status codes are computed for a fixture with each state and that the renderer maps them to the configured colours — **done**: rows are tinted by status role (warning = changed, archive = staged, symlink = untracked, hidden = ignored) and the path bar shows `[branch]`; engine tests cover each state, UI tests cover the mapping |
| 15.7 | Documentation | a "Git repositories" help topic (English + Czech): what the virtual folders are, how copying out of a revision works, and the security note about keys and tokens | topic present in both languages and linked from the keyboard overview — **started**: `Docs/Git repositories/{en,cs}.md` describes the engine and the configuration approval; the virtual folders are added when they exist |
| 15.8 | Write operations | stage/unstage, commit, branch create/switch, merge, pull, push, a log view and a diff viewer; every destructive action behind a dry run and a confirmation, never rewriting history automatically | planned only after 15.1–15.7 are green; safety rules (dry run first, confirmation, no automatic force-push, refuse to touch a dirty tree without asking) are part of the DoD when it is implemented — **partly done**: edit → commit → push with the question asked by default, plus switching/creating branches from the panel and fetch/pull/push/merge (32 engine tests, 15 model tests, 3 UI tests); staging arbitrary files and conflicts are still open |

## Phase 16 — Interactive AI workspace (agent chat as a panel)

### The flow this phase exists for

The panel owner described the target workflow the whole project is built towards: **one
panel is the agent**, the other is the working directory, and the agent has the same tools a
human has in that window —

1. read and edit a file in the visible directory (viewer, editor, search),
2. put the change on Git (stage, commit, push) and read the status colours back,
3. publish it over the panel's remote connection (SFTP/FTP upload),
4. and, when asked, investigate something (a failing disk, a permission problem) **with the
   user watching every step**.

That means the agent's tool surface is not a separate universe: it is the same command
catalogue and the same model API the panels use, exposed through MCP (Phase 8) and driven by
a conversation (this phase). The pieces already exist — `rc-agent` server and client, the
panel model, the Git engine, the remote panels — so this phase is about the conversation,
the context and the permissions, not about new capabilities.

**Security is part of the definition, not an afterthought.** Every tool call that writes,
deletes, publishes or executes must go through an approval step that names the exact
operation and target, a per-tool allowlist in the settings, and an audit trail the user can
read afterwards. Shell or disk-level investigation is only reachable when the user has
explicitly enabled it for the session, never by default.

**Goal:** keep the familiar two-pane commander workflow, but allow either pane
(or a tab inside it) to become an interactive **agent/chat view**. For example,
the left pane is a conversation with a local Ollama model and the right pane
shows a directory of JSON translation dictionaries. The user can ask the agent
to fill missing translations in the *same schema*, translate a document, or
propose a configuration-file change. The same workflow must also work with
remote directories, Git revisions and eventually virtual providers such as
UnderstandMe — without giving the model unrestricted access to the machine.

This phase extends the existing Phase 8 MCP client/server and Phase 10 agent
tool window; it does **not** redefine MCP as a language-model API. An Ollama
(or other LLM) connection supplies conversation/inference, whereas MCP tools
supply explicit, permission-checked capabilities. A model endpoint alone is not
an MCP server. RCommander should also remain fully usable without any AI.

Example user journey:

1. Open **Agent chat** in the left pane; choose a locally running Ollama model
   or an explicitly configured remote model provider. The right pane stays a
   normal local, SFTP or virtual directory panel.
2. Select `locales/*.json` in the right pane and choose **Share selection with
   agent** (or grant read access to that directory for this task). The chat
   displays the exact scope and filenames before the agent sees file contents.
3. Ask: "Compare these dictionaries with `en.json`; add missing keys to the
   Czech and Swahili dictionaries, preserve keys, nesting, placeholders and
   formatting, and show what you changed." Another task might be: "Translate
   this Markdown document" or "Suggest a safe edit to this TOML config".
4. RCommander inventories the files, reads only approved inputs, and asks the
   agent for a **proposed change set**, not an immediate overwrite. JSON/TOML
   parsers and task-specific validators check the result.
5. The other pane offers a diff / before-after preview, individual file/hunk
   approval, **Apply**, **Reject** and **Undo**. Approved changes are saved
   safely; rejected changes do not touch the originals.

| # | Step | Output | DoD (definition of done) |
| --- | --- | --- | --- |
| 16.1 | Agent chat panel and session model | `AgentChatSession` in the headless model, conversation history, model/provider selector, streaming replies, cancellation, chat-as-pane/tab and a switch back to file listing | UI test shows chat in either pane while the other pane remains navigable; a session can attach/detach without losing the current directory or selection; no model running means a useful error, not a frozen UI |
| 16.2 | Pluggable model connections | an `AgentProvider` abstraction with an **Ollama HTTP adapter** first, then configurable remote/OpenAI-compatible endpoints; model discovery, timeout/cancel, token/context budgets, clear local/remote indicators | in-process fake HTTP provider covers streaming, cancellation, malformed/timeout responses and reconnection; no cloud connection is made by default; credentials use the Phase 13.4 secret store and are never written to chat logs or settings in plain text |
| 16.3 | Explicit tool and file-scope permissions | connect chat sessions to existing `rc-agent` MCP tools and a constrained workspace-file adapter; read-only by default, per-task path/operation allowlist, optional approval before writes, visible tool-call log | tests refuse access outside the approved roots, symlink escapes and unapproved destructive commands; the model cannot acquire wider permissions merely by asking; remote files and untrusted documents are treated as **data**, never as tool instructions |
| 16.4 | Safe proposal / review / apply pipeline | `ChangeSet` with original hashes, textual/binary classification, diff view, syntax/format validation, per-file approval, atomic replace where supported, backup/undo and conflict detection | tests prove a declined change leaves bytes untouched, stale inputs cannot be silently overwritten, valid changes preserve encoding/newlines when requested, invalid JSON/TOML or cancelled writes do not corrupt originals; multi-file partial failure is reported with recoverable backups |
| 16.5 | Dictionary and document workflows | local directory comparison keyed by a chosen source locale; discover missing/extra keys, ask the agent to translate only missing values, preserve JSON structure, placeholders and escape sequences; translate selected text/Markdown with a preview | fixtures cover nested JSON, arrays, placeholders (`{name}`, `{0}`, etc.), Unicode/RTL, already-translated entries, invalid output and schema differences; existing translations are not replaced without explicit instruction; output is validated before Apply |
| 16.6 | UnderstandMe and other virtual data sources | a **provider interface** to list/read/write *capabilities* of a data source independently of the panel; a future UnderstandMe adapter can display projects/languages/dictionaries in a pane and compare/synchronise with local JSON via its API or an MCP bridge | offline provider fixture lists languages and entries, distinguishes read-only/writeable actions, previews proposed synchronisation, handles version conflicts and missing authentication; do not pretend an arbitrary MCP server automatically implements a filesystem |
| 16.7 | General editing tasks | select one or more Markdown, text, code or configuration files and request a translation, explanation, refactor or structured edit; show a scoped diff and optionally run explicitly configured validation commands | headless tests cover multi-file selections, unsupported/binary files, file-size/context limits, no-op proposals, malformed output and failures of optional validators; **no automatic arbitrary shell execution** and no silent write to config files |
| 16.8 | Opt-in Git working-tree assistant | watch an approved local repository for changes (debounce/coalesce events), inspect `git status` and diffs, optionally run allowlisted checks, summarise problems and **draft** GitHub issues or fixes; coordinate with Phase 15 Git panels | deterministic tests cover duplicate file events, untracked/ignored files, issue deduplication, offline mode and credentials; creating an issue, changing branches, committing or pushing requires separate approval; no automatic force-push |
| 16.9 | Async work, portability and documentation | bounded queues, cancellable operations, progress/diagnostics, clear provider capability indicators; English/Czech help topic with task examples, trust boundaries and privacy notes | UI/model tests with an in-process mock model and mock MCP server; no dependence on a live Ollama, GitHub or UnderstandMe account; desktop tested first, Android capability limitations documented honestly |
| 16.10 | Bidirectional pane context and live results | pane-neutral `WorkspaceContext`, opt-in context sharing, agent-to-pane operations, refresh/highlight events, F3 preview and proposal/diff entry points | headless/UI tests: chat left and files right **and vice versa**; selected file shared only after permission; creating files refreshes the other pane; F3 shows the new file; tab switches do not widen scope; stale events and cycles are suppressed |
| 16.11 | Permission profiles and approvals | layered global/provider/workspace/session policies, time-limited grants, operation-scoped approvals, audit trail, emergency stop/revoke | tests: deny wins, expired grants fail closed, changing pane paths never changes granted roots, no silent remote command, Git push or GitHub issue, every external write shows the destination and requires its own appropriate grant |
| 16.12 | Model/tool/data plugin contracts | versioned adapter manifests and registries; separate model APIs (Ollama and configurable endpoints), MCP tool discovery and capability-based data providers | fake providers and fake MCP servers verify discovery, conflicts, unavailable providers, refusal of unknown privileges, tool-chain cancellation and provenance of every tool call; no untrusted extension runs by default |
| 16.13 | Project scaffolding and approved commands | bounded project-task runner with working-directory constraints, configured commands such as `dotnet new`, `dotnet test`, `git init` and local commit, streaming logs and live file updates | temp-directory integration test creates a C# solution and test project using fixture/fake command runners; tests prohibit arbitrary commands, unsafe working paths, secrets in command/log output and unapproved external publication |
| 16.14 | Remote SSH workspace actions | browse/preview/edit via SFTP provider, optional separately authorised SSH command transport using verified host keys, isolated command scope, per-host identity and operation logs | mock SSH/SFTP integration tests for host verification, reauthentication, network loss, stale remote file conflicts and refusal of remote shell execution without its specific grant; depends on Phase 13 |
| 16.15 | Platform capability matrix | desktop-first feature toggles for agent chat, model runtime, MCP tools, project execution and SSH commands; optional/limited Android UI with no forced parity | Linux/Windows test matrix covers chat and tool workflows; Android build remains valid with desktop-only extensions disabled and unsupported actions hidden or clearly disabled; no requirement to ship an Android AI workstation |

### Shared, bidirectional pane workspace

The **two panes are peers**. The chat pane is not a separate, blind command
console: it can be paired with a file, archive, Git, remote or virtual-provider
pane through a **WorkspaceContext**. With the user's configurable consent, the
agent can inspect the other pane's current location, selected items, permitted
metadata, read-only previews and approved file contents. Its tool actions can
then create or modify items in that pane; the pane immediately refreshes and
highlights the result. A user can select a newly created file and press **F3**
to view its standard preview, or open the proposed diff before saving. The
agent can also respond to user-side navigation/selection changes *only when
sharing is enabled*. Switching focus, moving tabs or swapping pane sides must
not silently expand permission scopes.

A **two-way event channel** publishes approved selection/context changes to the
chat and applies operation progress, new/modified file notifications, previews
and task results back to the opposite pane. Use bounded/debounced events,
generation IDs and correlation IDs to prevent loops, duplicate refreshes and
stale previews. Workspace grants belong to a session and an explicit root or
resource identity, **not** to an arbitrary mutable "active pane" path.

Concrete desktop scenario:

1. In the right pane open `~/Projects` (or a chosen writable SSH/SFTP
   destination); in the left pane open Agent chat and select a local Ollama
   model or another configured provider. The user grants the session
   **create/edit in this project directory**.
2. Ask: "Create a C# solution here with a small CLI application, tests and
   README; initialise Git, run the tests, and prepare it for GitHub."
3. The agent plans the task, uses approved filesystem/project tools (and,
   optionally, authorised MCP tools) to create files, invoke `dotnet` in a
   constrained working directory, and report progress. The right pane lists
   new directories/files as they appear; **F3** previews a selected source file
   or README, while the chat shows what the agent has done.
4. The user reviews the project and tool log. Creating a Git repository and a
   *local* commit can follow an expressly granted policy; creating a remote
   repository and **pushing** are external write/publication actions and need
   separately granted permissions and a visible target (account/repo/branch).
   Never silently push, force-push, expose secrets or enable unrestricted
   command execution.
5. Future tasks can work in either direction: select files in the pane and
   send them to the agent, or ask the agent to create/update a file and
   immediately inspect it in the other pane.

### Agent permissions and plugin architecture

Separate **model providers**, **tool providers**, and **data providers**:

- **Model providers:** Ollama is the first reference adapter; independently
  configured adapters may connect to self-hosted or cloud model APIs. Provider
  discovery, per-profile settings, endpoints, model selection, timeouts,
  streaming and credentials are extensible through a versioned provider API.
  Providers are *not* necessarily MCP services.
- **Tool providers:** built-in file/project/Git/SSH capabilities plus any
  user-registered **MCP servers** with advertised schemas. The model can plan
  multi-step calls across authorised tools, but RCommander centrally validates
  every operation, its arguments and its access policy. No model or MCP tool
  gets authority simply by advertising a capability.
- **Data providers:** filesystem, archive, remote, Git, UnderstandMe and future
  virtual sources expose browse/read/write/preview/watch capabilities. Their
  operations are mediated by the same policy engine; a tool or provider may
  be available on one platform but absent on another.
- **Extensions:** declare capabilities, UI integration, minimum protocol/API
  version and trust level. Avoid requiring third-party Rust code to be compiled
  into the app; prefer isolated processes/protocol adapters for untrusted
  extensions, with opt-in installation and disabled-by-default permissions.
  An extension marketplace is **not** a prerequisite for Phase 16.

Provide policy profiles at **global → provider → workspace → session/task**
levels, with the most restrictive applicable rule taking precedence. Suggested
permission classes: metadata/selection visibility; read file contents; propose
edits; apply edits/create within an approved root; run an allowlisted command;
remote SSH execution; use network/MCP tools; Git local writes; external actions
such as GitHub issue creation or push. Support *ask every time*, *allow within
this scope/session* and *deny*, explicit expiry/revocation, dry-run previews,
audit logs and a **Stop agent / revoke grants** action. Defaults are read-only
and minimal. Do not treat injected instructions inside README, documents or
tool responses as authorisation.

### Desktop-first platform scope

The advanced two-way agent workspace is primarily a **Linux/Windows desktop**
feature. Android can retain the ordinary two-pane file manager, previews and
other platform-appropriate functions without implementing the full chat,
local model runtime, shell/SSH execution or agent-plugin hosting. Share the
headless core where useful; capability-gate or feature-gate desktop-only UI and
integrations, and keep Android builds clean when these features are disabled.
Avoid claiming feature parity where the OS permission model does not allow it.

**Important boundaries and dependencies:**

- **Local-first:** Ollama is an option, not a requirement; a self-hosted or remote
  provider can be selected. Do not send a file, prompt or API secret to a remote
  provider without clearly showing which provider is selected and what data is
  being shared.
- **Capabilities, not a fake filesystem:** ordinary local/remote panels and
  future Git/UnderstandMe virtual panels can share a browsing abstraction, but
  each provider advertises its supported operations. MCP is one possible
  transport/tool bridge, not a mandatory backend for all panels.
- **Human control:** chat may *propose* changes; users approve the exact scope
  and final diff. Reading a file and writing it are separate permissions.
  Watchers and automatic issue drafting are opt-in, and publication is approved.
- **Context discipline:** large files are processed with bounded reads/chunks,
  explicit truncation notices and task-scoped context; a token limit is not
  silently bypassed by submitting unrelated private files.
- **Ordering:** 16.1–16.5 and 16.7 can be developed against local fixtures and
  mocked providers using Phase 8's existing tool interfaces. Secrets and remote
  authentication reuse Phase 13.4; Git integration builds on Phase 15; the
  UnderstandMe provider requires a separately defined API/contract. No part of
  this phase is implemented merely by having the current MCP tools.

## Phase 17 — Rich document previewer (PDF, Office, Markdown, source code)

The F3 viewer covers text, images and a hexdump. This phase turns it into a
document viewer that developers and office users can actually work with, while
keeping the pixel/byte budgets that stop a single file from freezing the window.

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 17.1 | Markdown rendering | headings, lists, tables, code, links and images inside the viewer, using the existing `rc-help` Markdown model as the parser | fixtures render every block type; very large files stay paged; the encoding detection from `rc-viewer` still wins before parsing |
| 17.2 | Source-code highlighting | a small, bundled syntax highlighter for the languages developers open most (Rust, C/C++, Python, JS/TS, shell, TOML/JSON/YAML, C#, Markdown) with a configurable theme | golden-file tests per language; unknown languages fall back to plain text; highlighting never changes the copied text |
| 17.3 | PDF preview | first N pages rendered to bitmaps, page navigation, zoom, text selection where the file allows it; a pure-Rust decoder preferred so Android keeps working | fixture PDFs render; a corrupt or encrypted PDF reports a clear error; rendering is cancellable and memory-budgeted |
| 17.4 | Office documents (read-only) | OOXML (`.docx`, `.xlsx`, `.pptx`) and ODF text extracted to Markdown-ish text plus embedded images; old binary formats (`.doc`, `.xls`) shown as an explicit "not supported, open externally" note | fixtures for each supported format; sheet/table structure preserved reasonably; no format is silently mis-rendered |
| 17.5 | Viewer plugins and file-type rules | the viewer asks a registry which renderer claims a content type (the same registry Phase 18 uses), so new formats can be added without touching the viewer | a fake plugin claims an extension and renders in tests; ordering and fallback are deterministic |
| 17.6 | Documentation | a "Previewer formats" topic with the supported list, the limits and how to open a file externally when the internal viewer cannot show it | bilingual topic; status file lists what is native, what needs a plugin and what is not supported |

## Phase 18 — Plugin architecture (data sources and renderers)

Plugins are what turn a closed list of features into a platform: a web API source,
a new archive format or a document renderer without recompiling the application.
The reference product used Windows DLL interfaces; this project needs a portable
contract instead.

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 18.1 | Manifest and discovery | `plugins/<id>/plugin.toml` with name, version, capabilities, entry point and permissions; discovered at runtime like `locales/`, `Docs/` and `themes/` | manifest validation tests; a broken plugin is reported and skipped, never fatal |
| 18.2 | Data-source capability | a `Provider` contract that lists a virtual directory tree and reads/writes entries, so a web API or a custom store appears as a panel — reusing the existing `RemoteBackend`/panel plumbing | an in-process fake provider lists and opens a virtual tree in the panel; write access is opt-in per manifest capability |
| 18.3 | Renderer capability | a viewer renderer plugin contract (content type → rendered output) used by Phase 17.5 | the fake renderer from 17.5 is the first real plugin; wrong output is rejected by validation |
| 18.4 | Sandboxed execution | plugins run out of process (child process with a line protocol) with an explicit allowlist of paths and network hosts; a plugin can never reach the credential store | a plugin attempting an unapproved path or host is refused and logged; a crashing plugin does not take the application down |
| 18.5 | Example plugins and docs | at least one working example (a JSON-over-HTTP web API browser) and a developer guide describing the protocol, versioning and permissions | the example runs against a local fixture server; the guide is bilingual |

## Phase 19 — Integrated SSH terminal (PuTTY-style)

An IT professional who already uses the file manager to look at a server wants the
shell next to it. This is a **terminal emulator**, not a file transfer, so it gets
its own isolated phase.

| # | Step | Output | DoD |
| --- | --- | --- | --- |
| 19.1 | Terminal engine | a VT100/xterm-subset parser in a headless crate (`rc-terminal`): escape sequences, scrollback, cursor movement, colours 0–255 and true colour, UTF-8 and wide characters, resize events | a golden-file test suite of escape-sequence fixtures (the standard `vttest`-style cases) that needs no network; the parser never panics on malformed input |
| 19.2 | PTY and SSH channel | a local shell through a pseudo-terminal (unix) and a remote shell through the existing SSH session (reusing Phase 13's host-key and credential handling) | in-process tests for the local PTY on Linux; the SSH channel is covered by the opt-in integration test against a real server |
| 19.3 | Terminal widget | rendering with the bundled monospace font, key mapping (Ctrl/Alt combinations, function keys, clipboard copy/paste, optional Ctrl+Shift+C/V), selection, scrollback search, zoom | widget tests that key sequences reach the engine and that selection + clipboard round trip |
| 19.4 | Sessions | several terminal tabs bound to saved sessions, per-host profiles, reconnect after a dropped connection, a log file option | model tests for tab lifetimes, reconnect and per-session settings; a dropped connection shows a clear state instead of a frozen screen |
| 19.5 | Safety | no automatic command execution from the file manager side, confirmation for "open shell on this host", the terminal is a separate window by default | tests that opening a terminal does not run anything else; documentation of what the terminal can and cannot do |


## Ideas backlog (not scheduled yet, cheapest value first)

These are candidates that fit the "portable tool for a developer/administrator"
direction; each one is small enough to schedule as a sub-phase once the phases
above are green.

| Idea | Why it is worth it | Cost |
| --- | --- | --- |
| Portable mode | everything (settings, secrets, session, help, themes, plugins) next to the executable, already half-supported through `RCOMMANDER_CONFIG_DIR`; one documented switch plus an "on a USB stick" test | small |
| Directory comparison with three panes | a third "reference" directory (for example a Git revision) next to the two panels: shows what differs before syncing | medium |
| Batch rename presets and saved masks | the multi-rename dialog already has the pattern language; saving named presets makes it reusable per project | small |
| Archive-as-folder for remote files | stream a remote archive into the existing archive panel without a full download | medium |
| Disk-usage treemap | a second view of the duplicate finder data: what really occupies the disk | medium |
| Checksum watch ("fingerprint this folder") | store a checksum file per directory and report drift later; reuses `rc-core::checksum` | small |
| Command palette (fuzzy) | `Ctrl+Shift+P` over the 594-command catalogue; the model already dispatches by name | small |
| Session restore with unsaved-edit recovery | reopen tabs, panels and editor buffers after a crash | small |
| Per-directory action menus | `.rcommander.toml` next to a folder defines buttons (build, test, deploy) with confirmations | medium |
| Accessibility pass | full keyboard navigation and screen-reader labels (the panels already carry accessible names) | medium |

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
