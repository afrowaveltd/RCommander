# Git repositories

RCommander reads Git repositories without asking `git` to be installed: the same
code runs on Windows, Linux and Android.

## Your Git configuration

*Configuration ▸ Git configuration…* offers to read the Git configuration of this
machine. It asks first, because that configuration can contain secrets:

| Item | What it is used for |
| --- | --- |
| identity (`user.name`, `user.email`) | authoring commits, and showing who a commit belongs to |
| `credential.helper` | knowing how credentials are kept on this machine |
| stored credentials (`~/.git-credentials`) | signing in to a remote without retyping the password |
| `url.<base>.insteadOf` | expanding short remote aliases such as `gh:owner/repo` |
| `core.sshCommand`, `init.defaultBranch` | SSH command and default branch name |

The dialog lists exactly what was found — hosts and helper names, never a password —
and offers three answers: **Allow**, **Not now**, and a *Remember this decision*
checkbox. The answer lives in `settings.toml` as `[git] read_system_config = "ask" |
"allowed" | "denied"`, so refusing is not asked again until you change it (or use
*Stop using it* in the same dialog).

Passwords read from a `store` helper stay in memory for the session. They are never
written to `settings.toml`, never shown in the dialog, and never appear in a message
or a log line.

## Reading revisions

The engine behind the panels opens a repository (walking up to the enclosing one, as
git does) and exposes branches, tags, remote branches and the commit log, plus the
contents of any revision. Nothing in it writes to a repository.

## Not there yet

Browsing a repository as a panel (branches, tags and commits as folders), copying files
out of a revision, and cloning are the next steps of the plan; the engine they build on
is in place and tested.
