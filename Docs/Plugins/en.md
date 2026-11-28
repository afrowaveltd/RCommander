# Plugins

A plugin turns a program into a panel without compiling anything. The whole plugin is a TOML file
called `plugin.toml`, and the panel that appears is the same two panels you already use: the left
and right halves of the window, with their own columns, their own rows and their own keys.

## Where a plugin lives

| Directory | For |
| --- | --- |
| `<config>/plugins/<id>/plugin.toml` | what *you* installed. Wins over a bundled plugin with the same id |
| `plugins/` next to the application | examples that ship with RCommander |

`<config>` is your user configuration directory — on Linux `~/.config/rcommander`, on Windows
`%APPDATA%\rcommander`. A folder whose `plugin.toml` cannot be read is reported on the status line
and skipped; it never stops the application from starting.

The folder **name must equal the id**, because the id is the URL scheme: `notes://` is answered by
`plugins/notes/`. Re-reading the directory is the `cm_ReloadPlugins` command, so a new plugin needs
no restart.

## The smallest working plugin

```toml
[plugin]
id = "notes"                    # lower case, must equal the folder name
name = "Notes"
version = "1.0"
description = "Every Markdown note in a folder"

[source]
name = "Notes"                  # the label in the path bar
list = ["./list.sh"]            # prints a JSON array of objects
open = ["./open.sh", "{path}"]  # optional; {path} is the row's key
key = "path"                    # which JSON field identifies a row

[[source.columns]]
id = "title"                    # the first column is the row's name
label = "Note"
field = "title"
width = 300

[[source.columns]]
id = "words"
label = "Words"
field = "words"
numeric = true

[source.capabilities]
allow = ["view"]                # a listing you can read and nothing more
```

A complete, runnable version of this is in `plugins/examples/notes/` — copy that folder to
`<config>/plugins/notes` and open `notes://`.

## What the list command must print

A JSON array of objects, one object per row:

```json
[
  {"path": "/home/me/Notes/hello.md", "title": "hello", "words": 10},
  {"path": "/home/me/Notes/todo.md",  "title": "todo",  "words": 3}
]
```

Anything else is an error that says what arrived — an empty panel is never a stand-in for a
broken command, because "the tool did not answer" and "there is nothing here" must not look the
same.

## Arguments, and why there is no shell

`list` and `open` are fixed arrays. There is exactly one substitution in the whole manifest:
`{path}` in `open` receives the row's key. The value is passed as **one argument**, so a note
titled `; rm -rf ~` is a title and not an instruction. If you want a shell, you have to name it
yourself (`["sh", "-c", "…"]`), and then it is your shell and your rules.

Commands run with the plugin's own folder as the working directory, which is what lets a plugin
carry its own helper script (`./list.sh`) instead of depending on something installed system-wide.

## Columns

- `id` — stable inside the plugin, and never one the panel already uses (`name`, `ext`, `size`,
  `date`, `attrs`, `comment`, `mode`, `link`). Reusing one is refused by name.
- `label` — the header text.
- `field` — the JSON field that fills the cell. Defaults to `id`.
- `width` — preferred width in points. The first column always fills the rest of the panel.
- `numeric` — right-align, for numbers.

Column headers of a plugin source are not sortable buttons: the panel cannot compare two notes the
way it compares two file names.

## Capabilities

`allow` lists the file operations that exist in this place, from `copy`, `move`, `create`,
`delete`, `rename`, `view`. **The default is nothing**, and that default is the whole point: a
plugin that does not declare `delete` cannot have its rows deleted. The key is drawn as
unavailable, the command is refused before anything is staged, and no approval dialog appears —
because there is nothing to approve.

A declared capability is not a permission either. A plugin source is a *place*, and even where it
declares deletion, the policy, the approval dialog and the audit log decide exactly as they do for
a local file.

## The second backend: a tool instead of a program

Everything above describes a program that prints rows. The same manifest can instead name **a tool on
an MCP server** you already configured in the MCP agent panel, and nothing else changes: the columns,
the capability gate, the function-key bar and the panel are the same code.

```toml
[plugin]
id = "mail"
name = "MailCraft"

[source]
name = "Mail"
backend = "mcp"                 # a program is still the default
server = "mailcraft"            # the name it has in the MCP agent panel
list = ["list_messages"]        # one tool name, no arguments here
open = ["read_message"]
key = "id"

[source.list_arguments]         # fixed arguments of every listing call
folder = "inbox"

[source.open_arguments]         # {path} receives the row's key
id = "{path}"

[[source.columns]]
id = "subject"
label = "Subject"
field = "subject"

[source.capabilities]
allow = ["view"]
```

The tool must return the same shape a program prints: a JSON array of objects. A server that sends
`structuredContent` is read from there; one that sends text is parsed as JSON. `isError: true` from
the server becomes the panel's error message, word for word.

Two honest limits of this backend, both enforced rather than documented:

- **It cannot write.** Declaring `copy`, `move`, `create`, `delete` or `rename` is refused when the
  manifest loads, because the adapter has no way to perform them — a manifest is not allowed to
  promise what the code cannot do.
- **It connects per call.** A refresh opens the connection and closes it, so a listing never holds a
  child process open for a panel nobody is looking at. The price is one handshake per refresh.

A manifest that names a server nobody configured is reported once at load — *needs the MCP server
"mailcraft", which is not configured* — and skipped, rather than becoming a panel that fails on every
keystroke.

## Plugin localization

A plugin can ship its own language packs in `plugins/<id>/locales/`, using
exactly the application's TOML format. No manifest change is required.
Example: `plugins/notes/locales/cs.toml`:

```toml
[meta]
code = "cs"
name = "Czech"
native_name = "Čeština"
direction = "ltr"
version = 1

[strings]
"plugin.name" = "Poznámky"
"plugin.description" = "Správa poznámek"
"source.name" = "Moje poznámky"
"column.title.label" = "Název"
"column.words.label" = "Počet slov"
```

The loader reads the current language, its base language (for a regional
code), then English. Fallback is per key, finally to the original manifest
label. Plugin dictionaries are scoped by plugin id, never merged over host
or other plugin keys. Only display names, descriptions and column labels
are mapped automatically; additional keys are available through
`Model::plugin_t` and `Model::plugin_tf` for future UI integrations.
Identifiers, command arguments, data fields and capabilities remain unchanged.
Malformed/oversized packs report a problem but do not disable a valid
manifest. Re-reading plugins replaces their locale data; after switching
the UI language, use Reload plugins to update existing plugin labels.
The host UI language and RTL direction remain authoritative.

## What is not possible yet

- **Row verbs** (a *Reply* or *Export* entry in the right-click menu) are supported by compiled-in
  sources but not yet declared in a manifest.
- **Writing** through the MCP backend; declared row verbs; sorting inside a source.
- **Writing**: a manifest source cannot create, move or delete; `allow` merely states what would be
  meaningful if it could.
