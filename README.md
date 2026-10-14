<div align="center">

# Gezik

**A fast, lightweight, cross-platform file manager written in Rust with [Slint](https://slint.dev).**

Windows · macOS · Linux

[![CI](https://github.com/wenlar/gezik/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/wenlar/gezik/actions/workflows/ci.yml)

[Features](docs/features.md) · [Configuration](docs/configuration.md) · [Shortcuts](docs/shortcuts.md) · [Themes](docs/themes.md) · [Performance](docs/performance.md) · [License](#license)

</div>

<br>

| Startup to window | Idle memory | 100,000-file folder | 20 tabs vs 1 |
| :---: | :---: | :---: | :---: |
| **32–38 ms** | **5.1 MB** | **13.1 MB** | **+0.4 MB** |

<sub>Release build, Windows 11, default 900×600 window. Method and full results in [Performance](docs/performance.md).</sub>

## Goals

- Instant startup and low memory use
- The UI thread never waits on the file system
- Windows, macOS and Linux from one codebase

## Features

**Files.** Copy, cut and paste through the system clipboard, drag and drop to and from other programs (folders spring open as you rest on them), conflicts asked up front, undo and redo for the whole session, and a drop stack for collecting items from several folders. → [File operations](docs/features.md#file-operations)

**Batch work.** Batch rename with rules and a live preview, archives (open them as folders and change what is inside, extract and compress zip, 7z, tar and more), and conversion of pictures, text, audio, video and PDF, plus your own commands. → [Batch rename](docs/features.md#batch-rename) · [Archives](docs/features.md#archives) · [Inside archives](docs/features.md#inside-archives) · [Convert and PDF](docs/features.md#convert-and-pdf)

**Preview.** A preview pane and Quick Look (Space) that show code in color with line numbers, large logs from end to end, any file as hex, PDF pages with zoom, play video and audio (with ffmpeg, downloaded on request), and a file's properties: camera and exposure, artist, album and length, pages and author, read by Gezik itself. The same properties are list columns to sort, group and filter by (`artist:tarkan duration>3m`), a search criterion, and fields of batch rename templates (`{track:02} {artist} - {title}`). Nothing is read until something asks for it. → [Preview and properties](docs/features.md#preview-and-properties)

**Finding things.** A filter that narrows the folder as you type, search under any folder by name or content (Everything on Windows), flat view, saved searches, a command palette and quick open. → [Filter](docs/features.md#filter-and-selection) · [Search](docs/features.md#search)

**Navigation.** Tabs that come back as you left them, tab sets, pinned folders with aliases, groups, colours and icons, and an address bar with Recent and Frequent folders that also runs a command typed after `>` in the folder shown (its output in a panel, no terminal needed). → [Tabs, address bar and terminal](docs/features.md#tabs-address-bar-and-terminal) · [Commands in the address bar](docs/features.md#commands-in-the-address-bar)

**Layouts.** Two panes with F5/F6 copy and move and synced browsing, a folder tree in the sidebar, grouping by type, date or size, Miller columns, several windows with tabs you can tear off, and view rules that pick a folder's view by path, kind or content. → [Layouts](docs/features.md#layouts)

**Tags.** Color tags, star ratings and comments kept in the files (Finder and Dolphin read them), tag dots and columns, `tag:`/`rating:` filters and search, a TAGS sidebar section, and highlight rules that color rows. → [Tags](docs/features.md#tags)

**Phones.** Android phones show in the sidebar when plugged in (the system's device event, no polling; libmtp on macOS and Linux, loaded only when a phone is opened): browse their storages, copy files off and onto them, make folders, rename and delete, with progress, cancel and undo for what Gezik put there. → [Phones](docs/features.md#phones)

**Git.** In a repository, rows show their Git mark (modified, staged, untracked, conflict; ignored ones faint) and folders the most important one inside, with a Git column to sort and group by, `git:` filter words and the branch with ahead/behind in the status bar. Stage, unstage, commit, push, pull, fetch, switch branch, discard changes (to the trash first, one undo) and compare with HEAD from the row's menu or the status bar, with git's output in the command panel. The system's git runs only while a repository folder is shown or when you choose a command, and a repository's own settings never run a program unless git itself would (a commit's hooks). → [Git](docs/features.md#git)

**Languages.** English and Turkish, plus machine-translated German, Spanish, French and Simplified Chinese; Gezik follows the system's language unless you pick one. A new language is a plain `.po` file in the settings folder. → [Translating](docs/translating.md)

**The system.** A command line, a single instance, one Trash view for every bin, and PATH integration you can undo. → [System integration](docs/features.md#system-integration)

**Tools.** Checksums and their verification, file and folder compare, a sync (two-way, left to right or mirror) that asks first and deletes only to the trash (it is stateless: a delete spreads only with mirror or a row turned to `×`), duplicates, a size map and list export; Copy To with a filter, split and join; copies that can be verified, speed-limited and reordered in the queue. → [Tools](docs/features.md#tools) · [Compare and sync](docs/features.md#tabs-address-bar-and-terminal) · [Copy options](docs/features.md#file-operations) · [Export list](docs/features.md#export-list)

## Configuration

Gezik keeps its settings in a plain-text folder. Changes apply as soon as you save, with no restart. **Settings…** (Ctrl+, , macOS ⌘,) shows the same file in a window: pages, a search, each setting with its comment; a change is written at once and only its line in `settings.toml` changes.

| OS | Location |
|---|---|
| Windows | `%APPDATA%\gezik` |
| macOS | `~/Library/Application Support/gezik` |
| Linux | `~/.config/gezik` |

```toml
# settings.toml
theme = "auto"            # follows the system light/dark mode

[layout]
sidebar = "left"          # left | right | hidden
density = "comfortable"   # compact | comfortable
```

A theme only needs the values it changes. An accent alone also recolors the selection, focus ring and progress bars.

```toml
# themes/sunset.toml
base = "dark"

[colors]
accent = "#ff8a3d"
```

Themes from GitHub install from Settings ▸ Appearance ▸ Get Theme: paste a repository or `.toml` address, preview, install. A theme is data only (colors, sizes, a font name); nothing in it runs. [More](docs/themes.md#installing-from-github)

Settings move as one zip: Settings ▸ Sync and Backup ▸ Export Settings, then Import Settings on the other computer. The import shows what changes, warns about settings that run programs, backs up first and can be undone. [More](docs/configuration.md#backup-and-moving)

Portable mode: put an empty file named `gezik-portable` next to Gezik (next to `Gezik.app` on macOS, next to the AppImage on Linux) and everything Gezik keeps goes into `gezik-data/` beside it, so Gezik runs from a USB drive next to an installed copy. [More](docs/configuration.md#portable-mode)

Sync folder: Settings ▸ Sync and Backup ▸ Keep Settings in a Folder puts settings, themes and templates in a OneDrive, Dropbox, iCloud Drive or Syncthing folder shared by your computers; each keeps its own state, views, tools and tray. Changes made on two computers at once are never lost silently: Gezik re-applies its own edit when the file changed under it, notices the sync service's conflicting copy and lets you review it key by key. Without the folder Gezik starts from its last copy, read-only. [More](docs/configuration.md#sync-folder)

Updates: Gezik updates itself from its published GitHub Releases. Once a day at most it sends one request to `api.github.com` (no account, nothing about you or your files); a newer version is downloaded in the background, checked against the release's SHA-256 sums and installed when Gezik closes. Turn it off with `[update] check = false` (Settings ▸ General); **Check for Updates** in the command palette still asks when you choose it. Where Gezik can't replace its own files (Program Files, `/usr`) it only says that a new version is out. [More](docs/configuration.md#updates)

→ [Configuration](docs/configuration.md) · [Writing a theme](docs/themes.md) · [Keyboard shortcuts](docs/shortcuts.md)

## Build

```sh
cargo run --release -p gezik
cargo test
```

<details>
<summary><b>Project layout</b></summary>

<br>

| Crate | Contents |
|---|---|
| `crates/gezik-core` | platform-independent logic (listing, sorting, formatting, rename rules, patterns) |
| `crates/gezik-config` | settings, shortcuts and themes (no UI dependency) |
| `crates/gezik-ops` | the file operations engine (queue, conflicts, progress, undo) |
| `crates/gezik-batch` | batch operations (rename rules, archives, conversion, PDF) |
| `crates/gezik-search` | searching folder trees (scanner, name and content matching, Everything) |
| `crates/gezik-vfs` | places that are no folder on disk: inside archives (index, taking entries out, copying out, writing back) |
| `crates/gezik-platform` | the parts that differ per OS (drives, icons, clipboard, trash, menus) |
| `crates/gezik-i18n` | translations (`.po` catalogues, plural rules, the language in use; `gezik-lang` for translators) |
| `crates/gezik` | the desktop app (Slint UI) |

</details>

## License

> [!NOTE]
> Gezik is source-available and free for non-commercial use.

Gezik is licensed under the [PolyForm Noncommercial License 1.0.0](LICENSE.md).
You may use, modify and share it for any non-commercial purpose, including personal
use, research, education and use by non-profit organizations. Commercial use is not
permitted. When you share Gezik or a modified version, keep the license and the
`Required Notice` line from `LICENSE.md`.

Copyright Wenlar LLC.

Gezik's user interface is built with Slint, used under the
[Slint Royalty-free License](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md).

<a href="https://slint.dev"><img src="https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-whitebg.png" alt="Made with Slint" height="60"></a>
