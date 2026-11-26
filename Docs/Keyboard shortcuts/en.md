# Keyboard shortcuts

All shortcuts can be changed in *Configuration → Options → Shortcuts*; they are
stored in the `[shortcuts]` table of `settings.toml` as `"ctrl+f3" = "cm_SrcByName"`.

## The essentials

| Key | Action |
| --- | --- |
| `F1` | help |
| `F2` | re-read the directory |
| `F3` | preview the file |
| `F4` | edit the file |
| `F5` | copy |
| `F6` | move / rename |
| `F7` | create directory |
| `F8` | delete |
| `F9` | pack |
| `F10` | **quit** — the Norton Commander ending |
| `F11` | open |
| `F12` | open a terminal here |
| `Tab` | switch panel |
| `Enter` | open |
| `Backspace` | parent directory |
| `Insert` / `Space` | mark |
| `Esc` | cancel |

## The row follows what has the focus

The function keys are not one table — they are four, and the bar at the bottom of
the window draws the row of whatever is in front of you, with the name of that
surface at its left edge.

| Where | What `F1`…`F4` do |
| --- | --- |
| **files** | help, re-read, preview, edit — the classic set, up to `F12` = open a terminal here |
| **chat** | help about the chat, choose the model, open the prompts, save the question as a prompt |
| **editor** | help, save, save a copy, reload from disk; `F10` closes |
| **viewer** | help, re-read, and the three modes (`F3` text, `F4` hex, `F5` image), zoom on `F6`/`F7`, `F10` closes |

Over a conversation there is no cursor row to copy or delete, so those commands are
**absent** from its row rather than disabled: a button that cannot work is worse than
no button. `F1` is always help, and it opens the topic of the surface you pressed it
on — the chat, the editor, the viewer or the shortcuts themselves.

Under the plain keys the bar draws a second row of **modifier variants** — `Shift`,
`Ctrl` and `Alt` — and they are not decoration: hovering one names the command it
runs, clicking it runs it. They come from the key map, so a variant you rebind
appears with its new command and one that does not exist is not offered.

`F10` leaves the program, as it did in Norton Commander; `Alt+F4` does the same.
(The menu bar is always visible, so no key is needed to open a menu.)

The rows are read from the model, which is why the bar and the keys can never
disagree, and why the coming text frontend will have the same keyboard.

## Combinations

`Ctrl+F1`…`Ctrl+F7` change the view mode and the sort order, `Ctrl+Shift+F5`
creates a shortcut of the file, `Ctrl+M` the multi-rename tool, `Ctrl+D` the
hotlist, `Alt+Down` the directory history, `Alt+F7` the search, `Alt+F5`/`Alt+F6`
pack and unpack, `Ctrl+T` a new tab, `Ctrl+W` closes a tab and `Ctrl+Tab` cycles
through them. `Ctrl+Q` opens the quick view of the file under the cursor,
`Ctrl+Shift+F8` the directory tree and `Shift+F2` compares the two panels.

## Function keys in the bottom bar

The bar shows the current meaning of `F3`…`F8`: it changes with the context, for
example `View` becomes `Edit` for text files. Clicking a label runs the command,
right-clicking it opens the configuration.
