# Implementation Status

Snapshot of what is **verified working** in this repository, with the evidence
for each claim. The steps refer to [`ImplementationPlan.md`](ImplementationPlan.md).
A step is only marked *done* when its tests pass and the behaviour works end to
end; anything else is marked *partial* or *pending* with the gap spelled out.

Historical workspace evidence command (individual sections may name other commands; counts come from different revisions):

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
| 5 | Archive engine (`rc-archive`) | **baseline done; extensions pending** — 5.5–5.6 / issue #1 |
| 6 | Help system (`rc-help`, `Docs/`) | **done** |
| 7 | Remote file systems (`rc-remote`) | **partial** — FTP/SFTP panels and saved sessions implemented with recorded live SFTP verification; interactive prompt for missing credentials remains unfinished |
| 8 | MCP integration (`rc-agent`) | **done** — server + client tested, host bridge wired, `rcommander mcp` subcommand, and an agent panel in the UI for connecting to external servers and running their tools |
| 9 | Application model | **done** — including background job control (start, poll, cancel, aggregate progress) |
| 10 | egui user interface | **partial** — dual panels, thumbnails, dialogs, configurable button bar, editor/viewer/help, Git UI and background jobs are reported working; inline tree, search result views and thumbnail controls still need refinement |
| 11 | Platforms | **partial** — historical live Linux and Windows checks; Windows MSVC portable build #7 succeeded at 351baeaa; Linux UI regression #1 succeeded at 482618a; #8 is a separate revision; current Android APK unverified |
| 12 | Verification | **partial/current evidence required** — historical implemented-scope tests green; latest Windows targeted suite/release green, no new full same-revision workspace test/Clippy/format evidence |
| 13 | Secure remote access (SFTP/SSH, sessions, credentials) | **partial** — 13.1 (host keys), 13.2 (key file / agent / password), 13.4 (encrypted store, migration) and 13.3 (saved session profiles with `ssh_config` import), 13.5 (SFTP in the panel, verified against a real OpenSSH server) and 13.7 (the confirmed factory-reset page that makes a lost passphrase recoverable) are done; what remains is the interactive password prompt for connections whose secret is not in the store (today the dialog reopens pre-filled instead) |
| 14 | Colour themes and surface controls | **partial** — base themes/editor, fonts/chrome and surface-aware F-key row recorded; modifier row implemented, menu audit and advanced effects remain |
| 15 | Git repositories as panels | **partial** — repository panels, revision copy-out, clone, branch switching, staging/committing, fetch/pull/push/merge have recorded implementation; remaining credential and live-integration coverage needs review |
| 16 | Interactive AI workspace | **partial** — chat/streaming, panel, providers, prompt catalogue/editor/pins, copy/review/hunks/backup, policy engine and MCP catalogue recorded; model-to-MCP calls, checklist, workload/runtime, durable sessions and teams/Commons and typed resolution/custodian delegation (16.0f–h) open |
| 17.0 | Shared Markdown, syntax highlighting and annotations | **partial** — shared rendering/highlighter and language extensions recorded; annotations, remaining consumer migration and TUI coverage need verification |
| 17 | Rich document previewer | **partial** — Markdown/code/hex views; annotations, PDF/Office, binary inspection and viewer integration pending |
| 18 | Plugin architecture | **partial** — location/registry, table capabilities, TOML command/MCP sources and on/off manager implemented; manifest verbs, renderers, isolation, installer, agent catalogue/calls and Workshop pending |
| 19 | Integrated SSH terminal and PowerShell remoting | **pending** — SSH terminal (19.1–19.5), Windows WSMan/WinRM and optional PowerShell-over-SSH sessions, scoped agent execution and Windows diagnostic workloads (19.6–19.9); no RDP dependency |
| 20 | Optional privileged system capabilities | **pending** — narrow helper, OS authentication, verified backup/restore and optional installation; not required for normal chat |
| 21 | Portable host diagnostics, System Check and repair workloads | **pending** — read-only diagnostics, evidence-based approved repairs, preventive checks, portable technician mode, remote .NET/Apache deployment and optional diagnostic integrations |
| 22 | Native WSL integration and cross-OS workspaces | **pending** — native filesystem/execution providers, mount-aware path mapping and cross-OS tests |
| 23 | Database panels and administration (SQL and document stores) | **pending** — SQLite/PostgreSQL first, read-only browsing and SQL workspace, reviewed edits, comparison, transfers, backup/restore and AI DBA; SQL Server/MySQL/MariaDB and native MongoDB document provider (23.10) later |
| 24 | Afrowave Personality Studio and creative agent teams | **pending** — editable persona packs independent of model/role/permissions, optional creative teams, local/remote image providers, structured document composition and export, and a separately deployable affordable-model public gateway; reuse Phase 16 and keep Agent Commons text-only |
| 25 | Persistent Workspaces and durable runtime | **pending** — workspace/session restore, GUI-independent execution, durable to-do/step/attempt ledger, safe-unit drain, verified effect recovery, optional background/tray and restart |
| 26 | External Agent Runtimes | **pending** — optional DeepSeek Harness SDK adapter, native chat, scoped MCP, independent Git/release roles and multi-runtime teams |
| 27 | Capability Discovery and Plugin Workshop | **pending** — semantic category contracts, reusable panels, discovery across RC/Harness/MCP and independently reviewed plugin creation/activation |
| 28 | Virtual Hardware Laboratory | **pending** — virtualization providers, independent host and guest controls, exclusive approved USB, driver test/recovery and lab workspaces |
| 29 | Network Workbench | **pending** — scoped DNS/routes/proxy/TLS/VPN/firewall diagnostics, optional packet capture and evidence-based diagnosis |
| 30 | Browser and API Workbench | **pending** — HTTP/API collections and contract tests, browser adapter, end-to-end visual/functional acceptance |
| 31 | Local Knowledge Engine | **pending** — private local full-text index, optional OCR/embeddings and source-linked retrieval |
| 32 | Workflow Studio | **pending** — versioned reusable graphs, deterministic steps, agent handoffs, checkpoints and optional personal triggers |
| 33 | Remote Visual Workspace | **pending** — optional RDP/VNC/VM console and scoped graphical evidence/interaction |
| 34 | Directory Services and Identity | **pending** — AD/Samba/OpenLDAP panels, access diagnostics, reviewed object management and typed provisioning templates |
| 35 | Backup, Synchronization and Recovery | **pending** — provider adapters, comparison/sync previews, verified backup and restore drills |
| 36 | Developer Toolchain and Software Integrity | **pending** — SDK/toolchain inventory, optional LSP/DAP, dependency/license review, SBOM and artifact verification |
| 37 | Research and Internet Workbench | **pending** — early shared HTTPS/proxy, GitHub/Learn/search adapters, staged downloads and cited evidence |


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

`cargo test -p rc-core` → **244 tests, 0 failures.**

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

## Phase 7 — `rc-remote` (partial; historical baseline followed by updates)

`cargo test -p rc-remote` → **111 tests + 3 doctests, 0 failures** (offline, with
an in-process mock FTP server; 115 with `--features sftp`, 110 with
`--features ftps`).

Implemented and tested: FTP passive/EPSV and active PORT/EPRT, `MLSD` with
`LIST` fallback (including the year-vs-time and symlink cases), `MDTM`/`SIZE`/
`MLST`, chunked cancellable `get`/`put`, recursive `get_tree`/`put_tree` with
path sanitisation, error mapping (550 → not found, 530 → authentication, refused
connection → connect error). FTPS/SFTP were originally compile-checked and unit-covered only. **Later work** recorded in the secure-remote sections includes a live OpenSSH/SFTP test, host-key checks and encrypted credentials. The counts above describe the earlier baseline, not the latest implementation.

Wired into the application: `Ctrl+F` opens the connect dialog, which builds an
`rc_remote::ConnectionSpec`, connects and turns the active panel into a remote
directory (listing with sizes, dates, symlink kinds and unix modes);
`Ctrl+Shift+F` disconnects. Navigation, `download_selected` and `upload_paths`
work through a [`RemoteBackend`] abstraction, so the behaviour is covered by
tests with an in-memory backend while production uses the real client.

Remaining gaps: an interactive password prompt is needed when no stored secret is available. The FTP transfer path also needs a final audit against the common background queue; optional live SFTP tests must be reported separately from offline tests.

## Phase 15.4a — reading the machine's Git configuration (done, behind approval)

`cargo test -p rc-git --lib config` → **7 tests**, `cargo test -p rcommander --lib git`
→ **5 tests**, plus 2 UI tests.

`rc_git::config` reads the merged system, global and repository configuration with
libgit2 and exposes only what the application can use: identity, `credential.helper`,
`url.*.insteadOf` rewrites (with longest-prefix matching), `core.sshCommand`,
`init.defaultBranch`, the files it read and any problems, plus the credentials found in
a `store` helper's `~/.git-credentials`.

That last part is why the whole thing is gated. *Configuration ▸ Git configuration…*
loads the configuration **into the dialog** and shows exactly what was found — hosts
and helper names, never a password — with **Allow** / **Not now** and a *Remember this
decision* checkbox that writes `[git] read_system_config = "ask" | "allowed" |
"denied"` into `settings.toml`. `StoredCredential` has a hand-written `Debug` that
prints `<redacted>`, so a stray log line cannot leak a secret, and credentials never
touch the disk.

The model keeps the loaded configuration in `git_config`, offering `git_identity()`,
`git_credential_for(protocol, host)`, `git_rewrite_url(url)` and `git_config_summary()`
to the parts that will use them (remote authentication in 15.4b, clone in 15.5).

## Help text: a guard against characters nothing can draw (fix)

`cargo test -p rcommander --test ui_smoke every_help_character` → **1 test**.

Writing the new help topics exposed a mistake that had been in the documentation from the beginning:
the menu paths were written with `▸` (U+25B8), which none of the bundled fonts contains. The
application draws a **tofu box** for such a character, so every "Configuration ▸ Options" in the help
had been showing a small empty rectangle. `egui_kittest` turns the same thing into a panic, which is
how it was finally noticed.

28 help files now use `→`, which the fonts do contain, and a new test walks every `Docs/*/*.md` and
asks egui whether each character can be drawn with the bundled faces. A future topic that gains an
emoji or an exotic arrow fails here instead of shipping a box — help topics are user-editable files,
so this is the editor's guard as much as ours.

## Phase 16.17c — pins and recency (done)

`cargo test -p rcommander --lib pin_tests` → **4 tests**, one config round-trip test, one UI test.

The picker listed the catalogue alphabetically, which is a list nobody scrolls. It is now ordered by
usefulness: **pinned** prompts first (marked with `*`, a *Pin*/*Unpin* button on every row), then
what was used in this session, then the rest.

The split between the two is deliberate. Pins are a **setting** (`[prompts] pinned` in
`settings.toml`), so they survive a restart — a test writes and reads the file back, and one checks
that a settings file from before this section still loads. Recency is **memory only**: a file that
remembers which question you asked when would only grow, and its value expires within the hour.
The recent list has a budget of eight entries, which a test pins down.

The category filter and the search still win: a pinned Files prompt does not appear in the Git tab,
because an ordering that overrides an explicit filter is not an ordering, it is a bug.

## Phase 16.17b — the prompt editor (done)

`cargo test -p rc-core --lib prompt` → **13 tests**, `cargo test -p rcommander --lib editor_tests` →
**4**, plus a UI test that edits a shipped prompt and resets it.

*Configuration → Agent → Prompts…* (`cm_Prompts`) closes the gap the previous commit left open:
prompts can now be created, changed and reset without opening a text file. The page shows the
catalogue by category with a badge (*mine*, *edited*) and the form beside it: title, one-line
description, category for a new prompt, the body, and — the useful part — **what each variable in
the text will be replaced with right now**, which is the fastest way to notice that `{{host}}` is
empty because the panel is local.

The mechanism is one rule instead of three: **editing a built-in is a copy with the same id.** The
shipped file is never written to (an upgrade replaces it), the catalogue merges by id so the picker
shows one prompt and not two, and *Reset* means exactly "stop overriding, use the shipped text".
While the user edits, the page shows the shipped wording underneath, so resetting is not a leap of
faith. `rc_core::prompt` keeps the originals in a separate table (`originals`), which is also what
lets `is_overridden` answer the badge question honestly.

A new prompt gets an id derived from its title (`diagnostics.proc_to_spadlo`), because an id is what
a settings file or an audit entry would refer to and nobody should have to invent one. Non-ASCII
characters become underscores: the id has to survive a file name and a TOML key on every platform.

## Phase 16.16/16.17a — the prompt catalogue and its picker (done)

`cargo test -p rc-core --lib prompt` → **11 tests**, `cargo test -p rcommander --lib prompts` →
**7 tests**, plus **2 UI tests**.

There was no prompt catalogue at all: every session started from an empty input. `rc_core::prompt`
is a TOML catalogue grouped by category — files, Git, remote, build, diagnostics, general — with
**26 built-in prompts** shipped as data files (`prompts/*.toml`), a user layer in
`<config>/prompts/`, typed variables and a renderer.

The design rule that keeps the feature honest is in the module docs and in the help topic: **a
prompt is data**. Inserting one fills the input and grants nothing; permissions stay in the policy,
and there is deliberately no `{{file_content}}` variable that could quietly send a file somewhere.
The variables are the five things the application actually knows: `cwd`, `selection`,
`selection_count`, `model`, `host`.

Three decisions came out of writing the tests:

* **An unfillable variable stays visible.** `{{host}}` in a local session is printed as `{{host}}`
  and the preview says which names were not filled. A blank would be a silent bug; a visible marker
  is an honest one, and the input still shows what will be sent.
* **A broken file costs only that file.** A TOML typo, a prompt without an id or without text is
  reported in `problems()` and skipped, so one mistake cannot empty the catalogue. Built-in prompts
  are read-only by construction — an upgrade must never overwrite the user's words, so changing one
  means copying it.
* **Saving one prompt keeps its neighbours.** Saving rewrites the category's file after merging what
  is already there, which a test checks by saving two prompts and reloading from disk.

The picker sits in the chat input: category tabs, a search field, the list with a hover text on
built-in prompts and a **preview of the finished text** before the insertion. `Configuration →
Prompts` (the editor) and the reset-and-diff for an edited system prompt are the next slice; until
then the TOML file is the editor, which is versionable and portable, as the help topic says plainly.

## Phase 17.0b — C#, SQL, HTML, CSS and three shell families (done)

`cargo test -p rc-core --lib syntax` → **21 tests** (8 more than before).

The highlighter covered nine languages; it now covers the ones the panel owner actually meets:
**C#**, **SQL**, **HTML/XML** and **CSS**, plus **PowerShell** and **Windows batch** beside the POSIX
shells. Fence words and file names both work (`cs`, `csharp`, `c#`, `Main.cs`; `ps1`, `pwsh`;
`run.cmd`; `page.html`, `xml`; `site.css`, `scss`).

Four of them needed rules rather than keyword tables, and that is where the honest work was:

* **HTML** colours the tag name, the attribute names and the quoted values, and leaves the prose
  between them alone — that is what makes the structure readable instead of a coloured paragraph.
  An HTML comment carries over to the next line like any other block comment.
* **CSS** knows the difference between a selector, a property and a value: it tracks `{ … }` depth
  across lines, keeps the `.`/`#` of a selector attached to its word, colours a hex value as a
  number, and does not mistake `0` for a property name.
* **PowerShell** colours `$variables` (including `$env:PATH`), `-Parameters`, `Verb-Noun` cmdlets
  and its own keyword list, all matched without regard to case.
* **Batch** colours `REM`/`::` comments, `:labels`, `%VAR%`, `!VAR!` and the positional arguments
  `%1`/`%*` — the last of which have no closing `%`, which a naive scanner gets wrong.

SQL matches keywords in either case (`SELECT` and `select` are both real style), and its `--`
comments are claimed explicitly, because the generic scanner would read them as two minus signs.

**The Configuration menu was reorganised** at the same time, since a flat list of eighteen buttons
is a wall, not a menu: *Colors*, *Fonts*, *Security* and *Factory resets…* open the options dialog
on their own page (`cm_Colors`, `cm_Fonts`, `cm_Security`, `cm_Dangerous`), and **Git ▸** and
**Agent ▸** are submenus. *Show files* appears in the agent submenu only while a panel is actually
showing the chat, because a menu entry that does nothing is worse than no entry.

## Phase 16.1c — the chat lives in a panel (done)

`cargo test -p rcommander --lib chatpane` → **4 tests**, plus **2 UI tests**.

The chat was only a floating window, which is the wrong shape for the question it exists to answer:
a window covers the files it is talking about. A panel can host the conversation with the other
panel still listing a directory, and that is what the plan asked for.

`Panel.chat` is the whole state: when it is set the panel draws the chat instead of its listing, and
the listing is **not** touched — same directory, same cursor row, same marks. That is the property
the tests pin down, because the opposite (a pane that quietly reloads and loses your place) is the
kind of thing a user only notices after it bites them. `cm_ChatPane` puts the chat in the *passive*
panel, `cm_SrcChat`/`cm_TrgChat` toggle one named panel, and `cm_ChatFiles` — or the pane's own *Show
files* button — restores **both** panels, since two chat panes would leave no file list at all.

The window and the pane share one rendering function, so they cannot drift: the window keeps its
frame and title, the pane gets a header with the panel name and *Show files*. A panel is narrower, so
its input row is one line instead of two rather than pushing the transcript out of sight.

Keyboard handling needed no new rule: typing into the input takes the focus, and the existing
shortcut handler already steps aside when a widget has focus. That is why F-keys do not navigate the
hidden list while somebody is typing a question.

## Phase 18.2a — one `Location` variant for every plugin (done, first slice)

`cargo test -p rc-core --lib plugin_tests` → **5 tests**, `cargo test -p rcommander --lib sources` →
**7**, and the whole workspace still green.

The panel owner's question was the right one: instead of a `Location::Virtual` per kind of source,
**one variant** covers all of them — `Location::Plugin { source, path }`, where `source` is a
registered id and `path` is an opaque, slash-separated locator the source understands
(`mailcraft://inbox/42`). Every source that will ever exist fits that shape, so the enum never grows
again and no `match` in the codebase has to learn about the next one.

What that bought, concretely:

* the enum gained **one** arm in eight places, not eight arms per source kind;
* `display`/`parse` round-trip (`mailcraft://inbox/42`), and parsing accepts a plugin scheme only
  when the caller's registry names it — so a typo cannot become a source that fails mysteriously;
* **built-in schemes win**: a plugin cannot shadow `sftp`, `zip` or `ftp`, because installing a
  plugin must never change what an existing location means;
* navigation is path-like for free: `child` appends a segment, `parent` removes one, the root is
  named after the source — a tree-shaped source gets tabs, history and cursor handling without
  writing any of it;
* `as_path()` is `None`, which is the honest answer for a mail id and keeps every file operation
  from pretending otherwise.

The behaviour lives in `model::sources`: a `PanelSource` trait with three questions — *what is in
this location*, *what does opening this row mean*, *can it be read at all* — and a `SourceRegistry`
on the model. `Model::open_in` dispatches plugin locations there and hands the finished entries to
`Panel::open_with_entries`, so **`Panel` itself never learns that plugins exist**. A source that fails
produces an error in the panel, never an empty list, because "the server did not answer" and "this
folder is empty" must not look the same.

Next on this path: the source's **columns and actions** (18.2a), the **MCP-declared source with no
code** (18.2b), and the operation capability flags so `F8 Delete` is *absent* over an inbox rather
than failing at the end.

## Phase 16.37a — a remembered catalogue with per-tool trust (done)

`cargo test -p rc-core --lib mcptools` → **7 tests**, `cargo test -p rcommander --lib mcptools` →
**4**, plus a UI test that walks the three answers through the model.

The panel could connect to another application and list its tools, but it could not *remember*
anything about them: every connection started from zero, and nothing said which of a server's tools
were safe. `rc_core::mcptools` is that memory, stored in the settings so it survives a restart.

Every tool gets the three-state switch the panel owner asked for — **allow**, **ask**, **deny** — and
`ask` is the default for everything, including a brand-new tool a server added in an upgrade.

The danger estimate reads the **name and the argument schema, never the description**, because a
description is written by the server's author and may say anything. Four levels: reads, changes data,
destructive, unknown — and `unknown` is treated as carefully as destructive, never as safe.

Three rules give the catalogue its teeth, each with a test:

* **A destructive or unknown tool cannot be switched to `allow` by a click.** It needs the same typed
  confirmation a permanent file pre-approval needs, and the allowance is written to the audit log.
  Where the OS offers elevation the Phase 20 helper will be the authority; until then the typed word
  is the human's signature.
* **A tool that changes nature loses its answer.** An upgrade that adds a `recursive` flag to
  `read_file` changes the estimate, and the old `allow` is dropped back to `ask`. A renamed tool
  inherits nothing; a new tool starts at `ask`.
* **`deny` removes the tool from what the model is offered**, not merely from what it may call.

Security **zones** — loopback, the trusted overlay, client overlays, the domain and the internet,
each with its own posture — are designed in the plan as 16.36 and are the next step here: a zone says
how much the *peer* is trusted, the catalogue says what the *tool* does, and the stricter of the two
wins.

## Phase 16.4b — hunks and syntax checks in the review (done)

`cargo test -p rc-core --lib diff` → **13 tests**, `cargo test -p rc-core --lib validate` →
**5**, `cargo test -p rcommander --lib workspacereview` → **9**, plus a UI test that a broken JSON
file is refused while its hunks are still readable.

The review approved whole files, which is the wrong unit for the work: an agent that fixes a bug and
also reformats the file makes a change nobody wants to accept or reject wholesale. The diff is now
split into **hunks**, each with its own tick, and the file is assembled from the ones taken —
selecting everything reproduces the agent's copy byte for byte, selecting nothing reproduces the
original, and the tests check all three cases against each other because a merge that is *almost*
right is worse than none.

`rc_core::validate` puts a parser in front of the reader: JSON and TOML (plus `ini`/`cfg`/`conf`) are
checked in-process, and a file that fails is not pre-ticked, says why in the parser's own words, and
has none of its hunks ticked. A kind of file we cannot check returns `Unknown` and says **nothing** —
a false alarm on a valid file would train the user to ignore the warning, which is worse than not
having one. Running a compiler or a test suite is deliberately excluded: that executes code and
belongs behind its own approval.

What is left in 16.4 is a configurable checklist of approved commands (`cargo check`, a JSON Schema,
a formatter) that the review runs with progress and cancellation before showing the diff.

## Phase 16.10b / 16.4a — sending changes back, with a diff and a backup (done)

`cargo test -p rc-core --lib diff` → **7 tests**, `cargo test -p rcommander --lib workspacereview` →
**6**, plus a UI test that reviews two files and writes only the ticked one.

The workspace could prepare copies and hand them to the agent; it could not take anything back. It
can now, through the panel owner's rule: **no verified backup, no change**, and nothing written that
the user did not tick.

`rc_core::diff` is a small LCS line diff. It is honest about its limit: past a cell budget it stops
guessing and reports the file as wholly replaced, and the row says *large file* instead of pretending
to have read it. Long unchanged stretches collapse to `… n lines unchanged`, so a diff of two files
that differ in one place is three lines, not a thousand.

The review (`model::workspacereview`) is one dialog: the changed files with a tick each and their
diff beside them. Only files that changed are pre-ticked, a file that did not change is not written
at all, and a **binary** file is reported as differing and never sent back by a stray Enter.

Three safety rules, each with a test:

1. **Per-file approval.** `apply_workspace_approved` writes exactly the ticked files; a test applies
   a change to one file and checks the other is byte-identical.
2. **A stale original is refused.** If the original changed between "process with agent" and "send
   changes back", the copy is no longer a change to *that* file; overwriting the newer version would
   be the worst thing this feature could do. The row says so, the file is not ticked, and applying
   refuses — a test edits both sides and proves the human's version survives.
3. **Every write goes through the transaction layer**: `prepare` takes and *verifies* the backup
   first, then `apply` writes, then re-hashes. The backups land next to a manifest in
   `<config>/agent-workspaces/apply-<timestamp>/`, so an approved change is reversible.

Each write is recorded in the audit log with *approved in the workspace review* as its reason, and
the manifest is updated afterwards so the workspace stops claiming a difference that no longer
exists.

What is still open in 16.4: **hunk-by-hunk approval** and **running validators** (JSON/TOML parse,
`cargo check`) on the agent's output before the review appears.

## Phase 16.10a — process with the agent, on a copy (first slice, done)

`cargo test -p rc-core --lib workspace` → **7 tests**, `cargo test -p rcommander --lib
agentworkspace` → **4**, plus a UI test that runs the whole handoff.

The panel owner's idea, built: right-click a file or a folder, choose **Process with agent…**,
and the agent gets a **copy** in a workspace while the chat opens beside the files and asks what to
do. `rc_core::workspace` does the copying and the bookkeeping; `model::agentworkspace` connects it to
the app.

What makes it trustworthy is not the copying but the **manifest** (`workspace.toml`): for every file
it records the original path, the copy, the SHA-256 of the original and the size. That is what makes
"has the copy changed?" answerable without asking the agent — a test edits a copy and checks that the
original is byte-identical afterwards, which is the property the whole feature rests on.

Four decisions worth naming:

* **Copies, not the originals.** The workspace root is what the agent may see; the policy decides
  what it may reach. At the lowest privilege an agent that can only break a copy is one you can
  afford to let run.
* **One unreadable file does not cost the others.** Unreadable files are named in the transcript and
  skipped; if nothing could be copied at all, the workspace is removed and the user gets an error
  instead of an empty conversation about nothing.
* **Folders are copied as trees**, with a depth limit, because a symlink loop is a real thing in a
  file manager.
* **Discarding is the default end.** Closing the workspace deletes the copies; the originals were
  never in it. Sending changes back is Phase 16.4's review pipeline (diff, per-file approval, a
  transaction with a verified backup) and is deliberately not part of this slice — the help topic
  says so plainly.

The chat is seeded with what the agent can see and the reminder that it is a copy, so the next thing
the user does is describe the task rather than explain the setup.

## Phase 14.9 — a keyboard that follows the focused surface (done)

`cargo test -p rcommander --lib surfaces` → **7 tests**, plus **2 UI tests** (the bar changes with
the surface, and an export writes a readable file).

The F-key bar was one static table in the view layer: eight keys, always the file commands. Over the
chat that meant `F3` tried to preview a cursor row that does not exist, and there was no way to ask
the keyboard for "change the model" or "export the conversation".

`model::surfaces` now decides three things and is the only place that does: **which surface has the
focus** (editor, then viewer, then a panel showing the chat, then the file panels), **which command
each key means there**, and therefore **what the bar draws**. Four rows of twelve slots:

| Surface | `F1`…`F4` |
| --- | --- |
| files | help, re-read, preview, edit … `F12` = terminal here |
| chat | help, model picker, prompt picker, save the question as a prompt |
| editor | help, save, save a copy, reload |
| viewer | help, re-read, and the three modes, zoom, close |

Three rules keep it honest. An empty slot is **absent from the bar**, not disabled — a button that
cannot work is worse than no button. A test walks every row and checks that each command exists in
the catalogue or in the application's own list, so a renamed command cannot linger in a bar. And
`F1` is help everywhere, opening the **topic of the surface it was pressed on**: the chat, the
editor, the viewer or the shortcuts themselves.

Moving the actions into the model was the useful side effect. Editor save, save-a-copy, reload and
close, the chat's clear/stop/export/save-as-prompt, and the viewer's mode and zoom are all model
methods now, so the window's button, the function key and a headless caller share one implementation
— and the TUI frontend will get the same keyboard for free.

## Phase 14.10 — the window that never stays silent, and SSH that says why (done)

`cargo test -p rc-core --lib sound` → **3 tests**, `cargo test -p rc-core --lib expand` → **1**,
`cargo test -p rcommander --lib stage_tests` → **2**, `-p rc-remote` → **113**, plus a UI test for
the status line.

Three things came out of one evening of use.

**The model access fix was reviewed, not just accepted.** The incoming "keep cold model requests
alive" work is sound: it splits "activity" from "text" so a cold model can be *loading* without
being silent, cancels by shutting the socket (not by killing the server), and remembers the load
time per provider and model so the next cold start can give an estimate. Three things needed
tidying: `StreamEvent` had no documentation (it is public API now), the load-status probe thread
started even for providers that had already said they cannot answer *and* had no time limit, and a
deprecated `Id::new` was left in the chat window. The probe now only starts when the provider can
answer and stops after five minutes.

**SSH: the key that "did not exist".** The dialog hints at `~/.ssh/id_ed25519`, and nothing expanded
the `~` — so the key file the user was told to type was looked up as a literal directory called `~`,
and the failure looked like a *host key* problem, which made choosing "ignore" useless. `~` is now
expanded (`rc_core::drive::expand_home`, one test), a missing key is named with the path that was
actually tried, and an empty key path explains what to do instead of saying nothing.

**A connection now narrates itself.** `rc_remote::ConnectStage` (resolving → connecting → handshake
→ host key → authenticating → opening) travels through the connect job into the status line, so a
slow attempt says *authenticating* instead of showing nothing. The stage a failure stops at is the
answer to "why did it fail", and the help topic has a table for it.

**Status line and sound.** The status bar now shows the current activity with a spinner (connection
stage, transfer percentage, search, a thinking agent) above the message, and errors become
**alerts** in the warning colour that stay for twelve seconds; a finished answer is a check mark for
four. Sounds are `[sounds]` in the settings: on for errors and dropped connections, **off** for
successes, because a file manager that chirps on every copy gets muted and then misses the error.
They are the platform's own sounds (`canberra-gtk-play`/`paplay` on Linux, the system files on
macOS, a generated tone on Windows) with a fallback chain and a *Test* button; a machine that cannot
play them loses nothing, because every sound also writes to the status line.

## Phase 14.8b — the evening of small defects (done)

`cargo test -p rc-core --lib desktop` → **8 tests**, `cargo test -p rc-viewer --lib reformat` →
**3**, two model tests for the editor and the hex helpers, plus the UI suite.

The panel owner reported eight things that were wrong or missing. Each was a real defect and most
had a cause worth remembering.

1. **Enter and double-click opened the internal viewer** instead of doing what the system would do.
   A new `CommandOutcome::OpenExternally` splits "hand it to the platform" from "look at it here":
   Enter and a double-click run or open the file, `F3` stays the internal viewer. The pure half of
   the platform call lives in `rc_core::desktop` (which program, which arguments, per platform, with
   no shell string building), the impure half in `rcommander::system_open` (find the program, spawn
   it, never wait).
2. **The up and refresh buttons above a panel acted on the other panel.** They pushed a
   source-panel command without making their own panel the source. One line each: activate the
   panel the button sits in, then send the command.
3. **The editor could not save where it had no rights.** It now warns before the user types ("this
   file cannot be written"), reports the system's own error on a failed save, and offers two honest
   ways out: save a copy into the documents folder (numbered so a copy never overwrites), or ask the
   system for elevated rights (`pkexec` / UAC / `osascript`) — and say so plainly when elevation is
   not available. Binary files are refused with a reason instead of being shown as mojibake.
4. **The editor and the chat had no window controls.** `ui::workspace_window` gives both a title row
   with min / max / pin / close. Maximize and pin fill the rectangle the two panels occupy — captured
   every frame — so they follow a resize of the main window; minimize folds the window to its title
   bar without losing the buffer.
5. **The chat's bottom row was half outside the window** because the input height was subtracted
   from a guessed available height. The input row is now a bottom panel *inside* the window, so it
   keeps its own height while the transcript above scrolls.
6. **The context menu stopped at file operations.** It now offers the system's actions: open with
   (the native chooser on Windows, elsewhere a searchable list read from the desktop's `.desktop`
   files), send by e-mail, run in a terminal, open a terminal here.
7. **The hex dump could not be read.** The bytes-per-line slider changed a value nothing re-read, the
   whole dump was one label inside an unbounded scroll area (so the window grew instead of the
   content scrolling), and a row was just text. It now re-formats from its own bytes, scrolls with
   `show_rows` (so a multi-megabyte file costs the same as a small one), selects a row by click and
   shows that row as unsigned, signed, 16- and 32-bit numbers, and jumps to an offset typed as
   `0x200`, `512` or `200h`.
8. **The function keys do not follow the focused surface** — planned as Phase 14.9, with the chat's
   own row (provider, model, save prompt, export conversation) and the `Ctrl`/`Alt`/`Shift` variants
   the panel owner asked for, because the coming TUI frontend needs the same layer.

## Phase 17.0 — a small highlighter and six colour roles (done)

`cargo test -p rc-core --lib syntax` → **13 tests**, `cargo test -p rcommander --lib code_tests` →
**3**, plus a UI test that an answer's code block is drawn coloured.

Code blocks in a model answer, in a help topic and in the F3 viewer were monospaced grey text. They
are now coloured by a highlighter in `rc_core::syntax`: Rust, TOML, JSON, shell, Python,
JavaScript/TypeScript, C/C++, Go and Markdown, each with its own keyword table and its primitives
(`usize`, `string`, `rune`), plus strings with escapes, numbers, types, calls and comments.

Three properties were designed in rather than hoped for:

* **Colouring never changes the text.** Every test in the module checks that the concatenation of a
  line's spans equals the line, for all ten languages and for hostile input (unterminated strings
  and comments, tabs, Czech diacritics, triple quotes, a stray comment terminator). That property
  is what makes it safe to highlight an answer that is still streaming in.
* **The state that crosses a newline is the only state.** A block comment or a Python triple-quoted
  string continues on the next line because the highlighter carries exactly that, not because the
  caller re-reads the block.
* **No dependencies.** A regex engine would have been a large build cost for colouring a dozen
  tokens per line; the tokenizer is a single pass over `Vec<char>`.

Six theme roles (`code_keyword`, `code_string`, `code_comment`, `code_number`, `code_type`,
`code_function`) were added, so highlighting follows the theme instead of hard-coded colours. Every
shipped theme got its own six colours chosen for its character — the amber and AS/400 terminals stay
in their own hue, Solarized keeps Schoonover's accents — and the shipped-theme test now measures
each of them against that theme's background at 3:1. That test immediately failed on Solarized's
own `base01` comment colour (2.79:1), which is a real readability problem in that palette and not a
mistake in the test.

## Phase 14.7 — three text roles and your own fonts (done)

`cargo test -p rc-core --lib font` → **5 tests**, `cargo test -p rcommander --lib fonts` → **4**,
`--lib font_tests` → **3**, `--lib icon_fallback_tests` → **1**, plus a UI test.

There was one font size and no say about the face. There are now three roles — body (lists, menus,
dialogs), headings, and monospace (paths, the hex viewer, the editor, code in help) — each with its
own family and size, and each family can be any `ttf`/`otf` installed on the machine.

`rc_core::font` finds them: it walks the platform's font directories, depth-limited and with a file
budget, and turns a file name into a family (`DejaVuSansMono-Bold.ttf` → *DejaVu Sans Mono*, the
Bold not offered as a family of its own). Two things are deliberately strict. Only files that
**start** like a font are offered — a stray `.ttf` would be parsed lazily by egui and turn into a
panic on the first glyph — and a name with no recognisable family (`ab12cd.ttf`) is dropped, because
offering "Ab12cd" as a font would be a lie. `.ttc` collections are skipped: they hold several faces
and somebody would have to choose one.

The scan happens **once**, on first use: an installation that keeps the built-in fonts never walks
the font directories at all. The options page shows a live sample in all three roles, and a family
that was chosen but has since been removed from the system is reported on the page with the
built-in face still in use, instead of failing at start-up.

The subtle part was the icons. Their font is the *lowest* fallback of the built-in families, so a
custom family would have left every tool-bar symbol undrawable — and egui treats a missing glyph as
a panic. The icon font is therefore registered again as the last fallback of each custom family,
under a **unique name per family**: `add_font` ignores an insert whose name already exists, so
reusing the icon font's own name silently did nothing. A test asks egui for the family's font chain
and then for a glyph of every one of the 38 icons, which is exactly the check that fails if that
registration regresses.

## Phase 16.2a — model providers and in-app model choice (done)

`cargo test -p rc-llm` → **8 tests**, `cargo test -p rcommander --lib ai::` → **5 tests**, plus a
UI test.

New crate `rc-llm`: a small HTTP/1.1 client (plain sockets — no TLS stack, no async runtime, so
Android still builds), an `AgentProvider`-style `Provider` trait, and the **Ollama** adapter:
`/api/tags` for the model list (id, size, parameter count, capabilities) and `/api/chat` for one
question. Bodies are parsed by separate functions, so the protocol is tested without a server;
the transport is tested against an in-process fake HTTP server, and a live probe runs only when a
real Ollama answers (it never fails the suite).

Measured against the panel owner's machine: **52 models**, including `flint` (39.8B),
`ripple-preview` (64.8B, 2M context), `ornith-1.5:35b` and `nemotron-3.5-lightning:32.9B` — all
with `tools`, which is what the agent panel will need.

In the application, *Configuration ▸ AI model…* (`cm_AiModel`) lists what the provider offers with
capabilities next to each name, marks the chosen one, and lets the user **use** a model or **test**
it. Testing asks the model a trivial question on a worker thread, so a 60 GB model loading does
not freeze the window, and the answer is shown in the dialog and the status line. A fresh
installation with no model chosen takes the first tool-capable one.

## Phase 16.1b — answers rendered as Markdown (done)

`cargo test -p rcommander --lib markdown_view` → **5 tests**, plus **2 UI tests**.

A language model writes Markdown, so the chat was showing `**` and `#` instead of emphasis and
headings — the answer was correct and unpleasant to read. The block renderer that already existed
inside the help window now lives in `ui::markdown_view`: one module turns `rc_help`'s
[`Block`]s into widgets, and both the help window and the chat use it, so the two cannot drift
apart. Headings, paragraphs, lists, quotes, tables, fenced code and rules all render; inline bold,
italic and code keep their shape.

Rendering a **stream** needed two details. Parsing every line on every frame would re-parse the
whole transcript sixty times a second, so each line carries a stable `seq` and the view caches its
parsed blocks, re-parsing only while the text is still growing — keyed by identity rather than
position, because positions shift when the transcript is cleared. And because the parser never
fails (partial markup degrades to text), a half-received answer renders as it arrives: an unfinished
code fence keeps its code lines visible, which the tests check.

## Phase 16.0e — the window answers the agent's question (done)

`cargo test -p rc-agent --lib bridge` → **6 tests**, `cargo test -p rcommander --test
agent_approval` → **6 tests** (two of them drive the whole round trip).

Everything in 16.0d was reachable only for an agent running *inside* the application. A real MCP
client starts `rcommander mcp` as a subprocess, so the dialog could never appear in the window the
human is looking at — the terminal confirmer was the only path, and a client started from an IDE
had none at all.

`rc_agent::bridge` closes that gap with a **loopback TCP socket**: the window publishes a port and a
per-run token in an owner-only (`0600`) file in the runtime directory, a client connects, sends
`{token, tool, args}`, and waits. The window turns the request into the *same* approval dialog an
in-process agent gets — with a line saying that an agent is waiting, because a blocked client is
easy to overlook — and the human's answer travels back as `once` / `session` / `always` / `deny`.
The server never decides anything; it forwards, with a five-minute deadline that becomes a denial.

The transport choice is deliberate: loopback TCP works on every platform we build for without a new
dependency and without an async runtime. The token file is owner-only, so only the same user's
processes can read it; a same-user process could ask for a dialog it cannot see, which the human
still answers — the worst case is a nuisance prompt, never a silent approval. A wrong token gets
silence, not even a hint about what the port is.

`rcommander mcp` now prefers the window, falls back to the terminal, and refuses when it has
neither; the fallback is announced on stderr so nobody wonders which one is in charge. Two
integration tests run the whole round trip — the client blocks on the socket while the window
pumps its loop, opens the real dialog, answers it, and the client obeys — and both finish in a
fraction of a second because the bug they were written to catch (answering *after* the dialog had
closed, so the id was gone) failed loudly instead of hanging.

## Phase 16.0d — three answers, permanent pre-approvals and a security page (done)

`cargo test -p rcommander --lib policy` → **13 tests**, `cargo test -p rc-core --lib policy` →
**19 tests**, **2 UI tests** for the dialog and the page, plus the risk-statement key guard.

The approval dialog used to offer two buttons and a checkbox, which hid the most interesting
question: *how long* does this yes last? It now offers the three answers the panel owner asked for —
**allow now**, **allow for this session** and **allow always**, the last one being the red one.

**Allow now** is unchanged. **Allow for this session** writes an in-memory
`Policy::approved_this_session` entry next to the session grant, because an approval-worthy
operation is gated *after* the grant check: a grant alone would have been silently ignored and the
same question would have come back on the next call. The stop button clears both.

**Allow always** opens a second window that states the exact scope ("the agent may push and
upload, without asking, in every session"), says the decision is written to `settings.toml`, and
requires typing `yes` before *Confirm* is clickable. Only the exact word counts — `ano`, `yess` and
`true` are refusals — because a click can be a mis-click and a typed word cannot. The answer is
stored as `[[agent.preapproved]]` and read back by `Policy::from_settings`, so a restart remembers
it. It is deliberately bounded: the deny list still wins, and a profile that never offers an
operation (elevated work under `sandbox`) cannot pre-approve it either.

Writing the test for the session case exposed a real policy hole: `publish`, `execute` and
`network` take no path, so nothing can cover them with a path grant, and "no grant" made them a
**hard denial** — the dialog was unreachable for exactly the operations the panel owner wants to be
asked about (a push). A non-path operation without a grant now falls through to the approval gate
instead: "nobody pre-allowed it" means the human decides, not that the answer is no.

**Configuration ▸ Security** is the page that makes all of this inspectable: the profile with one
honest sentence each, which operations always ask, every stored pre-approval with its date and a
*Take it back* button, the live session approvals with *Stop everything now*, and the audit log's
hash-chain state plus its last line. The page also fixed a real bug: accepting the options dialog
never rebuilt the policy, so changing the profile had no effect until a restart.

## Phase 16.0c — an approval source outside the GUI (done)

`cargo test -p rc-agent --lib confirm` → **6 tests**, `cargo test -p rcommander --test
agent_approval` → **4 tests**, plus the flag test in the binary.

Until now the stdio server had **no** confirmer, so every destructive tool failed closed and the
Git tools it advertises (`git_commit`, `git_checkout`, `git_push`) could not be used at all. The
reason was not laziness: the obvious implementation is wrong. Standard input and output carry the
JSON-RPC protocol, so a question printed on stdout corrupts the stream and an answer read from
stdin swallows a message.

`rc_agent::confirm` therefore asks on the **terminal device** (`/dev/tty`, `CONIN$`) and answers
by these rules: only an explicit `y`/`yes`/`a`/`ano`/`ok` allows the call, everything else — an
empty line, a closed stream, a poisoned lock — is a **no**, and a process without a terminal gets
no confirmer at all, which keeps the historical fail-closed default as a deliberate choice.
`AllowAllConfirmer` exists only for `--yes`, and its name says what it does.

The `mcp` subcommand now takes `--ask` (default), `--yes`/`-y` and `--no-ask`, and the flags that
were advertised but never implemented (`--http`, `--token`) fail with an explicit message instead
of being silently ignored — an option that does nothing is worse than one that refuses.

`rcommander::agent_host::serve_streams` is the seam the tests drive: four integration tests run a
real `delete` through the real server with a fixed confirmer and check the outcome — refused means
the file is still there and the human was asked exactly once, approved means the file is gone, no
confirmer means nothing is deleted, and reading a directory asks nobody.

## Phase 16.1 — streaming answers and the agent chat (first slice, done)

`cargo test -p rc-llm` → **11 tests**, `cargo test -p rcommander --lib chat` → **6 tests**, plus
**2 UI tests**.

**Streaming.** `Provider::chat_stream` reads `/api/chat` with `"stream": true` and hands each piece
to a callback as it arrives, so the window can draw text while a large model is still generating.
The HTTP client grew a chunked-body reader, because that is how Ollama frames a stream; NDJSON
lines are parsed one at a time (`parse_stream_line`), and the final message is assembled from the
pieces the same way the non-streaming call builds it. A piece that carries only metadata (a bare
final `done`) is not a chunk. The tests drive an in-process fake server that emits the body in
several `Transfer-Encoding: chunked` writes, which is what exposed the need for the incremental
reader: the pieces arrive split (`"Hel"`, `"lo "`, `"there"`) and still come out as `"Hello there"`.

**The chat.** *Configuration ▸ Agent chat* (`cm_AiChat`) opens a conversation pane: transcript,
input line, the model in use, *New conversation* and *Stop*. The transcript **is** the request
history, so a follow-up question sees what was said before. The answer streams into the last line;
*Stop* keeps what arrived and marks it; a failure is written into the transcript and the status
line instead of vanishing. The call runs on a worker thread and the model polls a channel, so a
model that takes half a minute to load cannot freeze the UI, and the model layer stays free of
egui (the TUI frontend can use the same session).

The boundary is deliberate: the chat **cannot touch files**. Every action with side effects still
belongs to the policy gate, so a clever sentence in the transcript can never become a write
without an approval and an audit entry. `Docs/Agent chat/{en,cs}.md` says so plainly, together
with the honest limits (no tool calling in this window yet, plain-text answers, transcript only in
memory).

## Phase 16.0 / MCP — Git tools for the agent (done)

`cargo test -p rc-agent --test server_protocol` → **44 tests**, `cargo test -p rcommander --lib
git_host_tests` → **4 tests**, and the release binary was driven over stdio to confirm it.

`rc-agent` grew a `GitHost` capability (like the existing `LanguageHost`): eight tools —
`git_status`, `git_log`, `git_branches` (read-only) and `git_stage`, `git_commit`,
`git_checkout`, `git_pull`, `git_push` (destructive, confirmation required) — published only when
a front-end registers the capability. `rcommander` implements it on the shared model, and **every
mutating call goes through `Model::evaluate`**, so the agent gets the same three answers a human
would: allowed, refused with a reason, or "the user must approve this first".

Measured behaviour of the release binary (`rcommander mcp`) against a real temporary repository:

* `git_status` → `1 changed file(s) … M Modified a.txt`
* `git_branches` → `(current: master)`
* `git_log` → `ad8390a6  first  (Test)`
* `git_commit` → **refused**: "confirmation required: the destructive tool `git_commit` needs
  approval, but no approval handler is registered with this MCP server, so nothing was changed"

That last line is the point: the headless `mcp` subcommand has no approval handler, so destructive
tools fail closed instead of running unattended. A front-end that wants them (or a future
`--allow` mode) has to install a confirmer explicitly.

## Phase 16.0 — policy and safe-change foundation (engine done)

`cargo test -p rc-core --lib policy` → **12 tests**, `cargo test -p rc-core --lib transaction`
→ **7 tests**.

`rc_core::policy` is the capability decision shared by everything that can have side effects. A
request names an operation, a canonical resource, an optional destination and its origin; the
policy answers **allowed**, **denied with a reason** or **"ask the user"**.

* **Profiles** — `sandbox` (only granted roots), `user` (read anywhere the user can read, writes
  need grants) and `privileged` (everything, every privileged operation approval-gated).
* **Grants** bind operations to canonical roots with an optional expiry and a revocable id, so a
  grant can be named in the audit log and killed instantly.
* **Deny wins**, expired or revoked grants fail closed, symlinks and `..` are resolved before the
  comparison (a link out of the sandbox is refused), and the profile caps what an approval could
  unlock — a `sandbox` policy never offers to elevate.
* **Approval-worthy operations** (`execute`, `publish`, `privileged`) return `NeedsApproval`,
  never a silent allow, whatever the grants say.

`rc_core::transaction` implements the panel owner's hard rule: **no verified backup, no change.**
Prepare hashes the original, copies it into the transaction directory and **verifies the copy**
(length and SHA-256) before anything else happens; the caller's validator runs before any write;
apply writes a temporary file and renames it into place (atomic), then re-hashes the result; a
failure at any step restores every file already written and removes the ones the transaction
created. The manifest (original hash, new hash, backup path, created flag) is written as TOML, so
"restore from backup" is a real feature and a tampered backup is reported instead of pretending a
restore happened.

**16.0b — approval and audit.** `rc_core::audit` writes an append-only trail where every line
carries the hash of the previous one, so `verify()` reports the exact line where a log was edited
or shortened; `redact()` is the single place that decides how a secret looks, and an event can
never break the one-line rule (tabs and newlines are neutralised). `Policy::from_settings` builds
the policy from `[agent]` (profile, roots, deny list, approval list, with safe fallbacks so an
empty list cannot mean "no approvals").

The model wires it together: `Model::evaluate` checks the policy, writes the decision, and either
runs the request or refuses it; `Model::ask_approval` opens the dialog for approval-worthy work;
`approve_pending`/`refuse_pending` record the answer and can create a session grant;
`revoke_agent_grants()` is the stop button; `audit_state()` reports the chain. The **approval
dialog** names the origin, the operation, the canonical target, the destination, the agent's
reason and whether a verified backup makes it reversible — and says plainly when it does not
(a restart, a push, an elevated command). The human path is not policy-gated: the policy limits
the *agent's* reach, and `origin` keeps that visible in the log.

Writing this exposed a real gap: a write was only checked at its **resource**, so copying a file
*out of* the sandbox to an outside destination passed. A local destination is now a second
resource and must be covered by the same grant.

## Phase 14.6 — the window chrome follows the theme (done)

`cargo test -p rcommander --lib theme` → **12 tests**, plus a UI test.

Eleven roles were applied to the panels but the *widget styles* were left at egui's
defaults, so buttons, checkboxes, sliders and window title bars (the row with the ×) kept
a light grey background in every theme — unreadable in the dark, the AS/400 and the
Midnight Commander palettes.

`theme::visuals` now derives all four widget states from the palette: `inactive` and
`noninteractive` use `header_background` (so a button and a title bar are the same
material), `hovered` mixes that with `accent`, `active` mixes it further, text uses
`foreground`, outlines `panel_border`, and `widgets.open` — which egui paints window and
menu title bars with — is themed as well. A unit test asserts each field for the light and
the dark palette, and a UI test switches to a custom theme and checks the app palette and
the derived button/title colours.

## Phase 15.5 — cloning into a panel (done)

`cargo test -p rc-git` → **35 tests**, `cargo test -p rcommander --lib clone_tests` → **3
tests**.

`rc_git::clone_into` clones with the approved credentials and refuses an existing non-empty
target; a failed clone removes the directory it created, so nothing that looks like a
repository is left behind. *Configuration �� Clone repository…* (`cm_GitClone`) asks for the
URL, the target directory and a folder name suggested from the URL, runs the clone on a
worker thread and switches the panel to the new repository when it finishes.

## Phase 15.8d — staging and committing the selection (done)

`cargo test -p rc-git` → **33 tests**, `cargo test -p rcommander --lib stage_tests` → **3
tests**.

The engine can stage and unstage a list of paths (`stage_paths`, `unstage_paths`,
`unstage_path` via a default reset to `HEAD`), and a path that neither exists nor is tracked
is reported instead of being silently staged as a deletion.

In a repository panel the selection is the unit: *Stage selection* and *Unstage selection*
refresh the panel so the colours follow, and *Commit selection…* opens the commit dialog for
exactly those paths — the change is described as one file or `N files`, nothing else in the
tree is staged, and the message/push options are the same as for the editor flow.

Adding this exposed a colour bug: `git_bridge::status_role` decided by the `FileStatus`
alone, so a *staged* modification was painted with the warning colour. It now decides by the
side — worktree changes warn, staged changes are green, and only unstaged content wins when
both exist.

## Phase 15.3 — copying out of a revision (done)

`cargo test -p rcommander --lib extract_tests` → **2 tests**, `copy_out_tests` → **4
tests**.

`git_bridge::extract` reads a file — or a whole directory tree, recursively — out of any
revision into a staging directory, with a per-file (128 MB) and per-entry (1 GB) limit and
a depth cap. `Model::copy_from_git` then copies the result into the other panel through
the ordinary copy path, so the rename-on-collision policy and the reporting stay the same:
a free name is chosen first (`name (2)`, …), which also makes a file-versus-directory
collision safe.

Nothing is checked out and the working tree is never touched, so this is the safe way to
pull "that file from three commits ago" into the other panel.

## Phase 15.8c — fetch, pull, push and merge (done)

`cargo test -p rc-git` → **32 tests**, `cargo test -p rcommander --lib remote_tests` → **3
tests**, plus a UI test.

The engine grew `fetch` (with the credentials callback shared with push), `ahead_behind`,
`pull_fast_forward` and `merge_branch`. Both "pull" operations are deliberately
**fast-forward only**: a diverged branch, a dirty tree or a branch that is already ahead
produce a message instead of a half-merged working tree, which is the only safe default
for a file manager.

*Configuration ▸ Git remote…* shows the branch, its remote and the ahead/behind counts,
and offers **Fetch**, **Pull (fast-forward)**, **Push** and **Merge branch**. The three
network operations run on a worker thread with the credentials the user approved
(`git_push_job` became a general `GitNetJob` that reports a message), so a slow remote
never freezes the window; a pull reloads the panel afterwards.

The step also fixed a real bug the new tests exposed: a duplicated `git.pushed` key made
the TOML pack invalid, so the whole Czech language silently disappeared. `tests/i18n_keys.rs`
now asserts that no pack repeats a key.

## Phase 15.8b — switching branches from the panel (done)

`cargo test -p rcommander --lib branch_tests` → **5 tests**, plus a UI test.

*Configuration ▸ Switch branch…* (`cm_GitBranches`) opens a picker for the repository in
the active panel: every local branch with the current one marked, *Switch*, *Create and
switch*, and a *Switch anyway* checkbox that only appears when the tree is dirty. A switch
reloads the panel so the files and the branch label follow, creating a branch works the
same way, and both panels can sit on different branches.

The dirty tree is refused unless the checkbox is ticked, because losing an edit is worse
than being asked; the checkbox then uses libgit2's `safe` checkout, which keeps local
changes and reports any file it could not update.

## Phase 15.8a — edit, commit, push (done)

`cargo test -p rc-git` → **28 tests**, `cargo test -p rcommander --lib save_tests` → **7
tests**, plus a UI test that drives the whole flow.

The engine can now stage (`stage_path`, `stage_all`), commit the index (`commit_staged`,
with the author coming from the machine's Git configuration when the user allowed it) and
push a branch (`push_request` names the remote and refspec; `push` takes credentials for
one call only and falls back to the ssh agent, then to the credential helper).

The editor hooks into it: saving a file that lives in a repository calls
`Model::on_file_saved`, which — according to `[git] commit_on_save` (`ask` by default) —
either asks first, commits immediately, or does nothing. The dialog shows the file and an
editable message (template `[git] commit_message`, default `Edit {name}`), a *Push after
committing* checkbox, and both *always* and *never* can be remembered. The push runs on a
worker thread polled by the frame, so a slow remote never freezes the window.

## Phase 15.2b — Git repositories as panels (done)

`cargo test -p rcommander --lib git_bridge` → **6 tests**, plus 2 UI tests.

`Location` grew a `Git { root, rev, path }` variant: `rc_core::location` knows how to
display it (`/path/to/repo [branch:main]/src`), how to walk up and down, and how to name
it; `Location::Git` never claims to be a local path, so every existing `match` keeps
working.

`rcommander::git_bridge` turns a Git location into panel entries. The repository root
shows the working tree **plus** the virtual folders `branches`, `tags`, `commits` and
`remotes`; `branches/` lists the branches, and entering one switches the panel to that
revision's tree (`branches/main` is translated into a revision location instead of being
treated as a directory). `commits/` lists `<short id> <summary>` for the last 500 commits.

*Model ▸ Open as Git repository* (`cm_GitPanel`) converts the active panel into the
repository that contains its directory, and reports the branch in the status line and in
the path bar (`/repo  [main]`).

**Status colours** (15.6): the panel keeps a per-entry status map, and rows are tinted by
the theme role that status maps to — changed files use the warning colour, staged ones the
archive green, untracked ones the link colour and ignored ones the muted colour. Ignored
entries are asked per path (`status_should_ignore`), because libgit2's bulk walk does not
reliably visit them.

## Phase 15.2a — status and branch switching in the engine (done)

`cargo test -p rc-git` → **24 tests**.

`GitRepo::status()` reads the whole working tree with libgit2 (index *and* worktree
side, renames included) and returns a `WorkTreeStatus`: per path a `StatusEntry` with
an optional staged and an optional unstaged `FileStatus` (modified, added, deleted,
renamed, untracked, ignored, conflicted, type changed), `XY` codes in the style of
`git status --short`, a staged/modified count and a one-line summary. Ignored paths are
answered per path with `is_ignored()`, because libgit2's bulk walk only visits the
ignored files it happens to see — and the panel asks about exactly the names it draws.
`status_of()` folds a directory's children into the directory itself, so a folder with
modified content is not shown as clean.

`checkout_branch(name, allow_dirty)` and `create_branch(name)` are the first write
operations. Both refuse to touch a working tree with changes unless the caller insists
(`allow_dirty`), the forced path uses libgit2's `safe` checkout so a local edit is never
lost, and a clean tree is replaced outright so files of the other branch disappear
instead of lingering as untracked leftovers. The index is reset to the new `HEAD`
afterwards, so the switch itself never shows up as a pile of staged changes.

## Phase 15.1 — Git engine (done)

`cargo test -p rc-git` → **12 tests**, all offline, all against repositories the suite
builds itself.

`crates/rc-git` opens a repository (discovering upwards like git does, or exactly for a
`.git` directory and bare repositories) and exposes the read-only view the panels need:
`branches()`, `tags()`, `remote_branches()`, `all_refs()`, `commits(limit)`,
`head_branch()`, `head_id()`, `resolve()`, and per revision `list()`, `read_file()`,
`file_size()`, `is_dir()` and `names()`. A revision is a small `Revision` enum
(working tree, HEAD, branch, tag, commit, remote branch) with a round-tripping panel
spelling.

Safety belongs to the engine, not to its callers: paths inside a revision are
normalised and any `..` is refused, `read_file` takes a byte limit, a missing path or a
file where a directory is expected produces a message instead of an empty list, and a
bare repository explains that it has no working tree instead of silently listing
`.git`.

Backend: **`git2` with `vendored-libgit2`**, so the feature does not require a `git`
binary — which matters on Windows and is the only option on Android. The comparison
with gitoxide and with shelling out, and what would reverse the decision, is in
[`analysis/git-backend.md`](analysis/git-backend.md).

`crates/rc-git/src/fixture.rs` builds the repositories the tests use: two commits, a
branch and a tag that stay at the first commit, an ignored file, a nested directory and
a bare `file://` remote pushed from the fixture. `initial_head` is pinned to `main` so
the suite does not depend on the machine's `init.defaultBranch`.

## Phase 14 — colour themes (done)

`cargo test -p rc-core --lib theme` → **12 tests**, `cargo test -p rcommander --lib themes`
→ **5 tests**, plus 2 UI tests.

`rc_core::theme` makes colours data. A theme file has `[meta]` (name, base, author,
version, description), a `[colors]` table with one hex value per semantic role and an
optional `[file_types]` table of mask → colour rules. Roles are inherited from the
theme named in `base`, so a theme lists only what it changes and can never end up with
accidental black-on-black; every role ends up present in the resolved palette
(`Resolved::is_complete`). Unknown role names, unparsable colours, an empty mask and a
colour used as a base are all rejected with a message that names the role and lists
the known ones.

`ThemeSet::load_dir` discovers `*.toml` exactly like `locales/` and `Docs/`, sorted by
file name so a duplicate name resolves the same way on every platform (first file wins,
the duplicate is reported), keeps the built-in light/dark palettes available so a
missing directory is not fatal, follows one level of inheritance through another theme
file, and lets a file of the same name refine a built-in without recursing.

Eight themes ship in `themes/`: **Classic Light**, **Classic Dark**, **High Contrast**,
**AS/400 Green Screen**, **Midnight Commander**, **Windows Explorer**, **Solarized
Dark** and **Amber Terminal**. A test copies the shipped directory and asserts every file parses, that every resolved
palette is complete and that the six colour pairs which decide whether the window can be
used at all stay above a 3:1 contrast ratio — which is how the Solarized theme's
`hidden` role was caught at 2.79:1 and brightened.

Tool-bar icons are tinted by the *kind* of action rather than one accent colour:
destructive commands use `warning`, creating ones `archive`, moving ones `directory`,
navigation `accent`, searching and network commands `symlink`, quiet ones `hidden`. A
test asserts the grouping and that at least four distinct roles are in use, so the bar
cannot silently collapse back to one colour.

*Configuration ▸ Options ▸ Colors* is a real editor: a theme chooser, a role grid with
a colour picker per role, a live preview of a panel (cursor row, directory, hidden and
read-only files), contrast warnings with the measured ratio, and *Save* / *Delete* /
*Reload*. The actions sit in the first row, above the scrolling parts, so they stay
reachable however long the list gets. `Palette::for_model` is now the single place where
theme data becomes egui colours, and `cm_ThemeToggle`/`cm_ThemeNext`/`cm_ThemePrevious`
cycle through whatever themes are installed. The *Themes* help topic documents the
format, the roles and the evaluation order in both languages.

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

`cargo test -p rcommander --lib` → **249 tests, 0 failures.** Navigation, cursor
keeping, selection commands, view/sort commands, dialog validation for every
dialog, command dispatch over the whole implemented `cm_*` set, transfers,
deletes, mkdir/rename/attributes/comment/multi-rename against real temp
directories, command line (`cd`, `cm_*`), quick search, archive entry, keyboard
dispatch, RTL direction handling.

## Phase 10 — egui UI (partial)

`cargo test -p rcommander --test ui_smoke` → **45 tests, 0 failures** (headless
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

- The latest **reported** `cargo test --workspace` run is **1,273 tests,
  0 failures**; this documentation edit did **not** rerun tests. Previously
  listed per-crate figures were from inconsistent snapshots and claimed a
  conflicting **1,229 total**. Do not infer a corrected sum from them.
  **Next verification:** run the full workspace suite and capture unit,
  integration and doctest counts from the **same run**, with optional live SFTP
  tests reported separately, before publishing a new breakdown.
- `cargo clippy --workspace --all-targets`: **0 warnings** on a fresh build of
  every crate.
- `cargo fmt --all --check`: clean.
- Note on the environment: the working copy on this machine lives on a mount that
  developed I/O errors (unreadable objects in `.git/objects` and in
  `target/debug/incremental`, which makes `rustc` die with SIGBUS). The numbers above
  were produced from a fresh clone with `CARGO_TARGET_DIR=/tmp/rc-target`, and the work was
  committed and pushed from that clone. `paths::data_roots()` now also falls back to the
  compile-time crate directory, so the tests find `locales/` and `Docs/` from any target
  directory.
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

## Current gaps and baseline reconciliation (2026-11-27)

Earlier detailed sections record historical evidence. This list supersedes
their stale next-step notes; source presence alone is not end-to-end proof.

| Remaining gap / verification task | Plan reference |
| --- | --- |
| Missing-secret SFTP prompt, FTP queue/cancel/progress, Git credential/live checks | 7, 13, 15 |
| Inline tree, search, thumbnails and context menu target/native actions | 10/10.10 |
| Same-revision full tests, Clippy/format, actual Android APK | 11/12 |
| Surface F-keys and modifier row implemented; complete menu parity/cheat sheet | 14.9; 211be5f, eddb0d8 |
| Advanced theme effects and performance | 14.8 |
| Policy engine exists; ensure every tool/plugin/runtime uses it; zones and public egress | 16.0/16.11/16.35–37, 37.1 |
| Chat/providers/reviewed copies implemented; model-originated MCP calls, command checklist, remaining pane/translation/developer workflows | 16.1–15, 16.31–32 |
| Prompt catalogue/editor/pins implemented; edit diff, workload contracts/runtime/team coordination | 16.16–21, 16.27–30, 16.33 |
| Markdown export/import implemented (622b949); durable SQLite sessions/lifecycle/restore planned (cae8d68/5f8388c docs only) | 16.34, 25, 32.2 |
| Shared highlighter/Markdown recorded; annotations/script review, PDF/Office, binary ranges remaining | 17.0–17.6, 18.6 |
| Universal location, manifest command/MCP sources and manager implemented; remaining row verbs, renderer, executable isolation, staged installer, agent catalogue/calls | 18.2a/b, 18.3–5, 18.7, 27 |
| Craft model integration not yet demonstrated; configured MCP source is not model tool calling | 16.32, 18.5, 27.5 |
| MCP HTTP server flag historically unwired; verify/remove or implement securely, separately from HTTP client | 8, stage B |
| SSH/PowerShell, WSL, privileged helper, host diagnostics, VM/visual lab | 19–22, 28, 33 |
| Directory, backup, network, database, developer and browser workbenches | 23, 29–30, 34–36 |
| Research, private knowledge, workflows, external runtime, creative and Commons | 24–27, 31–32, 37, 16.22–30 |

**Verified Windows baseline:** [build #6](https://github.com/afrowaveltd/RCommander/actions/runs/45960374534)
on [67e8570](https://github.com/afrowaveltd/RCommander/commit/67e8570ca2bca4dd51fac33cbc8ed7d90347a4b1)
completed successfully: targeted drive/chat/plugin/prompt/LLM regression
tests, MSVC release, packaging, artifact upload and prerelease.
[Windows test release #6](https://github.com/afrowaveltd/RCommander/releases/tag/windows-test-6)
contains RCommander-Windows-x64.zip (8,762,178 bytes). Build #5 failed the
duplicate-ID test on case-insensitive Windows; #6 uses distinct parents
and the same duplicate validation path. No fresh Linux, actual desktop
or Android verification is inferred.

## Phases 25–28 — planned extensions (2026-11-27)

No implementation or test claims yet. Phase 25 consolidates workspace restore
and existing session/checkpoint plans; Phase 26 uses the documented upstream
DeepSeek Harness SDK (revision 4b399c3, source links and limitations in the
plan), with internal and external runtime interfaces kept separate.
Phase 27 extends existing plugin contracts and catalogue rather than
duplicating them; Phase 28 reuses scoped remote management and adds a
virtualization category plus guest/host isolation. Runtime, USB and
hypervisor behavior require real integration tests before claiming support.

## Runtime lifecycle and resource custodians — planned (2026-11-27)

Source inspection at 5ec11f3 (not a new test run): rc-core has no GUI
dependency; rcommander exposes headless Model and stdio MCP builds that
model without a window. However RCommanderApp owns the model, its UI tick
polls chat, and current chat/transfer workers use in-memory channels and
thread handles. The application crate still depends on egui/eframe.
Independent persistent runtime and TUI are not implemented. See
[core manifest](crates/rc-core/Cargo.toml),
[application manifest](crates/rcommander/Cargo.toml),
[entry point](crates/rcommander/src/main.rs),
[GUI tick](crates/rcommander/src/ui/app.rs),
[chat jobs](crates/rcommander/src/model/chat.rs) and
[transfer runner](crates/rcommander/src/ops_runner.rs).

New 16.0f–h: authenticated caller/role/scope, typed denial and permitted
resolution, resource custodians and correlated delegation, stop/revoke.
New 25.5–10: headless runtime and authenticated client boundary, SQLite
durable task/step/attempt ledger and meeting rooms, safe-point draining,
effect intent/receipt/reconciliation, optional background/tray and restart,
integrated acceptance. Define early in B, deliver in D before external
runtimes/teams. Normal hibernation requires settled verified units;
abrupt interruption is recovery-required, never a fictitious Done.
Owner requires immediate durable commit of every operational transition,
receipt and completed unit (25.6–8), not periodic batched checkpoints.
SQLite WAL/FULL or verified equivalent, artifact flush and commit-before-
dependent-dispatch, crash-boundary and failed-commit tests are specified;
physical power-loss durability depends on supported storage flush semantics.
SMTP acceptance and local receipt cannot be assumed atomic; queryable
external evidence/idempotency or explicit blocked recovery is required.
Owner also requires visible, responsive shutdown/hibernate progress
(25.7/25.9/25.10): real runtime lifecycle events, current unit and wait
reason, durable-save phase, approximately one-second confirmed completion
notice, then close. Failure/stall and explicit forced-stop paths remain
distinct from successful shutdown; GUI, TUI and CLI share event semantics.
No source changes or new runtime tests in this documentation update.

## Phases 29–36 — planned workbench extensions (2026-11-27)

All are planned, not implemented or tested. They extend existing engines
rather than duplicate them: network/backup diagnostics (21), browser and
toolchain acceptance (26), local retrieval (17/25), reusable workflows
(16/25), graphical sessions (19/28), directory categories and typed
provisioning templates (16/18/27). Read-only slices and fixtures first;
actual OS, server and tool compatibility must be verified. The owner's
Standard User / James Cooper example is a template test, not an actual
directory mutation or a grant of privileges.

## Phase 10.11 — practical UI feedback (2026-11-27)

Shared action catalogue, top-level Agent/Git menus, localized default toolbar
captions, personal layout save/reset/restore, local directory/function shortcuts
and right-click edit/remove submitted along with Linux labelled drive filtering
and lightweight inline numeric powers. Regression fixtures and Windows/Linux CI added.
Verification pending; do not mark done until
CI and real platform smoke tests. Unlabelled Linux mounts remain accessible
through ordinary path navigation; full TeX is not claimed.

## Handoff — wallet hardening and baseline documentation (2026-11-27)

- [Windows #7](https://github.com/afrowaveltd/RCommander/actions/runs/45974063106)
  succeeded at `351baeaa`; [Linux UI #1](https://github.com/afrowaveltd/RCommander/actions/runs/45974295709)
  succeeded at `482618a`. These are targeted suites, not full current verification.
- Commit [75889f7](https://github.com/afrowaveltd/RCommander/commit/75889f76045858e14d65aed5c3ad957de9f92a16)
  redacts MemoryStore Debug and invalid TOML diagnostics, and checks every
  Windows regression exit code. Windows [#8](https://github.com/afrowaveltd/RCommander/actions/runs/45986044750)
  was in progress when this handoff was written. It does not include subsequent changes.
- 13.4a **implemented in source, verification pending**: encrypted vault save now
  uses a same-directory private temporary file, writes and syncs ciphertext,
  then replaces the destination. Failed encryption or publication keeps the
  dirty state; regressions cover preserving the previous vault and cleaning up
  a failed replacement. The envelope is unchanged. Windows uses inherited ACLs;
  no directory fsync/power-loss guarantee or full memory zeroization is claimed.
  No Rust toolchain available in the editing environment; run
  `cargo test --locked -p rc-core --lib secrets::tests`, `cargo fmt --all --check`
  and the platform CI on the new revision. Do not mark 13.4a done until verified.
  Dedicated [Wallet regressions](.github/workflows/wallet-regressions.yml)
  runs the rc-core secret tests on Linux and Windows without building the GUI;
  inspect the exact workflow revision and record results after completion.
- README updated against source, documented features and historical CI evidence.
- 5.5–5.6 [issue #1](https://github.com/afrowaveltd/RCommander/issues/1) and
  16.0i [issue #2](https://github.com/afrowaveltd/RCommander/issues/2) now have
  stable plan references. They remain pending; do not infer implementation from
  the issue specifications. Reuse existing policy/secret abstractions.
- Next: verify 13.4a; continue stage A missing-secret SFTP prompt and UI checks,
  then stage B typed authorization/custodian contracts. Independently ready
  archive capability work can proceed without blocking normal file management.

## Next steps (dependency-ordered)

The [authoritative delivery roadmap](ImplementationPlan.md#dependency-ordered-delivery-roadmap-authoritative-2026-11-27)
is the single scheduling source. Work on the first unverified gate and
independent ready branches, not a numeric phase queue. Record exact
commands, revision and platform evidence after each accepted slice.

1. A: baseline and core usability; recheck current tests.
2. B: common authorization, typed resolution/custodian contracts, model-to-tool broker, early headless runtime/ledger boundary, transport.
3. C: early read-only research/GitHub/Learn/search and safe downloads.
4. D: SessionStore, durable native runtime/to-do ledger, safe-unit hibernation, verified effect recovery, background clients and workflow checkpoints.
5. E: reference plugins, Discovery and visual foundations.
6. F: external runtimes, roles, developer loop and workflow.
7. G/H: remote/lab and reviewed AutoPlugin, API/browser acceptance.
8. I/J: independent services and community/creative workflows.
9. K: ongoing polish and explicitly scoped release.

Phase 37 and the new 16.0f–h/25.5–10 steps are planned, not implemented.
Research deliberately precedes advanced Workshop, not follows Phase 36. This review is documentation-only;
no fresh tests were executed and no unsupported feature marked done.
Historical detailed evidence remains above.
