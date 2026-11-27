# Handoff to the RCommander development agent

Owner asked ChatGPT to help with immediate UI fixes while you continue the roadmap.
Base: 5645fb8; first volume/math change 08bde35. Focused Phase 10.11, not an alternative runtime.
- Shared model/actions.rs catalogue drives menu and function shortcut picker.
- Agent and Git are top-level; duplicate search is no longer calculate size.
- Factory captions translated at rendering; keep user captions. Both bundled/runtime EN/CS updated.
- Toolbar + opens function editor or pins local directory. Existing editor handles order, right-click edit/remove, saved custom layout and factory reset.
- Linux shortlists omit slash labels and pseudo/system mounts, full volumes() stays intact. Unlabelled mounts remain accessible through path navigation. Android/Windows not subject to new slash filter.
- Markdown recognizes a small numeric-power subset; no full TeX claim.
- New regression tests and Windows workflow; no local Rust compiler available. Inspect CI, fix errors, test visually Linux/Windows and update status before marking done.
- Do not reset user toolbar on startup; preserve settings and roadmap B/D for runtime.
- Follow-up: arbitrary custom icon artwork, legacy external command/parameter contract, richer math, complete command/menu audit.
