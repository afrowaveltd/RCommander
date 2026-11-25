# Themes

Colours are data, not code. Every colour the application draws comes from a theme
file in `themes/` next to the executable (or in the repository root when running
from a build directory). *Configuration → Options → Colors* edits them; the file
format is meant to be edited by hand as well.

## The file format

```toml
[meta]
name = "Ocean"
base = "dark"          # light, dark, high-contrast or another theme name
author = "you"
version = "1"
description = "Cold and calm"

[colors]
background = "#001a2b"
accent = "#00a0ff"

[file_types]
"*.rs" = "#ffcc66"
"*.md" = "#66ccff"
```

Only the roles you want to change need to be listed: everything else is inherited
from `base`, so a theme never ends up with accidental black text on a black
background. Colours are `#rrggbb`; the short `#rgb` form and a missing `#` are
also accepted. A role name that does not exist is an error and is reported in the
options page instead of being ignored silently.

## What the roles paint

A role is not only used for the file list: the window chrome takes its colours from the
same table, so a theme does not leave white buttons behind.

| Role | Also used for |
| --- | --- |
| `background` | window and dialog body, panels |
| `header_background` | **buttons**, checkboxes, sliders, window and menu title bars |
| `panel_border` | button outlines, separators, window frame |
| `foreground` | button and dialog text |
| `accent` | hovered and pressed buttons, focused panel frame |
| `row_alternate` | code blocks and faint fills |

Hover mixes the header colour with the accent, and a pressed button mixes it further, so
the three states stay distinguishable in every theme — including the monochrome ones.

## The roles

| Role | Where it is used |
| --- | --- |
| `background` | panel and window background |
| `foreground` | ordinary file names and dialog text |
| `cursor_background`, `cursor_foreground` | the cursor row in the active panel |
| `selected_background`, `selected_foreground` | marked rows |
| `directory` | directory names |
| `hidden` | hidden files and secondary labels |
| `readonly` | read-only files and error text |
| `archive` | archives |
| `symlink` | symbolic links |
| `header_background` | column headers, status line, tab strip |
| `panel_border` | panel frames and separators |
| `status_background` | status line background |
| `accent` | focused panel frame, tool-bar icons |
| `row_alternate` | every second row, so long lists stay readable |
| `warning` | destructive actions and failed checks |
| `code_keyword` | syntax highlighting: `fn`, `let`, `if`, … |
| `code_string` | string literals |
| `code_comment` | comments |
| `code_number` | numeric literals |
| `code_type` | type names and `SCREAMING_CASE` constants |
| `code_function` | a name directly followed by `(` |

The six `code_*` roles are what make a code block in a model answer, a help topic
or the F3 viewer readable without a grammar: a small highlighter labels keywords,
strings, comments, numbers, types and calls, and the theme decides the colours. A
theme that lists none of them inherits the base theme's choice, so an old theme
file keeps working and simply gains colour.

## Shipped themes

| Theme | Look |
| --- | --- |
| **Classic Light** | the traditional white list with a dark blue cursor bar |
| **Classic Dark** | the same layout on a dark background |
| **High Contrast** | black background, bright text, strong cursor — for poor displays, projectors and low vision |
| **AS/400 Green Screen** | the 5250 terminal: green on black, blue field labels, amber highlights |
| **Midnight Commander** | the classic two-pane blue, cyan directories, black-on-cyan selection |
| **Windows Explorer** | light and calm, Windows-style accent blue and soft selection |
| **Solarized Dark** | Schoonover's low-contrast palette, dark form |
| **Amber Terminal** | the other monochrome terminal: amber on black |

Every shipped theme is checked by a test: all roles resolve, and the six pairs that
decide whether the window can be used at all (text on background, cursor text on
cursor background, marked text on marked background, text on the alternate row,
muted text on background, accent on the header) stay above a 3:1 contrast ratio.

The two classic themes are also the built-in fallback, so the application starts even
when the `themes/` directory is missing.

Tool-bar icons follow the action, not the decoration: destructive commands use
`warning`, creating ones `archive`, moving ones `directory`, navigation `accent`,
searching and network commands `symlink`, and quiet commands `hidden`. A theme with a
bright red warning colour therefore also has bright red delete icons.

## Adding a theme

1. Copy an existing file to `themes/<name>.toml` and change `[meta] name`.
2. Set `base` to the theme you want to start from and list only the roles you are
   changing.
3. Use *Configuration → Options → Colors → Reload themes*, or restart.

The themes are discovered at start-up, exactly like `locales/` and `Docs/`; no
recompilation is needed and an unknown or broken file is reported in the options
page without stopping the others from loading.

## Editing in the application

The Colors page lists every role with a colour picker and shows a live preview of a
panel with a cursor row, a directory, a hidden file and a read-only file. The
contrast of the important pairs is checked as you edit: text on the background, the
cursor text on the cursor background, marked text on the marked background, and
ordinary text on the alternate row. A pair below 3:1 is reported with its measured
ratio rather than being silently accepted.

*Save theme* writes the file into the theme directory and makes it active.
*Delete theme* removes the file; the built-in themes cannot be deleted, so there is
always a way back to a readable colour scheme.

## Per-file-type colours

`[file_types]` maps a mask to a colour, e.g. `"*.log" = "#808080"`. The mask syntax
is the same as everywhere else in the application (it can be edited in
*Configuration → Options → Display*). When several masks match a name, the most
specific rule wins; a rule from the active theme takes precedence over nothing —
there is no second source of colours.

## Notes

* A theme file is read-only data: deleting every theme file restores the built-in
  light theme, and *Configuration → Options → Dangerous → Factory settings* clears
  the selected theme as part of the settings reset.
* The theme name stored in `settings.toml` is the file name in lower case, so
  renaming `name` inside a file does not lose the selection; the new name is simply
  shown in the options page.
