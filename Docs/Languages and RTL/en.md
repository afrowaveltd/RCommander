# Languages and RTL

The interface is translated with language packs; English is the default and
Czech ships with the application.

## Language packs

A pack is a single TOML file in the `locales` directory (`locales/de.toml`):

```toml
[meta]
code = "de"
name = "German"
native_name = "Deutsch"
direction = "ltr"      # optional, detected from the code otherwise

[strings]
"dialog.copy.title" = "Kopieren"
```

Drop the file next to the application (or into the user configuration
directory), restart, and choose it in *Configuration ▸ Options ▸ Language*.
Missing keys fall back to English, so a partial pack is usable immediately.

## Right-to-left languages

Languages written right to left (Arabic, Hebrew, Persian, Urdu, …) are detected
from the language code and from the `direction` field of the pack. When the
selected language is RTL:

- panels, file lists, menus and dialogs are mirrored horizontally,
- the path bar runs from right to left,
- the previewer renders text right-aligned and the editor follows the document.

Mixing scripts is supported: the previewer looks at the first strong character
of the text, so a Hebrew file inside an English UI is still shown correctly.

## Translating this help

This help is plain Markdown: `Docs/<Topic>/<language>.md`. Copy `en.md` to your
language, translate it, restart the app, and the topic appears in your language.
Topics without a translation are shown in English.
