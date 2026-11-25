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

## Editing, copying and resetting

*Configuration → Agent → Prompts…* (`cm_Prompts`) is the editor. It shows the
catalogue by category with a badge — *mine* for your own prompts, *edited* for a
built-in you have changed — and the form beside it.

| Button | What it does |
| --- | --- |
| **Copy to mine** | makes your own copy of a shipped prompt, keeping its id |
| **Reset** | drops your copy, so the shipped text applies again |
| **Delete** | removes one of your own prompts |
| **Save** | writes the prompt into `<config>/prompts/<category>.toml` |

**Editing a built-in is a copy with the same id.** That one mechanism does
everything: the shipped file is never touched (an upgrade replaces it), the picker
still shows a single prompt rather than two, and *Reset* has an obvious meaning —
stop overriding, use the shipped text. While you edit, the page shows the shipped
wording underneath, so "reset" is not a leap of faith.

The form also lists the variables the text uses and what each of them will be
replaced with right now, which is the fastest way to notice that `{{host}}` will be
empty because the panel is local.
