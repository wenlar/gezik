# Gezik

A fast, lightweight, cross-platform file manager written in Rust with [Slint](https://slint.dev).

Source-available and free for non-commercial use (see [License](#license)).

## Goals

- Instant startup and low memory use
- The UI thread never waits on the file system
- Windows, macOS and Linux from one codebase

## Layout

- `crates/gezik-core`: platform-independent logic (listing, sorting, formatting, rename rules, patterns)
- `crates/gezik-config`: settings, shortcuts and themes (no UI dependency)
- `crates/gezik-ops`: the file operations engine (queue, conflicts, progress, undo)
- `crates/gezik-batch`: batch operations (rename rules, archives, conversion, PDF)
- `crates/gezik-search`: searching folder trees (scanner, name and content matching, Everything)
- `crates/gezik-platform`: the parts that differ per OS (drives, icons, clipboard, trash, menus)
- `crates/gezik`: the desktop app (Slint UI)

## Settings and themes

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
- `templates/` — files and folders offered under **New ▸** (see [File operations](#file-operations)).
- `state.toml` — window size and position, column widths, the preview pane, the tabs of the
  last session and the folders you visited on this machine. Not meant to be copied.
- `views.toml` — the view (list or grid, sort, size) of each folder you changed. Local to
  this machine, like `state.toml`; at most 500 folders are remembered.
- `pending-deletes` — permanent deletes not finished yet (Gezik finishes them on start).
  Local to this machine; not meant to be copied.
- `system-changes.toml` — the changes Gezik made to the system on your command (see
  [System integration](#system-integration)). Written only by Gezik.

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

### Views

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

### File operations

Gezik copies, moves and deletes with its own engine on every OS:

- **Copy, cut, paste** (Ctrl+C / Ctrl+X / Ctrl+V) use the system clipboard: copy in Gezik
  and paste in Explorer, Finder or a Linux file manager (X11 or Wayland), or the other way
  round. Cut items look faded until they are pasted. With a picture or text on the clipboard
  and no files, paste makes a `Pasted image ….png` or `Pasted text ….txt`.
- **Drag and drop** files onto a folder in the list, the empty space (the folder shown), a
  folder or drive in the sidebar, a part of the address bar or a tab (rest on it to open it).
  The same drive moves, another drive copies; Shift moves, Ctrl (⌥ on macOS) copies; Alt or
  Ctrl+Shift on Windows, Ctrl+Shift on Linux and ⌘⌥ on macOS make links; the right button
  asks. Drop folders between pinned ones to pin them. Drags go out to Explorer, the desktop
  or any program that takes files, and come in from them; a drop is undone with Ctrl+Z like
  any other operation.
- **Conflicts are asked up front**: before anything is replaced, every item that already
  exists shows in one list with a decision each (Replace, Skip, Keep both, If newer).
  Nothing is replaced unless you choose so, and replaced files go to the trash.
- **Delete** moves to the Recycle Bin / Trash; **Shift+Delete** deletes for good, after a
  question. A permanent delete takes the items out of the folder at once and finishes in the
  background; if Gezik closes first, it finishes on the next start.
- **Undo / Redo** (Ctrl+Z / Ctrl+Y, ⌘Z / ⌘⇧Z on macOS) work for copy, move, rename, new
  items, links, trash and replace, for the whole session. Items changed since are left alone.
- **Rename in place** with F2 (Enter on macOS); **New folder** with Ctrl+Shift+N; **New ▸**
  also offers a text file, a Markdown file and everything in your `templates/` folder.
  **New folder with selection** (Ctrl+Alt+N, ⌃⌘N on macOS) moves the selected items into a
  new folder.
- **Create link**: a shortcut, junction or symbolic link on Windows, a symbolic link elsewhere.
- **Drop stack**: a strip above the status bar that collects items (Ctrl+Shift+S, or drag
  onto it) from several folders; then **Copy here** or **Move here**. Kept for the session only.
- Operations on the same drive wait for each other; different drives run at the same time.
  Long ones show in a panel above the status bar (fold it into the status bar with the
  arrow), with pause, cancel and, on Windows, progress on the taskbar button. Its
  **History** tab lists what finished this session.

```toml
[files]
confirm-trash = false     # ask before moving to the trash
copy-threads = "auto"     # auto (SSD 6, spinning disk 1, network 4) or 1-16
```

### Batch rename

Rename with two or more items selected (or the `batch-rename` shortcut) opens a layer with
rules on the left and a live preview on the right:

- **Replace** (plain or regex), **Number**, **Case**, **Add text**, **Extension**,
  **Template** and **Trim / clean**, applied in order. Each can be turned off for a moment.
- Template fields: `{name}`, `{ext}`, `{n:03}`, `{parent}`, `{date:%Y-%m-%d}`, `{taken:…}`
  (the photo's EXIF date) and `{size}`.
- Duplicate names are shown before anything changes; the whole rename is one Ctrl+Z.
- **Presets ▾ › Save current rules as…** keeps a rule set in `settings.toml` (`[[rename-presets]]`).

### Archives

- **Extract here** (one item at the top goes straight in, several go into a folder),
  **Extract to "name"** and **Extract to…**. Opens zip, 7z, rar, tar (gz, xz, bz2, zst), single
  `.gz`/`.xz`/`.bz2`/`.zst` files, cab, iso, cpio, ar and deb, with passwords and split
  volumes. Other formats go through 7-Zip.
- **Compress…** makes zip, 7z, tar.gz, tar.xz or tar (a single file also .gz or .xz), with
  Store / Fast / Normal / Best, a password (zip, 7z), "Encrypt file names" and split volumes
  (7z). **Compress to "name.zip"** skips the layer. **Add to existing archive…** adds to one.
- Unsafe paths in an archive (`..`, absolute paths) are refused.

```toml
[archives]
double-click = "system"   # system | extract-here
```

### Convert and PDF

**Convert…** in the row menu offers what fits the selection:

- **Pictures**: JPEG, PNG, WebP, BMP, AVIF; resize, rotate by EXIF, remove metadata. Presets
  such as "Resize photos (JPEG 1920 px)" and "Remove location data".
- **Text**: "Convert to UTF-8", Windows (CRLF) or Unix (LF) line endings.
- **Audio and video** (with ffmpeg): MP4 (H.264 + AAC), Smaller video, MP3, M4A, WAV,
  Remux to MP4, GIF from video.
- **PDF**: Images to PDF, Merge PDFs, Split PDF, PDF to images, Extract pages (with pdfium).
- **Your own commands** from `[[commands]]` (see the template's comments): a program and its
  arguments with placeholders like `{in}` and `{out}`, no shell. A command can have a
  `shortcut`, a `menu` heading and an `ask` question.

When ffmpeg, 7-Zip or pdfium is needed and not found, Gezik offers to download it (checked
against a fixed SHA-256) or uses the one on PATH.

```toml
[convert]
ffmpeg = ""               # path to ffmpeg; empty = Gezik's download, then PATH

[tools]
download = true           # offer to download 7-Zip, ffmpeg and pdfium when needed
seven-zip = ""            # path to 7-Zip; empty = Gezik's download, then PATH
```

### Filter and selection

- **Filter** (Ctrl+F or `/`) narrows the folder as you type. Parts split by `;`, `*` and `?`
  as wildcards (a part without them matches anywhere in the name), `!` in front leaves out:
  `*.jpg;*.png;!*draft*`. Save filters from the bar's ▾ button (`[[filters]]`).
- **Select / deselect by pattern** (Ctrl+= / Ctrl+-, or keypad + / −), **invert selection**
  (Ctrl+Shift+I), **select the same type** (Alt+keypad +) and **restore selection** (keypad /:
  the selection before the last file operation).

```toml
[keyboard]
typing = "jump"           # jump: letters go to a name | filter: letters open the filter
```

### Tabs, address bar and terminal

- Tabs come back as you left them (`[session] restore`). Ctrl+1…8 and Ctrl+9 pick a tab,
  Ctrl+Shift+T reopens a closed one, Ctrl+Shift+A lists them. A **locked** tab does not close.
- **Tab sets**: save the open tabs under a name and open them again from a tab's right-click
  menu (`[[tab-sets]]`); with two panes both panes' tabs are saved (`right = [...]`).
- The address bar (Ctrl+L) completes folder names and lists **Recent** and **Frequent**
  folders.
- **Pinned folders** can have an alias and a group; Alt+1…9 (⌘⌥1…9 on macOS) go to the first nine.
- **Open terminal** (Shift+F4 or Ctrl+Alt+T, ⌘⌥T on macOS) opens one in the folder; on
  Windows also as administrator. **Copy path** (Ctrl+Shift+C, ⌘⌥C on macOS) copies the
  selected paths; **Copy path as ▸** has quoted, forward slashes, name, folder, `file://` and
  UNC forms.

```toml
[history]
remember = true           # false also forgets the folders the address bar remembers

[session]
restore = true            # open the tabs of last time at start

[terminal]
# command = ["wezterm", "start", "--cwd", "{dir}"]   # unset: Gezik finds one
```

### Search

- **Search** (Ctrl+Shift+F or Ctrl+E, ⌘⇧F on macOS) searches under the folder shown, with the
  filter's pattern language. **Content** finds text in files; **Filters ▾** adds size, date,
  type, regex, match case and hidden items. Results fill a normal list: sort, filter,
  preview, copy, rename and batch rename work there. Back returns to the folder.
- On Windows, names come from Everything (voidtools) when it runs.
- **Show in folder** (Ctrl+Shift+E) opens a result's folder with it selected; **Copy with
  folders** keeps the folder structure under the search's folder.
- **Flat view** (Ctrl+B) shows every file under the folder in one list; again goes back.
- **Saved searches** (▾ menu › Save search…) live in `[[searches]]` and show in the sidebar.
- **Command palette** (Ctrl+Shift+P) runs any action, command or view option by name.
  **Quick open** (Ctrl+P) goes to pinned and recent folders, tabs, tab sets and saved
  searches; type `>` for commands.

```toml
[search]
everything = "auto"        # auto | off (Windows: Gezik's own scanner always)
skip = [".git", "node_modules"]   # folders a search does not go into (by name)
max-results = 250000       # 1000-2000000; a search stops there
content-max-size = "64 MB" # larger files are not read for text
```

### System integration

- **Command line**: `gezik [PATH...]` opens folders as tabs; a file opens its folder with it
  selected. Options: `--new-tab`, `--new-window`, `--select PATH`, `--unregister`,
  `--version`, `--help`.
- **Single instance**: a second `gezik` opens in the running window (in a tab already showing
  that folder, or a new one). **New window** (Ctrl+N, ⌘N on macOS) starts a separate Gezik.
- **Trash view**: the Recycle Bin / Trash row in the sidebar shows every bin in one list,
  with original location and date deleted. **Put Back**, **Delete Permanently** and **Empty
  Trash**. On macOS, Gezik needs Full Disk Access to show the Trash.
- **System Integration…** (View menu, or the command palette) adds `gezik` to PATH so it runs
  from a terminal (Windows: a small `gezik.cmd` and App Paths, under your user only;
  macOS/Linux: a `~/.local/bin/gezik` link). Every change is written to
  `system-changes.toml` first; **Undo all**, or `gezik --unregister`, takes them back.
  Nothing in the system changes unless you ask.
- **Get Info** (Alt+Enter, ⌘I on macOS): on Windows the system's Properties; on macOS and
  Linux Gezik's Info window with owner, group and permissions (each change is one Ctrl+Z,
  **Apply to enclosed items…** for folders). On macOS also the Hidden and Locked flags and
  the app that opens the file (**Change All…**).
- **macOS**: Finder's icons, localized names and Quick Look thumbnails; aliases open their
  original (**Make Alias**, ⌃⌘A); apps and other packages open instead of being entered
  (**Show Package Contents**); **Open With ▸**, **Share…** and **Quick Actions ▸** in the row
  menu. On Windows the row menu is Explorer's, with Gezik's items on top.

```toml
[system]
single-instance = true    # a second gezik opens in the running window; read at start
quick-look = "system"     # macOS: Space opens the system Quick Look panel, or "gezik"
```

### Keyboard shortcuts

| Action | Windows / Linux | macOS |
|---|---|---|
| New tab / close tab | Ctrl+T / Ctrl+W | ⌘T / ⌘W |
| New window | Ctrl+N | ⌘N |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab | same |
| Tab 1…8 / last tab | Ctrl+1…8 / Ctrl+9 | ⌘1…8 / ⌘9 |
| Reopen closed tab / list tabs | Ctrl+Shift+T / Ctrl+Shift+A | ⌘⇧T / ⌘⇧A |
| Back / forward | Alt+← / Alt+→ (or mouse side buttons) | ⌘[ / ⌘] |
| Parent folder | Alt+↑ | ⌘↑ |
| Type a path | Ctrl+L | ⌘L |
| Refresh | F5 or Ctrl+R (Ctrl+R with two panes) | ⌘R |
| Select all | Ctrl+A | ⌘A |
| List view / grid view | Ctrl+Shift+1 / Ctrl+Shift+2 | ⌘⇧1 / ⌘⇧2 |
| Show or hide the preview pane | Alt+P | Alt+P |
| Quick look (only while the file list has focus) | Space | Space |
| Show hidden items | Ctrl+H | ⌘⇧. |
| Filter | Ctrl+F or / | ⌘F or / |
| Search / flat view | Ctrl+Shift+F or Ctrl+E / Ctrl+B | ⌘⇧F / ⌘B |
| Two panes / switch to the other pane | F3 / Tab (file list) | ⌃⌘P / Tab (file list) |
| Copy / move to the other pane (two panes; asks first) | F5 / F6 | F5 / F6 |
| Swap the panes' tabs | Ctrl+U | ⌃⌘U |
| Command palette / quick open | Ctrl+Shift+P / Ctrl+P | ⌘⇧P / ⌘P |
| Copy / cut / paste | Ctrl+C / Ctrl+X / Ctrl+V | ⌘C / ⌘X / ⌘V (⌘⌥V moves) |
| Delete / delete permanently | Delete / Shift+Delete | ⌘⌫ / ⌘⌥⌫ |
| Rename | F2 | Enter |
| New folder / new folder with selection | Ctrl+Shift+N / Ctrl+Alt+N | ⌘⇧N / ⌃⌘N |
| Duplicate | (menu) | ⌘D |
| Undo / redo | Ctrl+Z / Ctrl+Y | ⌘Z / ⌘⇧Z |
| Open terminal | Shift+F4 or Ctrl+Alt+T | ⌘⌥T |
| Copy path | Ctrl+Shift+C | ⌘⌥C |
| Get Info (Windows: Properties) | Alt+Enter | ⌘I |
| Empty Trash | (menu) | ⌘⇧⌫ |

**What changed:** F3 now opens a second pane. Search is Ctrl+Shift+F or Ctrl+E. With two
panes F5 copies and F6 moves to the other pane; with one, F5 refreshes as before.

Change them in `settings.toml` under `[shortcuts]` (`"mod"` is ⌘ on macOS and Ctrl
elsewhere, `""` disables one, a list gives several keys). The template lists every action,
including those with no key by default (`show-trash`, `system-integration`,
`calculate-folder-sizes`, `toggle-stack` and more). Ctrl+wheel in a folder changes the grid
or icon size.

The file list keys are fixed: arrow keys, PgUp/PgDn, Home/End move the focus; Shift with
them extends the selection; Ctrl with the arrows moves the focus without selecting;
Ctrl+Space toggles the focused item; Enter opens the selection (on macOS Enter renames and ⌘↓ opens); Esc clears it. Type a
name's first letters to jump to it. In quick look, the arrows move through the folder
and Space or Esc closes it.

### Writing a theme

A theme only needs the values it changes; the rest comes from its `base` (`dark`, `light`,
`classic-dark`, `classic-light` or another theme's file name):

```toml
# themes/sunset.toml  ->  select it with  theme = "sunset"  in settings.toml
base = "dark"

[colors]
accent = "#ff8a3d"
icon-folder = "#ffb347"
```

See `themes/example.toml` for every color and size you can set. Colors you leave out are
worked out from the ones you set: an accent alone also recolors the selection, focus ring,
progress bars and so on. `icon-folder`, `icon-image`, `icon-video`, `icon-audio`,
`icon-archive`, `icon-document`, `icon-code` and `icon-other` color Gezik's own icons,
`focus-ring` is the keyboard focus in the file list and `marquee` is the rubber-band
selection (it can be translucent, e.g. `"#88c0d033"`). `inset` (0-16) is the gap around
the file list; the classic themes use 0. Older themes that set `folder-icon` or `file-icon`
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
