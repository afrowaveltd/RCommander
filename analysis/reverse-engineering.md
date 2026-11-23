# Reverse-engineering record

This document records **how** the behaviour of the reference product was
reconstructed, **what** evidence was collected, and **where the clean-room
boundary** was drawn. It exists so that every feature in RCommander can be traced
back to a documented observation instead of to guesswork.

The reference artifact was a shipped Windows distribution of a commercial
dual-pane file manager (a 32/64-bit combined installer, `PE32`, InstallShield
self-extracting, ~11 MB). Nothing from that distribution — no binary, no resource,
no extracted file, no decompiled code — is part of this repository. The extracted
material lived in a git-ignored `work/` directory and was used only as evidence.

---

## 1. Tooling

| Purpose | Tool | Notes |
| --- | --- | --- |
| Container unpacking | `7z` (25.01) | the appended ZIP payload and the inner `INSTALL.CAB` both extract with stock 7-Zip |
| Byte-level inspection | custom Python (stdlib) + `pefile` | PE resource walking, magic scans, carving |
| Static decompilation | **Ghidra 12.2** (headless, JDK 25) | full auto-analysis of the 64-bit image, then scripted inventory and focused decompilation |
| Documentation mining | Python `html.parser` | the shipped HTML help (CHM) was converted to text for specification writing |
| Data-file parsing | Python | command catalogue (`*.INC`), menu definitions (`*.MNU`), string tables (`RT_STRING`), Delphi form streams (`RT_RCDATA`) |

Ghidra ran as:

```bash
analyzeHeadless ghidra_proj rcommander \
  -import TOTALCMD64.EXE \
  -scriptPath re/ghidra_scripts \
  -postScript DumpInteresting.java <outdir>
```

with a custom script that (a) dumps program metadata, imports and sections,
(b) counts functions, (c) resolves every defined string that contains one of ~40
subsystem keywords (`lister`, `hex`, `thumbnail`, `wildcard`, `sync`, `ftp`,
`packer`, `sort`, `rename`, …) to the function that references it, and
(d) decompiles those functions (address, name, size, referencing strings, C body).

---

## 2. What was extracted (behavioural evidence)

| Evidence | Size | Used for |
| --- | --- | --- |
| Internal command catalogue (name, numeric id, functional section) | 594 commands in 24 sections | the command model of RCommander (`rc-core::command`, all commands preserved as data) |
| Menu definition (English) | 100 menu items in 8 menus with accelerators | menu structure, action grouping, default shortcut assignments |
| Keyboard reference (from the shipped help) | ~130 key combinations | `rc-core::keymap` defaults (`KeyMap::defaults`) |
| Help topics | 213 HTML pages | behavioural specifications 01–04 in this directory |
| Dialog resources | 7 dialog templates + 6 Delphi forms | dialog inventory, field sets, validation rules |
| String tables | 1 070 entries | cross-checking feature names and error semantics |
| Configuration reference | 740 documented option keys | `rc-core::config` model and the feature coverage checklist |
| Decompiled functions | see `work/re/out/ghidra/` (not committed) | confirming algorithm semantics where the documentation is silent |

## 3. Findings that shaped the implementation

- **Command surface.** Menus, button bars, keyboard shortcuts and the command
  line all address the same command identifiers. RCommander keeps that invariant:
  `Model::execute` takes the same identifiers, and the keyboard map, the UI and
  the MCP tools are all thin adapters over it.
- **Panel model.** Two panels, each with its own directory, marks, sort order,
  filter and history; the inactive panel is the default operation target. Marks
  survive a re-read of the directory.
- **Attribute presentation.** Directories and links are marked before attribute
  letters; unix modes are rendered in the classic three-triplet form.
- **Lister behaviour.** The previewer chooses the representation from content, not
  from the extension, and falls back to a hex dump for unknown binaries; text is
  decoded with encoding detection and handled right-to-left when the script
  demands it.
- **Archive transparency.** Archives are entered like directories, paths inside
  them are virtual locations, and extraction must refuse paths that escape the
  destination.
- **Transfer model.** Copy/move dialogs carry their own options (preserve times,
  verify, overwrite policy, background execution) rather than global modes.
- **No undo.** The reference documentation offers no undo for copy/move/delete;
  RCommander likewise confirms destructive operations instead of pretending to be
  able to revert them.

## 4. Clean-room boundary

**Extracted as facts** (allowed, kept):

- identifier names and numbers of commands (`cm_*`, numeric ids),
- option names and their documented meanings,
- keyboard bindings and their actions,
- dialog field sets and validation behaviour,
- observable algorithms and their edge cases.

**Not copied** (deliberately re-implemented or excluded):

- any source code or decompiled C code — the decompiler output was read to
  understand semantics, then the behaviour was re-implemented from the
  specification in original Rust,
- icons, bitmaps, cursors, audio and other binary resources,
- UI strings, help text and translation files — all user-visible text in
  RCommander was written for this project, in English and Czech,
- the product name and any trademark of the reference product.

## 5. Reproducibility

The reconstruction is reproducible in broad strokes from public information (the
reference application publishes its command catalogue and help content), and
exactly from the shipped distribution with the tooling listed above. Command
sequences, script sources and intermediate outputs are kept outside version
control to keep this repository free of third-party artifacts.

---

## 6. Corroborated configuration semantics

The Ghidra string cross-reference gave an independent confirmation of the
configuration model that [`spec-03-config-settings.md`](spec-03-config-settings.md)
documents from the help text. The functions below each reference a cluster of
option names; the clusters map one-to-one onto the settings groups of
`rc-core::config`.

| Function (64-bit image) | Referenced option names (excerpt) | Model implemented from it |
| --- | --- | --- |
| `FUN_00495050` | `VerifyCopy`, `CopyAllTimes`, `CopyLinks`, `CopyBlockSize`, `CopyHugeBlockSize`, `AlwaysCopyInBackground`, `CopyOverwriteDefault`, `CopyStayOnFile`, `SkippedFileCopy` | `CopySettings`, `TransferOptions`, overwrite policy |
| `FUN_00495050` | `HistoryLen`, `HistoryThinning`, `HistoryThinningDelta`, `ReverseHistory`, `HistoryAppend`, `OldStyleHistory` | `DirHistory` capacity, thinning, MRU order |
| `FUN_00495050` | `LockedTabChange`, `DirTabLimit`, `DirTabRevert`, `DirTabFilters`, `TabSwitchMode`, `TabEnterDirChange` | `tabs::PanelTab` (locked, per-tab filter) and `TabBar` lifetime rules |
| `FUN_00495050` | `SortDirsByName`, `SortUpper`, `SortDirExt`, `customsortorder`, `AddSortOrders` | `SortSpec` (dirs first, case handling, natural order, extra sort keys) |
| `FUN_00495050` | `PreloadThumbnails`, `ThumbnailMemoryLimit`, `ThumbnailSizeLimit` | viewer thumbnail pixel/size budget |
| `FUN_00495050` | `HebrewListerFix` | the lister's RTL behaviour |
| `FUN_00499a00` | `sortorder`, `negative Sortorder`, `AddSortOrders`, `lefttabs`, `righttabs`, `activetab` | reversed sorting and per-panel persisted tab state |
| `FUN_0049afe0` | `SaveHistory`, `QuickSearchHistoryAutoFilter`, `activetab` | history persistence and the quick-search filter model |

These are *observations* about which options the reference implementation reads,
not code that was copied: each option name is a documented configuration key, and
the behaviour behind it was re-implemented in Rust from the behavioural
specification.

## 7. Scale of the analysed artifact

| Measure | Value |
| --- | --- |
| Analysed image | 64-bit PE, image base `0x400000` |
| Functions after auto-analysis | 11 336 |
| Defined string data | 14 376 |
| Functions referencing a subsystem keyword | 420 |
| Functions decompiled for review | 220 |
| Imported Win32 API symbols | 100+ (advapi32, kernel32, user32, ws2_32 …) |
