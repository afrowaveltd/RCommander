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

Delete `settings.toml` to return to the defaults; keep a copy first if you have
customised the key map.
