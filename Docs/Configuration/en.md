# Configuration

*Configuration ▸ Options* edits everything; the changes are previewed live and
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

## The settings file

`settings.toml` is meant to be edited by hand as well. Unknown keys are kept, so a
newer version's file still loads. The file is created on first start and rewritten
when you choose *Configuration ▸ Save Settings*.

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

*Configuration ▸ Options ▸ Dangerous* collects the factory resets. Every entry
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
