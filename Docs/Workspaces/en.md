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

## What is not here yet

**Sending changed copies back to the originals.** That is Phase 16.4's review
pipeline — diff, per-file approval, a transaction with a verified backup, an audit
entry — and it is a separate, deliberate act by design. Until it lands the workspace
is a place to *look and think*, not to publish; you copy what you want across
yourself, with your own hands and your own marks.
