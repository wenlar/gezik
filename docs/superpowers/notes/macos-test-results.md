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
