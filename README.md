# Gezik

A fast, lightweight, cross-platform file manager written in Rust with [Slint](https://slint.dev).

## Goals

- Instant startup and low memory use
- The UI thread never waits on the file system
- Windows, macOS and Linux from one codebase

## Layout

- `crates/gezik-core`: platform-independent logic (listing, sorting, formatting)
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
- `state.toml` — window size and position on this machine. Not meant to be copied.

Changes apply as soon as you save; no restart needed. Problems (a typo, an invalid
color) show up in the status bar with the file and line, and never stop Gezik from
starting.

### Writing a theme

A theme only needs the values it changes; the rest comes from its `base`:

```toml
# themes/sunset.toml  ->  select it with  theme = "sunset"  in settings.toml
base = "dark"

[colors]
accent = "#ff8a3d"
folder-icon = "#ffb347"
```

See `themes/example.toml` for every color and size you can set.

## Performance checks (Windows)

```powershell
scripts/perf/measure.ps1   # startup time and idle memory
scripts/perf/stress.ps1    # 100,000-file folder: memory and scrolling CPU
```

## Build

```sh
cargo run --release -p gezik
cargo test
```
