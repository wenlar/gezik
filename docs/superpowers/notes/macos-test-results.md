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
- Everything was run by Claude Code on the Mac, at the maintainer's request. Clicks, drags and keys in Gezik's window went in as synthetic CGEvents (modifiers sent as real key down/up). Menu-bar items were chosen through Accessibility (System Events), because this Mac's menu bar hides itself. Results were checked with screenshots, `ls`/`xxd`/`ffprobe`, and the files on disk. Where a check needs a real hand (trackpad feel, a real mouse on a native drop menu), the item says so.
- From item 33 on, the config folder was given as `GEZIK_CONFIG_DIR=/private/tmp/gezik-cfg` (the same folder), because live reload doesn't work through the `/tmp` link (see the reload finding). Most file tests from 35 on ran in `/private/tmp/gezik-test` for the same reason (bug A).
- The maintainer answered macOS's privacy prompts (Photo Library for Terminal, allowed; a TextEdit automation prompt set off by the test tooling, not by Gezik).
- Test data was made on the Mac (`/tmp/gezik-test`): JPEGs and PNGs written with ImageIO (one with EXIF orientation 6 and GPS, a PNG with alpha), a HEIC written with ImageIO (**not** a real tiled iPhone HEIC), PDFs written with CoreGraphics (3, 5 and 240 pages, and one encrypted with the password `gezik`), the repo's `enc_aes256.pdf` and RAR files, a 4 s screen recording (`klip.mov`), an AAC `.m4a` from `say`, a Windows-1254 text, zips (single root, multi root, ZipCrypto with a password), a `tar.gz`, a `.dmg`, and a folder of 1,000 empty files of mixed types.

### Main findings

1. **Every quit by closing the last window aborts** with a crash report (`dnd::macos::Registration::drop` touches a thread-local that is already gone). ⌘Q is clean. See "(extra) Gezik aborts every time it exits".
2. **The system's `/tmp` and `/var` links break several things** (bug A): Put Back and ⌘Z of anything that went to the Trash are refused under `/tmp` or `/var` (22 failing tests). Live reload of `settings.toml` doesn't work with a symlinked config folder. Folder history keeps `/tmp/x` and `/private/tmp/x` apart.
3. **Default file manager (141):** LaunchServices refuses `public.folder` with -50 on macOS 26.5.1, also for a copied bundle, so 142-145 couldn't run.
4. **Folders and apps:** Desktop/Documents/Downloads and `/Applications`, `/System`, `/Users` get no special icons (86). `/Applications/Safari.app` (a symlink on macOS 26) is entered instead of started, and a plain folder named `x.app` can't be entered (92).
5. **Pictures:** a JPEG with EXIF orientation 6 is shown sideways in the grid and the preview (88).
6. **Quick Look panel:** closing it opens Gezik's own preview window as well, and three files show as one (98).
7. **Turkish-QWERTY-PC:** ⌘= needs a custom key (48). (⌘I only from the key that types `i` is what Finder does too, so not a bug; see 102.)
8. Smaller ones: `--new-window`/⌘N stack on the old window (112), a minimized Gezik comes back inactive (115), `{files}` passes absolute paths as the spec says (54), MP3/M4A from a silent `.mov` show a raw ffmpeg error (38), WIM is "Document" (34), undo of an 'Apply to enclosed items' from a group I'm not in does nothing (106), and the Get Info labels overlap the checkboxes (102).

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
| 54 | Command keys | PASS (as specified; one note) |
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
| 93 | Exe size | PASS (one build only) |
| 94 | Probe | PASS (section 4); section 5 NOT TESTED (no Quick Action installed) |
| 95 | Open With | PASS (60-file case and Safari.app/.pages menus not tried) |
| 96 | Share | PASS (nothing was sent; File ▸ Share… through Accessibility showed nothing) |
| 97 | Quick Actions | NOT TESTED (needs a Quick Action made in Automator) |
| 98 | Quick Look panel | **FAIL** (closing the panel falls back to Gezik's own window; no "1 of 3") |
| 99 | Unchanged | PASS |
| 100 | Exe size | PASS (20,942,592 bytes; see 93) |
| 101 | Probe | PASS |
| 102 | Opening | **FAIL** (labels overlap; fixed on fix/macos-2) |
| 103 | Permissions | PASS (except the "replaced file" case, see note) |
| 104 | Owner and group | PASS |
| 105 | Hidden and Locked | PASS (Finder hiding not looked at) |
| 106 | Apply to enclosed items | **FAIL** (undo of a group change away from a group I'm not in fails, and the mode isn't put back either) |
| 107 | Open with | PASS (Change All not confirmed on purpose) |
| 108 | Links and ACLs | PASS |
| 109 | Exe size | PASS (20,942,592 bytes; see 93) |
| 110 | Tests first | PASS |
| 111 | Hand-over | PASS |
| 112 | Help, version, new window | **FAIL** (the second window is not offset) |
| 113 | Hung or crashed Gezik | PASS |
| 114 | Off | PASS |
| 115 | Bring to front | **FAIL** (a minimized Gezik comes back but is not activated) |
| 116 | Tests first | **FAIL** (every restore under `/var` or `/tmp` is refused) |
| 117 | Full Disk Access off | PASS |
| 118 | Full Disk Access on | NOT TESTED (Full Disk Access was not turned on) |
| 119 | Real bins on USB volumes | PASS (HFS+ disk image only) |
| 120 | Put Back | PASS (partly) |
| 121 | Delete for good and Empty | PASS (Empty not confirmed on the real Trash) |
| 122 | Live and refused | PASS (partly) |
| 123 | Tests first | PASS |
| 124 | Panel | PASS |
| 125 | Add, no `~/.local/bin` | PASS (new-Terminal check not done) |
| 126 | Someone else's file | PASS |
| 127 | Gezik moved | NOT TESTED (no .app here) |
| 128 | `--unregister` | PASS |
| 129 | No journal | PASS (one note) |
| 130 | Tests and probe first | PASS |
| 131 | Sidebar | PASS (no OneDrive/Google Drive here) |
| 132 | State badges | PASS (download-safety of the preview/thumbnail not checked) |
| 133 | Download Now / Remove Download | NOT TESTED (would change the maintainer's iCloud files) |
| 134 | Data safety | NOT TESTED |
| 135 | OneDrive / Google Drive | NOT TESTED |
| 136 | Links | NOT TESTED |
| 137 | Late roots | NOT TESTED |
| 138 | Search reads no cloud-only file | NOT TESTED |
| 139 | Tests and probe first | PASS |
| 140 | Panel and palette | PASS |
| 141 | Make default from outside a bundle | **FAIL** (LaunchServices refuses public.folder with -50, symlinked or copied bundle) |
| 142 | `open ~/Documents` | NOT TESTED (needs a successful Make default, see 141) |
| 143 | `application:openURLs:` | NOT TESTED (needs a successful Make default, see 141) |
| 144 | Bundle launch while Gezik runs | NOT TESTED (needs a successful Make default, see 141) |
| 145 | Gezik moved | NOT TESTED (needs a successful Make default, see 141) |
| 146 | Restore and `--unregister` | PASS (nothing to restore after the failed Make default; see 141) |

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
- Edit ▸ Select by Pattern… (chosen through Accessibility) opens "Select by pattern" with "8 items match" for an empty field. `*.jpg` says "2 items match", and Return selects `a.jpg`, `b.jpg`. A bad pattern `!` says "Type a name after '!'" in red. The box reopens with the last pattern (`*.jpg`, selected).
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

#### 54. Command keys: PASS
The two commands from the checklist were added live (config under `/private/tmp`).
- The notice in the status bar says `settings.toml: commands[7]: shortcut "mod+f" is already used by filter; the command has no key`, and ⌘F still opens the filter.
- Select all (`a.txt`, `b.txt`, folder `alt`), ⌃⌥Z: "Zip together — Run Zip together on 3 items?" with Run / Cancel. Cancel runs nothing. Run makes one `together.zip`, and the panel row says "Running Zip together on 3 items — Done · can't be undone". ⌘Z does not remove it.
- **Note:** `unzip -l together.zip` lists `private/tmp/gezik-test/ziptest/a.txt`, `…/b.txt`, `…/alt/c.txt`.** `{files}` expands to absolute paths, so `zip -r` stores the whole path. In Finder or 7-Zip the archive opens as `private/tmp/gezik-test/ziptest/…`. The command runs in the folder (the zip landed there), so passing names relative to it (`a.txt`, `alt`) would give the expected archive. The checklist's own example hits this. The spec (2026-10-07-klavye-paketi-design.md) says `{files}` passes full paths, so this is as designed; a `{names}` token or a note in the template would help.
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
- Search `*.rs` in `proje/`, then the bar's ▾ ▸ Save search…: "Name for this search:" `Rust`, then "Save 'Rust' with /private/tmp/gezik-test/proje, or for the folder shown when it runs?" with Save with this folder / Save for any folder ({here}) / Cancel.
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
- Original file deleted, then the alias opened: "The original item can't be found — The alias 'dosya alias' can't be opened." with Delete Alias / OK. Delete Alias moved it to the Trash at once with no second question, and ⌘Z brought it back.
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

#### 95. Open With: PASS
In `acwith/` (`a.txt`, `b.md`, `c.png`):
- Right-click `a.txt`: "Open With ▸" right after "Open with default app", listing TextEdit (default) first, then other apps by name (Adobe Illustrator…, Firefox Developer Edition, Chrome, Notes, Numbers, Pages, Safari, Sublime Text, Xcode …), then "Other…". App names have no icons (known).
- `a.txt` + `b.md`: only apps that open both (no "(default)", TextEdit among them). `a.txt` + `c.png`: TextEdit is not offered (Illustrator, Firefox, Chrome, HP, Safari).
- Choosing TextEdit for `b.md` opened it in TextEdit (window "b.md").
- "Other…" opens a panel in /Applications. Cancel did nothing. (A synthetic click on Cancel missed at first and walked into a folder in the panel; Esc doesn't reach the panel while it isn't key. The real Cancel button closed it.)
- A plain folder's menu has no "Open With".
- Not tried: 60 files at once, Safari.app and a `.pages` package's menu.

#### 96. Share: PASS
Only the menu was checked; nothing was sent to Mail or Messages (that would send real mail).
- Right-click `a.txt` ▸ "Share…": the system share menu (header "a · Text Document · 2 bytes", AirDrop, Mail, Messages, Notes, Freeform, Simulator, Journal, Reminders, Copy, Edit Extensions…) opened at the row. It stayed up for 2 s+ until Esc, and closed cleanly.
- The command palette's "Share…" (pointer over the list) opened it at the pointer, with an arrow to it.
- File ▸ Share… chosen through Accessibility (System Events `click menu item`, three files selected) showed no menu. This fits Apple's note that the picker must open on a real mouse press. A click on the menu bar with a real mouse is still to be tried.
- Not tried: three files into Mail.

#### 97. Quick Actions: NOT TESTED
Not tested. It needs Quick Actions made in Automator's window (a files one, an image-only one, a slow shell one), which could not be scripted reliably here. `~/Library/Services` has none, and the probe found none for the four files ("0 in 0.1 ms"). With no file-taking Quick Action installed, the row menu has no "Quick Actions" item, as expected (seen in every menu in this run).

#### 98. Quick Look panel: FAIL
`[system] quick-look = "system"` (default), in `onizleme/`:
- Space on `belge-a.pdf` opened the system Quick Look panel (as in Finder: pages on the right, "Open with Adobe Acrobat"). With the panel open, ↓ moved Gezik's selection and the panel followed (`foto1.jpg`, then `foto-heic.heic`, "Open with Preview").
- Space again closed it, and the keyboard was Gezik's at once (↓ moved the selection).
- **Esc in the panel (one file): the panel closed and Gezik's own preview window ("klip.mov — Gezik") opened** in addition, and the status bar said "The system Quick Look panel did not open; Gezik's own is shown". The system panel *had* opened; its closing is taken as "did not open".
- **Three files selected, Space: the panel shows only one item (the HEIC, with ‹ › arrows) and no "1 of 3".** Space again closed it and then Gezik's own window "3 items selected — Gezik" (a generic icon, "3 items selected · Total size 166.3 KB") opened, with the same status message.
- In the second try with one file, Space and Esc then left only the main window. So the fallback fires in some cases and not others.
- Not tried: a video playing, a folder, `quick-look = "gezik"`.

#### 99. Unchanged: PASS
The row menu on `metin.txt` (with `[[commands]]` set up) has Open, Open with default app, Open With ▸, Open terminal here, Copy path as ▸, Make Alias, Create link, Compress…, Compress to, Convert…, Commands ▸, Cut, Copy, Duplicate, Rename, Move to Trash, Delete permanently, Get Info, Share…. The three submenus (Open With, Copy path as, Commands) show together (no Quick Actions here, see 97). It opened at once. Gezik had 13 threads before the menu and 13 after it closed (`ps -M`).

#### 93. Exe size: PASS
`ls -l target/release/gezik` on `master` `cbf1ab0`: 20,942,592 bytes. The branches `feat/system-9a1`/`9a2`/`9a3` are merged, so there is no before/after pair to compare on this Mac (as the guide allows). Run 1's `feat/batch-ops-5c` build was 15,755,408 bytes, so 9a1-9b5 together added about 5.2 MB.

#### 101. Probe: PASS
See "Probes" above: section 6 is all PASS, users/groups 133/162 in 19.0 ms, and `cargo test -p gezik-platform attrs` (8) and `cargo test -p gezik-ops attrs` (9) pass.

#### 102. Opening: FAIL
- File ▸ Get Info on `rapor.pdf`: a panel on the right, "rapor.pdf Info", with Kind, Size ("18.3 KB in 1 file"), Where, Created, Modified, Last opened, then Sharing & Permissions (Owner, Group ▾, Read/Write/Execute for Owner/Group/Everyone), Hidden, Locked, Octal, Open with, Done. Esc closes it and the list has the keyboard again.
- ⌘I on Turkish-QWERTY-PC works with the key that types `i` (right of L), not the key at I's place (it types `ı`). Finder does the same (checked: ⌘ + the I-place key opens no Info window in Finder either), so this is not a bug.
- **The row labels Owner / Group / Everyone are drawn on top of the Read and Write checkboxes** ("ReadOwne✓Write", "ReadEveryo☐Write"). The label column is too narrow for the text at this size.
- Two items (`a.txt` 644, `b.txt` 600): "2 items", "5 B in 2 files", Where is their folder. Group Read and Everyone Read show a dash, and Octal is empty.
- Not tried: a folder's "calculating…" size, and "several folders".

#### 103. Permissions: PASS
On `rapor.pdf` (644):
- Group ▸ Write: `ls -l` showed `rw-rw-r--` at once. Esc, then ⌘Z: back to `rw-r--r--`.
- Octal `600` + Return: `rw-------`. `4755`: "Setuid, setgid and sticky can't be changed here" in red, nothing changed.
- Two files with different modes: the Group/Everyone Read boxes are dashed. Clicking the dashed Group Read made it the same for both (`b.txt` became `rw-r-----`).
- With the panel open, `chmod 777` in Terminal, then a box ticked: "1 item changed since; shown as it is now" in red, the boxes show 777, and that click did nothing. The next click worked (owner execute off: `rw-rwxrwx`).
- **Note:** with the panel open, the file was replaced in Terminal (`cp other.pdf x && mv x rapor.pdf`, new inode, 644), then a box ticked: the new file was not changed (`rw-r--r--`), and the note stayed "1 item changed since; shown as it is now". PASS for safety.

#### 104. Owner and group: PASS
- Group ▾ lists my groups (everyone, staff, _appstore, localaccounts, admin, _lpadmin, _developer, com.apple.access_ssh …, as `id -Gn`). Choosing admin: `ls -l` shows `admin`.
- A setuid file (`chmod 4755`, `rwsr-xr-x`): typing `staff` for its group kept `rws` (`-rwsr-xr-x staff`). The panel shows "Special: setuid (not changed here)" next to Octal 755.
- Group `wheel` (not mine) + Return: "Requires administrator: 1 item not changed" in red, and the group stayed `staff`.
- Owner `root` + Return: the same note, and the owner stayed. Owner `nobody-at-all`: "No user is named 'nobody-at-all'", and nothing ran.
- Not checked: the operations panel row "Requires administrator".

#### 105. Hidden and Locked: PASS
On `a.txt`:
- Tick Hidden: `ls -lO` shows `hidden`. Gezik still lists the file (known gap).
- Tick Locked: `uchg,hidden`. The permission boxes, Octal, Owner and Group are greyed. Untick Locked: `hidden`, and they work again.
- After Done, ⌘Z undid each change in order: the unlock (→ `uchg,hidden`), the lock (→ `hidden`), the hide (→ `-`).
- While the file is locked the Group field is greyed, so "a locked file given a group you are not in" can't be started from the panel. (Typing into the greyed field and Return closed the panel instead.)
- Finder's own view of the hidden file and the lock badge were not looked at.

#### 106. Apply to enclosed items: FAIL
Folder `kutu` with two files, a script (`+x`), a subfolder and a symlink to a file outside.
- Folder set to 750 and group staff, then "Apply to enclosed items…": the question "Everything inside 'kutu' gets this folder's owner, group and permissions. Files keep execute only where they had it; links are left as they are. Undo puts each item back." Cancel did nothing.
- Apply: `ls -lR` shows the files `rw-r-----`, the script `rwxr-x---`, the subfolder `rwxr-x---`, all group staff. The symlink and the file it leads to are unchanged.
- **In `/private/tmp` the items were group `wheel` before** (BSD inherits the parent's group), and I am not in wheel. **⌘Z then failed for every item**: "Undoing Change attributes of 1 item — 4 items failed", with Details `f1.txt: Requires administrator`, `alt: …`, `betik.sh: …`, `f2.txt: …`. The modes were not put back either (still `rw-r-----`), because the mode and the group are restored as one step. Putting back the mode, and saying only the group needs an administrator, would be better.
- Under `$TMPDIR` (group staff before and after): Apply, then ⌘Z put every enclosed item back (`rw-r--r--`, `rwxr-xr-x`), and a second ⌘Z put the folder back (`rwxr-xr-x`).
- Not tried: the big folder with progress and Cancel.

#### 107. Open with: PASS
- One PDF (`rapor.pdf`): "Open with: Adobe Acrobat ▾" lists Adobe Acrobat (default), the other PDF apps, Preview, Safari … and Other…. Choosing Preview changed only that file (`NSWorkspace urlForApplication`: `rapor.pdf` → Preview, `belge-a.pdf` still → Adobe Acrobat). It was set back to Acrobat the same way.
- "Change All…" asks "Open every Document (com.adobe.pdf) file, like 'rapor.pdf', with Adobe Acrobat? This changes it for all your files of this type, in every app." with Change All / Cancel. Cancel changed nothing. Change All was **not** confirmed, so the Mac's PDF default was not touched. (The kind shows as "Document" for com.adobe.pdf here.)
- A folder (`klasor`) and a symlink have no "Open with" row. A `.txt` has one (TextEdit). The `.app` and "Not set" cases were not tried. macOS 26.5.1.

#### 108. Links and ACLs: PASS
- Get Info on a symlink (`a-link` → `a.txt`): Kind "Symbolic link", 27 B, the permission boxes and Octal greyed. Typing group `staff` changed the link's own group (`ls -l`: staff) and not the file's (`ls -lL`: wheel).
- `chmod +a "everyone deny delete" b.txt`: Get Info says "This item has access control entries; they are not shown or changed here".

#### 117. Full Disk Access off: PASS
Terminal (which runs Gezik) has no Full Disk Access.
- Sidebar Trash (under the drives) and Go ▸ Show Trash: the status bar says "Gezik needs Full Disk Access to show the Trash", and a box asks once: "Gezik needs Full Disk Access to show the Trash. Allow it in System Settings ▸ Privacy & Security ▸ Full Disk Access, then open the Trash again." with Open Privacy Settings / Not Now.
- Open Privacy Settings opened System Settings straight at **Privacy & Security ▸ Full Disk Access** (window title "Full Disk Access"). The setting was not changed.
- Opening the Trash again in the same run: the note, no box.
- Items in the GezikHedef image's `/Volumes/GezikHedef/.Trashes/501` are still listed (see 119).

#### 118. Full Disk Access on: NOT TESTED
Not tested: turning on Full Disk Access for Terminal is a system privacy change, left to the maintainer. So the `~/.Trash/.DS_Store` reader (`ptbL`/`ptbN`) was only tried on the disk image's bin (119), where it worked.

#### 119. Real bins on USB volumes: PASS
On GezikHedef (an HFS+ disk image standing in for a USB stick): a file trashed with Finder (AppleScript `delete`) lands in `.Trashes/501/usb-cop.txt`. Gezik listed it at once, and about a second later (when Finder wrote `.Trashes/501/.DS_Store`) showed its Original location `/Volumes/GezikHedef` and Date deleted. Put Back returned it to the volume (not the startup disk). APFS and exFAT/FAT sticks and the foreign-owner case were not tried.

#### 120. Put Back: PASS
Row menu ▸ Put Back on `usb-cop.txt`: it went back to `/Volumes/GezikHedef` and left `.Trashes/501`. ⌘Z put it back in the Trash. **The Trash list doesn't follow** right after Put Back or its undo: the row stayed while the file was on the volume, and stayed after ⌘R while the file had gone back in. Not tried: a deleted original folder, a name conflict, an item with no known place.

#### 121. Delete for good and Empty: PASS
- ⌘⌫ in the Trash on `usb-cop.txt`: "Delete 'usb-cop.txt' permanently? This cannot be undone." Delete / Cancel. Delete removed it from `.Trashes/501`.
- File ▸ Empty Trash…: "Empty the Trash? Permanently delete 1 item (4 B)? This cannot be undone." Empty / Cancel. Cancel did nothing. Empty was not confirmed, because the box counts only the bins Gezik can see, and `~/.Trash` couldn't be listed without Full Disk Access.
- File ▸ Empty Trash… with nothing visible: the status bar says "The Trash is empty".
- Not tried: ⌘⇧⌫, the sidebar/background menus, cancelling an empty with a big folder.

#### 122. Live and refused: PASS
In the Trash: ⌘C, ⌘X, ⌘V and ⇧⌘N say "Not available in the Trash" in the status bar and do nothing. A file trashed in Finder on GezikHedef showed in Gezik's Trash within about 1 s. Go ▸ Show Trash opens it. Not tried: Activity Monitor's open files, ⌘D, rename, a command key, dragging rows out.

#### 124. Panel: PASS
View ▸ System Integration… (menu bar) opens a box with four rows: Default file manager (Folders open in Finder, Off, Make default), Command line (PATH) (gezik is not a command yet, Off, Add), Changes made: 0 (Nothing written to the system), Undo all system changes (Undo all). Esc closes it. Typing in its field did nothing to the rows. ↓ and Return work.

#### 125. Add, no ~/.local/bin: PASS
`~/.local` existed (share, state), `~/.local/bin` did not. Add asked "Add gezik to PATH?" and named every place: the folder `~/.local` if not there, the folder `~/.local/bin` if not there, and the link `~/.local/bin/gezik` to `…/target/release/gezik`. After Add: `~/.local/bin` (0755) has the `gezik` link to the real binary (here the bare `target/release/gezik`, not a `.app`). `system-changes.toml` noted the folder and the link, each with `before = { absent = true }`. The hint box "gezik is a command now" shows `export PATH="$HOME/.local/bin:$PATH"`, and Copy put that line on the clipboard. `~/.zshrc` was not edited, so the "new Terminal" step was not done.

#### 126. Someone else's file: PASS
With `echo hi > ~/.local/bin/gezik`: the panel's PATH row says `/Users/macbookpro/.local/bin/gezik was not …` (made by Gezik; left alone), Off, with no button, and the file still says `hi`. The same with `ln -s /usr/bin/true ~/.local/bin/gezik`: the link was left as it was. Both were cleaned up.

#### 127. Gezik moved: NOT TESTED
Not tested: there is no `Gezik.app` here to copy elsewhere (the build is a bare binary).

#### 128. --unregister: PASS
With Gezik running: `gezik --unregister` printed `undone: /Users/macbookpro/.local/bin/gezik` and `undone: /Users/macbookpro/.local/bin` and exited 0. The link and `~/.local/bin` were gone, and `~/.local` (which existed before) stayed. A second run: `No system-changes.toml: taking back what has Gezik's names`, `Nothing of Gezik's was found.`, exit 0. With another file put in `~/.local/bin` first: `undone: …/gezik`, `left (not empty): /Users/macbookpro/.local/bin`, exit 1, and the folder stayed.

#### 129. No journal: PASS
- Add, then `system-changes.toml` deleted, then `gezik --unregister`: `No system-changes.toml: taking back what has Gezik's names`, `undone: /Users/macbookpro/.local/bin/gezik`, exit 0. Only the link was swept; `~/.local/bin` stayed (empty).
- `version = 9` in a fresh `system-changes.toml`: the panel shows Default file manager and Command line "Fix or delete system-changes.toml first" with `?`, and Changes made "?" with "system-changes.toml cannot be read: version 9 is not one this Gezik knows". `gezik --unregister` printed that line and "nothing was changed", exited 2, and the file was unchanged.
- **Note:** the panel still offers "Undo all" while the journal can't be read. Add wasn't tried in that state.

#### 131. Sidebar: PASS
A CLOUD heading right after PINNED (before SEARCHES) with "iCloud Drive" (cloud icon). A click went to `~/Library/Mobile Documents/com~apple~CloudDocs` (tab "iCloud Drive"). `[sidebar] cloud = false` hid the heading live (the row badges stayed), and `true` brought it back. Resting the pointer on the row showed no tip in the screenshot taken 1.5 s later. No OneDrive or Google Drive is installed.

#### 132. State badges: PASS
In iCloud Drive: files that are downloaded (`ls -lO` without `dataless`) have a small green check on the icon's corner. Selected alone, the status bar ends with "· Available on this device". A file that is only in the cloud (`f8.csv`, `compressed,dataless`) has a cloud badge, and the status bar ends with "· Online only". Outside the cloud folders there is no badge and no words. The list view did not download `f8.csv` (still `dataless` afterwards). The preview panel and grid thumbnails on a cloud-only file were not tried.

#### 133. Download Now / Remove Download: NOT TESTED
The row menu on a cloud-only file has "Download Now" and "Remove Download" (after Delete permanently). Neither was run, because they would download or evict the maintainer's own iCloud files. A test file made for this is needed.

#### 134-138. Data safety, OneDrive/Google Drive, links, late roots, search: NOT TESTED
Not tested. 134 and 138 need a test file to be evicted and uploaded again (turning Wi-Fi off, Remove Download), which would touch the maintainer's iCloud Drive. 135: neither OneDrive nor Google Drive is installed (`~/Library/CloudStorage` doesn't exist). 136 and 137 were left with them.

#### 140. Panel and palette: PASS
System Integration… shows "Default file manager — Folders open in Finder", Off, "Make default" as the first row (seen in 124). The command palette lists "Make Gezik the default file manager" and "Restore the system file manager" (kind Command).

#### 141. Make default from outside a bundle: FAIL
The bare `target/release/gezik`, from the palette ("Make Gezik the default file manager").
- The confirmation names every place it writes: `~/Applications` (if not there), `~/Applications/Gezik.app`, `…/Contents`, `…/Contents/MacOS`, the file `Info.plist`, the link `…/MacOS/gezik` → `target/release/gezik`, "the app for folders (public.folder): com.wenlar.gezik" and "NSFileViewer (Reveal in Finder) = com.wenlar.gezik". (It was not started from `~/Downloads`, so the temporary-place warning was not seen.)
- **Make Default failed:** "Could not make Gezik the default file manager — public.folder: LaunchServices refused it (-50); what this run made was taken back". The rollback was clean: no `~/Applications/Gezik.app`, no `NSFileViewer`, no `system-changes.toml`, and the handler is still `com.apple.finder`.
- **Not the symlink's fault:** a hand-made `~/Applications/Gezik.app` with a *copied* binary and the repo's `crates/gezik/macos/Info.plist` registered fine (`LSRegisterURL` 0), but `LSSetDefaultRoleHandlerForContentType("public.folder", …, "com.wenlar.gezik")` returned -50 for both `kLSRolesAll` and `kLSRolesViewer`. On macOS 26.5.1 LaunchServices does not let a third-party app take `public.folder` this way. The copy route won't fix it. Setting only `NSFileViewer` (Reveal in Finder) may still be possible, and is worth a separate try. The test bundle was unregistered (`lsregister -u`) and deleted, and the handler is `com.apple.finder` again.

#### 142-145: NOT TESTED
They need Gezik to be the folder handler, which 141 could not make on macOS 26.5.1.

#### 146. Restore and `--unregister`: PASS (after the failed attempt)
After the failed Make default (and the hand-made test bundle removed), `gezik --unregister` printed `No system-changes.toml: taking back what has Gezik's names` / `Nothing of Gezik's was found.` and exited 0. The probe and `defaults read -g NSFileViewer` show Finder's state: `public.folder` → `com.apple.finder`, no `NSFileViewer`. The Restore command itself had nothing to restore.

### Fixes on fix/macos-2

Made on `fix/macos-2` (from `master` at `6fcae22`), each checked on this Mac. After them, `cargo test --workspace` passes (all 28 suites; it had 28 failing tests), and clippy and `cargo fmt --check` are clean.

| Finding | Fix | Checked |
|---|---|---|
| Abort when the last window closes | `Registration::drop` uses `TARGET.try_with` | Debug build, window closed twice: clean exit, no panic, no crash report |
| A: restore refused under `/tmp`, `/var` | `fs::through_system_links` reads macOS's own root links (root-owned `/tmp`, `/var`, `/etc` → `/private/…`) before the way back is checked; other links still refuse | The 22 tests pass |
| B: non-UTF-8 names | The tests skip where the file system can't hold such a name | Pass |
| C: Safari.app entered; `x.app` not enterable | `is_package` follows a link, and a `*.app` folder with no `Contents` is a plain folder | Safari started from `/Applications`; `x.app` entered |
| C/86: no special folder icons | `folder_has_own_icon` knows Desktop, Documents, Downloads, Movies, Music, Pictures, Public, Library, Applications in the home folder, and `/Applications`, `/Library`, `/System`, `/Users` | `~` shows Finder's icons |
| D: Windows-only Everything test | `#[cfg(windows)]` | — |
| E: link-loop size test | Expects the link's own bytes to be left out (unix sizes regular files only) | Pass |
| Live reload through a linked config folder | The watcher reads FSEvents' `/private/…` paths back under the folder as given | `theme` edits applied with `GEZIK_CONFIG_DIR=/tmp/gezik-cfg` |
| Folder history kept `/tmp/x` and `/private/tmp/x` apart | Visits go through `through_system_links` | — |
| 88: EXIF orientation ignored | `decode_image` applies the decoder's orientation (new test with an orientation-6 JPEG) | Pass |
| 98: Quick Look fallback window | The fallback waits 500 ms and opens Gezik's window only if the panel isn't there | Open/close with 1 and 3 items, Space and Esc: no extra window |
| 102: Get Info labels overlap | `x: 0` on the three labels | Screenshot |
| 38: raw ffmpeg error for a silent video | "it has no sound to keep" (new test) | Pass |
| 34: WIM is "Document" | An archive the system only calls the generic "Document" gets Gezik's name | `x.wim` shows "WIM archive" |
| 106: undo refused for a foreign group, modes not put back | A refused owner/group still lets the mode and flags through, and still reports "Requires administrator" (new test) | Apply in `/tmp` then ⌘Z: modes back, group stays staff |
| 112: new windows stacked | The saved position (and the 32-point offset) is set again once the native window exists | First window at its saved place, second 32 points off |
| 115: minimized Gezik not activated | Focus and `activateIgnoringOtherApps` again 400 ms after un-minimizing | Twice: Gezik in front |

Not changed:
- 141: macOS 26 refuses `public.folder` (-50) for any third-party bundle. This needs a decision (for example, keep only `NSFileViewer`, or say so in the confirmation).
- 54: `{files}` passes full paths as the spec says.
- 26/40: the native drop menu and Commands submenu not taking synthetic clicks. To be tried with a real mouse first.

## Run 3 (release check)

- Date: 2026-10-12
- Branch: `master` at `14036ca` (results on `test/macos-run3`)
- Machine: MacBook Pro, Apple M2 Pro (Mac14,10)
- macOS 26.5.1 (25F80). System languages en-TR, tr-TR. Keyboard layout Turkish-QWERTY-PC. Appearance: Dark.
- Toolchain: rustc 1.99.0 (b940084d7 2026-09-28)
- Exe: `target/release/gezik` 24,836,272 bytes
- Run by Claude Code on the Mac, at the maintainer's request. Window driven with synthetic CGEvents, checked with `screencapture` and the file system.
- Run: `PATH=/usr/bin:/bin:/usr/sbin:/sbin GEZIK_CONFIG_DIR=/tmp/gezik-cfg ./target/release/gezik /tmp/gezik-rel`, from an empty `/tmp/gezik-cfg`. The repo was used where it is (`~/Work/gezik`), not cloned to `~/gezik`.
- Rules the maintainer added for this run: only `/tmp` test folders and the `GEZIK_CONFIG_DIR` config; `~/Library/Application Support/gezik`, system settings, the keyboard layout and login items are not touched; whatever an item changes (PATH, links, Finder tags) is undone when the item ends; bugs are recorded, not fixed. So `~/GezikReleaseTest`, `~/gezik-tag-probe.txt` and `~/Downloads` are not used: the sub-steps that need them are NOT TESTED, the portable bundle (R49–R51) was made under `/tmp/gezik-rel`, and R52 (writes the real config) is NOT TESTED. Screenshots are in `/tmp/gezik-run3/` (not committed).
- The run was done in two sessions on 2026-10-12/13: R1–R5 first; after the maintainer allowed the synthetic-input helper, the test data was made again and R6–R51 followed. Menu-bar items and the app menu were read and chosen through Accessibility (System Events) where clicking was not reliable. ⌘, and other punctuation keys were sent with the character the Turkish-QWERTY-PC layout puts on that key.
- A Gezik from Run 2 (pid 96680, started 2026-10-10 on `/private/tmp/gezik-test`) was still running and was left alone.
- `~/Library/Application Support/gezik` did not exist before the run (fingerprint empty). `mdfind 'kMDItemUserTags == "*"'` listed 0 files before the run. `~/.local/bin` did not exist.
- HEIC made with sips.

### Summary

| Item | Result | Note |
|---|---|---|
| R1 [build] | FAIL | 2771 passed, 16 failed (gezik 6, gezik-config 1, gezik-core 2, gezik-ops 6, gezik-platform 1) |
| R2 [340–342] | PASS | tests pass, but they skip themselves on the startup disk (R13); Finder probe NOT TESTED (home) |
| R3 [343] | PASS | exFAT image keeps tags |
| R4 [13a–13f, 14e] | PASS | pdfium tests too (17 + 1 + 11) |
| R5 [1] | PASS | live theme reload works through `/tmp` |
| R6 [2] | PASS | |
| R7 [5, 6] | PASS | Finder interop both ways |
| R8 [7, 11] | PASS | template default is `confirm-trash = false`; Put Back under `/tmp` fixed |
| R9 [9, 98] | PASS | upright, no extra window, pages through 3 |
| R10 [72] | PASS | |
| R11 [231, 235] | PASS | |
| R12 [261, 263, Run 2 #1] | PASS | no crash on closing the last window |
| R13 [344] | FAIL | **tags never written to files on the startup disk** (`drive_root` → `/`, read-only system volume); no `–` for a shell tag; no title |
| R14 [345, 346] | FAIL | row right-click menu has no Tags |
| R15 [347] | FAIL | Finder tags not written or read (R13) |
| R16 [348, 349] | PASS | dots, `+2`, ring, Tags column (Gezik's store) |
| R17 [350, 351] | PASS | |
| R18 [353–356] | FAIL | rating/comment only in Gezik's store (R13); rating filter count says `6 / 6` |
| R19 [357, 358] | PASS | Spotlight part with a home file NOT TESTED |
| R20 [359] | PASS | Finder drag NOT TESTED |
| R21 [361] | FAIL | saved search has no `min-rating`/`comment` keys; This Mac NOT TESTED |
| R22 [362, 363] | PASS | |
| R23 [13a 366] | PASS | ⌥P does nothing on Turkish-QWERTY-PC |
| R24 [13a 367] | PASS | memory flat |
| R25 [13a 368] | FAIL | View ▸ Show Preview as Hex does nothing |
| R26 [13b 369] | PASS | pdfium downloaded and loaded; narrow pane NOT TESTED |
| R27 [13b 370] | FAIL | no zoom % in the pane; Fit never becomes Fit Width |
| R28 [13b 371] | PASS | password link only after `›` |
| R29 [13b 372] | PASS | worker goes at 10 s |
| R30 [13c 373] | PASS | ffmpeg 9.0.2 downloaded; sound NOT TESTED (no ears) |
| R31 [13c 374] | PASS | |
| R32 [13c 375] | PASS | system volume untouched |
| R33 [13c 376, 377] | FAIL | `broken.mp4` shown as text, not `Cannot play this file` |
| R34 [13c 378] | PASS | |
| R35 [13d 379] | FAIL | HEIC Dimensions `32 × 24` (not upright) |
| R36 [13d 380, 381] | FAIL | capital `Ö` shown as `O` in properties |
| R37 [13e 382] | PASS | Return does not pick in More Columns… |
| R38 [13e 383] | PASS | |
| R39 [13f 385] | FAIL | OK closes the dialog on an invalid value |
| R40 [13f 386] | PASS | |
| R41 [14a 365] | PASS | memory does not come back down after closing |
| R42 [14a 366] | PASS | |
| R43 [14a 367] | PASS | |
| R44 [14b 369] | FAIL | menu bar keeps the old Copy Path key; old key still works |
| R45 [14c 370] | FAIL | no Replace/Keep Both on installing again |
| R46 [14c 370] | FAIL | Delete Theme's question opens behind Settings |
| R47 [14d 372] | PASS | |
| R48 [14d 373] | PASS | |
| R49 [14e 374] | PASS | bundle in `/tmp` (maintainer's rule) |
| R50 [14e 374] | PASS | 7-Zip download NOT TESTED |
| R51 [14e, Linux 272] | PASS | |
| R52 [14e 375] | NOT TESTED | writes the real config; needs Gatekeeper |
| R53 | PASS | clean-up checks all clean |

**37 PASS, 15 FAIL, 1 NOT TESTED** (53 items). Sound, Spotlight-under-home, Finder-from-home, the narrow pane and the 7-Zip download are NOT TESTED sub-steps inside passing items.

### Main findings

1. **Finder tags, ratings and comments never reach files on the startup disk** (R13, R15, R18, R2): on APFS `/`, `/tmp` and `/Users` share a device number, so `drive_root` returns `/` and the sealed system volume makes `holds = false`; everything goes to `tags-store.tsv`. The macOS meta tests skip themselves for the same reason.
2. `cargo test --workspace` has 16 failures on macOS (R1), mostly Windows/Linux expectations and the verified-copy test hook.
3. Preview: View ▸ Show Preview as Hex does nothing (R25); the pane shows no zoom and Fit never becomes Fit Width (R27); `broken.mp4` is shown as text (R33); HEIC dimensions are not upright (R35); a capital `Ö` loses its dots in the property rows (R36).
4. Settings: a changed key is not shown in the menu bar and the old one still works (R44); installing a theme again has no Replace/Keep Both (R45); Delete Theme asks in the main window, hidden behind Settings (R46); Properties… OK closes on an invalid value (R39); a saved search keeps its words only in `pattern` (R21).
5. Smaller: row right-click menu has no Tags (R14); ⌥P does nothing on Turkish-QWERTY-PC (R23); Return does not choose in More Columns… (R37); the rating filter count says `6 / 6` (R18); the bare binary's app menu says `gezik` and has no Settings… (R41); RSS stays up after the Settings window closes (R41); the template's default is `confirm-trash = false` (R8).

### Items

#### R1 [Run 2 build] Workspace tests: FAIL
- `cargo test --workspace` stops at the `gezik` crate (759 passed, 6 failed), so it was rerun with `--no-fail-fast`: **2771 passed, 16 failed, 23 ignored**. Log: `/tmp/gezik-run3/r1b.log`.
- `gezik`: `admin::tests::folders_are_resolved_before_the_prompt_the_item_is_not` (left `"\\?\/C:\Program Files\x"`, right `"C:\Program Files\x"`); `integration::tests::the_default_row_says_what_it_does` (detail does not end with `(another file manager answers "Show in folder")`); `settings_window::shortcuts::tests::rows_show_every_action_and_command` (`"Win+T"` vs `"Ctrl+T"`), `a_commands_key_goes_to_its_entry`, `a_pressed_key_is_waited_for_written_or_asked_about` ("its key already"), `reset_asks_when_another_has_the_default_key` (Write instead of Ask "Ctrl+W is used by New Tab"). These look like Windows/Linux expectations (Ctrl, `C:\`) running on macOS.
- `gezik-config`: `settings::tests::hotkey_settings_warn` ("bad values keep the defaults", settings.rs:4182).
- `gezik-core`: `elevated::tests::system_folders_and_homes_are_refused_themselves` (`C:\Users\teo` gives `Ok(())`); `highlight::tests::a_row_takes_well_under_a_microsecond_per_rule_set` (136.976 µs; timing, ran in parallel with a release Gezik).
- `gezik-ops`: `engine::tests::a_move_whose_copy_differs_twice_fails_and_keeps_the_source`, `engine::tests::a_replace_whose_copy_does_not_verify_puts_the_old_file_back`, `task::tests::a_copy_that_differs_twice_fails_with_where_and_leaves_nothing` (`unwrap_err` on `Ok`), `task::tests::a_verified_copy_counts_its_bytes_twice`, `task::tests::a_limited_copy_hears_a_cancel_at_once`, `tasks::sync::tests::a_verified_copy_that_differs_fails_and_holds_back_the_trash`. The verify-copy tests see no difference and count 0, so the test hook that corrupts the copy does not take effect on macOS.
- `gezik-platform`: `fs::secure::tests::a_name_is_a_name` (`"sub\\x"` accepted as a name).
- Crash: none.

#### R2 [340–342] Tag storage on a real Mac: PASS (tests); Finder probe NOT TESTED
- `cargo test -p gezik-core meta`: 34 passed. `-p gezik-platform meta`: 9 passed, 3 ignored. `-p gezik-ops meta`: 21 passed, 2 ignored. `-p gezik-ops tags`: 12 passed, 1 ignored.
- `meta::tests::finder_probe` and `finder_wrote_green` were not run: they write `~/gezik-tag-probe.txt` in the home folder, which this run may not use.

#### R3 [343] exFAT keeps tags: PASS
- `hdiutil create -size 200m -fs ExFAT -volname GZEXFAT /tmp/gzexfat.dmg`, attached at `/Volumes/GZEXFAT`. `GEZIK_TEST_TAG_DIR=/Volumes/GZEXFAT cargo test -p gezik-platform meta::tests::a_copy_to_an_exfat_disk_keeps_its_tags -- --ignored`: 1 passed. Detached and deleted afterwards.

#### R4 [13a–13f, 14e] Part tests: PASS
- `-p gezik-core syntax` 7 passed; `-p gezik preview::` 66 passed, 1 ignored; `-p gezik-core pdf_view` 7 passed; `-p gezik-batch fields` 38 passed; `-p gezik-core fields` 14 passed; `-p gezik field` 19 passed; `-p gezik-core template` 9 passed; `-p gezik-search properties` 2 passed; `-p gezik-config paths` 18 passed; `-p gezik-platform audio` matches only 1 ignored test (0 run).
- After R26 downloaded pdfium: `GEZIK_TEST_PDFIUM=/tmp/gezik-cfg/tools/pdfium-8086/libpdfium.dylib cargo test -p gezik-batch --test pdf_view --test pdf_worker --test pdf_tasks`: pdf_tasks 17 passed, pdf_view 1 passed, pdf_worker 11 passed.

#### R5 [1] Start: PASS
- Gezik opened on `/tmp/gezik-rel` with no error line (the Terminal printed only `Slint: long callback/instantiation chain detected`). `/tmp/gezik-cfg` held only `state.toml` and an empty `themes/` (no `settings.toml` was written at start).
- `theme = "light"` written to `/tmp/gezik-cfg/settings.toml` (given through the `/tmp` link): light within 2.5 s; `theme = "dark"`: dark; then `auto`. So Run 2's live reload fix works through the `/tmp` link. Screenshots: `/tmp/gezik-run3/r5-start.png`, `r5-light.png`, `r5-dark.png`.

#### R6 [2] Navigation: PASS
- Double-click `nav`, ⌘↓ on `a` opened `nav/a`; ⌘[ (the key next to P, which types `ğ` on Turkish-QWERTY-PC) went back, ⌘] forward, ⌘↑ up; ⌘T opened a Home tab, ⌘W closed it. Sidebar: FOLDERS (Home …), CLOUD, TAGS, then LOCATIONS with `GezikHedef`, `Macintosh HD`, Trash (scrolled). No beachball. Screenshots `r6-*.png`.
- Note: the defaults are ⌘⇧1 list and ⌘⇧2 grid (⌘1/⌘2 pick tabs); the checklist's "⌘2 grid, ⌘1 list" (R9, R16) does not match the defaults.

#### R7 [5, 6] Copy, cut, Finder: PASS
- ⌘C on `copy/lines.txt`, ⌘V in `nav/c`: a copy. ⌘X on `nav/a/one.txt`: the row turned faint (`r7-cut.png`); ⌘V in `nav/c` moved it; ⌘Z put it back in `nav/a`.
- ⌘C on `nav/c/lines.txt`, ⌘⌥V in `nav/a/b`: moved (gone from `nav/c`).
- Finder interop: `lines.txt` copied in Gezik and ⌘V in a Finder window on `nav/a` arrived there (Finder first showed its own Keep Both/Stop/Replace dialog in `nav/a/b`, where a `lines.txt` already was; Stop). `one.txt` copied in Finder, ⌘V in Gezik in `nav/c`: arrived.

#### R8 [7, 11] Trash and conflicts: PASS (with a note)
- With the fresh config, ⌘⌫ on `nav/c/two.txt` moved it to the Trash **without asking**: the template's default is `confirm-trash = false` (`crates/gezik-config/templates/settings.toml:55`), while the checklist expects "confirm on". ⌘Z put it back to `nav/c` (Run 2 bug A fixed). For the rest of the run `[files] confirm-trash = true` was set in `/tmp/gezik-cfg/settings.toml`.
- With it on: "Move two.txt to the Trash?" (Trash / Cancel), Return trashed it, ⌘Z put it back under `/tmp`.
- Pasting `lines.txt` into `nav/a/b`, which has one: "1 conflict · Copying lines.txt to /tmp/gezik-rel/nav/a/b" with Replace/Skip/Keep both/If newer (`r8-conflict.png`); Keep both + Start gave `lines (2).txt`; ⌘Z removed it.

#### R9 [9, 98] Quick Look and views: PASS
- `photos/iphone.jpg` (stored 32×24, EXIF orientation 6), Space: the system Quick Look panel, shown upright (portrait). Esc closed it; only the main Gezik window was left, no Gezik preview window (98 fix holds).
- ⌘A then Space: the panel has ‹ › and the index button; → paged `iphone.jpg` → `big12mp.jpg` → `iphone.heic`.
- ⌘⇧2 grid (upright thumbnails), ⌘⇧1 list, ⌘A selects 3 (`3 items · 3 selected (188.3 KB)`). Screenshots `r9-*.png`.

#### R10 [72] Search: PASS
- ⌘⇧F in `/tmp/gezik-rel`, `*.txt`, Return: 12 results with a Folder column, `12 results in 0.0 s`. Content, `file 3`, Return: `f3.txt` in `tags`, Match `1: file 3`. Esc closed the search bar, the results stayed.

#### R11 [231, 235] Two panes: PASS
- ⌃⌘P: a second pane on the same folder, active (right). Right pane to `nav/a/b`, Tab to the left, `copy/lines.txt`, fn+F5: `Copy "lines.txt" to /tmp/gezik-rel/nav/a/b?` (with a folder field, Verify copies, Filter…, Don't Ask Again; wording differs from the checklist's `Copy 1 item to …`). Return copied it, the right pane showed it; ⌘Z removed it. ⌃⌘P closed the pane.

#### R12 [261, 263, Run 2 finding 1] Windows and quitting: PASS
- ⌘N: a second window in the same process (one Gezik for `/tmp/gezik-rel`; the other `gezik` in `pgrep` is Run 2's). It opened at (602, 227), 24 points off the first at (578, 203).
- Closing it, then the last window, with the red button: Gezik quit with no dialog and no new report in `~/Library/Logs/DiagnosticReports` (Run 2 finding 1 fixed). Started again: the tabs came back (`copy` and `gezik-rel`, from `state.toml [session]`). ⌘Q also quit cleanly, no report.

#### R13 [344] The picker: FAIL (tags never reach the file)
- ⌃⌘L on `tags/f1.txt`–`f3.txt`: a picker with the seven colours and ⌃1…⌃7, Rating and Comment… (`r13-picker.png`). There is **no `Tags for 3 items` title**.
- `f1` got Orange from the shell first (`settag`, read back with `xattr`): the picker shows **no `–` on Orange**, and the list shows no orange dot for `f1`.
- Return on Red: `✓`. `Proj`, Return on `Create "Proj"` (row says `New tag`): `[tags] list` in settings.toml gained `{ name = "Proj", color = "none" }` (the whole default list was written out with it). Esc: dots in the list (red + a ring). ⌘Z took all of it back.
- **But nothing was written into the files**: `com.apple.metadata:_kMDItemUserTags` of `f1`–`f3` is unchanged (f1 still only Orange, f2/f3 none). The tags went to `/tmp/gezik-cfg/tags-store.tsv` (`tmp/gezik-rel/tags/f1.txt … tag 6 Red\utag 0 Proj`). The same through `/private/tmp/gezik-rel/tags` (`private/tmp/gezik-rel/tags/f1.txt … tag 6 Red`).
- Cause (found, not fixed): on this Mac `stat -f %d` gives the same device (16777229) for `/`, `/private`, `/tmp`, `/Users` and `/System/Volumes/Data` (APFS volume group). `drive_root` (`crates/gezik-platform/src/fs/unix.rs:204`) walks up while the device is the same, so it returns `/` for every path on the startup disk; `volume("/")` then `statvfs`es the sealed system volume, which is read-only, so `holds = false` (`crates/gezik-platform/src/meta/unix.rs:248`). `MetaStore` then neither reads nor writes Finder tags for any file on the startup disk, home folder included; everything goes to the store. The exFAT disk image (R3) is its own device, so it works there.
- So R2's macOS tests do not prove much: the platform tests return early when `!holds(&dir)` (`crates/gezik-platform/src/meta/mod.rs:234`, 260, 380, 395), so they "pass" without writing anything.

#### R14 [345, 346] Keys and menus: FAIL (row menu has no Tags)
- ⌃1 on `f1`–`f3`: status bar `Red added to 3 items`; ⌃1 again took it off; ⌃0: `Tags cleared from 3 items`, a 3-star rating set in the picker stayed (in `tags-store.tsv`: `rating 3`; `mdls` gives nothing under `/tmp`, and the files got nothing, see R13). ⌃1 is not taken by Mission Control on this Mac.
- File ▸ Tags: Red ⌃1, Orange ⌃2 … Gray ⌃7, Proj, Tags… ⌃⌘L, Clear Tags ⌃0, Rating ▸, Comment…, Clean Up Tag Store… (`r14-menu.png`).
- **A row's right-click menu has no Tags at all**: on `f1.txt` tagged Red the native menu is Open, Open with default app, Open With ▸, Open terminal here, Copy path as ▸, Make Alias, Create link, Compress…, Compress to "f1.zip", Convert…, Tools ▸, Cut, Copy, Duplicate, Rename, Move to Trash, Delete permanently, Get Info, Share… (`r14-ctx.png`). `context_menu.rs:823` (`add_tags`) should put a `Tags ▸` right after `Tools ▸` in the native menu. Drive and Trash rows not checked further (they have none by design).

#### R15 [347] Finder both ways: FAIL
- Tags given in Gezik do not reach Finder: the files' `_kMDItemUserTags` stay empty (R13 cause).
- `settag tags/f4.txt $'Blue\n4'` then ⌘R in Gezik: no blue dot on `f4` (`r15-blue.png`); Gezik does not read the file's tags either.
- `ls -lT` before and after: the modified dates did not change (only the `@` of the new attribute).

#### R16 [348, 349] Dots and the Tags column: PASS (in Gezik's store)
- `f5` with five tags (⌃1…⌃5): three dots then `+2`; `f6` with `Proj` (no colour): a ring; a selected row's dots get a light edge; crisp at 2×. The same in grid (⌘⇧2) and columns (⌃⌘3) (`r16-*.png`).
- Header right-click: Hide Modified, Show Created, Hide Type, Hide Size, Show Tags, Show Rating, Show Comment, More Columns…, Reset columns, Group by ▸. Show Tags: a Tags column after Size (`Red, Orange, Yello…`, `Proj`); a click on its header sorts tagged rows first.
- View ▸ Tag Dots off: no dots, `dots = false` under `[tags]` in settings.toml. Turned back on.
- These tags live only in `tags-store.tsv` (R13).

#### R17 [350, 351] Sort, group, filter: PASS (in Gezik's store)
- View ▸ Group By ▸ Tag: `● Red (2)`, `● Proj (1)`, `No Tags (3)` last (`r17-group.png`).
- ⌘F `tag:Red` and `tag:red`: f1, f5 (`2 / 6`); `tag:none`: f2, f3, f4; `tag:any`: f1, f5, f6; `*.txt tag:Red`: f1, f5.

#### R18 [353–356] Ratings and comments: FAIL (nothing reaches the file; `mdls` sub-steps)
- In Gezik: the picker's third star gives ★★★ (store `rating 3`); a 4th star on `f4` then Esc was one job and ⌘Z took it back. File ▸ Tags ▸ Comment… `Ready for review`, Save: stored (`comment Ready for review`). `f1`+`f2` with different comments: the field starts empty with `Comments differ across 2 items` and a `Remove Comments` button. Header ▸ Show Rating: `★★★☆☆`; its header sorts most stars first (the column was off-screen to the right until Modified and Type were hidden; the window is 900 pt wide). Filters `rating:>=3` (f1–f3) and `rating:none` (f4–f6) are right, **but the counter beside the field says `6 / 6` for both** (the tag filters said `2 / 6`, `3 / 6`).
- `mdls -name kMDItemStarRating` / `kMDItemFinderComment`: not possible under `/tmp` (not indexed), and the files got no attributes anyway (R13 cause), so the Finder side fails.

#### R19 [357, 358] TAGS sidebar and Spotlight: PASS (part); home-folder part NOT TESTED
- `mdfind 'kMDItemUserTags == "Red"cd' | wc -l`: 0 (nothing on this Mac is tagged Red in Spotlight, and Gezik writes no file tags, R13). `~/GezikReleaseTest/spot/s1.txt` was not made (home folder not used): NOT TESTED.
- No saved searches yet, so the sidebar is FOLDERS, CLOUD, TAGS (coloured dots, `Proj` with a ring), LOCATIONS. Clicking Red: a `Tag: Red` search tab `in Computer`, `0 results in 0.0 s via Spotlight (drives it does not index are not in it)`, the same as `mdfind`.
- Red's menu (Search this folder, Search everywhere, Filter this folder, Rename…, Color ▸, Move up, Move down, Remove from list, New tag…) ▸ Search this folder in `/tmp/gezik-rel/tags`: `f1.txt`, `f5.txt` (from Gezik's store), `2 results`.
- `[sidebar] tags = false`: the TAGS section went; `true`: back.

#### R20 [359] Drop to tag: PASS (Finder part NOT TESTED)
- Dragging two selected rows onto Purple in the sidebar: `Purple added to 2 items`, nothing moved; ⌘Z took it off. (The first synthetic drag did not start; the second did. It tagged `f4`+`f5` because the first attempt had moved the selection. The `Tag 2 items Purple` label was not caught on a screenshot.) Stored in `tags-store.tsv`, not in the files (R13).
- The drag of `~/GezikReleaseTest/spot/s2.txt` from Finder needs the home folder: NOT TESTED.

#### R21 [361] Search words: FAIL (saved fields); This Mac part NOT TESTED
- On `/tmp/gezik-rel/tags` (⌘⇧F): `tag:Red rating:3` → `f1.txt`; `comment:review` → `f1.txt` (the folder is walked).
- Scope This Mac through Spotlight needs a file under home with a rating and comment: NOT TESTED.
- ⌄ ▸ Save search… `RedReview` ▸ Save with this folder wrote `[[searches]] name = "RedReview"`, `folder = "/tmp/gezik-rel/tags"`, `pattern = "tag:Red rating:3 comment:review"`, `tags = ["Red"]`. There is **no `min-rating` and no `comment` key**; the words stay in `pattern`. (The `tags = ["Red"]` probably comes from the `Filters (1)` left on from clicking Red in the sidebar earlier, not from the words.)

#### R22 [362, 363] Highlight rules: PASS
- The template's four `[[highlight-rules]]` examples, uncommented in `/tmp/gezik-cfg/settings.toml`: `x.tmp` grey, folders changed today green, Red-tagged files (`f1`, `f5`, from the store) bold, in list, grid, columns and search results (`r22-*.png`).
- `min-size = "lots"`: the status bar says `settings.toml: highlight-rules[4]: min-size: expected a size like "500 MB", got "lots"`; the other rules still work.
- `theme = "light"`: folders dark green (`#2e7d32`); `theme = "dark"`: lighter green (`dark-color`); bold names crisp. The settings were put back afterwards.
- Also seen: once a search was saved, the sidebar got a SEARCHES section between CLOUD and TAGS.

#### R23 [13a 366] Code: PASS (with notes)
- With `[system] quick-look = "gezik"`. **⌥P did not open or close the preview pane** on Turkish-QWERTY-PC (Option+P types `π`; the default is `alt+p`, and the View menu item shows no key). View ▸ Show Preview did. (The window grew from 900 to 915 pt when the pane opened.)
- `code/main.rs` and `code/big.rs` (2.2 MB): coloured, line numbers, columns straight at 2×. Page Down, End (last line `}` whole), Home. `W` wrapped and unwrapped; `state.toml [preview] wrap-code = true` was written. Space opened Gezik's own window `big.rs — Gezik` with the same coloured text.
- Seen: after End in `big.rs` the pane showed the end as plain text without line numbers or colour (status `Pl… 2.2 MB · 100%`); the top is coloured. Probably the large-file path; noted for the maintainer to judge.

#### R24 [13a 367] A 3 GB log: PASS
- `big.log` 3,000,000,000 bytes (`2.8 GB`). Selected: the first lines at once, numbered, `1,311+ lines`. End: the last lines at once, `2.8 GB · 100%`. Scroll bar dragged to the middle: `2.8 GB · 49%`.
- RSS (KB): 143,392 before, 134,768 on select, 134,800 at End and middle. `big.log` removed afterwards.

#### R25 [13a 368] Hex: FAIL (menu item)
- `code/libjetbrains-toolbox-interop.dylib` (the first dylib over 100 KB under /Applications, 120,864 bytes): an `exec` icon and a `Hex` button. Space, `H`: rows `00000000 CF FA ED FE 0C 00 00 01 …` with `__TEXT` in the text column; End: last row `0001D810` (`0x0001_D730` at the top), which fits the size; `H` back to the icon. The pane's `Hex` button does the same (4 bytes a row in the narrow pane).
- **View ▸ Show Preview as Hex did nothing** with the dylib selected in the pane (tried twice, through Accessibility; the item is enabled, no checkmark).

#### R26 [13b 369] PDF pages (pdfium worker): PASS (narrow pane NOT TESTED)
- `docs/p50.pdf` (cupsfilter made 49 pages, not 50): page 1 at once with `‹ 1 › − + Fit`. `›`: `More pages need pdfium` with `Download`; the dialog "pdfium needed — More PDF pages need pdfium (~2.3 MB, free)" (Download / Where does it come from? / Cancel); Download put it in `/tmp/gezik-cfg/tools/pdfium-8086/libpdfium.dylib` (7,339,520 bytes) and the bar became `1 / 49` with no codesign or library-validation error.
- `›` `2 / 49`; Page Down `3 / 49`, Page Up `2 / 49`, End `49 / 49`, Home `1 / 49`; click on the number, `25`, Return: `25 / 49`. The same in Space's window (`Hex ‹ 25 / 49 › − 25% + Fit`). Pages sharp at 2×.
- Narrowing the pane to ≈220 px by dragging its edge did not work with the synthetic drag (pane stayed 280): NOT TESTED.

#### R27 [13b 370] Zoom: FAIL (Fit / Fit Width; no zoom text in the pane)
- In the pane (280 px) the bar is `‹ 25 / 49 › − + Fit`: `+`/`−` zoom (`−` shows the page smaller than Fit), but **the pane has no zoom percentage at all**; the button stays `Fit`.
- In Space's window the percentage sits between `−` and `+`: `+` up to `800%`, `1` gives `100%`, `0` fits. **Clicking `Fit` never shows `Fit Width`**: from a zoom it goes back to Fit, and pressed again it stays Fit (no `Fit Width` label at any point).
- Dragging at 800 % showed only the white middle of the page, so following the drag could not be judged. → did not change the page in Space's window.
- `photos/big12mp.jpg` (3000 × 4000) in Space's window: double-click `100%`, drag, `0` back to fit. RSS 150,624 → 209,312 (100 %) → 162,208 KB (after `0`).

#### R28 [13b 371] Password: PASS (with a note)
- `docs/enc_aes256.pdf`: a lock in place of the page and `‹ 1 › − + Fit`; `Password protected · Enter Password…` only appeared after `›` was pressed (not on selecting). A wrong password: `Wrong password`. `pw`: `1 / 3`, then `›` `2 / 3`.

#### R29 [13b 372] The worker goes: PASS
- After paging: one `gezik --pdf-view`. Selecting `report.docx`: still there at 2, 4, 6, 8 s, gone at 10 s. Selecting `p50.pdf` without paging: none started; `›`: one. `info.pdf` then `enc_aes256.pdf` and `›` within 10 s: still one.

#### R30 [13c 373] Play (AudioQueue): PASS (picture); sound NOT TESTED
- Gezik was started with the Homebrew-free PATH; there is no Homebrew ffmpeg on this Mac anyway. `▶` on `media/music/id3v23.mp3`: `Download · Play… 0:00 / --:--`; the dialog "ffmpeg needed — Playing video and audio needs ffmpeg (~20.2 MB, free)". Download put ffmpeg 9.0.2 (with ffprobe) in `/tmp/gezik-cfg/tools/ffmpeg-9.0.2/` and the mp3 then played by itself (`0:01 / 0:02`). The §3 media were then made with that ffmpeg (`av.mp4`, `av.mkv`, `av.webm` with VP8/Opus, `cover.png`, `music/long.mp3`).
- `media/av.mp4` selected: poster with `▶`, the pane bar `▶ 0:30 Mute` (no Hex and no volume bar in the 280 px pane, see R32); no ffmpeg running. `▶`: the first frame by 0.4 s, `0:02 / 0:30` after 2 s, the seek bar moving; two ffmpeg (one `-map 0:v:0 … -vf fps=30,scale=512:278`, one `-map 0:a:0 … -f s16le pipe:1`). `K` paused at 0:16 (picture still), `K` went on (0:18). Space's window: `Hex ▶ [seek] 0:04 / 0:30 Mute [volume]`, plays.
- Sound: no ears here, and neither `log stream --predicate 'subsystem == "com.apple.coreaudio"'` nor `processID == <gezik>` printed anything while it played, so whether sound reached the output is NOT TESTED (a person should check sync).

#### R31 [13c 374] Seek: PASS
- Dragging the seek bar while playing and releasing: it went on from there (0:21). With the pane focused, `L` 0:06 → 0:18, `J` twice → 0:01 (keys did nothing while the focus was elsewhere). The video's right-click: Play/Pause, Back 10 Seconds, Forward 10 Seconds, Mute/Unmute, Show as Hex.
- Paused at 0:17: both ffmpeg still there at 55 s, gone at 60 s. `K`: it went on from the same place (0:19), two ffmpeg again.

#### R32 [13c 375] Volume: PASS
- `osascript -e 'output volume of (get volume settings)'`: 56 before and after (read only). In Space's window the bar right of `Mute` set Gezik's volume while dragging (shorter bar); `M` muted (Mute lit, bar empty). Quit and start: still muted; `state.toml [preview] volume = 30`, `muted = true`. The 280 px pane has no Hex button and no volume bar (`▶ 0:30 Mute`).

#### R33 [13c 376, 377] Other files: FAIL (broken.mp4)
- `av.mkv` and `av.webm` (VP8/Opus) play (`0:02 / 0:30`, two ffmpeg on the file). Sound: NOT TESTED (no ears).
- **`broken.mp4` (11 bytes, `not a video`) is shown as text** (`1 not a video`) with no player, not `Cannot play this file` with the poster.
- `music/long.mp3`: the orange cover as poster, `3:20`; `▶` plays; dragged near the end (3:14) it stopped at `3:20 / 3:20` with `▶`; `▶` again started from `0:01`.

#### R34 [13c 378] Processes go: PASS (with a note)
- `av.webm` playing (two ffmpeg), another file selected: none left by 0.3 s; RSS 131,824 → 132,208 KB. `av.mp4` playing, the pane closed (View ▸ Show Preview): none by 0.5 s; RSS 132,832 → 132,960 KB.
- Quit (⌘Q) while `av.mp4` played in Space's window: no crash report. The two ffmpeg were still there 3 s after Gezik was gone and had gone by 8 s.

#### R35 [13d 379] Photo fields, HEIF: FAIL (HEIC dimensions not upright)
- `photos/iphone.jpg` (pane, Space's window): Photo group with Date Taken `2024-07-01 09:30`, Camera `Apple iPhone 15 Pro`, Lens `iPhone 15 Pro back camera 6.86mm f/1.78`, Exposure `1/250 s`, Aperture `f/1.8`, ISO `100`, Focal Length `6.9 mm`, Location `41.0082° N, 28.9784° E`; size line `24 × 32 pixels` (upright). The test photo's date is 2024-07-01.
- `photos/iphone.heic` (HEIC made with sips; no real iPhone HEIC at hand): the same Photo rows, nothing lost (sips shows make/model only, but Gezik read the EXIF block). **But its `Picture ▸ Dimensions` says `32 × 24`**, the stored size, while the picture is shown upright (portrait); the JPEG says `24 × 32`. The same in Get Info.
- Right-click on the Camera value ▸ Copy Value: the clipboard held `Apple iPhone 15 Pro` (the menu also has Show Column).
- Get Info (File ▸ Get Info) is a sheet on the right; Esc did not close it, and its Done button was below the window's bottom edge until the sheet was scrolled (window 632 pt tall).

#### R36 [13d 380, 381] Music and documents: FAIL (capital Ö shown as O)
- `media/music/id3v24.mp3`, `tags.m4a`, `tags.flac`: Media (Duration 0:02, codec MP3 / AAC / FLAC, Bit Rate, Sample Rate, Channels) and Music (Title `Ağıt Şarkısı`, Artist, Album `Gölge`, Album Artist, Genre `Halk`, Composer `Anonim`); the bar shows `0:02` before `▶`. Track and Year rows were below the pane's edge.
- `docs/report.docx`: Document Title, Author `Ayşe Yılmaz`, Subject `Bütçe`, Pages 12, Words 3,456. `docs/info.pdf`: Title, Author `Ay(s)e A` (right: the file has `(Ay\(s\)e \101)`), Subject, Pages 2.
- **A capital `Ö` loses its dots in the properties**: Artist `Çiğdem Oztürk`, Album Artist `Oztürk Ailesi` (ffprobe: `Çiğdem Öztürk`, `Öztürk Ailesi`), PDF Title `Yıllık Ozet` and DOCX Title `Yıllık Rapor & Ozet` (the files and the reader tests say `Özet`). Lower-case ö, ç, ğ, ı, ş and capital Ç, Ş are right, and a file named `Özet.txt` shows right in the list. The same in Get Info.
- `[preview] properties = false`: no property rows in the pane; Get Info still had them. Put back.

#### R37 [13e 382] Property columns: PASS (with a note)
- In `media/music`: header right-click ▸ `More Columns…`, `art` offers Artist and Album Artist (Music). **Return (and keypad Enter) did not take the highlighted row**: the typed text stayed and later typing piled up (`artalbumdurationp`). Clicking a row turns it on (`Shown`) and keeps the picker open; Esc closes it. Artist, Album and Duration were added that way.
- They fill for the rows on screen (`Çiğdem Öztürk`, `Gölge`, `Ünlü`, `Gezik Test`, `0:02`/`3:20`; the capital Ö is right in columns, unlike R36's properties). Duration's header sorts shortest first, again longest first (`long.mp3` 3:20 at the top). Artist narrowed to 150 survived a restart (`state.toml [[columns]] key = "artist" width = 150`).
- View ▸ Group By ▸ Property… ▸ Album: `Ünlü (1)`, `Gölge (4)`, `Gezik Test (1)`. No file here lacks an album, so `No Album` could not show.
- The new columns are to the right of Tags and Rating, so with the 280 px preview pane open they were off-screen.

#### R38 [13e 383] Property filter: PASS
- In `photos` (all three are the test photo's EXIF: 2024-07-01, Apple iPhone 15 Pro) ⌘F `taken>=2024 camera:iphone`: `3 / 3`; `taken>=2025`: `0 / 3`; `camera:canon`: `0 / 3`. The `Reading properties… n of m` state was not caught (three tiny files read at once).
- `duration>3x`: red `duration>3x: "3x" is not a duration (3m, 1:30)`, the list stays.

#### R39 [13f 385] Search by properties: FAIL (OK on an invalid value)
- ⌘⇧F in `media/music`, `*.mp3` (4 results); Filters ▸ `Properties…` ("Property words, like artist:tarkan duration>3m (empty: any)"): typing `art` lists `artist, album-artist` under the field. `artist:Çiğdem duration>3m`, OK, Search: only `long.mp3`, `Filters (1)`; the Filters menu reads `Properties… (artist:Çiğdem durat…`.
- `duration>3x`: the red line `duration>3x: "3x" is not a duration (3m, 1:30)` appears under the field at once, but **OK closes the dialog** (twice). The bad words were not applied (Filters stayed without a count, 4 results), but the checklist expects OK to do nothing.
- ⌄ ▸ Save search… `LongCigdem` ▸ Save with this folder: `[[searches]] name = "LongCigdem"`, `folder = "/tmp/gezik-rel/media/music"`, `pattern = "*.mp3"`, `where = "artist:Çiğdem duration>3m"`. The sidebar entry under SEARCHES ran it again: `long.mp3`, `Filters (1)`.

#### R40 [13f 386] Name templates: PASS
- `media/rename/` with the music files, ⌘A, File ▸ Rename Items… (`Rename 6 items`) ▸ + Add rule ▸ Name from template. `Fields ▾` ▸ Picture/Photo/Media/Music/Document; Music ▸ Track (`{track}`) inserted `{track}` (the native submenu runs past the window's bottom edge; reached with the arrow keys).
- `{track:02} {artist} - {title}`: `03 Çiğdem Öztürk - Ağıt Şarkısı.mp3`, `07 Öztürk - Gölge.mp3`; the three mp3 copies with the same tags got `3 duplicate names · fix the marked rows` (red ⚠). With a file without an artist (made with ffmpeg) and the optional `AC/DC` file: `05  - Sessiz.mp3` with ⚠ and `1 name with an empty field`; `03 AC_DC - Ağıt Şarkısı.mp3`.
- `{artist|Unknown}` fixed it (`05 Unknown - Sessiz.mp3`). Rename renamed all 4; ⌘Z put the old names back (`ls` the same as before).

#### R41 [14a 365] ⌘, opens it: PASS (with notes)
- ⌘, opens `Settings — Gezik` when sent as the Turkish-QWERTY-PC comma key (the key left of Z on ISO / `\` on ANSI); the US-position key did nothing, which is the layout, not Gezik. View ▸ Settings… opens it too. Pages: General, Appearance, Files and Tools, Layouts, Tags, Keyboard, System, Advanced, Sync and Backup (nine), plus Open settings.toml and Open Config Folder.
- A second ⌘, (main window in front) brought the same window forward, no second one. Closed with the red button and opened again: fresh (General page).
- RSS: 123,648 KB before, 141,616 open, 141,536 just after closing, 141,648 5 s later: it did not come back down.
- The app menu says `About gezik`, `Hide gezik`, `Quit gezik` (lower case), and has no Settings… item (it is under View).

#### R42 [14a 366] One line per change: PASS (with a note)
- Appearance ▸ Density ▸ compact: rows tightened at once (main window and Settings); `diff /tmp/before.toml settings.toml` printed only `[layout]` + `density = "compact"`.
- General ▸ Max fps `5`, Return: the bottom line red `Max fps: max-fps: expected 0 (no limit) or a number from 10 to 1000, got 5 — not written`, the file unchanged.
- Note: typing the default back (`120`, Return) wrote `max-fps = 120` into the file (removed again by hand).
- Pointing at `light` in the Theme list previewed it in the Settings window and the main window; moving away went back to dark.

#### R43 [14a 367] Broken file: PASS
- `[layout` appended to settings.toml with the window open: the bottom line `settings.toml line 37: unclosed table, expected \`]\` — fix it in settings.toml; until then these are read-only`, the controls greyed; clicking Reduce motion wrote nothing (`cmp` same). Line removed: the controls came back; Reduce motion wrote `reduce-motion = true`, then `false` (removed again by hand).

#### R44 [14b 369] Shortcuts page: FAIL (menu bar keeps the old key)
- Settings ▸ Keyboard: keys shown as `⇧⌘S`, `⌘[`, `⌃0`, `⌥⌘C`. Quit Gezik `⌘Q`, Hide Gezik `⌘H`, Hide Others `⌥⌘H`, Minimize `⌘M` have no Set Key… ("macOS keeps this key").
- Copy Path ▸ Set Key… ("Press the keys for Copy Path: Esc cancels, Backspace leaves it no key"), ⌘⌥K: the row says `⌥⌘K`; settings.toml got `[shortcuts]` + one line `copy-path = "mod+alt+k"`. ⌥⌘K copies the selected path (`/tmp/gezik-rel/copy/lines.txt`).
- **But Edit ▸ Copy Path in the menu bar still shows `⌥⌘C` (AX key `C`, Option), and `⌥⌘C` still copies the path too**: the menu bar did not pick up the new key, and the old one keeps working.
- New Tab ▸ Set Key…, ⌘W: `⌘W is used by Close Tab — Replace?` (Replace / Cancel); Esc: file unchanged; Replace: `close-tab = ""`, `new-tab = "mod+w"`. Right-click ▸ Reset to Default on Close Tab asked `⌘W is used by New Tab — Replace?`; Replace gave `close-tab = "mod+w"`, `new-tab = ""`; Reset to Default on New Tab gave `mod+t`. (After that Replace the Close Tab row was left in key-capture mode, so the next typing went there; Esc ended it.)
- ⇧⌘3 in the capture field: nothing reached it (still `Press keys…`), and macOS took a full-screen screenshot (`~/Desktop/Screenshot 2026-10-13 at 00.26.02.png`, written a few seconds later; deleted again). So the system shortcut wins over the capture field, as the checklist wants.
- The `[shortcuts]` lines were removed afterwards. While the Settings window is in front the menu bar has only the Apple and Gezik menus.

#### R45 [14c 370] Get a theme: FAIL (no Replace/Keep Both)
- Settings, search `theme`: a `Get Theme` field ("Paste a GitHub repository … or an https address of a .toml file and press Enter"). The palette's Get Theme… was not tried. Pasted `https://github.com/wenlar/gezik/blob/master/crates/gezik-config/tests/themes/nord.toml`, Return: `1 found: point at a choice in a row to preview the theme`, a `Nord — Not installed` row ("Installs as themes/nord.toml; point at Install to preview it"). The download count was not caught.
- Pointing at `Install`: the Settings window and the main window took Nord's blue-grey; moving away: back.
- `Install` (the first click on the dropdown item did nothing, the second worked): `/tmp/gezik-cfg/themes/nord.toml` with first line `# Installed from https://github.com/…/nord.toml (nord.toml) on 2026-10-13`; status `Installed Nord as themes/nord.toml: Use Now is in its row`; at once an installed `Nord — Installed` row (`themes/nord.toml — From …`) with `Use Now`, `Check for Updates`, `Open Source Page`, `Delete Theme`.
- `Use Now`: `theme = "nord"` in settings.toml, the windows switched. `Check for Updates`: fetched again, `Nord — Installed — The same as themes/nord.toml` (unchanged). `Open Source Page`: the GitHub page opened in the default browser (Firefox Developer Edition); that tab was closed again.
- **Installing it again does not ask `Replace`/`Keep Both`**: the fetched row stays `Installed` (`The same as themes/nord.toml`) and has no Install choice, so `nord-2.toml` cannot be made this way. (The question may only come for a changed file; not tested.)

#### R46 [14c 370] Refusals and delete: FAIL (Delete Theme's question opens behind Settings)
- `http://github.com/a/b`: red `Only https addresses are downloaded`.
- `https://github.com/wenlar/gezik/tree/master/crates/gezik-config/tests/themes`: red `Cannot download https://api.github.com/repos/wenlar/gezik/zipball/master: the download is larger than expected (more than 5 MB: paste the address of a theme's .toml file instead)`.
- No small multi-theme repository was at hand: "no multi-theme repository".
- `nord-2.toml` could not be made (R45), so `Delete Theme` was tried on `nord` (after `theme = "auto"`). **Choosing it in Settings shows nothing there**: the `Move nord.toml to the Trash?` question opens in the main window, behind the Settings window (`settings_window.rs` sends it to `with_focused`). Each try that seemed to do nothing queued another question; after bringing the main window forward, Trash moved the file to the Trash and two more copies of the question had to be cancelled. ⌘Z then brought `themes/nord.toml` back (same first line).

#### R47 [14d 372] Export and import: PASS (with a note)
- Second config `/tmp/gezik-cfg2` (nord.toml, `templates/Note.txt`); main Gezik quit; `GEZIK_CONFIG_DIR=/tmp/gezik-cfg2 gezik /tmp`. Music (under home) pinned from the sidebar's right-click (`pinned = ["{music}"]`), `theme = "nord"`. Settings ▸ Sync and Backup ▸ Export Settings `/tmp/gezik-export.zip`, Return: `Exported 3 files to /tmp/gezik-export.zip` (`gezik-export.toml`, `settings.toml`, `themes/nord.toml`, `templates/Note.txt`). The zip made here was used (no Windows zip).
- Main Gezik (`/tmp/gezik-cfg`): Import Settings `/tmp/gezik-export.zip`, Return: rows `Import settings.toml — 54 settings change: theme: auto → nord, …`, `Import Themes — nord.toml replaces`, `Import Templates — Note.txt new`, `Import from gezik-export.zip — 3 files to write`. Import: the Nord theme, PINNED Music (opens `/Users/macbookpro/Music`), New ▸ Note; `/tmp/gezik-cfg/backups/gezik-settings-2026-10-13-003646-918.zip`.
- Undo Import ▸ Undo: settings.toml and themes/nord.toml the same as a copy taken before (`diff`/`cmp`), `templates/` empty. No new `~/Documents/gezik-settings-*`.
- Note: the import replaces settings.toml whole, so the main config's saved searches (RedReview, LongCigdem) were gone until Undo Import, as the dialog says ("nothing is merged").

#### R48 [14d 373] The red row: PASS
- `[[commands]] name = "Say hi" run = ["echo", "hi"]` added to `/tmp/gezik-cfg2/settings.toml`, exported to `/tmp/gezik-export2.zip`. Previewed in the main Gezik: a red `These run programs on this computer — 1 in settings.toml — Command "Say hi": echo hi. Import only files you trust: nothing runs while importing.`, the bottom line red `1 setting in it runs programs (the red row): import only files you trust`.
- Cancel (clicked; Return on the highlighted Cancel did not close the list): `Nothing imported`; `diff -r` of the config folder against a copy taken before: no difference.
- Restore Backup: `2 backups` (R47's `…003646-918.zip` and `…003754-552.zip`, the second made by Undo Import). Choosing R47's showed its rows (`Import settings.toml — The same — No setting changes`, `Import Themes — The same`, `Nothing to import`); nothing written.

#### R49 [14e 374] Next to Gezik.app: PASS
- Under the maintainer's rule the bundle was made in `/tmp/gezik-rel/Portable` (not `~/GezikReleaseTest`): `Gezik.app/Contents/MacOS/gezik` + `Info.plist`, `codesign --force -s -`, `touch gezik-portable`. `~/Library/Application Support/gezik` did not exist before.
- `open …/Gezik.app`: `gezik-data/` appeared with `settings.toml`, `state.toml`, `themes/example.toml`, `templates/`. Settings ▸ Sync and Backup, first row: `Portable: settings are kept in /private/tmp/gezik-rel/Portable/gezik-data — On`; its `Open Data Folder` opened `gezik-data` in a tab.
- Theme changed (`theme = "light"`), Music pinned, ⌘Q: only `gezik-data` changed; `~/Library/Application Support/gezik` still does not exist (fingerprint unchanged). The app menu reads `About Gezik`, `Hide Gezik`, `Quit Gezik` (from the bundle; the bare binary says `gezik`, see R41), plus `Quit and Close All Windows`. `codesign --verify` passes.

#### R50 [14e 374] Two at once: PASS (7-Zip part NOT TESTED)
- Portable Gezik open (pid 40808), then `GEZIK_CONFIG_DIR=/tmp/gezik-cfg gezik /tmp/gezik-rel` (pid 40813): two processes, two windows, each with its own settings (portable: light theme, PINNED Music; the other: dark, SEARCHES RedReview/LongCigdem). `gezik --help` prints its usage as usual.
- In the portable one, View ▸ System Integration… then the Default file manager row: `Make Gezik the default file manager? Gezik is running in portable mode from /private/tmp/gezik-rel/Portable. These stop working when the drive is removed. …` (Make Default / Cancel); the Command line (PATH) row: `Add gezik to PATH? Gezik is running in portable mode from … ` (Add / Cancel). **Cancel** both times: no `system-changes.toml` in `gezik-data`, `~/.local/bin` still does not exist, nothing in LaunchServices.
- Compress… / 7-Zip download: not tried (no one to judge a large download): NOT TESTED.

#### R51 [14e, Linux 272] Read-only data folder: PASS
- Portable Gezik quit; `hdiutil create -srcfolder /tmp/gezik-rel/Portable -format UDRO -volname GZPORT /tmp/gzport.dmg`, attached; `open /Volumes/GZPORT/Gezik.app`: the portable settings (light, PINNED Music) and the status bar `portable: Portable data folder is read-only: changes are not saved`.
- Dragging a column header and pinning Desktop: no crash; the status bar said `settings.toml: cannot write: Read-only file system (os error 30)`; `shasum` of the image's `gezik-data/*.toml` unchanged. ⌘Q: no crash report. Detached and deleted. `~/Library/Application Support/gezik` still does not exist.

#### R52 [14e 375] App Translocation: NOT TESTED
- It writes `~/Library/Application Support/gezik`, which this run may not touch (and it needs a person for Gatekeeper).

#### R53 Clean-up record: PASS
- Every Gezik of this run quit with ⌘Q (pid 17964 of the first session was stopped with TERM); no `gezik*.ips` in `~/Library/Logs/DiagnosticReports` at any point of the run. No ffmpeg left. Run 2's Gezik (pid 96680) is still running, untouched.
- `GZEXFAT` and `GZPORT` detached and their images deleted (`hdiutil info | grep -c GZ`: 0). The portable `Gezik.app` unregistered with `lsregister -u`, then `rm -rf /tmp/gezik-rel /tmp/gezik-cfg2 /tmp/gezik-export*.zip`. `~/GezikReleaseTest` and `~/gezik-tag-probe.txt` were never made. `/tmp/gezik-cfg` is kept (it holds the downloaded ffmpeg 9.0.2 and pdfium-8086).
- `ls -la ~/.local/bin`: "No such file or directory".
- Application Support fingerprint: `diff /tmp/as-before.txt /tmp/as-after.txt` printed nothing (the folder still does not exist).
- `mdfind 'kMDItemUserTags == "*"' | wc -l`: 0, the same as before. The only shell-made Finder tags (`f1` Orange, `f4` Blue) were removed in their items.
- Outside the test folders: one stray `~/Desktop/Screenshot 2026-10-13 at 00.26.02.png` from R44's ⇧⌘3 was deleted (Desktop listing the same as before); one Firefox tab opened by R45's Open Source Page; `nord.toml` from R46 went to the Trash and was put back with ⌘Z (nothing left in the Trash from this run). No new `~/Documents/gezik-settings-*`.


### Fixes on `fix/macos-run3` and re-test (2026-10-13)

Branch `fix/macos-run3` from `master` at `838f00a`, one commit per fix, each naming its item. Re-tested with a release build of that branch, the same way as above (`/tmp/gezik-rel`, `GEZIK_CONFIG_DIR=/tmp/gezik-cfg`).

| Item | Before | After | Commit / note |
|---|---|---|---|
| R13, R15, R18 | FAIL | PASS | `8191f19`. On APFS the firmlinked data volume has the same device number as the sealed `/`, so `drive_root` stopped at `/` and the read-only system volume made `holds = false`: nothing was ever written to a file on the startup disk. Now the data volume decides; EROFS from the sealed part refuses that file only. Re-test: ⌃1 on `f1`–`f3` writes `_kMDItemUserTags` (`["Red\n6"]`) and `com.apple.FinderInfo` byte 9 `0C`; Finder's `label index` is 2 (Red); a rating writes `kMDItemStarRating`; ⌘Z removes them. A new test, `meta::tests::the_startup_disk_holds_records`, fails without the fix. |
| R1 | FAIL (16) | 6 left | `c74ca58` the verified-copy and limited-copy tests now turn APFS cloning off (a clone is not read back or slowed, by design); `7129743` a backslash is refused as a name on Windows only; `8890ec0` `*.x` patterns match from the end of the name (highlight rules: 6.2 µs → under 5 µs a row in release); `a321866` Windows home folders are recognised when the tests run on macOS; `814765e` the hotkey test expects macOS's own refusal. `cargo test --workspace --no-fail-fast`: **2810 passed, 6 failed, 23 ignored**. The 6 are in `admin.rs`, `integration.rs` and `settings_window/shortcuts.rs` (the "Win+T" label among them), which PR #114 changes; left to it. `cargo clippy --workspace --all-targets` and `cargo fmt --check`: clean. |
| R25 | FAIL | PASS | `31343cb`. The menu bar ran keyless actions only through `actions::run`; View ▸ Show Preview as Hex has no key and fell through. Now run as the palette runs them. Re-test: the menu item shows the dylib as hex. |
| R44 | FAIL | PASS | `bc4dfaf`. The menu bar's keys were literals in `app.slint`. Items now take their action's key from the shortcuts in effect, again after each settings change. Re-test: with `copy-path = "mod+alt+k"` Edit ▸ Copy Path shows ⌥⌘K (Accessibility still reported the old key until the menu was rebuilt, but the drawn menu and the keys are right), ⌥⌘K copies the path, ⌥⌘C no longer does. |
| R39 | FAIL | PASS | `926ca10`. Search criteria (Properties…, sizes, dates) now keep OK until the value parses. Re-test: `duration>3x` + OK stays open with the red reason; `art` + Return says `"art" is not a property word`. |
| R33 | FAIL | PASS | `03d8325`. A video or sound file is never shown as text. Re-test: `broken.mp4` shows the poster and `▶`; `▶` says `Cannot play this file`. |
| R35 | FAIL | PASS | `dc25b57`. A HEIC's `irot` turn swaps width and height. Re-test: the sips HEIC (`irot` 3) says `24 × 32`, like the JPEG. |
| R46 | FAIL | PASS | `aba75f4`. The main window comes forward before Delete Theme's question. Re-test: `Move nord-2.toml to the Trash?` in front; Trash, ⌘Z brought it back. |
| R36 | FAIL | PASS | `dc03c9d`. Not the readers: the software renderer cuts glyphs at a `Text`'s own box (`vendor/i-slint-renderer-software/lib.rs:2899`, a FIXME there), and a property value's box was exactly its font height, so the dots above a capital Ö/Ü were cut. A no-wrap experiment ruled out word wrap. The row's texts now start 3 px lower inside a taller box. Re-test: `Öztürk`, `Ünlü`, `Çiğdem Öztürk`, `Öztürk Ailesi`. |
| R14 | FAIL | PASS (test error) | No change. macOS uses Gezik's own row menu, where the tags are at the end of Tools ▸ under a greyed `Tags` (`• Red` on a tagged row), as the checklist says. The first run did not open Tools ▸. |
| R21 | FAIL | PASS (test error) | No change. Words typed in the bar stay in `pattern`; `tags`, `min-rating`, `comment` come from Filters ▸ Tags…/Rating/Comment… (the original item 361 says so). Re-test: Filters ▸ Tags… Red, Comment… review, Rating ★★★, Save search… wrote `tags = ["Red"]`, `min-rating = 3`, `comment = "review"`. |
| R27 | FAIL | PASS (test error) | No change. In Space's window: Fit from 25 % gave the fitted page, Fit again gave `Fit Width` (shown between − and +). The first run's clicks missed because the bar's buttons move when the figure comes and goes; the 280 px pane leaves the figure out by design (`!narrow`). |
| R45 | FAIL | PASS (test error) | No change. Replace / Keep Both are offered when the installed file differs (`Differs`); an identical one says `The same as themes/nord.toml`. Re-test: after changing the installed `nord.toml`, Get Theme showed `Differs`; Keep Both made `nord-2.toml`, row `Nord (nord-2.toml)`. Test themes put back afterwards. |

After the fixes: **51 PASS, 1 FAIL, 1 NOT TESTED** (53 items): R1 stays FAIL until PR #114 lands (its 6 tests), R52 NOT TESTED. Of the first run's 15 FAILs, 4 were test errors (R14, R21, R27, R45), 10 were fixed, and R1 is down from 16 failing tests to 6.

Clean-up after the re-test: `/tmp/gezik-rel` removed; Application Support fingerprint unchanged (folder still absent); `~/.local/bin` absent; `mdfind 'kMDItemUserTags == "*"'` 0; no `gezik*.ips`; Desktop unchanged; no disk images attached.

## Run 4 (master)

- Date: 2026-10-15
- Branch: `master` at `9e95032` (fixes and results on `fix/macos-run4`)
- Machine: MacBook Pro, Apple M2 Pro (Mac14,10)
- macOS 26.5.1 (25F80). System languages en-TR, tr-TR. Keyboard layout Turkish-QWERTY-PC. Appearance: Dark.
- Toolchain: rustc 1.99.0 (b940084d7 2026-09-28)
- Exe: `target/release/gezik` 28,165,888 bytes (master); 28,182,400 bytes with the fixes
- Run by Claude Code on the Mac, at the maintainer's request. Window driven with synthetic CGEvents and Accessibility (System Events), checked with `screencapture`, `stat`, `xattr` and the file system.
- Run: `PATH=/usr/bin:/bin:/usr/sbin:/sbin GEZIK_CONFIG_DIR=/tmp/gezik-cfg ./target/release/gezik /tmp/gezik-run4/data`, from an empty `/tmp/gezik-cfg` (Run 3's was moved to `/tmp/gezik-cfg-run3`, keeping its ffmpeg and pdfium). Test data under `/tmp/gezik-run4/data`; screenshots in `/tmp/gezik-run4/shots/` (not committed).
- Scope: the items of 14-macos.md that Run 3 did not run (371, 372–373 again with a zip from a second config, 375–378), then 15, 16, highlight-where, commands-more, l10n-overflow, update, network-16h, 16i–16m and 17. Rules for this run: temporary changes to system settings, `sudo` (run by the maintainer) and the real Application Support were allowed if undone when the item ended.
- 372–373 and 17 ask for a second computer (the Windows PC). The maintainer asked for every item to be run here, so the "other computer" is a second Gezik on this Mac with its own config (`GEZIK_CONFIG_DIR=/tmp/gezik-cfg-b`), against the real `https://sync.wenlar.com`. Windows-side behaviour (paths, `%APPDATA%`) is not covered.
- Run 2's Gezik (pid 96680, on `GEZIK_CONFIG_DIR=/private/tmp/gezik-cfg`) was still running; at the maintainer's choice it was quit with ⌘Q (no crash report) and `/tmp/gezik-cfg` emptied again.
- Fingerprint before: `~/Library/Application Support/gezik` absent; `~/.local/bin` absent; `mdfind 'kMDItemUserTags == "*"' | wc -l` 0; Desktop `.DS_Store`, `.localized`, one screenshot of July, `Thumbs.db`; no `gezik*.ips`; disk images: four CoreSimulator images and `/tmp/hedef.sparseimage` (GezikHedef, from earlier runs).
- HEIC made with sips (no iPhone photo at hand).

### Summary

| Item | Result | Note |
|---|---|---|
| A1 [build] | PASS | `cargo test --workspace --no-fail-fast` 3117 passed, 0 failed, 25 ignored; clippy `-D warnings` and `fmt --check` clean |
| A2 [17] live cloud test | PASS | 4 passed with `--include-ignored` (NSURLSession path) |
| A3 [16g] `prepare-libmtp.sh` | FAIL → fixed | four stops on a real Mac; release not published (no `gh` login here) |
| 371 [14c] | NOT TESTED | no proxy network |
| 372 [14d] | PASS | zip from a second config, not from Windows |
| 373 [14d] | PASS | as 372 |
| 375 [14e] | PASS (test error) | the app itself must be moved, not its folder |
| 376–378 [14f] | NOT TESTED | needs iCloud/Dropbox on the Windows PC too |
| 388 [15a] | PASS | |
| 389 [15a] | PASS | |
| 390 [15b] | PASS | |
| 391 [15b] | PASS | by the accessibility tree, not by ear |
| 392 [15b] | PASS | |
| 393 [15c] | PASS (test error) | Undo's job name is in the right-click menu; a broken settings.toml gives the defaults |
| 395 [15d] | PASS (test error) | the item's names look the same in both forms |
| 396 [l10n] | FAIL → fixed | Batch rename layer cut in the pseudo language |
| 397 [16a] | PASS | |
| 398 [16a] | PASS | Finder window instead of the Desktop |
| 399 [16a] | PASS | |
| 400 [16a] | PASS | |
| 401 [16b] | PASS | |
| 402 [16b] | FAIL → fixed | the kept copy was always numbered |
| 403 [16c] | PASS | |
| 404 [16c] | PASS | `>vim` keeps running, as vim does with a pipe |
| 405 [16d] | PASS (test error) | no-Command-Line-Tools part NOT TESTED: the item's stand-in does not work |
| 406 [16d] | PASS | |
| 407 [16e] | PASS | local bare remote; Keychain/ssh-agent parts NOT TESTED |
| 408 [16e] | PASS | |
| 409–412 [16g] | NOT TESTED | no Android phone |
| 413 [commands] | PASS | |
| 414 [commands] | PASS | phone part NOT TESTED |
| 415 [commands] | PASS | |
| 423 [highlight] | FAIL → fixed | `kind = "audio"` left out AIFF |
| 424 [highlight] | PASS | |
| 425 [highlight] | PASS | |
| 426–431 [update] | PASS | six items |
| 432 [update] | NOT TESTED | no release newer than rc.3 |
| 433–436 [16h] | NOT TESTED | no Mac or NAS announcing SMB |
| 443 [16i] | PASS | |
| 444 [16i] | PASS (test error) | the trashed link still counts |
| 445 [16i] | FAIL → fixed | the refusal named the wrong item |
| 446 [16i] | PASS (test error) | as 444 |
| 447 [16i] | PASS | |
| 448 [16i] | PASS | SMB part NOT TESTED |
| 453–458 [16j] | PASS | six items |
| 463–470 [16k] | PASS | eight items |
| 473–480 [16l] | PASS | eight items |
| 481 [16l] | FAIL → fixed | a read-only disk image sprang and offered a copy |
| 482 [16l] | PASS | |
| 483 [16m] | PASS | |
| 484 [16m] | PASS (test error) | no 160 px minimum |
| 485 [16m] | PASS (test error) | Swap Panes is ⌃⌘U on macOS |
| 486–492 [16m] | PASS | seven items |
| 493–502 [17] | PASS | ten items, second Gezik on this Mac |

**82 PASS, 6 FAIL, 13 NOT TESTED** (101 items, ranges counted per item). Eight PASS items were test errors (checklist fixed).

### Main findings

1. `scripts/tools/prepare-libmtp.sh` had never run on a Mac: libmtp's configure wants a pkg-config, the tarball's own Linux `gphoto2-endian.h` shadowed the generated one, and both of its checks (architectures, links) misread `lipo`/`otool` output. Fixed; the package builds (289,012 B, SHA-256 `9959f5557052f66882a77a1038a07bcd273548ec2c2c6b7c2e79f20cbc372864`), is universal, links only the system and `@loader_path`, and loads (`LIBMTP_Init`). The `libmtp-1.1.23-1` release and `tools.rs`'s PENDING entry wait for the maintainer (no `gh` login on this Mac).
2. Five app bugs, all fixed and re-tested: Keep a Copy in Downloads always numbered the name (402); AIFF/M4B were not sound for `kind` rules, search and icons (423); Create hard link named the first item instead of the folder it refused (445); drops onto a read-only volume were offered and sprang open (481); the Batch rename layer cut two buttons in long languages (396).
3. Eight checklist errors (375, 393, 395, 405, 444, 446, 484, 485): fixed in the checklists.
4. Smaller, not fixed (notes): the menu bar's Edit ▸ Undo never names the job; ⌘W does not close the Settings window; a broken settings.toml drops every setting to its default (as spec 14 says) including the language; the settings search for `import-settings` hides the "Import from …" row that the bottom line tells you to use; Go ▸ Show Trash opens System Settings ▸ Full Disk Access by itself; tests leave `gezik-16e-*` and `gezik-backup-test-*` folders in `$TMPDIR`; a stacked or bottom preview can squeeze a pane to no rows.

### Items

#### A1 Build and tests: PASS
- `cargo build --release -p gezik`: 5 min 05 s, 28,165,888 bytes.
- `cargo test --workspace --no-fail-fast`: 3117 passed, 0 failed, 25 ignored (Run 3's 6 leftover failures are gone).
- `cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.
- The checklists of this run name no separate probe.

#### A2 Live cloud sync test: PASS
- `cargo test -p gezik cloud_sync -- --include-ignored`: 4 passed, `the_live_server_carries_a_change_across` among them (3.38 s). The macOS HTTP path is `gezik_platform::http` (NSURLSession, `http/macos.rs`).

#### A3 libmtp package: FAIL → fixed (`9afaec6`), release not published
- `bash scripts/tools/prepare-libmtp.sh /tmp/gezik-run4/libmtp` stopped four times: `configure: error: pkg-config not found` (with `PKG_CONFIG=false`); `'byteswap.h' file not found` (the tarball's `src/gphoto2-endian.h` was written on Linux and found before the generated one); `libusb-1.0.0.dylib: x86_64 arm64` (the architecture check wanted two spaces between the names); `links something outside the package or the system` (it read otool's `(architecture …):` header line as a library).
- After the fix: `libmtp-1.1.23-macos-universal.tar.xz: 289012 B, sha256 9959f5557052f66882a77a1038a07bcd273548ec2c2c6b7c2e79f20cbc372864`; both dylibs `x86_64 arm64`, linked to `/usr/lib/libiconv`, `libobjc`, `libSystem`, IOKit, CoreFoundation, Security and `@loader_path`; `codesign --verify` passes; `ctypes.CDLL` + `LIBMTP_Init()` works.
- The maintainer agreed to publish, but `gh` has no login on this Mac (and no GitHub token in the Keychain), so the release and the `tools.rs` entry are left for the maintainer.

#### 371 [14c]: NOT TESTED
- No network with a proxy at hand.

#### 372 [14d]: PASS (second config, not Windows)
- The "Windows" side was a second Gezik on this Mac (`/tmp/gezik-cfg-b`): theme `benim` (a copy of `nord.toml` named Benim), `pinned = ["{documents}"]`, `templates/Not.txt`; Export Settings `/tmp/gezik-run4/gezik-win.zip` (`gezik-export.toml`, `settings.toml`, `themes/benim.toml`, `templates/Not.txt`).
- Import Settings in the main config: rows `Import settings.toml — 6 settings change: theme: light → benim, pinned: → {documents}, …`, `Import Themes — benim.toml new`, `Import Templates — Not.txt new`, `Import from gezik-win.zip — 3 files to write`. Import: the Benim theme at once, `pinned = ["{documents}"]`, `templates/Not.txt`, a new `backups/gezik-settings-2026-10-15-205137-210.zip`. Undo Import ▸ Undo: `settings.toml` the same as before (`diff`), themes and templates empty again.
- Note: with the settings search set to `import-settings` the `Import from …` row (with the Import choice) is not listed, while the bottom line says "choose Import in the last of these rows"; on the Sync and Backup page it is there.

#### 373 [14d]: PASS (second config, not Windows)
- `[[commands]] name = "Say hi" run = ["echo", "hi"]` added on the second config, exported to `gezik-win2.zip`. Preview: red `These run programs on this computer — 1 in settings.toml — Command "Say hi": echo hi. Import only files you trust: nothing runs while importing.`, red bottom line `1 setting in it runs programs (the red row): import only files you trust`. Cancel: `Nothing imported`, `diff -r` of the config folder against a copy: no difference. Restore Backup lists the backups (4).

#### 376–378 [14f]: NOT TESTED
- They need the same iCloud Drive or Dropbox folder on the Mac and on the Windows PC. (A local sync folder was set and stopped in 501.)
#### 375: PASS (test error, `ea00c02`)
- Gatekeeper "Gezik Not Opened" (Move to Trash/Done) for ad-hoc + quarantine; maintainer Open Anyway. Translocated: `~/Library/Application Support/gezik` created (settings, state, themes, templates, update/ empty), state [update] checked; note "portable: Move Gezik.app out of Downloads (or open it once from Finder after moving) to use portable mode" in red at the right of the status bar (seen on the 2nd start; the 1st start's shot ~1 min later had none). AppleScript Finder move of folder: still translocated (quarantine 0081->00c1). Maintainer dragged folder in Finder: still translocated. Maintainer dragged Gezik.app itself into moved3/ (with marker): runs from moved3, quarantine 01c1, gezik-data made, no note, Application Support unchanged after quit. Cleaned: `~/Library/Application Support/gezik` removed, zip removed from Downloads, lsregister -u.

#### 388: PASS
- per-app tr (defaults write com.wenlar.gezik AppleLanguages -array tr): UI Turkish (Bilgisayar, Görünüm, 7 öğe); Settings Dil "Sistem: Türkçe"; list auto, en, de/es/fr/zh-Hans "%0 gözden geçirildi", tr, pseudo. pseudo: titles «Ŝéţţıñĝš — Ĝéžık ·····», «data — Ĝéžık ··», « » draw; auto -> Ayarlar — Gezik. Notes: Type column (system kind "Folder") and sidebar Desktop/Documents/..., tag names stay English. Removed per-app language (defaults delete).

#### 389: PASS
- lang/pl.po (pot + X-Gezik-Name + Ustawienia): "pl — Polski — %0 çevrildi" within a second; chosen: Ustawienia — Gezik, rest English. Broken quote: "lang/pl.po line 13237: expected a string in double quotes", title back to system language (Ayarlar, process had tr). Fixed: Polish. "{} % translated" -> "procent": "lang/pl.po line 9397: "{0}" missing in the translation". Note: the count warning "lang/pl.po: 3.131 metinden 3.130 tanesi eksik" stayed in Turkish while language = pl.

#### 390: PASS
- 284 menu-bar lines framed (dump /tmp/gezik-run4/menus-pseudo.txt) incl. About/Hide/Quit Gezik, Tags/Rating/Group By/Date Format/Size Format; unframed: tag names, Services submenu (system), ★ rows. No Commands menu (none set), no Open Tab Set (none saved). Right-click row, empty list, tab, sidebar, column header all framed.

#### 391: PASS (by AX tree, not by ear)
- Back «Báçk ··», tab close «Çlöšé data ··», address «Åddréšš ··», rows by file name.

#### 392: PASS
- page names, rows, descriptions, layout.sidebar choices framed; right -> sidebar = "right", row «rıĝhţ ··»; Settings search "copy pa" -> Copy Path; palette "copy pa" and framed "Çöpý Páţh" find it. Notes: ⌘W does not close the Settings window (and ⌘⇧P there types into the search); sidebar.tree-follow description names Explorer on macOS.

#### 393: PASS (test error, checklist fixed)
- copy (Edit ▸ Undo plain «Üñdö»; empty-list menu «Üñdö «Çöpý 1 ıţém»»), trash question «Mövé s1.txt ţö ţhé Ţrášh?», conflict list, eject (GezikHedef, reattached), Checksums, Compare/Split status lines, Size map, Export list, Convert, Batch rename, System Integration rows + question (cancelled, nothing written), Get Info, picture and sound preview, search «3 résülţš ıñ 0.0 š». Broken settings.toml: whole app back to defaults (theme light->dark, language pseudo->system), warning English (toml's own words) — as spec 14 says ("bozukken varsayılanlar"). Notes: ⌘I on the Turkish-QWERTY-PC key that types ı does nothing (the i key works); sound file Convert preset defaults to MP4 (H.264 + AAC).

#### 395: PASS (test error, checklist fixed)
- 测试这检查对齐 / 日本語のファイル no boxes. AppleLanguages en-TR, ja-TR, tr-TR (restored to en-TR, tr-TR after): the two names pixel-identical before/after (shared forms). Probe (temporary test, reverted): text locale ja-TR -> fallback ["Hiragino Sans W4", "Helvetica", "Hiragino Sans GB W3"]; zh-Hans -> ["Hiragino Sans GB W3", ...]; PingFang SC not found by fontique. 直骨角今令.txt: 令 Japanese form with ja, Chinese form without.

#### 396: FAIL → fixed 3f74d65
- pseudo, 900 pt window: Get Info (Owner/Group/Everyone, Read/Write/Execute/Hidden/Locked) fits; Compress (levels, Password) fits; Convert text (Line endings) fits; conflict list Action column fits; Settings ▸ Keyboard key text elides ("«Ñöñé…") before Set Key…, never under it. Batch rename: "«Öñlý çháñ…" (Only changed) and "«Réšéţ ţö rü…" (Reset to rules) cut at the layer's right edge (fit at 1200 pt). After the fix: Preview elides away, "«Šörţ …", both fit; English unchanged. Window title follows the language at once. Notes: with the rename layer open, Rust-made texts (title "Rename 3 items", rule summaries, "3 will change") stay in the old language until it is reopened; the keyboard.typing description names Ctrl+F on macOS.

#### 397: PASS
- x.zip (ditto, as Finder Compress) double-click: lists docs, nothing new next to it; docs: …/x.zip/docs; ⌘↑ twice: arch with x.zip selected; ⌘L …/x.zip/docs opens; right-click has Open as Folder, Open with System App (Archive Utility extracted "docs 2", removed). Note: a zip's own menu offers Compress to "x.zip".

#### 398: PASS
- quick-look = "system": Space in look.zip opens Gezik's window photo.jpg — Gezik; ↑ to hp.pdf — Gezik; pane shows both; ⌘C pdf, ⌘T tab on /tmp/gezik-run4/drop, ⌘V: there, ⌘Z: gone. Finder ⌘V: nothing (clipboard text = path). Drag of 2 rows to a Finder window on /tmp/gezik-run4/drop (not Desktop): both arrive.

#### 399: PASS
- s.7z made with Gezik's 7-Zip 26.03 (gezik-tools, sha256 as manifest) -mhe=on -pgizli: asks; wrong "Wrong password. Try again:"; Cancel stays; gizli opens; d1.txt opens in TextEdit without asking. nest.zip/nest/src.tar.gz: shows src. (Entry counting not caught: tiny.)

#### 400: PASS
- tab in x.zip/docs, ⌘Q, start: back (state.toml [[session.tabs]] archive + inner). x.zip to Trash with Finder, start: tab on arch, status "/tmp/gezik-run4/data/arch/x.zip/docs no longer exists". x.zip remade with ditto (Trash not readable here).

#### 401: PASS
- y.zip (3 files): 2 files dragged from Finder (fsrc) into its list: 5 entries (unzip -l), ditto -x (Archive Utility's engine) reads them; ⌘Z: 3. Rename y1 -> r1 (Return), ⌘Z back; ⌘⌫ y2: "Delete y2.txt from the archive? You can undo this." Delete, ⌘Z back; ⇧⌘N "New folder" in rename, "yeni": two undo steps (rename, then the folder). ⇧-drag y3 from the zip to a second pane on /tmp/gezik-run4/drop: moved out (zip y1, y2). Final zip via ditto -x: y1, y2. Drag into r.rar: status "This kind of archive can only be read" (cursor not in screenshots). Old zips go to the Trash: Trash leftovers.

#### 402: FAIL → fixed 1686bbb
- w.zip notes.txt opens in TextEdit; ⌘S: "notes.txt changed … from w.zip" Update in Archive / Ignore; Ignore, save again: asked again; Update: zip has "Notes v3" (ditto -x too). Changed again, Ignore, ⌘↑: "notes.txt changed but is not in w.zip" Update in Archive / Discard / Keep a Copy in Downloads; Keep: ~/Downloads/"notes (2).txt" although there was no notes.txt (next_free always numbers). Removed from Downloads.

#### 403: PASS
- cmd/ ⌘L ">ls -la": panel with the listing, folder, "Exit code 0 · 0.0 s" green; two selected, >echo "$GEZIK_FILES": both paths, one per line; >sleep 30: "Running… 4.0 s", Stop: gone 0.07 s after the click, "Stopped · 3.1 s" red; >ls nothing-here: stderr in the warning colour, "Exit code 1" red; ">ls" pasted, click on the list: nothing runs.

#### 404: PASS (note)
- >vim prints its own "Output is not to a terminal / Input is not from a terminal" and keeps running (vim does the same outside Gezik with stdout to a pipe and stdin /dev/null); Gezik answers, a new command asks "A command is still running. Stop it?" (Cancel / Stop and Run). >cat ends at once (exit 0). ">" + ↓: cat, vim, sleep 30, ls nothing-here, echo "$GEZIK_FILES", ls -la (newest first), "> echo secret" not listed; Return writes ">ls -la" into the field without running, Return runs it. In x.zip and in Trash: "Commands run in a folder on disk". Note: Go ▸ Show Trash opened System Settings ▸ Full Disk Access by itself; the Trash list is empty without it.

#### 405: PASS (test error, `44e4b45`; the no-Command-Line-Tools part NOT TESTED)
- sudo xcode-select --switch /nonexistent: "xcode-select: error: invalid developer directory '/nonexistent'" (maintainer ran it); DEVELOPER_DIR=/nonexistent /usr/bin/xcode-select -p prints it with exit 0 (git.rs:171 asks exactly that). The CLT were not removed: no-CLT part NOT TESTED.

- Gezik with PATH=/usr/bin:/bin:/usr/sbin:/sbin finds /opt/homebrew/bin/git (2.55.0): repo with a changed README.md and new new.txt: orange dot, grey ?, status "⎇ main · 2 changed". Browsing nav/ and rules/: no git process (3 s sampled).

#### 406: PASS
- echo >> README.md: orange dot within 1.2 s, "⎇ main · 1 changed"; git add: green; commit: mark gone, "⎇ main" (no upstream); checkout -b other: "⎇ other". build/ in .gitignore (after git rm --cached: it had been committed by my setup): build faint. Header ▸ Show Git: Git column (Ignored, Untracked, Modified); its header sorts changes first (the ignored build folder stays on top: folders first); Group By ▸ Git: Modified (1), Untracked (1), Unchanged (2), Ignored (1); ⌘F git:changed: README.md, n2.txt "2 / 5". repo/x.zip/docs: no marks, no Git column.

#### 407: PASS (local bare remote, as agreed; HTTPS/Keychain and ssh-agent parts NOT TESTED)
- right-click README ▸ Git ▸ Stage: green, "Staged", no panel; status bar ⎇ other ▸ Commit…: "Commit to repo", faint line at column 72, Staged: README.md; ⌘↩: panel "git commit -F -" "[other fd47548] …", exit 0, status "⎇ other ↑1"; Push: "To /tmp/gezik-run4/remote.git 34754f3..fd47548 other -> other", ↑1 gone. Commit pushed from a second clone: Fetch: "↓1"; Pull: fast-forward, remote.txt appears. Switch Branch…: list "• other", "main" above the status bar; main: "⎇ main". Note: "Staged:" file name centred under a left label.

#### 408: PASS
- README changed + n2.txt new, both ▸ Git ▸ Discard Changes…: "Discard changes to 2 files? They go to the trash first, so you can undo." Discard: README as committed, n2 gone; ⌘Z: both back with the change. Compare with HEAD: "Compare HEAD - README.md ↔ README.md · 1 change". [tools] diff = "opendiff": FileMerge -left …/gezik-head-30699/HEAD - README.md -right … (quit afterwards, setting restored). pre-commit hook printing a line + exit 1: panel "hook says no: tests not run", "Exit code 1", no commit; next Commit… still has "hook test message". Trash leftovers: README.md and n2.txt copies (Discard).

#### 413: PASS
- [[commands]] Args (as in the item); ⌃⌘P second pane (F3 = fn+F3 did nothing here; the macOS key is ⌃⌘P), r1.txt selected right, l1+l2 left, right-click ▸ Commands ▸ Args: /tmp/gezik-args.txt = l1.txt, l2.txt, right/r1.txt, right (one per line). Pane closed, again from the menu and from the palette ("Args"): nothing written, status "Args — needs the second pane".

#### 414: PASS (phone part NOT TESTED)
- other pane in x.zip/docs: "Args — the other pane shows no folder on disk"; other pane showing search results (*.txt in data): the same; other pane with nothing selected: "Args — select the items in the other pane". (A folder selected in the active pane: "Args does not run on folders".) Window widened to 1400 pt for this: at 900 pt neither pane shows its search field.

#### 415: PASS
- "Resize half" menu = "Images/Resize", "Rotate it" menu = "Images" + shortcut ctrl+alt+r. Right-click ▸ Commands: Args, greyed "Images", Rotate it, greyed "Images ▸ Resize", Resize half. Menu bar Commands: "Images", "Rotate it ⌃⌥R" (only the keyed one).

#### 423: FAIL → fixed 02a1619 (red part PASS)
- rules where duration>3s red bold, then kind audio blue: long.aiff (4.2 s) and song.m4a (afconvert of it) red bold; notes.txt, sub theme colour; but short.aiff not blue: kind = "audio" alone colours song.m4a only — Kind::of has no aif/aiff (nor m4b, oga), while preview/Convert treat them as sound. 80 m4a (every other one > 3 s), scroll: new rows come in the theme's colour, then red or blue, never blue then red (frames sampled).

#### 424: PASS
- 3000 files, no where rule: CPU 0.0 % right after the list. where rule back (3000 real 4.2 s m4a): 0.3 s CPU time in all, 1.2 % then 0.0 %: only rows on screen read. (Note: my first try put the where rule after kind audio, so the rows were blue: first match wins, as documented.)

#### 425: PASS
- where = "duration>3x" green: status "settings.toml: highlight-rules[3]: where: duration>3x: "3x" is not a duration (3m, 1:30)"; the others still colour.

#### Setup for 426–431
- measure builds from fix/macos-run4 at ac692c2 (old: 0.1.0-rc.3 binary, make-app version 0.1.0-rc.2; new: workspace version bumped to 0.1.0-rc.99, not committed), /tmp/feed with releases.json; ~/Applications/Gezik.app (removed afterwards). /tmp/gezik-cfg's state.toml already had [update] checked from the release builds, so its [update] lines were removed first (as 428 says).

#### 426: PASS
- ~10 s: "Gezik 0.1.0-rc.99 is ready — installs when you close it · Restart now · What's new"; update/0.1.0-rc.99/ x/Gezik.app + ready (+ SHA256SUMS.txt). ✕, then TERM (⌘Q would install, 427) and start: no note; state.toml [update] checked, told = "0.1.0-rc.99".

#### 427: PASS
- ⌘Q: ~/Applications/Gezik.app --version 0.1.0-rc.99, Gezik.app.old 0.1.0-rc.3 (adhoc). Opened through LaunchServices (open --env): "Updated to 0.1.0-rc.99 · What's new"; .old and update/0.1.0-rc.99 gone.

#### 428: PASS
- old app back, [update] removed: Restart now: one new gezik (0.1.0-rc.99) with the same tabs (drop, data), one process. With [files] max-speed = "50 MB" and a 3 GB copy running: "1 operation is running — Quitting cancels them…" (Keep open / Cancel them and quit); Keep open, copy cancelled, no half file.

#### 429: PASS
- from a UDZO image of the old app: only "Gezik 0.1.0-rc.99 is out · Download", update/ empty. From ~/Downloads/GezikUpdTest with com.apple.quarantine: Gatekeeper "Not Opened", maintainer's Open Anyway (it started without the env, so with the real config: ~/Library/Application Support/gezik made, quit, removed), then open --env: translocated, only the "is out" note, update/ empty.

#### 430: PASS (note)
- one byte flipped: no note, update/0.1.0-rc.99 holds only SHA256SUMS.txt, stderr "gezik: update check: gezik-0.1.0-rc.99-macos-universal.app.tar.gz does not match its SHA-256". Right sum, empty gezik in the tarball: no note, "the downloaded Gezik does not start here or is not 0.1.0-rc.99". Note: the unpacked x/Gezik.app stays in update/0.1.0-rc.99 (no ready).

#### 431: PASS
- Settings search "update": Check (on), Channel (auto | stable | pre), Check for Updates key row, Updates "Gezik 0.1.0-rc.3. Not checked yet" → "Up to date … Last check: 2026-10-15 20:22". Help ▸ Check for Updates with an empty feed: "Gezik is up to date"; palette Check for Updates with the rc.99 feed: the ready note. [update] check = false: nothing at start (no update/ folder after 15 s); Help ▸ Check for Updates still fetched it.

#### 443: PASS
- a.txt ▸ Create hard link: "Link to a.txt" beside it, selected; ls -li: same inode 41605826, link count 2; a write through the link shows in a.txt.

#### 444: PASS (test error, checklist fixed)
- ⌘Z: the link goes to the Trash, a.txt keeps its content; ⌘⇧Z brings it back; ⌘Z again. But a.txt's link count stays 2 (ls -l and Get Info "Links: 2"): the trashed link is the same inode. (A first ⌘Z said "Nothing to undo": that link had been made by the Gezik before a restart.)

#### 445: FAIL → fixed
- a.txt + folder ▸ Create hard link: nothing made, the operations panel "1 item failed", Details "dir: Hard links work only on files" (one message, before any link). With Folders First off (a.txt first): "a.txt: Hard links work only on files" — the refusal named the first selected item, not the folder.

#### 446: PASS
- Get Info on a.txt with the link: "Links: 2" after Where; a folder: no Links line. (After the undo it still says 2, see 444.)

#### 447: PASS
- twice: "Link to a.txt", "Link to a (2).txt".

#### 448: PASS (SMB NOT TESTED)
- HFS+ GezikHedef: link made (inode 36, count 2); exFAT image: "e.txt: Operation not supported" with Retry. Images removed.

#### 453: PASS
- data/cmd pinned (right-click ▸ Pin to sidebar); pin ▸ Color ▸ Red: red pin icon, settings.toml { path = "…/cmd", color = "red" }; selected: accent colour; deselected: red.

#### 454: PASS
- Icon ▸ Star: star, icon = "star". Icon ▸ Default + Color ▸ None: back to plain "…/cmd".

#### 455: PASS
- 24 pins, one per icon, dark and light, 2×: all centred, none clipped, same stroke as the sidebar's own icons (screenshots ic-dark*.png, ic-light*.png).

#### 456: PASS
- "alias" here is the pin's name (Rename…), not a Finder alias: name = "Tarihler" shows the alias icon (folder with arrow) in the pin's colour; Icon ▸ Home replaces it; Default brings the alias icon back.

#### 457: PASS
- color = "teal", icon = "rocket": status "settings.toml: pinned: unknown color "teal" for …/cmd; expected a tag color or #rrggbb (+1 more)", default pin icon in the theme colour; after Move up on another pin both values still in the file; color = "#2e7d32" draws green.

#### 458: PASS
- Color ▸ "• None" / "• Red", Icon ▸ "• Default" (also for the invalid "rocket"); a custom #rrggbb marks none; submenus open with ↓ and →, Return chooses, Esc closes.

#### 463: PASS
- right-click ▸ Tools ▸ Set Dates…: "Set Dates of f1.txt", Modified on, Created, Accessed, Date and time / Now / Date taken (EXIF) / From file…, the file's own 2026-10-15 12:57:17; Esc: stat unchanged.

#### 464: PASS
- f1–f3 + d1, Created ticked, 2020-01-02 03:04:05, Set: stat B and M 2020-01-02 03:04:05 for all four (mdls kMDItemFSCreationDate 2020-01-02 00:04:05 +0000 = 03:04:05 TRT); 2020-02-30 keeps the dialog with "Enter the date as YYYY-MM-DD and the time as HH:MM:SS".

#### 465: PASS
- the empty list's menu said "Undo Set dates of 4 items" (the menu bar's Edit ▸ Undo shows only "Undo", see 393); ⌘Z: all four as before (B, M, A, diff of stat).

#### 466: PASS
- Now with only Accessed: A = 19:33:51 (the click), M unchanged.

#### 467: PASS
- photo.jpg + p.heic (EXIF 2024:07:01 09:30:00, sips HEIC; no real iPhone photo) + note.txt, Date taken, Modified + Created: photos B/M 2024-07-01 09:30:00 local; panel "Done · 1 item skipped", Details "Skipped: note.txt: No date taken".

#### 468: PASS
- From file… "nope.txt": red "No such file: nope.txt"; "ref.txt" (relative): shows /tmp/gezik-run4/data/exif/ref.txt under the buttons; with M, C, A ticked: p2.txt takes ref's 2015-05-06 07:08:09 for all three. (With only Modified ticked, Created moved too: macOS pulls the birth time back when the modified time goes before it.)

#### 469: PASS
- Get Info on p2.txt, click on the Modified line: Set Dates of p2.txt with its date; 2021-03-04, Set: Get Info shows Modified 2021-03-04 at once; ⌘Z, Get Info again: 2015-05-06.

#### 470: PASS
- ok.txt, locked.txt (chflags uchg), root.txt (made by the maintainer with sudo): ok.txt changed; "2 items failed", Details "locked.txt: Access denied", "root.txt: Access denied". Lock removed; root.txt removed by the maintainer.

#### 473: PASS
- move.txt dragged and rested on "one": opens after ~0.7 s, "Move to one"; dropped on empty space: moved into one, the pane stays there.

#### 474: PASS
- two springs (two, then inner), Esc before release: back to sp in one step; the release after Esc did nothing (files unchanged).

#### 475: PASS
- moving across rows never resting 0.7 s: none opened; wiggling inside one row (5 moves of 20 px in 0.5 s, then rest): it opened (measured from entering).

#### 476: PASS
- pinned folder (three) opens; a tree folder (Home ▸ Work) opens; a tag row (Red) does not ("Tag 1 item Red").

#### 477: PASS
- columns view (⌃⌘3): two.txt from the second column rested on "one" in the first: opens; grid (⌘⇧2): the "three" cell opens.

#### 478: PASS (Finder window on /tmp/gezik-run4/drop instead of the Desktop)
- spring into one, drag out to Finder, drop: copied there (moved: same volume), the Gezik pane back in sp. Again, back into the window without releasing, drop on a folder: the pane stays where it sprang, the file in three.

#### 479: PASS
- fromfinder.txt from a Finder window rested on "one": opens; drop: moved there (same volume), pane stays. Again, spring, back to Finder, release: the pane went back to sp, the file stayed in Finder.

#### 480: PASS
- measured from the pointer coming to rest on "one" (window title polled): 700 → 0.74 s, 2000 → 2.05 s, 1500 → 1.48 s, 9000 → 2.99 s (3000), 50 → 0.16 s after the glide's end (the timer starts as the pointer enters the row ~0.08 s before: ≈ 200 ms); 0: no folder opens in 3.3 s, a tab (cmd) still opens on rest; -1: status "settings.toml: drag.spring-delay: expected 0 (off) or a number of milliseconds, got -1", opens at 0.73 s (700). Settings ▸ General ▸ Spring delay row not used (the file was).

#### 481: FAIL → fixed b0b2f42
- a folder rested on itself does not open. A folder in a mounted UDRO image (GZRO4) in the second pane: opened, label "Copy to inside" (a drop would fail). After the fix: no spring, no label there; one still springs.

#### 482: PASS
- three (3,002 entries) with aaa and zzsub: resting at the row where zzsub lands, three opened, then zzsub opened without moving the mouse.

#### 483: PASS
- View ▸ Stack Panes: second pane under the first, full width, ✓ (AX mark), stacked = true; again: side by side, false, no mark.

#### 484: PASS (test error, checklist fixed)
- splitter at the line between: up-down arrow (screenshot with cursor), drag moves it; it stops where the lower pane keeps only its tabs, address bar and header (~108 px at 632 pt: the share is clamped to 20–80 % and each pane's own minimum), not 160 px; double-click: middle. Dragged to 0.358, unstacked: left pane ≈ 245 / 690 px; restart: state.toml [panes] split = 0.358 kept.

#### 485: PASS (test error, checklist fixed)
- stacked, Tab moves between the panes; F5 "Copy "docs" to /tmp/gezik-run4/drop?", F6 "Move …"; ⌘U does nothing — Swap Panes is ⌃⌘U on macOS (menu AX modifiers 4; template "mod+ctrl+u"); ⌃⌘U swaps.

#### 486: PASS
- window at its minimum 638 pt, stacked: second pane opens; side by side: status "The window is too narrow for two panes".

#### 487: PASS (note)
- Show Preview + Preview at Bottom: preview under the list, sidebar full height, ✓, position = "bottom"; the line above it drags (hover found at y=580), height kept after quit (state.toml [preview] height = 300). Note: dragged up far the preview takes all the room and no list row is left (the list has no minimum height).

#### 488: PASS (note)
- side by side and stacked: preview under both panes, full width; at 690 pt (two panes' minimum) the preview stays. Note: stacked with a 300 px preview the top pane shows no rows.

#### 489: PASS
- Preview at Bottom again: right, width 280.

#### 490: PASS
- Settings search "stacked" ▸ Stacked ticked: panes stacked at once, menu ✓; "preview.position" ▸ bottom: bottom at once, menu ✓.

#### 491: PASS
- palette "Stack Panes" and "Preview at Bottom" (Action, no key shown) toggle like the menu.

#### 492: PASS
- ⌘N second window with two panes; Stack Panes in one: both windows stacked.

#### 409–412 [16g]: NOT TESTED
- No Android phone. The libmtp package itself was built and loaded (A3).

#### 432 [update]: NOT TESTED
- No release newer than `v0.1.0-rc.3` is published.

#### 433–436 [16h]: NOT TESTED
- No other Mac or NAS announcing SMB over Bonjour on this network.

#### 493 [17]: PASS
- Settings ▸ Cloud Sync `Off` with Start Syncing… / Join With a Code…. Start Syncing…: `Synced just now`, the code row `Y09KY-…-DZCC9` with Sync Code ▸ Copy (the clipboard held the code). `/tmp/gezik-cfg` got `cloud/settings.toml`, `cloud-base`, `cloud-sync.toml`, and `location.toml` says `shared = "{local}/cloud"`.

#### 494 [17]: PASS
- `cloud-sync.toml` is `-rw-------`.

#### 495 [17]: PASS (second Gezik on this Mac)
- The second Gezik (`/tmp/gezik-cfg-b`): Join With a Code…, the code lower case without dashes: its theme became the first one's (`light`), bottom line `Your previous settings were backed up. Restore Backup ▸ brings them back.`, `backups/gezik-settings-2026-10-15-204432-348.zip`. One wrong character: `Not synced: This code has a typo`. The second step (another valid code) is covered by 502's "No settings found for this code".
- Note: right after joining the Cloud Sync row said `Not synced since Gezik started`.

#### 496 [17]: PASS
- Theme changed on the first, about 9 s later the second had it; density changed on the second, Sync Settings Now on the first (palette, `Settings synced`): it had `compact`. Neither change lost.

#### 497 [17]: PASS
- Theme `light` on the first, `auto` on the second, Sync Now on both: the second kept `auto` and got `cloud/settings.cloud-conflict-2026-10-15.toml`; Sync and Backup: `Conflicting copy: settings.cloud-conflict-2026-10-15.toml — Not reviewed` ▸ Review…: a `theme` row (This copy: auto — other copy: light); Other copy, Merge: `theme = "light"`, the copy gone.

#### 498 [17]: PASS (server made unreachable instead of Wi-Fi off)
- Wi-Fi was left on (the run needs the network); `cloud-sync.toml`'s `server` was set to `https://sync.invalid`. Sync Now: `Not synced — offline`, Gezik works on; started like that, its window was up in 0.30 s. Server put back, Sync Now: synced.

#### 499 [17]: PASS
- Density changed and ⌘Q at once: quitting took 0.72 s; Sync Now on the second: it had the change.

#### 500 [17]: PASS
- Palette "Sync Settings Now": `Settings synced`; "Show Sync Code": Sync and Backup with the code row. Neither has a key.

#### 501 [17]: PASS
- While syncing: `Keep Settings in a Folder — Off — Stop Cloud Sync first`. After Stop Syncing, a folder (`/tmp/gezik-run4/syncdir`) set: `Cloud Sync — Off — Stop keeping settings in a folder first`.

#### 502 [17]: PASS
- Stop Syncing on the second: `cloud/`, `cloud-base`, `cloud-sync.toml`, `location.toml` gone, `settings.toml` back (`Settings are kept on this computer again; the cloud copy is as it was`). Delete from Server… ▸ Delete on the first: `The cloud copy is deleted; settings are kept on this computer`. On the second, Join With a Code… with the old code: `No settings found for this code`.
- Note: the first kept a `settings.local.toml` (this computer's own values) after Delete.

### Fixes on `fix/macos-run4` and re-test (2026-10-15)

Branch `fix/macos-run4` from `master` at `9e95032`, one commit per fix, each naming its item. Re-tested with a release build of the branch at `b0b2f42`, the same way as above.

| Item | Before | After | Commit / note |
|---|---|---|---|
| A3 | FAIL | PASS (build); release not published | `9afaec6`. A pkg-config stand-in that only passes the version check; the tarball's Linux `gphoto2-endian.h` removed before building; the architecture and link checks read `lipo -archs` and `otool -L` correctly. Re-test: the package above, universal, signed, loads. Publishing `libmtp-1.1.23-1` and filling `tools.rs` need a `gh` login. |
| 396 | FAIL | PASS | `3f74d65`. The rename preview's "Preview" and "Sort by" labels elide, so "Only changed" and "Reset to rules" stay inside the layer. Re-test: pseudo at 900 pt, both whole; English unchanged. |
| 402 | FAIL | PASS | `1686bbb`. Keep a Copy in Downloads takes the plain name while it is free (new test `a_kept_copy_takes_its_own_name_while_it_is_free`). Re-test: `~/Downloads/notes.txt` ("Notes v5"); removed afterwards. |
| 423 | FAIL | PASS | `02a1619`. `aif`, `aiff`, `oga`, `m4b` are sound (`Kind::Audio`). Re-test: `short.aiff` and `long.aiff` blue with `kind = "audio"`. |
| 445 | FAIL | PASS | `ac692c2`. The refusal names the folder (or the item on another drive). Re-test with a.txt first: `zdir: Hard links work only on files`. |
| 481 | FAIL | PASS | `b0b2f42`. A drop target on a volume mounted read-only (statvfs `ST_RDONLY`, Windows' read-only volume flag) is refused, so it does not spring. Re-test: the UDRO image's folder neither springs nor offers a copy; a writable folder still springs. |
| 375 | PASS | PASS (test error) | `ea00c02`. Checklist: move `Gezik.app` itself in Finder. |
| 393 | PASS | PASS (test error) | `9cdac27`. Checklist: the job name is in the right-click Undo; a broken settings.toml is no test of the pseudo language. |
| 395 | PASS | PASS (test error) | `cf79585`. Checklist: compare `令`/`直`; Hiragino Sans GB stands in for PingFang SC. |
| 405 | PASS | PASS (test error) | `44e4b45`. Checklist: neither `xcode-select --switch /nonexistent` nor `DEVELOPER_DIR` stands in for a Mac without the Command Line Tools. |
| 444, 446, 484, 485 | PASS | PASS (test error) | `e9431d1`. Checklists: a trashed hard link still counts; the stacked splitter keeps a fifth of the room; Swap Panes is ⌃⌘U. |

After the fixes: **88 PASS, 0 FAIL, 13 NOT TESTED** (101 items); the libmtp release itself is still to be published.

On the branch: `cargo test --workspace --no-fail-fast` 3119 passed, 0 failed, 25 ignored (the two new tests); clippy `-D warnings` and `fmt --check` clean.

### Clean-up

- Every Gezik of the run quit with ⌘Q or TERM; no `gezik*.ips` in `~/Library/Logs/DiagnosticReports`.
- Removed: `/tmp/gezik-run4/data` and the other test folders under `/tmp/gezik-run4` (logs, screenshots and the libmtp package stay there), `/tmp/gezik-cfg-b`, `/tmp/old`, `/tmp/new`, `/tmp/feed`, `~/Applications/Gezik.app`(`.old`), `~/Downloads/GezikPortable*`, `~/Downloads/GezikUpdTest`, the disk images (GZEXFAT4, GZRO4, GezikRC2), `big.bin` on GezikHedef, `/tmp/gezik-args.txt`, `~/.viminfo` (made by 404's `vim`). LaunchServices entries of the test apps removed with `lsregister -u`. `/tmp/gezik-cfg` is kept (downloaded tools: none this run; Run 3's are in `/tmp/gezik-cfg-run3`).
- Undone: per-app language (`defaults delete com.wenlar.gezik AppleLanguages`), `AppleLanguages` back to `en-TR, tr-TR`, the locked file unlocked, `root.txt` removed by the maintainer; `xcode-select -p` is `/Applications/Xcode.app/Contents/Developer` as before.
- Fingerprint after: `~/Library/Application Support/gezik` absent (it was made twice by Gatekeeper's Open Anyway starting a test app without `GEZIK_CONFIG_DIR`, and removed each time); `~/.local/bin` absent; `mdfind 'kMDItemUserTags == "*"' | wc -l` 0; Desktop the same; disk images the same as before (four simulators and GezikHedef); the home folder has the same entries.
- Left in the Trash (this Mac gives no access to it from the shell): `x.zip` (400), the old copies of `y.zip` and `w.zip` (401, 402), `README.md` and `n2.txt` (408), `Link to a.txt` (444) and the files the Get Info / Discard tests put there.
