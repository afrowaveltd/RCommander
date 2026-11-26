#!/bin/sh
# Print every note as a JSON array of objects, which is the one shape a manifest source accepts.
# RCommander runs this from the plugin's own folder, so every path here is relative to it.
notes_dir="${RCOMMANDER_NOTES_DIR:-$HOME/Notes}"

# JSON needs two things escaped in a string: the backslash and the quote.
escape() {
    printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

printf '['
first=1
for file in "$notes_dir"/*.md; do
    [ -f "$file" ] || continue
    title=$(basename "$file" .md)
    words=$(wc -w <"$file" | tr -d ' ')
    modified=$(date -r "$file" +%Y-%m-%d 2>/dev/null || stat -c %y "$file" | cut -d' ' -f1)
    [ "$first" -eq 1 ] || printf ','
    first=0
    printf '{"path":"%s","title":"%s","words":%s,"modified":"%s"}' \
        "$(escape "$file")" "$(escape "$title")" "${words:-0}" "$(escape "$modified")"
done
printf ']\n'
