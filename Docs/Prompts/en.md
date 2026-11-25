# Prompts

A prompt is a saved question. RCommander ships a small catalogue of them, grouped
by what they are about, and you can add your own. Open the picker with the
**Prompts…** button in the chat input.

## A prompt is data, not a permission

The most important thing about this feature is what it does **not** do. Inserting a
prompt puts text into the chat input. It grants nothing:

- what an agent may touch is decided by the capability policy (see *Configuration →
  Security*), not by anything written in a prompt;
- a prompt that "asks" for more access is refused exactly like one that asks for
  nothing;
- the variables are filled from what the application already knows, and there is
  deliberately **no** variable for file *content* — a prompt cannot quietly send a
  file somewhere.

That is why prompts are plain TOML files you can read, copy, diff and put in a
repository.

## The catalogue

| Category | What it is for |
| --- | --- |
| **Files** | explaining, summarising, comparing, translating, suggesting a safe edit |
| **Git** | commit messages, diff review, "why did this stop working", commit hygiene |
| **Remote** | comparing local and remote, planning an upload, permissions, a failed transfer |
| **Build** | a failing build, a manifest, a dependency, an upgrade plan |
| **Diagnostics** | reading a log, a service that will not start, a full disk, "it got slow" |
| **General** | what the agent can see, a plan before doing, a critical review, teaching |

Shipped prompts are **read-only**: an upgrade replaces that file, so it must never
hold your edits. Copy one to change it — the copy is yours and upgrades leave it
alone.

## Variables

A prompt body may use `{{name}}`. These are filled in automatically:

| Variable | Value |
| --- | --- |
| `{{cwd}}` | the directory of the active panel |
| `{{selection}}` | the file or folder under the cursor |
| `{{selection_count}}` | how many entries are marked, when more than one |
| `{{model}}` | the model the chat will use |
| `{{host}}` | the host, when the panel shows a remote location |

The picker shows the finished text **before** you insert it, with the variables
already filled in. A variable that cannot be filled stays visible as `{{name}}` and
the preview says which ones: a blank would be a silent bug, a visible marker is an
honest one.

## Your own prompts

Prompts live next to your other configuration:

```
~/.config/rcommander/prompts/git.toml        # one file per category
```

```toml
[meta]
name = "My Git prompts"
category = "git"

[[prompt]]
id = "git.my-message"
title = "A commit message in our house style"
about = "Short subject, no list of files"
text = "Write a commit message for {{cwd}} in our style: a short subject, then why it mattered."
```

Save the file and reopen the picker; nothing needs restarting. An entry without a
title falls back to its id, a prompt without an id is reported, and a file with a
typo costs you only that file — the rest of the catalogue still loads.

## What is not here yet

The catalogue is read and searched; the editor that would let you add a prompt
without touching a text file is the next step (*Configuration → Prompts*), together
with the reset-and-diff for a system prompt you have edited. Until then the TOML
file *is* the editor, which is not a bad place to be: it is versionable, diffable
and portable.
