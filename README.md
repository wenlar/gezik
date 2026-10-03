# Gezik

A fast, lightweight, cross-platform file manager written in Rust with [Slint](https://slint.dev).

## Goals

- Instant startup and low memory use
- The UI thread never waits on the file system
- Windows, macOS and Linux from one codebase

## Layout

- `crates/gezik-core`: platform-independent logic (listing, sorting, formatting)
- `crates/gezik`: the desktop app (Slint UI)

## Build

```sh
cargo run --release -p gezik
cargo test
```
