# FTP connections

The network panel turns one side of the window into a remote file system.

## Connecting

`Ctrl+N` opens the *new connection* dialog, `Ctrl+F` the quick connect dialog.
Fill in host, port, user and password, choose FTP or SFTP, and optionally the
passive mode and initial directories. Connections can be saved; the list is part
of your settings file under `[ftp] connections`.

`Ctrl+Shift+F` disconnects and returns the panel to the local file system.

## Saved sessions

`Ctrl+N` opens the connect dialog, which has a *Saved sessions* section. Fill in a
connection once, give it a name and press *Save*: the profile lands in
`sessions.toml` next to the configuration, separate from `settings.toml`, and is
offered the next time the dialog opens. Clicking a saved session fills the form;
*Load* does the same for the selected one and *Delete* removes it. The password is
never part of the profile — it comes from the credential store, and a session whose
password is not stored yet simply reopens the dialog with the rest pre-filled.

An existing OpenSSH configuration can be imported: type the path (default
`~/.ssh/config`) and press *Import ssh_config*. Every concrete `Host` block becomes
a session, using `HostName`, `User`, `Port` and `IdentityFile`; pattern blocks such
as `Host *` or `Host 192.168.*` are skipped, and a name that already exists is
imported as `name 2` instead of overwriting anything.

Session actions also exist as model calls, so the MCP bridge and the command line
can list, save, rename, duplicate, delete and import sessions without the dialog.

Accepting the dialog starts the connection in the background: the window stays
usable, the status line says *Connecting to …*, and the panel fills in with the
server's directory when the connection is up. A host that does not answer costs a
message, not a frozen window.

## Working with a remote panel

Everything works as with a local directory: navigate, mark, copy in both
directions, rename, delete, create directories. Transfers run in the background
queue, so you can keep browsing while a large upload finishes.

## Passive and active mode

Passive mode (the default) works through almost every firewall and NAT and is
recommended. Active mode is available for servers that require it. If the server
supports `MLSD`, listings include exact timestamps and sizes; otherwise the
classic `LIST` output is parsed, including symlinks and file names with spaces.

## Timestamps and synchronisation

Servers differ in how they report time zones. RCommander treats times as UTC
unless the server reports a different offset, and can preserve the local
modification time when it is available through `MDTM`. This matters for
*synchronise directories*, which compares timestamps.

## Security

Passwords are never saved in the connection profile. Leave the password field
empty to use a stored password for that exact account. Enter a password to
override it for this attempt. **Remember entered password after connecting**
is off by default; select it only to save the newly entered password after a
successful connection. Key-file authentication has a separate masked passphrase
field; this field is not saved by this option. Prefer SFTP on untrusted networks.

## Where passwords are stored

Passwords and tokens are **never written to `settings.toml`**. The connection list
in that file only holds names, hosts, ports and user names; a password is kept in a
credential store:

* the operating system's keyring when one is available, and
* otherwise an encrypted file (`secrets.enc`) protected by a master passphrase
  (Argon2id key derivation, AES-256-GCM, owner-only file permissions).

An older settings file that still contains a password is read once and migrated:
the value moves into the credential store and is removed from the file. Saving the
settings can then never leak it again, because the field is not serialised at all.

The store's key is derived from your passphrase on every unlock and is not cached,
so closing the application locks the secrets. The dialog offers *create* when no
store exists yet and *unlock* when it does; a forgotten passphrase can be replaced
with the *start over* button in that dialog or with *Configuration → Options →
Dangerous → Saved credentials*, which deletes the store. Saved connections
themselves (host, user, port) stay intact, so you only have to type the passwords
again.

## SFTP and SSH

Switch *protocol* to **SFTP** in the connect dialog. SFTP needs a user name and
supports three ways to prove who you are:

| Authentication | What it needs |
| --- | --- |
| password | the password field, or a value already in the credential store |
| key file | the path of the private key; a passphrase is asked for when the key is protected. `~` is expanded, so `~/.ssh/id_ed25519` works |
| ssh-agent | nothing but a running agent with at least one identity loaded |

### Host keys

The server's host key is checked on every connection and remembered in
`known_hosts` next to the configuration:

* **trust on first use** (default) — the key is stored the first time and compared
  afterwards,
* **strict** — a host that is not known yet is refused, which is what you want on a
  managed network,
* **ignore** — no verification at all; use it only for a device that regenerates
  its key on every boot.

A **changed** key is refused in every mode *except* `ignore`, with both keys named in
the message: a changed host key is exactly what a man-in-the-middle looks like. If you
know why the server's key changed, delete its line from `known_hosts` and connect
again — or choose `ignore` for that one connection.

### When a connection fails

The status line narrates the attempt: *resolving the host name*, *opening the
connection*, *exchanging keys*, *checking the server's identity*, *authenticating*,
*reading the directory*. The step it stops at is the answer to "why did it fail":

| It stopped at | What to look at |
| --- | --- |
| resolving | the host name, and whether DNS works at all |
| opening | the port, a firewall, or the server not running |
| exchanging keys | the server speaks something this build cannot: try another port, or check that it really is SSH and not FTP |
| server identity | the host key: `known_hosts`, or the policy in the dialog |
| authenticating | the user name, the password, the key file, or the agent |
| reading the directory | the account may log in but not list that directory |

Two failures are named precisely because they used to look alike: *the private key
`…` does not exist* (with the path that was actually tried, after `~` expansion) and
*the server rejected the key `…`*. Choosing *ignore* for the host key fixes neither —
it is about the server's identity, not yours.

### Protocol, port and stored credentials

Saved login passwords are matched by protocol, user, host and effective TCP
port. An unset port uses 21 for FTP and explicit FTPS, or 22 for SFTP/SSH.
For implicit FTPS explicitly select 990 (FTPS requires a build with TLS support).
Existing nonzero ports are preserved. Supported aliases (SSH, FTPES, FTP+TLS)
use the same credential identity as their canonical protocol. FTP and FTPS
remain separate, even on the same port. Older vault entries under an incorrect
port or alias may require entering and remembering the password again; we do
not search other account keys or automatically move secrets.
