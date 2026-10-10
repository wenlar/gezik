# Features

- [File operations](#file-operations)
- [Batch rename](#batch-rename)
- [Archives](#archives)
- [Convert and PDF](#convert-and-pdf)
- [Filter and selection](#filter-and-selection)
- [Tabs, address bar and terminal](#tabs-address-bar-and-terminal)
- [Search](#search)
- [System integration](#system-integration)

## File operations

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

## Batch rename

Rename with two or more items selected (or the `batch-rename` shortcut) opens a layer with
rules on the left and a live preview on the right:

- **Replace** (plain or regex), **Number**, **Case**, **Add text**, **Extension**,
  **Template** and **Trim / clean**, applied in order. Each can be turned off for a moment.
- Template fields: `{name}`, `{ext}`, `{n:03}`, `{parent}`, `{date:%Y-%m-%d}`, `{taken:…}`
  (the photo's EXIF date) and `{size}`.
- Duplicate names are shown before anything changes; the whole rename is one Ctrl+Z.
- **Presets ▾ › Save current rules as…** keeps a rule set in `settings.toml` (`[[rename-presets]]`).

## Archives

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

## Convert and PDF

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

## Filter and selection

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

## Tabs, address bar and terminal

- Tabs come back as you left them (`[session] restore`). Ctrl+1…8 and Ctrl+9 pick a tab,
  Ctrl+Shift+T reopens a closed one, Ctrl+Shift+A lists them. A **locked** tab does not close.
- **Tab sets**: save the open tabs under a name and open them again from a tab's right-click
  menu (`[[tab-sets]]`); with two panes both panes' tabs are saved (`right = [...]`).
- The address bar (Ctrl+L) completes folder names and lists **Recent** and **Frequent**
  folders.
- **Pinned folders** can have an alias and a group; Alt+1…9 (⌘⌥1…9 on macOS) go to the first nine.
- **Columns view** (Ctrl+Shift+3, ⌃⌘3 on macOS, or View ▸ Columns) shows folders side by side
  as Miller columns: ↑↓ move in a column, → goes into the selected folder, ← goes back. The
  focused column's folder is the tab's location; moving between columns adds no Back step. A
  selected file is previewed in a last column (unless the preview pane is open). Drag files
  onto any column's rows or empty space; drag a column's edge to set every column's width.
  Search results, the flat view, the Trash and This PC show the list instead.
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

## Search

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

## System integration

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

---

[← Back to README](../README.md)
