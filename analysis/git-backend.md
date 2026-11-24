# Git backend: decision and trade-offs

Step 15.1 of [`ImplementationPlan.md`](../ImplementationPlan.md) asks for an
evaluated choice before any Git code is written. This note records the decision, the
reasons and what would change it.

## The three candidates

| | `git2` (libgit2 bindings) | `gix` (gitoxide, pure Rust) | running the `git` binary |
| --- | --- | --- | --- |
| Needs a `git` binary on the user's machine | no | no | **yes** |
| Language of the dependency | C (vendored), Rust bindings | pure Rust | none |
| Works on Android | yes (libgit2 builds for ARM; the NDK provides the compiler) | yes | no practical way |
| Build cost | compiles libgit2 once (~15 s here, cached afterwards) | large dependency tree, long first build | none |
| API stability | stable, mature, 1.x libgit2 | pre-1.0, API still moves | frozen but text-based |
| Effort for "read a tree from a revision" | small: `revparse` → `commit.tree()` → iterate | medium: plumbing crates must be assembled | medium: parse `ls-tree -l` output, quote rules, `-z` handling |
| Effort for future write operations (15.8) | small: index, commit, merge, push helpers exist | growing support | medium: run commands and parse their output |
| Risk of shelling out | — | — | output formats vary between git versions; locale and quoting edge cases; a missing binary breaks the feature at runtime |
| Cross-compilation | C compiler needed per target (already required by `rc-archive`'s zlib) | trivial | n/a |

## Decision

**`git2` with `vendored-libgit2`.**

Reasons, in the order that decided it:

1. **No runtime dependency.** A file manager cannot assume the user has Git
   installed — and on Android there is no `git` binary at all. Shipping libgit2
   inside the application makes the feature behave the same everywhere.
2. **The read path is short and obvious.** `revparse_single` → `peel_to_commit` →
   `tree()` → `get_path()` → `as_blob().content()` is exactly the model the virtual
   folders need, with no plumbing assembly and no output parsing.
3. **15.8 becomes feasible.** If write operations are ever approved, libgit2 already
   has index, commit, merge and push; shelling out would mean re-parsing porcelain
   output for every command, and `gix` support for some of those paths is newer.
4. **The cost is bounded.** The vendored build takes ~15 s once on this machine and
   the project already compiles C for compression support, so the toolchain
   requirement is not new.

`gix` remains the better long-term choice *if* the project ever wants to drop the C
dependency completely (for example to support a target without a C compiler). The
engine is written behind a small API — `GitRepo` with `list`, `read_file`,
`file_size`, `is_dir`, `branches`, `tags`, `commits` — so replacing the backend means
reimplementing those methods, not touching the panels.

Shelling out to `git` was rejected because it turns a build-time dependency into a
runtime one, and because Windows and Android installations cannot be assumed to have
it.

## Fixtures

`crates/rc-git/src/fixture.rs` builds every repository the tests need:

* two commits with changed and added files, a nested directory and an ignored file,
* a branch (`old`) and a light-weight tag (`v1.0`) that stay at the first commit,
* a bare repository used as a `file://` remote, pushed to from the fixture.

The fixtures are created with `git2` itself and pin `initial_head = "main"` so the
tests do not depend on the machine's `init.defaultBranch`. Everything runs offline;
no test touches the network or the user's own repositories.

## Security notes

* The engine is **read-only**: no operation in `rc-git` writes to a repository.
* Paths inside a revision are normalised and `..` is refused, so a crafted name
  cannot read outside the tree.
* `read_file` takes a byte limit, so a huge blob cannot exhaust memory before the
  caller decides what to do with it.
* Credentials for remotes (15.4) will go through the Phase 13.4 credential store;
  nothing in this crate stores or logs a secret.
