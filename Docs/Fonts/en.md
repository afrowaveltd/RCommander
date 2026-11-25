# Fonts

RCommander ships with two fonts and lets you replace either of them with any face
already installed on your machine. **Configuration → Options → Fonts**.

## The three roles

Text in a file manager is not all the same, so there are three roles instead of
one font size slider:

| Role | Where it is used | Default |
| --- | --- | --- |
| **Text, lists and menus** | file rows, menus, dialogs, messages | the built-in sans, 14 pt |
| **Headings** | dialog and panel headings | the built-in sans, larger |
| **Paths and code** | the path line, the hex viewer, the editor, code in help topics | the built-in monospace, 14 pt |

Each role has its own family and its own size, so a wide monospaced face can be
made smaller without shrinking the file list. The **Sample** block under the
pickers previews all three with the values you are about to accept.

## Where the list comes from

RCommander looks for `ttf` and `otf` files in the places the platform keeps fonts:

- Linux: `~/.local/share/fonts`, `~/.fonts`, `/usr/share/fonts`,
  `/usr/local/share/fonts`
- Windows: `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, `%SystemRoot%\Fonts`
- macOS: `~/Library/Fonts`, `/Library/Fonts`, `/System/Library/Fonts`

The list is built **once**, the first time somebody needs it, and never on a
start-up that uses the built-in fonts — a machine with thousands of font files
should not slow down the file manager just because the feature exists. *Look
again* rescans, which is what you need after installing a font while RCommander
is running.

## Honest limits

- **Family names are read from the file names.** `DejaVuSansMono-Bold.ttf` becomes
  *DejaVu Sans Mono*, and a Bold file is not offered as a family of its own. A file
  whose name carries no recognisable family (`ab12cd.ttf`) is not listed at all,
  because offering "Ab12cd" as a font would be a lie.
- **Font collections (`.ttc`) are not offered**: they contain several faces and
  RCommander would have to choose one for you.
- **Only files that start like a font are listed.** A stray file with a `.ttf`
  extension is skipped instead of crashing the first frame that draws a letter.
- A family that was chosen and then removed from the system is reported on the page
  ("body: … was not found") and the built-in face is used, so the window stays
  readable instead of failing at start-up.
- **Icons keep working.** The symbols in the tool bar come from a bundled font
  that stays the last fallback of every chosen family.

## In the settings file

```toml
[fonts]
body_family = "Noto Sans"
heading_family = ""          # empty = the body family, one size larger
mono_family = "JetBrains Mono"
heading_size = 18.0
mono_size = 14.0
```

`display.font_size` stays the body size, so an older settings file keeps working
and nothing has to be migrated.
