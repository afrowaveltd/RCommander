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

## Switching branches

*Configuration ▸ Switch branch…* opens the branch picker for the repository in the active
panel. It lists every local branch, marks the one that is checked out, and offers:

* **Switch** — check the selected branch out; the panel reloads and the path bar shows the
  new branch;
* **Create and switch** — make a new branch at the current commit and go there;
* a *Switch anyway* checkbox that appears only when the working tree has changes, because
  a dirty tree is refused by default and local edits are kept when you insist.

The switch itself is a `safe` checkout: a file with local changes is never overwritten, and
afterwards the index matches the new `HEAD`, so the switch does not look like a pile of
staged changes. Neither panel is special: each can sit on its own branch.

## Working with a remote

*Configuration ▸ Git remote…* shows the current branch, its remote and how far the two
sides have drifted (`2 to push, 1 to pull`), and offers three operations:

| Action | What it does |
| --- | --- |
| **Fetch** | downloads the remote's branches without touching the working tree |
| **Pull (fast-forward)** | fetches, then moves the branch forward when that is safe |
| **Push** | sends the current branch to its upstream (or `origin`) |

Fetch, pull and push run on a worker thread with the credentials you approved, so a slow
remote never freezes the window; the status line reports the result. **Pull only
fast-forwards**: if the branch has diverged, or the working tree has local changes, it
says so and changes nothing — a half-merged tree is worse than a clear message.

*Merge branch* merges a local branch into the current one, again fast-forward only. For
anything that would need a real three-way merge, use Git itself and reload the panel.

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

## Edit, commit, push

Open a file with `F4`, change it and press *Apply*: the file is saved, and because it
lives in a repository the commander offers to commit it. The dialog shows the file, an
editable commit message (template `[git] commit_message`, default `Edit {name}`) and a
*Push after committing* checkbox. Two checkboxes make it permanent:

* *Commit every saved file from now on* → `[git] commit_on_save = "always"`, which turns
  the dialog into a single click-free commit on every save;
* *Never ask again* → `"never"`, after which saving never commits anything.

The commit uses the identity from your Git configuration when you allowed the commander
to read it, and the push runs on a worker thread, so a slow remote never freezes the
window — the status line says *pushing…* and then *Pushed*. Credentials for HTTPS remotes
come from the credential store or from the `store` helper you approved; nothing is written
to disk.

## Not there yet

Copying files out of a revision, cloning a remote, and the remaining write operations
(staging arbitrary files, pull, merge, switching branches from the panel) are the next
steps; the engine already reads status, stages, commits, pushes, switches branches and
creates them, and it holds back on a dirty tree instead of losing an edit.
