# Gezik on macOS: build and manual test

Gezik has only been compiled for macOS so far (`cargo check`); nothing has run on a Mac. This file is for a person (or Claude Code on the Mac) testing it there. Write results into `docs/superpowers/notes/macos-test-results.md` (PASS / FAIL / NOT TESTED per item; for a FAIL: steps, expected, actual, a screenshot path).

## Build

```sh
xcode-select --install            # C/C++ compiler (UnRAR is C++); skip if already installed
curl https://sh.rustup.rs -sSf | sh   # Rust; then open a new terminal
git clone git@github.com:wenlar/gezik.git && cd gezik
git checkout master               # 5a + 5b; use feat/batch-ops-5c for later work
cargo build --release -p gezik
mkdir -p /tmp/gezik-cfg /tmp/gezik-test
GEZIK_CONFIG_DIR=/tmp/gezik-cfg ./target/release/gezik /tmp/gezik-test
```

Also run the automated tests once: `cargo test --workspace` (report failures with their output).

Test data: put a few files and folders in `/tmp/gezik-test`. Archives: make them with `zip`, `tar czf`, or 7-Zip if installed. A RAR file is in `crates/gezik-batch/tests/data/` (see SOURCES.md there).

## Checklist

Basics
1. The window opens. It follows the system light/dark mode, and switching the mode while Gezik runs changes the theme.
2. Navigation: double-click a folder, ⌘↓ opens, ⌘[ goes back, ⌘↑ goes up, ⌘T opens a new tab, ⌘W closes it, and the sidebar shows Home and the drives.
3. Enter renames (Finder style). Escape cancels. A name with `/` shows a red tip.
4. Cmd+Shift+N creates a new folder and opens it for renaming.
5. ⌘C / ⌘X / ⌘V work inside Gezik. ⌘⌥V pastes as a move.
6. **Finder interop:**
   - Copy a file in Gezik, then paste in Finder (⌘V).
   - Copy a file in Finder, then paste in Gezik.
   - Cut in Gezik shows the item faded.
7. **Trash:**
   - ⌘⌫ moves the item to the Trash and it appears there in Finder.
   - Ctrl+Z (⌘Z) brings it back.
   - ⌘⌥⌫ deletes permanently, after a question.
8. **Drag and drop:**
   - Drag from Gezik to a Finder window: the file is copied, or moved on the same volume.
   - Drag from Finder into Gezik: copy or move. ⌥ while dragging forces a copy.
   - Drag between two Gezik tabs.
   - Drag onto a sidebar folder.
9. Space opens quick look. ⌘A selects all. Grid view is ⌘2 and list view is ⌘1.
10. Copy a large file (1-2 GB): the operations panel shows progress, and pause, resume and cancel all work.
11. A conflict list appears when pasting over existing files. Replace, Skip and Keep both work, and Ctrl+Z undoes it.

5a, batch rename
12. Select 3 or more files and press Enter. The "Rename N items" layer opens.
13. Replace, Number and Case (try Turkish i/İ if the system language is Turkish) update the preview live. Rename applies the names, and ⌘Z undoes them.
14. Swap two names (a.txt ↔ b.txt) using hand-typed names, then undo.
15. Presets: Save, reopen the layer, apply the preset, Delete. Check that `settings.toml` in /tmp/gezik-cfg has `[[rename-presets]]`.

5b, archives
16. Right-click a zip, then Extract here. A single-root zip goes in as it is; a multi-root zip goes into a `<name>/` folder. ⌘Z removes what was extracted.
17. tar.gz, 7z and rar (from the test data) extract.
18. An AES zip asks for a password. The field shows dots and "Show" reveals the text. A wrong password asks again; the right one extracts.
19. Compress… layer: zip, then 7z with a password and "Encrypt file names", then 7z split into parts. Compress to "x.zip" without the layer.
20. Drag files onto a zip row. The label says "Add to x.zip" and the files are added. ⌘Z restores the old archive.
21. **7-Zip download:** open a `.wim` or `.dmg` archive (make a .wim with 7-Zip on another machine, or skip if none is available).
    - The "needs 7-Zip" box appears, then Download.
    - The tool lands in `/tmp/gezik-cfg/tools/7zip-26.03/7zz` and runs. Check for any Gatekeeper prompt.
    - The archive opens.
22. Cancel a big extraction. No `.gezik-*` folders are left in the target folder (`ls -a`).

Report anything that looks wrong: layout, fonts, Retina scaling, menus, ⌘ shortcuts that don't work.
