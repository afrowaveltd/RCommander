# Command line

The command line below the panels accepts three kinds of input:

1. **internal commands** — `cm_Copy`, `cm_SrcShort`, … These are the same
   identifiers used by menus, button bars, keyboard shortcuts and MCP agents, so
   anything you can do with a shortcut you can type here.
2. **directory changes** — `cd <path>`, `cd ..`, `cd /media/usb`.
3. **operating system commands** — anything else is handed to the shell of the
   platform, with the current directory of the active panel as the working
   directory.

Press `Enter` to run the line, `Alt+F8` to recall a previous line, and `Ctrl+Enter`
to insert the file name under the cursor.

## Why this matters for automation

Because menus, the button bar, keyboard shortcuts and the MCP server all speak
the same command identifiers, a script, a button and an AI agent can drive the
same operation. `cm_` identifiers are listed in the command reference of this
help and in the app's command palette.
