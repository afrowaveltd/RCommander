#!/bin/sh
# Open one note. The manifest passes the row's key — the file path — as $1.
if command -v xdg-open >/dev/null 2>&1; then
    exec xdg-open "$1"
fi
if command -v open >/dev/null 2>&1; then
    exec open "$1"
fi
echo "No desktop opener found; the note is $1" >&2
exit 1
