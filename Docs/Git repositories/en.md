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

## A repository as a panel

*Configuration ▸ Open as Git repository* turns the active panel into the repository that
contains its directory. The panel then shows the working tree **plus four virtual
folders**:

| Folder | What it holds |
| --- | --- |
| `branches` | every local branch; opening one shows that revision's tree |
| `tags` | every tag, same idea |
| `commits` | the last 500 commits as `<short id> <summary>` |
| `remotes` | remote-tracking branches |

The path bar shows the branch in brackets (`/home/me/project  [main]`), and the status
line says which repository was opened. Navigating inside a revision works like a normal
directory, and `..` goes back up.

## Colours that tell you where you are

Rows in a Git panel are tinted by their state instead of their file type:

| State | Colour |
| --- | --- |
| staged (added, renamed, staged delete) | the theme's green (`archive`) |
| changed on disk (modified, deleted, type change) | the warning colour |
| untracked | the link colour (`symlink`) |
| ignored | the muted colour (`hidden`) |
| merge conflict | the warning colour |

The tints are theme roles, so a high-contrast theme or a terminal-style theme changes them
together with everything else.

## Not there yet

Copying files out of a revision, cloning a remote, and the write operations (stage,
commit, pull, push, merge, switching branches from the panel) are the next steps; the
engine underneath already reads status, switches branches and creates them, and it holds
back on a dirty tree instead of losing an edit.
