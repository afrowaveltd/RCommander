# Custom columns and view modes

## Column sets

Besides the built-in *brief* and *full* views you can define your own column
sets: name, extension, size, date, attributes, unix permissions, owner, comment,
and the values provided by MCP content tools. Column sets have names and are
chosen from the view menu (`Shift+F1`).

## View modes

A view mode bundles a column set, a sort order, a filter and colours, so a single
keystroke switches the whole panel to, say, "only images, sorted by date, shown as
thumbnails". Modes are stored in the settings file and can be switched
automatically when you enter a directory whose name matches a rule.

## Thumbnails

`Ctrl+Shift+F1` shows a thumbnail grid. Thumbnails are cached on disk; the size
and the cache location are configurable. Large images are downscaled once and
reused, so browsing a photo directory stays fast.

## Colours by file type

Specific masks can be given their own colour — useful to make `*.log` grey,
`*.bak` red or archives green. Rules are evaluated in order, first match wins.

## Thumbnails

`Ctrl+Shift+F1` switches the panel to a thumbnail grid. Images are decoded with a
pixel budget and downscaled once, then cached as GPU textures keyed by path and
size, so scrolling through a directory with thousands of photos does not decode
anything twice. The cache is bounded (512 thumbnails, least-recently-used
entries are dropped), which keeps memory flat.

Directories show as `[DIR]` and everything that is not a decodable image falls
back to its extension, so the grid never stalls on a huge or broken file. The
thumbnail size is configurable (*Configuration ▸ Options ▸ Display*).
