# FTP connections

The network panel turns one side of the window into a remote file system.

## Connecting

`Ctrl+N` opens the *new connection* dialog, `Ctrl+F` the quick connect dialog.
Fill in host, port, user and password, choose FTP or SFTP, and optionally the
passive mode and initial directories. Connections can be saved; the list is part
of your settings file under `[ftp] connections`.

`Ctrl+Shift+F` disconnects and returns the panel to the local file system.

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

Passwords are stored in your configuration file only when you ask for it. Prefer
SFTP or FTPS for anything that is not a trusted local network, and check the
`[ftp]` section of the settings file to review what is stored.
