# Gezik on macOS: build and manual test

Run 1 (2026-10-06, `feat/batch-ops-5c`) covered items 1-22. Its fixes are on master: the `fix/macos` branch, and the shared UI fixes from `fix/shared-ui`. Sub-projects 5c (convert, user commands, ffmpeg), 5d (PDF, pdfium) and 6a (keyboard: filter, selection, tabs; items 47-51) have never run on a Mac.

Part 9a1 (system icons, Quick Look thumbnails, Finder names, aliases, packages; items 85-93) has never run on a Mac. Run it on `feat/system-9a1` (after the merge: `master`) and write a new section **"Run: 9a1"** in `macos-test-results.md`. Start with the probe, one command that tries every system call 9a1 relies on and prints PASS/FAIL per line (item 85):

```sh
cargo run --release -p gezik-platform --example mac_probe -- ~/Desktop/some.pdf ~/Pictures/photo.heic ~/Movies/clip.mov ~/Documents/report.pages
```

Paste its whole output into the results, then run `cargo test -p gezik-platform` (its macOS-only tests for icons, Finder names and aliases run only there).

Part 9a2 (Open With, Share, Quick Actions, the system Quick Look panel; items 94-100) has never run on a Mac. Run it on `feat/system-9a2` (after the merge: `master`) and write a new section **"Run: 9a2"** in `macos-test-results.md`. Start with the probe (item 94), then run `cargo test -p gezik-platform` (its macOS-only tests `text_files_open_in_textedit` and `types_and_conformance` run only there). Items 94-100 follow 9a1's numbers; part 9b1 also numbers its items from 85 on its own branch, so the branch merged second renumbers.

Part 9a3 (the Info window; items 101-109) has never run on a Mac. Run it on `feat/system-9a3` (after the merge: `master`) and write a new section **"Run: 9a3"** in `macos-test-results.md`. Start with the probe (item 101). Items 101-109 follow 9a2's numbers; parts 9b1, 9b2 and 9b3 number their items on their own branches, so the branch merged later renumbers.

This file is for a person, or Claude Code on the Mac, testing the state after 5d. Write the results into `docs/superpowers/notes/macos-test-results.md`, in a new section **"Run 2 (after 5d)"** below Run 1. Don't change Run 1.
- Give PASS, FAIL or NOT TESTED for every item.
- For a FAIL, give the steps, what you expected, what happened, and a screenshot path. Don't commit screenshots.
- Start the section with the date, the branch and short SHA, the machine, the macOS version and the keyboard layout.
- Add a summary table like Run 1's.

## Build

```sh
xcode-select --install            # C/C++ compiler (UnRAR is C++); skip if already installed
curl https://sh.rustup.rs -sSf | sh   # Rust; then open a new terminal
git clone git@github.com:wenlar/gezik.git && cd gezik   # or: git fetch in the existing clone
git checkout feat/batch-ops-5d    # until it is merged; after that: git checkout master && git pull
                                  # part 9a1: git checkout feat/system-9a1 (until it is merged)
                                  # part 9a2: git checkout feat/system-9a2 (until it is merged)
                                  # part 9a3: git checkout feat/system-9a3 (until it is merged)
git log -1 --oneline              # write this SHA into the results
cargo build --release -p gezik
rm -rf /tmp/gezik-cfg && mkdir -p /tmp/gezik-cfg /tmp/gezik-test
GEZIK_CONFIG_DIR=/tmp/gezik-cfg ./target/release/gezik /tmp/gezik-test
```

- Start from an empty `/tmp/gezik-cfg`, so that the 7-Zip, ffmpeg and pdfium download offers appear. Gezik downloads them into `/tmp/gezik-cfg/tools/`.
- Hide any ffmpeg on PATH while testing the ffmpeg download. Homebrew's is at `/opt/homebrew/bin/ffmpeg`. Start Gezik with a PATH that leaves it out:
  ```sh
  PATH=/usr/bin:/bin:/usr/sbin:/sbin GEZIK_CONFIG_DIR=/tmp/gezik-cfg ./target/release/gezik /tmp/gezik-test
  ```
- Run the automated tests once with `cargo test --workspace`, and report any failures with their output.
- After the pdfium download (item 42), also run the real-pdfium tests:
  ```sh
  GEZIK_TEST_PDFIUM=/tmp/gezik-cfg/tools/pdfium-8086/libpdfium.dylib \
      cargo test -p gezik-batch --test pdf_worker --test pdf_tasks
  ```
  Use the dylib's real path from `ls /tmp/gezik-cfg/tools/`.

### Test data

Put a few files and folders in `/tmp/gezik-test`. Also make:
- **Archives** with `zip`, `tar czf`, or 7-Zip if it is installed. A RAR file is in `crates/gezik-batch/tests/data/` (see SOURCES.md there).
- **A second volume** for cross-volume copies, because copies on the same APFS volume are clones and finish at once:
  ```sh
  hdiutil create -size 4g -fs HFS+ -volname GezikHedef -type SPARSE /tmp/hedef && hdiutil attach /tmp/hedef.sparseimage
  ```
- **Pictures:**
  - A few JPEGs from the camera or the web, one of them with EXIF orientation 6 (a portrait phone photo).
  - A PNG with transparency.
  - **A real iPhone HEIC** (AirDrop one from a phone; these are tiled, in Display P3). If there is no iPhone, use `sips -s format heic photo.jpg --out photo.heic`, and say in the results that it was made with sips.
- **Text** in Windows-1254: `printf 'ılık İstanbul\r\n' | iconv -f UTF-8 -t WINDOWS-1254 > tr1254.txt`.
- **Audio and video:** any .mov from the screen recorder (⇧⌘5) or the phone, and an .m4a or .mp3.
- **PDFs:**
  - Print any page to PDF from Safari, a few times.
  - Make one with 200+ pages (for example, join prints in Preview), for cancel and memory.
  - Make an encrypted one: in Preview, File ▸ Export…, then "Encrypt" with a password.

## Checklist

Items 1-22 are Run 1's, updated for what has been fixed since. Items 23-34 re-check the fixes. Items 35-46 are new (5c and 5d).

Basics
1. The window opens. It follows the system light/dark mode, and switching the mode while Gezik runs changes the theme.
2. Navigation:
   - Double-click a folder; ⌘↓ opens, ⌘[ goes back, ⌘] goes forward and ⌘↑ goes up.
   - ⌘T opens a new tab and ⌘W closes it.
   - The sidebar shows Home and the drives.
3. Enter renames (Finder style). Escape cancels. A name with `/` shows a red tip.
4. ⌘⇧N creates a new folder and opens it for renaming.
5. ⌘C / ⌘X / ⌘V work inside Gezik. ⌘⌥V pastes as a move.
6. **Finder interop:**
   - Copy a file in Gezik, then paste in Finder (⌘V).
   - Copy a file in Finder, then paste in Gezik.
   - Cut in Gezik shows the item faded.
7. **Trash:**
   - ⌘⌫ moves the item to the Trash and it appears there in Finder.
   - ⌘Z brings it back.
   - ⌘⌥⌫ deletes permanently, after a question.
8. **Drag and drop:**
   - Drag from Gezik to a Finder window: the file is copied, or moved on the same volume.
   - Drag from Finder into Gezik, onto the list and onto a folder row: the row is highlighted and the label says "Move to …". With ⌥ held it says "Copy to …" and copies.
   - Drag between two Gezik tabs.
   - Drag onto a sidebar folder.
9. Space opens quick look. ⌘A selects all. ⌘2 is grid view and ⌘1 is list view.
10. Copy a large file (1-2 GB) to GezikHedef. The operations panel shows progress, and pause, resume and cancel all work.
11. Pasting over existing files opens a conflict list. Replace, Skip and Keep both work, and ⌘Z undoes them.

5a, batch rename
12. Select 3 or more files and press Enter. The "Rename N items" layer opens.
13. Replace, Number and Case update the preview live. Rename applies the names, and ⌘Z undoes them.
    - With Turkish first in System Settings ▸ General ▸ Language & Region, Case gives Turkish i/İ in a normal launch, with no `LC_ALL`.
14. Swap two names (a.txt ↔ b.txt) by typing them, then undo.
15. Presets:
    - Save, reopen the layer, apply the preset, then Delete it.
    - Check that `settings.toml` in /tmp/gezik-cfg has `[[rename-presets]]`.
    - The Presets menu stays fully on screen (see 31).

5b, archives
16. Right-click a zip, then Extract here. A single-root zip goes in as it is; a multi-root zip goes into a `<name>/` folder. ⌘Z removes what was extracted.
17. tar.gz, 7z and rar (from the test data) extract.
18. An AES zip asks for a password. The field shows dots and "Show" reveals the text. A wrong password asks again; the right one extracts.
19. In the Compress… layer, make:
    - a zip;
    - a 7z with a password and "Encrypt file names";
    - a 7z split into parts.
    Also use "Compress to "x.zip"" from the menu, without the layer. Item 32 has the details of the name fixes.
20. Drag files onto a zip row. The label says "Add to x.zip" and the files are added. ⌘Z restores the old archive.
21. **7-Zip download:** open a `.wim` or `.dmg` archive (make a .dmg with `hdiutil create -format UDZO`).
    - The "needs 7-Zip" box appears. Choose Download.
    - The tool lands in `/tmp/gezik-cfg/tools/7zip-26.03/7zz` and runs. Note any Gatekeeper prompt.
    - The archive opens.
22. Cancel a big extraction. No `.gezik-*` folders are left in the target folder (`ls -a`).

Re-checks of the fix/macos fixes (see "Fixes on fix/macos" in the results)

23. **Live refresh through a symlink:** with Gezik on `/tmp/gezik-test`, `touch /tmp/gezik-test/canli.txt` shows up at once.
24. **⌘[ / ⌘] by key place:**
    - On the layout in use, ⌘ + the key right of P goes back, and ⌘ + the next key goes forward. On Turkish-QWERTY-PC these are the keys that type `ğ` and `ü`.
    - Go ▸ Back/Forward in the menu bar work too.
25. **Finder → Gezik drops:**
    - Onto a folder row: the row is highlighted and the file goes into the folder.
    - Onto a zip row: the file is added to the zip.
    - The label follows ⌥ while dragging, and ⌥ copies.
26. **Tab-rest drag:** drag a file onto another tab and rest until the tab opens.
    - Release on the tab: the file moves.
    - Do it again and release over the new list: the file moves.
    - A quick drop before the tab switches also moves it.
    - After a rest, drag out of the window into Finder and drop there. Finder gets the file, and no Gezik ghost is left when the pointer comes back over Gezik.
    - With a mouse that has a right button: right-drag, rest on a tab, release. Only the Copy here / Move here menu appears, with no second context menu after it.
27. **Sidebar drop onto another volume:** dropping a file from `/tmp` onto GezikHedef says "Copy to GezikHedef" and copies.
28. **Language:** with Turkish first in AppleLanguages, Case UPPER previews `İ` when Gezik is started from Terminal and when it is started with no locale variables, as an app from the Dock would be:
    ```sh
    env -u LANG -u LC_ALL -u LC_CTYPE GEZIK_CONFIG_DIR=/tmp/gezik-cfg ./target/release/gezik /tmp/gezik-test
    ```
    A bare binary double-clicked in Finder runs inside Terminal, so that is no test of this.
29. **Menu bar:**
    - There are File, Edit, View, Go and Window menus. The app menu is titled "Gezik", but "About/Hide/Quit gezik" is known (see "Known gaps").
    - ⌘M minimizes.
    - View ▸ as Grid / as List work with the mouse.
    - ⌘C in a name being edited copies text; in the list it copies the file.
    - No crash when menu items are used quickly one after another.
30. **Wording and dotfiles:**
    - The address bar starts with "Computer", and the sidebar section is "LOCATIONS".
    - The Type column shows Finder-style kinds ("Folder", "Plain Text Document").
    - Dotfiles are hidden by default. ⇧⌘. and View ▸ Show Hidden Files show them.
    - Ctrl+H does **not** toggle them on macOS (it is the Windows/Linux default); ⌘H hides the app.

Re-checks of the shared UI fixes (fix/shared-ui, on master)

31. **Menus at the window edge.**
    - Put the window in the middle of the screen, not maximized. Open each of these near the window's bottom-right corner:
      - the context menu on a row at the very bottom right;
      - the Commands ▸ submenu near the right edge;
      - Presets in the rename layer;
      - Preset, From and To in the Convert layer;
      - the View button's menu.
    - Take a **full-screen** screenshot (`screencapture -x ~/Desktop/menu-edge.png`). Don't use a window capture (`-l`, ⇧⌘4 + Space), which cuts everything at the window's frame.
    - On macOS these menus are native and may hang past the window's edge. That is a PASS.
    - It is a FAIL only if items are cut off or off the screen in the full-screen shot, or a list taller than the screen cannot scroll.
    - Run 1's "Presets menu runs past the right edge" and "context menu cut off at the bottom" were probably window captures. Say which they were.
32. **Compress name:**
    - The layer selects only the stem when it opens.
    - With 7z chosen, typing `sifreli` makes `sifreli.7z`.
    - Typing only `.zip` makes `<item name>.zip`, not `.zip.zip`.
    - After a 7z in the layer, the menu still says `Compress to "x.zip"` and makes a plain zip.
    - The layer opens with Split off every time, and `state.toml` keeps no split size afterwards.
33. **⌘ / Ctrl shortcuts:**
    - ⌘Enter starts the rename, Compress and Convert layers, from any field in them.
    - Ctrl+Enter is not needed; note what it does.
    - Esc closes each layer.
    - ⌘Z / ⇧⌘Z undo and redo after each layer's job.
    - Up and Down in a Gezik menu skip greyed lines. (A native macOS menu does this itself; note it if Gezik's menus are native here.)
34. **Archive kinds and icons:**
    - `x.wim` and `big.7z.001`/`.002` show the archive icon, with the types "WIM archive" and "Split 7Z archive".
    - Extract is offered on `.001`, and on a bare `film.001` that is the first part of a 7z set.
    - `clip.264` keeps its own name and isn't taken as an archive part.

5c, convert and user commands (never run on a Mac)

35. **Pictures:** right-click a JPEG, then Convert…
    - The layer opens. The right-click menu also has Commands ▸, and a command whose program is missing is grey.
    - Preset "Resize photos (JPEG 1920 px)" with output "Subfolder", then ⌘Enter. The portrait photo with EXIF orientation 6 comes out upright.
    - With "Remove metadata (incl. location)" on, `mdls -name kMDItemLatitude out.jpg` is empty. With it off, JPEG → JPEG keeps EXIF and location.
    - PNG with transparency → JPEG has a white background.
    - "Replace originals": the originals go to the Trash, and ⌘Z brings them back.
    - "Remove location data" on a phone JPEG drops the GPS tags. The picture isn't re-encoded: the size changes by only a few KB.
36. **Text:**
    - `tr1254.txt`: the layer says "Detected: Windows-1254". Converting to UTF-8 gives `ılık İstanbul` in TextEdit.
    - With "Replace originals", ⌘Z gives the old bytes back (`xxd`).
    - A UTF-8 file with `ı` → Windows-1252 shows an error line ("can't encode 'ı' in Windows-1252 (line N)"), and the file is unchanged.
37. **HEIC and the ffmpeg download (Gatekeeper, quarantine).** Start Gezik with ffmpeg off PATH.
    - Convert the HEIC to JPEG. The "ffmpeg needed" box appears. "Where does it come from?" opens the gezik-tools release page.
    - Choose Download. It lands in `/tmp/gezik-cfg/tools/ffmpeg-9.0.2/` (`ffmpeg`, `ffprobe`, LICENSE, SOURCE.txt), and the conversion continues by itself.
    - Record what these say about the downloaded `ffmpeg`:
      - `file` (arm64 or x86_64?);
      - `xattr -l` (is there a `com.apple.quarantine`?);
      - `codesign -dv`;
      - `spctl -a -vv`.
    - Note any Gatekeeper prompt.
    - **Quarantine test:** run `xattr -w com.apple.quarantine "0081;$(printf %x $(date +%s));Safari;" /tmp/gezik-cfg/tools/ffmpeg-9.0.2/ffmpeg`, restart Gezik and convert again.
      - Record whether it runs, Gatekeeper blocks it, or Gezik shows an error, and the exact text.
      - Then `xattr -d com.apple.quarantine …` and confirm that it works again.
    - **The HEIC result:**
      - right size and upright;
      - colours compared with Preview (a Display P3 → sRGB shift is a known limit);
      - a tiled iPhone HEIC must not come out as one 512 px tile.
    - Also try "Convert to WebP" with WebP (lossy), and "Convert to AVIF": both go through ffmpeg.
38. **Audio and video:**
    - MP3 and M4A (AAC) from the .mov, "Smaller video", "Remux to MP4" and "GIF from video".
    - Progress moves smoothly, and the outputs play in QuickTime.
    - Cancel a "Smaller video" halfway. No half file is left, and `pgrep -l ffmpeg` is empty.
39. **Pause and resume:** pause a long "Smaller video".
    - `pgrep -l ffmpeg` shows that ffmpeg has ended.
    - Resume starts that item again, and the output is complete and plays.
    - Do the same with a user command from 40.
40. **User commands.** Add these to `/tmp/gezik-cfg/settings.toml`. Gezik picks the change up live; check that it does.
    ```toml
    [[commands]]
    name = "Half size (sips)"
    run = ["sips", "-Z", "800", "{in}", "--out", "{out}"]
    output = "{name}-small.{ext}"
    types = ["jpg", "jpeg", "png"]
    parallel = 4

    [[commands]]
    name = "Thumbnail (qlmanage, outdir)"
    run = ["qlmanage", "-t", "-s", "256", "-o", "{outdir}", "{in}"]
    output = "{name}.{ext}.png"

    [[commands]]
    name = "Rotate in place (sips)"
    run = ["sips", "-r", "90", "{in}"]
    types = ["jpg", "png"]

    [[commands]]
    name = "Missing program"
    run = ["no-such-tool", "{in}"]
    ```
    - Commands ▸ in the right-click menu, and Commands in the Convert layer, list the ones for the file's type. "Missing program" is grey.
    - The `{out}` and `{outdir}` commands put their output next to the input, under the `output` name.
      - While a `{outdir}` command runs, nothing half-made shows in the folder; it runs in Gezik's staging folder.
      - A second run opens the conflict list.
    - "Rotate in place": ⌘Z gives the original back, from the copy Gezik put in the Trash.
    - A name with spaces and quotes (`it's a "test".jpg`) works.
    - An invalid command (a typo in a key) is reported and left out.

5d, PDF (never run on a Mac)

41. **Images to PDF** (needs no pdfium):
    - Select 3 JPEGs (one with orientation 6), a PNG with transparency and a HEIC. Right-click, then "Images to PDF…".
    - The HEIC is left out with a note.
    - Change the order by dragging in the list. Choose A4 and margin Small, then ⌘Enter.
    - Preview shows the pages in that order, with the portrait one upright.
42. **pdfium download (does the dylib load?)**
    - Select 2 PDFs, Convert…, then "Merge PDFs". The "pdfium needed" box appears. Choose Download.
    - It lands in `/tmp/gezik-cfg/tools/pdfium-8086/` (`libpdfium.dylib`, LICENSE, licenses/, SOURCE.txt). The merge continues by itself, and the result opens in Preview.
    - Record what `file`, `xattr -l`, `codesign -dv` and `spctl -a -vv -t open --context context:primary-signature` say about the dylib.
    - **Quarantine test:**
      - Add `com.apple.quarantine` to the dylib as in 37. Quit Gezik, start it again, and run a Split.
      - Record whether it loads, or whether the row says "pdfium does not load on this system: …" (with the text).
      - Remove the attribute and check that it loads again.
    - **Note:** a `cargo build` binary has no hardened runtime, so library validation doesn't apply. Whether a signed and notarized Gezik.app can load this dylib is still open. Write down what `codesign -dv --verbose=4 target/release/gezik` shows (flags, TeamIdentifier).
    - Run the `GEZIK_TEST_PDFIUM` tests from "Build".
43. **Split, extract, to images:**
    - Split PDF, Each page: the layer says "Makes N files" and makes N files.
    - Ranges `1-2, 5`: one file per range.
    - A range past the end shows the error live, before you start.
    - Extract pages to a Turkish file name (`çıktı ğüşö.pdf`).
    - PDF to images, PNG at 150 dpi and JPEG at 72 dpi: the page count and sizes are right, and the text is sharp.
    - Running a job again opens the conflict list. ⌘Z removes a job's outputs in one step.
44. **Encrypted PDF:**
    - Split or merge the encrypted PDF. The password box shows dots, and Show reveals the text.
    - A wrong password asks again. The right one works.
    - The output is not encrypted, and the layer said so.
    - In a merge, choosing Skip at the password leaves the whole merge out.
45. **No worker left behind:**
    - After every PDF job above, `pgrep -fl -- --pdf-worker` is empty.
    - Start "PDF to images" (PNG, 300 dpi) on the 200+ page PDF. Pause, resume, then Cancel halfway. No worker is left, and no half files or `.gezik-*` folders remain.
    - Afterwards, `lsof -p $(pgrep -x gezik) | grep -i pdfium` is empty: the library is only loaded in the worker.
    - Write down Gezik's RSS (`ps -o rss= -p $(pgrep -x gezik)`) before and after.
46. **Last choices and keys:**
    - Close and reopen the Convert layer on a PDF and on a picture. Each kind comes back with its own last choices; the range text does not come back.
    - Esc closes the layer and ⌘Enter starts it.
    - `state.toml` has `[convert]`.

6a, keyboard (filter, selection, tabs; never run on a Mac)

47. **Filter:**
    - ⌘F opens a bar above the list with the field focused. `jpg` leaves only names with `jpg`, and the counter says shown / all ("2 / 8").
    - ↓ gives the list the keyboard and the bar stays. Enter on a filtered folder opens it.
    - Esc on the list closes the filter, and a second Esc clears the selection. Esc in the field closes it too.
    - `/` on the list opens it as well.
    - A tab switch keeps each tab's filter. Going to another folder (or Back) opens it without one.
    - With a file named `İSTANBUL.txt`, `istanbul` finds it, and so does `ılık` for `ILIK.doc`.
48. **Pattern box and selection keys:**
    - ⌘= and ⌘- open "Select by pattern" / "Deselect by pattern", starting with the last pattern. "N items match" follows the text, and a bad pattern (`!` alone) is said in red.
    - ⌘⇧I inverts the selection.
    - With a numeric keypad (an external keyboard): keypad + / - open the box, and keypad / brings back the selection of the last delete (Delete, ⌘Z, keypad /). ⌥+keypad + on a `.jpg` adds every `.jpg`.
    - In the menu bar, Edit ▸ Filter…, Select by Pattern…, Deselect by Pattern…, Invert Selection, Select Same Type and Restore Selection work with the mouse, and show ⌘F / ⌘= / ⌘- / ⌘⇧I. Window ▸ Reopen Closed Tab shows ⌘⇧T.
    - On a layout where `=` needs Shift (Turkish Q), write down what ⌘= does, and whether `select-pattern = ["num+", "mod+shift+0"]` under `[shortcuts]` makes it work.
49. **Tabs:**
    - With four tabs, ⌘1…⌘4 switch, ⌘7 does nothing, and ⌘9 shows the last.
    - ⌘⇧1 shows the list and ⌘⇧2 the grid, from the keys and from View ▸ as List / as Grid (which show ⌘⇧1 / ⌘⇧2). Also on Turkish Q.
    - Go two folders deep in a tab, filter it, ⌘W, then ⌘⇧T: the tab comes back in its place with its filter, and ⌘[ goes to the folder before.
    - ⌘⇧A opens the tab picker. Typing filters it, ↑/↓ and Enter switch, and Esc closes it.
    - Right-click a tab, then Lock tab: a lock shows and the × goes. ⌘W leaves it open and the status bar says so. Unlock tab.
50. **Typing mode:** add `[keyboard]` `typing = "filter"` to `/tmp/gezik-cfg/settings.toml`. Gezik picks it up live, and a letter on the list opens the filter with it. Back to `"jump"`, a letter jumps to a name again.
51. **Saved filters:** ▾ in the bar, then Save as… "Resimler". `settings.toml` gets a `[[filters]]` entry. ▾ then Resimler fills the bar, and ▾, Delete "Resimler" removes it.

6b, keyboard (path completion, folder history, command keys; never run on a Mac)

52. **Path suggestions:**
    - ⌘L, then type `/Us`. After a short pause, a list under the field shows the sub-folders that match (`Users`), never files.
    - ↓ marks a row, ↑ goes back up (from the first row back to the text). Tab writes the marked row (or the first) into the field with a `/` at the end, and the list shows that folder's sub-folders. Enter on a marked row goes there; with none marked, it goes where the text says.
    - Esc closes the list and typing goes on; a second Esc ends typing and the path parts come back. A click on a row goes there.
    - `~/Desktop` and `$HOME/Downloads` (also `${HOME}/Downloads`) go there. `$GEZIK_NOPE/x` stays as it is and opens nothing; `~veli` is not expanded.
    - Typing doesn't stutter while the list is shown. On a mounted share that doesn't answer (unplug the network after mounting), typing stays smooth and the list is empty after about 1 s.
53. **Folder history:**
    - Go to a few folders. ⌘L, then delete the text: the empty field lists "Recent" (the last 5) and "Frequent" (the most visited).
    - In another folder, type a part of a visited folder's name: under the sub-folders, a "History" heading lists the matches.
    - Delete a visited folder in Finder, then open the empty list: it goes from the list within 1 s.
    - Go ▸ Clear Folder History empties the list, and the status bar says "Folder history cleared". `state.toml` has no `[history]` any more.
54. **Command keys.** Add to `/tmp/gezik-cfg/settings.toml`:
    ```toml
    [[commands]]
    name = "Zip together"
    run = ["zip", "-r", "together.zip", "{files}"]
    folders = true
    shortcut = "ctrl+alt+z"
    menu = "Archives"
    ask = true

    [[commands]]
    name = "Clash"
    run = ["true"]
    shortcut = "mod+f"
    ```
    - Select a few files and a folder, then ⌃⌥Z: "Run Zip together on N items?". Cancel runs nothing. Run makes one `together.zip` with all of them, and the panel row says "Done · can't be undone". ⌘Z doesn't undo it.
    - The menu bar has a Commands menu with a greyed "Archives" heading and "Zip together" under it, with ⌃⌥Z in the title. Choosing it asks the same question.
    - Right-click, then Commands ▸: the same greyed heading over its group. The heading can't be chosen.
    - The notice says `commands[2]: shortcut "mod+f" is already used by filter; the command has no key`, and ⌘F still opens the filter.
    - ⌃⌥Z while the path field or the filter field has the keyboard does nothing.
55. **No history:** add `[history]` `remember = false`. `state.toml` loses its `[history]`, and new visits are not kept. Take the line out again.

### 7a, daily

56. **Open terminal.** In a folder with a space in its name, ⌘⌥T (and File ▸ Open Terminal): Terminal.app opens a window in that folder. Right-click a folder ▸ "Open terminal here": the same, in that folder. Right-click empty space ▸ "Open terminal here": the folder shown. Quit Gezik: the Terminal window stays.
57. **Copy path.** Select a file named `it's ş #1.txt` and press ⌘⌥C (and Edit ▸ Copy Path). Right-click ▸ "Copy path as ▸" ▸ Quoted: pasted into Terminal, the shell reads the same file (`'…'\''…'`). file:// URL: pasted into Safari's address bar, it opens the file. With nothing selected, ⌘⌥C copies the folder shown. Copy a file with ⌘C, then ⌘⌥C: ⌘V in another folder pastes nothing.
58. **Session.** Open three tabs, lock one, put another in front, press ⌘Q, open Gezik again: the same tabs, the same one in front, the lock in place. Force quit, open again: the same. `open -a Gezik --args <folder>` (or `gezik <folder>` from Terminal) opens that folder in a new tab after the saved ones, in front. `[session]` `restore = false` in `settings.toml`: `state.toml` loses `[session]`, and one tab opens in `start-folder`.
59. **Tab sets.** Window ▸ Save Tabs As…, name it "Work": `settings.toml` has `[[tab-sets]]`. Window ▸ Open Tab Set ▸ Work opens the set after the tabs, its first tab in front. With a tab locked, "Replace tabs with…" (in the tab's right-click menu) keeps it and says so. Note whether the Window menu shows "Open Tab Set" (and the sets) as expected, since it is an `if … : Menu` inside the menu bar.
60. **Custom terminal.** `[terminal]` `command = ["open", "-a", "iTerm", "{dir}"]`: ⌘⌥T opens iTerm in the folder.

### 7b, daily

61. **Pinned groups.** Write by hand in `settings.toml`: `pinned = ["~/Documents", { path = "~/Downloads", name = "DL", group = "Work" }, { path = "~/Desktop", group = "Media" }, { path = "~/Pictures", group = "work" }]` (use your own full paths or `{documents}`-style tokens). The sidebar shows "PINNED", then "Work" (with DL and Pictures) and "Media", as written. Right-click a group heading: "Move group up/down", "Rename group…", "Ungroup", and each does what it says in the file. Drag a pin between two pins of another group: a line shows where, and it lands in that group. Right-click a pin ▸ "Rename…" (empty: the folder's own name again) and "Move to group ▸" (the other groups, "New group…", "No group").
62. **Pins 1-9.** ⌘⌥1…⌘⌥9 go to the pins in the sidebar's order, and Go ▸ Pinned 1…9 too. Try it with the US, Turkish Q and French AZERTY layouts: note whether the menu's shortcut fires or the key goes to the window, and whether ⌘⌥ plus a digit types a character anywhere (path field) instead.
63. **View menu.** View ▸ Hide Extensions, Folders First, Single-Click to Open, Show Hidden Items (⌘⇧.) are check marks; Date Format ▸ and Size Format ▸ have one checked choice each. Choosing one moves the check and writes `settings.toml` (`[view]`); change the file by hand and the checks follow. ⌘⇧. writes `show-hidden` and the "Show Hidden Items" check follows it.
64. **View options.** Hide Extensions: names lose their extension (folders keep theirs; F2 shows the whole name). Folders First off: folders sort among the files. Date Format ▸ Relative: a file saved just now says "1 min ago", one from today "Today 14:05". Size Format ▸ Decimal: a 1,500-byte file is `1.5 kB` (the same as Finder's Get Info). Single-Click to Open: one click opens a folder, ⌘-click and ⇧-click only select.
65. **Hidden by default.** With no `show-hidden` line, `.DS_Store` and other dot files are not shown (macOS default `show-hidden = false`), and the template in a new `settings.toml` has `# show-hidden = true …` as a comment.

### 7c, daily

66. **New ▸ and templates.** Right-click empty space: "New ▸" holds Folder, Text file, Markdown file, the templates, then "Open templates folder". "Open templates folder" opens `~/Library/Application Support/gezik/templates/` in a new Gezik tab. Put `Report.pages` (or any file) and a folder `Project/` with a file in it there: within a second "Project" and "Report" are in the menu (no extension, alphabetical); Finder's `.DS_Store` in that folder is not listed. Each item is made in the folder shown, numbered `(2)` when the name is taken (`Project` comes with its file), and its name is being edited; ⌘Z takes it away (to the Trash).
67. **New folder with selection.** Select three files, ⌃⌘N (and File ▸ New Folder with Selection, and the right-click item "New folder with selection"): they are in "New folder", which is selected and being renamed. ⌘Z puts them back and the folder goes to the Trash; ⌘⇧Z moves them in again. With "New folder" already there, the new one is `New folder (2)`.
68. **Paste as file.** A screenshot to the clipboard (⌃⇧⌘4) and ⌘V on the list: `Pasted image <date> <time>.png`, opens in Preview and looks right (Retina size; it may take a moment, no beach ball). Safari ▸ right-click an image ▸ "Copy Image" ▸ ⌘V: an image, not text. Text from TextEdit: right-click empty space says "Paste text as file", ⌘V makes `Pasted text … .txt` (UTF-8). Files copied in Finder (⌘C): ⌘V pastes the files, not a text file. All of this without Terminal.
69. **Links.** Right-click a file ▸ "Create link": `Link to <name>`, a symbolic link, not a Finder alias (Finder shows the arrow badge). The same on a folder. Drag a file inside Gezik with ⌘⌥ held: the cursor shows the link arrow, the label says "Create link in …", and dropping makes the link. Drag from Finder with ⌘⌥ onto Gezik: a link too. Right-drag (or ⌃-drag) ▸ "Create link here". ⌘Z on a link to a folder: the link goes to the Trash, the folder and its files stay (check in Finder, without Terminal).
70. **Drop stack.** ⌘⇧S and Edit ▸ Add to Drop Stack add the selection; the strip opens above the status bar; View ▸ Drop Stack shows and hides it. Drag files from Finder onto the strip (Finder keeps them). Go to another folder: "Copy here" copies them (one ⌘Z), "Move here" moves them and they leave the strip. Delete a stacked file in Finder: it fades and is left out. With many items, a two-finger swipe scrolls the strip. Drag an item from the strip to Finder or the Desktop. "Clear" empties it.
71. **History.** After a few jobs (one failing: copy onto a locked file, one cancelled), View ▸ Operation History and the status bar's "History" open it: newest first, with time and result; "Show in folder" goes there with the results selected; "Details" lists what failed.

### 8a, search

72. **Search.** ⌘⇧F and Edit ▸ Find… open the search bar above the list ("in <folder> ▾", the name field, Content, Filters, Search). In your home folder type `*.pdf`: results come while you type (the name cache), in a tab titled "Search: *.pdf" whose address bar ends in `Search "*.pdf"`; Back returns to the folder, Forward to the results without searching again. Folder, Modified and Size columns; sort by Folder; ⌘F filters the results. The status bar says "N results in X s" and "Skipped N folders (search.skip)". "Content" ▸ a word from a few text files (one saved as UTF-16 by TextEdit) ▸ Return: those files, with the line in the Match column.
73. **Privacy prompts.** A search in your home folder (first run of this build): macOS asks for Desktop, Documents and Downloads (TCC). Allow some and deny one: the denied folder and the `~/Library` folders macOS keeps to itself count as "N folders could not be read" in the status bar, and the bar's ▾ ▸ "N folders could not be read…" lists them. The search still ends, and no prompt comes twice in one run.
74. **Whole drive.** Scope menu ▸ "Whole drive (/)" with a name that exists under `/Users`: `/Users` is searched (it lives on the Data volume), `/System/Volumes/Data` is not walked a second time (no file appears twice in the results), and other volumes (a USB stick, a disk image under `/Volumes`) are not walked. Esc stops a long search within a moment; there is no beach ball, and scrolling and switching tabs stay smooth while it runs.
75. **Flat view.** ⌘B and View ▸ Flat View in a project folder: every file under it in one list (no folders), the Folder column; ⌘B again on a file goes to its folder with it selected. A symlink to a folder is a row and is not walked into.
76. **Show in folder.** ⌘⇧E and Go ▸ Show in Folder on a result: its folder opens with the file selected; Back returns to the results. The row menu's "Show in folder in new tab" opens it in a new tab.
77. **Results like a folder.** In the results: ⌘C, then ⌘V in another folder (flat). Right-click ▸ "Copy with folders" and ⌘V: the folders under the scope are made; one ⌘Z takes the copies and the made folders away. Move a result to the Trash (⌘⌫): it leaves the list; ⌘Z brings it back. Select three results from two folders ▸ the rename layer (`{n}`): numbered per folder; a name already in that folder says "already in the folder" on its row. Close and open Gezik with a search tab open: the tab comes back and searches again.

### Design round 1 (Graphite)

78. **Themes.** The four built-in themes (`light`, `dark`, `classic-light`, `classic-dark`) and `auto` following the system's light/dark switch (System Settings ▸ Appearance); `reduce-motion = true` stops the hover and popup fades; `density = "compact"`; the classic themes are flat (rows edge to edge, no rounded sheet).
79. **Drawing at 200 %.** On a Retina screen: 1 px lines and the sheet's rounded corner are crisp; the 11 px small text (column header, status bar, sidebar section labels) is readable in the font the system picks; the glyphs (Refresh and History arcs, the dot on Drive), the sidebar icons, the LOCATIONS section; View ▾ and a long popup menu.
80. **Thin scroll bar.** In a folder with thousands of files: a 6 px bar, no track, the theme's color (never the system's), darker on hover and drag, at least 24 px tall; dragging it follows the pointer to both ends; a click above or below scrolls a page; touchpad two-finger scrolling stays smooth (natural direction as the system setting says); the sidebar, a long popup menu, the conflict list and the rename layer have the same bar.

### 8b, folder sizes, command palette, saved searches

81. **Folder sizes.** In your home folder the Size column fills in for folders: `…` while a folder is worked out, the ones on screen first, then a size. Folders macOS asks about (Desktop, Documents, Downloads: TCC) and the parts of `~/Library` it keeps to itself show `≥ …` when they could not be read in full; the preview says "Some folders could not be read". A folder's preview shows its size and `N files, M folders`; three folders selected: the status bar sums them, with `+` while one is still `…`. Sort by Size (both ways): the `…` folders stay last, the list sorts again at most once a second, and the focused row stays where it is on screen. A symlink to a folder has no size. View ▸ Calculate Folder Sizes (and the folder row's menu) works on a network share with `folder-sizes = "local"`; with `folder-sizes = "off"` no folder gets a size. Leaving a big folder (`/System`) at once stops the walk: no beach ball, coming back shows the sizes that were done.
82. **Command palette.** ⌘⇧P opens the picker with `>` and the caret after it; typing keeps the `>` (`copy` lists Copy, Copy Path, Copy with Folders…, shortcuts on the right). Return runs the command, Esc closes it and the list has the keyboard back. Deleting the `>` turns it into Quick Open.
83. **Quick Open.** ⌘P lists places first (pinned, recent folders, tabs, tab sets, saved searches and filters); typing `down` finds Downloads; Alt+Return opens it in a new tab; the last line `Search for "x" in <folder>` starts a search. Go ▸ Quick Open… and Go ▸ Command Palette… do the same as the keys.
84. **Saved searches.** Search for something, then the bar's ▾ ▸ Save search…: a name, then "Save with this folder" or "Save for any folder ({here})". `~/Library/Application Support/gezik/settings.toml` gets a `[[searches]]` entry; comments written there by hand stay. The sidebar's SEARCHES section lists it (magnifier icon): a click runs it, the tab is titled with its name, right-click ▸ Run in new tab / Rename… / Delete. A `{here}` search run in another folder searches there.

### 9a1, system icons, thumbnails, Finder names, aliases, packages

85. **Probe.** `cargo run --release -p gezik-platform --example mac_probe -- ~/Desktop/some.pdf ~/Pictures/photo.heic ~/Movies/clip.mov ~/Documents/report.pages` (any files of these types; add a file that is only in iCloud and a symlink to it if you have one). Paste the whole output into the results. Every line is PASS (NONE is fine for a type Quick Look has no thumbnail for; an iCloud-only file says SKIP); "4 workers at once" must not crash (if it does, or icons come back blank on a worker, NSWorkspace icons have to be serialised); "cancel after 5 ms" says none for a PDF or video. Note the `folder_has_own_icon` times. Then open the folder it names in Finder: "a.txt alias" (now broken), "folder alias" and "Safari alias" show the alias arrow, "folder alias" opens the folder in Finder. Delete the folder. Also run `cargo test -p gezik-platform` and report failures.
86. **System icons.** With `icons = "system"` (the default): `/Applications` in the list and the grid shows each app's own icon; `~` shows Desktop, Documents, Downloads with Finder's special folder icons; `/` shows Applications, Library, System, Users as Finder does; a folder with a custom icon (Finder ▸ Get Info ▸ paste an image on the icon) shows it; a mounted disk image under `/Volumes` shows its volume icon; `.pdf`, `.txt`, `.zip`, `.md` and a file with no extension have Finder's document icons. Icons are the right way up, have clean transparent edges (no dark fringe) and are crisp at 200 %. `icons = "gezik"` brings Gezik's own icons back at once.
87. **Speed and memory.** A folder with 1,000 files of mixed types: the list draws without a visible pause (≤ 150 ms), fast scrolling is as smooth as with `icons = "gezik"`. Scroll `/Applications` in the grid at the largest size: Activity Monitor's memory for gezik stays within ~40 MB of what it was before (the icon and thumbnail caches are bounded). Nothing uses CPU once scrolling stops. A folder with 200 subfolders on a network share (SMB) lists without a pause: each folder is checked for its own icon; note if it is slow.
88. **Quick Look thumbnails.** Grid view with `thumbnails = true`: a PDF shows its first page, a HEIC photo, a MOV a frame, a Pages/Keynote/Numbers file its first page, a PSD (if any) its picture, a `.txt` its text; PNG/JPEG still come at once (Gezik's own); none is upside down. The preview panel shows the same for a PDF and a MOV. Scroll a folder of 500 PDFs fast to the end: the ones on screen come first. Leave a folder of big videos while thumbnails load: CPU drops within a moment; go back: the thumbnails that were cancelled come now (no blank tiles). iCloud Drive with "Optimize Mac Storage": a file that is only in iCloud (cloud icon in Finder) gets no thumbnail and no preview (its icon only) and is **not** downloaded (Finder still shows the cloud icon afterwards); the same for a symbolic link to such a file in another folder.
89. **Finder names.** Put Türkçe first in System Settings ▸ General ▸ Language & Region, log out and in (or restart), then start Gezik: the sidebar says Masaüstü, Belgeler, İndirilenler…; `~` lists Belgeler, Masaüstü, Müzik…; `/` lists Uygulamalar, Kullanıcılar, Sistem, Kitaplık; the address bar's parts and the tab title say Kullanıcılar › <you> › Belgeler; ⌘L shows the real path `/Users/<you>/Documents`. Sort by name: Documents sorts under D. Type "Doc" in the filter: Belgeler is found. F2/Enter on Belgeler edits "Documents" (the real name; Esc, don't rename it). Files in `~` keep their extensions. A pinned `~/Documents` keeps its own label. Put English back first afterwards.
90. **Opening aliases.** In Finder make aliases (⌃⌘A) of a file, a folder, a folder on another volume and `/Applications/Safari.app`. In Gezik double-click and Enter: the folder alias goes into the original folder (Back returns); the file alias opens the original in its app; the Safari alias starts Safari (Gezik does not go into Safari.app). Delete the original file, open the alias: "The original item can't be found" with Delete Alias (the alias goes to the Trash at once, with no second question; ⌘Z brings it back) and OK (nothing happens). A symbolic link to a file still opens the file; a broken one asks the same question. A symbolic link to a folder is entered as before.
91. **Make Alias.** ⌃⌘A, File ▸ Make Alias and right-click ▸ Make Alias (before "Create link") on a file, a folder and three items: `<name> alias` next to each (`rapor.pdf alias`), selected. Finder shows the arrow badge and opens them. Again on the same file: `rapor.pdf alias (2)`; the first alias is untouched. ⌘Z moves the aliases to the Trash; the originals are untouched. Move the original elsewhere in Finder: the alias still opens it.
92. **Packages.** In `/Applications` double-click Safari.app: Safari starts, Gezik does not go in; Enter does the same. Right-click Safari.app ▸ Show Package Contents (right after Open) and File ▸ Show Package Contents: Gezik goes into `Safari.app` (Contents). A Keynote/Pages document that is a folder (`.key`/`.pages` package, e.g. from an older version, or `.rtfd` from TextEdit) opens in its app. A plain folder you name `x.app` (`mkdir /tmp/gezik-test/x.app`) is entered on double-click. The command palette lists Make Alias and Show Package Contents.
93. **Exe size.** `ls -l target/release/gezik` before (on `master`) and after (on `feat/system-9a1`), both `cargo build --release -p gezik`: write both numbers; the difference should be under 256 KB plus the Quick Look and CoreGraphics bindings.

### 9a2, Open With, Share, Quick Actions, Quick Look panel

94. **Probe.** Make a Quick Action in Automator (File ▸ New ▸ Quick Action, "Workflow receives current files or folders in Finder", one "Reveal Finder Items" action, save as "Gezik Test"), then `cargo run --release -p gezik-platform --example mac_probe -- --service "Gezik Test" ~/Desktop/some.txt ~/Desktop/some.pdf ~/Pictures/photo.jpg /Applications/Safari.app`. Paste the whole output into the results. Section 4: every line PASS, each file lists its default app first, and note the "50 items" time (above 50 ms means the menu shows "Loading…" for 50 items). Section 5: the `.txt` offers "Gezik Test"; note the `pbs -dump_pboard` line; `perform "Gezik Test"` is PASS and a Finder window shows the test file.
95. **Open With.** Right-click a `.txt`: "Open With ▸" right after "Open with default app", TextEdit first with "(default)", other apps by name, then "Other…". Choose one: the file opens in it. Select a `.txt` and a `.md`: only apps that open both; select a `.txt` and a `.png`: TextEdit is not offered. "Other…" opens a panel in /Applications where only apps can be chosen; Cancel does nothing; choosing one opens the file in it. Right-click Safari.app and a `.pages` package: "Open With ▸" after "Show Package Contents". A plain folder and This PC rows have no "Open With". Select 60 files: the list is the focused one's, and choosing an app sends all 60 to it at once (not one window per file). If "Loading…" ever shows, right-click the same items again: the apps are there.
96. **Share.** Right-click a file ▸ "Share…": the system's share menu (AirDrop, Mail, Messages, Notes …) opens where you clicked; Mail gets the file as an attachment. Select three files ▸ Share… ▸ Mail: all three. File ▸ Share… and the command palette's "Share…" open it at the pointer (or the window's middle when the pointer is outside). Write whether the menu appeared every time (Apple says it should be opened on a mouse press). The menu stays up until you choose or click away (it is not closed at once) and closes cleanly.
97. **Quick Actions.** Right-click the `.txt`: "Quick Actions ▸" after "Share…" lists "Gezik Test"; choosing it reveals the file in Finder. An image-only Quick Action (Automator: "receives current image files") shows for a `.jpg` and not for a `.txt` or a folder; a mixed selection shows only actions every item fits. Also try a Quick Action that runs for a few seconds (Automator: "Run Shell Script" with `sleep 3; open "$@"`): it still gets the files (Gezik frees its pasteboard right after asking for the service; if a slow action gets nothing, write it down). Select more than 50 rows: no "Quick Actions" item. With no file-taking Quick Action installed, no "Quick Actions" item.
98. **Quick Look panel.** With `[system] quick-look = "system"` (the default): select a PDF, press Space: the system's Quick Look panel (as in Finder) shows it; Space again closes it. With the panel open, ↓/↑ in a list (or ←/→ in the grid) move Gezik's selection and the panel follows; select three files and press Space: the panel shows "1 of 3" and its own arrows page among them; Esc and the panel's close button close it, then Space opens it again. After the panel closes, the keyboard is Gezik's again (arrows move the selection at once, no extra click). A video plays, a folder shows its icon. `quick-look = "gezik"` brings Gezik's own window back at once. If the panel never opens, the status bar says so and Gezik's window opens: write it down (the default then becomes "gezik").
99. **Unchanged.** The row menu still has Make Alias, Create link, Copy path as ▸, Commands ▸ (when set up), and all four submenus show together on a file with commands. Right-clicking feels as quick as before; Activity Monitor shows no extra gezik thread after the menu closes.
100. **Exe size.** `ls -l target/release/gezik` on `feat/system-9a1` and on `feat/system-9a2`, both `cargo build --release -p gezik`: write both numbers; the difference should be under 256 KB plus the Quick Look UI bindings.

### 9a3, the Info window

101. **Probe.** `cargo run --release -p gezik-platform --example mac_probe` and paste the output. Section 6: every line PASS (it includes "setuid kept across a group change" and the locked-file and link checks); note the users/groups line (count and time; above 200 ms the window opens slowly). Also run `cargo test -p gezik-platform attrs` and `cargo test -p gezik-ops attrs` and report failures (their Mac/Linux-only tests: a link is not followed, setuid survives a group change, a locked file is unlocked first, a real chmod is undone through the job engine).
102. **Opening.** Select a PDF, press ⌘I: a panel on the right with "rapor.pdf Info", Kind, Size, Where, Created, Modified, Last opened. File ▸ Get Info and right-click ▸ Get Info (after "Delete permanently") do the same. On a folder the size says "calculating…" then a size. Three items: "3 items", "Where" their folder (or "several folders"), boxes that differ show a dash. Esc and Done close it; the list has the keyboard again.
103. **Permissions.** Tick Group ▸ Write: `ls -l` in Terminal shows `rw-rw-r--` at once; close the window, ⌘Z: back to `rw-r--r--`. Type 600 in Octal and Return: `rw-------`. Type 4755: "Setuid, setgid and sticky can't be changed here", nothing changes. Three files with different permissions: a dashed box turns on for all when clicked. With the window open, `chmod 777 <file>` in Terminal, then tick a box: the note says "1 item changed since; shown as it is now" and the boxes show 777; tick again: it works. With the window open, replace the file in Terminal (`cp other.pdf x && mv x rapor.pdf`), then tick a box: the new file is not changed (the note says "changed since").
104. **Owner and group.** Group ▾ lists your groups (staff, everyone, admin …); choose admin: `ls -l` shows it. A setuid file you own (`chmod 4755 f`): change its group to admin: `ls -l` still shows `rws`. Type `wheel` (if you are not in it) and Return: the note "Requires administrator: 1 item not changed" in red, the operations panel lists the item with "Requires administrator". Owner: type `root`, Return: the same note. Type `nobody-at-all`: "No user is named …", nothing runs.
105. **Hidden and Locked.** Tick Hidden: Finder no longer shows the file (Gezik still does: a known gap). Tick Locked: Finder shows the lock; the permission boxes, Octal, Owner and Group are greyed; untick Locked: they work again. ⌘Z after closing undoes each, in order. A locked file you try to give a group you are not in (refused): it is locked again afterwards (`ls -lO` shows `uchg`).
106. **Apply to enclosed items.** A folder with two files, a script (`chmod +x`), a subfolder and a symlink to a file outside it; set the folder to 750 and group staff, then "Apply to enclosed items…": the question; Cancel does nothing. Apply: `ls -lR` shows the files `rw-r-----`, the script `rwxr-x---`, the subfolder `rwxr-x---`, the symlink and the file it leads to unchanged, the folder itself unchanged. ⌘Z puts every item back. On a big folder (a copy of ~/Library/Caches) the operations panel shows progress and Cancel stops it; ⌘Z puts back the ones it did.
107. **Open with.** One PDF: "Open with: Preview ▾"; choose TextEdit: Finder's Get Info for that file says TextEdit, another PDF still opens in Preview. "Change All…" ▸ Change All: every PDF opens in TextEdit (write down whether macOS asked anything, and the macOS version: the call behind it is deprecated). Cancel and Esc change nothing. Put Preview back the same way. "Other…" opens the app panel. A folder, a link and several items have no "Open with" row; an `.app` has one. A file with no default app says "Not set".
108. **Links and ACLs.** Get Info on a symlink: Kind "Symbolic link", the permission boxes greyed; Group ▾ ▸ another group changes the link's own group (`ls -l` on the link), not the file's (`ls -lL`). `chmod +a "everyone deny delete" <file>`: Get Info says "This item has access control entries…".
109. **Exe size.** `ls -l target/release/gezik` on `feat/system-9a2` and on `feat/system-9a3`, both `cargo build --release -p gezik`: write both numbers; the difference should be under 256 KB.

### 9b1, command line and single instance

110. **Tests first.** In the clone: `cargo test -p gezik-platform instance`. These tests (socket, stale socket, hung and huge callers, peer uid) only compiled on Windows; they never ran on a Mac before.
111. **Hand-over.** With Gezik open, from Terminal: `/path/to/gezik ~/Documents` opens in the same window (if a Documents tab is open, it switches to it), the window comes to the front, and Terminal gets its prompt back at once. `gezik ~/Documents/x.pdf` opens the folder with `x.pdf` selected; Preview does not open.
112. **Help, version, new window.** `gezik --help` and `gezik --version` print to Terminal. `gezik --new-window ~` opens a second window, 32 px offset (note it if the Dock shows a second icon). ⌘N and File ▸ New Window do the same. Close the first window, then the second; open Gezik again: the first window's tabs come back.
113. **Hung or crashed Gezik.** `kill -STOP <pid>`, then `gezik ~`: its own window opens after about 2 s; `kill -CONT <pid>`. `kill -9 <pid>`, then `gezik ~`: a new first Gezik; `gezik /tmp` then goes to it. `ls -l "$TMPDIR"gezik-*` shows an `srw-------` socket and an `-rw-------` lock.
114. **Off.** `[system] single-instance = false` in settings.toml, restart Gezik: every call opens its own window.
115. **Bring to front.** The macOS arm of bringing the window forward (winit `set_minimized(false)` + `focus_window`) was not run or even compiled with tests on Windows: check that a minimized Gezik comes back and comes to the front, and note it if only the Dock icon bounces.

### 9b2, the Trash

116. **Tests first.** In the clone: `cargo test -p gezik-platform trash`, `cargo test -p gezik-platform fs`, `cargo test -p gezik-ops delete` and `cargo test -p gezik-ops restore`. The macOS arms (the `.DS_Store` reader on `~/.Trash`, `mac_original`'s volume rule, `only_items_in_a_bin_count` (only `~/.Trash/x` and `/Volumes/*/.Trashes/<uid>/x` may be deleted for good), `put_back_never_goes_through_a_link` with a symlink, `only_a_plain_file_is_read_as_a_record` with a FIFO, restore under `/var/folders`) only compiled on Windows; they never ran on a Mac. Then `cargo test --release -p gezik-platform ten_thousand -- --ignored` (10,000 items in under 1 s).
117. **Full Disk Access off** (System Settings ▸ Privacy & Security ▸ Full Disk Access: Gezik's terminal or binary not ticked). Click Trash in the sidebar (under the drives): the status bar says `Gezik needs Full Disk Access to show the Trash.` and a box asks once, `Open Privacy Settings` / `Not Now`. `Open Privacy Settings` opens Privacy & Security ▸ Full Disk Access (the `x-apple.systempreferences:…Privacy_AllFiles` address is **to be confirmed**: note where it lands). Open the Trash again in the same session: the note, no box. Items in a USB stick's `/Volumes/<stick>/.Trashes/<uid>` are still listed.
118. **Full Disk Access on.** A file trashed with Finder shows its own name, its Original location and its Date deleted. The `.DS_Store` reader (`ptbL`/`ptbN`) is **to be confirmed** against a real Mac trash: the place must be the one Finder's Put Back uses. Trash two files both named `a.txt` from two folders: Finder renames the second in the bin (`a 10.21.03.txt`); Gezik must show `a.txt` for both, each with its own place. A file trashed with Gezik (⌘⌫): does it have an Original location? (If not, `trashItemAtURL` writes no `ptbL`, and deviation 10's deferred work is needed; note it.) If the reader fails, items show a blank place and Put Back asks for a folder: note that as a failure, with a copy of `~/.Trash/.DS_Store` if you can.
119. **Real bins on USB volumes.** Trash a file on a USB stick (APFS or HFS+, and one exFAT/FAT stick) with Finder: it is listed with its place on the stick (`/Volumes/<stick>/…`). Put Back returns it to the stick, not to the startup disk. A `.Trashes/<uid>` that is a symlink or belongs to another user is not read (`sudo chown` another user on a test stick's folder, if you try it).
120. **Put Back** (row menu, File ▸ Put Back, the palette): the item goes back to its folder and leaves the list and Finder's Trash. Its folder deleted meanwhile: it is made again. A file of the same name there: the conflict list (Keep both gives `a (2).txt`, Replace puts the existing one in the Trash first). ⌘Z puts it back in the Trash. An item with no known place asks for a folder (starts at your home folder); a relative path or a file is refused with a note.
121. **Delete for good and Empty.** In the Trash, ⌘⌫ and the row menu's `Delete Permanently…` ask `Delete N items permanently?` / `This cannot be undone.`; Cancel does nothing. File ▸ Empty Trash…, ⌘⇧⌫, the background menu and the sidebar Trash row's menu ask `Empty the Trash?` / `Permanently delete N items (X GB)? This cannot be undone.` (no size if a folder is in it). The job panel shows progress; cancel an empty with a big folder: what is left still shows in Finder's Trash. Empty Trash with nothing in it: `The Trash is empty`.
122. **Live and refused.** With the Trash open, trash a file in Finder: it shows in Gezik within about 1 s. Go to another folder: Activity Monitor ▸ Gezik ▸ Open Files and Ports no longer lists `~/.Trash`. In the Trash: ⌘C, ⌘X, ⌘V, ⇧⌘N, ⌘D, the rename key and a `[[commands]]` key do nothing and the status bar says `Not available in the Trash`; opening a row (double-click, ⌘↓) opens nothing, with a note; rows can't be dragged out; Go ▸ Show Trash opens it.

### 9b3, the command line and --unregister

Write the results to `macos-test-results.md`. Use a Terminal for the commands; `gezik` below is the binary in the `.app` (or `~/.local/bin/gezik` once added).

123. **Tests first.** In the clone: `cargo test -p gezik-platform system` and `cargo test -p gezik system_changes`. The Unix arms (`replace_symlink` refusing a file or a foreign link, the sweep by exe name) only compiled on Windows; they never ran on a Mac.
124. **Panel.** Palette ▸ `System Integration…` and View ▸ `System Integration…` (menu bar): a box with three rows (`Command line (PATH)` Off/Add, `Changes made: 0`, `Undo all system changes`). Esc closes it; typing in the field does nothing; Up/Down and click work.
125. **Add, no `~/.local/bin`.** Move `~/.local/bin` away first if you have one. `Add gezik to PATH` (panel or palette) asks first and names every place it writes. After it: `~/.local` and `~/.local/bin` exist with mode 0755 and `ls -la ~/.local/bin` shows the `gezik` link pointing at the real binary inside the `.app`. The hint box shows `export PATH="$HOME/.local/bin:$PATH"`; `Copy` puts it on the clipboard (paste it in Terminal). With that line in `~/.zshrc`, a new Terminal's `gezik .` opens the folder in the running Gezik.
126. **Someone else's file.** Remove gezik from PATH, then `echo hi > ~/.local/bin/gezik` and Add again: the file is untouched (`cat` still says `hi`) and the panel says `<place> was not made by Gezik; left alone` (Off, no button). Do the same with `ln -s /usr/bin/true ~/.local/bin/gezik`: the link is untouched. Clean up both.
127. **Gezik moved.** Add, then copy the `.app` to another folder and open it from there: the panel says `Gezik's exe moved; gezik still starts <old path>` with `Update`. After Update, `ls -la ~/.local/bin/gezik` shows the new binary.
128. **`--unregister`.** Quit Gezik (or not: it must work either way). `gezik --unregister` prints one line per change and `echo $?` gives 0. The link is gone; `~/.local/bin` and `~/.local` are removed if Gezik made them and they are empty. Put another file in `~/.local/bin` before: the folder stays, its line says `left (not empty)` and `echo $?` gives 1. A second run says `No system-changes.toml: …` and `Nothing of Gezik's was found.` (0).
129. **No journal.** Add, delete `~/Library/Application Support/gezik/system-changes.toml` (or the `GEZIK_CONFIG_DIR` one), then `gezik --unregister`: only the link that points at this Gezik is swept; `~/.local/bin` stays. Write `version = 9` into a fresh `system-changes.toml`: the panel says `Fix or delete system-changes.toml first`, Add fails, `gezik --unregister` exits 2 and the file is unchanged.

### 9b5, cloud drives

Write the results to `macos-test-results.md` ("Run: 9b5"). Have iCloud Drive on, and OneDrive or Google Drive (File Provider) if you use them.

130. **Tests and probe first.** In the clone: `cargo test -p gezik-platform cloud`, then `cargo run -p gezik-platform --example cloud_probe -- ~/Library/Mobile\ Documents/com~apple~CloudDocs ~/Library/CloudStorage/*` and paste the output (the roots it finds, and each path's state). The macOS arms (`st_flags` → `SF_DATALESS`, the `CloudStorage` reader, the `NSFileManager` calls) only compiled on Windows; they never ran on a Mac.
131. **Sidebar.** A `CLOUD` heading right after the pinned items (before SEARCHES), with iCloud Drive, OneDrive, Google Drive (if installed) by their names; two OneDrive accounts show as `OneDrive (Personal)` / `OneDrive (…)`. Resting the pointer on a row shows the account and the path; a click goes there. `[sidebar] cloud = false` in settings.toml: the heading goes away (live), the row badges stay.
132. **State badges.** In iCloud Drive, a file that is only in the cloud: a small cloud on its icon's corner; selected alone, the status bar ends with `· Online only`. The preview panel and the grid's thumbnail start no download (Finder still shows the cloud icon on it). A downloaded file: a hollow check, `Available on this device`. Outside the cloud folders: no badge, no words.
133. **Download Now / Remove Download.** Right-click a cloud-only file ▸ `Download Now`: the job panel shows it, the file downloads, the badge turns into the check (the folder watcher; if it doesn't, note whether reloading the folder does it). On an uploaded file ▸ `Remove Download`: it goes back to the cloud badge. On a folder ▸ `Remove Download`: does it work, or do you get the note `Not known to be synced yet; left on this device`? (Deviation 2: write down which.) The palette has both commands too.
134. **Data safety.** Turn Wi-Fi off, edit a file in iCloud Drive, then `Remove Download`: the file stays, the job panel says `Not known to be synced yet; left on this device` (a note, not a failure). Turn Wi-Fi on, wait for the upload, try again: it is removed.
135. **OneDrive / Google Drive.** Items 132-134 in `~/Library/CloudStorage/OneDrive-…` and `GoogleDrive-…`: do `startDownloading…` / `evict…` work there, and is the "uploaded" key given (if not, `Remove Download` is always the note: write it down).
136. **Links.** A symlink inside iCloud Drive that points elsewhere: `Download Now` and `Remove Download` both say `A link; left as it is` and the target is untouched. A selection with one item outside the cloud folders: `Not in a cloud folder`, no job.
137. **Late roots.** Quit Gezik with an iCloud Drive folder as the last tab; open it again: the badges show without moving to another folder (the roots arrive after the folder).

Report anything else that looks wrong: layout, fonts, Retina scaling, ⌘ shortcuts that don't work, and the wording of the boxes.

## Known gaps (not bugs)

These are known differences from Finder and ForkLift (from the ForkLift comparison, §3). Don't report them as failures. A note is welcome if one hurts more than expected.
- **Icons and thumbnails:**
  - Alias rows show Finder's "Document" type and the generic document icon, not "Alias" with the arrow badge (Gezik reads no extra file data while listing).
  - Pinned folders keep their own labels; only the sidebar's known folders, the address bar, tab titles and the rows under `~` and `/` use Finder's localized names.
- **Context menu:**
  - Open With ▸ lists app names without their icons.
  - Quick Actions ▸ lists only Quick Actions and Services installed as bundles in ~/Library/Services and /Library/Services; services that apps provide (e.g. Terminal's "New Terminal at Folder") and the system's Markup/Rotate/Create PDF are not listed, and one turned off in System Settings still shows.
- **Folder sizes (8b):** a change deep inside a subfolder made outside Gezik shows the old size for up to 5 minutes (⌘R or F5 works it out again). Search results and the flat view show no folder sizes.
- **Missing features:**
  - Finder tags are not read or shown.
  - Gezik's own list does not hide items marked Hidden in Get Info (it hides by a leading dot only); Finder does.
  - Get Info has no "Change as administrator…" yet (part 9b7): changes the system refuses only say "Requires administrator".
  - Get Info shows no icon in its General part.
  - No iCloud Drive status, download or evict. What happens when a file that is only in the cloud is opened has not been tried; note it if you try.
  - Search walks the disk itself: Spotlight is not used (no Spotlight index, no `kMDItem` queries, no Spotlight comments or contents of PDFs and Office files). Results and the flat view don't follow changes made in Finder until ⌘R or F5.
  - No Column (⌘3) or Gallery (⌘4) view, and no ⌘+/⌘- text size.
  - No eject and no Connect to Server (⌘K).
  - Gezik can't be the default file viewer.
- **App bundle and menus:**
  - No `.app`, signing or notarization. The app menu items read "About/Hide/Quit gezik".
  - The menu bar shows the default shortcuts, not ones changed in settings.toml.
  - ⌘, does nothing.
- **Names:**
  - A kept copy is `b (2).txt` (Finder: `b 2.txt`).
  - A new folder is "New folder" (Finder: "untitled folder").
  - The interface is in English.
- **Drag and drop:**
  - Spring-loaded folders work only on tabs.
  - Drags of file promises from Photos or Mail probably don't arrive. This is worth trying and noting.
- **Look:** the controls are drawn by Slint. There is no Liquid Glass sidebar, no system accent colour, and no macOS window tabs.

## When you are done

Commit the results (and any fixes) in English, with no `Co-Authored-By` or Claude lines, and push to the branch you tested (`feat/batch-ops-5d`, or a new `fix/macos-2` from master for fixes).
