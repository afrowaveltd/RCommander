# Configuration

*Configuration → Options* edits everything; the changes are previewed live and
written to `settings.toml` when you accept them.

| Section | What it controls |
| --- | --- |
| Display | view mode, hidden files, `..` entry, font size, row height, icons, thumbnails, natural sorting |
| Language | the interface language, direction preview |
| Colors | light/dark theme, cursor and selection colours, per-file-type colours |
| Fonts | the three text roles — body, headings and monospace — with their own family and size |
| Editor | internal editor or an external command, font, tab width, line endings, word wrap |
| Packer | default archive format, compression level, archive extensions |
| FTP | passive mode, MLSD, encoding, timeouts, saved connections |
| Agent | MCP server on/off, transport, port, token, confirmation policy, MCP clients |
| Security | the agent's privilege profile, which operations always ask, stored pre-approvals, the live session stop button and the audit log |
| Shortcuts | the whole keyboard map, editable row by row |

## What the Configuration menu offers

The menu lists the pages that became features of their own, so nobody has to
remember which page holds which setting:

| Entry | Opens |
| --- | --- |
| *Options* | the dialog on its first page |
| *Colors*, *Fonts*, *Security*, *Factory resets…* | the dialog on that page |
| *Saved credentials* | the passphrase prompt for `secrets.enc` |
| *Dark ↔ normal*, *Next theme* | switches the colour theme |
| **Git →** | configuration, the Git panel, branches, remote, copy out, stage, commit, clone |
| **Agent →** | the model picker, the chat window, *Chat in the other panel*, *Show files* (only while a panel is showing the chat) and the MCP client panel |

Everything else lives where the work happens: file operations in *Files*, marking
in *Mark*, and the tools that act on a whole directory in *Commands*.

## The settings file

`settings.toml` is meant to be edited by hand as well. Unknown keys are kept, so a
newer version's file still loads. The file is created on first start and rewritten
when you choose *Configuration → Save Settings*.

## The status line, alerts and sounds

The status line is not decoration: **it never goes silent while something is
happening**. A connection says which step it is on, a transfer says its percentage
and the current file, a running search and a thinking agent say so too, each with a
spinner. "Nothing happened" is the one message a file manager must not send.

An error is an **alert**: it appears in the warning colour at the top of the status
line, has a `!` in front, and stays for a few seconds before folding away. A finished
answer appears the same way with a check mark and disappears sooner.

Sounds are optional and off for successes by default (`[sounds]` in the settings, or
*Configuration → Options → Display*):

| Setting | Default | What it does |
| --- | --- | --- |
| `enabled` | on | the master switch |
| `on_error` | on | a sound when an operation fails |
| `on_disconnect` | on | a sound when a connection drops |
| `on_agent_done` | **off** | a sound when the agent finishes an answer |

A file manager that chirps on every copy is a file manager people mute entirely, and
then they miss the error — which is why success is quiet by default. The sounds are
the platform's own (freedesktop events on Linux, the system sound files on macOS, a
generated tone on Windows), so nothing is downloaded and nothing has to be installed.
*Test the error sound* plays exactly what a failure would play.

Because a sound is only a notification, everything it announces is also written down:
a machine with the speakers off loses nothing.

## Security

The *Security* page decides what the agent may do. It applies to the agent only:
your own clicks in the panels are never gated by it, because a file manager that
asked itself for permission would be unusable.

### The approval dialog

When a request needs approval — a Git push from an agent, a delete outside the
shared folder, an elevated action — a dialog appears that names the origin, the
operation, the target and the destination, then lists **what saying yes means**
in plain words ("Data leaves this machine. A remote keeps its copy even if you
delete yours."). It offers three answers:

| Answer | What happens |
| --- | --- |
| **Allow now** | this one request runs; the next one asks again |
| **Allow for this session** | every request of that operation runs without asking until you quit, or press *Stop everything now* |
| **Allow always…** | a **second window** appears with the exact scope, a warning that the decision is written to `settings.toml`, and a text field where you must type `yes` before *Confirm* becomes clickable |

The typed word is the deliberate speed bump: a click can be a mis-click, typing
`yes` cannot. It is not security theatre — it is the difference between two
dialogs and one, and it also makes the *revoke* path findable, because the same
page lists every permanent answer with a *Take it back* button.

### What a pre-approval cannot do

- It cannot unlock a path on the **deny list** (`~/.ssh`, `.git` internals, …).
- It cannot widen the profile: a `sandbox` policy never offers elevated work, so
  it cannot pre-approve it either.
- It never hides itself. Every use writes the grant that allowed it into the
  audit log, and the log's hash chain shows if a line was edited or removed.

### The pages

- **Who the agent is** — the privilege profile (`sandbox`, `user`, `privileged`)
  with one honest sentence each.
- **Always ask before** — operations that open the dialog even when a grant
  covers them. Ticking nothing does not mean "ask nothing": the safe default
  (`execute`, `publish`, `privileged`) comes back.
- **Allowed permanently** — the stored pre-approvals, dated and revocable.
- **Allowed for this session** — what the current run approved, with the
  *Stop everything now* button that clears it and revokes the session grants.
- **Audit log** — how many decisions were recorded and whether the hash chain is
  intact; the last line is shown so you can see what just happened.

## Portable use

Set `RCOMMANDER_CONFIG_DIR` to a directory next to the executable and the whole
configuration — settings, hotlist, history, saved FTP connections — stays with
your installation, for example on a USB stick.

## Resetting

*Configuration → Options → Dangerous* collects the factory resets. Every entry
asks once more before it does anything, and the list says exactly what will be
lost:

| Reset | What it wipes |
| --- | --- |
| Factory settings | every option, the key map and the settings file |
| Tabs, history and hotlist | extra tabs, visited directories, command history, hotlist |
| Trusted SSH host keys | `known_hosts`, so the next connection asks again |
| Saved credentials | `secrets.enc` including its master passphrase |
| Everything | all of the above plus the saved session |

The first two are harmless: they change nothing about your files. The last three
remove data that cannot be recovered, which is the point — a forgotten master
passphrase has no recovery path other than deleting the store.

Deleting `settings.toml` by hand is equivalent to the first entry; keep a copy
first if you have customised the key map.
