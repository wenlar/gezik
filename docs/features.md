# Features

- [File operations](#file-operations)
- [Batch rename](#batch-rename)
- [Archives](#archives)
- [Convert and PDF](#convert-and-pdf)
- [Filter and selection](#filter-and-selection)
- [Tabs, address bar and terminal](#tabs-address-bar-and-terminal)
- [Layouts](#layouts)
- [Search](#search)
- [Tools](#tools)
- [Export list](#export-list)
- [Tags](#tags)
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
- **Copy To… / Move To…** (palette, row menu; F5/F6 with two panes ask the same): type the
  target folder, end it with a name mask to rename (`D:\Yedek\*.bak`, `*_old.*`), and with
  **Filter…** take only some files: `*.jpg *.png | thumbs\ *.tmp` (after `|`: left out; a
  name ending in `\` or `/` is a folder), `*.jpg>*.cr2` brings each `.jpg`'s `.cr2` along.
  No empty folders are made; a filtered move leaves the rest where it is.
- **Split File…** cuts a file into `name.001`, `name.002`… and a `name.crc` (Total
  Commander's); **Join Files** on any part puts it back together and checks its CRC before
  anything is placed (a damaged or missing part is named, nothing is written).
- **Drop stack**: a strip above the status bar that collects items (Ctrl+Shift+S, or drag
  onto it) from several folders; then **Copy here** or **Move here**. Kept for the session only.
- Operations on the same drive wait for each other; different drives run at the same time.
  Long ones show in a panel above the status bar (fold it into the status bar with the
  arrow), with pause, cancel and, on Windows, progress on the taskbar button. Its
  **History** tab lists what finished this session.
- **Verify copies**: each copied file is read back past the system's cache and compared
  with the original byte by byte; a copy that differs is copied once more, and a move keeps
  its source until the copy checked. Off by default (copies take about twice as long); on
  for all copies in the settings, for one F5/F6 in its question, or for a running job in the
  row's **⋯** menu. Clones (APFS, Btrfs, XFS) share the original's blocks and are not read.
- **Speed limit** per copy or move: in the settings for all, or in the row's **⋯ ▸ Speed
  limit** (1 to 100 MB/s) while it runs.
- **Queue**: a job waiting for its drive moves up or down with **⋯ ▸ Move up / Move down**.

```toml
[files]
confirm-trash = false     # ask before moving to the trash
copy-threads = "auto"     # auto (SSD 6, spinning disk 1, network 4) or 1-16
verify = false            # read each copy back and compare it with the original
max-speed = ""            # "20 MB/s", "500 KB/s"…; empty = no limit
```

## Batch rename

Rename with two or more items selected (or the `batch-rename` shortcut) opens a layer with
rules on the left and a live preview on the right:

- **Replace** (plain or regex), **Number**, **Case**, **Add text**, **Extension**,
  **Template** and **Trim / clean**, applied in order. Each can be turned off for a moment.
- Template fields: `{name}`, `{ext}`, `{n:03}`, `{parent}`, `{date:%Y-%m-%d}`, `{taken:…}`
  (the photo's EXIF date) and `{size}`, and every file property: `{artist}`, `{track:02}`,
  `{year}`, `{camera}`, `{width}x{height}`, `{duration}` (as `3m25s`). A number takes its
  digits, a date its format, a decimal its places; `{artist|Unknown}` is used when a file has
  none (without it the row is marked). **Fields ▾** next to the template lists them. Values
  from files are made safe for names (`/ \ : * ? " < > |` become `_`): a tag cannot move a
  file elsewhere.
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

### Inside archives

- **Double-click or Enter goes into an archive** as into a folder (**Open as Folder** in the
  row menu does so whatever double-click is set to; **Open with System App** opens it with
  its program). Every format above, and anything 7-Zip reads (dmg, wim, vhd, msi, …; 7-Zip is
  offered when it is missing). Typing or pasting a path through an archive works too:
  `C:\Backup\old.zip\docs\2024` (macOS and Linux `/home/x/old.zip/docs`), and so do tab
  sets and restored sessions. Up (Alt+↑ / ⌘↑) from an archive's top goes to its folder with
  the archive selected.
- Inside, sort, filter, group, preview (pictures, PDFs, text, sound and video), Quick Look and
  opening a file with its app work: files up to `[archives] preview-limit` (64 MB) are taken
  out to a temporary folder by themselves; larger ones only when you open them. Folder sizes
  show at once, and the **Packed** column (header menu, only inside archives) shows what each
  file takes packed.
- **Search** (Ctrl+Shift+F) inside an archive looks through its index by name, size, date and
  type (not by content or tags); the results open, preview and copy out like the rows.
- **Copy** (Ctrl+C) and paste into a folder, **Copy To…**, or dragging onto a folder in Gezik
  copies entries out, as one job with conflict questions and one Ctrl+Z. Dragging to another
  program takes the selection out first (up to 256 MB; use Copy To… for more). Copy Path
  gives the address inside the archive. Copy puts only the addresses on the system clipboard:
  pasting into Explorer or Finder gives nothing (drag, or Copy To…, instead).
- An encrypted archive asks for its password once (header-encrypted 7z and rar when it is
  opened, others when a file is first read); it is kept only while a tab shows the archive.
- An archive inside an archive opens the same way (up to 8 deep). tar.gz and solid 7z are read
  through once when opened, the status bar counting the entries; going elsewhere stops it.
- If the archive changes on disk its contents are read again; if it is deleted, the tab goes to
  its folder. Nothing is kept once no tab shows an archive (its index, password and
  temporary files go).
- **Changing an archive** (zip, 7z, tar, tar.gz, tar.bz2, tar.xz): drop or paste files and
  folders into it (from disk or from another archive), **Delete** (asked, and undoable),
  **Rename** (F2), **New folder** (Ctrl+Shift+N), **Cut** and paste, **Move To…**, and Shift
  while dragging moves. Each job writes the archive anew once, next to it, reads the new one
  back and checks it, and only then puts it in place: the old archive goes to the trash, so
  Ctrl+Z brings it back. If anything fails or you cancel, the archive stays exactly as it was.
  Dates, Unix permissions and the password are kept (new entries get the archive's password).
  Archives inside archives are written back through each level. rar, iso, cab, deb, single
  `.gz` files, archives in parts, tar.zst and what only 7-Zip reads stay read-only (the status
  bar says so). On a drive with no trash Gezik asks first, since the old archive cannot be
  kept.
- **Update in Archive**: a file opened from inside an archive and saved by its app is noticed;
  Gezik asks whether to put it back. Leaving the archive with a change not put back asks
  again (Update in Archive, Discard, or Keep a Copy in Downloads); quitting Gezik keeps such a
  change in Downloads.
- A small change to a solid 7z or a tar.gz packs the whole archive again, and every change
  leaves the old archive in the trash (a large archive fills it).

```toml
[archives]
double-click = "browse"   # browse | system | extract-here
preview-limit = 64        # MB: files inside an archive up to this size are extracted for the preview by themselves
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
diff = ""                 # an external compare program; empty = Gezik's own
diff-args = ["{left}", "{right}"]
folder-diff = false       # also hand it two folders
```

## Filter and selection

- **Filter** (Ctrl+F or `/`) narrows the folder as you type. Parts split by `;`, `*` and `?`
  as wildcards (a part without them matches anywhere in the name), `!` in front leaves out:
  `*.jpg;*.png;!*draft*`. Save filters from the bar's ▾ button (`[[filters]]`).
  Property words narrow by what is inside the files, together with the name parts:
  `*.mp3 artist:tarkan duration>3m`, `taken>=2024-05 camera:iphone`, `album:none`,
  `artist="Sezen Aksu"` (`:` holds, `=`, `<`, `<=`, `>`, `>=`, `any`, `none`). The folder's
  files are read once in the background ("Reading properties… 4,000 of 12,000"); until then
  the rows show faint. Folders only, not search results.
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
- **Compare** (Shift+F2, ⌃⌘C on macOS, or right-click ▸ Tools ▸ Compare): two selected
  files, or one selected in each pane, side by side. Changed lines and the changed part of
  each are marked; added and removed lines are tinted; line-ending and encoding differences
  are named in the header but do not count. Ignore whitespace and Ignore case are remembered.
  Alt+↓ / Alt+↑ (⌥↓ / ⌥↑) go to the next / previous change, Ctrl+C copies the line, F5
  reads the files again, Esc closes. Binary files and files over 16 MB are compared byte by
  byte. To use WinMerge, Meld, Beyond Compare or FileMerge instead, set `[tools] diff`
  (`diff-args` defaults to `["{left}", "{right}"]`; a macOS `.app` opens with `open -a`).
- **Compare folders** (the same Compare on two selected folders, one folder in each pane, or
  with two panes and anything else the two folders they show): both trees with their
  subfolders, matched by name (case ignored on Windows and macOS; macOS's decomposed names
  meet USB drives' composed ones), told apart by size and date (2 seconds apart is the same:
  FAT; Ignore 1 h for its daylight-saving shift; Ignore dates) and, with Content, byte by
  byte. The show buttons → = ≠ ← 1× 2× (with their numbers) filter the rows; `=` starts off.
  Filter takes `*.jpg *.cr2 | thumbs\ *.tmp` (after `|` left out; a name ending in `\` or
  `/` is a folder's, never read when left out); Enter or F5 applies it. Links are not
  followed, online-only files are not read. → / ← open and close folders, Enter on two files
  opens their compare, Ctrl+C copies paths, Esc stops a running read, then closes. Show Left
  / Show Right show the row in that pane and Hide puts the compare behind a status-bar chip
  (`Compare: foto (3 ≠)`): click it to come back, ✕ to let it go. The buttons and boxes are
  remembered (`[compare]` in state.toml). With `[tools] folder-diff = true` your compare
  program gets the two folders instead.
- **Synchronize** (in the folder compare): Mode picks Two-way (newer wins, what one side
  lacks is copied over), Left to right (only to the right) or Mirror (the right becomes the
  left; what only the right has goes to its trash). Each row's middle column is what it will
  do: `→` / `←` copy, `×→` / `←×` to the trash on that side, empty nothing. Ctrl+→ / Ctrl+←
  (⌘→ / ⌘← on macOS), Delete (marks a one-sided item for the trash), Space (nothing),
  Shift+Space (the mode's choice) and the Direction button change the selected rows; a
  folder's direction goes to everything under it. Synchronize… (Ctrl+Enter, ⌘↩) asks first
  with the counts and sizes (and Verify copies, from `[files] verify`), then runs one job: copies to the right, to the left, then the
  trash, and Ctrl+Z undoes all of it. A file is replaced only once its new copy is complete
  (the old one goes to the trash); one that changed since the compare is left alone and
  said in the job's Details; nothing goes to the trash if a copy failed. A drive without a
  trash gets no deletions, and replacing files there needs Replace them anyway. Sync is
  stateless: in Two-way a file deleted on one side is copied back from the other; turn that
  row to `×` or use Mirror to spread a delete. Save Profile… keeps the folders, mode, options
  and filter in settings.toml (`[[sync-profiles]]`); the palette's `Sync: <name>` (also with `>`) opens and
  compares one, never synchronizing without the question.
- **Select Differences** (right-click ▸ Tools with two panes, the palette, Tools on the macOS
  menu bar): in each pane, what the other pane's list lacks or has older. Only the two lists
  shown are compared (no subfolders, nothing read).
- **Commands in the address bar**: see [below](#commands-in-the-address-bar).
- **Open terminal** (Shift+F4 or Ctrl+Alt+T, ⌘⌥T on macOS) opens one in the folder; on
  Windows also as administrator. **Copy path** (Ctrl+Shift+C, ⌘⌥C on macOS) copies the
  selected paths; **Copy path as ▸** has quoted, forward slashes, name, folder, `file://` and
  UNC forms.

```toml
[history]
remember = true           # false also forgets the folders and commands the address bar remembers

[session]
restore = true            # open the tabs of last time at start

[terminal]
# command = ["wezterm", "start", "--cwd", "{dir}"]   # unset: Gezik finds one
```

### Commands in the address bar

- Type `>` and a command in the address bar (Ctrl+L, or **Run Command…** in the palette) and
  press **Enter**: it runs in the folder shown, with the system's shell (cmd on Windows,
  `$SHELL` on macOS and Linux; `[command-line] shell` picks PowerShell or another). Only Enter
  runs a command: pasting, choosing one from the list, a restored session or a tab set never
  do. Inside an archive, in This PC or the trash nothing runs ("Commands run in a folder on
  disk").
- Its output fills a panel under the list as it comes (error output in the warning color),
  with the command, the folder, the time it runs and, once it ended, its exit code (green for
  0, red otherwise). **Stop** ends it with everything it started; **Run Again**, **Copy
  Output** and **Close** (Esc on the list once it ended) are there too; drag the panel's top
  edge to size it. Scrolled up, the output stays as it is while more comes ("1,204 new lines ·
  End" under it); End, a click on that bar or scrolling to the bottom brings it up to date. **Command Output** (palette) shows or hides it. Each pane has its own; a
  running command ends with its pane, and when Gezik quits. A program a command started in
  the background (`start notepad`, `code .`, `gimp &`) goes on once the shell has ended.
- The selection goes in as environment variables, the text you type is never changed:
  `GEZIK_FILE` (the focused item), `GEZIK_FILES` (the selected ones) and `GEZIK_DIR`. With
  cmd each path already comes in double quotes and `GEZIK_FILES` is `"a" "b"`, so write them
  bare: `>copy %GEZIK_FILE% D:\backup` (a name with `&` or `|` stays a name; `"%GEZIK_FILE%"`
  would undo that). In PowerShell and the Unix shells the paths come as they are, `GEZIK_FILES`
  one per line: `>echo "$GEZIK_FILES"`, `>$env:GEZIK_FILES -split "`n"`. A selection too long
  for one variable (cmd about 7,000 characters, other Windows shells 30,000, Unix 100 KB)
  leaves `GEZIK_FILES` out: `GEZIK_FILES_LIST` names a temporary file with one path per
  line, removed when the command ends (the panel says so).
- No terminal: programs get no input (one that asks for some ends at once), colors and other
  escape codes are taken out, full-screen programs (vim, top, less) and password prompts (ssh,
  sudo) do not work; use **Open terminal** for those. `TERM=dumb`, `NO_COLOR=1`,
  `PAGER=cat`, `GIT_PAGER=cat` and `GIT_TERMINAL_PROMPT=0` are set. Commands run as you,
  never as administrator. On Windows cmd runs with its console in UTF-8 (`chcp 65001`), so
  `dir` and `echo` keep every letter (ğ, ş, ı too); a program that still writes in the old
  console code page is read in it. At most 2 MB is kept ("Earlier output dropped" above it); a flood of output is shown
  2,000 lines at a time and never holds up the program. cmd cannot work in a
  network folder (`\\server\share`): there Gezik does not start it (PowerShell can).
- ↑ and ↓ after `>` go through the commands run before (newest first, no repeats, the last
  100, kept in `state.toml`); Enter or a click on one only writes it in the field. A command
  typed with a space after `>` is not kept (one holding a password). `[history] remember =
  false` and **Clear Folder History** forget them too.

```toml
[command-line]
shell = ""                # "" = cmd (Windows) / $SHELL; "powershell", "pwsh" or a full path
shell-args = []           # before the command, for a full path
```

## Layouts

- **Two panes** (F3, ⌃⌘P on macOS) put a second file list beside the first, each with its
  own tabs; Tab switches between them and the divider can be dragged. F5 copies and F6 moves
  the selection into the other pane's folder (asking first, where that folder can still be
  changed); Ctrl+U swaps the panes' tabs. **Sync Browsing** makes a step into a sub-folder or
  up in both panes. One pane is the default; the second comes back if you left it open.
- **Folder tree**: the arrow on a folder in the sidebar (places, pinned folders, drives)
  opens it as a tree; the arrow keys move in it. **Show in Sidebar Tree** (View button menu)
  opens the tree down to the folder shown, `tree-follow = true` does it all the time. A
  branch is read only when you open it.
- **Grouping** (View ▸ Group by): by type, date (Today, Yesterday, Earlier this week …) or
  size, in the list and the grid, with headers that fold; **Collapse / Expand All Groups**.
  A folder keeps its grouping like its sort.
- **Columns view** (Ctrl+Shift+3, ⌃⌘3 on macOS, or View ▸ Columns) shows folders side by side
  as Miller columns: ↑↓ move in a column, → goes into the selected folder, ← goes back. The
  focused column's folder is the tab's location; moving between columns adds no Back step. A
  selected file is previewed in a last column (unless the preview pane is open). Drag files
  onto any column's rows or empty space; drag a column's edge to set every column's width.
  Search results, the flat view, the Trash and This PC show the list instead.
- **Windows**: New window (Ctrl+N, ⌘N on macOS) opens another window in the same Gezik. On
  Windows and macOS a tab dragged out of the tab strip becomes a window of its own, or joins
  another window's tabs where you drop it. All windows come back at the next start.
- **View rules** (`[[view-rules]]`): the first rule that matches a folder sets its view (mode,
  sort, grouping, grid size, list columns), by path (`{pictures}/**`), by kind (network, removable,
  cloud, search, …) or by content (`pictures >= 50%`). A view you set yourself wins; the View
  button menu's **Reset to rule** goes back to the rule.
- **Property columns**: the column header's right-click menu ▸ **More Columns…** (also the
  palette and View on the macOS menu bar) lists the file properties (Artist, Album, Duration,
  Date Taken, Camera, Dimensions, Pages…); up to 8 show after Size. Only the rows on screen are
  read, only for the columns shown, in the background. A click on the header sorts by it, View ▸
  **Sort by property…** and **Group by ▸ Artist** (or another text property, or Date Taken by
  year) read the folder once. A property row of the preview has **Show Column**. View rules
  name them too: `columns = ["artist", "album", "track", "duration"]`, `sort = "track"`,
  `group = "album"`.

```toml
[sidebar]
tree-follow = false       # the tree opens down to the folder shown

[panes]
confirm = true            # F5/F6 ask before copying or moving to the other pane

[[view-rules]]
content = "pictures >= 50%"
mode = "grid"

[[view-rules]]
path = "~/Downloads"
sort = "modified"
sort-dir = "desc"
group = "date"
```

## Preview and properties

- **Preview pane** (Alt+P) and **Quick Look** (Space): code in color with line numbers (about
  25 languages), text of any size from end to end, **Show as Hex** for any file, PDF pages
  with zoom (pdfium), pictures zoomed to their real size, video and audio with play, seek and
  volume (ffmpeg); both tools download only when asked.
- **Properties** under the preview and in Get Info: picture size, the photo's date, camera,
  lens and place, length and codecs, music tags and cover, document title, author and pages,
  read by Gezik itself from a bounded part of the file (`[preview] properties = false` hides
  them).
- The same properties as **list columns**, sorting, grouping and filter words (see Layouts and
  Filter), a **search** criterion and **batch rename** fields. Nothing is read until one of
  them asks; only the rows on screen, only the fields asked.

## Search

- **Search** (Ctrl+Shift+F or Ctrl+E, ⌘⇧F on macOS) searches under the folder shown, with the
  filter's pattern language. **Content** finds text in files; **Filters ▾** adds size, date,
  type, regex, match case and hidden items, and **Properties…**: the filter's property words
  (`artist:tarkan duration>3m`, `[[searches]] where = "…"`). Only files with a reader for a
  property are read, after the cheaper criteria. Results fill a normal list: sort, filter,
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

## Tools

Right-click ▸ Tools, the command palette, and on macOS the Tools menu. In short: checksums and
their verification, duplicates and the size map (below); file and folder compare, sync and
Select Differences (under [Tabs, address bar and terminal](#tabs-address-bar-and-terminal));
Copy To… / Move To… with a filter, Split File… / Join Files, copy verification, the speed
limit and the queue (under [File operations](#file-operations)); and the
[list export](#export-list).

- **Calculate Checksums…** shows CRC32, MD5, SHA-1, SHA-256 and SHA-512 of the selected files
  (folders: every file in them), each file read once. A value on the clipboard fills
  **Expected** and the matching row is marked. **Copy** gives `sha256sum` lines, **Save…**
  writes `.sha256`, `.md5`, `.sha1`, `.sha512` or `.sfv` files that `sha256sum -c` and
  `shasum -a 256 -c` accept, **Export CSV…** a table.
- **Verify Checksums** checks the files a `.sha256`, `.md5`, `.sha1`, `.sha512`, `.sfv`,
  `.crc` or `SHA256SUMS` file names: OK, FAILED, MISSING.
- Both run as jobs in the operations panel (pause, cancel); files that are only in the cloud
  are downloaded only if you agree.
- **Find Duplicates** (Tools ▸ on a folder or its empty space, the palette) opens the search bar to look for
  files with the same content in the selected folder or the one shown (Enter starts it; the
  name, size, type and date filters apply). Files are compared by size, then their first
  64 KB, then their whole content; empty files, links, hard links of one file and files only
  in the cloud are left out. The results come in groups, the most space to free first
  (`3 files × 12.4 MB · 24.8 MB could be freed`). The ▾ menu and a row's right-click select
  all but the newest, oldest or first copy in each group, all copies in one folder (one is
  always kept) or all but the focused one. Delete moves them to the trash (one undo); on a
  drive without a trash nothing is deleted. A saved search keeps it as `duplicates = true`.
- **Size Map** shows the selected folder (or the one shown) as tiles sized by what they
  take up. The scan runs in the background and stops when the map closes; F5 reads it again.
  A tile's right-click opens it, shows it in the pane, moves it to the trash or copies its path.

## Export list

Tools ▸ Export list… (right-click a row or the background; the palette's `Export List…`; Tools on the macOS menu bar) writes the list the pane shows, sorted and filtered as it is, to a file or the clipboard. The name's extension picks the format: `.txt` (one name, or with "Full paths in text" one path, per line), `.csv` (the columns shown plus Path, sizes in bytes and ISO dates; on Windows the system's list separator, `;` in Turkish, and a BOM so Excel reads it as UTF-8) or `.html` (one page, light and dark). With a selection, only the selected items go unless you untick the box. Search results and the flat view export their folders too; for the files in subfolders use the flat view first. Saving is a normal operation: a taken name asks, Ctrl+Z (⌘Z) removes the file.

## Tags

- **Tags, ratings and comments** live in the files themselves (NTFS streams on Windows, Finder's and Dolphin's extended attributes on macOS and Linux), so Finder and Dolphin see them; drives that keep none (FAT, exFAT) use Gezik's own store. Copies, moves and undo carry them.
- **Tagging:** Ctrl+Shift+L (⌃⌘L) opens the picker for the selection (tags, stars, `Comment…`); Alt+Shift+1…7 (⌃1…⌃7) toggle the first seven tags of `[tags] list`. Tag dots beside names, Tags, Rating and Comment columns, sorting and grouping by tag or rating.
- **Finding:** `tag:Red`, `rating:4`, `comment:review` in the filter bar and the search bar; the sidebar's TAGS section searches a tag (Spotlight on macOS), takes dropped files and manages the list.
- **Highlight rules:** `[[highlight-rules]]` in settings.toml color rows and make names bold by name pattern, kind, age, size, hidden, tag, rating or comment; the first matching rule wins. Tags are read for them only while a rule asks. Commented examples are in settings.toml.

## System integration

- **Command line**: `gezik [PATH...]` opens folders as tabs; a file opens its folder with it
  selected. Options: `--new-tab`, `--new-window`, `--select PATH`, `--unregister`,
  `--version`, `--help`.
- **Single instance**: a second `gezik` opens in the window used last (in a tab already
  showing that folder, or a new one). **New window** (Ctrl+N, ⌘N on macOS) opens a window in
  the same Gezik; `gezik --new-window [PATH...]` does too (a new process only if the running
  Gezik does not answer, or with `[system] single-instance = false`). A tab moves to another
  window from its menu (**Move to new window**, **Move to window ▸**).
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
