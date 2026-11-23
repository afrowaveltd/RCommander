# Getting started

RCommander is a dual-pane file manager. Two directory panels sit side by side;
one of them is *active* (its cursor is highlighted). Almost every operation acts
on the active panel and uses the other panel as its destination.

## First run

1. The left panel opens your home directory, the right panel your root or home
   directory as well — change either with `Alt+F1` / `Alt+F2`.
2. Move with the arrow keys, `PgUp`/`PgDn`, `Home`/`End`.
3. `Enter` opens the directory under the cursor (and archives, when enabled).
4. `Tab` switches the active panel.
5. The function-key bar at the bottom is the shortest path to the main
   operations: view, edit, copy, move, create, delete.

## Where settings live

All settings are stored in a single human readable file:

- Linux: `~/.config/rcommander/settings.toml`
- Windows: `%APPDATA%\RCommander\settings.toml`
- Android: the app's private configuration directory

Set the environment variable `RCOMMANDER_CONFIG_DIR` to keep the configuration
next to the program (portable installation).

## Tips

- Typing a letter jumps to the first file starting with that letter; keep
  typing to refine. `Esc` cancels.
- `Ctrl+D` shows your directory hotlist, `Alt+Down` the directory history.
- `F1` opens this help; the topics are ordinary text files you can edit.
