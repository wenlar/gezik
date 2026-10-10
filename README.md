<div align="center">

# Gezik

**A fast, lightweight, cross-platform file manager written in Rust with [Slint](https://slint.dev).**

Windows · macOS · Linux

[Features](docs/features.md) · [Configuration](docs/configuration.md) · [Shortcuts](docs/shortcuts.md) · [Themes](docs/themes.md) · [Performance](docs/performance.md) · [License](#license)

</div>

<br>

| Startup to window | Idle memory | 100,000-file folder | 20 tabs vs 1 |
| :---: | :---: | :---: | :---: |
| **24–35 ms** | **6.8 MB** | **17.2 MB** | **+0.1 MB** |

<sub>Release build, Windows 11. Method and full results in [Performance](docs/performance.md).</sub>

## Goals

- Instant startup and low memory use
- The UI thread never waits on the file system
- Windows, macOS and Linux from one codebase

## Features

**Files.** Copy, cut and paste through the system clipboard, drag and drop to and from other programs, conflicts asked up front, undo and redo for the whole session, and a drop stack for collecting items from several folders. → [File operations](docs/features.md#file-operations)

**Batch work.** Batch rename with rules and a live preview, archives (extract and compress zip, 7z, tar and more), and conversion of pictures, text, audio, video and PDF, plus your own commands. → [Batch rename](docs/features.md#batch-rename) · [Archives](docs/features.md#archives) · [Convert and PDF](docs/features.md#convert-and-pdf)

**Finding things.** A filter that narrows the folder as you type, search under any folder by name or content (Everything on Windows), flat view, saved searches, a command palette and quick open. → [Filter](docs/features.md#filter-and-selection) · [Search](docs/features.md#search)

**Navigation.** Tabs that come back as you left them, tab sets, two panes, pinned folders with aliases and groups, and an address bar with Recent and Frequent folders. → [Tabs, address bar and terminal](docs/features.md#tabs-address-bar-and-terminal)

**The system.** A command line, a single instance, one Trash view for every bin, and PATH integration you can undo. → [System integration](docs/features.md#system-integration)

## Configuration

Gezik keeps its settings in a plain-text folder. Changes apply as soon as you save, with no restart.

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
| `crates/gezik-platform` | the parts that differ per OS (drives, icons, clipboard, trash, menus) |
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
