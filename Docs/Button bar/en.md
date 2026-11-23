# Button bar

The button bar gives the most used commands a visible, mouse-friendly home. There
are two of them: a horizontal bar and a vertical one, either can be shown.

## Adding a button

*Configuration ▸ Button Bar* opens the editor. Every button has:

- a **command** — an internal command such as `cm_Copy`, an external program, or a
  directory to open,
- a **caption** and optional icon,
- optional **parameters** with placeholders: `%P` current source path, `%N` current
  file name, `%T` target path, `%M` target file name, `%S`/`%L` the marked files as
  a list, `%D` the current directory.

Because buttons, menus, shortcuts and MCP tools all use the same command
identifiers, a button is a convenient way to give an automation a face.

## Keyboard access

`Alt+Shift+F11` focuses the horizontal bar, `Alt+Shift+F12` the vertical one; then
the arrow keys move between buttons and `Enter` runs the focused one.
