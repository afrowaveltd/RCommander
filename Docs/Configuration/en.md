# Configuration

*Configuration ▸ Options* edits everything; the changes are previewed live and
written to `settings.toml` when you accept them.

| Section | What it controls |
| --- | --- |
| Display | view mode, hidden files, `..` entry, font size, row height, icons, thumbnails, natural sorting |
| Language | the interface language, direction preview |
| Colors | light/dark theme, cursor and selection colours, per-file-type colours |
| Editor | internal editor or an external command, font, tab width, line endings, word wrap |
| Packer | default archive format, compression level, archive extensions |
| FTP | passive mode, MLSD, encoding, timeouts, saved connections |
| Agent | MCP server on/off, transport, port, token, confirmation policy, MCP clients |
| Shortcuts | the whole keyboard map, editable row by row |

## The settings file

`settings.toml` is meant to be edited by hand as well. Unknown keys are kept, so a
newer version's file still loads. The file is created on first start and rewritten
when you choose *Configuration ▸ Save Settings*.

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
