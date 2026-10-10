# Configuration


Gezik keeps its settings in a plain-text folder:

| OS | Location |
|---|---|
| Windows | `%APPDATA%\gezik` |
| macOS | `~/Library/Application Support/gezik` |
| Linux | `~/.config/gezik` |

Set the `GEZIK_CONFIG_DIR` environment variable to use another folder.

- `settings.toml` — your preferences. Portable: copy it to another computer, on any OS. Every
  key is listed there with a comment.
- `themes/*.toml` — your themes. `themes/example.toml` is a commented starting point.
- `templates/` — files and folders offered under **New ▸** (see [File operations](features.md#file-operations)).
- `state.toml` — window size and position, column widths, the preview pane, the tabs of the
  last session and the folders you visited on this machine. Not meant to be copied.
- `views.toml` — the view (list or grid, sort, size) of each folder you changed. Local to
  this machine, like `state.toml`; at most 500 folders are remembered.
- `pending-deletes` — permanent deletes not finished yet (Gezik finishes them on start).
  Local to this machine; not meant to be copied.
- `system-changes.toml` — the changes Gezik made to the system on your command (see
  [System integration](features.md#system-integration)). Written only by Gezik.

The top of `settings.toml` holds:

- `theme` — `"auto"` follows the system light/dark mode (with `theme-light` and `theme-dark`),
  or name a theme.
- `start-folder` — where new tabs open: `"{home}"`, `"drives"` (This PC) or any folder.
- `max-fps` — most frames drawn per second, e.g. while scrolling (default 120; 0 = as many as
  the display refreshes). The software renderer redraws the visible text every frame, so on
  high refresh rate displays a cap saves a lot of CPU.
- `pinned` — folders pinned to the sidebar: a path, or `{ path, name, group }` for an alias
  and a group. Gezik updates this list when you pin, unpin, rename, group or reorder; the rest
  of the file, including your comments, is kept.

```toml
[layout]
sidebar = "left"          # left | right | hidden
density = "comfortable"   # compact | comfortable
reduce-motion = false     # true: no fades on hover and popups
```

Changes apply as soon as you save; no restart needed. Problems (a typo, an invalid
color) show up in the status bar with the file and line, and never stop Gezik from
starting.

## Views

The `[view]` section of `settings.toml` sets how folders look by default:

```toml
[view]
mode = "list"             # list | grid
sort = "name"             # name | modified | created | type | size
sort-dir = "asc"          # asc | desc
grid-size = "medium"      # small | medium | large
icons = "system"          # system | gezik (Gezik's own icons, colored by the theme)
folder-sizes = "off"      # off (only when asked) | local (not on network folders) | all
thumbnails = true         # pictures and videos show a preview in the grid
hide-extensions = false   # show names without their extension
folders-first = true      # folders before files in every sort
date-format = "system"    # relative | short | iso | system
size-format = "binary"    # binary (1024, KB) | decimal (1000, kB)
single-click-open = false # one click opens; Ctrl (Cmd) and Shift clicks still select
# show-hidden = true      # dot names and Windows' hidden items; default true (macOS: false)
show-system = false       # Windows only: protected system items (desktop.ini, $RECYCLE.BIN)
```

`icons = "system"` uses the operating system's icons and type names where Gezik can read
them; `"gezik"` uses Gezik's own icons, colored by the theme's `icon-*` colors.
A folder you change in the View menu (list or grid, sort, size) keeps its own view in
`views.toml`. "Apply to all folders" writes the current view into `[view]` (your comments
are kept); "Reset this folder" returns a folder to the default. The options from
`hide-extensions` down are the same in every folder, and the View menu changes them here too.

Folder sizes are off by default, so nothing is counted in the background. **Calculate
folder sizes** (row menu and View menu) counts the selected folders, or every folder shown,
on any drive. The preview pane (Alt+P) shows a picture, the start of a text file, a
thumbnail, folder item counts or file facts for the selection; Space opens a larger quick
look.

Themes: see [Writing a theme](themes.md). Shortcuts: see [Keyboard shortcuts](shortcuts.md).

---

[← Back to README](../README.md)
