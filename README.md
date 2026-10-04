# Gezik

A fast, lightweight, cross-platform file manager written in Rust with [Slint](https://slint.dev).

Source-available and free for non-commercial use (see [License](#license)).

## Goals

- Instant startup and low memory use
- The UI thread never waits on the file system
- Windows, macOS and Linux from one codebase

## Layout

- `crates/gezik-core`: platform-independent logic (listing, sorting, formatting)
- `crates/gezik-config`: settings and themes (no UI dependency)
- `crates/gezik`: the desktop app (Slint UI)

## Settings and themes

Gezik keeps its settings in a plain-text folder:

| OS | Location |
|---|---|
| Windows | `%APPDATA%\gezik` |
| macOS | `~/Library/Application Support/gezik` |
| Linux | `~/.config/gezik` |

Set the `GEZIK_CONFIG_DIR` environment variable to use another folder.

- `settings.toml` — your preferences. Portable: copy it to another computer, on any OS.
- `themes/*.toml` — your themes. `themes/example.toml` is a commented starting point.
- `state.toml` — window size and position, column widths and the preview pane on this
  machine. Not meant to be copied.
- `views.toml` — the view (list or grid, sort, size) of each folder you changed. Local to
  this machine, like `state.toml`; at most 500 folders are remembered.
- `start-folder` — where new tabs open: `"{home}"`, `"drives"` (This PC) or any folder.
- `max-fps` — most frames drawn per second, e.g. while scrolling (default 120; 0 = as many as
  the display refreshes). The software renderer redraws the visible text every frame, so on
  high refresh rate displays a cap saves a lot of CPU.
- `pinned` — folders pinned to the sidebar. Gezik updates this list when you pin,
  unpin or reorder; the rest of the file, including your comments, is kept.

Changes apply as soon as you save; no restart needed. Problems (a typo, an invalid
color) show up in the status bar with the file and line, and never stop Gezik from
starting.

### Views

The `[view]` section of `settings.toml` sets how folders look by default:

```toml
[view]
mode = "list"          # list | grid
sort = "name"          # name | modified | created | type | size
sort-dir = "asc"       # asc | desc
grid-size = "medium"   # small | medium | large
icons = "system"       # system | gezik (Gezik's own icons, colored by the theme)
thumbnails = true      # pictures and videos show a preview in the grid
```

`icons = "system"` uses the operating system's icons and type names where Gezik can read
them; `"gezik"` uses Gezik's own icons, colored by the theme's `icon-*` colors.
`thumbnails = false` turns picture previews off. A folder you change in the View menu
(list or grid, sort, size) keeps its own view in `views.toml`, which stays on this
machine. "Apply to all folders" writes the current view into `[view]` (your comments
are kept) and makes every folder open that way; "Reset this folder" returns a folder to
the default. The preview pane (Alt+P) shows a picture, the start of a text file, a
thumbnail, folder item counts or file facts for the selection; Space opens a larger quick
look.

### Keyboard shortcuts

| Action | Windows / Linux | macOS |
|---|---|---|
| New tab / close tab | Ctrl+T / Ctrl+W | ⌘T / ⌘W |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab | same |
| Back / forward | Alt+← / Alt+→ (or mouse side buttons) | ⌘[ / ⌘] |
| Parent folder | Alt+↑ | ⌘↑ |
| Type a path | Ctrl+L | ⌘L |
| Refresh | F5 | ⌘R |
| Select all (`select-all`) | Ctrl+A | ⌘A |
| List view / grid view (`view-list`, `view-grid`) | Ctrl+1 / Ctrl+2 | ⌘1 / ⌘2 |
| Show or hide the preview pane (`toggle-preview`) | Alt+P | Alt+P |
| Quick look (`quick-look`, only while the file list has focus) | Space | Space |

Change them in `settings.toml` under `[shortcuts]` (`"mod"` is ⌘ on macOS and Ctrl
elsewhere, `""` disables one). Ctrl+wheel in a folder changes the grid or icon size.

The file list keys are fixed: arrow keys, PgUp/PgDn, Home/End move the focus; Shift with
them extends the selection; Ctrl with the arrows moves the focus without selecting;
Ctrl+Space toggles the focused item; Enter opens the selection; Esc clears it. Type a
name's first letters to jump to it. In quick look, the arrows move through the folder
and Space or Esc closes it.

### Writing a theme

A theme only needs the values it changes; the rest comes from its `base`:

```toml
# themes/sunset.toml  ->  select it with  theme = "sunset"  in settings.toml
base = "dark"

[colors]
accent = "#ff8a3d"
icon-folder = "#ffb347"
```

See `themes/example.toml` for every color and size you can set. Besides the interface
colors, `icon-folder`, `icon-image`, `icon-video`, `icon-audio`, `icon-archive`,
`icon-document`, `icon-code` and `icon-other` color Gezik's own icons, `focus-ring` is
the keyboard focus in the file list and `marquee` is the rubber-band selection (it can
be translucent, e.g. `"#88c0d033"`). Older themes that set `folder-icon` or `file-icon`
keep working: they are still read as `icon-folder` and `icon-other`.

## Performance checks (Windows)

```powershell
scripts/perf/measure.ps1   # startup time and idle memory
scripts/perf/stress.ps1    # 100,000-file folder: memory and scrolling CPU
scripts/perf/tabs.ps1      # memory with 1 vs 20 tabs
scripts/perf/grid.ps1      # grid view, 1000 pictures: memory after paging through them
```
```sh
cargo test -p gezik-core --release -- --ignored sorting_100k   # sorting 100,000 names
```

Last measured (release build, Windows 11, 359 Hz display):

| Check | Result | Target |
|---|---|---|
| Startup to window | 24-35 ms | <= ~60 ms |
| Idle memory, default window (900×600) | 6.8 MB system icons, 6.3 MB `icons = "gezik"` | <= 7 MB (5.8 MB before the view work) |
| 100,000-file folder, after load | 17.2 MB | <= ~18 MB |
| Sorting 100,000 names | 39 ms | <= 50 ms |
| Scrolling CPU (100,000 files) | ~270 ms with `max-fps = 120` (~840 ms with `max-fps = 0`) | ~480 ms / 2.6 s |
| Grid, 1000 pictures, after paging through ~600 | 24 MB | <= ~50 MB |
| 1 vs 20 tabs | +0.1 MB | <= +2 MB |

Idle memory grows with the window: the software renderer keeps one frame of 4 bytes per
pixel, so a 1334×600 window measures 7.8 MB (system icons) instead of 6.8 MB. To measure
with a clean config, pass a folder: `scripts/perf/measure.ps1 -Config <folder>` (its
`settings.toml` may set `[view]` `icons = "gezik"`).

## Build

```sh
cargo run --release -p gezik
cargo test
```

## License

Gezik is licensed under the [PolyForm Noncommercial License 1.0.0](LICENSE.md).
You may use, modify and share it for any non-commercial purpose, including personal
use, research, education and use by non-profit organizations. Commercial use is not
permitted. When you share Gezik or a modified version, keep the license and the
`Required Notice` line from `LICENSE.md`.

Copyright Wenlar LLC.

Gezik's user interface is built with Slint, used under the
[Slint Royalty-free License](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md).

<a href="https://slint.dev"><img src="https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-whitebg.png" alt="Made with Slint" height="60"></a>
