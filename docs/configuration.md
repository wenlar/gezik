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
- `state.toml` — window size and position, column widths, the preview pane, the windows,
  panes and tabs of the last session and the folders you visited on this machine. Not meant
  to be copied.
- `views.toml` — the view (list, grid or columns, sort, grouping, size) of each folder you
  changed. Local to this machine, like `state.toml`; at most 500 folders are remembered.
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

## Settings window

**Settings…** (Ctrl+, on Windows and Linux, ⌘, on macOS; also in the View menu, at the end of
a folder's right-click menu and in the command palette) shows `settings.toml` in a window. An
app with a system-wide Ctrl+, (1Password, for one) takes the key before Gezik sees it: then use
the View menu or the palette, or give `settings` another key in `[shortcuts]`. On
the left are the pages (General, Appearance, Files and Tools, Layouts, Tags, Keyboard, System,
Advanced), then Open settings.toml and Open Config Folder; the search box above the rows looks
through every page by title, key and description. Each row has the comment the file has for it; a setting the file does not set
shows its default, marked "default".

- A change is written at once (there is no OK button), through the same writer as Gezik's
  other changes: only that line of `settings.toml` changes, its comment stays, and so do your
  other comments, the keys Gezik does not know and Windows line breaks. Typed values are
  written on Enter or when the field is left. A value Gezik would not read (a number out of
  range, a speed it does not understand) is not written; the line at the bottom says why.
- Pointing at a theme in the theme list shows it in every window until the pointer leaves.
- Keyboard: Tab goes through the page list (Up and Down choose a page), the search box and the
  rows; Ctrl+PageDown, Ctrl+PageUp and Ctrl+Tab change the page; Esc closes an open list, else
  the window.
- A choice opens its list under the row (Up, Down, Enter; Esc closes it). Right-click a row for **Reset to Default**, **Copy
  Key** (`layout.density`) and **Edit in settings.toml** (opens the file with your editor).
- The Keyboard page lists every action's keys, then each `[[commands]]` entry's ("Command:
  …"); on macOS also the menu bar's ⌘Q, ⌘H, ⌥⌘H and ⌘M, which cannot change. The search box
  finds them by name and by key (`ctrl+shift` lists the actions with Ctrl+Shift). **Set Key…**
  (Enter or Space on it) waits for the keys: press them; Esc cancels, Backspace leaves the
  action no key. A key another action or command has asks "Ctrl+W is used by Close Tab —
  Replace?" under the row: Replace takes it from that one (it keeps its other keys). Only the
  changed action is written, as one line in `[shortcuts]` (`copy-path = "mod+alt+c"`, `mod` so
  the file works on every system); a command's key goes to its own `[[commands]]` entry.
  **Reset to Default** writes the default keys in place. Keys that type in the file list (a
  letter alone), the file list's own keys and the Windows key are refused, with the reason at
  the bottom. `edit-shortcuts` (Keyboard Shortcuts… in the palette) opens this page.
- Other lists and tables (`pinned`, `[[commands]]`, rules …) show how many entries
  they have and are edited in the file. The tray icon and the global shortcut are changed in
  System Integration.
- If `settings.toml` does not read, the line at the bottom says which line, and the rows are
  read-only until it is fixed. Changes made in the file while the window is open show at once.

## Views

The `[view]` section of `settings.toml` sets how folders look by default:

```toml
[view]
mode = "list"             # list | grid | columns
sort = "name"             # name | modified | created | type | size
sort-dir = "asc"          # asc | desc
grid-size = "medium"      # small | medium | large
group = "none"            # none | type | date | size
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
A folder you change in the View menu (mode, sort, grouping, size) keeps its own view in
`views.toml`. "Apply to all folders" writes the current view into `[view]` (your comments
are kept); "Reset this folder" returns a folder to the default. [View rules](features.md#layouts)
pick a view by path, kind or content for folders you have not changed. The options from
`hide-extensions` down are the same in every folder, and the View menu changes them here too.

Folder sizes are off by default, so nothing is counted in the background. **Calculate
folder sizes** (row menu and View menu) counts the selected folders, or every folder shown,
on any drive. The preview pane (Alt+P) shows a picture, the start of a text file, a
thumbnail, folder item counts or file facts for the selection; Space opens a larger quick
look.

Themes: see [Writing a theme](themes.md). Shortcuts: see [Keyboard shortcuts](shortcuts.md).

---

[← Back to README](../README.md)
