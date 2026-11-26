# Working with the agent on a copy

Right-click a file or a folder and choose **Process with agent…**: the agent gets a
**copy** in a workspace and the chat opens beside your files, asking what you want
done. Your originals are not touched.

## Why a copy

Because a model that can only break a copy is a model you can afford to let run. The
lowest privilege level is therefore the *default* one: the agent sees files in

```
~/.config/rcommander/agent-workspaces/<timestamp>-<pid>/
```

and nothing else. What it may reach is the capability policy's business (see
*Configuration → Security*); what it can *see* is this folder.

## What the workspace holds

- a copy of every selected file, with folders copied as trees;
- a **manifest** (`workspace.toml`) recording, for every file, the original path, the
  copy, the SHA-256 of the original at the time and the size. That is what makes
  "which version is this?" answerable later, and what tells you whether a copy has
  changed — without asking the agent.

A file that cannot be read is named in the transcript and skipped; one unreadable
file in a selection does not cost you the others. If nothing at all could be copied,
the workspace is removed and you get an error instead of an empty conversation.

## The conversation

The transcript opens with a note from the application: which files the agent has,
where the copies are, and that the originals are untouched. Then the question
*"What should I do with this?"* waits in the input, so the next thing you do is
describe the task rather than explain the setup.

Closing the workspace from the menu deletes the copies — that is the safe end of a
session, and it is deliberately one entry away.

## The menu entries

| Entry | What it does |
| --- | --- |
| **Process with agent…** (`cm_ProcessWithAgent`) | prepare the workspace, open the chat, ask what to do |
| **Show the agent's copy** (`cm_OpenWorkspace`) | show the workspace folder in the panel, so you can look at exactly what the agent sees |
| **Close the agent workspace** (`cm_DiscardWorkspace`) | delete the copies |

## Sending changes back

**Send changes back…** (`cm_ReviewWorkspace`, in the same context menu) opens the
review: every file in the workspace with what happened to it, and — for the one you
click — its **diff**.

| What you see | What it means |
| --- | --- |
| `+ line` | the agent's copy adds this line |
| `- line` | the agent's copy removes it |
| `… 20 line(s) unchanged` | context was collapsed; nothing was hidden that changed |
| `binary` | not text, so no diff is possible — and it is never written back by accident |
| a warning on the row | the original changed after the workspace was made |

Only **ticked** files are written, and only the ones that actually changed are
ticked for you. Three rules make this safe:

1. **Nothing is written that you did not tick**, file by file.
2. **A stale original is refused.** If the file changed between "process with agent"
   and "send changes back", the copy is no longer a change to *that* file — and
   overwriting the newer version would be the worst thing this feature could do.
3. **Every write goes through the transaction layer**: a verified backup first, then
   the write, then a re-hash. The backups land in
   `~/.config/rcommander/agent-workspaces/apply-<timestamp>/` next to a manifest, so
   an approved change is reversible and an interrupted one is restored.

Each write is also recorded in the audit log, with *approved in the workspace review*
as its reason — a change with a side effect is never silent.

Afterwards the workspace is coherent again: the copies equal the originals, which is
what the manifest then says.

## Hunks: this change yes, that one no

The diff is split into **hunks** — one contiguous run of changes with a little context
around it — and each hunk has its own tick. That is the difference between "accept the
file" and "accept the fix but not the reformatting the agent also did".

```
[x] @@ -12 @@ +1 -1
      def handler(self):
    -     return None
    +     return self.value
```

Take every hunk and the file is written exactly as the agent left it; take none and
nothing is written; take some and the file is assembled from the ones you took. The
three cases are tested against each other, because a merge that is *almost* right is
worse than none at all.

## Syntax checks before you read the diff

A parser catches what a reader has to notice: an agent that "fixed" a JSON file by
removing a comma produces a change that looks plausible and breaks the program.

| File | Check |
| --- | --- |
| `.json` | parsed as JSON |
| `.toml`, `.ini`, `.cfg`, `.conf` | parsed as TOML |
| anything else | no check, and no claim that there was one |

A file that fails is **not pre-ticked**, its row says why in the parser's own words,
and none of its hunks are ticked either: writing syntax errors back is not something
anybody decides on purpose. A file we cannot check stays silent — a false alarm on a
valid file would train you to ignore the warning, which is worse than not having one.

Running a compiler or a test suite is deliberately **not** part of this: that executes
code, and it belongs behind its own approval.

## What is still open

The remaining piece of Phase 16.4 is a **checklist of approved commands** — `cargo
check`, a JSON Schema, a formatter — that a user can configure per project and that the
review runs (with progress and cancellation) before showing the diff.
