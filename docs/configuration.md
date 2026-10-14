# Configuration


Gezik keeps its settings in a plain-text folder:

| OS | Location |
|---|---|
| Windows | `%APPDATA%\gezik` |
| macOS | `~/Library/Application Support/gezik` |
| Linux | `~/.config/gezik` |

Set the `GEZIK_CONFIG_DIR` environment variable to use another folder; a `gezik-portable` file
next to Gezik keeps everything next to it instead (see [Portable mode](#portable-mode)).

- `settings.toml` — your preferences. Portable: copy it to another computer, on any OS. Every
  key is listed there with a comment.
- `themes/*.toml` — your themes. `themes/example.toml` is a commented starting point.
- `templates/` — files and folders offered under **New ▸** (see [File operations](features.md#file-operations)).
- `state.toml` — window size and position, column widths, the preview pane, the windows,
  panes and tabs of the last session, the folders you visited and the commands you ran in the
  address bar (`[command-line] history`, the last 100) on this machine. Not meant to be copied.
- `views.toml` — the view (list, grid or columns, sort, grouping, size) of each folder you
  changed. Local to this machine, like `state.toml`; at most 500 folders are remembered.
- `pending-deletes` — permanent deletes not finished yet (Gezik finishes them on start).
  Local to this machine; not meant to be copied.
- `system-changes.toml` — the changes Gezik made to the system on your command (see
  [System integration](features.md#system-integration)). Written only by Gezik. In portable
  mode one per computer: `system-changes-<computer id>.toml`, named by a short hash of the
  computer's id, with the computer's name inside.
- `tools/` — programs Gezik downloaded on your command (7-Zip, ffmpeg, pdfium). In portable mode
  one folder per system, `tools/<os>-<arch>/` (say `windows-x86_64`), so one drive serves three.
- `update/` — a downloaded update until it is installed (see [Updates](#updates)).
- `lang/*.po` — interface languages you add or fix (see [Translating Gezik](translating.md)):
  `lang/pl.po` adds Polish, `lang/tr.po` corrects the built-in Turkish. Gezik reloads one when
  it is saved and lists its problems (with line numbers) in the status bar's notices.
- `location.toml`, `settings.local.toml`, `sync-cache/` — only with a sync folder (see
  [Sync folder](#sync-folder)): where it is, this computer's own values, and its last copy.

The top of `settings.toml` holds:

- `theme` — `"auto"` follows the system light/dark mode (with `theme-light` and `theme-dark`),
  or name a theme.
- `language` — the interface language: `"auto"` (default) follows the system (Windows' display
  language, macOS' preferred languages, `LANGUAGE`/`LC_ALL`/`LC_MESSAGES`/`LANG` on Linux), or
  a code: `"en"`, a built-in language, or a file in `lang/` (`lang/pl.po` → `"pl"`). Texts a
  language lacks show in English. `"pseudo"` shows every text framed and lengthened
  (`«Ŝéţţıñĝš ···»`), for translators. A change applies at once, without a restart; the
  system's language is read when Gezik starts. File contents, settings keys and values,
  shortcut names in this file and the command line stay English.
- `start-folder` — where new tabs open: `"{home}"`, `"drives"` (This PC) or any folder.
- `max-fps` — most frames drawn per second, e.g. while scrolling (default 120; 0 = as many as
  the display refreshes). The software renderer redraws the visible text every frame, so on
  high refresh rate displays a cap saves a lot of CPU.
- `[command-line] shell` — the shell a `>` command in the address bar runs with: `""`
  (default) is cmd on Windows and `$SHELL` (else `/bin/sh`) on macOS and Linux; `"powershell"`
  or `"pwsh"`; or a full path, which gets `shell-args` (say `["-c"]`) and then the command.
  See [Commands in the address bar](features.md#commands-in-the-address-bar).
- `[git]` — Git marks in repositories (see [Git](features.md#git)): `status` (default
  `true`; `false` and git never runs), `program` (a full path to git or a name looked for on
  PATH; empty = `git` on PATH; on macOS `/usr/bin/git` only with the Command Line Tools, then
  Homebrew's; git 2.31 or newer), `ignored` (default
  `true`: ignored files are marked and drawn faint), `network` (default `false`: repositories
  on network drives are not looked for) and `timeout` (seconds a status may take, 1-600,
  default 10; also the longest Gezik waits for git to read a branch list or a file at
  HEAD for the Git commands).
- `[devices]` — `phones` (default `true`): Android phones and other portable devices (MTP)
  show in the sidebar when plugged in (see [Phones](features.md#phones)); `false` and
  Gezik does not look for them. `libmtp` (default empty; macOS and Linux): the full path of
  the libmtp to load; empty uses Homebrew's or Gezik's download on macOS
  (`/opt/homebrew/lib/libmtp.9.dylib`, `/usr/local/lib/libmtp.9.dylib`) and the system's
  `libmtp.so.9` on Linux.
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
Advanced, Sync and Backup), then Open settings.toml and Open Config Folder; the search box above the rows looks
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
  **Reset to Default** writes the default keys in place (a default key another action has now
  asks Replace? the same way). Keys that type in the file list (a
  letter alone), the file list's own keys and the Windows key are refused, with the reason at
  the bottom. `edit-shortcuts` (Keyboard Shortcuts… in the palette) opens this page.
- Other lists and tables (`pinned`, `[[commands]]`, rules …) show how many entries
  they have and are edited in the file. The tray icon and the global shortcut are changed in
  System Integration.
- If `settings.toml` does not read, the line at the bottom says which line, and the rows are
  read-only until it is fixed. Changes made in the file while the window is open show at once.
  Get Theme and the Sync and Backup page still work then (Restore Backup is how to get back).

## Backup and moving

Settings ▸ **Sync and Backup** (or **Export Settings…** / **Import Settings…** in the palette)
moves your settings to another computer, or to the same one after a reinstall.

- **Export Settings** writes one zip: `settings.toml`, the themes, the New ▸ templates (box
  **Export Templates**, on; at most 256 MB, 64 MB a file) and `views.toml` (box **Export Folder
  Views**, off: its paths are this computer's), with `gezik-export.toml` (format, version, OS,
  date). The path is a field: the default is `Documents/gezik-settings-<date>.zip`; edit it and
  press Enter. An existing file is not overwritten. `state.toml`, the tag store, the system
  changes, downloaded tools and the backups never go in.
- **Import Settings**: type or paste the zip's path and press Enter. Nothing is written yet: the
  rows show each kind of file (settings.toml with a summary such as "3 settings change: theme:
  dark → nord, …", the themes and templates as new or replaces) with a box to leave it out, and
  then **Import** or **Cancel**. Settings in the zip that run programs on this computer
  (`[[commands]]`, `[terminal] command`, the compare program, 7-Zip and ffmpeg paths,
  `[[sync-profiles]]`) are listed by name in a red row when they are new or differ from yours.
  Nothing runs while importing; import only files you trust.
- An import never merges: each file chosen replaces the one here. Before it writes, Gezik backs
  up the files it is about to touch (settings.toml, the themes, and the templates or views when
  those come in) to `backups/gezik-settings-<time>.zip` in the config folder; the last 10 are
  kept and older ones go to the trash. **Undo Import** puts back, byte for byte, what the last
  import replaced and removes what it added. **Restore Backup** lists the backups: choose one to
  see what it would put back, then Import.
- Limits: the zip at most 300 MB and 4,096 files, 64 MB each; only `settings.toml`, `views.toml`,
  `themes/<name>.toml` and `templates/…` come in. Names with `..`, absolute paths, drives, links
  and names Windows cannot hold are left out (the Import row lists them). A zip whose
  `settings.toml` or `views.toml` does not read is refused with the line. Paths written with a
  token (`{home}`, `{documents}`…) work on the other system; others may not exist there.

## Sync folder

Settings ▸ **Sync and Backup** ▸ **Keep Settings in a Folder**: type a folder in OneDrive,
Dropbox, iCloud Drive, Syncthing or on a network drive (say `~/Dropbox/Gezik`) and press Enter.
`settings.toml`, the themes and the templates are then kept there for every computer that uses
the folder; each computer keeps its own `state.toml`, `views.toml`, tag store, tools, backups and
system changes.

- An empty folder gets this computer's files (nothing there is written over). A folder another
  computer set up first asks: **Use the Settings in This Folder** (this computer's are backed up
  to `backups/`) or **Replace Them with This Computer's** (the folder's are backed up to
  `backups/` first). Nothing is merged. Templates are never written over in either direction.
- Where the folder is lives in `location.toml` in this computer's config folder, written with a
  token (`shared = "{home}/Dropbox/Gezik"`), so the same Dropbox found elsewhere on another
  system still works. The folder's `machines/` holds an empty file per computer that set it up
  (OneDrive names its conflicting copies by computer).
- **This computer only**: `settings.local.toml` in this computer's config folder (same format as
  `settings.toml`) is read over the shared file: its keys win, tables key by key, lists whole.
  Right-click a setting in the window ▸ **Only on This Computer** (the row then says "this
  computer"; **Use the Shared Value** takes it out again). `[system]` (the tray, the global
  shortcut) always goes there while a sync folder is used, and this computer's `[system]` is
  copied there when the folder is set up. Without a sync folder the file is not read.
- **Two computers at once**: Gezik writes a setting key by key; if `settings.toml` changed between
  its read and its write (another computer's change synced in), it makes the edit again on the
  new text, up to three times, then keeps the change and writes it at the next reload. A change
  that waits (the folder away, the file changing) is made again on the file as it is when it is
  written: lists take only the items you added, removed or moved, so another computer's new pins
  stay; a key both computers changed keeps the other's value and yours goes to `settings (Gezik
  conflicted copy).toml` for a Review. Waiting changes are kept in `pending-settings.toml` in this
  computer's folder, so quitting meanwhile loses none. When a
  sync service still made a conflicting copy (Dropbox and Nextcloud `settings (… conflicted copy
  …).toml`, Google Drive `settings (1).toml`, OneDrive `settings-<COMPUTER>.toml`, Syncthing
  `settings.sync-conflict-….toml`, iCloud `settings 2.toml`; for themes only the services'
  marked names, so a theme of yours named `Dracula 2.toml` stays a theme), the status bar
  says so and the page lists it with **Review…**: a row per key the copies differ in (a list such
  as `[[commands]]` is one row), **This copy** or **Other copy** each, then **Merge** (the keys
  chosen are written, the copy goes to the trash) or **Cancel**. Nothing is merged on its own.
  A sync service that makes no copy (iCloud at times: the last writer wins) can still lose one
  of two changes made at the same moment.
- **Folder not available** (the drive not plugged in, Dropbox not running): Gezik starts from the
  last copy in `sync-cache/`, the status bar says "Settings folder not available — using the last
  copy", and the settings window is read-only. Changes made meanwhile (a pin) are kept and
  written when the folder is back, even after a restart. Gezik notices the
  folder coming back while its parent folder is there; otherwise **Check Again**. Nothing polls.
- **Stop Keeping Settings There** copies the folder's files (or its last copy) back into this
  computer's config folder after a backup, with this computer's `settings.local.toml` values
  written into it, and removes `location.toml`; the folder is left as it
  is. Export and Import (above) work with a sync folder too: the shared files come from the
  folder, `views.toml` and the backups from this computer.

## Portable mode

Put a file named `gezik-portable` (empty; its contents do not matter) next to Gezik:

| OS | Next to |
|---|---|
| Windows | `gezik.exe` |
| macOS | `Gezik.app` (not inside it: that would break its signature) |
| Linux | the `gezik` program, or the `.AppImage` file |

Gezik then keeps everything (settings, themes, templates, state, views, tags, backups,
downloaded tools, the system changes) in `gezik-data/` next to that file, made on the first start
with the starter files. Nothing goes to the folder in the table above, and the portable Gezik
runs as a separate instance next to an installed one. Settings ▸ Sync and Backup shows
"Portable: settings are kept in …" with **Open Data Folder**.

- `GEZIK_CONFIG_DIR` still comes first.
- To move your current settings in: Export Settings in the installed Gezik, Import Settings in
  the portable one (see [Backup and moving](#backup-and-moving)). To stop: delete
  `gezik-portable`; `gezik-data/` stays.
- System integration (default file manager, PATH, the tray at sign-in, the global shortcut)
  still works, but each question first says that it stops working when the drive is removed.
  Its list of changes is kept per computer (`system-changes-<computer id>.toml`; the id stays when
  the computer is renamed or gets another network name), so a drive
  used on several computers never undoes another computer's changes.
- If `gezik-data/` cannot be written (a read-only drive or CD, `Program Files`), Gezik still
  opens with its settings, writes nothing, and the status bar says "Portable data folder is
  read-only: changes are not saved". If `gezik-data/` is not there and cannot be made, Gezik uses
  the usual folder and says so.
- macOS: a downloaded `Gezik.app` started from where it was unpacked may be run from a hidden,
  read-only copy (App Translocation), where the marker cannot be seen; Gezik then uses the usual
  folder and says "Move Gezik.app out of Downloads (or open it once from Finder after moving) to
  use portable mode".

## Updates

Gezik updates itself from its [GitHub Releases](https://github.com/wenlar/gezik/releases):

```toml
[update]
check = true              # false: Gezik never asks by itself
channel = "auto"          # auto | stable | pre
```

- **One request a day.** At most once every 20 hours, 10 seconds after start, Gezik asks
  `api.github.com` for the list of published releases (no account, nothing about you or your
  files is sent; GitHub sees your IP address and `Gezik` as the program). The time of the last
  check is kept in `state.toml`. `check = false` turns it off; **Check for Updates** (command
  palette, macOS Help menu, or Settings ▸ General ▸ Updates ▸ Check Now) still asks when you
  choose it.
- **Channel.** `auto` takes pre-releases (`0.2.0-rc.1`) while you run one, otherwise only
  releases; `stable` only releases; `pre` both. Gezik never goes to a lower version.
- **Download and install.** A newer version is downloaded in the background into `update/` in
  the config folder (`gezik-data/update/` in portable mode), checked against the release's
  `SHA256SUMS.txt`, unpacked and started once with `--version`. The status bar then says
  "Gezik 0.2.0 is ready — installs when you close it · Restart now · What's new" (once). When
  Gezik closes (with the tray icon on: when you quit it), the old program is renamed to
  `.old`, the new one put in its place, and the next start says "Updated to 0.2.0" and removes
  the `.old`. A download or install cut short leaves the old Gezik as it was.
- **Where Gezik can't write** (Program Files, `/usr`, `/opt`, macOS run from the disk image
  or from a quarantined copy in Downloads), nothing is downloaded: the status bar only says
  "Gezik 0.2.0 is out · Download".
- **Which Gezik updates itself:** the packages from the releases page. A Gezik built from the
  source, or installed by a package manager, only says that a new version is out.
- **What is checked.** The SHA-256 shows that the file is the one on the release; it does not
  show who made the release (Gezik has no signature of its own yet).

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
