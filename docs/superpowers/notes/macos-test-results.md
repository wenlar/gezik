# Gezik on macOS: test results

- Date: 2026-10-06
- Branch: `feat/batch-ops-5c` at `88d2230`
- Machine: Apple Silicon (aarch64), macOS 26 (Darwin 25.5.0), Retina display. System languages en-TR, tr-TR (English first). Keyboard layout **Turkish-QWERTY-PC**.
- Toolchain: rustc 1.99.0 / cargo 1.99.0, Apple clang 21.0.0
- Run: `GEZIK_CONFIG_DIR=/tmp/gezik-cfg ./target/release/gezik /tmp/gezik-test`
- Driven by Claude Code on the Mac. Clicks, drags and keys went in as synthetic CGEvents (modifiers sent as real key down/up, so the app sees flagsChanged). Results were checked with screenshots and on the file system.
- A second volume, `GezikHedef` (HFS+ sparse image under `/Volumes`), was used for cross-volume copies, because copies on the same APFS volume are clones and finish instantly.

## Summary

| # | Item | Result |
|---|------|--------|
| – | Build | PASS |
| – | `cargo test --workspace` | **FAIL** (4 tests) |
| 0 | (extra) Live refresh of the open folder | **FAIL** (symlinked paths) |
| 1 | Window opens, follows light/dark, switches live | PASS |
| 2 | Navigation keys, tabs, sidebar | **FAIL** (⌘[ on Turkish layout; fine on US) |
| 3 | Rename: Enter / Escape / `/` tip | PASS |
| 4 | ⌘⇧N new folder | PASS |
| 5 | ⌘C / ⌘X / ⌘V / ⌘⌥V inside Gezik | PASS |
| 6 | Finder clipboard interop | PASS |
| 7 | Trash, undo, permanent delete | PASS |
| 8 | Drag and drop | **FAIL** (drop after a tab switch hangs; drops from Finder ignore rows and ⌥; sidebar move across volumes) |
| 9 | Quick look, ⌘A, ⌘1 / ⌘2 | PASS |
| 10 | Large copy: progress, pause, resume, cancel | PASS |
| 11 | Conflict list: Replace / Skip / Keep both, undo | PASS |
| 12 | Batch rename layer opens | PASS |
| 13 | Replace / Number / Case live preview, rename, undo | PASS (Turkish casing only with `LC_ALL=tr_TR…`; see "Language detection") |
| 14 | Swap two names by hand, undo | PASS |
| 15 | Presets save / apply / delete, `settings.toml` | PASS |
| 16 | Extract here: single root, multi root, undo | PASS |
| 17 | tar.gz, 7z, rar extract | PASS |
| 18 | AES zip password prompt | PASS |
| 19 | Compress layer: zip, 7z + password + encrypted names, split, quick compress | PASS (with notes) |
| 20 | Drag onto a zip row adds, undo restores | PASS |
| 21 | 7-Zip download for .dmg | PASS |
| 22 | Cancel a big extraction leaves no `.gezik-*` | PASS |
| – | (extra) Language detection on macOS | **FAIL** |
| – | (extra) Menu bar and standard ⌘ shortcuts | **FAIL** (no ⌘M, no Edit/Window menus) |
| – | (extra) ⌘Z after a plain copy and a cut/paste | PASS |
| – | (extra) Window resizing | PASS (minor) |
| – | Retina vs. non-Retina scaling | NOT TESTED (no second display) |

## Build

PASS. `cargo build --release -p gezik` finished in 9m 32s, no warnings. (The machine's rustup `stable` toolchain was broken and had to be reinstalled first. That was a local problem, not Gezik's.)

## `cargo test --workspace`

FAIL: 4 tests fail. Everything else passes (gezik 118/121, gezik-ops 93/94, all other crates green).

```
folder_watch::tests::a_change_is_told_once_until_taken                  the watch never started
folder_watch::tests::an_old_folder_says_nothing_once_another_is_watched  the watch never started
folder_watch::tests::the_folder_itself_going_away_is_told                the watch never started
task::tests::a_large_copy_lands_under_its_name_only_when_complete        the copy did not start
```

- **folder_watch (3 tests):** `std::env::temp_dir()` is `/var/folders/…`, and `/var` is a symlink to `/private/var`. FSEvents reports `/private/var/…` paths, and `sidebar::same_path` compares with `==` (it is only lenient on Windows). So no event matches the watched folder. The app has the same bug; see item 0, where the cause was confirmed. Likely fix: canonicalize the watched folder (or the event paths) before comparing.
- **task copy test:** on macOS, `fs::unix::copy_file` calls `progress(0)` *before* `clonefile`. The test pauses the control first, so the copy blocks inside `progress(0)` before anything exists in `to/`, and the test waits for an entry that never appears. The test (and the comment in `copy_file`) expect the first progress call to come after the target exists; the clone path breaks that. In the app this only means a pause can land before the clone starts, which is harmless.

## Checklist details

### 0. Live refresh: FAIL
Opened at `/tmp/gezik-test`, then ran `touch /tmp/gezik-test/yeni.txt`. The list stayed at "26 items". The same happens in every folder opened under `/tmp/…`.

The same folder opened as `/private/tmp/gezik-test/klasor2` refreshes at once when a file is added, which confirms the symlink cause. `/Volumes/…` and Home are not affected; `/tmp`, `/var` and `/etc` are.

### 1. Theme: PASS
Opens in dark mode. Setting the system to light mode while Gezik runs switches it to the light theme right away; switching back works too.

### 2. Navigation: FAIL (⌘[ only)
- Double-click, ⌘↓ (with a folder selected), ⌘↑, ⌘T (a new tab opens at Home), ⌘W, the toolbar Back button and sidebar clicks (Home, GezikHedef) all work. ⌘L opens the path field.
- **⌘[ does nothing on the Turkish-QWERTY-PC layout.** That layout has no `[` key: the physical `[` key types `ğ`, and `[` is ⌥8.
  - `keys::chord_from_event` keeps only ASCII letters, digits, `[` and `]`, so ⌘ + the physical key (text `ğ`) becomes no chord.
  - ⌘⌥8 (text `[`) becomes Cmd+Alt+[, which doesn't match either.
  - Finder handles this through its menu key equivalents. Matching bracket shortcuts on the physical key code (or ignoring ⌥ when it was needed to type the character) would fix it.
  - **US layout: PASS.** Simulated by sending the `[` key with the text `[` (CGEventKeyboardSetUnicodeString). In `wim/test`, ⌘[ went back to `wim`. So only layouts without a `[` key are affected.

### 3. Rename: PASS
Enter opens the field with the stem selected (Finder style). Typing `q/w` shows the red tip "A name cannot contain /". Escape cancels and `a.txt` stays. Enter with `a2` renames the file to `a2.txt` on disk.

### 4. New folder: PASS
⌘⇧N created `New folder` with its name open for editing. Renamed it to `yeni`. (Finder calls a new folder "untitled folder"; just noting it.)

### 5. Clipboard inside Gezik: PASS
- ⌘C on `b.txt`, then ⌘V in `klasor2`: copied, source kept.
- ⌘X on `c.txt` shows the row faded. ⌘V in `klasor2` moves it.
- ⌘C on `d.txt`, then ⌘⌥V in `klasor1`: moved.

### 6. Finder interop: PASS
- ⌘C in Gezik puts a file URL on the pasteboard (`«class furl»`), and ⌘V in Finder pasted `b.txt`.
- ⌘C in Finder, then ⌘V in Gezik (`tek/`), pasted `finder.txt`.
- Cut shows the item faded (see 5).

### 7. Trash: PASS
- ⌘⌫ moved `a2.txt` to the Trash, and it shows in Finder's Trash.
- ⌘Z brought it back and it left the Trash.
- ⌘⌥⌫ asks "Delete a2.txt permanently? This cannot be undone." Cancel keeps the file. Delete removes it, and it doesn't go to the Trash.
- The first click after switching to Gezik from another app only focuses the window and doesn't select. Finder does the same, so this isn't a bug.

### 8. Drag and drop: FAIL
- **Gezik → Finder, same volume:** PASS. The file was moved (`istanbul_ılık.txt` from `gezik-test/` to `yeni/`).
- **Gezik → Finder, other volume:** PASS. `wim/test.wim` dragged into a Finder window on GezikHedef was copied; the source stayed.
- **Inside Gezik onto a folder row:** PASS. The row is highlighted, the label says "Move to takas", and the file is moved into `takas/`. With ⌥ held the label says "Copy to toplu" and the file is copied.
- **Finder → Gezik: FAIL (3 times).**
  - The file arrives, but always in the **open folder**. While hovering a folder row (`tek`, `klasor2`) or a zip row (`arsiv.zip`), nothing is highlighted and no label shows. The drop ignores the row and puts the file in `gezik-test/`.
  - **⌥ is ignored:** an ⌥-drag from Finder to Gezik **moved** the file (it left `yeni/`).
  - Control test: the same ⌥-drag method between two Finder windows copied the file. So the ⌥ really did reach the system, and the problem is on Gezik's side.
  - It looks like external drops never go through the row hit-testing that internal drags use, and don't read the modifier keys.
- **Between two Gezik tabs: FAIL when the tab has already switched.**
  - A quick drop on the other tab (released before it switches) works: `test.wim` moved into `klasor2`.
  - If the pointer rests on the tab, `drag.rs::tab_rested` → `nav.activate_tab(i)` switches to it ("Move to klasor2" shows). Releasing then **doesn't drop**: the drag ghost and label keep following the pointer after mouse-up, and only Escape ends it. Nothing is moved. Reproduced 3 times.
  - Likely cause: the switch rebuilds the list that started the drag, so the release never reaches the drag handler.
- **Onto a sidebar entry:** the drop works. But dropping `klasor2/c.txt` onto **GezikHedef** (a separate volume) showed "Move to GezikHedef" and **moved** the file. Gezik → Finder across volumes copies (see above), and so does Finder. FAIL on that point.

### 9. Quick look, select all, views: PASS
Space opens a quick look window ("ılık İstanbul" renders correctly, with type, size and dates), and Space closes it. ⌘A selects 27 of 27. ⌘2 shows the grid and ⌘1 the list.

### 10. Large copy: PASS
- `buyuk.bin` (1.5 GB) to GezikHedef: the panel shows "Copying buyuk.bin to /Volumes/GezikHedef", a bar, %, speed and time left, then "Done".
- The copy is written as `.gezik-copying-<id>-0` and renamed at the end.
- With `dev.bin` (5.9 GB):
  - Pause shows "Paused" and the temp file size stays fixed (1 143 996 416 bytes over 2 s).
  - Resume continues (38 %, 63.9 MB/s).
  - Cancel shows "Cancelled" and removes the temp file.
- On the same APFS volume a copy is a clone and finishes at once, so the panel only flashes.

### 11. Conflict list: PASS
Pasting `b.txt` over `klasor2/b.txt` opens "1 conflict · Copying b.txt to …" with source/target size and dates and "(newer)".
- Replace + Start: the target gets the new content. ⌘Z restores the old content.
- Keep both: creates `b (2).txt`. ⌘Z removes it. (Finder would name it `b 2.txt`.)
- Skip: nothing changes.
- On extracting `arsiv.tar.gz` over existing files, the list marks equal files as "identical" and offers "Hide identical".

### 12. Batch rename layer: PASS
4 files selected, Enter → "Rename 4 items".

### 13. Rules: PASS (Turkish casing only with a Turkish `LC_ALL`/`LANG`)
- Replace `foto` → `resim`, Number (001, at end) and Case UPPER update the preview live.
- Rename gave `RESIM IKINCI 001.txt` … `RESIM ÜÇÜNCÜ 004.txt`, and ⌘Z restored all 4 names.
- Relaunched with `LC_ALL=tr_TR.UTF-8`:
  - UPPER gives `FOTO İKİNCİ.txt`, `FOTO İLK.txt`, `FOTO İSTANBUL.txt`, `FOTO ÜÇÜNCÜ.txt` (on disk too), and ⌘Z restores them.
  - lower turns `ILIK IŞIK.txt` into `ılık ışık.txt`.
  - The Turkish rules themselves are right. But see "Language detection" below: a normal launch never picks Turkish.
- The rules from the last session come back when the layer is reopened. That seems intended, since they're saved in `state.toml`.

### 14. Swap: PASS
With the rules turned off, `a.txt` was given the name `b.txt` and `b.txt` the name `a.txt` (via the Name field):
- On disk the names are swapped and the contents follow them (`a.txt` now holds "icerik-b").
- No temp files are left.
- ⌘Z swaps them back.

### 15. Presets: PASS
- Presets → Save current rules → "Foto Resim". `/tmp/gezik-cfg/settings.toml` gets `[[rename-presets]]` with `name = "Foto Resim"` and three `[[rename-presets.rules]]`.
- Reopened the layer and deleted the rules. Applying the preset brought all three back.
- Delete "Foto Resim" removes it from the menu and from `settings.toml`.
- **Layout:** the Presets menu runs past the window's right edge, so "Save current rule…" and "Delete "Foto Res…" are cut off.

### 16. Extract here: PASS
- `single-root.zip` (root `kok/`) went in as `kok/`.
- `multi-root.zip` (`m1.txt`, `m2.txt`) went into `multi-root/`.
- ⌘Z twice removed both.

### 17. Other formats: PASS
- `arsiv.tar.gz`: extracted (to `arsiv/`).
- RAR5 multiple files, RAR5 with symlinks (links kept as links), RAR4 `test_read_format_rar.rar`, and multi-volume `test_rar_multivolume_single_file.part1.rar` (part1-3): all extracted.
- 7z: the archive made in 19 extracted after its password.
- Split 7z: `buyuk.7z.001/.002/.003` (made with `7zz -v700m`). "Extract here" on `.001` rebuilt `buyuk.bin`, byte-identical to the source (`cmp`).

### 18. AES zip: PASS
The zip was made with Gezik's own Compress (zip + password). `unzip` skips its entries as an "unsupported compression" method, i.e. AES.
- Extract asks "sifreli.zip is encrypted. Password:", and the field shows dots.
- "Show" reveals the text.
- A wrong password gives "Wrong password. Try again:". The right one extracts, and the content is checked.
- The temp `.gezik-deleting-*` folder of the failed try was gone afterwards.

### 19. Compress layer: PASS (with notes)
- zip: `arsiv.zip` made from the `arsiv` folder, and its contents check out.
- 7z with password + "Encrypt file names": `sifreli.7z`. Gezik asks for the password before it extracts.
- 7z, Store, split 700 MB: `buyuk.7z.001` / `.002` / `.003` (700 MB + 700 MB + 173 MB).
- "Compress to …" without the layer: works, but see below.
- Checked with 7-Zip 26.03:
  - `sifreli.zip` entries are "AES-256 Deflate" (WinZip AES).
  - `sifreli.7z` can't even be listed without the password (names are encrypted). With `-pgizli` it lists `sifreli/arsiv/klasor1/…`.
- **Notes:**
  - **The extension can be lost.** In the layer, the whole name `arsiv.zip` is selected. Typing `sifreli` replaces the extension too, and the archive was saved as `sifreli`, with no extension and so no longer recognized as a zip. Adding the extension for the chosen format (or selecting only the stem, like rename does) would fix it.
  - **Quick compress uses the last format, not zip.** After using 7z in the layer, the menu says `Compress to "b.7z"` and makes a 7z. The checklist expects `x.zip`.
  - **The split setting is remembered** (`[archive] split = 700000000` in `state.toml`). A later quick "Compress to …" of a big file would be split into parts without the user seeing it.
  - Split parts show as "001 File" / "002 File" with a plain file icon.

### 20. Add to archive: PASS
Dragging `b.txt` onto the `arsiv.zip` row shows "Add to arsiv.zip". `b.txt` is added (6 entries), and the source stays. ⌘Z restores the old archive byte for byte (same MD5).

### 21. 7-Zip download: PASS
A `.dmg` made with `hdiutil create -format UDZO`, then "Extract to "disk/"":
- The "7-Zip needed: Opening .dmg archives needs 7-Zip (~1.8 MB, free)" box appears.
- "Where does it come from?" opens `github.com/wenlar/gezik-tools` and `7-zip.org/license.txt` in the browser.
- Download installs `/tmp/gezik-cfg/tools/7zip-26.03/7zz` (universal x86_64 + arm64, ad-hoc signed) and `License.txt`.
- No Gatekeeper prompt: the file has no `com.apple.quarantine` attribute. `7zz i` runs and reports "7-Zip (z) 26.03 (arm64)". (`spctl` rejects it, "no usable signature", but without quarantine that doesn't matter.)
- The dmg then extracted (`m1.txt`, `m2.txt`).
- `.wim`: made with `7zz a -twim` from `klasor1`. "Extract to "test/"" gives `test/klasor1/{alt/y.txt,d.txt,x.txt}` (7-Zip was already installed by then). The `.wim` row has a plain file icon, not the archive icon.

### 22. Cancel extraction: PASS
Extracted a 5.9 GB store zip and pressed Cancel at 31 %. The panel says "Cancelled". `ls -a` afterwards shows only `dev.zip`: no `.gezik-*` left.

While it runs, the extraction is staged in `.gezik-deleting-x-<pid>-<n>/x/` and moved into place at the end. The ops panel only appears after about a second, so short jobs can't be cancelled from it.

## Extra checks

### Language detection: FAIL
`gezik_platform::language()` reads `LC_ALL` / `LC_CTYPE` / `LANG` on every non-Windows system. Its comment says "macOS apps get LANG from the system", but that's not true for GUI apps:
- `launchctl getenv LANG` is empty, so an app started from Finder or the Dock gets no language at all.
- Started from Terminal here, the app had `LANG=C.UTF-8` and `LC_CTYPE=UTF-8`, so it read "UTF-8".

Either way it never picks Turkish, even with Turkish first in System Settings. On macOS this should come from `CFLocaleCopyPreferredLanguages` (or `NSLocale.preferredLanguages`). The Turkish rules themselves work (see 13).

### Menu bar and ⌘ shortcuts: FAIL
The menu bar has only the app menu, named **"gezik"** (lowercase, from the binary name): About gezik, Services, Hide gezik ⌘H, Hide Others ⌥⌘H, Show All, Quit gezik ⌘Q.
- There's no File, Edit, View, Go or Window menu. So shortcuts can't be found from the menu, and macOS's own key-equivalent handling (which would fix ⌘[ on the Turkish layout) isn't used.
- ⌘Q quits (the next start removed nothing it shouldn't). ⌘H hides.
- **⌘M doesn't minimize.** The window stays (`AXMinimized` = false), and the shortcut table has no minimize action.
- ⌘, does nothing (settings live in `settings.toml`; there's no settings window).

### ⌘Z after a plain copy and a cut/paste: PASS
- Copied `ILIK IŞIK.txt` from `toplu/` to `takas/`. ⌘Z removed the copy, and the original stayed.
- Cut `a.txt` from `takas/` and pasted it into `toplu/`. ⌘Z put it back in `takas/`.

### Window resizing: PASS (minor)
At 480×360 the layout holds: the sidebar stays and the Type and Size columns drop out. The Modified column is cut at the right edge with no horizontal scroll. At 1500×950 the columns spread out cleanly.

### Not tested
- Retina vs. non-Retina scaling: only the built-in Retina display was available. At 2× everything looks sharp.

## Other things noticed

- **Windows wording on macOS:** the address bar starts with "This PC", the sidebar section is "DRIVES", and types read "TXT File", "File folder", "BIN File". Finder says "Plain Text", "Folder" and so on.
- **Hidden files are shown:** `.DS_Store` in `/tmp/gezik-test`, and every dot folder in Home (`.cargo`, `.claude`…). Finder hides them by default.
- **Context menus are cut off at the window's bottom edge:** a right-click on a row near the bottom opens the menu downward, and the lower items ("Cut", "Copy", … "Delete permanently") are cut off instead of the menu flipping up.
- `Extract to "sifreli/"` on an archive whose single root is `sifreli/`, while that folder already exists, merged into it and gave `sifreli/sifreli/…`. This is what 7-Zip does too; just noting it.
- Fonts and Retina scaling look sharp. Dialogs, panels and the light/dark themes are consistent.

## Fixes on fix/macos

Branch `fix/macos` from `master` (`104f168`), tested on the same Mac (Turkish-QWERTY-PC layout). One commit per fix. On macOS, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` all pass. Every fix was checked on screen with the steps that showed the bug, and checklist items 0, 2, 8 and 13, plus the menu and language extras, were run again on the final build.

| # | Bug | Commit | After the fix |
|---|-----|--------|---------------|
| 1 | Folder watch with symlinked paths | `7501764` | PASS |
| 2 | `a_large_copy_lands_under_its_name_only_when_complete` | `a5d0327` | PASS |
| 3 | ⌘[ / ⌘] without a `[` key | `27d1ee3` | PASS |
| 4 | Drops from Finder: rows, label, ⌥ | `ac9d4c5` | PASS |
| 5 | Drop after resting on a tab | `a820d77` (shared code) | PASS |
| 6 | Sidebar drop onto another volume moved | `ec72af8` | PASS |
| 7 | Language detection | `4826e75` | PASS |
| 8 | Menu bar, ⌘M, "Gezik" | `276e410` | PASS (with a note) |
| 9 | macOS wording, kinds, dotfiles | `2b20804`, `ec3247d`, `c594232` | PASS |
| – | clippy `drop_non_drop` off Windows (also on master) | `a120800` | PASS |

### 1. Folder watch: PASS
- **Cause:** as found before: FSEvents reports `/private/tmp/…`, the folder was watched as `/tmp/…`, and the paths were compared with `==`.
- **Fix:** `folder_watch::start` now watches and compares the canonical path on macOS; other systems are unchanged.
- **Tests:** the 3 folder_watch tests pass, plus a new `a_folder_opened_through_a_symlink_is_watched` (unix).
- **On screen (item 0):** with Gezik on `/tmp/gezik-test`, `touch canli0.txt` shows up at once.

### 2. Copy test: PASS
`copy_file` now clones first and calls `progress(0)` after the clone exists; a cancel there removes the clone. Both `a_large_copy_lands_under_its_name_only_when_complete` and `a_cancelled_large_copy_leaves_nothing` pass.

### 3. ⌘[ / ⌘]: PASS
- **Fix:** with ⌘ held, a press of the key right of P (`kVK_ANSI_LeftBracket`/`RightBracket`, read from the `NSEvent` being handled) is matched as `[` / `]`, by the key's place, as macOS's own shortcuts are. This is `gezik_platform::key_place` and `keys::shortcut_text`.
- **Tests:** key-code mapping, and `ğ`/`ü` with ⌘ give Back/Forward.
- **On screen:** on Turkish-QWERTY-PC, ⌘+(key that types `ğ`) goes back and ⌘+(`ü`) goes forward. The US layout (simulated `[`/`]` text) still works. The menu's Go ▸ Back/Forward work too.

### 4. Drops from Finder: PASS
- **Cause:** winit registers the NSWindow, not the view, for dragged files. So AppKit sent drags to winit's window delegate (DroppedFile, with no position and no keys), and Gezik's own `NSDraggingDestination` methods on the view were never called.
- **Fix:** the view is now registered for `NSPasteboardTypeFileURL`, so those methods are used.
- **On screen:**
  - Finder → the `tek` row: the row is highlighted, the label says "Move to tek", and the file goes into `tek/`.
  - With ⌥: the label says "Copy to klasor2", the file is copied, and the source stays.
  - Onto `arsiv.zip`: the row is highlighted and the file is added to the zip.
  - The label follows ⌥ while dragging. macOS's drag image has no text, so Gezik draws the label (macOS only).
- **Not automated:** an `NSDraggingInfo` test needs a real drag session.

### 5. Drop after a tab switch: PASS (shared code)
- **Cause:** opening the tab rebuilds the list, so the pressed entry and its Slint pointer grab are gone. A release over the tab bar then reached no one.
- **Fix:** after `tab_rested` opens a tab, the drag follows winit's `CursorMoved` / `MouseInput` events and is dropped on release.
- **Shared:** this is shared code: Slint and winit lose the grab the same way on Windows and Linux.
- **On screen, all three cases work and no ghost is left:**
  - Rest on the tab, then release on the tab: moved.
  - Rest on the tab, then release over the new list: moved.
  - Quick drop before the switch: moved.

### 6. Sidebar drop onto another volume: PASS
- **Cause:** drives came from `/Volumes`, where the startup disk is a link (`/Volumes/Macintosh HD` → `/`). A file under `/tmp` was on no known drive, and the lexical-root fallback said "same drive".
- **Fix:** each `/Volumes` entry is resolved, so the startup disk is `/`.
- **Tests:** `the_startup_disk_is_the_root` (macOS) and `a_volume_mounted_under_the_root_is_another_drive`.
- **On screen:** `d.txt` onto GezikHedef shows "Copy to GezikHedef" and is copied; the source stays.
- **Side effect:** clicking "Macintosh HD" in the sidebar now opens `/`.

### 7. Language: PASS
- **Fix:** on macOS `language()` takes the first of `NSLocale.preferredLanguages`, and only falls back to LC_ALL / LC_CTYPE / LANG if that list is empty.
- **Tests:** `language_is_the_first_preferred_one_on_macos`; it passes whatever LANG says.
- **On screen (item 13):** with Turkish first in AppleLanguages (set for the test, then put back to `en-TR, tr-TR`) and a normal launch (no LC_ALL, the terminal's LANG is `C.UTF-8`):
  - UPPER previews `FOTO İKİNCİ.txt` and `FOTO İLK.txt`.
  - lower previews `ılık ışık.txt`.

### 8. Menu bar: PASS (with a note)
- **Fix:** a `MenuBar` in app.slint, shown only on macOS (`native-menu-bar`, set from Rust there). Its menus:
  - **File:** New Tab ⌘T, New Folder ⇧⌘N, Quick Look, Rename, Rename Items…, Duplicate ⌘D, Move to Trash ⌘⌫, Delete Immediately… ⌥⌘⌫, Close Tab ⌘W.
  - **Edit:** Undo ⌘Z, Redo ⇧⌘Z, Cut, Copy, Paste, Move Item Here ⌥⌘V, Select All. (macOS adds its own AutoFill, Dictation and Emoji items, as it does for Finder.)
  - **View:** as List ⌘1, as Grid ⌘2, Show Preview, Show Hidden Files ⇧⌘., Refresh ⌘R, Enter Full Screen.
  - **Go:** Back ⌘[, Forward ⌘], Enclosing Folder ⌘↑, Go to Folder… ⌘L.
  - **Window:** Minimize ⌘M, Zoom, Show Next/Previous Tab.
- **How an item works:** choosing it, with its shortcut or with the mouse, plays the action's current shortcut to the window (`menu_bar.rs`). So it does what the keys do where the focus is.
  - Slint matches menu bar shortcuts before anything else, so they are switched off while the keys are played.
  - That happens after the menu is done with the item. An earlier build switched them during the activation; Slint rebuilt the menu under itself and panicked ("RefCell already borrowed", `i-slint-core/menus.rs:99`).
- **On screen:**
  - ⌘2 and ⌘1 work, and View ▸ as List with the mouse works.
  - ⌘C in a name being edited copies the text ("tek"); ⌘C in the list copies the file (`«class furl»`).
  - ⌘L, ⌘A and ⌘V in the path field work.
  - ⇧⌘. shows and hides dotfiles, and so does View ▸ Show Hidden Files.
  - ⌘M minimizes (`AXMinimized` = true).
  - No crash.
- **App name:** the executable carries an Info.plist (`__TEXT,__info_plist`, `crates/gezik/macos/Info.plist`, bundle id `com.wenlar.gezik`, which I chose; change it if there is a real one), so the app menu is titled **Gezik**. macOS window tabs are off, so "Show Tab Bar" / "Show All Tabs" are gone from View.
- **Note:** the items Slint adds to the app menu still read "About gezik", "Hide gezik" and "Quit gezik". Muda takes that name from `NSRunningApplication.localizedName`, which is the executable's file name for a binary outside an .app bundle. It will read "Gezik" once Gezik ships as `Gezik.app`.
- **Note:** the menu shows the default shortcuts. A shortcut rebound in settings.toml still works, and the menu item still runs the action, but the menu shows the default (Slint's `Keys` can't be built from Rust with the public API).
- **Tests:** `played_keys_are_the_chord` (every default chord played as Slint keys comes back as the same chord), and `⌘⇧.` → toggle-hidden.

### 9. Wording and defaults: PASS
- **Names:** on macOS the address bar starts with **Computer** (Finder's Go ▸ Computer) and the sidebar section is **LOCATIONS**.
- **Kinds:**
  - The Type column uses Launch Services kinds: "Folder", "Plain Text Document", "ZIP archive", and "Document" for unknown types, as in Finder's Kind column, in the system's language.
  - They come from `LSCopyKindStringForTypeInfo`. It is deprecated but is the only call that names a type from its extension alone.
  - `UTType.localizedDescription` was tried first; it gives lower-case "text" and "folder".
  - The fallback before the name arrives is "Folder" / "Document".
- **Dotfiles:**
  - On macOS, names starting with a dot are hidden by default (`.DS_Store`, `.gizli` are gone: 6 items instead of 8).
  - The new `toggle-hidden` action (⇧⌘. by default, View ▸ Show Hidden Files) shows them again.
  - The shortcut goes by the key that types `.` on the layout in use. Shift turns it into `>` (US) or `:` (Turkish); on Turkish-QWERTY-PC that is the key at the ANSI `/` place.
  - Windows and Linux list everything as before; toggle-hidden has no default binding there.
- **Tests:** type names (macOS), fallbacks, `without_dotfiles`, `DRIVES_NAME` in the crumb tests.

### Shared code touched
Each of these was kept to macOS where the logic allowed:
- `drag.rs` and `main.rs`: the tab-switch drop (5). Shared on purpose.
- `gezik-config` shortcuts: the `.` key, the new `toggle-hidden` action, `Action::from_name` / `Shortcuts::chord_for` made public. The Other platform has no default for toggle-hidden.
- `gezik-core`:
  - `DRIVES_NAME` is "This PC" except on macOS.
  - `fallback_type_name` is unchanged except on macOS.
  - A new `same_drive` test.
- `navigation.rs`: the clippy allow, off Windows only.

## Run 2 (master)

- Date: 2026-10-10
- Branch: `master` at `cbf1ab0` (results on `test/macos-run2`)
- Machine: MacBook Pro, Apple M2 Pro (Mac14,10), built-in Liquid Retina XDR display (3456 × 2234, 200 %)
- macOS 26.5.1 (25F80). System languages en-TR, tr-TR (English first). Keyboard layout **Turkish-QWERTY-PC**.
- Toolchain: rustc 1.99.0 / cargo 1.99.0, Apple clang 21.0.0
- Run: `GEZIK_CONFIG_DIR=/tmp/gezik-cfg ./target/release/gezik /tmp/gezik-test`, starting from an empty `/tmp/gezik-cfg`.
- Steps 1-2 (build, tests, probes, command line) were run by Claude Code on the Mac. The window items were done by the maintainer at the keyboard, walked through item by item, with Claude Code checking the file system and screenshots.
- Test data was made on the Mac (`/tmp/gezik-test`): JPEGs and PNGs written with ImageIO (one with EXIF orientation 6 and GPS, a PNG with alpha), a HEIC written with ImageIO (**not** a real tiled iPhone HEIC), PDFs written with CoreGraphics (3, 5 and 240 pages, and one encrypted with the password `gezik`), the repo's `enc_aes256.pdf` and RAR files, a 4 s screen recording (`klip.mov`), an AAC `.m4a` from `say`, a Windows-1254 text, zips (single root, multi root, ZipCrypto with a password), a `tar.gz`, a `.dmg`, and a folder of 1,000 empty files of mixed types.

### Summary

| # | Item | Result |
|---|------|--------|
| – | Build | PASS |
| – | `cargo test --workspace` | **FAIL** (28 tests in 6 suites) |
| – | (extra) Quitting Gezik / closing its last window | **FAIL** (aborts with a crash report every time) |
| – | (extra) settings.toml changes apply live with `GEZIK_CONFIG_DIR=/tmp/gezik-cfg` | **FAIL** (never, through the `/tmp` link) |
| 23 | Live refresh through a symlink | PASS |
| 24 | ⌘[ / ⌘] by key place | PASS |
| 25 | Finder → Gezik drops | PASS |
| 26 | Tab-rest drag | **FAIL** (the right-drag drop menu ignores the mouse) |
| 27 | Sidebar drop onto another volume | PASS |
| 28 | Language | PASS |
| 29 | Menu bar | PASS |
| 30 | Wording and dotfiles | PASS |
| 31 | Menus at the window edge | PASS |
| 32 | Compress name | PASS (one note) |
| 33 | ⌘ / Ctrl shortcuts | PASS (redo under /tmp hits bug A) |
| 34 | Archive kinds and icons | **FAIL** (WIM: type "Document", no archive icon) |
| 35 | Pictures | PASS |
| 36 | Text | PASS |
| 37 | HEIC and the ffmpeg download (Gatekeeper, quarantine) | PASS (HEIC made with ImageIO, not an iPhone) |
| 38 | Audio and video | **FAIL** (MP3/M4A from a silent .mov fail with a raw ffmpeg error) |
| 39 | Pause and resume | PASS (user-command part in 40) |
| 40 | User commands | PASS (one note: menu clicks in Commands ▸) |
| 41 | Images to PDF | PASS |
| 42 | pdfium download (does the dylib load?) | PASS (one test fails on a non-UTF-8 name, as B) |
| 43 | Split, extract, to images | PASS (one note) |
| 44 | Encrypted PDF | PASS |
| 45 | No worker left behind | PASS |
| 46 | Last choices and keys | PASS (one note) |
| 47 | Filter | PASS (Enter renames on macOS; / is ⇧7 on Turkish) |
| 48 | Pattern box and selection keys | PASS (⌘= needs a custom key on Turkish Q) |
| 49 | Tabs | PASS |
| 50 | Typing mode | PASS |
| 51 | Saved filters | PASS (one note) |
| 52 | Path suggestions | PASS (network-share part not tested) |
| 53 | Folder history | PASS (one note) |
| 54 | Command keys | **FAIL** ({files} gives absolute paths, so the zip holds /private/tmp/…) |
| 55 | No history | PASS |
| 56 | Open terminal | PASS |
| 57 | Copy path | PASS (one note: ⌘V after ⌘⌥C makes a text file, from 7c) |
| 58 | Session | PASS |
| 59 | Tab sets | PASS |
| 60 | Custom terminal | PASS (no iTerm here; a stand-in command was used) |
| 61 | Pinned groups | PASS |
| 62 | Pins 1-9 | PASS (Turkish Q only) |
| 63 | View menu | PASS |
| 64 | View options | PASS |
| 65 | Hidden by default | PASS |
| 66 | New ▸ and templates | PASS (templates folder follows GEZIK_CONFIG_DIR) |
| 67 | New folder with selection | PASS (row menu item not reached) |
| 68 | Paste as file | PASS (Safari image not tried) |
| 69 | Links | PASS (Finder ⌘⌥ drag and right-drag not tried) |
| 70 | Drop stack | PASS (Finder drags and scrolling not tried) |
| 71 | History | PASS (one note: "identical" by size and time) |
| 72 | Search | PASS ("results while you type" needs Enter in a large folder) |
| 73 | Privacy prompts | PASS (prompts answered by the maintainer) |
| 74 | Whole drive | PASS (smoothness while running not measured) |
| 75 | Flat view | PASS (one note: the folder link shows as a 0 B file) |
| 76 | Show in folder | PASS |
| 77 | Results like a folder | PASS (one note) |
| 78 | Themes | PASS (reduce-motion fades not judged) |
| 79 | Drawing at 200 % | PASS |
| 80 | Thin scroll bar | PASS (trackpad feel not judged) |
| 81 | Folder sizes | PASS (sizes are logical, not on disk; network share not tried) |
| 82 | Command palette | PASS |
| 83 | Quick Open | PASS |
| 84 | Saved searches | PASS (saved to GEZIK_CONFIG_DIR's settings.toml) |
| 85 | Probe | **FAIL** (probe: "cancel after 5 ms" still gave a PDF thumbnail; workspace tests fail, see below) |
| 86 | System icons | **FAIL** (no special icons for Desktop/Documents/Downloads or /Applications etc.) |
| 87 | Speed and memory | PASS (largest grid size and SMB not tried) |
| 88 | Quick Look thumbnails | **FAIL** (JPEG with EXIF orientation 6 shown sideways in grid and preview) |
| 89 | Finder names | NOT TESTED (needs Turkish as the system language and a log-out) |
| 90 | Opening aliases | PASS (sym-dosya opened in TextEdit not checked separately) |
| 91 | Make Alias | PASS (three-item selection and arrow badge not checked) |
| 92 | Packages | **FAIL** (Safari.app is entered; a plain x.app folder can't be entered) |
| 93 | Exe size | PASS (20,942,592 bytes) |
| 94 | Probe | NOT TESTED |
| 95 | Open With | NOT TESTED |
| 96 | Share | NOT TESTED |
| 97 | Quick Actions | NOT TESTED |
| 98 | Quick Look panel | NOT TESTED |
| 99 | Unchanged | NOT TESTED |
| 100 | Exe size | PASS (20,942,592 bytes) |
| 101 | Probe | PASS |
| 102 | Opening | NOT TESTED |
| 103 | Permissions | NOT TESTED |
| 104 | Owner and group | NOT TESTED |
| 105 | Hidden and Locked | NOT TESTED |
| 106 | Apply to enclosed items | NOT TESTED |
| 107 | Open with | NOT TESTED |
| 108 | Links and ACLs | NOT TESTED |
| 109 | Exe size | PASS (20,942,592 bytes) |
| 110 | Tests first | PASS |
| 111 | Hand-over | PASS |
| 112 | Help, version, new window | **FAIL** (the second window is not offset) |
| 113 | Hung or crashed Gezik | PASS |
| 114 | Off | PASS |
| 115 | Bring to front | **FAIL** (a minimized Gezik comes back but is not activated) |
| 116 | Tests first | **FAIL** (every restore under `/var` or `/tmp` is refused) |
| 117 | Full Disk Access off | NOT TESTED |
| 118 | Full Disk Access on | NOT TESTED |
| 119 | Real bins on USB volumes | NOT TESTED |
| 120 | Put Back | NOT TESTED |
| 121 | Delete for good and Empty | NOT TESTED |
| 122 | Live and refused | NOT TESTED |
| 123 | Tests first | PASS |
| 124 | Panel | NOT TESTED |
| 125 | Add, no `~/.local/bin` | NOT TESTED |
| 126 | Someone else's file | NOT TESTED |
| 127 | Gezik moved | NOT TESTED |
| 128 | `--unregister` | NOT TESTED |
| 129 | No journal | NOT TESTED |
| 130 | Tests and probe first | PASS |
| 131 | Sidebar | NOT TESTED |
| 132 | State badges | NOT TESTED |
| 133 | Download Now / Remove Download | NOT TESTED |
| 134 | Data safety | NOT TESTED |
| 135 | OneDrive / Google Drive | NOT TESTED |
| 136 | Links | NOT TESTED |
| 137 | Late roots | NOT TESTED |
| 138 | Search reads no cloud-only file | NOT TESTED |
| 139 | Tests and probe first | PASS |
| 140 | Panel and palette | NOT TESTED |
| 141 | Make default from outside a bundle | NOT TESTED |
| 142 | `open ~/Documents` | NOT TESTED |
| 143 | `application:openURLs:` | NOT TESTED |
| 144 | Bundle launch while Gezik runs | NOT TESTED |
| 145 | Gezik moved | NOT TESTED |
| 146 | Restore and `--unregister` | NOT TESTED |

### Build

PASS. `cargo build --release -p gezik` finished in 3m 31s with no warnings. `target/release/gezik` is 20,942,592 bytes.

### `cargo test --workspace`

FAIL. 28 tests fail in 6 suites, and all the other suites pass. 22 of the 28 come from one cause (A).

| Suite | Passed | Failed |
|---|---|---|
| `gezik-batch --lib` | 78 | 2 |
| `gezik-batch --test compress` | 12 | 3 |
| `gezik-batch --test convert_tasks` | 14 | 6 |
| `gezik-ops --lib` | 147 | 11 |
| `gezik-platform --lib` | 221 | 4 |
| `gezik-search --lib` | 104 | 2 |

**A. Restore from the Trash refuses any place under `/var` or `/tmp` (22 tests, and the app).** The message is `/var is a link: put the item back by hand`. `trash::check_way_back` (`crates/gezik-platform/src/trash/mod.rs:120`) checks the way back from the volume root when the place is outside the home folder. On macOS `/var`, `/tmp` and `/etc` are the system's own links to `/private/…`, so every item from `std::env::temp_dir()` (`/var/folders/…`) or from `/tmp` is refused. The doc comment says macOS's `/var` should not block it, but that only holds for places under the home folder. In the app this means that ⌘Z after ⌘⌫, or after any job that trashes something (replace, convert with "Replace originals", undo of a copy), fails for files under `/tmp/gezik-test`. A likely fix is to also accept the canonical form of the root-level system links on macOS (`/var` → `/private/var`, `/tmp` → `/private/tmp`), or to check the way from the canonicalized parent. The failing tests are:
`gezik-batch`: `tasks::convert::tests::{swapping_in_trashes_the_original_as_it_is, a_failed_move_brings_the_original_back_from_the_trash}`; `compress`: `add_undo_restores_the_old_archive`, `a_part_that_cannot_land_brings_the_old_set_back`, `sevenz_parts_replace_and_keep_both_cover_the_whole_set`; `convert_tasks`: `an_existing_output_goes_through_the_conflict_list`, `remove_location_keeps_each_format_in_place`, `replacing_originals_trashes_them_and_undo_brings_back_their_bytes`, `text_replaces_its_original_and_undo_brings_it_back`, `a_command_in_place_is_undone_from_the_trashed_original`, `pausing_ends_a_command_and_runs_it_again_once_resumed`; `gezik-ops`: `engine::tests::{undo_of_trash_restores, undo_and_redo_a_copy, undo_of_replace_brings_the_old_file_back}`, `tasks::copy::tests::undo_redo_undo_of_a_copy_with_folders_leaves_nothing`, `tasks::group::tests::the_items_go_into_a_new_folder_and_one_undo_brings_them_back`, `tasks::link::tests::trashing_a_symbolic_link_leaves_its_folder`, `tasks::restore::tests::{a_missing_folder_is_made_again, a_folder_coming_back_onto_a_folder_asks, restoring_onto_a_taken_name_asks}`; `gezik-platform`: `fs::tests::trash_and_restore_round_trip`, `trash::tests::put_back_never_goes_through_a_link`.
Side effect: these tests move their files into the real `~/.Trash` and, because the restore fails, they leave them there.

**B. APFS refuses names that are not UTF-8 (2 tests).** `task::tests::a_small_copy_whose_name_the_journal_cannot_hold_takes_a_temporary_name` and `tasks::copy::tests::keep_both_keeps_a_name_that_is_not_unicode` fail with `Illegal byte sequence (os error 92)` when they create the test file. This cannot happen on macOS, so these tests should skip themselves there.

**C. Safari.app is a symlink on macOS 26 (2 tests).** `/Applications/Safari.app` → `../System/Cryptexes/App/System/Applications/Safari.app`.
- `mac::finder::tests::folders_have_finder_names_and_links_resolve` fails with `assertion failed: is_package(Path::new("/Applications/Safari.app"))`. If `is_package` doesn't follow the link, then in the app Safari in `/Applications` may be shown and opened as a link and not as an app. Item 92 checks this.
- `mac::icons::tests::apps_folders_and_types_have_icons` fails with `the Data volume's folder`, because `folder_has_own_icon("/Applications")` is false here.

**D. Windows-only test not gated (1).** `everything::tests::folder_sizes_need_every_folder_sized` builds `C:\Work\a` paths. On Unix `file_name()` of that is the whole string, so `left: [("C:\\Work\\a", 10), …]` is not `right: [("a", 10), …]`. The test should be `#[cfg(windows)]`.

**E. Link size (1).** `size::tests::a_link_loop_is_added_up_once` gives `left: 4, right: 74`. The walk counts the file but not the size of the symlink itself (the symlink's `lstat` length is 70 on APFS). Either the walk or the test is wrong on macOS.

Other leftovers seen: the instance tests leave about 20 `gezik-t*.sock`/`.lock` pairs in `$TMPDIR`, and earlier test runs left `gezik-watch-parent-*` folders.

### (extra) Gezik aborts every time it exits: FAIL

Closing the last window of a Gezik process (red button) ends the process with `SIGABRT`, and macOS shows a "gezik quit unexpectedly" report. It happened for all three windows closed in item 112, and two `.ips` reports were written in `~/Library/Logs/DiagnosticReports/`. The release log says
`thread 'main' panicked at …/std/src/thread/local.rs:429:25: cannot access a Thread Local Storage value during or after destruction: AccessError`. With a debug build (`RUST_BACKTRACE=1 ./target/debug/gezik --new-window …`, then the close button), the cause is:
```
3: LocalKey<RefCell<Option<gezik_platform::dnd::macos::Target>>>::with::<Registration::drop::{closure#0}, …>
4: <gezik_platform::dnd::macos::Registration as Drop>::drop   at ./crates/gezik-platform/src/dnd/macos.rs:41:16
…
10: core::ptr::drop_glue::<gezik::drag::Inner>
…
20: std::sys::thread_local::native::eager::destroy::<RefCell<Option<gezik::drag::Drags>>>
24: std::sys::thread_local::guard::apple::enable::run_dtors
fatal runtime error: thread local panicked on drop, aborting
```
At exit, `gezik::drag`'s thread-local `Drags` is destroyed. It still holds the `dnd::Attached` → `Registration`, whose `drop` calls `TARGET.with(…)`. But `TARGET`, another thread-local, has already been destroyed. The likely fix is `let _ = TARGET.try_with(|t| t.borrow_mut().take());` in `Registration::drop` (and the same for `ON_END` if it is touched in a drop), or dropping `Drags` before the event loop returns. Tabs and settings were saved before the abort (the session came back), so no data was lost. But every quit looks like a crash to the user, and macOS shows the report.

### (extra) Live settings reload through a symlinked config folder: FAIL

The checklist's `GEZIK_CONFIG_DIR=/tmp/gezik-cfg` breaks live reload: with it, no edit to `settings.toml` reached Gezik. Neither in-place writes nor `sed -i` (rename) did, and neither did `theme`, `icons` or `[[commands]]`. Only a restart picked them up. Started with `GEZIK_CONFIG_DIR=/private/tmp/gezik-cfg` (the same folder), every change applied within about 2 s, including after a `sed -i`. This was measured on a pixel of the list background, switching `theme` between `auto` (dark) and `light`: with `/tmp` 27,28,32 every time; with `/private/tmp` 27,28,32 → 255,255,255 → 27,28,32 → 255,255,255 → 27,28,32. The settings watcher most likely compares FSEvents' `/private/tmp/…` paths with the `/tmp/…` path it was given, like Run 1's folder-watch bug and bug A. The default config folder (`~/Library/Application Support/gezik`) has no link in it, so users are probably not affected, but anyone with a symlinked home or config folder would be. All the live-reload items below were run with `GEZIK_CONFIG_DIR=/private/tmp/gezik-cfg`.
(One earlier `theme` edit did apply with `/tmp/gezik-cfg`, in a process that had already written `settings.toml` itself, from the View menu. So the watch may work for a while after Gezik's own write.)

### Command line and single instance (9b1, 9b3)

#### 110. Tests first: PASS
`cargo test -p gezik-platform instance`: 27 passed.

#### 111. Hand-over: PASS
With Gezik open on `/tmp/gezik-test`, `gezik "/tmp/gezik-test/klasör A"` returned in 10 ms and the running window opened a `klasör A` tab. `gezik /tmp/gezik-test/belge-a.pdf` switched back to the `gezik-test` tab with `belge-a.pdf` selected, and Preview did not open. Only one `gezik` process was running throughout.

#### 112. Help, version, new window: FAIL (window offset)
- `--help` and `--version` (`gezik 0.1.0`) print to Terminal and exit 0. PASS.
- `gezik --new-window /tmp/gezik-test/1000-dosya` starts a second process with its own window, **but the window has exactly the same frame as the first** (System Events: both at 578,203, size 900×632). The new window covers the old one, with no 32 px offset.
- ⌘N and File ▸ New Window each start a new process (3 \`gezik\` processes in all), but **every new window opens at the same default frame (578,203)**. It is not 32 px from the window it was opened from: with the first window moved to 1020,60, both new windows still sat at 578,203, on top of each other. There is no Dock to check for a second icon (the Dock is hidden on this Mac).
- Closing the first window, then the other two, and starting Gezik again brought back the first window's four tabs (\`gezik-test\`, \`tmp\`, \`klasör A\`, \`dil\`), with \`dil\` in front. PASS.

#### 113. Hung or crashed Gezik: PASS
- After `kill -STOP <pid>`, `gezik "/tmp/gezik-test/Klasör B"` opened its own window after 2.3 s. Then `kill -CONT`.
- After `kill -9` of every gezik, `gezik /tmp/gezik-test` started a new first Gezik and `gezik /tmp` went to it (10 ms, one process).
- `ls -l "$TMPDIR"gezik-*`: `srw------- … gezik-9322862850c400ed.sock`, `-rw------- … gezik-9322862850c400ed.lock`.

#### 114. Off: PASS
With `[system] single-instance = false`, two calls made two processes, each with its own window.

#### 116. Tests first: FAIL
- `cargo test -p gezik-platform trash`: 28 passed, 2 failed (`put_back_never_goes_through_a_link`, `fs::tests::trash_and_restore_round_trip`, both A).
- `cargo test -p gezik-platform fs`: 27 passed, 1 failed (A).
- `cargo test -p gezik-ops delete`: 26 passed.
- `cargo test -p gezik-ops restore`: 5 passed, 4 failed (A).
- `ten_thousand`: both of these tests are `#[cfg(windows)]` in `trash/mod.rs`, so `cargo test --release -p gezik-platform ten_thousand -- --ignored` runs 0 tests on a Mac. There is no Mac timing test for 10,000 items.

#### 123. Tests first: PASS
`cargo test -p gezik-platform system`: 17 passed. `cargo test -p gezik system_changes`: 30 passed.

#### 128. `--unregister` (before anything was added): PASS so far
`gezik --unregister` with no journal printed `No system-changes.toml: taking back what has Gezik's names` and `Nothing of Gezik's was found.`, and exited 0. The runs after an Add are in the window part.

### Probes

#### 85. Probe (9a1): FAIL (one line)
`cargo run --release -p gezik-platform --example mac_probe -- belge-a.pdf foto-heic.heic klip.mov notlar.txt` (all in `/tmp/gezik-test`). No Pages file was given. There is no iCloud-only file.
- Every line in sections 1-3 is PASS, and "4 workers at once" did not crash.
- **"cancel after 5 ms" gave `724x1024 in 9 ms`. It should give none for a PDF or video.** The same PDF had just been thumbnailed in section 2, so Quick Look probably answered from its cache. The check needs a PDF that was not thumbnailed before.
- The probe prints no `folder_has_own_icon` times.
- `cargo test -p gezik-platform`: 4 failures (A ×2, C ×2, see above).

#### 94. Probe (9a2, section 4)
Section 4 is all PASS, and each file lists its default app first. "50 items" took 7.2 ms, under the 50 ms limit. Section 5 needs the "Gezik Test" Quick Action, which is in the window part.

#### 101. Probe (9a3, section 6): PASS
Every line is PASS, including "setuid kept across a group change". The probe found 133 users and 162 groups in 19.0 ms. `cargo test -p gezik-platform attrs`: 8 passed. `cargo test -p gezik-ops attrs`: 9 passed.

#### 130. Tests and probe (9b5): PASS
`cargo test -p gezik-platform cloud`: 14 passed. `~/Library/CloudStorage` doesn't exist (no OneDrive or Google Drive). `cloud_probe`:
```
1 cloud root(s):
  iCloud Drive                              /Users/macbookpro/Library/Mobile Documents/com~apple~CloudDocs
/Users/macbookpro/Library/Mobile Documents/com~apple~CloudDocs: under Some("iCloud Drive"), attributes -, state Local
```

#### 139. Tests and probe (9b4): PASS
The tests are the same as in 123. Probe section 7 says `public.folder handler: Some("com.apple.finder")`, `NSFileViewer (global): None` and `NSApp delegate class (before winit): None`.

#### Probe output (whole)
```
== 1. Icons: the main thread and a worker give the same pixels (spec 4.1) ==
PASS Path("/tmp/gezik-test/belge-a.pdf"): main 64x64 in 30.9 ms, worker 64x64
PASS Path("/tmp/gezik-test/foto-heic.heic"): main 64x64 in 24.9 ms, worker 64x64
PASS Path("/tmp/gezik-test/klip.mov"): main 64x64 in 10.3 ms, worker 64x64
PASS Path("/tmp/gezik-test/notlar.txt"): main 64x64 in 12.1 ms, worker 64x64
PASS Folder: main 64x64 in 0.6 ms, worker 64x64
PASS Extension("pdf"): main 64x64 in 1.1 ms, worker 64x64
PASS Extension(""): main 64x64 in 0.8 ms, worker 64x64
1000 type icons on a worker: 1000 found in 143 ms
PASS 4 workers at once, 1000 folder icons: 1000 in 60 ms
== 2. Quick Look thumbnails on a worker while the main thread waits (spec 4.5) ==
PASS /tmp/gezik-test/belge-a.pdf: 181x256 in 91 ms
PASS /tmp/gezik-test/foto-heic.heic: 192x256 in 325 ms
PASS /tmp/gezik-test/klip.mov: 256x165 in 281 ms
PASS /tmp/gezik-test/notlar.txt: 256x256 in 177 ms
cancel after 5 ms: 724x1024 in 9 ms (expect none, under ~100 ms; a PNG/JPEG may come at once)
== 3. Finder names, packages, aliases (spec 4.2) ==
/tmp/gezik-test/belge-a.pdf: finder name None, package false, alias NotAlias
/tmp/gezik-test/foto-heic.heic: finder name None, package false, alias NotAlias
/tmp/gezik-test/klip.mov: finder name None, package false, alias NotAlias
/tmp/gezik-test/notlar.txt: finder name None, package false, alias NotAlias
PASS make + resolve a.txt alias: Ok(()) → Target { path: "/private/var/folders/xf/wdnn6l_s02d83rk1c7rycyz80000gn/T/gezik-probe-78349/a.txt", is_dir: false }
PASS make + resolve folder alias: Ok(()) → Target { path: "/private/var/folders/xf/wdnn6l_s02d83rk1c7rycyz80000gn/T/gezik-probe-78349/folder", is_dir: true }
PASS make + resolve Safari alias: Ok(()) → Target { path: "/System/Cryptexes/App/System/Applications/Safari.app", is_dir: true }
PASS a second alias of the same name is refused: Err(Os { code: 17, kind: AlreadyExists, message: "File exists" })
PASS original gone: Missing
== 4. Open With: LaunchServices on the main thread and a worker (spec 4.3) ==
PASS /tmp/gezik-test/belge-a.pdf: 29.6 ms main, 1.6 ms worker: Adobe Acrobat (default), Adobe Illustrator 2023, Adobe Illustrator 2024, Adobe Illustrator 2026, Adobe Photoshop (Beta), Adobe Photoshop 2024, Adobe Photoshop 2026, Books, ColorSync Utility, Firefox Developer Edition, Google Chrome, Google Chrome for Testing, HP, Preview, Safari, Save as Adobe PDF
PASS /tmp/gezik-test/foto-heic.heic: 3.0 ms main, 1.4 ms worker: Preview (default), Adobe Illustrator 2023, Adobe Illustrator 2024, Adobe Illustrator 2026, Adobe Photoshop (Beta), Adobe Photoshop 2024, Adobe Photoshop 2026, ColorSync Utility, HP, Image Playground, Safari, WhatsApp
PASS /tmp/gezik-test/klip.mov: 0.5 ms main, 0.4 ms worker: QuickTime Player (default), TV, WhatsApp
PASS /tmp/gezik-test/notlar.txt: 2.2 ms main, 1.5 ms worker: TextEdit (default), Adobe Illustrator 2023, Adobe Illustrator 2024, Adobe Illustrator 2026, Firefox Developer Edition, Google Chrome, Google Chrome for Testing, HP, Instruments, Notes, Numbers, Pages, Safari, Script Editor, Sublime Text, Xcode
apps for all 4 files: 0 in 0.7 ms
50 items (the menu waits 50 ms): 0 apps in 7.2 ms
== 5. Quick Actions: installed bundles, types, running one (spec 4.3, decision 26) ==
/tmp/gezik-test/belge-a.pdf: 0 in 0.1 ms: 
/tmp/gezik-test/foto-heic.heic: 0 in 0.1 ms: 
/tmp/gezik-test/klip.mov: 0 in 0.0 ms: 
/tmp/gezik-test/notlar.txt: 0 in 0.0 ms: 
pbs -dump_pboard: 37329 bytes, 5 NSSendFileTypes, 51 NSMenuItem in 25 ms (all services, apps' too)
(make a Quick Action in Automator or Shortcuts that takes files, then rerun with --service "<its name>")
== 6. Get Info: attributes without following links, names, ACLs (spec 4.4) ==
133 users, 162 groups, mine [12, 20, 33, 61, 79, 80, 81, 98, 100, 204, 250, 395, 398, 399, 400, 701] in 19.0 ms
PASS hide and lock
PASS a locked file's permissions are refused
PASS unlock and chmod in one write
PASS unhide
PASS a link's permissions are refused
PASS what the link leads to is untouched
PASS a link's own group (lchown)
PASS setuid kept across a group change
PASS no ACL at first
PASS an access control entry is seen
== 7. Default file manager: the folder handler, NSFileViewer, the app delegate (9b4) ==
public.folder handler: Some("com.apple.finder")
NSFileViewer (global): None
NSApp delegate class (before winit): None
(leave /var/folders/xf/wdnn6l_s02d83rk1c7rycyz80000gn/T/gezik-probe-78349 for Finder: its aliases should show the arrow badge; delete it afterwards)
```

#### 93 / 100 / 109. Exe size
`ls -l target/release/gezik` on `master` `cbf1ab0`: 20,942,592 bytes. (Run 1's `feat/batch-ops-5c` build was 15,755,408 bytes.)

### Window items

#### 23. Live refresh through a symlink: PASS
With Gezik on `/tmp/gezik-test`, `touch /tmp/gezik-test/canli.txt` showed up in the list within a second, with no ⌘R.

#### 24. ⌘[ / ⌘] by key place: PASS
On Turkish-QWERTY-PC, ⌘ğ (the key right of P) went back from `klasör A/alt` to `klasör A`, and ⌘ü went forward again. Go ▸ Back and Go ▸ Forward did the same; they were chosen through Accessibility (System Events `click menu item`), because the menu bar is set to hide itself on this Mac. The Go menu items carry `⌘[` / `⌘]` as their key equivalents (AX `AXMenuItemCmdChar`).

#### 25. Finder → Gezik drops: PASS
Finder list view beside Gezik; drags were synthetic CGEvents.
- Onto the `Klasör B` row: the row was highlighted, the label said "Move to Klasör B", and `d1.txt` moved from `/tmp/gezik-kaynak` into `Klasör B` (same volume).
- With ⌥ held: "Copy to Klasör B", and `d3.txt` was copied (the source stayed).
- Onto the `cok-kok.zip` row: "Add to cok-kok.zip", and `unzip -l` lists `d3.txt` next to the old two files.

#### 26. Tab-rest drag: FAIL
Drags inside Gezik from `gezik-test` onto the `klasör A` tab, as synthetic CGEvents.
- Rest on the tab (1.8 s) until it opens, then release on the tab: `a-r1.txt` moved. PASS.
- Rest, then release over the new list: `a-r2.txt` moved. PASS.
- Quick drop on the tab, with no rest: `a-r3.txt` moved. PASS.
- Rest, then drag out of the window into a Finder window and drop there: Finder got `a-r4.txt` (moved, same volume). Coming back over Gezik left no ghost. PASS.
- **Right-drag, rest on the tab, release:** only the Copy here / Move here / Create link here / Cancel menu appeared, with no second context menu after it. PASS for that part. **But the menu does not follow the mouse.** Hovering "Move here" doesn't highlight it, and clicking it closes the menu with nothing done (tried twice, `a-r5.txt` and `a-r6.txt` stayed). With the keyboard (↓, Return) the same menu works: `a-r5.txt` moved. The ordinary right-click menu in the same window does follow the same synthetic mouse (hover, then clicking Copy put the file on the clipboard). So this is specific to the menu opened at the end of a right-drag; it should be checked once with a real mouse to rule out the synthetic events.
- Screenshot: `run2/h3.png` (not committed).

#### 27. Sidebar drop onto another volume: PASS
Dragging `a-r6.txt` from `/tmp/gezik-test` onto GezikHedef in the sidebar highlighted the row and said "Copy to GezikHedef". The file was copied: it is on `/Volumes/GezikHedef` and still in `/tmp/gezik-test`.

#### 28. Language: PASS
Started with no locale variables (`env -u LANG -u LC_ALL -u LC_CTYPE …`), with Turkish put first for the process only (`-AppleLanguages "(tr-TR, en-TR)"`, the NSUserDefaults argument domain, which `NSLocale.preferredLanguages` reads). The system setting was not changed. Rename layer ▸ Change case ▸ UPPER on `bilgi.txt`, `ilik.txt`, `liste.txt` previews `BİLGİ.txt`, `İLİK.txt`, `LİSTE.txt`. Started the same way without the argument (English first, as on this Mac), the preview is `BILGI`, `ILIK`, `LISTE`.
Side note: Gezik's command line prints `unknown option -AppleLanguages (ignored)` and takes `(tr-TR, en-TR)` as a path (`not found; opening the last tabs`). The status bar shows this, and the stderr lines are printed four times. macOS passes `-NSDocumentRevisionsDebugMode`-style `-Key value` pairs to apps started from Xcode, so ignoring a `-AppleXxx value` pair silently may be worth it.

#### 29. Menu bar: PASS
- The menus are File, Edit, View, Go and Window. The app menu is titled "Gezik", and its items read "About gezik", "Hide gezik" and "Quit gezik" (known).
- ⌘M minimizes: AX `AXMinimized` is true.
- View ▸ as Grid and View ▸ as List (chosen through Accessibility, because this Mac's menu bar hides itself) switch the view.
- Enter on `bilgi.txt` starts the name edit, and ⌘C copies the selected text: `pbpaste` gives `bilgi`. After Esc, ⌘C in the list puts the file on the clipboard (`/tmp/gezik-test/dil/bilgi.txt` as a file URL).
- 18 View menu items in a row (as Grid, as List, Show Hidden Items ×2, Folders First ×2, three times), about 0.3 s apart: no crash.

#### 115. Bring to front: FAIL
- Gezik minimized with ⌘M, Finder in front, then `gezik /tmp/gezik-test/dil` from Terminal: the window is un-minimized (`AXMinimized` false) and switches to the folder, **but Finder stays the active app**. Gezik's window comes back inactive (grey traffic lights) behind whatever is in front. This happened both times it was tried.
- Not minimized, Finder in front, `gezik /tmp/gezik-test`: Gezik becomes the active app. PASS.
- So the un-minimize path doesn't activate the app. A likely cause is macOS 14+'s cooperative activation, which may ignore a self-activation that comes right after `set_minimized(false)`. The Dock can't be watched for a bounce on this Mac.

#### 30. Wording and dotfiles: PASS
- The address bar starts with "Computer", and the sidebar sections are FOLDERS, CLOUD and LOCATIONS.
- The Type column uses Finder's kinds ("Folder", "Plain Text Document", "PDF document", "QuickTime movie"). A `.rar` with no app for it says "Document".
- Dotfiles are hidden by default, and `settings.toml` gets `show-hidden = false`. ⇧⌘. (the `.` key on Turkish-QWERTY-PC) shows them (`.DS_Store`, `.gizli`), and View ▸ Show Hidden Items hides them again.
- Ctrl+H does nothing to the list. ⌘H hides the app: System Events `visible` is false.

#### 31. Menus at the window edge: PASS
Window in the middle of the screen (528,235, 1000×860, not maximized). Every menu was checked on a full-screen `screencapture -x`. These are native macOS menus:
- Context menu on `tek-kok.zip`, the last row near the bottom-right corner: it opens right of the pointer, past the window's edge, and is longer than the space below. It stops at the screen's bottom with a scroll arrow (⌄), and nothing is cut.
- Context menu on `seffaf.png` ▸ Commands ▸: the submenu opens to the right, past the window's edge, fully on screen. "Missing program — no-such-tool not found" is grey.
- Rename layer ▸ Presets: the menu hangs past the window's right edge, fully on screen.
- Convert layer ▸ Preset: a list with Image / PDF / Commands headings, fully on screen. (From and To are only in the text form of the layer, item 36.)
- The View button's menu (List … System Integration…): hangs past the right edge, fully on screen.
This Run used full-screen shots, and in all of them menus pass the window frame and are complete. So Run 1's "Presets menu runs past the right edge" and "context menu cut off at the bottom" were most likely window captures that cut at the frame.

#### 32. Compress name: PASS
- The layer opens with only the stem selected (`notlar` in `notlar.zip`).
- 7z chosen, `sifreli` typed: `sifreli.7z` (7-zip archive data).
- With zip chosen, typing only `.zip` makes `oku.zip` (not `.zip.zip`).
- **Note:** the layer remembers the last format, so it reopened on 7z. Typing only `.zip` there made **`.zip.7z`**, a dot file that is hidden by default on macOS. That is what was asked for in a sense, but it looks like nothing was made. Taking a typed `.zip` as "zip, named after the item" would match the zip case.
- After the 7z jobs, the row menu still says `Compress to "notlar.zip"`, and it made a plain zip (`Zip archive data … deflate`).
- Split shows None every time the layer opens (in 7z), and `state.toml` has no split size (no `[compress]` table at all).

#### 33. ⌘ / Ctrl shortcuts: PASS
- ⌘Enter starts each layer from a field inside it: Compress from the Name field (`sifreli.7z`, `oku.zip`), Rename from the Replace field (`bilgi.txt` → `BILgi.txt`), and Convert from the Quality field (`seffaf.png` → `seffaf.jpg`).
- Ctrl+Enter in the rename layer (with nothing that would change) did nothing visible, and the layer stayed open.
- Esc closes the rename, Compress and Convert layers.
- ⌘Z / ⇧⌘Z after each job: the rename (`BILgi.txt` ↔ `bilgi.txt`), the compress (`notlar.zip` gone and back) and the convert (`seffaf.jpg` gone and back) all undo and redo. **In `/tmp/gezik-test` the redo of the compress failed** with "notlar.zip: /tmp is a link: put the item back by hand" (bug A: the redo puts the archive back from the Trash). In `/private/tmp/gezik-test` it works. From here on, undo checks were done in `/private/tmp/gezik-test`.
- Up and Down: Gezik's menus here are native NSMenus (context menu, View button, Presets, Convert's Preset list), so macOS skips the greyed lines itself.
- Leftover: when the rename layer reopens, its Find field still holds the last text (`bil`), so typing appends to it (`bilste`).


#### 34. Archive kinds and icons: FAIL
Made in `arsiv/`: `big.7z.001`/`.002` (a Gezik-made 7z cut in two with `dd`; the parts join back to the same bytes), `film.001`/`.002` (copies of those), `x.wim` (an `MSWIM` header plus random bytes) and `clip.264` (random bytes).
- Types: `big.7z.001`/`.002` say "Split 7Z archive", `film.001`/`.002` say "Split archive", and `kucuk.7z` says "7-Zip Archive". **`x.wim` says "Document", not "WIM archive".**
- Icons: with `icons = "system"` every one of them has Finder's plain document icon. No archive icon appears for `.wim`, `.7z` or `.001`, because macOS has no app for those types here. Gezik's own icons could not be compared live (see the settings reload finding above).
- Extract here / Extract to "big/" / Extract to… are offered on `big.7z.001`, on the bare `film.001` ("Extract to "film/"") and on `x.wim`.
- `clip.264` is "Document", keeps its own name, and its menu has no Extract.

#### 35. Pictures: PASS
Run in `/private/tmp/gezik-test` (see bug A for `/tmp`). EXIF was read with a small Python TIFF reader, because Spotlight doesn't index `/tmp`, so `mdls` gives `(null)` for every key there.
- Right-click a JPEG ▸ Convert… opens the layer. The menu also has Commands ▸ with "Missing program — no-such-tool not found" greyed. With two items selected, the menu also offers "Images to PDF…".
- "Resize photos (JPEG 1920 px)", Subfolder "converted", ⌘Enter, on `portre-o6.jpg` (stored 1600×1200, orientation 6) and `foto1.jpg` (2400×1600): the outputs are `converted/portre-o6.jpg` 1200×1600 with orientation 1 (upright, the same picture Preview shows for the original) and `converted/foto1.jpg` 1920×1280.
- With "Remove metadata" off, JPEG → JPEG keeps EXIF, Make/Model and the GPS IFD. With it on, `converted/gps-kopya.jpg` has no EXIF block at all.
- `seffaf.png` (alpha) → JPEG: the transparent parts are white.
- "Replace originals" on `degistir.jpg`: the file was replaced (md5 changed, 87,653 → 66,976 bytes), the original went to the Trash, and ⌘Z brought back the original bytes (md5 `d6c21f34…` again).
- "Remove location data" on `konum.jpg` (a copy of `foto1.jpg` with GPS): the GPS IFD now holds only `GPSVersionID` (latitude and longitude are gone). The scan data after SOS is byte-for-byte the same (not re-encoded), and the file is 58 bytes smaller (115,337 → 115,279).

#### 36. Text: PASS
- `tr1254.txt` (`fd6c fd6b 20dd …`): the layer says "Detected: Windows-1254 (1 file)". Convert to UTF-8 with Replace originals gives `ılık İstanbul\r\n` (`c4b1 6cc4 b16b 20c4 b073 …`), checked with `cat`/`xxd` rather than TextEdit. ⌘Z gives the old bytes back (`xxd` identical).
- `utf8-i.txt` (`ılık utf8 satır`) → Windows-1252: the operations panel says "1 item failed", and Details says `utf8-i.txt: can't encode 'ı' in Windows-1252 (line 1)` with Retry / Close. The file is unchanged (same md5).
- The To list offers Keep each file's, UTF-8, UTF-16 LE/BE, Windows-1254 (ISO-8859-9), Windows-1252 (ISO-8859-1) and about 25 more.

#### 37. HEIC and the ffmpeg download (Gatekeeper, quarantine): PASS
No ffmpeg on PATH (`which ffmpeg`: not found).
- Convert `foto-heic.heic` → "Convert to JPEG": the box "ffmpeg needed — Reading HEIC and AVIF pictures needs ffmpeg 9 or newer (~20.2 MB, free)." appeared, with Download / Where does it come from? / Cancel. "Where does it come from?" opened `github.com/wenlar/gezik-tools/releases/tag/ffmpeg-9.0.2-1` in the default browser (Firefox).
- Download: `tools/ffmpeg-9.0.2/` has `ffmpeg`, `ffprobe`, `LICENSE` and `SOURCE.txt`, and the conversion went on by itself (`foto-heic.jpg` appeared without a second click).
- `file`: `Mach-O 64-bit executable arm64`. `xattr -l`: nothing (no `com.apple.quarantine`). `codesign -dv`: `Identifier=ffmpeg`, `flags=0x10000(runtime)`, `TeamIdentifier=KU3N25YGLU`, timestamp 20 Sep 2026. `spctl -a -vv`: `rejected (the code is valid but does not seem to be an app)`, `origin=Developer ID Application: Martin Riedl (KU3N25YGLU)` (normal for a command-line tool). There was no Gatekeeper prompt.
- **Quarantine test:** `xattr -w com.apple.quarantine "0081;…;Safari;"` on `ffmpeg`, then ⌘Q, Gezik started again and the HEIC converted again. It ran: no Gatekeeper prompt, no error, and the output appeared. After `xattr -d` it ran again as well.
- Result: 3024×4032, upright, sRGB (the source was written by ImageIO in sRGB, so there is no Display P3 shift to compare). It is the whole picture, not one 512 px tile, but the HEIC was made with ImageIO, **not** a real iPhone photo.
- `foto2.jpg` → WebP (lossy): `foto2.webp` (VP8, 2000×1500, 20,108 bytes). → AVIF: `foto2.avif` (ISO Media, AVIF Image, 2000×1500, 10,740 bytes). Both go through ffmpeg.

#### 38. Audio and video: FAIL
Sources: `klip.mov` (4 s screen recording with **no audio track**), `uzun.mov` (2 min 1080p H.264 + AAC) and `cok-uzun.mov` (10 min 4K H.264 + AAC), both made with the downloaded ffmpeg.
- On `uzun.mov`: MP3 → `uzun.mp3` (mp3, 120 s). M4A (AAC) → `uzun.m4a` (aac, 120 s). Smaller video → `uzun.mp4` (h264 1920×1080 + aac, 120 s, 42.6 MB from 179.7 MB). Remux to MP4 → h264 + aac, 120 s, same size (179.7 MB). GIF from video → `uzun.gif` 480×270, 120 s. `afinfo` reads all four audio/video files, and QuickTime Player opened `uzun.mp4` (duration 120.0).
- `klip.mov` → GIF: `klip.gif` 480×310, 4.08 s. PASS.
- **`klip.mov` → MP3 and → M4A both fail.** The row says "1 item failed", and Details shows ffmpeg's raw stderr: `klip.mov: ffmpeg failed (exit code 234): [out#0/mp3 @ 0x…] Output file does not contain any stream / Error opening output file /private/tmp/gezik-test/.gezik-copying-9090c66d50688b44-0. / Error opening output files: Invalid argument`. The file has no audio, so failing is right, but the message should say "klip.mov has no audio", and Convert could check the streams with ffprobe first (it already downloaded it). A screen recording made with ⇧⌘5 has no sound by default, so this is the common case on a Mac. No half file was left.
- Progress on the 10-min 4K "Smaller video" moved smoothly (2 %, 5 %, 8 %, 10 %, 13 % every 2 s, ~19 MB/s, time left ~1:10).
- Cancel at 39 %: the row went away, `pgrep -l ffmpeg` was empty, and the `.gezik-copying-…` part file (22.8 MB) was removed. No half output was left.
- Also seen: "Smaller video" and "Remux to MP4" both write `<name>.mp4`, so the second one needs the conflict list or a rename.

#### 39. Pause and resume: PASS
- Pause on the 10-min 4K "Smaller video" at 13 %: the row says "Paused", `pgrep -l ffmpeg` is empty within 2 s, and the `.gezik-copying-…` part file is gone.
- Resume: a new ffmpeg starts and the item starts over at 0 % ("2 % … ~10:28", then normal speed). The output `cok-uzun.mp4` is complete: h264 1920×1080 + aac, 600 s, 58.5 MB.
- The same with a user command is covered under 40.

#### 40. User commands: PASS
The four commands from the checklist and a fifth one, "Slow copy (sleep)" (`sh -c 'sleep 20; cp …'`), were added to `settings.toml`.
- **Live pick-up:** with `GEZIK_CONFIG_DIR=/tmp/gezik-cfg` the commands did not show until a restart (see the live-reload finding). With `/private/tmp/gezik-cfg`, "Slow copy" was in the menu within 2 s, without a restart.
- Right-click ▸ Commands ▸ and the Convert layer's Commands list show only the ones for the file's type: on `.txt`, Thumbnail, Missing program and Slow copy; on `.png`/`.jpg`, Half size, Thumbnail, Rotate and Missing program. "Missing program — no-such-tool not found" is grey.
- `{out}` ("Half size (sips)") on `it's a "test".jpg`: `it's a "test"-small.jpg`, 800×600, next to the input. The name with spaces and quotes works.
- `{outdir}` ("Thumbnail (qlmanage)") on `belge-a.pdf`: `belge-a.pdf.png` (180×256) next to the input. While it ran (polled every 0.5 s) nothing half-made showed in the folder. For "Slow copy", the output went to a hidden staging folder in the same folder, `.gezik-deleting-x-<pid>-<n>/notlar-slow.txt`, until it was done. That works, but the folder's name says "deleting" for a folder that holds work in progress.
- A second run of "Half size" on the same file opens the conflict list ("1 conflict · Running Half size (sips) on it's a "test".jpg", Replace / Skip / Keep both / If newer).
- "Rotate in place" on `dondur.jpg`: 2000 → 1500 px wide (rotated), and ⌘Z gave back the original (same md5 `d6c21f34…`, 2000 px).
- A command with a misspelt key (`runn`) is left out, and the status bar says `settings.toml: commands[6]: unknown key "runn" (known: name, run, output, types, folders, parallel, shortcut, menu, ask)`.
- Pause/resume of a user command (from 39): pausing "Slow copy" ended `sh` and `sleep` within 2 s and removed its staging folder. Resume ran it again from the start, and `notlar-slow.txt` (`merhaba`) appeared after 20 s.
- Note: in the native Commands ▸ submenu, a synthetic mouse click on an item closed nothing and ran nothing (the menu stayed open), while choosing it with the keyboard worked. This is like item 26's drop menu. Check with a real mouse.

#### 41. Images to PDF: PASS
In `pdf-resim/` (copies of `portre-o6.jpg` with orientation 6, `foto1.jpg`, `foto2.jpg`, `seffaf.png` with alpha, and `foto-heic.heic`): select all ▸ right-click ▸ "Images to PDF…".
- The HEIC is left out with the note "1 item skipped: not a picture Gezik reads itself". 4 pages are listed.
- Dragging `seffaf.png` from 4th to 1st in the Page order list works (the list renumbers). A4 and margin Small, then ⌘Enter.
- `pdf-resim.pdf` (318,692 bytes, 4 pages, not encrypted), rendered with CoreGraphics: the pages are in the dragged order (seffaf, foto1, foto2, portre). The three landscape pictures sit on landscape A4 (841×595 pt), and the portrait one on portrait A4 (595×841 pt), upright, the same as Preview shows the JPEG. The small margin is there.
- The PNG keeps its transparency in the PDF (no white fill), unlike the JPEG conversion in 35.

#### 42. pdfium download (does the dylib load?): PASS
- `belge-a.pdf` + `belge-b.pdf` ▸ Convert… ▸ Merge PDFs (the layer lists the merge order and says "Bookmarks, forms and document info are not kept. An encrypted PDF gives an unencrypted copy."). ⌘Enter opened "pdfium needed — PDF tools need pdfium (~2.3 MB, free)." with Download / Where does it come from? / Cancel.
- Download: `tools/pdfium-8086/` has `libpdfium.dylib` (7,339,520 bytes), `LICENSE`, `licenses/` (17 files) and `SOURCE.txt`. The merge went on by itself: `belge-a (merged).pdf` has 8 pages (3 + 5), not encrypted. Gezik selects the result in the list. It does **not** open it in Preview (PDFs open in Adobe Acrobat on this Mac anyway).
- dylib: `file`: `Mach-O 64-bit dynamically linked shared library arm64`. `xattr -l`: nothing. `codesign -dv`: `flags=0x20002(adhoc,linker-signed)`, `Signature=adhoc`, `TeamIdentifier=not set`. `spctl -a -vv -t open --context context:primary-signature`: `rejected`.
- **Quarantine test:** with `com.apple.quarantine` on the dylib, ⌘Q, Gezik started again, and Split PDF ▸ Each page on `belge-b.pdf`: the dylib loaded (5 files, `belge-b - page 1.pdf` … `page 5.pdf`, no error, no prompt). After `xattr -d` it loads as before.
- `codesign -dv --verbose=4 target/release/gezik`: `Identifier=gezik-1c86a0f1bd2c8619`, `flags=0x20002(adhoc,linker-signed)`, `Signature=adhoc`, `TeamIdentifier=not set`, no hardened runtime. So library validation does not apply to this build. A signed, notarized `Gezik.app` with the hardened runtime would refuse this ad-hoc-signed dylib unless it has `com.apple.security.cs.disable-library-validation`, or the dylib is re-signed with the same team. This is still open.
- `GEZIK_TEST_PDFIUM=…/libpdfium.dylib cargo test -p gezik-batch --test pdf_worker --test pdf_tasks`: `pdf_tasks` 17 passed. `pdf_worker` 10 passed, 1 failed: `odd_names_open_and_name_their_outputs` fails with `Illegal byte sequence (os error 92)` when it makes its non-UTF-8 file name (APFS, as B above).

#### 43. Split, extract, to images: PASS
In `pdf-test/` (copies of `belge-b.pdf` (5 pages), `buyuk-240.pdf` and `sifreli.pdf`), in `/private/tmp`.
- Split PDF ▸ Each page says "Makes 5 files" and made `belge-b - page 1.pdf` … `page 5.pdf` (in 42).
- Ranges `1-2, 5`: "Makes 2 files", and the outputs are `belge-b - pages 1-2.pdf` (2 pages) and `belge-b - page 5.pdf` (1 page).
- A range past the end (`1-2, 9`) says "Page 9 is past the end (5 pages)" in red while typing, before starting.
- Extract pages has no output-name field: the name comes from the source. With a source named `çıktı ğüşö.pdf`, Extract pages `2-3` ("Makes 1 PDF of 2 pages") made `çıktı ğüşö - pages 2-3.pdf` with 2 pages, so a Turkish name works.
- PDF to images, PNG 150 dpi: 5 files `belge-b - page N.png`, 1240×1754 px (A4 at 150 dpi), and the text is sharp. JPEG 72 dpi: 5 files, 595×842 px.
- Running the PNG job again opens the conflict list ("5 conflicts · Saving belge-b.pdf as PNG pictures"). ⌘Z after the JPEG job removed all 5 JPEGs in one step and left the PNGs.
- **Note:** Split ▸ Ranges with the Ranges field **empty** started at once and made `çıktı ğüşö - pages 1-5.pdf`, a copy of the whole file. Empty should probably keep Convert disabled, or say "type the ranges".
- The layer reopens with the last preset that was run for that kind (Split ▸ Ranges after a split, Extract after an extract).

#### 44. Encrypted PDF: PASS
`sifreli.pdf` (4 pages, written by CoreGraphics with user password `gezik`).
- The layer can't count its pages: Split says "Page count unknown (encrypted PDF)", and with two items "Makes 5 files, more from 1 encrypted PDF".
- Split ▸ Each page: the box "Password — sifreli.pdf is encrypted. Password:" appears, and the operations row says "Waiting for your decisions". The field shows dots (`••••••`), and Show reveals `yanlis`.
- A wrong password asks again: "Wrong password. Try again:". `gezik` works: 4 files `sifreli - page 1.pdf` … `page 4.pdf`, each 1 page and **not encrypted** (CoreGraphics `isEncrypted` false). The layer had said so ("An encrypted PDF gives an unencrypted copy.").
- Merge `belge-b.pdf` + `sifreli.pdf`, then Skip at the password: the row says "Done · 1 item skipped", Details says "sifreli.pdf: no password given; nothing was merged", and no merged file was written.

#### 45. No worker left behind: PASS
- After every PDF job above (merge, split, extract, PDF to images, the password cases), `pgrep -fl -- --pdf-worker` was empty.
- "PDF to images" PNG 300 dpi on the 240-page PDF finished in about 3 s (240 PNGs of ~178 KB), too fast to pause, so a 2,000-page PDF (`dev-2000.pdf`, CoreGraphics) was used: "Makes 2,000 pictures".
- While it runs, one `gezik --pdf-worker` process does the work, and the pages land in a hidden staging folder (`.gezik-deleting-x-<pid>-<n>`), not in the folder.
- Pause at ~3 %: the row says "Paused", the worker is gone within 2 s, and the staging folder stays (with its finished pages). Resume: a new worker starts ("8 %"). Cancel: within 3 s no worker is left, no `dev-2000 - page …` file is in the folder, and the staging folder and any `.gezik-*` folder are gone.
- `lsof -p $(pgrep -x gezik) | grep -i pdfium` is empty before and after: the library is only loaded in the worker.
- Gezik's RSS (`ps -o rss=`): 127,680 KB before the PDF jobs, 129,280 KB after the 240-page job and its undo, 129,360 KB after the 2,000-page cancel.
- A 240-page undo (⌘Z) removed all 240 PNGs at once.

#### 46. Last choices and keys: PASS
- `state.toml` has `[convert]` with `last-preset`, `image`, `text`, `media`, `pdf` and `last-output` lines, for example `pdf = "op=to-images split=each every=10 dpi=300 image=png size=a4 margin=small"` and `image = "preset=to-webp format=webp quality=85 …"`.
- Opened again on `foto1.jpg` after PDF jobs, the layer comes back with the picture's last choice (Convert to WebP, WebP, Rotate by EXIF on). On `belge-b.pdf` it comes back with the PDF's (PDF to images, PNG, 300 dpi). Each kind keeps its own.
- The range text does not come back: the Ranges and Pages fields are empty each time (seen in 43 and 44), and `state.toml` has no range.
- Esc closes the layer and ⌘Enter starts it (used throughout 35-45).
- **Note:** the output choice is shared, not per kind (`last-output`): "Replace originals" chosen for `degistir.jpg` in 35 was still chosen when the layer next opened on `tr1254.txt` in 36. That is easy to miss for a choice that sends originals to the Trash.

#### 47. Filter: PASS
In `filtre/` (8 items: `İSTANBUL.txt`, `ILIK.doc`, `a.jpg`, `b.jpg`, `c.png`, `d.txt`, `e.md`, `jpg-klasor/`).
- ⌘F opens a bar above the list with the field focused. `jpg` leaves `jpg-klasor`, `a.jpg`, `b.jpg`, and the counter says "3 / 8". There is also a "Search subfolders" button and a ▾ menu.
- ↓ gives the list the keyboard and the bar stays. **Enter on the filtered folder starts a rename** (macOS's Enter = rename, as item 3 asks), so it does not open. ⌘↓ opens it (`jpg-klasor`, without a filter there). Back returns to `filtre` without the filter.
- Esc on the list closes the filter. A second Esc leaves only the focus outline (selection cleared). Esc in the field closes the bar too.
- `/` on the list opens the filter: on Turkish-QWERTY-PC `/` is ⇧7 (the key at the ANSI `/` place types `.` and does nothing here).
- Each tab keeps its own filter: `png` in the `filtre` tab (1 / 8) and `il` in the `dil` tab (2 / 3) were both still there after switching back and forth.
- `istanbul` finds `İSTANBUL.txt` (1 / 8), and `ılık` finds `ILIK.doc` (1 / 8). Matching folds both ways: `il` also matches `BILgi.txt`, which strict Turkish casing (`BIL` → `bıl`) would not. That is friendly for a filter.

#### 48. Pattern box and selection keys: PASS
- Edit ▸ Select by Pattern… (chosen through Accessibility) opens "Select by pattern" with "8 items match" for an empty field. `*.jpg` says "2 items match", and Return selects `a.jpg`, `b.jpg`. A bad pattern `!` says "Type a name after \"!\"" in red. The box reopens with the last pattern (`*.jpg`, selected).
- ⌘⇧I inverts the selection (6 of 8 selected).
- The Edit menu shows Filter… ⌘F, Select by Pattern… ⌘=, Deselect by Pattern… ⌘-, Invert Selection ⌘⇧I, Select Same Type and Restore Selection (no keys). Window ▸ Reopen Closed Tab shows ⌘⇧T (AX key equivalents).
- **Turkish-QWERTY-PC:** `=` is ⇧0, and ⌘⇧0 does nothing by default. ⌘ + the key that types `-` (right of 0) opens "Deselect by pattern". With `select-pattern = ["num+", "mod+shift+0"]` under `[shortcuts]`, ⌘⇧0 opens Select by pattern (picked up live, with the config under `/private/tmp`).
- Not tested: the numeric keypad keys (keypad +, -, /, ⌥+keypad +); there is no external keyboard with a keypad here.

#### 49. Tabs: PASS
Window title was used to tell the tab in front.
- With four tabs (filtre, tmp, klasör A, dil), ⌘1…⌘4 switch to each. ⌘7 does nothing, and ⌘9 shows the last tab (from tab 1 too).
- ⌘⇧2 shows the grid and ⌘⇧1 the list. View ▸ as Grid / as List carry ⌘⇧2 / ⌘⇧1 (AX key equivalents). The digit keys are the same on Turkish Q.
- New tab (⌘T, opened next to the current one), `klasör A` then `alt` (two folders deep), filter `b` (1 / 1), then ⌘W: the tab closes. ⌘⇧T: it comes back in its place, still filtered (`b`, 1 / 1). ⌘[ goes back to `klasör A`.
- ⌘⇧A opens the tab picker (name + path for each tab, the current one marked). Typing `di` leaves `dil`, Return switches to it, and Esc closes the picker.
- Right-click a tab ▸ Lock tab: a lock shows before the name and the × goes. ⌘W leaves it open, and the status bar says "This tab is locked". The locked tab's menu is Duplicate / Unlock tab / Close other tabs / Save tabs as… (no Close). Unlock tab brings the × back.
- "Close other tabs" on the locked tab (chosen by mistake) closed the other four. ⌘⇧T four times brought them all back, in order.

#### 50. Typing mode: PASS
`[keyboard] typing = "filter"` was picked up live: `j` on the list opened the filter with `j` in it (3 / 8). Back to `"jump"` (live), `d` jumped to `d.txt`.

#### 51. Saved filters: PASS
- The bar's ▾ ▸ Save as… opens "Save filter — Name for this filter:". `Resimler` for the pattern `jpg` wrote `[[filters]] name = "Resimler" pattern = "jpg"` to `settings.toml`.
- With the field empty, ▾ lists Resimler, Save as… (grey) and Delete "Resimler". Resimler fills the bar with `jpg` (3 / 8). Delete "Resimler" took it out of `settings.toml`.
- **Note:** the ▾ menu is a native menu that opens with no item highlighted. A Return right after it opens does not pick the only item (Save as…). Instead it goes back to the filter field, and the text typed next replaced the filter (`Resimler` became the filter). ↓ then Return picks it. This matches how native NSMenus behave, but the filter field getting the keystrokes is surprising.

#### 52. Path suggestions: PASS
- ⌘L, `/Us`: after a short pause, a list under the field shows `Users` and `usr` (folders only, case-insensitive) and a "History" heading with `macbookpro  /Users`.
- ↓ marks `Users`, and Tab writes `/Users/` and shows its sub-folders (`macbookpro`, `Shared`).
- Esc closes the list and the text stays in the field. A second Esc ends typing and the path parts come back.
- `~/Desktop`, `$HOME/Downloads` and `${HOME}/Music` go to those folders.
- `$GEZIK_NOPE/x` stays as it is: the status bar says "Cannot open /Users/macbookpro/Music/$GEZIK_NOPE/x: It no longer exists", and nothing opens. `~veli` is not expanded (nothing opens). The message's "It no longer exists" is odd for a path that never existed.
- Typing was smooth while the list showed. The network-share case was not tested: no share is mounted here.

#### 53. Folder history: PASS
- ⌘L, then deleting the text: the empty field lists "Recent" (the last 5, newest first, with each folder's parent) and "Frequent".
- `kla` lists a "History" heading with the visited `klasör A`, `alt` and `jpg-klasor`. (`ala` matches nothing: the match is on the start of the name.)
- A visited folder deleted outside Gezik (`silinecek`, `rmdir`) was gone from the Recent list when it was opened about 1 s later.
- Go ▸ Clear Folder History: the status bar says "Folder history cleared", and `state.toml` has no `[history]` any more.
- **Note:** the same folder reached through `/tmp/…` and `/private/tmp/…` is kept twice (`klasör A  /tmp/gezik-test` and `klasör A  /private/tmp/gezik-test`, `alt` twice). Paths are not canonicalized before they are counted. It is the same `/tmp` link as A and the reload finding.

#### 54. Command keys: FAIL
The two commands from the checklist were added live (config under `/private/tmp`).
- The notice in the status bar says `settings.toml: commands[7]: shortcut "mod+f" is already used by filter; the command has no key`, and ⌘F still opens the filter.
- Select all (`a.txt`, `b.txt`, folder `alt`), ⌃⌥Z: "Zip together — Run Zip together on 3 items?" with Run / Cancel. Cancel runs nothing. Run makes one `together.zip`, and the panel row says "Running Zip together on 3 items — Done · can't be undone". ⌘Z does not remove it.
- **But `unzip -l together.zip` lists `private/tmp/gezik-test/ziptest/a.txt`, `…/b.txt`, `…/alt/c.txt`.** `{files}` expands to absolute paths, so `zip -r` stores the whole path. In Finder or 7-Zip the archive opens as `private/tmp/gezik-test/ziptest/…`. The command runs in the folder (the zip landed there), so passing names relative to it (`a.txt`, `alt`) would give the expected archive. The checklist's own example hits this.
- The menu bar has a **Commands** menu: a grey "Archives" heading and "Zip together    ⌃⌥Z" (the key is part of the title, not a key equivalent). Choosing it asks the same question ("on 1 item" with one row selected).
- Right-click ▸ Commands ▸: Thumbnail, Missing program (grey), Slow copy, Clash, then a grey "Archives" heading over Zip together. The heading can't be chosen.
- ⌃⌥Z while the path field or the filter field has the keyboard runs nothing. It types `z` into the field instead.

#### 55. No history: PASS
`[history] remember = false` was picked up live: `state.toml`'s `[[history.folders]]` entries (2 after a visit) were gone within 2.5 s. A visit to another folder afterwards added none. The line was set back to `true`.

#### 56. Open terminal: PASS
Folder `bosluklu klasör` (a space and a Turkish letter in the name). Each time, `pwd` in the new Terminal window said `/tmp/gezik-test/bosluklu klasör`.
- ⌘⌥T: Terminal.app opens a new window in that folder.
- File ▸ Open Terminal: the same.
- Right-click the folder's row ▸ "Open terminal here" (from its parent): the same folder.
- Right-click empty space ▸ "Open terminal here": the folder shown.
- ⌘Q in Gezik: the Terminal window stayed open (window count unchanged), and Gezik exited without a crash report.

#### 57. Copy path: PASS
File `it's ş #1.txt` in `yol/`.
- ⌘⌥C (Edit ▸ Copy Path carries ⌘⌥C) copies `/private/tmp/gezik-test/yol/it's ş #1.txt`.
- Right-click ▸ Copy path as ▸ (Full path / Quoted / Name / Folder path / file:// URL):
  - Quoted gives `'/private/tmp/gezik-test/yol/it'\''s ş #1.txt'`, and the shell reads it as the same file (`ls` found it).
  - file:// URL gives `file:///private/tmp/gezik-test/yol/it%27s%20%C5%9F%20%231.txt`, which decodes to the file (checked with Python rather than Safari).
- With nothing selected, ⌘⌥C copies the folder shown (`/private/tmp/gezik-test/yol`).
- ⌘C on the file, then ⌘⌥C, then ⌘V in another folder: no file was copied. **But 7c's paste-as-file made `Pasted text 2026-10-10 02.08.52.txt`** holding the path, because the clipboard now holds text. The checklist expects "pastes nothing". Both features work as each was designed, but together they surprise: a copied path pastes as a text file.

#### 58. Session: PASS
Six tabs (bosluklu klasör, tmp, klasör A (locked), klasör A, dil, bosluklu klasör), `dil` in front.
- ⌘Q, then Gezik started again with no folder argument: the same six tabs, `dil` in front, and the lock on the third.
- `tmp` put in front, then `kill -9`, then started again: the same tabs with `tmp` in front. The session survives a forced quit.
- `gezik <folder>` (`/private/tmp/gezik-test/yol`) on a cold start opened the saved tabs plus a new tab for that folder after them, in front.
- `[session] restore = false` (live): `state.toml` lost its `[session]`. After ⌘Q and a start, one tab opened in `start-folder` (`{home}`, "Home").
- Side note: a duplicate key in `settings.toml` (a second `restore` line, my own mistake) was reported on stderr as `settings.toml line 85: duplicate key` (three times), and the whole file seemed ignored: the old session came back. The status bar showed nothing. Telling the user in the window (as for the commands' notices) would help.

#### 59. Tab sets: PASS
- Window ▸ Save Tabs As… opens "Save tabs — Name for these tabs:". `Work` wrote `[[tab-sets]] name = "Work" tabs = ["/private/tmp/gezik-test/dil", "/private/tmp/gezik-test/filtre"]`.
- The Window menu shows "Open Tab Set" (after Save Tabs As…) once a set exists, with Work / Replace tabs with "Work" / Delete "Work" (read through Accessibility). So the `if … : Menu` inside the menu bar works on macOS.
- Window ▸ Open Tab Set ▸ Work opened the set's two tabs after the open ones, with its first tab (`dil`) in front.
- With tab 1 locked, a tab's right-click ▸ Open tab set ▸ Replace tabs with "Work": the locked tab stayed, the others were replaced by the set's two, and the status bar said "1 locked tab stays open".

#### 60. Custom terminal: PASS
iTerm is not installed. `[terminal] command = ["/usr/bin/touch", "/tmp/claude-501/custom-term.txt", "{dir}/.term-was-here"]` was used as a stand-in. Picked up live, ⌘⌥T in the `dil` tab ran it: both files were made, and the second one in `/private/tmp/gezik-test/dil`. So the setting is used and `{dir}` is the folder shown. Terminal.app did not open.

#### 61. Pinned groups: PASS
Written by hand: `pinned = ["{documents}", { path = "{downloads}", name = "DL", group = "Work" }, { path = "{desktop}", group = "Media" }, { path = "{pictures}", group = "work" }]`.
- Live, the sidebar shows PINNED: Documents, then "Work" (DL, Pictures) and "Media" (Desktop), as written. `work` joined `Work` (case-insensitive).
- Right-click a group heading: the first group offers "Move group down / Rename group… / Ungroup", the last "Rename group… / Ungroup" (no move past the ends).
  - Move group down put Media before Work, and the file was rewritten as a multi-line list with `group = "Work"` normalized.
  - Ungroup on Media dropped the `group` keys of its pins.
- Right-click a pin: Open in new tab / Unpin from sidebar / Move up/down / Rename… / Move to group ▸ / Search in this folder… / Open terminal here / Copy path as ▸.
  - Rename… set `name = "İş"` on Desktop. Rename… with an empty name removed the `name`, so the folder's own name came back.
  - Move to group ▸ offers the other groups, "New group…" and "No group". Media moved DL there.
- Dragging Pictures (Work) to between Desktop and DL (Media): a line shows where it goes. The pin landed in Media at that place, and the Work group, now empty, went away.

#### 62. Pins 1-9: PASS
Pins in order: Documents, Desktop, Pictures, DL (Downloads).
- On Turkish-QWERTY-PC, ⌘⌥1…⌘⌥4 go to them in the sidebar's order (the window title was checked each time). ⌘⌥5 with only four pins does nothing.
- Go ▸ Pinned 3 goes to Pictures, and Go ▸ Pinned 1 carries the key equivalent ⌘⌥1 (AX modifiers 2 = option + command).
- With the path field focused, ⌘⌥1 fired the menu's shortcut (went to Documents) and typed nothing into the field.
- Not tested: the US and French AZERTY layouts. The input source was not switched on this Mac.

#### 63. View menu: PASS
Read through Accessibility (`AXMenuItemMarkChar`):
- View ▸ Hide Extensions, Folders First, Single-Click to Open and Show Hidden Items are check-mark items. Date Format ▸ (Relative / Short / ISO / System) and Size Format ▸ (Binary / Decimal) each have exactly one checked choice (System, Binary at first).
- Choosing Hide Extensions and Date Format ▸ Relative moved the checks and wrote `hide-extensions = true` and `date-format = "relative"` under `[view]`.
- Editing the file by hand (`folders-first = false`, `size-format = "decimal"`): the checks followed within 2.5 s.
- ⌘⇧. (on Turkish-QWERTY-PC the key that types `.`) wrote `show-hidden = true` and checked Show Hidden Items, and again wrote `false`.

#### 64. View options: PASS
In `gorunum/`:
- Hide Extensions: names lose their extension (`bin1500`, `bugun`, `yeni`). The folder `klasor.v2` keeps its own. Enter/F2 shows the whole name (`bugun.txt`, with the stem selected).
- Folders First off: `a-klasor` and `klasor.v2` sort among the files.
- Date Format ▸ Relative: files written a minute before say "1 min ago", one from 00:30 today "Today 00:30", and an older one "2026-01-01 12:00".
- Size Format ▸ Decimal: a 1,500-byte file is `1.5 kB`.
- Single-Click to Open: one click on `a-klasor` opened it. ⌘-click and ⇧-click only selected (⇧-click selected a range), with no folder opened.
- All four were set back afterwards.

#### 65. Hidden by default: PASS
Seen at the start of the run (item 30): with a fresh `settings.toml` that has no `show-hidden` line, `.DS_Store` and `.gizli` were not shown. The new file's template has `# show-hidden = true      # dot names and Windows' hidden items; default true (macOS: false)` as a comment. A `show-hidden = false` line was only written after the first ⇧⌘..

#### 66. New ▸ and templates: PASS
- Right-click empty space ▸ New ▸: Folder, Text file, Markdown file, then the templates, then "Open templates folder".
- "Open templates folder" opened `/private/tmp/gezik-cfg/templates/` in a new Gezik tab and created it. It is under `GEZIK_CONFIG_DIR`, as the other settings are (`~/Library/Application Support/gezik/templates/` without it).
- `Report.pages` (a file), `Project/` with `README.md`, and a `.DS_Store` were put there. Within a second, New ▸ listed "Project" and "Report" (no extension, alphabetical), and `.DS_Store` was not listed.
- In the folder shown, Project made `Project/` with its `README.md`, with the name being edited. A second Project made `Project (2)`. Report made `Report.pages`.
- ⌘Z took the last one (`Report.pages`) away.

#### 67. New folder with selection: PASS
- Three files, ⌃⌘N: they moved into `New folder`, which is selected and being renamed. File ▸ New Folder with Selection carries ⌃⌘N.
- ⌘Z put the three back and the folder went (to the Trash). ⇧⌘Z moved them in again.
- With `New folder` already there, File ▸ New Folder with Selection on `t1.txt` + `t2.txt` made `New folder (2)`.
- The right-click item was not reached: type-select "New folder w" in the row menu didn't pick it, and a stray copy of the menu stayed drawn on screen until Esc and a click. This looks like the synthetic input; the menu bar path worked.

#### 68. Paste as file: PASS
In `yapistir/`:
- A screenshot to the clipboard (`screencapture -c`, 400×300 pt): ⌘V on the list made `Pasted image 2026-10-10 02.17.31.png`, 800×600 px (Retina size) and the same 176,644 bytes as the clipboard's PNG flavour. It appeared in about a second, with no beach ball.
- Text on the clipboard (`ılık İstanbul metni`): right-click empty space shows "Paste text as file", and ⌘V made `Pasted text 2026-10-10 02.17.43.txt` (UTF-8, the same text).
- A file copied in Finder (⌘C on `liste.txt`, clipboard: furl + text + icon): ⌘V pasted the file `liste.txt`, not a text file. The `«class furl»` wins over the text and icon flavours.
- Not tried: Safari ▸ Copy Image.

#### 69. Links: PASS
In `link/`:
- Right-click a file ▸ "Create link": `Link to dosya.txt -> /private/tmp/gezik-test/link/dosya.txt`, a symbolic link (`ls -l` shows `l`), not an alias. The same on a folder: `Link to hedef`. The folder link lists as "Folder" and the file link as "Plain Text Document", 38 B (the link's own size).
- Drag inside Gezik with ⌘⌥ held onto `kutu`: the label says "Create link in kutu", and dropping made `kutu/Link to Link to hedef` (a link to the dragged link; the row order had shifted after `kutu` was made).
- ⌘Z on that link, and again on `Link to hedef` (a link to a folder): each link went away, and `hedef/` and its `ic.txt` stayed.
- Not tried: a ⌘⌥ drag from Finder, and a right-drag ▸ "Create link here" (the drop menu doesn't take synthetic clicks, see 26). Finder's arrow badge was not checked.

#### 70. Drop stack: PASS
- ⌘⇧S with three files selected adds them, and the strip opens above the status bar ("3 items", each with ×, then Copy here / Move here / Clear).
- `y3.txt` deleted in Terminal: it fades in the strip within a few seconds.
- "Copy here" in the same folder: "Everything on the drop stack is already in this folder".
- In another folder: Copy here copied `y1.txt` and `y2.txt` and left the faded `y3.txt` out. One ⌘Z removed both copies. Move here moved them, and they left the strip (only the faded `y3.txt` stayed). Clear emptied it ("Drop files here").
- Edit ▸ Add to Drop Stack adds the selection (on an item already there: "Already on the drop stack"). View ▸ Drop Stack hides and shows the strip.
- Not tried: dragging from Finder onto the strip, dragging out of it, and a two-finger swipe with many items.

#### 71. History: PASS
- View ▸ Operation History (and the status bar's History) opens the History tab of the operations panel: newest first, each with its time, a title ("Moving 2 items to …", "Undoing Copy 2 items", "Creating a link to hedef") and a result.
- A copy onto a locked file (`chflags uchg`, then Replace): the entry says "1 failed" with Details. Details says `kilit.txt: "kilit.txt" couldn't be moved to the trash because you don't have permission to access it`, and the file kept its old content.
- "Show in folder" on "Moving 2 items to …/yigin/hedef" opened `hedef` with `y1.txt` and `y2.txt` selected.
- The list holds this run of Gezik only (it started at 02:08). The cancelled jobs from 38/45 were in an earlier process.
- **Note:** before that copy, the conflict list marked `kilit.txt` as "identical" and offered "Hide identical (1)", but the two files differ (`yeni` vs `eski`). They only have the same size (5 B) and the same modification second. Calling that "identical" (and defaulting to Skip) can hide a real change; size + time alone should not be called identical.

#### 72. Search: PASS
- ⌘⇧F (and Edit ▸ Find…, which carries ⌘⇧F) opens the search bar above the list: "in <folder> ▾", the name field ("Name, e.g. *.pdf;!*draft*"), Content, Filters ▾, Search.
- In the home folder, typing `*.pdf` gave no live results: the status bar said "Large folder: press Enter to search". After Return, a tab titled "Search: *.pdf", whose address bar ends in `Search "*.pdf"`, had 114 results with Name, Folder, Modified and Size columns. The status bar said "114 results in 4.9 s · Skipped 6,207 folders (search.skip) · 143 folders could not be read". (The file names are personal and not listed here.)
- ⌘[ went back to Home, and ⌘] went forward to the results without searching again.
- Content: in `icerik/`, "zebra42" with Content on, then Return, found `u8.txt` ("2: burada zebra42 var") and `u16.txt` (UTF-16 BE with BOM, "2: zebra42 utf16") and not `none.txt`: "2 results in 0.0 s", with the line in the Match column.
- Before the first home search, macOS asked "Terminal would like to access your Photo Library" (Gezik runs under Terminal). The maintainer allowed the prompts.

#### 73. Privacy prompts: PASS
- The first home search of this build ran after macOS's prompts were answered by the maintainer (all allowed). The prompt seen was "Terminal would like to access your Photo Library". It came when the home folder was opened with the search bar, before any search, so listing or showing `~/Pictures` set it off. Desktop/Documents/Downloads prompts may have been answered in Run 1 already. A denied folder was not tried.
- The `~/Library` folders macOS keeps to itself count as "N folders could not be read" in the status bar (143 the first time, 146 the second). The bar's ▾ ▸ "50 folders could not be read…" lists them (`~/Library/DoNotDisturb`, `StatusKit`, `Cookies`, `Caches/com.apple.Safari`, … "Access denied"). The menu's 50 vs the status bar's 146 suggests the list is capped at 50 without saying so.
- The search still ends, and no prompt came twice in the run.

#### 74. Whole drive: PASS
A unique file `gezik-wd-9137.txt` was put in `/Users/Shared` and on the GezikHedef disk image (`/Volumes/GezikHedef`).
- Scope menu: This folder / Users / Whole drive (/) / Computer. "Whole drive (/)" with `gezik-wd-9137*`: exactly 1 result, `Users/Shared/gezik-wd-9137.txt`, "1 result in 13.5 s · Skipped 6,229 folders (search.skip) · 435 folders could not be read". `/Users` was searched (it is on the Data volume), the file did not appear a second time through `/System/Volumes/Data`, and the copy on `/Volumes/GezikHedef` was not found (other volumes are not walked).
- (Scope "Users" found the same file in 6.7 s.)
- Esc 2 s into a whole-drive search stopped it at once: "Stopped · 0 results …", and gezik's CPU was 2 % 0.6 s later. No beach ball was seen. Scrolling and switching tabs during a long search were not measured.

#### 75. Flat view: PASS
`proje/` with `Cargo.toml`, `src/main.rs`, `src/alt/lib.rs`, `docs/oku.md` and `dil-link` → `/tmp/gezik-test/dil`.
- ⌘B: the tab becomes "proje (all files)", the address bar ends in "All files", and the list holds every file under it (no folders) with a Folder column (`src/alt`, `src`, `docs`): "5 results in 0.0 s".
- ⌘B again on `lib.rs` went to `src/alt` with `lib.rs` selected. View ▸ Flat View does the same as ⌘B.
- The symlink to a folder is one row and is not walked into (`dil`'s three files are not listed). **But it shows as `dil-link`, 0 B, with no type and a document icon**, not as a folder link.

#### 76. Show in folder: PASS
Search `*.rs` in `proje/` (2 results, Folder column `src`, `src/alt`).
- ⌘⇧E on `lib.rs` opened `src/alt` with `lib.rs` selected. ⌘[ returned to the results. Go ▸ Show in Folder (it carries ⌘⇧E) did the same.
- The row menu's "Show in folder in new tab" on `main.rs` opened `src` in a new tab next to the search tab, which stayed.
- (A click on the column header by mistake turned the sort to Name ↓; the results resort at once.)

#### 77. Results like a folder: PASS
Search `*.rs` in `proje/` (`main.rs` in `src`, `lib.rs` in `src/alt`).
- ⌘A, ⌘C in the results, then ⌘V in `sonuc-hedef/`: `main.rs` and `lib.rs` side by side (flat).
- Right-click ▸ "Copy with folders", then ⌘V: `src/main.rs` and `src/alt/lib.rs` with the folders made under the scope. One ⌘Z took the copies and the made folders away (the target folder was empty again).
- ⌘⌫ on `lib.rs` in the results: it left the list and the disk. ⌘Z brought it back to `src/alt` and into the list. (This is under `/private/tmp`, so bug A doesn't apply.)
- Two results from two folders ▸ Enter ▸ rename layer with Number 001 at end and "Per folder" on: `main 001.rs` and `lib 001.rs` (numbered per folder; off: 001 and 002). With `src/main 001.rs` already there, that row is marked in red with a warning icon, and the layer says "1 name already in the folder · 1 will change · fix the marked rows".
- Gezik quit and started with the search tab in front: the tab came back ("Search: *.rs") and searched again, now listing `x.rs` made in the meantime (3 results).
- **Note:** the results don't follow changes made outside Gezik (known), and pressing Return in the unchanged name field didn't search again either. `x.rs`, made after the search, only appeared after the restart. The bar has no visible way to run the same search again except changing the text.

#### 78. Themes: PASS
- `theme = "light"`, `"dark"`, `"classic-light"`, `"classic-dark"` each applied live (2.5 s after saving). The classic themes are flat: the list goes edge to edge with no rounded sheet, and they use a blue accent (Search button, tab underline) where light/dark use orange.
- `auto`: with System Settings' appearance switched (System Events `dark mode` false, then true again), Gezik went light and back to dark within 2 s.
- `density = "compact"`: rows go from 26 to 20 pt, and the sidebar and bars tighten.
- `reduce-motion = true` was set with no error, but hover and popup fades can't be judged from screenshots.

#### 79. Drawing at 200 %: PASS
Captured at native 2× pixels (`screencapture -R` on the Retina display): the 1 px lines (tab and button borders, the list sheet's edge) are crisp single device pixels, and the sheet's rounded corner is smooth. The 11 px text (column header "Name", status bar "3 results in 0.0 s", sidebar labels FOLDERS, PINNED, LOCATIONS) is sharp and readable in the system font. The Refresh arc, the History clock, the sidebar icons (house, folders, pins, cloud, drives, trash) and the View ▾ menu are crisp. Long popup menus were seen in 31.

#### 80. Thin scroll bar: PASS
In `1000-dosya/` (1,000 rows):
- A thin bar (6 pt, 12 device pixels), no track, grey in the dark theme (not the system's overlay bar). Its thumb is at least 24 pt tall even for 1,000 rows. On hover it is slightly darker.
- Dragging the thumb with the mouse went to the very end (the last rows shown, thumb at the bottom) and back to the very top.
- A click below the thumb scrolled one page down.
- Wheel scrolling (synthetic, 20 × 3 lines) moved the list smoothly.
- The sidebar has the same bar (visible when the operations panel shrinks it). The conflict list and the rename layer were not scrolled. Trackpad smoothness and natural direction need a real hand.

#### 81. Folder sizes: PASS
With `folder-sizes = "local"` (picked up live):
- In `/private/tmp/gezik-test` the Size column filled in for every folder within 3 s (`pdf-test` 1.7 MB, `pdf-resim` 652.9 KB, `1000-dosya` 0 B). In the home folder all folders had sizes within a few seconds. `Library` shows `≥ 146.2 GB` (parts macOS keeps to itself could not be read). The others are plain (Downloads 17.8 GB, Pictures 19.2 GB …).
- Sizes are the files' logical sizes (`klasör A` 16 B, where `du` says 24 KB on disk). That matches Finder's "size", not "on disk".
- Three folders selected: the status bar sums them ("3 selected (2.5 MB)").
- The preview for a folder shows its size and `N files, M folders` (`pdf-test`: 1.7 MB, 13 files, 0 folders). With the preview open, the Size column is hidden for lack of room.
- Sort by Size works both ways; the `…` state was too short to watch here.
- A symlink to a folder (`proje/dil-link`) shows no folder size.
- `/System`, then Back at once: gezik's CPU was 0 % 3 s later. Coming back showed the sizes that were done (Applications 1.2 GB, iOSSupport 555.8 MB) and `…` for the rest, which then went on.
- Not tried: a network share, View ▸ Calculate Folder Sizes with `"off"`, and the "Some folders could not be read" line in the preview. Set back to `"off"` afterwards.

#### 82. Command palette: PASS
- ⌘⇧P opens the picker with `>` and the caret after it, listing actions with their kind and shortcut (Back ⌘[, Close Tab ⌘W, Copy ⌘C, Copy Path ⌥⌘C, Cut ⌘X …).
- Typing keeps the `>`: `copy` lists Copy, Copy Path, Copy with Folders, and also the user command "Slow copy (sleep)" (kind "Command").
- `>copy path` + Return ran Copy Path (the clipboard got the focused row's path). Esc closes the palette and the list has the keyboard back (↓ moved the selection).
- Deleting the `>` turns it into Quick Open: the empty field lists Copy Path first (as recent), then the pinned places.

#### 83. Quick Open: PASS
- ⌘P lists places (Recent, Pinned, tabs, …). `down` finds Downloads (Recent) first, then "Download Now" and "Remove Download" (actions), DL (Pinned), and the last line `Search for "down" in macbookpro`.
- Alt+Return on Downloads opened it in a new tab, next to the current one.
- `zzqx` leaves only `Search for "zzqx" in sonuc-hedef`, and Return started that search (tab "Search: zzqx").
- Go ▸ Quick Open… (⌘P) and Go ▸ Command Palette… (⌘⇧P) carry the keys and do the same.

#### 84. Saved searches: PASS
- Search `*.rs` in `proje/`, then the bar's ▾ ▸ Save search…: "Name for this search:" `Rust`, then "Save \"Rust\" with /private/tmp/gezik-test/proje, or for the folder shown when it runs?" with Save with this folder / Save for any folder ({here}) / Cancel.
- "Save for any folder" wrote `[[searches]] name = "Rust" folder = "{here}" pattern = "*.rs"` to `/private/tmp/gezik-cfg/settings.toml` (`GEZIK_CONFIG_DIR`'s, not `~/Library/…`). A comment added by hand just before (`# kendi notum: kalsin`) stayed.
- The sidebar has a SEARCHES section (after CLOUD) with "Rust" and a magnifier icon.
- In `pdf-test/`, a click on Rust ran it there: the tab is titled "Rust", and the scope is "in pdf-test" (`{here}`), with no results (no .rs files there).
- Right-click on it: Run in new tab / Rename… / Delete.

#### 86. System icons: FAIL
With `icons = "system"`:
- `/Applications` in the list and the grid: each app's own icon, crisp, upright, with clean transparent edges (1Password, Adobe, Docker, Figma, Chrome, Keynote, Pages, Safari …). Names keep `.app` (`Docker.app`), which Finder hides.
- **`~` shows Desktop, Documents, Downloads, Movies, Music, Pictures, Public, Library as plain blue folders**, without Finder's special folder icons (in the list and the grid). **`/` shows Applications, Library, System, Users as plain folders too.** Only `home` (a link) and `dev` look different.
  Cause, from the code: `media.rs` asks NSWorkspace for a folder's own icon only when `folder_has_own_icon` is true (an `Icon\r` file, the FinderInfo custom-icon flag, or another device than the parent). The special folders have none of these. On macOS 26, `/`, `/Applications`, `/Users` and `~` all have the same `st_dev` (16777229, firmlinks), so the device rule doesn't fire either. That is also why the `apps_folders_and_types_have_icons` test fails (C above). A fix could ask NSWorkspace for the known folders by path (home's Desktop/Documents/Downloads/…, and `/Applications`, `/Library`, `/System`, `/Users`).
- A folder with a custom icon (set with `NSWorkspace setIcon`, so FinderInfo has the flag): its own icon (a calculator) shows. PASS.
- `/Volumes`: the mounted disk image GezikHedef shows its volume icon. Macintosh HD (a link) is a plain folder.
- `.pdf` (Acrobat's PDF icon, the default app here), `.zip` and a file with no extension have Finder's document icons. Empty `.txt` and `.md` files show a plain white square in the grid: that is Quick Look's thumbnail of an empty text, with no page frame around it.
- Switching `icons` live did not take while the config was under `/tmp` (see the reload finding), and the switch under `/private/tmp` was not repeated.

#### 87. Speed and memory: PASS
- `1000-dosya/` (1,000 files of four types): the list was drawn with icons and Finder kinds in a screenshot 0.62 s after Return (the screenshot itself takes ~0.5 s, so the list came well under that). Ten Page Downs in a row: CPU 1.2 % right after, 0 % two seconds later.
- `/Applications` in the grid (medium; changing `grid-size` in the file didn't change this folder, which keeps its own view in `views.toml`): RSS 158,176 KB before and 160,176 KB after six fast scrolls through the whole list and back, then 160,944 KB after six more. That is about 2.8 MB, well within ~40 MB. CPU is 0 % once scrolling stops.
- Not tried: the largest grid size (would need View ▸ size), and a folder of 200 subfolders on an SMB share.

#### 88. Quick Look thumbnails: FAIL
Grid view, `thumbnails = true`, in `onizleme/`:
- PDF: its first page. HEIC: the picture. MOV: a frame. `.txt`: its text on a white page. PNG/JPEG come at once (Gezik's own). The PNG with alpha shows its transparency over the dark background.
- **`portre-o6.jpg` (stored 1600×1200, EXIF orientation 6) is drawn landscape and not rotated**, in the grid thumbnail and in the preview panel. Quick Look (`qlmanage -t`) gives 192×256, upright. Gezik's own JPEG path ignores the EXIF orientation. (Convert in 35 and Images to PDF in 41 do apply it.)
- A Pages document (`scenario 2.pages`, a single-file zip package, copied from iCloud's Pages folder) shows the Pages document icon, not its first page.
- The preview panel shows the PDF's first page and a frame of the MOV.
- Not tried: a PSD, Keynote/Numbers, scrolling 500 PDFs, leaving a folder of big videos, and the iCloud-only case.

#### 89. Finder names: NOT TESTED
Not tested. Starting Gezik with `-AppleLanguages "(tr-TR)"` (the per-process trick used in 28) does not change Finder's localized names: `~` still lists Desktop, Documents, Downloads … in English. Those names follow the login session's language, so this needs Türkçe first in System Settings and a log-out, which was left to the maintainer.

#### 90. Opening aliases: PASS
Aliases made by Finder (AppleScript `make new alias file`) of a file, a folder, the GezikHedef volume and `/Applications/Safari.app`, plus symlinks to a file, a folder and a missing file.
- The folder alias (⌘↓) went into `orijinal`, and ⌘[ returned. The volume alias went into GezikHedef.
- The file alias opened `dosya.txt` in TextEdit (its window "dosya.txt" with "orijinal dosya" was on screen).
- The Safari alias started Safari (Safari became the front app), and Gezik stayed in `alias`.
- Original file deleted, then the alias opened: "The original item can't be found — The alias \"dosya alias\" can't be opened." with Delete Alias / OK. Delete Alias moved it to the Trash at once with no second question, and ⌘Z brought it back.
- A broken symlink (`sym-kirik`) gets the same question (its text says "alias" for a symlink). A symlink to a folder is entered (`sym-klasor`).
- In the list, the aliases and the file symlinks show as "Document" (known gap).

#### 91. Make Alias: PASS
- ⌃⌘A on `rapor.pdf`: `rapor.pdf alias` next to it, selected. Finder calls it "Alias" and its original item is `rapor.pdf`. Again on the same file: `rapor.pdf alias (2)`, and the first alias is untouched.
- File ▸ Make Alias on the folder `dosyalar`: `dosyalar alias`.
- ⌘Z moved the last alias (`dosyalar alias`) to the Trash. The originals are untouched.
- The original moved elsewhere in Finder (`takma-baska/`): Finder still resolves the alias to the new place (`/private/tmp/gezik-test/takma-baska/rapor.pdf`).
- Not checked: the row menu item, three items at once, and Finder's arrow badge.

#### 92. Packages: FAIL
- **`/Applications/Safari.app`** (on macOS 26 a symlink to `/System/Cryptexes/App/System/Applications/Safari.app`): ⌘↓ (open) **went into it** (`Safari.app ▸ Contents`) instead of starting Safari. Gezik's `is_package` doesn't follow the link (the failing test `folders_have_finder_names_and_links_resolve`, C). The same in the Safari alias case in 90 worked, because the alias resolves to the real bundle.
- `Numbers.app` (a real bundle): ⌘↓ started Numbers and Gezik stayed in `/Applications`. File ▸ Show Package Contents went into `Numbers.app` (Contents).
- **A plain folder named `x.app`** (`mkdir`): double-click does not enter it. The status bar says `Cannot open /private/tmp/gezik-test/x.app: Launcher "/usr/bin/open" "--" "/private/tmp/gezik-test/x.app" failed with ExitStatus(unix_wait_status(256))`. LaunchServices treats any `.app` folder as a package (it shows the "not allowed" app icon), so Gezik hands it to `open`, which fails. The checklist expects it to be entered. Typing its path in the address bar does enter it.
- The command palette lists Make Alias (⌃⌘A) and Show Package Contents.
- Not tried: a `.key`/`.pages` folder package or an `.rtfd`.
